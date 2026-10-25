//! Configurable actions and the platform traits that carry them out (SPEC §3.2, §9).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::keys::KeyCombo;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    /// Presses a shortcut (`combo`) or several in a row (`sequence`).
    Keys {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        combo: Option<KeyCombo>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        sequence: Vec<KeyCombo>,
    },
    LockScreen,
    /// Asks the foreground window to close; `force` terminates its process.
    CloseWindow {
        #[serde(default)]
        force: bool,
    },
    Minimize,
    /// Maximizes the foreground window, or restores it if already maximized.
    Maximize,
    Run {
        command: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        args: Vec<String>,
        /// Start without a console window.
        #[serde(default)]
        hidden: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
        /// Run the command through the system shell (`cmd /C` on Windows),
        /// so pipes, redirection, `&&` and variables work.
        #[serde(default)]
        shell: bool,
        /// Extra environment variables for the program.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        env: BTreeMap<String, String>,
        /// Wait for it to finish, up to `timeout_ms`, instead of leaving it
        /// running on its own.
        #[serde(default)]
        wait: bool,
        /// How long `wait` waits before giving up.
        #[serde(default = "run_timeout_default")]
        timeout_ms: u64,
    },
    Media {
        key: MediaKey,
    },
    /// Runs actions in order; use `wait` items for delays.
    Chain {
        actions: Vec<Action>,
    },
    /// Pause inside a `chain`.
    Wait {
        ms: u64,
    },
    Notify {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        message: String,
    },
    /// Pauses or resumes every gesture.
    ToggleEnabled,
    /// Re-runs the action of the last recognized gesture (SPEC §9.1, M4).
    RepeatLast,
    /// Opens or focuses the GlyphPad window (M4).
    ShowWindow,
    /// Lets the next symbol be drawn with one finger (draw mode `toggle`).
    DrawSymbol,
    /// Opens a file, a folder or a web address with the default program.
    Open {
        target: String,
    },
    /// Types a text, character by character, into the focused window.
    TypeText {
        text: String,
    },
    /// Arranges the foreground window.
    Window {
        op: WindowOp,
    },
    VirtualDesktop {
        op: DesktopOp,
    },
    Screenshot {
        #[serde(default)]
        mode: ScreenshotMode,
    },
    /// Makes a profile the active one (saved in the configuration).
    SwitchProfile {
        profile: String,
    },
    /// Moves to the next profile of the list, wrapping around.
    NextProfile,
    /// Shows the list of the active gestures on screen for a moment.
    ShowCheatSheet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowOp {
    /// The left half of the screen the window is on.
    SnapLeft,
    SnapRight,
    /// Moves the window to the next monitor, wrapping around.
    NextMonitor,
    /// Toggles "always on top".
    ToggleAlwaysOnTop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopOp {
    Previous,
    Next,
    New,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScreenshotMode {
    /// The system's snipping overlay: drag to choose an area.
    #[default]
    Region,
    /// The whole screen, saved to the Screenshots folder.
    Screen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKey {
    PlayPause,
    Next,
    Previous,
    Stop,
    VolumeUp,
    VolumeDown,
    Mute,
}

/// How long `run` waits by default: long enough for a script, short enough
/// not to hold the queue of actions forever.
fn run_timeout_default() -> u64 {
    30_000
}

impl Action {
    pub fn kind(&self) -> &'static str {
        match self {
            Action::Keys { .. } => "keys",
            Action::LockScreen => "lock_screen",
            Action::CloseWindow { .. } => "close_window",
            Action::Minimize => "minimize",
            Action::Maximize => "maximize",
            Action::Run { .. } => "run",
            Action::Media { .. } => "media",
            Action::Chain { .. } => "chain",
            Action::Wait { .. } => "wait",
            Action::Notify { .. } => "notify",
            Action::ToggleEnabled => "toggle_enabled",
            Action::RepeatLast => "repeat_last",
            Action::ShowWindow => "show_window",
            Action::DrawSymbol => "draw_symbol",
            Action::Open { .. } => "open",
            Action::TypeText { .. } => "type_text",
            Action::Window { .. } => "window",
            Action::VirtualDesktop { .. } => "virtual_desktop",
            Action::Screenshot { .. } => "screenshot",
            Action::SwitchProfile { .. } => "switch_profile",
            Action::NextProfile => "next_profile",
            Action::ShowCheatSheet => "show_cheat_sheet",
        }
    }

    /// A short name for the kind of action, for lists.
    pub fn short_label(kind: &str) -> &'static str {
        match kind {
            "keys" => "Shortcut",
            "lock_screen" => "Lock",
            "close_window" => "Close window",
            "minimize" => "Minimize",
            "maximize" => "Maximize",
            "run" => "Run program",
            "media" => "Media key",
            "chain" => "Several actions",
            "notify" => "Message",
            "toggle_enabled" => "Pause / resume",
            "repeat_last" => "Repeat last",
            "show_window" => "Show window",
            "draw_symbol" => "Draw symbol",
            "open" => "Open",
            "type_text" => "Type text",
            "window" => "Arrange window",
            "virtual_desktop" => "Virtual desktop",
            "screenshot" => "Screenshot",
            "switch_profile" => "Switch profile",
            "next_profile" => "Next profile",
            "show_cheat_sheet" => "Cheat sheet",
            _ => "Pause",
        }
    }

    /// Checks constraints serde cannot express. Returns the problem, if any.
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Action::Keys { combo, sequence } => match (combo, sequence.is_empty()) {
                (Some(_), false) => {
                    Err("`keys` takes either `combo` or `sequence`, not both".into())
                }
                (None, true) => Err("`keys` needs a `combo` or a `sequence`".into()),
                _ => Ok(()),
            },
            Action::Run {
                command,
                timeout_ms,
                ..
            } => {
                if command.trim().is_empty() {
                    return Err("`run` needs a non-empty `command`".into());
                }
                if !(100..=600_000).contains(timeout_ms) {
                    return Err("`run.timeout_ms` must be between 100 and 600000".into());
                }
                Ok(())
            }
            Action::Chain { actions } => {
                if actions.is_empty() {
                    return Err("`chain` needs at least one action".into());
                }
                for (i, a) in actions.iter().enumerate() {
                    if matches!(
                        a,
                        Action::Chain { .. }
                            | Action::RepeatLast
                            | Action::DrawSymbol
                            | Action::SwitchProfile { .. }
                            | Action::NextProfile
                            | Action::ShowCheatSheet
                    ) {
                        return Err(format!("`chain` item {} cannot be `{}`", i + 1, a.kind()));
                    }
                    a.validate()
                        .map_err(|e| format!("`chain` item {}: {e}", i + 1))?;
                }
                Ok(())
            }
            Action::Wait { ms } if *ms > 60_000 => Err("`wait` is limited to 60000 ms".into()),
            Action::Open { target } if target.trim().is_empty() => {
                Err("`open` needs a file, folder or address in `target`".into())
            }
            Action::TypeText { text } if text.is_empty() => {
                Err("`type_text` needs a non-empty `text`".into())
            }
            Action::TypeText { text } if text.chars().count() > 2000 => {
                Err("`type_text` is limited to 2000 characters".into())
            }
            Action::SwitchProfile { profile } if profile.trim().is_empty() => {
                Err("`switch_profile` needs a `profile`".into())
            }
            _ => Ok(()),
        }
    }

    /// Non-fatal remarks about an action that is valid but probably not what
    /// the user wants.
    pub fn warnings(&self) -> Vec<String> {
        let mut out = Vec::new();
        let combos: Vec<&KeyCombo> = match self {
            Action::Keys { combo, sequence } => combo.iter().chain(sequence).collect(),
            Action::Chain { actions } => {
                return actions.iter().flat_map(Action::warnings).collect();
            }
            _ => Vec::new(),
        };
        for c in combos {
            let s = c.to_string().to_ascii_lowercase();
            if s == "win+l" {
                out.push(
                    "Win+L cannot be simulated by software on Windows; use `lock_screen` instead"
                        .into(),
                );
            } else if s == "ctrl+alt+delete" {
                out.push("Ctrl+Alt+Delete cannot be simulated by software".into());
            }
        }
        out
    }
}

