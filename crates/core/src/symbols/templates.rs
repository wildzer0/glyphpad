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
