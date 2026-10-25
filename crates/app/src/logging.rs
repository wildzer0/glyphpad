//! Where the log goes (SPEC §10.4): the terminal, and in portable mode a
//! rotated file next to the configuration.
//!
//! A portable program has no installer and no event log entry of its own, so
//! the only place its diagnostics can be found later is next to itself. The
//! file is rotated by size, keeping one previous copy, so it can never grow
//! without bound.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Rotate after this much has been written to one file.
const MAX_BYTES: u64 = 1024 * 1024;
/// Name of the log next to the configuration.
pub const FILE_NAME: &str = "glyphpad.log";

struct Log {
    path: PathBuf,
    file: File,
    written: u64,
}

/// Writes every record to `stderr` and, when there is one, to the log file.
#[derive(Clone, Default)]
pub struct Tee {
    log: Option<Arc<Mutex<Log>>>,
}

impl Tee {
    /// Only the terminal.
    pub fn terminal() -> Self {
        Self::default()
    }

    /// The terminal and `<dir>/glyphpad.log`, appending to what is there.
    pub fn with_file(dir: &Path) -> io::Result<Self> {
        let path = dir.join(FILE_NAME);
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        let written = file.metadata().map(|m| m.len()).unwrap_or(0);
        let log = Log {
            path,
            file,
            written,
        };
        Ok(Self {
            log: Some(Arc::new(Mutex::new(log))),
        })
    }

    /// The log file, for a line in the status bar.
    pub fn path(&self) -> Option<PathBuf> {
        self.log
            .as_ref()
            .and_then(|l| l.lock().ok().map(|l| l.path.clone()))
    }
}

impl Write for Tee {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let _ = io::stderr().write_all(buf);
        if let Some(log) = &self.log
            && let Ok(mut log) = log.lock()
        {
            if log.written > 0 && log.written + buf.len() as u64 > MAX_BYTES {
                rotate(&mut log)?;
            }
            match log.file.write_all(buf) {
                Ok(()) => log.written += buf.len() as u64,
                // A full disk must not take the program down: the terminal
                // already has the record.
                Err(e) => return Err(e),
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        let _ = io::stderr().flush();
        if let Some(log) = &self.log
            && let Ok(mut log) = log.lock()
        {
            log.file.flush()?;
        }
        Ok(())
    }
}

/// Keeps the current log as `glyphpad.log.1` and starts a new one.
fn rotate(log: &mut Log) -> io::Result<()> {
    let previous = log.path.with_extension("log.1");
    let _ = std::fs::remove_file(&previous);
    let _ = std::fs::rename(&log.path, &previous);
    log.file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log.path)?;
    log.written = 0;
    Ok(())
}

impl<'a> tracing_subscriber::fmt::writer::MakeWriter<'a> for Tee {
    type Writer = Tee;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

/// Installs the global subscriber: `RUST_LOG` or `info`, to the terminal and
/// to `location`'s folder when it is a portable one.
pub fn init(location: Option<&crate::config_file::Location>) {
    // Beside the configuration, wherever that is: a windowed program has no
    // terminal to log into, and "send me the log" needs a file to send.
    let writer = match location.and_then(|l| l.path().parent()) {
        Some(dir) => Tee::with_file(dir).unwrap_or_else(|e| {
            eprintln!("cannot write the log file: {e}");
            Tee::terminal()
        }),
        None => Tee::terminal(),
    };
    let verbose = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into());
    let path = writer.path();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new(verbose))
        .with_writer(writer)
        .with_ansi(false)
        .init();
    if let Some(path) = path {
        tracing::info!("logging to {}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("glyphpad-log-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn writes_to_the_file_and_rotates_once_it_is_full() {
        let dir = scratch("rotate");
        let mut tee = Tee::with_file(&dir).unwrap();
        let line = vec![b'x'; 64 * 1024];
        for _ in 0..(MAX_BYTES / line.len() as u64 + 2) {
            tee.write_all(&line).unwrap();
        }
        tee.flush().unwrap();
        let log = dir.join(FILE_NAME);
        let previous = dir.join("glyphpad.log.1");
        assert!(log.is_file() && previous.is_file());
        assert!(std::fs::metadata(&log).unwrap().len() < MAX_BYTES);
        assert!(std::fs::metadata(&previous).unwrap().len() >= MAX_BYTES / 2);
        assert_eq!(tee.path(), Some(log));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn appending_continues_a_previous_log() {
        let dir = scratch("append");
        let path = dir.join(FILE_NAME);
        std::fs::write(&path, "earlier\n").unwrap();
        let mut tee = Tee::with_file(&dir).unwrap();
        tee.write_all(b"later\n").unwrap();
        tee.flush().unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("earlier") && text.contains("later"), "{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
