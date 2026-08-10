//! `bevy_ui` primitives for the Construction canary (Phase C, `rework-ui-design`).
//!
//! These primitives are purely visual — every helper here returns a bundle
//! that can be `commands.spawn(...)`'d directly. None of the existing egui
//! theme is touched; this module coexists with `crate::ui::theme` so the
//! rest of the game stays in egui until the canary is verified.
//!
//! Why a separate file: the existing `src/ui/theme.rs` is 1700+ lines of
//! egui code, and the new bevy_ui primitives use different types
//! (`Node`/`Text`/`TextFont` vs `egui::Ui`), so keeping them in one file
//! would force the file to bridge two render systems. A separate module
//! keeps the blast radius small and the diff reviewable.

use bevy::prelude::*;

// ─── Palette ──────────────────────────────────────────────────────────────

/// Body background — near-black deep navy.
pub const BODY_BG: Color = Color::srgba(0.008, 0.039, 0.094, 1.0);
/// Card background — translucent navy.
///
/// v0.5.3 (2026-08-10): lifted from `(0.031, 0.086, 0.172)` to
/// `(0.045, 0.110, 0.210)` so card surfaces separate from the darker
/// panel chrome (the construction backdrop is `(0.012, 0.024, 0.047)`)
/// — the previous value was so close to the panel that cards read
/// flat even with a drop shadow.
///
/// v0.5.3.5 (2026-08-10): neumorphism redesign. Per
/// `refactoringui.com/previews/building-your-color-palette` a
/// card-vs-panel delta of *three or more shades* is the threshold
/// below which the lift reads as flat on dark surfaces. The previous
/// `(0.045, 0.110, 0.210)` was only ~1 shade brighter than the panel,
/// so the directional border + outer cyan glow (the original
/// "3D effect") never read. Bumped to `(0.078, 0.165, 0.290)` —
/// ~3 shades brighter on a 0–1 luma scale while staying in the same
/// navy hue. `CARD_BG_HOVER` is bumped one more shade to keep the
/// "hovered = brighter" cue relative to the new resting fill.
pub const CARD_BG: Color = Color::srgba(0.078, 0.165, 0.290, 0.92);
/// Hovered card background — slightly brighter, also translucent.
/// v0.5.3.5: bumped from `(0.063, 0.118, 0.227)` (one shade above
/// the old `CARD_BG`) to `(0.105, 0.210, 0.345)` so the hover
/// contrast stays one shade above the new resting fill.
pub const CARD_BG_HOVER: Color = Color::srgba(0.105, 0.210, 0.345, 0.78);
/// Hairline / divider — dim cyan-navy.
pub const HAIRLINE: Color = Color::srgba(0.086, 0.188, 0.306, 1.0);
/// Cyan accent — primary UI accent for titles, CTAs, active states.
pub const CYAN: Color = Color::srgba(0.373, 0.784, 0.847, 1.0);
/// Cyan accent at 18% alpha — for the card outer border (subtle, not bright).
pub const CARD_BORDER: Color = Color::srgba(0.373, 0.784, 0.847, 0.18);
/// Card top + left edge highlight — full-cyan at 90% alpha. Used by
/// `spawn_card`'s per-edge `BorderColor` to give the bevel a
/// directional reading (light from top-left). v0.5.3.5 neumorphism
/// redesign.
pub const CARD_BORDER_HIGHLIGHT: Color = Color::srgba(0.498, 0.804, 0.847, 0.90);
/// Card bottom + right edge shadow — dim cyan at 30% alpha. The
/// complementary darker half of the bevel; combined with
/// [`CARD_BORDER_HIGHLIGHT`] it gives a 3D bevel reading without
/// requiring overlay children. v0.5.3.5 neumorphism redesign.
pub const CARD_BORDER_SHADOW: Color = Color::srgba(0.373, 0.784, 0.847, 0.30);

