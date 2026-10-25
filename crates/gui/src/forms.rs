//! Forms of a gesture: its trigger, its action and where it works
//! (SPEC §10.2). Each form edits a value in place and shows what is wrong
//! with it right where it is.

use eframe::egui::{self, Id, RichText};
use glyphpad_core::action::{Action, DesktopOp, MediaKey, ScreenshotMode, WindowOp};
use glyphpad_core::config::{Direction, Edge, GestureDef, GestureKind, OneOrMany, When};
use glyphpad_core::keys::KeyCombo;
use glyphpad_core::service::Command;
use glyphpad_core::trigger::{TriggerDef, Turn, Zoom};

use crate::theme::{self, pal, space};
use crate::widgets::{self as w, Tone};
use crate::{Ctx, icons};

pub const KINDS: [GestureKind; 9] = [
    GestureKind::Tap,
    GestureKind::Hold,
    GestureKind::Swipe,
    GestureKind::EdgeSwipe,
    GestureKind::Pinch,
    GestureKind::Rotate,
    GestureKind::Click,
    GestureKind::Symbol,
    GestureKind::Sequence,
];

pub fn kind_icon(k: GestureKind) -> &'static str {
    match k {
        GestureKind::Tap => icons::HAND_TAP,
        GestureKind::Hold => icons::FINGERPRINT,
        GestureKind::Swipe => icons::HAND_SWIPE_RIGHT,
        GestureKind::EdgeSwipe => icons::BOUNDING_BOX,
        GestureKind::Pinch => icons::ARROWS_IN_SIMPLE,
        GestureKind::Rotate => icons::ARROW_CLOCKWISE,
        GestureKind::Click => icons::CURSOR_CLICK,
        GestureKind::Symbol => icons::SCRIBBLE_LOOP,
        GestureKind::Sequence => icons::QUEUE,
    }
}

pub fn kind_label(k: GestureKind) -> String {
    let l = k.label();
    let mut c = l.chars();
    c.next().map_or(String::new(), |f| {
        f.to_uppercase().collect::<String>() + c.as_str()
    })
}

/// A label for the kinds in tiles: shorter than the list's.
fn tile_label(k: GestureKind) -> &'static str {
    match k {
        GestureKind::Tap => "Tap",
        GestureKind::Hold => "Hold",
        GestureKind::Swipe => "Swipe",
        GestureKind::EdgeSwipe => "Edge swipe",
        GestureKind::Pinch => "Pinch",
        GestureKind::Rotate => "Rotate",
        GestureKind::Click => "Click",
        GestureKind::Symbol => "Symbol",
        GestureKind::Sequence => "Sequence",
    }
}

/// Copies trigger fields into a gesture.
pub fn set_trigger(def: &mut GestureDef, t: &TriggerDef) {
    def.kind = t.kind.unwrap_or(def.kind);
    def.fingers = t.fingers;
    def.taps = t.taps;
    def.direction = t.direction;
    def.edge = t.edge;
    def.zoom = t.zoom;
    def.rotation = t.rotation;
    def.modifier_fingers = t.modifier_fingers;
    def.symbol = t.symbol.clone();
    def.confidence = t.confidence;
}

/// Fields a kind needs, with sensible starting values, when switching to it.
fn reset_for(kind: GestureKind, old: &TriggerDef) -> TriggerDef {
    let fingers = old.fingers.unwrap_or(3);
    let mut t = TriggerDef {
        kind: Some(kind),
        ..Default::default()
    };
    match kind {
        GestureKind::Tap | GestureKind::Hold => t.fingers = Some(fingers),
        GestureKind::Swipe => {
            t.fingers = Some(fingers);
            t.direction = Some(old.direction.unwrap_or(Direction::Up));
        }
        GestureKind::EdgeSwipe => {
            t.fingers = Some(1);
            t.edge = Some(Edge::Left);
            t.direction = Some(Direction::Right);
        }
        GestureKind::Pinch => {
            t.fingers = Some(fingers.max(2));
            t.zoom = Some(Zoom::In);
        }
        GestureKind::Rotate => {
            t.fingers = Some(fingers.max(2));
            t.rotation = Some(Turn::Clockwise);
        }
        GestureKind::Click => t.fingers = Some(fingers.min(2)),
        GestureKind::Symbol => t.symbol = Some(OneOrMany::One("L".into())),
        GestureKind::Sequence => {}
    }
    t
}

fn fingers(ui: &mut egui::Ui, v: &mut Option<u8>, min: u8) {
    let mut n = v.unwrap_or(3).max(min);
    w::stepper(ui, &mut n, min, glyphpad_core::MAX_CONTACTS as u8);
    *v = Some(n);
}

