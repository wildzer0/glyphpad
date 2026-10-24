//! The TOML configuration: schema, defaults and validation with readable
//! errors (line, field, reason) (SPEC §9).

use std::collections::HashSet;
use std::fmt;

use regex::Regex;
use serde::{Deserialize, Serialize};
use toml::Spanned;

use crate::action::{Action, AppInfo};

/// Version of the configuration format written by this build.
pub const CONFIG_VERSION: u32 = 1;

/// The example configuration shipped with the release, also used as the
/// initial configuration on first start.
pub const EXAMPLE_CONFIG: &str = include_str!("../../../glyphpad.example.toml");

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    /// Gestures with a `profile` only fire while it is active.
    pub active_profile: String,

    // Thresholds. Distances are in millimeters so they hold on touchpads of any size.
    /// Longest touch still counted as a tap. Windows' own tap detection
    /// accepts roughly 200–300 ms; slower touches feel like presses.
    pub tap_max_duration_ms: u64,
    /// How far any finger may drift during a tap. Resting fingers jitter by
    /// about 1 mm and multi-finger taps roll a little; 4 mm separates that
    /// from deliberate movement.
    pub tap_max_movement_mm: f32,
    /// Longest pause between the taps of a double or triple tap.
    pub multi_tap_gap_ms: u64,
    /// How long fingers must rest before a hold fires.
    pub hold_duration_ms: u64,
    pub hold_max_movement_mm: f32,
    /// Shortest travel of the fingers' centroid for a swipe. Deliberate
    /// swipes recorded on the reference touchpad travelled 23–45 mm while
    /// taps drift under 1 mm, so 18 mm keeps short swipes without catching
    /// fingers that merely settle.
    pub swipe_min_distance_mm: f32,
    /// Slowest average speed for a swipe; slower travel is a drag.
    pub swipe_min_speed_mm_s: f32,

    // Symbols and draw mode (M3).
    pub stroke_gap_ms: u64,
    pub default_confidence: f32,
    /// Draw-mode activation, interpreted from M3 on.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub draw_activation: Option<toml::Table>,

    // repeat_last (M4).
    pub repeat_timeout_s: Option<u64>,
    pub repeat_same_app_only: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            active_profile: "default".into(),
            tap_max_duration_ms: 250,
            tap_max_movement_mm: 4.0,
            multi_tap_gap_ms: 300,
            hold_duration_ms: 600,
            hold_max_movement_mm: 4.0,
            swipe_min_distance_mm: 18.0,
            swipe_min_speed_mm_s: 60.0,
            stroke_gap_ms: 400,
            default_confidence: 0.80,
            draw_activation: None,
            repeat_timeout_s: None,
            repeat_same_app_only: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateMode {
    Off,
    #[default]
    Notify,
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateChannel {
    #[default]
    Stable,
    Prerelease,
}

/// `[update]`, used by the updater (M6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UpdateSettings {
    pub mode: UpdateMode,
    pub channel: UpdateChannel,
    pub check_interval_h: u32,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self {
            mode: UpdateMode::Notify,
            channel: UpdateChannel::Stable,
            check_interval_h: 24,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GestureKind {
    Tap,
    Hold,
    Swipe,
    Symbol,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
    UpLeft,
    UpRight,
    DownLeft,
    DownRight,
}

impl Direction {
    pub fn is_diagonal(self) -> bool {
        matches!(
            self,
            Direction::UpLeft | Direction::UpRight | Direction::DownLeft | Direction::DownRight
        )
    }
}

impl fmt::Display for Direction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Direction::Up => "up",
            Direction::Down => "down",
            Direction::Left => "left",
            Direction::Right => "right",
            Direction::UpLeft => "up_left",
            Direction::UpRight => "up_right",
            Direction::DownLeft => "down_left",
            Direction::DownRight => "down_right",
        };
        f.write_str(s)
    }
}

/// A string or a list of strings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

impl OneOrMany {
    pub fn to_vec(&self) -> Vec<String> {
        match self {
            OneOrMany::One(s) => vec![s.clone()],
            OneOrMany::Many(v) => v.clone(),
        }
    }
}

/// Conditions under which a gesture fires.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct When {
    /// Foreground executable names, e.g. `["chrome.exe", "firefox.exe"]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<OneOrMany>,
    /// Regular expression matched against the foreground window title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// One `[[gesture]]` table as written in the file. A single flat struct for
/// every type keeps the file format simple; [`validate`] checks which fields
/// each type accepts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GestureDef {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: GestureKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingers: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub taps: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<Direction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<OneOrMany>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    pub action: Action,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<When>,
    /// Whether `repeat_last` may repeat this gesture's action (M4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repeatable: Option<bool>,
}

