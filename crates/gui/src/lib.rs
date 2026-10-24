//! The egui/eframe window: gestures, gesture review, settings and live
//! debug tabs (SPEC §10).
//!
//! The window is only a view: the engine runs on its own thread and talks
//! to it through [`Shared`] and [`Command`]s. While the window is hidden no
//! UI pass runs at all; eframe still calls [`eframe::App::logic`] when the
//! engine or the tray wake it, which is where tray clicks and "show window"
//! requests are handled.

mod canvas;
mod debug;
mod gestures;
mod icon;
mod review;
mod settings;
mod style;
mod wizard;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crossbeam_channel::Sender;
use eframe::egui::ViewportCommand;
use glyphpad_core::service::{Command, Shared, UiLink};

pub use eframe::egui;
pub use icon::icon_rgba;

/// What the tray menu asks for (SPEC §10.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    Open,
    ToggleEnabled,
    Reload,
    OpenConfigFolder,
    CheckUpdates,
    Quit,
}

/// The system tray icon, implemented by the application.
pub trait Tray {
    /// Actions requested since the last call.
    fn poll(&mut self) -> Vec<TrayAction>;
    /// Shows whether gestures are enabled or paused.
    fn set_enabled(&mut self, enabled: bool);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Gestures,
    Review,
    Settings,
    Debug,
}

pub struct Options {
    pub start_hidden: bool,
    pub tab: Tab,
    pub version: String,
}

/// Everything the window needs from the application.
pub struct Host {
    pub shared: Arc<Mutex<Shared>>,
    pub commands: Sender<Command>,
    pub ui: Arc<UiLink>,
    /// Creates the tray icon once the event loop runs; the context lets
    /// tray events wake the window up.
    pub tray: Box<dyn FnOnce(egui::Context) -> Option<Box<dyn Tray>>>,
    pub open_folder: Box<dyn Fn(&Path)>,
    /// Activates the window given its native handle, past the system's
    /// focus-stealing prevention.
    pub bring_to_front: Box<dyn Fn(isize)>,
}

pub fn run(options: Options, host: Host) -> Result<(), String> {
    let icon = egui::IconData {
        rgba: icon_rgba(64, false),
        width: 64,
        height: 64,
    };
    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("GlyphPad")
            .with_app_id("glyphpad")
            .with_inner_size([980.0, 700.0])
            .with_min_inner_size([680.0, 480.0])
            .with_icon(icon)
            .with_visible(!options.start_hidden),
        ..Default::default()
    };
    let Host {
        shared,
        commands,
        ui,
        tray,
        open_folder,
        bring_to_front,
    } = host;
    eframe::run_native(
        "GlyphPad",
        native,
        Box::new(move |cc| {
            style::install(&cc.egui_ctx);
            let ctx = cc.egui_ctx.clone();
            ui.set_repaint(move || ctx.request_repaint());
            ui.set_visible(!options.start_hidden);
            let tray = tray(cc.egui_ctx.clone());
            Ok(Box::new(App {
                shared,
                commands,
                ui,
                tray,
                open_folder,
                bring_to_front,
                hwnd: None,
                tab: options.tab,
                visible: !options.start_hidden,
                quitting: false,
                tray_enabled: None,
                version: options.version,
                gestures: gestures::State::default(),
                review: review::State::default(),
                settings: settings::State::default(),
                wizard: None,
            }))
        }),
    )
    .map_err(|e| e.to_string())
}

struct App {
    shared: Arc<Mutex<Shared>>,
    commands: Sender<Command>,
    ui: Arc<UiLink>,
    tray: Option<Box<dyn Tray>>,
    open_folder: Box<dyn Fn(&Path)>,
    bring_to_front: Box<dyn Fn(isize)>,
    /// Native handle of the window, once known.
    hwnd: Option<isize>,
    tab: Tab,
    visible: bool,
    /// Quitting for real, not hiding to the tray.
    quitting: bool,
    tray_enabled: Option<bool>,
    version: String,
    gestures: gestures::State,
    review: review::State,
    settings: settings::State,
    wizard: Option<wizard::Wizard>,
}

/// What the tabs get each frame: a snapshot of the shared state and a way
/// to send commands.
pub(crate) struct Ctx<'a> {
    pub shared: &'a Shared,
    pub commands: &'a Sender<Command>,
}

impl Ctx<'_> {
    pub fn send(&self, cmd: Command) {
        let _ = self.commands.send(cmd);
    }
}

impl App {
    fn snapshot(&self) -> Shared {
        self.shared.lock().map(|s| s.clone()).unwrap_or_default()
    }

    fn show_window(&mut self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
        if let Some(hwnd) = self.hwnd {
            (self.bring_to_front)(hwnd);
        }
        self.visible = true;
        self.ui.set_visible(true);
    }

    fn hide_window(&mut self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        self.visible = false;
        self.ui.set_visible(false);
    }

