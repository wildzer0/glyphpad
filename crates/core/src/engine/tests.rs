//! Scenario tests of the engine on synthetic frames at the 7 ms report
//! interval measured on the reference touchpad.

use super::*;
use crate::model::Contact;

const FRAME_US: u64 = 7_000;

/// Generates frames like a touchpad would.
struct Pad {
    t: u64,
    frames: Vec<TouchFrame>,
}

impl Pad {
    fn new() -> Self {
        Self {
            t: 1_000_000,
            frames: Vec::new(),
        }
    }

    fn push(&mut self, contacts: Vec<Contact>, button: bool) {
        self.frames.push(TouchFrame {
            timestamp_us: self.t,
            contacts,
            button_pressed: button,
        });
        self.t += FRAME_US;
    }

    fn wait_ms(&mut self, ms: u64) {
        self.t += ms * 1000;
    }

    /// `fingers` fingers 15 mm apart land together at `start`, move by
    /// `delta` over `ms` milliseconds and lift together.
    fn stroke(&mut self, fingers: u32, start: (f32, f32), delta: (f32, f32), ms: u64) {
        self.stroke_with(fingers, start, delta, ms, false, false);
    }

    fn stroke_with(
        &mut self,
        fingers: u32,
        start: (f32, f32),
        delta: (f32, f32),
        ms: u64,
        click: bool,
        palm: bool,
    ) {
        let steps = (ms * 1000 / FRAME_US).max(1);
        for i in 0..=steps {
            let k = i as f32 / steps as f32;
            let mut contacts: Vec<Contact> = (0..fingers)
                .map(|id| {
                    contact(
                        id,
                        start.0 + 15.0 * id as f32 + delta.0 * k,
                        start.1 + delta.1 * k,
                        true,
                    )
                })
                .collect();
            if palm {
                let mut p = contact(9, 120.0, 100.0, true);
                p.confident = false;
                contacts.push(p);
            }
            self.push(contacts, click && i == steps / 2);
        }
        let end = (start.0 + delta.0, start.1 + delta.1);
        let lifted = (0..fingers)
            .map(|id| contact(id, end.0 + 15.0 * id as f32, end.1, false))
            .collect();
        self.push(lifted, false);
    }

    fn tap(&mut self, fingers: u32) {
        self.stroke(fingers, (40.0, 40.0), (0.5, 0.5), 100);
    }
}

fn contact(id: u32, x: f32, y: f32, tip: bool) -> Contact {
    Contact {
        id,
        x_mm: x,
        y_mm: y,
        x_norm: x / 155.8,
        y_norm: y / 115.2,
        tip,
        confident: true,
    }
}

/// Feeds the frames, ticking at every deadline in between as the
/// application's loop does, then lets all timeouts expire.
fn run_at(engine: &mut Engine, frames: &[TouchFrame]) -> Vec<(u64, Observation)> {
    let mut out = Vec::new();
    let mut collect =
        |r: Vec<Recognition>| out.extend(r.into_iter().map(|r| (r.at_us, r.observation)));
    for f in frames {
        while let Some(d) = engine.next_deadline().filter(|&d| d <= f.timestamp_us) {
            collect(engine.tick(d));
        }
        collect(engine.on_frame(DeviceId(1), f));
    }
    while let Some(d) = engine.next_deadline() {
        collect(engine.tick(d));
    }
    out
}

fn run(engine: &mut Engine, frames: &[TouchFrame]) -> Vec<Observation> {
    run_at(engine, frames).into_iter().map(|(_, o)| o).collect()
}

fn engine(spec: Spec) -> Engine {
    Engine::new(spec, Thresholds::default())
}

fn tap(fingers: u8, taps: u8) -> Observation {
    Observation::Tap { fingers, taps }
}

fn spec_with(gestures: &str) -> Spec {
    Spec::from_config(&crate::config::parse(gestures).unwrap().config)
}

#[test]
fn single_taps_fire_on_lift() {
    for n in 1..=5 {
        let mut pad = Pad::new();
        pad.tap(n);
        let lift = pad.frames.last().unwrap().timestamp_us;
        let out = run_at(&mut engine(Spec::default()), &pad.frames);
        assert_eq!(out, vec![(lift, tap(n as u8, 1))], "{n} fingers");
    }
}

