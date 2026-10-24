//! The Gestures page (SPEC §10.2): the list in priority order, with search,
//! filters, quick enabling, drag & drop reordering, duplicate and delete,
//! and the editor of the selected gesture.

use eframe::egui::{self, Color32, Id, RichText, Sense};
use glyphpad_core::config::{GestureDef, GestureKind};
use glyphpad_core::config_edit::EditedGesture;
use glyphpad_core::conflicts::Conflict;
use glyphpad_core::service::{Command, EngineMode};
use glyphpad_core::trigger::TriggerDef;

use crate::Ctx;
use crate::editor::Editor;
use crate::forms::{self, KINDS};
use crate::style;
use crate::wizard::Start;

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum StateFilter {
    #[default]
    All,
    Enabled,
    Disabled,
}

#[derive(Default)]
pub struct State {
    search: String,
    kind: Option<GestureKind>,
    /// `Some(None)`: only gestures for all profiles.
    profile: Option<Option<String>>,
    state: StateFilter,
    selected: Option<usize>,
    confirm_delete: Option<usize>,
    /// Conflicts of the draft, recomputed when it changes.
    conflicts: Option<(u64, Vec<Conflict>)>,
    profile_dialog: bool,
    new_profile: String,
    rename: Option<(String, String)>,
}

/// Short description of a gesture's trigger.
pub fn describe(def: &GestureDef) -> String {
    if def.kind == GestureKind::Sequence {
        let steps: Vec<String> = def
            .steps
            .iter()
            .flatten()
            .map(|t| {
                t.build()
                    .map(|t| t.describe())
                    .unwrap_or_else(|_| "?".into())
            })
            .collect();
        return steps.join(" → ");
    }
    def.trigger_def()
        .build()
        .map(|t| t.describe())
        .unwrap_or_else(|e| format!("incomplete: {}", e[0].1))
}

fn draft_hash(e: &Editor) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    format!("{:?}", e.draft.gestures).hash(&mut h);
    h.finish()
}

/// Returns how to start the wizard, if asked.
pub fn show(ui: &mut egui::Ui, cx: &Ctx, ed: &mut Editor, st: &mut State) -> Option<Start> {
    let mut start = None;
    let symbols: Vec<String> = cx.shared.effective.keys().cloned().collect();
    let profiles = ed.draft.profiles();

    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            style::page_header(
                ui,
                "Gestures",
                "In priority order: when two could fire, the first one wins.",
            );
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
            if ui
                .button(RichText::new("✚ Record a new gesture").strong())
                .clicked()
            {
                start = Some(Start::New);
            }
            if ui.button("+ Empty gesture").clicked() {
                let name = ed.unique_name("new-gesture");
                let def = GestureDef::new(
                    name,
                    TriggerDef {
                        kind: Some(GestureKind::Tap),
                        fingers: Some(3),
                        ..Default::default()
                    },
                    glyphpad_core::action::Action::Notify {
                        title: None,
                        message: "Hello".into(),
                    },
                );
                st.selected = Some(ed.add(def));
            }
            let mut test = cx.shared.mode == EngineMode::Test;
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

    // Conflicts of the draft (SPEC §13).
    let hash = draft_hash(ed);
    if st.conflicts.as_ref().is_none_or(|c| c.0 != hash)
        && let Some(config) = ed.draft_config()
    {
        let found = glyphpad_core::conflicts::find(&config, &cx.shared.effective);
        st.conflicts = Some((hash, found));
    }
    if let Some((_, conflicts)) = &st.conflicts
        && !conflicts.is_empty()
    {
        egui::Frame::new()
            .fill(style::WARN.gamma_multiply(0.12))
            .corner_radius(egui::CornerRadius::same(8))
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.colored_label(
                    style::WARN,
                    RichText::new(format!("⚠ {} possible conflict(s)", conflicts.len())).strong(),
                );
                for c in conflicts {
                    ui.label(format!("{}: {}", c.gestures.join(" / "), c.reason));
                }
            });
        ui.add_space(6.0);
    }

    filters(ui, ed, st, &profiles);
    ui.add_space(4.0);

    if st.selected.is_some_and(|i| i >= ed.draft.gestures.len()) {
        st.selected = None;
    }
    egui::Panel::left("gesture-list")
        .resizable(true)
        .default_size(440.0)
        .size_range(340.0..=640.0)
        .show(ui, |ui| list(ui, ed, st));
    egui::CentralPanel::default().show(ui, |ui| match st.selected {
        Some(i) => detail(ui, cx, ed, st, i, &symbols, &profiles),
        None => {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new("Select a gesture to edit it")
                        .weak()
                        .size(16.0),
                );
                ui.label(RichText::new("or record a new one").weak());
            });
        }
    });

    delete_dialog(ui.ctx(), ed, st);
    profiles_dialog(ui.ctx(), ed, st);
    start
}

