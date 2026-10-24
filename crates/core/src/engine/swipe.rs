//! Swipe recognizer, decided when the fingers lift.

use crate::config::Direction;

use super::session::{Session, distance};
use super::{Observation, Spec, Thresholds};

/// Direction of a movement in touchpad coordinates (y grows downwards),
/// quantized to 4 sectors of 90° or 8 of 45°.
pub fn quantize(dx: f32, dy: f32, eight_way: bool) -> Direction {
    let angle = (-dy).atan2(dx).to_degrees(); // counter-clockwise from "right"
    let angle = if angle < 0.0 { angle + 360.0 } else { angle };
    if eight_way {
        const DIRS: [Direction; 8] = [
            Direction::Right,
            Direction::UpRight,
            Direction::Up,
            Direction::UpLeft,
            Direction::Left,
            Direction::DownLeft,
            Direction::Down,
            Direction::DownRight,
        ];
        DIRS[(((angle + 22.5) / 45.0) as usize) % 8]
    } else {
        const DIRS: [Direction; 4] = [
            Direction::Right,
            Direction::Up,
            Direction::Left,
            Direction::Down,
        ];
        DIRS[(((angle + 45.0) / 90.0) as usize) % 4]
    }
}

/// Stateless: a swipe is judged on the peak phase of the finished session,
/// the movement of the fingers' centroid while all of them were down. The
/// fingers lifting one after the other at the end do not count.
#[derive(Debug, Default)]
pub struct SwipeRecognizer;

impl SwipeRecognizer {
    pub fn session_ended(&self, s: &Session, spec: &Spec, t: &Thresholds) -> Option<Observation> {
        if s.consumed || s.clicked || s.max_fingers == 0 {
            return None;
        }
        let (a, b) = (s.peak_centroid_start, s.peak_centroid_end);
        let dist = distance(a, b);
        let secs = (s.peak_end_us.saturating_sub(s.peak_start_us)) as f32 / 1e6;
        let speed = if secs > 0.0 { dist / secs } else { 0.0 };
        if dist < t.swipe_min_distance_mm || speed < t.swipe_min_speed_mm_s {
            return None;
        }
        let fingers = s.max_fingers.min(u8::MAX as usize) as u8;
        Some(Observation::Swipe {
            fingers,
            direction: quantize(b.0 - a.0, b.1 - a.1, spec.eight_way(fingers)),
            distance_mm: dist,
            speed_mm_s: speed,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_way() {
        assert_eq!(quantize(10.0, 0.0, false), Direction::Right);
        assert_eq!(quantize(0.0, -10.0, false), Direction::Up);
        assert_eq!(quantize(-10.0, 1.0, false), Direction::Left);
        assert_eq!(quantize(2.0, 10.0, false), Direction::Down);
        assert_eq!(quantize(10.0, -9.0, false), Direction::Right); // 42°
        assert_eq!(quantize(9.0, -10.0, false), Direction::Up); // 48°
        assert_eq!(quantize(10.0, 0.5, false), Direction::Right); // just below horizontal
    }

    #[test]
    fn eight_way() {
        assert_eq!(quantize(10.0, -10.0, true), Direction::UpRight);
        assert_eq!(quantize(-10.0, -10.0, true), Direction::UpLeft);
        assert_eq!(quantize(-10.0, 10.0, true), Direction::DownLeft);
        assert_eq!(quantize(10.0, 10.0, true), Direction::DownRight);
        assert_eq!(quantize(10.0, -3.0, true), Direction::Right); // 17°
        assert_eq!(quantize(0.0, 10.0, true), Direction::Down);
    }
}
