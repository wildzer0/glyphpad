//! Exporting and importing gesture packs (SPEC §13): choose what goes in,
//! or preview what comes in and decide, gesture by gesture, what to do
//! with names that already exist.

use std::fs::File;

use eframe::egui::{self, Align, Layout};
use glyphpad_core::config::GestureDef;
use glyphpad_core::pack::{self, Pack};
use glyphpad_core::service::Command;

use crate::editor::{Editor, Slot};
use crate::theme::space;
use crate::widgets::{self as w, Tone};
use crate::{Ctx, icons};

#[derive(Default)]
pub struct State {
    export: Option<Export>,
    import: Option<Import>,
    /// Something went wrong reading or writing a file.
    problem: Option<String>,
}

struct Export {
    name: String,
    /// Gesture names and whether they go in.
    chosen: Vec<(String, bool)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Choice {
    Add,
    Skip,
    Replace,
    KeepBoth,
}

struct Import {
    pack: Pack,
    /// One per gesture of the pack.
    choices: Vec<Choice>,
    /// Symbols whose examples are brought in.
    symbols: Vec<(String, bool)>,
}

impl State {
    /// The "Share" menu of the Gestures page.
    pub fn menu(&mut self, ui: &mut egui::Ui, ed: &Editor) {
        let button = w::button(ui, icons::EXPORT, "Share");
        egui::Popup::menu(&button).show(|ui| {
            ui.set_min_width(190.0);
            if ui
                .button(format!("{}  Export gestures…", icons::UPLOAD_SIMPLE))
                .clicked()
            {
                self.export = Some(Export {
                    name: "My gestures".into(),
                    chosen: ed
                        .draft
                        .gestures
                        .iter()
                        .map(|g| (g.def.name.clone(), true))
                        .collect(),
                });
            }
            if ui
                .button(format!("{}  Import gestures…", icons::DOWNLOAD_SIMPLE))
                .clicked()
            {
                self.pick_import(ed);
            }
        });
    }

    fn pick_import(&mut self, ed: &Editor) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("GlyphPad pack", &[pack::EXTENSION])
            .pick_file()
        else {
            return;
        };
        match File::open(&path)
            .map_err(|e| e.to_string())
            .and_then(|f| pack::read(f).map_err(|e| e.to_string()))
        {
            Ok(pack) => {
                let taken: Vec<&str> = ed
                    .draft
                    .gestures
                    .iter()
                    .map(|g| g.def.name.as_str())
                    .collect();
                let choices = pack
                    .gestures
                    .iter()
                    .map(|g| {
                        if taken.contains(&g.name.as_str()) {
                            Choice::KeepBoth
                        } else {
                            Choice::Add
                        }
                    })
                    .collect();
                let symbols = pack.templates.keys().map(|s| (s.clone(), true)).collect();
                self.import = Some(Import {
                    pack,
                    choices,
                    symbols,
                });
            }
            Err(e) => self.problem = Some(format!("{}: {e}", path.display())),
        }
    }