/// The kinds of gesture as a grid of tiles.
fn kind_picker(ui: &mut egui::Ui, kind: GestureKind, allow_sequence: bool) -> Option<GestureKind> {
    let mut picked = None;
    let kinds: Vec<GestureKind> = KINDS
        .into_iter()
        .filter(|k| allow_sequence || *k != GestureKind::Sequence)
        .collect();
    let gap = space::S;
    let per_row = if ui.available_width() > 470.0 { 5 } else { 3 };
    let width = ((ui.available_width() - gap * (per_row as f32 - 1.0)) / per_row as f32).floor();
    ui.spacing_mut().item_spacing = egui::vec2(gap, gap);
    for chunk in kinds.chunks(per_row) {
        ui.horizontal(|ui| {
            for k in chunk {
                if w::tile(
                    ui,
                    kind_icon(*k),
                    tile_label(*k),
                    *k == kind,
                    egui::vec2(width, 64.0),
                )
                .clicked()
                    && *k != kind
                {
                    picked = Some(*k);
                }
            }
        });
    }
    picked
}

/// Edits a trigger. `allow_sequence` is false for the steps of a sequence.
pub fn trigger_form(
    ui: &mut egui::Ui,
    id: Id,
    t: &mut TriggerDef,
    symbols: &[String],
    default_confidence: f32,
    allow_sequence: bool,
) {
    let kind = t.kind.unwrap_or(GestureKind::Tap);
    if let Some(k) = kind_picker(ui, kind, allow_sequence) {
        *t = reset_for(k, t);
    }
    ui.add_space(space::XS);
    let kind = t.kind.unwrap_or(GestureKind::Tap);
    match kind {
        GestureKind::Tap => {
            w::row(ui, "Fingers", "", |ui| fingers(ui, &mut t.fingers, 1));
            w::row(ui, "Taps", "", |ui| {
                let mut taps = t.taps.unwrap_or(1);
                w::segmented(
                    ui,
                    &mut taps,
                    &[(1, "Single"), (2, "Double"), (3, "Triple")],
                );
                t.taps = (taps > 1).then_some(taps);
            });
            chord(ui, t);
        }
        GestureKind::Hold => w::row(ui, "Fingers", "", |ui| fingers(ui, &mut t.fingers, 1)),
        GestureKind::Swipe => {
            w::row(ui, "Fingers", "", |ui| fingers(ui, &mut t.fingers, 1));
            w::row(ui, "Direction", "", |ui| {
                let mut d = t.direction.unwrap_or(Direction::Up);
                w::direction_picker(ui, &mut d, true);
                t.direction = Some(d);
            });
            chord(ui, t);
        }
        GestureKind::EdgeSwipe => {
            w::row(ui, "Fingers", "", |ui| fingers(ui, &mut t.fingers, 1));
            w::row(ui, "Starts at the", "", |ui| {
                let mut e = t.edge.unwrap_or_default();
                w::segmented(
                    ui,
                    &mut e,
                    &[
                        (Edge::Left, "Left"),
                        (Edge::Right, "Right"),
                        (Edge::Top, "Top"),
                        (Edge::Bottom, "Bottom"),
                    ],
                );
                t.edge = Some(e);
            });
            w::row(ui, "Direction", "", |ui| {
                let mut d = t.direction.unwrap_or(Direction::Right);
                w::direction_picker(ui, &mut d, true);
                t.direction = Some(d);
            });
        }
        GestureKind::Pinch => {
            w::row(ui, "Fingers", "", |ui| fingers(ui, &mut t.fingers, 2));
            w::row(ui, "Fingers move", "", |ui| {
                let mut z = t.zoom.unwrap_or(Zoom::In);
                w::segmented(ui, &mut z, &[(Zoom::In, "Closer"), (Zoom::Out, "Apart")]);
                t.zoom = Some(z);
            });
        }
        GestureKind::Rotate => {
            w::row(ui, "Fingers", "", |ui| fingers(ui, &mut t.fingers, 2));
            w::row(ui, "Turn", "", |ui| {
                let mut r = t.rotation.unwrap_or(Turn::Clockwise);
                w::segmented_with_icons(
                    ui,
                    &mut r,
                    &[
                        (Turn::Clockwise, icons::ARROW_CLOCKWISE, "Clockwise"),
                        (
                            Turn::Counterclockwise,
                            icons::ARROW_COUNTER_CLOCKWISE,
                            "Counterclockwise",
                        ),
                    ],
                );
                t.rotation = Some(r);
            });
        }
        GestureKind::Click => {
            w::row(ui, "Fingers", "", |ui| fingers(ui, &mut t.fingers, 1));
            w::row(
                ui,
                "Drag",
                "The click itself still reaches Windows: raw input cannot hold it back.",
                |ui| {
                    let mut drag = t.direction.is_some();
                    ui.horizontal(|ui| {
                        w::switch(ui, &mut drag);
                        ui.label("Click and drag");
                    });
                    if drag {
                        let mut d = t.direction.unwrap_or(Direction::Down);
                        w::direction_picker(ui, &mut d, true);
                        t.direction = Some(d);
                    } else {
                        t.direction = None;
                    }
                },
            );
        }
        GestureKind::Symbol => {
            let mut chosen = t.symbol.as_ref().map(OneOrMany::to_vec).unwrap_or_default();
            w::row(ui, "Symbols", "Any of the chosen ones triggers it.", |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
                    for s in symbols {
                        let mut on = chosen.contains(s);
                        let r = w::chip(ui, s, on);
                        if r.clicked() {
                            on = !on;
                            if on {
                                chosen.push(s.clone());
                            } else {
                                chosen.retain(|x| x != s);
                            }
                        }
                    }
                });
                for s in chosen.iter().filter(|s| !symbols.contains(s)) {
                    w::warn_text(
                        ui,
                        &format!("“{s}” has no examples yet: record some in Review."),
                    );
                }
            });
            t.symbol = match chosen.len() {
                0 => None,
                1 => Some(OneOrMany::One(chosen[0].clone())),
                _ => Some(OneOrMany::Many(chosen)),
            };
            w::row(
                ui,
                "Minimum score",
                "How close a drawing must be to count.",
                |ui| {
                    ui.horizontal(|ui| {
                        let mut c = t.confidence.unwrap_or(default_confidence);
                        if w::slider(ui, egui::Slider::new(&mut c, 0.5..=0.99).fixed_decimals(2))
                            .changed()
                        {
                            t.confidence = Some(c);
                        }
                        if t.confidence.is_some() {
                            if w::icon_button(
                                ui,
                                icons::ARROW_COUNTER_CLOCKWISE,
                                "Restore the default",
                            )
                            .clicked()
                            {
                                t.confidence = None;
                            }
                        } else {
                            ui.label(w::faint(ui, "default"));
                        }
                    });
                },
            );
        }
        GestureKind::Sequence => {}
    }
    let _ = id;
}

