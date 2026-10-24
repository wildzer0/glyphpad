//! The gesture engine (SPEC §6): recognizers running in parallel over the
//! same touch frames, each an explicit state machine testable on its own.
//!
//! The engine produces [`Observation`]s, i.e. what the fingers did; matching
//! an observation to a configured gesture, and running its action, is done
//! by [`resolve`] and the application.
//!
//! Disambiguation policy:
//! - a session with a physical click produces no tap, hold or swipe
//!   ("click + gesture" is a gesture type of its own, M5);
//! - a hold fires while the fingers are down and consumes the session, so
//!   nothing else fires when they lift;
//! - taps and swipes are exclusive by their movement thresholds;
//! - a tap waits `multi_tap_gap_ms` before confirming only if a gesture with
//!   more taps and the same fingers is configured;
//! - among matching gestures the first in the file wins (priority order).

mod hold;
mod session;
mod swipe;
mod tap;

use std::collections::HashMap;
use std::fmt;

use crate::action::AppInfo;
use crate::config::{Config, Direction, Gesture, GestureKind, Settings};
use crate::model::{DeviceEvent, DeviceId, MAX_CONTACTS, TouchFrame};

pub use session::{Session, SessionTracker};
pub use swipe::quantize;

/// What the fingers did.
#[derive(Debug, Clone, PartialEq)]
pub enum Observation {
    Tap {
        fingers: u8,
        taps: u8,
    },
    Hold {
        fingers: u8,
    },
    Swipe {
        fingers: u8,
        direction: Direction,
        distance_mm: f32,
        speed_mm_s: f32,
    },
}

impl fmt::Display for Observation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Observation::Tap { fingers, taps } => write!(f, "tap fingers={fingers} taps={taps}"),
            Observation::Hold { fingers } => write!(f, "hold fingers={fingers}"),
            Observation::Swipe {
                fingers,
                direction,
                distance_mm,
                speed_mm_s,
            } => write!(
                f,
                "swipe fingers={fingers} direction={direction} ({distance_mm:.1} mm at {speed_mm_s:.0} mm/s)"
            ),
        }
    }
}

/// Thresholds from `[settings]`, in the units the recognizers use.
#[derive(Debug, Clone, PartialEq)]
pub struct Thresholds {
    pub tap_max_duration_us: u64,
    pub tap_max_movement_mm: f32,
    pub multi_tap_gap_us: u64,
    pub hold_duration_us: u64,
    pub hold_max_movement_mm: f32,
    pub swipe_min_distance_mm: f32,
    pub swipe_min_speed_mm_s: f32,
}

impl From<&Settings> for Thresholds {
    fn from(s: &Settings) -> Self {
        Self {
            tap_max_duration_us: s.tap_max_duration_ms * 1000,
            tap_max_movement_mm: s.tap_max_movement_mm,
            multi_tap_gap_us: s.multi_tap_gap_ms * 1000,
            hold_duration_us: s.hold_duration_ms * 1000,
            hold_max_movement_mm: s.hold_max_movement_mm,
            swipe_min_distance_mm: s.swipe_min_distance_mm,
            swipe_min_speed_mm_s: s.swipe_min_speed_mm_s,
        }
    }
}

impl Default for Thresholds {
    fn default() -> Self {
        Self::from(&Settings::default())
    }
}

/// Which variants the configured gestures need, indexed by finger count.
#[derive(Debug, Clone, PartialEq)]
pub struct Spec {
    max_taps: [u8; MAX_CONTACTS + 1],
    holds: [bool; MAX_CONTACTS + 1],
    eight_way: [bool; MAX_CONTACTS + 1],
}

impl Default for Spec {
    fn default() -> Self {
        Self {
            max_taps: [1; MAX_CONTACTS + 1],
            holds: [false; MAX_CONTACTS + 1],
            eight_way: [false; MAX_CONTACTS + 1],
        }
    }
}

impl Spec {
    pub fn from_config(config: &Config) -> Self {
        let mut spec = Self::default();
        for g in config.gestures.iter().filter(|g| g.def.enabled) {
            let n = usize::from(g.fingers()).min(MAX_CONTACTS);
            match g.def.kind {
                GestureKind::Tap => spec.max_taps[n] = spec.max_taps[n].max(g.taps()),
                GestureKind::Hold => spec.holds[n] = true,
                GestureKind::Swipe => {
                    spec.eight_way[n] |= g.def.direction.is_some_and(Direction::is_diagonal)
                }
                GestureKind::Symbol => {}
            }
        }
        spec
    }

    /// Recognizes every variant: triple taps, holds with any number of
    /// fingers, 8-way swipes. For diagnostics, not for normal use, because
    /// every tap then waits for a possible second one.
    pub fn everything() -> Self {
        Self {
            max_taps: [3; MAX_CONTACTS + 1],
            holds: [true; MAX_CONTACTS + 1],
            eight_way: [true; MAX_CONTACTS + 1],
        }
    }

    pub fn max_taps(&self, fingers: u8) -> u8 {
        self.max_taps[usize::from(fingers).min(MAX_CONTACTS)]
    }

    pub fn has_hold(&self, fingers: u8) -> bool {
        fingers > 0 && self.holds[usize::from(fingers).min(MAX_CONTACTS)]
    }

