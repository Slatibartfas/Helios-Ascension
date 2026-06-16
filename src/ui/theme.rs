//! Shared "Tactical OS" visual theme for all UI panels.
//!
//! Centralises palette constants, font helpers, and the global egui `Visuals`
//! configuration so every panel inherits the same dark-navy aesthetic.
//!
//! Many helper functions are part of the intended public API and will be picked
//! up as more panels migrate to the theme — suppress dead_code for this module.
#![allow(dead_code)]

use bevy::prelude::{
    default, BackgroundColor, BorderColor, Button, Commands, Entity, Node, Text, TextColor,
    TextFont, UiRect, Val,
};
use bevy_egui::egui;

use crate::astronomy::OceanType;
use crate::plugins::solar_system_data::BodyType;
use crate::research::types::TechCategory;
use crate::shipbuilding::types::ShipModuleCategory;

// ─── Spacing Scale ──────────────────────────────────────────────────────
//
// Single source of truth for the 4-px-based spacing grid. Every panel,
// sub-section, and intra-element gap in `src/ui/` should reference one of
// these constants instead of inlining literal f32 values. The scale is
// deliberately small (5 stops) so panels stay visually coherent.

/// Spacing scale in pixels, 4-px-based grid.
///
/// * `xs` — hairline gap (e.g. tight separators).
/// * `sm` — small gap between related elements (most common; default
///   intra-row spacing).
/// * `md` — medium gap (panel inner padding, section breathing room).
/// * `lg` — large gap (sub-section separation, generous tooltip padding).
/// * `xl` — extra-large gap (top-level panel separation).
#[allow(non_snake_case, non_upper_case_globals)]
pub mod Spacing {
    pub const xs: f32 = 4.0;
    pub const sm: f32 = 8.0;
    pub const md: f32 = 10.0;
    pub const lg: f32 = 12.0;
    pub const xl: f32 = 16.0;
}

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
/// Pale cyan used for historical probe orbit rings and the read-only
/// "Deep Space Probes" section in the fleet panel.  Matches the Bevy
/// `Color::PROBE_ORBIT` literal in the `theme::Color` submodule
/// (srgba(0.55, 0.85, 1.00, 0.55) → premul 77, 119, 140, 140) so the
/// on-starmap orbit line and the panel row indicator stay visually
/// consistent.  Uses `from_rgba_premultiplied` (const) rather than
/// `from_rgba_unmultiplied` (non-const) so the `pub const` initializer
/// is valid.
pub const PROBE_ORBIT: egui::Color32 = egui::Color32::from_rgba_premultiplied(77, 119, 140, 140);

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

// ─── Body Type Colours ──────────────────────────────────────────────────
//
// Display colours for celestial body types — used by the dossier, starmap,
// and any panel that labels or chips a body. Promoted from the local palette
// that used to live in `dossier_panel.rs`.

/// Star — bright golden, used for stellar bodies and stellar markers.
pub const BODY_STAR: egui::Color32 = egui::Color32::from_rgb(255, 220, 100);
/// Terrestrial planet — pale cyan-blue, used for rocky worlds.
pub const BODY_TERRESTRIAL: egui::Color32 = egui::Color32::from_rgb(100, 180, 255);
/// Gas giant — saturn-tan, used for Jupiter/Neptune-class worlds.
pub const BODY_GAS_GIANT: egui::Color32 = egui::Color32::from_rgb(230, 200, 130);
/// Dwarf planet — dim brown, used for Ceres/Pluto-class.
pub const BODY_DWARF_PLANET: egui::Color32 = egui::Color32::from_rgb(180, 140, 90);
/// Moon — pale grey-blue, used for natural satellites.
pub const BODY_MOON: egui::Color32 = egui::Color32::from_rgb(180, 180, 200);
/// Asteroid — dusty brown, used for belt objects and individual rocks.
pub const BODY_ASTEROID: egui::Color32 = egui::Color32::from_rgb(160, 130, 90);
/// Comet — pale cyan, used for icy visitors.
pub const BODY_COMET: egui::Color32 = egui::Color32::from_rgb(140, 200, 240);
/// Ring — pale tan, used for planetary ring systems.
pub const BODY_RING: egui::Color32 = egui::Color32::from_rgb(200, 180, 140);

/// Get the display colour for a celestial body type.
pub fn body_type_color(body_type: BodyType) -> egui::Color32 {
    match body_type {
        BodyType::Star => BODY_STAR,
        BodyType::Planet => BODY_TERRESTRIAL,
        BodyType::GasGiant => BODY_GAS_GIANT,
        BodyType::DwarfPlanet => BODY_DWARF_PLANET,
        BodyType::Moon => BODY_MOON,
        BodyType::Asteroid => BODY_ASTEROID,
        BodyType::Comet => BODY_COMET,
        BodyType::Ring => BODY_RING,
    }
}

// ─── Atmospheric Gas Colours ────────────────────────────────────────────
//
// The 10-colour palette that used to live privately in `dossier_panel.rs`'s
// `gas_color` helper. Each gas gets a named constant; `gas_color(name)` is
// the case-insensitive prefix dispatcher used by the dossier atmosphere
// section.

/// Nitrogen / N₂ — deep blue.
pub const GAS_N2: egui::Color32 = egui::Color32::from_rgb(26, 82, 118);
/// Oxygen / O₂ — cyan.
pub const GAS_O2: egui::Color32 = egui::Color32::from_rgb(0, 200, 220);
/// Carbon dioxide / CO₂ — amber.
pub const GAS_CO2: egui::Color32 = egui::Color32::from_rgb(230, 126, 34);
/// Argon / Ar — slate.
pub const GAS_AR: egui::Color32 = egui::Color32::from_rgb(86, 101, 115);
/// Methane / CH₄ — gold.
pub const GAS_CH4: egui::Color32 = egui::Color32::from_rgb(243, 156, 18);
/// Hydrogen / H₂ — pale blue.
pub const GAS_H2: egui::Color32 = egui::Color32::from_rgb(174, 214, 241);
/// Helium / He — light mint.
pub const GAS_HE: egui::Color32 = egui::Color32::from_rgb(213, 245, 227);
/// Sulfur dioxide / SO₂ — olive yellow.
pub const GAS_SO2: egui::Color32 = egui::Color32::from_rgb(180, 160, 30);
/// Neon / Ne — neon red.
pub const GAS_NE: egui::Color32 = egui::Color32::from_rgb(255, 100, 100);
/// Default gas colour — generic grey-blue for unrecognised gases.
pub const GAS_DEFAULT: egui::Color32 = egui::Color32::from_rgb(80, 80, 100);

/// Get the colour for an atmospheric gas by name (case-insensitive prefix).
pub fn gas_color(name: &str) -> egui::Color32 {
    let lower = name.to_lowercase();
    if lower.starts_with("n2") || lower.starts_with("nitrogen") {
        GAS_N2
    } else if lower.starts_with("o2") || lower.starts_with("oxygen") {
        GAS_O2
    } else if lower.starts_with("co2") || lower.starts_with("carbon d") {
        GAS_CO2
    } else if lower.starts_with("ar") || lower.starts_with("argon") {
        GAS_AR
    } else if lower.starts_with("ch4") || lower.starts_with("methane") {
        GAS_CH4
    } else if lower.starts_with("h2") || lower.starts_with("hydrogen") {
        GAS_H2
    } else if lower.starts_with("he") || lower.starts_with("helium") {
        GAS_HE
    } else if lower.starts_with("so2") || lower.starts_with("sulfur") {
        GAS_SO2
    } else if lower.starts_with("ne") || lower.starts_with("neon") {
        GAS_NE
    } else {
        GAS_DEFAULT
    }
}

// ─── Ocean / Surface Liquid Colours ─────────────────────────────────────

/// Surface water ocean — clear blue.
pub const OCEAN_WATER: egui::Color32 = egui::Color32::from_rgb(64, 164, 223);
/// Methane lake — golden brown (Titan).
pub const OCEAN_METHANE: egui::Color32 = egui::Color32::from_rgb(180, 140, 60);
/// Hydrocarbon lake — same as methane (Titan-style).
pub const OCEAN_HYDROCARBON: egui::Color32 = OCEAN_METHANE;
/// Ammonia ocean — purple.
pub const OCEAN_AMMONIA: egui::Color32 = egui::Color32::from_rgb(160, 120, 200);
/// Subsurface ocean — pale cyan, used for Europa/Enceladus-style sub-ice oceans.
pub const OCEAN_SUBSURFACE: egui::Color32 = egui::Color32::from_rgb(100, 180, 220);

/// Get the display colour for an ocean type.
pub fn ocean_color(ocean_type: OceanType) -> egui::Color32 {
    match ocean_type {
        OceanType::Water => OCEAN_WATER,
        OceanType::Methane => OCEAN_METHANE,
        OceanType::Hydrocarbon => OCEAN_HYDROCARBON,
        OceanType::Ammonia => OCEAN_AMMONIA,
        OceanType::Subsurface => OCEAN_SUBSURFACE,
    }
}

