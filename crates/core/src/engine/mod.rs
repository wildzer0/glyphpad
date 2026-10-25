//! The gesture engine (SPEC §6): recognizers running in parallel over the
//! same touch frames, each an explicit state machine testable on its own.
//!
//! The engine produces [`Observation`]s, i.e. what the fingers did; matching
//! an observation to a configured gesture, and running its action, is done
//! by [`resolve`] and the application.
//!
//! Disambiguation policy:
//! - a session with a physical click produces no tap, hold or swipe, only
//!   a click (or click-and-drag) gesture;
//! - a chord (fingers tapping or swiping while another rests) voids the
//!   session's own tap, hold or swipe;
//! - a hold fires while the fingers are down and consumes the session, so
//!   nothing else fires when they lift;
//! - taps and movements are exclusive by their thresholds; among swipe,
//!   pinch and rotation the one exceeding its threshold most wins;
//! - an edge swipe matches edge-swipe gestures before plain swipes;
//! - a tap waits `multi_tap_gap_ms` before confirming only if a gesture with
//!   more taps and the same fingers is configured;
//! - a touch that the draw mode turns into a stroke waits `stroke_gap_ms`
//!   for further strokes, then is matched against the symbols; if no
//!   configured symbol scores high enough and it was a single stroke, it is
//!   judged as a tap or swipe after all;
//! - among matching gestures the first in the file wins (priority order).

mod chord;
mod draw;
mod hold;
mod motion;
mod session;
mod tap;

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::sync::Arc;

use crate::action::AppInfo;
use crate::config::{Config, Direction, Edge, Gesture, Settings};
use crate::model::{DeviceEvent, DeviceId, MAX_CONTACTS, TouchFrame};
use crate::symbols::{Candidate, Sample, Stroke, SymbolRecognizer, templates};
use crate::trigger::{Trigger, Turn, Zoom};

pub use draw::DrawParams;
pub use motion::{Motion, motion, quantize};
pub use session::{Session, SessionTracker};

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
    /// A swipe that started at an edge of the touchpad.
    EdgeSwipe {
        edge: Edge,
        fingers: u8,
        direction: Direction,
        distance_mm: f32,
    },
    Pinch {
        fingers: u8,
        zoom: Zoom,
        /// Change of the fingers' mean distance from their centroid.
        change_mm: f32,
    },
    Rotate {
        fingers: u8,
        turn: Turn,
        degrees: f32,
    },
    /// A physical click, or a click and drag in `direction`.
    Click {
        fingers: u8,
        direction: Option<Direction>,
    },
    /// A tap or swipe made while `modifiers` other fingers rest.
    Chord {
        modifiers: u8,
        gesture: Box<Observation>,
    },
    /// A drawn symbol that scored at least its gesture's confidence.
    Symbol {
        name: String,
        score: f32,
        /// The best candidates, best first (the first is this symbol).
        candidates: Vec<Candidate>,
        strokes: Vec<Stroke>,
    },
    /// A drawing that matched no configured symbol well enough; logged with
    /// its best candidates.
    UnrecognizedSymbol {
        candidates: Vec<Candidate>,
        strokes: Vec<Stroke>,
    },
    /// A drawing made while recording templates; not matched at all.
    Sample {
        strokes: Vec<Stroke>,
    },
}

/// How many candidates observations keep.
const TOP_CANDIDATES: usize = 3;

