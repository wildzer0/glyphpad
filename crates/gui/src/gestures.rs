//! The Gestures page (SPEC §10.2): the list in priority order on the left,
//! the editor of the selected gesture on the right. Everything is saved as
//! you go, so the page has no modes: select, edit, done.

use eframe::egui::{self, Align, Color32, CornerRadius, Id, Layout, Rect, Sense, Stroke, Vec2};
use glyphpad_core::config::{GestureDef, GestureKind};
use glyphpad_core::conflicts::Conflict;
use glyphpad_core::trigger::TriggerDef;

use crate::Ctx;
use crate::editor::{Editor, Slot};
use crate::forms::{self, KINDS};
use crate::icons;
use crate::theme::{fonts, pal, radius, size, space};
use crate::widgets::{self as w, Tone};
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
    state: StateFilter,
    selected: Option<u64>,
    confirm_delete: Option<u64>,
    /// Conflicts of the draft, recomputed when it changes.
    conflicts: Option<(u64, Vec<Conflict>)>,
    profile_dialog: bool,
    new_profile: String,
    rename: Option<(String, String)>,
    /// The row being dragged to a new priority.
    dragging: Option<u64>,
    packs: crate::packs::State,
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

    w::page_header(
        ui,
        "Gestures",
        "When two could fire at once, the one higher in the list wins.",
        |ui| {
            if w::primary_button(ui, icons::RECORD, "Record a gesture").clicked() {
                start = Some(Start::New);
            }
            st.packs.menu(ui, ed);
            if w::button(ui, icons::PLUS, "Add manually").clicked() {
                let def = GestureDef::new(
                    ed.unique_name("new-gesture"),
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
        },
    );

    // Conflicts of the draft (SPEC §13).
    let hash = draft_hash(ed);
    if st.conflicts.as_ref().is_none_or(|c| c.0 != hash)
        && let Some(config) = ed.draft_config()
    {
        let found = glyphpad_core::conflicts::find(&config, &cx.shared.effective);
        st.conflicts = Some((hash, found));
    }

    if st.selected.is_some_and(|u| ed.index_of(u).is_none()) {
        st.selected = None;
    }
    let total_width = ui.available_width();
    egui::Panel::left("gesture-list")
        .resizable(true)
        .frame(egui::Frame::new())
        .default_size((total_width * 0.40).clamp(320.0, 420.0))
        .size_range(300.0..=(total_width * 0.55).max(300.0))
        .show_separator_line(false)
        .show(ui, |ui| {
            ui.add_space(2.0);
            list_panel(ui, ed, st, &profiles);
        });
    egui::CentralPanel::default()
        .frame(egui::Frame::new().inner_margin(egui::Margin {
            left: space::L as i8,
            ..Default::default()
        }))
        .show(ui, |ui| match st.selected.and_then(|u| ed.index_of(u)) {
            Some(i) => detail(ui, cx, ed, st, i, &symbols, &profiles),
            None => {
                let empty = ed.draft.gestures.is_empty();
                w::empty_state(
                    ui,
                    icons::HAND_TAP,
                    if empty {
                        "No gestures yet"
                    } else {
                        "Select a gesture to edit it"
                    },
                    if empty {
                        "Record one by doing it on the touchpad, or add one by hand."
                    } else {
                        "Changes are saved as you make them."
                    },
                    |_| {},
                );
            }
        });

    delete_dialog(ui.ctx(), ed, st);
    profiles_dialog(ui.ctx(), ed, st);
    st.packs.dialogs(ui.ctx(), ed, cx);
    start
}

fn matches_filters(st: &State, def: &GestureDef) -> bool {
    let q = st.search.to_lowercase();
    (q.is_empty()
        || format!("{} {} {}", def.name, describe(def), def.action.kind())
            .to_lowercase()
            .contains(&q))
        && st.kind.is_none_or(|k| k == def.kind)
        && match st.state {
            StateFilter::All => true,
            StateFilter::Enabled => def.enabled,
            StateFilter::Disabled => !def.enabled,
        }
}

fn filtering(st: &State) -> bool {
    !st.search.is_empty() || st.kind.is_some() || st.state != StateFilter::All
}

fn list_panel(ui: &mut egui::Ui, ed: &mut Editor, st: &mut State, profiles: &[String]) {
    w::search_field(ui, &mut st.search, "Search gestures");
    ui.add_space(space::XS);
    ui.horizontal(|ui| {
        w::segmented(
            ui,
            &mut st.state,
            &[
                (StateFilter::All, "All"),
                (StateFilter::Enabled, "On"),
                (StateFilter::Disabled, "Off"),
            ],
        );
        let mut kinds: Vec<(Option<GestureKind>, String)> = vec![(None, "All types".into())];
        kinds.extend(KINDS.iter().map(|k| (Some(*k), forms::kind_label(*k))));
        w::dropdown(ui, "filter-kind", &mut st.kind, &kinds, 118.0);
    });
    ui.add_space(space::XS);

    if let Some((_, conflicts)) = &st.conflicts
        && !conflicts.is_empty()
    {
        let n = conflicts.len();
        let text: Vec<String> = conflicts
            .iter()
            .map(|c| format!("{}: {}", c.gestures.join(" / "), c.reason))
            .collect();
        w::banner(
            ui,
            Tone::Warn,
            &format!("{n} possible conflict{}", if n == 1 { "" } else { "s" }),
            &text.join("\n"),
            |_| {},
        );
    }

    let footer_h = 62.0;
    let list_h = (ui.available_height() - footer_h).max(120.0);
    let p = pal(ui);
    egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(radius::CARD))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_height(list_h);
            egui::ScrollArea::vertical()
                .id_salt("gesture-scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| rows(ui, ed, st));
        });
    ui.add_space(space::S);
    ui.horizontal(|ui| {
        ui.label(w::dim(ui, "Active profile"));
        let mut active = ed.draft.settings.active_profile.clone();
        let options: Vec<(String, String)> =
            profiles.iter().map(|p| (p.clone(), p.clone())).collect();
        w::dropdown(ui, "active-profile", &mut active, &options, 130.0);
        ed.draft.settings.active_profile = active;
        if w::small_button(ui, "Manage").clicked() {
            st.profile_dialog = true;
        }
    });
}

