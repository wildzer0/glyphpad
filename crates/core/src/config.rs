//! The TOML configuration: schema, defaults and validation with readable
//! errors (line, field, reason) (SPEC §9).

use std::collections::HashSet;
use std::fmt;

use regex::Regex;
use serde::{Deserialize, Serialize};
use toml::Spanned;

use crate::action::{Action, AppInfo};
pub use crate::trigger::{Trigger, TriggerDef, Turn, Zoom};

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
    /// Which touchpad to use: part of its name, as the system reports it
    /// ("amazonbasics", say). Empty uses every touchpad that reports, and a
    /// machine with an internal one as well as an external will see both.
    pub touchpad: String,

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
    /// Change of the fingers' mean distance from their centroid for a pinch.
    /// Two fingers 40 mm apart pinched to 20 mm change it by 10 mm.
    pub pinch_min_mm: f32,
    /// Turn of the fingers around their centroid for a rotation.
    pub rotate_min_deg: f32,
    /// Depth of the edge zone where an edge swipe starts, in percent of the
    /// touchpad's size: 6 % is about 9 mm on a 155 mm touchpad.
    pub edge_zone_percent: f32,

    // Symbols and draw mode (SPEC §5, §6.2).
    /// How a touch becomes a stroke of a drawn symbol.
    pub draw_activation: DrawActivation,
    /// Strokes closer together than this form one symbol (an X is two strokes).
    pub stroke_gap_ms: u64,
    /// Shortest stroke; anything shorter is a tap or fingers settling.
    pub min_stroke_mm: f32,
    /// Minimum score for a symbol when the gesture sets no `confidence`.
    pub default_confidence: f32,
    /// How much better the best symbol must score than every other symbol for
    /// a drawing to count. `0` fires the best score whatever the others are;
    /// the default keeps two shapes that look alike from firing one of them.
    pub symbol_margin: f32,
    /// 0 keeps symbols orientation-sensitive: a rotated L is not an L.
    pub rotation_tolerance_deg: f32,

    // repeat_last (SPEC §9.1).
    /// Seconds after which `repeat_last` does nothing; none by default.
    pub repeat_timeout_s: Option<u64>,
    /// Do not repeat into a different foreground window (the key keeps the
    /// name from the specification).
    pub repeat_same_app_only: bool,

    // Window and tray (SPEC §10.1).
    /// The window's close button hides it to the tray instead of quitting.
    pub close_to_tray: bool,
    /// Start in the tray without showing the window.
    pub start_minimized: bool,
    /// Light or dark appearance of the window.
    pub theme: ThemeChoice,
    /// Recognitions kept in the history.
    pub history_size: usize,
    /// Also keep the history in `history.jsonl` next to the configuration.
    pub save_history: bool,

    /// Feedback overlay (SPEC §13).
    pub overlay: OverlaySettings,

    /// Start GlyphPad at sign-in (SPEC §10.4).
    pub autostart: bool,
    /// Gestures pause while one of these applications is in front (SPEC §13).
    pub pause_in_apps: Vec<String>,
    /// Gestures pause while a full-screen application (game, presentation)
    /// is in front.
    pub pause_when_fullscreen: bool,
    /// Profiles defined in the editor, besides those named by gestures.
    pub profiles: Vec<String>,
    /// Earlier versions of the configuration kept in `backups/`.
    pub backup_count: usize,
    /// Switch profile by itself while one of these applications is in front
    /// (SPEC §13). The first matching rule wins; with none, `active_profile`.
    pub profile_rules: Vec<ProfileRule>,
    /// Count how often each gesture is used and how many drawings are not
    /// recognized, in `stats.json` next to the configuration (SPEC §13).
    /// Nothing leaves the machine.
    pub statistics: bool,
    /// Language of the window.
    pub language: Language,
    /// How long the list of gestures stays on screen after the
    /// `show_cheat_sheet` action.
    pub cheat_sheet_s: u64,
}

/// "While `app` is in front, use `profile`".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileRule {
    /// Executable names, e.g. `["code.exe"]`.
    pub app: OneOrMany,
    pub profile: String,
}