    pub fn eight_way(&self, fingers: u8) -> bool {
        self.eight_way[usize::from(fingers).min(MAX_CONTACTS)]
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Recognition {
    pub device: DeviceId,
    pub observation: Observation,
    /// Timestamp, in the frame time base, of the moment of recognition.
    pub at_us: u64,
}

/// A session whose device sends nothing for this long is dropped: the lift
/// was lost (e.g. the device was unplugged mid-touch). Touchpads report
/// continuously while fingers rest on them, so this never cuts a real touch.
const STALE_SESSION_US: u64 = 1_500_000;

#[derive(Debug, Default)]
struct DeviceState {
    tracker: SessionTracker,
    tap: tap::TapRecognizer,
    hold: hold::HoldRecognizer,
    swipe: swipe::SwipeRecognizer,
    last_frame_us: u64,
}

impl DeviceState {
    fn reset(&mut self) {
        self.tracker.cancel();
        self.tap.reset();
        self.hold.reset();
    }
}

#[derive(Debug)]
pub struct Engine {
    spec: Spec,
    thresholds: Thresholds,
    devices: HashMap<DeviceId, DeviceState>,
}

impl Engine {
    pub fn new(spec: Spec, thresholds: Thresholds) -> Self {
        Self {
            spec,
            thresholds,
            devices: HashMap::new(),
        }
    }

    pub fn from_config(config: &Config) -> Self {
        Self::new(
            Spec::from_config(config),
            Thresholds::from(&config.settings),
        )
    }

    /// Applies a new configuration, keeping nothing of touches in progress.
    pub fn reconfigure(&mut self, config: &Config) {
        self.spec = Spec::from_config(config);
        self.thresholds = Thresholds::from(&config.settings);
        self.reset();
    }

    /// Forgets every touch in progress and pending tap sequence.
    pub fn reset(&mut self) {
        self.devices.values_mut().for_each(DeviceState::reset);
    }

    pub fn on_event(&mut self, event: &DeviceEvent) -> Vec<Recognition> {
        match event {
            DeviceEvent::Frame { device, frame } => self.on_frame(*device, frame),
            DeviceEvent::Connected(_) => Vec::new(),
            DeviceEvent::Disconnected { device } => {
                self.devices.remove(device);
                Vec::new()
            }
        }
    }

    pub fn on_frame(&mut self, device: DeviceId, frame: &TouchFrame) -> Vec<Recognition> {
        let (spec, t) = (&self.spec, &self.thresholds);
        let st = self.devices.entry(device).or_default();
        let ts = frame.timestamp_us;
        st.last_frame_us = ts;
        let mut out = Vec::new();

        match st.tracker.update(frame) {
            None => {}
            Some(session::SessionUpdate::Ended) => {
                if let Some(s) = st.tracker.take() {
                    st.hold.session_ended();
                    out.extend(st.tap.session_ended(&s, spec, t));
                    out.extend(st.swipe.session_ended(&s, spec, t));
                }
            }
            Some(update) => {
                if update == session::SessionUpdate::Began {
                    out.extend(st.tap.session_began(ts));
                }
                if let Some(s) = st.tracker.session()
                    && let Some(hold) = st.hold.update(s, spec, t)
                {
                    Self::consume(st, &mut out, hold);
                }
            }
        }
        out.into_iter()
            .map(|observation| Recognition {
                device,
                observation,
                at_us: ts,
            })
            .collect()
    }

    /// A hold fired mid-session: a tap sequence waiting for this touch is
    /// confirmed first, then the session is closed to other recognizers.
    fn consume(st: &mut DeviceState, out: &mut Vec<Observation>, hold: Observation) {
        st.tap.reset_into(out);
        if let Some(s) = st.tracker.session_mut() {
            s.consumed = true;
        }
        out.push(hold);
    }

    /// Advances time without new frames, for timeouts (multi-tap gap, holds,
    /// stale sessions). `now_us` is in the frame time base.
    pub fn tick(&mut self, now_us: u64) -> Vec<Recognition> {
        let t = &self.thresholds;
        let mut all = Vec::new();
        for (&device, st) in &mut self.devices {
            let mut out = Vec::new();
            if st.tracker.session().is_some() && now_us >= st.last_frame_us + STALE_SESSION_US {
                tracing::debug!(device = device.0, "dropping a stale touch session");
                st.tracker.cancel();
                st.hold.reset();
            }
            out.extend(st.tap.tick(now_us));
            if st.tracker.session().is_some()
                && let Some(hold) = st.hold.tick(now_us, t)
            {
                Self::consume(st, &mut out, hold);
            }
            all.extend(out.into_iter().map(|observation| Recognition {
                device,
                observation,
                at_us: now_us,
            }));
        }
        all
    }

    /// Earliest time at which [`tick`](Self::tick) may produce something.
    pub fn next_deadline(&self) -> Option<u64> {
        self.devices
            .values()
            .flat_map(|st| {
                [
                    st.tap.deadline(),
                    st.hold.deadline(&self.thresholds),
                    st.tracker
                        .session()
                        .map(|_| st.last_frame_us + STALE_SESSION_US),
                ]
            })
            .flatten()
            .min()
    }
}

/// The configured gesture an observation triggers: the first one, in
/// priority order, that matches and applies to the active profile and the
/// foreground app.
pub fn resolve<'a>(
    config: &'a Config,
    observation: &Observation,
    app: Option<&AppInfo>,
) -> Option<&'a Gesture> {
    config.gestures.iter().find(|g| {
        let d = &g.def;
        let matches = match *observation {
            Observation::Tap { fingers, taps } => {
                d.kind == GestureKind::Tap && g.fingers() == fingers && g.taps() == taps
            }
            Observation::Hold { fingers } => d.kind == GestureKind::Hold && g.fingers() == fingers,
            Observation::Swipe {
                fingers, direction, ..
            } => {
                d.kind == GestureKind::Swipe
                    && g.fingers() == fingers
                    && d.direction == Some(direction)
            }
        };
        matches && g.applies(&config.settings.active_profile, app)
    })
}

#[cfg(test)]
mod tests;
