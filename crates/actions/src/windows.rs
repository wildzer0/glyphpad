//! Windows actions (SPEC §4): `SendInput` for keys and media keys,
//! `LockWorkStation`, and window management on the foreground window.
//!
//! Platform FFI module, the only kind allowed to use `unsafe` (SPEC §2).
#![allow(unsafe_code)]

use std::collections::BTreeMap;
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::time::{Duration, Instant};

use glyphpad_core::action::{
    Action, ActionError, ActionExecutor, AppInfo, DesktopOp, ForegroundInfo, MediaKey,
    ScreenshotMode, WindowOp,
};
use glyphpad_core::keys::{Key, KeyCombo};
use windows::Win32::Foundation::{
    CloseHandle, GlobalFree, HANDLE, HGLOBAL, HWND, LPARAM, RECT, WPARAM,
};
use windows::Win32::Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITOR_DEFAULTTONEAREST, MONITORINFO,
    MonitorFromWindow,
};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::Win32::System::Ole::CF_UNICODETEXT;
use windows::Win32::System::Shutdown::LockWorkStation;
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
    QueryFullProcessImageNameW, TerminateProcess,
};
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GA_ROOTOWNER, GWL_EXSTYLE, GetAncestor, GetClassNameW, GetForegroundWindow,
    GetWindowLongW, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId, HWND_NOTOPMOST,
    HWND_TOPMOST, IsWindowVisible, IsZoomed, PostMessageW, SW_MAXIMIZE, SW_MINIMIZE, SW_RESTORE,
    SW_SHOWNORMAL, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos,
    ShowWindowAsync, WINDOW_EX_STYLE, WM_CLOSE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
};
use windows::core::{HSTRING, PWSTR, w};

/// `CREATE_NO_WINDOW`: run a console program without opening a console.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Window classes of the desktop and the taskbar. Closing the desktop
/// (`Progman`/`WorkerW`) opens the "Shut down Windows" dialog, so window
/// actions refuse to touch them.
const SHELL_CLASSES: &[&str] = &[
    "Progman",
    "WorkerW",
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
];

#[derive(Debug, Default)]
pub struct WindowsExecutor;

