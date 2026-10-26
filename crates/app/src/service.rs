//! The engine thread: touch events and interface commands in; gestures,
//! actions, history and feedback out (SPEC §10.1: the engine never depends
//! on the window and keeps running while it is hidden).

use std::collections::HashMap;
use std::io::Write as _;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use crossbeam_channel::{Receiver, Sender, select};
use glyphpad_core::action::{Action, ActionContext, ActionExecutor, AppInfo, ForegroundInfo};
use glyphpad_core::config::{Config, Gesture};
use glyphpad_core::dispatch::{Dispatched, Dispatcher};
use glyphpad_core::engine::{Engine, Observation, Recognition, Spec};
use glyphpad_core::repeat::{Decision, Repeater};
use glyphpad_core::service::{Command, EngineMode, HistoryEntry, Recorded, Shared, UiLink};
use glyphpad_core::symbols::{Sample, TemplateFile};
use glyphpad_core::trigger::Trigger;
use glyphpad_core::{DeviceEvent, DeviceId, DeviceInfo, config_edit};

use crate::config_file::{self, Location};
use crate::engine_loop::{Freezer, warn_about_native_gestures};
use crate::print;

/// Handle to the running engine thread.
#[derive(Clone)]
pub struct ServiceHandle {
    pub shared: Arc<Mutex<Shared>>,
    pub commands: Sender<Command>,
    pub ui: Arc<UiLink>,
}

pub struct Options {
    pub location: Location,
    /// Print recognitions in the terminal (`glyphpad debug`).
    pub print: bool,
    pub mode: EngineMode,
    /// The updater's inbox, which update commands from the window and the
    /// tray are forwarded to.
    pub updates: Option<Sender<crate::update::Request>>,
}

/// Something to show in the overlay; a no-op where there is none.
#[derive(Clone)]
struct Feedback {
    #[cfg(windows)]
    overlay: glyphpad_platform::windows::overlay::Overlay,
    #[cfg(windows)]
    cheat: glyphpad_platform::windows::cheatsheet::CheatSheet,
}

impl Feedback {
    fn new(_config: &Config) -> Self {
        Self {
            #[cfg(windows)]
            overlay: glyphpad_platform::windows::overlay::Overlay::spawn(
                _config.settings.overlay.clone(),
            ),
            #[cfg(windows)]
            cheat: glyphpad_platform::windows::cheatsheet::CheatSheet::spawn(),
        }
    }

    fn configure(&self, _config: &Config) {
        #[cfg(windows)]
        self.overlay.configure(_config.settings.overlay.clone());
    }

    fn cheat_sheet(
        &self,
        _title: String,
        _rows: Vec<(String, String, String, glyphpad_core::preview::Pictogram)>,
        _seconds: u64,
    ) {
        #[cfg(windows)]
        self.cheat.show(
            glyphpad_platform::windows::cheatsheet::Sheet {
                title: _title,
                rows: _rows
                    .into_iter()
                    .map(|(name, trigger, action, picture)| {
                        glyphpad_platform::windows::cheatsheet::Row {
                            name,
                            trigger,
                            action,
                            picture,
                        }
                    })
                    .collect(),
            },
            _seconds,
        );
    }

    fn touchpad(&self, _d: &DeviceInfo) {
        #[cfg(windows)]
        self.overlay.set_touchpad(_d.width_mm, _d.height_mm);
    }

    fn trail(&self, _strokes: Vec<Vec<(f32, f32)>>) {
        #[cfg(windows)]
        self.overlay.trail(_strokes);
    }

    fn end_trail(&self) {
        #[cfg(windows)]
        self.overlay.end_trail();
    }

    fn message(&self, title: impl Into<String>, detail: impl Into<String>, success: bool) {
        let (title, detail) = (title.into(), detail.into());
        tracing::debug!("overlay: {title} — {detail}");
        #[cfg(windows)]
        self.overlay
            .show(glyphpad_platform::windows::overlay::Message {
                title,
                detail,
                success,
            });
        #[cfg(not(windows))]
        let _ = success;
    }
}

pub fn spawn(events: Receiver<DeviceEvent>, options: Options) -> Result<ServiceHandle> {
    let loaded = config_file::load(&options.location, true)?;
    config_file::log_warnings(&loaded);
    let templates = config_file::load_templates(&options.location);
    let shared = Arc::new(Mutex::new(Shared {
        config: Arc::new(loaded.config.clone()),
        config_path: options.location.path().to_path_buf(),
        templates: Arc::new(templates.clone()),
        effective: Arc::new(templates.effective()),
        enabled: true,
        mode: options.mode,
        warnings: loaded.warnings.iter().map(ToString::to_string).collect(),
        profile: loaded.config.settings.active_profile.clone(),
        ..Default::default()
    }));
    let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
    let ui = Arc::new(UiLink::default());
    let watcher = watch(&options.location, cmd_tx.clone());
    let handle = ServiceHandle {
        shared: shared.clone(),
        commands: cmd_tx,
        ui: ui.clone(),
    };
    let feedback = Feedback::new(&loaded.config);
    let executor_feedback = feedback.clone();
    let actions = spawn_action_worker(
        glyphpad_actions::executor(),
        executor_feedback,
        glyphpad_actions::foreground(),
    );
    let loaded_config = Arc::new(loaded.config);
    let usage = std::fs::read_to_string(config_file::usage_path(&options.location))
        .map(|t| glyphpad_core::usage::Usage::from_json(&t))
        .unwrap_or_default();
    let mut service = Service {
        engine: Engine::from_config(&loaded_config),
        config: loaded_config.clone(),
        loaded: loaded_config,
        profile_override: None,
        usage,
        usage_dirty: false,
        usage_saved: Instant::now(),
        templates,
        location: options.location,
        print: options.print,
        mode: options.mode,
        updates: options.updates,
        enabled: true,
        freezer: None,
        feedback,
        foreground: glyphpad_actions::foreground(),
        actions,
        repeater: Repeater::default(),
        dispatcher: Dispatcher::default(),
        devices: HashMap::new(),
        clock: (0, Instant::now()),
        shared,
        ui,
        last_preview: Instant::now(),
        trail_shown: false,
        zone_hint: None,
        scrolling: None,
        last_auto_pause_check: Instant::now() - AUTO_PAUSE_POLL,
        config_text: Arc::new(String::new()),
        revision: 0,
        files_changed_at: None,
        _watcher: watcher,
    };
    service.read_config_text();
    service.rebuild_engine();
    service.publish_usage();
    let (panic_shared, panic_ui) = (service.shared.clone(), service.ui.clone());
    std::thread::Builder::new()
        .name("engine".into())
        .spawn(move || {
            // The engine owns everything that changes at runtime: if it
            // panics, nothing works any more. Say so in the window instead
            // of leaving a program that silently does nothing.
            let stopped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                service.run(events, cmd_rx)
            }));
            if stopped.is_err() {
                tracing::error!("the gesture engine stopped after an internal error");
                if let Ok(mut s) = panic_shared.lock() {
                    s.status = Some(
                        "The gesture engine stopped after an internal error: quit and start GlyphPad again."
                            .into(),
                    );
                }
                panic_ui.wake();
            }
        })?;
    Ok(handle)
}

