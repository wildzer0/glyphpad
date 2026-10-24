//! Live debug page (SPEC §10.1): fingers in real time, the stroke being
//! drawn, the recognizer's candidates and the device information.

use eframe::egui::{self, Color32, RichText, Stroke};
use glyphpad_core::service::{Command, EngineMode};

use crate::Ctx;
use crate::canvas::{self, Pad};
use crate::style;

pub fn show(ui: &mut egui::Ui, cx: &Ctx) {
    let s = cx.shared;
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            style::page_header(
                ui,
                "Live debug",
                "What the touchpad reports and what GlyphPad recognizes.",
            )
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
            let mut test = s.mode == EngineMode::Test;
            if ui
                .toggle_value(&mut test, "Test mode")
                .on_hover_text("Recognize gestures without running their actions")
                .changed()
            {
                cx.send(Command::SetMode(if test {
                    EngineMode::Test
                } else {
                    EngineMode::Normal
                }));
            }
        });
    });
    ui.columns(2, |cols| {
        let ui = &mut cols[0];
        style::card(ui, "Touchpad", |ui| {
            let (w_mm, h_mm) = s.pad_size_mm();
            let width = ui.available_width().min(560.0);
            let (_, painter, pad) = Pad::new(ui, width, w_mm, h_mm);
            canvas::draw_polylines(&painter, &pad, &s.drawing, Stroke::new(3.0, style::ACCENT));
            canvas::draw_contacts(&painter, &pad, &s.contacts);
            ui.add_space(4.0);
            egui::Grid::new("contacts")
                .striped(true)
                .num_columns(6)
                .show(ui, |ui| {
                    for h in ["id", "x mm", "y mm", "x", "y", "state"] {
                        ui.label(RichText::new(h).weak());
                    }
                    ui.end_row();
                    for c in &s.contacts {
                        ui.monospace(c.id.to_string());
                        ui.monospace(format!("{:6.1}", c.x_mm));
                        ui.monospace(format!("{:6.1}", c.y_mm));
                        ui.monospace(format!("{:.3}", c.x_norm));
                        ui.monospace(format!("{:.3}", c.y_norm));
                        ui.label(match (c.tip, c.confident) {
                            (false, _) => "lifting",
                            (true, false) => "palm",
                            (true, true) => "down",
                        });
                        ui.end_row();
                    }
                });
        });

        let ui = &mut cols[1];
        style::card(ui, "Last drawing", |ui| {
            if s.last_candidates.is_empty() {
                ui.weak("Nothing drawn yet.");
            }
            let threshold = s.config.settings.default_confidence;
            for c in &s.last_candidates {
                ui.add(
                    egui::ProgressBar::new(c.score)
                        .text(format!("{}   {:.2}", c.symbol, c.score))
                        .fill(if c.score >= threshold {
                            style::OK.gamma_multiply(0.6)
                        } else {
                            style::WARN.gamma_multiply(0.45)
                        }),
                );
            }
        });
        ui.add_space(8.0);
        style::card(ui, "Recent recognitions", |ui| {
            if s.history.is_empty() {
                ui.weak("None yet.");
            }
            for e in s.history.iter().rev().take(8) {
                ui.horizontal(|ui| {
                    ui.monospace(&e.time);
                    ui.label(e.gesture.as_deref().unwrap_or("—"));
                    if !e.executed {
                        ui.colored_label(Color32::GRAY, "not run");
                    }
                })
                .response
                .on_hover_text(&e.recognized);
            }
        });
        ui.add_space(8.0);
        style::card(ui, "Devices", |ui| {
            if s.devices.is_empty() {
                ui.weak("No Precision Touchpad connected.");
            }
            for d in &s.devices {
                ui.label(RichText::new(&d.name).strong());
                ui.weak(format!(
                    "{:04X}:{:04X}  ·  {:.1} × {:.1} mm  ·  up to {} contacts",
                    d.vendor_id, d.product_id, d.width_mm, d.height_mm, d.max_contacts
                ));
            }
        });
    });
}
