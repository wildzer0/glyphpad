//! Symbol templates: the on-disk format and the built-in defaults.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

/// One point of a drawn trace, with its time so playback can be faithful
/// (SPEC §10.1).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TracePoint {
    pub x_mm: f32,
    pub y_mm: f32,
    /// Microseconds since the first point of the sample.
    pub t_us: u64,
}

pub type Stroke = Vec<TracePoint>;

/// One recorded example of a symbol.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sample {
    pub strokes: Vec<Stroke>,
    /// Size of the touchpad it was drawn on, to show it in proportion.
    pub width_mm: f32,
    pub height_mm: f32,
}

impl Sample {
    pub fn xy_strokes(&self) -> Vec<Vec<(f32, f32)>> {
        self.strokes
            .iter()
            .map(|s| s.iter().map(|p| (p.x_mm, p.y_mm)).collect())
            .collect()
    }

    /// One sample holding the strokes of all of them, in order: the two
    /// halves of an X recorded apart become one X. The times are pushed along,
    /// with a pause between the parts, so the playback still makes sense.
    pub fn merged(samples: &[Sample]) -> Option<Self> {
        /// A pause between the parts, as if they had been drawn one after the
        /// other.
        const GAP_US: u64 = 100_000;
        let first = samples.first()?;
        let mut strokes: Vec<Stroke> = Vec::new();
        let mut offset = 0;
        for sample in samples {
            for stroke in &sample.strokes {
                strokes.push(
                    stroke
                        .iter()
                        .map(|p| TracePoint {
                            t_us: p.t_us + offset,
                            ..*p
                        })
                        .collect(),
                );
            }
            let end = sample
                .strokes
                .iter()
                .flatten()
                .map(|p| p.t_us)
                .max()
                .unwrap_or(0);
            offset += end + GAP_US;
        }
        Some(Self {
            strokes,
            width_mm: first.width_mm,
            height_mm: first.height_mm,
        })
    }

    /// One sample per stroke, each starting at time zero: a drawing that
    /// arrived as one can be kept as its separate strokes.
    pub fn split(&self) -> Vec<Self> {
        self.strokes
            .iter()
            .filter(|s| s.len() >= 2)
            .map(|stroke| {
                let start = stroke.first().map_or(0, |p| p.t_us);
                Self {
                    strokes: vec![
                        stroke
                            .iter()
                            .map(|p| TracePoint {
                                t_us: p.t_us - start,
                                ..*p
                            })
                            .collect(),
                    ],
                    width_mm: self.width_mm,
                    height_mm: self.height_mm,
                }
            })
            .collect()
    }
}

/// Version of the templates file format.
pub const TEMPLATES_FORMAT: u32 = 1;

/// `templates.json`: every user-recorded sample, by symbol name. Names are
/// case-sensitive (`q` and `Q` are different symbols), which is why this is
/// one file rather than one file per symbol on case-insensitive file systems.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateFile {
    pub format: u32,
    pub symbols: BTreeMap<String, Vec<Sample>>,
    /// Symbols whose built-in samples are not used.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub hidden_builtins: BTreeSet<String>,
}

impl Default for TemplateFile {
    fn default() -> Self {
        Self {
            format: TEMPLATES_FORMAT,
            symbols: BTreeMap::new(),
            hidden_builtins: BTreeSet::new(),
        }
    }
}

impl TemplateFile {
    pub fn parse(text: &str) -> Result<Self, String> {
        let file: Self = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if file.format > TEMPLATES_FORMAT {
            return Err(format!(
                "templates format {} is newer than this version supports ({TEMPLATES_FORMAT})",
                file.format
            ));
        }
        Ok(file)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("templates always serialize")
    }

    /// The templates in use: the built-in samples (unless hidden) plus the
    /// user's. Recorded samples add to the built-in ones rather than replace
    /// them: on the reference touchpad, five recorded L's alone no longer
    /// matched L's drawn a day earlier with a rising foot, which the built-in
    /// variants still cover.
    pub fn effective(&self) -> BTreeMap<String, Vec<Sample>> {
        let mut all = defaults();
        all.retain(|name, _| !self.hidden_builtins.contains(name));
        for (name, samples) in &self.symbols {
            all.entry(name.clone())
                .or_default()
                .extend(samples.iter().cloned());
        }
        all.retain(|_, samples| !samples.is_empty());
        all
    }
}

