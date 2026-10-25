//! The gesture recording wizard (SPEC §10.3), as a full page: kind,
//! recording with live feedback, samples to replay or discard, quality
//! check, name, action and conditions, save, then an offer to try it in
//! test mode.

use eframe::egui::{self, Align, Align2, CornerRadius, Id, Layout, Sense, Stroke, Vec2};
use glyphpad_core::action::Action;
use glyphpad_core::config::{GestureDef, GestureKind, OneOrMany};
use glyphpad_core::config_edit;
use glyphpad_core::infer::{self, Inferred};
use glyphpad_core::service::{Command, EngineMode, Recorded};
use glyphpad_core::symbols::{self, Assessment, Sample};
use glyphpad_core::trigger::TriggerDef;

use crate::canvas::{self, Pad, Playback};
use crate::forms;
use crate::theme::{fonts, pal, radius, size, space};
use crate::widgets::{self as w, Tone};
use crate::{Ctx, icons};

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

const WIDTH: f32 = 780.0;

pub struct Wizard {
    step: Step,
    kind: Kind,
    /// Only add examples to a symbol, no new gesture.
    templates_only: bool,
    symbol: String,
    target: u8,
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
        w::page_header(ui, &title, "", |ui| {
            if self.step != Step::Done && w::button(ui, icons::X, "Cancel").clicked() {
                cancel = true;
            }
        });
        if cancel {
            if cx.shared.mode.is_recording() {
                cx.send(Command::SetMode(EngineMode::Normal));
            }
            return Outcome::Closed;
        }
        self.steps_bar(ui);
        ui.add_space(space::L);
        let mut out = Outcome::Open;
        w::scroll_page(ui, "wizard", |ui| {
            w::centered_column(ui, WIDTH, |ui| {
                out = match self.step {
                    Step::Kind => self.kind_step(ui, cx),
                    Step::Record => self.record_step(ui, cx),
                    Step::Quality => self.quality_step(ui, cx),
                    Step::Details => self.details_step(ui, cx),
                    Step::Done => self.done_step(ui, cx),
                };
            });
        });
        out
    }

    /// Numbered circles joined by lines: where you are, what is done.
    fn steps_bar(&self, ui: &mut egui::Ui) {
        let p = pal(ui);
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
        let width = ui.available_width().min(WIDTH);
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 58.0), Sense::hover());
        let left = rect.left() + (rect.width() - width) / 2.0;
        let n = steps.len();
        let gap = width / n as f32;
        for (i, (s, name)) in steps.iter().enumerate() {
            let cx_ = left + gap * (i as f32 + 0.5);
            let center = egui::pos2(cx_, rect.top() + 14.0);
            if i + 1 < n {
                let done = *s < self.step;
                ui.painter().line_segment(
                    [
                        egui::pos2(cx_ + 18.0, center.y),
                        egui::pos2(cx_ + gap - 18.0, center.y),
                    ],
                    Stroke::new(2.0, if done { p.ok } else { p.border_strong }),
                );
            }
            let (fill, fg, label_color) = if *s == self.step {
                (p.accent, p.on_accent, p.text)
            } else if *s < self.step {
                (p.ok_soft, p.ok, p.text_dim)
            } else {
                (p.raised, p.text_faint, p.text_faint)
            };
            ui.painter().circle_filled(center, 13.0, fill);
            if *s < self.step {
                ui.painter().text(
                    center,
                    Align2::CENTER_CENTER,
                    icons::CHECK,
                    fonts::icon(14.0),
                    fg,
                );
            } else {
                ui.painter().text(
                    center,
                    Align2::CENTER_CENTER,
                    (i + 1).to_string(),
                    fonts::strong(size::SMALL),
                    fg,
                );
            }
            ui.painter().text(
                egui::pos2(cx_, rect.top() + 40.0),
                Align2::CENTER_CENTER,
                name,
                if *s == self.step {
                    fonts::strong(size::SMALL)
                } else {
                    fonts::text(size::SMALL)
                },
                label_color,
            );
        }
    }

    fn kind_option(
        ui: &mut egui::Ui,
        selected: bool,
        icon: &str,
        title: &str,
        text: &str,
    ) -> egui::Response {
        let p = pal(ui);
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 78.0), Sense::click());
        let (fill, stroke) = if selected {
            (p.accent_soft, Stroke::new(1.5, p.accent))
        } else if response.hovered() {
            (p.hover, Stroke::new(1.0, p.border_strong))
        } else {
            (p.surface, Stroke::new(1.0, p.border))
        };
        ui.painter().rect(
            rect,
            CornerRadius::same(radius::CARD),
            fill,
            stroke,
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            egui::pos2(rect.left() + 34.0, rect.center().y),
            Align2::CENTER_CENTER,
            icon,
            fonts::icon(28.0),
            if selected { p.accent } else { p.text_dim },
        );
        ui.painter().text(
            egui::pos2(rect.left() + 70.0, rect.center().y - 12.0),
            Align2::LEFT_CENTER,
            title,
            fonts::strong(size::SECTION),
            p.text,
        );
        let g = w::elided(
            ui,
            text,
            fonts::text(size::BODY),
            p.text_dim,
            rect.width() - 90.0,
        );
        ui.painter().galley(
            egui::pos2(rect.left() + 70.0, rect.center().y + 3.0),
            g,
            p.text_dim,
        );
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    }

    fn kind_step(&mut self, ui: &mut egui::Ui, cx: &Ctx) -> Outcome {
        if !self.templates_only {
            if Self::kind_option(
                ui,
                self.kind == Kind::Symbol,
                icons::SCRIBBLE_LOOP,
                "A symbol",
                "Draw a letter or shape on the touchpad, like an L.",
            )
            .clicked()
            {
                self.kind = Kind::Symbol;
                self.target = self.target.clamp(3, 10);
            }
            ui.add_space(space::S);
            if Self::kind_option(
                ui,
                self.kind == Kind::Demonstration,
                icons::HAND_SWIPE_RIGHT,
                "A movement",
                "Tap, hold, swipe or pinch: show it a few times and GlyphPad works it out.",
            )
            .clicked()
            {
                self.kind = Kind::Demonstration;
                self.target = self.target.clamp(2, 10);
            }
            ui.add_space(space::M);
        }
        w::section(ui, "", "", |ui| {
            if self.kind == Kind::Symbol {
                w::row(ui, "Symbol name", "e.g. L, q, star", |ui| {
                    ui.add_enabled_ui(!self.templates_only, |ui| {
                        w::text_field(ui, &mut self.symbol, "Name", 200.0);
                    });
                });
                w::row(
                    ui,
                    "Examples",
                    "More examples make it more reliable.",
                    |ui| {
                        w::stepper(ui, &mut self.target, 3, 10);
                    },
                );
                w::row(ui, "How you will draw it", "", |ui| {
                    ui.label(describe_draw_mode(cx));
                    ui.label(w::caption(ui, "An example ends when you pause."));
                });
            } else {
                w::row(ui, "Demonstrations", "Do the same thing each time.", |ui| {
                    w::stepper(ui, &mut self.target, 2, 10);
                });
            }
        });
        let ready = self.kind == Kind::Demonstration || !self.symbol.trim().is_empty();
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if w::primary_button_enabled(ui, icons::ARROW_RIGHT, "Start recording", ready).clicked()
            {
                self.symbol = self.symbol.trim().to_string();
                cx.send(Command::SetMode(match self.kind {
                    Kind::Symbol => EngineMode::RecordSymbol,
                    Kind::Demonstration => EngineMode::RecordDemonstration,
                }));
                self.step = Step::Record;
            }
        });
        Outcome::Open
    }

    fn record_step(&mut self, ui: &mut egui::Ui, cx: &Ctx) -> Outcome {
        let s = cx.shared;
        let p = pal(ui);
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

        let target = usize::from(self.target);
        // A pulsing dot says "recording" without words.
        let t = ui.input(|i| i.time) as f32;
        let pulse = 0.55 + 0.45 * (t * 3.0).sin().abs();
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(60));
        w::card_frame(ui).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
                ui.painter()
                    .circle_filled(r.center(), 6.0, p.bad.gamma_multiply(pulse));
                ui.label(w::strong("Recording"));
                ui.label(w::dim(ui, "Gestures and actions are paused meanwhile."));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(w::strong(format!("{n} / {target}")));
                });
            });
            ui.add_space(space::XS);
            let (bar, _) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 6.0), Sense::hover());
            ui.painter()
                .rect_filled(bar, CornerRadius::same(3), p.raised);
            let mut fill = bar;
            fill.set_width(bar.width() * (n as f32 / target.max(1) as f32).min(1.0));
            ui.painter()
                .rect_filled(fill, CornerRadius::same(3), p.accent);
        });
        ui.add_space(space::M);

        w::section(ui, "Live", "Do it on the touchpad now.", |ui| {
            let pad_w = ui.available_width().min(460.0);
            ui.vertical_centered(|ui| {
                let (_, painter, pad) = Pad::new(ui, pad_w, w_mm, h_mm);
                canvas::draw_polylines(&painter, &pad, &s.drawing, Stroke::new(3.5, p.accent));
                canvas::draw_contacts(&painter, &pad, &s.contacts);
            });
        });

        w::section(ui, "Recorded", "", |ui| {
            if s.recorded.is_empty() {
                ui.label(w::dim(ui, "Nothing yet."));
            }
            w::grid(ui, s.recorded.len(), 140.0, space::M, |ui, i, width| {
                let r = &s.recorded[i];
                match r {
                    Recorded::Sample(x) => {
                        canvas::thumbnail(ui, width, x, Some(self.playback.t_us));
                    }
                    Recorded::Demonstration { trace, observation } => {
                        let (_, painter, pad) = Pad::new(ui, width, w_mm, h_mm);
                        canvas::draw_fingers(
                            &painter,
                            &pad,
                            &trace.fingers,
                            Some(self.playback.t_us),
                        );
                        ui.add(
                            egui::Label::new(w::caption(ui, observation.to_string())).truncate(),
                        );
                    }
                }
                ui.horizontal(|ui| {
                    ui.label(w::faint(ui, format!("#{}", i + 1)));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if w::icon_button_tone(
                            ui,
                            icons::TRASH,
                            "Discard it and record another",
                            Tone::Bad,
                        )
                        .clicked()
                        {
                            cx.send(Command::DiscardRecorded(i));
                        }
                    });
                });
            });
        });

        let min = if self.kind == Kind::Symbol { 3 } else { 1 };
        let mut next = n >= target;
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            next |= w::primary_button_enabled(ui, icons::ARROW_RIGHT, "Check", n >= min)
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
        let p = pal(ui);
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
                for (i, (sample, (c, near))) in self
                    .samples
                    .iter()
                    .zip(a.consistency.iter().zip(&a.nearest_other))
                    .enumerate()
                {
                    w::card_frame(ui).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal_top(|ui| {
                            canvas::thumbnail(ui, 150.0, sample, Some(self.playback.t_us));
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(w::strong(format!("Example {}", i + 1)));
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        if w::icon_button_tone(
                                            ui,
                                            icons::TRASH,
                                            "Discard this example",
                                            Tone::Bad,
                                        )
                                        .clicked()
                                        {
                                            drop = Some(i);
                                        }
                                    });
                                });
                                let odd = self.samples.len() > 2 && *c < mean - 0.1;
                                ui.label(w::caption(ui, "Similarity to the other examples"));
                                score_line(ui, *c, if odd { Tone::Warn } else { Tone::Ok });
                                if odd {
                                    w::warn_text(ui, "Unlike the others: consider discarding it.");
                                }
                                if let Some(n) = near {
                                    let confusing = n.score >= threshold;
                                    let text = format!(
                                        "Closest other symbol: {} ({:.2})",
                                        n.symbol, n.score
                                    );
                                    if confusing {
                                        w::error_text(ui, &format!("{text}: may be confused"));
                                    } else {
                                        ui.label(w::caption(ui, text));
                                    }
                                }
                            });
                            // Side by side with the symbol it may be confused with.
                            if let Some(n) = near.as_ref().filter(|n| n.score >= threshold)
                                && let Some(other) =
                                    cx.shared.effective.get(&n.symbol).and_then(|s| s.first())
                            {
                                ui.vertical(|ui| {
                                    canvas::thumbnail(ui, 110.0, other, None);
                                    ui.label(w::caption(ui, &n.symbol));
                                });
                            }
                        });
                    });
                    ui.add_space(space::S);
                }
                if let Some(i) = drop {
                    self.samples.remove(i);
                    self.assessment = None;
                }
            }
            Kind::Demonstration => {
                w::section(ui, "", "", |ui| match infer::infer(&self.demos) {
                    Some(g) => {
                        ui.horizontal(|ui| {
                            ui.label(w::icon_text(icons::CHECK_CIRCLE, 24.0, p.ok));
                            ui.vertical(|ui| {
                                ui.label(w::strong(format!("Recognized: {}", describe(&g))));
                                ui.label(w::caption(
                                    ui,
                                    format!("{} of {} demonstrations agree.", g.agreeing, g.total),
                                ));
                            });
                        });
                        let conflicts = infer::conflicts(&cx.shared.config, &g);
                        if !conflicts.is_empty() {
                            ui.add_space(space::S);
                            w::warn_text(
                                ui,
                                &format!(
                                    "The same movement already triggers: {}",
                                    conflicts.join(", ")
                                ),
                            );
                        }
                        ui.add_space(space::XS);
                        w::hint(
                            ui,
                            "You can change type, fingers and direction in the next step.",
                        );
                    }
                    None => {
                        w::error_text(
                            ui,
                            "None of the demonstrations was a gesture GlyphPad knows. Record again.",
                        );
                    }
                });
            }
        }
        let mut outcome = Outcome::Open;
        ui.horizontal(|ui| {
            if w::button(ui, icons::ARROW_LEFT, "Record again").clicked() {
                cx.send(Command::SetMode(match self.kind {
                    Kind::Symbol => EngineMode::RecordSymbol,
                    Kind::Demonstration => EngineMode::RecordDemonstration,
                }));
                self.step = Step::Record;
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let ok = match self.kind {
                    Kind::Symbol => !self.samples.is_empty(),
                    Kind::Demonstration => infer::infer(&self.demos).is_some(),
                };
                if self.templates_only {
                    if w::primary_button_enabled(ui, icons::CHECK, "Save examples", ok).clicked() {
                        for sample in self.samples.drain(..) {
                            cx.send(Command::AddTemplate {
                                symbol: self.symbol.clone(),
                                sample,
                            });
                        }
                        outcome = Outcome::Closed;
                    }
                } else if w::primary_button_enabled(ui, icons::ARROW_RIGHT, "Next", ok).clicked() {
                    self.def = Some(self.initial_def(cx));
                    self.step = Step::Details;
                }
            });
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
        w::section(ui, "Name", "", |ui| {
            w::text_field(ui, &mut def.name, "Name", ui.available_width());
        });
        w::section(ui, "When", "What you do on the touchpad.", |ui| {
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
        w::section(ui, "Then", "What GlyphPad does.", |ui| {
            forms::action_form(ui, cx, id.with("action"), &mut def.action, false)
        });
        w::section(ui, "Only when", "Limit where the gesture works.", |ui| {
            forms::conditions_form(ui, cx, id.with("when"), def, &profiles)
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
            w::error_text(ui, e);
        }
        ui.horizontal(|ui| {
            if w::button(ui, icons::ARROW_LEFT, "Back").clicked() {
                self.step = Step::Quality;
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if w::primary_button_enabled(ui, icons::CHECK, "Save gesture", valid.is_ok())
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
        });
        Outcome::Open
    }

    fn done_step(&mut self, ui: &mut egui::Ui, cx: &Ctx) -> Outcome {
        let p = pal(ui);
        let mut outcome = Outcome::Open;
        w::section(ui, "", "", |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(space::M);
                ui.label(w::icon_text(icons::CHECK_CIRCLE, 40.0, p.ok));
                ui.label(w::strong("Saved"));
                if let Some(s) = &cx.shared.status {
                    ui.label(w::caption(ui, s));
                }
                ui.add_space(space::S);
                ui.label("Try it a few times in test mode to check that it is recognized.");
                ui.label(w::caption(ui, "No action runs in test mode."));
                ui.add_space(space::M);
                ui.horizontal(|ui| {
                    let width = 330.0;
                    ui.add_space(((ui.available_width() - width) / 2.0).max(0.0));
                    if w::primary_button(ui, icons::FLASK, "Try it in test mode").clicked() {
                        cx.send(Command::SetMode(EngineMode::Test));
                        outcome = Outcome::Test;
                    }
                    if w::button(ui, "", "Done").clicked() {
                        outcome = Outcome::Closed;
                    }
                });
                ui.add_space(space::S);
            });
        });
        outcome
    }
}

fn score_line(ui: &mut egui::Ui, v: f32, tone: Tone) {
    let p = pal(ui);
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(150.0, 6.0), Sense::hover());
        ui.painter()
            .rect_filled(rect, CornerRadius::same(3), p.raised);
        let mut fill = rect;
        fill.set_width(rect.width() * v.clamp(0.0, 1.0));
        ui.painter()
            .rect_filled(fill, CornerRadius::same(3), tone.fg(ui));
        ui.label(
            egui::RichText::new(format!("{v:.2}"))
                .font(fonts::mono(12.5))
                .color(p.text_dim),
        );
    });
}

fn describe_draw_mode(cx: &Ctx) -> String {
    use glyphpad_core::config::DrawActivation as D;
    match cx.shared.config.settings.draw_activation {
        D::Fingers { count } => format!("With {count} fingers"),
        D::Edge { edge, .. } => format!(
            "With one finger starting at the {} edge",
            format!("{edge:?}").to_lowercase()
        ),
        D::Hold { ms, .. } => format!("Rest one finger for {ms} ms, then draw"),
        D::Toggle { .. } => "With one finger".into(),
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