/// Cyan accent at 50% alpha — for chip frames (clearly visible).
pub const CYAN_BORDER: Color = Color::srgba(0.373, 0.784, 0.847, 0.50);
/// Cyan accent at 60% alpha — for the CTA border (more visible than the chip border).
pub const CYAN_BORDER_STRONG: Color = Color::srgba(0.373, 0.784, 0.847, 0.60);
/// Cyan accent at 65% alpha — for the inner top-edge highlight (the "glass rim").
pub const CYAN_RIM: Color = Color::srgba(0.373, 0.784, 0.847, 0.65);
/// Dim text — subtitles, captions, effect bullets.
pub const TEXT_DIM: Color = Color::srgba(0.498, 0.580, 0.659, 1.0);
/// Brighter text — body content.
pub const TEXT_BODY: Color = Color::srgba(0.831, 0.890, 0.937, 1.0);
/// Resource-state ore (orange) — StatsGrid tone.
pub const ORANGE_ORE: Color = Color::srgba(0.941, 0.627, 0.439, 1.0);
/// Resource-state green (finished product).
pub const GREEN_FIN: Color = Color::srgba(0.373, 0.784, 0.471, 1.0);
/// Warning / depletion / over-capacity tone (v0.5.2 PR-A.2).
/// Used by the Mining tab for "no deposit" and "over-grid"
/// indications, and as a fallback for any future food-deficit
/// / efficiency-under-50% reads. Matches the legacy egui
/// theme's `RED` (`src/ui/theme.rs`).
pub const RED: Color = Color::srgba(0.847, 0.373, 0.392, 1.0);
/// Bright gold-yellow for the energy icon + chip chrome (the
/// bolt-in-hex power chip on the construction menu cards). The
/// chip background is this colour at 12% alpha, the border at
/// 35%, and the icon at full saturation. v0.5.2 PR-A.7
/// (2026-08-04): yellow was chosen over cyan so the power chip
/// reads as a distinct third category (consumption vs
/// production, both yellow-chromed) while the text colour
/// inside the chip still differentiates the two with red/green
/// per the +/- convention. Matches the legacy `theme::GOLD`
/// (255, 215, 0) in 0..=1 space.
pub const YELLOW_ENERGY: Color = Color::srgba(1.0, 0.843, 0.0, 1.0);
/// Filled cyan CTA background — translucent so glass bleeds through.
pub const CTA_FILL: Color = Color::srgba(0.094, 0.298, 0.353, 0.85);
/// Filled cyan CTA hover background. v0.5.2: bumped from
/// `(0.137, 0.392, 0.471, 0.90)` (a subtle 0.04 RGB delta over the
/// resting fill that players read as "no hover") to a fully-opaque,
/// much brighter cyan that matches the active-chip background —
/// unmistakable at a glance.
pub const CTA_FILL_HOVER: Color = Color::srgba(0.275, 0.620, 0.706, 1.0);
/// Active chip background — solid bright cyan so the active state
/// reads as "selected" rather than "ghosted". Translucent enough to
/// keep the glass feel (78% alpha).
pub const ACTIVE_CHIP_BG: Color = Color::srgba(0.196, 0.529, 0.612, 0.78);
/// Inactive chip background - transparent so the container border shows through.
/// The active overlay resets inactive chips to this so spawn-time active state
/// doesn't persist after the marker is removed.
pub const INACTIVE_CHIP_BG: Color = Color::srgba(0.0, 0.0, 0.0, 0.0);
/// Active chip text color — bright white on the bright cyan background,
/// for high contrast and a "selected" reading.
pub const ACTIVE_CHIP_TEXT: Color = Color::WHITE;
/// Green — used for "Empty Queue" / success indicators.
pub const GREEN_OK: Color = Color::srgba(0.373, 0.784, 0.471, 1.0);
/// Yellow — used for ETA / time-remaining indicators.
pub const YELLOW_ETA: Color = Color::srgba(0.957, 0.749, 0.349, 1.0);

