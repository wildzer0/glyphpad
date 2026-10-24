//! Windows backend: Raw Input and the HID parser (SPEC §4).
//!
//! This is a platform FFI module, the only kind allowed to use `unsafe`
//! (SPEC §2). Every `unsafe` block states why it is sound.
#![allow(unsafe_code)]

pub mod console;
pub mod cursor;
mod device;
mod hid;
pub mod native_gestures;
pub mod overlay;
pub mod single_instance;
mod source;

pub use hid::MaxContactsSource;
pub use source::{RawInputSource, WindowKind};

use std::fmt::Write as _;

/// Text report of every Precision Touchpad's HID capabilities and of what
/// GlyphPad derives from them, for `glyphpad dump-descriptor`.
pub fn dump_descriptors() -> String {
    let mut out = String::new();
    let all_hid = device::hid_device_handles();
    let pads = device::touchpads();
    let _ = writeln!(
        out,
        "{} HID device(s), {} Precision Touchpad(s)\n",
        all_hid.len(),
        pads.len()
    );
    // Touchpads the layout parser rejected are still worth describing.
    for &handle in &all_hid {
        if !device::is_touchpad(handle) || pads.iter().any(|(h, _)| *h == handle) {
            continue;
        }
        let _ = writeln!(
            out,
            "Touchpad collection not usable: {}",
            device::device_name(handle).unwrap_or_default()
        );
        if let Some(pd) = device::preparsed_data(handle) {
            out.push_str(&hid::describe(&pd));
        }
        out.push('\n');
    }
    for (_, pad) in &pads {
        let l = &pad.layout;
        let _ = writeln!(out, "Touchpad: {}", pad.name);
        let _ = writeln!(out, "  path: {}", pad.path);
        let _ = writeln!(
            out,
            "  VID:PID {:04X}:{:04X}",
            pad.vendor_id, pad.product_id
        );
        let _ = writeln!(
            out,
            "  size: {:.2} × {:.2} mm{}",
            pad.geometry.width_mm,
            pad.geometry.height_mm,
            if pad.size_guessed {
                " (GUESSED: no physical units in descriptor)"
            } else {
                ""
            }
        );
        let _ = writeln!(
            out,
            "  max contacts: {} (source: {:?})",
            pad.max_contacts, pad.max_contacts_source
        );
        let _ = writeln!(
            out,
            "  finger slots per report: {} (link collections {:?}){}",
            l.fingers.len(),
            l.fingers,
            if usize::from(pad.max_contacts) > l.fingers.len() {
                " -> hybrid reporting mode"
            } else {
                ""
            }
        );
        let _ = writeln!(
            out,
            "  X logical {}..{}  Y logical {}..{}",
            l.x.logical_min, l.x.logical_max, l.y.logical_min, l.y.logical_max
        );
        let _ = writeln!(
            out,
            "  device handle access: {}",
            if pad.opened_rw {
                "read/write"
            } else {
                "none (zero-access handle; feature reports may still be readable)"
            }
        );
        out.push_str(&hid::describe(&pad.preparsed));
        out.push('\n');
    }
    out
}
