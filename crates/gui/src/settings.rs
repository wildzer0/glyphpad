//! Settings page (SPEC §10.1): every option of `[settings]` and `[update]`,
//! grouped by what the user is trying to do, the technical ones under
//! "Advanced", and the backups. It edits the same draft as the Gestures
//! page, which is saved as you go.

use eframe::egui::{self, Align, Layout};
use glyphpad_core::config::{
    Corner, DrawActivation, Edge, Settings, ThemeChoice, UpdateChannel, UpdateMode,
};
use glyphpad_core::service::Command;

use crate::editor::Editor;
use crate::forms;
use crate::theme::{self, pal, space};
use crate::widgets::{self as w, Tone};
use crate::{Ctx, icons};

#[derive(Default)]
pub struct State {
    restore: Option<String>,
    advanced: bool,
}

/// A value with its default and a "Restore default" button (SPEC §10.2).
fn with_default<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    v: &mut T,
    default: T,
    add: impl FnOnce(&mut egui::Ui, &mut T),
) {
    ui.horizontal(|ui| {
        add(ui, v);
        if *v != default {
            if w::icon_button(ui, icons::ARROW_COUNTER_CLOCKWISE, "Restore the default").clicked() {
                *v = default;
            }
        } else {
            ui.label(w::faint(ui, "default"));
        }
    });
}

fn ms(
    ui: &mut egui::Ui,
    label: &str,
    hint: &str,
    v: &mut u64,
    default: u64,
    range: std::ops::RangeInclusive<u64>,
) {
    w::row(ui, label, hint, |ui| {
        with_default(ui, v, default, |ui, v| {
            ui.add(egui::DragValue::new(v).range(range).suffix(" ms"));
        })
    });
}

fn mm(
    ui: &mut egui::Ui,
    label: &str,
    hint: &str,
    v: &mut f32,
    default: f32,
    range: std::ops::RangeInclusive<f32>,
) {
    w::row(ui, label, hint, |ui| {
        with_default(ui, v, default, |ui, v| {
            ui.add(
                egui::DragValue::new(v)
                    .range(range)
                    .speed(0.1)
                    .fixed_decimals(1)
                    .suffix(" mm"),
            );
        })
    });
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Fingers,
    Edge,
    Hold,
    Toggle,
}

fn edges() -> [(Edge, &'static str); 4] {
    [
        (Edge::Left, "Left"),
        (Edge::Right, "Right"),
        (Edge::Top, "Top"),
        (Edge::Bottom, "Bottom"),
    ]
}

/// "20261025-085103-001" in a backup's name as "25 Oct 2026, 08:51:03".
fn backup_label(name: &str) -> String {
    let stamp = name
        .strip_prefix("glyphpad-")
        .and_then(|s| s.get(..15))
        .filter(|s| s.len() == 15 && s.as_bytes()[8] == b'-');
    let Some(s) = stamp else {
        return name.to_string();
    };
    let month = match &s[4..6] {
        "01" => "Jan",
        "02" => "Feb",
        "03" => "Mar",
        "04" => "Apr",
        "05" => "May",
        "06" => "Jun",
        "07" => "Jul",
        "08" => "Aug",
        "09" => "Sep",
        "10" => "Oct",
        "11" => "Nov",
        "12" => "Dec",
        _ => return name.to_string(),
    };
    format!(
        "{} {month} {}, {}:{}:{}",
        s[6..8].trim_start_matches('0'),
        &s[..4],
        &s[9..11],
        &s[11..13],
        &s[13..15]
    )
}

pub fn show(ui: &mut egui::Ui, cx: &Ctx, ed: &mut Editor, st: &mut State) {
    w::page_header(
        ui,
        "Settings",
        "Changes are saved as you make them.",
        |_| {},
    );
    let d = Settings::default();
    let draft = &mut ed.draft;
    let (s, u) = (&mut draft.settings, &mut draft.update);
    w::scroll_page(ui, "settings", |ui| {
        w::centered_column(ui, theme::PAGE_MAX_WIDTH, |ui| {
            appearance_and_window(ui, s);
            drawing(ui, s, &d);
            repeat_and_pause(ui, cx, s);
            overlay(ui, s);
            updates(ui, cx, u);
            backups(ui, cx, s, st);
            advanced(ui, s, &d, st);
        });
    });

    if let Some(name) = st.restore.clone() {
        let mut close = false;
        w::dialog(ui.ctx(), "restore", "Restore this backup?", 400.0, |ui| {
            ui.label(format!(
                "The configuration will be replaced by the one saved on {}. The current one is backed up first.",
                backup_label(&name)
            ));
            ui.add_space(space::M);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if w::primary_button(ui, icons::CLOCK_COUNTER_CLOCKWISE, "Restore").clicked() {
                    cx.send(Command::RestoreBackup { name: name.clone() });
                    ed.reset(cx.shared);
                    close = true;
                }
                if w::button(ui, "", "Cancel").clicked() {
                    close = true;
                }
            });
        });
        if close {
            st.restore = None;
        }
    }
}

