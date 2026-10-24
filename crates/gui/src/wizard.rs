//! The gesture recording wizard (SPEC §10.3): kind, recording with live
//! feedback, samples to review or discard, quality check, name and action,
//! save, then an offer to try it in test mode.

use eframe::egui::{self, Color32, Stroke};
use glyphpad_core::action::{Action, MediaKey};
use glyphpad_core::config::{GestureDef, GestureKind, OneOrMany, When};
use glyphpad_core::config_edit;
use glyphpad_core::infer::{self, Inferred};
use glyphpad_core::service::{Command, EngineMode, Recorded};
use glyphpad_core::symbols::{self, Sample};

use crate::Ctx;
use crate::canvas::{self, Pad, Playback};

pub enum Start {
    New,
    /// Record more templates for an existing symbol.
    AddTemplates(String),
}

pub enum Outcome {
    Open,
    Closed,
    /// Saved; go try it in test mode.
    Test,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Symbol,
    Demonstration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Kind,
    Record,
    Quality,
    Details,
    Done,
}

pub struct Wizard {
    step: Step,
    kind: Kind,
    /// Only add templates, no new gesture.
    templates_only: bool,
    symbol: String,
    target: usize,
    playback: Playback,
    /// Frozen at the end of recording.
    samples: Vec<Sample>,
    demos: Vec<glyphpad_core::engine::Observation>,
    name: String,
    action: ActionDraft,
    confidence: Option<f32>,
    apps: String,
    saved: bool,
}