pub type ActionError = Box<dyn std::error::Error + Send + Sync + 'static>;

/// Carries out the platform actions. `chain`, `wait`, `toggle_enabled`,
/// `repeat_last`, `show_window` and `draw_symbol` are handled by the
/// application, never passed here.
pub trait ActionExecutor {
    fn execute(&self, action: &Action) -> Result<(), ActionError>;
}

/// The application in the foreground, for per-app conditions and for
/// actions that target the foreground window.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AppInfo {
    /// Executable file name, e.g. `chrome.exe`.
    pub exe: String,
    pub title: String,
    pub pid: u32,
    /// Identifier of the top-level window (0 if unknown).
    pub window: u64,
    /// The window covers its whole monitor (games, presentations, videos).
    pub fullscreen: bool,
}

pub trait ForegroundInfo {
    fn foreground_app(&self) -> Option<AppInfo>;

    /// Applications with a visible window, for picking per-app conditions.
    fn windows(&self) -> Vec<AppInfo> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Result<Action, toml::de::Error> {
        #[derive(Deserialize)]
        struct W {
            action: Action,
        }
        toml::from_str::<W>(s).map(|w| w.action)
    }

    #[test]
    fn run_options() {
        let run = parse(
            r#"action = { type = "run", command = "git pull && npm test", shell = true,
                wait = true, timeout_ms = 120000, cwd = "C:/dev",
                env = { NIGHT = "1" }, args = ["-NoProfile"] }"#,
        )
        .unwrap();
        assert_eq!(
            run,
            Action::Run {
                command: "git pull && npm test".into(),
                args: vec!["-NoProfile".into()],
                hidden: false,
                cwd: Some("C:/dev".into()),
                shell: true,
                env: [("NIGHT".to_string(), "1".to_string())].into(),
                wait: true,
                timeout_ms: 120_000,
            }
        );
        assert!(run.validate().is_ok());
        // A `wait` with no timeout is not written out, and the default is 30 s.
        let plain = parse(r#"action = { type = "run", command = "notepad" }"#).unwrap();
        assert_eq!(
            plain,
            Action::Run {
                command: "notepad".into(),
                args: Vec::new(),
                hidden: false,
                cwd: None,
                shell: false,
                env: Default::default(),
                wait: false,
                timeout_ms: 30_000,
            }
        );
        // The timeout is only usable within range, `wait` or not.
        for bad in [0u64, 99, 600_001] {
            let a = parse(&format!(
                r#"action = {{ type = "run", command = "x", timeout_ms = {bad} }}"#
            ))
            .unwrap();
            assert!(a.validate().is_err(), "{bad}");
        }
    }

