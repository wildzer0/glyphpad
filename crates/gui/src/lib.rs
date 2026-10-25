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
mod i18n;
mod icon;
mod icons;
mod packs;
mod review;
mod settings;
mod theme;
mod widgets;
mod wizard;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crossbeam_channel::Sender;
use eframe::egui::ViewportCommand;
use glyphpad_core::service::{Command, EngineMode, Shared, UiLink};

pub use eframe::egui;
pub use icon::icon_rgba;

/// `text` in the language of the window, for what the application itself
/// shows (the tray menu).
pub fn tr(text: &'static str) -> &'static str {
    i18n::tr(text)
}

/// What the tray menu asks for (SPEC §10.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayAction {
    Open,
    ToggleEnabled,
    Reload,
    OpenConfigFolder,
    CheckUpdates,
    Quit,
    SwitchProfile(String),
}

/// The system tray icon, implemented by the application.
pub trait Tray {
    /// Actions requested since the last call.
    fn poll(&mut self) -> Vec<TrayAction>;
    /// Shows whether gestures are enabled or paused.
    fn set_enabled(&mut self, enabled: bool);
    /// The profiles to choose from and the one in use.
    fn set_profiles(&mut self, names: &[String], active: &str);
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
    /// Opens a web page in the default browser.
    pub open_url: Box<dyn Fn(&str)>,
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
        open_url,
        bring_to_front,
    } = host;
    eframe::run_native(
        "GlyphPad",
        native,
        Box::new(move |cc| {
            let snapshot = shared.lock().map(|s| s.clone()).unwrap_or_default();
            let theme_choice = snapshot.config.settings.theme;
            theme::install(&cc.egui_ctx, theme_choice);
            i18n::set(snapshot.config.settings.language);
            let ctx = cc.egui_ctx.clone();
            ui.set_repaint(move || ctx.request_repaint());
            ui.set_visible(!options.start_hidden);
            let tray = tray(cc.egui_ctx.clone());
            let editor = editor::Editor::new(&snapshot);
            Ok(Box::new(App {
                icon: None,
                editor,
                applied_theme: theme_choice,
                applied_language: snapshot.config.settings.language,
                pending_wizard: None,
                shared,
                commands,
                ui,
                tray,
                open_folder,
                open_url,
                dismissed_failure: None,
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
    applied_theme: glyphpad_core::config::ThemeChoice,
    applied_language: glyphpad_core::config::Language,
    /// A wizard asked for while the draft has unsaved changes: it starts
    /// once they are saved, since the wizard writes to the same file.
    pending_wizard: Option<wizard::Start>,
    shared: Arc<Mutex<Shared>>,
    commands: Sender<Command>,
    ui: Arc<UiLink>,
    tray: Option<Box<dyn Tray>>,
    open_folder: Box<dyn Fn(&Path)>,
    open_url: Box<dyn Fn(&str)>,
    /// The update failure the user closed, so it is not shown again.
    dismissed_failure: Option<String>,
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
    pub version: &'a str,
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
                    let _ = self.commands.send(Command::CheckUpdates { manual: true });
                    self.show_window(ctx);
                }
                TrayAction::SwitchProfile(name) => {
                    let _ = self.commands.send(Command::SwitchProfile(Some(name)));
                }
                TrayAction::Quit => {
                    self.quitting = true;
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
            }
        }
        // An update took over: this process has nothing left to do.
        if self.ui.take_quit_request() {
            self.quitting = true;
            ctx.send_viewport_cmd(ViewportCommand::Close);
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
        if let Some(t) = self.tray.as_mut()
            && let Ok(s) = self.shared.lock()
        {
            t.set_profiles(&s.config.profile_names(), &s.profile);
        }
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
        let version = self.version.clone();
        let cx = Ctx {
            shared: &shared,
            commands: &commands,
            version: &version,
        };
        // The editor follows the file and saves the draft once it settles.
        if let Some(cmd) = self.editor.frame(ui.ctx(), &shared) {
            cx.send(cmd);
        }
        let language = self.editor.draft.settings.language;
        if language != self.applied_language {
            i18n::set(language);
            self.applied_language = language;
        }
        let choice = self.editor.draft.settings.theme;
        if choice != self.applied_theme {
            theme::apply_choice(ui.ctx(), choice);
            self.applied_theme = choice;
        }

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

        let p = theme::pal_of(ui.ctx());
        egui::Panel::left("nav")
            .resizable(false)
            .exact_size(theme::SIDEBAR_WIDTH)
            .frame(
                egui::Frame::new()
                    .fill(p.sidebar)
                    .inner_margin(egui::Margin::symmetric(12, 16)),
            )
            .show_separator_line(false)
            .show(ui, |ui| self.navigation(ui, &cx));
        // A hairline between the sidebar and the content.
        let nav_right = ui.max_rect().left() + theme::SIDEBAR_WIDTH;
        ui.painter().line_segment(
            [
                egui::pos2(nav_right, ui.max_rect().top()),
                egui::pos2(nav_right, ui.max_rect().bottom()),
            ],
            egui::Stroke::new(1.0, p.border),
        );
        egui::Panel::bottom("status")
            .frame(
                egui::Frame::new()
                    .fill(p.bg)
                    .inner_margin(egui::Margin::symmetric(theme::space::XL as i8, 6)),
            )
            .show_separator_line(false)
            .show(ui, |ui| self.status_bar(ui, &cx));
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(p.bg).inner_margin(egui::Margin {
                left: theme::space::XL as i8,
                right: theme::space::XL as i8,
                top: theme::space::XL as i8,
                bottom: 0,
            }))
            .show(ui, |ui| {
                self.update_banner(ui, &cx);
                self.save_banner(ui, &cx);
                if let Some(wz) = &mut self.wizard {
                    match wz.show(ui, &cx) {
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
        self.dialogs(ui.ctx(), &cx);
    }
}

impl App {
    fn start_wizard(&mut self, start: wizard::Start) {
        if self.editor.dirty() {
            // The wizard writes to the file: edits still waiting go first.
            self.editor.flush();
            self.pending_wizard = Some(start);
        } else {
            self.wizard = Some(wizard::Wizard::new(start));
        }
    }

    fn leave_wizard(&mut self, cx: &Ctx) {
        if self.wizard.take().is_some() && cx.shared.mode.is_recording() {
            cx.send(Command::SetMode(EngineMode::Normal));
        }
    }

    fn navigation(&mut self, ui: &mut egui::Ui, cx: &Ctx) {
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            let paused = !cx.shared.enabled;
            if self.icon.as_ref().is_none_or(|(pp, _)| *pp != paused) {
                let tex = ui.ctx().load_texture(
                    "app-icon",
                    egui::ColorImage::from_rgba_unmultiplied([64, 64], &icon_rgba(64, paused)),
                    egui::TextureOptions::LINEAR,
                );
                self.icon = Some((paused, tex));
            }
            if let Some((_, tex)) = &self.icon {
                ui.image((tex.id(), egui::vec2(32.0, 32.0)));
            }
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(egui::RichText::new("GlyphPad").font(theme::fonts::strong(17.0)));
                ui.label(widgets::faint_raw(ui, format!("v{}", self.version)));
            });
        });
        ui.add_space(theme::space::XL);
        for (tab, icon, name) in [
            (Tab::Gestures, icons::HAND_TAP, "Gestures"),
            (Tab::Review, icons::SCRIBBLE_LOOP, "Review"),
            (Tab::Settings, icons::GEAR_SIX, "Settings"),
            (Tab::Debug, icons::PULSE, "Live debug"),
        ] {
            let selected = self.tab == tab && self.wizard.is_none();
            if widgets::nav_item(ui, icon, name, selected).clicked() {
                self.leave_wizard(cx);
                self.tab = tab;
            }
            ui.add_space(2.0);
        }
        // The card pinned to the bottom of the sidebar: a nested panel
        // sizes itself to its content, unlike a bottom-up layout.
        egui::Panel::bottom("nav-status")
            .frame(egui::Frame::new())
            .show_separator_line(false)
            .show(ui, |ui| self.status_card(ui, cx));
    }

    /// Whether gestures run, and the two switches that change it.
    fn status_card(&mut self, ui: &mut egui::Ui, cx: &Ctx) {
        let p = theme::pal(ui);
        let s = cx.shared;
        let (label, tone) = match (s.enabled, s.mode, &s.auto_paused) {
            (_, EngineMode::Test, _) => ("Test mode", widgets::Tone::Warn),
            (false, _, _) => ("Paused", widgets::Tone::Neutral),
            (true, m, _) if m.is_recording() => ("Recording", widgets::Tone::Bad),
            (true, _, Some(_)) => ("Paused for this app", widgets::Tone::Neutral),
            (true, _, None) => ("Active", widgets::Tone::Ok),
        };
        egui::Frame::new()
            .fill(p.surface)
            .stroke(egui::Stroke::new(1.0, p.border))
            .corner_radius(egui::CornerRadius::same(theme::radius::CARD))
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let (r, _) =
                        ui.allocate_exact_size(egui::vec2(12.0, 18.0), egui::Sense::hover());
                    ui.painter().circle_filled(
                        r.center(),
                        5.0,
                        match tone {
                            widgets::Tone::Ok => p.ok,
                            widgets::Tone::Warn => p.warn,
                            widgets::Tone::Bad => p.bad,
                            _ => p.text_faint,
                        },
                    );
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.label(widgets::strong(label));
                        if let Some(why) = &s.auto_paused {
                            ui.add(egui::Label::new(widgets::faint_raw(ui, why)).truncate());
                        }
                    });
                });
                let names = cx.shared.config.profile_names();
                if names.len() > 1 {
                    ui.add_space(theme::space::XS);
                    ui.horizontal(|ui| {
                        ui.label(widgets::dim(ui, "Profile"));
                        let mut chosen = s.profile.clone();
                        let options: Vec<(String, std::borrow::Cow<'static, str>)> = names
                            .iter()
                            .map(|p| (p.clone(), p.clone().into()))
                            .collect();
                        if widgets::dropdown(ui, "sidebar-profile", &mut chosen, &options, 110.0)
                            .changed()
                        {
                            cx.send(Command::SwitchProfile(Some(chosen)));
                        }
                    });
                }
                ui.add_space(theme::space::XS);
                let mut enabled = s.enabled;
                ui.horizontal(|ui| {
                    if widgets::switch(ui, &mut enabled)
                        .on_hover_text(crate::i18n::tr("Pause or resume every gesture"))
                        .changed()
                    {
                        cx.send(Command::ToggleEnabled);
                    }
                    ui.label(widgets::dim(ui, "Gestures"));
                });
                let mut test = s.mode == EngineMode::Test;
                ui.horizontal(|ui| {
                    if widgets::switch(ui, &mut test)
                        .on_hover_text(crate::i18n::tr(
                            "Recognize gestures without running their actions",
                        ))
                        .changed()
                    {
                        cx.send(Command::SetMode(if test {
                            EngineMode::Test
                        } else {
                            EngineMode::Normal
                        }));
                    }
                    ui.label(widgets::dim(ui, "Test mode"));
                });
            });
    }

    /// The bottom bar: where the configuration lives, and whether what you
    /// changed is saved.
    fn status_bar(&mut self, ui: &mut egui::Ui, cx: &Ctx) {
        let p = theme::pal(ui);
        ui.horizontal(|ui| {
            let text = cx
                .shared
                .status
                .clone()
                .unwrap_or_else(|| cx.shared.config_path.display().to_string());
            ui.add(egui::Label::new(widgets::faint_raw(ui, text)).truncate());
            if !cx.shared.warnings.is_empty() {
                widgets::badge_raw(
                    ui,
                    &i18n::plural(cx.shared.warnings.len(), "{} warning", "{} warnings"),
                    widgets::Tone::Warn,
                )
                .on_hover_text(cx.shared.warnings.join("\n"));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                use editor::SaveState as S;
                let (icon, text, color) = match self.editor.state() {
                    S::Saved => (icons::CHECK_CIRCLE, "All changes saved", p.ok),
                    S::Waiting | S::Saving => (icons::CIRCLE_NOTCH, "Saving…", p.text_dim),
                    S::Invalid(_) => (icons::WARNING, "Not saved: needs attention", p.warn),
                    S::Conflict => (icons::WARNING, "Not saved: file changed", p.warn),
                    S::Failed(_) => (icons::WARNING_CIRCLE, "Could not save", p.bad),
                };
                ui.label(
                    egui::RichText::new(format!("{icon}  {}", i18n::tr(text)))
                        .size(theme::size::SMALL)
                        .color(color),
                );
                ui.add_space(theme::space::S);
                if widgets::icon_button(ui, icons::ARROW_CLOCKWISE, "Redo (Ctrl+Y)").clicked()
                    && self.editor.can_redo()
                {
                    self.editor.redo();
                }
                if widgets::icon_button(ui, icons::ARROW_COUNTER_CLOCKWISE, "Undo (Ctrl+Z)")
                    .clicked()
                    && self.editor.can_undo()
                {
                    self.editor.undo();
                }
            });
        });
    }

    /// Offers a new version, or says what the update is doing (SPEC §12.1).
    fn update_banner(&mut self, ui: &mut egui::Ui, cx: &Ctx) {
        use glyphpad_core::service::UpdateStatus as U;
        let notes = |text: &str| {
            let lines: Vec<&str> = text
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .collect();
            let mut out = lines.iter().take(6).copied().collect::<Vec<_>>().join("\n");
            if lines.len() > 6 {
                out.push_str("\n…");
            }
            out
        };
        match cx.shared.update.clone() {
            U::Available(u) => {
                let heading = i18n::tf(
                    if u.prerelease {
                        "Version {} is available, a pre-release (you have {})"
                    } else {
                        "Version {} is available (you have {})"
                    },
                    &[&u.version, cx.version],
                );
                widgets::banner_raw(
                    ui,
                    widgets::Tone::Accent,
                    &heading,
                    &notes(&u.notes),
                    |ui| {
                        if widgets::primary_button(
                            ui,
                            icons::DOWNLOAD_SIMPLE,
                            "Install and restart",
                        )
                        .clicked()
                        {
                            cx.send(Command::InstallUpdate);
                        }
                        if widgets::button(ui, "", "Ignore this version").clicked() {
                            cx.send(Command::IgnoreUpdate(u.version.clone()));
                        }
                        if widgets::button(ui, icons::ARROW_SQUARE_OUT, "Release notes").clicked() {
                            (self.open_url)(&u.url);
                        }
                    },
                );
            }
            U::Downloading(v) => widgets::banner_raw(
                ui,
                widgets::Tone::Accent,
                &i18n::tf("Downloading version {}…", &[&v]),
                i18n::tr(
                    "GlyphPad keeps working; it restarts by itself when the download is verified.",
                ),
                |_| {},
            ),
            U::Installing(v) | U::Restarting(v) => widgets::banner_raw(
                ui,
                widgets::Tone::Accent,
                &i18n::tf("Starting version {}…", &[&v]),
                i18n::tr("If it does not start, the current version is restored."),
                |_| {},
            ),
            U::Manual(u, why) => widgets::banner_raw(
                ui,
                widgets::Tone::Warn,
                &i18n::tf(
                    "Version {} is available, but GlyphPad cannot update itself here",
                    &[&u.version],
                ),
                &format!(
                    "{why}\n{}",
                    i18n::tr("Download it from the release page and replace this program.")
                ),
                |ui| {
                    if widgets::primary_button(ui, icons::ARROW_SQUARE_OUT, "Open the release page")
                        .clicked()
                    {
                        (self.open_url)(&u.url);
                    }
                },
            ),
            U::Failed(why) if self.dismissed_failure.as_deref() != Some(&why) => {
                widgets::banner_raw(
                    ui,
                    widgets::Tone::Bad,
                    i18n::tr("The update did not work"),
                    &why,
                    |ui| {
                        if widgets::button(ui, icons::X, "Dismiss").clicked() {
                            self.dismissed_failure = Some(why.clone());
                        }
                    },
                );
            }
            U::Failed(_) | U::Idle | U::Checking | U::UpToDate => {}
        }
    }

    /// Says what needs attention about saving, above the page.
    fn save_banner(&mut self, ui: &mut egui::Ui, cx: &Ctx) {
        use editor::SaveState as S;
        match self.editor.state().clone() {
            S::Invalid(why) => widgets::banner_raw(
                ui,
                widgets::Tone::Warn,
                i18n::tr("These changes are not saved yet"),
                &why,
                |_| {},
            ),
            S::Failed(why) => widgets::banner_raw(
                ui,
                widgets::Tone::Bad,
                i18n::tr("The configuration could not be saved"),
                &why,
                |ui| {
                    if widgets::button(ui, icons::ARROWS_CLOCKWISE, "Try again").clicked() {
                        self.editor.retry();
                    }
                },
            ),
            S::Conflict => widgets::banner(
                ui,
                widgets::Tone::Warn,
                "The configuration changed on disk",
                "It was edited outside GlyphPad while you were making changes here.",
                |ui| {
                    for (choice, icon, label) in [
                        (
                            editor::ConflictChoice::Reload,
                            icons::DOWNLOAD_SIMPLE,
                            "Load the file",
                        ),
                        (
                            editor::ConflictChoice::Overwrite,
                            icons::FLOPPY_DISK,
                            "Keep my changes",
                        ),
                        (
                            editor::ConflictChoice::ShowDiff,
                            icons::EYE,
                            "Show differences",
                        ),
                    ] {
                        if widgets::button(ui, icon, label).clicked()
                            && let Some(cmd) = self.editor.resolve_conflict(choice, cx.shared)
                        {
                            cx.send(cmd);
                        }
                    }
                },
            ),
            S::Saved | S::Waiting | S::Saving => {}
        }
    }

    fn dialogs(&mut self, ctx: &egui::Context, cx: &Ctx) {
        let p = theme::pal_of(ctx);
        if self.editor.show_diff {
            let diff = self.editor.diff(cx.shared);
            let mut close = false;
            widgets::dialog(
                ctx,
                "diff",
                "File on disk (−) and your changes (+)",
                720.0,
                |ui| {
                    egui::ScrollArea::both().max_height(420.0).show(ui, |ui| {
                        for (mark, line) in diff.iter().filter(|(m, _)| *m != ' ') {
                            let color = if *mark == '+' { p.ok } else { p.bad };
                            ui.label(
                                egui::RichText::new(format!("{mark} {line}"))
                                    .font(theme::fonts::mono(12.5))
                                    .color(color),
                            );
                        }
                    });
                    ui.add_space(theme::space::M);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if widgets::button(ui, "", "Close").clicked() {
                            close = true;
                        }
                    });
                },
            );
            if close {
                self.editor.show_diff = false;
            }
        }
        if let Some(start) = self.pending_wizard.take() {
            use editor::SaveState as S;
            match self.editor.state() {
                S::Saved => self.wizard = Some(wizard::Wizard::new(start)),
                S::Waiting | S::Saving => {
                    self.editor.flush();
                    self.pending_wizard = Some(start);
                    ctx.request_repaint_after(std::time::Duration::from_millis(50));
                }
                _ => {
                    // Something stops the draft from being saved: the
                    // banner says what; the wizard waits for the user.
                    let mut cancel = false;
                    widgets::dialog(
                        ctx,
                        "wizard-unsaved",
                        "Finish your changes first",
                        400.0,
                        |ui| {
                            ui.label(
                            "The new gesture is written to the same file as your pending changes, which cannot be saved yet. Fix them (see the message at the top), or undo them.",
                        );
                            ui.add_space(theme::space::M);
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if widgets::button(ui, "", "Cancel").clicked() {
                                        cancel = true;
                                    }
                                },
                            );
                        },
                    );
                    if !cancel {
                        self.pending_wizard = Some(start);
                    }
                }
            }
        }
    }
}