struct Service {
    engine: Engine,
    /// The configuration in force: the loaded one, with the profile a rule
    /// picked for the application in front, if any.
    config: Arc<Config>,
    /// The configuration as the file holds it, which the window edits.
    loaded: Arc<Config>,
    /// Profile chosen by a `profile_rules` entry for the foreground app.
    profile_override: Option<String>,
    usage: glyphpad_core::usage::Usage,
    usage_dirty: bool,
    usage_saved: Instant,
    templates: TemplateFile,
    location: Location,
    print: bool,
    mode: EngineMode,
    updates: Option<Sender<crate::update::Request>>,
    enabled: bool,
    freezer: Option<Freezer>,
    feedback: Feedback,
    foreground: Box<dyn ForegroundInfo + Send>,
    actions: Sender<Action>,
    repeater: Repeater,
    dispatcher: Dispatcher,
    devices: HashMap<DeviceId, DeviceInfo>,
    /// Last frame timestamp and when it arrived: maps wall-clock time onto
    /// the frame time base for timeouts.
    clock: (u64, Instant),
    shared: Arc<Mutex<Shared>>,
    ui: Arc<UiLink>,
    last_preview: Instant,
    /// The overlay is showing a drawing trail.
    trail_shown: bool,
    /// The zone gesture last announced while a finger rested on it, so the
    /// overlay says it once per touch.
    zone_hint: Option<String>,
    /// A `scroll_hold` in progress: it scrolls while the fingers move.
    scrolling: Option<ScrollDrag>,
    last_auto_pause_check: Instant,
    /// The configuration file as last loaded, to tell our own writes and
    /// unchanged saves from real edits on disk.
    config_text: Arc<String>,
    revision: u64,
    /// A watched file changed; checked once the burst of events is over.
    files_changed_at: Option<Instant>,
    _watcher: Option<notify::RecommendedWatcher>,
}

/// Saves from the editor closer together than this share one backup.
const BACKUP_MIN_AGE: Duration = Duration::from_secs(120);

/// Editors write files in several steps; wait for the burst to end.
const FILE_SETTLE: Duration = Duration::from_millis(300);

/// Watches the configuration's folder for changes to the configuration and
/// the templates (SPEC §9 hot reload).
fn watch(location: &Location, commands: Sender<Command>) -> Option<notify::RecommendedWatcher> {
    use notify::Watcher as _;
    let dir = location.path().parent()?.to_path_buf();
    let names = [
        location.path().file_name()?.to_os_string(),
        std::ffi::OsString::from(config_file::TEMPLATES_FILE_NAME),
    ];
    let handler = move |event: notify::Result<notify::Event>| {
        if let Ok(e) = event
            && e.paths
                .iter()
                .any(|p| p.file_name().is_some_and(|n| names.iter().any(|x| x == n)))
        {
            let _ = commands.send(Command::FilesChanged);
        }
    };
    match notify::recommended_watcher(handler) {
        Ok(mut w) => match w.watch(&dir, notify::RecursiveMode::NonRecursive) {
            Ok(()) => Some(w),
            Err(e) => {
                tracing::warn!("no hot reload: cannot watch {}: {e}", dir.display());
                None
            }
        },
        Err(e) => {
            tracing::warn!("no hot reload: {e}");
            None
        }
    }
}

/// Live feedback (overlay trail, debug view) is refreshed at most this often.
const LIVE_INTERVAL: Duration = Duration::from_millis(16);
/// Without timeouts pending, the loop still wakes up this rarely.
const IDLE_WAIT: Duration = Duration::from_secs(3600);
/// How often the foreground application is checked for automatic pausing,
/// so the tray icon follows it; a cheap query once a second.
const AUTO_PAUSE_POLL: Duration = Duration::from_secs(1);

impl Service {
    fn now_us(&self) -> u64 {
        self.clock.0 + self.clock.1.elapsed().as_micros() as u64
    }

    fn update_shared(&self, f: impl FnOnce(&mut Shared)) {
        if let Ok(mut s) = self.shared.lock() {
            f(&mut s);
        }
    }

    fn run(mut self, events: Receiver<DeviceEvent>, commands: Receiver<Command>) {
        warn_about_native_gestures(&self.config);
        self.sync_autostart();
        loop {
            self.watch_auto_pause();
            if self
                .files_changed_at
                .is_some_and(|t| t.elapsed() >= FILE_SETTLE)
            {
                self.files_changed_at = None;
                self.check_files();
            }
            let deadline = [self.engine.next_deadline(), self.dispatcher.deadline()]
                .into_iter()
                .flatten()
                .min();
            let idle = if self.auto_pause_configured() {
                AUTO_PAUSE_POLL
            } else {
                IDLE_WAIT
            };
            let wait = deadline.map_or(idle, |d| {
                Duration::from_micros(d.saturating_sub(self.now_us())).min(idle)
            });
            let recognitions = select! {
                recv(events) -> event => match event {
                    Ok(event) => self.on_event(event),
                    Err(_) => {
                        tracing::error!("the touch source stopped");
                        return;
                    }
                },
                recv(commands) -> cmd => match cmd {
                    Ok(cmd) => {
                        self.on_command(cmd);
                        Vec::new()
                    }
                    Err(_) => return,
                },
                default(wait) => self.engine.tick(self.now_us()),
            };
            if let Some(f) = &mut self.freezer {
                f.update(self.engine.drawing_now());
            }
            let mut dispatched = Vec::new();
            for r in recognitions {
                dispatched.extend(self.on_recognition(r));
            }
            let now = self.now_us();
            self.dispatcher.observe(self.engine.busy_since(), now);
            self.sync_trail();
            let config = self.config.clone();
            dispatched.extend(self.dispatcher.tick(&config, now));
            for d in Self::without_explained_drawings(dispatched) {
                self.on_dispatched(d);
            }
        }
    }

    /// Takes the drawing trail off the overlay once nothing is being drawn
    /// any more, whatever ended it: a lift, a timeout, a swipe or tap that
    /// nobody configured. Messages replace it on their own.
    fn sync_trail(&mut self) {
        if self.trail_shown && self.engine.drawing_preview().is_none() {
            self.trail_shown = false;
            self.feedback.end_trail();
        }
    }

