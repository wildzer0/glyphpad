// SPEC §10.4: no console window for the release binary on Windows; CLI
// subcommands attach to the parent's console instead.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod live;
mod print;

use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use glyphpad_core::DeviceEvent;
use glyphpad_core::TouchSource;
use glyphpad_core::capture::{Capture, CaptureWriter};
use glyphpad_core::replay::{Pacing, ReplaySource};
use glyphpad_core::stats::SessionStats;

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
    /// Play back a capture and print its frames and statistics.
    Replay {
        file: PathBuf,
        /// Reproduce the recorded timing instead of playing instantly.
        #[arg(long)]
        realtime: bool,
        /// Playback speed with --realtime.
        #[arg(long, default_value_t = 1.0)]
        speed: f32,
        /// Only print the statistics.
        #[arg(long)]
        stats_only: bool,
    },
    /// Print the HID capabilities of every Precision Touchpad.
    DumpDescriptor,
    /// Print live touch frames (the GUI debug tab comes in M4).
    Debug,
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
    match cli.command.unwrap_or(Cmd::Run) {
        Cmd::Run => {
            println!("GlyphPad {VERSION}: the gesture engine arrives in M2. Try `glyphpad debug`.");
            Ok(())
        }
        Cmd::Capture {
            file,
            seconds,
            force,
            verbose,
        } => capture(&file, seconds, force, verbose, toplevel_window),
        Cmd::Replay {
            file,
            realtime,
            speed,
            stats_only,
        } => replay(&file, realtime, speed, stats_only),
        Cmd::DumpDescriptor => {
            print!("{}", live::dump_descriptors()?);
            Ok(())
        }
        Cmd::Debug => debug(toplevel_window),
    }
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

fn replay(path: &Path, realtime: bool, speed: f32, stats_only: bool) -> Result<()> {
    let file = File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    let capture = Capture::read(BufReader::new(file))
        .with_context(|| format!("invalid capture {}", path.display()))?;
    println!(
        "Capture recorded by GlyphPad {}, {} events",
        capture.header.glyphpad_version,
        capture.events.len()
    );
    let stats = SessionStats::from_events(&capture.events);
    if !stats_only {
        let pacing = if realtime {
            Pacing::RealTime { speed }
        } else {
            Pacing::Instant
        };
        let (tx, rx) = crossbeam_channel::unbounded();
        let source = ReplaySource::new(capture, pacing);
        let player = std::thread::spawn(move || source.run(tx));
        for event in rx {
            print::event(&event);
        }
        player
            .join()
            .map_err(|_| anyhow::anyhow!("replay thread panicked"))?
            .map_err(|e| anyhow::anyhow!(e))?;
    }
    println!("\n{stats}");
    Ok(())
}

fn debug(toplevel_window: bool) -> Result<()> {
    let rx = live::start(toplevel_window)?;
    eprintln!("Showing live touch frames. Stop with Ctrl+C.");
    for event in rx {
        if let DeviceEvent::Connected(_) | DeviceEvent::Disconnected { .. } = event {
            eprintln!();
        }
        print::event(&event);
    }
    bail!("the touch source stopped")
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
