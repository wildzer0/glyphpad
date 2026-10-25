//! The application icon, drawn in code so no image files or decoders are
//! needed: a rounded tile with a gesture trail sweeping across it, the pad
//! itself and the stroke that gives the program its name.
//!
//! It lives in the core because two things draw it from here: the window and
//! the tray (with `glyphpad-gui`) and the build script of the executable,
//! which compiles it into a Windows resource so the file has the same icon.

/// RGBA pixels of a `size`×`size` icon; grey when gestures are paused.
pub fn icon_rgba(size: u32, paused: bool) -> Vec<u8> {
    let s = size as f32;
    let tile: [f32; 3] = if paused {
        [110.0, 112.0, 120.0]
    } else {
        [52.0, 120.0, 246.0]
    };
    let radius = s * 0.22;
    // The trail: a cubic curve that rises, dips and rises again, as a finger
    // drawing a letter would. Points are fractions of the icon.
    let trail = curve((0.24, 0.70), (0.30, 0.24), (0.62, 0.80), (0.78, 0.34), 48);
    let trail_width = s * 0.10;
    // Where the finger touched down.
    let dot = (trail[0].0 * s, trail[0].1 * s);
    let dot_radius = s * 0.085;
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            // Rounded-square tile, with one pixel of antialiasing.
            let dx = (px - s / 2.0).abs() - (s / 2.0 - radius);
            let dy = (py - s / 2.0).abs() - (s / 2.0 - radius);
            // Signed distance to the rounded square, negative inside.
            let outside = dx.max(0.0).hypot(dy.max(0.0)) + dx.max(dy).min(0.0) - radius;
            let shape = (0.5 - outside).clamp(0.0, 1.0);
            // The trail, as the distance to the curve.
            let scaled: Vec<(f32, f32)> = trail.iter().map(|p| (p.0 * s, p.1 * s)).collect();
            let d = polyline_distance(px, py, &scaled);
            let stroke = (trail_width / 2.0 + 0.5 - d).clamp(0.0, 1.0);
            // The finger at its start.
            let dot_ink = (dot_radius + 0.5 - (px - dot.0).hypot(py - dot.1)).clamp(0.0, 1.0);
            let ink = stroke.max(dot_ink);
            let mix = |c: f32| (c * (1.0 - ink) + 255.0 * ink) as u8;
            out.extend([
                mix(tile[0]),
                mix(tile[1]),
                mix(tile[2]),
                (shape * 255.0) as u8,
            ]);
        }
    }
    out
}

/// `steps` points of a cubic Bézier curve, in fractions of the icon.
fn curve(
    p0: (f32, f32),
    p1: (f32, f32),
    p2: (f32, f32),
    p3: (f32, f32),
    steps: usize,
) -> Vec<(f32, f32)> {
    (0..=steps)
        .map(|i| {
            let t = i as f32 / steps as f32;
            let u = 1.0 - t;
            let x = u * u * u * p0.0
                + 3.0 * u * u * t * p1.0
                + 3.0 * u * t * t * p2.0
                + t * t * t * p3.0;
            let y = u * u * u * p0.1
                + 3.0 * u * u * t * p1.1
                + 3.0 * u * t * t * p2.1
                + t * t * t * p3.1;
            (x, y)
        })
        .collect()
}

/// Distance from a point to a polyline, round caps included.
fn polyline_distance(px: f32, py: f32, points: &[(f32, f32)]) -> f32 {
    points
        .windows(2)
        .map(|w| {
            let (ax, ay, bx, by) = (w[0].0, w[0].1, w[1].0, w[1].1);
            let (vx, vy) = (bx - ax, by - ay);
            let len = vx * vx + vy * vy;
            let t = if len > 0.0 {
                (((px - ax) * vx + (py - ay) * vy) / len).clamp(0.0, 1.0)
            } else {
                0.0
            };
            (px - ax - t * vx).hypot(py - ay - t * vy)
        })
        .fold(f32::MAX, f32::min)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_has_the_right_size_and_transparent_corners() {
        let px = icon_rgba(32, false);
        assert_eq!(px.len(), 32 * 32 * 4);
        assert_eq!(px[3], 0, "top-left corner is transparent");
        let center = ((16 * 32 + 16) * 4 + 3) as usize;
        assert_eq!(px[center], 255);
    }

    #[test]
    fn the_trail_is_lighter_than_the_tile() {
        // A point on the curve, well inside the tile.
        let px = icon_rgba(64, false);
        let at = |x: u32, y: u32| {
            let i = ((y * 64 + x) * 4) as usize;
            (px[i], px[i + 1], px[i + 2])
        };
        let (r, _, _) = at(20, 30);
        assert!(r > 200, "the trail is white, got {r}");
    }

    #[test]
    fn a_paused_icon_is_grey() {
        // Every opaque pixel is a shade of grey or white: no blue left.
        // The paused tile is a cool grey, so the channels are close, not equal.
        for chunk in icon_rgba(32, true).chunks(4).filter(|c| c[3] == 255) {
            let (lo, hi) = (
                chunk[..3].iter().min().copied().unwrap_or(0),
                chunk[..3].iter().max().copied().unwrap_or(0),
            );
            assert!(hi - lo <= 12, "{chunk:?} is not grey");
        }
    }
}
