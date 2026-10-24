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

/// `templates.json` sits next to the configuration it belongs to.
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
