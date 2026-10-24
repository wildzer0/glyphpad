//! Fonts and visual style.

use std::sync::Arc;

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, Margin, Stroke, Visuals,
};

/// Accent used for selection, active states and strokes.
pub const ACCENT: Color32 = Color32::from_rgb(64, 132, 246);
pub const OK: Color32 = Color32::from_rgb(110, 205, 140);
pub const WARN: Color32 = Color32::from_rgb(236, 190, 80);
pub const BAD: Color32 = Color32::from_rgb(236, 110, 110);

/// The system UI font and its symbol and emoji fallbacks: egui's built-in
/// fonts lack most symbols (they rendered as boxes). On Windows these are
/// always installed; elsewhere the built-in fonts stay.
fn system_fonts() -> Vec<(&'static str, Vec<u8>, bool)> {
    let dir = std::env::var_os("WINDIR").map(|w| std::path::PathBuf::from(w).join("Fonts"));
    let Some(dir) = dir else { return Vec::new() };
    // (file, used as the main proportional font)
    [
        ("segoeui.ttf", true),
        ("seguisym.ttf", false),
        ("seguiemj.ttf", false),
    ]
    .into_iter()
    .filter_map(|(file, main)| std::fs::read(dir.join(file)).ok().map(|b| (file, b, main)))
    .collect()
}

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    for (name, bytes, main) in system_fonts() {
        fonts
            .font_data
            .insert(name.into(), Arc::new(FontData::from_owned(bytes)));
        let proportional = fonts.families.entry(FontFamily::Proportional).or_default();
        if main {
            proportional.insert(0, name.into());
        } else {
            proportional.push(name.into());
            fonts
                .families
                .entry(FontFamily::Monospace)
                .or_default()
                .push(name.into());
        }
    }
    ctx.set_fonts(fonts);

    let mut visuals = Visuals::dark();
    visuals.panel_fill = Color32::from_rgb(28, 29, 33);
    visuals.window_fill = Color32::from_rgb(34, 35, 40);
    visuals.extreme_bg_color = Color32::from_rgb(20, 21, 24);
    visuals.faint_bg_color = Color32::from_rgb(36, 37, 43);
    visuals.selection.bg_fill = ACCENT.gamma_multiply(0.55);
    visuals.selection.stroke = Stroke::new(1.0, ACCENT);
    visuals.hyperlink_color = ACCENT;
    let radius = CornerRadius::same(6);
    for w in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        w.corner_radius = radius;
    }
    visuals.window_corner_radius = CornerRadius::same(10);
    ctx.set_visuals(visuals);
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 7.0);
        style.spacing.button_padding = egui::vec2(10.0, 5.0);
        style.spacing.window_margin = Margin::same(14);
        style.spacing.slider_width = 150.0;
        style.spacing.interact_size.y = 24.0;
        for (text_style, size) in [
            (egui::TextStyle::Body, 14.5),
            (egui::TextStyle::Button, 14.5),
            (egui::TextStyle::Heading, 20.0),
            (egui::TextStyle::Small, 11.5),
            (egui::TextStyle::Monospace, 13.0),
        ] {
            if let Some(f) = style.text_styles.get_mut(&text_style) {
                f.size = size;
            }
        }
    });
}

/// A titled card: a framed group with some breathing room.
pub fn card<R>(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::group(ui.style())
        .fill(ui.visuals().faint_bg_color)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if !title.is_empty() {
                ui.label(egui::RichText::new(title).strong().size(15.5));
                ui.add_space(4.0);
            }
            add(ui)
        })
        .inner
}

/// A page title with an optional subtitle.
pub fn page_header(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.label(egui::RichText::new(title).heading());
    if !subtitle.is_empty() {
        ui.label(egui::RichText::new(subtitle).weak());
    }
    ui.add_space(6.0);
}
