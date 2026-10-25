//! HID report parsing through the Windows HID parser (`HidP_*`), starting
//! from the device's preparsed data. No byte offsets are hard-coded: every
//! value is located by usage page, usage and link collection (SPEC §4).

use std::fmt::Write as _;

use windows::Win32::Devices::HumanInterfaceDevice::{
    HIDP_BUTTON_CAPS, HIDP_CAPS, HIDP_LINK_COLLECTION_NODE, HIDP_REPORT_TYPE,
    HIDP_STATUS_INCOMPATIBLE_REPORT_ID, HIDP_STATUS_SUCCESS, HIDP_STATUS_USAGE_NOT_FOUND,
    HIDP_VALUE_CAPS, HidP_Feature, HidP_GetButtonCaps, HidP_GetCaps, HidP_GetLinkCollectionNodes,
    HidP_GetUsageValue, HidP_GetUsages, HidP_GetValueCaps, HidP_Input, HidP_MaxUsageListLength,
    PHIDP_PREPARSED_DATA,
};
use windows::Win32::Foundation::NTSTATUS;

use crate::ptp::{AxisCaps, RawContact, RawReport, length_unit_mm, unit_exponent, usage};

/// Owned copy of a device's preparsed data.
///
/// Stored as `u64`s so the buffer is 8-byte aligned, as the HID parser expects
/// for the opaque structure behind `PHIDP_PREPARSED_DATA`.
pub struct PreparsedData {
    buf: Vec<u64>,
}