impl Wizard {
    pub fn new(start: Start) -> Self {
        let (symbol, templates_only, step) = match start {
            Start::New => (String::new(), false, Step::Kind),
            Start::AddTemplates(s) => (s, true, Step::Kind),
        };
        Self {
            step,
            kind: Kind::Symbol,
            templates_only,
            symbol,
            target: 5,
            playback: Playback::default(),
            samples: Vec::new(),
            demos: Vec::new(),
            name: String::new(),
            action: ActionDraft::default(),
            confidence: None,
            apps: String::new(),
            saved: false,
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, cx: &Ctx) -> Outcome {
        let mut open = true;
        let mut outcome = Outcome::Open;
        let title = if self.templates_only {
            format!("Record templates of “{}”", self.symbol)
        } else {
            "New gesture".to_string()
        };
        egui::Window::new(title)
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size([720.0, 560.0])
            .show(ctx, |ui| {
                outcome = match self.step {
                    Step::Kind => self.kind_step(ui, cx),
                    Step::Record => self.record_step(ui, cx),
                    Step::Quality => self.quality_step(ui, cx),
                    Step::Details => self.details_step(ui, cx),
                    Step::Done => self.done_step(ui, cx),
                };
            });
        if !open {
            outcome = Outcome::Closed;
        }
        if matches!(outcome, Outcome::Closed) && cx.shared.mode.is_recording() {
            cx.send(Command::SetMode(EngineMode::Normal));
        }
        outcome
    }

    fn kind_step(&mut self, ui: &mut egui::Ui, cx: &Ctx) -> Outcome {
        if !self.templates_only {
            ui.label("What kind of gesture?");
            ui.radio_value(
                &mut self.kind,
                Kind::Symbol,
                "A symbol drawn on the touchpad",
            );
            ui.radio_value(
                &mut self.kind,
                Kind::Demonstration,
                "A tap, hold or swipe, shown by doing it",
            );
            ui.add_space(8.0);
        }
        if self.kind == Kind::Symbol {
            ui.horizontal(|ui| {
                ui.label("Symbol name");
                ui.add_enabled(
                    !self.templates_only,
                    egui::TextEdit::singleline(&mut self.symbol).hint_text("e.g. L, q, star"),
                );
            });
            ui.add(egui::Slider::new(&mut self.target, 3..=10).text("examples to draw"));
            ui.weak(format!(
                "Draw it the way you will use it: {}. An example ends when you pause.",
                describe_draw_mode(cx)
            ));
        } else {
            ui.add(egui::Slider::new(&mut self.target, 2..=10).text("demonstrations"));
        }
        ui.add_space(8.0);
        let ready = self.kind == Kind::Demonstration || !self.symbol.trim().is_empty();
        if ui
            .add_enabled(ready, egui::Button::new("Start recording →"))
            .clicked()
        {
            self.symbol = self.symbol.trim().to_string();
            cx.send(Command::SetMode(match self.kind {
                Kind::Symbol => EngineMode::RecordSymbol,
                Kind::Demonstration => EngineMode::RecordDemonstration,
            }));
            self.step = Step::Record;
        }
        Outcome::Open
    }

    fn record_step(&mut self, ui: &mut egui::Ui, cx: &Ctx) -> Outcome {
        let s = cx.shared;
        let n = s.recorded.len();
        ui.horizontal(|ui| {
            ui.colored_label(Color32::from_rgb(240, 110, 110), "● Recording");
            ui.label(format!("{n} of {}", self.target));
            ui.weak("Gestures and actions are suspended meanwhile.");
        });
        let (w_mm, h_mm) = s.pad_size_mm();
        let (_, painter, pad) = Pad::new(ui, ui.available_width().min(360.0), w_mm, h_mm);
        canvas::draw_polylines(
            &painter,
            &pad,
            &s.drawing,
            Stroke::new(3.0, Color32::from_rgb(90, 170, 255)),
        );
        canvas::draw_contacts(&painter, &pad, &s.contacts);

        let total = s
            .recorded
            .iter()
            .map(|r| match r {
                Recorded::Sample(x) => canvas::duration_us(&x.strokes),
                Recorded::Demonstration { trace, .. } => {
                    canvas::duration_us(trace.fingers.values())
                }
            })
            .max()
            .unwrap_or(0);
        self.playback.advance(ui.ctx(), total);
        ui.add_space(6.0);
        egui::ScrollArea::horizontal().show(ui, |ui| {
            ui.horizontal(|ui| {
                for (i, r) in s.recorded.iter().enumerate() {
                    ui.vertical(|ui| {
                        let (_, painter, pad) = Pad::new(ui, 140.0, w_mm, h_mm);
                        match r {
                            Recorded::Sample(x) => canvas::draw_strokes(
                                &painter,
                                &pad,
                                &x.strokes,
                                Some(self.playback.t_us),
                                true,
                            ),
                            Recorded::Demonstration { trace, observation } => {
                                canvas::draw_fingers(
                                    &painter,
                                    &pad,
                                    &trace.fingers,
                                    Some(self.playback.t_us),
                                );
                                ui.small(observation.to_string());
                            }
                        }
                        if ui
                            .small_button("Discard")
                            .on_hover_text("Drop it and record another")
                            .clicked()
                        {
                            cx.send(Command::DiscardRecorded(i));
                        }
                    });
                }
            });
        });

        let min = if self.kind == Kind::Symbol { 3 } else { 1 };
        ui.horizontal(|ui| {
            if ui.button("Cancel").clicked() {
                cx.send(Command::SetMode(EngineMode::Normal));
                return Outcome::Closed;
            }
            if ui
                .add_enabled(n >= min, egui::Button::new("Next →"))
                .on_disabled_hover_text(format!("Record at least {min}"))
                .clicked()
                || n >= self.target
            {
                self.samples.clear();
                self.demos.clear();
                for r in &s.recorded {
                    match r {
                        Recorded::Sample(x) => self.samples.push(x.clone()),
                        Recorded::Demonstration { observation, .. } => {
                            self.demos.push(observation.clone())
                        }
                    }
                }
                cx.send(Command::SetMode(EngineMode::Normal));
                self.step = Step::Quality;
            }
            Outcome::Open
        })
        .inner
    }

    fn quality_step(&mut self, ui: &mut egui::Ui, cx: &Ctx) -> Outcome {
        ui.heading("Quality check");
        match self.kind {
            Kind::Symbol => {
                let a = symbols::assess(&self.symbol, &self.samples, &cx.shared.effective);
                let mean = a.consistency.iter().sum::<f32>() / a.consistency.len().max(1) as f32;
                let total = canvas::duration_us(self.samples.iter().flat_map(|s| &s.strokes));
                self.playback.advance(ui.ctx(), total);
                let mut drop = None;
                egui::Grid::new("quality").striped(true).show(ui, |ui| {
                    for (i, (sample, (c, near))) in self
                        .samples
                        .iter()
                        .zip(a.consistency.iter().zip(&a.nearest_other))
                        .enumerate()
                    {
                        let (_, painter, pad) =
                            Pad::new(ui, 120.0, sample.width_mm, sample.height_mm);
                        canvas::draw_strokes(
                            &painter,
                            &pad,
                            &sample.strokes,
                            Some(self.playback.t_us),
                            true,
                        );
                        ui.vertical(|ui| {
                            let odd = self.samples.len() > 2 && *c < mean - 0.1;
                            let text = format!("consistency {c:.2}");
                            if odd {
                                ui.colored_label(
                                    Color32::from_rgb(240, 160, 80),
                                    format!("{text} — unlike the others"),
                                );
                            } else {
                                ui.label(text);
                            }
                            if let Some(n) = near {
                                let warn = n.score >= cx.shared.config.settings.default_confidence;
                                let t =
                                    format!("closest other symbol: {} {:.2}", n.symbol, n.score);
                                if warn {
                                    ui.colored_label(
                                        Color32::from_rgb(240, 110, 110),
                                        format!("{t} — may be confused"),
                                    );
                                } else {
                                    ui.weak(t);
                                }
                            }
                            if ui.small_button("Discard").clicked() {
                                drop = Some(i);
                            }
                        });
                        // Side by side with the symbol it resembles.
                        if let Some(n) = near
                            .as_ref()
                            .filter(|n| n.score >= cx.shared.config.settings.default_confidence)
                            && let Some(other) =
                                cx.shared.effective.get(&n.symbol).and_then(|s| s.first())
                        {
                            let (_, painter, pad) =
                                Pad::new(ui, 120.0, other.width_mm, other.height_mm);
                            canvas::draw_strokes(&painter, &pad, &other.strokes, None, true);
                        }
                        ui.end_row();
                    }
                });
                if let Some(i) = drop {
                    self.samples.remove(i);
                }
            }
            Kind::Demonstration => match infer::infer(&self.demos) {
                Some(g) => {
                    ui.label(format!(
                        "Recognized as: {} ({} of {} demonstrations agree)",
                        describe(&g),
                        g.agreeing,
                        g.total
                    ));
                    let conflicts = infer::conflicts(&cx.shared.config, &g);
                    if !conflicts.is_empty() {
                        ui.colored_label(
                            Color32::from_rgb(240, 160, 80),
                            format!(
                                "The same movement already triggers: {}",
                                conflicts.join(", ")
                            ),
                        );
                    }
                }
                None => {
                    ui.label("None of the demonstrations was a tap, hold or swipe. Go back and try again.");
                }
            },
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("← Record again").clicked() {
                cx.send(Command::SetMode(match self.kind {
                    Kind::Symbol => EngineMode::RecordSymbol,
                    Kind::Demonstration => EngineMode::RecordDemonstration,
                }));
                self.step = Step::Record;
            }
            let ok = match self.kind {
                Kind::Symbol => !self.samples.is_empty(),
                Kind::Demonstration => infer::infer(&self.demos).is_some(),
            };
            if self.templates_only {
                if ui
                    .add_enabled(ok, egui::Button::new("Save templates"))
                    .clicked()
                {
                    for sample in self.samples.drain(..) {
                        cx.send(Command::AddTemplate {
                            symbol: self.symbol.clone(),
                            sample,
                        });
                    }
                    return Outcome::Closed;
                }
            } else if ui.add_enabled(ok, egui::Button::new("Next →")).clicked() {
                if self.name.is_empty() {
                    self.name = match self.kind {
                        Kind::Symbol => self.symbol.clone(),
                        Kind::Demonstration => infer::infer(&self.demos)
                            .map(|g| describe(&g).replace([' ', ','], "-"))
                            .unwrap_or_default(),
                    };
                }
                self.step = Step::Details;
            }
            Outcome::Open
        })
        .inner
    }

