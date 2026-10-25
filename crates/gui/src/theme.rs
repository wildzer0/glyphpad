//! The visual system: one palette per theme, the type scale, spacing and
//! the egui style built from them. Everything the pages paint with comes
//! from here, so a color or a size is decided in exactly one place.
//!
//! Decisions, in short:
//! * Neutral, slightly cool grays with a single blue accent: color is kept
//!   for meaning (selection, success, warning, error), never decoration.
//! * Three elevations: the window background, the sidebar, and cards that
//!   sit on the background. Cards are separated by a hairline border, not
//!   by shadows, which stay crisp on every display.
//! * Light and dark are designed separately (the dark accent is lighter,
//!   the dark grays are not inverted copies), and each is checked for
//!   contrast of body text and of the dimmed text on its surfaces.

use std::sync::Arc;

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Shadow,
    Stroke, TextStyle, Theme, Visuals,
};
use glyphpad_core::config::ThemeChoice;

/// Spacing scale, in points. Layout uses only these values.
pub mod space {
    pub const XS: f32 = 4.0;
    pub const S: f32 = 8.0;
    pub const M: f32 = 12.0;
    pub const L: f32 = 16.0;
    pub const XL: f32 = 24.0;
}

/// Corner radii.
pub mod radius {
    /// Inputs, buttons, chips.
    pub const CONTROL: u8 = 6;
    /// Cards and panels.
    pub const CARD: u8 = 10;
}

pub const SIDEBAR_WIDTH: f32 = 220.0;
/// Widest a page of forms grows: long lines of controls are hard to scan.
pub const PAGE_MAX_WIDTH: f32 = 860.0;
/// Height shared by buttons, inputs and rows of controls.
pub const CONTROL_HEIGHT: f32 = 32.0;

pub struct Palette {
    pub dark: bool,
    /// Window background, behind the cards.
    pub bg: Color32,
    pub sidebar: Color32,
    /// Cards, inputs and the touchpad silhouette.
    pub surface: Color32,
    /// Resting fill of buttons and chips.
    pub raised: Color32,
    pub hover: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub text_faint: Color32,
    pub accent: Color32,
    /// Text and icons on top of `accent`.
    pub on_accent: Color32,
    pub accent_soft: Color32,
    pub ok: Color32,
    pub ok_soft: Color32,
    pub warn: Color32,
    pub warn_soft: Color32,
    pub bad: Color32,
    pub bad_soft: Color32,
    /// Strokes and fingers: distinguishable, and legible on `surface`.
    pub series: [Color32; 6],
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

static LIGHT: Palette = Palette {
    dark: false,
    bg: rgb(0xF4, 0xF5, 0xF8),
    sidebar: rgb(0xEB, 0xED, 0xF2),
    surface: rgb(0xFF, 0xFF, 0xFF),
    raised: rgb(0xF0, 0xF2, 0xF6),
    hover: rgb(0xE4, 0xE8, 0xEF),
    border: rgb(0xDF, 0xE2, 0xE9),
    border_strong: rgb(0xC6, 0xCB, 0xD6),
    text: rgb(0x1A, 0x1D, 0x24),
    text_dim: rgb(0x58, 0x61, 0x72),
    text_faint: rgb(0x80, 0x89, 0x99),
    accent: rgb(0x2F, 0x62, 0xE0),
    on_accent: rgb(0xFF, 0xFF, 0xFF),
    accent_soft: rgb(0xE3, 0xEB, 0xFD),
    ok: rgb(0x16, 0x86, 0x4B),
    ok_soft: rgb(0xE2, 0xF4, 0xEA),
    warn: rgb(0x9A, 0x62, 0x00),
    warn_soft: rgb(0xFB, 0xF0, 0xD9),
    bad: rgb(0xC6, 0x34, 0x34),
    bad_soft: rgb(0xFB, 0xE6, 0xE6),
    series: [
        rgb(0x2F, 0x62, 0xE0),
        rgb(0xD9, 0x6A, 0x0B),
        rgb(0x16, 0x86, 0x4B),
        rgb(0xB8, 0x3A, 0xA6),
        rgb(0x8A, 0x7A, 0x00),
        rgb(0x6A, 0x4F, 0xD6),
    ],
};

static DARK: Palette = Palette {
    dark: true,
    bg: rgb(0x14, 0x16, 0x1B),
    sidebar: rgb(0x0F, 0x11, 0x15),
    surface: rgb(0x1C, 0x1F, 0x26),
    raised: rgb(0x26, 0x2A, 0x33),
    hover: rgb(0x31, 0x36, 0x41),
    border: rgb(0x2C, 0x31, 0x3B),
    border_strong: rgb(0x40, 0x46, 0x53),
    text: rgb(0xE9, 0xEB, 0xF0),
    text_dim: rgb(0xA0, 0xA8, 0xB6),
    text_faint: rgb(0x72, 0x7A, 0x89),
    accent: rgb(0x6C, 0x9B, 0xFF),
    on_accent: rgb(0x0B, 0x13, 0x2B),
    accent_soft: rgb(0x20, 0x2E, 0x52),
    ok: rgb(0x4F, 0xC9, 0x8B),
    ok_soft: rgb(0x16, 0x32, 0x27),
    warn: rgb(0xE8, 0xB0, 0x4A),
    warn_soft: rgb(0x38, 0x2D, 0x14),
    bad: rgb(0xF2, 0x7A, 0x78),
    bad_soft: rgb(0x40, 0x1E, 0x20),
    series: [
        rgb(0x6C, 0x9B, 0xFF),
        rgb(0xF0, 0x9A, 0x4A),
        rgb(0x5C, 0xD0, 0x95),
        rgb(0xE2, 0x7F, 0xD2),
        rgb(0xE6, 0xD2, 0x5A),
        rgb(0x9C, 0x8C, 0xFF),
    ],
};

/// The palette of the theme `ui` is drawn in.
pub fn pal(ui: &egui::Ui) -> &'static Palette {
    if ui.visuals().dark_mode {
        &DARK
    } else {
        &LIGHT
    }
}