impl ActionExecutor for WindowsExecutor {
    fn execute(&self, action: &Action) -> Result<(), ActionError> {
        match action {
            Action::Keys { combo, sequence } => {
                for c in combo.iter().chain(sequence) {
                    send_combo(c)?;
                }
                Ok(())
            }
            // Win+L cannot be injected (the secure attention sequence is
            // filtered), so locking uses the dedicated API.
            // SAFETY: no arguments.
            Action::LockScreen => Ok(unsafe { LockWorkStation() }?),
            Action::CloseWindow { force } => close_foreground(*force),
            Action::Minimize => {
                let hwnd = target_window()?;
                // Async: a hung target window must not block us.
                // SAFETY: `hwnd` is a window handle; a stale one only makes the call fail.
                let _ = unsafe { ShowWindowAsync(hwnd, SW_MINIMIZE) };
                Ok(())
            }
            Action::Maximize => {
                let hwnd = target_window()?;
                // SAFETY: as above.
                unsafe {
                    let cmd = if IsZoomed(hwnd).as_bool() {
                        SW_RESTORE
                    } else {
                        SW_MAXIMIZE
                    };
                    let _ = ShowWindowAsync(hwnd, cmd);
                }
                Ok(())
            }
            Action::Run {
                command,
                args,
                hidden,
                cwd,
                shell,
                env,
                wait,
                timeout_ms,
            } => run(
                command,
                args,
                *hidden,
                cwd.as_deref(),
                *shell,
                env,
                *wait,
                *timeout_ms,
            ),
            Action::Copy { text } => set_clipboard_text(text),
            Action::PasteText { text } => {
                set_clipboard_text(text)?;
                // Give the clipboard owner a moment to take the data: some
                // applications read it on the next message they process.
                std::thread::sleep(Duration::from_millis(30));
                send_combo(&KeyCombo {
                    modifiers: vec![Key::Ctrl],
                    key: Key::Letter('V'),
                })
            }
            Action::Media { key } => send_combo(&KeyCombo {
                modifiers: Vec::new(),
                key: match key {
                    MediaKey::PlayPause => Key::MediaPlayPause,
                    MediaKey::Next => Key::MediaNext,
                    MediaKey::Previous => Key::MediaPrevious,
                    MediaKey::Stop => Key::MediaStop,
                    MediaKey::VolumeUp => Key::VolumeUp,
                    MediaKey::VolumeDown => Key::VolumeDown,
                    MediaKey::Mute => Key::VolumeMute,
                },
            }),
            Action::Notify { title, message } => {
                // Notifications need the tray icon, which arrives with the GUI (M4).
                tracing::info!(
                    title = title.as_deref().unwrap_or("GlyphPad"),
                    "notify: {message}"
                );
                Ok(())
            }
            Action::Open { target } => open_target(target),
            Action::TypeText { text } => type_text(text),
            Action::Window { op } => window_op(*op),
            Action::VirtualDesktop { op } => {
                // Windows' own shortcuts: Win+Ctrl+Left/Right, Win+Ctrl+D.
                let key = match op {
                    DesktopOp::Previous => Key::Left,
                    DesktopOp::Next => Key::Right,
                    DesktopOp::New => Key::Letter('D'),
                };
                send_combo(&KeyCombo {
                    modifiers: vec![Key::Win, Key::Ctrl],
                    key,
                })
            }
            Action::Screenshot { mode } => match mode {
                // Win+Shift+S: the snipping overlay.
                ScreenshotMode::Region => send_combo(&KeyCombo {
                    modifiers: vec![Key::Win, Key::Shift],
                    key: Key::Letter('S'),
                }),
                // Win+PrintScreen: the whole screen into Pictures\Screenshots.
                ScreenshotMode::Screen => send_combo(&KeyCombo {
                    modifiers: vec![Key::Win],
                    key: Key::PrintScreen,
                }),
            },
            other => Err(format!("`{}` is not a platform action", other.kind()).into()),
        }
    }
}

/// Opens a file, folder or address with the program Windows associates
/// with it (`ShellExecute` with the "open" verb).
fn open_target(target: &str) -> Result<(), ActionError> {
    let target = HSTRING::from(target.trim());
    // SAFETY: valid, NUL-terminated wide strings that outlive the call.
    let result = unsafe { ShellExecuteW(None, w!("open"), &target, None, None, SW_SHOWNORMAL) };
    // ShellExecute reports success with a value above 32.
    if result.0 as usize > 32 {
        Ok(())
    } else {
        Err(format!(
            "Windows could not open `{target}` (code {})",
            result.0 as usize
        )
        .into())
    }
}

/// Types `text` as Unicode characters, so any layout and any character
/// works; a newline presses Enter.
fn type_text(text: &str) -> Result<(), ActionError> {
    let mut inputs: Vec<INPUT> = Vec::new();
    for unit in text.replace("\r\n", "\n").encode_utf16() {
        if unit == u16::from(b'\n') {
            inputs.push(key_input(Key::Enter, false));
            inputs.push(key_input(Key::Enter, true));
            continue;
        }
        for up in [false, true] {
            let mut flags = KEYEVENTF_UNICODE;
            if up {
                flags |= KEYEVENTF_KEYUP;
            }
            inputs.push(INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(0),
                        wScan: unit,
                        dwFlags: flags,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            });
        }
    }
    // In chunks, so a long text does not overflow the input queue.
    for chunk in inputs.chunks(400) {
        // SAFETY: `chunk` is a valid slice of keyboard INPUTs with the matching size.
        let sent = unsafe { SendInput(chunk, std::mem::size_of::<INPUT>() as i32) };
        if sent as usize != chunk.len() {
            return Err(format!(
                "SendInput typed only {sent} of {} events (an elevated window has focus?)",
                chunk.len()
            )
            .into());
        }
    }
    Ok(())
}

fn monitor_info(monitor: HMONITOR) -> Option<MONITORINFO> {
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: `info` is a valid out pointer with `cbSize` set.
    unsafe { GetMonitorInfoW(monitor, &mut info) }
        .as_bool()
        .then_some(info)
}

