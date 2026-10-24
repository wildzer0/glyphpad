//! The main loop: touch events in, gestures and actions out.

use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use crossbeam_channel::{Receiver, RecvTimeoutError};
use glyphpad_core::config::Config;
use glyphpad_core::engine::{Engine, Observation, Recognition};
use glyphpad_core::symbols::Sample;
use glyphpad_core::{DeviceEvent, DeviceId, DeviceInfo, TouchFrame};

/// Maps wall-clock time onto the frame time base, so timeouts can be
/// checked between frames.
struct FrameClock {
    last_frame_us: u64,
    at: Instant,
}

impl FrameClock {
    fn now_us(&self) -> u64 {
        self.last_frame_us + self.at.elapsed().as_micros() as u64
    }

    fn observe(&mut self, frame: &TouchFrame) {
        self.last_frame_us = frame.timestamp_us;
        self.at = Instant::now();
    }
}

/// Pins the cursor while a one-finger symbol is drawn, if configured.
#[cfg(windows)]
pub struct Freezer {
    enabled: bool,
    guard: Option<glyphpad_platform::windows::cursor::CursorFreeze>,
}

#[cfg(windows)]
impl Freezer {
    pub fn new(config: &Config) -> Self {
        glyphpad_platform::windows::cursor::release_stale_freeze();
        Self {
            enabled: config.settings.draw_activation.freeze_cursor(),
            guard: None,
        }
    }

    pub fn update(&mut self, drawing: bool) {
        if !self.enabled {
            return;
        }
        match (drawing, self.guard.is_some()) {
            (true, false) => self.guard = glyphpad_platform::windows::cursor::CursorFreeze::new(),
            (false, true) => self.guard = None,
            _ => {}
        }
    }
}

#[cfg(not(windows))]
pub struct Freezer;

#[cfg(not(windows))]
impl Freezer {
    pub fn new(config: &Config) -> Self {
        if config.settings.draw_activation.freeze_cursor() {
            tracing::warn!("freeze_cursor is only available on Windows");
        }
        Self
    }

    pub fn update(&mut self, _drawing: bool) {}
}

/// Warns when Windows' own gestures for the drawing finger count are on:
/// they would fire together with every stroke (SPEC §5).
pub fn warn_about_native_gestures(config: &Config) {
    #[cfg(windows)]
    if let glyphpad_core::config::DrawActivation::Fingers { count } =
        config.settings.draw_activation
    {
        let on = glyphpad_platform::windows::native_gestures::enabled_for(count);
        if !on.is_empty() {
            tracing::warn!(
                "Windows' own {count}-finger {} are on and will run together with symbols drawn with {count} fingers; \
                 turn them off in Settings → Bluetooth & devices → Touchpad",
                on.join(" and ")
            );
        }
    }
    #[cfg(not(windows))]
    let _ = config;
}

fn engine_for(config: &Config, templates: &BTreeMap<String, Vec<Sample>>) -> Engine {
    let mut engine = Engine::from_config(config);
    engine.set_templates(templates, config.settings.rotation_tolerance_deg);
    engine
}

/// Offline evaluation of a capture: what would have been recognized.
pub fn evaluate(
    events: &[DeviceEvent],
    config: &Config,
    templates: &BTreeMap<String, Vec<Sample>>,
    everything: bool,
) -> Vec<Recognition> {
    let mut engine = if everything {
        let mut e = Engine::new(
            glyphpad_core::engine::Spec::everything(),
            (&config.settings).into(),
        );
        e.set_templates(templates, config.settings.rotation_tolerance_deg);
        e
    } else {
        engine_for(config, templates)
    };
    engine.process_all(events)
}

/// Records `count` drawings of a symbol with the configured draw mode.
/// Recognition and actions are suspended meanwhile (SPEC §10.3).
pub fn record(rx: Receiver<DeviceEvent>, config: &Config, count: usize) -> Result<Vec<Sample>> {
    let mut engine = Engine::from_config(config);
    engine.set_recording(true);
    let mut freezer = Freezer::new(config);
    warn_about_native_gestures(config);
    let mut devices: HashMap<DeviceId, DeviceInfo> = HashMap::new();
    let mut clock = FrameClock {
        last_frame_us: 0,
        at: Instant::now(),
    };
    let mut samples = Vec::new();
    eprintln!("Draw example 1 of {count}. An example ends when you pause.");
    while samples.len() < count {
        let event = match engine.next_deadline() {
            None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
            Some(d) => rx.recv_timeout(Duration::from_micros(d.saturating_sub(clock.now_us()))),
        };
        let recognitions = match event {
            Ok(event) => {
                match &event {
                    DeviceEvent::Frame { frame, .. } => clock.observe(frame),
                    DeviceEvent::Connected(d) => {
                        devices.insert(d.id, d.clone());
                    }
                    DeviceEvent::Disconnected { .. } => {}
                }
                engine.on_event(&event)
            }
            Err(RecvTimeoutError::Timeout) => engine.tick(clock.now_us()),
            Err(RecvTimeoutError::Disconnected) => bail!("the touch source stopped"),
        };
        freezer.update(engine.drawing_now());
        for r in recognitions {
            if let Observation::Sample { strokes } = r.observation {
                let (width_mm, height_mm) = devices
                    .get(&r.device)
                    .map_or((0.0, 0.0), |d| (d.width_mm, d.height_mm));
                samples.push(Sample {
                    strokes,
                    width_mm,
                    height_mm,
                });
                let n = samples.len();
                let next = if n < count {
                    format!(" Draw example {} of {count}.", n + 1)
                } else {
                    String::new()
                };
                eprintln!(
                    "  example {n} recorded ({} stroke(s)).{next}",
                    samples[n - 1].strokes.len()
                );
            }
        }
    }
    Ok(samples)
}
