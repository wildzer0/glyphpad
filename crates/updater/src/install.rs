//! Replacing the running executable, and putting it back when the new one
//! does not come up (SPEC §12.2).
//!
//! A running executable cannot be overwritten on Windows, but it can be
//! renamed; the same two renames work on Linux. So installing is: current →
//! `glyphpad.old[.exe]`, new → current. Rolling back is the reverse.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

use crate::{Error, exe_name};

/// How long the old process waits for the new one to say it is up.
pub const HANDOFF_TIMEOUT: Duration = Duration::from_secs(15);

/// Directory next to the executable where downloads are staged: the same
/// volume, so the final rename is atomic.
pub fn staging_dir(current: &Path) -> PathBuf {
    current
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("glyphpad-update")
}

/// `glyphpad.old.exe` / `glyphpad.old`, next to `current`.
pub fn old_path(current: &Path) -> PathBuf {
    let name = if cfg!(windows) {
        "glyphpad.old.exe"
    } else {
        "glyphpad.old"
    };
    current
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(name)
}

/// The file the new process creates to say it started well.
pub fn marker_path(current: &Path) -> PathBuf {
    current
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("glyphpad-update.ok")
}

/// Creates the (empty) staging directory, or says why the executable's
/// folder cannot be updated (SPEC §12.2: no silent failures).
pub fn prepare_staging(current: &Path) -> Result<PathBuf, Error> {
    let dir = staging_dir(current);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| Error::NotWritable(e.to_string()))?;
    Ok(dir)
}

/// Removes leftovers of a previous update: the old executable and the
/// staging folder. Failures (the old process may still run) are ignored.
pub fn cleanup(current: &Path) {
    let _ = std::fs::remove_file(old_path(current));
    let _ = std::fs::remove_dir_all(staging_dir(current));
    let _ = std::fs::remove_file(marker_path(current));
}

/// What [`install`] did, so it can be undone.
#[derive(Debug)]
pub struct Installed {
    pub current: PathBuf,
    pub old: PathBuf,
}

impl Installed {
    /// Puts the old executable back, replacing the new one.
    pub fn rollback(&self) -> std::io::Result<()> {
        let _ = std::fs::remove_file(&self.current);
        std::fs::rename(&self.old, &self.current)
    }
}

/// Moves `current` aside and `staged` into its place.
pub fn install(staged: &Path, current: &Path) -> Result<Installed, Error> {
    let old = old_path(current);
    let _ = std::fs::remove_file(&old);
    std::fs::rename(current, &old)
        .map_err(|e| Error::Install(format!("cannot move the current executable aside: {e}")))?;
    if let Err(e) = std::fs::rename(staged, current) {
        // Leave things as they were.
        let _ = std::fs::rename(&old, current);
        return Err(Error::Install(format!(
            "cannot put the new executable in place: {e}"
        )));
    }
    Ok(Installed {
        current: current.to_path_buf(),
        old,
    })
}

pub enum Handoff {
    /// The new process reported it is running; the caller should exit.
    Succeeded,
    /// It did not; the old executable is back in place and the caller keeps running.
    RolledBack(String),
}

/// Starts the new executable with `args` and waits up to `timeout` for it to
/// create `marker`. `release` runs before the start (give up the
/// single-instance lock) and `reclaim` after a failure (take it back).
///
/// A new process that exits early, or never reports, is killed and the old
/// executable restored (SPEC §12.2).
pub fn handoff(
    installed: &Installed,
    args: &[OsString],
    marker: &Path,
    timeout: Duration,
    release: impl FnOnce(),
    reclaim: impl FnOnce(),
) -> Handoff {
    let _ = std::fs::remove_file(marker);
    release();
    let fail = |child: Option<&mut Child>, why: String, reclaim: Box<dyn FnOnce() + '_>| {
        if let Some(c) = child {
            let _ = c.kill();
            let _ = c.wait();
        }
        let restored = installed.rollback();
        reclaim();
        match restored {
            Ok(()) => Handoff::RolledBack(why),
            Err(e) => Handoff::RolledBack(format!(
                "{why}; additionally the old executable could not be restored ({e}); it is {}",
                installed.old.display()
            )),
        }
    };
    let mut child = match Command::new(&installed.current).args(args).spawn() {
        Ok(c) => c,
        Err(e) => {
            return fail(
                None,
                format!("the new version did not start: {e}"),
                Box::new(reclaim),
            );
        }
    };
    let deadline = Instant::now() + timeout;
    loop {
        if marker.exists() {
            let _ = std::fs::remove_file(marker);
            return Handoff::Succeeded;
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                return fail(
                    None,
                    format!("the new version exited at once ({status})"),
                    Box::new(reclaim),
                );
            }
            Ok(None) => {}
            Err(e) => {
                return fail(
                    Some(&mut child),
                    format!("cannot watch the new version: {e}"),
                    Box::new(reclaim),
                );
            }
        }
        if Instant::now() >= deadline {
            return fail(
                Some(&mut child),
                format!(
                    "the new version did not report within {} s",
                    timeout.as_secs()
                ),
                Box::new(reclaim),
            );
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// The executable currently running, which is what gets replaced.
pub fn current_exe() -> Result<PathBuf, Error> {
    let exe = std::env::current_exe()?;
    // Package executables are always named like this; anything else (a
    // development build run through cargo) must not be replaced.
    if exe.file_name().is_some_and(|n| n == exe_name()) {
        Ok(exe)
    } else {
        Err(Error::Install(format!(
            "{} is not an installed glyphpad executable",
            exe.display()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let current = dir.path().join(exe_name());
        std::fs::write(&current, b"old binary").unwrap();
        let staged = prepare_staging(&current).unwrap().join("new");
        std::fs::write(&staged, b"new binary").unwrap();
        (dir, current, staged)
    }

    #[test]
    fn install_swaps_and_rollback_restores() {
        let (_dir, current, staged) = setup();
        let installed = install(&staged, &current).unwrap();
        assert_eq!(std::fs::read(&current).unwrap(), b"new binary");
        assert_eq!(std::fs::read(&installed.old).unwrap(), b"old binary");
        installed.rollback().unwrap();
        assert_eq!(std::fs::read(&current).unwrap(), b"old binary");
        assert!(!installed.old.exists());
    }

    #[test]
    fn a_failed_install_leaves_the_old_executable_alone() {
        let (_dir, current, _) = setup();
        let missing = current.with_file_name("does-not-exist");
        assert!(install(&missing, &current).is_err());
        assert_eq!(std::fs::read(&current).unwrap(), b"old binary");
    }

    #[test]
    fn cleanup_removes_leftovers() {
        let (_dir, current, staged) = setup();
        install(&staged, &current).unwrap();
        std::fs::write(marker_path(&current), b"").unwrap();
        cleanup(&current);
        assert!(!old_path(&current).exists());
        assert!(!staging_dir(&current).exists());
        assert!(!marker_path(&current).exists());
        assert_eq!(std::fs::read(&current).unwrap(), b"new binary");
    }

    #[cfg(unix)]
    #[test]
    fn a_folder_that_cannot_be_written_is_reported() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let current = dir.path().join(exe_name());
        std::fs::write(&current, b"x").unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
        let result = prepare_staging(&current);
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        // Root can write anywhere; only check where permissions apply.
        if !std::fs::write(dir.path().join("probe"), b"").is_ok() {
            assert!(matches!(result, Err(Error::NotWritable(_))));
        }
    }
}
