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
    /// Sequences still possible, by gesture index, each with its own
    /// `within_ms` in microseconds.
    candidates: Vec<(usize, u64)>,
    /// Index of the step each candidate expects next.
    next_step: usize,
    /// End of the step that started the sequence: the lift of its own touch,
    /// which for a hold comes well after it fired.
    step_end_us: u64,
    held: Vec<(Recognition, Option<AppInfo>)>,
}

impl Pending {
    /// Latest start of the next step, given one candidate's window.
    fn deadline_of(&self, within_us: u64) -> u64 {
        self.step_end_us + within_us
    }

    /// Latest start of the next step over all candidates, for the timeout.
    fn latest(&self) -> u64 {
        self.step_end_us
            + self
                .candidates
                .iter()
                .map(|&(_, within_us)| within_us)
                .max()
                .unwrap_or(0)
    }
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

/// The pause a sequence allows between one step ending and the next
/// starting: what `within_ms` limits, not the time the next step itself
/// takes.
fn within_us(config: &Config, index: usize) -> u64 {
    config.gestures[index].within_ms * 1000
}

/// The candidates' names, quoted, for a log line.
fn names(config: &Config, candidates: &[(usize, u64)]) -> String {
    candidates
        .iter()
        .map(|&(i, _)| format!("\"{}\"", config.gestures[i].def.name))
        .collect::<Vec<_>>()
        .join(" or ")
}

/// Fingers a recognition was made with.
fn observed_fingers(o: &Observation) -> Option<u8> {
    match o {
        Observation::Tap { fingers, .. }
        | Observation::Hold { fingers }
        | Observation::Swipe { fingers, .. }
        | Observation::EdgeSwipe { fingers, .. }
        | Observation::Pinch { fingers, .. }
        | Observation::Rotate { fingers, .. }
        | Observation::Click { fingers, .. } => Some(*fingers),
        Observation::Chord { gesture, .. } => observed_fingers(gesture),
        Observation::Symbol { .. }
        | Observation::UnrecognizedSymbol { .. }
        | Observation::Sample { .. } => None,
    }
}

/// What the configuration expects with this many fingers, for a recognition
/// that matched nothing. "No gesture configured" is a dead end: the usual
/// mistake is a gesture of the wrong kind — a three-finger touch held 1.7 s
/// is a hold, not the tap a sequence's first step asked for.
fn near_miss(config: &Config, observation: &Observation) -> Option<String> {
    let fingers = observed_fingers(observation)?;
    let configured: Vec<String> = config
        .gestures
        .iter()
        .filter(|g| g.def.enabled)
        .flat_map(|g| g.trigger.iter().chain(&g.steps))
        .filter(|t| t.fingers() == fingers)
        .map(Trigger::describe)
        .collect();
    (!configured.is_empty()).then(|| configured.join(", "))
}

impl Dispatcher {
    pub fn reset(&mut self) {
        self.pending = None;
        self.busy_since = None;
    }

    /// Tells where the engine is at `now_us`: `busy_since` is the start of
    /// the touch or drawing in progress. A step that began in time is waited
    /// for until it has been recognized, however long that takes (a swipe
    /// drawn as a stroke only after the stroke gap). Call after every engine
    /// step.
    ///
    /// The step's own touch may still be down after the step was recognized:
    /// a hold fires at `hold_duration_ms` while the fingers rest, and stays
    /// the same touch until they lift. A step is over when its touch is, so
    /// its window starts at the lift, however long the fingers rested.
    pub fn observe(&mut self, busy_since: Option<u64>, now_us: u64) {
        self.busy_since = busy_since;
        if let Some(p) = &mut self.pending
            && busy_since.is_some_and(|b| b <= p.step_end_us)
        {
            p.step_end_us = p.step_end_us.max(now_us);
        }
    }

    /// When the held gestures fire after all.
    pub fn deadline(&self) -> Option<u64> {
        let p = self.pending.as_ref()?;
        let latest = p.latest();
        Some(match self.busy_since {
            Some(b) if b <= latest => latest.max(b + ENGAGED_MAX_US),
            _ => latest,
        })
    }

    /// Whether a sequence is waiting for its next step.
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    fn flush(&mut self, config: &Config, reason: &str) -> Vec<Dispatched> {
        let Some(p) = self.pending.take() else {
            return Vec::new();
        };
        // A sequence that does not complete leaves its steps to fire on their
        // own, which for a step that is no gesture at all looks like nothing
        // happened: say why, in the log and in `replay`.
        tracing::info!(
            "sequence {} not completed: {reason}; its steps run on their own",
            names(config, &p.candidates)
        );
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
            Some(d) if now_us >= d => {
                self.flush(config, "no next step began within its `within_ms`")
            }
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
            // A candidate goes on only if this step is the one it expects and
            // it began within that sequence's own window.
            let advanced: Vec<(usize, u64)> = p
                .candidates
                .iter()
                .copied()
                .filter(|&(i, within_us)| {
                    matches_step(i, p.next_step) && r.started_us <= p.deadline_of(within_us)
                })
                .collect();
            if !advanced.is_empty() {
                // Candidates are in priority order, so the first complete one wins.
                if let Some(&(done, _)) = advanced
                    .iter()
                    .find(|&&(i, _)| config.gestures[i].steps.len() == p.next_step + 1)
                {
                    self.pending = None;
                    tracing::debug!(
                        "sequence \"{}\" completed by {}",
                        config.gestures[done].def.name,
                        r.observation
                    );
                    out.push(Dispatched {
                        gesture: Some(done),
                        recognition: r,
                        app,
                    });
                    return out;
                }
                p.next_step += 1;
                p.candidates = advanced;
                p.step_end_us = r.ended_us;
                tracing::debug!(
                    "sequence {}: step {} done, waiting for step {} within {} ms",
                    names(config, &p.candidates),
                    p.next_step,
                    p.next_step + 1,
                    p.latest().saturating_sub(r.ended_us) / 1000
                );
                p.held.push((r, app));
                return out;
            }
            // The sequence broke: what was held fires on its own.
            out.extend(self.flush(config, "the next gesture was not its next step"));
        }