/// Language of the window (SPEC §13).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    /// The system's, English when there is no translation.
    #[default]
    System,
    English,
    Italian,
}

/// Appearance of the window (SPEC §13): follow the system or force one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    #[default]
    BottomRight,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OverlaySettings {
    pub enabled: bool,
    pub position: Corner,
    /// How long a recognized gesture's name stays on screen.
    pub duration_ms: u64,
    /// 0 (invisible) to 1 (opaque).
    pub opacity: f32,
    /// Width of the overlay panel in pixels; its height follows the touchpad's proportions.
    pub width_px: u32,
}

impl Default for OverlaySettings {
    fn default() -> Self {
        Self {
            enabled: true,
            position: Corner::BottomRight,
            duration_ms: 1500,
            opacity: 0.85,
            width_px: 240,
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            active_profile: "default".into(),
            touchpad: String::new(),
            tap_max_duration_ms: 250,
            tap_max_movement_mm: 4.0,
            multi_tap_gap_ms: 300,
            hold_duration_ms: 600,
            hold_max_movement_mm: 4.0,
            swipe_min_distance_mm: 18.0,
            swipe_min_speed_mm_s: 60.0,
            pinch_min_mm: 8.0,
            rotate_min_deg: 25.0,
            edge_zone_percent: 6.0,
            draw_activation: DrawActivation::default(),
            stroke_gap_ms: 400,
            min_stroke_mm: 10.0,
            default_confidence: 0.80,
            symbol_margin: 0.02,
            rotation_tolerance_deg: 0.0,
            repeat_timeout_s: None,
            repeat_same_app_only: true,
            close_to_tray: true,
            start_minimized: false,
            theme: ThemeChoice::System,
            history_size: 20,
            save_history: false,
            overlay: OverlaySettings::default(),
            autostart: false,
            pause_in_apps: Vec::new(),
            pause_when_fullscreen: true,
            profiles: Vec::new(),
            backup_count: 10,
            profile_rules: Vec::new(),
            statistics: true,
            language: Language::System,
            cheat_sheet_s: 8,
        }
    }
}

/// Draw-mode strategies (SPEC §5). Raw Input cannot stop Windows from
/// moving the cursor, so a symbol must be drawn in a way the system ignores
/// (several fingers) or that GlyphPad can tell apart from pointing.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum DrawActivation {
    /// Draw with `count` fingers; the stroke is their centroid. Windows does
    /// not move the cursor with several fingers, but its own 3- and 4-finger
    /// gestures must be turned off.
    Fingers {
        #[serde(default = "default_draw_fingers")]
        count: u8,
    },
    /// Draw with one finger starting inside a zone along an edge.
    Edge {
        #[serde(default)]
        edge: Edge,
        /// Depth of the zone, in percent of the touchpad's size.
        #[serde(default = "default_zone_percent")]
        zone_percent: f32,
        /// Experimental: pin the cursor while drawing (Windows `ClipCursor`).
        #[serde(default)]
        freeze_cursor: bool,
    },
    /// Rest one finger for `ms`, then draw.
    Hold {
        #[serde(default = "default_hold_to_draw_ms")]
        ms: u64,
        #[serde(default)]
        freeze_cursor: bool,
    },
    /// Draw with one finger only after a gesture with the `draw_symbol`
    /// action; the arming lasts `timeout_ms`.
    Toggle {
        #[serde(default = "default_toggle_timeout_ms")]
        timeout_ms: u64,
        #[serde(default)]
        freeze_cursor: bool,
    },
}

impl Default for DrawActivation {
    fn default() -> Self {
        DrawActivation::Fingers {
            count: default_draw_fingers(),
        }
    }
}

impl DrawActivation {
    pub fn freeze_cursor(&self) -> bool {
        match *self {
            DrawActivation::Fingers { .. } => false,
            DrawActivation::Edge { freeze_cursor, .. }
            | DrawActivation::Hold { freeze_cursor, .. }
            | DrawActivation::Toggle { freeze_cursor, .. } => freeze_cursor,
        }
    }

    /// How long a `draw_symbol` arming lasts.
    pub fn arm_timeout_ms(&self) -> u64 {
        match *self {
            DrawActivation::Toggle { timeout_ms, .. } => timeout_ms,
            _ => default_toggle_timeout_ms(),
        }
    }
}

