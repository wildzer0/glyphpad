//! "Review" page (SPEC §10.1): every template of every symbol, animated
//! with its real timing, and the history of recognitions with the trace
//! overlaid on the template that recognized it.

use std::collections::BTreeMap;

use eframe::egui::{self, Align, Align2, Color32, CornerRadius, Layout, Rect, Sense, Stroke, Vec2};
use glyphpad_core::service::{Command, HistoryEntry};
use glyphpad_core::symbols::{Sample, templates};

use crate::canvas::{self, Pad, Playback};
use crate::theme::{fonts, pal, radius, size, space};
use crate::widgets::{self as w, Tone};
use crate::{Ctx, icons};

#[derive(Default, PartialEq, Clone, Copy)]
enum View {
    #[default]
    Templates,
    History,
    Statistics,
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
    confirm_reset: bool,
    /// User samples of the selected symbol chosen to be merged into one.
    merge_selection: std::collections::BTreeSet<usize>,
}

/// Returns a symbol to record more templates for, when asked.
pub fn show(ui: &mut egui::Ui, cx: &Ctx, st: &mut State) -> Option<String> {
    crate::tour::Tour::register(ui, crate::tour::Target::ReviewSymbols, ui.max_rect());
    w::page_header(
        ui,
        "Review",
        "The templates of every symbol, and what was recognized lately.",
        |ui| {
            w::segmented_with_icons(
                ui,
                &mut st.view,
                &[
                    (View::Templates, icons::SCRIBBLE_LOOP, "Templates"),
                    (View::History, icons::CLOCK_COUNTER_CLOCKWISE, "History"),
                    (View::Statistics, icons::CHART_BAR, "Statistics"),
                ],
            );
        },
    );
    match st.view {
        View::Templates => templates_view(ui, cx, st),
        View::History => {
            history_view(ui, cx, st);
            None
        }
        View::Statistics => {
            statistics_view(ui, cx, st);
            None
        }
    }
}