unsafe extern "system" fn collect_monitor(
    monitor: HMONITOR,
    _dc: HDC,
    _rect: *mut RECT,
    lparam: LPARAM,
) -> windows::core::BOOL {
    // SAFETY: `lparam` is the `&mut Vec<HMONITOR>` passed by `monitors()`.
    unsafe { (*(lparam.0 as *mut Vec<HMONITOR>)).push(monitor) };
    true.into()
}

/// Monitors left to right, then top to bottom.
fn monitors() -> Vec<(HMONITOR, MONITORINFO)> {
    let mut handles: Vec<HMONITOR> = Vec::new();
    // SAFETY: the callback only pushes into the Vec passed through `lparam`,
    // which outlives the call.
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(collect_monitor),
            LPARAM(&raw mut handles as isize),
        );
    }
    let mut out: Vec<(HMONITOR, MONITORINFO)> = handles
        .into_iter()
        .filter_map(|h| monitor_info(h).map(|i| (h, i)))
        .collect();
    out.sort_by_key(|(_, i)| (i.rcMonitor.left, i.rcMonitor.top));
    out
}

/// How far the window's visible frame lies inside its full rectangle: modern
/// windows have an invisible resize border, which would leave gaps when
/// two windows are snapped side by side.
fn invisible_border(hwnd: HWND, full: &RECT) -> RECT {
    let mut visible = RECT::default();
    // SAFETY: `visible` is a valid out buffer of the size passed.
    let ok = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            (&raw mut visible).cast(),
            std::mem::size_of::<RECT>() as u32,
        )
    };
    if ok.is_err() {
        return RECT::default();
    }
    RECT {
        left: visible.left - full.left,
        top: visible.top - full.top,
        right: full.right - visible.right,
        bottom: full.bottom - visible.bottom,
    }
}

fn place(hwnd: HWND, x: i32, y: i32, w: i32, h: i32) -> Result<(), ActionError> {
    let mut full = RECT::default();
    // SAFETY: valid out pointer; `hwnd` is a window handle.
    unsafe { GetWindowRect(hwnd, &mut full) }?;
    let b = invisible_border(hwnd, &full);
    // SAFETY: `hwnd` is a window handle; a stale one only makes the call fail.
    unsafe {
        SetWindowPos(
            hwnd,
            None,
            x - b.left,
            y - b.top,
            w + b.left + b.right,
            h + b.top + b.bottom,
            SWP_NOZORDER | SWP_NOACTIVATE,
        )
    }?;
    Ok(())
}