// ─── Resource category palette ──────────────────────────────────────────
//
// Per-category tints used by the build-card resource-demand rows
// (v0.5.2 PR-A.4 follow-up). Each category gets a distinct hue so
// the player can tell at a glance whether a cost is a Construction
// metal, a Volatile, a Precious metal, etc. The palette is tuned for
// the dark-navy `CARD_BG` background and is bright enough to read at
// 11 px (the canary's caption size). The cost-line icon PNG is tinted
// to the same color so icon + amount text share one hue per row.
//
// Resource category names come from `ResourceType::category()` in
// `src/economy/types.rs`. Keep these names in lockstep with that
// function; the helper `category_color()` below falls back to
// `TEXT_BODY` for an unknown category.

/// Biological (Food) — green-leaf.
pub const RESOURCE_BIOLOGICAL: Color = Color::srgba(0.498, 0.812, 0.467, 1.0);
/// Volatiles (Water, Hydrogen, Ammonia, Methane, Phosphorus) — cool
/// cyan-teal.
pub const RESOURCE_VOLATILES: Color = Color::srgba(0.310, 0.776, 0.831, 1.0);
/// Atmospheric Gases (Nitrogen, Oxygen, CO₂, Argon) — pale sky blue.
pub const RESOURCE_ATMOSPHERIC: Color = Color::srgba(0.561, 0.776, 0.949, 1.0);
/// Construction Materials (Iron, Aluminum, Titanium, …) — warm copper.
pub const RESOURCE_CONSTRUCTION: Color = Color::srgba(0.890, 0.580, 0.380, 1.0);
/// Fusion Fuel (He-3, Deuterium, Tritium) — bright sun-yellow.
pub const RESOURCE_FUSION: Color = Color::srgba(0.957, 0.812, 0.320, 1.0);
/// Fissiles (Uranium, Thorium, Plutonium) — radioactive lime.
pub const RESOURCE_FISSILE: Color = Color::srgba(0.682, 0.890, 0.388, 1.0);
/// Precious Metals (Gold, Silver, Platinum) — warm gold.
pub const RESOURCE_PRECIOUS: Color = Color::srgba(0.957, 0.749, 0.349, 1.0);
/// Strategic Materials (Copper, REE, Lithium, …) — violet.
pub const RESOURCE_STRATEGIC: Color = Color::srgba(0.776, 0.620, 0.949, 1.0);
/// Exotic (Antimatter, ExoticMatter, Metamaterials, Computronium) —
/// magenta-pink (stark, reads as "rare/special").
pub const RESOURCE_EXOTIC: Color = Color::srgba(0.949, 0.498, 0.776, 1.0);

/// Returns the `Color` associated with a resource category string.
/// Mirrors `ResourceType::category()` from `src/economy/types.rs`; the
/// fallback (`TEXT_BODY`) is intentional — the helper must never
/// panic on an unknown label so a future category addition doesn't
/// silently break card rendering.
pub fn category_color(category: &str) -> Color {
    match category {
        "Biological" => RESOURCE_BIOLOGICAL,
        "Volatiles" => RESOURCE_VOLATILES,
        "Atmospheric Gases" => RESOURCE_ATMOSPHERIC,
        "Construction" => RESOURCE_CONSTRUCTION,
        "Fusion Fuel" => RESOURCE_FUSION,
        "Fissiles" => RESOURCE_FISSILE,
        "Precious Metals" => RESOURCE_PRECIOUS,
        "Strategic" => RESOURCE_STRATEGIC,
        "Exotic" => RESOURCE_EXOTIC,
        _ => TEXT_BODY,
    }
}

/// Returns the `Color` associated with a `ResourceType`'s category.
/// Convenience wrapper around `category_color(&resource.category())`
/// so the build-card renderer doesn't have to import
/// `ResourceType` just to look up the tint.
pub fn category_color_for_resource(resource: &crate::economy::ResourceType) -> Color {
    category_color(resource.category())
}