/// Shape of a built-in symbol, in a unit box with y growing downwards.
fn shape(name: &str) -> Vec<Vec<(f32, f32)>> {
    let arc = |cx: f32, cy: f32, r: f32, from_deg: f32, to_deg: f32| -> Vec<(f32, f32)> {
        let steps = 24;
        (0..=steps)
            .map(|i| {
                let a = (from_deg + (to_deg - from_deg) * i as f32 / steps as f32).to_radians();
                (cx + r * a.cos(), cy - r * a.sin())
            })
            .collect()
    };
    match name {
        "L" => vec![vec![(0.0, 0.0), (0.0, 1.0), (0.65, 1.0)]],
        // Lowercase q: the bowl drawn counter-clockwise from its right side,
        // then the stem down on the right.
        "q" => {
            let mut s = arc(0.35, 0.35, 0.35, 0.0, 360.0);
            s.push((0.7, 1.1));
            vec![s]
        }
        "X" => vec![vec![(0.0, 0.0), (1.0, 1.0)], vec![(1.0, 0.0), (0.0, 1.0)]],
        "C" => vec![arc(0.5, 0.5, 0.5, 50.0, 310.0)],
        "V" => vec![vec![(0.0, 0.0), (0.5, 1.0), (1.0, 0.0)]],
        "Z" => vec![vec![(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)]],
        "O" => vec![arc(0.5, 0.5, 0.5, 90.0, 450.0)],
        "arrow-up" => vec![
            vec![(0.5, 1.0), (0.5, 0.0)],
            vec![(0.15, 0.35), (0.5, 0.0), (0.85, 0.35)],
        ],
        "arrow-down" => vec![
            vec![(0.5, 0.0), (0.5, 1.0)],
            vec![(0.15, 0.65), (0.5, 1.0), (0.85, 0.65)],
        ],
        _ => Vec::new(),
    }
}

/// Built-in symbols (SPEC §6.2).
pub const DEFAULT_SYMBOLS: &[&str] = &["L", "q", "X", "C", "V", "Z", "O", "arrow-up", "arrow-down"];

/// Size the built-in shapes are drawn at, a comfortable symbol on a touchpad.
const DEFAULT_SIZE_MM: f32 = 40.0;
/// Speed of the synthetic strokes, a moderate drawing pace.
const DEFAULT_SPEED_MM_S: f32 = 100.0;

/// Built-in samples: each shape as drawn, plus narrower, wider and slanted
/// variants, since people do not draw symbols with perfect proportions.
pub fn defaults() -> BTreeMap<String, Vec<Sample>> {
    // (horizontal scale, vertical scale, slant). Uniform scaling keeps
    // proportions, so variants must cover how differently people draw them:
    // on the reference touchpad freehand L's had a foot 1.5× the stem.
    let variants: [(f32, f32, f32); 7] = [
        (1.0, 1.0, 0.0),
        (0.7, 1.0, 0.0),
        (1.5, 1.0, 0.0),
        (2.2, 1.0, 0.0),
        (1.0, 0.7, 0.0),
        (1.0, 1.0, 0.15),
        (1.0, 1.0, -0.15),
    ];
    DEFAULT_SYMBOLS
        .iter()
        .map(|&name| {
            let samples = variants
                .iter()
                .map(|&(sx, sy, slant)| {
                    let mut t = 0u64;
                    let mut last: Option<(f32, f32)> = None;
                    let strokes = shape(name)
                        .iter()
                        .map(|stroke| {
                            densify(stroke)
                                .into_iter()
                                .map(|(x, y)| {
                                    let x_mm = (x * sx + slant * (1.0 - y)) * DEFAULT_SIZE_MM;
                                    let y_mm = y * sy * DEFAULT_SIZE_MM;
                                    if let Some((lx, ly)) = last {
                                        let d = ((x_mm - lx).powi(2) + (y_mm - ly).powi(2)).sqrt();
                                        t += (d / DEFAULT_SPEED_MM_S * 1e6) as u64;
                                    }
                                    last = Some((x_mm, y_mm));
                                    TracePoint {
                                        x_mm,
                                        y_mm,
                                        t_us: t,
                                    }
                                })
                                .collect()
                        })
                        .collect();
                    Sample {
                        strokes,
                        width_mm: 155.8,
                        height_mm: 115.2,
                    }
                })
                .collect();
            (name.to_string(), samples)
        })
        .collect()
}