        let starts: Vec<(usize, u64)> = config
            .gestures
            .iter()
            .enumerate()
            .filter(|(i, g)| {
                g.def.enabled
                    && g.applies(profile, app.as_ref())
                    && !g.steps.is_empty()
                    && matches_step(*i, 0)
            })
            .map(|(i, _)| (i, within_us(config, i)))
            .collect();
        if starts.is_empty() {
            let gesture = index_of(config, &r.observation, app.as_ref());
            if gesture.is_none()
                && let Some(hint) = near_miss(config, &r.observation)
            {
                tracing::info!(
                    "{} matched no gesture; with {} fingers the configuration has: {hint}",
                    r.observation,
                    observed_fingers(&r.observation).unwrap_or(0)
                );
            }
            out.push(Dispatched {
                gesture,
                recognition: r,
                app,
            });
        } else {
            let pending = Pending {
                candidates: starts,
                next_step: 1,
                step_end_us: r.ended_us,
                held: vec![(r, app)],
            };
            tracing::debug!(
                "sequence {}: step 1 done, waiting for step 2 within {} ms",
                names(config, &pending.candidates),
                pending.latest().saturating_sub(pending.step_end_us) / 1000
            );
            self.pending = Some(pending);
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
    // The latest frame time: what the dispatcher needs to know the step's own
    // touch is still down after a hold fired.
    let mut now_us = 0;
    for event in events {
        if let DeviceEvent::Frame { frame, .. } = event {
            now_us = frame.timestamp_us;
            while let Some(t) = next(engine, &dispatcher).filter(|&t| t <= now_us) {
                let rs = engine.tick(t);
                feed(rs, &mut dispatcher, &mut out);
                dispatcher.observe(engine.busy_since(), t);
                out.extend(dispatcher.tick(config, t));
            }
        }
        let rs = engine.on_event(event);
        feed(rs, &mut dispatcher, &mut out);
        dispatcher.observe(engine.busy_since(), now_us);
    }
    while let Some(t) = next(engine, &dispatcher) {
        let rs = engine.tick(t);
        feed(rs, &mut dispatcher, &mut out);
        dispatcher.observe(engine.busy_since(), t);
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
        d.observe(Some(500_000), 500_000);
        assert!(d.tick(&c, 700_000).is_empty());
        assert!(d.tick(&c, 3_000_000).is_empty());
        let out = d.on_recognition(&c, span(up(), 500, 1_300), None);
        assert_eq!(names(&c, &out), ["tap-then-up"]);
        // A touch left resting is not waited for forever.
        d.on_recognition(&c, rec(tap(), 2_000), None);
        d.observe(Some(2_200_000), 2_200_000);
        assert!(d.tick(&c, 6_000_000).is_empty());
        assert_eq!(names(&c, &d.tick(&c, 7_200_000)), ["tap3"]);
    }

    #[test]
    fn a_touch_begun_too_late_is_not_waited_for() {
        let c = config();
        let mut d = Dispatcher::default();
        d.on_recognition(&c, rec(tap(), 100), None);
        d.observe(Some(800_000), 800_000);
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
    fn a_recognition_nothing_matches_names_what_is_configured() {
        let c = config();
        // Three fingers resting 1.7 s are a hold, while the sequence's first
        // step wants a tap: "no gesture configured" would hide that.
        let hint = near_miss(&c, &Observation::Hold { fingers: 3 }).unwrap();
        assert!(hint.contains("tap with 3 fingers"), "{hint}");
        assert!(hint.contains("swipe up with 3 fingers"), "{hint}");
        // A finger count the configuration never uses has nothing to suggest.
        assert_eq!(near_miss(&c, &Observation::Hold { fingers: 5 }), None);
    }

    #[test]
    fn a_hold_leaves_its_window_running_until_its_fingers_lift() {
        let c = parse(
            r#"
[[gesture]]
name = "hold-then-up"
type = "sequence"
steps = [{ type = "hold", fingers = 3 }, { type = "swipe", fingers = 3, direction = "up" }]
within_ms = 500
action = { type = "minimize" }
"#,
        )
        .unwrap()
        .config;
        let mut d = Dispatcher::default();
        // A hold fires 600 ms after touch-down; the fingers rest on for
        // another 900 ms, and only then does the next step begin.
        assert!(
            d.on_recognition(&c, rec(Observation::Hold { fingers: 3 }, 600), None)
                .is_empty(),
            "the hold starts the sequence"
        );
        for now_ms in [700, 900, 1_100, 1_300, 1_500] {
            // The touch that began at 0 is the hold's own: still down.
            d.observe(Some(0), now_ms * 1000);
            assert!(
                d.tick(&c, now_ms * 1000).is_empty(),
                "the fingers are still down at {now_ms} ms"
            );
        }
        // They lift: the window starts here, so a step 400 ms later is in
        // time although it is 1.3 s after the hold fired.
        d.observe(None, 1_500_000);
        assert!(d.tick(&c, 1_900_000).is_empty());
        let out = d.on_recognition(&c, span(up(), 1_900, 2_100), None);
        assert_eq!(names(&c, &out), ["hold-then-up"]);
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
