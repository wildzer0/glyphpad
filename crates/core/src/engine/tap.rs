//! Tap and multi-tap recognizer.

use super::session::Session;
use super::{Observation, Spec, Thresholds};

#[derive(Debug, Clone, PartialEq)]
enum State {
    Idle,
    /// `taps` taps with `fingers` fingers seen; waiting to know whether
    /// another one follows before confirming.
    Pending {
        fingers: u8,
        taps: u8,
        /// Lift of the last tap; `None` while the next touch is in progress.
        deadline_us: Option<u64>,
    },
}

/// Waits `multi_tap_gap_ms` before confirming a tap only when a gesture
/// with more taps and the same fingers exists (SPEC §6.1); otherwise a tap
/// fires as soon as the fingers lift.
#[derive(Debug)]
pub struct TapRecognizer {
    state: State,
}

impl Default for TapRecognizer {
    fn default() -> Self {
        Self { state: State::Idle }
    }
}

impl TapRecognizer {
    pub fn deadline(&self) -> Option<u64> {
        match self.state {
            State::Pending { deadline_us, .. } => deadline_us,
            State::Idle => None,
        }
    }

    pub fn reset(&mut self) {
        self.state = State::Idle;
    }

    /// Confirms a pending sequence now, e.g. because a hold consumed the touch.
    pub fn reset_into(&mut self, out: &mut Vec<Observation>) {
        out.extend(self.flush());
    }

    fn flush(&mut self) -> Option<Observation> {
        match std::mem::replace(&mut self.state, State::Idle) {
            State::Pending { fingers, taps, .. } => Some(Observation::Tap { fingers, taps }),
            State::Idle => None,
        }
    }

    pub fn tick(&mut self, now_us: u64) -> Option<Observation> {
        match self.state {
            State::Pending {
                deadline_us: Some(d),
                ..
            } if now_us >= d => self.flush(),
            _ => None,
        }
    }

    /// A new touch started: it may be the next tap of a sequence.
    pub fn session_began(&mut self, now_us: u64) -> Option<Observation> {
        let expired = self.tick(now_us);
        if let State::Pending { deadline_us, .. } = &mut self.state {
            *deadline_us = None;
        }
        expired
    }

    /// The touch ended. Returns up to two observations: a sequence that this
    /// touch did not continue, and a completed one.
    pub fn session_ended(&mut self, s: &Session, spec: &Spec, t: &Thresholds) -> Vec<Observation> {
        let mut out = Vec::new();
        let is_tap = !s.consumed
            && !s.clicked
            && s.max_fingers > 0
            && s.duration_us() <= t.tap_max_duration_us
            && s.max_drift_mm <= t.tap_max_movement_mm;
        if !is_tap {
            out.extend(self.flush());
            return out;
        }
        let fingers = s.max_fingers.min(u8::MAX as usize) as u8;
        let taps = match self.state {
            State::Pending {
                fingers: f, taps, ..
            } if f == fingers => taps + 1,
            _ => {
                out.extend(self.flush());
                1
            }
        };
        if taps >= spec.max_taps(fingers) {
            self.state = State::Idle;
            out.push(Observation::Tap { fingers, taps });
        } else {
            self.state = State::Pending {
                fingers,
                taps,
                deadline_us: Some(s.last_us + t.multi_tap_gap_us),
            };
        }
        out
    }
}