fn chord(ui: &mut egui::Ui, t: &mut TriggerDef) {
    w::row(
        ui,
        "Chord",
        "Other fingers rest while these ones move.",
        |ui| {
            let mut on = t.modifier_fingers.is_some();
            ui.horizontal(|ui| {
                w::switch(ui, &mut on);
                ui.label("With resting fingers");
            });
            if on {
                let mut m = t.modifier_fingers.unwrap_or(1);
                ui.horizontal(|ui| {
                    w::stepper(ui, &mut m, 1, 3);
                    ui.label(w::dim(ui, "resting"));
                });
                t.modifier_fingers = Some(m);
                if t.taps.is_some() {
                    t.taps = None;
                }
            } else {
                t.modifier_fingers = None;
            }
        },
    );
}

/// The steps of a sequence and the pause allowed between them.
pub fn sequence_form(
    ui: &mut egui::Ui,
    id: Id,
    def: &mut GestureDef,
    symbols: &[String],
    default_confidence: f32,
) {
    let p = pal(ui);
    let steps = def.steps.get_or_insert_with(Vec::new);
    let mut remove = None;
    let mut swap = None;
    let n = steps.len();
    for (i, step) in steps.iter_mut().enumerate() {
        egui::Frame::new()
            .fill(p.bg)
            .corner_radius(egui::CornerRadius::same(theme::radius::CARD))
            .inner_margin(egui::Margin::same(space::M as i8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    w::badge(ui, &format!("Step {}", i + 1), Tone::Accent);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if w::icon_button_tone(ui, icons::TRASH, "Remove this step", Tone::Bad)
                            .clicked()
                        {
                            remove = Some(i);
                        }
                        if ui
                            .add_enabled(
                                i + 1 < n,
                                egui::Button::new(w::icon_text(
                                    icons::ARROW_DOWN,
                                    14.0,
                                    p.text_dim,
                                ))
                                .frame(false),
                            )
                            .on_hover_text("Move down")
                            .clicked()
                        {
                            swap = Some((i, i + 1));
                        }
                        if ui
                            .add_enabled(
                                i > 0,
                                egui::Button::new(w::icon_text(icons::ARROW_UP, 14.0, p.text_dim))
                                    .frame(false),
                            )
                            .on_hover_text("Move up")
                            .clicked()
                        {
                            swap = Some((i, i - 1));
                        }
                    });
                });
                trigger_form(
                    ui,
                    id.with(("step", i)),
                    step,
                    symbols,
                    default_confidence,
                    false,
                );
            });
        ui.add_space(space::S);
    }
    if let Some((a, b)) = swap {
        steps.swap(a, b);
    }
    if let Some(i) = remove {
        steps.remove(i);
    }
    let can_add = steps.len() < 4;
    if w::button_enabled(ui, icons::PLUS, "Add a step", can_add).clicked() {
        steps.push(TriggerDef {
            kind: Some(GestureKind::Tap),
            fingers: Some(3),
            ..Default::default()
        });
    }
    w::row(
        ui,
        "Time between steps",
        "The first step's own gesture waits for the next step before it runs.",
        |ui| {
            let mut within = def
                .within_ms
                .unwrap_or(glyphpad_core::config::DEFAULT_SEQUENCE_WITHIN_MS);
            if ui
                .add(
                    egui::DragValue::new(&mut within)
                        .range(100..=5000)
                        .suffix(" ms"),
                )
                .changed()
            {
                def.within_ms = Some(within);
            }
        },
    );
}

