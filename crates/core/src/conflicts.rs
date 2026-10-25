//! Conflict detection (SPEC §13): gestures that the same movement can set
//! off, and symbols too similar to tell apart reliably.

use std::collections::{BTreeMap, BTreeSet};

use crate::config::{Config, Gesture};
use crate::symbols::{Sample, SymbolRecognizer};
use crate::trigger::Trigger;

#[derive(Debug, Clone, PartialEq)]
pub struct Conflict {
    /// Names of the gestures involved.
    pub gestures: Vec<String>,
    pub reason: String,
}

/// Whether two gestures can be active at the same time: same profile (or
/// none) and foreground conditions that may both hold.
fn overlap(a: &Gesture, b: &Gesture) -> bool {
    let profiles = match (&a.def.profile, &b.def.profile) {
        (Some(x), Some(y)) => x == y,
        _ => true,
    };
    let apps = |g: &Gesture| -> Option<BTreeSet<String>> {
        g.def
            .when
            .as_ref()
            .and_then(|w| w.app.as_ref())
            .map(|a| a.to_vec().into_iter().map(|s| s.to_lowercase()).collect())
    };
    // Title conditions cannot be compared; they are assumed to overlap.
    let apps_overlap = match (apps(a), apps(b)) {
        (Some(x), Some(y)) => !x.is_disjoint(&y),
        _ => true,
    };
    profiles && apps_overlap
}

/// Whether two rectangles of the pad share an area. Edges that only touch
/// do not: a tap is in one zone or the other.
fn regions_overlap(a: &crate::trigger::Region, b: &crate::trigger::Region) -> bool {
    a.x[0] < b.x[1] && b.x[0] < a.x[1] && a.y[0] < b.y[1] && b.y[0] < a.y[1]
}

/// Triggers equal as movements (a symbol's confidence does not matter), and
/// zones that overlap for the same tap.
fn same_movement(a: &Trigger, b: &Trigger) -> bool {
    match (a, b) {
        (Trigger::Symbol { names: x, .. }, Trigger::Symbol { names: y, .. }) => {
            x.iter().any(|n| y.contains(n))
        }
        (
            Trigger::Zone {
                fingers: fa,
                taps: ta,
                region: ra,
            },
            Trigger::Zone {
                fingers: fb,
                taps: tb,
                region: rb,
            },
        ) => fa == fb && ta == tb && regions_overlap(ra, rb),
        _ => a == b,
    }
}

/// How many examples of each symbol are compared: the first few are enough to
/// see whether two symbols look alike, and each comparison costs a
/// recognition against every template of the other symbol.
const SAMPLES_COMPARED: usize = 3;

/// Mean best score of `a`'s samples against `b`'s templates.
fn resemblance(a: &[Sample], b: &[Sample], b_name: &str) -> f32 {
    let r = SymbolRecognizer::new(&BTreeMap::from([(b_name.to_string(), b.to_vec())]), 0.0);
    let scores: Vec<f32> = a
        .iter()
        .take(SAMPLES_COMPARED)
        .filter_map(|s| r.recognize(&s.xy_strokes()).first().map(|c| c.score))
        .collect();
    if scores.is_empty() {
        0.0
    } else {
        scores.iter().sum::<f32>() / scores.len() as f32
    }
}

/// How close to their thresholds lookalike symbols may score before they
/// are reported: drawings vary, so a mean resemblance within 0.05 of the
/// threshold means some drawings will cross it.
const SIMILARITY_MARGIN: f32 = 0.05;

/// Every conflict: [`movements`] and [`symbols`].
pub fn find(config: &Config, templates: &BTreeMap<String, Vec<Sample>>) -> Vec<Conflict> {
    let mut out = movements(config);
    out.extend(symbols(config, templates));
    out
}

