//! The component library: every control the pages use, drawn from the
//! theme. Pages never pick colors or sizes themselves, so the same thing
//! looks the same everywhere.

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, Layout, Margin, Rect, Response, RichText, Sense,
    Stroke, StrokeKind, Ui, Vec2,
};

use std::borrow::Cow;

use crate::i18n::tr;
use crate::theme::{self, fonts, pal, radius, size, space};

/// How a message or a state is meant: it decides the color and the icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Neutral,
    Accent,
    Ok,
    Warn,
    Bad,
}

impl Tone {
    pub fn fg(self, ui: &Ui) -> Color32 {
        let p = pal(ui);
        match self {
            Tone::Neutral => p.text_dim,
            Tone::Accent => p.accent,
            Tone::Ok => p.ok,
            Tone::Warn => p.warn,
            Tone::Bad => p.bad,
        }
    }

    pub fn bg(self, ui: &Ui) -> Color32 {
        let p = pal(ui);
        match self {
            Tone::Neutral => p.raised,
            Tone::Accent => p.accent_soft,
            Tone::Ok => p.ok_soft,
            Tone::Warn => p.warn_soft,
            Tone::Bad => p.bad_soft,
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Tone::Neutral | Tone::Accent => crate::icons::INFO,
            Tone::Ok => crate::icons::CHECK_CIRCLE,
            Tone::Warn => crate::icons::WARNING,
            Tone::Bad => crate::icons::WARNING_CIRCLE,
        }
    }
}

// ---------------------------------------------------------------- text

pub fn title_raw(text: impl Into<String>) -> RichText {
    RichText::new(text).font(fonts::strong(size::TITLE))
}

pub fn section_title(text: &'static str) -> RichText {
    RichText::new(tr(text)).font(fonts::strong(size::SECTION))
}

pub fn strong(text: &'static str) -> RichText {
    strong_raw(tr(text))
}

/// Bold text that is not translated: names the user chose, numbers.
pub fn strong_raw(text: impl Into<String>) -> RichText {
    RichText::new(text).font(fonts::strong(size::BODY))
}

pub fn dim(ui: &Ui, text: &'static str) -> RichText {
    dim_raw(ui, tr(text))
}

pub fn dim_raw(ui: &Ui, text: impl Into<String>) -> RichText {
    RichText::new(text).color(pal(ui).text_dim)
}

pub fn caption(ui: &Ui, text: &'static str) -> RichText {
    caption_raw(ui, tr(text))
}

pub fn caption_raw(ui: &Ui, text: impl Into<String>) -> RichText {
    RichText::new(text)
        .size(size::SMALL)
        .color(pal(ui).text_dim)
}

pub fn faint(ui: &Ui, text: &'static str) -> RichText {
    faint_raw(ui, tr(text))
}

pub fn faint_raw(ui: &Ui, text: impl Into<String>) -> RichText {
    RichText::new(text)
        .size(size::SMALL)
        .color(pal(ui).text_faint)
}

pub fn icon_text(icon: &str, size: f32, color: Color32) -> RichText {
    RichText::new(icon).font(fonts::icon(size)).color(color)
}

/// An icon followed by a label, as one string for a button.
fn labeled(icon: &str, label: &str) -> String {
    if label.is_empty() {
        icon.to_string()
    } else {
        format!("{icon}  {label}")
    }
}

// ------------------------------------------------------------ layout

/// A page's header: the title and what it is for on the left, the page's
/// main actions on the right.
pub fn page_header(
    ui: &mut Ui,
    title_text: &'static str,
    subtitle: &'static str,
    actions: impl FnOnce(&mut Ui),
) {
    page_header_raw(ui, tr(title_text), tr(subtitle), actions);
}

/// A header whose texts are already in the right language.
pub fn page_header_raw(
    ui: &mut Ui,
    title_text: &str,
    subtitle: &str,
    actions: impl FnOnce(&mut Ui),
) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.add(egui::Label::new(title_raw(title_text)).selectable(false));
            if !subtitle.is_empty() {
                ui.add(egui::Label::new(dim_raw(ui, subtitle)).selectable(false));
            }
        });
        ui.with_layout(Layout::right_to_left(Align::Min), actions);
    });
    ui.add_space(space::L);
}