    pub fn dialogs(&mut self, ctx: &egui::Context, ed: &mut Editor, cx: &Ctx) {
        self.export_dialog(ctx, ed, cx);
        self.import_dialog(ctx, ed, cx);
        if let Some(why) = self.problem.clone() {
            let mut close = false;
            w::dialog(ctx, "pack-problem", "That did not work", 420.0, |ui| {
                w::error_text(ui, &why);
                ui.add_space(space::M);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if w::button(ui, "", "Close").clicked() {
                        close = true;
                    }
                });
            });
            if close {
                self.problem = None;
            }
        }
    }

    fn export_dialog(&mut self, ctx: &egui::Context, ed: &Editor, cx: &Ctx) {
        let Some(export) = &mut self.export else {
            return;
        };
        let mut close = false;
        let mut save = false;
        w::dialog(ctx, "export-pack", "Export gestures", 460.0, |ui| {
            ui.label(w::dim(
                ui,
                "The file holds the chosen gestures and the examples of the symbols they use. Share it, or keep it as a backup.",
            ));
            ui.add_space(space::S);
            w::text_field(
                ui,
                &mut export.name,
                "Name of the pack",
                ui.available_width(),
            );
            ui.add_space(space::S);
            egui::ScrollArea::vertical()
                .max_height(260.0)
                .show(ui, |ui| {
                    for (name, on) in &mut export.chosen {
                        ui.horizontal(|ui| {
                            w::switch(ui, on);
                            ui.label(name.as_str());
                        });
                    }
                });
            ui.add_space(space::M);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let any = export.chosen.iter().any(|(_, on)| *on);
                if w::primary_button_enabled(ui, icons::UPLOAD_SIMPLE, "Save as…", any).clicked()
                {
                    save = true;
                }
                if w::button(ui, "", "Cancel").clicked() {
                    close = true;
                }
            });
        });
        if save {
            let defs: Vec<GestureDef> = ed
                .draft
                .gestures
                .iter()
                .filter(|g| export.chosen.iter().any(|(n, on)| *on && *n == g.def.name))
                .map(|g| g.def.clone())
                .collect();
            let file_name = format!(
                "{}.{}",
                export.name.trim().replace(['/', '\\'], "-"),
                pack::EXTENSION
            );
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("GlyphPad pack", &[pack::EXTENSION])
                .set_file_name(file_name)
                .save_file()
            {
                let result = File::create(&path)
                    .map_err(|e| e.to_string())
                    .and_then(|f| {
                        pack::write(
                            f,
                            export.name.trim(),
                            cx.version,
                            &defs,
                            &cx.shared.templates.symbols,
                        )
                        .map_err(|e| e.to_string())
                    });
                match result {
                    Ok(()) => close = true,
                    Err(e) => {
                        self.problem = Some(format!("{}: {e}", path.display()));
                        close = true;
                    }
                }
            }
        }
        if close {
            self.export = None;
        }
    }

    fn import_dialog(&mut self, ctx: &egui::Context, ed: &mut Editor, cx: &Ctx) {
        let Some(import) = &mut self.import else {
            return;
        };
        let mut close = false;
        let mut apply = false;
        let existing: Vec<(String, String)> = ed
            .draft
            .gestures
            .iter()
            .map(|g| {
                (
                    g.def.name.clone(),
                    super::gestures::describe(&g.def) + &g.def.profile.clone().unwrap_or_default(),
                )
            })
            .collect();
        w::dialog(
            ctx,
            "import-pack",
            &format!("Import “{}”", import.pack.name),
            560.0,
            |ui| {
                ui.label(w::dim(
                    ui,
                    "Choose what to do with each gesture. Nothing changes until you press Import.",
                ));
                ui.add_space(space::S);
                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .show(ui, |ui| {
                        for (g, choice) in import.pack.gestures.iter().zip(&mut import.choices) {
                            let same_name = existing.iter().any(|(n, _)| *n == g.name);
                            let key = super::gestures::describe(g)
                                + &g.profile.clone().unwrap_or_default();
                            let same_move =
                                existing.iter().find(|(n, k)| *k == key && *n != g.name);
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.set_width(250.0);
                                    ui.label(w::strong(&g.name));
                                    ui.label(w::caption(ui, super::gestures::describe(g)));
                                });
                                let options: Vec<(Choice, String)> = if same_name {
                                    vec![
                                        (Choice::KeepBoth, "Keep both".into()),
                                        (Choice::Replace, "Replace mine".into()),
                                        (Choice::Skip, "Skip".into()),
                                    ]
                                } else {
                                    vec![(Choice::Add, "Add".into()), (Choice::Skip, "Skip".into())]
                                };
                                w::dropdown(
                                    ui,
                                    ("import-choice", &g.name),
                                    choice,
                                    &options,
                                    140.0,
                                );
                                if same_name {
                                    w::badge(ui, "name exists", Tone::Warn);
                                }
                            });
                            if let Some((other, _)) = same_move {
                                w::warn_text(
                                    ui,
                                    &format!("Same movement as your “{other}”: both would fire."),
                                );
                            }
                            ui.add_space(space::XS);
                        }
                        if !import.symbols.is_empty() {
                            ui.add_space(space::S);
                            ui.label(w::strong("Symbol examples"));
                            for (symbol, on) in &mut import.symbols {
                                let count = import.pack.templates.get(symbol).map_or(0, Vec::len);
                                let have = cx.shared.effective.contains_key(symbol);
                                ui.horizontal(|ui| {
                                    w::switch(ui, on);
                                    ui.label(format!(
                                        "{symbol}: {count} example{}",
                                        if count == 1 { "" } else { "s" }
                                    ));
                                    if have {
                                        ui.label(w::faint(ui, "added to the ones you have"));
                                    }
                                });
                            }
                        }
                    });
                ui.add_space(space::M);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let any = import.choices.iter().any(|c| *c != Choice::Skip)
                        || import.symbols.iter().any(|(_, on)| *on);
                    if w::primary_button_enabled(ui, icons::DOWNLOAD_SIMPLE, "Import", any)
                        .clicked()
                    {
                        apply = true;
                    }
                    if w::button(ui, "", "Cancel").clicked() {
                        close = true;
                    }
                });
            },
        );
        if apply {
            for (def, choice) in import.pack.gestures.iter().zip(&import.choices) {
                let mut def = def.clone();
                match choice {
                    Choice::Skip => {}
                    Choice::Add => {
                        ed.add(def);
                    }
                    Choice::KeepBoth => {
                        def.name = ed.unique_name(&format!("{}-imported", def.name));
                        ed.add(def);
                    }
                    Choice::Replace => {
                        if let Some(Slot { def: mine, .. }) = ed
                            .draft
                            .gestures
                            .iter_mut()
                            .find(|g| g.def.name == def.name)
                        {
                            *mine = def;
                        }
                    }
                }
            }
            for (symbol, on) in &import.symbols {
                if !*on {
                    continue;
                }
                for sample in import.pack.templates.get(symbol).into_iter().flatten() {
                    cx.send(Command::AddTemplate {
                        symbol: symbol.clone(),
                        sample: sample.clone(),
                    });
                }
            }
            close = true;
        }
        if close {
            self.import = None;
        }
    }
}