    fn build(&self, cx: &Ctx) -> Result<GestureDef, String> {
        let name = self.name.trim().to_string();
        if name.is_empty() {
            return Err("give the gesture a name".into());
        }
        if cx.shared.config.gestures.iter().any(|g| g.def.name == name) {
            return Err(format!("a gesture named “{name}” already exists"));
        }
        let action = self.action.build()?;
        let apps: Vec<String> = self
            .apps
            .split(',')
            .map(|a| a.trim().to_string())
            .filter(|a| !a.is_empty())
            .collect();
        let when = (!apps.is_empty()).then_some(When {
            app: Some(OneOrMany::Many(apps)),
            title: None,
        });
        let mut def = GestureDef {
            name,
            kind: GestureKind::Symbol,
            fingers: None,
            taps: None,
            direction: None,
            symbol: None,
            confidence: None,
            action,
            enabled: true,
            profile: None,
            when,
            repeatable: None,
        };
        match self.kind {
            Kind::Symbol => {
                def.symbol = Some(OneOrMany::One(self.symbol.clone()));
                def.confidence = self.confidence;
            }
            Kind::Demonstration => {
                let g = infer::infer(&self.demos).ok_or("no demonstration to learn from")?;
                def.kind = g.kind;
                def.fingers = Some(g.fingers);
                def.taps = g.taps.filter(|&t| t > 1);
                def.direction = g.direction;
            }
        }
        // The same validation as the configuration file's.
        config_edit::append_gesture("", &def)?;
        Ok(def)
    }

