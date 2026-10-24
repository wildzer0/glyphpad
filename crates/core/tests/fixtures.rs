//! Regression tests on touch sessions recorded on real hardware (SPEC §8).

use std::collections::BTreeMap;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

use glyphpad_core::capture::Capture;
use glyphpad_core::config;
use glyphpad_core::dispatch;
use glyphpad_core::engine::{Engine, Observation};
use glyphpad_core::symbols::TemplateFile;

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
    // Templates the user recorded for L, q and X; the other symbols use the built-in ones.
    let templates =
        TemplateFile::parse(&std::fs::read_to_string(dir.join("templates.json")).unwrap())
            .unwrap()
            .effective();

    for (file, want) in &expected {
        let capture = Capture::read(BufReader::new(File::open(dir.join(file)).unwrap()))
            .unwrap_or_else(|e| panic!("{file}: {e}"));
        let mut engine = Engine::from_config(&config);
        engine.set_templates(&templates, config.settings.rotation_tolerance_deg);
        let got: Vec<String> = dispatch::run_all(&mut engine, &config, &capture.events)
            .iter()
            // Diagnostics of strokes that turned out not to be symbols.
            .filter(|d| {
                !matches!(
                    d.recognition.observation,
                    Observation::UnrecognizedSymbol { .. }
                )
            })
            .map(|d| match d.gesture {
                Some(i) => config.gestures[i].def.name.clone(),
                None => format!("unmatched {}", d.recognition.observation),
            })
            .collect();
        assert_eq!(&got, want, "{file}");
    }
}