// ─── Spacing ──────────────────────────────────────────────────────────────

/// 4-px-based grid (matches `theme::Spacing` names).
pub const SPACE_XS: f32 = 4.0;
pub const SPACE_SM: f32 = 8.0;
pub const SPACE_MD: f32 = 10.0;
pub const SPACE_LG: f32 = 12.0;
pub const SPACE_XL: f32 = 16.0;

/// Vertical footprint reserved at the bottom of construction cards for
/// the absolutely-positioned Queue CTA. Equals the CTA's own height
/// (`Val::Px(32.0)`) plus the CTA's `bottom: SPACE_LG` offset and one
/// extra `SPACE_LG` of breathing room between the ETA row and the CTA.
/// The card uses this as its `padding-bottom` so the flex content
/// (subtitle, stats, hairline, effects, ETA) never extends into the
/// CTA's absolute bounding box.
pub const CTA_FOOTPRINT: f32 = 56.0;

// ─── Layout Sizes ─────────────────────────────────────────────────────────

/// Height of the tab strip (Overview / Buildings / Build / Stockpiles).
pub const TAB_STRIP_H: f32 = 44.0;
/// Font size for the sub-tab row (Overview / Buildings / Build /
/// Stockpiles). Bigger than the chip default (16) so the top-level
/// section selector reads as the dominant control in the panel.
pub const TAB_FONT_SIZE: f32 = 18.0;

/// Height of a single chip (tab / build qty / filter / category). 24 px
/// matches the prototype's compact chip style.
pub const CHIP_H: f32 = 20.0;

// ─── Type Sizes ───────────────────────────────────────────────────────────

pub const TITLE_SIZE: f32 = 28.0;
pub const SUBTITLE_SIZE: f32 = 14.0;
pub const SECTION_SIZE: f32 = 16.0;
pub const BODY_SIZE: f32 = 14.0;
pub const CAPTION_SIZE: f32 = 12.0;

// ─── Hairline Divider Bundle ──────────────────────────────────────────────

#[derive(Bundle, Clone)]
pub struct HairlineBundle {
    pub node: Node,
    pub bg: BackgroundColor,
    pub name: Name,
}

impl Default for HairlineBundle {
    fn default() -> Self {
        Self {
            node: Node {
                height: Val::Px(1.0),
                ..default()
            },
            bg: BackgroundColor(HAIRLINE),
            name: Name::new("hairline"),
        }
    }
}

// ─── Text Helpers ──────────────────────────────────────────────────────────

// ── Duration Formatter ─────────────────────────────────────

/// Format a `Duration` (in seconds) as a compact "Xd Yh Zm" string.
/// Drops leading zero units (e.g. "0y 6d 2h" becomes "6d 2h").
pub fn format_duration_compact(total_seconds: f64) -> String {
    if total_seconds <= 0.0 {
        return "0s".to_string();
    }
    let mut s = total_seconds;
    let years = (s / (365.25 * 24.0 * 3600.0)) as u32;
    s -= years as f64 * 365.25 * 24.0 * 3600.0;
    let days = (s / (24.0 * 3600.0)) as u32;
    s -= days as f64 * 24.0 * 3600.0;
    let hours = (s / 3600.0) as u32;
    s -= hours as f64 * 3600.0;
    let minutes = (s / 60.0) as u32;
    s -= minutes as f64 * 60.0;
    let seconds = s as u32;

    let mut parts: Vec<String> = Vec::new();
    if years > 0 {
        parts.push(format!("{}y", years));
    }
    if days > 0 {
        parts.push(format!("{}d", days));
    }
    if hours > 0 {
        parts.push(format!("{}h", hours));
    }
    if minutes > 0 {
        parts.push(format!("{}m", minutes));
    }
    if seconds > 0 || parts.is_empty() {
        parts.push(format!("{}s", seconds));
    }
    parts.join(" ")
}