    fn on_event(&mut self, event: DeviceEvent) -> Vec<Recognition> {
        match &event {
            DeviceEvent::Frame { device, frame } => {
                // With one pad of several chosen, the others' frames are not
                // gestures: an internal pad under the wrists would otherwise
                // fire what the external one is meant to.
                if !self.pad_wanted(*device) {
                    return Vec::new();
                }
                self.clock = (frame.timestamp_us, Instant::now());
                // The frame that lifts the last finger is never skipped:
                // it is the only one to say nothing touches any more.
                let live = self.last_preview.elapsed() >= LIVE_INTERVAL
                    || frame.contacts.iter().all(|c| !c.tip);
                let out = self.engine.on_event(&event);
                // A hold-then-drag scrolls as the fingers move.
                if let Some(mut drag) = self.scrolling {
                    if frame.contacts.iter().any(|c| c.tip && c.confident) {
                        let (_, y) = mean_position(frame);
                        let notches = drag.update(y);
                        self.scrolling = Some(drag);
                        if notches != 0 {
                            let _ = self.actions.send(Action::Scroll {
                                amount: notches,
                                smooth_ms: Some(60),
                            });
                        }
                    } else {
                        // Fingers lifted: the drag is over.
                        self.scrolling = None;
                    }
                }
                if live {
                    self.last_preview = Instant::now();
                    let preview = self.engine.drawing_preview();
                    if let Some((id, strokes)) = &preview
                        && let Some(d) = self.devices.get(id)
                    {
                        self.trail_shown = true;
                        let (w, h) = (d.width_mm.max(1.0), d.height_mm.max(1.0));
                        self.feedback.trail(
                            strokes
                                .iter()
                                .map(|s| s.iter().map(|&(x, y)| (x / w, y / h)).collect())
                                .collect(),
                        );
                    }
                    // A zone is a button: with the overlay on, say which
                    // gesture it would run as soon as the fingers land on it,
                    // not only once the tap is over (SPEC §13).
                    if self.config.settings.overlay.enabled {
                        let fingers = frame
                            .contacts
                            .iter()
                            .filter(|c| c.tip && c.confident)
                            .count();
                        let hint = if fingers == 0 {
                            None
                        } else {
                            let (x, y) = mean_position(frame);
                            self.config.zone_at(fingers as u8, x, y).map(|g| {
                                let what = g
                                    .trigger
                                    .as_ref()
                                    .map(Trigger::describe)
                                    .unwrap_or_default();
                                (g.def.name.clone(), what)
                            })
                        };
                        if hint.as_ref().map(|(n, _)| n) != self.zone_hint.as_ref() {
                            self.zone_hint = hint.as_ref().map(|(n, _)| n.clone());
                            if let Some((name, what)) = hint {
                                self.feedback.message(name, what, true);
                            }
                        }
                    }
                    let contacts = frame.contacts.clone();
                    let device = *device;
                    self.update_shared(|s| {
                        s.active_device = Some(device);
                        s.contacts = contacts;
                        s.drawing = preview.map(|p| p.1).unwrap_or_default();
                    });
                    if self.ui.is_visible() {
                        self.ui.wake();
                    }
                }
                out
            }
            DeviceEvent::Connected(d) => {
                tracing::info!(
                    "touchpad ready: {} ({:.0}×{:.0} mm)",
                    d.name,
                    d.width_mm,
                    d.height_mm
                );
                if self.print {
                    print::event(&event);
                }
                if self.devices.is_empty() {
                    self.feedback.touchpad(d);
                }
                self.devices.insert(d.id, d.clone());
                self.publish_devices();
                self.engine.on_event(&event)
            }
            DeviceEvent::Disconnected { device } => {
                if self.print {
                    print::event(&event);
                }
                self.devices.remove(device);
                self.publish_devices();
                self.engine.on_event(&event)
            }
        }
    }

    /// Whether frames from `device` are the ones to use: with
    /// `[settings] touchpad` set, only that pad's, otherwise every pad's.
    fn pad_wanted(&self, device: DeviceId) -> bool {
        let wanted = self.config.settings.touchpad.trim();
        if wanted.is_empty() {
            return true;
        }
        self.devices
            .get(&device)
            .is_some_and(|d| d.name.to_lowercase().contains(&wanted.to_lowercase()))
    }

    fn publish_devices(&self) {
        let mut devices: Vec<DeviceInfo> = self.devices.values().cloned().collect();
        devices.sort_by_key(|d| d.id);
        self.update_shared(|s| s.devices = devices);
        self.ui.wake();
    }

    fn pad_size(&self, device: DeviceId) -> (f32, f32) {
        self.devices
            .get(&device)
            .map_or((100.0, 70.0), |d| (d.width_mm, d.height_mm))
    }

    fn on_recognition(&mut self, r: Recognition) -> Vec<Dispatched> {
        let (width_mm, height_mm) = self.pad_size(r.device);
        match self.mode {
            EngineMode::RecordSymbol => {
                if let Observation::Sample { strokes } = r.observation {
                    self.update_shared(|s| {
                        s.recorded.push(Recorded::Sample(Sample {
                            strokes,
                            width_mm,
                            height_mm,
                        }))
                    });
                    self.ui.wake();
                }
                return Vec::new();
            }
            EngineMode::RecordDemonstration => {
                if !matches!(
                    r.observation,
                    Observation::Symbol { .. }
                        | Observation::UnrecognizedSymbol { .. }
                        | Observation::Sample { .. }
                ) {
                    self.update_shared(|s| {
                        s.recorded.push(Recorded::Demonstration {
                            observation: r.observation.clone(),
                            trace: r.trace.clone(),
                        })
                    });
                    self.ui.wake();
                }
                return Vec::new();
            }
            EngineMode::Normal | EngineMode::Test => {}
        }
        let app = self.foreground.foreground_app();
        let config = self.config.clone();
        self.dispatcher.on_recognition(&config, r, app)
    }

    /// Drops the "drawing not recognized" diagnostics that the same stroke
    /// explains. A step drawn with the drawing finger count — the
    /// three-finger swipe that closes a sequence, say — is judged as a
    /// drawing first: the diagnostic would be a second history entry, without
    /// a gesture, for one movement that did run one.
    fn without_explained_drawings(dispatched: Vec<Dispatched>) -> Vec<Dispatched> {
        let spans: Vec<(u64, u64)> = dispatched
            .iter()
            .filter(|d| d.gesture.is_some())
            .map(|d| (d.recognition.started_us, d.recognition.ended_us))
            .collect();
        dispatched
            .into_iter()
            .filter(|d| {
                if !matches!(
                    d.recognition.observation,
                    Observation::UnrecognizedSymbol { .. }
                ) {
                    return true;
                }
                // The gesture came from a stroke inside this drawing.
                !spans.iter().any(|&(start, end)| {
                    start >= d.recognition.started_us && end <= d.recognition.ended_us
                })
            })
            .collect()
    }

