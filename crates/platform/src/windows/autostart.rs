//! Starting with Windows (SPEC §10.4): a value under the current user's
//! `Run` key, which needs no administrator rights.

use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
};
use windows::core::{HSTRING, w};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE: &str = "GlyphPad";

/// The command line registered to run at sign-in, if any.
pub fn current() -> Option<String> {
    let mut buf = [0u16; 1024];
    let mut size = std::mem::size_of_val(&buf) as u32;
    // SAFETY: `buf` and `size` describe a writable buffer.
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(RUN_KEY),
            &HSTRING::from(VALUE),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if r.is_err() {
        return None;
    }
    let len = (size as usize / 2).saturating_sub(1).min(buf.len());
    Some(String::from_utf16_lossy(&buf[..len]))
}

/// Registers `command_line` to run at sign-in, or removes it.
pub fn set(command_line: Option<&str>) -> Result<(), String> {
    // SAFETY: NUL-terminated strings that outlive the calls.
    unsafe {
        match command_line {
            Some(cmd) => {
                let wide: Vec<u16> = cmd.encode_utf16().chain(Some(0)).collect();
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    &HSTRING::from(RUN_KEY),
                    w!("GlyphPad"),
                    REG_SZ.0,
                    Some(wide.as_ptr().cast()),
                    (wide.len() * 2) as u32,
                )
                .ok()
                .map_err(|e| e.to_string())
            }
            None => {
                let r =
                    RegDeleteKeyValueW(HKEY_CURRENT_USER, &HSTRING::from(RUN_KEY), w!("GlyphPad"));
                if r.is_ok() || r == ERROR_FILE_NOT_FOUND {
                    Ok(())
                } else {
                    Err(r.to_hresult().message())
                }
            }
        }
    }
}