/// The conflicts that do not look at the templates: two gestures the same
/// movement can set off. Cheap — microseconds — so the window can run it
/// whenever the draft changes.
pub fn movements(config: &Config) -> Vec<Conflict> {
    let mut out = Vec::new();
    let gestures: Vec<&Gesture> = config.gestures.iter().filter(|g| g.def.enabled).collect();
    for (i, a) in gestures.iter().enumerate() {
        for b in &gestures[i + 1..] {
            if !overlap(a, b) {
                continue;
            }
            if let (Some(ta), Some(tb)) = (&a.trigger, &b.trigger) {
                if same_movement(ta, tb) {
                    let reason = match (ta, tb) {
                        (Trigger::Zone { .. }, Trigger::Zone { .. }) => format!(
                            "the zones of “{}” and “{}” overlap; only “{}” (first in the list) will ever fire",
                            a.def.name, b.def.name, a.def.name
                        ),
                        _ => format!(
                            "both trigger on “{}”; only “{}” (first in the list) will ever fire",
                            ta.describe(),
                            a.def.name
                        ),
                    };
                    out.push(Conflict {
                        gestures: vec![a.def.name.clone(), b.def.name.clone()],
                        reason,
                    });
                }
                for (hold, chord) in [(ta, tb), (tb, ta)] {
                    if let (
                        Trigger::Hold { fingers },
                        Trigger::Tap { modifiers, .. } | Trigger::Swipe { modifiers, .. },
                    ) = (hold, chord)
                        && fingers == modifiers
                    {
                        out.push(Conflict {
                            gestures: vec![a.def.name.clone(), b.def.name.clone()],
                            reason: format!(
                                "the hold with {fingers} finger(s) fires while the chord's modifier rests, before the chord"
                            ),
                        });
                    }
                }
            }
            if a.steps.len() > 1 && a.steps == b.steps {
                out.push(Conflict {
                    gestures: vec![a.def.name.clone(), b.def.name.clone()],
                    reason: "the two sequences have the same steps".into(),
                });
            }
        }
    }

    out
}

