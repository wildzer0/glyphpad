//! Windows' own multi-finger touchpad gestures, which run alongside ours
//! (Raw Input cannot suppress them), so GlyphPad warns when they are on for
//! the finger count used to draw (SPEC §5).

use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows::core::HSTRING;

/// Values under this key mirror Settings → Bluetooth & devices → Touchpad.
/// Observed on Windows 11: `ThreeFingerSlideEnabled` and
/// `FourFingerSlideEnabled` are 0 once the swipe gestures are set to
/// "Nothing", and the `…TapEnabled` values do not exist until the tap
/// setting is changed, while the default (search, notifications) is on. So a
/// missing value counts as enabled and 0 as disabled, the same for taps by
/// analogy (to be confirmed on hardware).
const KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\PrecisionTouchPad";

fn dword(name: &str) -> Option<u32> {
    let mut data = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    // SAFETY: `data` and `size` are valid out pointers for a DWORD.
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(KEY),
            &HSTRING::from(name),
            RRF_RT_REG_DWORD,
            None,
            Some((&raw mut data).cast()),
            Some(&mut size),
        )
    };
    r.is_ok().then_some(data)
}

/// Native gestures enabled for `fingers` fingers, e.g. `["swipes", "taps"]`.
/// Only 3 and 4 fingers have native gestures.
pub fn enabled_for(fingers: u8) -> Vec<&'static str> {
    let prefix = match fingers {
        3 => "Three",
        4 => "Four",
        _ => return Vec::new(),
    };
    [
        ("swipes", "FingerSlideEnabled"),
        ("taps", "FingerTapEnabled"),
    ]
    .into_iter()
    .filter(|(_, suffix)| dword(&format!("{prefix}{suffix}")) != Some(0))
    .map(|(what, _)| what)
    .collect()
}
