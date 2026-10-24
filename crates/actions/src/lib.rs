//! Execution of configured actions (keys, lock screen, close window, run, …)
//! on each supported platform.

use glyphpad_core::action::{ActionExecutor, ForegroundInfo};

#[cfg(windows)]
pub mod windows;

/// The action executor of this platform.
pub fn executor() -> Box<dyn ActionExecutor + Send> {
    #[cfg(windows)]
    {
        Box::new(windows::WindowsExecutor)
    }
    #[cfg(not(windows))]
    {
        Box::new(unsupported::Unsupported)
    }
}

/// The foreground-window information source of this platform.
pub fn foreground() -> Box<dyn ForegroundInfo + Send> {
    #[cfg(windows)]
    {
        Box::new(windows::WindowsForeground)
    }
    #[cfg(not(windows))]
    {
        Box::new(unsupported::Unsupported)
    }
}

#[cfg(not(windows))]
mod unsupported {
    use glyphpad_core::action::{Action, ActionError, ActionExecutor, AppInfo, ForegroundInfo};

    /// Placeholder until the Linux backend (M7).
    pub struct Unsupported;

    impl ActionExecutor for Unsupported {
        fn execute(&self, action: &Action) -> Result<(), ActionError> {
            Err(format!(
                "`{}` is not implemented on this platform yet (M7)",
                action.kind()
            )
            .into())
        }
    }

    impl ForegroundInfo for Unsupported {
        fn foreground_app(&self) -> Option<AppInfo> {
            None
        }
    }
}
