//! Small pictures of gestures for the cheat sheet (SPEC §13): dots for the
//! fingers, arrows for their movement, the template for a symbol. Drawn in
//! the unit square (x right, y down), so any surface can scale them.

use std::collections::BTreeMap;

use crate::config::{Direction, Edge};
use crate::symbols::Sample;
use crate::trigger::{Trigger, Turn, Zoom};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Pictogram {
    /// Open paths: strokes and arrows.
    pub lines: Vec<Vec<(f32, f32)>>,
    /// Fingers: `(x, y, ring)`; a ring is a finger that only rests.
    pub dots: Vec<(f32, f32, bool)>,
}

const MAX_SHOWN: u8 = 5;

fn unit(d: Direction) -> (f32, f32) {
    let s = std::f32::consts::FRAC_1_SQRT_2;
    match d {
        Direction::Up => (0.0, -1.0),
        Direction::Down => (0.0, 1.0),
        Direction::Left => (-1.0, 0.0),
        Direction::Right => (1.0, 0.0),
        Direction::UpLeft => (-s, -s),
        Direction::UpRight => (s, -s),
        Direction::DownLeft => (-s, s),
        Direction::DownRight => (s, s),
    }
}

/// A line from `from` to `to` with an arrowhead.
fn arrow(from: (f32, f32), to: (f32, f32)) -> Vec<Vec<(f32, f32)>> {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let len = (dx * dx + dy * dy).sqrt().max(1e-3);
    let (ux, uy) = (dx / len, dy / len);
    let barb = |angle: f32| {
        let (s, c) = angle.sin_cos();
        // The direction back along the arrow, turned by ±angle.
        let (bx, by) = (-ux * c + uy * s, -uy * c - ux * s);
        (to.0 + bx * 0.12, to.1 + by * 0.12)
    };
    vec![vec![from, to], vec![barb(0.5), to, barb(-0.5)]]
}

/// Finger dots side by side around `center`, perpendicular to `dir`.
fn fingers_at(center: (f32, f32), n: u8, dir: (f32, f32), ring: bool) -> Vec<(f32, f32, bool)> {
    let n = n.clamp(1, MAX_SHOWN);
    let (px, py) = (-dir.1, dir.0);
    (0..n)
        .map(|i| {
            let k = (f32::from(i) - f32::from(n - 1) / 2.0) * 0.11;
            (center.0 + px * k, center.1 + py * k, ring)
        })
        .collect()
}

fn arc(turn: Turn) -> Vec<(f32, f32)> {
    let steps = 24;
    let (sweep, start) = (std::f32::consts::PI * 1.5, -std::f32::consts::FRAC_PI_2);
    let sign = if turn == Turn::Clockwise { 1.0 } else { -1.0 };
    (0..=steps)
        .map(|i| {
            let a = start + sign * sweep * i as f32 / steps as f32;
            (0.5 + 0.32 * a.cos(), 0.5 + 0.32 * a.sin())
        })
        .collect()
}

/// Fits the first template of the first symbol into the unit square.
fn symbol(names: &[String], templates: &BTreeMap<String, Vec<Sample>>) -> Pictogram {
    let strokes = names
        .iter()
        .find_map(|n| templates.get(n).and_then(|t| t.first()))
        .map(|s| &s.strokes);
    let Some(strokes) = strokes else {
        return Pictogram::default();
    };
    let points = || strokes.iter().flatten();
    let min = |f: fn(&crate::symbols::TracePoint) -> f32| points().map(f).fold(f32::MAX, f32::min);
    let max = |f: fn(&crate::symbols::TracePoint) -> f32| points().map(f).fold(f32::MIN, f32::max);
    let (x0, x1) = (min(|p| p.x_mm), max(|p| p.x_mm));
    let (y0, y1) = (min(|p| p.y_mm), max(|p| p.y_mm));
    let k = 0.7 / (x1 - x0).max(y1 - y0).max(1e-3);
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    Pictogram {
        lines: strokes
            .iter()
            .map(|s| {
                s.iter()
                    .map(|p| (0.5 + (p.x_mm - cx) * k, 0.5 + (p.y_mm - cy) * k))
                    .collect()
            })
            .collect(),
        dots: Vec::new(),
    }
}