    /// A recognition and the gesture it triggers, sequences resolved.
    fn on_dispatched(&mut self, d: Dispatched) {
        let Dispatched {
            gesture,
            recognition: r,
            app,
        } = d;
        let (width_mm, height_mm) = self.pad_size(r.device);
        let config = self.config.clone();
        let gesture = gesture.map(|i| &config.gestures[i]);
        if self.print {
            println!("{}", print::recognition(&r, gesture));
        }
        let (symbol, score) = match &r.observation {
            Observation::Symbol {
                name,
                score,
                candidates,
                ..
            } => {
                let c = candidates.clone();
                self.update_shared(|s| s.last_candidates = c);
                (Some(name.clone()), Some(*score))
            }
            Observation::UnrecognizedSymbol { candidates, .. } => {
                let c = candidates.clone();
                self.update_shared(|s| s.last_candidates = c);
                (None, candidates.first().map(|c| c.score))
            }
            _ => (None, None),
        };

        let test = self.mode == EngineMode::Test;
        let mut executed = false;
        match gesture {
            Some(g) => {
                let label = format!("{}{}", g.def.name, if test { " (test)" } else { "" });
                // A zone is a place on the pad: name it, instead of saying
                // "tap" and leaving the user to guess which one fired.
                let detail = match &g.trigger {
                    Some(t @ Trigger::Zone { .. }) => t.describe(),
                    _ => r.observation.to_string(),
                };
                self.feedback.message(label, detail, true);
                if !test {
                    executed = self.execute(g, app.clone());
                    if executed {
                        let name = g.def.name.clone();
                        self.count(|u| u.record_run(&name));
                    }
                }
            }
            None => match &r.observation {
                Observation::UnrecognizedSymbol { candidates, .. } => {
                    // SPEC §6.2: below the threshold nothing happens; show and
                    // log the best candidate.
                    let best = candidates.first().map_or("no candidates".to_string(), |c| {
                        format!("best: {} {:.2}", c.symbol, c.score)
                    });
                    tracing::info!("drawing not recognized ({best})");
                    self.feedback.message("Not recognized", best, false);
                    if !test {
                        // Kept by symbol, so the statistics say which one is
                        // being drawn badly rather than only how often.
                        let closest = candidates.first().map(|c| (c.symbol.as_str(), c.score));
                        self.count(|u| u.record_unrecognized(closest));
                    }
                }
                Observation::Symbol { name, score, .. } => {
                    tracing::info!("symbol {name} ({score:.2}): no gesture for it here");
                    self.feedback
                        .message(format!("Symbol {name}"), "no gesture configured", false);
                    if !test {
                        self.count(|u| u.unmapped += 1);
                    }
                }
                other => tracing::debug!("{other} (no gesture configured)"),
            },
        }

        // Taps and swipes nobody configured would flood the history.
        if gesture.is_some()
            || symbol.is_some()
            || matches!(r.observation, Observation::UnrecognizedSymbol { .. })
        {
            let entry = HistoryEntry {
                time: jiff::Zoned::now().strftime("%H:%M:%S").to_string(),
                recognized: r.observation.to_string(),
                symbol,
                score,
                gesture: gesture.map(|g| g.def.name.clone()),
                action: gesture.map(|g| g.def.action.kind().to_string()),
                app: app.map(|a| a.exe),
                executed,
                trace: r.trace.clone(),
                width_mm,
                height_mm,
            };
            self.save_history(&entry);
            let limit = config.settings.history_size;
            self.update_shared(|s| {
                s.history.push_back(entry);
                while s.history.len() > limit {
                    s.history.pop_front();
                }
            });
            self.ui.wake();
        }
    }

    /// Runs a gesture's action; returns whether something ran.
    fn execute(&mut self, g: &Gesture, app: Option<AppInfo>) -> bool {
        let action = &g.def.action;
        if let Action::ToggleEnabled = action {
            self.enabled = !self.enabled;
            self.engine.reset();
            let enabled = self.enabled;
            tracing::info!("gestures {}", if enabled { "enabled" } else { "paused" });
            self.feedback.message(
                if enabled {
                    "Gestures on"
                } else {
                    "Gestures paused"
                },
                "",
                enabled,
            );
            self.update_shared(|s| s.enabled = enabled);
            self.ui.wake();
            return true;
        }
        if !self.enabled {
            tracing::debug!("paused: ignoring gesture \"{}\"", g.def.name);
            return false;
        }
        if let Some(reason) = self.auto_pause_reason(app.as_ref()) {
            tracing::info!("gesture \"{}\" not run: {reason}", g.def.name);
            return false;
        }
        tracing::info!(
            "gesture \"{}\" in {} → {}",
            g.def.name,
            app.as_ref().map_or("?", |a| a.exe.as_str()),
            action.kind()
        );
        match action {
            Action::RepeatLast => {
                match self
                    .repeater
                    .decide(&self.config.settings, app.as_ref(), Instant::now())
                {
                    Decision::Repeat(last) => {
                        tracing::info!("repeating \"{}\" → {}", last.gesture, last.action.kind());
                        let action = self.with_placeholders(&last.action, app.as_ref());
                        let _ = self.actions.send(action);
                        true
                    }
                    Decision::Nothing => {
                        tracing::info!("repeat_last: nothing to repeat yet");
                        false
                    }
                    Decision::Expired => {
                        tracing::info!(
                            "repeat_last: the last action is older than repeat_timeout_s"
                        );
                        false
                    }
                    Decision::OtherApp { then, now } => {
                        tracing::info!(
                            "repeat_last: not repeated, another window is in front ({then} → {now})"
                        );
                        self.feedback.message(
                            "Not repeated",
                            format!("it ran in another window ({then}); now {now} is in front"),
                            false,
                        );
                        false
                    }
                }
            }
            Action::DrawSymbol => {
                let timeout = self.config.settings.draw_activation.arm_timeout_ms();
                self.engine.arm_drawing(self.now_us() + timeout * 1000);
                self.feedback
                    .message("Draw a symbol", "with one finger", true);
                true
            }
            Action::ShowWindow => {
                self.ui.request_show();
                true
            }
            Action::SwitchProfile { profile } => {
                self.set_active_profile(profile.clone());
                true
            }
            Action::NextProfile => {
                self.next_profile();
                true
            }
            Action::ShowCheatSheet => {
                self.show_cheat_sheet();
                true
            }
            Action::ScrollHold { mm_per_notch } => {
                // The frames that follow scroll, until the fingers lift.
                self.scrolling = Some(ScrollDrag::new(*mm_per_notch));
                true
            }
            other => {
                let action = self.with_placeholders(other, app.as_ref());
                let _ = self.actions.send(action);
                self.repeater.record(g, app, Instant::now());
                true
            }
        }
    }

    /// The values a placeholder in an action's text stands for (SPEC §9):
    /// the window in front, the active profile, the date and the time.
    fn action_context(&self, app: Option<&AppInfo>) -> ActionContext {
        let now = jiff::Zoned::now();
        ActionContext {
            app: app.map(|a| a.exe.clone()).unwrap_or_default(),
            title: app.map(|a| a.title.clone()).unwrap_or_default(),
            profile: self.config.settings.active_profile.clone(),
            date: now.strftime("%Y-%m-%d").to_string(),
            time: now.strftime("%H:%M").to_string(),
            clipboard: None,
        }
    }

