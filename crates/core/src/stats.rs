//! Summary statistics of a touch session, used to check captures recorded
//! on real hardware (frame rate, contact counts, hybrid-mode assembly).

use std::collections::BTreeSet;
use std::fmt;

use crate::model::{DeviceEvent, TouchFrame};

#[derive(Debug, Default, Clone, PartialEq)]
pub struct SessionStats {
    pub devices: usize,
    pub disconnects: usize,
    pub frames: usize,
    /// From the first to the last frame of a device.
    pub duration_us: u64,
    /// Most fingers touching at once (tip down, confident or not).
    pub max_touching: usize,
    /// Number of frames by count of contacts touching, index = count.
    pub touching_histogram: Vec<usize>,
    pub distinct_ids: BTreeSet<u32>,
    pub lifts: usize,
    pub palm_contacts: usize,
    pub button_frames: usize,
    pub interval_min_us: Option<u64>,
    pub interval_median_us: Option<u64>,
    pub interval_max_us: Option<u64>,
    /// Frames whose timestamp went backwards (must be 0).
    pub non_monotonic: usize,
    /// Frames repeating a contact id (an assembly bug if > 0).
    pub duplicate_ids: usize,
    /// Lowest, highest and previous timestamp of every device, and the
    /// intervals between consecutive frames of a device. Filled by
    /// [`SessionStats::add`] and reduced by [`SessionStats::finish`].
    devices_time: std::collections::BTreeMap<crate::model::DeviceId, (u64, u64, u64)>,
    intervals: Vec<u64>,
}

impl SessionStats {
    pub fn from_events<'a>(events: impl IntoIterator<Item = &'a DeviceEvent>) -> Self {
        let mut s = Self::default();
        for event in events {
            s.add(event);
        }
        s.finish();
        s
    }

    /// Folds one event in. Frames of several devices are kept apart: their
    /// timestamps need not share a base, and a capture that outlives a
    /// device reset can go backwards.
    pub fn add(&mut self, event: &DeviceEvent) {
        match event {
            DeviceEvent::Connected(_) => self.devices += 1,
            DeviceEvent::Disconnected { .. } => self.disconnects += 1,
            DeviceEvent::Frame { device, frame } => {
                let ts = frame.timestamp_us;
                match self.devices_time.get_mut(device) {
                    Some((low, high, previous)) => {
                        if ts < *previous {
                            self.non_monotonic += 1;
                        } else {
                            self.intervals.push(ts - *previous);
                        }
                        *previous = ts;
                        *low = (*low).min(ts);
                        *high = (*high).max(ts);
                    }
                    None => {
                        self.devices_time.insert(*device, (ts, ts, ts));
                    }
                }
                self.add_frame(frame);
            }
        }
    }

    /// Computes what needs the whole stream: the duration and the frame
    /// interval summary. Call after the last [`add`](Self::add).
    pub fn finish(&mut self) {
        self.duration_us = self
            .devices_time
            .values()
            .map(|&(low, high, _)| high.saturating_sub(low))
            .max()
            .unwrap_or(0);
        self.intervals.sort_unstable();
        self.interval_min_us = self.intervals.first().copied();
        self.interval_max_us = self.intervals.last().copied();
        self.interval_median_us = self.intervals.get(self.intervals.len() / 2).copied();
    }

    fn add_frame(&mut self, frame: &TouchFrame) {
        self.frames += 1;
        let touching = frame.contacts.iter().filter(|c| c.tip).count();
        self.max_touching = self.max_touching.max(touching);
        if self.touching_histogram.len() <= touching {
            self.touching_histogram.resize(touching + 1, 0);
        }
        self.touching_histogram[touching] += 1;
        let mut ids = BTreeSet::new();
        for c in &frame.contacts {
            self.distinct_ids.insert(c.id);
            if !ids.insert(c.id) {
                self.duplicate_ids += 1;
            }
            if !c.tip {
                self.lifts += 1;
            }
            if c.tip && !c.confident {
                self.palm_contacts += 1;
            }
        }
        if frame.button_pressed {
            self.button_frames += 1;
        }
    }
}

