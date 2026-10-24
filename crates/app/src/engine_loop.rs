//! The main loop: touch events in, gestures and actions out.

use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use glyphpad_core::action::{Action, ActionExecutor};
use glyphpad_core::config::Config;
use glyphpad_core::engine::{Engine, Recognition, resolve};
use glyphpad_core::{DeviceEvent, TouchFrame};

use crate::print;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Execute the actions of recognized gestures.
    Run,
    /// Print recognitions without executing anything; optionally frames too.
    Debug { frames: bool },
}

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

pub fn run(rx: Receiver<DeviceEvent>, config: Config, mode: Mode) -> Result<()> {
    let mut engine = Engine::from_config(&config);
    let foreground = glyphpad_actions::foreground();
    let actions = (mode == Mode::Run).then(|| spawn_action_worker(glyphpad_actions::executor()));
    let mut enabled = true;
    let mut clock = FrameClock {
        last_frame_us: 0,
        at: Instant::now(),
    };

    loop {
        // Block without a timeout when nothing is pending: no polling, no
        // CPU use while idle (SPEC §2).
        let event = match engine.next_deadline() {
            None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
            Some(deadline) => {
                let wait = Duration::from_micros(deadline.saturating_sub(clock.now_us()));
                rx.recv_timeout(wait)
            }
        };
        let recognitions = match event {
            Ok(event) => {
                if let DeviceEvent::Frame { frame, .. } = &event {
                    clock.observe(frame);
                }
                match mode {
                    Mode::Debug { frames } => {
                        if frames || !matches!(event, DeviceEvent::Frame { .. }) {
                            print::event(&event);
                        }
                    }
                    Mode::Run => {
                        if let DeviceEvent::Connected(d) = &event {
                            tracing::info!(
                                "touchpad ready: {} ({:.0}×{:.0} mm)",
                                d.name,
                                d.width_mm,
                                d.height_mm
                            );
                        }
                    }
                }
                engine.on_event(&event)
            }
            Err(RecvTimeoutError::Timeout) => engine.tick(clock.now_us()),
            Err(RecvTimeoutError::Disconnected) => bail!("the touch source stopped"),
        };

        for r in recognitions {
            let app = foreground.foreground_app();
            let gesture = resolve(&config, &r.observation, app.as_ref());
            if let Mode::Debug { .. } = mode {
                println!("{}", print::recognition(&r, gesture));
                continue;
            }
            let Some(gesture) = gesture else {
                tracing::debug!("{} (no gesture configured)", r.observation);
                continue;
            };
            let action = &gesture.def.action;
            if let Action::ToggleEnabled = action {
                enabled = !enabled;
                engine.reset();
                tracing::info!("gestures {}", if enabled { "enabled" } else { "paused" });
                continue;
            }
            if !enabled {
                tracing::debug!("paused: ignoring gesture \"{}\"", gesture.def.name);
                continue;
            }
            tracing::info!(
                "gesture \"{}\" ({}) in {} → {}",
                gesture.def.name,
                r.observation,
                app.as_ref().map_or("?", |a| a.exe.as_str()),
                action.kind()
            );
            if let Some(tx) = &actions {
                let _ = tx.send(action.clone());
            }
        }
    }
}

/// Actions run on their own thread so a slow one (`wait`, a program being
/// started) never delays recognition.
fn spawn_action_worker(executor: Box<dyn ActionExecutor + Send>) -> Sender<Action> {
    let (tx, rx) = crossbeam_channel::unbounded::<Action>();
    std::thread::Builder::new()
        .name("actions".into())
        .spawn(move || {
            for action in rx {
                run_action(executor.as_ref(), &action);
            }
        })
        .expect("cannot spawn the action thread");
    tx
}

fn run_action(executor: &dyn ActionExecutor, action: &Action) {
    match action {
        Action::Chain { actions } => actions.iter().for_each(|a| run_action(executor, a)),
        Action::Wait { ms } => std::thread::sleep(Duration::from_millis(*ms)),
        Action::RepeatLast | Action::ShowWindow => {
            tracing::warn!("`{}` arrives with the GUI (M4)", action.kind())
        }
        Action::ToggleEnabled => tracing::warn!("`toggle_enabled` inside a chain is ignored"),
        other => {
            if let Err(e) = executor.execute(other) {
                tracing::error!("action `{}` failed: {e}", other.kind());
            }
        }
    }
}

/// Offline evaluation of a capture: what would have been recognized.
pub fn evaluate(events: &[DeviceEvent], config: &Config, everything: bool) -> Vec<Recognition> {
    let mut engine = if everything {
        Engine::new(
            glyphpad_core::engine::Spec::everything(),
            (&config.settings).into(),
        )
    } else {
        Engine::from_config(config)
    };
    engine.process_all(events)
}