const ACTIONS: &[(&str, &str, &str)] = &[
    ("keys", icons::KEYBOARD, "Press a shortcut"),
    ("lock_screen", icons::LOCK_SIMPLE, "Lock the PC"),
    ("close_window", icons::X, "Close the window in front"),
    ("minimize", icons::MINUS, "Minimize the window"),
    ("maximize", icons::SQUARE, "Maximize / restore the window"),
    ("run", icons::TERMINAL_WINDOW, "Run a program"),
    ("media", icons::PLAY_PAUSE, "Media key"),
    ("chain", icons::STACK, "Several actions in a row"),
    ("notify", icons::BELL, "Show a message"),
    ("toggle_enabled", icons::POWER, "Pause / resume gestures"),
    ("repeat_last", icons::REPEAT, "Repeat the last gesture"),
    ("show_window", icons::APP_WINDOW, "Show the GlyphPad window"),
    (
        "draw_symbol",
        icons::SCRIBBLE_LOOP,
        "Draw a symbol with one finger next",
    ),
    (
        "open",
        icons::ARROW_SQUARE_OUT,
        "Open a file, folder or address",
    ),
    ("type_text", icons::TEXT_AA, "Type a text"),
    ("window", icons::SQUARES_FOUR, "Arrange the window in front"),
    ("virtual_desktop", icons::MONITOR, "Switch virtual desktop"),
    ("screenshot", icons::CAMERA, "Take a screenshot"),
    ("switch_profile", icons::USER_SWITCH, "Switch to a profile"),
    ("next_profile", icons::USERS, "Switch to the next profile"),
    (
        "show_cheat_sheet",
        icons::LIST_CHECKS,
        "Show the list of gestures",
    ),
];

/// Kinds that need GlyphPad itself and so cannot be steps of a chain.
const NOT_IN_CHAINS: &[&str] = &[
    "chain",
    "repeat_last",
    "draw_symbol",
    "toggle_enabled",
    "show_window",
    "switch_profile",
    "next_profile",
    "show_cheat_sheet",
];

pub fn action_icon(kind: &str) -> &'static str {
    ACTIONS
        .iter()
        .find(|(k, _, _)| *k == kind)
        .map_or(icons::TIMER, |(_, i, _)| i)
}

pub fn action_label(kind: &str) -> &'static str {
    ACTIONS
        .iter()
        .find(|(k, _, _)| *k == kind)
        .map_or("Pause", |(_, _, l)| l)
}

/// A short name of an action, for lists.
pub fn action_short(kind: &str) -> &'static str {
    Action::short_label(kind)
}

fn default_action(kind: &str) -> Action {
    match kind {
        "keys" => Action::Keys {
            combo: Some("Ctrl+C".parse().expect("valid")),
            sequence: Vec::new(),
        },
        "lock_screen" => Action::LockScreen,
        "close_window" => Action::CloseWindow { force: false },
        "minimize" => Action::Minimize,
        "maximize" => Action::Maximize,
        "run" => Action::Run {
            command: String::new(),
            args: Vec::new(),
            hidden: false,
            cwd: None,
        },
        "media" => Action::Media {
            key: MediaKey::PlayPause,
        },
        "chain" => Action::Chain {
            actions: vec![Action::Keys {
                combo: Some("Ctrl+C".parse().expect("valid")),
                sequence: Vec::new(),
            }],
        },
        "wait" => Action::Wait { ms: 200 },
        "notify" => Action::Notify {
            title: None,
            message: "Hello".into(),
        },
        "toggle_enabled" => Action::ToggleEnabled,
        "repeat_last" => Action::RepeatLast,
        "show_window" => Action::ShowWindow,
        "open" => Action::Open {
            target: String::new(),
        },
        "type_text" => Action::TypeText {
            text: String::new(),
        },
        "window" => Action::Window {
            op: WindowOp::SnapLeft,
        },
        "virtual_desktop" => Action::VirtualDesktop {
            op: DesktopOp::Next,
        },
        "screenshot" => Action::Screenshot {
            mode: ScreenshotMode::Region,
        },
        "switch_profile" => Action::SwitchProfile {
            profile: "default".into(),
        },
        "next_profile" => Action::NextProfile,
        "show_cheat_sheet" => Action::ShowCheatSheet,
        _ => Action::DrawSymbol,
    }
}