/// A column at most `max_width` wide, centered in the available space:
/// forms get unreadable when their lines run across a whole monitor.
pub fn centered_column<R>(ui: &mut Ui, max_width: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    let avail = ui.available_width();
    let width = avail.min(max_width);
    let side = ((avail - width) / 2.0).max(0.0);
    ui.horizontal_top(|ui| {
        ui.add_space(side);
        ui.vertical(|ui| {
            ui.set_width(width);
            add(ui)
        })
        .inner
    })
    .inner
}

/// A page body that scrolls: pages are taller than small windows.
pub fn scroll_page<R>(ui: &mut Ui, id: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::ScrollArea::vertical()
        .id_salt(id)
        .auto_shrink([false, false])
        // Room for the hairline borders of cards, which the scroll area's
        // clip rectangle would otherwise cut in half.
        .content_margin(Margin::symmetric(2, 2))
        .show(ui, |ui| {
            let r = add(ui);
            ui.add_space(space::XL);
            r
        })
        .inner
}

pub fn card_frame(ui: &Ui) -> egui::Frame {
    let p = pal(ui);
    egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(radius::CARD))
        .inner_margin(Margin::symmetric(space::L as i8 + 2, space::L as i8))
}

/// A group of related controls: a title, what it does, then the rows.
pub fn section<R>(
    ui: &mut Ui,
    heading: &'static str,
    description: &'static str,
    add: impl FnOnce(&mut Ui) -> R,
) -> R {
    let r = card_frame(ui)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if !heading.is_empty() {
                ui.add(egui::Label::new(section_title(heading)).selectable(false));
            }
            if !description.is_empty() {
                ui.add(egui::Label::new(caption(ui, description)).selectable(false));
            }
            if !heading.is_empty() || !description.is_empty() {
                ui.add_space(space::XS);
            }
            // Without a title the card starts with its first row: no rule above it.
            ui.data_mut(|d| {
                d.insert_temp(
                    egui::Id::unique("skip-divider"),
                    heading.is_empty() && description.is_empty(),
                )
            });
            add(ui)
        })
        .inner;
    ui.add_space(space::M);
    r
}

/// The label column takes a third of the row, within sensible bounds, so
/// the same row works in a wide settings page and in a narrow side panel.
fn label_width(ui: &Ui) -> f32 {
    (ui.available_width() * 0.36).clamp(130.0, 220.0)
}

/// One setting: its name and a hint on the left, the control on the right.
/// Rows are separated by a hairline.
pub fn row(ui: &mut Ui, label: &'static str, hint: &'static str, add: impl FnOnce(&mut Ui)) {
    row_raw(ui, tr(label), hint, add);
}

/// A row whose label is not a literal of the program (a date, a name).
pub fn row_raw(ui: &mut Ui, label: &str, hint: &'static str, add: impl FnOnce(&mut Ui)) {
    divider(ui);
    let label_w = label_width(ui);
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(
            Vec2::new(label_w, 0.0),
            Layout::top_down(Align::Min),
            |ui| {
                ui.set_width(label_w - space::M);
                ui.add_space(5.0);
                ui.add(egui::Label::new(RichText::new(label)).selectable(false));
                if !hint.is_empty() {
                    ui.add(egui::Label::new(caption(ui, hint)).selectable(false));
                }
            },
        );
        ui.vertical(|ui| {
            ui.set_min_height(theme::CONTROL_HEIGHT);
            add(ui)
        });
    });
}

/// Rows with a switch as their control: the label is the whole sentence.
pub fn switch_row(
    ui: &mut Ui,
    label: &'static str,
    hint: &'static str,
    value: &mut bool,
) -> Response {
    divider(ui);
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(ui.available_width() - 56.0);
            ui.add_space(5.0);
            ui.add(egui::Label::new(tr(label)).selectable(false));
            if !hint.is_empty() {
                ui.add(egui::Label::new(caption(ui, hint)).selectable(false));
            }
            ui.add_space(5.0);
        });
        ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
            ui.add_space(2.0);
            switch(ui, value)
        })
        .inner
    })
    .inner
}