const ROW_HEIGHT: f32 = 58.0;

fn rows(ui: &mut egui::Ui, ed: &mut Editor, st: &mut State) {
    let p = pal(ui);
    let reorderable = !filtering(st);
    if ed.draft.gestures.is_empty() {
        ui.add_space(space::L);
        ui.vertical_centered(|ui| ui.label(w::dim(ui, "No gestures yet.")));
        return;
    }
    let conflicting: Vec<String> = st
        .conflicts
        .as_ref()
        .map(|(_, c)| c.iter().flat_map(|c| c.gestures.clone()).collect())
        .unwrap_or_default();
    let mut drawn: Vec<(u64, Rect)> = Vec::new();
    let mut shown = 0;
    let n = ed.draft.gestures.len();
    for i in 0..n {
        if !matches_filters(st, &ed.draft.gestures[i].def) {
            continue;
        }
        shown += 1;
        let Slot { uid, def } = &mut ed.draft.gestures[i];
        let uid = *uid;
        let selected = st.selected == Some(uid);
        let dragged = st.dragging == Some(uid);
        let width = ui.available_width();
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, ROW_HEIGHT), Sense::click());
        drawn.push((uid, rect));
        if ui.is_rect_visible(rect) {
            let fill = if selected {
                p.accent_soft
            } else if response.hovered() {
                p.raised
            } else {
                Color32::TRANSPARENT
            };
            ui.painter().rect_filled(
                rect.shrink2(Vec2::new(4.0, 2.0)),
                CornerRadius::same(radius::CONTROL + 2),
                fill,
            );
            let alpha: f32 = if def.enabled && !dragged { 1.0 } else { 0.5 };
            // Grip.
            let grip_rect =
                Rect::from_min_size(rect.min + Vec2::new(6.0, 0.0), Vec2::new(20.0, ROW_HEIGHT));
            if reorderable {
                ui.painter().text(
                    grip_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    icons::DOTS_SIX_VERTICAL,
                    fonts::icon(16.0),
                    p.text_faint
                        .gamma_multiply(if response.hovered() || selected {
                            1.0
                        } else {
                            0.5
                        }),
                );
            }
            // Kind badge.
            let badge = Rect::from_center_size(
                egui::pos2(rect.left() + 46.0, rect.center().y),
                Vec2::splat(34.0),
            );
            ui.painter().rect_filled(
                badge,
                CornerRadius::same(radius::CONTROL + 2),
                if selected { p.surface } else { p.raised },
            );
            ui.painter().text(
                badge.center(),
                egui::Align2::CENTER_CENTER,
                forms::kind_icon(def.kind),
                fonts::icon(18.0),
                if selected { p.accent } else { p.text_dim }.gamma_multiply(f32::max(alpha, 0.7)),
            );
            // Text.
            let text_left = rect.left() + 72.0;
            let text_w = (rect.right() - 64.0 - text_left).max(40.0);
            let name = elided(
                ui,
                &def.name,
                fonts::strong(size::BODY),
                p.text.gamma_multiply(alpha),
                text_w,
            );
            ui.painter()
                .galley(egui::pos2(text_left, rect.center().y - 19.0), name, p.text);
            let sub = format!(
                "{}  →  {}",
                describe(def),
                forms::action_short(def.action.kind())
            );
            let sub = elided(
                ui,
                &sub,
                fonts::text(size::SMALL),
                p.text_dim.gamma_multiply(alpha),
                text_w,
            );
            ui.painter().galley(
                egui::pos2(text_left, rect.center().y + 3.0),
                sub,
                p.text_dim,
            );
        }
        // The switch sits on top of the row and takes its own clicks.
        let switch_rect = Rect::from_center_size(
            egui::pos2(rect.right() - 36.0, rect.center().y),
            Vec2::new(38.0, 22.0),
        );
        if conflicting.contains(&def.name) {
            let r = Rect::from_center_size(
                egui::pos2(rect.right() - 68.0, rect.center().y),
                Vec2::splat(18.0),
            );
            ui.painter().text(
                r.center(),
                egui::Align2::CENTER_CENTER,
                icons::WARNING,
                fonts::icon(16.0),
                p.warn,
            );
        }
        ui.scope_builder(egui::UiBuilder::new().max_rect(switch_rect), |ui| {
            w::switch(ui, &mut def.enabled).on_hover_text(if def.enabled {
                "On: click to disable"
            } else {
                "Off: click to enable"
            });
        });
        // The grip starts a drag.
        if reorderable {
            let grip = ui.interact(
                Rect::from_min_size(rect.min, Vec2::new(30.0, ROW_HEIGHT)),
                Id::unique(("grip", uid)),
                Sense::drag(),
            );
            if grip.drag_started() {
                st.dragging = Some(uid);
                st.selected = Some(uid);
            }
            grip.on_hover_cursor(egui::CursorIcon::Grab);
        }
        if response.clicked() {
            st.selected = Some(uid);
        }
    }
    if shown == 0 {
        w::empty_state(
            ui,
            icons::MAGNIFYING_GLASS,
            "No gesture matches",
            "Try another search or clear the filters.",
            |_| {},
        );
    }
    finish_drag(ui, ed, st, &drawn);
}

