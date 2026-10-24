//! The $Q point-cloud recognizer (Vatavu, Anthony, Wobbrock, "$Q: A
//! Super-Quick, Articulation-Invariant Stroke-Gesture Recognizer for
//! Low-Resource Devices", MobileHCI 2018).
//!
//! A gesture is a cloud of points: the order and direction of its strokes do
//! not matter, so an X drawn as `\` then `/` or `/` then `\` is the same X.
//! $Q gives the same results as $P (the greedy cloud matching) and is much
//! faster thanks to two lower bounds computed from a lookup table, which let
//! it skip most of the matching work.

/// Points per normalized cloud. 32 is enough for $P's accuracy on simple
/// symbols; the paper recommends 64 for $Q at no real cost.
pub const N: usize = 64;
/// Clouds are mapped onto integer-like coordinates `0..MAX_COORD`.
const MAX_COORD: f32 = 1024.0;
/// Side of the lookup table of nearest points.
const LUT_SIZE: usize = 64;
const LUT_SCALE: f32 = MAX_COORD / LUT_SIZE as f32;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
    pub stroke: u32,
}

fn sq_dist(a: &Point, b: &Point) -> f32 {
    (a.x - b.x).powi(2) + (a.y - b.y).powi(2)
}

fn path_length(points: &[Point]) -> f32 {
    points
        .windows(2)
        .filter(|w| w[0].stroke == w[1].stroke)
        .map(|w| sq_dist(&w[0], &w[1]).sqrt())
        .sum()
}

/// Resamples to `n` points equally spaced along the strokes, never
/// interpolating across the gap between two strokes.
fn resample(points: &[Point], n: usize) -> Vec<Point> {
    let interval = path_length(points) / (n - 1) as f32;
    let mut pts = points.to_vec();
    let mut out = vec![pts[0]];
    let mut acc = 0.0;
    let mut i = 1;
    while i < pts.len() {
        if pts[i].stroke == pts[i - 1].stroke {
            let d = sq_dist(&pts[i - 1], &pts[i]).sqrt();
            if acc + d >= interval && d > 0.0 {
                let k = (interval - acc) / d;
                let q = Point {
                    x: pts[i - 1].x + k * (pts[i].x - pts[i - 1].x),
                    y: pts[i - 1].y + k * (pts[i].y - pts[i - 1].y),
                    stroke: pts[i].stroke,
                };
                out.push(q);
                pts.insert(i, q);
                acc = 0.0;
            } else {
                acc += d;
            }
        }
        i += 1;
    }
    // Rounding can leave the last point out.
    while out.len() < n {
        out.push(*pts.last().unwrap_or(&out[0]));
    }
    out.truncate(n);
    out
}

/// A normalized point cloud with its lookup table.
#[derive(Debug, Clone)]
pub struct Cloud {
    points: Vec<Point>,
    /// For each LUT cell, the index of the nearest point.
    lut: Vec<u8>,
}

impl Cloud {
    /// Builds a cloud from strokes of `(x, y)` points in any unit, rotated
    /// by `rotation_deg` around their centroid. Returns `None` for input
    /// without extent (a tap, a single point).
    pub fn new(strokes: &[Vec<(f32, f32)>], rotation_deg: f32) -> Option<Self> {
        let mut points: Vec<Point> = strokes
            .iter()
            .enumerate()
            .flat_map(|(i, s)| {
                s.iter().map(move |&(x, y)| Point {
                    x,
                    y,
                    stroke: i as u32,
                })
            })
            .collect();
        if points.len() < 2 || path_length(&points) <= f32::EPSILON {
            return None;
        }
        if rotation_deg != 0.0 {
            let (cx, cy) = centroid(&points);
            let (s, c) = rotation_deg.to_radians().sin_cos();
            for p in &mut points {
                let (dx, dy) = (p.x - cx, p.y - cy);
                p.x = cx + dx * c - dy * s;
                p.y = cy + dx * s + dy * c;
            }
        }
        let mut points = resample(&points, N);

        // Uniform scale, preserving the aspect ratio: an L stays an L and a
        // line does not blow up into a square.
        let (min_x, max_x, min_y, max_y) = points.iter().fold(
            (f32::MAX, f32::MIN, f32::MAX, f32::MIN),
            |(a, b, c, d), p| (a.min(p.x), b.max(p.x), c.min(p.y), d.max(p.y)),
        );
        let size = (max_x - min_x).max(max_y - min_y);
        if size <= f32::EPSILON {
            return None;
        }
        for p in &mut points {
            p.x = (p.x - min_x) / size;
            p.y = (p.y - min_y) / size;
        }
        // Centroid at the origin, then onto 0..MAX_COORD.
        let (cx, cy) = centroid(&points);
        for p in &mut points {
            p.x = ((p.x - cx + 1.0) / 2.0 * (MAX_COORD - 1.0)).clamp(0.0, MAX_COORD - 1.0);
            p.y = ((p.y - cy + 1.0) / 2.0 * (MAX_COORD - 1.0)).clamp(0.0, MAX_COORD - 1.0);
        }

        let mut lut = vec![0u8; LUT_SIZE * LUT_SIZE];
        for cx in 0..LUT_SIZE {
            for cy in 0..LUT_SIZE {
                let mut best = (f32::MAX, 0usize);
                for (i, p) in points.iter().enumerate() {
                    let (px, py) = lut_cell(p);
                    let d = (px as f32 - cx as f32).powi(2) + (py as f32 - cy as f32).powi(2);
                    if d < best.0 {
                        best = (d, i);
                    }
                }
                lut[cx * LUT_SIZE + cy] = best.1 as u8;
            }
        }
        Some(Self { points, lut })
    }

