//! Hold recognizer: fingers resting still for a while.

use std::collections::BTreeMap;

use super::session::{Point, Session, distance};
use super::{Observation, Spec, Thresholds};

#[derive(Debug, Clone, PartialEq)]
enum State {
    Idle,
    /// `fingers` fingers have rested near `anchors` since `since_us`.
    Armed {
        fingers: u8,
        since_us: u64,
        anchors: BTreeMap<u32, Point>,
    },
    /// Fired for this session; nothing more until all fingers lift.
    Fired,
}

/// Fires while the fingers are still down, as soon as the hold lasted long
/// enough, and consumes the session. Only finger counts used by a hold
/// gesture arm it, so resting fingers between other movements do not.
#[derive(Debug)]
pub struct HoldRecognizer {
    state: State,
}

impl Default for HoldRecognizer {
    fn default() -> Self {
        Self { state: State::Idle }
    }
}

impl HoldRecognizer {
    pub fn deadline(&self, t: &Thresholds) -> Option<u64> {
        match self.state {
            State::Armed { since_us, .. } => Some(since_us + t.hold_duration_us),
            _ => None,
        }
    }

    pub fn reset(&mut self) {
        self.state = State::Idle;
    }

    pub fn update(&mut self, s: &Session, spec: &Spec, t: &Thresholds) -> Option<Observation> {
        if s.consumed || s.chorded || s.clicked {
            if !matches!(self.state, State::Fired) {
                self.state = State::Idle;
            }
            return None;
        }
        if matches!(self.state, State::Fired) {
            return None;
        }
        let n = s.count().min(u8::MAX as usize) as u8;
        let positions: BTreeMap<u32, Point> =
            s.fingers.iter().map(|(id, f)| (*id, f.pos)).collect();

        let still = match &self.state {
            State::Armed {
                fingers, anchors, ..
            } => {
                *fingers == n
                    && anchors.keys().eq(positions.keys())
                    && anchors
                        .iter()
                        .zip(positions.values())
                        .all(|((_, a), p)| distance(*a, *p) <= t.hold_max_movement_mm)
            }
            _ => false,
        };
        if !still {
            self.state = if spec.has_hold(n) {
                State::Armed {
                    fingers: n,
                    since_us: s.last_us,
                    anchors: positions,
                }
            } else {
                State::Idle
            };
        }
        self.tick(s.last_us, t)
    }

    pub fn tick(&mut self, now_us: u64, t: &Thresholds) -> Option<Observation> {
        match self.state {
            State::Armed {
                fingers, since_us, ..
            } if now_us >= since_us + t.hold_duration_us => {
                self.state = State::Fired;
                Some(Observation::Hold { fingers })
            }
            _ => None,
        }
    }

    pub fn session_ended(&mut self) {
        self.state = State::Idle;
    }
}