/// Format a `Duration` (in seconds) as a **fixed-width** zero-padded
/// "Dd HHh MMm SSs" string. The output string always has the same
/// length regardless of the value, so layout-driven chips / text
/// columns don't shift width as the timer ticks down.
///
/// Examples:
///   format_duration_padded(0)               -> "00d 00h 00m 00s"
///   format_duration_padded(45)              -> "00d 00h 00m 45s"
///   format_duration_padded(3661)            -> "00d 01h 01m 01s"
///   format_duration_padded(7 * 24 * 3600)   -> "07d 00h 00m 00s"
///   format_duration_padded(80 * 24 * 3600)  -> "80d 00h 00m 00s"
///
/// Negative or non-finite inputs are clamped to 0.
///
/// The "d" field is the only unbounded portion (the player can have
/// arbitrarily long queue ETAs at low output rates). Everything else
/// fits in 2 digits.
pub fn format_duration_padded(total_seconds: f64) -> String {
    if !total_seconds.is_finite() || total_seconds <= 0.0 {
        return "00d 00h 00m 00s".to_string();
    }
    let mut s = total_seconds;
    let days = (s / (24.0 * 3600.0)) as u32;
    s -= days as f64 * 24.0 * 3600.0;
    let hours = (s / 3600.0) as u32;
    s -= hours as f64 * 3600.0;
    let minutes = (s / 60.0) as u32;
    s -= minutes as f64 * 60.0;
    let seconds = s as u32;

    format!("{:02}d {:02}h {:02}m {:02}s", days, hours, minutes, seconds)
}

// ─── Chip Button ────────────────────────────────────────────────────────────

/// Bundle for a chip-styled button (tabs, build qty, filter, category chips).
///
/// The chip has:
/// - **Compact height** (CHIP_H = 22 px) — matches the prototype's chip style.
/// - **3 px corner radius** — softer than the cards, consistent with the prototype.
/// - **Active state** has a brighter translucent background + full-alpha cyan
///   border (visually "pressed" or "selected").
/// - **Inactive state** has a transparent background + dim border (visually
///   "outlined", a quiet chip).
/// - **Built-in Button** so picking hover/press is auto-wired (same as the
///   Queue CTA).
/// - **Active chips** get a `BoxShadow` glow (cyan, 6 px blur, 0 offset)
///   for a "lit" look that stands out from inactive chips.
///
/// The runtime hover system mutates the colors. Always pair it with a
/// system that mutates BackgroundColor + BorderColor + TextColor based
/// on `Interaction` (see `tick_chip_button_hover` below).
#[derive(Bundle, Clone)]
pub struct ChipButtonBundle {
    pub button: Button,
    pub node: Node,
    pub bg: BackgroundColor,
    pub border: BorderColor,
    pub name: Name,
    /// `Pickable` makes the chip clickable via the picking backend.
    /// Without this, hover still works (via the focus system) but
    /// `Interaction::Pressed` never fires.
    pub pickable: Pickable,
}

impl ChipButtonBundle {
    /// Create a new chip button. `is_active` selects between the active and
    /// inactive visual states. The label is set via [`spawn_chip_text`]
    /// after the bundle is spawned.
    pub fn new(label: &str, is_active: bool) -> Self {
        Self::new_with_border(label, is_active, None)
    }

