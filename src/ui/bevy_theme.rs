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

#![allow(dead_code)]

use bevy::prelude::*;

// ─── Palette ──────────────────────────────────────────────────────────────

/// Body background — near-black deep navy.
pub const BODY_BG: Color = Color::srgba(0.008, 0.039, 0.094, 1.0);
/// Card background — translucent navy (the Earth bleeds through).
pub const CARD_BG: Color = Color::srgba(0.031, 0.086, 0.172, 0.92);
/// Hovered card background — slightly brighter, also translucent.
pub const CARD_BG_HOVER: Color = Color::srgba(0.063, 0.118, 0.227, 0.78);
/// Hairline / divider — dim cyan-navy.
pub const HAIRLINE: Color = Color::srgba(0.086, 0.188, 0.306, 1.0);
/// Cyan accent — primary UI accent for titles, CTAs, active states.
pub const CYAN: Color = Color::srgba(0.373, 0.784, 0.847, 1.0);
/// Cyan accent at 16% alpha — for hairlines and 1px borders.
#[allow(dead_code)]
pub const CYAN_HAIRLINE: Color = Color::srgba(0.373, 0.784, 0.847, 0.16);
/// Cyan accent at 18% alpha — for the card outer border (subtle, not bright).
pub const CARD_BORDER: Color = Color::srgba(0.373, 0.784, 0.847, 0.18);
/// Cyan accent at 50% alpha — for chip frames (clearly visible).
pub const CYAN_BORDER: Color = Color::srgba(0.373, 0.784, 0.847, 0.50);
/// Cyan accent at 60% alpha — for the CTA border (more visible than the chip border).
pub const CYAN_BORDER_STRONG: Color = Color::srgba(0.373, 0.784, 0.847, 0.60);
/// Cyan accent at 65% alpha — for the inner top-edge highlight (the "glass rim").
pub const CYAN_RIM: Color = Color::srgba(0.373, 0.784, 0.847, 0.65);
/// Cyan accent at 100% alpha — for active CTAs.
#[allow(dead_code)]
pub const CYAN_FILLED: Color = Color::srgba(0.373, 0.784, 0.847, 1.0);
/// Dim text — subtitles, captions, effect bullets.
pub const TEXT_DIM: Color = Color::srgba(0.498, 0.580, 0.659, 1.0);
/// Brighter text — body content.
pub const TEXT_BODY: Color = Color::srgba(0.831, 0.890, 0.937, 1.0);
/// Category chip color (cyan).
#[allow(dead_code)]
pub const CHIP_CYAN: Color = Color::srgba(0.373, 0.784, 0.847, 1.0);
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
/// Filled cyan CTA background — translucent so glass bleeds through.
pub const CTA_FILL: Color = Color::srgba(0.094, 0.298, 0.353, 0.85);
/// Filled cyan CTA hover background.
pub const CTA_FILL_HOVER: Color = Color::srgba(0.137, 0.392, 0.471, 0.90);
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
/// Top-edge inner highlight — light cyan at 80% alpha for the 3D lift
/// effect on cards (lighter line on the inside top edge of the card,
/// giving the impression of light hitting the top of a glass surface).
pub const CARD_TOP_HIGHLIGHT: Color = Color::srgba(0.498, 0.733, 0.804, 0.80);

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

/// Height of the global top AppBar (panel title + subtitle row).
pub const APPBAR_H: f32 = 46.0;
/// Height of the global resource strip (Treasury / Balance / Energy / Active Colony).
pub const RESOURCE_STRIP_H: f32 = 36.0;
/// Height of the tab strip (Overview / Buildings / Build / Stockpiles).
pub const TAB_STRIP_H: f32 = 44.0;
/// Height of the underline indicator on the active tab / filter / category
/// chip. Tight (2 px) so it reads as a single line, not a separate row.
pub const TAB_UNDERLINE_H: f32 = 2.0;
/// Font size for the sub-tab row (Overview / Buildings / Build /
/// Stockpiles). Bigger than the chip default (16) so the top-level
/// section selector reads as the dominant control in the panel.
pub const TAB_FONT_SIZE: f32 = 18.0;
/// Height of the category chip row.
#[allow(dead_code)]
pub const CHIP_ROW_H: f32 = 28.0;

