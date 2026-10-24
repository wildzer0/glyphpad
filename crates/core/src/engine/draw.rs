//! Draw mode (SPEC §5): which touches are strokes of a symbol, and grouping
//! strokes into one symbol (SPEC §6.2).

use crate::config::{DrawActivation, Edge};
use crate::symbols::{Stroke, TracePoint};

use super::session::{Session, TraceSample, distance};

/// Trace points closer than this to the previous kept one are sensor jitter
/// (resting fingers wobble by a few tenths of a millimeter).
const JITTER_MM: f32 = 0.3;

#[derive(Debug, Clone, PartialEq)]
pub struct DrawParams {
    pub activation: DrawActivation,
    pub stroke_gap_us: u64,
    pub min_stroke_mm: f32,
    /// How still a finger must stay for hold-to-draw.
    pub hold_still_mm: f32,
}

#[derive(Debug)]
enum State {
    Idle,
    Collecting {
        strokes: Vec<Stroke>,
        /// Time of the first point of the symbol; trace times count from it.
        origin_us: u64,
        /// Fingers of the first stroke.
        fingers: usize,
        /// The session of the first stroke, judged as a tap or swipe if the
        /// symbol is not recognized and it stays the only stroke.
        fallback: Option<Box<Session>>,
        /// Lift of the last stroke plus the stroke gap; `None` while another
        /// touch is in progress.
        deadline_us: Option<u64>,
    },
}

#[derive(Debug)]
pub struct DrawRecognizer {
    state: State,
}

impl Default for DrawRecognizer {
    fn default() -> Self {
        Self { state: State::Idle }
    }
}

/// One-finger drawing armed by the `draw_symbol` action, or permanently
/// while recording templates in `toggle` mode.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Arming {
    pub until_us: Option<u64>,
    pub always: bool,
}

impl Arming {
    fn at(&self, t_us: u64) -> bool {
        self.always || self.until_us.is_some_and(|u| t_us <= u)
    }
}

/// A finished symbol: its strokes and, if it had only one, the session to
/// fall back on.
pub struct Finished {
    pub strokes: Vec<Stroke>,
    pub fallback: Option<Box<Session>>,
}

impl DrawRecognizer {
    pub fn reset(&mut self) {
        self.state = State::Idle;
    }

    pub fn deadline(&self) -> Option<u64> {
        match self.state {
            State::Collecting { deadline_us, .. } => deadline_us,
            State::Idle => None,
        }
    }

    /// A new touch started: it may be the next stroke.
    pub fn session_began(&mut self) {
        if let State::Collecting { deadline_us, .. } = &mut self.state {
            *deadline_us = None;
        }
    }

    /// Whether the session in progress is (so far) being drawn, e.g. to
    /// freeze the cursor while it is.
    pub fn drawing(&self, s: &Session, p: &DrawParams, arming: Arming) -> bool {
        if s.consumed || s.clicked {
            return false;
        }
        let Some(first) = s.trace.first() else {
            return false;
        };
        if s.max_fingers == 1 && (arming.at(s.start_us) || self.continues_one_finger()) {
            return true;
        }
        match p.activation {
            DrawActivation::Fingers { count } => s.count() == usize::from(count),
            DrawActivation::Edge {
                edge, zone_percent, ..
            } => s.max_fingers == 1 && in_zone(first, edge, zone_percent),
            DrawActivation::Hold { ms, .. } => {
                s.max_fingers == 1 && hold_start(&s.trace, ms, p.hold_still_mm).is_some()
            }
            DrawActivation::Toggle { .. } => false,
        }
    }