    fn nearest(&self, p: &Point) -> usize {
        let (x, y) = lut_cell(p);
        usize::from(self.lut[x * LUT_SIZE + y])
    }
}

fn centroid(points: &[Point]) -> (f32, f32) {
    let n = points.len() as f32;
    let (sx, sy) = points
        .iter()
        .fold((0.0, 0.0), |(x, y), p| (x + p.x, y + p.y));
    (sx / n, sy / n)
}

fn lut_cell(p: &Point) -> (usize, usize) {
    let clamp = |v: f32| ((v / LUT_SCALE).round() as usize).min(LUT_SIZE - 1);
    (clamp(p.x), clamp(p.y))
}

/// Greedy matching of `a` onto `b` starting at point `start`: each point of
/// `a` takes the nearest unmatched point of `b`, with decreasing weights.
/// Stops early once the sum reaches `best_so_far`.
fn cloud_distance(a: &Cloud, b: &Cloud, start: usize, best_so_far: f32) -> f32 {
    let mut unmatched: Vec<usize> = (0..N).collect();
    let mut i = start;
    let mut weight = N as f32;
    let mut sum = 0.0;
    loop {
        let (k, d) = unmatched
            .iter()
            .enumerate()
            .map(|(k, &j)| (k, sq_dist(&a.points[i], &b.points[j])))
            .fold((0, f32::MAX), |best, x| if x.1 < best.1 { x } else { best });
        unmatched.swap_remove(k);
        sum += weight * d;
        if sum >= best_so_far {
            return sum;
        }
        weight -= 1.0;
        i = (i + 1) % N;
        if i == start {
            return sum;
        }
    }
}

/// Lower bounds of `cloud_distance(a, b, start)` for every start that is a
/// multiple of `step`, from the distance of each point of `a` to its nearest
/// point of `b` (ignoring that points can be matched only once).
fn lower_bounds(a: &Cloud, b: &Cloud, step: usize) -> Vec<f32> {
    let mut lb = vec![0.0; N / step + 1];
    let mut sat = [0.0f32; N];
    for i in 0..N {
        let d = sq_dist(&a.points[i], &b.points[b.nearest(&a.points[i])]);
        sat[i] = if i == 0 { d } else { sat[i - 1] + d };
        lb[0] += (N - i) as f32 * d;
    }
    for i in (step..N).step_by(step) {
        lb[i / step] = lb[0] + i as f32 * sat[N - 1] - N as f32 * sat[i - 1];
    }
    lb
}

/// Distance between two clouds, or a value ≥ `best_so_far` when they cannot
/// beat it.
pub fn distance(a: &Cloud, b: &Cloud, mut best_so_far: f32) -> f32 {
    let step = (N as f32).sqrt() as usize;
    let lb_ab = lower_bounds(a, b, step);
    let lb_ba = lower_bounds(b, a, step);
    for i in (0..N).step_by(step) {
        if lb_ab[i / step] < best_so_far {
            best_so_far = best_so_far.min(cloud_distance(a, b, i, best_so_far));
        }
        if lb_ba[i / step] < best_so_far {
            best_so_far = best_so_far.min(cloud_distance(b, a, i, best_so_far));
        }
    }
    best_so_far
}

/// The same distance without lower bounds or early abandoning: the
/// reference `distance` must agree with exactly.
pub fn distance_exhaustive(a: &Cloud, b: &Cloud) -> f32 {
    let step = (N as f32).sqrt() as usize;
    (0..N)
        .step_by(step)
        .flat_map(|i| {
            [
                cloud_distance(a, b, i, f32::MAX),
                cloud_distance(b, a, i, f32::MAX),
            ]
        })
        .fold(f32::MAX, f32::min)
}

/// Root-mean-square point distance implied by a cloud distance, as a
/// fraction of the normalized gesture size.
///
/// The distance is a sum of squared distances weighted N, N−1, …, 1;
/// dividing by the total weight gives a mean square in cloud coordinates,
/// where the gesture's larger side spans half of `MAX_COORD`.
pub fn rms_fraction(distance: f32) -> f32 {
    let total_weight = (N * (N + 1) / 2) as f32;
    (distance / total_weight).sqrt() / ((MAX_COORD - 1.0) / 2.0)
}
