//! Raw Input device enumeration and identification.

use windows::Win32::Devices::HumanInterfaceDevice::{
    HidD_GetFeature, HidD_GetManufacturerString, HidD_GetProductString,
};
use windows::Win32::Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::UI::Input::{
    GetRawInputDeviceInfoW, GetRawInputDeviceList, RAWINPUTDEVICELIST, RID_DEVICE_INFO,
    RIDI_DEVICEINFO, RIDI_DEVICENAME, RIDI_PREPARSEDDATA, RIM_TYPEHID,
};
use windows::core::PCWSTR;

use super::hid::{MaxContactsSource, PreparsedData, TouchpadLayout};
use crate::ptp::{Geometry, usage};

/// `GetRawInputDeviceInfoW` and `GetRawInputDeviceList` return `(UINT)-1` on error.
const RAW_INPUT_ERROR: u32 = u32::MAX;

/// Handles of every HID device known to Raw Input.
pub fn hid_device_handles() -> Vec<HANDLE> {
    let size = std::mem::size_of::<RAWINPUTDEVICELIST>() as u32;
    let mut count = 0u32;
    // SAFETY: a null list with a valid count pointer queries the number of devices.
    if unsafe { GetRawInputDeviceList(None, &mut count, size) } == RAW_INPUT_ERROR {
        return Vec::new();
    }
    // Devices can appear between the two calls; leave some headroom.
    let mut list = vec![RAWINPUTDEVICELIST::default(); count as usize + 8];
    let mut len = list.len() as u32;
    // SAFETY: `list` has room for `len` entries.
    let n = unsafe { GetRawInputDeviceList(Some(list.as_mut_ptr()), &mut len, size) };
    if n == RAW_INPUT_ERROR {
        return Vec::new();
    }
    list.truncate(n as usize);
    list.into_iter()
        .filter(|d| d.dwType == RIM_TYPEHID)
        .map(|d| d.hDevice)
        .collect()
}

pub fn device_name(device: HANDLE) -> Option<String> {
    let mut chars = 0u32;
    // SAFETY: a null buffer queries the required size, in characters for RIDI_DEVICENAME.
    unsafe { GetRawInputDeviceInfoW(Some(device), RIDI_DEVICENAME, None, &mut chars) };
    if chars == 0 {
        return None;
    }
    let mut buf = vec![0u16; chars as usize];
    // SAFETY: `buf` holds `chars` UTF-16 units.
    let n = unsafe {
        GetRawInputDeviceInfoW(
            Some(device),
            RIDI_DEVICENAME,
            Some(buf.as_mut_ptr().cast()),
            &mut chars,
        )
    };
    if n == RAW_INPUT_ERROR {
        return None;
    }
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..end]))
}

/// Vendor id, product id, usage page and usage of a HID device.
pub fn hid_info(device: HANDLE) -> Option<(u16, u16, u16, u16)> {
    let mut info = RID_DEVICE_INFO {
        cbSize: std::mem::size_of::<RID_DEVICE_INFO>() as u32,
        ..Default::default()
    };
    let mut size = info.cbSize;
    // SAFETY: `info` is a correctly sized RID_DEVICE_INFO with `cbSize` set.
    let n = unsafe {
        GetRawInputDeviceInfoW(
            Some(device),
            RIDI_DEVICEINFO,
            Some((&raw mut info).cast()),
            &mut size,
        )
    };
    if n == RAW_INPUT_ERROR || info.dwType != RIM_TYPEHID {
        return None;
    }
    // SAFETY: `dwType == RIM_TYPEHID` selects the `hid` variant.
    let hid = unsafe { info.Anonymous.hid };
    Some((
        hid.dwVendorId as u16,
        hid.dwProductId as u16,
        hid.usUsagePage,
        hid.usUsage,
    ))
}

pub fn is_touchpad(device: HANDLE) -> bool {
    hid_info(device)
        .is_some_and(|(_, _, page, u)| page == usage::PAGE_DIGITIZER && u == usage::TOUCH_PAD)
}

pub fn preparsed_data(device: HANDLE) -> Option<PreparsedData> {
    let mut size = 0u32;
    // SAFETY: a null buffer queries the required size in bytes.
    unsafe { GetRawInputDeviceInfoW(Some(device), RIDI_PREPARSEDDATA, None, &mut size) };
    if size == 0 {
        return None;
    }
    let mut buf = vec![0u8; size as usize];
    // SAFETY: `buf` holds `size` bytes.
    let n = unsafe {
        GetRawInputDeviceInfoW(
            Some(device),
            RIDI_PREPARSEDDATA,
            Some(buf.as_mut_ptr().cast()),
            &mut size,
        )
    };
    if n == RAW_INPUT_ERROR {
        return None;
    }
    buf.truncate(n as usize);
    Some(PreparsedData::from_bytes(&buf))
}

/// A HID device handle opened with `CreateFileW`, closed on drop.
struct HidFile(HANDLE);

impl HidFile {
    /// Opens the device read/write, falling back to zero access, which needs no
    /// special rights and still allows reading strings and often feature reports.
    fn open(path: &str) -> Option<(Self, bool)> {
        let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
        let try_open = |access: u32| {
            // SAFETY: `wide` is a NUL-terminated UTF-16 path that outlives the call.
            unsafe {
                CreateFileW(
                    PCWSTR(wide.as_ptr()),
                    access,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    None,
                    OPEN_EXISTING,
                    FILE_FLAGS_AND_ATTRIBUTES(0),
                    None,
                )
            }
            .ok()
        };
        if let Some(h) = try_open(GENERIC_READ.0 | GENERIC_WRITE.0) {
            return Some((Self(h), true));
        }
        try_open(0).map(|h| (Self(h), false))
    }