#[test]
fn double_tap_when_configured() {
    let spec = spec_with(
        r#"[[gesture]]
name = "d"
type = "tap"
fingers = 3
taps = 2
action = { type = "lock_screen" }"#,
    );
    let mut pad = Pad::new();
    pad.tap(3);
    pad.wait_ms(150);
    pad.tap(3);
    let lift = pad.frames.last().unwrap().timestamp_us;
    // Fires on the second lift: 2 is the most taps configured.
    assert_eq!(
        run_at(&mut engine(spec.clone()), &pad.frames),
        vec![(lift, tap(3, 2))]
    );

    // A lone tap waits for the gap before being confirmed as single.
    let mut pad = Pad::new();
    pad.tap(3);
    let lift = pad.frames.last().unwrap().timestamp_us;
    let out = run_at(&mut engine(spec), &pad.frames);
    assert_eq!(out, vec![(lift + 300_000, tap(3, 1))]);
}

#[test]
fn taps_too_far_apart_are_separate() {
    let mut pad = Pad::new();
    pad.tap(2);
    pad.wait_ms(500);
    pad.tap(2);
    assert_eq!(
        run(&mut engine(Spec::everything()), &pad.frames),
        vec![tap(2, 1), tap(2, 1)]
    );
}

#[test]
fn different_fingers_break_a_sequence() {
    let mut pad = Pad::new();
    pad.tap(2);
    pad.wait_ms(100);
    pad.tap(3);
    assert_eq!(
        run(&mut engine(Spec::everything()), &pad.frames),
        vec![tap(2, 1), tap(3, 1)]
    );
}

#[test]
fn triple_tap() {
    let mut pad = Pad::new();
    for _ in 0..3 {
        pad.tap(1);
        pad.wait_ms(120);
    }
    assert_eq!(
        run(&mut engine(Spec::everything()), &pad.frames),
        vec![tap(1, 3)]
    );
}

#[test]
fn slow_or_moving_touches_are_not_taps() {
    let mut pad = Pad::new();
    pad.stroke(2, (40.0, 40.0), (0.0, 0.0), 400);
    pad.wait_ms(500);
    pad.stroke(2, (40.0, 40.0), (10.0, 0.0), 100);
    assert_eq!(run(&mut engine(Spec::default()), &pad.frames), vec![]);
}

#[test]
fn staggered_fingers_still_make_one_tap() {
    let mut pad = Pad::new();
    let c = |id, tip| contact(id, 40.0 + 15.0 * id as f32, 40.0, tip);
    pad.push(vec![c(0, true)], false);
    pad.push(vec![c(0, true), c(1, true)], false);
    for _ in 0..8 {
        pad.push(vec![c(0, true), c(1, true), c(2, true)], false);
    }
    pad.push(vec![c(0, true), c(1, false), c(2, true)], false);
    pad.push(vec![c(0, false), c(2, true)], false);
    pad.push(vec![c(2, false)], false);
    assert_eq!(
        run(&mut engine(Spec::default()), &pad.frames),
        vec![tap(3, 1)]
    );
}

#[test]
fn clicks_and_palms() {
    let mut pad = Pad::new();
    pad.stroke_with(1, (40.0, 40.0), (0.0, 0.0), 100, true, false);
    assert_eq!(run(&mut engine(Spec::everything()), &pad.frames), vec![]);

    let mut pad = Pad::new();
    pad.stroke_with(3, (40.0, 40.0), (0.0, 0.0), 100, false, true);
    assert_eq!(
        run(&mut engine(Spec::default()), &pad.frames),
        vec![tap(3, 1)]
    );
}

#[test]
fn hold_fires_while_touching_and_consumes() {
    let spec = spec_with(
        r#"[[gesture]]
name = "h"
type = "hold"
fingers = 2
action = { type = "lock_screen" }"#,
    );
    let mut pad = Pad::new();
    let start = pad.t;
    pad.stroke(2, (40.0, 40.0), (1.0, 0.0), 900);
    let out = run_at(&mut engine(spec.clone()), &pad.frames);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].1, Observation::Hold { fingers: 2 });
    let at = out[0].0 - start;
    assert!((600_000..=610_000).contains(&at), "fired after {at} µs");

    // Not configured for 3 fingers: nothing.
    let mut pad = Pad::new();
    pad.stroke(3, (40.0, 40.0), (0.0, 0.0), 900);
    assert_eq!(run(&mut engine(spec.clone()), &pad.frames), vec![]);

    // Moving fingers re-arm the hold: no hold during a slow drag.
    let mut pad = Pad::new();
    pad.stroke(2, (40.0, 40.0), (60.0, 0.0), 1_500);
    assert_eq!(run(&mut engine(spec), &pad.frames), vec![]);
}

