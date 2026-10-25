//! Local usage statistics (SPEC §13): how often each gesture ran and how
//! many drawings were not recognized, to see what to improve. The numbers
//! live in `stats.json` next to the configuration and never leave the machine.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Usage {
    /// Times each gesture's action ran, by gesture name.
    pub gestures: BTreeMap<String, u64>,
    /// Drawings that matched no symbol well enough.
    pub unrecognized: u64,
    /// Symbols that were recognized but have no gesture in the active profile.
    pub unmapped: u64,
    /// Date the counting started (`YYYY-MM-DD`).
    pub since: Option<String>,
}

impl Usage {
    pub fn record_run(&mut self, gesture: &str) {
        *self.gestures.entry(gesture.to_string()).or_default() += 1;
    }

    pub fn total_runs(&self) -> u64 {
        self.gestures.values().sum()
    }

    pub fn from_json(text: &str) -> Self {
        serde_json::from_str(text).unwrap_or_default()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_and_round_trips() {
        let mut u = Usage::default();
        u.record_run("lock");
        u.record_run("lock");
        u.record_run("close");
        u.unrecognized = 1;
        assert_eq!(u.total_runs(), 3);
        assert_eq!(Usage::from_json(&u.to_json()), u);
        assert_eq!(Usage::from_json("garbage"), Usage::default());
    }
}
