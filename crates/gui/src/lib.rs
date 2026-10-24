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
mod editor;
mod forms;
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
            .with_inner_size([1180.0, 780.0])
            .with_min_inner_size([860.0, 560.0])
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
            let editor = editor::Editor::new(&shared.lock().map(|s| s.clone()).unwrap_or_default());
            Ok(Box::new(App {
                icon: None,
                editor,
                pending_wizard: None,
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
    /// The icon in the navigation bar, and whether it shows "paused".
    icon: Option<(bool, egui::TextureHandle)>,
    editor: editor::Editor,
    /// A wizard asked for while the draft has unsaved changes.
    pending_wizard: Option<wizard::Start>,
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
        self.editor.sync(&shared);

        // Undo / redo of the draft, when not typing (text fields have their own).
        if !ui.ctx().egui_wants_keyboard_input() {
            let (undo, redo) = ui.input_mut(|i| {
                (
                    i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z),
                    i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y)
                        || i.consume_key(
                            egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                            egui::Key::Z,
                        ),
                )
            });
            if undo {
                self.editor.undo();
            }
            if redo {
                self.editor.redo();
            }
        }

        egui::Panel::left("nav")
            .resizable(false)
            .exact_size(200.0)
            .show(ui, |ui| self.navigation(ui, &cx));
        egui::Panel::bottom("status").show(ui, |ui| status_bar(ui, &cx));
        if self.editor.dirty() || self.editor.external_change {
            egui::Panel::top("savebar").show(ui, |ui| self.save_bar(ui, &cx));
        }
        egui::CentralPanel::default().show(ui, |ui| {
            if let Some(w) = &mut self.wizard {
                match w.show(ui, &cx) {
                    wizard::Outcome::Open => {}
                    wizard::Outcome::Closed => self.wizard = None,
                    wizard::Outcome::Test => {
                        self.wizard = None;
                        self.tab = Tab::Debug;
                    }
                }
                return;
            }
            match self.tab {
                Tab::Gestures => {
                    if let Some(start) =
                        gestures::show(ui, &cx, &mut self.editor, &mut self.gestures)
                    {
                        self.start_wizard(start);
                    }
                }
                Tab::Review => {
                    if let Some(symbol) = review::show(ui, &cx, &mut self.review) {
                        self.start_wizard(wizard::Start::AddTemplates(symbol));
                    }
                }
                Tab::Settings => settings::show(ui, &cx, &mut self.editor, &mut self.settings),
                Tab::Debug => debug::show(ui, &cx),
            }
        });
        self.editor.track(ui.ctx());
        self.dialogs(ui.ctx(), &cx);
    }
}

impl App {
    fn start_wizard(&mut self, start: wizard::Start) {
        if self.editor.dirty() {
            // The wizard saves to the file: unsaved edits must go first.
            self.pending_wizard = Some(start);
        } else {
            self.wizard = Some(wizard::Wizard::new(start));
        }
    }