    fn string(
        &self,
        get: unsafe fn(HANDLE, *mut core::ffi::c_void, u32) -> bool,
    ) -> Option<String> {
        // HID strings are at most 126 UTF-16 units plus the terminator.
        let mut buf = [0u16; 128];
        // SAFETY: `buf` is writable for its full byte length.
        let ok = unsafe {
            get(
                self.0,
                buf.as_mut_ptr().cast(),
                std::mem::size_of_val(&buf) as u32,
            )
        };
        if !ok {
            return None;
        }
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        let s = String::from_utf16_lossy(&buf[..end]).trim().to_string();
        (!s.is_empty()).then_some(s)
    }

    fn feature(&self, report_id: u8, len: usize) -> Option<Vec<u8>> {
        let mut buf = vec![0u8; len.max(2)];
        buf[0] = report_id;
        // SAFETY: `buf` is writable for `buf.len()` bytes; byte 0 selects the report.
        let ok = unsafe { HidD_GetFeature(self.0, buf.as_mut_ptr().cast(), buf.len() as u32) };
        ok.then_some(buf)
    }
}

impl Drop for HidFile {
    fn drop(&mut self) {
        // SAFETY: the handle was returned by CreateFileW and is closed once.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// Everything needed to decode one touchpad's reports.
pub struct Touchpad {
    pub path: String,
    pub name: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub preparsed: PreparsedData,
    pub layout: TouchpadLayout,
    pub geometry: Geometry,
    pub max_contacts: u8,
    pub max_contacts_source: MaxContactsSource,
    /// Whether the device could be opened with read/write access. Without it
    /// a zero-access handle is used, through which Windows still serves strings
    /// and, on some devices, feature reports.
    pub opened_rw: bool,
    /// Physical size was missing from the descriptor and is a guess.
    pub size_guessed: bool,
}

/// Fallback surface width when the descriptor declares no physical units, a
/// typical laptop touchpad. PTP certification requires units, so this should
/// only happen with non-compliant devices.
const FALLBACK_WIDTH_MM: f32 = 100.0;

impl Touchpad {
    pub fn open(device: HANDLE) -> Option<Self> {
        let (vendor_id, product_id, page, u) = hid_info(device)?;
        if page != usage::PAGE_DIGITIZER || u != usage::TOUCH_PAD {
            return None;
        }
        let path = device_name(device)?;
        let preparsed = preparsed_data(device)?;
        let layout = match TouchpadLayout::new(&preparsed) {
            Ok(layout) => layout,
            Err(e) => {
                tracing::warn!(%path, "ignoring touchpad: {e}");
                return None;
            }
        };

        let file = HidFile::open(&path);
        let name = file
            .as_ref()
            .and_then(|(f, _)| {
                let product = f.string(HidD_GetProductString);
                let maker = f.string(HidD_GetManufacturerString);
                match (maker, product) {
                    (Some(m), Some(p)) if !p.starts_with(&m) => Some(format!("{m} {p}")),
                    (_, Some(p)) => Some(p),
                    (m, None) => m,
                }
            })
            .unwrap_or_else(|| format!("Touchpad {vendor_id:04X}:{product_id:04X}"));
        let feature_value = file
            .as_ref()
            .zip(layout.contact_count_max_report())
            .and_then(|((f, _), (id, len))| {
                let report = f.feature(id, len)?;
                layout.parse_contact_count_max(&preparsed, &report)
            });
        let (max_contacts, max_contacts_source) = layout.max_contacts(feature_value);

        let (width, height) = (layout.x.extent_mm(), layout.y.extent_mm());
        let size_guessed = width.is_none() || height.is_none();
        let (width_mm, height_mm) = match (width, height) {
            (Some(w), Some(h)) => (w, h),
            _ => {
                let aspect = (layout.y.logical_max - layout.y.logical_min) as f32
                    / (layout.x.logical_max - layout.x.logical_min).max(1) as f32;
                tracing::warn!(%path, "descriptor has no physical size; assuming {FALLBACK_WIDTH_MM} mm wide");
                (FALLBACK_WIDTH_MM, FALLBACK_WIDTH_MM * aspect)
            }
        };
        let geometry = Geometry {
            x: layout.x,
            y: layout.y,
            width_mm,
            height_mm,
        };

        Some(Self {
            path,
            name,
            vendor_id,
            product_id,
            preparsed,
            layout,
            geometry,
            max_contacts,
            max_contacts_source,
            opened_rw: file.as_ref().is_some_and(|(_, rw)| *rw),
            size_guessed,
        })
    }

    pub fn info(&self, id: glyphpad_core::DeviceId) -> glyphpad_core::DeviceInfo {
        glyphpad_core::DeviceInfo {
            id,
            path: self.path.clone(),
            name: self.name.clone(),
            vendor_id: self.vendor_id,
            product_id: self.product_id,
            width_mm: self.geometry.width_mm,
            height_mm: self.geometry.height_mm,
            max_contacts: self.max_contacts,
        }
    }
}

/// Every Precision Touchpad currently connected.
pub fn touchpads() -> Vec<(HANDLE, Touchpad)> {
    hid_device_handles()
        .into_iter()
        .filter(|&h| is_touchpad(h))
        .filter_map(|h| Touchpad::open(h).map(|t| (h, t)))
        .collect()
}
