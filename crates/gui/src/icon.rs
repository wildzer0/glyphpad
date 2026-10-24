//! The application icon, drawn in code so no image files or decoders are
//! needed: a rounded square with a drawn "L" stroke.

/// RGBA pixels of a `size`×`size` icon; gray when gestures are paused.
pub fn icon_rgba(size: u32, paused: bool) -> Vec<u8> {
    let s = size as f32;
    let bg: [f32; 3] = if paused {
        [110.0, 112.0, 120.0]
    } else {
        [52.0, 120.0, 246.0]
    };
    let radius = s * 0.22;
    // The stroke: down, then right, like the lock symbol.
    let path = [(0.34, 0.24), (0.34, 0.70), (0.74, 0.70)];
    let width = s * 0.11;
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            // Rounded-square coverage with one pixel of antialiasing.
            let dx = (px - s / 2.0).abs() - (s / 2.0 - radius);
            let dy = (py - s / 2.0).abs() - (s / 2.0 - radius);
            let outside = (dx.max(0.0).hypot(dy.max(0.0)) - radius).max(dx.max(dy).min(0.0));
            let shape = (0.5 - outside).clamp(0.0, 1.0);
            // Distance to the stroke polyline.
            let d = path
                .windows(2)
                .map(|w| {
                    let (ax, ay, bx, by) = (w[0].0 * s, w[0].1 * s, w[1].0 * s, w[1].1 * s);
                    let (vx, vy) = (bx - ax, by - ay);
                    let t =
                        (((px - ax) * vx + (py - ay) * vy) / (vx * vx + vy * vy)).clamp(0.0, 1.0);
                    (px - ax - t * vx).hypot(py - ay - t * vy)
                })
                .fold(f32::MAX, f32::min);
            let ink = (width / 2.0 + 0.5 - d).clamp(0.0, 1.0);
            let mix = |c: f32| (c * (1.0 - ink) + 255.0 * ink) as u8;
            out.extend([mix(bg[0]), mix(bg[1]), mix(bg[2]), (shape * 255.0) as u8]);
        }
    }
    out
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
}
