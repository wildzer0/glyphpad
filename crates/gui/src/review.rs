//! "Review gestures" tab (SPEC §10.1): every template of every symbol,
//! animated with its real timing, and the history of recognitions with the
//! trace overlaid on the template that recognized it.

use std::collections::BTreeMap;

use eframe::egui::{self, Color32};
use glyphpad_core::service::{Command, HistoryEntry};
use glyphpad_core::symbols::{Sample, templates};

use crate::Ctx;
use crate::canvas::{self, Pad, Playback};

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
}

/// Returns a symbol to record more templates for, when asked.
pub fn show(ui: &mut egui::Ui, cx: &Ctx, st: &mut State) -> Option<String> {
    ui.horizontal(|ui| {
        ui.heading("Review gestures");
        ui.selectable_value(&mut st.view, View::Templates, "Templates");
        ui.selectable_value(
            &mut st.view,
            View::History,
            format!("History ({})", cx.shared.history.len()),
        );
    });
    ui.separator();
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
        .show(ui, |ui| {
            ui.strong("Symbols");
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (name, samples) in effective.iter() {
                    let used = cx.shared.config.gestures.iter().any(|g| {
                        g.def
                            .symbol
                            .as_ref()
                            .is_some_and(|s| s.to_vec().contains(name))
                    });
                    let label = format!(
                        "{name}  ({}){}",
                        samples.len(),
                        if used { "  ●" } else { "" }
                    );
                    if ui
                        .selectable_label(st.symbol.as_ref() == Some(name), label)
                        .clicked()
                    {
                        st.symbol = Some(name.clone());
                        st.playback.restart();
                    }
                }
                ui.weak("● used by a gesture");
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
        ui.heading(&symbol);
        if ui.button("➕ Record templates").clicked() {
            record = Some(symbol.clone());
        }
        if builtin.contains_key(&symbol) {
            let mut use_builtin = !hidden;
            if ui
                .checkbox(&mut use_builtin, "Use the built-in templates")
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
    ui.add_space(4.0);

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            for (i, sample) in samples.iter().enumerate() {
                ui.vertical(|ui| {
                    let (_, painter, pad) = Pad::new(ui, 200.0, sample.width_mm, sample.height_mm);
                    canvas::draw_strokes(
                        &painter,
                        &pad,
                        &sample.strokes,
                        Some(st.playback.t_us),
                        true,
                    );
                    ui.horizontal(|ui| {
                        if i < n_builtin {
                            ui.weak(format!("built-in {}", i + 1));
                        } else {
                            let user_index = i - n_builtin;
                            ui.label(format!("recorded {}", user_index + 1));
                            if ui
                                .small_button("🗑")
                                .on_hover_text("Delete this template")
                                .clicked()
                            {
                                cx.send(Command::DeleteTemplate {
                                    symbol: symbol.clone(),
                                    index: user_index,
                                });
                            }
                        }
                    });
                });
            }
        });
    });
    record
}

fn history_view(ui: &mut egui::Ui, cx: &Ctx, st: &mut State) {
    let history = &cx.shared.history;
    if history.is_empty() {
        ui.label("No gestures recognized yet. The history keeps the last ones in memory.");
        return;
    }
    if st.entry.is_some_and(|i| i >= history.len()) {
        st.entry = None;
    }
    egui::Panel::left("history")
        .resizable(true)
        .default_size(420.0)
        .show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("history-grid")
                    .striped(true)
                    .num_columns(5)
                    .show(ui, |ui| {
                        for h in ["Time", "Gesture", "Score", "Action", "App"] {
                            ui.strong(h);
                        }
                        ui.end_row();
                        for (i, e) in history.iter().enumerate().rev() {
                            let name = e.gesture.clone().unwrap_or_else(|| "(none)".into());
                            if ui.selectable_label(st.entry == Some(i), &e.time).clicked() {
                                st.entry = Some(i);
                                st.playback.restart();
                                st.assign_to = e.symbol.clone().unwrap_or_default();
                            }
                            ui.label(name).on_hover_text(&e.recognized);
                            ui.label(e.score.map_or(String::new(), |s| format!("{s:.2}")));
                            let action = e.action.clone().unwrap_or_default();
                            ui.label(if e.executed {
                                action
                            } else if action.is_empty() {
                                String::new()
                            } else {
                                format!("{action} (not run)")
                            });
                            ui.label(e.app.clone().unwrap_or_default());
                            ui.end_row();
                        }
                    });
            });
        });

    let Some(entry) = st.entry.and_then(|i| history.get(i)) else {
        ui.label("Select a recognition to replay it.");
        return;
    };
    entry_view(ui, cx, st, entry);
}

fn entry_view(ui: &mut egui::Ui, cx: &Ctx, st: &mut State, e: &HistoryEntry) {
    ui.strong(&e.recognized);
    let strokes = &e.trace.strokes;
    let total = if strokes.is_empty() {
        canvas::duration_us(e.trace.fingers.values())
    } else {
        canvas::duration_us(strokes)
    };
    st.playback.controls(ui, total);
    st.playback.advance(ui.ctx(), total);
    let width = ui.available_width().min(520.0);
    let (_, painter, pad) = Pad::new(ui, width, e.width_mm, e.height_mm);
    if strokes.is_empty() {
        canvas::draw_fingers(&painter, &pad, &e.trace.fingers, Some(st.playback.t_us));
        return;
    }
    // The template that recognized it, scaled onto the drawing, in gray.
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
            egui::Stroke::new(6.0, Color32::from_gray(90)),
        );
    }
    canvas::draw_strokes(&painter, &pad, strokes, Some(st.playback.t_us), true);
    ui.weak("Gray: the first template of the recognized symbol, scaled onto the drawing.");

    ui.horizontal(|ui| {
        let sample = || Sample {
            strokes: strokes.clone(),
            width_mm: e.width_mm,
            height_mm: e.height_mm,
        };
        if let Some(symbol) = &e.symbol
            && ui
                .button(format!("Add as a template of “{symbol}”"))
                .clicked()
        {
            cx.send(Command::AddTemplate {
                symbol: symbol.clone(),
                sample: sample(),
            });
        }
        ui.label("or add it to");
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
}
