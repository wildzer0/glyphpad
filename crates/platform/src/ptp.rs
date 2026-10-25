//! OS-independent Precision Touchpad logic: HID unit conversion, frame
//! assembly across reports (hybrid mode) and Scan Time handling.
//!
//! The Windows backend extracts raw values with the HID parser and feeds them
//! here; keeping this part free of FFI makes it testable on every platform.

use glyphpad_core::{Contact, MAX_CONTACTS, TouchFrame};

/// HID usages read from a Precision Touchpad (SPEC §4).
pub mod usage {
    pub const PAGE_GENERIC_DESKTOP: u16 = 0x01;
    pub const PAGE_BUTTON: u16 = 0x09;
    pub const PAGE_DIGITIZER: u16 = 0x0D;

    pub const X: u16 = 0x30;
    pub const Y: u16 = 0x31;

    pub const TOUCH_PAD: u16 = 0x05;
    pub const FINGER: u16 = 0x22;
    pub const TIP_SWITCH: u16 = 0x42;
    pub const CONFIDENCE: u16 = 0x47;
    pub const CONTACT_ID: u16 = 0x51;
    pub const CONTACT_COUNT: u16 = 0x54;
    pub const CONTACT_COUNT_MAXIMUM: u16 = 0x55;
    pub const SCAN_TIME: u16 = 0x56;

    pub const BUTTON_1: u16 = 0x01;
}

/// Decodes a HID unit exponent.
///
/// The HID spec stores it as a 4-bit two's complement nibble (0xE = -2), but
/// some descriptors put a full signed byte (0xFE = -2) in the item; both forms
/// are accepted.
pub fn unit_exponent(raw: u32) -> i32 {
    if raw <= 0xF {
        if raw >= 0x8 {
            raw as i32 - 16
        } else {
            raw as i32
        }
    } else {
        i32::from((raw & 0xFF) as u8 as i8)
    }
}

/// Millimeters in one unit of a HID length `Unit` code, if it is a length.
///
/// The low nibble is the unit system (1 = SI linear, centimeters; 3 = English
/// linear, inches) and the next nibble the exponent of the length dimension,
/// which must be 1 for a plain distance.
pub fn length_unit_mm(units: u32) -> Option<f64> {
    if (units >> 4) & 0xF != 1 || units >> 8 != 0 {
        return None;
    }
    match units & 0xF {
        0x1 => Some(10.0),
        0x3 => Some(25.4),
        _ => None,
    }
}

/// Range of one axis as declared by the HID descriptor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AxisCaps {
    pub logical_min: i32,
    pub logical_max: i32,
    pub physical_min: i32,
    pub physical_max: i32,
    pub units: u32,
    pub units_exp: u32,
}

impl AxisCaps {
    /// Physical length of the axis in millimeters, or `None` when the
    /// descriptor does not declare physical units (physical min = max = 0
    /// means "same as logical" in HID, which carries no size information).
    pub fn extent_mm(&self) -> Option<f32> {
        if self.physical_min == 0 && self.physical_max == 0 {
            return None;
        }
        let unit_mm = length_unit_mm(self.units)?;
        let span = f64::from(self.physical_max) - f64::from(self.physical_min);
        let mm = span * 10f64.powi(unit_exponent(self.units_exp)) * unit_mm;
        (mm > 0.0).then_some(mm as f32)
    }

    /// Position normalized to 0..1 over the logical range, clamped.
    pub fn normalize(&self, raw: i32) -> f32 {
        let span = f64::from(self.logical_max) - f64::from(self.logical_min);
        if span <= 0.0 {
            return 0.0;
        }
        ((f64::from(raw) - f64::from(self.logical_min)) / span).clamp(0.0, 1.0) as f32
    }

    /// Sign-extends a raw value of `bit_size` bits when the logical range is
    /// signed, since the HID parser returns the bits unextended.
    pub fn decode(&self, raw: u32, bit_size: u16) -> i32 {
        if self.logical_min < 0 && bit_size > 0 && bit_size < 32 {
            let shift = 32 - u32::from(bit_size);
            ((raw << shift) as i32) >> shift
        } else {
            raw as i32
        }
    }
}