    /// `action` with its placeholders filled in. The clipboard is read only
    /// when the action asks for it, since reading it can fail or block.
    fn with_placeholders(&self, action: &Action, app: Option<&AppInfo>) -> Action {
        let mut ctx = self.action_context(app);
        if ActionContext::mentions_clipboard(action) {
            ctx.clipboard = glyphpad_actions::clipboard_text();
        }
        ctx.resolve(action)
    }

    fn auto_pause_configured(&self) -> bool {
        let s = &self.config.settings;
        s.pause_when_fullscreen || !s.pause_in_apps.is_empty()
    }

    /// Why gestures are paused for this foreground application (SPEC §13).
    fn auto_pause_reason(&self, app: Option<&AppInfo>) -> Option<String> {
        let app = app?;
        let s = &self.config.settings;
        if s.pause_in_apps
            .iter()
            .any(|a| a.eq_ignore_ascii_case(&app.exe))
        {
            return Some(format!("paused while {} is in front", app.exe));
        }
        if s.pause_when_fullscreen && app.fullscreen {
            return Some(format!("paused while {} is full screen", app.exe));
        }
        None
    }

    fn watch_auto_pause(&mut self) {
        if self.last_auto_pause_check.elapsed() < AUTO_PAUSE_POLL {
            return;
        }
        self.last_auto_pause_check = Instant::now();
        self.flush_usage();
        let needs_app =
            self.auto_pause_configured() || !self.loaded.settings.profile_rules.is_empty();
        let app = needs_app
            .then(|| self.foreground.foreground_app())
            .flatten();
        self.follow_profile_rules(app.as_ref());
        let reason = if self.auto_pause_configured() {
            self.auto_pause_reason(app.as_ref())
        } else {
            None
        };
        let changed = self.shared.lock().is_ok_and(|s| s.auto_paused != reason);
        if changed {
            if let Some(r) = &reason {
                tracing::info!("gestures {r}");
            }
            self.update_shared(|s| s.auto_paused = reason);
            self.ui.wake();
        }
    }

    /// Makes the sign-in registration match `autostart` (SPEC §10.4).
    fn sync_autostart(&self) {
        #[cfg(windows)]
        {
            use glyphpad_platform::windows::autostart;
            let wanted = self.config.settings.autostart.then(|| {
                let exe = std::env::current_exe().unwrap_or_default();
                match &self.location {
                    Location::Explicit(p) => {
                        format!("\"{}\" run --config \"{}\"", exe.display(), p.display())
                    }
                    _ => format!("\"{}\" run", exe.display()),
                }
            });
            if autostart::current() != wanted {
                match autostart::set(wanted.as_deref()) {
                    Ok(()) => tracing::info!(
                        "start with Windows: {}",
                        if wanted.is_some() { "on" } else { "off" }
                    ),
                    Err(e) => tracing::warn!("cannot change start with Windows: {e}"),
                }
            }
        }
    }

    fn save_history(&self, entry: &HistoryEntry) {
        if !self.config.settings.save_history {
            return;
        }
        let path = config_file::history_path(&self.location);
        let line = match serde_json::to_string(entry) {
            Ok(l) => l,
            Err(e) => return tracing::warn!("cannot serialize history: {e}"),
        };
        let written = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .and_then(|mut f| writeln!(f, "{line}"));
        if let Err(e) = written {
            tracing::warn!("cannot write {}: {e}", path.display());
        }
    }

    /// The loaded configuration with the profile a rule picked, if any.
    fn effective_config(&self) -> Arc<Config> {
        match &self.profile_override {
            Some(p) if *p != self.loaded.settings.active_profile => {
                let mut c = (*self.loaded).clone();
                c.settings.active_profile = p.clone();
                Arc::new(c)
            }
            _ => self.loaded.clone(),
        }
    }

    /// Makes `name` the active profile and saves the choice in the file.
    fn set_active_profile(&mut self, name: String) {
        let value = name.clone();
        match self.edit_config(|text| {
            config_edit::set_value(text, &["settings", "active_profile"], value)
        }) {
            Ok(()) => {
                self.reload();
                self.feedback.message("Profile", name, true);
            }
            Err(e) => self.set_status(format!("profile not changed: {e}")),
        }
    }

    fn next_profile(&mut self) {
        let names = self.loaded.profile_names();
        let current = &self.loaded.settings.active_profile;
        let at = names.iter().position(|p| p == current).unwrap_or(0);
        if let Some(next) = names.get((at + 1) % names.len().max(1)).cloned()
            && names.len() > 1
        {
            self.set_active_profile(next);
        }
    }

    /// Follows `profile_rules`: while a listed application is in front its
    /// profile is used, without touching the file.
    fn follow_profile_rules(&mut self, app: Option<&AppInfo>) {
        let wanted = app
            .and_then(|a| self.loaded.profile_for(a))
            .map(str::to_string);
        if wanted == self.profile_override {
            return;
        }
        // Not in the middle of a gesture: rebuilding forgets what is in progress.
        let busy = self
            .shared
            .lock()
            .is_ok_and(|s| s.contacts.iter().any(|c| c.tip));
        if busy {
            return;
        }
        self.profile_override = wanted;
        self.config = self.effective_config();
        let name = self.config.settings.active_profile.clone();
        tracing::info!("profile: {name}");
        self.rebuild_engine();
        self.feedback.message("Profile", name, true);
    }

    /// The gestures that work now, with a picture each, on screen for a while.
    fn show_cheat_sheet(&mut self) {
        let config = &self.config;
        let active = &config.settings.active_profile;
        let effective = self.templates.effective();
        let app = self.foreground.foreground_app();
        let rows: Vec<(String, String, String, glyphpad_core::preview::Pictogram)> = config
            .gestures
            .iter()
            .filter(|g| g.applies(active, app.as_ref()))
            .map(|g| {
                let (trigger, picture) = match &g.trigger {
                    Some(t) => (t.describe(), glyphpad_core::preview::of(t, &effective)),
                    None => (
                        g.steps
                            .iter()
                            .map(|t| t.describe())
                            .collect::<Vec<_>>()
                            .join(" → "),
                        glyphpad_core::preview::Pictogram::default(),
                    ),
                };
                (
                    g.def.name.clone(),
                    trigger,
                    Action::short_label(g.def.action.kind()).to_string(),
                    picture,
                )
            })
            .collect();
        let title = format!("Gestures · profile {active}");
        let seconds = config.settings.cheat_sheet_s;
        self.feedback.cheat_sheet(title, rows, seconds);
    }

    fn publish_usage(&self) {
        let usage = Arc::new(self.usage.clone());
        self.update_shared(|s| s.usage = usage);
        self.ui.wake();
    }

    /// Statistics are written at most every 20 s, and only if they changed.
    fn flush_usage(&mut self) {
        if !self.usage_dirty || self.usage_saved.elapsed() < Duration::from_secs(20) {
            return;
        }
        self.usage_dirty = false;
        self.usage_saved = Instant::now();
        let path = config_file::usage_path(&self.location);
        if let Err(e) = config_file::write_atomically(&path, &self.usage.to_json()) {
            tracing::warn!("cannot save the statistics: {e}");
        }
    }