pub fn divider(ui: &mut Ui) {
    let p = pal(ui);
    let skip = ui
        .data_mut(|d| d.remove_temp::<bool>(egui::Id::unique("skip-divider")))
        .unwrap_or(false);
    if skip {
        return;
    }
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, p.border);
    ui.add_space(space::XS);
}

/// A thin rule that is always painted (between blocks, not rows).
pub fn rule(ui: &mut Ui) {
    let p = pal(ui);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, p.border);
}

// ------------------------------------------------------------ controls

/// A pill switch: an immediate, binary setting.
pub fn switch(ui: &mut Ui, value: &mut bool) -> Response {
    let p = pal(ui);
    let size = Vec2::new(38.0, 22.0);
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click());
    if response.clicked() {
        *value = !*value;
        response.mark_changed();
    }
    let t = ui.ctx().animate_bool_with_time(response.id, *value, 0.12);
    let track = if ui.is_enabled() {
        lerp_color(p.border_strong, p.accent, t)
    } else {
        p.border
    };
    let track = if response.hovered() && ui.is_enabled() {
        lerp_color(track, p.text, 0.08)
    } else {
        track
    };
    ui.painter()
        .rect_filled(rect, CornerRadius::same(11), track);
    let knob_x = egui::lerp((rect.left() + 11.0)..=(rect.right() - 11.0), t);
    let knob = if *value || !p.dark {
        Color32::WHITE
    } else {
        p.text_dim
    };
    ui.painter().circle_filled(
        egui::pos2(knob_x, rect.center().y),
        8.0,
        if ui.is_enabled() { knob } else { p.surface },
    );
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect.expand(2.0),
            CornerRadius::same(13),
            Stroke::new(2.0, p.accent),
            StrokeKind::Outside,
        );
    }
    response
}

pub fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(
        l(a.r(), b.r()),
        l(a.g(), b.g()),
        l(a.b(), b.b()),
        l(a.a(), b.a()),
    )
}

/// A group of mutually exclusive options shown side by side.
pub fn segmented<T: PartialEq + Copy>(
    ui: &mut Ui,
    value: &mut T,
    options: &[(T, &'static str)],
) -> Response {
    segmented_with_icons(
        ui,
        value,
        &options
            .iter()
            .map(|(v, l)| (*v, "", *l))
            .collect::<Vec<_>>(),
    )
}

pub fn segmented_with_icons<T: PartialEq + Copy>(
    ui: &mut Ui,
    value: &mut T,
    options: &[(T, &str, &'static str)],
) -> Response {
    let p = pal(ui);
    let mut changed = false;
    let inner = egui::Frame::new()
        .fill(p.raised)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(radius::CONTROL + 2))
        .inner_margin(Margin::same(2))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            // In a right-to-left row (page header actions) `horizontal` runs
            // right to left too: list the options backwards to keep them in order.
            let backwards = ui.layout().prefer_right_to_left();
            let ordered: Vec<&(T, &str, &'static str)> = if backwards {
                options.iter().rev().collect()
            } else {
                options.iter().collect()
            };
            ui.horizontal(|ui| {
                for (v, icon, label) in ordered {
                    let selected = *value == *v;
                    let text = labeled(icon, tr(label));
                    let galley_color = if selected { p.text } else { p.text_dim };
                    let button = egui::Button::new(RichText::new(text).color(galley_color))
                        .fill(if selected {
                            p.surface
                        } else {
                            Color32::TRANSPARENT
                        })
                        .stroke(if selected {
                            Stroke::new(1.0, p.border)
                        } else {
                            Stroke::NONE
                        })
                        .corner_radius(CornerRadius::same(radius::CONTROL))
                        .min_size(Vec2::new(0.0, theme::CONTROL_HEIGHT - 8.0));
                    let r = ui.add(button);
                    if r.clicked() && !selected {
                        *value = *v;
                        changed = true;
                    }
                }
            })
            .response
        });
    let mut r = inner.response;
    if changed {
        r.mark_changed();
    }
    r
}

/// A button that is only an icon, with a tooltip: row actions, toolbars.
pub fn icon_button(ui: &mut Ui, icon: &str, tip: &'static str) -> Response {
    icon_button_tone(ui, icon, tip, Tone::Neutral)
}

pub fn icon_button_tone(ui: &mut Ui, icon: &str, tip: &'static str, tone: Tone) -> Response {
    let p = pal(ui);
    let size = Vec2::splat(30.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let enabled = ui.is_enabled();
    if response.hovered() && enabled {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(radius::CONTROL),
            if tone == Tone::Bad {
                p.bad_soft
            } else {
                p.hover
            },
        );
    }
    let color = match (enabled, response.hovered(), tone) {
        (false, _, _) => p.text_faint.gamma_multiply(0.6),
        (_, true, Tone::Bad) => p.bad,
        (_, true, _) => p.text,
        _ => p.text_dim,
    };
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        icon,
        fonts::icon(size::ICON),
        color,
    );
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(radius::CONTROL),
            Stroke::new(2.0, p.accent),
            StrokeKind::Inside,
        );
    }
    response.on_hover_text(tr(tip))
}