fn yes() -> bool {
    true
}

fn is_true(b: &bool) -> bool {
    *b
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    config_version: Option<u32>,
    #[serde(default)]
    settings: Settings,
    #[serde(default)]
    update: UpdateSettings,
    #[serde(default, rename = "gesture")]
    gestures: Vec<Spanned<GestureDef>>,
}

/// A validated gesture.
#[derive(Debug, Clone)]
pub struct Gesture {
    pub def: GestureDef,
    /// Line of the gesture's table in the file (1-based).
    pub line: usize,
    title_regex: Option<Regex>,
    apps: Vec<String>,
}

impl Gesture {
    /// Fingers of a tap, hold or swipe.
    pub fn fingers(&self) -> u8 {
        self.def.fingers.unwrap_or(0)
    }

    pub fn taps(&self) -> u8 {
        self.def.taps.unwrap_or(1)
    }

    /// Whether the gesture may fire with this profile and foreground app.
    pub fn applies(&self, active_profile: &str, app: Option<&AppInfo>) -> bool {
        if !self.def.enabled {
            return false;
        }
        if self
            .def
            .profile
            .as_deref()
            .is_some_and(|p| p != active_profile)
        {
            return false;
        }
        if !self.apps.is_empty() {
            let Some(app) = app else { return false };
            if !self.apps.iter().any(|a| a.eq_ignore_ascii_case(&app.exe)) {
                return false;
            }
        }
        if let Some(re) = &self.title_regex {
            let Some(app) = app else { return false };
            if !re.is_match(&app.title) {
                return false;
            }
        }
        true
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub config_version: u32,
    pub settings: Settings,
    pub update: UpdateSettings,
    /// In priority order: earlier gestures win (SPEC §10.2).
    pub gestures: Vec<Gesture>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            config_version: CONFIG_VERSION,
            settings: Settings::default(),
            update: UpdateSettings::default(),
            gestures: Vec::new(),
        }
    }
}

/// A problem in the file, located as precisely as possible.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub line: Option<usize>,
    pub column: Option<usize>,
    /// Gesture name and field, e.g. `gesture "lock" → fingers`.
    pub field: Option<String>,
    pub message: String,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.line, self.column) {
            (Some(l), Some(c)) => write!(f, "line {l}, column {c}: ")?,
            (Some(l), None) => write!(f, "line {l}: ")?,
            _ => {}
        }
        if let Some(field) = &self.field {
            write!(f, "{field}: ")?;
        }
        f.write_str(&self.message)
    }
}

#[derive(Debug, Clone)]
pub struct Loaded {
    pub config: Config,
    pub warnings: Vec<Diagnostic>,
}

/// 1-based line and column of a byte offset.
fn position(text: &str, offset: usize) -> (usize, usize) {
    let before = &text[..offset.min(text.len())];
    let line = before.matches('\n').count() + 1;
    let column = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
    (line, column)
}