    /// The part of a finished session that is a stroke, if any.
    pub fn stroke_of(
        &self,
        s: &Session,
        p: &DrawParams,
        arming: Arming,
    ) -> Option<Vec<TraceSample>> {
        if s.consumed || s.clicked || s.trace.is_empty() {
            return None;
        }
        let one_finger = s.max_fingers == 1;
        // Once a symbol is under way, or one-finger drawing is armed, a
        // one-finger touch continues it whatever the activation mode, so a
        // symbol can be finished in the same way it was started.
        let samples: Vec<TraceSample> =
            if one_finger && (arming.at(s.start_us) || self.continues_one_finger()) {
                s.trace.clone()
            } else {
                match p.activation {
                    DrawActivation::Fingers { count } => {
                        let n = usize::from(count);
                        if s.max_fingers != n {
                            return None;
                        }
                        s.trace.iter().filter(|t| t.count == n).copied().collect()
                    }
                    DrawActivation::Edge {
                        edge, zone_percent, ..
                    } => {
                        if !one_finger || !in_zone(&s.trace[0], edge, zone_percent) {
                            return None;
                        }
                        s.trace.clone()
                    }
                    DrawActivation::Hold { ms, .. } => {
                        if !one_finger {
                            return None;
                        }
                        let from = hold_start(&s.trace, ms, p.hold_still_mm)?;
                        s.trace[from..].to_vec()
                    }
                    DrawActivation::Toggle { .. } => return None,
                }
            };
        let samples = dejitter(samples);
        let length: f32 = samples.windows(2).map(|w| distance(w[0].mm, w[1].mm)).sum();
        (samples.len() >= 2 && length >= p.min_stroke_mm).then_some(samples)
    }

    /// A one-finger symbol is under way.
    fn continues_one_finger(&self) -> bool {
        matches!(self.state, State::Collecting { fingers: 1, .. })
    }

    pub fn add(&mut self, samples: Vec<TraceSample>, session: Session, p: &DrawParams) {
        let end = session.last_us;
        match &mut self.state {
            State::Idle => {
                let origin_us = samples[0].t_us;
                self.state = State::Collecting {
                    strokes: vec![to_stroke(&samples, origin_us)],
                    origin_us,
                    fingers: session.max_fingers,
                    fallback: Some(Box::new(session)),
                    deadline_us: Some(end + p.stroke_gap_us),
                };
            }
            State::Collecting {
                strokes,
                origin_us,
                fallback,
                deadline_us,
                ..
            } => {
                strokes.push(to_stroke(&samples, *origin_us));
                *fallback = None;
                *deadline_us = Some(end + p.stroke_gap_us);
            }
        }
    }

    /// Strokes of the symbol in progress so far.
    pub fn strokes(&self) -> &[Stroke] {
        match &self.state {
            State::Collecting { strokes, .. } => strokes,
            State::Idle => &[],
        }
    }

    /// Ends the symbol in progress, if any.
    pub fn finish(&mut self) -> Option<Finished> {
        match std::mem::replace(&mut self.state, State::Idle) {
            State::Collecting {
                strokes, fallback, ..
            } => Some(Finished { strokes, fallback }),
            State::Idle => None,
        }
    }
}

fn in_zone(t: &TraceSample, edge: Edge, zone_percent: f32) -> bool {
    let z = zone_percent / 100.0;
    match edge {
        Edge::Left => t.norm.0 <= z,
        Edge::Right => t.norm.0 >= 1.0 - z,
        Edge::Top => t.norm.1 <= z,
        Edge::Bottom => t.norm.1 >= 1.0 - z,
    }
}

/// Index where drawing starts after a hold: the finger stayed within
/// `still_mm` of where it touched for at least `ms`, then moved on.
/// Returns the last still sample when the finger is still resting.
fn hold_start(trace: &[TraceSample], ms: u64, still_mm: f32) -> Option<usize> {
    let first = trace.first()?;
    let moved = trace
        .iter()
        .position(|t| distance(t.mm, first.mm) > still_mm);
    let last_still = moved.map_or(trace.len() - 1, |i| i - 1);
    (trace[last_still].t_us - first.t_us >= ms * 1000).then_some(last_still)
}

fn dejitter(samples: Vec<TraceSample>) -> Vec<TraceSample> {
    let mut out: Vec<TraceSample> = Vec::with_capacity(samples.len());
    for s in samples {
        if out.last().is_none_or(|l| distance(l.mm, s.mm) >= JITTER_MM) {
            out.push(s);
        }
    }
    out
}

fn to_stroke(samples: &[TraceSample], origin_us: u64) -> Stroke {
    samples
        .iter()
        .map(|t| TracePoint {
            x_mm: t.mm.0,
            y_mm: t.mm.1,
            t_us: t.t_us.saturating_sub(origin_us),
        })
        .collect()
}