/// The one filled button of a view: what the user most likely wants next.
pub fn primary_button(ui: &mut Ui, icon: &str, label: &'static str) -> Response {
    primary_button_enabled(ui, icon, label, true)
}

pub fn primary_button_enabled(
    ui: &mut Ui,
    icon: &str,
    label: &'static str,
    enabled: bool,
) -> Response {
    let p = pal(ui);
    let button = egui::Button::new(RichText::new(labeled(icon, tr(label))).color(p.on_accent))
        .fill(p.accent)
        .stroke(Stroke::NONE)
        .min_size(Vec2::new(0.0, theme::CONTROL_HEIGHT));
    ui.add_enabled(enabled, button)
}

pub fn button(ui: &mut Ui, icon: &str, label: &'static str) -> Response {
    ui.add(
        egui::Button::new(labeled(icon, tr(label))).min_size(Vec2::new(0.0, theme::CONTROL_HEIGHT)),
    )
}

pub fn button_enabled(ui: &mut Ui, icon: &str, label: &'static str, enabled: bool) -> Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(labeled(icon, tr(label))).min_size(Vec2::new(0.0, theme::CONTROL_HEIGHT)),
    )
}

/// A destructive action: red text, never filled.
pub fn danger_button(ui: &mut Ui, icon: &str, label: &'static str) -> Response {
    let p = pal(ui);
    ui.add(
        egui::Button::new(RichText::new(labeled(icon, tr(label))).color(p.bad))
            .min_size(Vec2::new(0.0, theme::CONTROL_HEIGHT)),
    )
}

/// A compact button for inline use (inside rows of controls).
pub fn small_button(ui: &mut Ui, label: &'static str) -> Response {
    ui.add(
        egui::Button::new(RichText::new(tr(label)).size(size::SMALL))
            .min_size(Vec2::new(0.0, 24.0)),
    )
}

/// A small pill: a state, a count, a tag.
pub fn badge(ui: &mut Ui, text: &'static str, tone: Tone) -> Response {
    badge_raw(ui, tr(text), tone)
}

/// A badge whose text is already in the right language.
pub fn badge_raw(ui: &mut Ui, text: &str, tone: Tone) -> Response {
    let p = pal(ui);
    let fg = match tone {
        Tone::Neutral => p.text_dim,
        t => t.fg(ui),
    };
    egui::Frame::new()
        .fill(tone.bg(ui))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(8, 2))
        .show(ui, |ui| {
            ui.add(
                egui::Label::new(RichText::new(text).size(size::SMALL).color(fg)).selectable(false),
            )
        })
        .response
}

/// A removable tag (an app name in a list).
pub fn tag(ui: &mut Ui, text: &str) -> bool {
    let p = pal(ui);
    let mut removed = false;
    egui::Frame::new()
        .fill(p.raised)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(radius::CONTROL))
        .inner_margin(Margin::symmetric(8, 2))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.horizontal(|ui| {
                ui.add(egui::Label::new(RichText::new(text).size(size::SMALL)).selectable(false));
                let r = ui.add(
                    egui::Button::new(icon_text(crate::icons::X, 12.0, p.text_dim))
                        .frame(false)
                        .min_size(Vec2::splat(18.0)),
                );
                removed = r.on_hover_text("Remove").clicked();
            });
        });
    removed
}