pub fn parse(text: &str) -> Result<Loaded, Vec<Diagnostic>> {
    let raw: RawConfig = toml::from_str(text).map_err(|e| {
        let (line, column) = match e.span() {
            Some(span) => {
                let (l, c) = position(text, span.start);
                (Some(l), Some(c))
            }
            None => (None, None),
        };
        vec![Diagnostic {
            line,
            column,
            field: None,
            message: e.message().trim().to_string(),
        }]
    })?;

    let mut errors: Vec<Diagnostic> = Vec::new();
    let mut warnings = Vec::new();
    fn err(errors: &mut Vec<Diagnostic>, line: Option<usize>, field: String, message: String) {
        errors.push(Diagnostic {
            line,
            column: None,
            field: Some(field),
            message,
        })
    }

    let config_version = raw.config_version.unwrap_or(CONFIG_VERSION);
    if config_version > CONFIG_VERSION {
        err(
            &mut errors,
            None,
            "config_version".into(),
            format!(
                "the file uses format {config_version}, written by a newer GlyphPad; this version reads format {CONFIG_VERSION}"
            ),
        );
    }

    let s = &raw.settings;
    for (name, ok) in [
        (
            "tap_max_duration_ms",
            (1..=5_000).contains(&s.tap_max_duration_ms),
        ),
        ("tap_max_movement_mm", s.tap_max_movement_mm > 0.0),
        (
            "multi_tap_gap_ms",
            (1..=5_000).contains(&s.multi_tap_gap_ms),
        ),
        (
            "hold_duration_ms",
            (1..=30_000).contains(&s.hold_duration_ms),
        ),
        ("hold_max_movement_mm", s.hold_max_movement_mm > 0.0),
        ("swipe_min_distance_mm", s.swipe_min_distance_mm > 0.0),
        ("swipe_min_speed_mm_s", s.swipe_min_speed_mm_s >= 0.0),
        ("stroke_gap_ms", (1..=5_000).contains(&s.stroke_gap_ms)),
        (
            "default_confidence",
            (0.0..=1.0).contains(&s.default_confidence),
        ),
    ] {
        if !ok {
            err(
                &mut errors,
                None,
                format!("settings → {name}"),
                "value out of range".into(),
            );
        }
    }
    if s.draw_activation.is_some() {
        warnings.push(Diagnostic {
            line: None,
            column: None,
            field: Some("settings → draw_activation".into()),
            message: "draw mode arrives with symbol recognition (M3); ignored for now".into(),
        });
    }

    let mut names = HashSet::new();
    let mut gestures = Vec::new();
    for spanned in raw.gestures {
        let line = position(text, spanned.span().start).0;
        let def = spanned.into_inner();
        let label = format!("gesture \"{}\"", def.name);
        let before = errors.len();
        let mut field_err = |field: &str, message: String| {
            err(
                &mut errors,
                Some(line),
                format!("{label} → {field}"),
                message,
            );
        };

        if def.name.trim().is_empty() {
            field_err("name", "must not be empty".into());
        } else if !names.insert(def.name.clone()) {
            field_err("name", "another gesture has the same name".into());
        }

        let needs_fingers = matches!(
            def.kind,
            GestureKind::Tap | GestureKind::Hold | GestureKind::Swipe
        );
        match (needs_fingers, def.fingers) {
            (true, None) => field_err(
                "fingers",
                format!("required for a {:?} gesture", def.kind).to_lowercase(),
            ),
            (true, Some(n)) if !(1..=crate::MAX_CONTACTS as u8).contains(&n) => field_err(
                "fingers",
                format!("must be between 1 and {}", crate::MAX_CONTACTS),
            ),
            (false, Some(_)) => field_err(
                "fingers",
                "only taps, holds and swipes take `fingers`".into(),
            ),
            _ => {}
        }
        match (def.kind, def.taps) {
            (GestureKind::Tap, Some(t)) if !(1..=3).contains(&t) => {
                field_err("taps", "must be 1, 2 or 3".into())
            }
            (GestureKind::Tap, _) | (_, None) => {}
            (_, Some(_)) => field_err("taps", "only taps take `taps`".into()),
        }
        match (def.kind, def.direction) {
            (GestureKind::Swipe, None) => field_err("direction", "required for a swipe".into()),
            (GestureKind::Swipe, _) | (_, None) => {}
            (_, Some(_)) => field_err("direction", "only swipes take `direction`".into()),
        }
        match (def.kind, &def.symbol) {
            (GestureKind::Symbol, None) => {
                field_err("symbol", "required for a symbol gesture".into())
            }
            (GestureKind::Symbol, Some(s))
                if s.to_vec().iter().any(|n| n.trim().is_empty()) || s.to_vec().is_empty() =>
            {
                field_err("symbol", "symbol names must not be empty".into())
            }
            (GestureKind::Symbol, _) | (_, None) => {}
            (_, Some(_)) => field_err("symbol", "only symbol gestures take `symbol`".into()),
        }
        match (def.kind, def.confidence) {
            (GestureKind::Symbol, Some(c)) if !(0.0..=1.0).contains(&c) => {
                field_err("confidence", "must be between 0 and 1".into())
            }
            (GestureKind::Symbol, _) | (_, None) => {}
            (_, Some(_)) => field_err(
                "confidence",
                "only symbol gestures take `confidence`".into(),
            ),
        }
        if let Err(e) = def.action.validate() {
            field_err("action", e);
        }

        let mut title_regex = None;
        let mut apps = Vec::new();
        if let Some(when) = &def.when {
            if let Some(t) = &when.title {
                match Regex::new(t) {
                    Ok(re) => title_regex = Some(re),
                    Err(e) => field_err("when → title", format!("invalid regular expression: {e}")),
                }
            }
            if let Some(a) = &when.app {
                apps = a.to_vec();
                if apps.iter().any(|a| a.trim().is_empty()) {
                    field_err("when → app", "application names must not be empty".into());
                }
            }
        }

        let mut warn = |field: &str, message: String| {
            warnings.push(Diagnostic {
                line: Some(line),
                column: None,
                field: Some(format!("{label} → {field}")),
                message,
            })
        };
        for w in def.action.warnings() {
            warn("action", w);
        }
        if def.kind == GestureKind::Symbol {
            warn(
                "type",
                "symbol recognition arrives in M3; this gesture is ignored for now".into(),
            );
        }
        if matches!(def.action, Action::RepeatLast | Action::ShowWindow) {
            warn(
                "action",
                format!(
                    "`{}` arrives with the GUI (M4); it does nothing for now",
                    def.action.kind()
                ),
            );
        }

        if errors.len() == before {
            gestures.push(Gesture {
                def,
                line,
                title_regex,
                apps,
            });
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(Loaded {
        config: Config {
            config_version,
            settings: raw.settings,
            update: raw.update,
            gestures,
        },
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn errors(text: &str) -> Vec<String> {
        parse(text)
            .unwrap_err()
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    #[test]
    fn example_config_is_valid() {
        let loaded = parse(EXAMPLE_CONFIG).unwrap_or_else(|e| panic!("{e:?}"));
        assert!(!loaded.config.gestures.is_empty());
    }

    #[test]
    fn empty_config_uses_defaults() {
        let loaded = parse("").unwrap();
        assert_eq!(loaded.config.settings, Settings::default());
        assert!(loaded.config.gestures.is_empty());
    }

    #[test]
    fn spec_example_parses() {
        let text = r#"
[settings]
draw_activation = { mode = "fingers", count = 3 }
stroke_gap_ms = 400
default_confidence = 0.80

[[gesture]]
name = "lock"
type = "symbol"
symbol = "L"
action = { type = "lock_screen" }

[[gesture]]
name = "close-app"
type = "symbol"
symbol = ["q", "X"]
confidence = 0.85
action = { type = "close_window", force = false }

[[gesture]]
name = "terminal"
type = "tap"
fingers = 4
action = { type = "run", command = "wt.exe" }

[[gesture]]
name = "next-tab-in-browser"
type = "swipe"
fingers = 3
direction = "right"
when = { app = ["chrome.exe", "firefox.exe"] }
action = { type = "keys", combo = "Ctrl+Tab" }
"#;
        let loaded = parse(text).unwrap();
        assert_eq!(loaded.config.gestures.len(), 4);
        assert_eq!(loaded.config.gestures[2].line, 20);
        // draw_activation and the two symbol gestures.
        assert_eq!(loaded.warnings.len(), 3);
    }

    #[test]
    fn syntax_errors_have_a_position() {
        let e = errors("[settings]\ntap_max_duration_ms = \"fast\"\n");
        assert!(e[0].starts_with("line 2, column"), "{e:?}");
        let e = errors("[settings]\ntap_max_durration_ms = 100\n");
        assert!(e[0].contains("tap_max_durration_ms"), "{e:?}");
    }

    #[test]
    fn semantic_errors_name_line_and_field() {
        let text = r#"
[[gesture]]
name = "a"
type = "tap"
fingers = 11
action = { type = "lock_screen" }

[[gesture]]
name = "a"
type = "swipe"
fingers = 3
taps = 2
action = { type = "keys" }
"#;
        let e = errors(text);
        assert_eq!(
            e,
            vec![
                "line 2: gesture \"a\" → fingers: must be between 1 and 10",
                "line 8: gesture \"a\" → name: another gesture has the same name",
                "line 8: gesture \"a\" → taps: only taps take `taps`",
                "line 8: gesture \"a\" → direction: required for a swipe",
                "line 8: gesture \"a\" → action: `keys` needs a `combo` or a `sequence`",
            ]
        );
    }

    #[test]
    fn newer_format_is_rejected() {
        assert!(errors("config_version = 99")[0].contains("newer GlyphPad"));
    }

    #[test]
    fn conditions() {
        let text = r#"
[[gesture]]
name = "browser"
type = "tap"
fingers = 3
profile = "work"
when = { app = "Chrome.exe", title = "GitHub" }
action = { type = "lock_screen" }
"#;
        let g = &parse(text).unwrap().config.gestures[0];
        let app = |exe: &str, title: &str| AppInfo {
            exe: exe.into(),
            title: title.into(),
            pid: 1,
        };
        assert!(g.applies(
            "work",
            Some(&app("chrome.exe", "wildzer0/glyphpad · GitHub"))
        ));
        assert!(!g.applies("default", Some(&app("chrome.exe", "GitHub"))));
        assert!(!g.applies("work", Some(&app("firefox.exe", "GitHub"))));
        assert!(!g.applies("work", Some(&app("chrome.exe", "Gmail"))));
        assert!(!g.applies("work", None));
        assert!(
            errors(&text.replace("GitHub", "("))
                .iter()
                .any(|e| e.contains("when → title"))
        );
    }
}
