//! The gesture recording wizard (SPEC §10.3), as a full page: kind,
//! recording with live feedback, samples to replay or discard, quality
//! check, name, action and conditions, save, then an offer to try it in
//! test mode.

use eframe::egui::{self, Color32, Id, RichText, Stroke};
use glyphpad_core::action::Action;
use glyphpad_core::config::{GestureDef, GestureKind, OneOrMany};
use glyphpad_core::config_edit;
use glyphpad_core::infer::{self, Inferred};
use glyphpad_core::service::{Command, EngineMode, Recorded};
use glyphpad_core::symbols::{self, Assessment, Sample};
use glyphpad_core::trigger::TriggerDef;

use crate::Ctx;
use crate::canvas::{self, Pad, Playback};
use crate::forms;
use crate::style;

pub enum Start {
    New,
    /// Record more examples of an existing symbol.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd)]
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
    /// Only add examples to a symbol, no new gesture.
    templates_only: bool,
    symbol: String,
    target: usize,
    playback: Playback,
    /// Frozen when recording ends.
    samples: Vec<Sample>,
    demos: Vec<glyphpad_core::engine::Observation>,
    /// Computed once per set of samples: assessing is expensive.
    assessment: Option<Assessment>,
    def: Option<GestureDef>,
    validation: Option<(String, Result<(), String>)>,
}