/// Lookalike symbols: the expensive half, since every pair needs the
/// recognizer run over the examples. It changes only when the symbols in use,
/// their confidence or the templates do, so the window caches it.
pub fn symbols(config: &Config, templates: &BTreeMap<String, Vec<Sample>>) -> Vec<Conflict> {
    let mut out = Vec::new();
    let gestures: Vec<&Gesture> = config.gestures.iter().filter(|g| g.def.enabled).collect();
    // Symbols used by gestures, with the lowest confidence asked for each.
    let mut used: BTreeMap<String, f32> = BTreeMap::new();
    for g in &gestures {
        if let Some(Trigger::Symbol { names, confidence }) = &g.trigger {
            for n in names {
                let c = confidence.unwrap_or(config.settings.default_confidence);
                let e = used.entry(n.clone()).or_insert(c);
                *e = e.min(c);
            }
        }
    }
    let names: Vec<&String> = used.keys().collect();
    for (i, x) in names.iter().enumerate() {
        for y in &names[i + 1..] {
            let (Some(sx), Some(sy)) = (templates.get(*x), templates.get(*y)) else {
                continue;
            };
            let threshold = used[*x].min(used[*y]);
            let score = resemblance(sx, sy, y).max(resemblance(sy, sx, x));
            if score >= threshold - SIMILARITY_MARGIN {
                out.push(Conflict {
                    gestures: gestures
                        .iter()
                        .filter(|g| matches!(&g.trigger, Some(Trigger::Symbol { names, .. }) if names.contains(x) || names.contains(y)))
                        .map(|g| g.def.name.clone())
                        .collect(),
                    reason: format!(
                        "the symbols “{x}” and “{y}” look alike (score {score:.2}, threshold {threshold:.2}); record more distinct examples"
                    ),
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::parse;
    use crate::symbols::templates::defaults;

    fn conflicts(text: &str) -> Vec<Conflict> {
        find(&parse(text).unwrap().config, &defaults())
    }

    const TAP3: &str = "type = \"tap\"\nfingers = 3\naction = { type = \"lock_screen\" }";

    #[test]
    fn same_movement_overlapping_conditions() {
        let c = conflicts(&format!(
            "[[gesture]]\nname = \"a\"\n{TAP3}\n[[gesture]]\nname = \"b\"\n{TAP3}\n"
        ));
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].gestures, ["a", "b"]);

        // Different apps: no conflict.
        let c = conflicts(&format!(
            "[[gesture]]\nname = \"a\"\n{TAP3}\nwhen = {{ app = \"x.exe\" }}\n[[gesture]]\nname = \"b\"\n{TAP3}\nwhen = {{ app = \"y.exe\" }}\n"
        ));
        assert!(c.is_empty());
        // Different profiles: no conflict.
        let c = conflicts(&format!(
            "[[gesture]]\nname = \"a\"\n{TAP3}\nprofile = \"p\"\n[[gesture]]\nname = \"b\"\n{TAP3}\nprofile = \"q\"\n"
        ));
        assert!(c.is_empty());
    }

    #[test]
    fn the_two_halves_split_the_conflicts() {
        let config = parse(&format!(
            "[[gesture]]\nname = \"a\"\n{TAP3}\n[[gesture]]\nname = \"b\"\n{TAP3}\n"
        ))
        .unwrap()
        .config;
        assert_eq!(movements(&config).len(), 1);
        assert!(symbols(&config, &defaults()).is_empty());
        assert_eq!(find(&config, &defaults()).len(), 1);
    }

    #[test]
    fn zones_that_overlap_conflict() {
        let zone = |name: &str, x: [f32; 2], y: [f32; 2]| {
            format!(
                "[[gesture]]\nname = \"{name}\"\ntype = \"zone\"\nfingers = 1\nregion = {{ x = [{}, {}], y = [{}, {}] }}\naction = {{ type = \"lock_screen\" }}\n",
                x[0], x[1], y[0], y[1]
            )
        };
        // Sharing part of the pad: a tap there could be either.
        let c = conflicts(
            &(zone("a", [0.0, 25.0], [0.0, 20.0]) + &zone("b", [20.0, 45.0], [0.0, 20.0])),
        );
        assert_eq!(c.len(), 1, "{c:?}");
        assert!(c[0].reason.contains("overlap"), "{}", c[0].reason);
        // Apart: no conflict.
        let c = conflicts(
            &(zone("a", [0.0, 25.0], [0.0, 20.0]) + &zone("b", [30.0, 55.0], [0.0, 20.0])),
        );
        assert!(c.is_empty(), "{c:?}");
        // Touching edges are not overlapping.
        let c = conflicts(
            &(zone("a", [0.0, 25.0], [0.0, 20.0]) + &zone("b", [25.0, 50.0], [0.0, 20.0])),
        );
        assert!(c.is_empty(), "{c:?}");
        // Different finger counts never fire on one tap.
        let c = conflicts(
            &(zone("a", [0.0, 25.0], [0.0, 20.0])
                + "[[gesture]]\nname = \"b\"\ntype = \"zone\"\nfingers = 2\nregion = { x = [0.0, 25.0], y = [0.0, 20.0] }\naction = { type = \"lock_screen\" }\n"),
        );
        assert!(c.is_empty(), "{c:?}");
    }

    #[test]
    fn hold_versus_chord() {
        let c = conflicts(
            r#"
[[gesture]]
name = "hold"
type = "hold"
fingers = 1
action = { type = "lock_screen" }

[[gesture]]
name = "chord"
type = "swipe"
fingers = 2
modifier_fingers = 1
direction = "up"
action = { type = "lock_screen" }
"#,
        );
        assert_eq!(c.len(), 1, "{c:?}");
    }

    #[test]
    fn lookalike_symbols() {
        let c = conflicts(
            r#"
[[gesture]]
name = "circle"
type = "symbol"
symbol = "O"
action = { type = "lock_screen" }

[[gesture]]
name = "cee"
type = "symbol"
symbol = "C"
confidence = 0.7
action = { type = "lock_screen" }

[[gesture]]
name = "zed"
type = "symbol"
symbol = "Z"
action = { type = "lock_screen" }
"#,
        );
        // O and C resemble each other at a 0.7 threshold; Z resembles neither.
        assert!(c.iter().any(|c| c.reason.contains("“C” and “O”")), "{c:?}");
        assert!(!c.iter().any(|c| c.reason.contains("“Z”")), "{c:?}");
    }
}
