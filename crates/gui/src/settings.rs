//! Settings page (SPEC §10.1): every option of `[settings]` and `[update]`,
//! grouped, the technical ones under "Advanced", and the backups. It edits
//! the same draft as the Gestures page: Save writes both.

use eframe::egui::{self, Id, RichText};
use glyphpad_core::config::{Corner, DrawActivation, Edge, Settings, UpdateChannel, UpdateMode};
use glyphpad_core::service::Command;

use crate::Ctx;
use crate::editor::Editor;
use crate::style;

#[derive(Default)]
pub struct State {
    restore: Option<String>,
    new_app: String,
}

fn row(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.add_sized([210.0, 20.0], egui::Label::new(label));
        add(ui);
    });
}

/// A value with its default and a "Restore default" button (SPEC §10.2).
fn with_default<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    v: &mut T,
    default: T,
    add: impl FnOnce(&mut egui::Ui, &mut T),
) {
    add(ui, v);
    if *v != default
        && ui
            .small_button("↺")
            .on_hover_text("Restore the default")
            .clicked()
    {
        *v = default;
    }
}

fn ms(
    ui: &mut egui::Ui,
    label: &str,
    v: &mut u64,
    default: u64,
    range: std::ops::RangeInclusive<u64>,
) {
    row(ui, label, |ui| {
        with_default(ui, v, default, |ui, v| {
            ui.add(egui::DragValue::new(v).range(range).suffix(" ms"));
        })
    });
}

fn mm(
    ui: &mut egui::Ui,
    label: &str,
    v: &mut f32,
    default: f32,
    range: std::ops::RangeInclusive<f32>,
) {
    row(ui, label, |ui| {
        with_default(ui, v, default, |ui, v| {
            ui.add(
                egui::DragValue::new(v)
                    .range(range)
                    .speed(0.1)
                    .suffix(" mm"),
            );
        })
    });
}

const MODES: [&str; 4] = [
    "several fingers",
    "one finger from an edge",
    "one finger after a rest",
    "one finger after a gesture",
];

