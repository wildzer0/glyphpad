//! Configurable actions and the platform traits that carry them out (SPEC §3.2, §9).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::config::When;
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
    /// Brings the first window of `app` to the front, or starts `command`
    /// when the program is not running.
    FocusOrLaunch {
        /// Executable to look for, e.g. `code.exe`.
        app: String,
        command: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        args: Vec<String>,
    },
    Media {
        key: MediaKey,
    },
    /// Turns the mouse wheel: positive scrolls down, one step per notch.
    Scroll {
        amount: i32,
        /// Spread the notches over this long instead of turning them all at
        /// once, which reads as a smooth scroll.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        smooth_ms: Option<u64>,
    },
    /// Turns the wheel while the fingers move, after a hold: the gesture
    /// fires once and then every `mm_per_notch` of vertical travel scrolls.
    /// Handled by the application, since it lasts as long as the touch.
    ScrollHold {
        #[serde(default = "scroll_hold_default")]
        mm_per_notch: f32,
    },
    /// Runs another action again, `times` times.
    Repeat {
        #[serde(default = "repeat_default")]
        times: u8,
        action: Box<Action>,
    },
    /// Runs the action only when the window in front matches `when`: a
    /// condition on one step of a chain, where the gesture's own condition
    /// covers the whole of it.
    OnlyIf {
        when: When,
        action: Box<Action>,
    },
    /// Clicks a mouse button where the pointer is.
    MouseClick {
        #[serde(default)]
        button: MouseButton,
        /// Two clicks in quick succession (opens, renames a file, selects a word).
        #[serde(default)]
        double: bool,
    },
    /// Presses the left button, moves the pointer by `dx`/`dy` pixels and
    /// releases it: a selection, a drag and drop, a slider.
    Drag {
        dx: i32,
        dy: i32,
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
    /// Puts a text on the clipboard.
    Copy {
        text: String,
    },
    /// Puts a text on the clipboard and pastes it into the focused window
    /// (`Ctrl+V`), which is faster and more faithful than typing it.
    PasteText {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MouseButton {
    #[default]
    Left,
    Right,
    Middle,
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

/// Five millimetres per wheel notch, the feel of a phone: about a line of
/// text.
fn scroll_hold_default() -> f32 {
    5.0
}

/// `repeat` runs its action five times unless told otherwise.
fn repeat_default() -> u8 {
    5
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
            Action::FocusOrLaunch { .. } => "focus_or_launch",
            Action::Media { .. } => "media",
            Action::Scroll { .. } => "scroll",
            Action::Repeat { .. } => "repeat",
            Action::ScrollHold { .. } => "scroll_hold",
            Action::OnlyIf { .. } => "only_if",
            Action::MouseClick { .. } => "mouse_click",
            Action::Drag { .. } => "drag",
            Action::Chain { .. } => "chain",
            Action::Wait { .. } => "wait",
            Action::Notify { .. } => "notify",
            Action::ToggleEnabled => "toggle_enabled",
            Action::RepeatLast => "repeat_last",
            Action::ShowWindow => "show_window",
            Action::DrawSymbol => "draw_symbol",
            Action::Open { .. } => "open",
            Action::TypeText { .. } => "type_text",
            Action::Copy { .. } => "copy",
            Action::PasteText { .. } => "paste_text",
            Action::Window { .. } => "window",
            Action::VirtualDesktop { .. } => "virtual_desktop",
            Action::Screenshot { .. } => "screenshot",
            Action::SwitchProfile { .. } => "switch_profile",
            Action::NextProfile => "next_profile",
            Action::ShowCheatSheet => "show_cheat_sheet",
        }
    }

    /// Actions only the application can carry out: a `chain` (and a `repeat`
    /// or an `only_if`) cannot hold them.
    fn is_app_level(&self) -> bool {
        matches!(
            self,
            Action::Chain { .. }
                | Action::RepeatLast
                | Action::DrawSymbol
                | Action::SwitchProfile { .. }
                | Action::NextProfile
                | Action::ShowCheatSheet
                | Action::ToggleEnabled
                | Action::ShowWindow
                | Action::ScrollHold { .. }
        )
    }

    /// Whether the action starts a program (`run`, `focus_or_launch`), which
    /// a pack from someone else could use to do anything at all: the import
    /// asks first.
    pub fn runs_a_program(&self) -> bool {
        match self {
            Action::Run { .. } | Action::FocusOrLaunch { .. } => true,
            Action::Chain { actions } => actions.iter().any(Action::runs_a_program),
            Action::Repeat { action, .. } | Action::OnlyIf { action, .. } => {
                action.runs_a_program()
            }
            _ => false,
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
            "focus_or_launch" => "Open or focus a program",
            "media" => "Media key",
            "scroll" => "Scroll",
            "repeat" => "Repeat",
            "scroll_hold" => "Scroll while moving",
            "only_if" => "Only when",
            "mouse_click" => "Mouse click",
            "drag" => "Drag",
            "chain" => "Several actions",
            "notify" => "Message",
            "toggle_enabled" => "Pause / resume",
            "repeat_last" => "Repeat last",
            "show_window" => "Show window",
            "draw_symbol" => "Draw symbol",
            "open" => "Open",
            "type_text" => "Type text",
            "copy" => "Copy to clipboard",
            "paste_text" => "Paste the text",
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
                    if matches!(a, Action::Chain { .. }) || a.is_app_level() {
                        return Err(format!("`chain` item {} cannot be `{}`", i + 1, a.kind()));
                    }
                    a.validate()
                        .map_err(|e| format!("`chain` item {}: {e}", i + 1))?;
                }
                Ok(())
            }
            Action::FocusOrLaunch { app, command, .. } => {
                if app.trim().is_empty() {
                    return Err("`focus_or_launch` needs an `app` to look for".into());
                }
                if command.trim().is_empty() {
                    return Err("`focus_or_launch` needs a `command` to start".into());
                }
                Ok(())
            }
            Action::Copy { text } | Action::PasteText { text } => {
                if text.is_empty() {
                    return Err("`copy` and `paste_text` need a non-empty `text`".into());
                }
                if text.chars().count() > 100_000 {
                    return Err("`copy` and `paste_text` take at most 100000 characters".into());
                }
                Ok(())
            }
            Action::Scroll { amount, smooth_ms } => {
                if *amount == 0 {
                    return Err("`scroll` needs a non-zero `amount`".into());
                }
                if amount.abs() > 100 {
                    return Err("`scroll` turns at most 100 notches at a time".into());
                }
                if smooth_ms.is_some_and(|ms| ms > 2_000) {
                    return Err("`scroll.smooth_ms` is at most 2000".into());
                }
                Ok(())
            }
            Action::ScrollHold { mm_per_notch } => {
                if !(1.0..=50.0).contains(mm_per_notch) {
                    return Err("`scroll_hold.mm_per_notch` must be between 1 and 50".into());
                }
                Ok(())
            }
            Action::Repeat { times, action } => {
                if !(1..=50).contains(times) {
                    return Err("`repeat` runs 1 to 50 times".into());
                }
                if matches!(**action, Action::Repeat { .. }) {
                    return Err("`repeat` cannot hold another `repeat`".into());
                }
                if action.is_app_level() {
                    return Err(format!("`repeat` cannot hold `{}`", action.kind()));
                }
                action.validate().map_err(|e| format!("`repeat` → {e}"))
            }
            Action::OnlyIf { when, action } => {
                if when.app.is_none() && when.title.is_none() {
                    return Err("`only_if` needs an `app` or a `title`".into());
                }
                if let Some(t) = &when.title
                    && let Err(e) = regex::Regex::new(t)
                {
                    return Err(format!(
                        "`only_if` → title: invalid regular expression: {e}"
                    ));
                }
                if matches!(**action, Action::OnlyIf { .. }) {
                    return Err("`only_if` cannot hold another `only_if`".into());
                }
                if action.is_app_level() {
                    return Err(format!("`only_if` cannot hold `{}`", action.kind()));
                }
                action.validate().map_err(|e| format!("`only_if` → {e}"))
            }
            Action::Drag { dx, dy } => {
                if *dx == 0 && *dy == 0 {
                    return Err("`drag` needs a `dx` or a `dy`".into());
                }
                if dx.abs() > 4000 || dy.abs() > 4000 {
                    return Err("`drag` moves at most 4000 pixels at a time".into());
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

/// The values a placeholder in an action's text can stand for, gathered when
/// the action runs. `date`, `time` and `clipboard` are filled by the caller,
/// which is the side that knows how to read them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActionContext {
    /// Executable name of the foreground application.
    pub app: String,
    /// Title of the foreground window.
    pub title: String,
    pub profile: String,
    /// Local date, `YYYY-MM-DD`.
    pub date: String,
    /// Local time, `HH:MM`.
    pub time: String,
    /// The clipboard's text, read only when the action asks for it.
    pub clipboard: Option<String>,
}

impl ActionContext {
    /// Replaces `{{app}}`, `{{title}}`, `{{profile}}`, `{{date}}`, `{{time}}`
    /// and `{{clipboard}}`. Anything else between braces is left as it is, so
    /// a literal `{{` is harmless.
    pub fn expand(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(start) = rest.find("{{") {
            out.push_str(&rest[..start]);
            let after = &rest[start + 2..];
            let Some(end) = after.find("}}") else {
                out.push_str(&rest[start..]);
                return out;
            };
            match after[..end].trim() {
                "app" => out.push_str(&self.app),
                "title" => out.push_str(&self.title),
                "profile" => out.push_str(&self.profile),
                "date" => out.push_str(&self.date),
                "time" => out.push_str(&self.time),
                "clipboard" => out.push_str(self.clipboard.as_deref().unwrap_or_default()),
                _ => {
                    out.push_str("{{");
                    out.push_str(&after[..end]);
                    out.push_str("}}");
                }
            }
            rest = &after[end + 2..];
        }
        out.push_str(rest);
        out
    }

    /// A copy of `action` with every placeholder in its texts filled in.
    pub fn resolve(&self, action: &Action) -> Action {
        match action {
            Action::Run {
                command,
                args,
                hidden,
                cwd,
                shell,
                env,
                wait,
                timeout_ms,
            } => Action::Run {
                command: self.expand(command),
                args: args.iter().map(|a| self.expand(a)).collect(),
                hidden: *hidden,
                cwd: cwd.as_ref().map(|c| self.expand(c)),
                shell: *shell,
                env: env
                    .iter()
                    .map(|(k, v)| (k.clone(), self.expand(v)))
                    .collect(),
                wait: *wait,
                timeout_ms: *timeout_ms,
            },
            Action::FocusOrLaunch { app, command, args } => Action::FocusOrLaunch {
                app: self.expand(app),
                command: self.expand(command),
                args: args.iter().map(|a| self.expand(a)).collect(),
            },
            Action::Open { target } => Action::Open {
                target: self.expand(target),
            },
            Action::Notify { title, message } => Action::Notify {
                title: title.as_ref().map(|t| self.expand(t)),
                message: self.expand(message),
            },
            Action::TypeText { text } => Action::TypeText {
                text: self.expand(text),
            },
            Action::Copy { text } => Action::Copy {
                text: self.expand(text),
            },
            Action::PasteText { text } => Action::PasteText {
                text: self.expand(text),
            },
            Action::Chain { actions } => Action::Chain {
                actions: actions.iter().map(|a| self.resolve(a)).collect(),
            },
            Action::Repeat { times, action } => Action::Repeat {
                times: *times,
                action: Box::new(self.resolve(action)),
            },
            Action::OnlyIf { when, action } => Action::OnlyIf {
                when: when.clone(),
                action: Box::new(self.resolve(action)),
            },
            other => other.clone(),
        }
    }

    /// Whether the action asks for `{{clipboard}}`, so the caller reads the
    /// clipboard only when it is needed.
    pub fn mentions_clipboard(action: &Action) -> bool {
        fn mentions(text: &str) -> bool {
            text.contains("{{clipboard}}")
        }
        match action {
            Action::Run {
                command,
                args,
                cwd,
                env,
                ..
            } => {
                mentions(command)
                    || args.iter().any(|a| mentions(a))
                    || cwd.as_deref().is_some_and(mentions)
                    || env.values().any(|v| mentions(v))
            }
            Action::Open { target } => mentions(target),
            Action::Notify { title, message } => {
                mentions(message) || title.as_deref().is_some_and(mentions)
            }
            Action::TypeText { text } => mentions(text),
            Action::Chain { actions } => actions.iter().any(Self::mentions_clipboard),
            Action::Repeat { action, .. } | Action::OnlyIf { action, .. } => {
                Self::mentions_clipboard(action)
            }
            _ => false,
        }
    }
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
    fn repeat_and_only_if_are_checked() {
        let repeat = parse(r#"action = { type = "repeat", times = 3, action = { type = "keys", combo = "Ctrl+C" } }"#)
            .unwrap();
        assert_eq!(repeat.kind(), "repeat");
        assert!(repeat.validate().is_ok());
        // Out of range, nested, or holding something only the app can do.
        for bad in [
            r#"action = { type = "repeat", times = 0, action = { type = "minimize" } }"#,
            r#"action = { type = "repeat", times = 51, action = { type = "minimize" } }"#,
            r#"action = { type = "repeat", times = 2, action = { type = "repeat", times = 2, action = { type = "minimize" } } }"#,
            r#"action = { type = "repeat", times = 2, action = { type = "show_cheat_sheet" } }"#,
        ] {
            assert!(
                parse(bad).unwrap().validate().is_err(),
                "{bad} should be refused"
            );
        }
        let only = parse(
            r#"action = { type = "only_if", when = { app = "chrome.exe" }, action = { type = "lock_screen" } }"#,
        )
        .unwrap();
        assert!(only.validate().is_ok());
        // No condition, a broken regular expression, or a nested `only_if`.
        for bad in [
            r#"action = { type = "only_if", when = {}, action = { type = "lock_screen" } }"#,
            r#"action = { type = "only_if", when = { title = "(" }, action = { type = "lock_screen" } }"#,
            r#"action = { type = "only_if", when = { app = "a" }, action = { type = "only_if", when = { app = "b" }, action = { type = "minimize" } } }"#,
        ] {
            assert!(
                parse(bad).unwrap().validate().is_err(),
                "{bad} should be refused"
            );
        }
        // A smooth scroll is bounded in time.
        assert!(
            parse(r#"action = { type = "scroll", amount = 3, smooth_ms = 5000 }"#)
                .unwrap()
                .validate()
                .is_err()
        );
    }

    #[test]
    fn a_window_condition_matches_what_it_says() {
        use crate::config::OneOrMany;
        let app = |exe: &str, title: &str| AppInfo {
            exe: exe.into(),
            title: title.into(),
            pid: 1,
            window: 1,
            fullscreen: false,
        };
        let when = crate::config::When {
            app: Some(OneOrMany::One("chrome.exe".into())),
            title: Some("^Inbox".into()),
        };
        assert!(when.matches(Some(&app("chrome.exe", "Inbox — Chrome"))));
        assert!(!when.matches(Some(&app("firefox.exe", "Inbox — Firefox"))));
        assert!(!when.matches(Some(&app("chrome.exe", "Something else"))));
        assert!(!when.matches(None));
        // The title alone is enough.
        let title_only = crate::config::When {
            app: None,
            title: Some("Inbox".into()),
        };
        assert!(title_only.matches(Some(&app("anything.exe", "An Inbox"))));
        // A broken expression matches nothing (the configuration refuses it
        // before it gets here).
        let broken = crate::config::When {
            app: None,
            title: Some("(".into()),
        };
        assert!(!broken.matches(Some(&app("x.exe", "("))));
    }

    #[test]
    fn a_program_is_recognized_inside_a_chain_too() {
        let plain = parse(r#"action = { type = "keys", combo = "Ctrl+C" }"#).unwrap();
        assert!(!plain.runs_a_program());
        let run = parse(r#"action = { type = "run", command = "notepad" }"#).unwrap();
        assert!(run.runs_a_program());
        let focus =
            parse(r#"action = { type = "focus_or_launch", app = "code.exe", command = "code" }"#)
                .unwrap();
        assert!(focus.runs_a_program());
        let chain = Action::Chain {
            actions: vec![
                Action::Minimize,
                Action::Chain {
                    actions: vec![Action::Run {
                        command: "cmd".into(),
                        args: Vec::new(),
                        hidden: true,
                        cwd: None,
                        shell: false,
                        env: Default::default(),
                        wait: false,
                        timeout_ms: 30_000,
                    }],
                },
            ],
        };
        assert!(chain.runs_a_program());
    }

    #[test]
    fn mouse_actions_are_validated() {
        assert!(
            parse(r#"action = { type = "scroll", amount = 3 }"#)
                .unwrap()
                .validate()
                .is_ok()
        );
        assert!(
            parse(r#"action = { type = "scroll", amount = 0 }"#)
                .unwrap()
                .validate()
                .is_err()
        );
        assert!(
            parse(r#"action = { type = "scroll", amount = 500 }"#)
                .unwrap()
                .validate()
                .is_err()
        );
        let click = parse(r#"action = { type = "mouse_click", button = "middle", double = true }"#)
            .unwrap();
        assert_eq!(click.kind(), "mouse_click");
        assert!(click.validate().is_ok());
        // The default button is the left one.
        assert!(matches!(
            parse(r#"action = { type = "mouse_click" }"#).unwrap(),
            Action::MouseClick {
                button: MouseButton::Left,
                double: false
            }
        ));
        assert!(
            parse(r#"action = { type = "drag", dx = 0, dy = 0 }"#)
                .unwrap()
                .validate()
                .is_err()
        );
        assert!(
            parse(r#"action = { type = "drag", dx = 10, dy = -200 }"#)
                .unwrap()
                .validate()
                .is_ok()
        );
        // A chain may use them.
        let chain = parse(
            r#"action = { type = "chain", actions = [
                { type = "scroll", amount = 3 }, { type = "mouse_click" } ] }"#,
        )
        .unwrap();
        assert!(chain.validate().is_ok());
    }

    #[test]
    fn focus_or_launch_needs_a_program_and_a_command() {
        let ok = parse(
            r#"action = { type = "focus_or_launch", app = "code.exe", command = "code", args = ["-n"] }"#,
        )
        .unwrap();
        assert_eq!(ok.kind(), "focus_or_launch");
        assert!(ok.validate().is_ok());
        assert!(
            parse(r#"action = { type = "focus_or_launch", app = " ", command = "code" }"#)
                .unwrap()
                .validate()
                .is_err()
        );
        assert!(
            parse(r#"action = { type = "focus_or_launch", app = "code.exe", command = "" }"#)
                .unwrap()
                .validate()
                .is_err()
        );
        // A `chain` may start it: it needs no window of GlyphPad's own.
        let chain = parse(
            r#"action = { type = "chain", actions = [
                { type = "focus_or_launch", app = "code.exe", command = "code" },
                { type = "wait", ms = 50 } ] }"#,
        )
        .unwrap();
        assert!(chain.validate().is_ok());
    }

    #[test]
    fn copy_and_paste_need_a_text() {
        let copy = parse(r#"action = { type = "copy", text = "hello" }"#).unwrap();
        assert_eq!(copy.kind(), "copy");
        assert!(copy.validate().is_ok());
        let paste = parse(r#"action = { type = "paste_text", text = "hello" }"#).unwrap();
        assert!(paste.validate().is_ok());
        assert!(
            parse(r#"action = { type = "copy", text = "" }"#)
                .unwrap()
                .validate()
                .is_err()
        );
        // Both take placeholders too.
        let ctx = ActionContext {
            app: "chrome.exe".into(),
            ..Default::default()
        };
        assert_eq!(
            ctx.resolve(&paste),
            Action::PasteText {
                text: "hello".into()
            }
        );
        assert_eq!(
            ctx.resolve(&Action::Copy {
                text: "{{app}}".into()
            }),
            Action::Copy {
                text: "chrome.exe".into()
            }
        );
    }

    #[test]
    fn placeholders_are_filled_in() {
        let ctx = ActionContext {
            app: "chrome.exe".into(),
            title: "Inbox — Chrome".into(),
            profile: "work".into(),
            date: "2026-10-25".into(),
            time: "09:30".into(),
            clipboard: Some("pasted".into()),
        };
        assert_eq!(
            ctx.expand("{{app}} | {{title}} | {{profile}} | {{date}} {{time}} | {{clipboard}}"),
            "chrome.exe | Inbox — Chrome | work | 2026-10-25 09:30 | pasted"
        );
        // Unknown names and a lone brace stay as they are: a literal `{{` in
        // a command is harmless.
        assert_eq!(
            ctx.expand("{{nope}} {{ app }} {{"),
            "{{nope}} chrome.exe {{"
        );
        // Without a clipboard the placeholder is empty, not literal.
        let empty = ActionContext {
            clipboard: None,
            ..ctx.clone()
        };
        assert_eq!(empty.expand("[{{clipboard}}]"), "[]");

        let run = Action::Run {
            command: "git commit -m {{title}}".into(),
            args: vec!["--author".into(), "{{app}}".into()],
            hidden: false,
            cwd: Some("C:/{{profile}}".into()),
            shell: true,
            env: [("WHO".to_string(), "{{app}}".to_string())].into(),
            wait: false,
            timeout_ms: 30_000,
        };
        let Action::Run {
            command,
            args,
            cwd,
            env,
            ..
        } = ctx.resolve(&run)
        else {
            panic!("still a run");
        };
        assert_eq!(command, "git commit -m Inbox — Chrome");
        assert_eq!(args, ["--author", "chrome.exe"]);
        assert_eq!(cwd.as_deref(), Some("C:/work"));
        assert_eq!(env.get("WHO").map(String::as_str), Some("chrome.exe"));
        // A chain is resolved item by item.
        let chain = ctx.resolve(&Action::Chain {
            actions: vec![
                Action::Notify {
                    title: None,
                    message: "{{app}}".into(),
                },
                run.clone(),
            ],
        });
        let Action::Chain { actions } = chain else {
            panic!("still a chain");
        };
        assert_eq!(
            actions[0],
            Action::Notify {
                title: None,
                message: "chrome.exe".into()
            }
        );
    }

    #[test]
    fn the_clipboard_is_only_read_when_asked_for() {
        let plain = Action::Run {
            command: "notepad".into(),
            args: Vec::new(),
            hidden: false,
            cwd: None,
            shell: false,
            env: Default::default(),
            wait: false,
            timeout_ms: 30_000,
        };
        assert!(!ActionContext::mentions_clipboard(&plain));
        assert!(ActionContext::mentions_clipboard(&Action::Chain {
            actions: vec![Action::TypeText {
                text: "{{clipboard}}".into()
            }],
        }));
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