fn filters(ui: &mut egui::Ui, ed: &mut Editor, st: &mut State, profiles: &[String]) {
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut st.search)
                .hint_text("🔍 Search")
                .desired_width(170.0),
        );
        egui::ComboBox::from_id_salt("filter-kind")
            .selected_text(st.kind.map_or("All types".into(), forms::kind_label))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut st.kind, None, "All types");
                for k in KINDS {
                    ui.selectable_value(&mut st.kind, Some(k), forms::kind_label(k));
                }
            });
        egui::ComboBox::from_id_salt("filter-profile")
            .selected_text(match &st.profile {
                None => "All profiles".to_string(),
                Some(None) => "No profile".into(),
                Some(Some(p)) => p.clone(),
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut st.profile, None, "All profiles");
                ui.selectable_value(&mut st.profile, Some(None), "No profile");
                for p in profiles {
                    ui.selectable_value(&mut st.profile, Some(Some(p.clone())), p);
                }
            });
        egui::ComboBox::from_id_salt("filter-state")
            .selected_text(match st.state {
                StateFilter::All => "Enabled and disabled",
                StateFilter::Enabled => "Enabled",
                StateFilter::Disabled => "Disabled",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut st.state, StateFilter::All, "Enabled and disabled");
                ui.selectable_value(&mut st.state, StateFilter::Enabled, "Enabled");
                ui.selectable_value(&mut st.state, StateFilter::Disabled, "Disabled");
            });
        ui.separator();
        ui.label("Active profile");
        let mut active = ed.draft.settings.active_profile.clone();
        egui::ComboBox::from_id_salt("active-profile")
            .selected_text(&active)
            .show_ui(ui, |ui| {
                for p in profiles {
                    ui.selectable_value(&mut active, p.clone(), p);
                }
            });
        ed.draft.settings.active_profile = active;
        if ui.button("Profiles…").clicked() {
            st.profile_dialog = true;
        }
    });
}

fn matches_filters(st: &State, def: &GestureDef) -> bool {
    let q = st.search.to_lowercase();
    (q.is_empty()
        || format!("{} {} {}", def.name, describe(def), def.action.kind())
            .to_lowercase()
            .contains(&q))
        && st.kind.is_none_or(|k| k == def.kind)
        && st.profile.as_ref().is_none_or(|p| *p == def.profile)
        && match st.state {
            StateFilter::All => true,
            StateFilter::Enabled => def.enabled,
            StateFilter::Disabled => !def.enabled,
        }
}

