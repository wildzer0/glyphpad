//! Local usage statistics (SPEC §13): how often each gesture ran and how
//! many drawings were not recognized, to see what to improve. The numbers
//! live in `stats.json` next to the configuration and never leave the machine.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Usage {
    /// Times each gesture's action ran, by gesture name.
    pub gestures: BTreeMap<String, u64>,
    /// Drawings that matched no symbol well enough.
    pub unrecognized: u64,
    /// The symbol each of those drawings came closest to, with how often and
    /// how close: the hint for tuning the symbol that is being drawn badly.
    pub unrecognized_candidates: BTreeMap<String, BestCandidate>,
    /// Symbols that were recognized but have no gesture in the active profile.
    pub unmapped: u64,
    /// Date the counting started (`YYYY-MM-DD`).
    pub since: Option<String>,
}

/// How close the unrecognized drawings came to one symbol.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BestCandidate {
    pub count: u64,
    /// Score of the latest one, 0–1.
    pub last_score: f32,
}

impl Usage {
    /// Counts a drawing that was not recognized, with the symbol it came
    /// closest to and how close (SPEC §13: what to tune).
    pub fn record_unrecognized(&mut self, best: Option<(&str, f32)>) {
        self.unrecognized += 1;
        if let Some((symbol, score)) = best {
            let entry = self
                .unrecognized_candidates
                .entry(symbol.to_string())
                .or_default();
            entry.count += 1;
            entry.last_score = score;
        }
    }

    /// The candidates, the most frequent first, at most `limit` of them.
    pub fn closest_symbols(&self, limit: usize) -> Vec<(&str, BestCandidate)> {
        let mut rows: Vec<(&str, BestCandidate)> = self
            .unrecognized_candidates
            .iter()
            .map(|(name, c)| (name.as_str(), c.clone()))
            .collect();
        rows.sort_by(|a, b| b.1.count.cmp(&a.1.count).then(a.0.cmp(b.0)));
        rows.truncate(limit);
        rows
    }

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
        u.record_unrecognized(Some(("X", 0.74)));
        u.record_unrecognized(Some(("X", 0.71)));
        u.record_unrecognized(Some(("q", 0.69)));
        u.record_unrecognized(None);
        assert_eq!(u.unrecognized, 4);
        let best = u.closest_symbols(5);
        assert_eq!(best[0].0, "X");
        assert_eq!(best[0].1.count, 2);
        assert!((best[0].1.last_score - 0.71).abs() < 1e-6);
        assert_eq!(best[1].0, "q");
        assert_eq!(u.closest_symbols(1).len(), 1);
        assert_eq!(u.total_runs(), 3);
        assert_eq!(Usage::from_json(&u.to_json()), u);
        assert_eq!(Usage::from_json("garbage"), Usage::default());
    }
}
