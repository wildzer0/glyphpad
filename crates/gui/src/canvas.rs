//! Drawing traces on a touchpad silhouette with its real proportions, and
//! their animated playback (SPEC §10.1).

use std::collections::BTreeMap;
use std::time::Instant;

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use glyphpad_core::Contact;
use glyphpad_core::symbols::Stroke as Trace;

/// Colors of successive strokes or fingers.
pub const PALETTE: [Color32; 6] = [
    Color32::from_rgb(90, 170, 255),
    Color32::from_rgb(255, 170, 70),
    Color32::from_rgb(120, 220, 140),
    Color32::from_rgb(230, 110, 200),
    Color32::from_rgb(240, 225, 100),
    Color32::from_rgb(150, 140, 255),
];

pub fn color(i: usize) -> Color32 {
    PALETTE[i % PALETTE.len()]
}

/// A touchpad drawn on screen; maps millimeters to screen points.
pub struct Pad {
    pub rect: Rect,
    pub width_mm: f32,
    pub height_mm: f32,
}

impl Pad {
    /// Allocates a silhouette `width` points wide with the touchpad's
    /// proportions and paints its background.
    pub fn new(
        ui: &mut egui::Ui,
        width: f32,
        width_mm: f32,
        height_mm: f32,
    ) -> (egui::Response, egui::Painter, Pad) {
        let (w_mm, h_mm) = (width_mm.max(1.0), height_mm.max(1.0));
        let size = Vec2::new(width, width * h_mm / w_mm);
        let (response, painter) = ui.allocate_painter(size, Sense::click());
        let rect = response.rect;
        let visuals = ui.visuals();
        painter.rect_filled(rect, 8.0, visuals.extreme_bg_color);
        painter.rect_stroke(
            rect,
            8.0,
            visuals.widgets.noninteractive.bg_stroke,
            StrokeKind::Inside,
        );
        (
            response,
            painter,
            Pad {
                rect,
                width_mm: w_mm,
                height_mm: h_mm,
            },
        )
    }

    pub fn to_screen(&self, x_mm: f32, y_mm: f32) -> Pos2 {
        Pos2::new(
            self.rect.left() + x_mm / self.width_mm * self.rect.width(),
            self.rect.top() + y_mm / self.height_mm * self.rect.height(),
        )
    }
}

/// Last timestamp of a set of strokes.
pub fn duration_us<'a>(strokes: impl IntoIterator<Item = &'a Trace>) -> u64 {
    strokes
        .into_iter()
        .filter_map(|s| s.last().map(|p| p.t_us))
        .max()
        .unwrap_or(0)
}

/// Paints strokes up to `until_us` (all of them if `None`): each stroke in
/// its own color, getting brighter towards its end to show direction, with
/// its start marked by a dot and its order number.
pub fn draw_strokes(
    painter: &egui::Painter,
    pad: &Pad,
    strokes: &[Trace],
    until_us: Option<u64>,
    numbered: bool,
) {
    for (i, stroke) in strokes.iter().enumerate() {
        let base = color(i);
        let visible: Vec<_> = stroke
            .iter()
            .filter(|p| until_us.is_none_or(|t| p.t_us <= t))
            .collect();
        let n = visible.len();
        for (k, w) in visible.windows(2).enumerate() {
            let alpha = 0.35 + 0.65 * (k as f32 / n.max(2) as f32);
            painter.line_segment(
                [
                    pad.to_screen(w[0].x_mm, w[0].y_mm),
                    pad.to_screen(w[1].x_mm, w[1].y_mm),
                ],
                Stroke::new(3.0, base.gamma_multiply(alpha)),
            );
        }
        if let Some(first) = visible.first() {
            let p = pad.to_screen(first.x_mm, first.y_mm);
            painter.circle_filled(p, 4.5, base);
            if numbered && strokes.len() > 1 {
                painter.text(
                    p + Vec2::new(7.0, -7.0),
                    Align2::LEFT_BOTTOM,
                    (i + 1).to_string(),
                    FontId::proportional(12.0),
                    base,
                );
            }
        }
        // The pen position while playing.
        if until_us.is_some()
            && let Some(last) = visible.last()
            && visible.len() < stroke.len()
        {
            painter.circle_stroke(
                pad.to_screen(last.x_mm, last.y_mm),
                6.0,
                Stroke::new(2.0, Color32::WHITE),
            );
        }
    }
}

/// Paints the path of every finger, one color per contact.
pub fn draw_fingers(
    painter: &egui::Painter,
    pad: &Pad,
    fingers: &BTreeMap<u32, Trace>,
    until_us: Option<u64>,
) {
    let strokes: Vec<Trace> = fingers.values().cloned().collect();
    draw_strokes(painter, pad, &strokes, until_us, false);
}

