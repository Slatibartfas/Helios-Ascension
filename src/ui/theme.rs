//! Shared "Tactical OS" visual theme for all UI panels.
//!
//! Centralises palette constants, font helpers, and the global egui `Visuals`
//! configuration so every panel inherits the same dark-navy aesthetic.
//!
//! Many helper functions are part of the intended public API and will be picked
//! up as more panels migrate to the theme — suppress dead_code for this module.
#![allow(dead_code)]

use bevy_egui::egui;

// ─── Core Palette ────────────────────────────────────────────────────────

/// Deep navy background at high opacity — used for panel fills.
pub const BG: egui::Color32 = egui::Color32::from_rgba_premultiplied(8, 13, 26, 244);
/// Fully opaque version of the background for `CentralPanel` / `Visuals`.
pub const BG_SOLID: egui::Color32 = egui::Color32::from_rgb(8, 13, 26);
/// Slightly lighter panel surface (cards, tiles, sub-sections).
pub const SURFACE: egui::Color32 = egui::Color32::from_rgb(13, 17, 23);
/// Mid-tone surface for hovered / raised elements.
pub const SURFACE_RAISED: egui::Color32 = egui::Color32::from_rgb(20, 26, 36);
/// Bright widget / input background tint.
pub const SURFACE_INPUT: egui::Color32 = egui::Color32::from_rgb(16, 20, 30);

// ─── Accent Colours ──────────────────────────────────────────────────────

/// Primary cyan accent for highlights, selection, and interactive elements.
pub const ACCENT: egui::Color32 = egui::Color32::from_rgb(0, 242, 255);
/// Dimmed accent (~31% alpha) for secondary outlines and inactive glyphs.
pub const ACCENT_DIM: egui::Color32 = egui::Color32::from_rgba_premultiplied(0, 242, 255, 80);
/// Very faint accent for borders, grid lines.
pub const BORDER: egui::Color32 = egui::Color32::from_rgba_premultiplied(0, 242, 255, 40);

// ─── Semantic Colours ────────────────────────────────────────────────────

/// Positive / success — used for good status, income, production.
pub const GREEN: egui::Color32 = egui::Color32::from_rgb(39, 174, 96);
/// Warning / amber — used for caution states, moderate thresholds.
pub const AMBER: egui::Color32 = egui::Color32::from_rgb(230, 170, 50);
/// Gold — treasury / financial values.
pub const GOLD: egui::Color32 = egui::Color32::from_rgb(255, 215, 0);
/// Negative / danger — errors, deficits, damage.
pub const RED: egui::Color32 = egui::Color32::from_rgb(231, 76, 60);
/// Research Point blue.
pub const RP_BLUE: egui::Color32 = egui::Color32::from_rgb(100, 200, 255);
/// Engineering Point teal.
pub const EP_TEAL: egui::Color32 = egui::Color32::from_rgb(100, 255, 200);
/// Star / warm gold for star names and starmap labels.
pub const STAR_GOLD: egui::Color32 = egui::Color32::from_rgb(255, 220, 100);
/// Gravity-assist / flyby purple accent.
pub const GRAVITY_ASSIST: egui::Color32 = egui::Color32::from_rgb(180, 130, 255);

// ─── Text Colours ────────────────────────────────────────────────────────

/// Bright foreground (primary text).
pub const TEXT: egui::Color32 = egui::Color32::from_rgb(210, 220, 235);
/// Value / data foreground — slightly brighter than normal text.
pub const TEXT_VALUE: egui::Color32 = egui::Color32::from_rgb(200, 215, 240);
/// Dimmed label text (secondary information, captions).
pub const TEXT_DIM: egui::Color32 = egui::Color32::from_rgb(120, 140, 170);
/// Very faint hint text.
pub const TEXT_HINT: egui::Color32 = egui::Color32::from_rgb(90, 105, 130);
/// Inactive nav-bar icon tint — bright enough to read clearly but distinct from the
/// active cyan accent so the selected button still stands out.
pub const ICON_INACTIVE: egui::Color32 = egui::Color32::from_rgb(190, 205, 225);

// ─── Resource Category Colours ───────────────────────────────────────────

/// Volatiles (Water, H₂, NH₃…)
pub const CAT_VOLATILES: egui::Color32 = egui::Color32::from_rgb(80, 190, 255);
/// Atmospheric gases
pub const CAT_ATMOSPHERIC: egui::Color32 = egui::Color32::from_rgb(170, 210, 255);
/// Construction materials (Iron, Al, Ti…)
pub const CAT_CONSTRUCTION: egui::Color32 = egui::Color32::from_rgb(205, 150, 80);
/// Fusion fuel
pub const CAT_FUSION: egui::Color32 = egui::Color32::from_rgb(255, 100, 200);
/// Fissile materials
pub const CAT_FISSILES: egui::Color32 = egui::Color32::from_rgb(80, 230, 80);
/// Precious metals
pub const CAT_PRECIOUS: egui::Color32 = egui::Color32::from_rgb(255, 215, 0);
/// Strategic materials (Copper, REE, Li, S)
pub const CAT_STRATEGIC: egui::Color32 = egui::Color32::from_rgb(180, 120, 255);
/// Exotic materials (Antimatter, Exotic Matter, Metamaterials, Computronium)
pub const CAT_EXOTIC: egui::Color32 = egui::Color32::from_rgb(255, 60, 120);

// ─── Font Helpers ────────────────────────────────────────────────────────

/// Section heading font (Hubot Sans SemiBold Condensed).
pub fn heading() -> egui::FontId {
    egui::FontId::new(13.0, egui::FontFamily::Name("semibold".into()))
}

