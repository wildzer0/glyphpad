//! From recognitions to gestures, including sequences (SPEC §6.1: gesture B
//! within T ms of gesture A).
//!
//! When a recognition is the first step of a configured sequence, what it
//! would trigger on its own is held back for the sequence's `within_ms`, as
//! a single tap waits for a possible double tap. If the next steps follow
//! in time the sequence fires and the held gestures are dropped; if the
//! sequence breaks or times out, they fire in order after all.

use crate::action::AppInfo;
use crate::config::{Config, DrawActivation};
use crate::engine::{Engine, Observation, Recognition, resolve};
use crate::model::DeviceEvent;
use crate::trigger::Trigger;

/// A recognition and the gesture it triggers (index into `config.gestures`).
#[derive(Debug, Clone, PartialEq)]
pub struct Dispatched {
    pub gesture: Option<usize>,
    pub recognition: Recognition,
    /// Foreground application when it was recognized.
    pub app: Option<AppInfo>,
}

#[derive(Debug)]
struct Pending {
    /// Sequences still possible, by gesture index.
    candidates: Vec<usize>,
    /// Index of the step each candidate expects next.
    next_step: usize,
    /// Latest end of the next step for the sequence to go on.
    step_deadline_us: u64,
    /// When the held gestures fire after all: the step deadline, plus the
    /// stroke gap if the next step may be drawn as a stroke, which is only
    /// recognized that long after the fingers lift.
    deadline_us: u64,
    held: Vec<(Recognition, Option<AppInfo>)>,
}

#[derive(Debug, Default)]
pub struct Dispatcher {
    pending: Option<Pending>,
}

fn index_of(config: &Config, observation: &Observation, app: Option<&AppInfo>) -> Option<usize> {
    let g = resolve(config, observation, app)?;
    config.gestures.iter().position(|x| std::ptr::eq(x, g))
}

/// Recognitions that never take part in sequences: diagnostics and samples.
fn is_diagnostic(o: &Observation) -> bool {
    matches!(
        o,
        Observation::UnrecognizedSymbol { .. } | Observation::Sample { .. }
    )
}

/// Whether the touch for `step` may be taken for a stroke of a symbol, and
/// so recognized only once the stroke gap has passed.
fn may_be_stroke(config: &Config, step: &Trigger) -> bool {
    let fingers = step.fingers();
    matches!(step, Trigger::Swipe { .. } | Trigger::EdgeSwipe { .. })
        && match config.settings.draw_activation {
            DrawActivation::Fingers { count } => fingers == count || fingers == 1,
            DrawActivation::Edge { .. } | DrawActivation::Hold { .. } => fingers == 1,
            DrawActivation::Toggle { .. } => fingers == 1,
        }
}

/// Deadlines of a sequence waiting for step `step` of `candidates` after
/// the previous one ended at `ended_us`: the step must end by the first,
/// the held gestures fire at the second.
fn deadlines(config: &Config, candidates: &[usize], step: usize, ended_us: u64) -> (u64, u64) {
    let within = candidates
        .iter()
        .map(|&i| config.gestures[i].within_ms)
        .max()
        .unwrap_or(0);
    let by = ended_us + within * 1000;
    let stroke = candidates
        .iter()
        .filter_map(|&i| config.gestures[i].steps.get(step))
        .any(|t| may_be_stroke(config, t));
    let grace = if stroke {
        config.settings.stroke_gap_ms * 1000
    } else {
        0
    };
    (by, by + grace)
}

impl Dispatcher {
    pub fn reset(&mut self) {
        self.pending = None;
    }

    pub fn deadline(&self) -> Option<u64> {
        self.pending.as_ref().map(|p| p.deadline_us)
    }

    fn flush(&mut self, config: &Config) -> Vec<Dispatched> {
        let Some(p) = self.pending.take() else {
            return Vec::new();
        };
        p.held
            .into_iter()
            .map(|(r, app)| Dispatched {
                gesture: index_of(config, &r.observation, app.as_ref()),
                recognition: r,
                app,
            })
            .collect()
    }

    /// Lets a held sequence time out.
    pub fn tick(&mut self, config: &Config, now_us: u64) -> Vec<Dispatched> {
        match &self.pending {
            Some(p) if now_us >= p.deadline_us => self.flush(config),
            _ => Vec::new(),
        }
    }

    pub fn on_recognition(
        &mut self,
        config: &Config,
        r: Recognition,
        app: Option<AppInfo>,
    ) -> Vec<Dispatched> {
        let mut out = self.tick(config, r.at_us);
        if is_diagnostic(&r.observation) {
            out.push(Dispatched {
                gesture: None,
                recognition: r,
                app,
            });
            return out;
        }
        let conf = config.settings.default_confidence;
        let profile = &config.settings.active_profile;
        let matches_step = |i: usize, step: usize| {
            let g = &config.gestures[i];
            g.steps
                .get(step)
                .is_some_and(|t| t.specificity(&r.observation, conf) > 0)
        };

        if let Some(p) = &mut self.pending {
            let advanced: Vec<usize> = p
                .candidates
                .iter()
                .copied()
                .filter(|&i| matches_step(i, p.next_step))
                .collect();
            // The window counts to the end of the movement, not to the
            // moment it was recognized.
            if !advanced.is_empty() && r.ended_us <= p.step_deadline_us {
                // Candidates are in priority order, so the first complete one wins.
                if let Some(&done) = advanced
                    .iter()
                    .find(|&&i| config.gestures[i].steps.len() == p.next_step + 1)
                {
                    self.pending = None;
                    out.push(Dispatched {
                        gesture: Some(done),
                        recognition: r,
                        app,
                    });
                    return out;
                }
                p.next_step += 1;
                (p.step_deadline_us, p.deadline_us) =
                    deadlines(config, &advanced, p.next_step, r.ended_us);
                p.candidates = advanced;
                p.held.push((r, app));
                return out;
            }
            // The sequence broke: what was held fires on its own.
            out.extend(self.flush(config));
        }

        let starts: Vec<usize> = config
            .gestures
            .iter()
            .enumerate()
            .filter(|(i, g)| {
                g.def.enabled
                    && g.applies(profile, app.as_ref())
                    && !g.steps.is_empty()
                    && matches_step(*i, 0)
            })
            .map(|(i, _)| i)
            .collect();
        if starts.is_empty() {
            out.push(Dispatched {
                gesture: index_of(config, &r.observation, app.as_ref()),
                recognition: r,
                app,
            });
        } else {
            let (step_deadline_us, deadline_us) = deadlines(config, &starts, 1, r.ended_us);
            self.pending = Some(Pending {
                candidates: starts,
                next_step: 1,
                step_deadline_us,
                deadline_us,
                held: vec![(r, app)],
            });
        }
        out
    }
}

