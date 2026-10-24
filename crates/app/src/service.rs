//! The engine thread: touch events and interface commands in; gestures,
//! actions, history and feedback out (SPEC §10.1: the engine never depends
//! on the window and keeps running while it is hidden).

use std::collections::HashMap;
use std::io::Write as _;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use crossbeam_channel::{Receiver, Sender, select};
use glyphpad_core::action::{Action, ActionExecutor, AppInfo, ForegroundInfo};
use glyphpad_core::config::{Config, Gesture};
use glyphpad_core::engine::{Engine, Observation, Recognition, Spec, resolve};
use glyphpad_core::repeat::{Decision, Repeater};
use glyphpad_core::service::{Command, EngineMode, HistoryEntry, Recorded, Shared, UiLink};
use glyphpad_core::symbols::{Sample, TemplateFile};
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
}

/// Something to show in the overlay; a no-op where there is none.
#[derive(Clone)]
struct Feedback {
    #[cfg(windows)]
    overlay: glyphpad_platform::windows::overlay::Overlay,
}

impl Feedback {
    fn new(_config: &Config) -> Self {
        Self {
            #[cfg(windows)]
            overlay: glyphpad_platform::windows::overlay::Overlay::spawn(
                _config.settings.overlay.clone(),
            ),
        }
    }

    fn configure(&self, _config: &Config) {
        #[cfg(windows)]
        self.overlay.configure(_config.settings.overlay.clone());
    }

    fn touchpad(&self, _d: &DeviceInfo) {
        #[cfg(windows)]
        self.overlay.set_touchpad(_d.width_mm, _d.height_mm);
    }

    fn trail(&self, _strokes: Vec<Vec<(f32, f32)>>) {
        #[cfg(windows)]
        self.overlay.trail(_strokes);
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
        ..Default::default()
    }));
    let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
    let ui = Arc::new(UiLink::default());
    let handle = ServiceHandle {
        shared: shared.clone(),
        commands: cmd_tx,
        ui: ui.clone(),
    };
    let feedback = Feedback::new(&loaded.config);
    let executor_feedback = feedback.clone();
    let actions = spawn_action_worker(glyphpad_actions::executor(), executor_feedback);
    let mut service = Service {
        engine: Engine::from_config(&loaded.config),
        config: Arc::new(loaded.config),
        templates,
        location: options.location,
        print: options.print,
        mode: options.mode,
        enabled: true,
        freezer: None,
        feedback,
        foreground: glyphpad_actions::foreground(),
        actions,
        repeater: Repeater::default(),
        devices: HashMap::new(),
        clock: (0, Instant::now()),
        shared,
        ui,
        last_preview: Instant::now(),
    };
    service.rebuild_engine();
    std::thread::Builder::new()
        .name("engine".into())
        .spawn(move || service.run(events, cmd_rx))?;
    Ok(handle)
}

struct Service {
    engine: Engine,
    config: Arc<Config>,
    templates: TemplateFile,
    location: Location,
    print: bool,
    mode: EngineMode,
    enabled: bool,
    freezer: Option<Freezer>,
    feedback: Feedback,
    foreground: Box<dyn ForegroundInfo + Send>,
    actions: Sender<Action>,
    repeater: Repeater,
    devices: HashMap<DeviceId, DeviceInfo>,
    /// Last frame timestamp and when it arrived: maps wall-clock time onto
    /// the frame time base for timeouts.
    clock: (u64, Instant),
    shared: Arc<Mutex<Shared>>,
    ui: Arc<UiLink>,
    last_preview: Instant,
}