/// The name of an egui key as GlyphPad writes it, for shortcut capture.
pub fn key_name(key: egui::Key) -> Option<String> {
    use egui::Key as K;
    let name = match key {
        K::ArrowUp => "Up".into(),
        K::ArrowDown => "Down".into(),
        K::ArrowLeft => "Left".into(),
        K::ArrowRight => "Right".into(),
        K::OpenBracket => "[".into(),
        K::CloseBracket => "]".into(),
        K::Backtick => "`".into(),
        K::Equals => "=".into(),
        K::Quote => "'".into(),
        k => {
            let n = k.name().to_string();
            n.strip_prefix("Num").map_or(n.clone(), str::to_string)
        }
    };
    name.parse::<glyphpad_core::keys::Key>()
        .ok()
        .map(|k| k.to_string())
}

/// A shortcut field: typed, or captured by pressing it.
fn shortcut_field(ui: &mut egui::Ui, id: Id, combo: &mut Option<KeyCombo>) {
    let p = pal(ui);
    let text_id = id.with("text");
    let capture_id = id.with("capture");
    let mut text: String = ui
        .data_mut(|d| d.get_temp(text_id))
        .unwrap_or_else(|| combo.as_ref().map(ToString::to_string).unwrap_or_default());
    let mut capturing: bool = ui.data_mut(|d| d.get_temp(capture_id)).unwrap_or(false);
    ui.horizontal(|ui| {
        let edit = w::text_field(ui, &mut text, "e.g. Ctrl+Shift+T", 170.0);
        if edit.changed()
            && let Ok(c) = text.parse::<KeyCombo>()
        {
            *combo = Some(c);
        }
        let label = if capturing {
            "Press the keys…"
        } else {
            "Record"
        };
        let btn = if capturing {
            egui::Button::new(
                RichText::new(format!("{}  {label}", icons::RECORD)).color(p.on_accent),
            )
            .fill(p.bad)
            .stroke(egui::Stroke::NONE)
        } else {
            egui::Button::new(format!("{}  {label}", icons::RECORD))
        };
        if ui
            .add(btn.min_size(egui::vec2(0.0, theme::CONTROL_HEIGHT)))
            .on_hover_text("Press the key combination to use")
            .clicked()
        {
            capturing = !capturing;
        }
        if capturing {
            ui.ctx().request_repaint();
            let pressed = ui.input_mut(|i| {
                let mut found = None;
                i.events.retain(|e| match e {
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } if found.is_none() => {
                        found = Some((*key, *modifiers));
                        false
                    }
                    _ => true,
                });
                found
            });
            if let Some((key, m)) = pressed
                && let Some(k) = key_name(key)
            {
                let mut s = String::new();
                for (on, name) in [(m.ctrl, "Ctrl+"), (m.alt, "Alt+"), (m.shift, "Shift+")] {
                    if on {
                        s.push_str(name);
                    }
                }
                s.push_str(&k);
                if let Ok(c) = s.parse() {
                    *combo = Some(c);
                    text = s;
                }
                capturing = false;
            }
        }
    });
    let valid = text.parse::<KeyCombo>().is_ok();
    if valid && !text.is_empty() {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for (i, part) in text.split('+').enumerate() {
                if i > 0 {
                    ui.label(w::faint(ui, "+"));
                }
                w::kbd(ui, part.trim());
            }
        });
    }
    if !valid && !text.is_empty() {
        w::error_text(ui, "Not a valid shortcut");
    }
    if text.eq_ignore_ascii_case("win+l") {
        w::warn_text(ui, "Win+L cannot be simulated; use “Lock the PC” instead.");
    }
    w::hint(
        ui,
        "The Windows key and some system shortcuts are taken by Windows before GlyphPad sees them: type those.",
    );
    ui.data_mut(|d| {
        d.insert_temp(text_id, text);
        d.insert_temp(capture_id, capturing);
    });
}