fn appearance_and_window(ui: &mut egui::Ui, s: &mut Settings) {
    w::section(ui, "Appearance", "", |ui| {
        w::row(ui, "Theme", "“System” follows Windows.", |ui| {
            w::segmented_with_icons(
                ui,
                &mut s.theme,
                &[
                    (ThemeChoice::System, icons::CIRCLE_HALF, "System"),
                    (ThemeChoice::Light, icons::SUN, "Light"),
                    (ThemeChoice::Dark, icons::MOON, "Dark"),
                ],
            );
        });
    });
    w::section(ui, "Window and tray", "", |ui| {
        w::switch_row(
            ui,
            "Keep running in the tray when the window is closed",
            "Gestures keep working with the window hidden.",
            &mut s.close_to_tray,
        );
        w::switch_row(ui, "Start in the tray", "", &mut s.start_minimized);
        w::switch_row(ui, "Start with Windows", "", &mut s.autostart);
        w::row(ui, "History length", "Recognitions kept in Review.", |ui| {
            ui.add(egui::DragValue::new(&mut s.history_size).range(1..=1000));
        });
        w::switch_row(
            ui,
            "Also save the history to a file",
            "history.jsonl, next to the configuration.",
            &mut s.save_history,
        );
    });
}

fn drawing(ui: &mut egui::Ui, s: &mut Settings, d: &Settings) {
    w::section(
        ui,
        "Drawing symbols",
        "Windows moves the pointer for one finger, so a symbol needs a way to be told apart from pointing.",
        |ui| {
            let mut mode = match s.draw_activation {
                DrawActivation::Fingers { .. } => Mode::Fingers,
                DrawActivation::Edge { .. } => Mode::Edge,
                DrawActivation::Hold { .. } => Mode::Hold,
                DrawActivation::Toggle { .. } => Mode::Toggle,
            };
            let before = mode;
            w::row(ui, "Draw with", "", |ui| {
                ui.vertical(|ui| {
                    w::segmented(
                        ui,
                        &mut mode,
                        &[
                            (Mode::Fingers, "Several fingers"),
                            (Mode::Edge, "From an edge"),
                            (Mode::Hold, "After a rest"),
                            (Mode::Toggle, "After a gesture"),
                        ],
                    );
                });
            });
            if mode != before {
                s.draw_activation = match mode {
                    Mode::Fingers => DrawActivation::Fingers { count: 3 },
                    Mode::Edge => DrawActivation::Edge {
                        edge: Edge::Left,
                        zone_percent: 10.0,
                        freeze_cursor: false,
                    },
                    Mode::Hold => DrawActivation::Hold {
                        ms: 400,
                        freeze_cursor: false,
                    },
                    Mode::Toggle => DrawActivation::Toggle {
                        timeout_ms: 5000,
                        freeze_cursor: false,
                    },
                };
            }
            match &mut s.draw_activation {
                DrawActivation::Fingers { count } => {
                    w::row(
                        ui,
                        "Fingers",
                        "Turn off Windows' own gestures for this many fingers.",
                        |ui| {
                            w::stepper(ui, count, 2, 5);
                        },
                    );
                }
                DrawActivation::Edge {
                    edge,
                    zone_percent,
                    freeze_cursor,
                } => {
                    w::row(ui, "Edge", "The stroke must start there.", |ui| {
                        w::segmented(ui, edge, &edges());
                    });
                    w::row(ui, "Depth of the zone", "", |ui| {
                        w::slider(ui, egui::Slider::new(zone_percent, 1.0..=50.0).suffix(" %"));
                    });
                    freeze(ui, freeze_cursor);
                }
                DrawActivation::Hold {
                    ms: rest,
                    freeze_cursor,
                } => {
                    w::row(ui, "Rest before drawing", "", |ui| {
                        ui.add(egui::DragValue::new(rest).range(100..=5000).suffix(" ms"));
                    });
                    freeze(ui, freeze_cursor);
                }
                DrawActivation::Toggle {
                    timeout_ms,
                    freeze_cursor,
                } => {
                    w::row(
                        ui,
                        "Stays armed for",
                        "Give a gesture the “Draw a symbol with one finger next” action.",
                        |ui| {
                            ui.add(
                                egui::DragValue::new(timeout_ms)
                                    .range(500..=60_000)
                                    .suffix(" ms"),
                            );
                        },
                    );
                    freeze(ui, freeze_cursor);
                }
            }
            w::row(
                ui,
                "Minimum score",
                "How close a drawing must be to a template.",
                |ui| {
                    with_default(
                        ui,
                        &mut s.default_confidence,
                        d.default_confidence,
                        |ui, v| {
                            w::slider(ui, egui::Slider::new(v, 0.5..=0.99).fixed_decimals(2));
                        },
                    )
                },
            );
            ms(
                ui,
                "Pause that ends a symbol",
                "Strokes closer together than this are one symbol.",
                &mut s.stroke_gap_ms,
                d.stroke_gap_ms,
                100..=2000,
            );
            w::row(
                ui,
                "Rotation tolerance",
                "0° means a rotated L is not an L.",
                |ui| {
                    with_default(
                        ui,
                        &mut s.rotation_tolerance_deg,
                        d.rotation_tolerance_deg,
                        |ui, v| {
                            w::slider(ui, egui::Slider::new(v, 0.0..=45.0).suffix("°"));
                        },
                    )
                },
            );
        },
    );
}

