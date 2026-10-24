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
    /// From the first to the last frame.
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
}

impl SessionStats {
    pub fn from_events<'a>(events: impl IntoIterator<Item = &'a DeviceEvent>) -> Self {
        let mut s = Self::default();
        let mut first_ts = None;
        let mut last_ts: Option<u64> = None;
        let mut intervals = Vec::new();
        for event in events {
            match event {
                DeviceEvent::Connected(_) => s.devices += 1,
                DeviceEvent::Disconnected { .. } => s.disconnects += 1,
                DeviceEvent::Frame { frame, .. } => {
                    let ts = frame.timestamp_us;
                    first_ts.get_or_insert(ts);
                    if let Some(prev) = last_ts {
                        if ts < prev {
                            s.non_monotonic += 1;
                        } else {
                            intervals.push(ts - prev);
                        }
                    }
                    last_ts = Some(ts);
                    s.add_frame(frame);
                }
            }
        }
        s.duration_us = last_ts.unwrap_or(0) - first_ts.unwrap_or(0);
        intervals.sort_unstable();
        s.interval_min_us = intervals.first().copied();
        s.interval_max_us = intervals.last().copied();
        s.interval_median_us = intervals.get(intervals.len() / 2).copied();
        s
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
