use proptest::prelude::*;

use super::dollar_q::{Cloud, distance, distance_exhaustive};
use super::templates::defaults;
use super::*;

fn recognizer() -> SymbolRecognizer {
    SymbolRecognizer::new(&defaults(), 0.0)
}

fn shape(name: &str, variant: usize) -> Vec<Vec<(f32, f32)>> {
    defaults()[name][variant].xy_strokes()
}

fn transform(
    strokes: &[Vec<(f32, f32)>],
    f: impl Fn((f32, f32)) -> (f32, f32),
) -> Vec<Vec<(f32, f32)>> {
    strokes
        .iter()
        .map(|s| s.iter().map(|&p| f(p)).collect())
        .collect()
}

#[test]
fn each_default_recognizes_itself() {
    let r = recognizer();
    for name in templates::DEFAULT_SYMBOLS {
        let best = &r.recognize(&shape(name, 0))[0];
        assert_eq!(best.symbol, *name);
        assert!(best.score > 0.99, "{name}: {}", best.score);
    }
}

#[test]
fn stroke_order_and_direction_do_not_matter() {
    let r = recognizer();
    let x = shape("X", 0);
    let swapped = vec![x[1].iter().rev().copied().collect(), x[0].clone()];
    let a = r.recognize(&x)[0].clone();
    let b = r.recognize(&swapped)[0].clone();
    assert_eq!(a.symbol, "X");
    assert_eq!(b.symbol, "X");
    assert!(
        (a.score - b.score).abs() < 0.05,
        "{} vs {}",
        a.score,
        b.score
    );
}

#[test]
fn rotation_matters_unless_tolerated() {
    // An L turned by 90° clockwise is not an L.
    let l = shape("L", 0);
    let turned = transform(&l, |(x, y)| (-y, x));
    let strict = recognizer().recognize(&turned);
    let l_score = strict.iter().find(|c| c.symbol == "L").unwrap().score;
    assert!(l_score < 0.6, "{l_score}");

    // A slight tilt is tolerated when configured.
    let (s, c) = 20f32.to_radians().sin_cos();
    let tilted = transform(&l, |(x, y)| (x * c - y * s, x * s + y * c));
    let strict = recognizer().recognize(&tilted)[0].score;
    let tolerant = SymbolRecognizer::new(&defaults(), 25.0).recognize(&tilted);
    assert_eq!(tolerant[0].symbol, "L");
    assert!(tolerant[0].score > strict);
}

#[test]
fn degenerate_traces_have_no_candidates() {
    assert!(recognizer().recognize(&[vec![(1.0, 1.0)]]).is_empty());
    assert!(
        recognizer()
            .recognize(&[vec![(1.0, 1.0), (1.0, 1.0)]])
            .is_empty()
    );
}

fn arb_strokes() -> impl Strategy<Value = Vec<Vec<(f32, f32)>>> {
    prop::collection::vec(
        prop::collection::vec((0.0f32..50.0, 0.0f32..50.0), 2..20),
        1..3,
    )
    .prop_filter("needs extent", |s| Cloud::new(s, 0.0).is_some())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// SPEC §8: translating or scaling a trace must not change the result.
    #[test]
    fn translation_and_scale_invariance(
        name in prop::sample::select(templates::DEFAULT_SYMBOLS),
        variant in 0usize..7,
        dx in -200.0f32..200.0,
        dy in -200.0f32..200.0,
        k in 0.2f32..5.0,
    ) {
        let r = recognizer();
        let original = shape(name, variant);
        let moved = transform(&original, |(x, y)| (x * k + dx, y * k + dy));
        let a = r.recognize(&original);
        let b = r.recognize(&moved);
        prop_assert_eq!(&a[0].symbol, &b[0].symbol);
        for (ca, cb) in a.iter().zip(&b) {
            prop_assert!((ca.score - cb.score).abs() < 1e-3, "{} {} vs {}", ca.symbol, ca.score, cb.score);
        }
    }

    /// The lower bounds and early abandoning of $Q never change the distance.
    #[test]
    fn q_matches_exhaustive_matching(a in arb_strokes(), b in arb_strokes()) {
        let (ca, cb) = (Cloud::new(&a, 0.0).unwrap(), Cloud::new(&b, 0.0).unwrap());
        prop_assert_eq!(distance(&ca, &cb, f32::MAX), distance_exhaustive(&ca, &cb));
    }
}
