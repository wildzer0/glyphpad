// SPEC §10.4: no console window for the release binary on Windows; CLI
// subcommands attach to the parent's console instead.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod config_file;
mod engine_loop;
mod live;
mod logging;
mod print;
mod service;
#[cfg(windows)]
mod tray;
mod update;

use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use glyphpad_core::capture::{Capture, CaptureWriter};
use glyphpad_core::config::Config;
use glyphpad_core::stats::SessionStats;
use glyphpad_core::symbols::TemplateFile;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Customizable touchpad gesture engine.
#[derive(Parser)]
#[command(name = "glyphpad", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Cmd>,

    /// Receive Raw Input on a hidden top-level window instead of a
    /// message-only one (Windows; for diagnosing missing background input).
    #[arg(long, global = true)]
    toplevel_window: bool,

    /// Configuration file to use instead of the portable or standard one.
    #[arg(long, global = true, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Set by the old version when it starts the new one after an update.
    #[arg(long, global = true, hide = true)]
    post_update: bool,

    /// Take releases from a folder instead of GitHub (debug builds only).
    #[cfg(debug_assertions)]
    #[arg(long, global = true, hide = true, value_name = "DIR")]
    update_source: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run the gesture engine (default).
    Run,
    /// Record normalized touch frames to a JSON Lines file.
    Capture {
        file: PathBuf,
        /// Stop after this many seconds (otherwise stop with Ctrl+C).
        #[arg(long)]
        seconds: Option<f64>,
        /// Overwrite the file if it exists.
        #[arg(long)]
        force: bool,
        /// Also print every frame while recording.
        #[arg(long)]
        verbose: bool,
    },
    /// Run a capture through the gesture engine and print what it recognizes.
    Replay {
        file: PathBuf,
        /// Recognize every variant (any fingers, holds, triple taps, 8
        /// directions) instead of only what the configuration uses.
        #[arg(long)]
        all: bool,
        /// Also print every frame.
        #[arg(long)]
        frames: bool,
    },
    /// Print the HID capabilities of every Precision Touchpad.
    DumpDescriptor,
    /// Open the live debug tab in test mode and print recognized gestures
    /// with their scores; no action runs.
    Debug,
    /// Record examples of a drawn symbol and save them as its templates.
    Record {
        /// Symbol name, as used in `symbol = "..."` (case-sensitive).
        name: String,
        /// How many examples to draw.
        #[arg(long, default_value_t = 5, value_parser = clap::value_parser!(u8).range(1..=20))]
        samples: u8,
        /// Replace the examples recorded before for this symbol instead of
        /// adding to them (the built-in ones are kept either way).
        #[arg(long)]
        replace: bool,
    },
    /// Check for a newer version and install it.
    Update {
        /// Only report whether a newer version exists.
        #[arg(long)]
        check: bool,
    },
    /// Validate a configuration file and print its errors and warnings.
    CheckConfig {
        /// Defaults to the configuration GlyphPad would use.
        file: Option<PathBuf>,
    },
}

