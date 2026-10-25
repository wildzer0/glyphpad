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
    /// Conflicts of the draft, recomputed a moment after it changes.
    conflicts: Option<(u64, Vec<Conflict>)>,
    /// When the draft last changed in a way that needs a new check.
    conflicts_at: Option<std::time::Instant>,
    /// Which profile the overview of the zones shows; `None` is all of them.
    zone_profile: Option<String>,
    /// The expensive half of the conflict check — do two symbols look alike —
    /// with the key it was computed for: it depends only on the symbols in
    /// use, their confidence and the templates.
    symbol_conflicts: Option<(u64, Vec<Conflict>)>,
    profile_dialog: bool,
    new_profile: String,
    rename: Option<(String, String)>,
    /// The row being dragged to a new priority.
    dragging: Option<u64>,
    /// Names seen last frame, to notice a gesture added from outside this
    /// page (the recording wizard, another program).
    known: Option<Vec<String>>,
    /// A row to bring into view on this frame.
    scroll_to: Option<u64>,
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
                    .map(|t| crate::i18n::describe_trigger(&t))
                    .unwrap_or_else(|_| "?".into())
            })
            .collect();
        return steps.join(" → ");
    }
    def.trigger_def()
        .build()
        .map(|t| crate::i18n::describe_trigger(&t))
        .unwrap_or_else(|e| format!("{} {}", crate::i18n::tr("incomplete:"), e[0].1))
}

/// How long the draft must rest before conflicts are looked for again:
/// looking for them compares every pair of symbols, which is far too slow to
/// do on every keystroke.
const CONFLICT_DELAY: std::time::Duration = std::time::Duration::from_millis(300);

/// What the expensive half of the conflict check depends on: the symbols in
/// use with their confidence, and the templates (by the address of the
/// published map, which changes whenever they are reloaded).
fn symbol_key(config: &glyphpad_core::config::Config, cx: &Ctx) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for g in &config.gestures {
        if let Some(t @ glyphpad_core::trigger::Trigger::Symbol { .. }) = &g.trigger {
            format!("{t:?}").hash(&mut h);
        }
    }
    (std::sync::Arc::as_ptr(&cx.shared.effective) as usize).hash(&mut h);
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

    // Conflicts of the draft (SPEC §13), once it stops changing. Comparing
    // every pair of symbols takes hundreds of milliseconds on a real
    // configuration, so that half is cached and the rest waits for the user
    // to stop typing or dragging.
    let revision = ed.draft_revision();
    if st.conflicts.as_ref().is_some_and(|c| c.0 == revision) {
        st.conflicts_at = None;
    } else if ui.ctx().egui_wants_keyboard_input() || ui.input(|i| i.pointer.any_down()) {
        st.conflicts_at = Some(std::time::Instant::now());
        ui.ctx().request_repaint_after(CONFLICT_DELAY);
    } else {
        let waiting = st.conflicts_at.get_or_insert_with(std::time::Instant::now);
        let ready = st.conflicts.is_none() || waiting.elapsed() >= CONFLICT_DELAY;
        if ready && let Some(config) = ed.draft_config() {
            let key = symbol_key(&config, cx);
            let symbols = match &st.symbol_conflicts {
                Some((cached, list)) if *cached == key => list.clone(),
                _ => {
                    let list = glyphpad_core::conflicts::symbols(&config, &cx.shared.effective);
                    st.symbol_conflicts = Some((key, list.clone()));
                    list
                }
            };
            let mut found = glyphpad_core::conflicts::movements(&config);
            found.extend(symbols);
            st.conflicts = Some((revision, found));
            st.conflicts_at = None;
        } else {
            ui.ctx().request_repaint_after(CONFLICT_DELAY);
        }
    }

    // A gesture added from somewhere else — the recording wizard, another
    // program editing the file — is selected and brought into view: at the
    // bottom of a long list it would otherwise look as if it had not been
    // saved at all.
    let names: Vec<String> = ed
        .draft
        .gestures
        .iter()
        .map(|s| s.def.name.clone())
        .collect();
    if let Some(known) = &st.known
        && let Some(uid) = appeared(known, ed)
    {
        st.selected = Some(uid);
        st.scroll_to = st.selected;
    }
    st.known = Some(names);

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