/// Paints live contacts: circles with their id, hollow for palms.
pub fn draw_contacts(painter: &egui::Painter, pad: &Pad, contacts: &[Contact]) {
    for c in contacts.iter().filter(|c| c.tip) {
        let p = pad.to_screen(c.x_mm, c.y_mm);
        let col = color(c.id as usize);
        if c.confident {
            painter.circle_filled(p, 11.0, col.gamma_multiply(0.85));
        } else {
            painter.circle_stroke(p, 11.0, Stroke::new(2.0, col));
        }
        painter.text(
            p,
            Align2::CENTER_CENTER,
            c.id.to_string(),
            FontId::monospace(11.0),
            Color32::BLACK,
        );
    }
}

/// Paints a polyline in millimeters.
pub fn draw_polylines(
    painter: &egui::Painter,
    pad: &Pad,
    lines: &[Vec<(f32, f32)>],
    stroke: Stroke,
) {
    for line in lines {
        for w in line.windows(2) {
            painter.line_segment(
                [pad.to_screen(w[0].0, w[0].1), pad.to_screen(w[1].0, w[1].1)],
                stroke,
            );
        }
    }
}

/// Bounding box of strokes, in millimeters.
pub fn bounds(strokes: &[Trace]) -> Option<(f32, f32, f32, f32)> {
    let mut it = strokes.iter().flatten();
    let first = it.next()?;
    Some(it.fold(
        (first.x_mm, first.y_mm, first.x_mm, first.y_mm),
        |(a, b, c, d), p| (a.min(p.x_mm), b.min(p.y_mm), c.max(p.x_mm), d.max(p.y_mm)),
    ))
}

/// `template` moved and uniformly scaled onto the bounding box of `onto`,
/// to compare a drawing with the template that recognized it.
pub fn fit_onto(template: &[Trace], onto: &[Trace]) -> Vec<Trace> {
    let (Some(a), Some(b)) = (bounds(template), bounds(onto)) else {
        return template.to_vec();
    };
    let (aw, ah) = ((a.2 - a.0).max(1e-3), (a.3 - a.1).max(1e-3));
    let (bw, bh) = (b.2 - b.0, b.3 - b.1);
    let k = (bw / aw).max(bh / ah);
    let (cx_a, cy_a) = ((a.0 + a.2) / 2.0, (a.1 + a.3) / 2.0);
    let (cx_b, cy_b) = ((b.0 + b.2) / 2.0, (b.1 + b.3) / 2.0);
    template
        .iter()
        .map(|s| {
            s.iter()
                .map(|p| glyphpad_core::symbols::TracePoint {
                    x_mm: cx_b + (p.x_mm - cx_a) * k,
                    y_mm: cy_b + (p.y_mm - cy_a) * k,
                    t_us: p.t_us,
                })
                .collect()
        })
        .collect()
}

/// Play/pause, speed, loop and scrubbing of an animated trace.
pub struct Playback {
    pub playing: bool,
    pub speed: f32,
    pub looping: bool,
    pub t_us: u64,
    last: Option<Instant>,
}

impl Default for Playback {
    fn default() -> Self {
        Self {
            playing: true,
            speed: 1.0,
            looping: true,
            t_us: 0,
            last: None,
        }
    }
}

/// Pause at the end of a loop before starting over.
const LOOP_PAUSE_US: u64 = 600_000;

impl Playback {
    pub fn restart(&mut self) {
        self.t_us = 0;
        self.playing = true;
    }

    /// Advances time and keeps repainting while playing (only while the
    /// window is shown: hidden windows run no UI pass at all).
    pub fn advance(&mut self, ctx: &egui::Context, total_us: u64) {
        let now = Instant::now();
        if self.playing {
            if let Some(last) = self.last {
                self.t_us += (now.duration_since(last).as_micros() as f32 * self.speed) as u64;
            }
            if self.t_us > total_us + LOOP_PAUSE_US {
                if self.looping {
                    self.t_us = 0;
                } else {
                    self.t_us = total_us;
                    self.playing = false;
                }
            }
            ctx.request_repaint();
        }
        self.last = Some(now);
    }

    pub fn controls(&mut self, ui: &mut egui::Ui, total_us: u64) {
        ui.horizontal(|ui| {
            if ui
                .button(if self.playing {
                    "⏸ Pause"
                } else {
                    "▶ Play"
                })
                .clicked()
            {
                if !self.playing && self.t_us >= total_us {
                    self.t_us = 0;
                }
                self.playing = !self.playing;
            }
            ui.add(
                egui::Slider::new(&mut self.speed, 0.25..=2.0)
                    .text("speed")
                    .suffix("×"),
            );
            ui.checkbox(&mut self.looping, "Loop");
            let mut ms = self.t_us.min(total_us) as f32 / 1000.0;
            let max = (total_us as f32 / 1000.0).max(1.0);
            if ui
                .add(egui::Slider::new(&mut ms, 0.0..=max).suffix(" ms"))
                .changed()
            {
                self.t_us = (ms * 1000.0) as u64;
                self.playing = false;
            }
        });
    }
}