fn window_op(op: WindowOp) -> Result<(), ActionError> {
    let hwnd = target_window()?;
    match op {
        WindowOp::ToggleAlwaysOnTop => {
            // SAFETY: `hwnd` is a window handle.
            let top = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) } as u32 & WS_EX_TOPMOST.0 != 0;
            // SAFETY: as above.
            unsafe {
                SetWindowPos(
                    hwnd,
                    Some(if top { HWND_NOTOPMOST } else { HWND_TOPMOST }),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                )
            }?;
            Ok(())
        }
        WindowOp::SnapLeft | WindowOp::SnapRight => {
            // SAFETY: `hwnd` is a window handle.
            let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
            let info = monitor_info(monitor).ok_or("cannot read the monitor's size")?;
            let work = info.rcWork;
            let half = (work.right - work.left) / 2;
            let x = if op == WindowOp::SnapLeft {
                work.left
            } else {
                work.left + half
            };
            // A maximized window cannot be moved: restore it first.
            // SAFETY: as above.
            unsafe {
                if IsZoomed(hwnd).as_bool() {
                    let _ = ShowWindowAsync(hwnd, SW_RESTORE);
                    std::thread::sleep(std::time::Duration::from_millis(120));
                }
            }
            place(hwnd, x, work.top, half, work.bottom - work.top)
        }
        WindowOp::NextMonitor => {
            let all = monitors();
            if all.len() < 2 {
                return Err("there is only one monitor".into());
            }
            // SAFETY: `hwnd` is a window handle.
            let current = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
            let at = all.iter().position(|(h, _)| *h == current).unwrap_or(0);
            let from = all[at].1.rcWork;
            let to = all[(at + 1) % all.len()].1.rcWork;
            // SAFETY: as above.
            let maximized = unsafe { IsZoomed(hwnd).as_bool() };
            if maximized {
                // SAFETY: as above.
                unsafe {
                    let _ = ShowWindowAsync(hwnd, SW_RESTORE);
                }
                std::thread::sleep(std::time::Duration::from_millis(120));
            }
            let mut r = RECT::default();
            // SAFETY: valid out pointer.
            unsafe { GetWindowRect(hwnd, &mut r) }?;
            let (fw, fh) = (
                (from.right - from.left).max(1),
                (from.bottom - from.top).max(1),
            );
            let (tw, th) = (to.right - to.left, to.bottom - to.top);
            // Same relative place, same size where it fits.
            let w = (r.right - r.left).min(tw);
            let h = (r.bottom - r.top).min(th);
            let x = to.left + ((r.left - from.left) as i64 * i64::from(tw) / i64::from(fw)) as i32;
            let y = to.top + ((r.top - from.top) as i64 * i64::from(th) / i64::from(fh)) as i32;
            let x = x.clamp(to.left, to.right - w);
            let y = y.clamp(to.top, to.bottom - h);
            place(hwnd, x, y, w, h)?;
            if maximized {
                // SAFETY: as above.
                unsafe {
                    let _ = ShowWindowAsync(hwnd, SW_MAXIMIZE);
                }
            }
            Ok(())
        }
    }
}

fn vk(key: Key) -> (VIRTUAL_KEY, bool) {
    // The bool marks keys of the extended set, which need
    // KEYEVENTF_EXTENDEDKEY to be told apart from their numpad twins.
    match key {
        Key::Ctrl => (VK_LCONTROL, false),
        Key::Shift => (VK_LSHIFT, false),
        Key::Alt => (VK_LMENU, false),
        Key::Win => (VK_LWIN, true),
        Key::Letter(c) => (VIRTUAL_KEY(c as u16), false),
        Key::Digit(d) => (VIRTUAL_KEY(u16::from(b'0' + d)), false),
        Key::F(n) => (VIRTUAL_KEY(VK_F1.0 + u16::from(n) - 1), false),
        Key::Tab => (VK_TAB, false),
        Key::Enter => (VK_RETURN, false),
        Key::Escape => (VK_ESCAPE, false),
        Key::Space => (VK_SPACE, false),
        Key::Backspace => (VK_BACK, false),
        Key::Delete => (VK_DELETE, true),
        Key::Insert => (VK_INSERT, true),
        Key::Home => (VK_HOME, true),
        Key::End => (VK_END, true),
        Key::PageUp => (VK_PRIOR, true),
        Key::PageDown => (VK_NEXT, true),
        Key::Up => (VK_UP, true),
        Key::Down => (VK_DOWN, true),
        Key::Left => (VK_LEFT, true),
        Key::Right => (VK_RIGHT, true),
        Key::PrintScreen => (VK_SNAPSHOT, true),
        Key::Menu => (VK_APPS, true),
        Key::Minus => (VK_OEM_MINUS, false),
        Key::Plus => (VK_OEM_PLUS, false),
        Key::Comma => (VK_OEM_COMMA, false),
        Key::Period => (VK_OEM_PERIOD, false),
        Key::Semicolon => (VK_OEM_1, false),
        Key::Slash => (VK_OEM_2, false),
        Key::Backquote => (VK_OEM_3, false),
        Key::BracketLeft => (VK_OEM_4, false),
        Key::Backslash => (VK_OEM_5, false),
        Key::BracketRight => (VK_OEM_6, false),
        Key::Quote => (VK_OEM_7, false),
        Key::MediaPlayPause => (VK_MEDIA_PLAY_PAUSE, true),
        Key::MediaNext => (VK_MEDIA_NEXT_TRACK, true),
        Key::MediaPrevious => (VK_MEDIA_PREV_TRACK, true),
        Key::MediaStop => (VK_MEDIA_STOP, true),
        Key::VolumeUp => (VK_VOLUME_UP, true),
        Key::VolumeDown => (VK_VOLUME_DOWN, true),
        Key::VolumeMute => (VK_VOLUME_MUTE, true),
    }
}