/// Runs recorded events through the engine and the dispatcher
/// deterministically, ticking both at every deadline (replays and tests).
pub fn run_all<'a>(
    engine: &mut Engine,
    config: &Config,
    events: impl IntoIterator<Item = &'a DeviceEvent>,
) -> Vec<Dispatched> {
    let mut dispatcher = Dispatcher::default();
    let mut out = Vec::new();
    let feed = |rs: Vec<Recognition>, d: &mut Dispatcher, out: &mut Vec<Dispatched>| {
        for r in rs {
            out.extend(d.on_recognition(config, r, None));
        }
    };
    let next = |e: &Engine, d: &Dispatcher| {
        [e.next_deadline(), d.deadline()]
            .into_iter()
            .flatten()
            .min()
    };
    for event in events {
        if let DeviceEvent::Frame { frame, .. } = event {
            while let Some(t) = next(engine, &dispatcher).filter(|&t| t <= frame.timestamp_us) {
                let rs = engine.tick(t);
                feed(rs, &mut dispatcher, &mut out);
                out.extend(dispatcher.tick(config, t));
            }
        }
        let rs = engine.on_event(event);
        feed(rs, &mut dispatcher, &mut out);
    }
    while let Some(t) = next(engine, &dispatcher) {
        let rs = engine.tick(t);
        feed(rs, &mut dispatcher, &mut out);
        out.extend(dispatcher.tick(config, t));
    }
    out
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::config::{Direction, parse};
    use crate::model::DeviceId;

    fn config() -> Config {
        parse(
            r#"
[[gesture]]
name = "tap3"
type = "tap"
fingers = 3
action = { type = "lock_screen" }

[[gesture]]
name = "up3"
type = "swipe"
fingers = 3
direction = "up"
action = { type = "lock_screen" }

[[gesture]]
name = "tap-then-up"
type = "sequence"
steps = [{ type = "tap", fingers = 3 }, { type = "swipe", fingers = 3, direction = "up" }]
within_ms = 500
action = { type = "minimize" }
"#,
        )
        .unwrap()
        .config
    }

    fn rec(observation: Observation, at_ms: u64) -> Recognition {
        Recognition {
            device: DeviceId(1),
            observation,
            at_us: at_ms * 1000,
            ended_us: at_ms * 1000,
            trace: Arc::default(),
        }
    }

    fn tap() -> Observation {
        Observation::Tap {
            fingers: 3,
            taps: 1,
        }
    }

    fn up() -> Observation {
        Observation::Swipe {
            fingers: 3,
            direction: Direction::Up,
            distance_mm: 30.0,
            speed_mm_s: 150.0,
        }
    }

    fn names(c: &Config, d: &[Dispatched]) -> Vec<String> {
        d.iter()
            .map(|d| {
                d.gesture
                    .map_or("-".into(), |i| c.gestures[i].def.name.clone())
            })
            .collect()
    }

    #[test]
    fn completed_sequence_replaces_its_steps() {
        let c = config();
        let mut d = Dispatcher::default();
        assert!(d.on_recognition(&c, rec(tap(), 100), None).is_empty());
        // 500 ms window plus the 400 ms stroke gap: the 3-finger swipe may
        // be drawn as a stroke, which is recognized that much later.
        assert_eq!(d.deadline(), Some(1_000_000));
        let out = d.on_recognition(&c, rec(up(), 400), None);
        assert_eq!(names(&c, &out), ["tap-then-up"]);
        assert!(d.deadline().is_none());
    }

    #[test]
    fn timeout_releases_the_first_step() {
        let c = config();
        let mut d = Dispatcher::default();
        assert!(d.on_recognition(&c, rec(tap(), 100), None).is_empty());
        assert!(d.tick(&c, 900_000).is_empty());
        assert_eq!(names(&c, &d.tick(&c, 1_000_000)), ["tap3"]);
        // A late second step is just a swipe.
        assert_eq!(
            names(&c, &d.on_recognition(&c, rec(up(), 1_300), None)),
            ["up3"]
        );
    }

    #[test]
    fn broken_sequence_releases_in_order() {
        let c = config();
        let mut d = Dispatcher::default();
        d.on_recognition(&c, rec(tap(), 100), None);
        let other = Observation::Hold { fingers: 2 };
        assert_eq!(
            names(&c, &d.on_recognition(&c, rec(other, 200), None)),
            ["tap3", "-"]
        );
    }

    #[test]
    fn steps_not_starting_a_sequence_pass_through() {
        let c = config();
        let mut d = Dispatcher::default();
        assert_eq!(
            names(&c, &d.on_recognition(&c, rec(up(), 100), None)),
            ["up3"]
        );
    }
}