fn statistics_view(ui: &mut egui::Ui, cx: &Ctx, st: &mut State) {
    let p = pal(ui);
    let usage = &cx.shared.usage;
    w::scroll_page(ui, "statistics", |ui| {
        w::centered_column(ui, crate::theme::PAGE_MAX_WIDTH, |ui| {
            if !cx.shared.config.settings.statistics {
                w::banner(
                    ui,
                    Tone::Neutral,
                    "Statistics are off",
                    "Turn them on in Settings → Window and tray. The numbers stay on this computer.",
                    |_| {},
                );
            }
            w::section(
                ui,
                "Overview",
                "Counted on this computer only, in stats.json next to the configuration.",
                |ui| {
                    w::row(ui, "Gestures run", "", |ui| {
                        ui.label(w::strong_raw(usage.total_runs().to_string()));
                    });
                    w::row(
                        ui,
                        "Drawings not recognized",
                        "They matched no symbol well enough: record more examples, or lower the minimum score.",
                        |ui| {
                            ui.label(w::strong_raw(usage.unrecognized.to_string()));
                        },
                    );
                    w::row(
                        ui,
                        "Symbols with no gesture",
                        "Recognized, but nothing is set up for them in the active profile.",
                        |ui| {
                            ui.label(w::strong_raw(usage.unmapped.to_string()));
                        },
                    );
                    let closest = usage.closest_symbols(5);
                    if !closest.is_empty() {
                        w::row(
                            ui,
                            "Closest symbols",
                            "What the unrecognized drawings came closest to: the ones being drawn badly, or whose minimum score is too high.",
                            |ui| {
                                ui.vertical(|ui| {
                                    for (symbol, best) in &closest {
                                        ui.horizontal(|ui| {
                                            ui.label(w::strong_raw((*symbol).to_string()));
                                            let text = crate::i18n::tf(
                                                "{} times, last score {}",
                                                &[
                                                    &best.count.to_string(),
                                                    &format!("{:.2}", best.last_score),
                                                ],
                                            );
                                            ui.label(w::dim_raw(ui, text));
                                        });
                                    }
                                });
                            },
                        );
                    }
                    if let Some(since) = &usage.since {
                        w::row(ui, "Counting since", "", |ui| {
                            ui.label(since);
                        });
                    }
                },
            );
            w::section(ui, "By gesture", "", |ui| {
                let max = usage.gestures.values().copied().max().unwrap_or(1).max(1);
                let mut rows: Vec<(String, u64)> = cx
                    .shared
                    .config
                    .gestures
                    .iter()
                    .map(|g| {
                        (
                            g.def.name.clone(),
                            usage.gestures.get(&g.def.name).copied().unwrap_or(0),
                        )
                    })
                    .collect();
                rows.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
                if rows.is_empty() {
                    ui.label(w::dim(ui, "No gestures yet."));
                }
                for (name, count) in rows {
                    ui.horizontal(|ui| {
                        ui.add_sized(
                            [180.0, 24.0],
                            egui::Label::new(w::strong_raw(name)).truncate(),
                        );
                        let (bar, _) = ui.allocate_exact_size(
                            Vec2::new((ui.available_width() - 70.0).max(40.0), 8.0),
                            Sense::hover(),
                        );
                        ui.painter()
                            .rect_filled(bar, CornerRadius::same(4), p.raised);
                        let mut fill = bar;
                        fill.set_width(bar.width() * count as f32 / max as f32);
                        ui.painter()
                            .rect_filled(fill, CornerRadius::same(4), p.accent);
                        ui.label(
                            egui::RichText::new(if count == 0 {
                                "never".into()
                            } else {
                                count.to_string()
                            })
                            .font(fonts::mono(12.5))
                            .color(if count == 0 {
                                p.text_faint
                            } else {
                                p.text
                            }),
                        );
                    });
                }
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if w::danger_button(ui, icons::TRASH, "Reset the counts").clicked() {
                    st.confirm_reset = true;
                }
            });
        });
    });
    if st.confirm_reset {
        let mut close = false;
        w::dialog(
            ui.ctx(),
            "reset-usage",
            "Reset the statistics?",
            360.0,
            |ui| {
                ui.label(crate::i18n::tr("All counts go back to zero."));
                ui.add_space(space::M);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if w::danger_button(ui, icons::TRASH, "Reset").clicked() {
                        cx.send(Command::ResetUsage);
                        close = true;
                    }
                    if w::button(ui, "", "Cancel").clicked() {
                        close = true;
                    }
                });
            },
        );
        if close {
            st.confirm_reset = false;
        }
    }
}