fn format_candidates(c: &[Candidate]) -> String {
    let list: Vec<String> = c
        .iter()
        .map(|c| format!("{} {:.2}", c.symbol, c.score))
        .collect();
    if list.is_empty() {
        "no candidates".into()
    } else {
        list.join(", ")
    }
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
            Observation::Symbol {
                name,
                score,
                candidates,
                strokes,
            } => write!(
                f,
                "symbol {name} score={score:.2} strokes={} (candidates: {})",
                strokes.len(),
                format_candidates(candidates)
            ),
            Observation::UnrecognizedSymbol {
                candidates,
                strokes,
            } => write!(
                f,
                "unrecognized drawing strokes={} (best: {})",
                strokes.len(),
                format_candidates(candidates)
            ),
            Observation::Sample { strokes } => write!(f, "drawn sample strokes={}", strokes.len()),
            Observation::EdgeSwipe {
                edge,
                fingers,
                direction,
                distance_mm,
            } => write!(
                f,
                "edge swipe from {edge:?} fingers={fingers} direction={direction} ({distance_mm:.1} mm)"
            ),
            Observation::Pinch {
                fingers,
                zoom,
                change_mm,
            } => write!(f, "pinch {zoom:?} fingers={fingers} ({change_mm:+.1} mm)"),
            Observation::Rotate {
                fingers,
                turn,
                degrees,
            } => write!(f, "rotate {turn:?} fingers={fingers} ({degrees:+.0}°)"),
            Observation::Click {
                fingers,
                direction: None,
            } => write!(f, "click fingers={fingers}"),
            Observation::Click {
                fingers,
                direction: Some(d),
            } => write!(f, "click and drag {d} fingers={fingers}"),
            Observation::Chord { modifiers, gesture } => {
                write!(f, "chord ({modifiers} resting) + {gesture}")
            }
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
    pub pinch_min_mm: f32,
    pub rotate_min_deg: f32,
    pub edge_zone: f32,
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
            pinch_min_mm: s.pinch_min_mm,
            rotate_min_deg: s.rotate_min_deg,
            edge_zone: s.edge_zone_percent / 100.0,
        }
    }
}

impl Default for Thresholds {
    fn default() -> Self {
        Self::from(&Settings::default())
    }
}

/// Which variants the configured gestures need, indexed by finger count.
/// Recognizers only report what some gesture can use, so ordinary pointer
/// movement and clicks produce nothing.
#[derive(Debug, Clone, PartialEq)]
pub struct Spec {
    max_taps: [u8; MAX_CONTACTS + 1],
    holds: [bool; MAX_CONTACTS + 1],
    swipes: [bool; MAX_CONTACTS + 1],
    eight_way: [bool; MAX_CONTACTS + 1],
    edge_swipes: [bool; MAX_CONTACTS + 1],
    pinches: [bool; MAX_CONTACTS + 1],
    rotations: [bool; MAX_CONTACTS + 1],
    clicks: [bool; MAX_CONTACTS + 1],
    /// Some gesture is a chord (a resting finger as a modifier).
    pub chords: bool,
    /// Draw mode; `None` disables symbols.
    pub draw: Option<DrawParams>,
    /// Lowest confidence of the gestures using each symbol. `None` accepts
    /// every symbol at `default_confidence` (diagnostics).
    symbol_thresholds: Option<BTreeMap<String, f32>>,
    default_confidence: f32,
}

impl Default for Spec {
    fn default() -> Self {
        Self {
            max_taps: [1; MAX_CONTACTS + 1],
            holds: [false; MAX_CONTACTS + 1],
            swipes: [false; MAX_CONTACTS + 1],
            eight_way: [false; MAX_CONTACTS + 1],
            edge_swipes: [false; MAX_CONTACTS + 1],
            pinches: [false; MAX_CONTACTS + 1],
            rotations: [false; MAX_CONTACTS + 1],
            clicks: [false; MAX_CONTACTS + 1],
            chords: false,
            draw: None,
            symbol_thresholds: Some(BTreeMap::new()),
            default_confidence: Settings::default().default_confidence,
        }
    }
}

fn draw_params(s: &Settings) -> DrawParams {
    DrawParams {
        activation: s.draw_activation,
        stroke_gap_us: s.stroke_gap_ms * 1000,
        min_stroke_mm: s.min_stroke_mm,
        hold_still_mm: s.hold_max_movement_mm,
    }
}

