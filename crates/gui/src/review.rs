//! "Review gestures" page (SPEC §10.1): every template of every symbol,
//! animated with its real timing, and the history of recognitions with the
//! trace overlaid on the template that recognized it.

use std::collections::BTreeMap;

use eframe::egui::{self, Color32, RichText};
use glyphpad_core::service::{Command, HistoryEntry};
use glyphpad_core::symbols::{Sample, templates};

use crate::Ctx;
use crate::canvas::{self, Pad, Playback};
use crate::style;

#[derive(Default, PartialEq)]
enum View {
    #[default]
    Templates,
    History,
}

#[derive(Default)]
pub struct State {
    view: View,
    symbol: Option<String>,
    playback: Playback,
    /// Index into the history, newest last.
    entry: Option<usize>,
    assign_to: String,
    builtin_counts: Option<BTreeMap<String, usize>>,
    confirm_delete: Option<(String, usize)>,
}

const THUMB: f32 = 180.0;

/// Returns a symbol to record more templates for, when asked.
pub fn show(ui: &mut egui::Ui, cx: &Ctx, st: &mut State) -> Option<String> {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            style::page_header(
                ui,
                "Review gestures",
                "Templates of every symbol and the latest recognitions.",
            )
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
            ui.selectable_value(
                &mut st.view,
                View::History,
                format!("History ({})", cx.shared.history.len()),
            );
            ui.selectable_value(&mut st.view, View::Templates, "Templates");
        });
    });
    match st.view {
        View::Templates => templates_view(ui, cx, st),
        View::History => {
            history_view(ui, cx, st);
            None
        }
    }
}

fn templates_view(ui: &mut egui::Ui, cx: &Ctx, st: &mut State) -> Option<String> {
    let mut record = None;
    let effective = cx.shared.effective.clone();
    let builtin = st
        .builtin_counts
        .get_or_insert_with(|| {
            templates::defaults()
                .into_iter()
                .map(|(k, v)| (k, v.len()))
                .collect()
        })
        .clone();
    if st
        .symbol
        .as_ref()
        .is_none_or(|s| !effective.contains_key(s))
    {
        st.symbol = effective.keys().next().cloned();
    }

    egui::Panel::left("symbols")
        .resizable(false)
        .exact_size(190.0)
        .show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (name, samples) in effective.iter() {
                    let used = cx.shared.config.gestures.iter().any(|g| {
                        g.def
                            .symbol
                            .as_ref()
                            .is_some_and(|s| s.to_vec().contains(name))
                    });
                    let selected = st.symbol.as_ref() == Some(name);
                    let r = ui.add_sized(
                        [170.0, 28.0],
                        egui::Button::selectable(
                            selected,
                            format!("{name}   ·   {}", samples.len()),
                        ),
                    );
                    if r.clicked() {
                        st.symbol = Some(name.clone());
                        st.playback.restart();
                    }
                    if used {
                        r.on_hover_text("Used by a gesture");
                    }
                }
            });
        });

    let Some(symbol) = st.symbol.clone() else {
        ui.label("No templates.");
        return None;
    };
    let samples: &[Sample] = effective.get(&symbol).map_or(&[], Vec::as_slice);
    let hidden = cx.shared.templates.hidden_builtins.contains(&symbol);
    let n_builtin = if hidden {
        0
    } else {
        builtin.get(&symbol).copied().unwrap_or(0)
    };

    ui.horizontal(|ui| {
        ui.label(RichText::new(&symbol).size(22.0).strong());
        if ui.button("✚ Record more examples").clicked() {
            record = Some(symbol.clone());
        }
        if builtin.contains_key(&symbol) {
            let mut use_builtin = !hidden;
            if ui
                .checkbox(&mut use_builtin, "Use the built-in examples")
                .changed()
            {
                cx.send(Command::HideBuiltins {
                    symbol: symbol.clone(),
                    hidden: !use_builtin,
                });
            }
        }
    });
    let total = canvas::duration_us(samples.iter().flat_map(|s| &s.strokes));
    st.playback.controls(ui, total);
    st.playback.advance(ui.ctx(), total);
    ui.add_space(6.0);

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(12.0, 12.0);
            for (i, sample) in samples.iter().enumerate() {
                egui::Frame::new()
                    .fill(ui.visuals().faint_bg_color)
                    .corner_radius(egui::CornerRadius::same(8))
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        ui.set_width(THUMB);
                        ui.vertical(|ui| {
                            canvas::thumbnail(ui, THUMB, sample, Some(st.playback.t_us));
                            ui.horizontal(|ui| {
                                if i < n_builtin {
                                    ui.weak(format!("built-in {}", i + 1));
                                } else {
                                    let user_index = i - n_builtin;
                                    ui.label(format!("recorded {}", user_index + 1));
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui
                                                .small_button("🗑")
                                                .on_hover_text("Delete this example")
                                                .clicked()
                                            {
                                                st.confirm_delete =
                                                    Some((symbol.clone(), user_index));
                                            }
                                        },
                                    );
                                }
                            });
                        });
                    });
            }
        });
    });

    if let Some((sym, index)) = st.confirm_delete.clone() {
        egui::Modal::new(egui::Id::unique("delete-template")).show(ui.ctx(), |ui| {
            ui.heading("Delete this example?");
            ui.label(format!("Recorded example {} of “{sym}”.", index + 1));
            ui.horizontal(|ui| {
                if ui
                    .button(RichText::new("Delete").color(style::BAD))
                    .clicked()
                {
                    cx.send(Command::DeleteTemplate {
                        symbol: sym.clone(),
                        index,
                    });
                    st.confirm_delete = None;
                }
                if ui.button("Cancel").clicked() {
                    st.confirm_delete = None;
                }
            });
        });
    }
    record
}