// ─── Status Colours ─────────────────────────────────────────────────────

/// Warning — amber-yellow status (deficits, attention needed).
pub const STATUS_WARN: egui::Color32 = egui::Color32::from_rgb(255, 200, 0);
/// Error — red status (failures, critical).
pub const STATUS_ERROR: egui::Color32 = egui::Color32::from_rgb(255, 100, 100);
/// Success — green status (positive, complete).
pub const STATUS_SUCCESS: egui::Color32 = egui::Color32::from_rgb(100, 255, 100);
/// Success-dim — dimmer green for secondary positive highlights.
pub const STATUS_SUCCESS_DIM: egui::Color32 = egui::Color32::from_rgb(100, 220, 100);
/// Neutral — light grey for plain/informational text.
pub const STATUS_NEUTRAL: egui::Color32 = egui::Color32::from_rgb(220, 220, 220);
/// Muted — mid grey for inactive/disabled UI.
pub const STATUS_MUTED: egui::Color32 = egui::Color32::from_rgb(180, 180, 180);

// ─── Anchor / Marker ────────────────────────────────────────────────────

/// Anchor / marker — orange-gold for ⚓ glyphs and similar emphasis marks.
pub const ANCHOR: egui::Color32 = egui::Color32::from_rgb(255, 200, 100);

// ─── Selected-Button / Active-Item Background ───────────────────────────

/// Dark teal background used for the active/selected state of row buttons
/// and time-scale presets — sits between `SURFACE_RAISED` and `ACCENT` so
/// the bright `ACCENT` text reads cleanly on top.
pub const BUTTON_ACTIVE_BG: egui::Color32 = egui::Color32::from_rgb(0, 55, 70);

// ─── Focus Ring ─────────────────────────────────────────────────────────
//
// egui 0.33 does not draw an automatic focus ring on most widgets, so any
// panel that wants a visible keyboard-focus indicator needs to draw one
// itself via `theme::focus_ring_stroke()`. The colour is a desaturated
// cyan-amber so it reads as "active, not pressed" against the dark
// `SURFACE` / `SURFACE_RAISED` widget backgrounds and doesn't get confused
// with the bright cyan `ACCENT` used for hover / selection.

/// Focus ring stroke colour — drawn around widgets that currently hold
/// keyboard focus. Pairs with `focus_ring_stroke()` for a ready-to-paint
/// [`egui::Stroke`].
pub const FOCUS_RING: egui::Color32 = egui::Color32::from_rgb(255, 200, 80);
/// Standard focus-ring width in points. Use the version returned by
/// `focus_ring_stroke()` so panels don't have to remember the width.
pub const FOCUS_RING_WIDTH: f32 = 1.75;

/// Build the standard focus-ring stroke (colour + width).
pub fn focus_ring_stroke() -> egui::Stroke {
    egui::Stroke::new(FOCUS_RING_WIDTH, FOCUS_RING)
}

/// Draw a focus ring around `rect` if `focused` is true. Convenience helper
/// for the common "ring around a button when it has keyboard focus" pattern —
/// callers that already have a `Rect` (e.g. from `response.rect`) and
/// already know the focus state can call this rather than open-coding the
/// `painter.rect_stroke` call.
pub fn paint_focus_ring(painter: &egui::Painter, rect: egui::Rect, focused: bool) {
    if !focused {
        return;
    }
    painter.rect_stroke(
        rect.expand(2.0),
        3.0,
        focus_ring_stroke(),
        egui::StrokeKind::Outside,
    );
}

// ─── Surface Variants ───────────────────────────────────────────────────

/// Dim border for fine sub-elements (axis pips, thin dividers) that should
/// be slightly darker than `BORDER` so they read as inset rather than chrome.
pub const BORDER_DIM: egui::Color32 = egui::Color32::from_rgb(40, 45, 55);
/// Slightly lighter variant of `SURFACE` for resource-symbol tiles where
/// `SURFACE` would blend into the panel background.
pub const SURFACE_RAISED_2: egui::Color32 = egui::Color32::from_rgb(50, 55, 65);

// ─── Tooltip & Overlay Surfaces ────────────────────────────────────────
//
// Semi-transparent panel fills used by tooltip frames in `src/ui/mod.rs`
// and the tech-tree popover in `src/ui/tech_tree.rs`. Pulled out of inline
// `from_rgba_unmultiplied(...)` calls so the audit script has a single
// home for them. Alpha 245 keeps the underlying scene visible (~4% bleed).

/// Tooltip frame fill — used by the 3 shipbuilding/misc hover tooltips
/// in `src/ui/mod.rs` (LP hover, library hover, blueprint hover).
pub const TOOLTIP_BG: egui::Color32 = egui::Color32::from_rgba_unmultiplied_const(12, 16, 28, 245);
/// Tech-tree popover fill — same nominal hue as `SURFACE` but at 245 alpha
/// so the grid behind the tooltip remains slightly visible.
pub const TOOLTIP_BG_ALT: egui::Color32 =
    egui::Color32::from_rgba_unmultiplied_const(13, 17, 23, 245);

// ─── Difficulty / Tier Colours ──────────────────────────────────────────

/// Moderate difficulty — yellow used in the dossier cost-indicator chips.
pub const DIFFICULTY_MODERATE: egui::Color32 = egui::Color32::from_rgb(200, 200, 50);
/// Star-icon (solar system view) — yellow used for the ★ glyph in dossier headers.
pub const SOLAR_STAR: egui::Color32 = egui::Color32::from_rgb(200, 200, 50);
/// Tier 4 (high) — bright cyan, used in dossier resource-availability chips.
pub const TIER_4: egui::Color32 = egui::Color32::from_rgb(120, 200, 255);
/// Tier 3 (mid) — pale blue.
pub const TIER_3: egui::Color32 = egui::Color32::from_rgb(180, 200, 220);
/// Tier ≤ 2 / unknown — dim grey.
pub const TIER_OTHER: egui::Color32 = egui::Color32::from_rgb(80, 90, 100);

/// Get the dossier tier colour for a numeric tier (0..=5).
pub fn tier_color(tier: u8) -> egui::Color32 {
    match tier {
        5 => ACCENT,
        4 => TIER_4,
        3 => TIER_3,
        2 => TEXT_DIM,
        _ => TIER_OTHER,
    }
}

// ─── Tech Tree Node State Colours ───────────────────────────────────────
//
// Drawn on the tech-tree canvas (see `tech_tree.rs`). Each state has a
// dimmer base colour plus a brighter "in-path" variant so the player's
// planned-research path stands out against ambient available research.

/// Node fill — unlocked, ambient.
pub const TECH_NODE_UNLOCKED: egui::Color32 = egui::Color32::from_rgb(25, 70, 25);
/// Node fill — unlocked, in path.
pub const TECH_NODE_UNLOCKED_PATH: egui::Color32 = egui::Color32::from_rgb(30, 90, 30);
/// Node fill — researching, ambient.
pub const TECH_NODE_RESEARCHING: egui::Color32 = egui::Color32::from_rgb(15, 50, 95);
/// Node fill — researching, in path.
pub const TECH_NODE_RESEARCHING_PATH: egui::Color32 = egui::Color32::from_rgb(20, 60, 110);
/// Node fill — available, ambient.
pub const TECH_NODE_AVAILABLE: egui::Color32 = egui::Color32::from_rgb(70, 60, 15);
/// Node fill — available, in path.
pub const TECH_NODE_AVAILABLE_PATH: egui::Color32 = egui::Color32::from_rgb(90, 75, 15);
/// Node fill — locked, ambient.
pub const TECH_NODE_LOCKED: egui::Color32 = egui::Color32::from_rgb(45, 45, 50);
/// Node fill — locked, in path.
pub const TECH_NODE_LOCKED_PATH: egui::Color32 = egui::Color32::from_rgb(60, 60, 60);
/// Node text — unlocked (light green for visibility on dark green).
pub const TECH_TEXT_UNLOCKED: egui::Color32 = egui::Color32::from_rgb(180, 255, 180);
/// Node text — available (warm cream on amber background).
pub const TECH_TEXT_AVAILABLE: egui::Color32 = egui::Color32::from_rgb(255, 240, 180);

/// Compute the tech-tree node fill colour from a (in_path, unlocked,
/// researching, can_research) tuple — replaces the duplicated match arms
/// in `tech_tree.rs` that previously had 8 hardcoded `Color32::from_rgb`.
#[allow(clippy::too_many_arguments)]
pub fn tech_node_color(
    in_path: bool,
    unlocked: bool,
    researching: bool,
    can_research: bool,
) -> egui::Color32 {
    match (in_path, unlocked, researching, can_research) {
        (true, true, _, _) => TECH_NODE_UNLOCKED_PATH,
        (true, _, true, _) => TECH_NODE_RESEARCHING_PATH,
        (true, _, _, true) => TECH_NODE_AVAILABLE_PATH,
        (true, _, _, _) => TECH_NODE_LOCKED_PATH,
        (false, true, _, _) => TECH_NODE_UNLOCKED,
        (false, _, true, _) => TECH_NODE_RESEARCHING,
        (false, _, _, true) => TECH_NODE_AVAILABLE,
        (false, _, _, _) => TECH_NODE_LOCKED,
    }
}