    fn navigation(&mut self, ui: &mut egui::Ui, cx: &Ctx) {
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            let paused = !cx.shared.enabled;
            if self.icon.as_ref().is_none_or(|(p, _)| *p != paused) {
                let tex = ui.ctx().load_texture(
                    "app-icon",
                    egui::ColorImage::from_rgba_unmultiplied([32, 32], &icon_rgba(32, paused)),
                    egui::TextureOptions::LINEAR,
                );
                self.icon = Some((paused, tex));
            }
            if let Some((_, tex)) = &self.icon {
                ui.image((tex.id(), egui::vec2(28.0, 28.0)));
            }
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("GlyphPad").size(18.0).strong());
                ui.weak(format!("v{}", self.version));
            });
        });
        ui.add_space(14.0);
        for (tab, icon, name) in [
            (Tab::Gestures, "✋", "Gestures"),
            (Tab::Review, "▶", "Review gestures"),
            (Tab::Settings, "⚙", "Settings"),
            (Tab::Debug, "🔍", "Live debug"),
        ] {
            let selected = self.tab == tab && self.wizard.is_none();
            let text = egui::RichText::new(format!("{icon}   {name}")).size(15.0);
            if ui
                .add_sized([180.0, 34.0], egui::Button::selectable(selected, text))
                .clicked()
            {
                self.tab = tab;
            }
        }
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            ui.add_space(8.0);
            let s = cx.shared;
            let (label, color) = match (s.enabled, s.mode, &s.auto_paused) {
                (_, glyphpad_core::service::EngineMode::Test, _) => ("Test mode", style::WARN),
                (false, _, _) => ("Paused", egui::Color32::GRAY),
                (true, m, _) if m.is_recording() => ("Recording", style::BAD),
                (true, _, Some(_)) => ("Paused for this app", egui::Color32::GRAY),
                (true, _, None) => ("Active", style::OK),
            };
            let button = egui::Button::new(egui::RichText::new(format!("●  {label}")).color(color));
            if ui
                .add_sized([180.0, 30.0], button)
                .on_hover_text(
                    s.auto_paused
                        .clone()
                        .unwrap_or_else(|| "Click to pause or resume all gestures".into()),
                )
                .clicked()
            {
                cx.send(Command::ToggleEnabled);
            }
        });
    }

    fn save_bar(&mut self, ui: &mut egui::Ui, cx: &Ctx) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let valid = self.editor.check();
            ui.colored_label(
                style::WARN,
                egui::RichText::new("●  Unsaved changes").strong(),
            );
            if ui
                .add_enabled(
                    valid.is_ok(),
                    egui::Button::new(egui::RichText::new("💾 Save").strong()),
                )
                .clicked()
                && let Some(cmd) = self.editor.save(cx.shared)
            {
                cx.send(cmd);
            }
            if ui.button("Discard").clicked() {
                self.editor.discard(cx.shared);
            }
            ui.separator();
            if ui
                .add_enabled(self.editor.can_undo(), egui::Button::new("↶ Undo"))
                .on_hover_text("Ctrl+Z")
                .clicked()
            {
                self.editor.undo();
            }
            if ui
                .add_enabled(self.editor.can_redo(), egui::Button::new("↷ Redo"))
                .on_hover_text("Ctrl+Y")
                .clicked()
            {
                self.editor.redo();
            }
            if let Err(e) = valid {
                ui.separator();
                ui.colored_label(style::BAD, e.lines().next().unwrap_or_default())
                    .on_hover_text(e);
            }
        });
        ui.add_space(4.0);
    }

    fn dialogs(&mut self, ctx: &egui::Context, cx: &Ctx) {
        if self.editor.external_change && !self.editor.show_diff {
            egui::Modal::new(egui::Id::unique("external-change")).show(ctx, |ui| {
                ui.set_width(440.0);
                ui.heading("The configuration changed on disk");
                ui.label("It was edited outside GlyphPad while you had unsaved changes here.");
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    for (choice, label) in [
                        (editor::ConflictChoice::Reload, "Reload from the file"),
                        (
                            editor::ConflictChoice::Overwrite,
                            "Overwrite it with my changes",
                        ),
                        (editor::ConflictChoice::ShowDiff, "Show the differences"),
                    ] {
                        if ui.button(label).clicked()
                            && let Some(cmd) = self.editor.resolve_conflict(choice, cx.shared)
                        {
                            cx.send(cmd);
                        }
                    }
                });
            });
        }
        if self.editor.show_diff {
            let diff = self.editor.diff(cx.shared);
            egui::Modal::new(egui::Id::unique("diff")).show(ctx, |ui| {
                ui.set_width(720.0);
                ui.heading("File on disk (−) and your changes (+)");
                egui::ScrollArea::both().max_height(460.0).show(ui, |ui| {
                    for (mark, line) in diff.iter().filter(|(m, _)| *m != ' ') {
                        let color = if *mark == '+' { style::OK } else { style::BAD };
                        ui.label(
                            egui::RichText::new(format!("{mark} {line}"))
                                .monospace()
                                .color(color),
                        );
                    }
                });
                if ui.button("Close").clicked() {
                    self.editor.show_diff = false;
                }
            });
        }
        if let Some(start) = self.pending_wizard.take() {
            let mut keep = Some(start);
            egui::Modal::new(egui::Id::unique("wizard-unsaved")).show(ctx, |ui| {
                ui.set_width(380.0);
                ui.heading("Unsaved changes");
                ui.label("The new gesture is written to the configuration: save or discard your changes first.");
                ui.horizontal(|ui| {
                    if ui.button("Save and continue").clicked()
                        && let Some(cmd) = self.editor.save(cx.shared)
                    {
                        cx.send(cmd);
                        self.wizard = keep.take().map(wizard::Wizard::new);
                    }
                    if ui.button("Discard and continue").clicked() {
                        self.editor.discard(cx.shared);
                        self.wizard = keep.take().map(wizard::Wizard::new);
                    }
                    if ui.button("Cancel").clicked() {
                        keep = None;
                    }
                });
            });
            self.pending_wizard = keep;
        }
    }
}

fn status_bar(ui: &mut egui::Ui, cx: &Ctx) {
    ui.horizontal(|ui| {
        match &cx.shared.status {
            Some(s) => {
                ui.label(s);
            }
            None => {
                ui.weak(format!(
                    "Configuration: {}",
                    cx.shared.config_path.display()
                ));
            }
        }
        if !cx.shared.warnings.is_empty() {
            ui.colored_label(
                style::WARN,
                format!("⚠ {} warning(s)", cx.shared.warnings.len()),
            )
            .on_hover_text(cx.shared.warnings.join("\n"));
        }
    });
}
