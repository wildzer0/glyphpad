//! A sequence built from two real sessions (SPEC §6.1): a three-finger tap
//! from `three-finger-double-taps` followed by a three-finger swipe up from
//! `three-finger-swipes-up`, the pause between them varied.

use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

use glyphpad_core::capture::Capture;
use glyphpad_core::config;
use glyphpad_core::dispatch;
use glyphpad_core::engine::Engine;
use glyphpad_core::model::DeviceEvent;

const CONFIG: &str = r#"
[settings]
draw_activation = { mode = "fingers", count = 3 }

[[gesture]]
name = "tap"
type = "tap"
fingers = 3
action = { type = "lock_screen" }

[[gesture]]
name = "up"
type = "swipe"
fingers = 3
direction = "up"
action = { type = "lock_screen" }

[[gesture]]
name = "tap-then-up"
type = "sequence"
steps = [{ type = "tap", fingers = 3 }, { type = "swipe", fingers = 3, direction = "up" }]
within_ms = 800
action = { type = "minimize" }
"#;

fn read(name: &str) -> Capture {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name);
    Capture::read(BufReader::new(File::open(path).unwrap())).unwrap()
}

/// The events up to and including the first lift of every finger.
fn first_session(capture: &Capture) -> Vec<DeviceEvent> {
    let mut out = Vec::new();
    let mut touched = false;
    for e in &capture.events {
        if let DeviceEvent::Frame { frame, .. } = e {
            let down = frame.contacts.iter().any(|c| c.tip);
            touched |= down;
            out.push(e.clone());
            if touched && !down {
                break;
            }
        }
    }
    out
}

fn timestamp(e: &DeviceEvent) -> u64 {
    match e {
        DeviceEvent::Frame { frame, .. } => frame.timestamp_us,
        _ => 0,
    }
}

/// Names of what fires when the swipe touches down `pause_ms` after the tap lifted.
fn run(pause_ms: u64) -> Vec<String> {
    let config = config::parse(CONFIG).unwrap().config;
    let tap = first_session(&read("three-finger-double-taps.jsonl"));
    let mut swipe = first_session(&read("three-finger-swipes-up.jsonl"));
    let shift = timestamp(tap.last().unwrap()) + pause_ms * 1000 - timestamp(&swipe[0]);
    for e in &mut swipe {
        if let DeviceEvent::Frame { frame, .. } = e {
            frame.timestamp_us += shift;
        }
    }
    let events: Vec<DeviceEvent> = tap.into_iter().chain(swipe).collect();
    let mut engine = Engine::from_config(&config);
    dispatch::run_all(&mut engine, &config, &events)
        .iter()
        .filter_map(|d| d.gesture.map(|i| config.gestures[i].def.name.clone()))
        .collect()
}

#[test]
fn a_calm_pause_between_the_steps_completes_the_sequence() {
    // The swipe lasts about 200 ms, so 600 ms of pause used to overrun the 800 ms.
    for pause in [100, 400, 600, 780] {
        assert_eq!(run(pause), ["tap-then-up"], "pause of {pause} ms");
    }
}

#[test]
fn a_longer_pause_leaves_two_separate_gestures() {
    for pause in [900, 1500] {
        assert_eq!(run(pause), ["tap", "up"], "pause of {pause} ms");
    }
}

const SWIPE_SEQUENCE: &str = r#"
[settings]
draw_activation = { mode = "fingers", count = 3 }
stroke_gap_ms = 400

[[gesture]]
name = "up"
type = "swipe"
fingers = 3
direction = "up"
action = { type = "lock_screen" }

[[gesture]]
name = "up-then-up"
type = "sequence"
steps = [{ type = "swipe", fingers = 3, direction = "up" }, { type = "swipe", fingers = 3, direction = "up" }]
within_ms = 800
action = { type = "minimize" }
"#;

/// The events of the `index`-th touch session of a capture.
fn session(capture: &Capture, index: usize) -> Vec<DeviceEvent> {
    let mut sessions: Vec<Vec<DeviceEvent>> = Vec::new();
    let mut current: Vec<DeviceEvent> = Vec::new();
    let mut touched = false;
    for e in &capture.events {
        if let DeviceEvent::Frame { frame, .. } = e {
            let down = frame.contacts.iter().any(|c| c.tip);
            if down {
                touched = true;
                current.push(e.clone());
            } else if touched {
                current.push(e.clone());
                sessions.push(std::mem::take(&mut current));
                touched = false;
            }
        }
    }
    sessions.get(index).cloned().unwrap_or_default()
}

/// What fires when a second three-finger swipe follows the first after
/// `pause_ms`. Both are drawn with the drawing finger count, so their
/// strokes fall into one drawing whenever the pause is under `stroke_gap_ms`.
fn run_swipes(pause_ms: u64) -> Vec<String> {
    let config = config::parse(SWIPE_SEQUENCE).unwrap().config;
    let capture = read("three-finger-swipes-up.jsonl");
    let (first, mut second) = (session(&capture, 0), session(&capture, 1));
    let lift = timestamp(first.last().unwrap());
    let shift = lift as i64 + pause_ms as i64 * 1000 - timestamp(&second[0]) as i64;
    for e in &mut second {
        if let DeviceEvent::Frame { frame, .. } = e {
            frame.timestamp_us = (frame.timestamp_us as i64 + shift) as u64;
        }
    }
    let events: Vec<DeviceEvent> = first.into_iter().chain(second).collect();
    let mut engine = Engine::from_config(&config);
    dispatch::run_all(&mut engine, &config, &events)
        .iter()
        .filter(|d| {
            !matches!(
                d.recognition.observation,
                glyphpad_core::engine::Observation::UnrecognizedSymbol { .. }
            )
        })
        .filter_map(|d| d.gesture.map(|i| config.gestures[i].def.name.clone()))
        .collect()
}

#[test]
fn two_drawn_swipes_at_a_natural_pace_are_a_sequence() {
    // Under `stroke_gap_ms` the two strokes are one drawing that is no
    // symbol; each stroke is judged on its own, so the sequence still fires.
    for pause in [50, 150, 300, 390, 500, 780] {
        assert_eq!(run_swipes(pause), ["up-then-up"], "pause of {pause} ms");
    }
}

#[test]
fn a_drawn_sequence_past_within_ms_leaves_two_gestures() {
    assert_eq!(run_swipes(1200), ["up", "up"]);
}