/// Height of a single chip (tab / build qty / filter / category). 24 px
/// matches the prototype's compact chip style.
pub const CHIP_H: f32 = 20.0;

// ─── Type Sizes ───────────────────────────────────────────────────────────

pub const TITLE_SIZE: f32 = 28.0;
pub const SUBTITLE_SIZE: f32 = 14.0;
pub const SECTION_SIZE: f32 = 16.0;
pub const BODY_SIZE: f32 = 14.0;
pub const CAPTION_SIZE: f32 = 12.0;
#[allow(dead_code)]
pub const MONO_SIZE: f32 = 12.0;

// ─── AppBar Bundle ────────────────────────────────────────────────────────

/// Bundle for the top AppBar (panel title + subtitle row).
///
/// The AppBar is a Flex row, 46 px tall, no visible border, with the title
/// left-aligned and the subtitle inline next to it. The body of the
/// component tree is responsible for any other content (status pip, etc).
///
/// The bundle includes `Name` so the spawned entity is debuggable in the
/// Bevy scene view.
#[derive(Bundle, Clone)]
pub struct AppBarBundle {
    pub node: Node,
    pub bg: BackgroundColor,
    pub name: Name,
}

impl Default for AppBarBundle {
    fn default() -> Self {
        Self {
            node: Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                height: Val::Px(APPBAR_H),
                padding: UiRect::horizontal(Val::Px(SPACE_LG)),
                column_gap: Val::Px(SPACE_XL),
                ..default()
            },
            bg: BackgroundColor(BODY_BG),
            name: Name::new("appbar"),
        }
    }
}

// ─── Card Bundle ──────────────────────────────────────────────────────────

/// Bundle for the build-card style (4-column grid item).
///
/// The card is a Flex column with 12 px internal padding, 12 px gap, and
/// a 1 px cyan border at 30% alpha. The card body content is added by the
/// caller as children.
#[derive(Bundle, Clone)]
pub struct CardBundle {
    pub node: Node,
    pub bg: BackgroundColor,
    pub border: BorderColor,
    pub name: Name,
}

impl Default for CardBundle {
    fn default() -> Self {
        Self {
            node: Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(SPACE_LG)),
                row_gap: Val::Px(SPACE_SM),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            bg: BackgroundColor(CARD_BG),
            border: BorderColor::all(CARD_BORDER),
            name: Name::new("card"),
        }
    }
}

// ─── Filled CTA Bundle ────────────────────────────────────────────────────

/// Bundle for the filled cyan CTA button (e.g. "Queue").
#[derive(Bundle, Clone)]
pub struct CtaBundle {
    pub node: Node,
    pub bg: BackgroundColor,
    pub border: BorderColor,
    pub interaction: Interaction,
    pub name: Name,
}

impl Default for CtaBundle {
    fn default() -> Self {
        Self {
            node: Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                height: Val::Px(24.0),
                padding: UiRect::horizontal(Val::Px(SPACE_MD)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(3.0)),
                ..default()
            },
            bg: BackgroundColor(CTA_FILL),
            border: BorderColor::all(CYAN_BORDER),
            interaction: Interaction::None,
            name: Name::new("cta"),
        }
    }
}

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

    format!(
        "{:02}d {:02}h {:02}m {:02}s",
        days, hours, minutes, seconds
    )
}