impl PreparsedData {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut buf = vec![0u64; bytes.len().div_ceil(8)];
        // SAFETY: `buf` holds at least `bytes.len()` bytes and the regions
        // cannot overlap (one is freshly allocated).
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf.as_mut_ptr().cast(), bytes.len());
        }
        Self { buf }
    }

    fn handle(&self) -> PHIDP_PREPARSED_DATA {
        PHIDP_PREPARSED_DATA(self.buf.as_ptr() as isize)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum HidError {
    #[error("{call} failed with NTSTATUS {status:#010x}")]
    Status { call: &'static str, status: i32 },
    #[error("not a Precision Touchpad: {0}")]
    NotATouchpad(&'static str),
}

fn check(call: &'static str, status: NTSTATUS) -> Result<(), HidError> {
    if status == HIDP_STATUS_SUCCESS {
        Ok(())
    } else {
        Err(HidError::Status {
            call,
            status: status.0,
        })
    }
}

fn value_caps(pd: &PreparsedData, kind: HIDP_REPORT_TYPE, count: u16) -> Vec<HIDP_VALUE_CAPS> {
    if count == 0 {
        return Vec::new();
    }
    let mut caps = vec![HIDP_VALUE_CAPS::default(); usize::from(count)];
    let mut len = count;
    // SAFETY: `caps` has room for `len` entries and `pd` is valid preparsed data.
    let status = unsafe { HidP_GetValueCaps(kind, caps.as_mut_ptr(), &mut len, pd.handle()) };
    if status != HIDP_STATUS_SUCCESS {
        return Vec::new();
    }
    caps.truncate(usize::from(len));
    caps
}

fn button_caps(pd: &PreparsedData, kind: HIDP_REPORT_TYPE, count: u16) -> Vec<HIDP_BUTTON_CAPS> {
    if count == 0 {
        return Vec::new();
    }
    let mut caps = vec![HIDP_BUTTON_CAPS::default(); usize::from(count)];
    let mut len = count;
    // SAFETY: as in `value_caps`.
    let status = unsafe { HidP_GetButtonCaps(kind, caps.as_mut_ptr(), &mut len, pd.handle()) };
    if status != HIDP_STATUS_SUCCESS {
        return Vec::new();
    }
    caps.truncate(usize::from(len));
    caps
}

fn link_collections(pd: &PreparsedData, count: u16) -> Vec<HIDP_LINK_COLLECTION_NODE> {
    let mut nodes = vec![HIDP_LINK_COLLECTION_NODE::default(); usize::from(count)];
    let mut len = u32::from(count);
    // SAFETY: `nodes` has room for `len` entries.
    let status = unsafe { HidP_GetLinkCollectionNodes(nodes.as_mut_ptr(), &mut len, pd.handle()) };
    if status != HIDP_STATUS_SUCCESS {
        return Vec::new();
    }
    nodes.truncate(len as usize);
    nodes
}

/// Usage of a value cap, or the first usage of a range.
fn cap_usage(cap: &HIDP_VALUE_CAPS) -> u16 {
    // SAFETY: the union variant is selected by `IsRange`, as documented.
    unsafe {
        if cap.IsRange {
            cap.Anonymous.Range.UsageMin
        } else {
            cap.Anonymous.NotRange.Usage
        }
    }
}

fn button_usage_range(cap: &HIDP_BUTTON_CAPS) -> (u16, u16) {
    // SAFETY: the union variant is selected by `IsRange`, as documented.
    unsafe {
        if cap.IsRange {
            (cap.Anonymous.Range.UsageMin, cap.Anonymous.Range.UsageMax)
        } else {
            (cap.Anonymous.NotRange.Usage, cap.Anonymous.NotRange.Usage)
        }
    }
}

fn axis_caps(cap: &HIDP_VALUE_CAPS) -> AxisCaps {
    AxisCaps {
        logical_min: cap.LogicalMin,
        logical_max: cap.LogicalMax,
        physical_min: cap.PhysicalMin,
        physical_max: cap.PhysicalMax,
        units: cap.Units,
        units_exp: cap.UnitsExp,
    }
}

/// Where the maximum number of contacts came from, best source first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaxContactsSource {
    /// Value of the Contact Count Maximum feature report, read from the device.
    FeatureReport,
    /// Logical maximum of the Contact Count Maximum feature usage.
    FeatureLogicalMax,
    /// Logical maximum of the Contact Count input usage.
    ContactCountLogicalMax,
    /// Number of finger collections in the input report.
    FingerSlots,
}

/// How a Precision Touchpad lays out its input reports.
pub struct TouchpadLayout {
    pub caps: HIDP_CAPS,
    /// Link collection index of every finger slot, in report order.
    pub fingers: Vec<u16>,
    pub x: AxisCaps,
    pub y: AxisCaps,
    x_bits: u16,
    y_bits: u16,
    has_contact_count: bool,
    has_scan_time: bool,
    has_confidence: bool,
    contact_count_max_feature: Option<HIDP_VALUE_CAPS>,
    contact_count_logical_max: Option<i32>,
    /// Scratch space for `HidP_GetUsages`: the touch path runs a hundred
    /// times a second and must not allocate a vector per call.
    usages: Vec<u16>,
    flags: Vec<u16>,
}

impl TouchpadLayout {
    pub fn new(pd: &PreparsedData) -> Result<Self, HidError> {
        let mut caps = HIDP_CAPS::default();
        // SAFETY: `pd` is valid preparsed data, `caps` a valid out pointer.
        check("HidP_GetCaps", unsafe {
            HidP_GetCaps(pd.handle(), &mut caps)
        })?;
        if caps.UsagePage != usage::PAGE_DIGITIZER || caps.Usage != usage::TOUCH_PAD {
            return Err(HidError::NotATouchpad(
                "top-level collection is not 0x0D/0x05",
            ));
        }

        let values = value_caps(pd, HidP_Input, caps.NumberInputValueCaps);
        let is_finger = |c: &HIDP_VALUE_CAPS| {
            c.LinkUsagePage == usage::PAGE_DIGITIZER && c.LinkUsage == usage::FINGER
        };
        let find = |page: u16, u: u16| {
            values
                .iter()
                .filter(|c| is_finger(c) && c.UsagePage == page && cap_usage(c) == u)
                .collect::<Vec<_>>()
        };

        let xs = find(usage::PAGE_GENERIC_DESKTOP, usage::X);
        let ys = find(usage::PAGE_GENERIC_DESKTOP, usage::Y);
        let (Some(x), Some(y)) = (xs.first(), ys.first()) else {
            return Err(HidError::NotATouchpad("no X/Y in a finger collection"));
        };
        let mut fingers: Vec<u16> = xs.iter().map(|c| c.LinkCollection).collect();
        fingers.sort_unstable();
        fingers.dedup();

        let top_level = |u: u16| {
            values
                .iter()
                .find(|c| c.UsagePage == usage::PAGE_DIGITIZER && cap_usage(c) == u)
        };
        let contact_count = top_level(usage::CONTACT_COUNT);

        let buttons = button_caps(pd, HidP_Input, caps.NumberInputButtonCaps);
        let has_confidence = buttons.iter().any(|b| {
            let (lo, hi) = button_usage_range(b);
            b.UsagePage == usage::PAGE_DIGITIZER && (lo..=hi).contains(&usage::CONFIDENCE)
        });

        let features = value_caps(pd, HidP_Feature, caps.NumberFeatureValueCaps);
        let contact_count_max_feature = features.into_iter().find(|c| {
            c.UsagePage == usage::PAGE_DIGITIZER && cap_usage(c) == usage::CONTACT_COUNT_MAXIMUM
        });

        // SAFETY: `pd` is valid preparsed data.
        let max_usages = unsafe { HidP_MaxUsageListLength(HidP_Input, None, pd.handle()) };

        let max_usages = max_usages.max(16);
        Ok(Self {
            caps,
            x: axis_caps(x),
            y: axis_caps(y),
            x_bits: x.BitSize,
            y_bits: y.BitSize,
            fingers,
            has_contact_count: contact_count.is_some(),
            has_scan_time: top_level(usage::SCAN_TIME).is_some(),
            has_confidence,
            contact_count_max_feature,
            contact_count_logical_max: contact_count.map(|c| c.LogicalMax),
            usages: vec![0; max_usages as usize],
            flags: vec![0; max_usages as usize],
        })
    }

    /// Maximum simultaneous contacts, given the Contact Count Maximum feature
    /// value if it could be read from the device.
    pub fn max_contacts(&self, feature_value: Option<u32>) -> (u8, MaxContactsSource) {
        let plausible = |v: i64| (1..=glyphpad_core::MAX_CONTACTS as i64).contains(&v);
        if let Some(v) = feature_value.map(i64::from).filter(|&v| plausible(v)) {
            return (v as u8, MaxContactsSource::FeatureReport);
        }
        if let Some(v) = self
            .contact_count_max_feature
            .map(|c| i64::from(c.LogicalMax))
            .filter(|&v| plausible(v))
        {
            return (v as u8, MaxContactsSource::FeatureLogicalMax);
        }
        if let Some(v) = self
            .contact_count_logical_max
            .map(i64::from)
            .filter(|&v| plausible(v))
        {
            return (v as u8, MaxContactsSource::ContactCountLogicalMax);
        }
        let slots = self.fingers.len().min(glyphpad_core::MAX_CONTACTS) as u8;
        (slots, MaxContactsSource::FingerSlots)
    }

    /// Report id and length of the feature report holding Contact Count Maximum.
    pub fn contact_count_max_report(&self) -> Option<(u8, usize)> {
        self.contact_count_max_feature
            .map(|c| (c.ReportID, usize::from(self.caps.FeatureReportByteLength)))
    }

    /// Extracts Contact Count Maximum from a feature report read from the device.
    pub fn parse_contact_count_max(&self, pd: &PreparsedData, report: &[u8]) -> Option<u32> {
        let mut value = 0u32;
        // SAFETY: `report` is a complete feature report of this device.
        let status = unsafe {
            HidP_GetUsageValue(
                HidP_Feature,
                usage::PAGE_DIGITIZER,
                None,
                usage::CONTACT_COUNT_MAXIMUM,
                &mut value,
                pd.handle(),
                report,
            )
        };
        (status == HIDP_STATUS_SUCCESS).then_some(value)
    }

    /// Parses one input report. Returns `None` for reports of other report
    /// ids (e.g. vendor data) that carry no touch data.
    pub fn parse(
        &mut self,
        pd: &PreparsedData,
        report: &mut [u8],
    ) -> Result<Option<RawReport>, HidError> {
        // Destructured so the scratch buffers can be borrowed while the
        // layout's other fields are read.
        let Self {
            fingers,
            x,
            y,
            x_bits,
            y_bits,
            has_confidence,
            has_contact_count,
            has_scan_time,
            usages,
            flags,
            ..
        } = self;
        let value = |page: u16,
                     link: Option<u16>,
                     u: u16,
                     report: &[u8]|
         -> Result<Option<u32>, HidError> {
            let mut v = 0u32;
            // SAFETY: `report` is one complete input report of this device.
            let status = unsafe {
                HidP_GetUsageValue(HidP_Input, page, link, u, &mut v, pd.handle(), report)
            };
            match status {
                s if s == HIDP_STATUS_SUCCESS => Ok(Some(v)),
                s if s == HIDP_STATUS_USAGE_NOT_FOUND
                    || s == HIDP_STATUS_INCOMPATIBLE_REPORT_ID =>
                {
                    Ok(None)
                }
                s => Err(HidError::Status {
                    call: "HidP_GetUsageValue",
                    status: s.0,
                }),
            }
        };

        // The first finger's X tells whether this report carries touch data at all.
        let first = fingers[0];
        if value(usage::PAGE_GENERIC_DESKTOP, Some(first), usage::X, report)?.is_none() {
            return Ok(None);
        }

        let contact_count = if *has_contact_count {
            Some(value(usage::PAGE_DIGITIZER, None, usage::CONTACT_COUNT, report)?.unwrap_or(0))
        } else {
            None
        };
        let scan_time = if *has_scan_time {
            value(usage::PAGE_DIGITIZER, None, usage::SCAN_TIME, report)?.map(|v| v as u16)
        } else {
            None
        };
        let pressed = pressed_usages(pd, usage::PAGE_BUTTON, None, report, usages)?;
        let button = usages[..pressed].contains(&usage::BUTTON_1);

        let mut contacts = Vec::with_capacity(fingers.len());
        for &link in fingers.iter() {
            let pressed = pressed_usages(pd, usage::PAGE_DIGITIZER, Some(link), report, flags)?;
            let down = &flags[..pressed];
            let id =
                value(usage::PAGE_DIGITIZER, Some(link), usage::CONTACT_ID, report)?.unwrap_or(0);
            let vx = value(usage::PAGE_GENERIC_DESKTOP, Some(link), usage::X, report)?.unwrap_or(0);
            let vy = value(usage::PAGE_GENERIC_DESKTOP, Some(link), usage::Y, report)?.unwrap_or(0);
            contacts.push(RawContact {
                id,
                x: x.decode(vx, *x_bits),
                y: y.decode(vy, *y_bits),
                tip: down.contains(&usage::TIP_SWITCH),
                // Devices without the Confidence usage never flag palms.
                confident: !*has_confidence || down.contains(&usage::CONFIDENCE),
            });
        }

        Ok(Some(RawReport {
            contact_count,
            scan_time,
            button,
            contacts,
        }))
    }
}

/// Reads the usages of `page` (in `link`, when given) that are on in
/// `report` into `out`, and returns how many. Does not allocate: `out` is a
/// scratch buffer of `max_usages` entries.
fn pressed_usages(
    pd: &PreparsedData,
    page: u16,
    link: Option<u16>,
    report: &mut [u8],
    out: &mut [u16],
) -> Result<usize, HidError> {
    let mut len = out.len() as u32;
    // SAFETY: `out` has room for `len` entries; `report` is a complete report.
    let status = unsafe {
        HidP_GetUsages(
            HidP_Input,
            page,
            link,
            out.as_mut_ptr(),
            &mut len,
            pd.handle(),
            report,
        )
    };
    match status {
        s if s == HIDP_STATUS_SUCCESS => Ok(len as usize),
        s if s == HIDP_STATUS_USAGE_NOT_FOUND || s == HIDP_STATUS_INCOMPATIBLE_REPORT_ID => Ok(0),
        s => Err(HidError::Status {
            call: "HidP_GetUsages",
            status: s.0,
        }),
    }
}

/// Human-readable dump of every capability of the device, for
/// `glyphpad dump-descriptor` on hardware we have never seen.
pub fn describe(pd: &PreparsedData) -> String {
    let mut out = String::new();
    let mut caps = HIDP_CAPS::default();
    // SAFETY: `pd` is valid preparsed data, `caps` a valid out pointer.
    let status = unsafe { HidP_GetCaps(pd.handle(), &mut caps) };
    if status != HIDP_STATUS_SUCCESS {
        let _ = writeln!(out, "  HidP_GetCaps failed: {:#010x}", status.0);
        return out;
    }
    let _ = writeln!(
        out,
        "  top-level collection: usage page {:#06x}, usage {:#06x}",
        caps.UsagePage, caps.Usage
    );
    let _ = writeln!(
        out,
        "  report lengths: input {} B, output {} B, feature {} B",
        caps.InputReportByteLength, caps.OutputReportByteLength, caps.FeatureReportByteLength
    );

    let _ = writeln!(
        out,
        "  link collections ({}):",
        caps.NumberLinkCollectionNodes
    );
    for (i, n) in link_collections(pd, caps.NumberLinkCollectionNodes)
        .iter()
        .enumerate()
    {
        let _ = writeln!(
            out,
            "    [{i:2}] usage {:#06x}/{:#06x}  parent {}  children {}",
            n.LinkUsagePage, n.LinkUsage, n.Parent, n.NumberOfChildren
        );
    }

    for (name, kind, nv, nb) in [
        (
            "input",
            HidP_Input,
            caps.NumberInputValueCaps,
            caps.NumberInputButtonCaps,
        ),
        (
            "feature",
            HidP_Feature,
            caps.NumberFeatureValueCaps,
            caps.NumberFeatureButtonCaps,
        ),
    ] {
        let _ = writeln!(out, "  {name} value caps ({nv}):");
        for c in value_caps(pd, kind, nv) {
            let usage = if c.IsRange {
                // SAFETY: `IsRange` selects the `Range` variant.
                let r = unsafe { c.Anonymous.Range };
                format!("{:#06x}..{:#06x}", r.UsageMin, r.UsageMax)
            } else {
                format!("{:#06x}", cap_usage(&c))
            };
            let mm = axis_caps(&c)
                .extent_mm()
                .map_or(String::new(), |mm| format!("  = {mm:.2} mm"));
            let _ = writeln!(
                out,
                "    report {:#04x} link {:2} ({:#06x}/{:#06x})  usage {:#06x}/{usage}  bits {}×{}  \
                 logical {}..{}  physical {}..{}  unit {:#x} exp {:#x} (10^{}, {}){mm}",
                c.ReportID,
                c.LinkCollection,
                c.LinkUsagePage,
                c.LinkUsage,
                c.UsagePage,
                c.BitSize,
                c.ReportCount,
                c.LogicalMin,
                c.LogicalMax,
                c.PhysicalMin,
                c.PhysicalMax,
                c.Units,
                c.UnitsExp,
                unit_exponent(c.UnitsExp),
                match length_unit_mm(c.Units) {
                    Some(10.0) => "cm",
                    Some(_) => "inch",
                    None => "no length unit",
                },
            );
        }
        let _ = writeln!(out, "  {name} button caps ({nb}):");
        for b in button_caps(pd, kind, nb) {
            let (lo, hi) = button_usage_range(&b);
            let _ = writeln!(
                out,
                "    report {:#04x} link {:2} ({:#06x}/{:#06x})  usage {:#06x}/{lo:#06x}..{hi:#06x}",
                b.ReportID, b.LinkCollection, b.LinkUsagePage, b.LinkUsage, b.UsagePage
            );
        }
    }
    out
}