    fn count(&mut self, f: impl FnOnce(&mut glyphpad_core::usage::Usage)) {
        if !self.config.settings.statistics {
            return;
        }
        if self.usage.since.is_none() {
            self.usage.since = Some(jiff::Zoned::now().strftime("%Y-%m-%d").to_string());
        }
        f(&mut self.usage);
        self.usage_dirty = true;
        self.publish_usage();
    }

    fn rebuild_engine(&mut self) {
        self.dispatcher.reset();
        let effective = self.templates.effective();
        let config = &self.config;
        self.engine = if self.mode == EngineMode::RecordDemonstration {
            // Demonstrations recognize every variant, without drawing.
            let mut spec = Spec::everything();
            spec.draw = None;
            Engine::new(spec, (&config.settings).into())
        } else {
            let mut e = Engine::from_config(config);
            e.set_templates(&effective, config.settings.rotation_tolerance_deg);
            e.set_recording(self.mode == EngineMode::RecordSymbol);
            if self.mode == EngineMode::RecordSymbol
                && matches!(
                    config.settings.draw_activation,
                    glyphpad_core::config::DrawActivation::Toggle { .. }
                )
            {
                // Only `toggle` arms the drawing: there is no other way to
                // reach it while recording. The other modes draw the way the
                // settings say — three fingers, an edge, a hold — so a single
                // finger does not become an example (and does not fire).
                e.keep_drawing_armed(true);
            }
            e
        };
        self.freezer = Some(Freezer::new(config));
        self.feedback.configure(config);
        let (templates, config) = (Arc::new(self.templates.clone()), self.loaded.clone());
        let profile = self.config.settings.active_profile.clone();
        let mode = self.mode;
        self.update_shared(|s| {
            s.effective = Arc::new(effective);
            s.templates = templates;
            s.config = config;
            s.profile = profile;
            s.mode = mode;
        });
        self.ui.wake();
    }

    fn set_status(&self, status: impl Into<String>) {
        let status = status.into();
        tracing::info!("{status}");
        self.update_shared(|s| s.status = Some(status));
        self.ui.wake();
    }

    /// Re-reads configuration and templates. An invalid file never stops
    /// the engine: the last valid configuration stays active (SPEC §9).
    fn reload(&mut self) {
        match config_file::load(&self.location, false) {
            Ok(loaded) => {
                config_file::log_warnings(&loaded);
                let warnings = loaded.warnings.iter().map(ToString::to_string).collect();
                self.loaded = Arc::new(loaded.config);
                self.config = self.effective_config();
                self.update_shared(|s| s.warnings = warnings);
                self.read_config_text();
                self.sync_autostart();
                self.templates = config_file::load_templates(&self.location);
                self.rebuild_engine();
                self.set_status("configuration reloaded");
            }
            Err(e) => self.set_status(format!("{e:#}; keeping the previous configuration")),
        }
    }

    /// Publishes the file's text and a new revision for the editor.
    fn read_config_text(&mut self) {
        let text = std::fs::read_to_string(self.location.path()).unwrap_or_default();
        self.config_text = Arc::new(text);
        self.revision += 1;
        let (text, revision) = (self.config_text.clone(), self.revision);
        let backups = config_file::list_backups(&self.location);
        self.update_shared(|s| {
            s.config_text = text;
            s.revision = revision;
            s.backups = backups;
        });
    }

    /// After a change on disk: reload what really changed. Our own writes
    /// come back here too and are recognized by their content.
    fn check_files(&mut self) {
        let text = std::fs::read_to_string(self.location.path()).unwrap_or_default();
        if text != *self.config_text {
            tracing::info!("the configuration changed on disk");
            self.reload();
            return;
        }
        let templates = config_file::load_templates(&self.location);
        if templates != self.templates {
            tracing::info!("the templates changed on disk");
            self.templates = templates;
            self.rebuild_engine();
            self.set_status("templates reloaded");
        }
    }

    /// Writes a new configuration after backing up the current one. With
    /// `throttle_backup`, saves that follow each other closely (the editor
    /// saves automatically) share the backup made before the first.
    fn write_config(&mut self, text: &str, throttle_backup: bool) -> Result<(), String> {
        glyphpad_core::config::parse(text).map_err(|errors| {
            errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ")
        })?;
        let keep = self.config.settings.backup_count;
        if throttle_backup {
            config_file::backup_unless_recent(&self.location, keep, BACKUP_MIN_AGE)
        } else {
            config_file::backup(&self.location, keep)
        }
        .map_err(|e| format!("{e:#}"))?;
        config_file::write_atomically(self.location.path(), text).map_err(|e| format!("{e:#}"))?;
        Ok(())
    }

    fn write_templates(&self) -> Result<()> {
        config_file::write_atomically(
            &config_file::templates_path(&self.location),
            &self.templates.to_json(),
        )
    }

    fn edit_config(
        &mut self,
        edit: impl FnOnce(&str) -> Result<String, String>,
    ) -> Result<(), String> {
        let path = self.location.path();
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let new = edit(&text)?;
        self.write_config(&new, false)
    }

    fn forward(&self, request: crate::update::Request) {
        if let Some(tx) = &self.updates {
            let _ = tx.send(request);
        }
    }

