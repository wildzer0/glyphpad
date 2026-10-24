//! Regression tests on touch sessions recorded on real hardware (SPEC §8).

use std::collections::BTreeMap;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

use glyphpad_core::capture::Capture;
use glyphpad_core::config;
use glyphpad_core::engine::{Engine, Observation, resolve};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

#[test]
fn recorded_sessions_produce_expected_gestures() {
    let dir = fixtures_dir();
    let gestures = std::fs::read_to_string(dir.join("gestures.toml")).unwrap();
    let config = config::parse(&gestures).unwrap().config;
    let expected: BTreeMap<String, Vec<String>> =
        toml::from_str(&std::fs::read_to_string(dir.join("expected.toml")).unwrap()).unwrap();
    assert!(!expected.is_empty());

    for (file, want) in &expected {
        let capture = Capture::read(BufReader::new(File::open(dir.join(file)).unwrap()))
            .unwrap_or_else(|e| panic!("{file}: {e}"));
        let got: Vec<String> = Engine::from_config(&config)
            .process_all(&capture.events)
            .iter()
            // Diagnostics of strokes that turned out not to be symbols.
            .filter(|r| !matches!(r.observation, Observation::UnrecognizedSymbol { .. }))
            .map(|r| match resolve(&config, &r.observation, None) {
                Some(g) => g.def.name.clone(),
                None => format!("unmatched {}", r.observation),
            })
            .collect();
        assert_eq!(&got, want, "{file}");
    }
}