pub fn pal_of(ctx: &egui::Context) -> &'static Palette {
    match ctx.theme() {
        Theme::Dark => &DARK,
        Theme::Light => &LIGHT,
    }
}

/// Font families beyond egui's own.
pub mod fonts {
    use super::{FontFamily, FontId};

    pub fn semibold() -> FontFamily {
        FontFamily::Name("semibold".into())
    }

    pub fn icons() -> FontFamily {
        FontFamily::Name("icons".into())
    }

    pub fn text(size: f32) -> FontId {
        FontId::proportional(size)
    }

    pub fn strong(size: f32) -> FontId {
        FontId::new(size, semibold())
    }

    pub fn icon(size: f32) -> FontId {
        FontId::new(size, icons())
    }

    pub fn mono(size: f32) -> FontId {
        FontId::monospace(size)
    }
}

/// Type scale.
pub mod size {
    pub const TITLE: f32 = 22.0;
    pub const SECTION: f32 = 15.0;
    pub const BODY: f32 = 14.0;
    pub const SMALL: f32 = 12.5;
    pub const ICON: f32 = 17.0;
}

/// Windows' own UI fonts when present: egui's built-in fonts lack the
/// symbols used in labels (arrows, ×, ·) and look foreign next to the
/// system. Elsewhere egui's fonts stay.
fn system_fonts() -> Vec<(&'static str, Vec<u8>)> {
    let Some(dir) = std::env::var_os("WINDIR").map(|w| std::path::PathBuf::from(w).join("Fonts"))
    else {
        return Vec::new();
    };
    ["segoeui.ttf", "seguisb.ttf", "seguisym.ttf", "consola.ttf"]
        .into_iter()
        .filter_map(|file| std::fs::read(dir.join(file)).ok().map(|b| (file, b)))
        .collect()
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let system = system_fonts();
    let has = |name: &str| system.iter().any(|(n, _)| *n == name);
    let (has_text, has_semibold, has_symbols, has_mono) = (
        has("segoeui.ttf"),
        has("seguisb.ttf"),
        has("seguisym.ttf"),
        has("consola.ttf"),
    );
    for (name, bytes) in system {
        fonts
            .font_data
            .insert(name.into(), Arc::new(FontData::from_owned(bytes)));
    }
    fonts.font_data.insert(
        "phosphor".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/Phosphor-subset.ttf"
        ))),
    );

    let proportional = fonts.families.entry(FontFamily::Proportional).or_default();
    if has_text {
        proportional.insert(0, "segoeui.ttf".into());
    }
    if has_symbols {
        proportional.push("seguisym.ttf".into());
    }
    // Icons are private-use codepoints. They come first because the system
    // symbol font maps some of the same codepoints to other glyphs; the
    // icon font has nothing else, so no text can resolve to it by mistake.
    proportional.insert(0, "phosphor".into());
    let base: Vec<String> = proportional.clone();

    let mono = fonts.families.entry(FontFamily::Monospace).or_default();
    if has_mono {
        mono.insert(0, "consola.ttf".into());
    }
    if has_symbols {
        mono.push("seguisym.ttf".into());
    }
    mono.insert(0, "phosphor".into());

    let mut semibold = base.clone();
    if has_semibold {
        semibold.insert(0, "seguisb.ttf".into());
    }
    fonts.families.insert(self::fonts::semibold(), semibold);
    // Icons only: their private-use codepoints cannot clash with text.
    fonts
        .families
        .insert(self::fonts::icons(), vec!["phosphor".into()]);
    ctx.set_fonts(fonts);
}