/// Adds points every 0.02 units along each segment, like a real trace.
fn densify(stroke: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let mut out = vec![stroke[0]];
    for w in stroke.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
        let steps = (len / 0.02).ceil().max(1.0) as usize;
        for i in 1..=steps {
            let k = i as f32 / steps as f32;
            out.push((a.0 + (b.0 - a.0) * k, a.1 + (b.1 - a.1) * k));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(x: f32, t_us: u64) -> TracePoint {
        TracePoint {
            x_mm: x,
            y_mm: 0.0,
            t_us,
        }
    }

    fn sample(strokes: Vec<Vec<TracePoint>>) -> Sample {
        Sample {
            strokes,
            width_mm: 155.8,
            height_mm: 115.2,
        }
    }

    #[test]
    fn two_examples_merge_into_one_with_a_pause_between() {
        let a = sample(vec![vec![point(0.0, 0), point(10.0, 100_000)]]);
        let b = sample(vec![vec![point(20.0, 0), point(30.0, 50_000)]]);
        let merged = Sample::merged(&[a, b]).expect("two samples");
        assert_eq!(merged.strokes.len(), 2);
        // The second part starts after the first (100 ms) plus a 100 ms pause.
        assert_eq!(merged.strokes[1][0].t_us, 200_000);
        assert_eq!(merged.strokes[1][1].t_us, 250_000);
        assert_eq!(merged.width_mm, 155.8);
        // Nothing to merge is nothing.
        assert!(Sample::merged(&[]).is_none());
    }

    #[test]
    fn one_example_divides_into_its_strokes() {
        let s = sample(vec![
            vec![point(0.0, 0), point(10.0, 40_000), point(20.0, 80_000)],
            vec![point(5.0, 200_000), point(15.0, 240_000)],
            // A single point is not a stroke worth keeping.
            vec![point(1.0, 300_000)],
        ]);
        let parts = s.split();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].strokes.len(), 1);
        assert_eq!(parts[0].strokes[0].len(), 3);
        // Each starts at time zero, so the playback of the part is its own.
        assert_eq!(parts[1].strokes[0][0].t_us, 0);
        assert_eq!(parts[1].strokes[0][1].t_us, 40_000);
        // One stroke alone has nothing to divide.
        assert!(
            sample(vec![vec![point(0.0, 0), point(1.0, 10)]])
                .split()
                .len()
                == 1
        );
    }

    #[test]
    fn every_default_has_samples() {
        let d = defaults();
        assert_eq!(d.len(), DEFAULT_SYMBOLS.len());
        for (name, samples) in &d {
            assert_eq!(samples.len(), 7, "{name}");
            assert!(samples.iter().all(|s| !s.strokes.is_empty()));
        }
        assert_eq!(d["X"][0].strokes.len(), 2);
    }

    #[test]
    fn user_samples_add_to_defaults() {
        let mut file = TemplateFile::default();
        let sample = Sample {
            strokes: vec![vec![
                TracePoint {
                    x_mm: 0.0,
                    y_mm: 0.0,
                    t_us: 0,
                },
                TracePoint {
                    x_mm: 10.0,
                    y_mm: 0.0,
                    t_us: 100,
                },
            ]],
            width_mm: 100.0,
            height_mm: 60.0,
        };
        file.symbols.insert("L".into(), vec![sample.clone()]);
        file.symbols.insert("hook".into(), vec![sample]);
        let eff = file.effective();
        assert_eq!(eff["L"].len(), 8);
        assert_eq!(eff["hook"].len(), 1);
        assert_eq!(eff["V"].len(), 7);
        file.hidden_builtins.insert("L".into());
        file.hidden_builtins.insert("V".into());
        let eff = file.effective();
        assert_eq!(eff["L"].len(), 1);
        assert!(!eff.contains_key("V"));
        let parsed = TemplateFile::parse(&file.to_json()).unwrap();
        assert_eq!(parsed, file);
    }
}
