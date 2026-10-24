//! Normalized touch data model produced by every backend (SPEC §3.1).

use serde::{Deserialize, Serialize};

/// Maximum number of simultaneous contacts the model supports.
pub const MAX_CONTACTS: usize = 10;

/// Identifier of a touch device, unique for the lifetime of one source.
///
/// Backends assign it on connection; a device that is unplugged and plugged
/// back in may get a new id. Use [`DeviceInfo::path`] to recognize it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DeviceId(pub u32);

/// One complete sample of the touchpad: every contact reported in a scan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TouchFrame {
    /// Monotonic timestamp in microseconds, derived from the HID Scan Time when available.
    pub timestamp_us: u64,
    /// Contacts in this frame, including the ones being lifted (`tip == false`).
    pub contacts: Vec<Contact>,
    /// Physical click of the touchpad.
    pub button_pressed: bool,
}

impl TouchFrame {
    /// Contacts that are touching the surface and are not palms.
    pub fn active_contacts(&self) -> impl Iterator<Item = &Contact> {
        self.contacts.iter().filter(|c| c.tip && c.confident)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Contact {
    /// Stable id of the finger while it stays on the surface.
    pub id: u32,
    /// Position in millimeters from the top-left corner of the surface.
    pub x_mm: f32,
    pub y_mm: f32,
    /// Position normalized to 0..1 over the logical range.
    pub x_norm: f32,
    pub y_norm: f32,
    /// The finger touches the surface.
    pub tip: bool,
    /// `false` for palms and accidental contacts.
    pub confident: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub id: DeviceId,
    /// OS path of the device (e.g. the Raw Input device name on Windows).
    pub path: String,
    pub name: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub width_mm: f32,
    pub height_mm: f32,
    /// Number of simultaneous contacts the device reports.
    pub max_contacts: u8,
}

/// What a [`TouchSource`](crate::source::TouchSource) emits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DeviceEvent {
    Connected(DeviceInfo),
    Disconnected { device: DeviceId },
    Frame { device: DeviceId, frame: TouchFrame },
}
