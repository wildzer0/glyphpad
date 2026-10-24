//! Form widgets for gestures: the trigger, the action and the conditions
//! (SPEC §10.2).

use eframe::egui::{self, Color32, Id, RichText};
use glyphpad_core::action::{Action, MediaKey};
use glyphpad_core::config::{Direction, Edge, GestureDef, GestureKind, OneOrMany, When};
use glyphpad_core::keys::KeyCombo;
use glyphpad_core::service::Command;
use glyphpad_core::trigger::{TriggerDef, Turn, Zoom};

use crate::Ctx;
use crate::style;

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

const DIRECTIONS: [Direction; 8] = [
    Direction::Up,
    Direction::Down,
    Direction::Left,
    Direction::Right,
    Direction::UpLeft,
    Direction::UpRight,
    Direction::DownLeft,
    Direction::DownRight,
];

pub fn kind_label(k: GestureKind) -> String {
    let l = k.label();
    let mut c = l.chars();
    c.next().map_or(String::new(), |f| {
        f.to_uppercase().collect::<String>() + c.as_str()
    })
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

fn direction_combo(ui: &mut egui::Ui, id: Id, label: &str, d: &mut Direction) {
    egui::ComboBox::from_id_salt(id.with(label))
        .selected_text(d.to_string().replace('_', "-"))
        .show_ui(ui, |ui| {
            for x in DIRECTIONS {
                ui.selectable_value(d, x, x.to_string().replace('_', "-"));
            }
        });
}

fn fingers(ui: &mut egui::Ui, v: &mut Option<u8>, min: u8) {
    let mut n = v.unwrap_or(3).max(min);
    ui.add(egui::Slider::new(&mut n, min..=glyphpad_core::MAX_CONTACTS as u8).text("fingers"));
    *v = Some(n);
}

fn row(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.add_sized([110.0, 20.0], egui::Label::new(RichText::new(label).weak()));
        add(ui);
    });
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
    row(ui, "Type", |ui| {
        egui::ComboBox::from_id_salt(id.with("kind"))
            .selected_text(kind_label(kind))
            .show_ui(ui, |ui| {
                for k in KINDS
                    .into_iter()
                    .filter(|k| allow_sequence || *k != GestureKind::Sequence)
                {
                    if ui.selectable_label(k == kind, kind_label(k)).clicked() && k != kind {
                        *t = reset_for(k, t);
                    }
                }
            });
    });
    let kind = t.kind.unwrap_or(GestureKind::Tap);
    match kind {
        GestureKind::Tap => {
            row(ui, "Fingers", |ui| fingers(ui, &mut t.fingers, 1));
            row(ui, "Taps", |ui| {
                let mut taps = t.taps.unwrap_or(1);
                for n in 1..=3 {
                    ui.radio_value(
                        &mut taps,
                        n,
                        ["single", "double", "triple"][usize::from(n - 1)],
                    );
                }
                t.taps = (taps > 1).then_some(taps);
            });
            chord(ui, t);
        }
        GestureKind::Hold => row(ui, "Fingers", |ui| fingers(ui, &mut t.fingers, 1)),
        GestureKind::Swipe => {
            row(ui, "Fingers", |ui| fingers(ui, &mut t.fingers, 1));
            row(ui, "Direction", |ui| {
                let mut d = t.direction.unwrap_or(Direction::Up);
                direction_combo(ui, id, "dir", &mut d);
                t.direction = Some(d);
            });
            chord(ui, t);
        }
        GestureKind::EdgeSwipe => {
            row(ui, "Fingers", |ui| fingers(ui, &mut t.fingers, 1));
            row(ui, "From edge", |ui| {
                let mut e = t.edge.unwrap_or_default();
                for x in [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom] {
                    ui.radio_value(&mut e, x, format!("{x:?}").to_lowercase());
                }
                t.edge = Some(e);
            });
            row(ui, "Direction", |ui| {
                let mut d = t.direction.unwrap_or(Direction::Right);
                direction_combo(ui, id, "dir", &mut d);
                t.direction = Some(d);
            });
        }
        GestureKind::Pinch => {
            row(ui, "Fingers", |ui| fingers(ui, &mut t.fingers, 2));
            row(ui, "Fingers move", |ui| {
                let mut z = t.zoom.unwrap_or(Zoom::In);
                ui.radio_value(&mut z, Zoom::In, "closer (pinch in)");
                ui.radio_value(&mut z, Zoom::Out, "apart (pinch out)");
                t.zoom = Some(z);
            });
        }
        GestureKind::Rotate => {
            row(ui, "Fingers", |ui| fingers(ui, &mut t.fingers, 2));
            row(ui, "Turn", |ui| {
                let mut r = t.rotation.unwrap_or(Turn::Clockwise);
                ui.radio_value(&mut r, Turn::Clockwise, "clockwise");
                ui.radio_value(&mut r, Turn::Counterclockwise, "counterclockwise");
                t.rotation = Some(r);
            });
        }
        GestureKind::Click => {
            row(ui, "Fingers", |ui| fingers(ui, &mut t.fingers, 1));
            row(ui, "Drag", |ui| {
                let mut drag = t.direction.is_some();
                ui.checkbox(&mut drag, "click and drag");
                if drag {
                    let mut d = t.direction.unwrap_or(Direction::Down);
                    direction_combo(ui, id, "drag", &mut d);
                    t.direction = Some(d);
                } else {
                    t.direction = None;
                }
            });
            ui.weak("The click itself still reaches Windows: raw input cannot hold it back.");
        }
        GestureKind::Symbol => {
            let mut chosen = t.symbol.as_ref().map(OneOrMany::to_vec).unwrap_or_default();
            row(ui, "Symbols", |ui| {
                ui.horizontal_wrapped(|ui| {
                    for s in symbols {
                        let mut on = chosen.contains(s);
                        if ui.toggle_value(&mut on, s).changed() {
                            if on {
                                chosen.push(s.clone());
                            } else {
                                chosen.retain(|x| x != s);
                            }
                        }
                    }
                    for s in chosen.clone().iter().filter(|s| !symbols.contains(s)) {
                        ui.colored_label(style::WARN, s)
                            .on_hover_text("No templates yet: record some in Review gestures");
                    }
                });
            });
            t.symbol = match chosen.len() {
                0 => None,
                1 => Some(OneOrMany::One(chosen[0].clone())),
                _ => Some(OneOrMany::Many(chosen)),
            };
            row(ui, "Minimum score", |ui| {
                let mut c = t.confidence.unwrap_or(default_confidence);
                if ui.add(egui::Slider::new(&mut c, 0.5..=0.99)).changed() {
                    t.confidence = Some(c);
                }
                if t.confidence.is_some() {
                    if ui.small_button("Restore default").clicked() {
                        t.confidence = None;
                    }
                } else {
                    ui.weak(format!("default {default_confidence:.2}"));
                }
            });
        }
        GestureKind::Sequence => {}
    }
}