/// Spawn a `Text` entity with the given content, font, size, and color.
///
/// Convenience wrapper that wraps `Text`, `TextFont`, `TextColor`,
/// `TextLayout` in a bundle. The text is left-aligned by default.
#[allow(dead_code)]
pub fn text_components(text: &str, font: Handle<Font>, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(text),
        TextFont {
            font,
            font_size: size,
            ..default()
        },
        TextColor(color),
        TextLayout::default(),
    )
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
    pub fn new_with_border(
        label: &str,
        is_active: bool,
        border_override: Option<UiRect>,
    ) -> Self {
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
            TextColor(if is_active {
                ACTIVE_CHIP_TEXT
            } else {
                CYAN
            }),
            ChipTextNode { is_active },
        ))
        .id();
    commands.entity(parent).add_child(text_entity);
    text_entity
}

/// Hover / press effect system for chip buttons.
///
/// On hover: background brightens to ACTIVE_CHIP_BG, text inverts to
/// bright white for contrast.
/// On press: same as hover, with a small scale-down.
/// On release: returns to the chip's default (active or inactive).
///
/// Chips have no individual border (the container provides it), so this
/// system only mutates BackgroundColor + TextColor + UiTransform on the
/// button entity. The text is in a child node (marked `ChipTextNode`) and
/// the system mutates the child's TextColor via `Children`.
pub fn tick_chip_button_hover(
    mut button_query: Query<
        (&Interaction, &mut BackgroundColor, &mut UiTransform, &Children),
        With<Button>,
    >,
    mut text_query: Query<&mut TextColor, With<ChipTextNode>>,
) {
    // Plan: read interactions, then mutate in two passes to sidestep
    // borrow-checker conflicts between `iter_mut` on the button query and
    // the `text_query.get_mut(child)` call inside the loop.
    let mut bg_scale_plans: Vec<(BackgroundColor, Vec2)> =
        Vec::with_capacity(button_query.iter().len());
    let mut child_color_plan: Vec<(Entity, Color)> = Vec::new();
    for (interaction, _, _, children) in button_query.iter() {
        let (bg, scale, text_color) = match interaction {
            Interaction::Pressed => (
                BackgroundColor(ACTIVE_CHIP_BG),
                Vec2::splat(0.96),
                ACTIVE_CHIP_TEXT,
            ),
            Interaction::Hovered => (
                BackgroundColor(ACTIVE_CHIP_BG),
                Vec2::splat(1.00),
                ACTIVE_CHIP_TEXT,
            ),
            Interaction::None => (
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
                Vec2::splat(1.00),
                CYAN,
            ),
        };
        bg_scale_plans.push((bg, scale));
        for child in children.iter() {
            child_color_plan.push((child, text_color));
        }
    }
    // Apply button background + scale. Re-iterate mutably and consume
    // the plan in lockstep.
    let mut plan_iter = bg_scale_plans.into_iter();
    for (_, mut bg, mut ui_transform, _) in button_query.iter_mut() {
        if let Some((new_bg, new_scale)) = plan_iter.next() {
            *bg = new_bg;
            ui_transform.scale = new_scale;
        }
    }
    // Apply child text colors.
    for (child, color) in child_color_plan.iter() {
        if let Ok(mut text_color) = text_query.get_mut(*child) {
            *text_color = TextColor(*color);
        }
    }
}

/// System that reverts chips to their **active** state every frame, so a
/// chip that's marked active in the construction UI stays visually
/// highlighted even after mouse-out. This is run alongside
/// `tick_chip_button_hover` — the hover system wins on hover/press, this
/// system wins on the next frame for the active chips.
///
/// In the canary phase, every chip with `is_active = true` stays highlighted
/// by the construction UI's static selection (e.g. "Build" tab, "All"
/// filter, "x1" build qty, "Infrastructure" category). Real interactivity
/// (clicking to toggle active state) is Phase C4 work.

