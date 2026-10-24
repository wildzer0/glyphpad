//! Drawn-symbol recognition (SPEC §6.2).

pub mod dollar_q;
pub mod templates;

use std::collections::BTreeMap;

use dollar_q::Cloud;
pub use templates::{Sample, Stroke, TemplateFile, TracePoint};

/// A symbol's similarity to the drawn trace, 0..1.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub symbol: String,
    pub score: f32,
}

/// RMS point distance, as a fraction of the symbol's size, at which the
/// score drops to 0. Calibrated on the reference touchpad: three freehand
/// three-finger L's against the built-in L were 5–6 % of their size apart
/// (score ≈ 0.85), while the nearest other symbol was 12.5 % or more apart
/// (score ≤ 0.69), leaving the default confidence of 0.80 in between.
const ZERO_SCORE_RMS: f32 = 0.4;

pub fn score(distance: f32) -> f32 {
    (1.0 - dollar_q::rms_fraction(distance) / ZERO_SCORE_RMS).clamp(0.0, 1.0)
}

struct Template {
    symbol: String,
    cloud: Cloud,
}

/// Matches traces against every template of every symbol.
pub struct SymbolRecognizer {
    templates: Vec<Template>,
    /// Candidate rotations, in degrees, tried within the configured tolerance.
    rotations: Vec<f32>,
}

impl SymbolRecognizer {
    /// `rotation_tolerance_deg` of 0 keeps symbols orientation-sensitive (a
    /// rotated L is not an L); a positive value also tries the trace rotated
    /// by up to that many degrees either way, in steps of at most 5°.
    pub fn new(symbols: &BTreeMap<String, Vec<Sample>>, rotation_tolerance_deg: f32) -> Self {
        let templates = symbols
            .iter()
            .flat_map(|(name, samples)| {
                samples.iter().filter_map(move |s| {
                    Cloud::new(&s.xy_strokes(), 0.0).map(|cloud| Template {
                        symbol: name.clone(),
                        cloud,
                    })
                })
            })
            .collect();
        let tol = rotation_tolerance_deg.abs().min(180.0);
        let steps = (tol / 5.0).ceil() as i32;
        let rotations = (-steps..=steps)
            .map(|i| {
                if steps == 0 {
                    0.0
                } else {
                    tol * i as f32 / steps as f32
                }
            })
            .collect();
        Self {
            templates,
            rotations,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.templates.is_empty()
    }

    /// The best score of each symbol, best first. Empty when the trace has
    /// no extent.
    pub fn recognize(&self, strokes: &[Vec<(f32, f32)>]) -> Vec<Candidate> {
        let clouds: Vec<Cloud> = self
            .rotations
            .iter()
            .filter_map(|&r| Cloud::new(strokes, r))
            .collect();
        if clouds.is_empty() {
            return Vec::new();
        }
        let mut best: BTreeMap<&str, f32> = BTreeMap::new();
        for t in &self.templates {
            let entry = best.entry(&t.symbol).or_insert(f32::MAX);
            for c in &clouds {
                *entry = dollar_q::distance(c, &t.cloud, *entry);
            }
        }
        let mut out: Vec<Candidate> = best
            .into_iter()
            .map(|(symbol, d)| Candidate {
                symbol: symbol.to_string(),
                score: score(d),
            })
            .collect();
        out.sort_by(|a, b| b.score.total_cmp(&a.score));
        out
    }
}

/// Quality of freshly recorded samples of a symbol (SPEC §10.3).
#[derive(Debug, Clone, PartialEq)]
pub struct Assessment {
    /// For each sample, its score against the other new samples: a low one
    /// marks a sample unlike the rest.
    pub consistency: Vec<f32>,
    /// For each sample, the most similar *other* existing symbol.
    pub nearest_other: Vec<Option<Candidate>>,
}

pub fn assess(
    name: &str,
    samples: &[Sample],
    existing: &BTreeMap<String, Vec<Sample>>,
) -> Assessment {
    let consistency = (0..samples.len())
        .map(|i| {
            let rest: Vec<Sample> = samples
                .iter()
                .enumerate()
                .filter(|&(j, _)| j != i)
                .map(|(_, s)| s.clone())
                .collect();
            if rest.is_empty() {
                return 1.0;
            }
            let r = SymbolRecognizer::new(&BTreeMap::from([(name.to_string(), rest)]), 0.0);
            r.recognize(&samples[i].xy_strokes())
                .first()
                .map_or(0.0, |c| c.score)
        })
        .collect();
    let others: BTreeMap<String, Vec<Sample>> = existing
        .iter()
        .filter(|(n, _)| n.as_str() != name)
        .map(|(n, s)| (n.clone(), s.clone()))
        .collect();
    let r = SymbolRecognizer::new(&others, 0.0);
    let nearest_other = samples
        .iter()
        .map(|s| r.recognize(&s.xy_strokes()).into_iter().next())
        .collect();
    Assessment {
        consistency,
        nearest_other,
    }
}

#[cfg(test)]
mod tests;