/// Panel / body title font (Hubot Sans ExtraBold Expanded).
pub fn title() -> egui::FontId {
    egui::FontId::new(20.0, egui::FontFamily::Name("heading".into()))
}

/// Monospace font at a specific size.
pub fn mono(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Monospace)
}

/// Proportional body font at a specific size.
pub fn body(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Proportional)
}

// ─── Common Widgets ──────────────────────────────────────────────────────

/// Standard dark panel frame used by side panels.
pub fn panel_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(BG)
        .stroke(egui::Stroke::new(1.0, BORDER))
        .inner_margin(egui::Margin::same(10))
}

/// Frame for central panels (fully opaque).
pub fn central_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(BG_SOLID)
        .stroke(egui::Stroke::new(1.0, BORDER))
        .inner_margin(egui::Margin::same(8))
}

/// Frame for prominent section cards inside full-screen menus.
pub fn section_frame() -> egui::Frame {
    egui::Frame::NONE
        .inner_margin(egui::Margin::same(2))
        .corner_radius(4.0)
}

/// Slightly raised variant used for nested summary blocks.
pub fn elevated_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(SURFACE)
        .stroke(egui::Stroke::new(1.0, BORDER))
        .inner_margin(egui::Margin::same(8))
        .corner_radius(3.0)
}

/// Frame for tooltip popups.
pub fn tooltip_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(egui::Color32::from_rgba_unmultiplied(12, 16, 28, 245))
        .stroke(egui::Stroke::new(1.5, ACCENT_DIM))
        .inner_margin(egui::Margin::same(10))
        .corner_radius(4.0)
}

/// Thin horizontal tactical divider.
pub fn divider(ui: &mut egui::Ui) {
    ui.add_space(4.0);
    let rect = ui.available_rect_before_wrap();
    let y = rect.top();
    ui.painter().hline(
        rect.left()..=rect.right(),
        y,
        egui::Stroke::new(1.0, BORDER),
    );
    ui.add_space(6.0);
}

/// Draw a dim-label + value row in a grid.
pub fn stat_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.label(egui::RichText::new(label).font(mono(10.0)).color(TEXT_DIM));
    ui.label(
        egui::RichText::new(value)
            .font(mono(12.0))
            .color(TEXT_VALUE),
    );
    ui.end_row();
}

/// Get colour for a resource category name.
pub fn category_color(category: &str) -> egui::Color32 {
    match category {
        "Volatiles" => CAT_VOLATILES,
        "Atmospheric Gases" => CAT_ATMOSPHERIC,
        "Construction" => CAT_CONSTRUCTION,
        "Fusion Fuel" => CAT_FUSION,
        "Fissiles" => CAT_FISSILES,
        "Precious Metals" => CAT_PRECIOUS,
        "Strategic" => CAT_STRATEGIC,
        "Exotic" => CAT_EXOTIC,
        _ => TEXT_DIM,
    }
}

// ─── Global Visuals ──────────────────────────────────────────────────────

/// Configure the egui context with the Tactical OS dark theme.
///
/// Call once at startup via a dedicated system.
pub fn apply_global_visuals(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();

    // ── Backgrounds ──────────────────────────────────────────────
    visuals.panel_fill = BG_SOLID;
    visuals.window_fill = egui::Color32::from_rgb(10, 14, 24);
    visuals.extreme_bg_color = SURFACE_INPUT;
    visuals.faint_bg_color = SURFACE;

    // ── Text & selection ─────────────────────────────────────────
    visuals.override_text_color = Some(TEXT);
    visuals.selection.bg_fill = egui::Color32::from_rgba_premultiplied(0, 160, 180, 60);
    visuals.selection.stroke = egui::Stroke::new(1.0, ACCENT);

    // ── Window chrome ────────────────────────────────────────────
    visuals.window_stroke = egui::Stroke::new(1.0, BORDER);
    visuals.window_shadow = egui::Shadow::NONE;
    visuals.popup_shadow = egui::Shadow::NONE;

    // ── Separators ───────────────────────────────────────────────
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(0.5, BORDER);

    // ── Widget states ────────────────────────────────────────────
    // Non-interactive (labels, separators)
    visuals.widgets.noninteractive.bg_fill = SURFACE;
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, TEXT);
    visuals.widgets.noninteractive.weak_bg_fill = SURFACE;

    // Inactive (buttons at rest)
    visuals.widgets.inactive.bg_fill = SURFACE;
    visuals.widgets.inactive.weak_bg_fill = SURFACE;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(0.5, BORDER);
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, TEXT_VALUE);

    // Hovered
    visuals.widgets.hovered.bg_fill = SURFACE_RAISED;
    visuals.widgets.hovered.weak_bg_fill = SURFACE_RAISED;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, ACCENT_DIM);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, ACCENT);

    // Active (pressed)
    visuals.widgets.active.bg_fill = egui::Color32::from_rgb(0, 80, 90);
    visuals.widgets.active.weak_bg_fill = egui::Color32::from_rgb(0, 80, 90);
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.5, ACCENT);
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.5, ACCENT);

    // Open (e.g. combo box, expanded collapsing header)
    visuals.widgets.open.bg_fill = SURFACE_RAISED;
    visuals.widgets.open.weak_bg_fill = SURFACE_RAISED;
    visuals.widgets.open.bg_stroke = egui::Stroke::new(1.0, ACCENT_DIM);
    visuals.widgets.open.fg_stroke = egui::Stroke::new(1.0, ACCENT);

    // ── Misc ─────────────────────────────────────────────────────
    visuals.striped = true;
    visuals.slider_trailing_fill = true;

    ctx.set_visuals(visuals);
    ctx.style_mut(|style| {
        style.interaction.tooltip_delay = 0.2;
    });
}
