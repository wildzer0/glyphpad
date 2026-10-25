//! What sets a gesture off (SPEC §6.1): validated triggers and how they
//! match the engine's observations.

use serde::{Deserialize, Serialize};

use crate::config::{Direction, Edge, GestureKind, OneOrMany};
use crate::engine::Observation;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Zoom {
    /// Fingers moving closer together.
    In,
    /// Fingers spreading apart.
    Out,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Turn {
    Clockwise,
    Counterclockwise,
}

/// A rectangle of the touchpad, in millimetres from its top-left corner
/// (SPEC §2: spatial thresholds are in millimetres, so a zone survives a
/// change of touchpad).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Region {
    /// Left and right edges.
    pub x: [f32; 2],
    /// Top and bottom edges.
    pub y: [f32; 2],
}

impl Region {
    /// Whether a touch at `(x, y)` millimetres is inside.
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x[0] && x <= self.x[1] && y >= self.y[0] && y <= self.y[1]
    }
}

/// The trigger fields of a gesture, as written in the file; also the shape
/// of each step of a sequence.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerDef {
    #[serde(rename = "type")]
    pub kind: Option<GestureKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingers: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub taps: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<Direction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge: Option<Edge>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zoom: Option<Zoom>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<Turn>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modifier_fingers: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<OneOrMany>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    /// Overrides of the global threshold, for this gesture alone: a swipe has
    /// to be longer (or shorter) than the setting says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_distance_mm: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_speed_mm_s: Option<f32>,
    /// A pinch has to change the fingers' spread by this many millimetres.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_change_mm: Option<f32>,
    /// A rotation has to turn this many degrees.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_deg: Option<f32>,
    /// In a sequence step: the pause allowed after this step before the next
    /// one starts, overriding the sequence's own `within_ms`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub within_ms: Option<u64>,
    /// Where on the touchpad a `zone` gesture has to land.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<Region>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Trigger {
    /// A tap inside a rectangle of the touchpad.
    Zone {
        fingers: u8,
        taps: u8,
        region: Region,
    },
    /// `modifiers` resting fingers make it a chord.
    Tap {
        fingers: u8,
        taps: u8,
        modifiers: u8,
    },
    Hold {
        fingers: u8,
    },
    Swipe {
        fingers: u8,
        direction: Direction,
        modifiers: u8,
        min_distance_mm: Option<f32>,
        min_speed_mm_s: Option<f32>,
    },
    /// A swipe starting at an edge of the touchpad.
    EdgeSwipe {
        edge: Edge,
        fingers: u8,
        direction: Direction,
        min_distance_mm: Option<f32>,
        min_speed_mm_s: Option<f32>,
    },
    Pinch {
        fingers: u8,
        zoom: Zoom,
        min_change_mm: Option<f32>,
    },
    Rotate {
        fingers: u8,
        turn: Turn,
        min_deg: Option<f32>,
    },
    /// A physical click with `fingers` fingers, or a click-and-drag in `direction`.
    Click {
        fingers: u8,
        direction: Option<Direction>,
    },
    Symbol {
        names: Vec<String>,
        confidence: Option<f32>,
    },
}

