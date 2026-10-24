//! Configurable actions and the platform traits that carry them out (SPEC §3.2, §9).

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
            Action::Run { command, .. } if command.trim().is_empty() => {
                Err("`run` needs a non-empty `command`".into())
            }
            Action::Chain { actions } => {
                if actions.is_empty() {
                    return Err("`chain` needs at least one action".into());
                }
                for (i, a) in actions.iter().enumerate() {
                    if matches!(
                        a,
                        Action::Chain { .. } | Action::RepeatLast | Action::DrawSymbol
                    ) {
                        return Err(format!("`chain` item {} cannot be `{}`", i + 1, a.kind()));
                    }
                    a.validate()
                        .map_err(|e| format!("`chain` item {}: {e}", i + 1))?;
                }
                Ok(())
            }
            Action::Wait { ms } if *ms > 60_000 => Err("`wait` is limited to 60000 ms".into()),
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
}