// ─── Tech-Tree Prerequisite Edge Colours ────────────────────────────────
//
// Drawn by `tech_tree.rs` as the connector-line colour between a tech
// node and its prerequisites. The in-path variant is opaque (255) so
// the planned-research line pops; the unlocked/default variants are
// faint (80/60 alpha) so non-path edges recede into the background.

/// Prereq edge — both ends of the connector are in the player's research path.
pub const PREREQ_IN_PATH: egui::Color32 = egui::Color32::from_rgba_premultiplied(255, 200, 0, 255);
/// Prereq edge — the prerequisite is already unlocked (and at least one
/// end is not in path). Pale green to signal "this gate is met".
pub const PREREQ_UNLOCKED: egui::Color32 =
    egui::Color32::from_rgba_premultiplied(100, 255, 100, 80);
/// Prereq edge — default state. Dim grey so the line is visible without
/// drawing attention.
pub const PREREQ_DEFAULT: egui::Color32 = egui::Color32::from_rgba_premultiplied(120, 120, 120, 60);

// ─── Resources Bar Metric Colours ───────────────────────────────────────
//
// History-panel series accents. The Kardashev / PowerProduced values
// delegate to existing tokens; the rest are bespoke to keep distinct
// metrics visually distinct in the bar chart legend.

/// Population metric — pale green.
pub const RB_POPULATION: egui::Color32 = egui::Color32::from_rgb(116, 224, 170);
/// Colonies metric — warm gold.
pub const RB_COLONIES: egui::Color32 = egui::Color32::from_rgb(236, 197, 96);
/// Ships metric — pale blue.
pub const RB_SHIPS: egui::Color32 = egui::Color32::from_rgb(120, 178, 255);
/// Survey coverage / surveyed bodies — pale teal.
pub const RB_SURVEY: egui::Color32 = egui::Color32::from_rgb(121, 235, 210);
/// Housing bar full-fill — pale blue used when the housing capacity bar
/// is below 85% (RED/AMBER handle the over-budget states).
pub const RB_HOUSING: egui::Color32 = egui::Color32::from_rgb(100, 180, 255);

// ─── Misc Helpers ───────────────────────────────────────────────────────

/// Return `c` with its alpha channel replaced by `a` (channels left
/// unmultiplied). The audit script flags `Color32::from_rgba_unmultiplied`
/// even when the source colour is a runtime value, so dynamic-alpha call
/// sites should reach for this helper instead of inlining the constructor.
pub fn with_alpha(c: egui::Color32, a: u8) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

/// Push a colour towards `RED` by adding R and clamping G/B downwards.
/// Used by the porkchop plot (and any future "out-of-budget" tint) to
/// flag a colour as un-affordable without replacing it outright — the
/// player still sees the underlying colormap band, just shifted red.
///
/// Lives in `theme.rs` (the audit allowlist) because the audit script
/// flags every `Color32::from_rgba_unmultiplied` call site, even when
/// the components are runtime-computed.
pub fn red_tint(c: egui::Color32) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(
        c.r().saturating_add(60),
        c.g().saturating_sub(40).max(40),
        c.b().saturating_sub(40).max(40),
        c.a(),
    )
}

/// Build an `egui::Color32` from an `(r, g, b, a)` tuple.  The audit
/// flags every `Color32::from_rgba_unmultiplied` call site outside
/// `theme.rs`, so any data→colour translation (e.g. the porkchop
/// colormap stops in `porkchop_config.ron`) funnels through here.
pub fn color32_from_rgba(rgba: (u8, u8, u8, u8)) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(rgba.0, rgba.1, rgba.2, rgba.3)
}

/// Linearly interpolate between two RGBA tuples and return the result as
/// an `egui::Color32`.  Used by the porkchop colormap sampler (and any
/// future RON-driven colormap) to convert `(ΔV km/s, RGBA)` stop pairs
/// into a continuous surface.
pub fn lerp_rgba(a: (u8, u8, u8, u8), b: (u8, u8, u8, u8), t: f32) -> egui::Color32 {
    let lerp = |x: u8, y: u8| -> u8 {
        let v = x as f32 + (y as f32 - x as f32) * t;
        v.round().clamp(0.0, 255.0) as u8
    };
    egui::Color32::from_rgba_unmultiplied(
        lerp(a.0, b.0),
        lerp(a.1, b.1),
        lerp(a.2, b.2),
        lerp(a.3, b.3),
    )
}

/// Compute the blink-pulsed fill colour for the dashboard's pause button.
/// `blink` is a 0.0..=1.0 alpha value driven by the time-controls animation.
pub fn pause_button_fill(blink: f32) -> egui::Color32 {
    let r = (13.0 + 100.0 * blink) as u8;
    let g = (17.0f32 * (1.0 - blink * 0.8)) as u8;
    let b = (23.0f32 * (1.0 - blink * 0.9)) as u8;
    egui::Color32::from_rgb(r, g, b)
}

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

// ─── Text Builders ───────────────────────────────────────────────────────
//
// Standard text builders used across all panels so label / value / caption
// rendering stays consistent. Every panel that draws a label, a value, or a
// caption should reach for these instead of re-rolling the same
// `RichText::new(text).font(mono(N)).color(TEXT_DIM)` chain.
//
//   * `label(text)`        — small uppercase stat-row label (mono 10pt, dim)
//   * `value(text)`        — stat-row value (mono 12pt, bright)
//   * `caption(text)`      — explanatory hint under a value (body 10pt, hint)
//   * `kbd_shortcut_label(text)` — keycap-style chip for a F-key / hotkey
//     label in a tooltip. Bold mono in the accent colour so the key
//     stands out from the surrounding explanatory text.

/// Small uppercase stat-row label (e.g. `DISTANCE`, `MASS`).
pub fn label(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).font(mono(10.0)).color(TEXT_DIM)
}

/// Stat-row value, brighter than the label so the eye lands on it.
pub fn value(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).font(mono(12.0)).color(TEXT_VALUE)
}

/// Explanatory caption / hint under a value. Proportional font, very dim so
/// it doesn't compete with the data it annotates.
pub fn caption(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).font(body(10.0)).color(TEXT_HINT)
}

/// Keycap-style chip for a hotkey label (`F1`, `Shift+F12`, `1`, `Esc`…).
/// Bold mono in the accent colour so the key reads as a discrete affordance
/// inside a tooltip or near a button.
pub fn kbd_shortcut_label(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text)
        .font(mono(10.0))
        .color(ACCENT)
        .strong()
}

// ─── Common Widgets ──────────────────────────────────────────────────────

/// Standard dark panel frame used by side panels.
pub fn panel_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(BG)
        .stroke(egui::Stroke::new(1.0, BORDER))
        .inner_margin(egui::Margin::same(Spacing::md as i8))
}

/// Frame for central panels (fully opaque).
pub fn central_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(BG_SOLID)
        .stroke(egui::Stroke::new(1.0, BORDER))
        .inner_margin(egui::Margin::same(Spacing::sm as i8))
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
        .inner_margin(egui::Margin::same(Spacing::sm as i8))
        .corner_radius(3.0)
}

/// Frame for tooltip popups.
pub fn tooltip_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(egui::Color32::from_rgba_unmultiplied(12, 16, 28, 245))
        .stroke(egui::Stroke::new(1.5, ACCENT_DIM))
        .inner_margin(egui::Margin::same(Spacing::md as i8))
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
///
/// Must be called inside an `egui::Grid` (the `end_row()` advances to the
/// next row). Labels and values use the standard `theme::label` /
/// `theme::value` builders so every panel renders the same typographic
/// scale.
pub fn stat_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.label(egui::RichText::new(label).font(mono(10.0)).color(TEXT_DIM));
    ui.label(
        egui::RichText::new(value)
            .font(mono(12.0))
            .color(TEXT_VALUE),
    );
    ui.end_row();
}