/// The gesture that appeared since `known`, when the draft is the file's and
/// not the user's own edits: what the recording wizard just saved, or another
/// program editing the file.
fn appeared(known: &[String], ed: &Editor) -> Option<u64> {
    if ed.dirty() {
        return None;
    }
    ed.draft
        .gestures
        .iter()
        .find(|s| !known.contains(&s.def.name))
        .map(|s| s.uid)
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
        let mut kinds: Vec<(Option<GestureKind>, std::borrow::Cow<'static, str>)> =
            vec![(None, "All types".into())];
        kinds.extend(
            KINDS
                .iter()
                .map(|k| (Some(*k), forms::kind_label(*k).into())),
        );
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
        w::banner_raw(
            ui,
            Tone::Warn,
            &crate::i18n::plural(n, "{} possible conflict", "{} possible conflicts"),
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
        let options: Vec<(String, std::borrow::Cow<'static, str>)> = profiles
            .iter()
            .map(|p| (p.clone(), p.clone().into()))
            .collect();
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
        if st.scroll_to == Some(uid) {
            ui.scroll_to_rect(rect, Some(Align::Center));
            st.scroll_to = None;
        }
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
                crate::i18n::tr(forms::action_short(def.action.kind()))
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
                crate::i18n::tr("On: click to disable")
            } else {
                crate::i18n::tr("Off: click to enable")
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

/// A zone gesture as the overview draws it.
struct ZoneOnPad {
    uid: u64,
    name: String,
    regions: Vec<glyphpad_core::trigger::Region>,
    /// Another gesture's zone overlaps this one: a tap there could go either way.
    conflict: bool,
}

/// Every zone of the draft, for the overview: what the page shows all at once
/// instead of one gesture at a time.
fn zones_on_pad(st: &State, ed: &Editor, filter: Option<&str>) -> Vec<ZoneOnPad> {
    let conflicting: Vec<String> = st
        .conflicts
        .as_ref()
        .map(|(_, c)| c.iter().flat_map(|c| c.gestures.clone()).collect())
        .unwrap_or_default();
    ed.draft
        .gestures
        .iter()
        .filter(|s| s.def.enabled)
        .filter(|s| match filter {
            None => true,
            Some(p) => s.def.profile.as_deref().is_none_or(|g| g == p),
        })
        .filter_map(|s| {
            let trigger = s.def.trigger_def().build().ok()?;
            let glyphpad_core::trigger::Trigger::Zone { regions, .. } = trigger else {
                return None;
            };
            Some(ZoneOnPad {
                uid: s.uid,
                name: s.def.name.clone(),
                regions,
                conflict: conflicting.contains(&s.def.name),
            })
        })
        .collect()
}

/// The pad with every zone on it, the selected gesture's ones brighter.
/// Returns the gesture whose zone was clicked.
fn draw_zone_overview(
    ui: &mut egui::Ui,
    pad: (f32, f32),
    zones: &[ZoneOnPad],
    selected: u64,
) -> Option<u64> {
    let p = pal(ui);
    let width = ui.available_width().clamp(180.0, 380.0);
    let scale = width / pad.0.max(1.0);
    let size = egui::vec2(pad.0 * scale, pad.1 * scale);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, radius::CARD, p.surface);
    painter.rect_stroke(
        rect,
        radius::CARD,
        egui::Stroke::new(1.0, p.border_strong),
        egui::StrokeKind::Inside,
    );
    for (i, zone) in zones.iter().enumerate() {
        // One colour per gesture, red where two overlap.
        let colour = if zone.conflict {
            p.bad
        } else {
            p.series[i % p.series.len()]
        };
        let on = zone.uid == selected;
        for r in &zone.regions {
            let area = egui::Rect::from_two_pos(
                egui::pos2(rect.left() + r.x[0] * scale, rect.top() + r.y[0] * scale),
                egui::pos2(rect.left() + r.x[1] * scale, rect.top() + r.y[1] * scale),
            );
            painter.rect_filled(area, 3, colour.gamma_multiply(if on { 0.35 } else { 0.18 }));
            painter.rect_stroke(
                area,
                3,
                egui::Stroke::new(if on { 2.0 } else { 1.0 }, colour),
                egui::StrokeKind::Inside,
            );
        }
    }
    if response.clicked()
        && let Some(pos) = response.interact_pointer_pos()
    {
        let (mx, my) = ((pos.x - rect.left()) / scale, (pos.y - rect.top()) / scale);
        // The last drawn is on top: pick what the user sees.
        return zones
            .iter()
            .rev()
            .find(|z| z.regions.iter().any(|r| r.contains(mx, my)))
            .map(|z| z.uid);
    }
    None
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
    // The pad the zones are for: the one chosen in Settings, else the one in
    // use. Its size is what the zone map is drawn with.
    let pad = cx.shared.pad_size_mm_for(&ed.draft.settings.touchpad);
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
    // Read before the editor borrows the gesture itself: the overview shows
    // the zones of every gesture, this one included.
    let kind = ed.draft.gestures[i].def.kind;
    let zones = if kind == GestureKind::Zone {
        zones_on_pad(st, ed, st.zone_profile.as_deref())
    } else {
        Vec::new()
    };
    let mut select_zone: Option<u64> = None;
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
                                    // An explicit id: the field keeps the
                                    // focus when something above it appears
                                    // or disappears (a banner, a warning).
                                    .id(id.with("name"))
                                    .font(fonts::strong(18.0))
                                    .frame(egui::Frame::new())
                                    .hint_text(crate::i18n::tr("Name"))
                                    .desired_width(ui.available_width() - 120.0),
                            );
                            ui.add(
                                egui::Label::new(w::caption_raw(ui, describe(def))).truncate(),
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
                                if ui
                                    .add(egui::Button::new(w::label_job_sized(
                                        icons::COPY,
                                        crate::i18n::tr("Duplicate"),
                                        14.0,
                                        13.0,
                                        pal(ui).text,
                                    )))
                                    .clicked()
                                {
                                    duplicate = true;
                                }
                                if ui
                                    .button(egui::RichText::new(
                                        crate::i18n::tr("Delete"),
                                    ).color(pal(ui).bad))
                                    .clicked()
                                {
                                    delete = Some(uid);
                                }
                            });
                            w::switch(ui, &mut def.enabled)
                                .on_hover_text(crate::i18n::tr("Enabled"));
                        });
                    });
                    if def.name.trim().is_empty() {
                        w::error_text(ui, "The name must not be empty");
                    } else if others.contains(&def.name) {
                        w::error_text(ui, "Another gesture has this name");
                    }
                });
                if !conflicts.is_empty() {
                    w::banner_raw(
                        ui,
                        Tone::Warn,
                        crate::i18n::tr("Could fire together with another gesture"),
                        &conflicts.join("\n"),
                        |_| {},
                    );
                }
                if kind == GestureKind::Zone {
                    w::section(
                        ui,
                        "All zones",
                        "Every zone together, so overlaps show: click one to edit its gesture.",
                        |ui| {
                            let mut filter = st.zone_profile.clone();
                            let mut options: Vec<(Option<String>, std::borrow::Cow<'static, str>)> =
                                vec![(None, "All profiles".into())];
                            for profile in profiles {
                                options.push((Some(profile.clone()), profile.clone().into()));
                            }
                            ui.horizontal(|ui| {
                                w::dropdown(ui, "zone-profile", &mut filter, &options, 170.0);
                            });
                            st.zone_profile = filter;
                            ui.add_space(space::XS);
                            if let Some(clicked) = draw_zone_overview(ui, pad, &zones, uid) {
                                select_zone = Some(clicked);
                            }
                            ui.add_space(space::XS);
                            if zones.is_empty() {
                                ui.label(w::faint(ui, "No zones yet."));
                            }
                            ui.horizontal_wrapped(|ui| {
                                let p = pal(ui);
                                for (i, z) in zones.iter().enumerate() {
                                    let colour = if z.conflict {
                                        p.bad
                                    } else {
                                        p.series[i % p.series.len()]
                                    };
                                    ui.label(egui::RichText::new("●").color(colour).size(10.0));
                                    if ui.selectable_label(z.uid == uid, &z.name).clicked() {
                                        select_zone = Some(z.uid);
                                    }
                                }
                            });
                        },
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
                        pad,
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
                        forms::sequence_form(
                            ui,
                            id.with("steps"),
                            def,
                            symbols,
                            default_confidence,
                            pad,
                        );
                    } else {
                        def.steps = None;
                        def.within_ms = None;
                        if let Err(errors) = def.trigger_def().build() {
                            for (field, e) in errors {
                                w::error_text_raw(ui, &format!("{field}: {e}"));
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
    if let Some(clicked) = select_zone {
        st.selected = Some(clicked);
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
        ui.label(crate::i18n::tf(
            "“{}” will be removed. You can undo this with Ctrl+Z.",
            &[&name],
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
                ui.label(w::strong_raw(p));
                ui.label(w::faint_raw(
                    ui,
                    crate::i18n::plural(used, "{} gesture", "{} gestures"),
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
            // A rule naming a profile that no longer exists would keep it
            // alive and silently apply nothing.
            ed.draft.settings.profile_rules.retain(|r| r.profile != p);
            if ed.draft.settings.active_profile == p {
                ed.draft.settings.active_profile = "default".into();
            }
        }
        if let Some((old, new)) = &mut st.rename {
            let mut done = false;
            ui.add_space(space::S);
            ui.horizontal(|ui| {
                ui.label(crate::i18n::tf("Rename “{}” to", &[old]));
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
                for rule in &mut ed.draft.settings.profile_rules {
                    if rule.profile == old {
                        rule.profile = new.clone();
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

#[cfg(test)]
mod tests {
    use glyphpad_core::config;

    use super::*;
    use crate::editor::Editor;
    use glyphpad_core::service::Shared;
    use std::sync::Arc;

    const FILE: &str = "[settings]\nclose_to_tray = true\n\n[[gesture]]\nname = \"a\"\ntype = \"tap\"\nfingers = 1\naction = { type = \"lock_screen\" }\n\n[[gesture]]\nname = \"b\"\ntype = \"tap\"\nfingers = 2\naction = { type = \"lock_screen\" }\n";

    fn shared(text: &str) -> Shared {
        Shared {
            config: Arc::new(config::parse(text).unwrap().config),
            config_text: Arc::new(text.to_string()),
            revision: 1,
            ..Default::default()
        }
    }

    #[test]
    fn a_gesture_the_page_did_not_know_is_the_one_to_show() {
        let s = shared(FILE);
        let e = Editor::new(&s);
        let (a, b) = (e.draft.gestures[0].uid, e.draft.gestures[1].uid);
        // Both known: nothing new.
        assert_eq!(appeared(&["a".into(), "b".into()], &e), None);
        // The wizard added "b" to the file: it is the one to select.
        assert_eq!(appeared(&["a".into()], &e), Some(b));
        // The user's own edits are not "added from outside".
        let mut dirty = Editor::new(&s);
        dirty.draft.gestures.push(Slot {
            uid: 99,
            def: dirty.draft.gestures[0].def.clone(),
        });
        assert!(dirty.dirty());
        assert_eq!(appeared(&["a".into(), "b".into()], &dirty), None);
        let _ = a;
    }
}