fn elided(
    ui: &egui::Ui,
    text: &str,
    font: egui::FontId,
    color: Color32,
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    w::elided(ui, text, font, color, width)
}

/// Shows where a dragged row would land and moves it on release.
fn finish_drag(ui: &mut egui::Ui, ed: &mut Editor, st: &mut State, drawn: &[(u64, Rect)]) {
    let Some(dragged) = st.dragging else { return };
    let p = pal(ui);
    let Some(pointer) = ui.ctx().pointer_interact_pos() else {
        st.dragging = None;
        return;
    };
    // The insertion point is the boundary nearest to the pointer.
    let mut target = drawn.len();
    for (k, (_, rect)) in drawn.iter().enumerate() {
        if pointer.y < rect.center().y {
            target = k;
            break;
        }
    }
    if let (Some((_, first)), Some((_, last))) = (drawn.first(), drawn.last()) {
        let y = drawn.get(target).map_or(last.bottom(), |(_, r)| r.top());
        let x0 = first.left() + 8.0;
        let x1 = first.right() - 8.0;
        ui.painter().line_segment(
            [egui::pos2(x0, y), egui::pos2(x1, y)],
            Stroke::new(2.5, p.accent),
        );
        ui.painter().circle_filled(egui::pos2(x0, y), 4.0, p.accent);
    }
    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    if ui.input(|i| i.pointer.any_released()) {
        let from = ed.index_of(dragged);
        let before = drawn.get(target).map(|(u, _)| *u);
        if let Some(from) = from {
            let slot = ed.draft.gestures.remove(from);
            let to = before
                .and_then(|u| ed.index_of(u))
                .unwrap_or(ed.draft.gestures.len());
            ed.draft.gestures.insert(to, slot);
        }
        st.dragging = None;
    }
}

