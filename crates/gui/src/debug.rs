//! Live debug page (SPEC §10.1): fingers in real time, the stroke being
//! drawn, the recognizer's candidates and the device information.

use eframe::egui::{self, Align2, CornerRadius, Sense, Stroke, Vec2};
use glyphpad_core::service::EngineMode;

use crate::canvas::{self, Pad};
use crate::theme::{fonts, pal, size, space};
use crate::widgets::{self as w, Tone};
use crate::{Ctx, icons};

pub fn show(ui: &mut egui::Ui, cx: &Ctx) {
    let s = cx.shared;
    w::page_header(
        ui,
        "Live debug",
        "What the touchpad reports and what GlyphPad recognizes.",
        |ui| {
            if s.mode == EngineMode::Test {
                w::badge(ui, "Test mode: actions do not run", Tone::Warn);
            }
        },
    );
    w::scroll_page(ui, "debug", |ui| {
        let wide = ui.available_width() > 900.0;
        let left = if wide {
            (ui.available_width() * 0.58).min(640.0)
        } else {
            ui.available_width()
        };
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(left);
                touchpad(ui, cx, left);
                if !wide {
                    side(ui, cx);
                }
            });
            if wide {
                ui.add_space(space::L);
                ui.vertical(|ui| side(ui, cx));
            }
        });
    });
}

fn touchpad(ui: &mut egui::Ui, cx: &Ctx, width: f32) {
    let s = cx.shared;
    let p = pal(ui);
    w::section(ui, "Touchpad", "", |ui| {
        let (w_mm, h_mm) = s.pad_size_mm();
        let pad_w = (width - 2.0 * space::XL).min(560.0);
        let (_, painter, pad) = Pad::new(ui, pad_w, w_mm, h_mm);
        canvas::draw_polylines(&painter, &pad, &s.drawing, Stroke::new(3.5, p.accent));
        canvas::draw_contacts(&painter, &pad, &s.contacts);
        if s.contacts.iter().all(|c| !c.tip) && s.drawing.is_empty() {
            painter.text(
                pad.rect.center(),
                Align2::CENTER_CENTER,
                "Touch the touchpad",
                fonts::text(size::BODY),
                p.text_faint,
            );
        }
        ui.add_space(space::S);
        egui::Grid::new("contacts")
            .num_columns(6)
            .spacing(Vec2::new(18.0, 4.0))
            .show(ui, |ui| {
                for h in ["id", "x mm", "y mm", "x", "y", "state"] {
                    ui.label(w::faint(ui, h));
                }
                ui.end_row();
                for c in &s.contacts {
                    ui.label(egui::RichText::new(c.id.to_string()).font(fonts::mono(12.5)));
                    ui.label(
                        egui::RichText::new(format!("{:6.1}", c.x_mm)).font(fonts::mono(12.5)),
                    );
                    ui.label(
                        egui::RichText::new(format!("{:6.1}", c.y_mm)).font(fonts::mono(12.5)),
                    );
                    ui.label(
                        egui::RichText::new(format!("{:.3}", c.x_norm)).font(fonts::mono(12.5)),
                    );
                    ui.label(
                        egui::RichText::new(format!("{:.3}", c.y_norm)).font(fonts::mono(12.5)),
                    );
                    match (c.tip, c.confident) {
                        (false, _) => w::badge(ui, "lifting", Tone::Neutral),
                        (true, false) => w::badge(ui, "palm", Tone::Warn),
                        (true, true) => w::badge(ui, "down", Tone::Ok),
                    };
                    ui.end_row();
                }
            });
        if s.contacts.is_empty() {
            ui.label(w::faint(ui, "No fingers on the touchpad."));
        }
    });
}

/// A score as a bar with the threshold marked.
fn score_bar(ui: &mut egui::Ui, name: &str, score: f32, threshold: f32) {
    let p = pal(ui);
    let hit = score >= threshold;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 28.0), Sense::hover());
    let label_w = 64.0;
    let value_w = 44.0;
    ui.painter().text(
        egui::pos2(rect.left(), rect.center().y),
        Align2::LEFT_CENTER,
        name,
        fonts::strong(size::BODY),
        p.text,
    );
    let track = egui::Rect::from_min_max(
        egui::pos2(rect.left() + label_w, rect.center().y - 5.0),
        egui::pos2(rect.right() - value_w, rect.center().y + 5.0),
    );
    ui.painter()
        .rect_filled(track, CornerRadius::same(5), p.raised);
    let mut fill = track;
    fill.set_width(track.width() * score.clamp(0.0, 1.0));
    ui.painter()
        .rect_filled(fill, CornerRadius::same(5), if hit { p.ok } else { p.warn });
    let x = track.left() + track.width() * threshold.clamp(0.0, 1.0);
    ui.painter().line_segment(
        [
            egui::pos2(x, track.top() - 3.0),
            egui::pos2(x, track.bottom() + 3.0),
        ],
        Stroke::new(1.5, p.text_dim),
    );
    ui.painter().text(
        egui::pos2(rect.right(), rect.center().y),
        Align2::RIGHT_CENTER,
        format!("{score:.2}"),
        fonts::mono(12.5),
        if hit { p.ok } else { p.text_dim },
    );
}

fn side(ui: &mut egui::Ui, cx: &Ctx) {
    let s = cx.shared;
    w::section(
        ui,
        "Last drawing",
        "The marker shows the minimum score a gesture needs.",
        |ui| {
            if s.last_candidates.is_empty() {
                ui.label(w::dim(ui, "Nothing drawn yet."));
            }
            let threshold = s.config.settings.default_confidence;
            for c in &s.last_candidates {
                score_bar(ui, &c.symbol, c.score, threshold);
            }
        },
    );
    w::section(ui, "Recent recognitions", "", |ui| {
        if s.history.is_empty() {
            ui.label(w::dim(ui, "None yet."));
        }
        for e in s.history.iter().rev().take(8) {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(&e.time)
                        .font(fonts::mono(12.5))
                        .color(pal(ui).text_dim),
                );
                ui.label(w::strong(e.gesture.as_deref().unwrap_or("no gesture")));
                if !e.executed {
                    w::badge(ui, "not run", Tone::Neutral);
                }
            })
            .response
            .on_hover_text(&e.recognized);
        }
    });
    w::section(ui, "Devices", "", |ui| {
        if s.devices.is_empty() {
            w::banner(
                ui,
                Tone::Warn,
                "No Precision Touchpad connected",
                "GlyphPad reconnects by itself when one appears.",
                |_| {},
            );
        }
        for d in &s.devices {
            ui.horizontal(|ui| {
                ui.label(w::icon_text(icons::MOUSE_SIMPLE, 18.0, pal(ui).text_dim));
                ui.vertical(|ui| {
                    ui.label(w::strong(&d.name));
                    ui.label(w::caption(
                        ui,
                        format!(
                            "{:04X}:{:04X}  ·  {:.1} × {:.1} mm  ·  up to {} contacts",
                            d.vendor_id, d.product_id, d.width_mm, d.height_mm, d.max_contacts
                        ),
                    ));
                });
            });
        }
    });
}