    /// Like [`new`](Self::new) but with an optional border override.
    pub fn new_with_border(label: &str, is_active: bool, border_override: Option<UiRect>) -> Self {
        Self {
            button: Button,
            node: Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                height: Val::Px(CHIP_H),
                // Horizontal-only padding. The chip's height (CHIP_H = 20)
                // exactly matches the text's line height (16 px * 1.2 ≈ 19.2),
                // so the text node (also 20 px tall) fills the chip
                // exactly with no top/bottom padding — the text appears
                // visually centered because the line box and the chip
                // box are the same height.
                padding: UiRect::horizontal(Val::Px(SPACE_MD)),
                // Chips inside a container are borderless; the container
                // provides the visual frame. Tabs that need a custom
                // border (e.g. active tab underline) pass `border_override`.
                border: border_override.unwrap_or(UiRect::default()),
                border_radius: BorderRadius::all(Val::Px(3.0)),
                ..default()
            },
            bg: BackgroundColor(if is_active {
                ACTIVE_CHIP_BG
            } else {
                Color::srgba(0.0, 0.0, 0.0, 0.0)
            }),
            // Chips have no individual border; the container provides it.
            border: BorderColor::all(Color::NONE),
            name: Name::new(format!("chip_{}", label)),
            pickable: Pickable::default(),
        }
    }
}

/// Marker component for the **child text node** spawned by
/// [`spawn_chip_text`]. The chip button itself does NOT carry the `Text`;
/// instead, the text is a separate child entity that fills the button via
/// `flex_grow: 1.0` + `width: 100%`. This is the standard bevy_ui 0.18
/// pattern for centering text inside a flex container — putting `Text`
/// directly on a `Node` does NOT participate in the parent's flex layout,
/// so the text would sit at its natural (left) position regardless of the
/// parent's `justify_content`. The child node solves this.
#[derive(Component)]
pub struct ChipTextNode {
    pub is_active: bool,
}

/// Spawn the text child of a chip button. Call this **after** the
/// `ChipButtonBundle` is spawned and before the bundle is added to a
/// parent. Returns the child text entity.
///
/// The child has:
/// - `flex_grow: 1.0` so it fills the parent's width
/// - `width: Val::Percent(100.0)` as a fallback for parents that don't
///   propagate flex_grow to text children
/// - `justify_content: Center` + `align_items: Center` so the text is
///   centered both horizontally and vertically
pub fn spawn_chip_text(
    commands: &mut Commands,
    parent: Entity,
    label: &str,
    body_font: Handle<Font>,
    is_active: bool,
    font_size: f32,
) -> Entity {
    let text_entity = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                // flex_grow: 1.0 + explicit width/height makes the text child
                // fill the button exactly. Val::Percent(100.0) for height
                // doesn't always resolve correctly inside a flex row parent;
                // using Val::Px(CHIP_H) is unambiguous. The parent's
                // horizontal padding is preserved (the child fills the
                // content box, not the border box).
                flex_grow: 1.0,
                flex_shrink: 1.0,
                width: Val::Percent(100.0),
                height: Val::Px(CHIP_H),
                ..default()
            },
            Text::new(label.to_string()),
            TextFont {
                font: body_font,
                // Font size is a parameter so callers can override it for
                // larger sub-tab labels (TAB_FONT_SIZE = 18) while keeping
                // the default 16 for the smaller filter/category rows.
                font_size,
                ..default()
            },
            // bevy 0.18 has no TextLayout::line_height. The text's vertical
            // position is controlled by the flex layout (align_items +
            // height on the text child node). Setting height = CHIP_H and
            // align_items = Center on the text child centers the line box
            // vertically, and the chip body's vertical padding (SPACE_XS)
            // gives the glyphs breathing room so they read as centered.
            TextLayout {
                justify: Justify::Center,
                ..default()
            },
            // Active chips use bright white text on the cyan fill.
            TextColor(if is_active { ACTIVE_CHIP_TEXT } else { CYAN }),
            ChipTextNode { is_active },
        ))
        .id();
    commands.entity(parent).add_child(text_entity);
    text_entity
}