fn action_picker(ui: &mut egui::Ui, id: Id, action: &mut Action, nested: bool) {
    let kind = action.kind();
    let current = format!("{}  {}", action_icon(kind), action_label(kind));
    egui::ComboBox::from_id_salt(id.with("kind"))
        .width(280.0)
        .selected_text(current)
        .show_ui(ui, |ui| {
            for (k, icon, label) in ACTIONS {
                if nested && NOT_IN_CHAINS.contains(k) {
                    continue;
                }
                if ui
                    .selectable_label(*k == kind, format!("{icon}  {label}"))
                    .clicked()
                    && *k != kind
                {
                    *action = default_action(k);
                }
            }
            if nested
                && ui
                    .selectable_label(kind == "wait", format!("{}  Pause", icons::TIMER))
                    .clicked()
                && kind != "wait"
            {
                *action = default_action("wait");
            }
        });
}

/// Edits an action; `nested` hides the kinds a chain cannot contain.
pub fn action_form(ui: &mut egui::Ui, cx: &Ctx, id: Id, action: &mut Action, nested: bool) {
    ui.horizontal(|ui| {
        action_picker(ui, id, action, nested);
        if !nested
            && w::button(ui, icons::PLAY, "Try it")
                .on_hover_text("Run this action now")
                .clicked()
        {
            cx.send(Command::RunAction(action.clone()));
        }
    });
    ui.add_space(space::XS);
    match action {
        Action::Keys { combo, sequence } => {
            if sequence.is_empty() {
                shortcut_field(ui, id.with("combo"), combo);
            } else {
                ui.label(format!(
                    "Sequence: {}",
                    sequence
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
                if w::small_button(ui, "Use a single shortcut instead").clicked() {
                    *combo = sequence.first().cloned();
                    sequence.clear();
                }
            }
        }
        Action::CloseWindow { force } => {
            ui.horizontal(|ui| {
                w::switch(ui, force);
                ui.label("Terminate the program instead of asking it to close");
            });
        }
        Action::Run {
            command,
            args,
            hidden,
            cwd,
        } => {
            ui.horizontal(|ui| {
                w::text_field(ui, command, "program, e.g. wt.exe", 280.0);
                if w::button(ui, icons::FOLDER_OPEN, "Browse").clicked()
                    && let Some(p) = rfd::FileDialog::new()
                        .add_filter("Programs", &["exe", "bat", "cmd", "ps1"])
                        .pick_file()
                {
                    *command = p.display().to_string();
                }
            });
            let mut a = args.join(" ");
            if w::text_field(ui, &mut a, "arguments, separated by spaces", 360.0).changed() {
                *args = a.split_whitespace().map(str::to_string).collect();
            }
            let mut dir = cwd.clone().unwrap_or_default();
            if w::text_field(ui, &mut dir, "working folder (optional)", 360.0).changed() {
                *cwd = (!dir.trim().is_empty()).then_some(dir);
            }
            ui.horizontal(|ui| {
                w::switch(ui, hidden);
                ui.label("Without a console window");
            });
        }
        Action::Media { key } => {
            egui::ComboBox::from_id_salt(id.with("media"))
                .selected_text(media_label(*key))
                .show_ui(ui, |ui| {
                    for m in [
                        MediaKey::PlayPause,
                        MediaKey::Next,
                        MediaKey::Previous,
                        MediaKey::Stop,
                        MediaKey::VolumeUp,
                        MediaKey::VolumeDown,
                        MediaKey::Mute,
                    ] {
                        ui.selectable_value(key, m, media_label(m));
                    }
                });
        }
        Action::Notify { title, message } => {
            let mut t = title.clone().unwrap_or_default();
            if w::text_field(ui, &mut t, "title (optional)", 360.0).changed() {
                *title = (!t.is_empty()).then_some(t);
            }
            w::text_field(ui, message, "message", 360.0);
        }
        Action::Wait { ms } => {
            ui.add(egui::DragValue::new(ms).range(10..=60_000).suffix(" ms"));
        }
        Action::Chain { actions } => chain_form(ui, cx, id, actions),
        Action::Open { target } => {
            ui.horizontal(|ui| {
                w::text_field(ui, target, "https://…, a file or a folder", 300.0);
                if w::button(ui, icons::FOLDER_OPEN, "File…").clicked()
                    && let Some(p) = rfd::FileDialog::new().pick_file()
                {
                    *target = p.display().to_string();
                }
                if w::button(ui, icons::FOLDER_OPEN, "Folder…").clicked()
                    && let Some(p) = rfd::FileDialog::new().pick_folder()
                {
                    *target = p.display().to_string();
                }
            });
            w::hint(ui, "Opens it with the program Windows uses for it.");
        }
        Action::TypeText { text } => {
            ui.add(
                egui::TextEdit::multiline(text)
                    .hint_text("The text to type")
                    .desired_rows(3)
                    .desired_width(ui.available_width()),
            );
            w::hint(
                ui,
                "Typed into the window that has the focus; a new line presses Enter.",
            );
        }
        Action::Window { op } => {
            w::dropdown(
                ui,
                id.with("window-op"),
                op,
                &[
                    (
                        WindowOp::SnapLeft,
                        "Fill the left half of the screen".to_string(),
                    ),
                    (
                        WindowOp::SnapRight,
                        "Fill the right half of the screen".to_string(),
                    ),
                    (
                        WindowOp::NextMonitor,
                        "Move to the next monitor".to_string(),
                    ),
                    (
                        WindowOp::ToggleAlwaysOnTop,
                        "Keep on top, on or off".to_string(),
                    ),
                ],
                300.0,
            );
        }
        Action::VirtualDesktop { op } => {
            w::segmented(
                ui,
                op,
                &[
                    (DesktopOp::Previous, "Previous"),
                    (DesktopOp::Next, "Next"),
                    (DesktopOp::New, "New desktop"),
                ],
            );
        }
        Action::Screenshot { mode } => {
            w::segmented(
                ui,
                mode,
                &[
                    (ScreenshotMode::Region, "Choose an area"),
                    (ScreenshotMode::Screen, "Whole screen"),
                ],
            );
            w::hint(
                ui,
                "An area opens Windows' snipping overlay; the whole screen is saved in Pictures → Screenshots.",
            );
        }
        Action::SwitchProfile { profile } => {
            let names: Vec<(String, String)> = cx
                .shared
                .config
                .profile_names()
                .into_iter()
                .map(|p| (p.clone(), p))
                .collect();
            w::dropdown(ui, id.with("profile-pick"), profile, &names, 220.0);
            w::text_field(ui, profile, "or a profile name", 220.0);
        }
        _ => {}
    }
    if let Err(e) = action.validate() {
        w::error_text(ui, &e);
    }
}

fn media_label(m: MediaKey) -> &'static str {
    match m {
        MediaKey::PlayPause => "Play / pause",
        MediaKey::Next => "Next track",
        MediaKey::Previous => "Previous track",
        MediaKey::Stop => "Stop",
        MediaKey::VolumeUp => "Volume up",
        MediaKey::VolumeDown => "Volume down",
        MediaKey::Mute => "Mute",
    }
}

fn chain_form(ui: &mut egui::Ui, cx: &Ctx, id: Id, actions: &mut Vec<Action>) {
    let p = pal(ui);
    let n = actions.len();
    let (mut remove, mut swap) = (None, None);
    for (i, a) in actions.iter_mut().enumerate() {
        egui::Frame::new()
            .fill(p.bg)
            .corner_radius(egui::CornerRadius::same(theme::radius::CARD))
            .inner_margin(egui::Margin::same(space::M as i8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    w::badge(ui, &format!("{}", i + 1), Tone::Neutral);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if w::icon_button_tone(ui, icons::TRASH, "Remove", Tone::Bad).clicked() {
                            remove = Some(i);
                        }
                        if ui
                            .add_enabled(
                                i + 1 < n,
                                egui::Button::new(w::icon_text(
                                    icons::ARROW_DOWN,
                                    14.0,
                                    p.text_dim,
                                ))
                                .frame(false),
                            )
                            .on_hover_text("Move down")
                            .clicked()
                        {
                            swap = Some((i, i + 1));
                        }
                        if ui
                            .add_enabled(
                                i > 0,
                                egui::Button::new(w::icon_text(icons::ARROW_UP, 14.0, p.text_dim))
                                    .frame(false),
                            )
                            .on_hover_text("Move up")
                            .clicked()
                        {
                            swap = Some((i, i - 1));
                        }
                    });
                });
                action_form(ui, cx, id.with(("chain", i)), a, true);
            });
        ui.add_space(space::S);
    }
    if let Some((a, b)) = swap {
        actions.swap(a, b);
    }
    if let Some(i) = remove {
        actions.remove(i);
    }
    ui.horizontal(|ui| {
        if w::button(ui, icons::PLUS, "Action").clicked() {
            actions.push(default_action("keys"));
        }
        if w::button(ui, icons::TIMER, "Pause").clicked() {
            actions.push(default_action("wait"));
        }
    });
}