/// The picture of what a trigger asks the user to do.
pub fn of(trigger: &Trigger, templates: &BTreeMap<String, Vec<Sample>>) -> Pictogram {
    let mut p = Pictogram::default();
    match trigger {
        Trigger::Zone { regions, .. } => {
            let Some(region) = regions.first() else {
                return p;
            };
            // A schematic pad: the cheat sheet's pictogram is not to scale.
            const PAD: (f32, f32) = (150.0, 100.0);
            let x = |v: f32| (v / PAD.0).clamp(0.0, 1.0);
            let y = |v: f32| (v / PAD.1).clamp(0.0, 1.0);
            let (left, right) = (x(region.x[0]), x(region.x[1]));
            let (top, bottom) = (y(region.y[0]), y(region.y[1]));
            p.lines = vec![vec![
                (left, top),
                (right, top),
                (right, bottom),
                (left, bottom),
                (left, top),
            ]];
            p.dots = vec![((left + right) / 2.0, (top + bottom) / 2.0, false)];
        }
        Trigger::Tap {
            fingers, modifiers, ..
        } => {
            p.dots = fingers_at((0.5, 0.42), *fingers, (1.0, 0.0), false);
            if *modifiers > 0 {
                p.dots
                    .extend(fingers_at((0.5, 0.8), *modifiers, (1.0, 0.0), true));
            }
        }
        Trigger::Hold { fingers } => {
            p.dots = fingers_at((0.5, 0.5), *fingers, (1.0, 0.0), true);
        }
        Trigger::Swipe {
            fingers,
            direction,
            modifiers,
            ..
        } => {
            let d = unit(*direction);
            let (from, to) = (
                (0.5 - d.0 * 0.3, 0.5 - d.1 * 0.3),
                (0.5 + d.0 * 0.3, 0.5 + d.1 * 0.3),
            );
            p.lines = arrow(from, to);
            p.dots = fingers_at(from, *fingers, d, false);
            if *modifiers > 0 {
                p.dots
                    .extend(fingers_at((0.5, 0.88), *modifiers, (1.0, 0.0), true));
            }
        }
        Trigger::EdgeSwipe {
            edge,
            fingers,
            direction,
            ..
        } => {
            let d = unit(*direction);
            let (from, rail) = match edge {
                Edge::Left => ((0.1, 0.5), vec![(0.04, 0.1), (0.04, 0.9)]),
                Edge::Right => ((0.9, 0.5), vec![(0.96, 0.1), (0.96, 0.9)]),
                Edge::Top => ((0.5, 0.12), vec![(0.1, 0.04), (0.9, 0.04)]),
                Edge::Bottom => ((0.5, 0.88), vec![(0.1, 0.96), (0.9, 0.96)]),
            };
            let to = (
                (from.0 + d.0 * 0.55).clamp(0.05, 0.95),
                (from.1 + d.1 * 0.55).clamp(0.05, 0.95),
            );
            p.lines = arrow(from, to);
            p.lines.push(rail);
            p.dots = fingers_at(from, (*fingers).min(2), d, false);
        }
        Trigger::Pinch { zoom, .. } => {
            let (near, far) = (0.1, 0.34);
            let (a, b) = if *zoom == Zoom::In {
                (far, near)
            } else {
                (near, far)
            };
            for s in [-1.0_f32, 1.0] {
                p.lines.extend(arrow(
                    (0.5 + s * a, 0.5 + s * a * 0.6),
                    (0.5 + s * b, 0.5 + s * b * 0.6),
                ));
            }
        }
        Trigger::Rotate { turn, .. } => {
            let path = arc(*turn);
            let n = path.len();
            let tip = path[n - 1];
            let before = path[n - 3];
            p.lines.push(path);
            p.lines.extend(arrow(before, tip).into_iter().skip(1));
        }
        Trigger::Click { fingers, direction } => {
            p.dots = fingers_at((0.5, 0.45), *fingers, (1.0, 0.0), false);
            if let Some(d) = direction {
                let u = unit(*d);
                p.lines = arrow((0.5, 0.45), (0.5 + u.0 * 0.35, 0.45 + u.1 * 0.35));
            } else {
                p.dots.push((0.5, 0.85, true));
            }
        }
        Trigger::Symbol { names, .. } => p = symbol(names, templates),
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trigger::TriggerDef;

    fn build(toml: &str) -> Trigger {
        let def: TriggerDef = toml::from_str(toml).expect("a trigger");
        def.build().expect("valid")
    }

    #[test]
    fn every_kind_has_a_picture_inside_the_unit_square() {
        for t in [
            "type = \"tap\"\nfingers = 3",
            "type = \"tap\"\nfingers = 2\nmodifier_fingers = 1",
            "type = \"hold\"\nfingers = 5",
            "type = \"swipe\"\nfingers = 4\ndirection = \"up_left\"",
            "type = \"edge_swipe\"\nfingers = 1\nedge = \"right\"\ndirection = \"left\"",
            "type = \"pinch\"\nfingers = 2\nzoom = \"in\"",
            "type = \"pinch\"\nfingers = 2\nzoom = \"out\"",
            "type = \"rotate\"\nfingers = 2\nrotation = \"counterclockwise\"",
            "type = \"click\"\nfingers = 1",
            "type = \"click\"\nfingers = 2\ndirection = \"down\"",
        ] {
            let p = of(&build(t), &BTreeMap::new());
            assert!(!p.lines.is_empty() || !p.dots.is_empty(), "{t}");
            let all = p
                .lines
                .iter()
                .flatten()
                .copied()
                .chain(p.dots.iter().map(|d| (d.0, d.1)));
            for (x, y) in all {
                assert!(
                    (-0.01..=1.01).contains(&x) && (-0.01..=1.01).contains(&y),
                    "{t}: {x},{y}"
                );
            }
        }
    }

    #[test]
    fn a_symbol_is_drawn_from_its_template() {
        use crate::symbols::{Sample, TracePoint};
        let tp = |x, y| TracePoint {
            x_mm: x,
            y_mm: y,
            t_us: 0,
        };
        let mut templates = BTreeMap::new();
        templates.insert(
            "L".to_string(),
            vec![Sample {
                strokes: vec![vec![tp(10.0, 10.0), tp(10.0, 40.0), tp(30.0, 40.0)]],
                width_mm: 100.0,
                height_mm: 70.0,
            }],
        );
        let t = Trigger::Symbol {
            names: vec!["L".into()],
            confidence: None,
        };
        let p = of(&t, &templates);
        assert_eq!(p.lines.len(), 1);
        assert_eq!(p.lines[0].len(), 3);
        let missing = Trigger::Symbol {
            names: vec!["nope".into()],
            confidence: None,
        };
        assert_eq!(of(&missing, &templates), Pictogram::default());
    }
}