fn duplicate_at(ed: &mut Editor, st: &mut State, i: usize) {
    let mut def = ed.draft.gestures[i].def.clone();
    def.name = ed.unique_name(&format!("{}-copy", def.name));
    let uid = ed.add(def);
    // Right below the original.
    let slot = ed.draft.gestures.pop().expect("just added");
    ed.draft.gestures.insert(i + 1, slot);
    st.selected = Some(uid);
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
    let mut delete = None;
    {
        let uid = ed.draft.gestures[i].uid;
        let def = &mut ed.draft.gestures[i].def;
        let id = Id::unique(("gesture-editor", uid));
        let conflicts: Vec<String> = st
            .conflicts
            .as_ref()
            .map(|(_, c)| {
                c.iter()
                    .filter(|c| c.gestures.contains(&def.name))
                    .map(|c| format!("{}: {}", c.gestures.join(" / "), c.reason))
                    .collect()
            })
            .unwrap_or_default();
        egui::ScrollArea::vertical()
            .id_salt(("detail-scroll", uid))
            .auto_shrink([false, false])
            .content_margin(egui::Margin::symmetric(2, 2))
            .show(ui, |ui| {
                w::section(ui, "", "", |ui| {
                    ui.horizontal(|ui| {
                        ui.label(w::icon_text(forms::kind_icon(def.kind), 22.0, pal(ui).accent));
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;
                            ui.add(
                                egui::TextEdit::singleline(&mut def.name)
                                    .font(fonts::strong(18.0))
                                    .frame(egui::Frame::new())
                                    .hint_text("Name")
                                    .desired_width(ui.available_width() - 120.0),
                            );
                            ui.add(
                                egui::Label::new(w::caption(ui, describe(def))).truncate(),
                            );
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let more = ui.add(
                                egui::Button::new(w::icon_text(icons::DOTS_THREE, 20.0, pal(ui).text_dim))
                                    .frame(false)
                                    .min_size(Vec2::splat(30.0)),
                            );
                            egui::Popup::menu(&more).show(|ui| {
                                ui.set_min_width(150.0);
                                if ui.button(format!("{}  Duplicate", icons::COPY)).clicked() {
                                    duplicate = true;
                                }
                                if ui
                                    .button(
                                        egui::RichText::new(format!("{}  Delete", icons::TRASH))
                                            .color(pal(ui).bad),
                                    )
                                    .clicked()
                                {
                                    delete = Some(uid);
                                }
                            });
                            w::switch(ui, &mut def.enabled)
                                .on_hover_text("Enabled");
                        });
                    });
                    if def.name.trim().is_empty() {
                        w::error_text(ui, "The name must not be empty");
                    } else if others.contains(&def.name) {
                        w::error_text(ui, "Another gesture has this name");
                    }
                });
                if !conflicts.is_empty() {
                    w::banner(
                        ui,
                        Tone::Warn,
                        "Could fire together with another gesture",
                        &conflicts.join("\n"),
                        |_| {},
                    );
                }
                w::section(ui, "When", "What you do on the touchpad.", |ui| {
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
                                w::error_text(ui, &format!("{field}: {e}"));
                            }
                        }
                    }
                    ui.add_space(space::XS);
                    w::hint(
                        ui,
                        "Distances, durations and speeds apply to every gesture: Settings → Advanced.",
                    );
                });
                w::section(ui, "Then", "What GlyphPad does.", |ui| {
                    forms::action_form(ui, cx, id.with("action"), &mut def.action, false)
                });
                w::section(ui, "Only when", "Limit where the gesture works.", |ui| {
                    forms::conditions_form(ui, cx, id.with("when"), def, profiles)
                });
            });
    }
    if duplicate {
        duplicate_at(ed, st, i);
    }
    if let Some(u) = delete {
        st.confirm_delete = Some(u);
    }
}