/// Hover / press effect system for chip buttons.
///
/// Bundle for a horizontal row of chip buttons wrapped in a single
/// bordered container (Build qty, Filter, Category rows in the
/// Construction panel).
///
/// The container has:
/// - **Compact height** = CHIP_H (22 px) + a few px padding for visual
///   breathing room. Real height is set by the caller to fit the row.
/// - **1 px cyan border** at 50% alpha — clearly visible frame.
/// - **4 px corner radius** — softer than the cards.
/// - **3 px row gap** between chips.
/// - **Translucent backdrop** (very dark navy, ~80% alpha) so the chip
///   row is visually grouped as a single UI control against the panel.
///
/// Chips inside the container are borderless (their own `Node.border` is
/// `UiRect::default()`); the container provides the visual frame.
#[derive(Bundle, Clone)]
pub struct ChipRowContainerBundle {
    pub node: Node,
    pub bg: BackgroundColor,
    pub border: BorderColor,
    pub name: Name,
}

impl ChipRowContainerBundle {
    /// Create a new chip-row container. The caller sets `height` explicitly
    /// (it varies between rows — build qty is taller than filter).
    ///
    /// The container is **content-bounded** (no fixed width) — it shrinks
    /// to fit the chips inside. The root container's `align_items: Start`
    /// is what makes the row hug the left edge of the panel. The vertical
    /// spacing between rows comes from the root's `row_gap`.
    pub fn new(row_name: &str, height: f32) -> Self {
        Self {
            node: Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                // Content-bounded width (Val::Auto). The container hugs
                // the chips inside; the root's `align_items: Start` keeps
                // it on the left edge of the panel.
                width: Val::Auto,
                height: Val::Px(height),
                padding: UiRect::all(Val::Px(SPACE_XS)),
                column_gap: Val::Px(SPACE_XS),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            bg: BackgroundColor(Color::srgba(0.012, 0.039, 0.078, 0.40)),
            border: BorderColor::all(CYAN_BORDER),
            name: Name::new(format!("chip_row_{}", row_name)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_duration_padded_zero_and_negative() {
        assert_eq!(format_duration_padded(0.0), "00d 00h 00m 00s");
        assert_eq!(format_duration_padded(-1.0), "00d 00h 00m 00s");
        assert_eq!(format_duration_padded(f64::NAN), "00d 00h 00m 00s");
    }

    #[test]
    fn format_duration_padded_sub_minute() {
        // 45 seconds.
        assert_eq!(format_duration_padded(45.0), "00d 00h 00m 45s");
    }

    #[test]
    fn format_duration_padded_one_minute_one_second() {
        // 61 seconds.
        assert_eq!(format_duration_padded(61.0), "00d 00h 01m 01s");
    }

    #[test]
    fn format_duration_padded_one_hour() {
        // 3600 seconds.
        assert_eq!(format_duration_padded(3600.0), "00d 01h 00m 00s");
    }

    #[test]
    fn format_duration_padded_one_day() {
        // 86400 seconds.
        assert_eq!(format_duration_padded(86400.0), "01d 00h 00m 00s");
    }

    #[test]
    fn format_duration_padded_complex() {
        // 8 days, 17 hours, 57 minutes, 55 seconds — the value from the
        // screenshot. Locks the exact "Dd HHh MMm SSs" format.
        let secs = 8.0 * 86400.0 + 17.0 * 3600.0 + 57.0 * 60.0 + 55.0;
        assert_eq!(format_duration_padded(secs), "08d 17h 57m 55s");
    }

    #[test]
    fn format_duration_padded_three_digit_days() {
        // 100 days — the "d" field is unbounded.
        let secs = 100.0 * 86400.0;
        assert_eq!(format_duration_padded(secs), "100d 00h 00m 00s");
    }

    #[test]
    fn format_duration_padded_never_shortens() {
        // The whole point of the formatter is fixed width. Verify
        // adjacent-second values produce the same length.
        let a = format_duration_padded(45.0);
        let b = format_duration_padded(46.0);
        let c = format_duration_padded(59.0);
        let d = format_duration_padded(60.0);
        let e = format_duration_padded(61.0);
        assert_eq!(a.len(), b.len());
        assert_eq!(b.len(), c.len());
        assert_eq!(c.len(), d.len());
        assert_eq!(d.len(), e.len());
    }
}
