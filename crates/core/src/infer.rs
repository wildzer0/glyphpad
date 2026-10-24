//! Gestures "by demonstration" (SPEC §10.3): the user performs a gesture a
//! few times and GlyphPad deduces its type, fingers, direction and taps.

use std::collections::HashMap;

use crate::config::{Config, Direction, Edge, GestureKind};
use crate::engine::Observation;
use crate::trigger::{Trigger, TriggerDef, Turn, Zoom};

/// What the demonstrations had in common.
#[derive(Debug, Clone, PartialEq)]
pub struct Inferred {
    pub kind: GestureKind,
    pub fingers: u8,
    pub taps: Option<u8>,
    pub direction: Option<Direction>,
    pub edge: Option<Edge>,
    pub zoom: Option<Zoom>,
    pub rotation: Option<Turn>,
    /// Resting fingers of a chord.
    pub modifiers: Option<u8>,
    /// How many demonstrations agreed with this, out of `total`.
    pub agreeing: usize,
    pub total: usize,
}

type Key = (
    GestureKind,
    u8,
    Option<u8>,
    Option<Direction>,
    Option<Edge>,
    Option<Zoom>,
    Option<Turn>,
    Option<u8>,
);

fn key(o: &Observation) -> Option<Key> {
    Some(match *o {
        Observation::Tap { fingers, taps } => (
            GestureKind::Tap,
            fingers,
            Some(taps),
            None,
            None,
            None,
            None,
            None,
        ),
        Observation::Hold { fingers } => (
            GestureKind::Hold,
            fingers,
            None,
            None,
            None,
            None,
            None,
            None,
        ),
        Observation::Swipe {
            fingers, direction, ..
        } => (
            GestureKind::Swipe,
            fingers,
            None,
            Some(direction),
            None,
            None,
            None,
            None,
        ),
        Observation::EdgeSwipe {
            edge,
            fingers,
            direction,
            ..
        } => (
            GestureKind::EdgeSwipe,
            fingers,
            None,
            Some(direction),
            Some(edge),
            None,
            None,
            None,
        ),
        Observation::Pinch { fingers, zoom, .. } => (
            GestureKind::Pinch,
            fingers,
            None,
            None,
            None,
            Some(zoom),
            None,
            None,
        ),
        Observation::Rotate { fingers, turn, .. } => (
            GestureKind::Rotate,
            fingers,
            None,
            None,
            None,
            None,
            Some(turn),
            None,
        ),
        Observation::Click { fingers, direction } => (
            GestureKind::Click,
            fingers,
            None,
            direction,
            None,
            None,
            None,
            None,
        ),
        Observation::Chord {
            modifiers,
            ref gesture,
        } => {
            let mut k = key(gesture)?;
            k.7 = Some(modifiers);
            k
        }
        _ => return None,
    })
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
    let (kind, fingers, taps, direction, edge, zoom, rotation, modifiers) = best;
    Some(Inferred {
        kind,
        fingers,
        taps,
        direction,
        edge,
        zoom,
        rotation,
        modifiers,
        agreeing: counts[&best],
        total,
    })
}

impl Inferred {
    pub fn trigger(&self) -> Option<Trigger> {
        TriggerDef {
            kind: Some(self.kind),
            fingers: Some(self.fingers),
            taps: self.taps.filter(|&t| t > 1),
            direction: self.direction,
            edge: self.edge,
            zoom: self.zoom,
            rotation: self.rotation,
            modifier_fingers: self.modifiers,
            ..Default::default()
        }
        .build()
        .ok()
    }
}

/// Configured gestures that the same movement would also trigger, ignoring
/// conditions (they may overlap).
pub fn conflicts<'a>(config: &'a Config, g: &Inferred) -> Vec<&'a str> {
    let Some(t) = g.trigger() else {
        return Vec::new();
    };
    config
        .gestures
        .iter()
        .filter(|c| c.trigger.as_ref() == Some(&t))
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