fn freeze(ui: &mut egui::Ui, v: &mut bool) {
    w::switch_row(
        ui,
        "Freeze the pointer while drawing",
        "Experimental: keeps the pointer in place until the symbol is done.",
        v,
    );
}

fn repeat_and_pause(ui: &mut egui::Ui, cx: &Ctx, s: &mut Settings) {
    w::section(ui, "Repeat the last gesture", "", |ui| {
        w::switch_row(
            ui,
            "Only in the window where it ran",
            "Avoids closing another program by repeating “close window”.",
            &mut s.repeat_same_app_only,
        );
        w::row(ui, "Expires", "", |ui| {
            let mut expires = s.repeat_timeout_s.is_some();
            ui.horizontal(|ui| {
                w::switch(ui, &mut expires);
                let mut secs = s.repeat_timeout_s.unwrap_or(60);
                ui.add_enabled(
                    expires,
                    egui::DragValue::new(&mut secs)
                        .range(1..=3600)
                        .prefix("after ")
                        .suffix(" s"),
                );
                s.repeat_timeout_s = expires.then_some(secs);
            });
        });
    });
    w::section(
        ui,
        "Automatic pause",
        "Gestures step aside where they would get in the way.",
        |ui| {
            w::switch_row(
                ui,
                "Pause in full-screen apps",
                "Games and presentations.",
                &mut s.pause_when_fullscreen,
            );
            w::row(ui, "Pause in these apps", "", |ui| {
                forms::app_list(ui, cx, egui::Id::unique("pause-apps"), &mut s.pause_in_apps);
                if let Some(r) = &cx.shared.auto_paused {
                    w::badge(ui, &format!("Paused now: {r}"), Tone::Warn);
                }
            });
        },
    );
}