    #[test]
    fn parses_actions() {
        assert_eq!(
            parse(r#"action = { type = "lock_screen" }"#).unwrap(),
            Action::LockScreen
        );
        assert_eq!(
            parse(r#"action = { type = "close_window", force = true }"#).unwrap(),
            Action::CloseWindow { force: true }
        );
        let keys = parse(r#"action = { type = "keys", combo = "Ctrl+Tab" }"#).unwrap();
        assert!(keys.validate().is_ok());
        let chain = parse(
            r#"action = { type = "chain", actions = [
                { type = "keys", combo = "Ctrl+C" }, { type = "wait", ms = 100 },
                { type = "media", key = "play_pause" } ] }"#,
        )
        .unwrap();
        assert!(chain.validate().is_ok());
        assert_eq!(chain.kind(), "chain");
    }

    #[test]
    fn rejects_bad_actions() {
        assert!(parse(r#"action = { type = "explode" }"#).is_err());
        assert!(parse(r#"action = { type = "close_window", forse = true }"#).is_err());
        assert!(parse(r#"action = { type = "keys", combo = "Ctrl+Nope" }"#).is_err());
        let both = parse(r#"action = { type = "keys", combo = "A", sequence = ["B"] }"#).unwrap();
        assert!(both.validate().is_err());
        let neither = parse(r#"action = { type = "keys" }"#).unwrap();
        assert!(neither.validate().is_err());
        let nested =
            parse(r#"action = { type = "chain", actions = [{ type = "repeat_last" }] }"#).unwrap();
        assert!(nested.validate().is_err());
    }

    #[test]
    fn warns_about_win_l() {
        let a = parse(r#"action = { type = "keys", combo = "Win+L" }"#).unwrap();
        assert_eq!(a.warnings().len(), 1);
    }

    #[test]
    fn parses_and_validates_the_extra_actions() {
        for (toml, kind) in [
            (
                r#"action = { type = "open", target = "https://example.com" }"#,
                "open",
            ),
            (
                r#"action = { type = "type_text", text = "hello" }"#,
                "type_text",
            ),
            (
                r#"action = { type = "window", op = "snap_left" }"#,
                "window",
            ),
            (
                r#"action = { type = "window", op = "next_monitor" }"#,
                "window",
            ),
            (
                r#"action = { type = "virtual_desktop", op = "next" }"#,
                "virtual_desktop",
            ),
            (r#"action = { type = "screenshot" }"#, "screenshot"),
            (
                r#"action = { type = "screenshot", mode = "screen" }"#,
                "screenshot",
            ),
            (
                r#"action = { type = "switch_profile", profile = "work" }"#,
                "switch_profile",
            ),
            (r#"action = { type = "next_profile" }"#, "next_profile"),
            (
                r#"action = { type = "show_cheat_sheet" }"#,
                "show_cheat_sheet",
            ),
        ] {
            let a = parse(toml).unwrap_or_else(|e| panic!("{toml}: {e}"));
            assert_eq!(a.kind(), kind);
            assert!(a.validate().is_ok(), "{toml}");
        }
        assert!(
            parse(r#"action = { type = "open", target = " " }"#)
                .unwrap()
                .validate()
                .is_err()
        );
        assert!(
            parse(r#"action = { type = "type_text", text = "" }"#)
                .unwrap()
                .validate()
                .is_err()
        );
        let long = "x".repeat(2001);
        assert!(
            parse(&format!(
                r#"action = {{ type = "type_text", text = "{long}" }}"#
            ))
            .unwrap()
            .validate()
            .is_err()
        );
        // Actions that need the application cannot hide in a chain.
        let chain =
            parse(r#"action = { type = "chain", actions = [{ type = "next_profile" }] }"#).unwrap();
        assert!(chain.validate().is_err());
    }
}