/// A selectable row of a master list: a leading glyph, a title, a note.
fn list_item(
    ui: &mut egui::Ui,
    selected: bool,
    lead: &str,
    lead_is_icon: bool,
    title: &str,
    note: &str,
) -> egui::Response {
    let p = pal(ui);
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 50.0), Sense::click());
    let fill = if selected {
        p.accent_soft
    } else if response.hovered() {
        p.raised
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(
        rect.shrink2(Vec2::new(4.0, 1.0)),
        CornerRadius::same(radius::CONTROL + 2),
        fill,
    );
    let badge = Rect::from_center_size(
        egui::pos2(rect.left() + 30.0, rect.center().y),
        Vec2::splat(32.0),
    );
    ui.painter().rect_filled(
        badge,
        CornerRadius::same(radius::CONTROL + 2),
        if selected { p.surface } else { p.raised },
    );
    ui.painter().text(
        badge.center(),
        Align2::CENTER_CENTER,
        lead,
        if lead_is_icon {
            fonts::icon(17.0)
        } else {
            fonts::strong(17.0)
        },
        if selected { p.accent } else { p.text },
    );
    let text_left = rect.left() + 56.0;
    let width = rect.right() - text_left - 10.0;
    let t = w::elided(ui, title, fonts::strong(size::BODY), p.text, width);
    ui.painter()
        .galley(egui::pos2(text_left, rect.center().y - 17.0), t, p.text);
    let n = w::elided(ui, note, fonts::text(size::SMALL), p.text_dim, width);
    ui.painter()
        .galley(egui::pos2(text_left, rect.center().y + 2.0), n, p.text_dim);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn master_frame<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let p = pal(ui);
    egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(radius::CARD))
        .inner_margin(egui::Margin::same(4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_height(ui.available_height());
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, add)
                .inner
        })
        .inner
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
        .exact_size(220.0)
        .frame(egui::Frame::new())
        .show_separator_line(false)
        .show(ui, |ui| {
            master_frame(ui, |ui| {
                for (name, samples) in effective.iter() {
                    let used = cx.shared.config.gestures.iter().any(|g| {
                        g.def
                            .symbol
                            .as_ref()
                            .is_some_and(|s| s.to_vec().contains(name))
                    });
                    let selected = st.symbol.as_ref() == Some(name);
                    let note = format!(
                        "{}{}",
                        crate::i18n::plural(samples.len(), "{} example", "{} examples"),
                        if used {
                            crate::i18n::tr(" · in use")
                        } else {
                            ""
                        }
                    );
                    let lead: String = name.chars().next().map(String::from).unwrap_or_default();
                    if list_item(ui, selected, &lead, false, name, &note).clicked() {
                        st.symbol = Some(name.clone());
                        st.playback.restart();
                    }
                }
            });
        });

    egui::CentralPanel::default()
        .frame(egui::Frame::new().inner_margin(egui::Margin {
            left: space::L as i8,
            ..Default::default()
        }))
        .show(ui, |ui| {
            let Some(symbol) = st.symbol.clone() else {
                w::empty_state(
                    ui,
                    icons::SCRIBBLE_LOOP,
                    "No templates",
                    "Record a symbol from Gestures.",
                    |_| {},
                );
                return;
            };
            let samples: &[Sample] = effective.get(&symbol).map_or(&[], Vec::as_slice);
            let hidden = cx.shared.templates.hidden_builtins.contains(&symbol);
            let n_builtin = if hidden {
                0
            } else {
                builtin.get(&symbol).copied().unwrap_or(0)
            };

            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(&symbol).font(fonts::strong(26.0)));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if w::primary_button(ui, icons::RECORD, "Record more examples").clicked() {
                        record = Some(symbol.clone());
                    }
                    let chosen = st.merge_selection.len();
                    if w::button_enabled(ui, icons::STACK, "Merge the chosen ones", chosen >= 2)
                        .on_hover_text(crate::i18n::tr(
                            "The two halves of an X recorded apart become one example.",
                        ))
                        .clicked()
                    {
                        cx.send(Command::MergeTemplates {
                            symbol: symbol.clone(),
                            indices: st.merge_selection.iter().copied().collect(),
                        });
                        st.merge_selection.clear();
                    }
                    if builtin.contains_key(&symbol) {
                        let mut use_builtin = !hidden;
                        if w::switch(ui, &mut use_builtin)
                            .on_hover_text(crate::i18n::tr(
                                "Use the examples that come with GlyphPad",
                            ))
                            .changed()
                        {
                            cx.send(Command::HideBuiltins {
                                symbol: symbol.clone(),
                                hidden: !use_builtin,
                            });
                        }
                        ui.label(w::dim(ui, "Built-in examples"));
                    }
                });
            });
            ui.add_space(space::S);
            let total = canvas::duration_us(samples.iter().flat_map(|s| &s.strokes));
            st.playback.controls(ui, total);
            st.playback.advance(ui.ctx(), total);
            ui.add_space(space::M);

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .content_margin(egui::Margin::symmetric(2, 2))
                .show(ui, |ui| {
                    w::grid(ui, samples.len(), 170.0, space::M, |ui, i, width| {
                        sample_card(ui, cx, st, &symbol, &samples[i], i, n_builtin, width);
                    });
                });
        });

    if let Some((sym, index)) = st.confirm_delete.clone() {
        let mut close = false;
        w::dialog(
            ui.ctx(),
            "delete-template",
            "Delete this example?",
            340.0,
            |ui| {
                ui.label(crate::i18n::tf(
                    "Recorded example {} of “{}” will be removed.",
                    &[&(index + 1).to_string(), &sym],
                ));
                ui.add_space(space::M);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if w::danger_button(ui, icons::TRASH, "Delete").clicked() {
                        cx.send(Command::DeleteTemplate {
                            symbol: sym.clone(),
                            index,
                        });
                        close = true;
                    }
                    if w::button(ui, "", "Cancel").clicked() {
                        close = true;
                    }
                });
            },
        );
        if close {
            st.confirm_delete = None;
        }
    }
    record
}