    fn config_dir(&self) -> PathBuf {
        let path = self
            .shared
            .lock()
            .map(|s| s.config_path.clone())
            .unwrap_or_default();
        path.parent().map(Path::to_path_buf).unwrap_or_default()
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if self.hwnd.is_none() {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            if let Ok(h) = frame.window_handle()
                && let RawWindowHandle::Win32(w) = h.as_raw()
            {
                self.hwnd = Some(w.hwnd.get());
            }
        }
        let actions = self.tray.as_mut().map(|t| t.poll()).unwrap_or_default();
        for action in actions {
            match action {
                TrayAction::Open => self.show_window(ctx),
                TrayAction::ToggleEnabled => {
                    let _ = self.commands.send(Command::ToggleEnabled);
                }
                TrayAction::Reload => {
                    let _ = self.commands.send(Command::Reload);
                }
                TrayAction::OpenConfigFolder => (self.open_folder)(&self.config_dir()),
                TrayAction::CheckUpdates => {
                    if let Ok(mut s) = self.shared.lock() {
                        s.status = Some("Update checks arrive in a later version (M6).".into());
                    }
                    self.show_window(ctx);
                }
                TrayAction::Quit => {
                    self.quitting = true;
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
            }
        }
        if self.ui.take_show_request() {
            self.show_window(ctx);
        }

        // SPEC §10.1: with close_to_tray the window's X hides it instead.
        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting {
            let close_to_tray = self
                .shared
                .lock()
                .map(|s| s.config.settings.close_to_tray)
                .unwrap_or(false);
            if close_to_tray && self.tray.is_some() {
                ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                self.hide_window(ctx);
            }
        }

        // The tray icon turns gray when paused, by hand or automatically.
        let enabled = self
            .shared
            .lock()
            .map(|s| s.enabled && s.auto_paused.is_none())
            .unwrap_or(true);
        if self.tray_enabled != Some(enabled) {
            if let Some(t) = self.tray.as_mut() {
                t.set_enabled(enabled);
            }
            self.tray_enabled = Some(enabled);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let shared = self.snapshot();
        let commands = self.commands.clone();
        let cx = Ctx {
            shared: &shared,
            commands: &commands,
        };

        egui::Panel::top("tabs").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("GlyphPad");
                ui.separator();
                for (tab, name) in [
                    (Tab::Gestures, "Gestures"),
                    (Tab::Review, "Review gestures"),
                    (Tab::Settings, "Settings"),
                    (Tab::Debug, "Live debug"),
                ] {
                    if ui.selectable_label(self.tab == tab, name).clicked() {
                        self.tab = tab;
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.weak(format!("v{}", self.version));
                    let (label, color) = match (shared.enabled, shared.mode) {
                        (_, glyphpad_core::service::EngineMode::Test) => {
                            ("TEST MODE", egui::Color32::from_rgb(240, 200, 80))
                        }
                        (false, _) => ("PAUSED", egui::Color32::GRAY),
                        (true, m) if m.is_recording() => {
                            ("RECORDING", egui::Color32::from_rgb(240, 110, 110))
                        }
                        (true, _) => ("ACTIVE", egui::Color32::from_rgb(120, 220, 140)),
                    };
                    if ui
                        .add(
                            egui::Button::new(egui::RichText::new(label).color(color)).frame(false),
                        )
                        .on_hover_text("Click to pause or resume all gestures")
                        .clicked()
                    {
                        cx.send(Command::ToggleEnabled);
                    }
                });
            });
        });

        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                match &shared.status {
                    Some(s) => {
                        ui.label(s);
                    }
                    None => {
                        ui.weak(format!("Configuration: {}", shared.config_path.display()));
                    }
                }
                if !shared.warnings.is_empty() {
                    ui.colored_label(
                        egui::Color32::from_rgb(240, 200, 80),
                        format!("⚠ {} warning(s)", shared.warnings.len()),
                    )
                    .on_hover_text(shared.warnings.join("\n"));
                }
            });
        });

        egui::CentralPanel::default().show(ui, |ui| match self.tab {
            Tab::Gestures => {
                if let Some(start) = gestures::show(ui, &cx, &mut self.gestures) {
                    self.wizard = Some(wizard::Wizard::new(start));
                }
            }
            Tab::Review => {
                if let Some(symbol) = review::show(ui, &cx, &mut self.review) {
                    self.wizard = Some(wizard::Wizard::new(wizard::Start::AddTemplates(symbol)));
                }
            }
            Tab::Settings => settings::show(ui, &cx, &mut self.settings),
            Tab::Debug => debug::show(ui, &cx),
        });

        if let Some(w) = &mut self.wizard {
            match w.show(ui.ctx(), &cx) {
                wizard::Outcome::Open => {}
                wizard::Outcome::Closed => self.wizard = None,
                wizard::Outcome::Test => {
                    self.wizard = None;
                    self.tab = Tab::Debug;
                }
            }
        }
    }
}