fn chord(ui: &mut egui::Ui, t: &mut TriggerDef) {
    row(ui, "Chord", |ui| {
        let mut on = t.modifier_fingers.is_some();
        ui.checkbox(&mut on, "while other fingers rest");
        if on {
            let mut m = t.modifier_fingers.unwrap_or(1);
            ui.add(egui::Slider::new(&mut m, 1..=3).text("resting"));
            t.modifier_fingers = Some(m);
            if t.taps.is_some() {
                t.taps = None;
            }
        } else {
            t.modifier_fingers = None;
        }
    });
}

/// The steps of a sequence and the pause allowed between them.
pub fn sequence_form(
    ui: &mut egui::Ui,
    id: Id,
    def: &mut GestureDef,
    symbols: &[String],
    default_confidence: f32,
) {
    let steps = def.steps.get_or_insert_with(Vec::new);
    let mut remove = None;
    let mut swap = None;
    let n = steps.len();
    for (i, step) in steps.iter_mut().enumerate() {
        style::card(ui, &format!("Step {}", i + 1), |ui| {
            trigger_form(
                ui,
                id.with(("step", i)),
                step,
                symbols,
                default_confidence,
                false,
            );
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(i > 0, egui::Button::new("↑").small())
                    .clicked()
                {
                    swap = Some((i, i - 1));
                }
                if ui
                    .add_enabled(i + 1 < n, egui::Button::new("↓").small())
                    .clicked()
                {
                    swap = Some((i, i + 1));
                }
                if ui.small_button("Remove step").clicked() {
                    remove = Some(i);
                }
            });
        });
    }
    if let Some((a, b)) = swap {
        steps.swap(a, b);
    }
    if let Some(i) = remove {
        steps.remove(i);
    }
    ui.horizontal(|ui| {
        if ui
            .add_enabled(steps.len() < 4, egui::Button::new("+ Add step"))
            .clicked()
        {
            steps.push(TriggerDef {
                kind: Some(GestureKind::Tap),
                fingers: Some(3),
                ..Default::default()
            });
        }
        let mut within = def
            .within_ms
            .unwrap_or(glyphpad_core::config::DEFAULT_SEQUENCE_WITHIN_MS);
        ui.label("each within");
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
    });
    ui.weak("The first step's own gesture waits for the next step before it runs.");
}