/// One finger slot read from a report.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RawContact {
    pub id: u32,
    pub x: i32,
    pub y: i32,
    pub tip: bool,
    pub confident: bool,
}

/// The values of one HID input report.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RawReport {
    /// `None` when the device has no Contact Count usage.
    pub contact_count: Option<u32>,
    pub scan_time: Option<u16>,
    pub button: bool,
    /// Every finger slot of the report, in link collection order; slots past
    /// the contact count hold stale data.
    pub contacts: Vec<RawContact>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RawFrame {
    pub scan_time: Option<u16>,
    pub button: bool,
    pub contacts: Vec<RawContact>,
}

#[derive(Debug)]
struct Pending {
    expected: usize,
    frame: RawFrame,
}

/// Rebuilds complete frames from input reports.
///
/// In *hybrid mode* a device with more contacts than finger slots per report
/// spreads one frame over several reports: only the first carries the real
/// Contact Count, the following ones report 0 and share its Scan Time.
#[derive(Debug, Default)]
pub struct FrameAssembler {
    pending: Option<Pending>,
    last_button: bool,
    /// The last emitted frame had fingers down, so a report that says "no
    /// contacts" is the lift that ends the touch rather than stray noise.
    last_touched: bool,
}

impl FrameAssembler {
    pub fn push(&mut self, report: RawReport) -> Option<RawFrame> {
        let RawReport {
            contact_count,
            scan_time,
            button,
            contacts,
        } = report;

        let Some(count) = contact_count else {
            // No Contact Count usage: every report is a whole frame and only
            // slots with the tip down are meaningful.
            self.pending = None;
            return Some(self.emit(RawFrame {
                scan_time,
                button,
                contacts: contacts.into_iter().filter(|c| c.tip).collect(),
            }));
        };

        if count > 0 {
            if self.pending.is_some() {
                tracing::debug!("incomplete touch frame discarded: a new frame started");
            }
            self.pending = Some(Pending {
                expected: (count as usize).min(MAX_CONTACTS),
                frame: RawFrame {
                    scan_time,
                    button,
                    contacts: Vec::with_capacity(count as usize),
                },
            });
        } else {
            match &mut self.pending {
                Some(p) if p.frame.scan_time == scan_time => p.frame.button |= button,
                Some(_) => {
                    tracing::debug!("incomplete touch frame discarded: scan time changed");
                    self.pending = None;
                    return None;
                }
                None => {
                    // A stray empty report. A change of the physical button,
                    // or the end of a touch that was still on the surface in
                    // the last frame, is worth a frame; anything else would
                    // look like all fingers lifting.
                    if button == self.last_button && !self.last_touched {
                        return None;
                    }
                    return Some(self.emit(RawFrame {
                        scan_time,
                        button,
                        contacts: Vec::new(),
                    }));
                }
            }
        }

        let p = self.pending.as_mut()?;
        let missing = p.expected - p.frame.contacts.len();
        p.frame.contacts.extend(contacts.into_iter().take(missing));
        if p.frame.contacts.len() < p.expected {
            return None;
        }
        let frame = self.pending.take()?.frame;
        Some(self.emit(frame))
    }

    fn emit(&mut self, frame: RawFrame) -> RawFrame {
        self.last_button = frame.button;
        self.last_touched = frame.contacts.iter().any(|c| c.tip);
        frame
    }
}

/// Builds a monotonic microsecond clock from the 16-bit HID Scan Time.
///
/// Scan Time counts in 100 µs units and wraps every 6.5535 s, and devices
/// reset it after an idle period, so it is only trusted for the interval
/// between two frames when it agrees with the host clock. Otherwise the host
/// interval is used. This keeps the device's precise inter-frame timing while
/// staying monotonic across wraps, resets and long pauses.
#[derive(Debug, Default)]
pub struct ScanClock {
    last: Option<(Option<u16>, u64, u64)>,
}