/// A toggle that looks like a chip: an option among several that can all be on.
pub fn chip(ui: &mut Ui, text: &str, selected: bool) -> Response {
    let p = pal(ui);
    let (fill, stroke, fg) = if selected {
        (p.accent_soft, Stroke::new(1.0, p.accent), p.accent)
    } else {
        (p.raised, Stroke::new(1.0, p.border), p.text)
    };
    ui.add(
        egui::Button::new(
            RichText::new(text)
                .font(fonts::strong(size::BODY))
                .color(fg),
        )
        .fill(fill)
        .stroke(stroke)
        .min_size(Vec2::new(36.0, theme::CONTROL_HEIGHT)),
    )
}

/// A value shown as a keyboard key.
pub fn kbd(ui: &mut Ui, text: &str) {
    let p = pal(ui);
    egui::Frame::new()
        .fill(p.raised)
        .stroke(Stroke::new(1.0, p.border_strong))
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::symmetric(6, 1))
        .show(ui, |ui| {
            ui.add(
                egui::Label::new(RichText::new(text).font(fonts::mono(12.0)).color(p.text))
                    .selectable(false),
            );
        });
}

/// A message about the state of things, with an icon and optional actions.
pub fn banner(
    ui: &mut Ui,
    tone: Tone,
    heading: &'static str,
    body: &'static str,
    actions: impl FnOnce(&mut Ui),
) {
    banner_raw(ui, tone, tr(heading), tr(body), actions);
}

/// A banner whose texts are already in the right language.
pub fn banner_raw(
    ui: &mut Ui,
    tone: Tone,
    heading: &str,
    body: &str,
    actions: impl FnOnce(&mut Ui),
) {
    let fg = tone.fg(ui);
    egui::Frame::new()
        .fill(tone.bg(ui))
        .stroke(Stroke::new(1.0, fg.gamma_multiply(0.35)))
        .corner_radius(CornerRadius::same(radius::CARD))
        .inner_margin(Margin::symmetric(space::M as i8 + 2, space::M as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.label(icon_text(tone.icon(), 18.0, fg));
                ui.vertical(|ui| {
                    ui.add(
                        egui::Label::new(RichText::new(heading).font(fonts::strong(size::BODY)))
                            .selectable(false),
                    );
                    if !body.is_empty() {
                        ui.add(egui::Label::new(
                            RichText::new(body).size(size::SMALL + 0.5),
                        ));
                    }
                    ui.horizontal(actions);
                });
            });
        });
    ui.add_space(space::M);
}

/// What an empty list or a not-yet-chosen item shows: an icon, one line
/// saying what it is, one saying what to do.
pub fn empty_state(
    ui: &mut Ui,
    icon: &str,
    heading: &'static str,
    body: &'static str,
    actions: impl FnOnce(&mut Ui),
) {
    let p = pal(ui);
    ui.vertical_centered(|ui| {
        // A little above the middle of the space there is.
        ui.add_space(((ui.available_height() - 170.0) * 0.3).clamp(space::XL, 120.0));
        ui.label(icon_text(icon, 34.0, p.text_faint));
        ui.add_space(space::S);
        ui.add(egui::Label::new(strong(heading)).selectable(false));
        ui.add(egui::Label::new(caption(ui, body)).selectable(false));
        ui.add_space(space::S);
        ui.horizontal(|ui| {
            // Center the actions: horizontal() left-aligns inside a centered column.
            actions(ui)
        });
    });
}