/// Like [`stat_row`] but the label cell shows a hover tooltip. The dossier
/// uses this to expand acronym-style stat names (`DISTANCE`, `GRAVITY`)
/// without crowding the value column.
pub fn stat_row_with_tooltip(ui: &mut egui::Ui, label: &str, value: &str, tooltip: &str) {
    let label_response = ui.label(egui::RichText::new(label).font(mono(10.0)).color(TEXT_DIM));
    label_response.on_hover_text(tooltip);
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

// ─── Ship Module Category Colours ────────────────────────────────────────

/// Accent colour for a ship-module category, used for slot borders and other
/// heavy UI chrome where the visual emphasis must read at a glance.
pub fn module_slot_accent_color(category: ShipModuleCategory) -> bevy::prelude::Color {
    match category {
        ShipModuleCategory::FlightSystems => bevy::prelude::Color::srgb(1.0, 0.62, 0.28),
        ShipModuleCategory::Bridges
        | ShipModuleCategory::PowerThermal
        | ShipModuleCategory::Sensors
        | ShipModuleCategory::UtilitySupport
        | ShipModuleCategory::Maintenance
        | ShipModuleCategory::SpecialScience => bevy::prelude::Color::srgb(0.26, 0.86, 1.0),
        ShipModuleCategory::Weapons
        | ShipModuleCategory::FireControl
        | ShipModuleCategory::ArmorDefense
        | ShipModuleCategory::Magazines
        | ShipModuleCategory::PointDefense
        | ShipModuleCategory::Armor
        | ShipModuleCategory::ElectronicWarfare => bevy::prelude::Color::srgb(1.0, 0.34, 0.34),
        ShipModuleCategory::FuelStorage | ShipModuleCategory::CargoStorage => {
            bevy::prelude::Color::srgb(0.56, 0.92, 0.66)
        }
        ShipModuleCategory::CrewSystems
        | ShipModuleCategory::Habitats
        | ShipModuleCategory::Medical => bevy::prelude::Color::srgb(0.9, 0.84, 0.58),
        ShipModuleCategory::ConstructionISRU | ShipModuleCategory::Construction => {
            bevy::prelude::Color::srgb(0.96, 0.72, 0.38)
        }
    }
}

/// Detail / chip colour for a ship-module category — used for category
/// buttons, badges, and other small UI marks where the slot-accent palette
/// would be too saturated.
pub fn module_category_color(category: ShipModuleCategory) -> bevy::prelude::Color {
    match category {
        ShipModuleCategory::FlightSystems => bevy::prelude::Color::srgb(0.35, 0.88, 1.0),
        ShipModuleCategory::Bridges => bevy::prelude::Color::srgb(0.56, 0.82, 1.0),
        ShipModuleCategory::PowerThermal => bevy::prelude::Color::srgb(1.0, 0.76, 0.28),
        ShipModuleCategory::FuelStorage | ShipModuleCategory::CargoStorage => {
            bevy::prelude::Color::srgb(0.45, 0.85, 0.66)
        }
        ShipModuleCategory::Weapons => bevy::prelude::Color::srgb(1.0, 0.46, 0.35),
        ShipModuleCategory::FireControl => bevy::prelude::Color::srgb(0.8, 0.7, 1.0),
        ShipModuleCategory::Sensors => bevy::prelude::Color::srgb(0.5, 0.92, 0.9),
        ShipModuleCategory::Magazines => bevy::prelude::Color::srgb(0.94, 0.82, 0.58),
        ShipModuleCategory::PointDefense => bevy::prelude::Color::srgb(1.0, 0.68, 0.54),
        ShipModuleCategory::Armor | ShipModuleCategory::ArmorDefense => {
            bevy::prelude::Color::srgb(0.86, 0.92, 1.0)
        }
        ShipModuleCategory::CrewSystems
        | ShipModuleCategory::Habitats
        | ShipModuleCategory::Medical => bevy::prelude::Color::srgb(0.85, 0.82, 0.64),
        ShipModuleCategory::UtilitySupport => bevy::prelude::Color::srgb(0.62, 0.85, 1.0),
        ShipModuleCategory::Maintenance => bevy::prelude::Color::srgb(0.72, 0.84, 0.94),
        ShipModuleCategory::ConstructionISRU | ShipModuleCategory::Construction => {
            bevy::prelude::Color::srgb(0.9, 0.72, 0.45)
        }
        ShipModuleCategory::ElectronicWarfare => bevy::prelude::Color::srgb(1.0, 0.62, 0.78),
        ShipModuleCategory::SpecialScience => bevy::prelude::Color::srgb(0.6, 1.0, 0.8),
    }
}

// ─── Tech Category Colours ──────────────────────────────────────────────

/// Get the colour for a technology research category — drives the tech
/// tree's per-category borders, labels, and category band backgrounds.
/// Centralised here so panels beyond the tech tree can pick up the same
/// category tint.
pub fn tech_category_color(category: TechCategory) -> egui::Color32 {
    match category {
        TechCategory::Electronics => egui::Color32::from_rgb(100, 150, 255),
        TechCategory::Propulsion => egui::Color32::from_rgb(255, 150, 50),
        TechCategory::Energy => egui::Color32::from_rgb(255, 255, 50),
        TechCategory::Physics => egui::Color32::from_rgb(150, 100, 255),
        TechCategory::Military => egui::Color32::from_rgb(255, 50, 50),
        TechCategory::Weapons => egui::Color32::from_rgb(200, 50, 50),
        TechCategory::DefensiveSystems => egui::Color32::from_rgb(50, 150, 255),
        TechCategory::Materials => egui::Color32::from_rgb(150, 150, 50),
        TechCategory::Construction => egui::Color32::from_rgb(200, 150, 100),
        TechCategory::Biology => egui::Color32::from_rgb(50, 255, 150),
        TechCategory::Sensors => egui::Color32::from_rgb(100, 255, 255),
        TechCategory::SpaceTechnology => egui::Color32::from_rgb(150, 200, 255),
        TechCategory::Sociology => egui::Color32::from_rgb(255, 150, 200),
        TechCategory::LifeSupport => GREEN,
        TechCategory::Industry => egui::Color32::from_rgb(180, 180, 50),
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

// ─── Bevy `Color` Mirror ─────────────────────────────────────────────────
//
// Bevy UI 0.18 (used by the Shipbuilding workspace — see `docs/UI.md` §1)
// needs `bevy::prelude::Color`, not `egui::Color32`. Most panels use egui
// and the egui constants above, but the three-pane shipbuilding shell (and
// the audit in `scripts/audit_bevy_color_literals.py`) need a parallel
// namespace with matching values. PR-B adds the mirror; PR-D wires
// `populate_tab_strip` through it.
//
// Values are the sRGB counterparts of the matching `Color32` constants
// above (divided by 255). Where the egui constant uses alpha, the Bevy
// mirror uses `.set_a(...)` on the same RGB; the named `*_A` constants
// carry the alpha explicitly for callers that need it.

/// Bevy `Color` mirror of the egui tokens above. Most panels stay on
/// egui's `Color32`; the Shipbuilding workspace (and any future Bevy UI
/// shell) reads from this namespace so the literal `Color::srgb(...)`
/// drift that triggered the GRA-54 regression can't recur.
#[allow(non_snake_case, non_upper_case_globals)]
pub mod Color {
    use bevy::prelude::Color;

    // ── Surfaces ────────────────────────────────────────────────
    /// Deep navy opaque — mirrors `BG_SOLID`.
    pub const BG_SOLID: Color = Color::srgb(0.031, 0.051, 0.102);
    /// Panel surface — mirrors `SURFACE`.
    pub const SURFACE: Color = Color::srgb(0.051, 0.067, 0.090);
    /// Raised widget surface — mirrors `SURFACE_RAISED`.
    pub const SURFACE_RAISED: Color = Color::srgb(0.078, 0.102, 0.141);
    /// Bright input tint — mirrors `SURFACE_INPUT`.
    pub const SURFACE_INPUT: Color = Color::srgb(0.063, 0.078, 0.118);

    // ── Accent / Border ─────────────────────────────────────────
    /// Primary cyan — mirrors `ACCENT`.
    pub const ACCENT: Color = Color::srgb(0.0, 0.949, 1.0);
    /// Faint accent border — mirrors `BORDER`.
    pub const BORDER: Color = Color::srgb(0.0, 0.949, 1.0);

    // ── Text ────────────────────────────────────────────────────
    /// Primary foreground — mirrors `TEXT`.
    pub const TEXT: Color = Color::srgb(0.824, 0.863, 0.922);
    /// Brighter value foreground — mirrors `TEXT_VALUE`.
    pub const TEXT_VALUE: Color = Color::srgb(0.784, 0.843, 0.941);

    // ─── Tab Strip (Pattern 3 / Pattern 4) ──────────────────────
    //
    // Values preserve the visual identity chosen before the GRA-54
    // theme consolidation. PR-D migrates the `populate_tab_strip` call
    // site to read from these constants.

    /// Active tab background (selected).
    pub const TAB_ACTIVE_BG: Color = Color::srgb(0.1, 0.28, 0.34);
    /// Active tab border (selected).
    pub const TAB_ACTIVE_BORDER: Color = Color::srgb(0.0, 0.95, 1.0);
    /// Inactive tab background.
    pub const TAB_INACTIVE_BG: Color = Color::srgb(0.045, 0.07, 0.1);
    /// Inactive tab border.
    pub const TAB_INACTIVE_BORDER: Color = Color::srgb(0.18, 0.3, 0.36);
    /// Active tab text.
    pub const TAB_ACTIVE_TEXT: Color = Color::srgb(0.9, 0.98, 1.0);
    /// Inactive tab text.
    pub const TAB_INACTIVE_TEXT: Color = Color::srgb(0.78, 0.86, 0.9);

    // ── Selected-button background (used by time-scale presets, etc.)
    /// Mirrors `BUTTON_ACTIVE_BG`.
    pub const BUTTON_ACTIVE_BG: Color = Color::srgb(0.0, 0.216, 0.275);

    // ── Panel Titles (Bevy `section_h1_bevy` only) ─────────────────
    //
    // The 3-pane shell of the Shipbuilding workspace uses a slightly
    // desaturated cyan-blue (`(0.55, 0.95, 1.0)`) for its pane titles
    // (Logistics Hub / Design Blueprint / Engineering Analytics) so
    // they read as a distinct category from the bright cyan used for
    // interactive accents. Picked during the WorkspaceShell refactor
    // in GRA-70 — previously an inline `Color::srgb(0.55, 0.95, 1.0)`
    // call; promoted here so the audit + Kilo CR can verify the
    // visual identity is preserved.

    /// Desaturated cyan-blue used by `section_h1_bevy` for pane
    /// titles. Distinct from `ACCENT` so headings don't compete with
    /// interactive elements.
    pub const PANEL_TITLE: Color = Color::srgb(0.55, 0.95, 1.0);

    // ── Status Surfaces (info / success / warning / danger) ─────────
    //
    // Triples for the four canonical status surfaces used by the
    // shipbuilding workspace chips, the building state pills, and
    // the ship state indicators. Each status has a dim background,
    // a saturated border, and a text colour that matches the
    // border. Pre-GRA-93 these were 12+ inline `Color::srgb` literals
    // scattered through `shipbuilding_workspace.rs`; the
    // migration to these tokens is the GRA-93 follow-up to the
    // GRA-67 Bevy `Color` mirror.

    /// Info-status background (cool blue-black, ~3% lightness).
    pub const STATUS_INFO_BG: Color = Color::srgb(0.02, 0.04, 0.07);
    /// Info-status border (mid-cyan, brighter than `BORDER`).
    pub const STATUS_INFO_BORDER: Color = Color::srgb(0.22, 0.72, 0.86);
    /// Info-status text (light cyan, mirrors `PANEL_TITLE`).
    pub const STATUS_INFO_TEXT: Color = Color::srgb(0.55, 0.95, 1.0);

    /// Success-status background (dim green-black).
    pub const STATUS_SUCCESS_BG: Color = Color::srgb(0.08, 0.18, 0.14);
    /// Success-status border (bright spring green).
    pub const STATUS_SUCCESS_BORDER: Color = Color::srgb(0.38, 0.94, 0.7);
    /// Success-status text (mint green).
    pub const STATUS_SUCCESS_TEXT: Color = Color::srgb(0.5, 0.92, 0.62);

    /// Warning-status background (dim amber-black).
    pub const STATUS_WARNING_BG: Color = Color::srgb(0.18, 0.12, 0.08);
    /// Warning-status border (saturated orange).
    pub const STATUS_WARNING_BORDER: Color = Color::srgb(0.78, 0.52, 0.36);
    /// Warning-status text (warm gold).
    pub const STATUS_WARNING_TEXT: Color = Color::srgb(0.98, 0.78, 0.36);

    /// Danger-status background (dim red-black).
    pub const STATUS_DANGER_BG: Color = Color::srgb(0.12, 0.08, 0.09);
    /// Danger-status border (saturated coral red).
    pub const STATUS_DANGER_BORDER: Color = Color::srgb(0.86, 0.42, 0.38);
    /// Danger-status text (coral red).
    pub const STATUS_DANGER_TEXT: Color = Color::srgb(1.0, 0.45, 0.4);

    // ── Orbit & Trajectory Colours ───────────────────────────────────
    //
    // Used by `OrbitPath` rendering (Bevy) and the historical-probe
    // spawn in `src/fleets/historical_probes.rs`.  Picked during the
    // GRA-131 (historical probes) PR so the probe orbit-ring tint is
    // distinct from the planet orbit (PLANET_ORBIT) and the comet
    // (yellow) paths.

    /// Orbit ring drawn for the four historical probes (V1/V2/Parker/NH).
    /// Light cyan at 55% alpha so the heliocentric background reads
    /// through the partial-hyperbola path.
    pub const PROBE_ORBIT: Color = Color::srgba(0.55, 0.85, 1.00, 0.55);

    // ── Tooltips & Overlays ─────────────────────────────────────────
    //
    // Mirrors the egui `TOOLTIP_BG` / `TOOLTIP_BG_ALT` set above. Used
    // by the shipbuilding hover tooltip, the analytics tooltip, and
    // the WorkspaceShell banner. The alpha 0.96 lets the starfield
    // backdrop show through faintly.

    /// Tooltip background — semi-transparent deep navy.
    pub const TOOLTIP_BG: Color = Color::srgba(0.02, 0.04, 0.07, 0.96);
    /// Tooltip border — mid-cyan accent.
    pub const TOOLTIP_BORDER: Color = Color::srgb(0.22, 0.72, 0.86);
    /// Tooltip title text — light cyan, mirrors `PANEL_TITLE`.
    pub const TOOLTIP_TITLE: Color = Color::srgb(0.55, 0.95, 1.0);

    // ── Panel Containers (workspace shells) ─────────────────────────
    //
    // The `spawn_panel` helper and the WorkspaceShell status banner
    // use these three pairs. The 0.92 alpha lets the backdrop show
    // through subtly so the panels feel "screen-glassy" rather than
    // opaque boxes.

    /// Generic panel background — semi-transparent dark navy.
    pub const PANEL_BG: Color = Color::srgba(0.03, 0.05, 0.08, 0.92);
    /// Generic panel border — slightly dimmer cyan than the tooltip
    /// border, so panels read as "less interactive" than tooltips.
    pub const PANEL_BORDER: Color = Color::srgb(0.15, 0.78, 0.88);
    /// Panel title text — light cyan, mirrors `PANEL_TITLE`.
    pub const PANEL_TITLE_TEXT: Color = Color::srgb(0.55, 0.95, 1.0);

    // ── Workspace Loading State (WorkspaceShell) ───────────────────
    //
    // The "Initializing shipbuilding workspace..." banner uses these
    // three values. Distinct from the panel pair above because the
    // banner is rendered as a sub-element of the shell root, not as
    // a freestanding panel.

    /// Loading-state background for the WorkspaceShell root.
    pub const LOADING_BG: Color = Color::srgba(0.015, 0.02, 0.035, 0.96);
    /// Loading-state banner background.
    pub const LOADING_BANNER_BG: Color = Color::srgba(0.03, 0.08, 0.12, 0.96);
    /// Loading-state banner border — same cyan as the tooltip border.
    pub const LOADING_BANNER_BORDER: Color = Color::srgb(0.22, 0.72, 0.86);
    /// Loading-state banner text — pale cyan.
    pub const LOADING_BANNER_TEXT: Color = Color::srgb(0.82, 0.94, 0.98);

    // ── Blueprint Canvas ────────────────────────────────────────────
    //
    // The blueprint canvas is a sub-element of the Blueprint panel
    // with its own (slightly darker) background. The border matches
    // the panel border so the canvas reads as "nested inside panel".

    /// Blueprint canvas background — slightly darker than `PANEL_BG`.
    pub const BLUEPRINT_CANVAS_BG: Color = Color::srgba(0.018, 0.032, 0.055, 0.94);
    /// Blueprint canvas border — matches `PANEL_BORDER`.
    pub const BLUEPRINT_CANVAS_BORDER: Color = Color::srgb(0.15, 0.78, 0.88);

    // ── Stat Chip Text Colours ──────────────────────────────────────
    //
    // The chip-row text colours that `spawn_analytics_chip_row`
    // consumes. Each chip has a (label, value, delta, accent) tuple
    // and the accent is one of these values. Pre-GRA-93 the values
    // were inline literals in every chip row call; the migration
    // routes them through these named constants so the
    // `audit_bevy_color_literals` baseline can shrink to ≈0.

    /// Chip text — body grey (used for Material Cost body lines, etc.).
    pub const CHIP_TEXT_BODY: Color = Color::srgb(0.82, 0.87, 0.9);
    /// Chip text — light text (used for "Save Design" / "Reset Hull"
    /// button labels and other action-button text).
    pub const CHIP_TEXT_LIGHT: Color = Color::srgb(0.92, 0.96, 0.98);
    /// Chip text — pure cyan accent.
    pub const CHIP_TEXT_CYAN: Color = Color::srgb(0.0, 0.95, 1.0);
    /// Chip text — green (positive deltas).
    pub const CHIP_TEXT_GREEN: Color = Color::srgb(0.5, 0.92, 0.58);
    /// Chip text — orange (load / build stats).
    pub const CHIP_TEXT_ORANGE: Color = Color::srgb(0.86, 0.82, 0.58);
    /// Chip text — pale blue (DOCK stat).
    pub const CHIP_TEXT_DOCK: Color = Color::srgb(0.82, 0.9, 0.96);
    /// Chip text — power-load (warm orange).
    pub const CHIP_TEXT_POWER_LOAD: Color = Color::srgb(1.0, 0.64, 0.24);
    /// Chip text — ordnance (coral).
    pub const CHIP_TEXT_ORDNANCE: Color = Color::srgb(1.0, 0.46, 0.35);
    /// Chip text — ISRU (golden brown).
    pub const CHIP_TEXT_ISRU: Color = Color::srgb(0.9, 0.72, 0.45);
    /// Chip text — build (mint).
    pub const CHIP_TEXT_BUILD: Color = Color::srgb(0.5, 0.92, 0.9);
    /// Chip text — launch (cream).
    pub const CHIP_TEXT_LAUNCH: Color = Color::srgb(0.86, 0.82, 0.58);
    /// Chip text — warning (coral red, used for "Missing required
    /// slots" lines).
    pub const CHIP_TEXT_WARN: Color = Color::srgb(1.0, 0.55, 0.45);

    // ── Building State Pills (4 background colours) ────────────────
    //
    // The 4-state building status indicator used in the shipbuilding
    // workspace and the dossier. Pre-GRA-93 these were inline
    // literals; the migration to these tokens is part of the GRA-93
    // follow-up.

    /// Building state — Operational (dim green-black).
    pub const BUILDING_OPERATIONAL_BG: Color = Color::srgb(0.1, 0.32, 0.22);
    /// Building state — Construction (dim cyan-blue).
    pub const BUILDING_CONSTRUCTION_BG: Color = Color::srgb(0.1, 0.18, 0.24);
    /// Building state — Degraded (dim amber).
    pub const BUILDING_DEGRADED_BG: Color = Color::srgb(0.18, 0.12, 0.08);
    /// Building state — Offline (dim red).
    pub const BUILDING_OFFLINE_BG: Color = Color::srgb(0.14, 0.08, 0.09);

    // ── Mining Efficiency Bands (4 band colours) ───────────────────
    //
    // The 4-band mine-efficiency display used by the analytics panel.
    // Pre-GRA-93 these were inline literals; the migration to these
    // tokens is part of the GRA-93 follow-up.

    /// Mining band — High (≥75% yield, bright green).
    pub const MINE_BAND_HIGH: Color = Color::srgb(0.5, 0.92, 0.58);
    /// Mining band — Mid (50–75% yield, amber).
    pub const MINE_BAND_MID: Color = Color::srgb(0.98, 0.78, 0.36);
    /// Mining band — Low (25–50% yield, coral).
    pub const MINE_BAND_LOW: Color = Color::srgb(0.86, 0.42, 0.38);
    /// Mining band — None (≤25% yield, dim grey).
    pub const MINE_BAND_NONE: Color = Color::srgb(0.22, 0.35, 0.42);

    // ── Soft Status Variants (danger / warning softer alternatives) ─
    //
    // The Save/Reset/Clear button rows (lines ~1371-1429) and the
    // "no modules match" empty filter row (lines ~1461-1486) use
    // softer danger and warning pairs than the canonical
    // `STATUS_DANGER_*` / `STATUS_WARNING_*` triples. These are the
    // four pairs in that softer family.

    /// Soft danger-status background (warmer than `STATUS_DANGER_BG`).
    pub const STATUS_DANGER_BG_SOFT: Color = Color::srgb(0.13, 0.08, 0.09);
    /// Soft danger-status border (dim coral, not the saturated
    /// `STATUS_DANGER_BORDER`).
    pub const STATUS_DANGER_BORDER_SOFT: Color = Color::srgb(0.58, 0.3, 0.32);
    /// Soft danger-status text (pale coral pink).
    pub const STATUS_DANGER_TEXT_SOFT: Color = Color::srgb(0.94, 0.84, 0.84);

    /// Soft warning-status border (dim amber, not the saturated
    /// `STATUS_WARNING_BORDER`).
    pub const STATUS_WARNING_BORDER_SOFT: Color = Color::srgb(0.62, 0.42, 0.28);
    /// Soft warning-status text (warm peach).
    pub const STATUS_WARNING_TEXT_SOFT: Color = Color::srgb(0.92, 0.78, 0.62);
    /// Soft warning-status background (with alpha — used for the
    /// "no modules match" empty filter row).
    pub const STATUS_WARNING_BG_SOFT_ALPHA: Color = Color::srgba(0.12, 0.1, 0.07, 0.92);
    /// Soft warning text — cream (used by the "Clear" button inside
    /// the soft-warning empty-filter row).
    pub const STATUS_WARNING_TEXT_CREAM: Color = Color::srgb(0.96, 0.86, 0.74);

    // ── Bright status accents (chip / pill outlines, etc.) ───────
    //
    // The chip-row palette and the analytics tooltip use saturated
    // accents that don't quite fit the softer "status" family. These
    // are the unique per-chip values seen in
    // `shipbuilding_workspace.rs` (and a few in the engineering-status
    // helper at L4872-4920).

    /// Bright coral red — stronger than `STATUS_DANGER_TEXT`; used
    /// for the "Locked by prerequisite technology" / "engineering
    /// definition missing" labels and the engineering status
    /// "Negative" tone.
    pub const STATUS_BRIGHT_CORAL: Color = Color::srgb(1.0, 0.42, 0.38);
    /// Bright orange-red — used for power-load chip text and the
    /// launch-tower power-load chip variant.
    pub const STATUS_ORANGE_RED: Color = Color::srgb(1.0, 0.6, 0.32);
    /// Bright orange — used for the "Power Load" chip colour.
    pub const STATUS_BRIGHT_ORANGE: Color = Color::srgb(0.96, 0.54, 0.28);
    /// Gold / saturated yellow — used for the EngineeringStatus
    /// "in progress" tone.
    pub const STATUS_GOLD: Color = Color::srgb(0.86, 0.78, 0.34);
    /// Bright green — used for engineering "complete" tone and the
    /// analytics throughput chip.
    pub const STATUS_BRIGHT_GREEN: Color = Color::srgb(0.66, 0.96, 0.7);
    /// Soft green — used for the delta-suffix on chip rows.
    pub const STATUS_SOFT_GREEN: Color = Color::srgb(0.68, 0.9, 0.76);
    /// Green-cyan — used for the "shield" / "mitigation" chip
    /// accent and the dossier_secondary chip rows.
    pub const STATUS_GREEN_CYAN: Color = Color::srgb(0.45, 0.85, 0.66);

    // ── Cyan accent shades (chip outlines, focused-row borders) ─
    //
    // The chip outlines and the blueprint panel focused-row
    // borders use a small palette of cyan-leaning values. These
    // constants cover the unique ones in `shipbuilding_workspace.rs`
    // that aren't already covered by the existing `ACCENT` /
    // `STATUS_INFO_BORDER` / `TAB_ACTIVE_BORDER` triples.

    /// Bright cyan — used for the Blueprint panel "selected" border
    /// and the focused-row outline.
    pub const ACCENT_BRIGHT: Color = Color::srgb(0.2, 0.92, 0.98);
    /// Bright cyan (slightly dimmer than `ACCENT_BRIGHT`).
    pub const ACCENT_BRIGHT_2: Color = Color::srgb(0.34, 0.86, 0.94);
    /// Bright cyan (mid-bright, used for focused-row mix).
    pub const ACCENT_BRIGHT_3: Color = Color::srgb(0.36, 0.88, 0.98);
    /// Bright cyan (lighter, used for blueprint-mix base).
    pub const ACCENT_BRIGHT_4: Color = Color::srgb(0.4, 0.9, 1.0);
    /// Bright blue — used for the focused-row mix in the Blueprint
    /// panel and the dossier-tonal accents.
    pub const ACCENT_BLUE: Color = Color::srgb(0.46, 0.78, 1.0);
    /// Bright blue (slightly lighter than `ACCENT_BLUE`).
    pub const ACCENT_BLUE_2: Color = Color::srgb(0.5, 0.86, 1.0);
    /// Pale cyan — used for the slot card "outlined" accent.
    pub const ACCENT_CYAN: Color = Color::srgb(0.52, 0.8, 0.88);
    /// Dim cyan border (used for disabled / unfocused tab borders).
    pub const ACCENT_CYAN_DIM: Color = Color::srgb(0.22, 0.45, 0.54);
    /// Dim cyan border (slightly lighter than `ACCENT_CYAN_DIM`).
    pub const ACCENT_CYAN_DIM_2: Color = Color::srgb(0.22, 0.48, 0.58);
    /// Mid cyan — used for the focused-row mix target.
    pub const ACCENT_CYAN_MID: Color = Color::srgb(0.28, 0.72, 0.82);

    // ── Body / text / label shades ───────────────────────────────
    //
    // The shipbuilding workspace uses a small palette of
    // blueish-grey / cyan-grey / warm-grey / peach text shades that
    // don't fit the "primary" / "dim" / "hint" egui-mirror
    // `TEXT` / `TEXT_VALUE` / `TEXT_DIM` triple. These constants
    // cover the unique ones in `shipbuilding_workspace.rs`.

    /// Body text — bright white (used for the workspace filter
    /// "type to filter" hint when a query is active).
    pub const TEXT_BRIGHT_WHITE: Color = Color::srgb(0.95, 0.98, 1.0);
    /// Body text — bluish white (used for the secondary
    /// panel text).
    pub const TEXT_BLUISH_WHITE: Color = Color::srgb(0.86, 0.93, 0.98);
    /// Body text — pale cyan-grey (used as the slot/card label
    /// text).
    pub const TEXT_PALE: Color = Color::srgb(0.88, 0.93, 0.96);
    /// Body text — light grey (used for descriptions, secondary
    /// "section" labels, and the queue text block).
    pub const TEXT_LIGHT: Color = Color::srgb(0.84, 0.9, 0.94);
    /// Body text — medium grey (used for "module description"
    /// lines and the "no hull selected" hint).
    pub const TEXT_MEDIUM: Color = Color::srgb(0.6, 0.7, 0.76);
    /// Body text — dim grey (used for the empty-filter-row
    /// placeholder when no query is typed).
    pub const TEXT_DIM: Color = Color::srgb(0.5, 0.6, 0.66);
    /// Body text — muted grey (used for the engineering "no
    /// project" status line).
    pub const TEXT_MUTED: Color = Color::srgb(0.66, 0.75, 0.8);
    /// Body text — very light grey (used for the "cell sub-label").
    pub const TEXT_VERY_LIGHT: Color = Color::srgb(0.78, 0.84, 0.88);
    /// Body text — medium-dim grey (used for the rating cell).
    pub const TEXT_MEDIUM_DIM: Color = Color::srgb(0.56, 0.58, 0.62);
    /// Body text — very dim grey (used for the
    /// "shadow / inset" surface).
    pub const TEXT_VERY_DIM: Color = Color::srgb(0.28, 0.28, 0.3);

    // ── Workspace Surface Variants (slot / module / panel) ─────
    //
    // The slot / module card / queue row / empty-filter row
    // patterns use a small palette of "deep navy" surface
    // backgrounds. The selection / unselected / previewed / dim
    // states cycle through these values. The numeric suffix is the
    // rough-lightness ordering — `SURFACE_SLOT` is the
    // "selected slot" highlight, lower numbers are darker.

    /// Slot surface — selected / highlighted (matches
    /// `BUILDING_CONSTRUCTION_BG`).
    pub const SURFACE_SLOT: Color = Color::srgb(0.1, 0.18, 0.24);
    /// Slot surface — base (used for unselected slot / fleet /
    /// design row backgrounds).
    pub const SURFACE_SLOT_BASE: Color = Color::srgb(0.05, 0.08, 0.12);
    /// Module card surface — base (slightly lighter than
    /// `SURFACE_SLOT_BASE` for the module card row).
    pub const SURFACE_MODULE_CARD_BASE: Color = Color::srgb(0.055, 0.08, 0.12);
    /// Module card surface — previewed (highlight when a card is
    /// being previewed but not yet installed).
    pub const SURFACE_MODULE_CARD_PREVIEW: Color = Color::srgb(0.12, 0.22, 0.34);
    /// Surface — dim cyan variant (used for the active
    /// "Loaded" surface row).
    pub const SURFACE_DIM_CYAN: Color = Color::srgb(0.06, 0.2, 0.24);
    /// Surface — dim cyan variant 2.
    pub const SURFACE_DIM_CYAN_2: Color = Color::srgb(0.08, 0.2, 0.24);
    /// Surface — deep navy (slightly different from
    /// `SURFACE_RAISED`).
    pub const SURFACE_DEEP: Color = Color::srgb(0.06, 0.11, 0.16);
    /// Surface — almost-black (used for the
    /// "cell separator" / "very dim" background).
    pub const SURFACE_BLACK: Color = Color::srgb(0.08, 0.08, 0.09);
    /// Surface — slate (used for the "module availability" panel).
    pub const SURFACE_SLATE: Color = Color::srgb(0.08, 0.13, 0.19);
    /// Surface — soft green (used for the "Saved!" status
    /// confirmation surface).
    pub const SURFACE_SOFT_GREEN: Color = Color::srgb(0.08, 0.2, 0.14);
    /// Surface — dim cyan mid (used for the "Loaded" surface row).
    pub const SURFACE_DIM_CYAN_3: Color = Color::srgb(0.1, 0.18, 0.22);
    /// Surface — dim cyan mid 2.
    pub const SURFACE_DIM_CYAN_4: Color = Color::srgb(0.1, 0.18, 0.28);
    /// Surface — neutral grey (used for the
    /// "no project" placeholder).
    pub const SURFACE_NEUTRAL: Color = Color::srgb(0.14, 0.14, 0.16);
    /// Surface — deep navy variant 1.
    pub const SURFACE_DEEP_2: Color = Color::srgb(0.04, 0.1, 0.14);
    /// Surface — deep navy variant 2 (used for the blueprint
    /// canvas-outer surface).
    pub const SURFACE_DEEP_3: Color = Color::srgb(0.045, 0.12, 0.15);
    /// Surface — deep navy variant 3 (used for the
    /// "subtle" inline background).
    pub const SURFACE_DEEP_4: Color = Color::srgb(0.03, 0.08, 0.11);

    // ── Alpha overlay surfaces (sub-elements inside panels) ────
    //
    // The blueprint canvas, panel-internal sub-panels, and
    // surface-alpha highlights use these. The alpha is
    // preserved in the constant so callers can read the
    // translucency directly from the name.

    /// Blueprint canvas background — slightly dimmer than
    /// `BLUEPRINT_CANVAS_BG`.
    pub const BLUEPRINT_CANVAS_BG_DIM: Color = Color::srgba(0.018, 0.032, 0.05, 0.94);
    /// Panel background — solid (matches `PANEL_BG` with 0.98
    /// alpha for the "fully opaque" feel).
    pub const PANEL_BG_SOLID: Color = Color::srgba(0.03, 0.05, 0.08, 0.98);
    /// Surface alpha (used for the dossier "in-line" surface
    /// background).
    pub const SURFACE_ALPHA: Color = Color::srgba(0.04, 0.07, 0.11, 0.92);
    /// Mid cyan alpha (used for the "section sub-heading" border).
    pub const ACCENT_ALPHA_MID: Color = Color::srgba(0.16, 0.34, 0.4, 0.45);
    /// Faint accent alpha (used for the "subtle" background tint).
    pub const ACCENT_ALPHA_FAINT: Color = Color::srgba(0.18, 0.55, 0.64, 0.12);
    /// Faint accent alpha (slightly stronger than
    /// `ACCENT_ALPHA_FAINT`).
    pub const ACCENT_ALPHA_FAINT_2: Color = Color::srgba(0.18, 0.55, 0.64, 0.18);
    /// Green alpha (used for the analytics throughput chip
    /// background).
    pub const GREEN_ALPHA: Color = Color::srgba(0.45, 0.92, 0.56, 0.55);
    /// Pale cyan alpha (used for the "cell sub-label"
    /// background).
    pub const PALE_CYAN_ALPHA: Color = Color::srgba(0.7, 0.85, 0.92, 0.32);
    /// Coral red alpha (used for the "Locked!" chip background).
    pub const CORAL_RED_ALPHA: Color = Color::srgba(1.0, 0.38, 0.3, 0.55);
}

// ─── Section Headers (Pattern 2 + Pattern 4) ────────────────────────────
//
// Replaces ad-hoc `RichText::new("...").font(heading_font()).color(ACCENT)`
// / `color(TEXT_DIM)` calls scattered across the dossier and other panels.
// The three sizes form a small typographic scale: h1 ≈ panel title (already
// covered by `title()`), h2 ≈ section header, h3 ≈ sub-section header.

/// H2 — section header. Uppercase 13pt semi-bold in `ACCENT`. Mirrors the
/// `STAR PROPERTIES` / `HABITABILITY` / `ATMOSPHERE` pattern in
/// `dossier_panel.rs`. Adds 8px trailing space; the caller is expected
/// to provide the leading space (typically via `section_divider` for
/// Pattern 2 ledgers, or via the panel's inner margin for Pattern 4
/// strip contents).
pub fn section_h2(ui: &mut egui::Ui, label: impl Into<String>) {
    ui.label(
        egui::RichText::new(label.into())
            .font(heading())
            .color(ACCENT),
    );
    ui.add_space(Spacing::sm);
}

/// H3 — sub-section header. Uppercase 10pt mono in `TEXT_DIM` (the
/// dimmer sibling of `section_h2` for nested sections). Adds 4px
/// trailing space; the caller provides the leading space.
pub fn section_h3(ui: &mut egui::Ui, label: impl Into<String>) {
    ui.label(
        egui::RichText::new(label.into())
            .font(mono(10.0))
            .color(TEXT_DIM)
            .strong(),
    );
    ui.add_space(Spacing::xs);
}

/// H1 — panel title. Slightly larger and more padded than `title()`
/// (which is just the font). Use this at the top of a Pattern 2 ledger
/// or a full-screen modal; subsequent sections should use `section_h2`.
/// Color is `ACCENT` to match the dossier's "body name" treatment — the
/// only place this primitive is used in PR-B.
pub fn section_h1(ui: &mut egui::Ui, label: impl Into<String>) {
    ui.add_space(Spacing::sm);
    ui.label(
        egui::RichText::new(label.into())
            .font(title())
            .color(ACCENT),
    );
    ui.add_space(Spacing::md);
}

// ─── Sub-tab Strip (Pattern 4, egui) ─────────────────────────────────────
//
// Generic tab strip used by every in-panel sub-tab UI. The strip is a
// horizontal `ui.horizontal(...)` block of buttons, not a separate
// `TopBottomPanel`. The active tab is rendered with `ACCENT` text + a
// 2px bottom underline; inactive tabs use `TEXT` with no underline.
//
// Generic over the `Tab` trait (`src/ui/tab.rs`) so a panel's `*Tab`
// enum implements `Tab` once and is rendered uniformly everywhere.
// `on_select` is invoked with the chosen tab when its button is
// clicked; the caller is responsible for mutating its own state
// (e.g. `ui_state.selected_tab = tab`). The return value is the tab
// that should be treated as active *for the rest of this frame* —
// useful when the caller wants the click to take effect immediately.

/// Render a horizontal sub-tab strip. The active tab's label is in
/// `ACCENT` with a 2px bottom underline; inactive tabs in `TEXT` with
/// no underline.
///
/// `tabs` is the panel's full list (rendered in order), `active` is
/// the currently selected tab, and `on_select` is called with the
/// clicked tab. The return value is `active` (the caller can ignore
/// it; it exists so panels that want the new selection to take
/// effect within the same frame can `let next = tab_strip(...);
/// ui_state.selected_tab = next;`).
pub fn tab_strip<T: crate::ui::tab::Tab>(
    ui: &mut egui::Ui,
    tabs: &[T],
    active: T,
    mut on_select: impl FnMut(T),
) -> T {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = Spacing::md;
        for tab in tabs {
            let is_active = *tab == active;
            let label = match tab.icon() {
                Some(icon) => format!("{icon} {}", tab.label()),
                None => tab.label().into_owned(),
            };
            let text = egui::RichText::new(label)
                .font(mono(11.0))
                .color(if is_active { ACCENT } else { TEXT });
            let response = ui.add(
                egui::Button::new(text)
                    .frame(false)
                    .sense(egui::Sense::click()),
            );
            if response.clicked() {
                on_select(*tab);
            }
            // 2px bottom underline for the active tab — matches the
            // top-menu bar's accent treatment (Pattern 1) at a
            // smaller font size.
            if is_active {
                let rect = response.rect;
                let underline_y = rect.bottom() - 1.0;
                ui.painter().hline(
                    rect.left()..=rect.right(),
                    underline_y,
                    egui::Stroke::new(2.0, ACCENT),
                );
            }
        }
    });
    active
}

// ─── Ledger Panel (Pattern 2, egui) ──────────────────────────────────────
//
// Wraps `panel_frame()` + a `CollapsingHeader` so a Pattern 2 ledger
// can express a "this section is collapsible" affordance without
// re-rolling the same paint code at every call site. Title is
// rendered with `section_h2`; contents run inside a `CollapsingHeader`
// that defaults to open on first render so the dossier's existing
// always-open sections (Resources, Colony, …) don't disappear.
//
// The generic `T` is reserved for future typed callbacks (e.g. a
// `Filter` token) so the signature can grow without churn; today it
// only carries the `id` used by `CollapsingHeader::default_open` and
// the section's egui `Id` salt.

/// Render a collapsible ledger section. The title is drawn in the
/// `section_h2` style; the body is a `CollapsingHeader` that defaults
/// to open. `contents` is called with the `egui::Ui` for the body
/// whenever the section is open.
///
/// `id` should be a stable per-section identifier so egui remembers
/// the open/closed state across frames. The generic `T` is reserved
/// for future typed tokens; pass `&()` if you don't need a filter.
pub fn ledger_panel<T>(
    ui: &mut egui::Ui,
    id: &str,
    title: impl Into<String>,
    _token: &T,
    contents: impl FnOnce(&mut egui::Ui),
) {
    section_h2(ui, title);
    let header = egui::CollapsingHeader::new("")
        .id_salt(id)
        .default_open(true)
        .show_unindented(ui, |ui| {
            contents(ui);
        });
    let _ = header; // collapse-artifact; nothing to do with the response
}

// ─── Section Header (Pattern 3 mirror, Bevy UI 0.18) ─────────────────────
//
// Bevy UI 0.18 sibling of the egui `section_h1` above. Spawns a
// `Text` child of `parent_entity` styled as the canonical "pane
// title" — the 15-pt size the shipbuilding sub-panes (Logistics Hub /
// Design Blueprint / Engineering Analytics) have used since the
// pattern was introduced, paired with `Color::ACCENT` so the title
// matches the rest of the design system rather than carrying its
// own ad-hoc cyan.
//
// The 15-pt size is intentionally smaller than the egui
// `section_h1` (which uses the 20-pt `title()` font) because the
// Pattern 3 panes sit side-by-side in a 250-/Auto/270-px column
// layout where 20-pt labels would crowd the chrome. A 20-pt
// `section_h1` mirror can be added in a follow-up if a Bevy ledger
// panel ever needs the full panel-title treatment.
//
// Spacing around the title is the caller's responsibility: the
// parent `Node`'s `row_gap` and outer `padding` already produce the
// `Spacing::sm` / `Spacing::md` rhythm that the egui variant adds
// inline. Callers that need extra top/bottom air can spawn a
// spacer `Node` before or after this call.

/// Render a Bevy UI pane-title header. Spawns a `Text` child of
/// `parent_entity` with the 15-pt body font and the desaturated
/// `Color::PANEL_TITLE`.
///
/// Mirrors the egui `section_h1` semantic (canonical "this section
/// is the panel title" treatment) for Bevy UI 0.18 — used by the
/// shipbuilding sub-pane headers via `spawn_panel`. The colour is
/// intentionally distinct from `Color::ACCENT` so a heading reads
/// as a label and not as an interactive element.
pub fn section_h1_bevy(commands: &mut Commands, parent_entity: Entity, label: impl Into<String>) {
    commands.entity(parent_entity).with_children(|parent| {
        parent.spawn((
            Text::new(label.into()),
            TextFont {
                font_size: 15.0,
                ..default()
            },
            TextColor(Color::PANEL_TITLE),
        ));
    });
}

// ─── Sub-tab Strip (Pattern 3 mirror, Bevy UI 0.18) ──────────────────────
//
// Bevy UI 0.18 sibling of the egui `tab_strip` above. Spawns a
// `Button` per tab as a child of `tabs_root` and applies the same
// `theme::Color::*` tokens (`TAB_ACTIVE_BG` / `TAB_INACTIVE_BG` /
// `TAB_ACTIVE_BORDER` / `TAB_INACTIVE_BORDER` / `TAB_ACTIVE_TEXT` /
// `TAB_INACTIVE_TEXT`). Per-tab width and per-tab extras (e.g. the
// `ShipbuildingWorkspaceTabButton { tab }` marker) are out of scope
// for the primitive — call sites that need a marker component
// should re-implement the loop locally, or wrap the primitive and
// add the marker after spawning.
//
// PR-B ships the primitive + the mirror; PR-D migrates
// `populate_tab_strip` to read from `theme::Color`.

/// Render a horizontal Bevy UI sub-tab strip as children of
/// `tabs_root`. Each tab is a `Button` with the standard
/// active/inactive palette from `theme::Color::*`. `width` and
/// `min_height` are per-tab sizing; the caller can post-process the
/// spawned entities (e.g. add a workspace-specific marker
/// component) by `entity.get()`-ing the spawned children after
/// the call returns — but the typical flow is to call this once
/// per tab-strip refresh from a system that owns the workspace
/// state.
pub fn tab_strip_bevy<T: crate::ui::tab::Tab>(
    commands: &mut Commands,
    tabs_root: Entity,
    tabs: &[T],
    active: T,
) {
    commands.entity(tabs_root).with_children(|parent| {
        for tab in tabs {
            let selected = *tab == active;
            let label = match tab.icon() {
                Some(icon) => format!("{icon} {}", tab.label()),
                None => tab.label().into_owned(),
            };
            parent.spawn((
                Button,
                Node {
                    min_width: Val::Px(136.0),
                    min_height: Val::Px(30.0),
                    padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(if selected {
                    Color::TAB_ACTIVE_BG
                } else {
                    Color::TAB_INACTIVE_BG
                }),
                BorderColor::all(if selected {
                    Color::TAB_ACTIVE_BORDER
                } else {
                    Color::TAB_INACTIVE_BORDER
                }),
                Text::new(label),
                TextFont {
                    font_size: 11.0,
                    ..default()
                },
                TextColor(if selected {
                    Color::TAB_ACTIVE_TEXT
                } else {
                    Color::TAB_INACTIVE_TEXT
                }),
            ));
        }
    });
}