    fn on_command(&mut self, cmd: Command) {
        match cmd {
            Command::SetMode(mode) => {
                if mode.is_recording() {
                    self.update_shared(|s| s.recorded.clear());
                }
                self.mode = mode;
                self.rebuild_engine();
            }
            Command::ToggleEnabled => {
                self.enabled = !self.enabled;
                self.engine.reset();
                let enabled = self.enabled;
                self.update_shared(|s| s.enabled = enabled);
                self.ui.wake();
            }
            Command::Reload => self.reload(),
            Command::SaveGesture {
                def,
                symbol_samples,
            } => {
                let name = def.name.clone();
                // Validate the configuration change before touching any file.
                let text = match std::fs::read_to_string(self.location.path()) {
                    Ok(t) => t,
                    Err(e) => {
                        return self.set_status(format!("cannot read the configuration: {e}"));
                    }
                };
                let new = match config_edit::append_gesture(&text, &def) {
                    Ok(n) => n,
                    Err(e) => return self.set_status(format!("gesture not saved: {e}")),
                };
                // The configuration first: if it cannot be written, the
                // samples it would have used must not stay behind.
                if let Err(e) = self.write_config(&new, false) {
                    return self.set_status(format!("gesture not saved: {e}"));
                }
                let mut templates_error = None;
                if let Some((symbol, samples)) = symbol_samples {
                    self.templates
                        .symbols
                        .entry(symbol)
                        .or_default()
                        .extend(samples);
                    if let Err(e) = self.write_templates() {
                        templates_error = Some(format!("{e:#}"));
                    }
                }
                // The configuration is read back whatever happened to the
                // examples: the gesture is in the file, and the window has to
                // show it even when its templates could not be written.
                self.reload();
                match templates_error {
                    Some(e) => self.set_status(format!(
                        "gesture \"{name}\" saved, but its examples were not: {e}"
                    )),
                    None => self.set_status(format!("gesture \"{name}\" saved")),
                }
            }
            Command::AddTemplate { symbol, sample } => {
                self.templates
                    .symbols
                    .entry(symbol.clone())
                    .or_default()
                    .push(sample);
                match self.write_templates() {
                    Ok(()) => {
                        self.rebuild_engine();
                        self.set_status(format!("template added to \"{symbol}\""));
                    }
                    Err(e) => self.set_status(format!("template not saved: {e:#}")),
                }
            }
            Command::AddTemplates { symbol, samples } => {
                self.templates
                    .symbols
                    .entry(symbol.clone())
                    .or_default()
                    .extend(samples);
                match self.write_templates() {
                    Ok(()) => {
                        self.rebuild_engine();
                        self.set_status(format!("templates added to \"{symbol}\""));
                    }
                    Err(e) => self.set_status(format!("templates not saved: {e:#}")),
                }
            }
            Command::MergeTemplates { symbol, indices } => {
                let Some(list) = self.templates.symbols.get_mut(&symbol) else {
                    return self.set_status(format!("\"{symbol}\" has no examples"));
                };
                let mut chosen: Vec<usize> = indices
                    .iter()
                    .copied()
                    .filter(|&i| i < list.len())
                    .collect();
                chosen.sort_unstable();
                chosen.dedup();
                if chosen.len() < 2 {
                    return self.set_status("choose at least two examples to merge");
                }
                let samples: Vec<Sample> = chosen.iter().map(|&i| list[i].clone()).collect();
                let merged = Sample::merged(&samples).expect("at least two samples");
                // The merged one takes the first's place; the others go.
                let keep = chosen[0];
                for &i in chosen.iter().rev() {
                    list.remove(i);
                }
                list.insert(keep.min(list.len()), merged);
                match self.write_templates() {
                    Ok(()) => {
                        self.rebuild_engine();
                        self.set_status(format!(
                            "{} examples of \"{symbol}\" merged into one",
                            chosen.len()
                        ));
                    }
                    Err(e) => self.set_status(format!("templates not saved: {e:#}")),
                }
            }
            Command::SplitTemplate { symbol, index } => {
                let Some(list) = self.templates.symbols.get_mut(&symbol) else {
                    return self.set_status(format!("\"{symbol}\" has no examples"));
                };
                if index >= list.len() {
                    return self.set_status("that example does not exist");
                }
                let parts = list[index].split();
                if parts.len() < 2 {
                    return self.set_status("this example has a single stroke: nothing to divide");
                }
                let n = parts.len();
                list.remove(index);
                for (k, part) in parts.into_iter().enumerate() {
                    list.insert(index + k, part);
                }
                match self.write_templates() {
                    Ok(()) => {
                        self.rebuild_engine();
                        self.set_status(format!(
                            "example of \"{symbol}\" divided into {n} strokes"
                        ));
                    }
                    Err(e) => self.set_status(format!("templates not saved: {e:#}")),
                }
            }
            Command::DeleteTemplate { symbol, index } => {
                if let Some(list) = self.templates.symbols.get_mut(&symbol)
                    && index < list.len()
                {
                    list.remove(index);
                    if list.is_empty() {
                        self.templates.symbols.remove(&symbol);
                    }
                    match self.write_templates() {
                        Ok(()) => {
                            self.rebuild_engine();
                            self.set_status(format!("template of \"{symbol}\" deleted"));
                        }
                        Err(e) => self.set_status(format!("templates not saved: {e:#}")),
                    }
                }
            }
            Command::HideBuiltins { symbol, hidden } => {
                if hidden {
                    self.templates.hidden_builtins.insert(symbol);
                } else {
                    self.templates.hidden_builtins.remove(&symbol);
                }
                match self.write_templates() {
                    Ok(()) => self.rebuild_engine(),
                    Err(e) => self.set_status(format!("templates not saved: {e:#}")),
                }
            }
            Command::SetSetting { path, value } => {
                let keys: Vec<&str> = path.iter().map(String::as_str).collect();
                match self.edit_config(|text| config_edit::set_value(text, &keys, value)) {
                    Ok(()) => self.reload(),
                    Err(e) => self.set_status(format!("setting not saved: {e}")),
                }
            }
            Command::RemoveSetting { path } => {
                let keys: Vec<&str> = path.iter().map(String::as_str).collect();
                match self.edit_config(|text| config_edit::remove_value(text, &keys)) {
                    Ok(()) => self.reload(),
                    Err(e) => self.set_status(format!("setting not saved: {e}")),
                }
            }
            Command::WriteConfig {
                text,
                base_revision,
            } => {
                if base_revision != self.revision {
                    self.update_shared(|s| s.write_refusals += 1);
                    return self
                        .set_status("not saved: the configuration changed on disk meanwhile");
                }
                match self.write_config(&text, true) {
                    Ok(()) => {
                        self.reload();
                        self.set_status("configuration saved");
                    }
                    Err(e) => self.set_status(format!("not saved: {e}")),
                }
            }
            Command::RestoreBackup { name } => {
                let path = config_file::backups_dir(&self.location).join(&name);
                match std::fs::read_to_string(&path)
                    .map_err(|e| e.to_string())
                    .and_then(|t| self.write_config(&t, false))
                {
                    Ok(()) => {
                        self.reload();
                        self.set_status(format!("restored {name}"));
                    }
                    Err(e) => self.set_status(format!("cannot restore {name}: {e}")),
                }
            }
            Command::CheckUpdates { manual } => {
                self.forward(crate::update::Request::Check { manual })
            }
            Command::InstallUpdate => self.forward(crate::update::Request::Install),
            Command::IgnoreUpdate(version) => self.forward(crate::update::Request::Ignore(version)),
            Command::SwitchProfile(Some(name)) => self.set_active_profile(name),
            Command::SwitchProfile(None) => self.next_profile(),
            Command::ResetUsage => {
                self.usage = glyphpad_core::usage::Usage::default();
                self.usage_dirty = true;
                self.usage_saved = Instant::now() - Duration::from_secs(60);
                self.publish_usage();
                self.flush_usage();
            }
            Command::FilesChanged => self.files_changed_at = Some(Instant::now()),
            Command::ListWindows => {
                let apps = self.foreground.windows();
                self.update_shared(|s| s.open_apps = apps);
                self.ui.wake();
            }
            Command::RunAction(action) => {
                let _ = self.actions.send(action);
            }
            Command::DiscardRecorded(i) => {
                self.update_shared(|s| {
                    if i < s.recorded.len() {
                        s.recorded.remove(i);
                    }
                });
                self.ui.wake();
            }
            Command::ClearRecorded => {
                self.update_shared(|s| s.recorded.clear());
                self.ui.wake();
            }
        }
    }
}

