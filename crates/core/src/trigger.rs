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
}

#[derive(Debug, Clone, PartialEq)]
pub enum Trigger {
    /// `modifiers` resting fingers make it a chord.
    Tap { fingers: u8, taps: u8, modifiers: u8 },
    Hold { fingers: u8 },
    Swipe { fingers: u8, direction: Direction, modifiers: u8 },
    /// A swipe starting at an edge of the touchpad.
    EdgeSwipe { edge: Edge, fingers: u8, direction: Direction },
    Pinch { fingers: u8, zoom: Zoom },
    Rotate { fingers: u8, turn: Turn },
    /// A physical click with `fingers` fingers, or a click-and-drag in `direction`.
    Click { fingers: u8, direction: Option<Direction> },
    Symbol { names: Vec<String>, confidence: Option<f32> },
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
                errors.push(("fingers", format!("must be between {min_fingers} and {max}")));
            }
        }
        match kind {
            GestureKind::Tap => {
                allowed.extend(["taps", "modifier_fingers"]);
            }
            GestureKind::Swipe => {
                need("direction", self.direction.is_some(), &mut errors);
                allowed.push("modifier_fingers");
            }
            GestureKind::EdgeSwipe => {
                need("edge", self.edge.is_some(), &mut errors);
                need("direction", self.direction.is_some(), &mut errors);
            }
            GestureKind::Pinch => need("zoom", self.zoom.is_some(), &mut errors),
            GestureKind::Rotate => need("rotation", self.rotation.is_some(), &mut errors),
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
        ] {
            if present && !allowed.contains(&field) {
                errors.push((field, format!("a {} gesture does not take `{field}`", kind.label())));
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
            errors.push(("modifier_fingers", format!("fingers and modifier fingers exceed {max}")));
        }
        if kind == GestureKind::Tap && modifiers > 0 && self.taps.is_some_and(|t| t > 1) {
            errors.push(("taps", "chords support single taps only".into()));
        }
        if let Some(c) = self.confidence
            && !(0.0..=1.0).contains(&c)
        {
            errors.push(("confidence", "must be between 0 and 1".into()));
        }
        let names = self.symbol.as_ref().map(OneOrMany::to_vec).unwrap_or_default();
        if self.symbol.is_some() && (names.is_empty() || names.iter().any(|n| n.trim().is_empty())) {
            errors.push(("symbol", "symbol names must not be empty".into()));
        }
        if !errors.is_empty() {
            return Err(errors);
        }
        Ok(match kind {
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
            },
            GestureKind::EdgeSwipe => Trigger::EdgeSwipe {
                edge: self.edge.unwrap_or_default(),
                fingers,
                direction: self.direction.unwrap_or(Direction::Up),
            },
            GestureKind::Pinch => Trigger::Pinch {
                fingers,
                zoom: self.zoom.unwrap_or(Zoom::In),
            },
            GestureKind::Rotate => Trigger::Rotate {
                fingers,
                turn: self.rotation.unwrap_or(Turn::Clockwise),
            },
            GestureKind::Click => Trigger::Click {
                fingers,
                direction: self.direction,
            },
            GestureKind::Symbol => Trigger::Symbol {
                names,
                confidence: self.confidence,
            },
            GestureKind::Sequence => return Err(vec![("type", "a sequence step cannot be a sequence".into())]),
        })
    }
}