fn delete_dialog(ctx: &egui::Context, ed: &mut Editor, st: &mut State) {
    let Some(uid) = st.confirm_delete else { return };
    let Some(i) = ed.index_of(uid) else {
        st.confirm_delete = None;
        return;
    };
    let name = ed.draft.gestures[i].def.name.clone();
    let mut close = false;
    w::dialog(ctx, "confirm-delete", "Delete this gesture?", 360.0, |ui| {
        ui.label(format!(
            "“{name}” will be removed. You can undo this with Ctrl+Z."
        ));
        ui.add_space(space::M);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if w::danger_button(ui, icons::TRASH, "Delete").clicked() {
                ed.draft.gestures.remove(i);
                st.selected = None;
                close = true;
            }
            if w::button(ui, "", "Cancel").clicked() {
                close = true;
            }
        });
    });
    if close {
        st.confirm_delete = None;
    }
}

fn profiles_dialog(ctx: &egui::Context, ed: &mut Editor, st: &mut State) {
    if !st.profile_dialog {
        return;
    }
    let profiles = ed.draft.profiles();
    let mut close = false;
    w::dialog(ctx, "profiles", "Profiles", 440.0, |ui| {
        ui.label(w::dim(
            ui,
            "Gestures with a profile only work while it is active.",
        ));
        ui.add_space(space::S);
        let mut delete = None;
        for p in &profiles {
            let used = ed
                .draft
                .gestures
                .iter()
                .filter(|g| g.def.profile.as_ref() == Some(p))
                .count();
            ui.horizontal(|ui| {
                ui.label(w::strong(p));
                ui.label(w::faint(
                    ui,
                    format!("{used} gesture{}", if used == 1 { "" } else { "s" }),
                ));
                if *p == ed.draft.settings.active_profile {
                    w::badge(ui, "active", Tone::Ok);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if w::icon_button_tone(
                        ui,
                        icons::TRASH,
                        "Delete: its gestures then work in all profiles",
                        Tone::Bad,
                    )
                    .clicked()
                    {
                        delete = Some(p.clone());
                    }
                    if w::icon_button(ui, icons::PENCIL_SIMPLE, "Rename").clicked() {
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
            ui.add_space(space::S);
            ui.horizontal(|ui| {
                ui.label(format!("Rename “{old}” to"));
                w::text_field(ui, new, "", 140.0);
                done = w::button_enabled(ui, "", "OK", !new.trim().is_empty()).clicked();
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
        ui.add_space(space::M);
        w::rule(ui);
        ui.add_space(space::M);
        ui.horizontal(|ui| {
            w::text_field(ui, &mut st.new_profile, "New profile", 200.0);
            if w::button(ui, icons::PLUS, "Create").clicked() {
                let p = st.new_profile.trim().to_string();
                if !p.is_empty() && !profiles.contains(&p) {
                    ed.draft.settings.profiles.push(p);
                }
                st.new_profile.clear();
            }
        });
        ui.add_space(space::M);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if w::button(ui, "", "Done").clicked() {
                close = true;
            }
        });
    });
    if close {
        st.profile_dialog = false;
        st.rename = None;
    }
}
