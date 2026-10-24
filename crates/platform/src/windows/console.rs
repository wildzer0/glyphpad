//! Console attachment for the GUI-subsystem release binary (SPEC §10.4).

use windows::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE, HANDLE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Console::{
    ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE,
    SetStdHandle,
};
use windows::core::w;

/// Attaches to the console of the parent process (the shell that started
/// us), so CLI subcommands can print although the release binary has no
/// console of its own. Returns `false` when there is no parent console, e.g.
/// when started from Explorer.
///
/// Known limitation: the shell does not wait for a GUI-subsystem program, so
/// its prompt may appear before our output.
pub fn attach_parent_console() -> bool {
    // SAFETY: AttachConsole has no pointer arguments.
    if unsafe { AttachConsole(ATTACH_PARENT_PROCESS) }.is_err() {
        return false;
    }
    for which in [STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
        // Keep handles inherited from a redirection (`> file`, pipes); only
        // missing ones are pointed at the newly attached console.
        // SAFETY: GetStdHandle has no pointer arguments.
        let current = unsafe { GetStdHandle(which) }.unwrap_or_default();
        if !current.is_invalid() && !current.0.is_null() {
            continue;
        }
        // SAFETY: static NUL-terminated name; the handle stays open for the process lifetime.
        let console: Option<HANDLE> = unsafe {
            CreateFileW(
                w!("CONOUT$"),
                GENERIC_READ.0 | GENERIC_WRITE.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                None,
            )
        }
        .ok();
        if let Some(h) = console {
            // SAFETY: `h` is a valid console handle.
            let _ = unsafe { SetStdHandle(which, h) };
        }
    }
    true
}