fn key_input(key: Key, up: bool) -> INPUT {
    let (vk, extended) = vk(key);
    let mut flags = KEYBD_EVENT_FLAGS(0);
    if extended {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    if up {
        flags |= KEYEVENTF_KEYUP;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// Presses the modifiers, taps the key and releases the modifiers in
/// reverse order, in one `SendInput` call so no real keystroke can land in
/// between.
fn send_combo(combo: &KeyCombo) -> Result<(), ActionError> {
    let mut modifiers = combo.modifiers.clone();
    // The plus key shares its virtual key with `=`: without Shift a
    // `Ctrl+Plus` would arrive as `Ctrl+=`.
    if combo.key == Key::Plus && !modifiers.contains(&Key::Shift) {
        modifiers.push(Key::Shift);
    }
    let mut inputs: Vec<INPUT> = modifiers.iter().map(|&m| key_input(m, false)).collect();
    inputs.push(key_input(combo.key, false));
    inputs.push(key_input(combo.key, true));
    inputs.extend(modifiers.iter().rev().map(|&m| key_input(m, true)));
    // SAFETY: `inputs` is a valid slice of keyboard INPUTs with the matching size.
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent as usize != inputs.len() {
        // Blocked by UIPI (target runs elevated) or by the secure desktop.
        return Err(format!(
            "SendInput injected {sent} of {} events for {combo} (an elevated window has focus?)",
            inputs.len()
        )
        .into());
    }
    Ok(())
}

fn window_class(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    // SAFETY: `buf` is writable for its length.
    let n = unsafe { GetClassNameW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// The top-level window of the foreground application.
fn target_window() -> Result<HWND, ActionError> {
    // SAFETY: no arguments.
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0.is_null() {
        return Err("there is no foreground window".into());
    }
    // A dialog's root owner is the application's main window.
    // SAFETY: `hwnd` is a window handle.
    let root = unsafe { GetAncestor(hwnd, GA_ROOTOWNER) };
    let hwnd = if root.0.is_null() { hwnd } else { root };
    let class = window_class(hwnd);
    if SHELL_CLASSES.contains(&class.as_str()) {
        return Err(format!("refusing to act on the desktop or taskbar ({class})").into());
    }
    Ok(hwnd)
}

fn window_pid(hwnd: HWND) -> u32 {
    let mut pid = 0u32;
    // SAFETY: `pid` is a valid out pointer.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    pid
}

/// Starts `command`, through `cmd /C` when `shell` is set, and waits for it
/// when `wait` is. A program started without `wait` keeps running on its own.
#[allow(clippy::too_many_arguments)]
fn run(
    command: &str,
    args: &[String],
    hidden: bool,
    cwd: Option<&str>,
    shell: bool,
    env: &BTreeMap<String, String>,
    wait: bool,
    timeout_ms: u64,
) -> Result<(), ActionError> {
    let mut cmd = if shell {
        // One line, as the user would type it in a terminal: `cmd /C` parses
        // pipes, redirection, `&&` and `%VARIABLES%`. Arguments are appended,
        // quoted when they contain a space.
        let mut line = command.to_string();
        for arg in args {
            line.push(' ');
            line.push_str(&quote_for_cmd(arg));
        }
        let mut c = Command::new("cmd");
        c.arg("/C").arg(line);
        c
    } else {
        let mut c = Command::new(command);
        c.args(args);
        c
    };
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    for (key, value) in env {
        cmd.env(key, value);
    }
    if hidden {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("cannot start `{command}`: {e}"))?;
    if !wait {
        return Ok(());
    }
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    // In a chain the next step runs anyway: branching would
                    // need a condition on the exit code, which is not in the
                    // action set.
                    tracing::warn!("`{command}` exited with {status}");
                }
                return Ok(());
            }
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                return Err(format!("`{command}` did not finish within {timeout_ms} ms").into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return Err(format!("cannot wait for `{command}`: {e}").into()),
        }
    }
}

/// Quotes one argument for `cmd /C`: in double quotes when it holds a space
/// or a quote, since that is what `cmd` splits on.
fn quote_for_cmd(arg: &str) -> String {
    if arg.is_empty() || arg.contains([' ', '"']) {
        format!("\"{}\"", arg.replace('"', "\"\""))
    } else {
        arg.to_string()
    }
}

/// Puts `text` on the clipboard, replacing what was there.
fn set_clipboard_text(text: &str) -> Result<(), ActionError> {
    // UTF-16 with its terminator: what `CF_UNICODETEXT` is.
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes = wide.len() * std::mem::size_of::<u16>();
    // SAFETY: the clipboard is opened and closed within this call. The block
    // allocated by `GlobalAlloc` is either handed to the clipboard, which owns
    // it from then on, or freed here on every error path.
    unsafe {
        if OpenClipboard(None).is_err() {
            return Err("another program is holding the clipboard".into());
        }
        let result = (|| -> Result<(), ActionError> {
            EmptyClipboard().map_err(|e| format!("cannot empty the clipboard: {e}"))?;
            let memory = GlobalAlloc(GMEM_MOVEABLE, bytes)
                .map_err(|e| format!("cannot allocate for the clipboard: {e}"))?;
            let ptr = GlobalLock(memory);
            if ptr.is_null() {
                let _ = GlobalFree(Some(memory));
                return Err("cannot lock the clipboard memory".into());
            }
            std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr as *mut u16, wide.len());
            let _ = GlobalUnlock(memory);
            if let Err(e) = SetClipboardData(u32::from(CF_UNICODETEXT.0), Some(HANDLE(memory.0))) {
                let _ = GlobalFree(Some(memory));
                return Err(format!("cannot put the text on the clipboard: {e}").into());
            }
            Ok(())
        })();
        let _ = CloseClipboard();
        result
    }
}

/// The clipboard's text, for the `{{clipboard}}` placeholder. `None` when the
/// clipboard is empty, holds no text, or another program is holding it: an
/// action with the placeholder must still run.
pub fn clipboard_text() -> Option<String> {
    // SAFETY: the clipboard is opened and closed within this call; the handle
    // `GetClipboardData` returns belongs to the clipboard, not to us, so it is
    // only locked for reading and never freed.
    unsafe {
        if OpenClipboard(None).is_err() {
            return None;
        }
        let text = read_text_from_clipboard();
        let _ = CloseClipboard();
        text
    }
}

/// Reads `CF_UNICODETEXT` while the clipboard is open.
///
/// SAFETY: called with the clipboard open on this thread; `GlobalLock`'s
/// pointer is valid until the matching `GlobalUnlock`.
unsafe fn read_text_from_clipboard() -> Option<String> {
    let handle = unsafe { GetClipboardData(u32::from(CF_UNICODETEXT.0)) }.ok()?;
    if handle.is_invalid() {
        return None;
    }
    let memory = HGLOBAL(handle.0);
    let ptr = unsafe { GlobalLock(memory) };
    if ptr.is_null() {
        return None;
    }
    let text = unsafe { PWSTR(ptr as *mut u16).to_string() }.ok();
    let _ = unsafe { GlobalUnlock(memory) };
    text
}

fn close_foreground(force: bool) -> Result<(), ActionError> {
    let hwnd = target_window()?;
    let pid = window_pid(hwnd);
    let exe = process_exe(pid).unwrap_or_default();
    // Terminating explorer.exe would take the taskbar and desktop with it.
    if force && !exe.eq_ignore_ascii_case("explorer.exe") && pid != std::process::id() {
        // SAFETY: plain process API calls; the handle is closed below.
        unsafe {
            let process = OpenProcess(PROCESS_TERMINATE, false, pid)
                .map_err(|e| format!("cannot terminate {exe} (pid {pid}): {e}"))?;
            let result = TerminateProcess(process, 1);
            let _ = CloseHandle(process);
            result.map_err(|e| format!("cannot terminate {exe} (pid {pid}): {e}"))?;
        }
        return Ok(());
    }
    // SAFETY: posting a message with no pointer payload.
    unsafe { PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)) }?;
    Ok(())
}