#[test]
fn hold_fires_without_new_frames() {
    // A device that stops reporting resting fingers: the tick fires the hold.
    let mut e = engine(Spec::everything());
    let f = TouchFrame {
        timestamp_us: 0,
        contacts: vec![contact(0, 10.0, 10.0, true)],
        button_pressed: false,
    };
    assert!(e.on_frame(DeviceId(1), &f).is_empty());
    assert_eq!(e.next_deadline(), Some(600_000));
    let r = e.tick(600_000);
    assert_eq!(r[0].observation, Observation::Hold { fingers: 1 });
    // Then the stale session is dropped.
    assert_eq!(e.next_deadline(), Some(STALE_SESSION_US));
    assert!(e.tick(STALE_SESSION_US).is_empty());
    assert_eq!(e.next_deadline(), None);
}

#[test]
fn pending_tap_is_confirmed_before_a_hold() {
    let mut pad = Pad::new();
    pad.tap(2);
    pad.wait_ms(100);
    pad.stroke(2, (40.0, 40.0), (0.0, 0.0), 800);
    assert_eq!(
        run(&mut engine(Spec::everything()), &pad.frames),
        vec![tap(2, 1), Observation::Hold { fingers: 2 }]
    );
}

fn swipe_dir(out: &[Observation]) -> Vec<(u8, Direction)> {
    out.iter()
        .map(|o| match o {
            Observation::Swipe {
                fingers, direction, ..
            } => (*fingers, *direction),
            other => panic!("not a swipe: {other:?}"),
        })
        .collect()
}

#[test]
fn swipes() {
    let mut pad = Pad::new();
    pad.stroke(3, (30.0, 60.0), (40.0, 0.0), 200);
    pad.wait_ms(300);
    pad.stroke(4, (30.0, 80.0), (0.0, -40.0), 200);
    pad.wait_ms(300);
    pad.stroke(3, (30.0, 80.0), (30.0, -20.0), 200); // 34°
    let out = run(&mut engine(Spec::default()), &pad.frames);
    assert_eq!(
        swipe_dir(&out),
        vec![
            (3, Direction::Right),
            (4, Direction::Up),
            (3, Direction::Right)
        ]
    );
    // With a diagonal gesture configured for 3 fingers the last one is diagonal.
    let out = run(&mut engine(Spec::everything()), &pad.frames);
    assert_eq!(swipe_dir(&out)[2], (3, Direction::UpRight));
}

#[test]
fn slow_or_short_moves_are_not_swipes() {
    let mut pad = Pad::new();
    pad.stroke(3, (30.0, 60.0), (40.0, 0.0), 2_000); // 20 mm/s
    pad.wait_ms(300);
    pad.stroke(3, (30.0, 60.0), (15.0, 0.0), 100); // 15 mm
    assert_eq!(run(&mut engine(Spec::default()), &pad.frames), vec![]);
}

#[test]
fn staggered_lift_does_not_bend_a_swipe() {
    let mut pad = Pad::new();
    pad.stroke(3, (30.0, 60.0), (40.0, 0.0), 200);
    // Rewrite the final lift as fingers leaving one by one while the last
    // one slides down: only the 3-finger phase counts.
    pad.frames.pop();
    let c = |id, x, y, tip| contact(id, x, y, tip);
    pad.push(
        vec![
            c(0, 70.0, 60.0, false),
            c(1, 85.0, 60.0, true),
            c(2, 100.0, 60.0, true),
        ],
        false,
    );
    pad.push(
        vec![c(1, 85.0, 90.0, false), c(2, 100.0, 95.0, true)],
        false,
    );
    pad.push(vec![c(2, 100.0, 110.0, false)], false);
    assert_eq!(
        swipe_dir(&run(&mut engine(Spec::default()), &pad.frames)),
        vec![(3, Direction::Right)]
    );
}

#[test]
fn resolve_respects_priority_and_conditions() {
    let config = crate::config::parse(
        r#"
[[gesture]]
name = "browser"
type = "swipe"
fingers = 3
direction = "right"
when = { app = "chrome.exe" }
action = { type = "keys", combo = "Ctrl+Tab" }

[[gesture]]
name = "anywhere"
type = "swipe"
fingers = 3
direction = "right"
action = { type = "lock_screen" }

[[gesture]]
name = "off"
type = "tap"
fingers = 2
enabled = false
action = { type = "lock_screen" }
"#,
    )
    .unwrap()
    .config;
    let swipe = Observation::Swipe {
        fingers: 3,
        direction: Direction::Right,
        distance_mm: 40.0,
        speed_mm_s: 200.0,
    };
    let chrome = AppInfo {
        exe: "chrome.exe".into(),
        ..Default::default()
    };
    let name = |app| resolve(&config, &swipe, app).map(|g| g.def.name.as_str());
    assert_eq!(name(Some(&chrome)), Some("browser"));
    assert_eq!(name(None), Some("anywhere"));
    assert!(resolve(&config, &tap(2, 1), None).is_none());
    assert!(resolve(&config, &tap(3, 1), None).is_none());
}