type FieldErrors = Vec<(&'static str, String)>;

impl TriggerDef {
    /// Validates the fields for the kind; errors name the field.
    pub fn build(&self) -> Result<Trigger, FieldErrors> {
        let mut errors: FieldErrors = Vec::new();
        let Some(kind) = self.kind else {
            return Err(vec![("type", "required".into())]);
        };
        let max = crate::MAX_CONTACTS as u8;
        let mut allowed: Vec<&str> = Vec::new();
        let mut need = |field: &'static str, present: bool, errors: &mut FieldErrors| {
            allowed.push(field);
            if !present {
                errors.push((field, format!("required for a {} gesture", kind.label())));
            }
        };

        let fingers = self.fingers.unwrap_or(0);
        let min_fingers = match kind {
            GestureKind::Pinch | GestureKind::Rotate => 2,
            _ => 1,
        };
        if kind != GestureKind::Symbol && kind != GestureKind::Sequence {
            need("fingers", self.fingers.is_some(), &mut errors);
            if self.fingers.is_some() && !(min_fingers..=max).contains(&fingers) {
                errors.push((
                    "fingers",
                    format!("must be between {min_fingers} and {max}"),
                ));
            }
        }
        match kind {
            GestureKind::Tap => {
                allowed.extend(["taps", "modifier_fingers"]);
            }
            GestureKind::Swipe => {
                need("direction", self.direction.is_some(), &mut errors);
                allowed.extend(["modifier_fingers", "min_distance_mm", "min_speed_mm_s"]);
            }
            GestureKind::EdgeSwipe => {
                need("edge", self.edge.is_some(), &mut errors);
                need("direction", self.direction.is_some(), &mut errors);
                allowed.extend(["min_distance_mm", "min_speed_mm_s"]);
            }
            GestureKind::Zone => {
                need("region", self.region.is_some(), &mut errors);
                allowed.extend(["taps", "region"]);
            }
            GestureKind::Pinch => {
                need("zoom", self.zoom.is_some(), &mut errors);
                allowed.push("min_change_mm");
            }
            GestureKind::Rotate => {
                need("rotation", self.rotation.is_some(), &mut errors);
                allowed.push("min_deg");
            }
            GestureKind::Click => allowed.push("direction"),
            GestureKind::Symbol => {
                need("symbol", self.symbol.is_some(), &mut errors);
                allowed.push("confidence");
            }
            GestureKind::Hold | GestureKind::Sequence => {}
        }
        for (field, present) in [
            ("fingers", self.fingers.is_some()),
            ("taps", self.taps.is_some()),
            ("direction", self.direction.is_some()),
            ("edge", self.edge.is_some()),
            ("zoom", self.zoom.is_some()),
            ("rotation", self.rotation.is_some()),
            ("modifier_fingers", self.modifier_fingers.is_some()),
            ("symbol", self.symbol.is_some()),
            ("confidence", self.confidence.is_some()),
            ("min_distance_mm", self.min_distance_mm.is_some()),
            ("min_speed_mm_s", self.min_speed_mm_s.is_some()),
            ("min_change_mm", self.min_change_mm.is_some()),
            ("min_deg", self.min_deg.is_some()),
            ("region", self.region.is_some()),
        ] {
            if present && !allowed.contains(&field) {
                errors.push((
                    field,
                    format!("a {} gesture does not take `{field}`", kind.label()),
                ));
            }
        }
        if let Some(t) = self.taps
            && !(1..=3).contains(&t)
        {
            errors.push(("taps", "must be 1, 2 or 3".into()));
        }
        let modifiers = self.modifier_fingers.unwrap_or(0);
        if self.modifier_fingers.is_some() && !(1..=3).contains(&modifiers) {
            errors.push(("modifier_fingers", "must be 1, 2 or 3".into()));
        } else if modifiers > 0 && fingers + modifiers > max {
            errors.push((
                "modifier_fingers",
                format!("fingers and modifier fingers exceed {max}"),
            ));
        }
        if kind == GestureKind::Tap && modifiers > 0 && self.taps.is_some_and(|t| t > 1) {
            errors.push(("taps", "chords support single taps only".into()));
        }
        if let Some(c) = self.confidence
            && !(0.0..=1.0).contains(&c)
        {
            errors.push(("confidence", "must be between 0 and 1".into()));
        }
        if let Some(d) = self.min_distance_mm
            && !(1.0..=200.0).contains(&d)
        {
            errors.push(("min_distance_mm", "must be between 1 and 200".into()));
        }
        if let Some(v) = self.min_speed_mm_s
            && !(0.0..=10_000.0).contains(&v)
        {
            errors.push(("min_speed_mm_s", "must be between 0 and 10000".into()));
        }
        if let Some(c) = self.min_change_mm
            && !(1.0..=100.0).contains(&c)
        {
            errors.push(("min_change_mm", "must be between 1 and 100".into()));
        }
        if let Some(d) = self.min_deg
            && !(1.0..=180.0).contains(&d)
        {
            errors.push(("min_deg", "must be between 1 and 180".into()));
        }
        if let Some(w) = self.within_ms
            && !(100..=5_000).contains(&w)
        {
            errors.push(("within_ms", "must be between 100 and 5000".into()));
        }
        if let Some(r) = &self.region {
            let in_range = |v: f32| (0.0..=400.0).contains(&v);
            if !(in_range(r.x[0]) && in_range(r.x[1]) && in_range(r.y[0]) && in_range(r.y[1])) {
                errors.push(("region", "edges must be between 0 and 400 mm".into()));
            } else if r.x[0] >= r.x[1] || r.y[0] >= r.y[1] {
                errors.push((
                    "region",
                    "the first edge of `x` and `y` must be left of and above the second".into(),
                ));
            }
        }
        let names = self
            .symbol
            .as_ref()
            .map(OneOrMany::to_vec)
            .unwrap_or_default();
        if self.symbol.is_some() && (names.is_empty() || names.iter().any(|n| n.trim().is_empty()))
        {
            errors.push(("symbol", "symbol names must not be empty".into()));
        }
        if !errors.is_empty() {
            return Err(errors);
        }
        Ok(match kind {
            GestureKind::Zone => Trigger::Zone {
                fingers,
                taps: self.taps.unwrap_or(1),
                region: self.region.unwrap_or(Region {
                    x: [0.0, 0.0],
                    y: [0.0, 0.0],
                }),
            },
            GestureKind::Tap => Trigger::Tap {
                fingers,
                taps: self.taps.unwrap_or(1),
                modifiers,
            },
            GestureKind::Hold => Trigger::Hold { fingers },
            GestureKind::Swipe => Trigger::Swipe {
                fingers,
                direction: self.direction.unwrap_or(Direction::Up),
                modifiers,
                min_distance_mm: self.min_distance_mm,
                min_speed_mm_s: self.min_speed_mm_s,
            },
            GestureKind::EdgeSwipe => Trigger::EdgeSwipe {
                edge: self.edge.unwrap_or_default(),
                fingers,
                direction: self.direction.unwrap_or(Direction::Up),
                min_distance_mm: self.min_distance_mm,
                min_speed_mm_s: self.min_speed_mm_s,
            },
            GestureKind::Pinch => Trigger::Pinch {
                fingers,
                zoom: self.zoom.unwrap_or(Zoom::In),
                min_change_mm: self.min_change_mm,
            },
            GestureKind::Rotate => Trigger::Rotate {
                fingers,
                turn: self.rotation.unwrap_or(Turn::Clockwise),
                min_deg: self.min_deg,
            },
            GestureKind::Click => Trigger::Click {
                fingers,
                direction: self.direction,
            },
            GestureKind::Symbol => Trigger::Symbol {
                names,
                confidence: self.confidence,
            },
            GestureKind::Sequence => {
                return Err(vec![(
                    "type",
                    "a sequence step cannot be a sequence".into(),
                )]);
            }
        })
    }
}