const ACTIONS: &[(&str, &str)] = &[
    ("keys", "Press a shortcut"),
    ("lock_screen", "Lock the PC"),
    ("close_window", "Close the window in front"),
    ("minimize", "Minimize the window"),
    ("maximize", "Maximize / restore the window"),
    ("run", "Run a program"),
    ("media", "Media key"),
    ("chain", "Several actions in a row"),
    ("notify", "Show a message"),
    ("toggle_enabled", "Pause / resume gestures"),
    ("repeat_last", "Repeat the last gesture"),
    ("show_window", "Show the GlyphPad window"),
    ("draw_symbol", "Draw a symbol with one finger next"),
];

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
    let text_id = id.with("text");
    let capture_id = id.with("capture");
    let mut text: String = ui
        .data_mut(|d| d.get_temp(text_id))
        .unwrap_or_else(|| combo.as_ref().map(ToString::to_string).unwrap_or_default());
    let mut capturing: bool = ui.data_mut(|d| d.get_temp(capture_id)).unwrap_or(false);
    ui.horizontal(|ui| {
        let edit = ui.add(
            egui::TextEdit::singleline(&mut text)
                .desired_width(150.0)
                .hint_text("e.g. Ctrl+Shift+T"),
        );
        if edit.changed()
            && let Ok(c) = text.parse::<KeyCombo>()
        {
            *combo = Some(c);
        }
        if ui
            .selectable_label(
                capturing,
                if capturing {
                    "Press the shortcut…"
                } else {
                    "Record"
                },
            )
            .on_hover_text("Press the key combination to use")
            .clicked()
        {
            capturing = !capturing;
        }
        if capturing {
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
    if !valid && !text.is_empty() {
        ui.colored_label(style::BAD, "Not a valid shortcut");
    }
    if text.eq_ignore_ascii_case("win+l") {
        ui.colored_label(
            style::WARN,
            "Win+L cannot be simulated; use “Lock the PC” instead.",
        );
    }
    ui.weak("The Windows key and some system shortcuts are taken by Windows before GlyphPad sees them: type those.");
    ui.data_mut(|d| {
        d.insert_temp(text_id, text);
        d.insert_temp(capture_id, capturing);
    });
}

/// Edits an action; `nested` hides the kinds a chain cannot contain.
pub fn action_form(ui: &mut egui::Ui, cx: &Ctx, id: Id, action: &mut Action, nested: bool) {
    let kind = action.kind();
    ui.horizontal(|ui| {
        let label = ACTIONS
            .iter()
            .find(|(k, _)| *k == kind)
            .map_or(if kind == "wait" { "Pause" } else { kind }, |(_, l)| *l);
        egui::ComboBox::from_id_salt(id.with("kind"))
            .width(230.0)
            .selected_text(label)
            .show_ui(ui, |ui| {
                for (k, l) in ACTIONS {
                    if nested
                        && matches!(
                            *k,
                            "chain"
                                | "repeat_last"
                                | "draw_symbol"
                                | "toggle_enabled"
                                | "show_window"
                        )
                    {
                        continue;
                    }
                    if ui.selectable_label(*k == kind, *l).clicked() && *k != kind {
                        *action = default_action(k);
                    }
                }
                if nested
                    && ui.selectable_label(kind == "wait", "Pause").clicked()
                    && kind != "wait"
                {
                    *action = default_action("wait");
                }
            });
        if !nested
            && ui
                .button("▶ Run now")
                .on_hover_text("Try the action")
                .clicked()
        {
            cx.send(Command::RunAction(action.clone()));
        }
    });
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
                if ui.small_button("Use a single shortcut instead").clicked() {
                    *combo = sequence.first().cloned();
                    sequence.clear();
                }
            }
        }
        Action::CloseWindow { force } => {
            ui.checkbox(force, "Terminate the program instead of asking it to close");
        }
        Action::Run {
            command,
            args,
            hidden,
            cwd,
        } => {
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(command)
                        .desired_width(260.0)
                        .hint_text("program, e.g. wt.exe"),
                );
                if ui.button("Browse…").clicked()
                    && let Some(p) = rfd::FileDialog::new()
                        .add_filter("Programs", &["exe", "bat", "cmd", "ps1"])
                        .pick_file()
                {
                    *command = p.display().to_string();
                }
            });
            let mut a = args.join(" ");
            if ui
                .add(
                    egui::TextEdit::singleline(&mut a)
                        .desired_width(320.0)
                        .hint_text("arguments, separated by spaces"),
                )
                .changed()
            {
                *args = a.split_whitespace().map(str::to_string).collect();
            }
            let mut dir = cwd.clone().unwrap_or_default();
            if ui
                .add(
                    egui::TextEdit::singleline(&mut dir)
                        .desired_width(320.0)
                        .hint_text("working folder (optional)"),
                )
                .changed()
            {
                *cwd = (!dir.trim().is_empty()).then_some(dir);
            }
            ui.checkbox(hidden, "Without a console window");
        }
        Action::Media { key } => {
            egui::ComboBox::from_id_salt(id.with("media"))
                .selected_text(format!("{key:?}"))
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
                        ui.selectable_value(key, m, format!("{m:?}"));
                    }
                });
        }
        Action::Notify { title, message } => {
            let mut t = title.clone().unwrap_or_default();
            if ui
                .add(egui::TextEdit::singleline(&mut t).hint_text("title (optional)"))
                .changed()
            {
                *title = (!t.is_empty()).then_some(t);
            }
            ui.add(egui::TextEdit::singleline(message).hint_text("message"));
        }
        Action::Wait { ms } => {
            ui.add(egui::DragValue::new(ms).range(10..=60_000).suffix(" ms"));
        }
        Action::Chain { actions } => {
            let n = actions.len();
            let (mut remove, mut swap) = (None, None);
            for (i, a) in actions.iter_mut().enumerate() {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.weak(format!("{}.", i + 1));
                        if ui
                            .add_enabled(i > 0, egui::Button::new("↑").small())
                            .clicked()
                        {
                            swap = Some((i, i - 1));
                        }
                        if ui
                            .add_enabled(i + 1 < n, egui::Button::new("↓").small())
                            .clicked()
                        {
                            swap = Some((i, i + 1));
                        }
                        if ui.small_button("✕").on_hover_text("Remove").clicked() {
                            remove = Some(i);
                        }
                    });
                    action_form(ui, cx, id.with(("chain", i)), a, true);
                });
            }
            if let Some((a, b)) = swap {
                actions.swap(a, b);
            }
            if let Some(i) = remove {
                actions.remove(i);
            }
            ui.horizontal(|ui| {
                if ui.button("+ Action").clicked() {
                    actions.push(default_action("keys"));
                }
                if ui.button("+ Pause").clicked() {
                    actions.push(default_action("wait"));
                }
            });
        }
        _ => {}
    }
    if let Err(e) = action.validate() {
        ui.colored_label(style::BAD, e);
    }
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
    row(ui, "Only in apps", |ui| {
        ui.vertical(|ui| {
            ui.horizontal_wrapped(|ui| {
                let mut remove = None;
                for (i, a) in apps.iter().enumerate() {
                    if ui
                        .button(format!("{a}  ✕"))
                        .on_hover_text("Remove")
                        .clicked()
                    {
                        remove = Some(i);
                    }
                }
                if let Some(i) = remove {
                    apps.remove(i);
                }
                if apps.is_empty() {
                    ui.weak("everywhere");
                }
            });
            ui.horizontal(|ui| {
                let add = egui::ComboBox::from_id_salt(id.with("pick-app"))
                    .selected_text("Add an open app…")
                    .width(220.0)
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
                            ui.weak("No list yet");
                        }
                    });
                if add.response.clicked() {
                    cx.send(Command::ListWindows);
                }
                let typed_id = id.with("typed-app");
                let mut typed: String = ui.data_mut(|d| d.get_temp(typed_id)).unwrap_or_default();
                let r = ui.add(
                    egui::TextEdit::singleline(&mut typed)
                        .desired_width(120.0)
                        .hint_text("or type name.exe"),
                );
                if r.lost_focus()
                    && ui.input(|i| i.key_pressed(egui::Key::Enter))
                    && !typed.trim().is_empty()
                {
                    apps.push(typed.trim().to_string());
                    typed.clear();
                }
                ui.data_mut(|d| d.insert_temp(typed_id, typed));
            });
        });
    });
    when.app = match apps.len() {
        0 => None,
        1 => Some(OneOrMany::One(apps[0].clone())),
        _ => Some(OneOrMany::Many(apps)),
    };
    row(ui, "Window title", |ui| {
        let mut t = when.title.clone().unwrap_or_default();
        if ui
            .add(
                egui::TextEdit::singleline(&mut t)
                    .desired_width(220.0)
                    .hint_text("regular expression (optional)"),
            )
            .changed()
        {
            when.title = (!t.trim().is_empty()).then_some(t);
        }
        if let Some(t) = &when.title
            && let Err(e) = regex_check(t)
        {
            ui.colored_label(style::BAD, e);
        }
    });
    if when.app.is_none() && when.title.is_none() {
        def.when = None;
    }
    row(ui, "Profile", |ui| {
        egui::ComboBox::from_id_salt(id.with("profile"))
            .selected_text(def.profile.clone().unwrap_or_else(|| "all profiles".into()))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut def.profile, None, "all profiles");
                for p in profiles {
                    ui.selectable_value(&mut def.profile, Some(p.clone()), p);
                }
            });
    });
    row(ui, "Repeatable", |ui| {
        egui::ComboBox::from_id_salt(id.with("repeat"))
            .selected_text(match def.repeatable {
                None => "default",
                Some(true) => "yes",
                Some(false) => "no",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut def.repeatable, None, "default");
                ui.selectable_value(&mut def.repeatable, Some(true), "yes");
                ui.selectable_value(&mut def.repeatable, Some(false), "no");
            });
        ui.weak("by “Repeat the last gesture”");
    });
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

/// A colored badge for a gesture's state.
pub fn badge(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.25))
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(6, 1))
        .show(ui, |ui| ui.label(RichText::new(text).small().color(color)));
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
}
