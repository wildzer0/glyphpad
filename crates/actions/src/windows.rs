//! Windows actions (SPEC §4): `SendInput` for keys and media keys,
//! `LockWorkStation`, and window management on the foreground window.
//!
//! Platform FFI module, the only kind allowed to use `unsafe` (SPEC §2).
#![allow(unsafe_code)]

use std::os::windows::process::CommandExt;
use std::process::Command;

use glyphpad_core::action::{
    Action, ActionError, ActionExecutor, AppInfo, ForegroundInfo, MediaKey,
};
use glyphpad_core::keys::{Key, KeyCombo};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
};
use windows::Win32::System::Shutdown::LockWorkStation;
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
    QueryFullProcessImageNameW, TerminateProcess,
};
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GA_ROOTOWNER, GWL_EXSTYLE, GetAncestor, GetClassNameW, GetForegroundWindow,
    GetWindowLongW, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
    IsZoomed, PostMessageW, SW_MAXIMIZE, SW_MINIMIZE, SW_RESTORE, ShowWindowAsync, WINDOW_EX_STYLE,
    WM_CLOSE, WS_EX_TOOLWINDOW,
};
use windows::core::PWSTR;

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
            } => {
                let mut cmd = Command::new(command);
                cmd.args(args);
                if let Some(dir) = cwd {
                    cmd.current_dir(dir);
                }
                if *hidden {
                    cmd.creation_flags(CREATE_NO_WINDOW);
                }
                // Detached: the child keeps running on its own.
                cmd.spawn()
                    .map_err(|e| format!("cannot start `{command}`: {e}"))?;
                Ok(())
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
            other => Err(format!("`{}` is not a platform action", other.kind()).into()),
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
    let mut inputs: Vec<INPUT> = combo
        .modifiers
        .iter()
        .map(|&m| key_input(m, false))
        .collect();
    inputs.push(key_input(combo.key, false));
    inputs.push(key_input(combo.key, true));
    inputs.extend(combo.modifiers.iter().rev().map(|&m| key_input(m, true)));
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
