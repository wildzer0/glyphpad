//! From recognitions to gestures, including sequences (SPEC §6.1: gesture B
//! within T ms of gesture A).
//!
//! When a recognition is the first step of a configured sequence, what it
//! would trigger on its own is held back for the sequence's `within_ms`, as
//! a single tap waits for a possible double tap. If the next steps follow
//! in time the sequence fires and the held gestures are dropped; if the
//! sequence breaks or times out, they fire in order after all.

use crate::action::AppInfo;
use crate::config::Config;
use crate::engine::{Engine, Observation, Recognition, resolve};
use crate::model::DeviceEvent;

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
    /// Latest start of the next step for the sequence to go on.
    step_deadline_us: u64,
    held: Vec<(Recognition, Option<AppInfo>)>,
}

/// A touch that began in time is waited for at most this long: a gesture is
/// only recognized once it is over, and a finger left resting must not hold
/// the gestures back forever.
const ENGAGED_MAX_US: u64 = 5_000_000;

#[derive(Debug, Default)]
pub struct Dispatcher {
    pending: Option<Pending>,
    /// Start of the touch or drawing the engine is in the middle of.
    busy_since: Option<u64>,
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

/// Latest start of step `step` of `candidates` after the previous one ended
/// at `ended_us`. The pause between the steps is what `within_ms` limits,
/// not the time the next step itself takes.
fn step_deadline(config: &Config, candidates: &[usize], ended_us: u64) -> u64 {
    let within = candidates
        .iter()
        .map(|&i| config.gestures[i].within_ms)
        .max()
        .unwrap_or(0);
    ended_us + within * 1000
}

impl Dispatcher {
    pub fn reset(&mut self) {
        self.pending = None;
        self.busy_since = None;
    }

    /// Tells where the engine is: `busy_since` is the start of the touch or
    /// drawing in progress. A step that began in time is waited for until it
    /// has been recognized, however long that takes (a swipe drawn as a
    /// stroke only after the stroke gap). Call after every engine step.
    pub fn observe(&mut self, busy_since: Option<u64>) {
        self.busy_since = busy_since;
    }

    /// When the held gestures fire after all.
    pub fn deadline(&self) -> Option<u64> {
        let p = self.pending.as_ref()?;
        Some(match self.busy_since {
            Some(b) if b <= p.step_deadline_us => p.step_deadline_us.max(b + ENGAGED_MAX_US),
            _ => p.step_deadline_us,
        })
    }

    /// Whether a sequence is waiting for its next step.
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
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
        match self.deadline() {
            Some(d) if now_us >= d => self.flush(config),
            _ => Vec::new(),
        }
    }

    pub fn on_recognition(
        &mut self,
        config: &Config,
        r: Recognition,
        app: Option<AppInfo>,
    ) -> Vec<Dispatched> {
        let mut out = Vec::new();
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
            // The pause counts up to the moment the movement began.
            if !advanced.is_empty() && r.started_us <= p.step_deadline_us {
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
                p.step_deadline_us = step_deadline(config, &advanced, r.ended_us);
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
            self.pending = Some(Pending {
                step_deadline_us: step_deadline(config, &starts, r.ended_us),
                candidates: starts,
                next_step: 1,
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
                dispatcher.observe(engine.busy_since());
                out.extend(dispatcher.tick(config, t));
            }
        }
        let rs = engine.on_event(event);
        feed(rs, &mut dispatcher, &mut out);
        dispatcher.observe(engine.busy_since());
    }
    while let Some(t) = next(engine, &dispatcher) {
        let rs = engine.tick(t);
        feed(rs, &mut dispatcher, &mut out);
        dispatcher.observe(engine.busy_since());
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
        span(observation, at_ms, at_ms)
    }

    /// A recognition of a movement from `start_ms` to `end_ms`.
    fn span(observation: Observation, start_ms: u64, end_ms: u64) -> Recognition {
        Recognition {
            device: DeviceId(1),
            observation,
            at_us: end_ms * 1000,
            ended_us: end_ms * 1000,
            started_us: start_ms * 1000,
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
        // 500 ms of pause after the tap ended.
        assert_eq!(d.deadline(), Some(600_000));
        let out = d.on_recognition(&c, rec(up(), 400), None);
        assert_eq!(names(&c, &out), ["tap-then-up"]);
        assert!(d.deadline().is_none());
    }

    #[test]
    fn the_pause_counts_to_the_start_of_the_next_step() {
        let c = config();
        let mut d = Dispatcher::default();
        d.on_recognition(&c, rec(tap(), 100), None);
        // Starts 450 ms after the tap, takes 300 ms: still in time.
        let out = d.on_recognition(&c, span(up(), 550, 850), None);
        assert_eq!(names(&c, &out), ["tap-then-up"]);
    }

    #[test]
    fn a_step_begun_in_time_is_waited_for() {
        let c = config();
        let mut d = Dispatcher::default();
        d.on_recognition(&c, rec(tap(), 100), None);
        // The fingers touched down in time: nothing fires while it lasts,
        // not even when the pause is over.
        d.observe(Some(500_000));
        assert!(d.tick(&c, 700_000).is_empty());
        assert!(d.tick(&c, 3_000_000).is_empty());
        let out = d.on_recognition(&c, span(up(), 500, 1_300), None);
        assert_eq!(names(&c, &out), ["tap-then-up"]);
        // A touch left resting is not waited for forever.
        d.on_recognition(&c, rec(tap(), 2_000), None);
        d.observe(Some(2_200_000));
        assert!(d.tick(&c, 6_000_000).is_empty());
        assert_eq!(names(&c, &d.tick(&c, 7_200_000)), ["tap3"]);
    }

    #[test]
    fn a_touch_begun_too_late_is_not_waited_for() {
        let c = config();
        let mut d = Dispatcher::default();
        d.on_recognition(&c, rec(tap(), 100), None);
        d.observe(Some(800_000));
        assert_eq!(names(&c, &d.tick(&c, 600_000)), ["tap3"]);
    }

    #[test]
    fn timeout_releases_the_first_step() {
        let c = config();
        let mut d = Dispatcher::default();
        assert!(d.on_recognition(&c, rec(tap(), 100), None).is_empty());
        assert!(d.tick(&c, 500_000).is_empty());
        assert_eq!(names(&c, &d.tick(&c, 600_000)), ["tap3"]);
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
