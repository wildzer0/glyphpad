//! Chord recognizer (SPEC §6.1): one or more fingers rest as a modifier
//! while other fingers tap or swipe, like holding a key while clicking.

use std::collections::{BTreeMap, BTreeSet};

use super::motion::quantize;
use super::session::{Point, Session, distance};
use super::{Observation, Spec, Thresholds};

/// How long a finger must already rest, still, to count as a modifier when
/// other fingers join. Fingers of an ordinary multi-finger touch land within
/// a few tens of milliseconds of each other.
const MODIFIER_REST_US: u64 = 250_000;

#[derive(Debug)]
struct Group {
    modifiers: BTreeSet<u32>,
    /// Start and latest position of each finger of the gesture.
    members: BTreeMap<u32, (Point, Point)>,
    max_members: usize,
    start_us: u64,
    max_drift_mm: f32,
}

#[derive(Debug, Default)]
pub struct ChordRecognizer {
    group: Option<Group>,
}

fn centroid<'a>(points: impl Iterator<Item = &'a Point>) -> Point {
    let (mut x, mut y, mut n) = (0.0, 0.0, 0.0);
    for p in points {
        x += p.0;
        y += p.1;
        n += 1.0;
    }
    if n == 0.0 { (0.0, 0.0) } else { (x / n, y / n) }
}

impl ChordRecognizer {
    pub fn reset(&mut self) {
        self.group = None;
    }

    pub fn update(&mut self, s: &Session, spec: &Spec, t: &Thresholds) -> Option<Observation> {
        if !spec.chords || s.consumed || s.clicked {
            self.group = None;
            return None;
        }
        let now = s.last_us;
        let Some(g) = &mut self.group else {
            let resting: BTreeSet<u32> = s
                .fingers
                .iter()
                .filter(|(_, f)| {
                    now - f.down_us >= MODIFIER_REST_US && f.drift_mm <= t.hold_max_movement_mm
                })
                .map(|(id, _)| *id)
                .collect();
            let fresh: BTreeMap<u32, (Point, Point)> = s
                .fingers
                .iter()
                .filter(|(_, f)| now - f.down_us < MODIFIER_REST_US)
                .map(|(id, f)| (*id, (f.start, f.pos)))
                .collect();
            if !resting.is_empty() && !fresh.is_empty() {
                self.group = Some(Group {
                    modifiers: resting,
                    max_members: fresh.len(),
                    members: fresh,
                    start_us: now,
                    max_drift_mm: 0.0,
                });
            }
            return None;
        };

        // A modifier that lifts or moves cancels the chord.
        if g.modifiers.iter().any(|id| {
            s.fingers
                .get(id)
                .is_none_or(|f| f.drift_mm > t.hold_max_movement_mm)
        }) {
            self.group = None;
            return None;
        }
        for (id, f) in &s.fingers {
            if g.modifiers.contains(id) {
                continue;
            }
            let entry = g.members.entry(*id).or_insert((f.start, f.pos));
            entry.1 = f.pos;
            g.max_drift_mm = g.max_drift_mm.max(distance(entry.0, entry.1));
        }
        let down = g
            .members
            .keys()
            .filter(|id| s.fingers.contains_key(id))
            .count();
        g.max_members = g.max_members.max(down);
        if down > 0 {
            return None;
        }

        // Every gesture finger lifted while the modifiers kept resting.
        let g = self.group.take()?;
        let fingers = g.max_members.min(u8::MAX as usize) as u8;
        let duration = now - g.start_us;
        let a = centroid(g.members.values().map(|m| &m.0));
        let b = centroid(g.members.values().map(|m| &m.1));
        let travel = distance(a, b);
        let speed = travel / (duration.max(1) as f32 / 1e6);
        let gesture =
            if duration <= t.tap_max_duration_us && g.max_drift_mm <= t.tap_max_movement_mm {
                Observation::Tap { fingers, taps: 1 }
            } else if travel >= t.swipe_min_distance_mm && speed >= t.swipe_min_speed_mm_s {
                Observation::Swipe {
                    fingers,
                    direction: quantize(b.0 - a.0, b.1 - a.1, spec.eight_way(fingers)),
                    distance_mm: travel,
                    speed_mm_s: speed,
                }
            } else {
                return None;
            };
        Some(Observation::Chord {
            modifiers: g.modifiers.len().min(u8::MAX as usize) as u8,
            gesture: Box::new(gesture),
        })
    }
}
