//! What the updater remembers between runs: when it last asked, the ETag
//! of the answer, and the version the user chose to ignore.

use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    /// Seconds since the Unix epoch of the last successful check.
    pub last_check: Option<u64>,
    /// The ETag of the last release list. It is only sent together with a
    /// list still in memory: a 304 carries no releases, so sending it from a
    /// fresh process would report "up to date" without having checked.
    pub etag: Option<String>,
    /// "Ignore this version" (SPEC §12.1).
    pub ignored: Option<String>,
}

impl State {
    /// A missing or unreadable file is an empty state: nothing here is
    /// worth refusing to start for.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self).unwrap_or_default())?;
        std::fs::rename(tmp, path)
    }

    /// Whether the interval since the last check has passed.
    pub fn due(&self, now: u64, interval_h: u32) -> bool {
        self.last_check
            .is_none_or(|t| now.saturating_sub(t) >= u64::from(interval_h) * 3600)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_tolerates_garbage() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("update-state.json");
        assert_eq!(State::load(&path), State::default());
        let s = State {
            last_check: Some(100),
            etag: Some("\"abc\"".into()),
            ignored: Some("0.2.0".into()),
        };
        s.save(&path).unwrap();
        assert_eq!(State::load(&path), s);
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(State::load(&path), State::default());
    }

    #[test]
    fn due_after_the_interval() {
        let s = State {
            last_check: Some(1000),
            ..Default::default()
        };
        assert!(!s.due(1000 + 3600 * 23, 24));
        assert!(s.due(1000 + 3600 * 24, 24));
        assert!(State::default().due(0, 24));
    }
}