/// Actions run on their own thread so a slow one (`wait`, a program being
/// started) never delays recognition.
fn spawn_action_worker(
    executor: Box<dyn ActionExecutor + Send>,
    feedback: Feedback,
    foreground: Box<dyn ForegroundInfo + Send>,
) -> Sender<Action> {
    let (tx, rx) = crossbeam_channel::unbounded::<Action>();
    std::thread::Builder::new()
        .name("actions".into())
        .spawn(move || {
            for action in rx {
                run_action(executor.as_ref(), &feedback, foreground.as_ref(), &action);
            }
        })
        .expect("cannot spawn the action thread");
    tx
}

/// Turns the vertical movement of one touch into wheel notches, for
/// `scroll_hold`: the hold fires once, then every `mm_per_notch` of travel
/// scrolls, positive downwards.
#[derive(Debug, Clone, Copy)]
struct ScrollDrag {
    mm_per_notch: f32,
    /// Where the fingers were last frame; `None` until the first.
    last_y: Option<f32>,
    /// Movement not yet worth a notch, in millimetres.
    carry: f32,
}

impl ScrollDrag {
    fn new(mm_per_notch: f32) -> Self {
        Self {
            mm_per_notch: mm_per_notch.max(1.0),
            last_y: None,
            carry: 0.0,
        }
    }

    /// Moves to the fingers' new height and returns the whole notches that
    /// movement earned; the rest stays for the next frame.
    fn update(&mut self, y_mm: f32) -> i32 {
        let Some(last) = self.last_y else {
            self.last_y = Some(y_mm);
            return 0;
        };
        self.carry += y_mm - last;
        self.last_y = Some(y_mm);
        let notches = (self.carry / self.mm_per_notch).trunc();
        self.carry -= notches * self.mm_per_notch;
        notches.clamp(-20.0, 20.0) as i32
    }
}

/// Where the fingers of a frame are, in millimetres: their mean.
fn mean_position(frame: &glyphpad_core::TouchFrame) -> (f32, f32) {
    let touching: Vec<&glyphpad_core::Contact> = frame
        .contacts
        .iter()
        .filter(|c| c.tip && c.confident)
        .collect();
    if touching.is_empty() {
        return (0.0, 0.0);
    }
    let n = touching.len() as f32;
    (
        touching.iter().map(|c| c.x_mm).sum::<f32>() / n,
        touching.iter().map(|c| c.y_mm).sum::<f32>() / n,
    )
}

fn run_action(
    executor: &dyn ActionExecutor,
    feedback: &Feedback,
    foreground: &dyn ForegroundInfo,
    action: &Action,
) {
    match action {
        Action::Chain { actions } => actions
            .iter()
            .for_each(|a| run_action(executor, feedback, foreground, a)),
        Action::Repeat { times, action } => {
            for _ in 0..*times {
                run_action(executor, feedback, foreground, action);
            }
        }
        Action::OnlyIf { when, action } => {
            // The condition is evaluated now, not when the gesture fired: a
            // chain may have changed the window in front.
            let app = foreground.foreground_app();
            if when.matches(app.as_ref()) {
                run_action(executor, feedback, foreground, action);
            } else {
                tracing::info!(
                    "`only_if` did not match ({} is in front): `{}` not run",
                    app.as_ref().map_or("nothing", |a| a.exe.as_str()),
                    action.kind()
                );
            }
        }
        Action::Wait { ms } => std::thread::sleep(Duration::from_millis(*ms)),
        Action::Notify { title, message } => {
            tracing::info!("notify: {message}");
            feedback.message(
                title.clone().unwrap_or_else(|| "GlyphPad".into()),
                message.clone(),
                true,
            );
        }
        Action::ToggleEnabled
        | Action::DrawSymbol
        | Action::RepeatLast
        | Action::ShowWindow
        | Action::SwitchProfile { .. }
        | Action::NextProfile
        | Action::ShowCheatSheet => {
            tracing::warn!("`{}` inside a chain is ignored", action.kind())
        }
        other => {
            if let Err(e) = executor.execute(other) {
                tracing::error!("action `{}` failed: {e}", other.kind());
                feedback.message(format!("{} failed", other.kind()), e.to_string(), false);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dispatched(
        observation: Observation,
        start_ms: u64,
        end_ms: u64,
        gesture: Option<usize>,
    ) -> Dispatched {
        Dispatched {
            gesture,
            recognition: Recognition {
                device: glyphpad_core::DeviceId(1),
                observation,
                at_us: end_ms * 1000,
                ended_us: end_ms * 1000,
                started_us: start_ms * 1000,
                trace: Default::default(),
            },
            app: None,
        }
    }

    fn drawing() -> Observation {
        Observation::UnrecognizedSymbol {
            candidates: Vec::new(),
            strokes: Vec::new(),
        }
    }

    fn swipe() -> Observation {
        Observation::Swipe {
            fingers: 3,
            direction: glyphpad_core::config::Direction::Down,
            distance_mm: 40.0,
            speed_mm_s: 200.0,
        }
    }

    #[test]
    fn a_drag_turns_movement_into_whole_notches() {
        let mut drag = ScrollDrag::new(5.0);
        assert_eq!(drag.update(10.0), 0, "the first frame only sets the start");
        assert_eq!(drag.update(12.0), 0, "2 mm is not a notch yet");
        assert_eq!(drag.update(16.0), 1, "6 mm in all: one notch, 1 mm kept");
        // Upwards gives negative notches, which scroll up.
        assert_eq!(drag.update(4.0), -2, "12 mm up: two notches up");
        // A tiny notch size and a long move stay within the cap.
        let mut fast = ScrollDrag::new(1.0);
        fast.update(0.0);
        assert_eq!(fast.update(500.0), 20);
    }

    #[test]
    fn a_drawing_whose_own_stroke_ran_a_gesture_is_not_reported_twice() {
        // The three-finger swipe that closed a sequence: the drawing spans
        // 1.0–2.0 s, the stroke inside it 1.8–2.0 s.
        let kept = Service::without_explained_drawings(vec![
            dispatched(drawing(), 1_000, 2_000, None),
            dispatched(swipe(), 1_800, 2_000, Some(0)),
        ]);
        assert_eq!(kept.len(), 1, "{kept:?}");
        assert!(matches!(
            kept[0].recognition.observation,
            Observation::Swipe { .. }
        ));
    }

    #[test]
    fn a_drawing_nothing_explains_stays_in_the_history() {
        let kept = Service::without_explained_drawings(vec![
            dispatched(drawing(), 1_000, 2_000, None),
            // A gesture from another movement, outside the drawing.
            dispatched(swipe(), 3_000, 3_200, Some(0)),
        ]);
        assert_eq!(kept.len(), 2, "{kept:?}");
    }
}