fn list(ui: &mut egui::Ui, ed: &mut Editor, st: &mut State) {
    let mut moved: Option<(usize, usize)> = None;
    let mut duplicate = None;
    egui::ScrollArea::vertical().show(ui, |ui| {
        let n = ed.draft.gestures.len();
        if n == 0 {
            ui.weak("No gestures yet.");
        }
        for i in 0..n {
            let def = &mut ed.draft.gestures[i].def;
            if !matches_filters(st, def) {
                continue;
            }
            let selected = st.selected == Some(i);
            let (_, dropped) = ui.dnd_drop_zone::<usize, _>(egui::Frame::new(), |ui| {
                let fill = if selected {
                    style::ACCENT.gamma_multiply(0.25)
                } else {
                    ui.visuals().faint_bg_color
                };
                egui::Frame::new()
                    .fill(fill)
                    .corner_radius(egui::CornerRadius::same(8))
                    .inner_margin(egui::Margin::symmetric(8, 6))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.dnd_drag_source(Id::unique(("gesture-drag", i)), i, |ui| {
                                ui.label(RichText::new("⠿").size(18.0).weak())
                                    .on_hover_text("Drag to change priority");
                            });
                            ui.checkbox(&mut def.enabled, "").on_hover_text("Enabled");
                            let text = ui.vertical(|ui| {
                                let name = RichText::new(&def.name).strong();
                                ui.label(if def.enabled { name } else { name.weak() });
                                ui.label(
                                    RichText::new(format!(
                                        "{}  →  {}",
                                        describe(def),
                                        def.action.kind()
                                    ))
                                    .small()
                                    .weak(),
                                );
                            });
                            if text.response.interact(Sense::click()).clicked() {
                                st.selected = Some(i);
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.small_button("🗑").on_hover_text("Delete").clicked() {
                                        st.confirm_delete = Some(i);
                                    }
                                    if ui.small_button("⧉").on_hover_text("Duplicate").clicked() {
                                        duplicate = Some(i);
                                    }
                                    if ui.small_button("✎").on_hover_text("Edit").clicked() {
                                        st.selected = Some(i);
                                    }
                                    if let Some(p) = &def.profile {
                                        forms::badge(ui, p, style::ACCENT);
                                    }
                                    if def.when.is_some() {
                                        forms::badge(ui, "per app", Color32::GRAY);
                                    }
                                },
                            );
                        });
                    });
            });
            if let Some(from) = dropped {
                moved = Some((*from, i));
            }
        }
    });
    if let Some((from, to)) = moved
        && from != to
        && from < ed.draft.gestures.len()
    {
        let g = ed.draft.gestures.remove(from);
        ed.draft.gestures.insert(to, g);
        st.selected = Some(to);
    }
    if let Some(i) = duplicate {
        duplicate_at(ed, st, i);
    }
}

fn duplicate_at(ed: &mut Editor, st: &mut State, i: usize) {
    let mut def = ed.draft.gestures[i].def.clone();
    def.name = ed.unique_name(&format!("{}-copy", def.name));
    ed.draft
        .gestures
        .insert(i + 1, EditedGesture { origin: None, def });
    st.selected = Some(i + 1);
}

fn detail(
    ui: &mut egui::Ui,
    cx: &Ctx,
    ed: &mut Editor,
    st: &mut State,
    i: usize,
    symbols: &[String],
    profiles: &[String],
) {
    let default_confidence = ed.draft.settings.default_confidence;
    let others: Vec<String> = ed
        .draft
        .gestures
        .iter()
        .enumerate()
        .filter(|(j, _)| *j != i)
        .map(|(_, g)| g.def.name.clone())
        .collect();
    let mut duplicate = false;
    {
        let def = &mut ed.draft.gestures[i].def;
        let id = Id::unique(("gesture-editor", i));
        egui::ScrollArea::vertical().show(ui, |ui| {
            style::card(ui, "Gesture", |ui| {
                ui.horizontal(|ui| {
                    ui.add_sized(
                        [110.0, 20.0],
                        egui::Label::new(RichText::new("Name").weak()),
                    );
                    ui.add(egui::TextEdit::singleline(&mut def.name).desired_width(240.0));
                    ui.checkbox(&mut def.enabled, "Enabled");
                });
                if def.name.trim().is_empty() {
                    ui.colored_label(style::BAD, "The name must not be empty");
                } else if others.contains(&def.name) {
                    ui.colored_label(style::BAD, "Another gesture has this name");
                }
            });
            ui.add_space(6.0);
            style::card(ui, "Trigger", |ui| {
                let mut t = def.trigger_def();
                forms::trigger_form(
                    ui,
                    id.with("trigger"),
                    &mut t,
                    symbols,
                    default_confidence,
                    true,
                );
                forms::set_trigger(def, &t);
                if def.kind == GestureKind::Sequence {
                    if def.steps.is_none() {
                        def.steps = Some(vec![
                            TriggerDef {
                                kind: Some(GestureKind::Tap),
                                fingers: Some(3),
                                ..Default::default()
                            },
                            TriggerDef {
                                kind: Some(GestureKind::Swipe),
                                fingers: Some(3),
                                direction: Some(glyphpad_core::config::Direction::Up),
                                ..Default::default()
                            },
                        ]);
                    }
                    forms::sequence_form(ui, id.with("steps"), def, symbols, default_confidence);
                } else {
                    def.steps = None;
                    def.within_ms = None;
                    if let Err(errors) = def.trigger_def().build() {
                        for (field, e) in errors {
                            ui.colored_label(style::BAD, format!("{field}: {e}"));
                        }
                    }
                }
                ui.weak(
                    "Distances, durations and speeds apply to all gestures: Settings → Advanced.",
                );
            });
            ui.add_space(6.0);
            style::card(ui, "Action", |ui| {
                forms::action_form(ui, cx, id.with("action"), &mut def.action, false)
            });
            ui.add_space(6.0);
            style::card(ui, "Conditions", |ui| {
                forms::conditions_form(ui, cx, id.with("when"), def, profiles)
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                duplicate = ui.button("Duplicate").clicked();
                if ui
                    .button(RichText::new("Delete").color(style::BAD))
                    .clicked()
                {
                    st.confirm_delete = Some(i);
                }
            });
        });
    }
    if duplicate {
        duplicate_at(ed, st, i);
    }
}