fn default_draw_fingers() -> u8 {
    3
}
fn default_zone_percent() -> f32 {
    10.0
}
/// Long enough to tell apart from a pause while moving the pointer.
fn default_hold_to_draw_ms() -> u64 {
    400
}
fn default_toggle_timeout_ms() -> u64 {
    5_000
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Edge {
    #[default]
    Left,
    Right,
    Top,
    Bottom,
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
    /// A tap inside a rectangle of the touchpad.
    Zone,
    Hold,
    Swipe,
    EdgeSwipe,
    Pinch,
    Rotate,
    Click,
    Symbol,
    /// Gestures performed one after the other.
    Sequence,
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

impl When {
    /// Whether the window in front satisfies these conditions, evaluated when
    /// an action guarded by `only_if` runs. An invalid regular expression
    /// matches nothing: the configuration check rejects it first.
    pub fn matches(&self, app: Option<&crate::action::AppInfo>) -> bool {
        if let Some(list) = &self.app {
            let Some(app) = app else { return false };
            if !list
                .to_vec()
                .iter()
                .any(|a| a.eq_ignore_ascii_case(&app.exe))
            {
                return false;
            }
        }
        if let Some(pattern) = &self.title {
            let Some(app) = app else { return false };
            match regex::Regex::new(pattern) {
                Ok(re) if re.is_match(&app.title) => {}
                _ => return false,
            }
        }
        true
    }
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
    /// Resting fingers that make a tap or swipe a chord.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modifier_fingers: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<Direction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge: Option<Edge>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zoom: Option<Zoom>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<Turn>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<OneOrMany>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    /// Threshold overrides for this gesture alone (swipe distance and speed,
    /// pinch spread, rotation degrees); the global ones still apply when
    /// these are absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_distance_mm: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_speed_mm_s: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_change_mm: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_deg: Option<f32>,
    /// The physical button fires a `zone` gesture too.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub click: Option<bool>,
    /// Where a `zone` gesture has to land, in millimetres: one region, or
    /// several ("any corner").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<crate::trigger::Region>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub regions: Option<Vec<crate::trigger::Region>>,
    /// The gestures of a sequence, in order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steps: Option<Vec<TriggerDef>>,
    /// Longest pause between the steps of a sequence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub within_ms: Option<u64>,
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
    /// What sets it off; `None` for a sequence.
    pub trigger: Option<Trigger>,
    /// The steps of a sequence.
    pub steps: Vec<Trigger>,
    /// Pause allowed after each step, in milliseconds: the step's own
    /// `within_ms`, else the sequence's.
    pub step_within_ms: Vec<u64>,
    pub within_ms: u64,
    title_regex: Option<Regex>,
    apps: Vec<String>,
}

/// Default pause allowed between the steps of a sequence.
pub const DEFAULT_SEQUENCE_WITHIN_MS: u64 = 800;

impl Config {
    /// The zone gesture a touch of `fingers` fingers at `(x, y)` millimetres
    /// would set off. The overlay announces it while the fingers rest, before
    /// the tap is over; an application condition cannot be checked here, so a
    /// gesture that has one is left out.
    pub fn zone_at(&self, fingers: u8, x: f32, y: f32) -> Option<&Gesture> {
        self.gestures.iter().find(|g| {
            g.applies(&self.settings.active_profile, None)
                && matches!(
                    &g.trigger,
                    Some(crate::trigger::Trigger::Zone { fingers: f, regions, .. })
                        if *f == fingers && regions.iter().any(|r| r.contains(x, y))
                )
        })
    }
}

impl GestureDef {
    /// The trigger fields, as checked by [`TriggerDef::build`].
    pub fn trigger_def(&self) -> TriggerDef {
        TriggerDef {
            kind: Some(self.kind),
            fingers: self.fingers,
            taps: self.taps,
            direction: self.direction,
            edge: self.edge,
            zoom: self.zoom,
            rotation: self.rotation,
            modifier_fingers: self.modifier_fingers,
            symbol: self.symbol.clone(),
            confidence: self.confidence,
            min_distance_mm: self.min_distance_mm,
            min_speed_mm_s: self.min_speed_mm_s,
            min_change_mm: self.min_change_mm,
            min_deg: self.min_deg,
            within_ms: None,
            region: self.region,
            regions: self.regions.clone(),
            click: self.click,
        }
    }

    /// A gesture definition with only a name, a trigger and an action.
    pub fn new(name: impl Into<String>, trigger: TriggerDef, action: Action) -> Self {
        Self {
            name: name.into(),
            kind: trigger.kind.unwrap_or(GestureKind::Tap),
            fingers: trigger.fingers,
            taps: trigger.taps,
            modifier_fingers: trigger.modifier_fingers,
            direction: trigger.direction,
            edge: trigger.edge,
            zoom: trigger.zoom,
            rotation: trigger.rotation,
            symbol: trigger.symbol,
            confidence: trigger.confidence,
            min_distance_mm: trigger.min_distance_mm,
            min_speed_mm_s: trigger.min_speed_mm_s,
            min_change_mm: trigger.min_change_mm,
            min_deg: trigger.min_deg,
            region: trigger.region,
            regions: trigger.regions,
            click: trigger.click,
            steps: None,
            within_ms: None,
            action,
            enabled: true,
            profile: None,
            when: None,
            repeatable: None,
        }
    }
}

impl Gesture {
    /// Fingers of the trigger (0 for symbols and sequences).
    pub fn fingers(&self) -> u8 {
        self.trigger.as_ref().map_or(0, Trigger::fingers)
    }

    pub fn taps(&self) -> u8 {
        match self.trigger {
            Some(Trigger::Tap { taps, .. }) => taps,
            _ => 1,
        }
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

impl Config {
    /// Profile names known to the configuration: declared, used by a
    /// gesture or by a rule, and the active one.
    pub fn profile_names(&self) -> Vec<String> {
        let s = &self.settings;
        let mut out = s.profiles.clone();
        let used = self
            .gestures
            .iter()
            .filter_map(|g| g.def.profile.clone())
            .chain(s.profile_rules.iter().map(|r| r.profile.clone()))
            .chain(std::iter::once(s.active_profile.clone()));
        for p in used {
            if !out.contains(&p) {
                out.push(p);
            }
        }
        out
    }

    /// The profile a rule asks for while `app` is in front.
    pub fn profile_for(&self, app: &AppInfo) -> Option<&str> {
        self.settings
            .profile_rules
            .iter()
            .find(|r| {
                r.app
                    .to_vec()
                    .iter()
                    .any(|a| a.eq_ignore_ascii_case(&app.exe))
            })
            .map(|r| r.profile.as_str())
    }
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

/// Line of a `key = …` entry of `[settings]`, for diagnostics: settings are
/// deserialized as one table, which loses the position of each value.
fn settings_line(text: &str, key: &str) -> Option<usize> {
    let mut inside = false;
    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('[') {
            inside = trimmed.starts_with("[settings]") || trimmed.starts_with("[settings.");
            continue;
        }
        if inside && trimmed.starts_with(key) && trimmed[key.len()..].trim_start().starts_with('=')
        {
            return Some(i + 1);
        }
    }
    None
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
        ("touchpad", s.touchpad.chars().count() <= 120),
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
        ("pinch_min_mm", s.pinch_min_mm > 0.0),
        ("rotate_min_deg", (1.0..=180.0).contains(&s.rotate_min_deg)),
        (
            "edge_zone_percent",
            (1.0..=30.0).contains(&s.edge_zone_percent),
        ),
        ("history_size", (1..=1_000).contains(&s.history_size)),
        ("cheat_sheet_s", (1..=120).contains(&s.cheat_sheet_s)),
        ("backup_count", s.backup_count <= 100),
        (
            "overlay → duration_ms",
            (100..=30_000).contains(&s.overlay.duration_ms),
        ),
        (
            "overlay → opacity",
            (0.05..=1.0).contains(&s.overlay.opacity),
        ),
        (
            "overlay → width_px",
            (80..=2_000).contains(&s.overlay.width_px),
        ),
        ("min_stroke_mm", s.min_stroke_mm > 0.0),
        (
            "rotation_tolerance_deg",
            (0.0..=180.0).contains(&s.rotation_tolerance_deg),
        ),
        (
            "default_confidence",
            (0.0..=1.0).contains(&s.default_confidence),
        ),
    ] {
        if !ok {
            let line = settings_line(text, name.split(' ').next().unwrap_or(name));
            err(
                &mut errors,
                line,
                format!("settings → {name}"),
                "value out of range".into(),
            );
        }
    }
    for (i, rule) in s.profile_rules.iter().enumerate() {
        if rule.profile.trim().is_empty() || rule.app.to_vec().iter().all(|a| a.trim().is_empty()) {
            err(
                &mut errors,
                settings_line(text, "profile_rules"),
                format!("settings → profile_rules → item {}", i + 1),
                "needs an `app` and a `profile`".into(),
            );
        }
    }
    let draw_line = settings_line(text, "draw_activation");
    match s.draw_activation {
        DrawActivation::Fingers { count } if !(1..=crate::MAX_CONTACTS as u8).contains(&count) => err(
            &mut errors,
            draw_line,
            "settings → draw_activation → count".into(),
            format!("must be between 1 and {}", crate::MAX_CONTACTS),
        ),
        DrawActivation::Fingers { count: 1 } => warnings.push(Diagnostic {
            line: None,
            column: None,
            field: Some("settings → draw_activation → count".into()),
            message: "with one finger every pointer movement is a stroke; use `edge`, `hold` or `toggle` instead".into(),
        }),
        DrawActivation::Edge { zone_percent, .. } if !(1.0..=50.0).contains(&zone_percent) => err(
            &mut errors,
            draw_line,
            "settings → draw_activation → zone_percent".into(),
            "must be between 1 and 50".into(),
        ),
        DrawActivation::Hold { ms, .. } if !(100..=5_000).contains(&ms) => err(
            &mut errors,
            draw_line,
            "settings → draw_activation → ms".into(),
            "must be between 100 and 5000".into(),
        ),
        DrawActivation::Toggle { timeout_ms, .. } if !(500..=60_000).contains(&timeout_ms) => err(
            &mut errors,
            draw_line,
            "settings → draw_activation → timeout_ms".into(),
            "must be between 500 and 60000".into(),
        ),
        _ => {}
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

        let mut trigger = None;
        let mut steps = Vec::new();
        let mut step_within_ms = Vec::new();
        let within_ms = def.within_ms.unwrap_or(DEFAULT_SEQUENCE_WITHIN_MS);
        if def.kind == GestureKind::Sequence {
            let mut stray = def.trigger_def();
            stray.kind = None;
            if stray != TriggerDef::default() {
                field_err(
                    "type",
                    "a sequence only takes `steps` and `within_ms`".into(),
                );
            }
            match &def.steps {
                None => field_err("steps", "required for a sequence".into()),
                Some(list) if !(2..=4).contains(&list.len()) => {
                    field_err("steps", "a sequence has 2 to 4 steps".into())
                }
                Some(list) => {
                    for (i, step) in list.iter().enumerate() {
                        // The pause after a step is what its own `within_ms`
                        // says, or the sequence's for all of them.
                        step_within_ms.push(step.within_ms.unwrap_or(within_ms));
                        match step.build() {
                            Ok(t) => steps.push(t),
                            Err(es) => {
                                for (field, message) in es {
                                    field_err(&format!("steps[{}] → {field}", i + 1), message);
                                }
                            }
                        }
                    }
                }
            }
            if !(100..=5_000).contains(&within_ms) {
                field_err("within_ms", "must be between 100 and 5000".into());
            }
        } else {
            if def.steps.is_some() {
                field_err("steps", "only sequences take `steps`".into());
            }
            if def.within_ms.is_some() {
                field_err("within_ms", "only sequences take `within_ms`".into());
            }
            match def.trigger_def().build() {
                Ok(t) => trigger = Some(t),
                Err(es) => {
                    for (field, message) in es {
                        field_err(field, message);
                    }
                }
            }
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

        if errors.len() == before {
            gestures.push(Gesture {
                def,
                line,
                trigger,
                steps,
                step_within_ms,
                within_ms,
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
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        assert_eq!(
            loaded.config.settings.draw_activation,
            DrawActivation::Fingers { count: 3 }
        );
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
                "line 8: gesture \"a\" → direction: required for a swipe gesture",
                "line 8: gesture \"a\" → taps: a swipe gesture does not take `taps`",
                "line 8: gesture \"a\" → action: `keys` needs a `combo` or a `sequence`",
            ]
        );
    }

    #[test]
    fn draw_activation_modes() {
        let mode = |t: &str| {
            parse(&format!("[settings]\ndraw_activation = {t}"))
                .map(|l| l.config.settings.draw_activation)
        };
        assert_eq!(
            mode(r#"{ mode = "edge", edge = "right", freeze_cursor = true }"#).unwrap(),
            DrawActivation::Edge {
                edge: Edge::Right,
                zone_percent: 10.0,
                freeze_cursor: true
            }
        );
        assert_eq!(
            mode(r#"{ mode = "hold" }"#).unwrap(),
            DrawActivation::Hold {
                ms: 400,
                freeze_cursor: false
            }
        );
        assert!(mode(r#"{ mode = "toggle", timeout_ms = 10 }"#).is_err());
        assert!(mode(r#"{ mode = "fingers", count = 3, freeze_cursor = true }"#).is_err());
        assert!(mode(r#"{ mode = "telepathy" }"#).is_err());
    }

    #[test]
    fn new_gesture_kinds() {
        let text = r#"
[[gesture]]
name = "zoom"
type = "pinch"
fingers = 2
zoom = "out"
action = { type = "keys", combo = "Ctrl+Plus" }

[[gesture]]
name = "turn"
type = "rotate"
fingers = 2
rotation = "clockwise"
action = { type = "lock_screen" }

[[gesture]]
name = "edge"
type = "edge_swipe"
fingers = 1
edge = "right"
direction = "left"
action = { type = "lock_screen" }

[[gesture]]
name = "chord"
type = "swipe"
fingers = 2
modifier_fingers = 1
direction = "up"
action = { type = "lock_screen" }

[[gesture]]
name = "drag"
type = "click"
fingers = 2
direction = "down"
action = { type = "lock_screen" }

[[gesture]]
name = "combo"
type = "sequence"
steps = [{ type = "tap", fingers = 3 }, { type = "swipe", fingers = 3, direction = "up" }]
within_ms = 600
action = { type = "lock_screen" }
"#;
        let g = parse(text).unwrap().config.gestures;
        assert_eq!(g.len(), 6);
        assert_eq!(
            g[0].trigger,
            Some(Trigger::Pinch {
                fingers: 2,
                zoom: Zoom::Out,
                min_change_mm: None
            })
        );
        assert_eq!(
            g[3].trigger,
            Some(Trigger::Swipe {
                fingers: 2,
                direction: Direction::Up,
                modifiers: 1,
                min_distance_mm: None,
                min_speed_mm_s: None
            })
        );
        assert!(g[5].trigger.is_none());
        assert_eq!(g[5].steps.len(), 2);
        assert_eq!(g[5].within_ms, 600);
        assert_eq!(
            g[2].trigger.as_ref().unwrap().describe(),
            "swipe left from the right edge with 1 finger"
        );
    }

    #[test]
    fn new_gesture_kinds_are_validated() {
        let e = errors(
            r#"
[[gesture]]
name = "a"
type = "pinch"
fingers = 1
action = { type = "lock_screen" }

[[gesture]]
name = "b"
type = "sequence"
fingers = 2
steps = [{ type = "tap" }]
action = { type = "lock_screen" }
"#,
        );
        assert!(
            e.iter()
                .any(|e| e.contains("\"a\" → fingers: must be between 2 and 10")),
            "{e:?}"
        );
        assert!(
            e.iter().any(|e| e.contains("\"a\" → zoom: required")),
            "{e:?}"
        );
        assert!(
            e.iter()
                .any(|e| e.contains("\"b\" → type: a sequence only takes")),
            "{e:?}"
        );
        assert!(
            e.iter()
                .any(|e| e.contains("\"b\" → steps: a sequence has 2 to 4 steps")),
            "{e:?}"
        );
    }

    #[test]
    fn settings_errors_name_their_line() {
        let e = errors("[settings]\ntap_max_duration_ms = 0\nstart_minimized = false\n");
        assert_eq!(
            e,
            ["line 2: settings → tap_max_duration_ms: value out of range"]
        );
        let e = errors("[settings]\ndraw_activation = { mode = \"fingers\", count = 99 }\n");
        assert!(
            e[0].starts_with("line 2: settings → draw_activation"),
            "{e:?}"
        );
        let e = errors("[settings]\nprofile_rules = [{ app = \"\", profile = \"x\" }]\n");
        assert!(
            e[0].starts_with("line 2: settings → profile_rules"),
            "{e:?}"
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
            window: 0,
            fullscreen: false,
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

    #[test]
    fn profile_rules_pick_the_profile_of_the_foreground_app() {
        let text = r#"
[settings]
active_profile = "default"
profile_rules = [
  { app = ["code.exe", "devenv.exe"], profile = "dev" },
  { app = "vlc.exe", profile = "media" },
]

[[gesture]]
name = "a"
type = "tap"
fingers = 1
profile = "dev"
action = { type = "lock_screen" }
"#;
        let config = parse(text).unwrap().config;
        let app = |exe: &str| AppInfo {
            exe: exe.into(),
            ..Default::default()
        };
        assert_eq!(config.profile_for(&app("Code.exe")), Some("dev"));
        assert_eq!(config.profile_for(&app("vlc.exe")), Some("media"));
        assert_eq!(config.profile_for(&app("notepad.exe")), None);
        let names = config.profile_names();
        assert!(names.contains(&"dev".to_string()) && names.contains(&"media".to_string()));
        assert!(names.contains(&"default".to_string()));
        // A rule with nothing in it is an error.
        assert!(parse("[settings]\nprofile_rules = [{ app = \"\", profile = \"x\" }]\n").is_err());
    }

    #[test]
    fn any_of_a_gesture_zones_matches() {
        let c = parse(
            r#"
[[gesture]]
name = "corners"
type = "zone"
fingers = 1
regions = [
    { x = [0.0, 24.0], y = [0.0, 18.0] },
    { x = [130.0, 155.0], y = [95.0, 115.0] },
]
action = { type = "lock_screen" }
"#,
        )
        .unwrap()
        .config;
        assert!(c.zone_at(1, 5.0, 5.0).is_some(), "the top-left one");
        assert!(c.zone_at(1, 140.0, 100.0).is_some(), "the bottom-right one");
        assert!(
            c.zone_at(1, 70.0, 50.0).is_none(),
            "in the middle of neither"
        );
        // One zone or several, not both.
        assert!(
            parse(
                r#"
[[gesture]]
name = "both"
type = "zone"
fingers = 1
region = { x = [0.0, 24.0], y = [0.0, 18.0] }
regions = [{ x = [30.0, 50.0], y = [0.0, 18.0] }]
action = { type = "lock_screen" }
"#
            )
            .is_err()
        );
    }

    #[test]
    fn the_zone_under_a_touch_is_found() {
        let c = parse(
            r#"
[[gesture]]
name = "corner"
type = "zone"
fingers = 1
region = { x = [0.0, 25.0], y = [0.0, 20.0] }
action = { type = "lock_screen" }

[[gesture]]
name = "two"
type = "zone"
fingers = 2
region = { x = [0.0, 25.0], y = [0.0, 20.0] }
action = { type = "lock_screen" }
"#,
        )
        .unwrap()
        .config;
        assert_eq!(
            c.zone_at(1, 10.0, 8.0).map(|g| g.def.name.as_str()),
            Some("corner")
        );
        // Outside the rectangle, or the wrong finger count: nothing.
        assert!(c.zone_at(1, 40.0, 8.0).is_none());
        assert!(c.zone_at(2, 10.0, 8.0).map(|g| g.def.name.as_str()) == Some("two"));
        assert!(c.zone_at(3, 10.0, 8.0).is_none());
    }
}
