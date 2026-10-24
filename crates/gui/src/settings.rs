//! Settings tab (SPEC §10.1): every option of `[settings]` and `[update]`,
//! grouped, with the technical ones under "Advanced". Changes stay in a
//! draft until saved; saving writes only what changed, keeping the file's
//! comments.

use eframe::egui;
use glyphpad_core::config::{
    Corner, DrawActivation, Edge, Settings, UpdateChannel, UpdateMode, UpdateSettings,
};
use glyphpad_core::service::Command;

use crate::Ctx;

#[derive(Default)]
pub struct State {
    /// The values the draft started from.
    base: Option<(Settings, UpdateSettings)>,
    draft: Option<(Settings, UpdateSettings)>,
}

/// Leaves of a serialized table as paths; `draw_activation` is one value
/// because its keys depend on its mode.
fn flatten(prefix: &[String], v: &toml::Value, out: &mut Vec<(Vec<String>, toml::Value)>) {
    match v {
        toml::Value::Table(t) if prefix.last().is_none_or(|k| k != "draw_activation") => {
            for (k, x) in t {
                let mut p = prefix.to_vec();
                p.push(k.clone());
                flatten(&p, x, out);
            }
        }
        other => out.push((prefix.to_vec(), other.clone())),
    }
}

fn leaves(section: &str, v: &impl serde::Serialize) -> Vec<(Vec<String>, toml::Value)> {
    let mut out = Vec::new();
    if let Ok(v) = toml::Value::try_from(v) {
        flatten(&[section.to_string()], &v, &mut out);
    }
    out
}

/// Commands that turn `base` into `draft` in the file.
fn changes(base: &(Settings, UpdateSettings), draft: &(Settings, UpdateSettings)) -> Vec<Command> {
    let mut out = Vec::new();
    for (section, b, d) in [
        (
            "settings",
            leaves("settings", &base.0),
            leaves("settings", &draft.0),
        ),
        (
            "update",
            leaves("update", &base.1),
            leaves("update", &draft.1),
        ),
    ] {
        let _ = section;
        for (path, value) in &d {
            if !b.iter().any(|(p, v)| p == path && v == value) {
                out.push(Command::SetSetting {
                    path: path.clone(),
                    value: value.clone(),
                });
            }
        }
        for (path, _) in &b {
            if !d.iter().any(|(p, _)| p == path) {
                out.push(Command::RemoveSetting { path: path.clone() });
            }
        }
    }
    out
}

fn ms(ui: &mut egui::Ui, label: &str, v: &mut u64, range: std::ops::RangeInclusive<u64>) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::DragValue::new(v).range(range).suffix(" ms"));
    });
}

fn mm(ui: &mut egui::Ui, label: &str, v: &mut f32, range: std::ops::RangeInclusive<f32>) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(
            egui::DragValue::new(v)
                .range(range)
                .speed(0.1)
                .suffix(" mm"),
        );
    });
}