#[allow(clippy::too_many_arguments)]
fn sample_card(
    ui: &mut egui::Ui,
    cx: &Ctx,
    st: &mut State,
    symbol: &str,
    sample: &Sample,
    i: usize,
    n_builtin: usize,
    width: f32,
) {
    let inner = width - 2.0 * space::S - 2.0;
    w::card_frame(ui)
        .inner_margin(egui::Margin::same(space::S as i8))
        .show(ui, |ui| {
            ui.set_width(inner);
            ui.vertical(|ui| {
                canvas::thumbnail(ui, inner, sample, Some(st.playback.t_us));
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    if i < n_builtin {
                        ui.label(w::faint_raw(
                            ui,
                            crate::i18n::tf("Built-in {}", &[&(i + 1).to_string()]),
                        ));
                    } else {
                        let user_index = i - n_builtin;
                        ui.label(w::caption_raw(
                            ui,
                            crate::i18n::tf("Recorded {}", &[&(user_index + 1).to_string()]),
                        ));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if w::icon_button_tone(
                                ui,
                                icons::TRASH,
                                "Delete this example",
                                Tone::Bad,
                            )
                            .clicked()
                            {
                                st.confirm_delete = Some((symbol.to_string(), user_index));
                            }
                            // A drawing that arrived as one example can be
                            // kept as its separate strokes, and two examples
                            // can be put together as one.
                            if sample.strokes.len() >= 2
                                && w::icon_button(
                                    ui,
                                    icons::ARROWS_LEFT_RIGHT,
                                    "Divide into its strokes",
                                )
                                .clicked()
                            {
                                cx.send(Command::SplitTemplate {
                                    symbol: symbol.to_string(),
                                    index: user_index,
                                });
                            }
                            let chosen = st.merge_selection.contains(&user_index);
                            if ui
                                .selectable_label(
                                    chosen,
                                    egui::RichText::new(icons::CHECK).font(fonts::icon(13.0)),
                                )
                                .on_hover_text(crate::i18n::tr("Choose to merge"))
                                .clicked()
                            {
                                if chosen {
                                    st.merge_selection.remove(&user_index);
                                } else {
                                    st.merge_selection.insert(user_index);
                                }
                            }
                        });
                    }
                });
            });
        });
}

fn history_view(ui: &mut egui::Ui, cx: &Ctx, st: &mut State) {
    let history = &cx.shared.history;
    if history.is_empty() {
        w::empty_state(
            ui,
            icons::CLOCK_COUNTER_CLOCKWISE,
            "Nothing recognized yet",
            "The latest gestures are kept here while GlyphPad runs.",
            |_| {},
        );
        return;
    }
    if st.entry.is_some_and(|i| i >= history.len()) {
        st.entry = None;
    }
    egui::Panel::left("history")
        .resizable(false)
        .exact_size(380.0_f32.min(ui.available_width() * 0.45))
        .frame(egui::Frame::new())
        .show_separator_line(false)
        .show(ui, |ui| {
            master_frame(ui, |ui| {
                for (i, e) in history.iter().enumerate().rev() {
                    let selected = st.entry == Some(i);
                    let title = format!(
                        "{}  {}",
                        e.time,
                        e.gesture
                            .clone()
                            .unwrap_or_else(|| crate::i18n::tr("no gesture").into())
                    );
                    let mut note = e.recognized.clone();
                    if let Some(a) = &e.app {
                        note = format!("{note}  ·  {a}");
                    }
                    let lead = if e.executed {
                        icons::CHECK
                    } else {
                        icons::PROHIBIT
                    };
                    if list_item(ui, selected, lead, true, &title, &note).clicked() {
                        st.entry = Some(i);
                        st.playback.restart();
                        st.assign_to = e.symbol.clone().unwrap_or_default();
                    }
                }
            });
        });

    egui::CentralPanel::default()
        .frame(egui::Frame::new().inner_margin(egui::Margin {
            left: space::L as i8,
            ..Default::default()
        }))
        .show(ui, |ui| {
            let Some(entry) = st.entry.and_then(|i| history.get(i)) else {
                w::empty_state(
                    ui,
                    icons::PLAY,
                    "Select a recognition",
                    "It is replayed here with its real timing.",
                    |_| {},
                );
                return;
            };
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .content_margin(egui::Margin::symmetric(2, 2))
                .show(ui, |ui| entry_view(ui, cx, st, entry));
        });
}