impl Spec {
    pub fn from_config(config: &Config) -> Self {
        let s = &config.settings;
        let mut spec = Self {
            draw: Some(draw_params(s)),
            default_confidence: s.default_confidence,
            ..Self::default()
        };
        let mut thresholds = BTreeMap::new();
        // Steps of sequences need recognizing like gestures of their own.
        let triggers = config
            .gestures
            .iter()
            .filter(|g| g.def.enabled)
            .flat_map(|g| g.trigger.iter().chain(&g.steps));
        for t in triggers {
            let n = usize::from(t.fingers()).min(MAX_CONTACTS);
            match t {
                Trigger::Tap {
                    taps, modifiers, ..
                } => {
                    spec.max_taps[n] = spec.max_taps[n].max(*taps);
                    spec.chords |= *modifiers > 0;
                }
                Trigger::Hold { .. } => spec.holds[n] = true,
                Trigger::Swipe {
                    direction,
                    modifiers,
                    ..
                } => {
                    spec.swipes[n] = true;
                    spec.eight_way[n] |= direction.is_diagonal();
                    spec.chords |= *modifiers > 0;
                }
                Trigger::EdgeSwipe { direction, .. } => {
                    spec.edge_swipes[n] = true;
                    spec.eight_way[n] |= direction.is_diagonal();
                }
                Trigger::Pinch { .. } => spec.pinches[n] = true,
                Trigger::Rotate { .. } => spec.rotations[n] = true,
                Trigger::Click { direction, .. } => {
                    spec.clicks[n] = true;
                    spec.eight_way[n] |= direction.is_some_and(Direction::is_diagonal);
                }
                Trigger::Symbol { names, confidence } => {
                    let conf = confidence.unwrap_or(s.default_confidence);
                    for name in names {
                        let t: &mut f32 = thresholds.entry(name.clone()).or_insert(conf);
                        *t = t.min(conf);
                    }
                }
            }
        }
        spec.symbol_thresholds = Some(thresholds);
        spec
    }

    /// Recognizes every variant: triple taps, holds with any number of
    /// fingers, 8-way swipes, pinches, rotations, clicks, every symbol with
    /// the default draw mode. For diagnostics, not for normal use, because
    /// every tap then waits for a possible second one.
    pub fn everything() -> Self {
        Self {
            max_taps: [3; MAX_CONTACTS + 1],
            holds: [true; MAX_CONTACTS + 1],
            swipes: [true; MAX_CONTACTS + 1],
            eight_way: [true; MAX_CONTACTS + 1],
            edge_swipes: [true; MAX_CONTACTS + 1],
            pinches: [true; MAX_CONTACTS + 1],
            rotations: [true; MAX_CONTACTS + 1],
            clicks: [true; MAX_CONTACTS + 1],
            chords: true,
            draw: Some(draw_params(&Settings::default())),
            symbol_thresholds: None,
            default_confidence: Settings::default().default_confidence,
        }
    }

    fn at(table: &[bool; MAX_CONTACTS + 1], fingers: u8) -> bool {
        fingers > 0 && table[usize::from(fingers).min(MAX_CONTACTS)]
    }

    pub fn has_edge_swipe(&self, fingers: u8) -> bool {
        Self::at(&self.edge_swipes, fingers)
    }

    pub fn has_pinch(&self, fingers: u8) -> bool {
        Self::at(&self.pinches, fingers)
    }

    pub fn has_rotation(&self, fingers: u8) -> bool {
        Self::at(&self.rotations, fingers)
    }

    pub fn has_click(&self, fingers: u8) -> bool {
        Self::at(&self.clicks, fingers)
    }

    pub fn max_taps(&self, fingers: u8) -> u8 {
        self.max_taps[usize::from(fingers).min(MAX_CONTACTS)]
    }

    pub fn has_hold(&self, fingers: u8) -> bool {
        fingers > 0 && self.holds[usize::from(fingers).min(MAX_CONTACTS)]
    }

    pub fn has_swipe(&self, fingers: u8) -> bool {
        fingers > 0 && self.swipes[usize::from(fingers).min(MAX_CONTACTS)]
    }

    pub fn eight_way(&self, fingers: u8) -> bool {
        self.eight_way[usize::from(fingers).min(MAX_CONTACTS)]
    }

