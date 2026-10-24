//! Deterministic playback of a [`Capture`] as a [`TouchSource`] (SPEC §8).

use std::time::Duration;

use crossbeam_channel::Sender;

use crate::capture::Capture;
use crate::model::{DeviceEvent, DeviceInfo};
use crate::source::{SourceError, TouchSource};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Pacing {
    /// Emit everything immediately. Timestamps still come from the capture,
    /// so anything driven by them behaves exactly as when recorded.
    Instant,
    /// Sleep between frames to reproduce the recorded timing, scaled by `speed`.
    RealTime { speed: f32 },
}

pub struct ReplaySource {
    capture: Capture,
    pacing: Pacing,
}

impl ReplaySource {
    pub fn new(capture: Capture, pacing: Pacing) -> Self {
        Self { capture, pacing }
    }
}

impl TouchSource for ReplaySource {
    fn devices(&self) -> Vec<DeviceInfo> {
        self.capture
            .events
            .iter()
            .filter_map(|e| match e {
                DeviceEvent::Connected(info) => Some(info.clone()),
                _ => None,
            })
            .collect()
    }

    fn run(self, tx: Sender<DeviceEvent>) -> Result<(), SourceError> {
        let mut last_ts: Option<u64> = None;
        for event in self.capture.events {
            if let (Pacing::RealTime { speed }, DeviceEvent::Frame { frame, .. }) =
                (self.pacing, &event)
            {
                if let Some(prev) = last_ts {
                    let delta = frame.timestamp_us.saturating_sub(prev);
                    let scaled = delta as f64 / f64::from(speed.max(0.01));
                    std::thread::sleep(Duration::from_micros(scaled as u64));
                }
                last_ts = Some(frame.timestamp_us);
            }
            if tx.send(event).is_err() {
                break; // Receiver gone: nobody is listening any more.
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::{CAPTURE_FORMAT, CaptureHeader};

    fn capture() -> Capture {
        Capture {
            header: CaptureHeader {
                format: CAPTURE_FORMAT,
                glyphpad_version: "test".into(),
            },
            events: crate::capture::tests::sample_events(),
        }
    }

    #[test]
    fn replays_every_event_in_order() {
        let source = ReplaySource::new(capture(), Pacing::Instant);
        assert_eq!(source.devices().len(), 1);
        let (tx, rx) = crossbeam_channel::unbounded();
        source.run(tx).unwrap();
        let replayed: Vec<_> = rx.iter().collect();
        assert_eq!(replayed, capture().events);
    }

    #[test]
    fn real_time_pacing_reproduces_gaps() {
        // Frames are 8 ms apart in the sample; at 4x that is 2 ms.
        let source = ReplaySource::new(capture(), Pacing::RealTime { speed: 4.0 });
        let (tx, rx) = crossbeam_channel::unbounded();
        let start = std::time::Instant::now();
        source.run(tx).unwrap();
        assert!(start.elapsed() >= Duration::from_millis(2));
        assert_eq!(rx.iter().count(), 4);
    }

    #[test]
    fn stops_when_receiver_is_dropped() {
        let source = ReplaySource::new(capture(), Pacing::Instant);
        let (tx, rx) = crossbeam_channel::bounded(0);
        drop(rx);
        source.run(tx).unwrap();
    }
}