fn delete_dialog(ctx: &egui::Context, ed: &mut Editor, st: &mut State) {
    let Some(i) = st.confirm_delete else { return };
    let Some(name) = ed.draft.gestures.get(i).map(|g| g.def.name.clone()) else {
        st.confirm_delete = None;
        return;
    };
    egui::Modal::new(Id::unique("confirm-delete")).show(ctx, |ui| {
        ui.set_width(320.0);
        ui.heading("Delete gesture?");
        ui.label(format!("“{name}” will be removed when you save."));
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui
                .button(RichText::new("Delete").color(style::BAD))
                .clicked()
            {
                ed.draft.gestures.remove(i);
                st.selected = None;
                st.confirm_delete = None;
            }
            if ui.button("Cancel").clicked() {
                st.confirm_delete = None;
            }
        });
    });
}

fn profiles_dialog(ctx: &egui::Context, ed: &mut Editor, st: &mut State) {
    if !st.profile_dialog {
        return;
    }
    let profiles = ed.draft.profiles();
    egui::Modal::new(Id::unique("profiles")).show(ctx, |ui| {
        ui.set_width(400.0);
        ui.heading("Profiles");
        ui.weak("Gestures with a profile only work while it is active.");
        ui.add_space(6.0);
        let mut delete = None;
        for p in &profiles {
            ui.horizontal(|ui| {
                let used = ed
                    .draft
                    .gestures
                    .iter()
                    .filter(|g| g.def.profile.as_ref() == Some(p))
                    .count();
                ui.label(RichText::new(p).strong());
                ui.weak(format!("{used} gesture(s)"));
                if *p == ed.draft.settings.active_profile {
                    forms::badge(ui, "active", style::OK);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button("Delete")
                        .on_hover_text("Its gestures then work in all profiles")
                        .clicked()
                    {
                        delete = Some(p.clone());
                    }
                    if ui.small_button("Rename").clicked() {
                        st.rename = Some((p.clone(), p.clone()));
                    }
                });
            });
        }
        if let Some(p) = delete {
            for g in &mut ed.draft.gestures {
                if g.def.profile.as_ref() == Some(&p) {
                    g.def.profile = None;
                }
            }
            ed.draft.settings.profiles.retain(|x| *x != p);
            if ed.draft.settings.active_profile == p {
                ed.draft.settings.active_profile = "default".into();
            }
        }
        if let Some((old, new)) = &mut st.rename {
            let mut done = false;
            ui.horizontal(|ui| {
                ui.label(format!("Rename “{old}” to"));
                ui.text_edit_singleline(new);
                done = ui.button("OK").clicked() && !new.trim().is_empty();
            });
            if done {
                let (old, new) = (old.clone(), new.trim().to_string());
                for g in &mut ed.draft.gestures {
                    if g.def.profile.as_ref() == Some(&old) {
                        g.def.profile = Some(new.clone());
                    }
                }
                for p in &mut ed.draft.settings.profiles {
                    if *p == old {
                        *p = new.clone();
                    }
                }
                if ed.draft.settings.active_profile == old {
                    ed.draft.settings.active_profile = new;
                }
                st.rename = None;
            }
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut st.new_profile).hint_text("new profile"));
            if ui.button("Create").clicked() {
                let p = st.new_profile.trim().to_string();
                if !p.is_empty() && !profiles.contains(&p) {
                    ed.draft.settings.profiles.push(p);
                }
                st.new_profile.clear();
            }
        });
        ui.add_space(6.0);
        if ui.button("Close").clicked() {
            st.profile_dialog = false;
            st.rename = None;
        }
    });
}