fn overlay(ui: &mut egui::Ui, s: &mut Settings) {
    w::section(
        ui,
        "Feedback overlay",
        "A small panel on screen that shows your stroke and what was recognized.",
        |ui| {
            let o = &mut s.overlay;
            w::switch_row(ui, "Show the overlay", "", &mut o.enabled);
            ui.add_enabled_ui(o.enabled, |ui| {
                w::row(ui, "Position", "", |ui| {
                    w::segmented(
                        ui,
                        &mut o.position,
                        &[
                            (Corner::TopLeft, "Top left"),
                            (Corner::TopRight, "Top right"),
                            (Corner::BottomLeft, "Bottom left"),
                            (Corner::BottomRight, "Bottom right"),
                        ],
                    );
                });
                w::row(ui, "Shown for", "", |ui| {
                    ui.add(
                        egui::DragValue::new(&mut o.duration_ms)
                            .range(100..=30_000)
                            .suffix(" ms"),
                    );
                });
                w::row(ui, "Opacity", "", |ui| {
                    w::slider(
                        ui,
                        egui::Slider::new(&mut o.opacity, 0.1..=1.0).fixed_decimals(2),
                    );
                });
                w::row(ui, "Width", "", |ui| {
                    w::slider(
                        ui,
                        egui::Slider::new(&mut o.width_px, 120..=600).suffix(" px"),
                    );
                });
            });
        },
    );
}

fn updates(ui: &mut egui::Ui, cx: &Ctx, u: &mut glyphpad_core::config::UpdateSettings) {
    w::section(ui, "Updates", "", |ui| {
        w::row(ui, "Installed version", "", |ui| {
            ui.label(w::strong(format!("v{}", cx.version)));
        });
        w::row(ui, "Check for updates", "", |ui| {
            w::segmented(
                ui,
                &mut u.mode,
                &[
                    (UpdateMode::Off, "Never"),
                    (UpdateMode::Notify, "Tell me"),
                    (UpdateMode::Auto, "Install automatically"),
                ],
            );
        });
        w::row(ui, "Channel", "", |ui| {
            w::segmented(
                ui,
                &mut u.channel,
                &[
                    (UpdateChannel::Stable, "Stable"),
                    (UpdateChannel::Prerelease, "Pre-releases"),
                ],
            );
        });
        w::row(ui, "Check every", "", |ui| {
            ui.add(
                egui::DragValue::new(&mut u.check_interval_h)
                    .range(1..=720)
                    .suffix(" hours"),
            );
        });
        w::row(ui, "Now", "", |ui| {
            use glyphpad_core::service::UpdateStatus as U;
            let busy = matches!(
                cx.shared.update,
                U::Checking | U::Downloading(_) | U::Installing(_) | U::Restarting(_)
            );
            ui.horizontal(|ui| {
                if w::button_enabled(ui, icons::ARROWS_CLOCKWISE, "Check now", !busy).clicked() {
                    cx.send(Command::CheckUpdates { manual: true });
                }
                let (text, tone) = match &cx.shared.update {
                    U::Checking => ("Checking…".to_string(), None),
                    U::UpToDate => ("You have the latest version.".into(), Some(Tone::Ok)),
                    U::Available(a) => (
                        format!("Version {} is available.", a.version),
                        Some(Tone::Accent),
                    ),
                    U::Failed(why) => (why.clone(), Some(Tone::Bad)),
                    _ => (String::new(), None),
                };
                if !text.is_empty() {
                    match tone {
                        Some(t) => {
                            ui.label(egui::RichText::new(text).color(t.fg(ui)));
                        }
                        None => {
                            ui.label(w::dim(ui, text));
                        }
                    }
                }
            });
            if let Some(when) = &cx.shared.last_update_check {
                ui.label(w::caption(ui, format!("Last checked {when}.")));
            }
        });
    });
}

fn backups(ui: &mut egui::Ui, cx: &Ctx, s: &mut Settings, st: &mut State) {
    w::section(
        ui,
        "Backups",
        "The file is backed up before it changes; a burst of edits shares one backup.",
        |ui| {
            w::row(ui, "Keep", "", |ui| {
                ui.add(
                    egui::DragValue::new(&mut s.backup_count)
                        .range(0..=100)
                        .suffix(" versions"),
                );
            });
            if cx.shared.backups.is_empty() {
                w::row(ui, "Saved versions", "", |ui| {
                    ui.label(w::dim(ui, "None yet."));
                });
            }
            for b in cx.shared.backups.iter().take(8) {
                w::row(ui, &backup_label(b), "", |ui| {
                    if w::small_button(ui, "Restore…").clicked() {
                        st.restore = Some(b.clone());
                    }
                });
            }
        },
    );
}

