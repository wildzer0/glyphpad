//! A *touch session* runs from the first finger touching to the last one
//! lifting. The tracker turns frames into session state that every
//! recognizer reads.

use std::collections::{BTreeMap, BTreeSet};

use crate::model::TouchFrame;

pub type Point = (f32, f32);

pub fn distance(a: Point, b: Point) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

#[derive(Debug, Clone, PartialEq)]
pub struct Finger {
    pub start: Point,
    pub pos: Point,
}

/// The fingers' centroid in one frame of the session.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TraceSample {
    pub t_us: u64,
    /// Centroid in millimeters.
    pub mm: Point,
    /// Centroid normalized to 0..1 over the surface.
    pub norm: Point,
    /// Fingers touching in this frame.
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    pub start_us: u64,
    pub last_us: u64,
    /// Fingers touching now (tip down and confident), by contact id.
    pub fingers: BTreeMap<u32, Finger>,
    /// Most fingers touching at once so far.
    pub max_fingers: usize,
    /// Largest distance any finger moved from where it touched down.
    pub max_drift_mm: f32,
    /// The physical button was pressed at some point.
    pub clicked: bool,
    /// The *peak* phase: the time with `max_fingers` fingers down, from the
    /// moment that count was reached until the first finger lifts.
    pub peak_start_us: u64,
    pub peak_end_us: u64,
    pub peak_centroid_start: Point,
    pub peak_centroid_end: Point,
    pub peak_over: bool,
    /// A recognizer fired during the session (e.g. a hold), so no other
    /// gesture may come from it.
    pub consumed: bool,
    /// Contacts counted as fingers and later flagged as palms. The session
    /// stays open until they lift.
    pub palms: BTreeSet<u32>,
    /// Centroid of the fingers in every frame with at least one finger down.
    pub trace: Vec<TraceSample>,
}

impl Session {
    fn new(ts: u64) -> Self {
        Self {
            start_us: ts,
            last_us: ts,
            fingers: BTreeMap::new(),
            max_fingers: 0,
            max_drift_mm: 0.0,
            clicked: false,
            peak_start_us: ts,
            peak_end_us: ts,
            peak_centroid_start: (0.0, 0.0),
            peak_centroid_end: (0.0, 0.0),
            peak_over: false,
            consumed: false,
            palms: BTreeSet::new(),
            trace: Vec::new(),
        }
    }

    pub fn count(&self) -> usize {
        self.fingers.len()
    }

    pub fn duration_us(&self) -> u64 {
        self.last_us - self.start_us
    }

    pub fn centroid(&self) -> Point {
        let n = self.fingers.len().max(1) as f32;
        let (sx, sy) = self
            .fingers
            .values()
            .fold((0.0, 0.0), |(x, y), f| (x + f.pos.0, y + f.pos.1));
        (sx / n, sy / n)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SessionUpdate {
    /// The first finger touched.
    Began,
    /// Fingers moved, touched or lifted while the session goes on.
    Changed,
    /// The last finger lifted.
    Ended,
}

#[derive(Debug, Default)]
pub struct SessionTracker {
    session: Option<Session>,
}

impl SessionTracker {
    pub fn session(&self) -> Option<&Session> {
        self.session.as_ref()
    }

    pub fn session_mut(&mut self) -> Option<&mut Session> {
        self.session.as_mut()
    }

    /// Drops the current session without ending it (device gone, stale data).
    pub fn cancel(&mut self) {
        self.session = None;
    }

    /// Takes the session that just ended.
    pub fn take(&mut self) -> Option<Session> {
        self.session.take()
    }

    pub fn update(&mut self, frame: &TouchFrame) -> Option<SessionUpdate> {
        let ts = frame.timestamp_us;
        // Palms and other contacts flagged as not confident are ignored
        // entirely (SPEC §6.2).
        let active: Vec<_> = frame.active_contacts().collect();

        let began = self.session.is_none();
        if began && active.is_empty() {
            return None;
        }
        let s = self.session.get_or_insert_with(|| Session::new(ts));
        s.last_us = ts;
        s.clicked |= frame.button_pressed;

        // Touchpads often report a palm as a confident contact for the first
        // few frames (30–70 ms on the reference device) and flag it only
        // afterwards. By then it was counted as a finger and its removal
        // would look like a lift, turning every palm into a tap; so a late
        // palm voids the whole session instead, and the session lasts until
        // the palm lifts, so fingers touching beside it start nothing either.
        for c in &frame.contacts {
            if c.tip && !c.confident && s.fingers.contains_key(&c.id) {
                s.palms.insert(c.id);
                s.consumed = true;
            }
        }
        s.palms
            .retain(|id| frame.contacts.iter().any(|c| c.id == *id && c.tip));

        s.fingers.retain(|id, _| active.iter().any(|c| c.id == *id));
        for c in &active {
            let pos = (c.x_mm, c.y_mm);
            let f = s.fingers.entry(c.id).or_insert(Finger { start: pos, pos });
            f.pos = pos;
            s.max_drift_mm = s.max_drift_mm.max(distance(f.start, pos));
        }

        let n = s.fingers.len();
        if n > 0 {
            let k = n as f32;
            let sum = active.iter().fold((0.0, 0.0, 0.0, 0.0), |a, c| {
                (a.0 + c.x_mm, a.1 + c.y_mm, a.2 + c.x_norm, a.3 + c.y_norm)
            });
            s.trace.push(TraceSample {
                t_us: ts,
                mm: (sum.0 / k, sum.1 / k),
                norm: (sum.2 / k, sum.3 / k),
                count: n,
            });
        }
        if n > s.max_fingers {
            s.max_fingers = n;
            s.peak_over = false;
            s.peak_start_us = ts;
            s.peak_centroid_start = s.centroid();
        }
        if !s.peak_over {
            if n == s.max_fingers {
                s.peak_end_us = ts;
                s.peak_centroid_end = s.centroid();
            } else {
                s.peak_over = true;
            }
        }

        Some(if n == 0 && s.palms.is_empty() {
            SessionUpdate::Ended
        } else if began {
            SessionUpdate::Began
        } else {
            SessionUpdate::Changed
        })
    }
}
