//! `repeat_last`: running the last gesture's action again (SPEC §9.1).

use std::time::{Duration, Instant};

use crate::action::{Action, AppInfo};
use crate::config::{Gesture, Settings};

/// Whether `repeat_last` may repeat this gesture's action. A gesture's own
/// `repeatable` wins; otherwise locking, pausing and window toggling are
/// excluded, since repeating them by accident is surprising or harmful.
pub fn is_repeatable(gesture: &Gesture) -> bool {
    gesture.def.repeatable.unwrap_or(!matches!(
        gesture.def.action,
        Action::LockScreen
            | Action::ToggleEnabled
            | Action::ShowWindow
            | Action::RepeatLast
            | Action::DrawSymbol
    ))
}

#[derive(Debug, Clone, PartialEq)]
pub struct LastAction {
    pub gesture: String,
    pub action: Action,
    /// Foreground application when it ran.
    pub app: Option<AppInfo>,
    pub at: Instant,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    Repeat(LastAction),
    /// Nothing to repeat yet.
    Nothing,
    /// The last action is older than `repeat_timeout_s`.
    Expired,
    /// Another application is in the foreground (`repeat_same_app_only`).
    OtherApp {
        then: String,
        now: String,
    },
}

/// Remembers the last repeatable action. `repeat_last` never records
/// itself, so repeating several times keeps repeating the original action.
#[derive(Debug, Default)]
pub struct Repeater {
    last: Option<LastAction>,
}

impl Repeater {
    pub fn last(&self) -> Option<&LastAction> {
        self.last.as_ref()
    }

    /// Records a gesture whose action just ran.
    pub fn record(&mut self, gesture: &Gesture, app: Option<AppInfo>, at: Instant) {
        if is_repeatable(gesture) {
            self.last = Some(LastAction {
                gesture: gesture.def.name.clone(),
                action: gesture.def.action.clone(),
                app,
                at,
            });
        }
    }

    pub fn decide(&self, settings: &Settings, app: Option<&AppInfo>, now: Instant) -> Decision {
        let Some(last) = &self.last else {
            return Decision::Nothing;
        };
        if let Some(timeout) = settings.repeat_timeout_s
            && now.saturating_duration_since(last.at) > Duration::from_secs(timeout)
        {
            return Decision::Expired;
        }
        if settings.repeat_same_app_only {
            let then = last.app.as_ref();
            // The same window, not merely the same program: a `close_window`
            // repeated after switching to another Explorer or browser window
            // (same process) would close the wrong one.
            let same = match (then, app) {
                (Some(a), Some(b)) if a.window != 0 && b.window != 0 => a.window == b.window,
                (Some(a), Some(b)) => a.pid == b.pid && a.exe.eq_ignore_ascii_case(&b.exe),
                (None, None) => true,
                _ => false,
            };
            if !same {
                let name = |a: Option<&AppInfo>| a.map_or("unknown".to_string(), |a| a.exe.clone());
                return Decision::OtherApp {
                    then: name(then),
                    now: name(app),
                };
            }
        }
        Decision::Repeat(last.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::parse;

    fn gestures() -> Vec<Gesture> {
        parse(
            r#"
[[gesture]]
name = "close"
type = "tap"
fingers = 3
action = { type = "close_window" }

[[gesture]]
name = "lock"
type = "tap"
fingers = 4
action = { type = "lock_screen" }

[[gesture]]
name = "secret"
type = "tap"
fingers = 5
repeatable = false
action = { type = "keys", combo = "Ctrl+S" }

[[gesture]]
name = "lock-again"
type = "tap"
fingers = 2
repeatable = true
action = { type = "lock_screen" }
"#,
        )
        .unwrap()
        .config
        .gestures
    }

    fn app(exe: &str, pid: u32) -> AppInfo {
        AppInfo {
            exe: exe.into(),
            title: String::new(),
            pid,
            window: 0,
        }
    }

    fn window(exe: &str, pid: u32, window: u64) -> AppInfo {
        AppInfo {
            window,
            ..app(exe, pid)
        }
    }

    #[test]
    fn exclusions() {
        let g = gestures();
        assert!(is_repeatable(&g[0]));
        assert!(!is_repeatable(&g[1]));
        assert!(!is_repeatable(&g[2]));
        assert!(is_repeatable(&g[3]));
    }

    #[test]
    fn repeats_only_in_the_same_app_and_in_time() {
        let g = gestures();
        let t0 = Instant::now();
        let mut settings = Settings::default();
        let mut r = Repeater::default();
        assert_eq!(r.decide(&settings, None, t0), Decision::Nothing);

        r.record(&g[0], Some(app("notepad.exe", 7)), t0);
        // Non-repeatable gestures do not replace it.
        r.record(&g[1], Some(app("notepad.exe", 7)), t0);
        match r.decide(&settings, Some(&app("Notepad.exe", 7)), t0) {
            Decision::Repeat(last) => assert_eq!(last.gesture, "close"),
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            r.decide(&settings, Some(&app("notepad.exe", 8)), t0),
            Decision::OtherApp { .. }
        ));
        settings.repeat_same_app_only = false;
        assert!(matches!(
            r.decide(&settings, Some(&app("chrome.exe", 9)), t0),
            Decision::Repeat(_)
        ));

        // Another window of the same process is another target.
        settings.repeat_same_app_only = true;
        let mut r2 = Repeater::default();
        r2.record(&g[0], Some(window("explorer.exe", 5, 100)), t0);
        assert!(matches!(
            r2.decide(&settings, Some(&window("explorer.exe", 5, 200)), t0),
            Decision::OtherApp { .. }
        ));
        assert!(matches!(
            r2.decide(&settings, Some(&window("explorer.exe", 5, 100)), t0),
            Decision::Repeat(_)
        ));

        settings.repeat_timeout_s = Some(60);
        let later = t0 + Duration::from_secs(61);
        assert_eq!(r.decide(&settings, None, later), Decision::Expired);
    }
}