fn advanced(ui: &mut egui::Ui, s: &mut Settings, d: &Settings, st: &mut State) {
    let p = pal(ui);
    let header = ui.horizontal(|ui| {
        let icon = if st.advanced {
            icons::CARET_DOWN
        } else {
            icons::CARET_RIGHT
        };
        let r = ui.add(
            egui::Button::new(format!("{icon}  Advanced"))
                .frame(false)
                .min_size(egui::vec2(0.0, theme::CONTROL_HEIGHT)),
        );
        ui.label(w::faint(ui, "Thresholds of the recognizers"));
        r
    });
    if header.inner.clicked() {
        st.advanced = !st.advanced;
    }
    let _ = p;
    if !st.advanced {
        return;
    }
    ui.add_space(space::S);
    w::section(
        ui,
        "Thresholds",
        "Distances are in millimeters, so they hold on touchpads of any size.",
        |ui| {
            ms(
                ui,
                "Longest tap",
                "",
                &mut s.tap_max_duration_ms,
                d.tap_max_duration_ms,
                50..=2000,
            );
            mm(
                ui,
                "Tap movement up to",
                "How far a finger may drift during a tap.",
                &mut s.tap_max_movement_mm,
                d.tap_max_movement_mm,
                0.5..=20.0,
            );
            ms(
                ui,
                "Pause between taps",
                "Longest gap in a double or triple tap.",
                &mut s.multi_tap_gap_ms,
                d.multi_tap_gap_ms,
                50..=2000,
            );
            ms(
                ui,
                "Hold for",
                "",
                &mut s.hold_duration_ms,
                d.hold_duration_ms,
                100..=10_000,
            );
            mm(
                ui,
                "Hold movement up to",
                "",
                &mut s.hold_max_movement_mm,
                d.hold_max_movement_mm,
                0.5..=20.0,
            );
            mm(
                ui,
                "Shortest swipe",
                "",
                &mut s.swipe_min_distance_mm,
                d.swipe_min_distance_mm,
                3.0..=100.0,
            );
            w::row(ui, "Slowest swipe", "Slower movement is a drag.", |ui| {
                with_default(
                    ui,
                    &mut s.swipe_min_speed_mm_s,
                    d.swipe_min_speed_mm_s,
                    |ui, v| {
                        ui.add(egui::DragValue::new(v).range(0.0..=1000.0).suffix(" mm/s"));
                    },
                )
            });
            mm(
                ui,
                "Pinch distance",
                "Change of the fingers' spread.",
                &mut s.pinch_min_mm,
                d.pinch_min_mm,
                1.0..=50.0,
            );
            w::row(ui, "Rotation", "Turn needed for a rotate.", |ui| {
                with_default(ui, &mut s.rotate_min_deg, d.rotate_min_deg, |ui, v| {
                    ui.add(egui::DragValue::new(v).range(1.0..=180.0).suffix("°"));
                })
            });
            w::row(ui, "Edge zone", "Depth where an edge swipe starts.", |ui| {
                with_default(
                    ui,
                    &mut s.edge_zone_percent,
                    d.edge_zone_percent,
                    |ui, v| {
                        ui.add(egui::DragValue::new(v).range(1.0..=30.0).suffix(" %"));
                    },
                )
            });
            mm(
                ui,
                "Shortest stroke",
                "Anything shorter is a tap or fingers settling.",
                &mut s.min_stroke_mm,
                d.min_stroke_mm,
                1.0..=50.0,
            );
        },
    );
}

#[cfg(test)]
mod tests {
    use super::backup_label;

    #[test]
    fn backup_names_read_as_dates() {
        assert_eq!(
            backup_label("glyphpad-20261025-085103-001.toml"),
            "25 Oct 2026, 08:51:03"
        );
        assert_eq!(backup_label("odd.toml"), "odd.toml");
    }
}
