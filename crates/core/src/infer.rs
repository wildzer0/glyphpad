//! Gestures "by demonstration" (SPEC §10.3): the user performs a gesture a
//! few times and GlyphPad deduces its type, fingers, direction and taps.

use std::collections::HashMap;

use crate::config::{Config, Direction, GestureKind};
use crate::engine::Observation;

/// What the demonstrations had in common.
#[derive(Debug, Clone, PartialEq)]
pub struct Inferred {
    pub kind: GestureKind,
    pub fingers: u8,
    pub taps: Option<u8>,
    pub direction: Option<Direction>,
    /// How many demonstrations agreed with this, out of `total`.
    pub agreeing: usize,
    pub total: usize,
}

fn key(o: &Observation) -> Option<(GestureKind, u8, Option<u8>, Option<Direction>)> {
    match *o {
        Observation::Tap { fingers, taps } => Some((GestureKind::Tap, fingers, Some(taps), None)),
        Observation::Hold { fingers } => Some((GestureKind::Hold, fingers, None, None)),
        Observation::Swipe {
            fingers, direction, ..
        } => Some((GestureKind::Swipe, fingers, None, Some(direction))),
        _ => None,
    }
}

/// The most common gesture among the demonstrations, or `None` if none was
/// a tap, hold or swipe.
pub fn infer(demonstrations: &[Observation]) -> Option<Inferred> {
    let mut counts: HashMap<_, usize> = HashMap::new();
    let mut order = Vec::new();
    let mut total = 0;
    for k in demonstrations.iter().filter_map(key) {
        total += 1;
        let n = counts.entry(k).or_insert(0);
        if *n == 0 {
            order.push(k);
        }
        *n += 1;
    }
    // Ties go to the variant seen first.
    let mut best = None;
    for k in order {
        if best.is_none_or(|b| counts[&k] > counts[&b]) {
            best = Some(k);
        }
    }
    let best = best?;
    let (kind, fingers, taps, direction) = best;
    Some(Inferred {
        kind,
        fingers,
        taps,
        direction,
        agreeing: counts[&best],
        total,
    })
}

/// Configured gestures that the same movement would also trigger, ignoring
/// conditions (they may overlap).
pub fn conflicts<'a>(config: &'a Config, g: &Inferred) -> Vec<&'a str> {
    config
        .gestures
        .iter()
        .filter(|c| {
            c.def.kind == g.kind
                && c.fingers() == g.fingers
                && (g.kind != GestureKind::Tap || Some(c.taps()) == g.taps)
                && (g.kind != GestureKind::Swipe || c.def.direction == g.direction)
        })
        .map(|c| c.def.name.as_str())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn swipe(fingers: u8, direction: Direction) -> Observation {
        Observation::Swipe {
            fingers,
            direction,
            distance_mm: 40.0,
            speed_mm_s: 150.0,
        }
    }

    #[test]
    fn majority_wins() {
        let demos = [
            swipe(3, Direction::Up),
            Observation::UnrecognizedSymbol {
                candidates: vec![],
                strokes: vec![],
            },
            swipe(3, Direction::Up),
            swipe(3, Direction::UpLeft),
        ];
        let g = infer(&demos).unwrap();
        assert_eq!(
            (g.kind, g.fingers, g.direction),
            (GestureKind::Swipe, 3, Some(Direction::Up))
        );
        assert_eq!((g.agreeing, g.total), (2, 3));
        assert!(infer(&[]).is_none());
    }

    #[test]
    fn finds_conflicts() {
        let config = crate::config::parse(
            r#"
[[gesture]]
name = "a"
type = "tap"
fingers = 3
taps = 2
action = { type = "lock_screen" }

[[gesture]]
name = "b"
type = "tap"
fingers = 3
action = { type = "lock_screen" }
"#,
        )
        .unwrap()
        .config;
        let g = infer(&[Observation::Tap {
            fingers: 3,
            taps: 2,
        }])
        .unwrap();
        assert_eq!(conflicts(&config, &g), vec!["a"]);
    }
}