pub fn tick_chip_button_active_overlay(
    mut chips: Query<(Entity, &super::construction::ChipKind, &mut BackgroundColor, &Children), With<Button>>,
    mut text_query: Query<&mut TextColor, With<ChipTextNode>>,
    active: Res<super::construction::ActiveChips>,
) {
    // Walk all chips, apply ACTIVE_CHIP_BG to the one matching the
    // resource's row-specific active index, and INACTIVE_CHIP_BG to
    // all others. This is the single source of truth: the ActiveChips
    // resource determines the visual state, no markers needed.
    for (_entity, kind, mut bg, children) in chips.iter_mut() {
        let is_active = match kind {
            super::construction::ChipKind::Tab(idx) => *idx == active.tab,
            super::construction::ChipKind::Qty(qty) => *qty == active.qty,
            super::construction::ChipKind::Category(idx) => *idx == active.category,
            // Filter chips are unused (replaced by Category in v3.9)
            super::construction::ChipKind::Filter(_) => false,
            // v0.5.2 PR-A.2: Mining tab qty chip. The active
            // overlay for the Mining tab qty chips is set by
            // `spawn_mining_qty_row` (re-spawned each refresh);
            // the bevy_theme.rs active overlay doesn't touch them.
            // Returning false here prevents the Build tab's overlay
            // from painting over the Mining tab's qty chips.
            super::construction::ChipKind::MiningQty(_) => false,
        };
        if is_active {
            *bg = BackgroundColor(ACTIVE_CHIP_BG);
            for child in children.iter() {
                if let Ok(mut text_color) = text_query.get_mut(child) {
                    *text_color = TextColor(ACTIVE_CHIP_TEXT);
                }
            }
        } else {
            *bg = BackgroundColor(INACTIVE_CHIP_BG);
            for child in children.iter() {
                if let Ok(mut text_color) = text_query.get_mut(child) {
                    *text_color = TextColor(TEXT_BODY);
                }
            }
        }
    }
}


/// System: ensure every active chip has a subtle cyan glow, and every
/// inactive chip has no glow. The active state is determined by the
/// `ActiveChips` resource (single source of truth — the same resource
/// `tick_chip_button_active_overlay` reads).
///
/// The glow is a small `BoxShadow` (4 px blur, cyan at 35% alpha) similar
/// to the active-tab underline. We **add** it on the active chip and
/// **remove** it from every other chip every frame — this is symmetric
/// so the glow always tracks the active chip, even when the player
/// clicks across rows (e.g. tab "Build" → tab "Overview" must move the
/// glow from the Build tab to the Overview tab, not leave it stuck on
/// Build). Cost is negligible: < 30 chips per frame.
pub fn tick_active_chip_glow(
    mut commands: Commands,
    chips: Query<
        (Entity, &super::construction::ChipKind, Option<&BoxShadow>),
        With<Button>,
    >,
    active: Res<super::construction::ActiveChips>,
) {
    for (entity, kind, existing_shadow) in chips.iter() {
        let is_active = match kind {
            super::construction::ChipKind::Tab(idx) => *idx == active.tab,
            super::construction::ChipKind::Qty(qty) => *qty == active.qty,
            super::construction::ChipKind::Category(idx) => *idx == active.category,
            super::construction::ChipKind::Filter(_) => false,
            // v0.5.2 PR-A.2: see the active-overlay system
            // above — MiningQty chips are managed by the Mining
            // tab's own refresh system, not by this overlay.
            super::construction::ChipKind::MiningQty(_) => false,
        };
        if is_active {
            if existing_shadow.is_none() {
                commands.entity(entity).insert(BoxShadow::new(
                    Color::srgba(0.373, 0.784, 0.847, 0.35),
                    Val::Px(0.0),
                    Val::Px(0.0),
                    Val::Px(0.0),
                    Val::Px(4.0),
                ));
            }
        } else if existing_shadow.is_some() {
            // Inactive chip: remove the glow so the visual selection
            // tracks the active chip at all times.
            commands.entity(entity).remove::<BoxShadow>();
        }
    }
}

/// Marker for a chip that should be visually rendered as active. Set this
/// on chips whose `is_active` was true at spawn time, so the overlay
/// system can find them.
#[derive(Component)]
pub struct ChipActive;

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