    /// Score a symbol needs to be recognized; `None` if no gesture uses it,
    /// so drawing it can still fall back to a swipe.
    fn symbol_threshold(&self, name: &str) -> Option<f32> {
        match &self.symbol_thresholds {
            Some(t) => t.get(name).copied(),
            None => Some(self.default_confidence),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Recognition {
    pub device: DeviceId,
    pub observation: Observation,
    /// Timestamp, in the frame time base, of the moment of recognition.
    pub at_us: u64,
    /// When the movement itself ended. Later than `at_us` never; earlier for
    /// a swipe drawn as a stroke, which is only recognized once the stroke
    /// gap has passed. Sequences measure their window from here.
    pub ended_us: u64,
    /// When the movement itself began (the touch-down, or the first point of
    /// a drawing). Sequences allow their pause up to here.
    pub started_us: u64,
    /// What the fingers drew, for the history and its playback.
    pub trace: Arc<GestureTrace>,
}

/// The movement behind a recognition, with point times, so the history can
/// replay it faithfully (SPEC §10.1).
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct GestureTrace {
    /// Path of every finger, by contact id (taps, holds, swipes).
    pub fingers: BTreeMap<u32, Stroke>,
    /// Strokes of a drawing.
    pub strokes: Vec<Stroke>,
}

fn session_trace(s: &Session) -> Arc<GestureTrace> {
    Arc::new(GestureTrace {
        fingers: s.paths.clone(),
        strokes: Vec::new(),
    })
}

type Out = Vec<(Observation, Arc<GestureTrace>)>;

/// Points in millimeters.
pub type Polyline = Vec<(f32, f32)>;

/// A session whose device sends nothing for this long is dropped: the lift
/// was lost (e.g. the device was unplugged mid-touch). Touchpads report
/// continuously while fingers rest on them, so this never cuts a real touch.
const STALE_SESSION_US: u64 = 1_500_000;

#[derive(Debug, Default)]
struct DeviceState {
    tracker: SessionTracker,
    tap: tap::TapRecognizer,
    hold: hold::HoldRecognizer,
    motion: motion::MotionRecognizer,
    click: motion::ClickRecognizer,
    chord: chord::ChordRecognizer,
    draw: draw::DrawRecognizer,
    last_frame_us: u64,
    /// Where the movement of the symbol finished last ended, until used.
    symbol_ended_us: Option<u64>,
    /// Where that movement began, until used.
    symbol_started_us: Option<u64>,
    /// Start of the latest touch session.
    touch_start_us: u64,
    /// Trace of the last finished session, for taps confirmed later.
    last_trace: Arc<GestureTrace>,
}

/// End of the movement behind an observation: the lift of the last stroke
/// for what came out of a symbol in progress, else the moment itself.
fn ended_of(observation: &Observation, symbol_ended_us: Option<u64>, at_us: u64) -> u64 {
    match observation {
        Observation::Tap { .. } | Observation::Hold { .. } => at_us,
        _ => symbol_ended_us.map_or(at_us, |e| e.min(at_us)),
    }
}

/// Start of the movement behind an observation: the first point of a symbol
/// in progress for what came out of one, else the latest touch-down.
fn started_of(
    observation: &Observation,
    symbol_started_us: Option<u64>,
    touch_start_us: u64,
    at_us: u64,
) -> u64 {
    match observation {
        Observation::Tap { .. } | Observation::Hold { .. } => touch_start_us,
        _ => symbol_started_us.unwrap_or(touch_start_us),
    }
    .min(at_us)
}

impl DeviceState {
    fn reset(&mut self) {
        self.tracker.cancel();
        self.tap.reset();
        self.hold.reset();
        self.chord.reset();
        self.draw.reset();
    }
}

pub struct Engine {
    spec: Spec,
    thresholds: Thresholds,
    symbols: SymbolRecognizer,
    devices: HashMap<DeviceId, DeviceState>,
    /// Drawings become samples instead of being recognized (`glyphpad record`).
    recording: bool,
    arming: draw::Arming,
}

impl Engine {
    /// An engine with the built-in symbol templates.
    pub fn new(spec: Spec, thresholds: Thresholds) -> Self {
        Self {
            spec,
            thresholds,
            symbols: SymbolRecognizer::new(&templates::defaults(), 0.0),
            devices: HashMap::new(),
            recording: false,
            arming: draw::Arming::default(),
        }
    }

    pub fn from_config(config: &Config) -> Self {
        let mut engine = Self::new(
            Spec::from_config(config),
            Thresholds::from(&config.settings),
        );
        engine.set_templates(
            &templates::defaults(),
            config.settings.rotation_tolerance_deg,
        );
        engine
    }

    /// Replaces the symbol templates.
    pub fn set_templates(
        &mut self,
        symbols: &BTreeMap<String, Vec<Sample>>,
        rotation_tolerance_deg: f32,
    ) {
        self.symbols = SymbolRecognizer::new(symbols, rotation_tolerance_deg);
    }

    /// In recording mode drawings produce [`Observation::Sample`]s.
    pub fn set_recording(&mut self, recording: bool) {
        self.recording = recording;
        self.reset();
    }

    /// Applies a new configuration, keeping nothing of touches in progress.
    /// Templates are kept; set them again if they changed.
    pub fn reconfigure(&mut self, config: &Config) {
        self.spec = Spec::from_config(config);
        self.thresholds = Thresholds::from(&config.settings);
        self.reset();
    }

    /// Lets the next symbol be drawn with one finger, until `until_us`
    /// (the `draw_symbol` action).
    pub fn arm_drawing(&mut self, until_us: u64) {
        self.arming.until_us = Some(until_us);
    }

    /// Keeps one-finger drawing armed for every symbol (recording templates
    /// in `toggle` mode).
    pub fn keep_drawing_armed(&mut self, always: bool) {
        self.arming.always = always;
    }

    /// A symbol was completed: a `draw_symbol` arming is spent.
    fn spend_arming(&mut self, out: &[Recognition]) {
        if out.iter().any(|r| {
            matches!(
                r.observation,
                Observation::Symbol { .. }
                    | Observation::UnrecognizedSymbol { .. }
                    | Observation::Sample { .. }
            )
        }) {
            self.arming.until_us = None;
        }
    }

    /// Whether some touch in progress is being drawn, e.g. to freeze the
    /// cursor meanwhile.
    pub fn drawing_now(&self) -> bool {
        let Some(p) = &self.spec.draw else {
            return false;
        };
        self.devices.values().any(|st| {
            st.tracker
                .session()
                .is_some_and(|s| st.draw.drawing(s, p, self.arming))
        })
    }

    /// The symbol being drawn, in millimeters: its finished strokes and the
    /// one in progress, for live feedback.
    pub fn drawing_preview(&self) -> Option<(DeviceId, Vec<Polyline>)> {
        let p = self.spec.draw.as_ref()?;
        self.devices.iter().find_map(|(&id, st)| {
            let drawing = st
                .tracker
                .session()
                .filter(|s| st.draw.drawing(s, p, self.arming));
            if drawing.is_none() && st.draw.strokes().is_empty() {
                return None;
            }
            let mut strokes: Vec<Polyline> = st
                .draw
                .strokes()
                .iter()
                .map(|s| s.iter().map(|p| (p.x_mm, p.y_mm)).collect())
                .collect();
            if let Some(s) = drawing {
                strokes.push(st.draw.preview(s, p, self.arming));
            }
            Some((id, strokes))
        })
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
        let (spec, t, symbols, recording, arming) = (
            &self.spec,
            &self.thresholds,
            &self.symbols,
            self.recording,
            self.arming,
        );
        let st = self.devices.entry(device).or_default();
        let ts = frame.timestamp_us;
        st.last_frame_us = ts;
        let mut out: Out = Vec::new();

        let update = st.tracker.update(frame);
        let began = update == Some(session::SessionUpdate::Began);
        match update {
            None => {}
            Some(session::SessionUpdate::Ended) => {
                if let Some(s) = st.tracker.take() {
                    st.hold.session_ended();
                    let stroke = spec
                        .draw
                        .as_ref()
                        .and_then(|p| Some((st.draw.stroke_of(&s, p, arming)?, p)));
                    match stroke {
                        Some((samples, p)) => {
                            // A tap sequence cannot continue through a stroke.
                            Self::flush_taps(st, &mut out);
                            st.draw.add(samples, s, p);
                        }
                        None => {
                            // Not a stroke: a symbol in progress is complete.
                            out.extend(Self::finish_symbol(st, spec, t, symbols, recording));
                            let trace = session_trace(&s);
                            st.last_trace = trace.clone();
                            let obs = st.tap.session_ended(&s, spec, t).into_iter();
                            out.extend(
                                obs.chain(st.motion.session_ended(&s, spec, t))
                                    .chain(st.click.session_ended(&s, spec, t))
                                    .map(|o| (o, trace.clone())),
                            );
                        }
                    }
                }
            }
            Some(update) => {
                if update == session::SessionUpdate::Began {
                    let previous = st.last_trace.clone();
                    out.extend(st.tap.session_began(ts).map(|o| (o, previous)));
                    if st.draw.deadline().is_some_and(|d| ts >= d) {
                        out.extend(Self::finish_symbol(st, spec, t, symbols, recording));
                    }
                    st.draw.session_began();
                }
                if let Some(s) = st.tracker.session()
                    && let Some(hold) = st.hold.update(s, spec, t)
                {
                    Self::consume(st, &mut out, hold);
                }
                if let Some(s) = st.tracker.session()
                    && let Some(chord) = st.chord.update(s, spec, t)
                {
                    Self::flush_taps(st, &mut out);
                    let trace = st.tracker.session().map(session_trace).unwrap_or_default();
                    if let Some(s) = st.tracker.session_mut() {
                        s.chorded = true;
                    }
                    out.push((chord, trace));
                }
            }
        }
        let ended = st.symbol_ended_us.take();
        let started = st.symbol_started_us.take();
        let touch_start = st.touch_start_us;
        if began {
            st.touch_start_us = ts;
        }
        let out: Vec<Recognition> = out
            .into_iter()
            .map(|(observation, trace)| Recognition {
                device,
                ended_us: ended_of(&observation, ended, ts),
                started_us: started_of(&observation, started, touch_start, ts),
                observation,
                at_us: ts,
                trace,
            })
            .collect();
        self.spend_arming(&out);
        out
    }

    /// Confirms a tap sequence waiting for more taps; it belongs to the
    /// last finished session.
    fn flush_taps(st: &mut DeviceState, out: &mut Out) {
        let mut taps = Vec::new();
        st.tap.reset_into(&mut taps);
        out.extend(taps.into_iter().map(|o| (o, st.last_trace.clone())));
    }

    /// Recognizes the symbol in progress, if any; an unrecognized single
    /// stroke is judged as a tap or swipe instead.
    fn finish_symbol(
        st: &mut DeviceState,
        spec: &Spec,
        t: &Thresholds,
        symbols: &SymbolRecognizer,
        recording: bool,
    ) -> Out {
        let Some(done) = st.draw.finish() else {
            return Vec::new();
        };
        st.symbol_ended_us = Some(done.ended_us);
        st.symbol_started_us = Some(done.started_us);
        let trace = Arc::new(GestureTrace {
            fingers: BTreeMap::new(),
            strokes: done.strokes.clone(),
        });
        if recording {
            return vec![(
                Observation::Sample {
                    strokes: done.strokes,
                },
                trace,
            )];
        }
        let xy: Vec<Vec<(f32, f32)>> = done
            .strokes
            .iter()
            .map(|s| s.iter().map(|p| (p.x_mm, p.y_mm)).collect())
            .collect();
        let mut candidates = symbols.recognize(&xy);
        candidates.truncate(TOP_CANDIDATES);
        if let Some(best) = candidates.first()
            && spec
                .symbol_threshold(&best.symbol)
                .is_some_and(|min| best.score >= min)
        {
            return vec![(
                Observation::Symbol {
                    name: best.symbol.clone(),
                    score: best.score,
                    candidates,
                    strokes: done.strokes,
                },
                trace,
            )];
        }
        let mut out = vec![(
            Observation::UnrecognizedSymbol {
                candidates,
                strokes: done.strokes,
            },
            trace,
        )];
        if let Some(s) = done.fallback {
            let trace = session_trace(&s);
            let obs = st.tap.session_ended(&s, spec, t).into_iter();
            out.extend(
                obs.chain(st.motion.session_ended(&s, spec, t))
                    .map(|o| (o, trace.clone())),
            );
        }
        out
    }

    /// A hold fired mid-session: a tap sequence waiting for this touch is
    /// confirmed first, then the session is closed to other recognizers.
    fn consume(st: &mut DeviceState, out: &mut Out, hold: Observation) {
        Self::flush_taps(st, out);
        let trace = st.tracker.session().map(session_trace).unwrap_or_default();
        if let Some(s) = st.tracker.session_mut() {
            s.consumed = true;
        }
        out.push((hold, trace));
    }

    /// Advances time without new frames, for timeouts (multi-tap gap, holds,
    /// stale sessions). `now_us` is in the frame time base.
    pub fn tick(&mut self, now_us: u64) -> Vec<Recognition> {
        let (spec, t, symbols, recording) =
            (&self.spec, &self.thresholds, &self.symbols, self.recording);
        let mut all = Vec::new();
        for (&device, st) in &mut self.devices {
            let mut out: Out = Vec::new();
            if st.tracker.session().is_some() && now_us >= st.last_frame_us + STALE_SESSION_US {
                tracing::debug!(device = device.0, "dropping a stale touch session");
                st.tracker.cancel();
                st.hold.reset();
            }
            let previous = st.last_trace.clone();
            out.extend(st.tap.tick(now_us).map(|o| (o, previous)));
            if st.draw.deadline().is_some_and(|d| now_us >= d) {
                out.extend(Self::finish_symbol(st, spec, t, symbols, recording));
            }
            if st.tracker.session().is_some()
                && let Some(hold) = st.hold.tick(now_us, t)
            {
                Self::consume(st, &mut out, hold);
            }
            let ended = st.symbol_ended_us.take();
            let started = st.symbol_started_us.take();
            let touch_start = st.touch_start_us;
            all.extend(out.into_iter().map(|(observation, trace)| Recognition {
                device,
                ended_us: ended_of(&observation, ended, now_us),
                started_us: started_of(&observation, started, touch_start, now_us),
                observation,
                at_us: now_us,
                trace,
            }));
        }
        self.spend_arming(&all);
        all
    }

    /// Runs a recorded session deterministically: ticks at every deadline
    /// between frames, exactly as the live loop does, then lets the last
    /// timeouts expire.
    pub fn process_all<'a>(
        &mut self,
        events: impl IntoIterator<Item = &'a DeviceEvent>,
    ) -> Vec<Recognition> {
        let mut out = Vec::new();
        for event in events {
            if let DeviceEvent::Frame { frame, .. } = event {
                while let Some(d) = self.next_deadline().filter(|&d| d <= frame.timestamp_us) {
                    out.extend(self.tick(d));
                }
            }
            out.extend(self.on_event(event));
        }
        while let Some(d) = self.next_deadline() {
            out.extend(self.tick(d));
        }
        out
    }

    /// Start of the touch or drawing in progress, if any: something is being
    /// done and will be recognized once it is over.
    pub fn busy_since(&self) -> Option<u64> {
        self.devices
            .values()
            .flat_map(|st| [st.tracker.session().map(|s| s.start_us), st.draw.origin()])
            .flatten()
            .min()
    }

    /// Earliest time at which [`tick`](Self::tick) may produce something.
    pub fn next_deadline(&self) -> Option<u64> {
        self.devices
            .values()
            .flat_map(|st| {
                [
                    st.tap.deadline(),
                    st.draw.deadline(),
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

/// The configured gesture an observation triggers: among the gestures that
/// match and apply to the active profile and the foreground app, the most
/// specific one, then the first in priority order. Sequences are matched by
/// the [`Dispatcher`](crate::dispatch::Dispatcher), not here.
pub fn resolve<'a>(
    config: &'a Config,
    observation: &Observation,
    app: Option<&AppInfo>,
) -> Option<&'a Gesture> {
    let conf = config.settings.default_confidence;
    let mut best: Option<(&Gesture, u8)> = None;
    for g in &config.gestures {
        let Some(t) = &g.trigger else { continue };
        let s = t.specificity(observation, conf);
        if s > best.map_or(0, |b| b.1) && g.applies(&config.settings.active_profile, app) {
            best = Some((g, s));
        }
    }
    best.map(|b| b.0)
}

#[cfg(test)]
mod tests;