/// Live feedback (overlay trail, debug view) is refreshed at most this often.
const LIVE_INTERVAL: Duration = Duration::from_millis(16);
/// Without timeouts pending, the loop still wakes up this rarely.
const IDLE_WAIT: Duration = Duration::from_secs(3600);

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
        loop {
            let wait = self.engine.next_deadline().map_or(IDLE_WAIT, |d| {
                Duration::from_micros(d.saturating_sub(self.now_us()))
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
            for r in recognitions {
                self.on_recognition(r);
            }
        }
    }

    fn on_event(&mut self, event: DeviceEvent) -> Vec<Recognition> {
        match &event {
            DeviceEvent::Frame { device, frame } => {
                self.clock = (frame.timestamp_us, Instant::now());
                let live = self.last_preview.elapsed() >= LIVE_INTERVAL;
                let out = self.engine.on_event(&event);
                if live {
                    self.last_preview = Instant::now();
                    let preview = self.engine.drawing_preview();
                    if let Some((id, strokes)) = &preview
                        && let Some(d) = self.devices.get(id)
                    {
                        let (w, h) = (d.width_mm.max(1.0), d.height_mm.max(1.0));
                        self.feedback.trail(
                            strokes
                                .iter()
                                .map(|s| s.iter().map(|&(x, y)| (x / w, y / h)).collect())
                                .collect(),
                        );
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

    fn on_recognition(&mut self, r: Recognition) {
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
                return;
            }
            EngineMode::RecordDemonstration => {
                if matches!(
                    r.observation,
                    Observation::Tap { .. } | Observation::Hold { .. } | Observation::Swipe { .. }
                ) {
                    self.update_shared(|s| {
                        s.recorded.push(Recorded::Demonstration {
                            observation: r.observation.clone(),
                            trace: r.trace.clone(),
                        })
                    });
                    self.ui.wake();
                }
                return;
            }
            EngineMode::Normal | EngineMode::Test => {}
        }

        let app = self.foreground.foreground_app();
        let config = self.config.clone();
        let gesture = resolve(&config, &r.observation, app.as_ref());
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
                self.feedback
                    .message(label, r.observation.to_string(), true);
                if !test {
                    executed = self.execute(g, app.clone());
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
                }
                Observation::Symbol { name, score, .. } => {
                    tracing::info!("symbol {name} ({score:.2}): no gesture for it here");
                    self.feedback
                        .message(format!("Symbol {name}"), "no gesture configured", false);
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
                        let _ = self.actions.send(last.action);
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
            other => {
                let _ = self.actions.send(other.clone());
                self.repeater.record(g, app, Instant::now());
                true
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

    fn rebuild_engine(&mut self) {
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
            if self.mode == EngineMode::RecordSymbol {
                // In `toggle` draw mode every example is armed while recording.
                e.keep_drawing_armed(true);
            }
            e
        };
        self.freezer = Some(Freezer::new(config));
        self.feedback.configure(config);
        let (templates, config) = (Arc::new(self.templates.clone()), self.config.clone());
        let mode = self.mode;
        self.update_shared(|s| {
            s.effective = Arc::new(effective);
            s.templates = templates;
            s.config = config;
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
                self.config = Arc::new(loaded.config);
                self.update_shared(|s| s.warnings = warnings);
                self.templates = config_file::load_templates(&self.location);
                self.rebuild_engine();
                self.set_status("configuration reloaded");
            }
            Err(e) => self.set_status(format!("{e:#}; keeping the previous configuration")),
        }
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
        config_file::write_atomically(path, &new).map_err(|e| format!("{e:#}"))?;
        Ok(())
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
                if let Some((symbol, samples)) = symbol_samples {
                    self.templates
                        .symbols
                        .entry(symbol)
                        .or_default()
                        .extend(samples);
                    if let Err(e) = self.write_templates() {
                        return self.set_status(format!("templates not saved: {e:#}"));
                    }
                }
                if let Err(e) = config_file::write_atomically(self.location.path(), &new) {
                    return self.set_status(format!("gesture not saved: {e:#}"));
                }
                self.reload();
                self.set_status(format!("gesture \"{name}\" saved"));
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
) -> Sender<Action> {
    let (tx, rx) = crossbeam_channel::unbounded::<Action>();
    std::thread::Builder::new()
        .name("actions".into())
        .spawn(move || {
            for action in rx {
                run_action(executor.as_ref(), &feedback, &action);
            }
        })
        .expect("cannot spawn the action thread");
    tx
}

fn run_action(executor: &dyn ActionExecutor, feedback: &Feedback, action: &Action) {
    match action {
        Action::Chain { actions } => actions
            .iter()
            .for_each(|a| run_action(executor, feedback, a)),
        Action::Wait { ms } => std::thread::sleep(Duration::from_millis(*ms)),
        Action::Notify { title, message } => {
            tracing::info!("notify: {message}");
            feedback.message(
                title.clone().unwrap_or_else(|| "GlyphPad".into()),
                message.clone(),
                true,
            );
        }
        Action::ToggleEnabled | Action::DrawSymbol | Action::RepeatLast | Action::ShowWindow => {
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
