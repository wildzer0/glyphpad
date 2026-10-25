//! Finding and loading `glyphpad.toml` (SPEC §2 portable mode, §9).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use glyphpad_core::config::{self, EXAMPLE_CONFIG, Loaded};
use glyphpad_core::symbols::TemplateFile;

pub const FILE_NAME: &str = "glyphpad.toml";

#[derive(Debug, Clone, PartialEq)]
pub enum Location {
    /// Given with `--config`.
    Explicit(PathBuf),
    /// Next to the executable: portable mode.
    Portable(PathBuf),
    /// The OS config directory.
    Standard(PathBuf),
}

impl Location {
    pub fn path(&self) -> &Path {
        match self {
            Location::Explicit(p) | Location::Portable(p) | Location::Standard(p) => p,
        }
    }

    pub fn describe(&self) -> String {
        let kind = match self {
            Location::Explicit(_) => "given on the command line",
            Location::Portable(_) => "portable mode",
            Location::Standard(_) => "user config directory",
        };
        format!("{} ({kind})", self.path().display())
    }
}

pub fn locate(explicit: Option<&Path>) -> Result<Location> {
    if let Some(p) = explicit {
        return Ok(Location::Explicit(p.to_path_buf()));
    }
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    {
        let portable = dir.join(FILE_NAME);
        if portable.is_file() {
            return Ok(Location::Portable(portable));
        }
    }
    let dirs = directories::ProjectDirs::from("", "", "GlyphPad")
        .context("cannot determine the user's config directory")?;
    Ok(Location::Standard(dirs.config_dir().join(FILE_NAME)))
}

/// Reads and validates the configuration. In the standard location a
/// missing file is created from the example on first start.
pub fn load(location: &Location, create_if_missing: bool) -> Result<Loaded> {
    let path = location.path();
    if !path.exists() {
        match location {
            Location::Standard(_) if create_if_missing => {
                if let Some(dir) = path.parent() {
                    std::fs::create_dir_all(dir)
                        .with_context(|| format!("cannot create {}", dir.display()))?;
                }
                std::fs::write(path, EXAMPLE_CONFIG)
                    .with_context(|| format!("cannot write {}", path.display()))?;
                tracing::info!("created the default configuration at {}", path.display());
            }
            _ => bail!("{} does not exist", path.display()),
        }
    }
    let text =
        std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    config::parse(&text).map_err(|errors| {
        let list: Vec<String> = errors.iter().map(|e| format!("  {e}")).collect();
        anyhow::anyhow!(
            "invalid configuration {}:\n{}",
            path.display(),
            list.join("\n")
        )
    })
}

pub fn log_warnings(loaded: &Loaded) {
    for w in &loaded.warnings {
        tracing::warn!("config: {w}");
    }
}

pub const TEMPLATES_FILE_NAME: &str = "templates.json";
pub const HISTORY_FILE_NAME: &str = "history.jsonl";

/// `history.jsonl`, when `save_history` is on, sits next to the configuration.
pub fn history_path(location: &Location) -> PathBuf {
    location.path().parent().map_or_else(
        || PathBuf::from(HISTORY_FILE_NAME),
        |d| d.join(HISTORY_FILE_NAME),
    )
}

/// `templates.json` sits next to the configuration it belongs to.
/// Where the updater keeps what it remembers, next to the configuration.
pub fn update_state_path(location: &Location) -> PathBuf {
    location.path().parent().map_or_else(
        || PathBuf::from("update-state.json"),
        |d| d.join("update-state.json"),
    )
}

pub fn templates_path(location: &Location) -> PathBuf {
    location.path().parent().map_or_else(
        || PathBuf::from(TEMPLATES_FILE_NAME),
        |d| d.join(TEMPLATES_FILE_NAME),
    )
}