impl fmt::Display for SessionStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ms = |us: Option<u64>| {
            us.map_or("-".to_string(), |us| {
                format!("{:.1} ms", us as f64 / 1000.0)
            })
        };
        writeln!(
            f,
            "devices connected:     {} (disconnects: {})",
            self.devices, self.disconnects
        )?;
        writeln!(
            f,
            "frames:                {} over {:.2} s",
            self.frames,
            self.duration_us as f64 / 1e6
        )?;
        writeln!(
            f,
            "frame interval:        min {}  median {}  max {}",
            ms(self.interval_min_us),
            ms(self.interval_median_us),
            ms(self.interval_max_us)
        )?;
        writeln!(f, "max fingers touching:  {}", self.max_touching)?;
        let hist: Vec<String> = self
            .touching_histogram
            .iter()
            .enumerate()
            .filter(|&(_, &n)| n > 0)
            .map(|(k, n)| format!("{k}:{n}"))
            .collect();
        writeln!(f, "frames by fingers:     {}", hist.join("  "))?;
        writeln!(f, "distinct contact ids:  {:?}", self.distinct_ids)?;
        writeln!(f, "lifts (tip up):        {}", self.lifts)?;
        writeln!(f, "palm contacts:         {}", self.palm_contacts)?;
        writeln!(f, "frames with click:     {}", self.button_frames)?;
        writeln!(f, "non-monotonic frames:  {}", self.non_monotonic)?;
        write!(f, "duplicate-id frames:   {}", self.duplicate_ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::DeviceId;

    #[test]
    fn timestamps_that_go_backwards_do_not_underflow() {
        let device = DeviceId(1);
        let frame = |ts| DeviceEvent::Frame {
            device,
            frame: TouchFrame {
                timestamp_us: ts,
                contacts: Vec::new(),
                button_pressed: false,
            },
        };
        let events = [frame(1_000_000), frame(500_000), frame(700_000)];
        let s = SessionStats::from_events(&events);
        assert_eq!(s.non_monotonic, 1);
        assert_eq!(s.duration_us, 500_000, "from the first frame to the lowest");
        assert_eq!(s.frames, 3);
    }

    #[test]
    fn devices_are_measured_separately() {
        let frame = |device, ts| DeviceEvent::Frame {
            device: DeviceId(device),
            frame: TouchFrame {
                timestamp_us: ts,
                contacts: Vec::new(),
                button_pressed: false,
            },
        };
        // Two devices whose clocks are not comparable.
        let events = [
            frame(1, 5_000_000),
            frame(2, 10_000),
            frame(1, 5_008_000),
            frame(2, 18_000),
        ];
        let s = SessionStats::from_events(&events);
        assert_eq!(s.non_monotonic, 0);
        assert_eq!(s.duration_us, 8_000);
        assert_eq!(s.interval_min_us, Some(8_000));
    }

    #[test]
    fn summarizing_is_incremental_and_repeatable() {
        let events = crate::capture::tests::sample_events();
        let mut one_at_a_time = SessionStats::default();
        for e in &events {
            one_at_a_time.add(e);
        }
        one_at_a_time.finish();
        assert_eq!(one_at_a_time, SessionStats::from_events(&events));
    }

    #[test]
    fn summarizes_sample_session() {
        let events = crate::capture::tests::sample_events();
        let s = SessionStats::from_events(&events);
        assert_eq!((s.devices, s.disconnects, s.frames), (1, 1, 2));
        assert_eq!(s.duration_us, 8_000);
        assert_eq!(s.max_touching, 2);
        assert_eq!(s.touching_histogram, vec![1, 0, 1]);
        assert_eq!(s.lifts, 1);
        assert_eq!(s.button_frames, 1);
        assert_eq!(s.interval_median_us, Some(8_000));
        assert_eq!((s.non_monotonic, s.duplicate_ids), (0, 0));
        assert!(s.to_string().contains("max fingers touching:  2"));
    }
}