pub fn show(ui: &mut egui::Ui, cx: &Ctx, ed: &mut Editor, st: &mut State) {
    style::page_header(
        ui,
        "Settings",
        "Changes are saved together with the gestures.",
    );
    let d = Settings::default();
    let draft = &mut ed.draft;
    let (s, u) = (&mut draft.settings, &mut draft.update);
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.columns(2, |cols| {
            let ui = &mut cols[0];
            style::card(ui, "Window and tray", |ui| {
                ui.checkbox(
                    &mut s.close_to_tray,
                    "Closing the window keeps GlyphPad running in the tray",
                );
                ui.checkbox(&mut s.start_minimized, "Start in the tray");
                ui.checkbox(&mut s.autostart, "Start with Windows");
                row(ui, "History length", |ui| {
                    ui.add(egui::DragValue::new(&mut s.history_size).range(1..=1000));
                });
                ui.checkbox(
                    &mut s.save_history,
                    "Also save the history to history.jsonl",
                );
            });
            ui.add_space(8.0);
            style::card(ui, "Drawn symbols", |ui| {
                let mode = match s.draw_activation {
                    DrawActivation::Fingers { .. } => 0,
                    DrawActivation::Edge { .. } => 1,
                    DrawActivation::Hold { .. } => 2,
                    DrawActivation::Toggle { .. } => 3,
                };
                let mut new_mode = mode;
                row(ui, "Draw with", |ui| {
                    egui::ComboBox::from_id_salt("draw-mode")
                        .selected_text(MODES[mode])
                        .show_ui(ui, |ui| {
                            for (i, name) in MODES.into_iter().enumerate() {
                                ui.selectable_value(&mut new_mode, i, name);
                            }
                        });
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
                        row(ui, "Fingers", |ui| {
                            ui.add(egui::Slider::new(count, 2..=5));
                        });
                        ui.weak("Turn off Windows' own gestures for this many fingers.");
                    }
                    DrawActivation::Edge {
                        edge,
                        zone_percent,
                        freeze_cursor,
                    } => {
                        row(ui, "Edge", |ui| {
                            for e in [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom] {
                                ui.radio_value(edge, e, format!("{e:?}").to_lowercase());
                            }
                        });
                        row(ui, "Zone", |ui| {
                            ui.add(egui::Slider::new(zone_percent, 1.0..=50.0).suffix(" %"));
                        });
                        ui.checkbox(
                            freeze_cursor,
                            "Freeze the pointer while drawing (experimental)",
                        );
                    }
                    DrawActivation::Hold {
                        ms: rest,
                        freeze_cursor,
                    } => {
                        row(ui, "Rest before drawing", |ui| {
                            ui.add(egui::DragValue::new(rest).range(100..=5000).suffix(" ms"));
                        });
                        ui.checkbox(
                            freeze_cursor,
                            "Freeze the pointer while drawing (experimental)",
                        );
                    }
                    DrawActivation::Toggle {
                        timeout_ms,
                        freeze_cursor,
                    } => {
                        row(ui, "Armed for", |ui| {
                            ui.add(
                                egui::DragValue::new(timeout_ms)
                                    .range(500..=60_000)
                                    .suffix(" ms"),
                            );
                        });
                        ui.checkbox(
                            freeze_cursor,
                            "Freeze the pointer while drawing (experimental)",
                        );
                        ui.weak("Give a gesture the “Draw a symbol with one finger next” action.");
                    }
                }
                row(ui, "Minimum score", |ui| {
                    with_default(
                        ui,
                        &mut s.default_confidence,
                        d.default_confidence,
                        |ui, v| {
                            ui.add(egui::Slider::new(v, 0.5..=0.99));
                        },
                    )
                });
                ms(
                    ui,
                    "Pause that ends a symbol",
                    &mut s.stroke_gap_ms,
                    d.stroke_gap_ms,
                    100..=2000,
                );
                row(ui, "Rotation tolerance", |ui| {
                    with_default(
                        ui,
                        &mut s.rotation_tolerance_deg,
                        d.rotation_tolerance_deg,
                        |ui, v| {
                            ui.add(egui::Slider::new(v, 0.0..=45.0).suffix("°"));
                        },
                    )
                });
            });
            ui.add_space(8.0);
            style::card(ui, "Repeat the last gesture", |ui| {
                ui.checkbox(
                    &mut s.repeat_same_app_only,
                    "Only in the window where it ran",
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

            let ui = &mut cols[1];
            style::card(ui, "Automatic pause", |ui| {
                ui.checkbox(
                    &mut s.pause_when_fullscreen,
                    "Pause while a full-screen app is in front (games, presentations)",
                );
                ui.label("Pause while one of these apps is in front:");
                ui.horizontal_wrapped(|ui| {
                    let mut remove = None;
                    for (i, a) in s.pause_in_apps.iter().enumerate() {
                        if ui.button(format!("{a}  ✕")).clicked() {
                            remove = Some(i);
                        }
                    }
                    if let Some(i) = remove {
                        s.pause_in_apps.remove(i);
                    }
                });
                ui.horizontal(|ui| {
                    let pick = egui::ComboBox::from_id_salt("pause-pick")
                        .selected_text("Add an open app…")
                        .show_ui(ui, |ui| {
                            let mut seen = std::collections::BTreeSet::new();
                            for a in &cx.shared.open_apps {
                                if seen.insert(a.exe.to_lowercase())
                                    && ui.selectable_label(false, &a.exe).clicked()
                                    && !s.pause_in_apps.contains(&a.exe)
                                {
                                    s.pause_in_apps.push(a.exe.clone());
                                }
                            }
                        });
                    if pick.response.clicked() {
                        cx.send(Command::ListWindows);
                    }
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut st.new_app)
                            .hint_text("or type name.exe")
                            .desired_width(120.0),
                    );
                    if r.lost_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter))
                        && !st.new_app.trim().is_empty()
                    {
                        s.pause_in_apps.push(st.new_app.trim().to_string());
                        st.new_app.clear();
                    }
                });
                if let Some(r) = &cx.shared.auto_paused {
                    ui.colored_label(style::WARN, format!("Now: {r}"));
                }
            });
            ui.add_space(8.0);
            style::card(ui, "Feedback overlay", |ui| {
                let o = &mut s.overlay;
                ui.checkbox(
                    &mut o.enabled,
                    "Show the stroke and the recognized gesture on screen",
                );
                row(ui, "Position", |ui| {
                    egui::ComboBox::from_id_salt("overlay-pos")
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
                });
                row(ui, "Shown for", |ui| {
                    ui.add(
                        egui::DragValue::new(&mut o.duration_ms)
                            .range(100..=30_000)
                            .suffix(" ms"),
                    );
                });
                row(ui, "Opacity", |ui| {
                    ui.add(egui::Slider::new(&mut o.opacity, 0.1..=1.0));
                });
                row(ui, "Width", |ui| {
                    ui.add(egui::Slider::new(&mut o.width_px, 120..=600).suffix(" px"));
                });
            });
            ui.add_space(8.0);
            style::card(ui, "Updates", |ui| {
                row(ui, "Mode", |ui| {
                    for (m, l) in [
                        (UpdateMode::Off, "off"),
                        (UpdateMode::Notify, "notify"),
                        (UpdateMode::Auto, "automatic"),
                    ] {
                        ui.radio_value(&mut u.mode, m, l);
                    }
                });
                row(ui, "Channel", |ui| {
                    ui.radio_value(&mut u.channel, UpdateChannel::Stable, "stable");
                    ui.radio_value(&mut u.channel, UpdateChannel::Prerelease, "pre-releases");
                });
                row(ui, "Check every", |ui| {
                    ui.add(
                        egui::DragValue::new(&mut u.check_interval_h)
                            .range(1..=720)
                            .suffix(" h"),
                    );
                });
                ui.add_enabled(false, egui::Button::new("Check now"))
                    .on_disabled_hover_text("Automatic updates arrive in a later version (M6).");
            });
            ui.add_space(8.0);
            style::card(ui, "Backups", |ui| {
                row(ui, "Keep", |ui| {
                    ui.add(
                        egui::DragValue::new(&mut s.backup_count)
                            .range(0..=100)
                            .suffix(" versions"),
                    );
                });
                if cx.shared.backups.is_empty() {
                    ui.weak("No backups yet: one is made every time the configuration is saved.");
                }
                egui::ScrollArea::vertical()
                    .id_salt("backups")
                    .max_height(150.0)
                    .show(ui, |ui| {
                        for b in &cx.shared.backups {
                            ui.horizontal(|ui| {
                                ui.monospace(b);
                                if ui.small_button("Restore…").clicked() {
                                    st.restore = Some(b.clone());
                                }
                            });
                        }
                    });
            });
        });

        ui.add_space(8.0);
        egui::CollapsingHeader::new(RichText::new("Advanced").strong())
            .default_open(false)
            .show(ui, |ui| {
                style::card(ui, "Thresholds", |ui| {
                    ms(
                        ui,
                        "Longest tap",
                        &mut s.tap_max_duration_ms,
                        d.tap_max_duration_ms,
                        50..=2000,
                    );
                    mm(
                        ui,
                        "Tap movement up to",
                        &mut s.tap_max_movement_mm,
                        d.tap_max_movement_mm,
                        0.5..=20.0,
                    );
                    ms(
                        ui,
                        "Pause between multi-taps",
                        &mut s.multi_tap_gap_ms,
                        d.multi_tap_gap_ms,
                        50..=2000,
                    );
                    ms(
                        ui,
                        "Hold for",
                        &mut s.hold_duration_ms,
                        d.hold_duration_ms,
                        100..=10_000,
                    );
                    mm(
                        ui,
                        "Hold movement up to",
                        &mut s.hold_max_movement_mm,
                        d.hold_max_movement_mm,
                        0.5..=20.0,
                    );
                    mm(
                        ui,
                        "Shortest swipe",
                        &mut s.swipe_min_distance_mm,
                        d.swipe_min_distance_mm,
                        3.0..=100.0,
                    );
                    row(ui, "Slowest swipe", |ui| {
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
                        "Pinch: spread change",
                        &mut s.pinch_min_mm,
                        d.pinch_min_mm,
                        1.0..=50.0,
                    );
                    row(ui, "Rotation: turn", |ui| {
                        with_default(ui, &mut s.rotate_min_deg, d.rotate_min_deg, |ui, v| {
                            ui.add(egui::DragValue::new(v).range(1.0..=180.0).suffix("°"));
                        })
                    });
                    row(ui, "Edge zone", |ui| {
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
                        &mut s.min_stroke_mm,
                        d.min_stroke_mm,
                        1.0..=50.0,
                    );
                });
            });
    });

    if let Some(name) = st.restore.clone() {
        egui::Modal::new(Id::unique("restore")).show(ui.ctx(), |ui| {
            ui.set_width(380.0);
            ui.heading("Restore backup?");
            ui.label(format!(
                "The configuration will be replaced by {name}. The current one is backed up first."
            ));
            if ed.dirty() {
                ui.colored_label(style::WARN, "Your unsaved changes will be lost.");
            }
            ui.horizontal(|ui| {
                if ui.button("Restore").clicked() {
                    cx.send(Command::RestoreBackup { name: name.clone() });
                    ed.discard(cx.shared);
                    st.restore = None;
                }
                if ui.button("Cancel").clicked() {
                    st.restore = None;
                }
            });
        });
    }
}
