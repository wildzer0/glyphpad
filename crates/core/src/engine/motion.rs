//! Movement recognizer: swipes, edge swipes, pinches and rotations, decided
//! when the fingers lift, and click-and-drag.

use std::collections::BTreeMap;

use crate::config::{Direction, Edge};
use crate::trigger::{Turn, Zoom};

use super::session::{Point, Session, distance};
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

/// How the fingers moved during the peak phase of a session.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Motion {
    /// Travel of the centroid.
    pub dx: f32,
    pub dy: f32,
    pub distance_mm: f32,
    pub speed_mm_s: f32,
    /// Change of the mean distance of the fingers from their centroid:
    /// positive when they spread apart.
    pub spread_mm: f32,
    /// Mean turn of the fingers around their centroid, clockwise positive
    /// on screen (y grows downwards).
    pub turn_deg: f32,
}

fn centroid(points: &[Point]) -> Point {
    let n = points.len().max(1) as f32;
    let (x, y) = points
        .iter()
        .fold((0.0, 0.0), |a, p| (a.0 + p.0, a.1 + p.1));
    (x / n, y / n)
}

/// Spread and turn of the fingers present at both ends of the peak phase.
fn spread_and_turn(start: &BTreeMap<u32, Point>, end: &BTreeMap<u32, Point>) -> (f32, f32) {
    let common: Vec<(Point, Point)> = start
        .iter()
        .filter_map(|(id, a)| end.get(id).map(|b| (*a, *b)))
        .collect();
    if common.len() < 2 {
        return (0.0, 0.0);
    }
    let c0 = centroid(&common.iter().map(|p| p.0).collect::<Vec<_>>());
    let c1 = centroid(&common.iter().map(|p| p.1).collect::<Vec<_>>());
    let n = common.len() as f32;
    let spread0 = common.iter().map(|(a, _)| distance(*a, c0)).sum::<f32>() / n;
    let spread1 = common.iter().map(|(_, b)| distance(*b, c1)).sum::<f32>() / n;
    let turn = common
        .iter()
        .map(|(a, b)| {
            let a0 = (a.1 - c0.1).atan2(a.0 - c0.0);
            let a1 = (b.1 - c1.1).atan2(b.0 - c1.0);
            let mut d = (a1 - a0).to_degrees();
            while d > 180.0 {
                d -= 360.0;
            }
            while d < -180.0 {
                d += 360.0;
            }
            d
        })
        .sum::<f32>()
        / n;
    (spread1 - spread0, turn)
}

pub fn motion(s: &Session) -> Motion {
    let (a, b) = (s.peak_centroid_start, s.peak_centroid_end);
    let distance_mm = distance(a, b);
    let secs = (s.peak_end_us.saturating_sub(s.peak_start_us)) as f32 / 1e6;
    let (spread_mm, turn_deg) = spread_and_turn(&s.peak_start_fingers, &s.peak_end_fingers);
    Motion {
        dx: b.0 - a.0,
        dy: b.1 - a.1,
        distance_mm,
        speed_mm_s: if secs > 0.0 { distance_mm / secs } else { 0.0 },
        spread_mm,
        turn_deg,
    }
}

/// The edge zone the touch started in, if any (the nearest edge wins in a corner).
fn start_edge(s: &Session, zone: f32) -> Option<Edge> {
    let (x, y) = s.peak_start_norm;
    [
        (Edge::Left, x),
        (Edge::Right, 1.0 - x),
        (Edge::Top, y),
        (Edge::Bottom, 1.0 - y),
    ]
    .into_iter()
    .filter(|&(_, d)| d <= zone)
    .min_by(|a, b| a.1.total_cmp(&b.1))
    .map(|(e, _)| e)
}

/// Stateless: judged on the peak phase of the finished session, the
/// movement while all fingers were down; fingers lifting one after the
/// other at the end do not count. When a session could be several things
/// (fingers that spread while drifting sideways), the movement that exceeds
/// its threshold by the largest factor wins.
#[derive(Debug, Default)]
pub struct MotionRecognizer;

impl MotionRecognizer {
    pub fn session_ended(&self, s: &Session, spec: &Spec, t: &Thresholds) -> Option<Observation> {
        if s.consumed || s.chorded || s.clicked || s.max_fingers == 0 {
            return None;
        }
        let fingers = s.max_fingers.min(u8::MAX as usize) as u8;
        let m = motion(s);
        let swipe_like = spec.has_swipe(fingers) || spec.has_edge_swipe(fingers);
        let ratio = |enabled: bool, value: f32, min: f32| if enabled { value / min } else { 0.0 };
        let swipe = if m.speed_mm_s >= t.swipe_min_speed_mm_s {
            ratio(swipe_like, m.distance_mm, t.swipe_min_distance_mm)
        } else {
            0.0
        };
        let pinch = ratio(spec.has_pinch(fingers), m.spread_mm.abs(), t.pinch_min_mm);
        let rotate = ratio(
            spec.has_rotation(fingers),
            m.turn_deg.abs(),
            t.rotate_min_deg,
        );
        let best = swipe.max(pinch).max(rotate);
        if best < 1.0 {
            return None;
        }
        if best == pinch {
            return Some(Observation::Pinch {
                fingers,
                zoom: if m.spread_mm < 0.0 {
                    Zoom::In
                } else {
                    Zoom::Out
                },
                change_mm: m.spread_mm,
            });
        }
        if best == rotate {
            return Some(Observation::Rotate {
                fingers,
                turn: if m.turn_deg > 0.0 {
                    Turn::Clockwise
                } else {
                    Turn::Counterclockwise
                },
                degrees: m.turn_deg,
            });
        }
        let direction = quantize(m.dx, m.dy, spec.eight_way(fingers));
        if spec.has_edge_swipe(fingers)
            && let Some(edge) = start_edge(s, t.edge_zone)
        {
            return Some(Observation::EdgeSwipe {
                edge,
                fingers,
                direction,
                distance_mm: m.distance_mm,
            });
        }
        spec.has_swipe(fingers).then_some(Observation::Swipe {
            fingers,
            direction,
            distance_mm: m.distance_mm,
            speed_mm_s: m.speed_mm_s,
        })
    }
}

/// Click recognizer: a session with the physical button pressed, as a
/// click or, if the fingers travelled a swipe's distance, a click-and-drag.
#[derive(Debug, Default)]
pub struct ClickRecognizer;

impl ClickRecognizer {
    pub fn session_ended(&self, s: &Session, spec: &Spec, t: &Thresholds) -> Option<Observation> {
        if !s.clicked || s.consumed || s.chorded {
            return None;
        }
        let fingers = s.max_fingers.min(u8::MAX as usize) as u8;
        if !spec.has_click(fingers) {
            return None;
        }
        let m = motion(s);
        let direction = (m.distance_mm >= t.swipe_min_distance_mm)
            .then(|| quantize(m.dx, m.dy, spec.eight_way(fingers)));
        Some(Observation::Click { fingers, direction })
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