fn history_view(ui: &mut egui::Ui, cx: &Ctx, st: &mut State) {
    let history = &cx.shared.history;
    if history.is_empty() {
        ui.label("No gestures recognized yet. The last ones are kept here while GlyphPad runs.");
        return;
    }
    if st.entry.is_some_and(|i| i >= history.len()) {
        st.entry = None;
    }
    egui::Panel::left("history")
        .resizable(false)
        .exact_size(420.0)
        .show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (i, e) in history.iter().enumerate().rev() {
                    let selected = st.entry == Some(i);
                    let fill = if selected {
                        style::ACCENT.gamma_multiply(0.25)
                    } else {
                        ui.visuals().faint_bg_color
                    };
                    let r = egui::Frame::new()
                        .fill(fill)
                        .corner_radius(egui::CornerRadius::same(8))
                        .inner_margin(egui::Margin::symmetric(10, 6))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                ui.monospace(&e.time);
                                ui.label(
                                    RichText::new(
                                        e.gesture.clone().unwrap_or_else(|| "(no gesture)".into()),
                                    )
                                    .strong(),
                                );
                                if let Some(s) = e.score {
                                    ui.weak(format!("{s:.2}"));
                                }
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        let (text, color) = if e.executed {
                                            (e.action.clone().unwrap_or_default(), style::OK)
                                        } else {
                                            ("not run".to_string(), Color32::GRAY)
                                        };
                                        ui.colored_label(color, text);
                                    },
                                );
                            });
                            ui.add(
                                egui::Label::new(
                                    RichText::new(format!(
                                        "{}  ·  {}",
                                        e.recognized,
                                        e.app.clone().unwrap_or_default()
                                    ))
                                    .small()
                                    .weak(),
                                )
                                .truncate(),
                            );
                        })
                        .response
                        .interact(egui::Sense::click());
                    if r.clicked() {
                        st.entry = Some(i);
                        st.playback.restart();
                        st.assign_to = e.symbol.clone().unwrap_or_default();
                    }
                    ui.add_space(4.0);
                }
            });
        });

    let Some(entry) = st.entry.and_then(|i| history.get(i)) else {
        ui.vertical_centered(|ui| {
            ui.add_space(40.0);
            ui.weak("Select a recognition to replay it.");
        });
        return;
    };
    entry_view(ui, cx, st, entry);
}

fn entry_view(ui: &mut egui::Ui, cx: &Ctx, st: &mut State, e: &HistoryEntry) {
    // A fixed width: the playback must not resize anything around it.
    let width = (ui.available_width() - 16.0).clamp(260.0, 560.0);
    ui.allocate_ui(egui::vec2(width, ui.available_height()), |ui| {
        ui.set_width(width);
        ui.add(egui::Label::new(RichText::new(&e.recognized).strong()).truncate());
        let strokes = &e.trace.strokes;
        let total = if strokes.is_empty() {
            canvas::duration_us(e.trace.fingers.values())
        } else {
            canvas::duration_us(strokes)
        };
        st.playback.controls(ui, total);
        st.playback.advance(ui.ctx(), total);
        let (_, painter, pad) = Pad::new(ui, width, e.width_mm, e.height_mm);
        if strokes.is_empty() {
            canvas::draw_fingers(&painter, &pad, &e.trace.fingers, Some(st.playback.t_us));
            ui.weak("Each finger in its own color.");
            return;
        }
        if let Some(symbol) = &e.symbol
            && let Some(template) = cx.shared.effective.get(symbol).and_then(|s| s.first())
        {
            let fitted = canvas::fit_onto(&template.strokes, strokes);
            let lines: Vec<Vec<(f32, f32)>> = fitted
                .iter()
                .map(|s| s.iter().map(|p| (p.x_mm, p.y_mm)).collect())
                .collect();
            canvas::draw_polylines(
                &painter,
                &pad,
                &lines,
                egui::Stroke::new(7.0, Color32::from_gray(75)),
            );
        }
        canvas::draw_strokes(&painter, &pad, strokes, Some(st.playback.t_us), true);
        ui.weak("In gray: the recognized symbol's first template, scaled onto the drawing.");
        ui.add_space(6.0);
        let sample = || Sample {
            strokes: strokes.clone(),
            width_mm: e.width_mm,
            height_mm: e.height_mm,
        };
        style::card(ui, "Use this drawing", |ui| {
            if let Some(symbol) = &e.symbol
                && ui
                    .button(format!("Add it as an example of “{symbol}”"))
                    .clicked()
            {
                cx.send(Command::AddTemplate {
                    symbol: symbol.clone(),
                    sample: sample(),
                });
            }
            ui.horizontal(|ui| {
                ui.label("Add it to");
                egui::ComboBox::from_id_salt("assign")
                    .selected_text(if st.assign_to.is_empty() {
                        "symbol…".to_string()
                    } else {
                        st.assign_to.clone()
                    })
                    .show_ui(ui, |ui| {
                        for name in cx.shared.effective.keys() {
                            ui.selectable_value(&mut st.assign_to, name.clone(), name);
                        }
                    });
                ui.add(
                    egui::TextEdit::singleline(&mut st.assign_to)
                        .hint_text("or a new name")
                        .desired_width(100.0),
                );
                if ui
                    .add_enabled(!st.assign_to.trim().is_empty(), egui::Button::new("Add"))
                    .clicked()
                {
                    cx.send(Command::AddTemplate {
                        symbol: st.assign_to.trim().to_string(),
                        sample: sample(),
                    });
                }
            });
            ui.weak("Useful when it was recognized as the wrong symbol.");
        });
    });
}
