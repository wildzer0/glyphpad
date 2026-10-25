//! One GlyphPad at a time (SPEC §10.1): a named mutex tells whether another
//! instance runs, and a message-only window lets a second instance ask the
//! first to show its window before exiting.

use std::cell::RefCell;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, ERROR_SUCCESS, GetLastError, HANDLE, HWND, LPARAM, LRESULT,
    SetLastError, WPARAM,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, FindWindowExW, GetMessageW, HWND_MESSAGE,
    MSG, PostMessageW, RegisterClassExW, WINDOW_EX_STYLE, WM_APP, WNDCLASSEXW, WS_POPUP,
};
use windows::core::{PCWSTR, w};

/// Per-user (the `Local\` namespace is per session).
const MUTEX_NAME: PCWSTR = w!("Local\\GlyphPad.SingleInstance");
const WINDOW_CLASS: PCWSTR = w!("GlyphPadInstance");
/// "Show your window", sent by a second instance.
const WM_SHOW_WINDOW: u32 = WM_APP + 1;

/// Held by the running instance; dropping it lets another one start.
pub struct InstanceGuard(HANDLE);

// SAFETY: the guard owns a kernel mutex handle, which any thread may close;
// the updater thread gives the lock up while the main thread keeps running.
unsafe impl Send for InstanceGuard {}

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        // SAFETY: the mutex handle is owned by this guard and closed once.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

pub enum Instance {
    /// We are the only instance; `on_show` runs when another one asks.
    Primary(InstanceGuard),
    /// Another instance runs and was asked to show its window.
    Secondary,
}

thread_local! {
    static ON_SHOW: RefCell<Option<Box<dyn Fn()>>> = const { RefCell::new(None) };
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_SHOW_WINDOW {
        let _ = std::panic::catch_unwind(|| {
            ON_SHOW.with_borrow(|f| {
                if let Some(f) = f {
                    f();
                }
            })
        });
        return LRESULT(0);
    }
    // SAFETY: forwarding the original arguments.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

pub fn acquire(on_show: impl Fn() + Send + 'static) -> Instance {
    // A stale error from an earlier call must not look like "already exists".
    // SAFETY: no arguments.
    unsafe { SetLastError(ERROR_SUCCESS) };
    // SAFETY: a static name and no security attributes.
    let mutex = unsafe { CreateMutexW(None, false, MUTEX_NAME) };
    // SAFETY: read right after the call that set it.
    let existed = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    let Ok(mutex) = mutex else {
        tracing::warn!("cannot create the single-instance mutex; running anyway");
        return Instance::Primary(InstanceGuard(HANDLE::default()));
    };
    if existed {
        // SAFETY: the duplicate handle is ours to close.
        let _ = unsafe { CloseHandle(mutex) };
        signal_primary();
        return Instance::Secondary;
    }
    std::thread::Builder::new()
        .name("single-instance".into())
        .spawn(move || listen(on_show))
        .expect("cannot spawn the single-instance thread");
    Instance::Primary(InstanceGuard(mutex))
}

/// Takes the lock again after giving it up for an update that then failed.
/// The window that answers "show yourself" is still there, so no listener
/// is started. `None` if somebody else holds the lock now.
pub fn reacquire() -> Option<InstanceGuard> {
    // SAFETY: no arguments.
    unsafe { SetLastError(ERROR_SUCCESS) };
    // SAFETY: a static name and no security attributes.
    let mutex = unsafe { CreateMutexW(None, false, MUTEX_NAME) }.ok()?;
    // SAFETY: read right after the call that set it.
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        // SAFETY: the duplicate handle is ours to close.
        let _ = unsafe { CloseHandle(mutex) };
        return None;
    }
    Some(InstanceGuard(mutex))
}

fn signal_primary() {
    // The first instance may still be creating its window.
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        // SAFETY: static class name; searching message-only windows.
        if let Ok(hwnd) =
            unsafe { FindWindowExW(Some(HWND_MESSAGE), None, WINDOW_CLASS, PCWSTR::null()) }
        {
            // SAFETY: posting a message with no pointer payload.
            let _ = unsafe { PostMessageW(Some(hwnd), WM_SHOW_WINDOW, WPARAM(0), LPARAM(0)) };
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    tracing::warn!("another GlyphPad is running but did not answer");
}

fn listen(on_show: impl Fn() + 'static) {
    ON_SHOW.with_borrow_mut(|f| *f = Some(Box::new(on_show)));
    // SAFETY: plain window creation with static strings and a valid window procedure.
    let created = unsafe {
        GetModuleHandleW(None).and_then(|module| {
            let instance = module.into();
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(wndproc),
                hInstance: instance,
                lpszClassName: WINDOW_CLASS,
                ..Default::default()
            };
            RegisterClassExW(&wc);
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                WINDOW_CLASS,
                w!("GlyphPad"),
                WS_POPUP,
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                Some(instance),
                None,
            )
        })
    };
    if let Err(e) = created {
        tracing::warn!("cannot create the single-instance window: {e}");
        return;
    }
    let mut msg = MSG::default();
    // SAFETY: standard message loop on this thread's windows.
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 > 0 {
        unsafe { DispatchMessageW(&msg) };
    }
}
