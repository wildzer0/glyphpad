//! Experimental cursor freeze for one-finger draw modes (SPEC §5 strategy 5).
//!
//! Raw Input cannot stop Windows from moving the pointer, but `ClipCursor`
//! can confine it to a 1×1 rectangle where it already is, so a symbol drawn
//! with one finger does not drag the pointer around. Taps still click.

use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::UI::WindowsAndMessaging::{ClipCursor, GetClipCursor, GetCursorPos};

/// Keeps the cursor pinned while alive; releases it on drop, including
/// while unwinding from a panic.
pub struct CursorFreeze(());

impl CursorFreeze {
    pub fn new() -> Option<Self> {
        let mut p = POINT::default();
        // SAFETY: `p` is a valid out pointer.
        unsafe { GetCursorPos(&mut p) }.ok()?;
        let rect = RECT {
            left: p.x,
            top: p.y,
            right: p.x + 1,
            bottom: p.y + 1,
        };
        // SAFETY: `rect` outlives the call.
        unsafe { ClipCursor(Some(&rect)) }.ok()?;
        Some(Self(()))
    }
}

impl Drop for CursorFreeze {
    fn drop(&mut self) {
        // SAFETY: a null rectangle removes the confinement.
        let _ = unsafe { ClipCursor(None) };
    }
}

/// Releases a freeze left behind by a previous run that crashed before its
/// guard could run. Only a 1×1 confinement, which is ours, is touched, so
/// a game's own cursor clip is left alone.
pub fn release_stale_freeze() {
    let mut r = RECT::default();
    // SAFETY: `r` is a valid out pointer.
    if unsafe { GetClipCursor(&mut r) }.is_ok() && r.right - r.left == 1 && r.bottom - r.top == 1 {
        tracing::warn!("releasing a cursor freeze left by a previous run");
        // SAFETY: a null rectangle removes the confinement.
        let _ = unsafe { ClipCursor(None) };
    }
}
