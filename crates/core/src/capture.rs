//! Recorded touch sessions in JSON Lines (SPEC §8).
//!
//! The first line is a [`CaptureHeader`]; every following line is one
//! [`DeviceEvent`]. One record per line keeps captures appendable while
//! recording, diffable, and readable up to the last complete line if the
//! recorder is killed.

use std::io::{BufRead, Write};

use serde::{Deserialize, Serialize};

use crate::model::DeviceEvent;

/// Version of the capture format, bumped on incompatible changes.
pub const CAPTURE_FORMAT: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaptureHeader {
    pub format: u32,
    pub glyphpad_version: String,
}

#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("line {line}: {source}")]
    Parse {
        line: usize,
        #[source]
        source: serde_json::Error,
    },
    #[error("the capture is empty")]
    Empty,
    #[error("unsupported capture format {0} (this build reads format {CAPTURE_FORMAT})")]
    UnsupportedFormat(u32),
}

pub struct CaptureWriter<W: Write> {
    out: W,
}

impl<W: Write> CaptureWriter<W> {
    pub fn new(mut out: W, glyphpad_version: &str) -> Result<Self, CaptureError> {
        let header = CaptureHeader {
            format: CAPTURE_FORMAT,
            glyphpad_version: glyphpad_version.to_string(),
        };
        write_line(&mut out, &header)?;
        Ok(Self { out })
    }

    /// Appends an event and flushes, so a capture interrupted with Ctrl+C
    /// loses at most the event being written.
    pub fn write(&mut self, event: &DeviceEvent) -> Result<(), CaptureError> {
        write_line(&mut self.out, event)
    }
}

fn write_line<W: Write, T: Serialize>(out: &mut W, value: &T) -> Result<(), CaptureError> {
    serde_json::to_writer(&mut *out, value).map_err(std::io::Error::from)?;
    out.write_all(b"\n")?;
    out.flush()?;
    Ok(())
}

/// A capture fully loaded in memory.
#[derive(Debug, Clone, PartialEq)]
pub struct Capture {
    pub header: CaptureHeader,
    pub events: Vec<DeviceEvent>,
}

impl Capture {
    pub fn read<R: BufRead>(input: R) -> Result<Self, CaptureError> {
        let mut lines = input
            .lines()
            .enumerate()
            .filter(|(_, l)| l.as_ref().map_or(true, |l| !l.trim().is_empty()));
        let (_, first) = lines.next().ok_or(CaptureError::Empty)?;
        let header: CaptureHeader = serde_json::from_str(&first?)
            .map_err(|source| CaptureError::Parse { line: 1, source })?;
        if header.format != CAPTURE_FORMAT {
            return Err(CaptureError::UnsupportedFormat(header.format));
        }
        let events = lines
            .map(|(i, line)| {
                serde_json::from_str(&line?).map_err(|source| CaptureError::Parse {
                    line: i + 1,
                    source,
                })
            })
            .collect::<Result<_, _>>()?;
        Ok(Self { header, events })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::model::{Contact, DeviceId, DeviceInfo, TouchFrame};

    pub(crate) fn sample_events() -> Vec<DeviceEvent> {
        let device = DeviceId(1);
        let contact = |id, x: f32, tip| Contact {
            id,
            x_mm: x,
            y_mm: 10.0,
            x_norm: x / 100.0,
            y_norm: 0.25,
            tip,
            confident: true,
        };
        vec![
            DeviceEvent::Connected(DeviceInfo {
                id: device,
                path: r"\\?\HID#VID_1234&PID_5678".into(),
                name: "Test pad".into(),
                vendor_id: 0x1234,
                product_id: 0x5678,
                width_mm: 100.0,
                height_mm: 40.0,
                max_contacts: 5,
            }),
            DeviceEvent::Frame {
                device,
                frame: TouchFrame {
                    timestamp_us: 1_000,
                    contacts: vec![contact(0, 10.0, true), contact(1, 30.5, true)],
                    button_pressed: false,
                },
            },
            DeviceEvent::Frame {
                device,
                frame: TouchFrame {
                    timestamp_us: 9_000,
                    contacts: vec![contact(0, 11.25, false)],
                    button_pressed: true,
                },
            },
            DeviceEvent::Disconnected { device },
        ]
    }

    #[test]
    fn round_trip() {
        let events = sample_events();
        let mut buf = Vec::new();
        let mut writer = CaptureWriter::new(&mut buf, "1.2.3").unwrap();
        for e in &events {
            writer.write(e).unwrap();
        }
        let text = String::from_utf8(buf).unwrap();
        assert_eq!(text.lines().count(), events.len() + 1);
        let capture = Capture::read(text.as_bytes()).unwrap();
        assert_eq!(capture.header.glyphpad_version, "1.2.3");
        assert_eq!(capture.events, events);
    }

    #[test]
    fn reports_line_of_bad_record() {
        let text = "{\"format\":1,\"glyphpad_version\":\"x\"}\n\n{\"type\":\"bogus\"}\n";
        match Capture::read(text.as_bytes()) {
            Err(CaptureError::Parse { line, .. }) => assert_eq!(line, 3),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn rejects_other_formats() {
        let text = "{\"format\":99,\"glyphpad_version\":\"x\"}\n";
        assert!(matches!(
            Capture::read(text.as_bytes()),
            Err(CaptureError::UnsupportedFormat(99))
        ));
        assert!(matches!(Capture::read(&b""[..]), Err(CaptureError::Empty)));
    }
}
