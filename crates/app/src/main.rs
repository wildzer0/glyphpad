// SPEC §10.4: no console window for the release binary on Windows; CLI
// subcommands attach to the parent's console instead.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod config_file;
mod engine_loop;
mod live;
mod print;

use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use glyphpad_core::capture::{Capture, CaptureWriter};
use glyphpad_core::config::Config;
use glyphpad_core::engine::resolve;
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
    /// Print live recognized gestures without executing them (the GUI debug
    /// tab comes in M4).
    Debug {
        /// Also print every touch frame.
        #[arg(long)]
        frames: bool,
    },
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
    /// Validate a configuration file and print its errors and warnings.
    CheckConfig {
        /// Defaults to the configuration GlyphPad would use.
        file: Option<PathBuf>,
    },
}

fn main() {
    #[cfg(windows)]
    glyphpad_platform::windows::console::attach_parent_console();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    if let Err(e) = run(Cli::parse()) {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    let toplevel_window = cli.toplevel_window;
    let config_arg = cli.config.as_deref();
    match cli.command.unwrap_or(Cmd::Run) {
        Cmd::Run => {
            let (config, templates) = load_config(config_arg)?;
            let rx = live::start(toplevel_window)?;
            engine_loop::run(rx, config, &templates.effective(), engine_loop::Mode::Run)
        }
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
        Cmd::Debug { frames } => {
            let (config, templates) = load_config(config_arg)?;
            let rx = live::start(toplevel_window)?;
            eprintln!("Showing recognized gestures (not executed). Stop with Ctrl+C.");
            engine_loop::run(
                rx,
                config,
                &templates.effective(),
                engine_loop::Mode::Debug { frames },
            )
        }
        Cmd::Record {
            name,
            samples,
            replace,
        } => record(config_arg, toplevel_window, &name, samples.into(), replace),
        Cmd::CheckConfig { file } => check_config(file.as_deref().or(config_arg)),
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
    let mut stats = SessionStats::default();
    let mut events = Vec::new();
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
                events.push(event);
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                bail!("the touch source stopped")
            }
        }
        if last_report.elapsed() >= Duration::from_secs(1) {
            stats = SessionStats::from_events(&events);
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
    if stats.frames != events.len() {
        stats = SessionStats::from_events(&events);
    }
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
    let recognitions = engine_loop::evaluate(&capture.events, config, templates, all);
    println!("\nRecognized ({}):", recognitions.len());
    for r in &recognitions {
        println!(
            "{}",
            print::recognition(r, resolve(config, &r.observation, None))
        );
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