    fn details_step(&mut self, ui: &mut egui::Ui, cx: &Ctx) -> Outcome {
        egui::Grid::new("details")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label("Gesture name");
                ui.text_edit_singleline(&mut self.name);
                ui.end_row();
                ui.label("Action");
                ui.vertical(|ui| {
                    self.action.ui(ui);
                    if ui
                        .small_button("▶ Run now")
                        .on_hover_text("Try the action")
                        .clicked()
                        && let Ok(a) = self.action.build()
                    {
                        cx.send(Command::RunAction(a));
                    }
                });
                ui.end_row();
                if self.kind == Kind::Symbol {
                    ui.label("Minimum score");
                    ui.horizontal(|ui| {
                        let mut custom = self.confidence.is_some();
                        ui.checkbox(&mut custom, "custom");
                        let mut c = self
                            .confidence
                            .unwrap_or(cx.shared.config.settings.default_confidence);
                        ui.add_enabled(custom, egui::Slider::new(&mut c, 0.5..=0.99));
                        self.confidence = custom.then_some(c);
                    });
                    ui.end_row();
                }
                ui.label("Only in apps");
                ui.add(
                    egui::TextEdit::singleline(&mut self.apps)
                        .hint_text("e.g. chrome.exe, firefox.exe (empty: everywhere)"),
                );
                ui.end_row();
            });
        let built = self.build(cx);
        if let Err(e) = &built {
            ui.colored_label(Color32::from_rgb(240, 110, 110), e);
        }
        ui.horizontal(|ui| {
            if ui.button("← Back").clicked() {
                self.step = Step::Quality;
            }
            if ui
                .add_enabled(built.is_ok(), egui::Button::new("💾 Save"))
                .clicked()
                && let Ok(def) = built
            {
                cx.send(Command::SaveGesture {
                    def,
                    symbol_samples: (self.kind == Kind::Symbol)
                        .then(|| (self.symbol.clone(), self.samples.clone())),
                });
                self.saved = true;
                self.step = Step::Done;
            }
            Outcome::Open
        })
        .inner
    }

    fn done_step(&mut self, ui: &mut egui::Ui, cx: &Ctx) -> Outcome {
        ui.heading("Saved");
        ui.label(cx.shared.status.clone().unwrap_or_default());
        ui.label(
            "Try it a few times in test mode to check that it is recognized: actions will not run.",
        );
        ui.horizontal(|ui| {
            if ui.button("Try it in test mode").clicked() {
                cx.send(Command::SetMode(EngineMode::Test));
                return Outcome::Test;
            }
            if ui.button("Close").clicked() {
                return Outcome::Closed;
            }
            Outcome::Open
        })
        .inner
    }
}

fn describe_draw_mode(cx: &Ctx) -> String {
    use glyphpad_core::config::DrawActivation as D;
    match cx.shared.config.settings.draw_activation {
        D::Fingers { count } => format!("with {count} fingers"),
        D::Edge { edge, .. } => {
            format!("with one finger starting at the {edge:?} edge").to_lowercase()
        }
        D::Hold { ms, .. } => format!("rest one finger for {ms} ms, then draw"),
        D::Toggle { .. } => {
            "with one finger (each example is armed automatically while recording)".into()
        }
    }
}

fn describe(g: &Inferred) -> String {
    match g.kind {
        GestureKind::Tap => format!("{}× tap with {} finger(s)", g.taps.unwrap_or(1), g.fingers),
        GestureKind::Hold => format!("hold with {} finger(s)", g.fingers),
        GestureKind::Swipe => format!(
            "swipe {} with {} finger(s)",
            g.direction.map(|d| d.to_string()).unwrap_or_default(),
            g.fingers
        ),
        GestureKind::Symbol => "symbol".into(),
    }
}