fn process_exe(pid: u32) -> Option<String> {
    // SAFETY: plain process API calls; the handle is closed before returning.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let result = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        );
        let _ = CloseHandle(process);
        result.ok()?;
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        path.rsplit('\\').next().map(str::to_string)
    }
}

#[derive(Debug, Default)]
pub struct WindowsForeground;

impl ForegroundInfo for WindowsForeground {
    fn foreground_app(&self) -> Option<AppInfo> {
        // SAFETY: no arguments.
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.0.is_null() {
            return None;
        }
        let mut buf = [0u16; 512];
        // SAFETY: `buf` is writable for its length.
        let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
        let title = String::from_utf16_lossy(&buf[..n.max(0) as usize]);
        let pid = window_pid(hwnd);
        // SAFETY: `hwnd` is a window handle.
        let root = unsafe { GetAncestor(hwnd, GA_ROOTOWNER) };
        let window = if root.0.is_null() { hwnd } else { root };
        Some(AppInfo {
            // Elevated or protected processes may refuse even limited queries.
            exe: process_exe(pid).unwrap_or_default(),
            title,
            pid,
            window: window.0 as usize as u64,
            fullscreen: is_fullscreen(window),
        })
    }

    fn windows(&self) -> Vec<AppInfo> {
        let mut handles: Vec<HWND> = Vec::new();
        // SAFETY: the callback only pushes into the Vec passed through
        // `lparam`, which outlives the EnumWindows call.
        unsafe {
            let _ = EnumWindows(Some(collect_window), LPARAM(&raw mut handles as isize));
        }
        let mut out: Vec<AppInfo> = handles
            .into_iter()
            .filter_map(|hwnd| {
                let mut buf = [0u16; 512];
                // SAFETY: `buf` is writable for its length.
                let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
                let title = String::from_utf16_lossy(&buf[..n.max(0) as usize]);
                let pid = window_pid(hwnd);
                let exe = process_exe(pid)?;
                (!title.trim().is_empty()).then_some(AppInfo {
                    exe,
                    title,
                    pid,
                    window: hwnd.0 as usize as u64,
                    fullscreen: false,
                })
            })
            .collect();
        out.sort_by(|a, b| {
            a.exe
                .to_lowercase()
                .cmp(&b.exe.to_lowercase())
                .then(a.title.cmp(&b.title))
        });
        out
    }
}