impl Wizard {
    pub fn new(start: Start) -> Self {
        let (symbol, templates_only) = match start {
            Start::New => (String::new(), false),
            Start::AddTemplates(s) => (s, true),
        };
        Self {
            step: Step::Kind,
            kind: Kind::Symbol,
            templates_only,
            symbol,
            target: 5,
            playback: Playback::default(),
            samples: Vec::new(),
            demos: Vec::new(),
            assessment: None,
            def: None,
            validation: None,
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, cx: &Ctx) -> Outcome {
        let title = if self.templates_only {
            format!("Record examples of “{}”", self.symbol)
        } else {
            "New gesture".to_string()
        };
        let mut cancel = false;
        ui.horizontal(|ui| {
            ui.vertical(|ui| style::page_header(ui, &title, ""));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                cancel = self.step != Step::Done && ui.button("Cancel").clicked();
            });
        });
        if cancel {
            if cx.shared.mode.is_recording() {
                cx.send(Command::SetMode(EngineMode::Normal));
            }
            return Outcome::Closed;
        }
        self.steps_bar(ui);
        ui.add_space(8.0);
        match self.step {
            Step::Kind => self.kind_step(ui, cx),
            Step::Record => self.record_step(ui, cx),
            Step::Quality => self.quality_step(ui, cx),
            Step::Details => self.details_step(ui, cx),
            Step::Done => self.done_step(ui, cx),
        }
    }

    fn steps_bar(&self, ui: &mut egui::Ui) {
        let steps: &[(Step, &str)] = if self.templates_only {
            &[
                (Step::Kind, "Start"),
                (Step::Record, "Record"),
                (Step::Quality, "Check"),
            ]
        } else {
            &[
                (Step::Kind, "Kind"),
                (Step::Record, "Record"),
                (Step::Quality, "Check"),
                (Step::Details, "Name and action"),
                (Step::Done, "Try it"),
            ]
        };
        ui.horizontal(|ui| {
            for (i, (s, name)) in steps.iter().enumerate() {
                let (color, strong) = if *s == self.step {
                    (style::ACCENT, true)
                } else if *s < self.step {
                    (style::OK, false)
                } else {
                    (Color32::GRAY, false)
                };
                let text = RichText::new(format!("{}  {name}", i + 1)).color(color);
                ui.label(if strong { text.strong() } else { text });
                if i + 1 < steps.len() {
                    ui.weak("—");
                }
            }
        });
    }

    fn kind_step(&mut self, ui: &mut egui::Ui, cx: &Ctx) -> Outcome {
        style::card(ui, "", |ui| {
            if !self.templates_only {
                ui.label(RichText::new("What kind of gesture?").strong());
                ui.radio_value(
                    &mut self.kind,
                    Kind::Symbol,
                    "A symbol drawn on the touchpad",
                );
                ui.radio_value(
                    &mut self.kind,
                    Kind::Demonstration,
                    "A tap, hold, swipe, pinch… shown by doing it a few times",
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
                ui.add(egui::Slider::new(&mut self.target, 3..=10).text("examples"));
                ui.weak(format!(
                    "Draw it the way you will use it: {}. An example ends when you pause.",
                    describe_draw_mode(cx)
                ));
            } else {
                ui.add(egui::Slider::new(&mut self.target, 2..=10).text("demonstrations"));
            }
        });
        ui.add_space(8.0);
        let ready = self.kind == Kind::Demonstration || !self.symbol.trim().is_empty();
        if ui
            .add_enabled(
                ready,
                egui::Button::new(RichText::new("Start recording  →").strong()),
            )
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
        let (w_mm, h_mm) = s.pad_size_mm();
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

        ui.horizontal(|ui| {
            ui.colored_label(style::BAD, RichText::new("● Recording").strong());
            ui.label(format!("{n} of {}", self.target));
            ui.weak("Gestures and actions are suspended meanwhile.");
        });
        ui.columns(2, |cols| {
            let ui = &mut cols[0];
            style::card(ui, "Live", |ui| {
                let (_, painter, pad) = Pad::new(ui, ui.available_width().min(420.0), w_mm, h_mm);
                canvas::draw_polylines(&painter, &pad, &s.drawing, Stroke::new(3.0, style::ACCENT));
                canvas::draw_contacts(&painter, &pad, &s.contacts);
            });
            let ui = &mut cols[1];
            style::card(ui, "Recorded", |ui| {
                if s.recorded.is_empty() {
                    ui.weak("Nothing yet.");
                }
                egui::ScrollArea::vertical()
                    .max_height(420.0)
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            for (i, r) in s.recorded.iter().enumerate() {
                                ui.vertical(|ui| {
                                    ui.set_width(130.0);
                                    match r {
                                        Recorded::Sample(x) => {
                                            canvas::thumbnail(
                                                ui,
                                                130.0,
                                                x,
                                                Some(self.playback.t_us),
                                            );
                                        }
                                        Recorded::Demonstration { trace, observation } => {
                                            let (_, painter, pad) = Pad::new(ui, 130.0, w_mm, h_mm);
                                            canvas::draw_fingers(
                                                &painter,
                                                &pad,
                                                &trace.fingers,
                                                Some(self.playback.t_us),
                                            );
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(observation.to_string()).small(),
                                                )
                                                .truncate(),
                                            );
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
            });
        });

        let min = if self.kind == Kind::Symbol { 3 } else { 1 };
        let mut next = n >= self.target;
        ui.horizontal(|ui| {
            next |= ui
                .add_enabled(
                    n >= min,
                    egui::Button::new(RichText::new("Check  →").strong()),
                )
                .on_disabled_hover_text(format!("Record at least {min}"))
                .clicked();
        });
        if next {
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
            self.assessment = None;
            cx.send(Command::SetMode(EngineMode::Normal));
            self.step = Step::Quality;
        }
        Outcome::Open
    }

    fn quality_step(&mut self, ui: &mut egui::Ui, cx: &Ctx) -> Outcome {
        let threshold = cx.shared.config.settings.default_confidence;
        match self.kind {
            Kind::Symbol => {
                let a = self
                    .assessment
                    .get_or_insert_with(|| {
                        symbols::assess(&self.symbol, &self.samples, &cx.shared.effective)
                    })
                    .clone();
                let mean = a.consistency.iter().sum::<f32>() / a.consistency.len().max(1) as f32;
                let total = canvas::duration_us(self.samples.iter().flat_map(|s| &s.strokes));
                self.playback.advance(ui.ctx(), total);
                let mut drop = None;
                egui::ScrollArea::vertical()
                    .max_height(ui.available_height() - 60.0)
                    .show(ui, |ui| {
                        for (i, (sample, (c, near))) in self
                            .samples
                            .iter()
                            .zip(a.consistency.iter().zip(&a.nearest_other))
                            .enumerate()
                        {
                            style::card(ui, "", |ui| {
                                ui.horizontal(|ui| {
                                    canvas::thumbnail(ui, 140.0, sample, Some(self.playback.t_us));
                                    ui.vertical(|ui| {
                                        ui.set_width(300.0);
                                        ui.label(
                                            RichText::new(format!("Example {}", i + 1)).strong(),
                                        );
                                        let odd = self.samples.len() > 2 && *c < mean - 0.1;
                                        ui.add(
                                            egui::ProgressBar::new(*c)
                                                .desired_width(240.0)
                                                .text(format!("like the others: {c:.2}"))
                                                .fill(if odd {
                                                    style::WARN.gamma_multiply(0.5)
                                                } else {
                                                    style::OK.gamma_multiply(0.5)
                                                }),
                                        );
                                        if odd {
                                            ui.colored_label(
                                                style::WARN,
                                                "Unlike the others: consider discarding it.",
                                            );
                                        }
                                        if let Some(n) = near {
                                            let confusing = n.score >= threshold;
                                            let text = format!(
                                                "Closest other symbol: {} ({:.2})",
                                                n.symbol, n.score
                                            );
                                            if confusing {
                                                ui.colored_label(
                                                    style::BAD,
                                                    format!("{text} — may be confused"),
                                                );
                                            } else {
                                                ui.weak(text);
                                            }
                                        }
                                        if ui.small_button("Discard").clicked() {
                                            drop = Some(i);
                                        }
                                    });
                                    // Side by side with the symbol it may be confused with.
                                    if let Some(n) = near.as_ref().filter(|n| n.score >= threshold)
                                        && let Some(other) = cx
                                            .shared
                                            .effective
                                            .get(&n.symbol)
                                            .and_then(|s| s.first())
                                    {
                                        ui.vertical(|ui| {
                                            canvas::thumbnail(ui, 140.0, other, None);
                                            ui.weak(&n.symbol);
                                        });
                                    }
                                });
                            });
                            ui.add_space(6.0);
                        }
                    });
                if let Some(i) = drop {
                    self.samples.remove(i);
                    self.assessment = None;
                }
            }
            Kind::Demonstration => {
                style::card(ui, "", |ui| match infer::infer(&self.demos) {
                    Some(g) => {
                        ui.label(
                            RichText::new(format!("Recognized: {}", describe(&g)))
                                .size(17.0)
                                .strong(),
                        );
                        ui.label(format!(
                            "{} of {} demonstrations agree.",
                            g.agreeing, g.total
                        ));
                        let conflicts = infer::conflicts(&cx.shared.config, &g);
                        if !conflicts.is_empty() {
                            ui.colored_label(
                                style::WARN,
                                format!(
                                    "The same movement already triggers: {}",
                                    conflicts.join(", ")
                                ),
                            );
                        }
                        ui.weak("You can change type, fingers and direction in the next step.");
                    }
                    None => {
                        ui.label("None of the demonstrations was a gesture GlyphPad knows. Record again.");
                    }
                })
            }
        }
        ui.add_space(8.0);
        let mut outcome = Outcome::Open;
        ui.horizontal(|ui| {
            if ui.button("←  Record again").clicked() {
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
                    .add_enabled(
                        ok,
                        egui::Button::new(RichText::new("Save examples").strong()),
                    )
                    .clicked()
                {
                    for sample in self.samples.drain(..) {
                        cx.send(Command::AddTemplate {
                            symbol: self.symbol.clone(),
                            sample,
                        });
                    }
                    outcome = Outcome::Closed;
                }
            } else if ui
                .add_enabled(ok, egui::Button::new(RichText::new("Next  →").strong()))
                .clicked()
            {
                self.def = Some(self.initial_def(cx));
                self.step = Step::Details;
            }
        });
        outcome
    }

    fn initial_def(&self, cx: &Ctx) -> GestureDef {
        let taken = |n: &str| cx.shared.config.gestures.iter().any(|g| g.def.name == n);
        let unique = |base: String| {
            if !taken(&base) {
                base
            } else {
                (2..)
                    .map(|i| format!("{base}-{i}"))
                    .find(|n| !taken(n))
                    .unwrap_or(base)
            }
        };
        let action = Action::Notify {
            title: None,
            message: "It works!".into(),
        };
        match self.kind {
            Kind::Symbol => GestureDef::new(
                unique(format!("draw-{}", self.symbol)),
                TriggerDef {
                    kind: Some(GestureKind::Symbol),
                    symbol: Some(OneOrMany::One(self.symbol.clone())),
                    ..Default::default()
                },
                action,
            ),
            Kind::Demonstration => {
                let g = infer::infer(&self.demos).expect("checked before");
                GestureDef::new(
                    unique(describe(&g).replace([' ', ','], "-").replace("--", "-")),
                    TriggerDef {
                        kind: Some(g.kind),
                        fingers: Some(g.fingers),
                        taps: g.taps.filter(|&t| t > 1),
                        direction: g.direction,
                        zoom: g.zoom,
                        rotation: g.rotation,
                        edge: g.edge,
                        modifier_fingers: g.modifiers,
                        ..Default::default()
                    },
                    action,
                )
            }
        }
    }

    fn details_step(&mut self, ui: &mut egui::Ui, cx: &Ctx) -> Outcome {
        let symbols: Vec<String> = cx
            .shared
            .effective
            .keys()
            .cloned()
            .chain(std::iter::once(self.symbol.clone()))
            .collect();
        let profiles: Vec<String> =
            std::iter::once(cx.shared.config.settings.active_profile.clone())
                .chain(cx.shared.config.settings.profiles.iter().cloned())
                .collect();
        let Some(def) = &mut self.def else {
            return Outcome::Closed;
        };
        let id = Id::unique("wizard-details");
        egui::ScrollArea::vertical()
            .max_height(ui.available_height() - 50.0)
            .show(ui, |ui| {
                style::card(ui, "Name", |ui| {
                    ui.add(egui::TextEdit::singleline(&mut def.name).desired_width(260.0));
                });
                ui.add_space(6.0);
                style::card(ui, "Trigger", |ui| {
                    let mut t = def.trigger_def();
                    forms::trigger_form(
                        ui,
                        id.with("trigger"),
                        &mut t,
                        &symbols,
                        cx.shared.config.settings.default_confidence,
                        false,
                    );
                    forms::set_trigger(def, &t);
                });
                ui.add_space(6.0);
                style::card(ui, "Action", |ui| {
                    forms::action_form(ui, cx, id.with("action"), &mut def.action, false)
                });
                ui.add_space(6.0);
                style::card(ui, "Conditions", |ui| {
                    forms::conditions_form(ui, cx, id.with("when"), def, &profiles)
                });
            });
        // Validated against the file it will be appended to.
        let key = format!("{def:?}");
        if self.validation.as_ref().is_none_or(|v| v.0 != key) {
            let r = config_edit::append_gesture(&cx.shared.config_text, def).map(|_| ());
            self.validation = Some((key, r));
        }
        let valid = self
            .validation
            .as_ref()
            .map(|v| v.1.clone())
            .unwrap_or(Ok(()));
        if let Err(e) = &valid {
            ui.colored_label(style::BAD, e);
        }
        ui.horizontal(|ui| {
            if ui.button("←  Back").clicked() {
                self.step = Step::Quality;
            }
            if ui
                .add_enabled(
                    valid.is_ok(),
                    egui::Button::new(RichText::new("💾 Save").strong()),
                )
                .clicked()
            {
                cx.send(Command::SaveGesture {
                    def: Box::new(def.clone()),
                    symbol_samples: (self.kind == Kind::Symbol)
                        .then(|| (self.symbol.clone(), self.samples.clone())),
                });
                self.step = Step::Done;
            }
        });
        Outcome::Open
    }

    fn done_step(&mut self, ui: &mut egui::Ui, cx: &Ctx) -> Outcome {
        let mut outcome = Outcome::Open;
        style::card(ui, "Saved", |ui| {
            ui.label(cx.shared.status.clone().unwrap_or_default());
            ui.label("Try it a few times in test mode to check that it is recognized; no action runs there.");
            ui.horizontal(|ui| {
                if ui
                    .button(RichText::new("Try it in test mode").strong())
                    .clicked()
                {
                    cx.send(Command::SetMode(EngineMode::Test));
                    outcome = Outcome::Test;
                }
                if ui.button("Done").clicked() {
                    outcome = Outcome::Closed;
                }
            });
        });
        outcome
    }
}

fn describe_draw_mode(cx: &Ctx) -> String {
    use glyphpad_core::config::DrawActivation as D;
    match cx.shared.config.settings.draw_activation {
        D::Fingers { count } => format!("with {count} fingers"),
        D::Edge { edge, .. } => format!(
            "with one finger starting at the {} edge",
            format!("{edge:?}").to_lowercase()
        ),
        D::Hold { ms, .. } => format!("rest one finger for {ms} ms, then draw"),
        D::Toggle { .. } => "with one finger".into(),
    }
}

fn describe(g: &Inferred) -> String {
    let t = TriggerDef {
        kind: Some(g.kind),
        fingers: Some(g.fingers),
        taps: g.taps,
        direction: g.direction,
        zoom: g.zoom,
        rotation: g.rotation,
        edge: g.edge,
        modifier_fingers: g.modifiers,
        ..Default::default()
    };
    t.build()
        .map(|t| t.describe())
        .unwrap_or_else(|_| g.kind.label().to_string())
}
