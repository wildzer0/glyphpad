//! The Gestures tab: the configured gestures and the entry point of the
//! recording wizard. The full editor comes in M5 (SPEC §10.2).

use eframe::egui;
use glyphpad_core::config::Gesture;
use glyphpad_core::service::{Command, EngineMode};

use crate::Ctx;
use crate::wizard::Start;

#[derive(Default)]
pub struct State {
    filter: String,
}

pub fn trigger(g: &Gesture) -> String {
    match &g.trigger {
        Some(t) => t.describe(),
        None => {
            let steps: Vec<String> = g.steps.iter().map(|t| t.describe()).collect();
            format!("sequence: {}", steps.join(", then "))
        }
    }
}

fn conditions(g: &Gesture) -> String {
    let mut parts = Vec::new();
    if let Some(p) = &g.def.profile {
        parts.push(format!("profile {p}"));
    }
    if let Some(w) = &g.def.when {
        if let Some(a) = &w.app {
            parts.push(a.to_vec().join(", "));
        }
        if let Some(t) = &w.title {
            parts.push(format!("title ~ {t}"));
        }
    }
    parts.join("; ")
}

/// Returns how to start the wizard if asked.
pub fn show(ui: &mut egui::Ui, cx: &Ctx, st: &mut State) -> Option<Start> {
    let mut start = None;
    ui.horizontal(|ui| {
        ui.heading("Gestures");
        if ui.button("➕ New gesture").clicked() {
            start = Some(Start::New);
        }
        let mut test = cx.shared.mode == EngineMode::Test;
        if ui
            .checkbox(&mut test, "Test mode")
            .on_hover_text("Recognize gestures without running their actions")
            .changed()
        {
            cx.send(Command::SetMode(if test {
                EngineMode::Test
            } else {
                EngineMode::Normal
            }));
        }
        if ui
            .button("⟳ Reload")
            .on_hover_text("Read glyphpad.toml and templates again")
            .clicked()
        {
            cx.send(Command::Reload);
        }
        ui.add(
            egui::TextEdit::singleline(&mut st.filter)
                .hint_text("Search…")
                .desired_width(160.0),
        );
    });
    ui.weak(
        "Gestures are listed in priority order: when two could fire, the first wins. Editing, \
         reordering and deleting from here arrives in the next version; until then edit \
         glyphpad.toml and press Reload.",
    );
    ui.separator();

    let filter = st.filter.to_lowercase();
    egui::ScrollArea::vertical().show(ui, |ui| {
        egui::Grid::new("gesture-list")
            .striped(true)
            .num_columns(6)
            .spacing([16.0, 6.0])
            .show(ui, |ui| {
                for h in ["", "Name", "Trigger", "Action", "Conditions", "Enabled"] {
                    ui.strong(h);
                }
                ui.end_row();
                for (i, g) in cx.shared.config.gestures.iter().enumerate() {
                    let row = format!("{} {} {}", g.def.name, trigger(g), g.def.action.kind())
                        .to_lowercase();
                    if !filter.is_empty() && !row.contains(&filter) {
                        continue;
                    }
                    ui.weak((i + 1).to_string());
                    ui.label(&g.def.name)
                        .on_hover_text(format!("line {} of the configuration", g.line));
                    ui.label(trigger(g));
                    ui.label(g.def.action.kind());
                    ui.label(conditions(g));
                    ui.label(if g.def.enabled { "yes" } else { "no" });
                    ui.end_row();
                }
            });
        if cx.shared.config.gestures.is_empty() {
            ui.label("No gestures yet. Create one with “New gesture”.");
        }
    });
    start
}