/// Visible, unowned, non-tool windows: what the taskbar would show.
unsafe extern "system" fn collect_window(hwnd: HWND, lparam: LPARAM) -> windows::core::BOOL {
    // SAFETY: `lparam` is the `&mut Vec<HWND>` passed by `windows()`.
    unsafe {
        let ex = WINDOW_EX_STYLE(GetWindowLongW(hwnd, GWL_EXSTYLE) as u32);
        let owner = GetAncestor(hwnd, GA_ROOTOWNER);
        if IsWindowVisible(hwnd).as_bool()
            && (ex.0 & WS_EX_TOOLWINDOW.0) == 0
            && (owner.0.is_null() || owner == hwnd)
        {
            let list = &mut *(lparam.0 as *mut Vec<HWND>);
            list.push(hwnd);
        }
    }
    true.into()
}

/// The window covers its whole monitor and is not the desktop.
fn is_fullscreen(hwnd: HWND) -> bool {
    if SHELL_CLASSES.contains(&window_class(hwnd).as_str()) {
        return false;
    }
    let mut rect = RECT::default();
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: valid out pointers; `hwnd` is a window handle.
    unsafe {
        if GetWindowRect(hwnd, &mut rect).is_err() {
            return false;
        }
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return false;
        }
    }
    let m = info.rcMonitor;
    rect.left <= m.left && rect.top <= m.top && rect.right >= m.right && rect.bottom >= m.bottom
}