/// The user's templates; a missing file means none yet. A damaged file is
/// reported and ignored rather than stopping the program (SPEC §9).
pub fn load_templates(location: &Location) -> TemplateFile {
    let path = templates_path(location);
    match std::fs::read_to_string(&path) {
        Ok(text) => TemplateFile::parse(&text).unwrap_or_else(|e| {
            tracing::error!("ignoring {}: {e}", path.display());
            TemplateFile::default()
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => TemplateFile::default(),
        Err(e) => {
            tracing::error!("cannot read {}: {e}", path.display());
            TemplateFile::default()
        }
    }
}

/// Writes atomically: a temporary file next to the target, then a rename,
/// so a crash never leaves a half-written file.
pub fn write_atomically(path: &Path, contents: &str) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, contents).with_context(|| format!("cannot write {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("cannot replace {}", path.display()))?;
    Ok(())
}

/// `backups/` next to the configuration (SPEC §10.2).
pub fn backups_dir(location: &Location) -> PathBuf {
    location
        .path()
        .parent()
        .map_or_else(|| PathBuf::from("backups"), |d| d.join("backups"))
}

/// Copies the current configuration into `backups/` before it is
/// overwritten, keeping the `keep` most recent copies.
pub fn backup(location: &Location, keep: usize) -> Result<()> {
    let path = location.path();
    if keep == 0 || !path.exists() {
        return Ok(());
    }
    let dir = backups_dir(location);
    std::fs::create_dir_all(&dir).with_context(|| format!("cannot create {}", dir.display()))?;
    let stamp = jiff::Zoned::now().strftime("%Y%m%d-%H%M%S").to_string();
    // A counter, always present, keeps names in time order within a second.
    let mut n = 1;
    let mut target = dir.join(format!("glyphpad-{stamp}-{n:03}.toml"));
    while target.exists() {
        n += 1;
        target = dir.join(format!("glyphpad-{stamp}-{n:03}.toml"));
    }
    std::fs::copy(path, &target)
        .with_context(|| format!("cannot back up to {}", target.display()))?;
    for old in list_backups(location).into_iter().skip(keep) {
        let _ = std::fs::remove_file(dir.join(old));
    }
    Ok(())
}

/// Like [`backup`], but does nothing when the newest backup is younger than
/// `min_age`: a burst of automatic saves from the editor keeps the version
/// from before the burst instead of flooding `backups/`.
pub fn backup_unless_recent(
    location: &Location,
    keep: usize,
    min_age: std::time::Duration,
) -> Result<()> {
    let newest = list_backups(location).into_iter().next();
    let recent = newest
        .and_then(|name| std::fs::metadata(backups_dir(location).join(name)).ok())
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.elapsed().ok())
        .is_some_and(|age| age < min_age);
    if recent {
        return Ok(());
    }
    backup(location, keep)
}

/// Backup file names, newest first (their names sort by time).
pub fn list_backups(location: &Location) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(backups_dir(location)) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.starts_with("glyphpad-") && n.ends_with(".toml"))
        .collect();
    names.sort_by(|a, b| b.cmp(a));
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backups_are_pruned_to_the_newest() {
        let dir = std::env::temp_dir().join(format!("glyphpad-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let location = Location::Explicit(dir.join(FILE_NAME));
        std::fs::write(location.path(), "config_version = 1\n").unwrap();
        for _ in 0..4 {
            backup(&location, 3).unwrap();
        }
        let names = list_backups(&location);
        assert_eq!(names.len(), 3, "{names:?}");
        assert!(
            names.windows(2).all(|w| w[0] > w[1]),
            "newest first: {names:?}"
        );
        backup(&location, 0).unwrap();
        assert_eq!(list_backups(&location).len(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_burst_of_saves_shares_one_backup() {
        let dir = std::env::temp_dir().join(format!("glyphpad-burst-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let location = Location::Explicit(dir.join(FILE_NAME));
        std::fs::write(location.path(), "config_version = 1\n").unwrap();
        let minute = std::time::Duration::from_secs(60);
        for _ in 0..3 {
            backup_unless_recent(&location, 10, minute).unwrap();
        }
        assert_eq!(list_backups(&location).len(), 1);
        // With no minimum age every save backs up.
        backup_unless_recent(&location, 10, std::time::Duration::ZERO).unwrap();
        assert_eq!(list_backups(&location).len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