/// A list of application names (executables), with removal, a picker over
/// the open windows and manual entry.
pub fn app_list(ui: &mut egui::Ui, cx: &Ctx, id: Id, apps: &mut Vec<String>) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
        let mut remove = None;
        for (i, a) in apps.iter().enumerate() {
            if w::tag(ui, a) {
                remove = Some(i);
            }
        }
        if let Some(i) = remove {
            apps.remove(i);
        }
    });
    ui.horizontal(|ui| {
        let add = egui::ComboBox::from_id_salt(id.with("pick-app"))
            .selected_text(format!("{}  Add an open app", icons::PLUS))
            .width(200.0)
            .show_ui(ui, |ui| {
                let mut seen = std::collections::BTreeSet::new();
                for a in &cx.shared.open_apps {
                    if seen.insert(a.exe.to_lowercase())
                        && ui
                            .selectable_label(false, format!("{}  —  {}", a.exe, a.title))
                            .clicked()
                        && !apps.iter().any(|x| x.eq_ignore_ascii_case(&a.exe))
                    {
                        apps.push(a.exe.clone());
                    }
                }
                if cx.shared.open_apps.is_empty() {
                    ui.label(w::dim(ui, "Open windows are being listed…"));
                }
            });
        if add.response.clicked() {
            cx.send(Command::ListWindows);
        }
        let typed_id = id.with("typed-app");
        let mut typed: String = ui.data_mut(|d| d.get_temp(typed_id)).unwrap_or_default();
        let r = w::text_field(ui, &mut typed, "or type name.exe, then Enter", 190.0);
        if r.lost_focus()
            && ui.input(|i| i.key_pressed(egui::Key::Enter))
            && !typed.trim().is_empty()
        {
            let t = typed.trim().to_string();
            if !apps.iter().any(|x| x.eq_ignore_ascii_case(&t)) {
                apps.push(t);
            }
            typed.clear();
        }
        ui.data_mut(|d| d.insert_temp(typed_id, typed));
    });
}