pub fn show(ui: &mut egui::Ui, cx: &Ctx, st: &mut State) {
    let current = (
        cx.shared.config.settings.clone(),
        cx.shared.config.update.clone(),
    );
    let dirty = st.draft.is_some() && st.draft != st.base;
    // Pick up changes made elsewhere (file edited, wizard) when not editing.
    if !dirty && st.base.as_ref() != Some(&current) {
        st.base = Some(current.clone());
        st.draft = Some(current.clone());
    }
    let external_change = dirty && st.base.as_ref() != Some(&current);

    ui.horizontal(|ui| {
        ui.heading("Settings");
        if dirty {
            ui.colored_label(egui::Color32::from_rgb(240, 200, 80), "● unsaved changes");
        }
        if ui
            .add_enabled(dirty, egui::Button::new("💾 Save"))
            .clicked()
            && let (Some(b), Some(d)) = (&st.base, &st.draft)
        {
            for c in changes(b, d) {
                cx.send(c);
            }
            st.base = st.draft.clone();
        }
        if ui
            .add_enabled(dirty, egui::Button::new("Discard changes"))
            .clicked()
        {
            st.draft = Some(current.clone());
            st.base = Some(current.clone());
        }
    });
    if external_change {
        ui.colored_label(
            egui::Color32::from_rgb(240, 160, 80),
            "The configuration changed on disk meanwhile; saving writes only the values you changed here.",
        );
    }
    ui.separator();
    let Some((s, u)) = st.draft.as_mut() else {
        return;
    };

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.collapsing("Window and tray", |ui| {
            ui.checkbox(
                &mut s.close_to_tray,
                "Closing the window keeps GlyphPad running in the tray",
            );
            ui.checkbox(&mut s.start_minimized, "Start in the tray");
            ui.horizontal(|ui| {
                ui.label("History length");
                ui.add(egui::DragValue::new(&mut s.history_size).range(1..=1000));
            });
            ui.checkbox(
                &mut s.save_history,
                "Also save the history to history.jsonl",
            );
            ui.weak("Start with Windows: comes with the full settings editor.");
        })
        .header_response
        .on_hover_text("SPEC §10.1");

        ui.collapsing("Drawn symbols", |ui| {
            let mode = match s.draw_activation {
                DrawActivation::Fingers { .. } => 0,
                DrawActivation::Edge { .. } => 1,
                DrawActivation::Hold { .. } => 2,
                DrawActivation::Toggle { .. } => 3,
            };
            let mut new_mode = mode;
            egui::ComboBox::from_label("Draw with")
                .selected_text(
                    [
                        "several fingers",
                        "one finger from an edge",
                        "one finger after a rest",
                        "one finger after a gesture",
                    ][mode],
                )
                .show_ui(ui, |ui| {
                    for (i, name) in [
                        "several fingers",
                        "one finger from an edge",
                        "one finger after a rest",
                        "one finger after a gesture",
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        ui.selectable_value(&mut new_mode, i, name);
                    }
                });
            if new_mode != mode {
                s.draw_activation = match new_mode {
                    0 => DrawActivation::Fingers { count: 3 },
                    1 => DrawActivation::Edge {
                        edge: Edge::Left,
                        zone_percent: 10.0,
                        freeze_cursor: false,
                    },
                    2 => DrawActivation::Hold {
                        ms: 400,
                        freeze_cursor: false,
                    },
                    _ => DrawActivation::Toggle {
                        timeout_ms: 5000,
                        freeze_cursor: false,
                    },
                };
            }
            match &mut s.draw_activation {
                DrawActivation::Fingers { count } => {
                    ui.add(egui::Slider::new(count, 2..=5).text("fingers"));
                    ui.weak("Turn off Windows' own gestures for this many fingers.");
                }
                DrawActivation::Edge {
                    edge,
                    zone_percent,
                    freeze_cursor,
                } => {
                    egui::ComboBox::from_label("Edge")
                        .selected_text(format!("{edge:?}"))
                        .show_ui(ui, |ui| {
                            for e in [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom] {
                                ui.selectable_value(edge, e, format!("{e:?}"));
                            }
                        });
                    ui.add(
                        egui::Slider::new(zone_percent, 1.0..=50.0)
                            .text("zone")
                            .suffix(" %"),
                    );
                    ui.checkbox(
                        freeze_cursor,
                        "Freeze the pointer while drawing (experimental)",
                    );
                }
                DrawActivation::Hold {
                    ms: rest,
                    freeze_cursor,
                } => {
                    ms(ui, "Rest before drawing", rest, 100..=5000);
                    ui.checkbox(
                        freeze_cursor,
                        "Freeze the pointer while drawing (experimental)",
                    );
                }
                DrawActivation::Toggle {
                    timeout_ms,
                    freeze_cursor,
                } => {
                    ms(ui, "Drawing stays armed for", timeout_ms, 500..=60_000);
                    ui.checkbox(
                        freeze_cursor,
                        "Freeze the pointer while drawing (experimental)",
                    );
                    ui.weak("Give a gesture the “draw a symbol” action to arm it.");
                }
            }
            ui.add(egui::Slider::new(&mut s.default_confidence, 0.5..=0.99).text("minimum score"));
            ms(
                ui,
                "Pause that ends a symbol",
                &mut s.stroke_gap_ms,
                100..=2000,
            );
            ui.add(
                egui::Slider::new(&mut s.rotation_tolerance_deg, 0.0..=45.0)
                    .text("rotation tolerance")
                    .suffix("°"),
            );
        });

        ui.collapsing("Feedback overlay", |ui| {
            let o = &mut s.overlay;
            ui.checkbox(
                &mut o.enabled,
                "Show the stroke and the recognized gesture on screen",
            );
            egui::ComboBox::from_label("Position")
                .selected_text(format!("{:?}", o.position))
                .show_ui(ui, |ui| {
                    for c in [
                        Corner::TopLeft,
                        Corner::TopRight,
                        Corner::BottomLeft,
                        Corner::BottomRight,
                    ] {
                        ui.selectable_value(&mut o.position, c, format!("{c:?}"));
                    }
                });
            ms(ui, "Show for", &mut o.duration_ms, 100..=30_000);
            ui.add(egui::Slider::new(&mut o.opacity, 0.1..=1.0).text("opacity"));
            ui.add(
                egui::Slider::new(&mut o.width_px, 120..=600)
                    .text("width")
                    .suffix(" px"),
            );
        });

        ui.collapsing("Repeat last gesture", |ui| {
            ui.checkbox(
                &mut s.repeat_same_app_only,
                "Only in the application where it ran",
            );
            let mut expires = s.repeat_timeout_s.is_some();
            ui.horizontal(|ui| {
                ui.checkbox(&mut expires, "Expires after");
                let mut secs = s.repeat_timeout_s.unwrap_or(60);
                ui.add_enabled(
                    expires,
                    egui::DragValue::new(&mut secs).range(1..=3600).suffix(" s"),
                );
                s.repeat_timeout_s = expires.then_some(secs);
            });
        });

        ui.collapsing("Updates", |ui| {
            egui::ComboBox::from_label("Mode")
                .selected_text(format!("{:?}", u.mode))
                .show_ui(ui, |ui| {
                    for m in [UpdateMode::Off, UpdateMode::Notify, UpdateMode::Auto] {
                        ui.selectable_value(&mut u.mode, m, format!("{m:?}"));
                    }
                });
            egui::ComboBox::from_label("Channel")
                .selected_text(format!("{:?}", u.channel))
                .show_ui(ui, |ui| {
                    for c in [UpdateChannel::Stable, UpdateChannel::Prerelease] {
                        ui.selectable_value(&mut u.channel, c, format!("{c:?}"));
                    }
                });
            ui.horizontal(|ui| {
                ui.label("Check every");
                ui.add(
                    egui::DragValue::new(&mut u.check_interval_h)
                        .range(1..=720)
                        .suffix(" h"),
                );
            });
            ui.add_enabled(false, egui::Button::new("Check now"))
                .on_disabled_hover_text("Automatic updates arrive in a later version (M6).");
        });

        egui::CollapsingHeader::new("Advanced")
            .default_open(false)
            .show(ui, |ui| {
                ms(ui, "Longest tap", &mut s.tap_max_duration_ms, 50..=2000);
                mm(
                    ui,
                    "Tap movement up to",
                    &mut s.tap_max_movement_mm,
                    0.5..=20.0,
                );
                ms(
                    ui,
                    "Pause between multi-taps",
                    &mut s.multi_tap_gap_ms,
                    50..=2000,
                );
                ms(ui, "Hold for", &mut s.hold_duration_ms, 100..=10_000);
                mm(
                    ui,
                    "Hold movement up to",
                    &mut s.hold_max_movement_mm,
                    0.5..=20.0,
                );
                mm(
                    ui,
                    "Shortest swipe",
                    &mut s.swipe_min_distance_mm,
                    3.0..=100.0,
                );
                ui.horizontal(|ui| {
                    ui.label("Slowest swipe");
                    ui.add(
                        egui::DragValue::new(&mut s.swipe_min_speed_mm_s)
                            .range(0.0..=1000.0)
                            .suffix(" mm/s"),
                    );
                });
                mm(ui, "Shortest stroke", &mut s.min_stroke_mm, 1.0..=50.0);
                if ui.button("Restore all defaults").clicked() {
                    let defaults = Settings::default();
                    *s = Settings {
                        active_profile: s.active_profile.clone(),
                        ..defaults
                    };
                }
            });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_changed_values_are_written() {
        let base = (Settings::default(), UpdateSettings::default());
        let mut draft = base.clone();
        draft.0.close_to_tray = false;
        draft.0.overlay.opacity = 0.5;
        draft.0.draw_activation = DrawActivation::Hold {
            ms: 300,
            freeze_cursor: true,
        };
        draft.1.mode = UpdateMode::Off;
        let paths: Vec<String> = changes(&base, &draft)
            .iter()
            .map(|c| match c {
                Command::SetSetting { path, .. } => path.join("."),
                Command::RemoveSetting { path } => format!("-{}", path.join(".")),
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(
            paths,
            [
                "settings.draw_activation",
                "settings.close_to_tray",
                "settings.overlay.opacity",
                "update.mode"
            ]
        );
        let mut d2 = base.clone();
        d2.0.repeat_timeout_s = Some(30);
        let back = changes(&(d2.0.clone(), d2.1.clone()), &base);
        assert!(
            matches!(&back[0], Command::RemoveSetting { path } if path.join(".") == "settings.repeat_timeout_s")
        );
    }
}