fn main() {
    #[cfg(windows)]
    glyphpad_platform::windows::console::attach_parent_console();

    let cli = Cli::parse();
    // The log file lives next to the configuration, which needs the command
    // line: a portable GlyphPad leaves its diagnostics beside itself.
    logging::init(config_file::locate(cli.config.as_deref()).ok().as_ref());

    if let Err(e) = run(cli) {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    let toplevel_window = cli.toplevel_window;
    let config_arg = cli.config.as_deref();
    #[cfg(debug_assertions)]
    let update_source = cli.update_source.clone();
    #[cfg(not(debug_assertions))]
    let update_source: Option<PathBuf> = None;
    let extra = GuiExtra {
        post_update: cli.post_update,
        update_source: update_source.clone(),
    };
    match cli.command.unwrap_or(Cmd::Run) {
        Cmd::Run => gui(config_arg, toplevel_window, false, extra),
        Cmd::Update { check } => update_command(config_arg, update_source, check),
        Cmd::Capture {
            file,
            seconds,
            force,
            verbose,
        } => capture(&file, seconds, force, verbose, toplevel_window),
        Cmd::Replay { file, all, frames } => {
            let (config, templates) = match config_arg {
                Some(_) => load_config(config_arg)?,
                None if all => (Config::default(), TemplateFile::default()),
                None => load_config(None)?,
            };
            replay(&file, &config, &templates.effective(), all, frames)
        }
        Cmd::DumpDescriptor => {
            print!("{}", live::dump_descriptors()?);
            Ok(())
        }
        Cmd::Debug => gui(config_arg, toplevel_window, true, extra),
        Cmd::Record {
            name,
            samples,
            replace,
        } => record(config_arg, toplevel_window, &name, samples.into(), replace),
        Cmd::CheckConfig { file } => check_config(file.as_deref().or(config_arg)),
    }
}

/// What an update adds to starting the window.
struct GuiExtra {
    /// This process was started by the version it replaces.
    post_update: bool,
    update_source: Option<PathBuf>,
}

/// The single-instance lock, where the updater can give it up and take it
/// back (SPEC §12.2).
#[cfg(windows)]
static INSTANCE: std::sync::Mutex<
    Option<glyphpad_platform::windows::single_instance::InstanceGuard>,
> = std::sync::Mutex::new(None);

/// The window, the tray and the engine: `run` (and `debug`, which starts in
/// test mode on the live debug tab and prints recognitions).
fn gui(explicit: Option<&Path>, toplevel_window: bool, debug: bool, extra: GuiExtra) -> Result<()> {
    use std::sync::{Arc, OnceLock};

    use glyphpad_core::service::{EngineMode, UiLink};

    // A second instance asks the first one to show its window and exits
    // (SPEC §10.1). `debug` is a diagnostic and may run alongside.
    let link: Arc<OnceLock<Arc<UiLink>>> = Arc::default();
    #[cfg(windows)]
    if !debug {
        let slot = link.clone();
        match glyphpad_platform::windows::single_instance::acquire(move || {
            if let Some(ui) = slot.get() {
                ui.request_show();
            }
        }) {
            glyphpad_platform::windows::single_instance::Instance::Primary(guard) => {
                if let Ok(mut held) = INSTANCE.lock() {
                    *held = Some(guard);
                }
            }
            glyphpad_platform::windows::single_instance::Instance::Secondary => {
                println!("GlyphPad is already running; showing its window.");
                return Ok(());
            }
        }
    }

    let location = config_file::locate(explicit)?;
    tracing::info!("configuration: {}", location.describe());
    let rx = live::start(toplevel_window)?;
    let (updates_tx, updates_rx) = crossbeam_channel::unbounded();
    let state_path = config_file::update_state_path(&location);
    let svc = service::spawn(
        rx,
        service::Options {
            location,
            print: debug,
            mode: if debug {
                EngineMode::Test
            } else {
                EngineMode::Normal
            },
            updates: Some(updates_tx),
        },
    )?;
    let _ = link.set(svc.ui.clone());
    let on_started = after_update_housekeeping(&svc, &extra);
    // A development build must not replace itself with a release.
    if !debug && (!cfg!(debug_assertions) || extra.update_source.is_some()) {
        update::spawn(
            updates_rx,
            update::Settings {
                shared: svc.shared.clone(),
                ui: svc.ui.clone(),
                commands: svc.commands.clone(),
                state_path,
                source: update::make_source(VERSION, extra.update_source.clone()),
                version: semver::Version::parse(VERSION).context("the program's version")?,
                release_lock: Box::new(|| {
                    #[cfg(windows)]
                    if let Ok(mut held) = INSTANCE.lock() {
                        held.take();
                    }
                }),
                reclaim_lock: Box::new(|| {
                    #[cfg(windows)]
                    if let Ok(mut held) = INSTANCE.lock() {
                        *held = glyphpad_platform::windows::single_instance::reacquire();
                    }
                }),
            },
        );
    }
    let start_minimized = svc
        .shared
        .lock()
        .map(|s| s.config.settings.start_minimized)
        .unwrap_or(false);
    // Only hide at start when a tray icon can bring the window back.
    let start_hidden = cfg!(windows) && start_minimized && !debug;
    if debug {
        eprintln!("Test mode: gestures are recognized and printed, no action runs.");
    }

    glyphpad_gui::run(
        glyphpad_gui::Options {
            start_hidden,
            tab: if debug {
                glyphpad_gui::Tab::Debug
            } else {
                glyphpad_gui::Tab::Gestures
            },
            version: VERSION.to_string(),
        },
        glyphpad_gui::Host {
            shared: svc.shared.clone(),
            commands: svc.commands.clone(),
            ui: svc.ui.clone(),
            tray: Box::new(|ctx| {
                #[cfg(windows)]
                {
                    tray::create(ctx)
                }
                #[cfg(not(windows))]
                {
                    let _ = ctx;
                    None
                }
            }),
            open_folder: Box::new(open_folder),
            open_url: Box::new(|url| open_folder(Path::new(url))),
            bring_to_front: Box::new(|_hwnd| {
                #[cfg(windows)]
                glyphpad_platform::windows::foreground::bring_to_front(_hwnd);
            }),
            on_started,
        },
    )
    .map_err(|e| anyhow::anyhow!("the window failed: {e}"))
}

/// A started-by-update process tells the old one it came up and says so on
/// screen; any other start tidies up after an earlier update (SPEC §12.2).
///
/// The "I am up" marker is written only from the returned closure, which runs
/// once the window's event loop is alive: a version that dies while creating
/// its window must not stop the old one from rolling back.
fn after_update_housekeeping(
    svc: &service::ServiceHandle,
    extra: &GuiExtra,
) -> Box<dyn Fn() + Send + Sync> {
    let Ok(exe) = glyphpad_updater::install::current_exe() else {
        return Box::new(|| {});
    };
    // The old executable can only be deleted once its process has exited,
    // which after a fresh update takes a few seconds.
    let cleanup_exe = exe.clone();
    std::thread::spawn(move || {
        for _ in 0..30 {
            glyphpad_updater::install::cleanup(&cleanup_exe);
            if !glyphpad_updater::install::old_path(&cleanup_exe).exists() {
                break;
            }
            std::thread::sleep(Duration::from_secs(2));
        }
    });
    if !extra.post_update {
        return Box::new(|| {});
    }
    let marker = glyphpad_updater::install::marker_path(&exe);
    let commands = svc.commands.clone();
    Box::new(move || {
        // The configuration is loaded, the engine runs and the window is up.
        if let Err(e) = std::fs::write(&marker, VERSION) {
            tracing::warn!("cannot write the update marker: {e}");
        }
        let _ = commands.send(glyphpad_core::service::Command::RunAction(
            glyphpad_core::action::Action::Notify {
                title: Some("GlyphPad updated".into()),
                message: format!("Now running version {VERSION}"),
            },
        ));
    })
}

/// `glyphpad update [--check]`: look for a newer version and, without
/// `--check`, install it. A running GlyphPad keeps the old version until restarted.
fn update_command(explicit: Option<&Path>, source_dir: Option<PathBuf>, check: bool) -> Result<()> {
    let location = config_file::locate(explicit)?;
    let loaded = config_file::load(&location, false)?;
    let current = semver::Version::parse(VERSION).context("the program's version")?;
    let source = update::make_source(VERSION, source_dir);
    let state_path = config_file::update_state_path(&location);
    let mut state = glyphpad_updater::state::State::load(&state_path);
    let mut cache = Vec::new();
    let found = update::find(
        source.as_ref(),
        &mut state,
        &mut cache,
        &current,
        update::channel(loaded.config.update.channel),
    )?;
    if let Err(e) = state.save(&state_path) {
        tracing::warn!("cannot save the update state: {e}");
    }
    let Some(release) = found else {
        println!("GlyphPad {VERSION} is up to date.");
        return Ok(());
    };
    println!(
        "Version {} is available (you have {VERSION}).",
        release.version
    );
    if !release.notes.trim().is_empty() {
        println!("\n{}\n", release.notes.trim());
    }
    println!("Release page: {}", release.page_url);
    if check {
        return Ok(());
    }
    match update::download_and_install(source.as_ref(), &release) {
        Ok(_) => {
            println!(
                "Updated to {}. Restart GlyphPad to use it (a running one keeps the old version until then).",
                release.version
            );
            Ok(())
        }
        Err(glyphpad_updater::Error::NotWritable(why)) => {
            bail!(
                "cannot update in place ({why}). Download version {} from {}",
                release.version,
                release.page_url
            )
        }
        Err(e) => Err(e.into()),
    }
}

fn open_folder(path: &Path) {
    let opener = if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    if let Err(e) = std::process::Command::new(opener).arg(path).spawn() {
        tracing::warn!("cannot open {}: {e}", path.display());
    }
}

/// The configuration and the user's symbol templates next to it.
fn load_config(explicit: Option<&Path>) -> Result<(Config, TemplateFile)> {
    let location = config_file::locate(explicit)?;
    tracing::info!("configuration: {}", location.describe());
    let loaded = config_file::load(&location, true)?;
    config_file::log_warnings(&loaded);
    let templates = config_file::load_templates(&location);
    let effective = templates.effective();
    tracing::info!(
        "{} gesture(s), templates for {} symbol(s) ({} recorded)",
        loaded.config.gestures.len(),
        effective.len(),
        templates.symbols.len()
    );
    for g in &loaded.config.gestures {
        for s in g.def.symbol.iter().flat_map(|s| s.to_vec()) {
            if !effective.contains_key(&s) {
                tracing::warn!(
                    "gesture \"{}\" uses symbol \"{s}\", which has no templates; record it with `glyphpad record {s}`",
                    g.def.name
                );
            }
        }
    }
    Ok((loaded.config, templates))
}

fn record(
    explicit: Option<&Path>,
    toplevel_window: bool,
    name: &str,
    count: usize,
    replace: bool,
) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        bail!("the symbol name must not be empty");
    }
    let location = config_file::locate(explicit)?;
    let (config, mut templates) = load_config(explicit)?;
    let rx = live::start(toplevel_window)?;
    eprintln!(
        "Recording {count} example(s) of \"{name}\" with draw mode {:?}. Gestures are suspended.",
        config.settings.draw_activation
    );
    let samples = engine_loop::record(rx, &config, count)?;

    let assessment = glyphpad_core::symbols::assess(name, &samples, &templates.effective());
    println!("\nQuality check:");
    let mean = assessment.consistency.iter().sum::<f32>() / assessment.consistency.len() as f32;
    for (i, (c, near)) in assessment
        .consistency
        .iter()
        .zip(&assessment.nearest_other)
        .enumerate()
    {
        let near = near.as_ref().map_or(String::new(), |n| {
            format!(", most similar other symbol: {} {:.2}", n.symbol, n.score)
        });
        // An example well below the others was probably drawn differently.
        let odd = if count > 2 && *c < mean - 0.1 {
            "  ← unlike the others; consider recording again"
        } else {
            ""
        };
        println!("  sample {}: consistency {c:.2}{near}{odd}", i + 1);
    }
    if let Some(n) = assessment
        .nearest_other
        .iter()
        .flatten()
        .filter(|n| n.score >= config.settings.default_confidence)
        .max_by(|a, b| a.score.total_cmp(&b.score))
    {
        println!(
            "  warning: \"{name}\" may be confused with \"{}\" (score {:.2} ≥ confidence {:.2})",
            n.symbol, n.score, config.settings.default_confidence
        );
    }

    let entry = templates.symbols.entry(name.to_string()).or_default();
    if replace {
        entry.clear();
    }
    entry.extend(samples);
    let total = entry.len();
    let path = config_file::templates_path(&location);
    config_file::write_atomically(&path, &templates.to_json())?;
    println!(
        "\nSaved {count} example(s) of \"{name}\" ({total} in total) to {}",
        path.display()
    );
    Ok(())
}