/// Where and when the gesture works: apps, window title, profile.
pub fn conditions_form(
    ui: &mut egui::Ui,
    cx: &Ctx,
    id: Id,
    def: &mut GestureDef,
    profiles: &[String],
) {
    let when = def.when.get_or_insert_with(When::default);
    let mut apps = when.app.as_ref().map(OneOrMany::to_vec).unwrap_or_default();
    w::row(
        ui,
        "Only in these apps",
        "Leave empty to work everywhere.",
        |ui| app_list(ui, cx, id, &mut apps),
    );
    when.app = match apps.len() {
        0 => None,
        1 => Some(OneOrMany::One(apps[0].clone())),
        _ => Some(OneOrMany::Many(apps)),
    };
    w::row(
        ui,
        "Window title",
        "A regular expression, optional.",
        |ui| {
            let mut t = when.title.clone().unwrap_or_default();
            if w::text_field(ui, &mut t, "e.g. ^Inbox", 280.0).changed() {
                when.title = (!t.trim().is_empty()).then_some(t);
            }
            if let Some(t) = &when.title
                && let Err(e) = regex_check(t)
            {
                w::error_text(ui, &e);
            }
        },
    );
    if when.app.is_none() && when.title.is_none() {
        def.when = None;
    }
    w::row(
        ui,
        "Profile",
        "The gesture works only while it is active.",
        |ui| {
            egui::ComboBox::from_id_salt(id.with("profile"))
                .selected_text(def.profile.clone().unwrap_or_else(|| "All profiles".into()))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut def.profile, None, "All profiles");
                    for p in profiles {
                        ui.selectable_value(&mut def.profile, Some(p.clone()), p);
                    }
                });
        },
    );
    w::row(
        ui,
        "Repeatable",
        "Whether “Repeat the last gesture” may run it again.",
        |ui| {
            w::segmented(
                ui,
                &mut def.repeatable,
                &[(None, "Default"), (Some(true), "Yes"), (Some(false), "No")],
            );
        },
    );
}

fn regex_check(t: &str) -> Result<(), String> {
    // The same check as the configuration's, through a throwaway gesture.
    let toml = format!(
        "[[gesture]]\nname = \"x\"\ntype = \"tap\"\nfingers = 1\nwhen = {{ title = {} }}\naction = {{ type = \"lock_screen\" }}\n",
        toml::Value::String(t.to_string())
    );
    glyphpad_core::config::parse(&toml)
        .map(|_| ())
        .map_err(|e| e.first().map_or_else(String::new, |d| d.message.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captured_keys_map_to_glyphpad_names() {
        assert_eq!(key_name(egui::Key::A).as_deref(), Some("A"));
        assert_eq!(key_name(egui::Key::F4).as_deref(), Some("F4"));
        assert_eq!(key_name(egui::Key::ArrowUp).as_deref(), Some("Up"));
        assert_eq!(key_name(egui::Key::Num7).as_deref(), Some("7"));
        assert_eq!(key_name(egui::Key::Tab).as_deref(), Some("Tab"));
        assert_eq!(
            key_name(egui::Key::OpenBracket).as_deref(),
            Some("BracketLeft")
        );
    }

    #[test]
    fn every_action_has_an_icon_and_a_label() {
        for (kind, _, _) in ACTIONS {
            assert_ne!(action_icon(kind), icons::TIMER, "{kind}");
            assert_ne!(action_label(kind), "Pause", "{kind}");
        }
    }
}