fn entry_view(ui: &mut egui::Ui, cx: &Ctx, st: &mut State, e: &HistoryEntry) {
    let p = pal(ui);
    let strokes = &e.trace.strokes;
    w::section(ui, "", "", |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(w::strong_raw(&e.recognized));
            if let Some(a) = &e.action {
                w::badge_raw(ui, a, if e.executed { Tone::Ok } else { Tone::Neutral });
            }
            if !e.executed {
                w::badge(ui, "not run", Tone::Neutral);
            }
        });
        let total = if strokes.is_empty() {
            canvas::duration_us(e.trace.fingers.values())
        } else {
            canvas::duration_us(strokes)
        };
        st.playback.controls(ui, total);
        st.playback.advance(ui.ctx(), total);
        ui.add_space(space::S);
        let width = ui.available_width().min(560.0);
        let (_, painter, pad) = Pad::new(ui, width, e.width_mm, e.height_mm);
        if strokes.is_empty() {
            canvas::draw_fingers(&painter, &pad, &e.trace.fingers, Some(st.playback.t_us));
            ui.label(w::caption(ui, "Each finger in its own color."));
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
            canvas::draw_polylines(&painter, &pad, &lines, Stroke::new(8.0, p.border_strong));
        }
        canvas::draw_strokes(&painter, &pad, strokes, Some(st.playback.t_us), true);
        ui.label(w::caption(
            ui,
            "The gray shape is the recognized symbol's first template, scaled onto the drawing.",
        ));
    });
    if strokes.is_empty() {
        return;
    }
    let sample = || Sample {
        strokes: strokes.clone(),
        width_mm: e.width_mm,
        height_mm: e.height_mm,
    };
    w::section(
        ui,
        "Use this drawing",
        "Useful when it was recognized as the wrong symbol.",
        |ui| {
            if let Some(symbol) = &e.symbol {
                w::row_raw(
                    ui,
                    &crate::i18n::tf("Example of “{}”", &[symbol]),
                    "",
                    |ui| {
                        if w::button(ui, icons::PLUS, "Add it").clicked() {
                            cx.send(Command::AddTemplate {
                                symbol: symbol.clone(),
                                sample: sample(),
                            });
                        }
                    },
                );
            }
            w::row(ui, "Example of another symbol", "", |ui| {
                ui.horizontal(|ui| {
                    let options: Vec<(String, std::borrow::Cow<'static, str>)> = cx
                        .shared
                        .effective
                        .keys()
                        .map(|k| (k.clone(), k.clone().into()))
                        .collect();
                    w::dropdown(ui, "assign", &mut st.assign_to, &options, 90.0);
                    w::text_field(ui, &mut st.assign_to, "or a new name", 130.0);
                    if w::button_enabled(ui, icons::PLUS, "Add", !st.assign_to.trim().is_empty())
                        .clicked()
                    {
                        cx.send(Command::AddTemplate {
                            symbol: st.assign_to.trim().to_string(),
                            sample: sample(),
                        });
                    }
                });
            });
        },
    );
}
