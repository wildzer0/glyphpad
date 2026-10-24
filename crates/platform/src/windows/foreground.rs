//! Bringing our own window to the foreground.
//!
//! Windows refuses `SetForegroundWindow` to a process that is not in the
//! foreground and flashes its taskbar button instead, which is what happens
//! when a gesture (raw input does not count as user input to us) asks to
//! show the window. Briefly attaching our input queue to the foreground
//! window's thread lifts the restriction.

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId, IsIconic, SW_RESTORE, SW_SHOW,
    SetForegroundWindow, ShowWindow,
};

/// Restores, shows and activates `hwnd`. Must run on the thread that owns
/// the window.
pub fn bring_to_front(hwnd: isize) {
    let hwnd = HWND(hwnd as *mut _);
    // SAFETY: window calls on a handle owned by this thread; the thread
    // input attachment is undone before returning.
    unsafe {
        let _ = ShowWindow(
            hwnd,
            if IsIconic(hwnd).as_bool() {
                SW_RESTORE
            } else {
                SW_SHOW
            },
        );
        let foreground = GetForegroundWindow();
        let theirs = GetWindowThreadProcessId(foreground, None);
        let ours = GetCurrentThreadId();
        let attached =
            theirs != 0 && theirs != ours && AttachThreadInput(ours, theirs, true).as_bool();
        let _ = BringWindowToTop(hwnd);
        let _ = SetForegroundWindow(hwnd);
        if attached {
            let _ = AttachThreadInput(ours, theirs, false);
        }
    }
}