fn visuals(p: &Palette) -> Visuals {
    let mut v = if p.dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    v.panel_fill = p.bg;
    v.window_fill = p.surface;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = CornerRadius::same(radius::CARD + 2);
    v.window_shadow = Shadow {
        offset: [0, 8],
        blur: 24,
        spread: 0,
        color: Color32::from_black_alpha(if p.dark { 140 } else { 44 }),
    };
    v.popup_shadow = Shadow {
        offset: [0, 4],
        blur: 14,
        spread: 0,
        color: Color32::from_black_alpha(if p.dark { 120 } else { 36 }),
    };
    v.menu_corner_radius = CornerRadius::same(radius::CONTROL + 2);
    v.extreme_bg_color = p.surface;
    v.faint_bg_color = p.raised;
    v.code_bg_color = p.raised;
    v.override_text_color = Some(p.text);
    v.weak_text_color = Some(p.text_dim);
    v.hyperlink_color = p.accent;
    v.warn_fg_color = p.warn;
    v.error_fg_color = p.bad;
    v.selection.bg_fill = p.accent_soft;
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.text_cursor.stroke = Stroke::new(1.5, p.accent);
    v.striped = false;
    v.slider_trailing_fill = true;
    v.handle_shape = egui::style::HandleShape::Circle;

    let r = CornerRadius::same(radius::CONTROL);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.surface;
    w.noninteractive.weak_bg_fill = p.surface;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);

    w.inactive.bg_fill = p.raised;
    w.inactive.weak_bg_fill = p.raised;
    w.inactive.bg_stroke = Stroke::new(1.0, p.border);
    w.inactive.fg_stroke = Stroke::new(1.0, p.text);

    w.hovered.bg_fill = p.hover;
    w.hovered.weak_bg_fill = p.hover;
    w.hovered.bg_stroke = Stroke::new(1.0, p.border_strong);
    w.hovered.fg_stroke = Stroke::new(1.0, p.text);
    w.hovered.expansion = 0.0;

    w.active.bg_fill = p.accent_soft;
    w.active.weak_bg_fill = p.accent_soft;
    w.active.bg_stroke = Stroke::new(1.0, p.accent);
    w.active.fg_stroke = Stroke::new(1.0, p.text);
    w.active.expansion = 0.0;

    w.open.bg_fill = p.hover;
    w.open.weak_bg_fill = p.hover;
    w.open.bg_stroke = Stroke::new(1.0, p.border_strong);
    w.open.fg_stroke = Stroke::new(1.0, p.text);

    for s in [
        &mut w.noninteractive,
        &mut w.inactive,
        &mut w.hovered,
        &mut w.active,
        &mut w.open,
    ] {
        s.corner_radius = r;
    }
    v
}

fn style(style: &mut egui::Style) {
    style.spacing.item_spacing = egui::vec2(space::S, space::S);
    style.spacing.button_padding = egui::vec2(space::M, 6.0);
    style.spacing.interact_size = egui::vec2(36.0, CONTROL_HEIGHT);
    style.spacing.window_margin = Margin::same(space::XL as i8);
    style.spacing.menu_margin = Margin::same(space::XS as i8);
    style.spacing.indent = space::L;
    style.spacing.slider_width = 180.0;
    style.spacing.slider_rail_height = 4.0;
    style.spacing.combo_width = 180.0;
    style.spacing.text_edit_width = 220.0;
    style.spacing.scroll.bar_width = 8.0;
    style.spacing.scroll.floating = true;
    style.spacing.scroll.floating_width = 4.0;
    style.spacing.scroll.floating_allocated_width = 0.0;
    style.spacing.icon_width = 18.0;
    style.spacing.icon_width_inner = 10.0;
    style.spacing.icon_spacing = space::S;
    style.spacing.tooltip_width = 320.0;
    style.interaction.tooltip_delay = 0.35;
    style.animation_time = 0.12;
    style.text_styles = [
        (TextStyle::Heading, fonts::strong(size::TITLE)),
        (TextStyle::Body, fonts::text(size::BODY)),
        (TextStyle::Button, fonts::text(size::BODY)),
        (TextStyle::Small, fonts::text(size::SMALL)),
        (TextStyle::Monospace, fonts::mono(13.0)),
    ]
    .into();
}

/// Installs fonts, both themes and the appearance the user chose.
pub fn install(ctx: &egui::Context, choice: ThemeChoice) {
    install_fonts(ctx);
    for (theme, p) in [(Theme::Light, &LIGHT), (Theme::Dark, &DARK)] {
        ctx.set_visuals_of(theme, visuals(p));
        ctx.style_mut_of(theme, style);
    }
    apply_choice(ctx, choice);
}

/// Follows the system, or forces one theme.
pub fn apply_choice(ctx: &egui::Context, choice: ThemeChoice) {
    ctx.set_theme(match choice {
        ThemeChoice::System => egui::ThemePreference::System,
        ThemeChoice::Light => egui::ThemePreference::Light,
        ThemeChoice::Dark => egui::ThemePreference::Dark,
    });
}