/// A search box with a magnifier, as one control.
pub fn search_field(ui: &mut Ui, text: &mut String, hint: &'static str) -> Response {
    let p = pal(ui);
    let frame = egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(radius::CONTROL))
        .inner_margin(Margin::symmetric(8, 0));
    frame
        .show(ui, |ui| {
            ui.set_height(theme::CONTROL_HEIGHT - 2.0);
            ui.horizontal_centered(|ui| {
                ui.label(icon_text(
                    crate::icons::MAGNIFYING_GLASS,
                    15.0,
                    p.text_faint,
                ));
                let r = ui.add_sized(
                    [ui.available_width() - 28.0, theme::CONTROL_HEIGHT - 2.0],
                    egui::TextEdit::singleline(text)
                        .hint_text(tr(hint))
                        .frame(egui::Frame::new())
                        .margin(Margin::ZERO)
                        .align(Align2::LEFT_CENTER),
                );
                if !text.is_empty()
                    && ui
                        .add(
                            egui::Button::new(icon_text(crate::icons::X, 13.0, p.text_dim))
                                .frame(false),
                        )
                        .on_hover_text(tr("Clear"))
                        .clicked()
                {
                    text.clear();
                }
                r
            })
            .inner
        })
        .inner
}

/// A text field that is an input like the others: bordered, fixed height.
pub fn text_field(ui: &mut Ui, text: &mut String, hint: &'static str, width: f32) -> Response {
    ui.add_sized(
        [width, theme::CONTROL_HEIGHT],
        egui::TextEdit::singleline(text)
            .hint_text(tr(hint))
            .align(Align2::LEFT_CENTER)
            .margin(Margin::symmetric(8, 0)),
    )
}

pub fn error_text(ui: &mut Ui, text: &'static str) {
    error_text_raw(ui, tr(text));
}

pub fn error_text_raw(ui: &mut Ui, text: &str) {
    let p = pal(ui);
    ui.horizontal(|ui| {
        ui.label(icon_text(crate::icons::WARNING_CIRCLE, 15.0, p.bad));
        ui.add(egui::Label::new(
            RichText::new(text).size(size::SMALL + 0.5).color(p.bad),
        ));
    });
}

pub fn warn_text(ui: &mut Ui, text: &'static str) {
    warn_text_raw(ui, tr(text));
}

pub fn warn_text_raw(ui: &mut Ui, text: &str) {
    let p = pal(ui);
    ui.horizontal(|ui| {
        ui.label(icon_text(crate::icons::WARNING, 15.0, p.warn));
        ui.add(egui::Label::new(
            RichText::new(text).size(size::SMALL + 0.5).color(p.warn),
        ));
    });
}

pub fn hint(ui: &mut Ui, text: &'static str) {
    ui.add(egui::Label::new(caption(ui, text)));
}

// -------------------------------------------------------------- nav

/// An entry of the sidebar.
pub fn nav_item(ui: &mut Ui, icon: &str, label: &'static str, selected: bool) -> Response {
    let p = pal(ui);
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 36.0), Sense::click());
    let fill = if selected {
        p.accent_soft
    } else if response.hovered() {
        p.hover
    } else {
        Color32::TRANSPARENT
    };
    ui.painter()
        .rect_filled(rect, CornerRadius::same(radius::CONTROL + 2), fill);
    let fg = if selected { p.accent } else { p.text };
    ui.painter().text(
        egui::pos2(rect.left() + 14.0, rect.center().y),
        Align2::LEFT_CENTER,
        icon,
        fonts::icon(18.0),
        if selected { p.accent } else { p.text_dim },
    );
    ui.painter().text(
        egui::pos2(rect.left() + 44.0, rect.center().y),
        Align2::LEFT_CENTER,
        tr(label),
        if selected {
            fonts::strong(size::BODY)
        } else {
            fonts::text(size::BODY)
        },
        fg,
    );
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(radius::CONTROL + 2),
            Stroke::new(2.0, p.accent),
            StrokeKind::Inside,
        );
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A number with minus and plus buttons: a small count (fingers, taps).
pub fn stepper(ui: &mut Ui, value: &mut u8, min: u8, max: u8) -> Response {
    let p = pal(ui);
    let mut changed = false;
    let mut response = egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(radius::CONTROL))
        .inner_margin(Margin::same(1))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.horizontal(|ui| {
                if step_button(ui, crate::icons::MINUS, *value > min).clicked() {
                    *value = value.saturating_sub(1).max(min);
                    changed = true;
                }
                let (rect, _) = ui.allocate_exact_size(Vec2::new(34.0, 30.0), Sense::hover());
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    value.to_string(),
                    fonts::strong(size::BODY),
                    p.text,
                );
                if step_button(ui, crate::icons::PLUS, *value < max).clicked() {
                    *value = (*value + 1).min(max);
                    changed = true;
                }
            });
        })
        .response;
    if changed {
        response.mark_changed();
    }
    response
}