/// A basic action editor; the complete one, with shortcut capture and
/// chains, comes with the configuration editor (SPEC §10.2, M5).
pub struct ActionDraft {
    kind: &'static str,
    combo: String,
    command: String,
    args: String,
    hidden: bool,
    force: bool,
    media: MediaKey,
    message: String,
}

impl Default for ActionDraft {
    fn default() -> Self {
        Self {
            kind: "notify",
            combo: String::new(),
            command: String::new(),
            args: String::new(),
            hidden: false,
            force: false,
            media: MediaKey::PlayPause,
            message: "It works!".into(),
        }
    }
}

const KINDS: &[(&str, &str)] = &[
    ("keys", "Press keys"),
    ("lock_screen", "Lock the PC"),
    ("close_window", "Close the window in front"),
    ("minimize", "Minimize the window"),
    ("maximize", "Maximize / restore the window"),
    ("run", "Run a program"),
    ("media", "Media key"),
    ("notify", "Show a message"),
    ("toggle_enabled", "Pause / resume gestures"),
    ("repeat_last", "Repeat the last gesture"),
    ("show_window", "Show the GlyphPad window"),
    ("draw_symbol", "Draw a symbol with one finger next"),
];

impl ActionDraft {
    fn ui(&mut self, ui: &mut egui::Ui) {
        let label = KINDS
            .iter()
            .find(|(k, _)| *k == self.kind)
            .map_or("", |(_, l)| *l);
        egui::ComboBox::from_id_salt("action-kind")
            .selected_text(label)
            .show_ui(ui, |ui| {
                for (k, l) in KINDS {
                    ui.selectable_value(&mut self.kind, *k, *l);
                }
            });
        match self.kind {
            "keys" => {
                ui.add(egui::TextEdit::singleline(&mut self.combo).hint_text("e.g. Ctrl+Shift+T"));
                ui.weak("Win+L and Ctrl+Alt+Delete cannot be simulated; use “Lock the PC”.");
            }
            "close_window" => {
                ui.checkbox(
                    &mut self.force,
                    "Terminate the program instead of asking it to close",
                );
            }
            "run" => {
                ui.add(
                    egui::TextEdit::singleline(&mut self.command).hint_text("program, e.g. wt.exe"),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut self.args)
                        .hint_text("arguments, separated by spaces"),
                );
                ui.checkbox(&mut self.hidden, "Without a console window");
            }
            "media" => {
                egui::ComboBox::from_id_salt("media-key")
                    .selected_text(format!("{:?}", self.media))
                    .show_ui(ui, |ui| {
                        for m in [
                            MediaKey::PlayPause,
                            MediaKey::Next,
                            MediaKey::Previous,
                            MediaKey::Stop,
                            MediaKey::VolumeUp,
                            MediaKey::VolumeDown,
                            MediaKey::Mute,
                        ] {
                            ui.selectable_value(&mut self.media, m, format!("{m:?}"));
                        }
                    });
            }
            "notify" => {
                ui.text_edit_singleline(&mut self.message);
            }
            _ => {}
        }
    }

    fn build(&self) -> Result<Action, String> {
        let a = match self.kind {
            "keys" => Action::Keys {
                combo: Some(self.combo.parse().map_err(|e: String| e)?),
                sequence: Vec::new(),
            },
            "lock_screen" => Action::LockScreen,
            "close_window" => Action::CloseWindow { force: self.force },
            "minimize" => Action::Minimize,
            "maximize" => Action::Maximize,
            "run" => Action::Run {
                command: self.command.trim().to_string(),
                args: self.args.split_whitespace().map(str::to_string).collect(),
                hidden: self.hidden,
                cwd: None,
            },
            "media" => Action::Media { key: self.media },
            "notify" => Action::Notify {
                title: None,
                message: self.message.clone(),
            },
            "toggle_enabled" => Action::ToggleEnabled,
            "repeat_last" => Action::RepeatLast,
            "show_window" => Action::ShowWindow,
            "draw_symbol" => Action::DrawSymbol,
            other => return Err(format!("unknown action {other}")),
        };
        a.validate()?;
        Ok(a)
    }
}
