//! Live debug tab (SPEC §10.1): fingers in real time, the stroke being
//! drawn, the recognizer's candidates and the device information.

use eframe::egui::{self, Color32, Stroke};
use glyphpad_core::service::{Command, EngineMode};

use crate::Ctx;
use crate::canvas::{self, Pad};

pub fn show(ui: &mut egui::Ui, cx: &Ctx) {
    let s = cx.shared;
    ui.horizontal(|ui| {
        ui.heading("Live debug");
        let mut test = s.mode == EngineMode::Test;
        if ui
            .checkbox(&mut test, "Test mode (recognize without running actions)")
            .changed()
        {
            cx.send(Command::SetMode(if test {
                EngineMode::Test
            } else {
                EngineMode::Normal
            }));
        }
    });
    ui.separator();
    ui.columns(2, |cols| {
        let ui = &mut cols[0];
        let (w_mm, h_mm) = s.pad_size_mm();
        let width = ui.available_width().min(560.0);
        let (_, painter, pad) = Pad::new(ui, width, w_mm, h_mm);
        canvas::draw_polylines(
            &painter,
            &pad,
            &s.drawing,
            Stroke::new(3.0, Color32::from_rgb(90, 170, 255)),
        );
        canvas::draw_contacts(&painter, &pad, &s.contacts);
        ui.add_space(6.0);
        egui::Grid::new("contacts").striped(true).show(ui, |ui| {
            for h in ["id", "x mm", "y mm", "x", "y", "tip", "palm"] {
                ui.strong(h);
            }
            ui.end_row();
            for c in &s.contacts {
                ui.label(c.id.to_string());
                ui.label(format!("{:.1}", c.x_mm));
                ui.label(format!("{:.1}", c.y_mm));
                ui.label(format!("{:.3}", c.x_norm));
                ui.label(format!("{:.3}", c.y_norm));
                ui.label(if c.tip { "down" } else { "up" });
                ui.label(if c.confident { "" } else { "palm" });
                ui.end_row();
            }
        });

        let ui = &mut cols[1];
        ui.strong("Last drawing: candidates");
        if s.last_candidates.is_empty() {
            ui.weak("Nothing drawn yet.");
        }
        for c in &s.last_candidates {
            let threshold = s.config.settings.default_confidence;
            ui.add(
                egui::ProgressBar::new(c.score)
                    .text(format!("{}  {:.2}", c.symbol, c.score))
                    .fill(if c.score >= threshold {
                        Color32::from_rgb(70, 150, 90)
                    } else {
                        Color32::from_rgb(120, 110, 60)
                    }),
            );
        }
        ui.add_space(8.0);
        ui.strong("Recent recognitions");
        for e in s.history.iter().rev().take(8) {
            ui.label(format!(
                "{}  {}{}",
                e.time,
                e.gesture.as_deref().unwrap_or("—"),
                if e.executed { "" } else { "  (not run)" }
            ))
            .on_hover_text(&e.recognized);
        }
        ui.add_space(8.0);
        ui.strong("Devices");
        for d in &s.devices {
            ui.label(format!(
                "{}  {:04X}:{:04X}\n{:.1} × {:.1} mm, up to {} contacts",
                d.name, d.vendor_id, d.product_id, d.width_mm, d.height_mm, d.max_contacts
            ));
        }
        if s.devices.is_empty() {
            ui.weak("No Precision Touchpad connected.");
        }
    });
}