/// Largest disagreement between Scan Time and host intervals still blamed on
/// message-queue jitter rather than a wrap or reset.
const SCAN_HOST_TOLERANCE_US: u64 = 20_000;

impl ScanClock {
    /// Timestamp of a frame received at `host_us` (any monotonic host clock).
    pub fn timestamp(&mut self, scan_time: Option<u16>, host_us: u64) -> u64 {
        let ts = match self.last {
            None => host_us,
            Some((last_scan, last_host, last_ts)) => {
                let host_delta = host_us.saturating_sub(last_host);
                let delta = match (scan_time, last_scan) {
                    (Some(now), Some(prev)) => {
                        let scan_delta = u64::from(now.wrapping_sub(prev)) * 100;
                        if scan_delta.abs_diff(host_delta) <= SCAN_HOST_TOLERANCE_US {
                            scan_delta
                        } else {
                            host_delta
                        }
                    }
                    _ => host_delta,
                };
                last_ts + delta
            }
        };
        self.last = Some((scan_time, host_us, ts));
        ts
    }
}

/// Converts raw logical coordinates to the normalized model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Geometry {
    pub x: AxisCaps,
    pub y: AxisCaps,
    pub width_mm: f32,
    pub height_mm: f32,
}

impl Geometry {
    pub fn frame(&self, raw: &RawFrame, timestamp_us: u64) -> TouchFrame {
        TouchFrame {
            timestamp_us,
            button_pressed: raw.button,
            contacts: raw
                .contacts
                .iter()
                .map(|c| {
                    let x_norm = self.x.normalize(c.x);
                    let y_norm = self.y.normalize(c.y);
                    Contact {
                        id: c.id,
                        x_mm: x_norm * self.width_mm,
                        y_mm: y_norm * self.height_mm,
                        x_norm,
                        y_norm,
                        tip: c.tip,
                        confident: c.confident,
                    }
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_exponents() {
        assert_eq!(unit_exponent(0x0), 0);
        assert_eq!(unit_exponent(0x7), 7);
        assert_eq!(unit_exponent(0x8), -8);
        assert_eq!(unit_exponent(0xE), -2);
        assert_eq!(unit_exponent(0xFE), -2);
    }

    #[test]
    fn length_units() {
        assert_eq!(length_unit_mm(0x11), Some(10.0));
        assert_eq!(length_unit_mm(0x13), Some(25.4));
        assert_eq!(length_unit_mm(0x00), None);
        assert_eq!(length_unit_mm(0x12), None); // SI rotation (radians)
        assert_eq!(length_unit_mm(0x21), None); // area, not length
        assert_eq!(length_unit_mm(0x1011), None); // has a time dimension
    }

    fn axis(physical_max: i32, units: u32, units_exp: u32) -> AxisCaps {
        AxisCaps {
            logical_min: 0,
            logical_max: 4000,
            physical_min: 0,
            physical_max,
            units,
            units_exp,
        }
    }

    #[test]
    fn extent_in_centimeters_and_inches() {
        // 1050 × 10^-2 cm = 105 mm, the typical PTP encoding.
        let mm = axis(1050, 0x11, 0xE).extent_mm().unwrap();
        assert!((mm - 105.0).abs() < 1e-3);
        // 413 × 10^-2 in = 104.9 mm.
        let mm = axis(413, 0x13, 0xE).extent_mm().unwrap();
        assert!((mm - 104.902).abs() < 1e-3);
        assert_eq!(axis(0, 0x11, 0xE).extent_mm(), None);
        assert_eq!(axis(1050, 0, 0).extent_mm(), None);
    }

    #[test]
    fn normalize_and_decode() {
        let a = axis(1050, 0x11, 0xE);
        assert_eq!(a.normalize(0), 0.0);
        assert_eq!(a.normalize(2000), 0.5);
        assert_eq!(a.normalize(5000), 1.0);
        let signed = AxisCaps {
            logical_min: -100,
            ..a
        };
        assert_eq!(signed.decode(0xFFF, 12), -1);
        assert_eq!(a.decode(0xFFF, 12), 4095);
    }

    fn c(id: u32, tip: bool) -> RawContact {
        RawContact {
            id,
            x: id as i32 * 100,
            y: 50,
            tip,
            confident: true,
        }
    }

    fn report(count: u32, scan: u16, contacts: Vec<RawContact>) -> RawReport {
        RawReport {
            contact_count: Some(count),
            scan_time: Some(scan),
            button: false,
            contacts,
        }
    }

    #[test]
    fn parallel_mode_ignores_stale_slots() {
        let mut asm = FrameAssembler::default();
        let frame = asm
            .push(report(
                2,
                10,
                vec![
                    c(0, true),
                    c(1, true),
                    c(7, false),
                    c(8, false),
                    c(9, false),
                ],
            ))
            .unwrap();
        assert_eq!(frame.contacts, vec![c(0, true), c(1, true)]);
    }

    #[test]
    fn hybrid_mode_accumulates_reports() {
        // 5 contacts over reports with 2 finger slots each.
        let mut asm = FrameAssembler::default();
        assert!(
            asm.push(report(5, 10, vec![c(0, true), c(1, true)]))
                .is_none()
        );
        assert!(
            asm.push(report(0, 10, vec![c(2, true), c(3, true)]))
                .is_none()
        );
        let frame = asm
            .push(report(0, 10, vec![c(4, true), c(5, false)]))
            .unwrap();
        let ids: Vec<u32> = frame.contacts.iter().map(|c| c.id).collect();
        assert_eq!(ids, [0, 1, 2, 3, 4]);
        assert_eq!(frame.scan_time, Some(10));
    }

    #[test]
    fn hybrid_frame_broken_by_new_scan_is_dropped() {
        let mut asm = FrameAssembler::default();
        assert!(
            asm.push(report(3, 10, vec![c(0, true), c(1, true)]))
                .is_none()
        );
        // Continuation lost; a continuation of another scan must not complete it.
        assert!(
            asm.push(report(0, 11, vec![c(2, true), c(3, true)]))
                .is_none()
        );
        // A new frame still works.
        let frame = asm
            .push(report(1, 12, vec![c(0, true), c(1, true)]))
            .unwrap();
        assert_eq!(frame.contacts, vec![c(0, true)]);
    }

    #[test]
    fn new_frame_replaces_incomplete_one() {
        let mut asm = FrameAssembler::default();
        assert!(
            asm.push(report(4, 10, vec![c(0, true), c(1, true)]))
                .is_none()
        );
        let frame = asm
            .push(report(2, 11, vec![c(0, true), c(1, true)]))
            .unwrap();
        assert_eq!(frame.contacts.len(), 2);
    }

    #[test]
    fn a_report_without_contacts_ends_a_touch_in_progress() {
        // Some PTPs report the lift as Contact Count 0 without repeating the
        // finger slots; dropping that report would leave the engine thinking
        // the touch never ended.
        let mut asm = FrameAssembler::default();
        assert!(asm.push(report(0, 1, vec![])).is_none(), "nothing to end");
        assert_eq!(
            asm.push(report(1, 2, vec![c(0, true)]))
                .map(|f| f.contacts.len()),
            Some(1)
        );
        let lift = asm.push(report(0, 3, vec![])).expect("the lift frame");
        assert!(lift.contacts.is_empty());
        // Once emitted, further empty reports are noise again.
        assert!(asm.push(report(0, 4, vec![])).is_none());
        assert!(asm.push(report(0, 5, vec![])).is_none());
    }

    #[test]
    fn a_lift_frame_that_clears_the_tips_is_not_followed_by_a_second_one() {
        // The reference touchpad reports the lift as a frame whose slots all
        // have Tip Switch 0, then a Contact Count 0 report.
        let mut asm = FrameAssembler::default();
        asm.push(report(1, 1, vec![c(0, true)]));
        let lift = asm
            .push(report(1, 2, vec![c(0, false)]))
            .expect("lift frame");
        assert_eq!(lift.contacts.len(), 1);
        assert!(asm.push(report(0, 3, vec![c(0, false)])).is_none());
    }

    #[test]
    fn stray_empty_reports_only_carry_button_changes() {
        let mut asm = FrameAssembler::default();
        assert!(asm.push(report(0, 1, vec![c(0, false)])).is_none());
        let mut pressed = report(0, 2, vec![]);
        pressed.button = true;
        let frame = asm.push(pressed.clone()).unwrap();
        assert!(frame.button && frame.contacts.is_empty());
        assert!(asm.push(pressed).is_none());
    }

    #[test]
    fn button_in_any_report_of_the_frame() {
        let mut asm = FrameAssembler::default();
        assert!(
            asm.push(report(3, 5, vec![c(0, true), c(1, true)]))
                .is_none()
        );
        let mut second = report(0, 5, vec![c(2, true), c(3, true)]);
        second.button = true;
        assert!(asm.push(second).unwrap().button);
    }

    #[test]
    fn without_contact_count_tip_selects_slots() {
        let mut asm = FrameAssembler::default();
        let frame = asm
            .push(RawReport {
                contact_count: None,
                scan_time: None,
                button: false,
                contacts: vec![c(0, true), c(1, false), c(2, true)],
            })
            .unwrap();
        assert_eq!(frame.contacts, vec![c(0, true), c(2, true)]);
    }

    #[test]
    fn scan_clock_uses_device_timing() {
        let mut clock = ScanClock::default();
        assert_eq!(clock.timestamp(Some(100), 1_000_000), 1_000_000);
        // Device says 8.0 ms, host saw 9.5 ms of queue jitter: trust the device.
        assert_eq!(clock.timestamp(Some(180), 1_009_500), 1_008_000);
    }

    #[test]
    fn scan_clock_handles_wrap() {
        let mut clock = ScanClock::default();
        clock.timestamp(Some(65_500), 0);
        // 65500 -> 44 wraps: 80 units = 8 ms.
        assert_eq!(clock.timestamp(Some(44), 8_000), 8_000);
    }

    #[test]
    fn scan_clock_falls_back_to_host_after_reset_or_idle() {
        let mut clock = ScanClock::default();
        clock.timestamp(Some(5_000), 0);
        // Device reset its counter after 2 s of idle.
        assert_eq!(clock.timestamp(Some(0), 2_000_000), 2_000_000);
        // Long pause, more than a full wrap: scan delta is meaningless.
        assert_eq!(clock.timestamp(Some(80), 12_000_000), 12_000_000);
        // No scan time at all.
        assert_eq!(clock.timestamp(None, 12_010_000), 12_010_000);
    }

    #[test]
    fn scan_clock_is_monotonic() {
        let mut clock = ScanClock::default();
        let mut prev = 0;
        for (i, scan) in [10u16, 90, 85, 170, 0, 65_535, 3].into_iter().enumerate() {
            let ts = clock.timestamp(Some(scan), i as u64 * 8_000);
            assert!(ts >= prev);
            prev = ts;
        }
    }

    #[test]
    fn geometry_maps_to_millimeters() {
        let g = Geometry {
            x: axis(1050, 0x11, 0xE),
            y: AxisCaps {
                logical_max: 2000,
                ..axis(600, 0x11, 0xE)
            },
            width_mm: 105.0,
            height_mm: 60.0,
        };
        let raw = RawFrame {
            scan_time: None,
            button: true,
            contacts: vec![RawContact {
                id: 3,
                x: 2000,
                y: 500,
                tip: true,
                confident: false,
            }],
        };
        let f = g.frame(&raw, 42);
        assert_eq!(f.timestamp_us, 42);
        assert!(f.button_pressed);
        let c = f.contacts[0];
        assert_eq!((c.id, c.tip, c.confident), (3, true, false));
        assert!((c.x_mm - 52.5).abs() < 1e-4 && (c.y_mm - 15.0).abs() < 1e-4);
        assert_eq!((c.x_norm, c.y_norm), (0.5, 0.25));
    }
}