impl Trigger {
    /// How specifically this trigger matches the observation: 0 for no
    /// match. A swipe from an edge matches edge-swipe gestures (2) before
    /// plain swipe gestures (1), whatever their order (SPEC §6.1: the most
    /// specific gesture wins).
    pub fn specificity(&self, obs: &Observation, default_confidence: f32) -> u8 {
        let yes = |b: bool| if b { 2 } else { 0 };
        match (self, obs) {
            (Trigger::Tap { fingers, taps, modifiers: 0 }, Observation::Tap { fingers: f, taps: t }) => {
                yes(f == fingers && t == taps)
            }
            (Trigger::Tap { fingers, taps, modifiers }, Observation::Chord { modifiers: m, gesture })
                if *modifiers > 0 =>
            {
                yes(m == modifiers && matches!(**gesture, Observation::Tap { fingers: f, taps: t } if f == *fingers && t == *taps))
            }
            (Trigger::Hold { fingers }, Observation::Hold { fingers: f }) => yes(f == fingers),
            (Trigger::Swipe { fingers, direction, modifiers: 0 }, Observation::Swipe { fingers: f, direction: d, .. }) => {
                yes(f == fingers && d == direction)
            }
            (Trigger::Swipe { fingers, direction, modifiers: 0 }, Observation::EdgeSwipe { fingers: f, direction: d, .. }) => {
                u8::from(f == fingers && d == direction)
            }
            (Trigger::Swipe { fingers, direction, modifiers }, Observation::Chord { modifiers: m, gesture })
                if *modifiers > 0 =>
            {
                yes(m == modifiers
                    && matches!(**gesture, Observation::Swipe { fingers: f, direction: d, .. } if f == *fingers && d == *direction))
            }
            (
                Trigger::EdgeSwipe { edge, fingers, direction },
                Observation::EdgeSwipe { edge: e, fingers: f, direction: d, .. },
            ) => yes(e == edge && f == fingers && d == direction),
            (Trigger::Pinch { fingers, zoom }, Observation::Pinch { fingers: f, zoom: z, .. }) => {
                yes(f == fingers && z == zoom)
            }
            (Trigger::Rotate { fingers, turn }, Observation::Rotate { fingers: f, turn: t, .. }) => {
                yes(f == fingers && t == turn)
            }
            (Trigger::Click { fingers, direction }, Observation::Click { fingers: f, direction: d }) => {
                yes(f == fingers && d == direction)
            }
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
            | Trigger::Click { fingers, .. } => fingers,
            Trigger::Symbol { .. } => 0,
        }
    }

    /// A short human description, e.g. `swipe up with 3 fingers`.
    pub fn describe(&self) -> String {
        let f = |n: u8| if n == 1 { "1 finger".to_string() } else { format!("{n} fingers") };
        let chord = |m: u8| if m > 0 { format!(" while {} rest", f(m)) } else { String::new() };
        match self {
            Trigger::Tap { fingers, taps: 1, modifiers } => format!("tap with {}{}", f(*fingers), chord(*modifiers)),
            Trigger::Tap { fingers, taps, modifiers } => format!("{taps}× tap with {}{}", f(*fingers), chord(*modifiers)),
            Trigger::Hold { fingers } => format!("hold {}", f(*fingers)),
            Trigger::Swipe { fingers, direction, modifiers } => {
                format!("swipe {direction} with {}{}", f(*fingers), chord(*modifiers))
            }
            Trigger::EdgeSwipe { edge, fingers, direction } => {
                format!("swipe {direction} from the {} edge with {}", format!("{edge:?}").to_lowercase(), f(*fingers))
            }
            Trigger::Pinch { fingers, zoom } => {
                format!("pinch {} with {}", if *zoom == Zoom::In { "in" } else { "out" }, f(*fingers))
            }
            Trigger::Rotate { fingers, turn } => format!(
                "rotate {} with {}",
                if *turn == Turn::Clockwise { "clockwise" } else { "counterclockwise" },
                f(*fingers)
            ),
            Trigger::Click { fingers, direction: None } => format!("click with {}", f(*fingers)),
            Trigger::Click { fingers, direction: Some(d) } => format!("click and drag {d} with {}", f(*fingers)),
            Trigger::Symbol { names, .. } => format!("draw {}", names.join(" or ")),
        }
    }
}

impl GestureKind {
    pub fn label(self) -> &'static str {
        match self {
            GestureKind::Tap => "tap",
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