impl Trigger {
    /// How specifically this trigger matches the observation: 0 for no
    /// match. A swipe from an edge matches edge-swipe gestures (2) before
    /// plain swipe gestures (1), whatever their order (SPEC §6.1: the most
    /// specific gesture wins).
    pub fn specificity(&self, obs: &Observation, default_confidence: f32) -> u8 {
        self.specificity_at(obs, default_confidence, None)
    }

    /// The same, with where the touch landed in millimetres: only a `zone`
    /// gesture looks at it, and without it a zone never matches.
    pub fn specificity_at(
        &self,
        obs: &Observation,
        default_confidence: f32,
        at: Option<(f32, f32)>,
    ) -> u8 {
        let yes = |b: bool| if b { 2 } else { 0 };
        match (self, obs) {
            (
                Trigger::Zone {
                    fingers,
                    taps,
                    region,
                },
                Observation::Tap {
                    fingers: f,
                    taps: t,
                },
            ) => u8::from(
                f == fingers && t == taps && at.is_some_and(|(x, y)| region.contains(x, y)),
            ),
            (
                Trigger::Tap {
                    fingers,
                    taps,
                    modifiers: 0,
                },
                Observation::Tap {
                    fingers: f,
                    taps: t,
                },
            ) => yes(f == fingers && t == taps),
            (
                Trigger::Tap {
                    fingers,
                    taps,
                    modifiers,
                },
                Observation::Chord {
                    modifiers: m,
                    gesture,
                },
            ) if *modifiers > 0 => yes(m == modifiers
                && matches!(**gesture, Observation::Tap { fingers: f, taps: t } if f == *fingers && t == *taps)),
            (Trigger::Hold { fingers }, Observation::Hold { fingers: f }) => yes(f == fingers),
            (
                Trigger::Swipe {
                    fingers,
                    direction,
                    modifiers: 0,
                    min_distance_mm,
                    min_speed_mm_s,
                },
                Observation::Swipe {
                    fingers: f,
                    direction: d,
                    distance_mm,
                    speed_mm_s,
                },
            ) => yes(f == fingers
                && d == direction
                && min_distance_mm.is_none_or(|min| *distance_mm >= min)
                && min_speed_mm_s.is_none_or(|min| *speed_mm_s >= min)),
            (
                Trigger::Swipe {
                    fingers,
                    direction,
                    modifiers: 0,
                    min_distance_mm,
                    ..
                },
                Observation::EdgeSwipe {
                    fingers: f,
                    direction: d,
                    distance_mm,
                    ..
                },
            ) => u8::from(
                f == fingers
                    && d == direction
                    && min_distance_mm.is_none_or(|min| *distance_mm >= min),
            ),
            (
                Trigger::Swipe {
                    fingers,
                    direction,
                    modifiers,
                    min_distance_mm,
                    min_speed_mm_s,
                },
                Observation::Chord {
                    modifiers: m,
                    gesture,
                },
            ) if *modifiers > 0 => yes(m == modifiers
                && matches!(
                    **gesture,
                    Observation::Swipe { fingers: f, direction: d, distance_mm, speed_mm_s }
                        if f == *fingers
                            && d == *direction
                            && min_distance_mm.is_none_or(|min| distance_mm >= min)
                            && min_speed_mm_s.is_none_or(|min| speed_mm_s >= min)
                )),
            (
                Trigger::EdgeSwipe {
                    edge,
                    fingers,
                    direction,
                    min_distance_mm,
                    min_speed_mm_s,
                },
                Observation::EdgeSwipe {
                    edge: e,
                    fingers: f,
                    direction: d,
                    distance_mm,
                },
            ) => yes(e == edge
                && f == fingers
                && d == direction
                && min_distance_mm.is_none_or(|min| *distance_mm >= min)
                // The observation carries no speed; a gesture asking for a
                // minimum speed cannot be matched from an edge swipe.
                && min_speed_mm_s.is_none_or(|min| min <= 0.0)),
            (
                Trigger::Pinch {
                    fingers,
                    zoom,
                    min_change_mm,
                },
                Observation::Pinch {
                    fingers: f,
                    zoom: z,
                    change_mm,
                },
            ) => yes(f == fingers
                && z == zoom
                && min_change_mm.is_none_or(|min| change_mm.abs() >= min)),
            (
                Trigger::Rotate {
                    fingers,
                    turn,
                    min_deg,
                },
                Observation::Rotate {
                    fingers: f,
                    turn: t,
                    degrees,
                },
            ) => yes(f == fingers && t == turn && min_deg.is_none_or(|min| degrees.abs() >= min)),
            (
                Trigger::Click { fingers, direction },
                Observation::Click {
                    fingers: f,
                    direction: d,
                },
            ) => yes(f == fingers && d == direction),
            (Trigger::Symbol { names, confidence }, Observation::Symbol { name, score, .. }) => {
                yes(names.contains(name) && *score >= confidence.unwrap_or(default_confidence))
            }
            _ => 0,
        }
    }