fn step_button(ui: &mut Ui, icon: &str, enabled: bool) -> Response {
    let p = pal(ui);
    ui.add_enabled(
        enabled,
        egui::Button::new(icon_text(icon, 14.0, p.text_dim))
            .frame(false)
            .min_size(Vec2::new(30.0, 30.0)),
    )
}

/// Eight directions as a compass: faster to read than a drop-down. With
/// `diagonals` false only the four axes can be chosen.
pub fn direction_picker(
    ui: &mut Ui,
    value: &mut glyphpad_core::config::Direction,
    diagonals: bool,
) -> Response {
    use glyphpad_core::config::Direction as D;
    let p = pal(ui);
    let grid = [
        [D::UpLeft, D::Up, D::UpRight],
        [D::Left, D::Up, D::Right],
        [D::DownLeft, D::Down, D::DownRight],
    ];
    let icons = [
        [
            crate::icons::ARROW_UP_LEFT,
            crate::icons::ARROW_UP,
            crate::icons::ARROW_UP_RIGHT,
        ],
        [crate::icons::ARROW_LEFT, "", crate::icons::ARROW_RIGHT],
        [
            crate::icons::ARROW_DOWN_LEFT,
            crate::icons::ARROW_DOWN,
            crate::icons::ARROW_DOWN_RIGHT,
        ],
    ];
    let mut changed = false;
    let cell = 34.0;
    let (rect, mut response) =
        ui.allocate_exact_size(Vec2::splat(cell * 3.0 + 4.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(radius::CARD), p.raised);
    for (r, row) in grid.iter().enumerate() {
        for (c, dir) in row.iter().enumerate() {
            let icon = icons[r][c];
            if icon.is_empty() {
                continue;
            }
            let diagonal = r != 1 && c != 1;
            let cell_rect = Rect::from_min_size(
                rect.min + Vec2::new(2.0 + c as f32 * cell, 2.0 + r as f32 * cell),
                Vec2::splat(cell),
            )
            .shrink(2.0);
            let enabled = diagonals || !diagonal;
            let r = ui.interact(
                cell_rect,
                response.id.with((r, c)),
                if enabled {
                    Sense::click()
                } else {
                    Sense::hover()
                },
            );
            let selected = *dir == *value;
            let fill = if selected {
                p.accent
            } else if r.hovered() && enabled {
                p.hover
            } else {
                Color32::TRANSPARENT
            };
            ui.painter()
                .rect_filled(cell_rect, CornerRadius::same(radius::CONTROL), fill);
            let fg = if selected {
                p.on_accent
            } else if enabled {
                p.text_dim
            } else {
                p.text_faint.gamma_multiply(0.45)
            };
            ui.painter().text(
                cell_rect.center(),
                Align2::CENTER_CENTER,
                icon,
                fonts::icon(18.0),
                fg,
            );
            if r.clicked() {
                *value = *dir;
                changed = true;
            }
        }
    }
    if changed {
        response.mark_changed();
    }
    response
}

/// A selectable tile: an icon above a name (the kinds of gesture).
pub fn tile(ui: &mut Ui, icon: &str, label: &'static str, selected: bool, size: Vec2) -> Response {
    let p = pal(ui);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let (fill, stroke, fg) = if selected {
        (p.accent_soft, Stroke::new(1.5, p.accent), p.accent)
    } else if response.hovered() {
        (p.hover, Stroke::new(1.0, p.border_strong), p.text)
    } else {
        (p.surface, Stroke::new(1.0, p.border), p.text_dim)
    };
    ui.painter().rect(
        rect,
        CornerRadius::same(radius::CONTROL + 2),
        fill,
        stroke,
        StrokeKind::Inside,
    );
    ui.painter().text(
        egui::pos2(rect.center().x, rect.top() + size.y * 0.38),
        Align2::CENTER_CENTER,
        icon,
        fonts::icon(22.0),
        fg,
    );
    ui.painter().text(
        egui::pos2(rect.center().x, rect.bottom() - size.y * 0.24),
        Align2::CENTER_CENTER,
        tr(label),
        if selected {
            fonts::strong(size::SMALL)
        } else {
            fonts::text(size::SMALL)
        },
        if selected { p.text } else { p.text_dim },
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// `text` on one line, ending in "…" when it does not fit `width`.
pub fn elided(
    ui: &Ui,
    text: &str,
    font: egui::FontId,
    color: Color32,
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_string(), font, color);
    job.wrap = egui::text::TextWrapping {
        max_width: width.max(1.0),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    ui.painter().layout_job(job)
}

/// A modal dialog with a title, a body and a row of buttons.
pub fn dialog<R>(
    ctx: &egui::Context,
    id: &str,
    heading: &'static str,
    width: f32,
    add: impl FnOnce(&mut Ui) -> R,
) -> egui::ModalResponse<R> {
    dialog_raw(ctx, id, tr(heading), width, add)
}

/// A dialog whose title is already in the right language.
pub fn dialog_raw<R>(
    ctx: &egui::Context,
    id: &str,
    heading: &str,
    width: f32,
    add: impl FnOnce(&mut Ui) -> R,
) -> egui::ModalResponse<R> {
    egui::Modal::new(egui::Id::unique(id)).show(ctx, |ui| {
        ui.set_width(width);
        ui.add(
            egui::Label::new(RichText::new(heading).font(fonts::strong(size::SECTION)))
                .selectable(false),
        );
        ui.add_space(space::S);
        add(ui)
    })
}

/// A dropdown that looks like the other controls and shows its choice.
pub fn dropdown<T: PartialEq + Clone>(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    value: &mut T,
    options: &[(T, Cow<'static, str>)],
    width: f32,
) -> Response {
    // A literal of the program is translated; a name the user chose is not.
    let shown = |l: &Cow<'static, str>| match l {
        Cow::Borrowed(s) => tr(s).to_string(),
        Cow::Owned(s) => s.clone(),
    };
    let current = options
        .iter()
        .find(|(v, _)| v == value)
        .map(|(_, l)| shown(l))
        .unwrap_or_default();
    let mut changed = false;
    let r = egui::ComboBox::from_id_salt(id)
        .width(width)
        .selected_text(current)
        .show_ui(ui, |ui| {
            for (v, label) in options {
                if ui.selectable_label(v == value, shown(label)).clicked() && v != value {
                    *value = v.clone();
                    changed = true;
                }
            }
        });
    let mut response = r.response;
    if changed {
        response.mark_changed();
    }
    response
}

/// Lays `count` items out in as many equal columns as fit `min_width`,
/// stretched to fill the row, so previews never overflow and never leave a
/// ragged edge. `add` gets the item index and the width of its column.
pub fn grid(
    ui: &mut Ui,
    count: usize,
    min_width: f32,
    gap: f32,
    mut add: impl FnMut(&mut Ui, usize, f32),
) {
    let avail = ui.available_width();
    let cols = (((avail + gap) / (min_width + gap)).floor() as usize).clamp(1, count.max(1));
    let width = ((avail - gap * (cols as f32 - 1.0)) / cols as f32).floor();
    for row in (0..count).step_by(cols) {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for i in row..(row + cols).min(count) {
                ui.vertical(|ui| {
                    ui.set_width(width);
                    add(ui, i, width);
                });
            }
        });
        ui.add_space(gap);
    }
}

/// A slider with a knob in proportion to the controls around it: egui
/// sizes the knob from the row height, which is larger than a knob wants.
pub fn slider(ui: &mut Ui, slider: egui::Slider<'_>) -> Response {
    ui.scope(|ui| {
        ui.spacing_mut().interact_size.y = 20.0;
        ui.add(slider)
    })
    .inner
}
