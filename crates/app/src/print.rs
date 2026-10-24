//! Compact text rendering of touch events for `debug` and `replay`.

use std::fmt::Write as _;

use glyphpad_core::{DeviceEvent, TouchFrame};

pub fn event(event: &DeviceEvent) {
    println!("{}", format_event(event));
}

pub fn format_event(event: &DeviceEvent) -> String {
    match event {
        DeviceEvent::Connected(d) => format!(
            "[device {}] connected: {} ({:04X}:{:04X}), {:.1}×{:.1} mm, max {} contacts",
            d.id.0, d.name, d.vendor_id, d.product_id, d.width_mm, d.height_mm, d.max_contacts
        ),
        DeviceEvent::Disconnected { device } => format!("[device {}] disconnected", device.0),
        DeviceEvent::Frame { device, frame } => {
            format!("[device {}] {}", device.0, format_frame(frame))
        }
    }
}

/// One line per frame: time, click, and per contact `#id x,y` in mm with
/// flags `T` (tip down, `-` when lifting) and `P` (palm, not confident).
pub fn format_frame(frame: &TouchFrame) -> String {
    let mut out = format!(
        "{:>10.3} s {} n={}",
        frame.timestamp_us as f64 / 1e6,
        if frame.button_pressed {
            "CLICK"
        } else {
            "     "
        },
        frame.contacts.iter().filter(|c| c.tip).count()
    );
    for c in &frame.contacts {
        let _ = write!(
            out,
            " | #{} {:6.1},{:5.1} {}{}",
            c.id,
            c.x_mm,
            c.y_mm,
            if c.tip { 'T' } else { '-' },
            if c.confident { ' ' } else { 'P' }
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use glyphpad_core::Contact;

    #[test]
    fn frame_line() {
        let frame = TouchFrame {
            timestamp_us: 1_234_567,
            button_pressed: true,
            contacts: vec![Contact {
                id: 2,
                x_mm: 12.34,
                y_mm: 5.0,
                x_norm: 0.1,
                y_norm: 0.1,
                tip: true,
                confident: false,
            }],
        };
        assert_eq!(
            format_frame(&frame),
            "     1.235 s CLICK n=1 | #2   12.3,  5.0 TP"
        );
    }
}
