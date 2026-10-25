//! Tap and multi-tap recognizer.

use std::sync::Arc;

use super::session::Session;
use super::{GestureTrace, Observation, Spec, Thresholds};

/// A tap sequence the recognizer confirms, with the times of the movement
/// behind it: the touch-down of its first tap and the lift of its last, so a
/// sequence that starts with a tap measures its pause from the real lift.
#[derive(Debug, Clone, PartialEq)]
pub struct TapHit {
    pub observation: Observation,
    pub started_us: u64,
    pub ended_us: u64,
    /// Paths of the fingers of the last tap.
    pub trace: Arc<GestureTrace>,
}

#[derive(Debug, Clone, PartialEq)]
struct Pending {
    fingers: u8,
    taps: u8,
    /// Lift of the last tap; `None` while the next touch is in progress.
    deadline_us: Option<u64>,
    /// Touch-down of the first tap of the sequence.
    started_us: u64,
    /// Lift of the last tap.
    ended_us: u64,
    trace: Arc<GestureTrace>,
}

#[derive(Debug, Clone, PartialEq)]
enum State {
    Idle,
    /// `taps` taps with `fingers` fingers seen; waiting to know whether
    /// another one follows before confirming.
    Pending(Pending),
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
            State::Pending(Pending { deadline_us, .. }) => deadline_us,
            State::Idle => None,
        }
    }

    pub fn reset(&mut self) {
        self.state = State::Idle;
    }

    /// Confirms a pending sequence now, e.g. because a hold consumed the touch.
    pub fn reset_into(&mut self, out: &mut Vec<TapHit>) {
        out.extend(self.flush());
    }

    fn flush(&mut self) -> Option<TapHit> {
        match std::mem::replace(&mut self.state, State::Idle) {
            State::Pending(p) => Some(TapHit {
                observation: Observation::Tap {
                    fingers: p.fingers,
                    taps: p.taps,
                },
                started_us: p.started_us,
                ended_us: p.ended_us,
                trace: p.trace,
            }),
            State::Idle => None,
        }
    }

    pub fn tick(&mut self, now_us: u64) -> Option<TapHit> {
        match self.state {
            State::Pending(Pending {
                deadline_us: Some(d),
                ..
            }) if now_us >= d => self.flush(),
            _ => None,
        }
    }

    /// A new touch started: it may be the next tap of a sequence.
    pub fn session_began(&mut self, now_us: u64) -> Option<TapHit> {
        let expired = self.tick(now_us);
        if let State::Pending(p) = &mut self.state {
            p.deadline_us = None;
        }
        expired
    }

    /// The touch in progress can no longer be the next tap of the sequence:
    /// it lasts longer or moved farther than a tap. The waiting sequence is
    /// over, so it is confirmed at once instead of after this touch ends,
    /// which would delay it for as long as the touch lasts (a drag, a palm,
    /// a hold) and stretch a sequence's window by the same amount.
    pub fn cannot_continue(&self, s: &Session, t: &Thresholds) -> bool {
        match self.state {
            State::Pending(Pending {
                deadline_us: None, ..
            }) => s.duration_us() > t.tap_max_duration_us || s.max_drift_mm > t.tap_max_movement_mm,
            _ => false,
        }
    }

    /// The touch ended. Returns up to two observations: a sequence that this
    /// touch did not continue, and a completed one.
    pub fn session_ended(&mut self, s: &Session, spec: &Spec, t: &Thresholds) -> Vec<TapHit> {
        let mut out = Vec::new();
        let is_tap = !s.consumed
            && !s.chorded
            && !s.clicked
            && s.max_fingers > 0
            && s.duration_us() <= t.tap_max_duration_us
            && s.max_drift_mm <= t.tap_max_movement_mm;
        if !is_tap {
            out.extend(self.flush());
            return out;
        }
        let fingers = s.max_fingers.min(u8::MAX as usize) as u8;
        let continues = matches!(&self.state, State::Pending(p) if p.fingers == fingers);
        let (taps, started_us) = match &self.state {
            State::Pending(p) if continues => (p.taps + 1, p.started_us),
            _ => {
                out.extend(self.flush());
                (1, s.start_us)
            }
        };
        let trace = Arc::new(GestureTrace {
            fingers: s.paths.clone(),
            strokes: Vec::new(),
        });
        if taps >= spec.max_taps(fingers) {
            self.state = State::Idle;
            out.push(TapHit {
                observation: Observation::Tap { fingers, taps },
                started_us,
                ended_us: s.last_us,
                trace,
            });
        } else {
            self.state = State::Pending(Pending {
                fingers,
                taps,
                deadline_us: Some(s.last_us + t.multi_tap_gap_us),
                started_us,
                ended_us: s.last_us,
                trace,
            });
        }
        out
    }
}