fn check_config(explicit: Option<&Path>) -> Result<()> {
    let location = config_file::locate(explicit)?;
    let loaded = config_file::load(&location, false)?;
    println!(
        "{}: OK, {} gesture(s)",
        location.path().display(),
        loaded.config.gestures.len()
    );
    for w in &loaded.warnings {
        println!("  warning: {w}");
    }
    Ok(())
}

fn capture(
    path: &Path,
    seconds: Option<f64>,
    force: bool,
    verbose: bool,
    toplevel_window: bool,
) -> Result<()> {
    if path.exists() && !force {
        bail!(
            "{} already exists (use --force to overwrite)",
            path.display()
        );
    }
    let file = File::create(path).with_context(|| format!("cannot create {}", path.display()))?;
    let mut writer = CaptureWriter::new(BufWriter::new(file), VERSION)?;
    let rx = live::start(toplevel_window)?;

    let deadline = seconds.map(|s| Instant::now() + Duration::from_secs_f64(s));
    eprintln!(
        "Recording to {}{}. Touch the touchpad now.",
        path.display(),
        match seconds {
            Some(s) => format!(" for {s} s"),
            None => " until Ctrl+C".to_string(),
        }
    );
    // The frames go to the file as they arrive: only the summary is kept, so
    // a long recording costs neither memory nor a second pass over everything.
    let mut stats = SessionStats::default();
    let mut last_report = Instant::now();
    loop {
        let timeout = deadline.map_or(Duration::from_millis(500), |d| {
            d.saturating_duration_since(Instant::now())
                .min(Duration::from_millis(500))
        });
        match rx.recv_timeout(timeout) {
            Ok(event) => {
                writer.write(&event)?;
                if verbose {
                    print::event(&event);
                }
                stats.add(&event);
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                bail!("the touch source stopped")
            }
        }
        if last_report.elapsed() >= Duration::from_secs(1) {
            eprintln!(
                "  {} frames, max {} fingers",
                stats.frames, stats.max_touching
            );
            last_report = Instant::now();
        }
        if deadline.is_some_and(|d| Instant::now() >= d) {
            break;
        }
    }
    stats.finish();
    eprintln!("\nSaved {}.\n{stats}", path.display());
    Ok(())
}

fn replay(
    path: &Path,
    config: &Config,
    templates: &std::collections::BTreeMap<String, Vec<glyphpad_core::symbols::Sample>>,
    all: bool,
    frames: bool,
) -> Result<()> {
    let file = File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    let capture = Capture::read(BufReader::new(file))
        .with_context(|| format!("invalid capture {}", path.display()))?;
    println!(
        "Capture recorded by GlyphPad {}, {} events",
        capture.header.glyphpad_version,
        capture.events.len()
    );
    if frames {
        capture.events.iter().for_each(print::event);
    }
    let dispatched = engine_loop::evaluate(&capture.events, config, templates, all);
    println!("\nRecognized ({}):", dispatched.len());
    for d in &dispatched {
        let gesture = d.gesture.map(|i| &config.gestures[i]);
        println!("{}", print::recognition(&d.recognition, gesture));
    }
    println!("\n{}", SessionStats::from_events(&capture.events));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_is_well_formed() {
        Cli::command().debug_assert();
    }
}