    /// Fingers the movement uses (modifiers excluded); 0 for symbols.
    pub fn fingers(&self) -> u8 {
        match *self {
            Trigger::Tap { fingers, .. }
            | Trigger::Hold { fingers }
            | Trigger::Swipe { fingers, .. }
            | Trigger::EdgeSwipe { fingers, .. }
            | Trigger::Pinch { fingers, .. }
            | Trigger::Rotate { fingers, .. }
            | Trigger::Click { fingers, .. }
            | Trigger::Zone { fingers, .. } => fingers,
            Trigger::Symbol { .. } => 0,
        }
    }

    /// A short human description, e.g. `swipe up with 3 fingers`.
    pub fn describe(&self) -> String {
        let f = |n: u8| {
            if n == 1 {
                "1 finger".to_string()
            } else {
                format!("{n} fingers")
            }
        };
        let chord = |m: u8| {
            if m > 0 {
                format!(" while {} rest", f(m))
            } else {
                String::new()
            }
        };
        match self {
            Trigger::Tap {
                fingers,
                taps: 1,
                modifiers,
            } => format!("tap with {}{}", f(*fingers), chord(*modifiers)),
            Trigger::Tap {
                fingers,
                taps,
                modifiers,
            } => format!("{taps}× tap with {}{}", f(*fingers), chord(*modifiers)),
            Trigger::Hold { fingers } => format!("hold {}", f(*fingers)),
            Trigger::Swipe {
                fingers,
                direction,
                modifiers,
                ..
            } => {
                format!(
                    "swipe {direction} with {}{}",
                    f(*fingers),
                    chord(*modifiers)
                )
            }
            Trigger::EdgeSwipe {
                edge,
                fingers,
                direction,
                ..
            } => {
                format!(
                    "swipe {direction} from the {} edge with {}",
                    format!("{edge:?}").to_lowercase(),
                    f(*fingers)
                )
            }
            Trigger::Pinch { fingers, zoom, .. } => {
                format!(
                    "pinch {} with {}",
                    if *zoom == Zoom::In { "in" } else { "out" },
                    f(*fingers)
                )
            }
            Trigger::Rotate { fingers, turn, .. } => format!(
                "rotate {} with {}",
                if *turn == Turn::Clockwise {
                    "clockwise"
                } else {
                    "counterclockwise"
                },
                f(*fingers)
            ),
            Trigger::Click {
                fingers,
                direction: None,
            } => format!("click with {}", f(*fingers)),
            Trigger::Click {
                fingers,
                direction: Some(d),
            } => format!("click and drag {d} with {}", f(*fingers)),
            Trigger::Symbol { names, .. } => format!("draw {}", names.join(" or ")),
            Trigger::Zone {
                fingers,
                taps,
                region,
            } => {
                let tap = if *taps == 1 {
                    "tap".to_string()
                } else {
                    format!("{taps}× tap")
                };
                format!(
                    "{tap} with {} at {}–{} × {}–{} mm",
                    f(*fingers),
                    region.x[0],
                    region.x[1],
                    region.y[0],
                    region.y[1]
                )
            }
        }
    }
}

impl GestureKind {
    pub fn label(self) -> &'static str {
        match self {
            GestureKind::Tap => "tap",
            GestureKind::Zone => "zone",
            GestureKind::Hold => "hold",
            GestureKind::Swipe => "swipe",
            GestureKind::EdgeSwipe => "edge swipe",
            GestureKind::Pinch => "pinch",
            GestureKind::Rotate => "rotate",
            GestureKind::Click => "click",
            GestureKind::Symbol => "symbol",
            GestureKind::Sequence => "sequence",
        }
    }
}
