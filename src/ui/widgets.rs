//! Reusable native Bevy UI widgets (v0.5.2, 2026-08-06).
//!
//! The foundation for the planned bevy_ui rollout to all menus
//! (currently the Construction canary is the only native-UI surface).
//! Anything here is menu-agnostic — no construction-specific types,
//! no game-state coupling — so a future Shipbuilding / Research /
//! Fleet bevy_ui menu can depend on it without pulling in the
//! Construction canary.
//!
//! What lives here:
//!
//! - [`UiFonts`] — the three canonical font handles, loaded ONCE at
//!   Startup. Every other bevy_ui menu reads `Res<UiFonts>` instead
//!   of calling `asset_server.load("fonts/...")` per frame (the
//!   Construction canary used to do exactly that in five per-frame
//!   systems — the handle is path-cached so it worked, but it was
//!   ~5 wasted registry lookups per frame).
//! - [`spawn_scrollable_container`] — the shared `Overflow::scroll_y`
//!   column pattern. The Construction canary hand-rolled five
//!   near-identical scroll containers; this is the one helper.
//! - [`spawn_text_label`] — a one-line text node with a font +
//!   size + colour, the most common child in any bevy_ui menu.
//! - [`HoverElevation`] + [`tick_ui_hover_elevation`] — the shared
//!   hover/press styling widget (scale, border, background, shadow,
//!   z-index lift) so every menu's cards get the same interactive
//!   3D treatment without per-menu hover systems.
//! - [`card_shadow`] / [`card_shadow_hover`] — the canonical
//!   raised-card drop shadows (dual-layer: tight contact + soft
//!   cast) for the dark-on-dark theme.

use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::prelude::*;
use bevy::ui::ShadowStyle;
use bevy::window::PrimaryWindow;
use std::collections::HashMap;

/// Canonical body font path. Regular-weight Inter for paragraphs,
/// card bodies, and default UI text.
pub const FONT_BODY: &str = "fonts/Inter-Regular.otf";
/// Semi-bold Inter for headers, stat labels, and emphasis.
pub const FONT_MEDIUM: &str = "fonts/Inter-SemiBold.otf";
/// Geist Mono for numbers, ETA timers, and tabular readouts.
pub const FONT_MONO: &str = "fonts/GeistMono-Medium.ttf";

/// Top offset (px) for native bevy_ui full-window panels so they sit
/// below the global egui chrome (top resource bar + icon strip).
/// Both the Construction canary and the Shipbuilding workspace
/// hard-coded `126.0` (v0.5.2, 2026-08-06) — this is the shared const.
pub const UI_CHROME_TOP_PX: f32 = 126.0;

/// The three canonical font handles, loaded once at Startup.
///
/// ## Why (v0.5.2, 2026-08-06)
///
/// The Construction canary loaded the same three fonts inside
/// five per-frame systems (`update_buildings_body`,
/// `update_mining_body`, `update_overview_queue`,
/// `refresh_colony_dropdown`, `spawn_queue_row`). `asset_server.load`
/// is path-cached so each call returns the same handle, but it is
/// still a registry lookup per call — and the spawn helpers thread
/// three separate `&Handle<Font>` params around. `Res<UiFonts>` folds
/// all three into one resource that any future menu can read.
#[derive(Resource, Clone, Default)]
pub struct UiFonts {
    pub body: Handle<Font>,
    pub medium: Handle<Font>,
    pub mono: Handle<Font>,
}

/// Startup system: populate the [`UiFonts`] resource.
///
/// Registered once in `UIPlugin::build` (see `src/ui/mod.rs`).
/// Runs before any bevy_ui menu spawns so `Res<UiFonts>` is always
/// populated by the first frame.
pub fn init_ui_fonts(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(UiFonts {
        body: asset_server.load(FONT_BODY),
        medium: asset_server.load(FONT_MEDIUM),
        mono: asset_server.load(FONT_MONO),
    });
}

/// Spawn a `Overflow::scroll_y` column container with `min_height: 0`
/// + `flex_grow: 1` — the canonical scrollable-body pattern.
///
/// ## Why (v0.5.2, 2026-08-06)
///
/// The Construction canary hand-rolled five near-identical scroll
/// containers (Build card grid, Mining body, Buildings body, Overview
/// queue, Queue panel body). Each was a `FlexDirection::Column` with
/// `overflow.y = Scroll`, `min_height: 0` (so `flex_grow: 1` actually
/// participates in flex layout — without `min_height: 0` a tall child
/// overflows the parent instead of shrinking it) and `flex_grow: 1.0`.
/// This is that pattern in one helper.
///
/// `row_gap` is the vertical gap between children (e.g.
/// `SPACE_XS`); `extra` is any bundle to attach (a marker
/// `Component` — e.g. `MiningContent`, `QueuePanelBody` — or
/// `Name`). Returns the entity id.
pub fn spawn_scrollable_container<B: Bundle>(
    commands: &mut Commands,
    parent: Entity,
    name: &'static str,
    row_gap: f32,
    extra: B,
) -> Entity {
    let node = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                width: Val::Percent(100.0),
                flex_grow: 1.0,
                // `min_height: 0` is load-bearing — see the doc
                // comment above.
                min_height: Val::Px(0.0),
                row_gap: Val::Px(row_gap),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            extra,
            Name::new(name),
        ))
        .id();
    commands.entity(parent).add_child(node);
    node
}

/// [`spawn_scrollable_container`] variant for use inside a
/// `with_children(|builder| ...)` closure, where the parent is a
/// [`ChildSpawnerCommands`] (Bevy 0.18's `ChildBuilder`) rather than
/// a raw [`Entity`].
///
/// Returns [`EntityCommands`] (not `Entity`) so the caller can chain
/// `.with_children(...)` directly — mirroring the original
/// `parent.spawn((Node {...})).with_children(...)` shape.
///
/// ## Why (v0.5.2, 2026-08-06)
///
/// The Shipbuilding workspace spawns its scrollable component
/// database inside a `with_children` closure — a different parent
/// handle than the `Commands`-based helper. This variant keeps the
/// same canonical scrollable shape for both spawn styles.
pub fn spawn_scrollable_container_child<'w, 'b, B: Bundle>(
    builder: &'b mut ChildSpawnerCommands<'w>,
    name: &'static str,
    row_gap: f32,
    extra: B,
) -> EntityCommands<'b> {
    builder.spawn((
        Node {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            width: Val::Percent(100.0),
            flex_grow: 1.0,
            min_height: Val::Px(0.0),
            row_gap: Val::Px(row_gap),
            overflow: Overflow::scroll_y(),
            ..default()
        },
        extra,
        Name::new(name),
    ))
}

/// Spawn a single-line text node. The most common bevy_ui leaf.
///
/// Returns the entity id. `font` is a [`UiFonts`] field (or any
/// `Handle<Font>`); callers that already hold a handle pass it
/// directly. `marker` is an optional `Component` (or `()`).
pub fn spawn_text_label<B: Bundle>(
    commands: &mut Commands,
    parent: Entity,
    text: impl Into<String>,
    font: Handle<Font>,
    font_size: f32,
    color: Color,
    marker: B,
) -> Entity {
    let node = commands
        .spawn((
            Text::new(text),
            TextFont {
                font,
                font_size,
                ..default()
            },
            TextColor(color),
            marker,
        ))
        .id();
    commands.entity(parent).add_child(node);
    node
}

// ─── Card chrome (raised / "3D" primitives) ─────────────────────────────

/// Tight contact shadow layer for a raised bevy_ui card.
///
/// v0.5.3.5 neumorphism redesign (2026-08-10). Research from the
/// canonical Hype4 neumorphism generator + Refactoring UI palette
/// guidance: an outward cyan glow reads as "neon outline", NOT
/// "raised glass" — the raised-glass reading on dark surfaces comes
/// from a *directional* drop shadow (cast to bottom-right) plus a
/// bright top-left edge on the card itself. The card-vs-panel
/// brightness delta in `CARD_BG` carries the silhouette; this
/// shadow just gives the lift a "floor" below the card.
///
/// Offset is now `(+4, +5)` with a 10 px blur and dark navy colour —
/// a single strong contact shadow with a clear light-from-top-left
/// direction, no neon halo.
pub const CARD_SHADOW: ShadowStyle = ShadowStyle {
    color: Color::srgba(0.0, 0.02, 0.05, 0.55),
    x_offset: Val::Px(4.0),
    y_offset: Val::Px(5.0),
    spread_radius: Val::Px(0.0),
    blur_radius: Val::Px(10.0),
};

/// Soft wide-cast layer for a raised card — see [`CARD_SHADOW`].
///
/// Wider + softer so the directional shadow's edge fades smoothly
/// rather than ending in a hard line; offset matches the contact
/// shadow's direction.
pub const CARD_SHADOW_SOFT: ShadowStyle = ShadowStyle {
    color: Color::srgba(0.0, 0.02, 0.05, 0.35),
    x_offset: Val::Px(2.0),
    y_offset: Val::Px(8.0),
    spread_radius: Val::Px(0.0),
    blur_radius: Val::Px(22.0),
};

/// v0.5.3.5: outer cyan glow removed entirely (was a "neon" reading,
/// not a "raised glass" reading). The constant stays for API
/// compatibility but no longer participates in `card_shadow()`.
/// A future surface that *wants* the neon reading (a glowing button,
/// a power-on indicator) can still compose it manually.
#[allow(dead_code)]
pub const CARD_SHADOW_GLOW: ShadowStyle = ShadowStyle {
    color: Color::srgba(0.373, 0.784, 0.847, 0.14),
    x_offset: Val::Px(2.0),
    y_offset: Val::Px(2.0),
    spread_radius: Val::Px(0.0),
    blur_radius: Val::Px(6.0),
};

/// Hovered tight contact shadow — `scale 1.02` + `ZIndex 1` already
/// give the hover lift; the shadow just deepens slightly so the
/// hovered card looks further away from the panel.
pub const CARD_SHADOW_HOVER: ShadowStyle = ShadowStyle {
    color: Color::srgba(0.0, 0.02, 0.05, 0.70),
    x_offset: Val::Px(5.0),
    y_offset: Val::Px(7.0),
    spread_radius: Val::Px(0.0),
    blur_radius: Val::Px(12.0),
};

/// Hovered soft wide-cast layer — see [`CARD_SHADOW_HOVER`].
pub const CARD_SHADOW_HOVER_SOFT: ShadowStyle = ShadowStyle {
    color: Color::srgba(0.0, 0.02, 0.05, 0.40),
    x_offset: Val::Px(3.0),
    y_offset: Val::Px(11.0),
    spread_radius: Val::Px(0.0),
    blur_radius: Val::Px(26.0),
};

/// v0.5.3.5: hovered outer cyan glow removed (same rationale as
/// [`CARD_SHADOW_GLOW`]). Kept for API compatibility.
#[allow(dead_code)]
pub const CARD_SHADOW_HOVER_GLOW: ShadowStyle = ShadowStyle {
    color: Color::srgba(0.373, 0.784, 0.847, 0.24),
    x_offset: Val::Px(3.0),
    y_offset: Val::Px(3.0),
    spread_radius: Val::Px(0.0),
    blur_radius: Val::Px(8.0),
};

/// The canonical resting drop shadow for a raised card — the
/// directional dark-only pair (contact + soft cast). Both layers
/// cast to the bottom-right; no neon halo. v0.5.3.5.
pub fn card_shadow() -> BoxShadow {
    BoxShadow(vec![CARD_SHADOW, CARD_SHADOW_SOFT])
}

/// The canonical hovered drop shadow for a raised card.
pub fn card_shadow_hover() -> BoxShadow {
    BoxShadow(vec![CARD_SHADOW_HOVER, CARD_SHADOW_HOVER_SOFT])
}

/// Per-node hover/press styling for bevy_ui.
///
/// Insert on any pickable node that should react to hover/press with
/// a scale shift, border / background colour change, a deeper shadow,
/// and/or a `ZIndex` lift above its siblings.
/// [`tick_ui_hover_elevation`](tick_ui_hover_elevation) applies the
/// state; [`Default`] is an identity (no visual change) so callers
/// opt in field-by-field.
///
/// ## Ownership rule (how the tick system decides what to touch)
///
/// - **Scale** is always managed (`base_scale` / `hover_scale` /
///   `press_scale` always exist).
/// - **Border / background / shadow** are each managed only when the
///   *pair* is set — e.g. both `border` and `border_hover` `Some`.
///   This keeps rest-state restore symmetric: if only `bg_hover`
///   were set, the hover colour would stick after mouse-out.
/// - **`z_lift`** is independent: hover → `ZIndex(1)`, mouse-out →
///   `ZIndex(0)`. The node gets a `ZIndex` from [`Node`]'s required
///   components automatically.
///
/// The node must be pickable (carry [`Pickable`] / [`Button`]) so
/// `Interaction` updates; the system skips nodes without it.
#[derive(Component, Debug, Clone)]
pub struct HoverElevation {
    /// Resting scale — written back on `Interaction::None`.
    pub base_scale: Vec2,
    /// Scale while hovered.
    pub hover_scale: Vec2,
    /// Scale while pressed.
    pub press_scale: Vec2,
    /// Resting border state — a full per-edge `BorderColor` (Bevy
    /// 0.18's `BorderColor` has `top/right/bottom/left` fields).
    /// `Some` hands border ownership to the tick system; pairs with
    /// [`Self::border_hover`]. v0.5.3.5: changed from `Option<Color>`
    /// to `Option<BorderColor>` so a hover swap can restore a
    /// directional per-edge bevel (e.g. bright top + left, dim
    /// bottom + right) instead of collapsing it to a uniform colour
    /// on mouse-out.
    pub border_rest: Option<BorderColor>,
    /// Border colour while hovered / pressed — always uniform.
    /// The hover state overrides the per-edge bevel so the
    /// "highlighted" reading is unmistakable; the resting bevel
    /// comes back when the cursor leaves.
    pub border_hover: Option<Color>,
    /// Resting background colour — pairs with [`Self::bg_hover`].
    pub bg: Option<Color>,
    /// Background colour while hovered / pressed.
    pub bg_hover: Option<Color>,
    /// Resting drop shadow — pairs with [`Self::shadow_hover`].
    pub shadow: Option<BoxShadow>,
    /// Drop shadow while hovered / pressed.
    pub shadow_hover: Option<BoxShadow>,
    /// Lift `ZIndex` 0 → 1 while hovered so the node paints above
    /// its siblings (draw order only — no layout impact).
    pub z_lift: bool,
}

impl Default for HoverElevation {
    fn default() -> Self {
        Self {
            base_scale: Vec2::ONE,
            hover_scale: Vec2::ONE,
            press_scale: Vec2::ONE,
            border_rest: None,
            border_hover: None,
            bg: None,
            bg_hover: None,
            shadow: None,
            shadow_hover: None,
            z_lift: false,
        }
    }
}

/// System: apply [`HoverElevation`] to nodes that carry it.
///
/// Single query — no B0001 (dual-query) risk. `Changed<Interaction>`
/// keeps the system near-free on frames where nothing is hovered;
/// a `Local<HashMap<Entity, Interaction>>` suppresses redundant
/// re-writes when the interaction state is unchanged (mirrors the
/// flicker-mitigation pattern in
/// `construction::tick_construction_cta_hover`). Only the field
/// pairs that are `Some` are managed — see the [`HoverElevation`]
/// ownership rule.
pub fn tick_ui_hover_elevation(
    mut nodes: Query<
        (
            Entity,
            &Interaction,
            &HoverElevation,
            &mut UiTransform,
            &mut BorderColor,
            &mut BackgroundColor,
            &mut ZIndex,
            Option<&mut BoxShadow>,
        ),
        Changed<Interaction>,
    >,
    mut prev_state: Local<std::collections::HashMap<Entity, Interaction>>,
) {
    for (
        entity,
        interaction,
        elevation,
        mut transform,
        mut border,
        mut bg,
        mut zindex,
        mut shadow,
    ) in nodes.iter_mut()
    {
        // Skip when the interaction state is unchanged since last
        // write. First frame (`prev == None`) always writes so
        // newly-spawned nodes get their base paint.
        if prev_state.get(&entity).copied() == Some(*interaction) {
            continue;
        }
        match *interaction {
            Interaction::Hovered | Interaction::Pressed => {
                let pressed = *interaction == Interaction::Pressed;
                transform.scale = if pressed {
                    elevation.press_scale
                } else {
                    elevation.hover_scale
                };
                if elevation.border_rest.is_some() {
                    if let Some(hover) = elevation.border_hover {
                        border.set_all(hover);
                    }
                }
                if elevation.bg.is_some() {
                    if let Some(hover) = elevation.bg_hover {
                        bg.0 = hover;
                    }
                }
                if elevation.shadow.is_some() {
                    if let (Some(hover), Some(box_shadow)) =
                        (&elevation.shadow_hover, shadow.as_mut())
                    {
                        // `Option<&mut T>` query items yield `Mut<T>`
                        // in Bevy 0.18 — assign through it.
                        **box_shadow = hover.clone();
                    }
                }
                if elevation.z_lift {
                    zindex.0 = 1;
                }
            }
            Interaction::None => {
                transform.scale = elevation.base_scale;
                if let Some(rest) = elevation.border_rest {
                    *border = rest;
                }
                if let Some(base) = elevation.bg {
                    bg.0 = base;
                }
                if let (Some(base), Some(box_shadow)) = (&elevation.shadow, shadow.as_mut()) {
                    **box_shadow = base.clone();
                }
                if elevation.z_lift {
                    zindex.0 = 0;
                }
            }
        }
        prev_state.insert(entity, *interaction);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_fonts_default_has_null_handles() {
        // The resource must be `Default`-able so `init_resource` works
        // before the Startup system runs (a test App that never runs
        // Startup still gets a valid resource).
        let fonts = UiFonts::default();
        assert_eq!(fonts.body, Handle::<Font>::default());
        assert_eq!(fonts.medium, Handle::<Font>::default());
        assert_eq!(fonts.mono, Handle::<Font>::default());
    }

    #[test]
    fn font_path_constants_match_assets() {
        // Guard against a typo'd path — `asset_server.load` would
        // silently log "Path not found" otherwise.
        assert!(std::path::Path::new("assets").join(FONT_BODY).exists());
        assert!(std::path::Path::new("assets").join(FONT_MEDIUM).exists());
        assert!(std::path::Path::new("assets").join(FONT_MONO).exists());
    }

    #[test]
    fn hover_elevation_applies_hover_state_and_restores() {
        let mut app = App::new();
        app.add_systems(Update, tick_ui_hover_elevation);

        // v0.5.3.5 neumorphism: `border_rest` is now the full
        // per-edge `BorderColor` rather than a single colour so the
        // tick system can restore a directional bevel after a hover
        // swap (the resting bevel survives the mouse-out frame).
        let rest_border = BorderColor {
            top: Color::srgba(0.9, 0.9, 0.9, 1.0),
            bottom: Color::srgba(0.1, 0.1, 0.1, 1.0),
            left: Color::srgba(0.9, 0.9, 0.9, 1.0),
            right: Color::srgba(0.1, 0.1, 0.1, 1.0),
        };
        let elevation = HoverElevation {
            hover_scale: Vec2::splat(1.02),
            press_scale: Vec2::splat(0.98),
            border_rest: Some(rest_border),
            border_hover: Some(Color::WHITE),
            bg: Some(Color::srgba(0.05, 0.1, 0.2, 0.9)),
            bg_hover: Some(Color::srgba(0.2, 0.3, 0.4, 0.9)),
            shadow: Some(card_shadow()),
            shadow_hover: Some(card_shadow_hover()),
            z_lift: true,
            ..default()
        };
        let entity = app
            .world_mut()
            .spawn((
                Node::default(),
                elevation.clone(),
                Interaction::None,
                BoxShadow::default(),
            ))
            .id();

        // First frame: base state applied (newly-spawned nodes always
        // paint their rest state).
        app.update();
        {
            let world = app.world();
            assert_eq!(world.get::<UiTransform>(entity).unwrap().scale, Vec2::ONE);
            assert_eq!(*world.get::<BorderColor>(entity).unwrap(), rest_border);
            assert_eq!(
                world.get::<BackgroundColor>(entity).unwrap().0,
                elevation.bg.unwrap()
            );
            // v0.5.3.5: the resting shadow is now the 2-layer
            // directional pair (no outer cyan glow).
            assert_eq!(
                world.get::<BoxShadow>(entity).unwrap().0,
                vec![CARD_SHADOW, CARD_SHADOW_SOFT]
            );
            assert_eq!(world.get::<ZIndex>(entity).unwrap().0, 0);
        }

        // Hovered: lift + brighten + z-index above siblings.
        *app.world_mut().get_mut::<Interaction>(entity).unwrap() = Interaction::Hovered;
        app.update();
        {
            let world = app.world();
            assert_eq!(
                world.get::<UiTransform>(entity).unwrap().scale,
                Vec2::splat(1.02)
            );
            // Hovered border is uniform CYAN — overrides the per-edge
            // bevel for an unmistakable "selected" reading.
            assert_eq!(world.get::<BorderColor>(entity).unwrap().top, Color::WHITE);
            assert_eq!(
                world.get::<BackgroundColor>(entity).unwrap().0,
                elevation.bg_hover.unwrap()
            );
            // v0.5.3.5: hovered shadow is the 2-layer directional pair.
            assert_eq!(
                world.get::<BoxShadow>(entity).unwrap().0,
                vec![CARD_SHADOW_HOVER, CARD_SHADOW_HOVER_SOFT]
            );
            assert_eq!(world.get::<ZIndex>(entity).unwrap().0, 1);
        }

        // Mouse-out: restore the base state, including the per-edge
        // bevel.
        *app.world_mut().get_mut::<Interaction>(entity).unwrap() = Interaction::None;
        app.update();
        {
            let world = app.world();
            assert_eq!(world.get::<UiTransform>(entity).unwrap().scale, Vec2::ONE);
            assert_eq!(*world.get::<BorderColor>(entity).unwrap(), rest_border);
            assert_eq!(
                world.get::<BackgroundColor>(entity).unwrap().0,
                elevation.bg.unwrap()
            );
            assert_eq!(
                world.get::<BoxShadow>(entity).unwrap().0,
                vec![CARD_SHADOW, CARD_SHADOW_SOFT]
            );
            assert_eq!(world.get::<ZIndex>(entity).unwrap().0, 0);
        }
    }

    #[test]
    fn hover_elevation_uses_press_scale_when_pressed() {
        let mut app = App::new();
        app.add_systems(Update, tick_ui_hover_elevation);
        let elevation = HoverElevation {
            hover_scale: Vec2::splat(1.02),
            press_scale: Vec2::splat(0.98),
            ..default()
        };
        let entity = app
            .world_mut()
            .spawn((Node::default(), elevation, Interaction::None))
            .id();
        app.update();
        *app.world_mut().get_mut::<Interaction>(entity).unwrap() = Interaction::Pressed;
        app.update();
        assert_eq!(
            app.world().get::<UiTransform>(entity).unwrap().scale,
            Vec2::splat(0.98)
        );
    }

    #[test]
    fn hover_elevation_skips_nodes_without_the_component() {
        let mut app = App::new();
        app.add_systems(Update, tick_ui_hover_elevation);
        let entity = app
            .world_mut()
            .spawn((Node::default(), Interaction::Hovered))
            .id();
        app.update();
        // The system never runs for this entity — the default scale stays.
        assert_eq!(
            app.world().get::<UiTransform>(entity).unwrap().scale,
            Vec2::ONE
        );
    }

    #[test]
    fn hover_elevation_leaves_unmanaged_shadow_alone() {
        let mut app = App::new();
        app.add_systems(Update, tick_ui_hover_elevation);
        // shadow / shadow_hover both None → the system must not touch
        // the node's BoxShadow (the caller owns it inline).
        let elevation = HoverElevation {
            hover_scale: Vec2::splat(1.05),
            ..default()
        };
        let entity = app
            .world_mut()
            .spawn((
                Node::default(),
                elevation,
                Interaction::None,
                BoxShadow::default(),
            ))
            .id();
        app.update();
        *app.world_mut().get_mut::<Interaction>(entity).unwrap() = Interaction::Hovered;
        app.update();
        assert!(app.world().get::<BoxShadow>(entity).unwrap().0.is_empty());
    }
}

// =====================================================================
// Chip row machinery (Phase 2: extracted from `bevy_theme.rs` and
// `construction::*`. Owns the chip marker enum, the active-state
// resource, and the three per-frame tick systems that drive chip
// hover / press / active-overlay / glow visuals.)
// =====================================================================

/// Identifies which row a chip belongs to and what value it carries.
///
/// Generic replacement for the construction-only `ChipKind` enum.
/// The tick systems read `ChipGroup` to decide which chip in a row
/// is currently "active" (selected tab / qty / category).
///
/// Filter chips are intentionally omitted — the original enum had a
/// `Filter(BuildFilter)` variant but the tick systems treated it as
/// always-inactive (return `false`), so it was effectively dead.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChipGroup {
    /// Sub-tab chip. Index is the tab's position in the row.
    Tab(usize),
    /// Build-quantity chip. Value is the multiplier (1, 5, 10, ...).
    Qty(u32),
    /// Build-category chip. Index is the category's position.
    Category(usize),
}

/// Single source of truth for which chip in each row is currently
/// "active" (selected). The three tick systems read this resource
/// every frame and paint the matching chip in `ACTIVE_CHIP_BG` while
/// the rest stay at `INACTIVE_CHIP_BG`.
///
/// `mining_qty` is construction-specific (Mining tab keeps its own
/// qty separate from the Build tab's qty) but lives here so
/// construction can keep a single `ActiveChips` resource rather than
/// splitting it. The chip tick systems ignore it.
#[derive(Resource, Debug, Clone)]
pub struct ActiveChips {
    /// Active sub-tab index (0=Overview, 1=Buildings, 2=Build, 3=Mining).
    pub tab: usize,
    /// Active build-qty multiplier (Build tab).
    pub qty: u32,
    /// Active filter/category index (0..8 = category, 9 = All).
    pub category: usize,
    /// Active mining-qty multiplier (Mining tab). Construction-only;
    /// the chip tick systems do not read it.
    pub mining_qty: u32,
}

impl Default for ActiveChips {
    fn default() -> Self {
        Self {
            tab: 2,        // Build tab is default
            qty: 1,        // x1 is default
            category: 8,   // "All" is default
            mining_qty: 1, // x1 is default for the Mining tab
        }
    }
}

/// Hover-state machine for chip buttons.
///
/// On hover: background brightens to `ACTIVE_CHIP_BG`, text inverts to
/// bright white. On press: same as hover with a small scale-down.
/// On release: returns to the chip's default state.
///
/// PERF-CRITICAL filter: `With<ChipGroup>` scopes the system to chip
/// entities ONLY. Without it the `With<Button>` filter would match
/// every button in the world and re-paint them every frame (the
/// v0.5.0-era "huge compute for a simple menu" sink).
pub fn tick_chip_button_hover(
    mut button_query: Query<
        (&Interaction, &mut BackgroundColor, &mut UiTransform, &Children),
        (With<Button>, With<ChipGroup>),
    >,
    mut text_query: Query<&mut TextColor, With<crate::ui::bevy_theme::ChipTextNode>>,
) {
    use crate::ui::bevy_theme::{ACTIVE_CHIP_BG, ACTIVE_CHIP_TEXT, CYAN};
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
    let mut plan_iter = bg_scale_plans.into_iter();
    for (_, mut bg, mut ui_transform, _) in button_query.iter_mut() {
        if let Some((new_bg, new_scale)) = plan_iter.next() {
            *bg = new_bg;
            ui_transform.scale = new_scale;
        }
    }
    for (child, color) in child_color_plan.iter() {
        if let Ok(mut text_color) = text_query.get_mut(*child) {
            *text_color = TextColor(*color);
        }
    }
}

/// Reverts chips to their **active** state every frame, so a chip
/// marked active in `ActiveChips` stays highlighted even after
/// mouse-out. `tick_chip_button_hover` wins on hover/press; this
/// system wins the next frame for the active chip.
pub fn tick_chip_button_active_overlay(
    mut chips: Query<
        (Entity, &ChipGroup, &mut BackgroundColor, &Children),
        With<Button>,
    >,
    mut text_query: Query<&mut TextColor, With<crate::ui::bevy_theme::ChipTextNode>>,
    active: Res<ActiveChips>,
) {
    use crate::ui::bevy_theme::{
        ACTIVE_CHIP_BG, ACTIVE_CHIP_TEXT, INACTIVE_CHIP_BG, TEXT_BODY,
    };
    for (_entity, kind, mut bg, children) in chips.iter_mut() {
        let is_active = match kind {
            ChipGroup::Tab(idx) => *idx == active.tab,
            ChipGroup::Qty(qty) => *qty == active.qty,
            ChipGroup::Category(idx) => *idx == active.category,
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

/// Ensure the active chip has a subtle cyan glow and every other
/// chip has none. Active state determined by `ActiveChips`. The
/// glow is added/removed symmetrically so it tracks the active
/// chip across row switches.
pub fn tick_active_chip_glow(
    mut commands: Commands,
    chips: Query<(Entity, &ChipGroup, Option<&BoxShadow>), With<Button>>,
    active: Res<ActiveChips>,
) {
    for (entity, kind, existing_shadow) in chips.iter() {
        let is_active = match kind {
            ChipGroup::Tab(idx) => *idx == active.tab,
            ChipGroup::Qty(qty) => *qty == active.qty,
            ChipGroup::Category(idx) => *idx == active.category,
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
            commands.entity(entity).remove::<BoxShadow>();
        }
    }
}

// =====================================================================
// Click rise-edge helper (Phase 3: extracts the hand-rolled
// `Local<HashMap<Entity, Interaction>>` preamble from every
// construction click system. ~15-20 LOC saved per system.)
// =====================================================================

/// Walk every Button entity matching `<Button, With<M>>`, emit
/// `on_pressed(entity, &M)` exactly once per rise edge
/// (non-Pressed -> Pressed transition). The previous-state cache
/// lives in the supplied `Local<HashMap<...>>` so each system
/// keeps its own edge history across frames.
pub fn detect_rising_edges<M: Component, B: bevy::ecs::query::QueryFilter>(
    prev: &mut Local<HashMap<Entity, Interaction>>,
    query: &Query<(Entity, &Interaction, &M), B>,
    mut on_pressed: impl FnMut(Entity, &M),
) {
    let mut current: HashMap<Entity, Interaction> = HashMap::new();
    for (entity, interaction, marker) in query.iter() {
        let prev_interaction = prev.get(&entity).copied().unwrap_or(Interaction::None);
        if *interaction == Interaction::Pressed && prev_interaction != Interaction::Pressed {
            on_pressed(entity, marker);
        }
        current.insert(entity, *interaction);
    }
    **prev = current;
}



/// Same as [`detect_rising_edges`] but for click systems that match on
/// only `<Entity, &Interaction>` (no marker `M`). E.g. a single button
/// in a one-off panel.
pub fn detect_rising_edges_no_marker<B: bevy::ecs::query::QueryFilter>(
    prev: &mut Local<HashMap<Entity, Interaction>>,
    query: &Query<(Entity, &Interaction), B>,
    mut on_pressed: impl FnMut(Entity),
) {
    let mut current: HashMap<Entity, Interaction> = HashMap::new();
    for (entity, interaction) in query.iter() {
        let prev_interaction = prev.get(&entity).copied().unwrap_or(Interaction::None);
        if *interaction == Interaction::Pressed && prev_interaction != Interaction::Pressed {
            on_pressed(entity);
        }
        current.insert(entity, *interaction);
    }
    **prev = current;
}

// =====================================================================
// Tooltip primitive (Phase 4: extracted from shipbuilding_tooltip.rs
// and construction/tooltip.rs. Generic request-driven tooltip with
// 250 ms latency, viewport clamping, cursor mirroring, and the
// `CANARY_ROOT_TOP_PX` coordinate-frame guard.)
// =====================================================================

/// Color tone for a tooltip stat row. Maps to a `bevy::Color` via
/// [`tone_color`] (or callers can map to egui `Color32` if needed).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TooltipTone {
    Neutral,
    Positive,
    Warning,
    Negative,
    Accent,
    Muted,
}

/// A single line / paragraph / spacer inside a tooltip body.
#[derive(Clone, Debug)]
pub enum TooltipEntry {
    Paragraph(String),
    Stat {
        label: String,
        value: String,
        tone: TooltipTone,
    },
    Spacer,
}

/// The data any consumer pushes into [`TooltipRequest`] to display
/// a tooltip. The title is rendered as the tooltip header; entries
/// populate the body in order.
#[derive(Clone, Debug, Default)]
pub struct TooltipContent {
    pub title: String,
    pub entries: Vec<TooltipEntry>,
}

/// Single source of truth for the active tooltip. Any system that
/// wants a tooltip writes here; [`tick_tooltip`] reads it each frame.
///
/// `Option` semantics: `None` means "no tooltip this frame" (the
/// `tick_tooltip` system hides the overlay in that case). Callers
/// should set this to `Some` while hovering and `None` when the
/// hover ends.
#[derive(Resource, Default)]
pub struct TooltipRequest {
    pub content: Option<TooltipContent>,
    /// Seconds since the hover started. Populated by the consumer
    /// (typically via the time-since-hover-start tracker). `tick_tooltip`
    /// hides the overlay until this exceeds `HOVER_LATENCY_SECS`.
    pub hover_started_at: Option<f32>,
}

/// Singleton marker on the tooltip overlay root. Spawned once at
/// startup (typically by the panel's `setup_X` function) and
/// positioned by `tick_tooltip` each frame.
#[derive(Component)]
pub struct TooltipOverlay;

/// Marker on the title text node of the tooltip overlay.
#[derive(Component)]
pub struct TooltipTitle;

/// Marker on the body container (a `Node` whose `Children` are
/// re-spawned every time the content changes).
#[derive(Component)]
pub struct TooltipBody;

/// 250 ms hover latency (matches shipbuilding's GRA-17 tooltip).
/// Consumers can override this in their own `tick_tooltip` if needed.
pub const TOOLTIP_HOVER_LATENCY_SECS: f32 = 0.25;

/// Map a [`TooltipTone`] to a [`bevy::Color`]. Default palette mirrors
/// the shipbuilding native tooltip; callers can swap this for their
/// own theme if needed.
pub fn tone_color(tone: TooltipTone) -> Color {
    match tone {
        TooltipTone::Neutral => Color::srgb(0.831, 0.890, 0.937), // TEXT_LIGHT-ish
        TooltipTone::Positive => Color::srgb(0.15, 1.00, 0.35),   // STATUS_SUCCESS
        TooltipTone::Warning => Color::srgb(1.00, 0.75, 0.20),    // STATUS_WARNING
        TooltipTone::Negative => Color::srgb(1.00, 0.30, 0.30),    // STATUS_DANGER
        TooltipTone::Accent => Color::srgb(0.37, 0.78, 0.85),     // CYAN
        TooltipTone::Muted => Color::srgb(0.50, 0.58, 0.66),
    }
}

/// Re-spawn the tooltip body's children to reflect `content`. The
/// caller is responsible for clearing any stale children first.
///
/// Mirrors `populate_native_tooltip_body` from shipbuilding but uses
/// the generic [`TooltipEntry`] enum.
pub fn populate_tooltip_body(
    commands: &mut Commands,
    body_entity: Entity,
    body_children: &Query<&Children>,
    content: &TooltipContent,
) {
    if let Ok(children) = body_children.get(body_entity) {
        for child in children.iter() {
            commands.entity(child).despawn();
        }
    }
    commands.entity(body_entity).with_children(|parent| {
        for entry in &content.entries {
            match entry {
                TooltipEntry::Paragraph(text) => {
                    parent.spawn((
                        Text::new(text.clone()),
                        TextFont { font_size: 10.0, ..default() },
                        TextColor(tone_color(TooltipTone::Muted)),
                    ));
                }
                TooltipEntry::Stat { label, value, tone } => {
                    parent
                        .spawn(Node {
                            width: Val::Percent(100.0),
                            flex_direction: FlexDirection::Row,
                            column_gap: Val::Px(6.0),
                            ..default()
                        })
                        .with_children(|row| {
                            row.spawn((
                                Text::new(format!("{}:", label)),
                                TextFont { font_size: 10.5, ..default() },
                                TextColor(tone_color(TooltipTone::Neutral)),
                            ));
                            row.spawn((
                                Text::new(value.clone()),
                                TextFont { font_size: 10.0, ..default() },
                                TextColor(tone_color(*tone)),
                            ));
                        });
                }
                TooltipEntry::Spacer => {
                    parent.spawn(Node {
                        width: Val::Px(1.0),
                        height: Val::Px(4.0),
                        ..default()
                    });
                }
            }
        }
    });
}

/// Per-frame driver for the cursor-following tooltip overlay.
///
/// Reads `TooltipRequest`, applies `TOOLTIP_HOVER_LATENCY_SECS` latency,
/// positions the overlay next to the cursor with viewport clamping,
/// and re-spawns the body children to reflect the requested content.
///
/// `top_offset_px` is subtracted from the cursor's Y to translate
/// window-space coords into the overlay's parent's content-area
/// coords. The construction canary uses `126.0` (the AppBar's height)
/// because the canary root is anchored at `top: 126` (see
/// `setup_construction`). Pass `0.0` for an overlay that is a
/// direct child of the window (no anchored parent).
pub fn tick_tooltip(
    mut commands: Commands,
    time: Res<Time>,
    primary_window: Query<&Window, With<PrimaryWindow>>,
    request: Res<TooltipRequest>,
    mut overlay_node: Single<&mut Node, With<TooltipOverlay>>,
    mut title_text: Single<&mut Text, (With<TooltipTitle>, Without<TooltipBody>)>,
    body_children: Query<&Children>,
    body_entity_q: Single<Entity, (With<TooltipBody>, Without<TooltipTitle>)>,
    top_offset_px: f32,
) {
    // No request this frame: hide the overlay.
    let Some(content) = &request.content else {
        overlay_node.display = Display::None;
        return;
    };

    // Latency gate: don't show until the cursor has hovered long enough.
    let elapsed = request
        .hover_started_at
        .map(|t| time.elapsed_secs() - t)
        .unwrap_or(0.0);
    if elapsed < TOOLTIP_HOVER_LATENCY_SECS {
        overlay_node.display = Display::None;
        return;
    }

    // Need the cursor and the primary window for positioning.
    let Ok(window) = primary_window.single() else {
        overlay_node.display = Display::None;
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        overlay_node.display = Display::None;
        return;
    };

    // Position the overlay next to the cursor with viewport clamping.
    // Standard tooltip offset: 16 px right, 12 px down.
    const CURSOR_OFFSET_X: f32 = 16.0;
    const CURSOR_OFFSET_Y: f32 = 12.0;
    // Standard tooltip size; consumers can override by resizing the
    // overlay Node directly after spawning.
    const TOOLTIP_W: f32 = 240.0;
    const TOOLTIP_H: f32 = 48.0;
    let max_left = (window.width() - TOOLTIP_W).max(0.0);
    let max_top = (window.height() - top_offset_px - 72.0 - TOOLTIP_H).max(0.0);
    overlay_node.left = Val::Px((cursor.x + CURSOR_OFFSET_X).clamp(0.0, max_left));
    overlay_node.top = Val::Px(
        (cursor.y - top_offset_px + CURSOR_OFFSET_Y).clamp(0.0, max_top),
    );
    overlay_node.display = Display::Flex;

    // Title text.
    **title_text = Text::new(content.title.clone());

    // Body: re-spawn children to reflect new content.
    populate_tooltip_body(&mut commands, *body_entity_q, &body_children, content);
}

// =====================================================================
// Scrollbar primitive (Phase 5: extracted from construction/scrollbar.rs.
// Generalised so shipbuilding + any other panel can adopt it. Per-track
// metrics stored as a Component (no more singleton resource bug).)
// =====================================================================

use bevy::picking::events::{Press, Release};
use bevy::picking::pointer::PointerButton;
use bevy::ui::RelativeCursorPosition;

/// Component on the scrollbar track entity. Holds the entity whose
/// `ScrollPosition` this scrollbar drives.
#[derive(Component, Clone, Copy)]
pub struct ScrollbarTrack {
    /// The scrollable body (an `Overflow::scroll_y` container with
    /// `ScrollPosition`).
    pub target: Entity,
}

/// Component on the scrollbar thumb (the draggable part inside the track).
#[derive(Component)]
pub struct ScrollbarThumb;

/// Per-track scrollbar metrics, stored on the track entity (not as
/// a singleton Resource). Fixes the latent bug where two simultaneously
/// visible tracks would clobber each other's measurements.
#[derive(Component, Default, Debug, Clone, Copy)]
pub struct ScrollbarMetrics {
    pub usable_track_height: f32,
    pub thumb_height: f32,
    pub max_scroll: f32,
}

/// Drag state for the currently-active scrollbar drag. One shared
/// Resource (drag is mouse-global, not per-track).
#[derive(Resource, Default)]
pub struct ScrollbarDragState {
    pub active: bool,
    pub started_on_track: bool,
    pub press_track_y: f32,
}

// On-press observer for the thumb.
pub fn on_thumb_press(
    on: On<Pointer<Press>>,
    mut drag: ResMut<ScrollbarDragState>,
) {
    if on.event.button != PointerButton::Primary {
        return;
    }
    drag.active = true;
    drag.started_on_track = false;
    drag.press_track_y = 0.0;
}

// On-release observer for the thumb.
pub fn on_thumb_release(
    on: On<Pointer<Release>>,
    mut drag: ResMut<ScrollbarDragState>,
) {
    if on.event.button != PointerButton::Primary {
        return;
    }
    drag.active = false;
    drag.started_on_track = false;
}

// On-press observer for the track (jump-to-position).
pub fn on_track_press(
    on: On<Pointer<Press>>,
    mut drag: ResMut<ScrollbarDragState>,
    track_query: Query<&RelativeCursorPosition, With<ScrollbarTrack>>,
) {
    if on.event.button != PointerButton::Primary {
        return;
    }
    drag.active = true;
    drag.started_on_track = true;
    let y = track_query
        .get(on.entity)
        .ok()
        .and_then(|rcp| rcp.normalized)
        .map(|n| n.y)
        .unwrap_or(0.0);
    drag.press_track_y = y;
}

// On-release observer for the track.
pub fn on_track_release(
    on: On<Pointer<Release>>,
    mut drag: ResMut<ScrollbarDragState>,
) {
    if on.event.button != PointerButton::Primary {
        return;
    }
    drag.active = false;
    drag.started_on_track = false;
}

/// Spawn the always-visible vertical scrollbar (track + thumb + observers)
/// parented to a panel root, aimed at a specific scrollable body.
///
/// `track_top_px` / `track_bottom_px` set the vertical extent of the
/// track (relative to the parent root).
pub fn spawn_scrollbar(
    commands: &mut Commands,
    root: Entity,
    target: Entity,
    track_name: &'static str,
    track_top_px: f32,
    track_bottom_px: f32,
) {
    let scrollbar_track = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(2.0),
                top: Val::Px(track_top_px),
                bottom: Val::Px(track_bottom_px),
                width: Val::Px(12.0),
                border_radius: BorderRadius::all(Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.06)),
            ZIndex(10),
            Pickable::default(),
            RelativeCursorPosition::default(),
            ScrollbarMetrics::default(),
            ScrollbarTrack { target },
            Name::new(track_name),
        ))
        .id();
    commands.entity(root).add_child(scrollbar_track);
    commands.entity(scrollbar_track).observe(on_track_press);
    commands.entity(scrollbar_track).observe(on_track_release);

    let scrollbar_thumb = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                height: Val::Px(0.0),
                border_radius: BorderRadius::all(Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.37, 0.78, 0.85, 0.6)), // CYAN.with_alpha(0.6)
            ZIndex(11),
            Pickable::default(),
            ScrollbarThumb,
            Name::new("scrollbar_thumb"),
        ))
        .id();
    commands.entity(scrollbar_track).add_child(scrollbar_thumb);
    commands.entity(scrollbar_thumb).observe(on_thumb_press);
    commands.entity(scrollbar_thumb).observe(on_thumb_release);
}

/// Per-frame scrollbar driver: computes the thumb's height + vertical
/// position from the target's `ScrollPosition`. Per-track metrics
/// stored on the track entity (no singleton Resource).
pub fn tick_scrollbar(
    track_query: Query<(Entity, &ComputedNode, &ScrollbarTrack)>,
    scrollable_query: Query<&ComputedNode>,
    pos_query: Query<&ScrollPosition>,
    mut metrics_query: Query<&mut ScrollbarMetrics>,
    children_query: Query<&Children>,
    mut thumb_query: Query<(Entity, &mut Node), With<ScrollbarThumb>>,
) {
    // Per-track scoping: each track has its own metrics + thumb. We
    // index thumbs by their parent track so the loop body only
    // updates the thumb that belongs to the current track.
    let mut thumb_for_track: std::collections::HashMap<Entity, Entity> =
        std::collections::HashMap::new();
    for (thumb_entity, _) in thumb_query.iter() {
        if let Ok(children) = children_query.get(thumb_entity) {
            for parent in children.iter() {
                thumb_for_track.insert(parent, thumb_entity);
            }
        }
    }

    for (track_entity, track_computed, track) in track_query.iter() {
        let Ok(mut metrics) = metrics_query.get_mut(track_entity) else {
            continue;
        };
        let Ok(target_computed) = scrollable_query.get(track.target) else {
            continue;
        };
        let Ok(scroll_pos) = pos_query.get(track.target) else {
            continue;
        };
        let track_height = track_computed.size().y;
        let target_size = target_computed.size();
        let target_content = target_computed.content_size().y;
        let usable_track = (track_height - 16.0).max(0.0); // 8 px top + 8 px bottom padding
        if target_content <= target_size.y || target_size.y <= 0.0 {
            if let Some(&thumb_entity) = thumb_for_track.get(&track_entity) {
                if let Ok((_, mut node)) = thumb_query.get_mut(thumb_entity) {
                    node.height = Val::Px(0.0);
                    node.top = Val::Px(0.0);
                }
            }
            metrics.usable_track_height = usable_track;
            metrics.thumb_height = 0.0;
            metrics.max_scroll = 0.0;
            continue;
        }
        let ratio = (target_size.y / target_content).clamp(0.0, 1.0);
        let thumb_height = (usable_track * ratio).max(12.0);
        let max_scroll_y = (target_content - target_size.y).max(0.0);
        let scroll_progress = if max_scroll_y > 0.0 {
            (scroll_pos.y / max_scroll_y).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let thumb_y = scroll_progress * (usable_track - thumb_height);
        metrics.usable_track_height = usable_track;
        metrics.thumb_height = thumb_height;
        metrics.max_scroll = max_scroll_y;
        if let Some(&thumb_entity) = thumb_for_track.get(&track_entity) {
            if let Ok((_, mut node)) = thumb_query.get_mut(thumb_entity) {
                node.height = Val::Px(thumb_height);
                node.top = Val::Px(thumb_y);
            }
        }
    }
}

/// Wheel-scroll handler for `Overflow::scroll_y` containers. Walks
/// the hover map from the cursor's deepest hit up to the root looking
/// for either:
///   1. A `ScrollbarTrack` (jump-scroll to the track's target).
///   2. A `Node` whose `overflow.y` is `OverflowAxis::Scroll` (scroll it).
///
/// If neither is found, the wheel event is ignored. No construction-
/// specific fallbacks; any panel can register this system without
/// further wiring.
pub fn tick_ui_scroll_on_wheel(
    mut wheel_events: MessageReader<bevy::input::mouse::MouseWheel>,
    hover_map: Res<bevy::picking::hover::HoverMap>,
    nodes: Query<&bevy::ui::ComputedNode>,
    mut scrollables: Query<&mut ScrollPosition>,
    parents: Query<&bevy::ecs::hierarchy::ChildOf>,
    tracks: Query<&ScrollbarTrack>,
) {
    use bevy::picking::pointer::PointerId;

    for event in wheel_events.read() {
        if event.y == 0.0 {
            continue;
        }
        let pointer_id = PointerId::Mouse;
        let Some(hovered_entities) = hover_map.0.get(&pointer_id) else {
            continue;
        };
        if hovered_entities.is_empty() {
            continue;
        }
        let start_entity = *hovered_entities
            .keys()
            .next()
            .expect("non-empty checked above");
        let mut scrollable: Option<Entity> = None;
        if let Ok(track) = tracks.get(start_entity) {
            scrollable = Some(track.target);
        } else {
            let mut cursor = start_entity;
            for _ in 0..10 {
                if let Ok(track) = tracks.get(cursor) {
                    scrollable = Some(track.target);
                    break;
                }
                let Ok(parent) = parents.get(cursor) else { break; };
                cursor = parent.0;
            }
        }
        if let Some(scrollable_entity) = scrollable {
            if let Ok(mut pos) = scrollables.get_mut(scrollable_entity) {
                if let Ok(computed) = nodes.get(scrollable_entity) {
                    let max_y = (computed.content_size().y - computed.size().y).max(0.0);
                    let new_y = (pos.y - event.y * 24.0).clamp(0.0, max_y);
                    pos.y = new_y;
                }
            }
        }
    }
}

// =====================================================================
// KeyedList reconciler (Phase 6: extracts the hand-rolled Local<HashMap>
// reconcilers that previously lived in construction/{buildings,dropdown,
// overview}.rs. Tracks a K -> Entity map; on each call, given a new
// set of live keys, despawns orphans and leaves existing rows alive
// so per-row mutation systems can update them in place.)
// =====================================================================

/// Tracks a `(K -> Entity)` map for a bevy_ui list view.
///
/// Usage pattern (per frame):
/// ```ignore
/// fn my_system(mut list: Local<KeyedList<MyKey>>, mut commands: Commands, ...) {
///     // 1. Compute the live key set from your data source.
///     let live_keys: Vec<MyKey> = ...;
///     // 2. Reconcile: despawn orphans + spawn missing.
///     list.reconcile(&mut commands, parent, &live_keys, |commands, parent, key| {
///         spawn_my_row(commands, parent, key)
///     });
///     // 3. Per-row updates (background, label, etc.) for both existing
/// //    and freshly-spawned rows.
///     for (key, &entity) in list.iter() {
///         ...
///     }
/// }
/// ```
///
/// The struct wraps a `HashMap<K, Entity>` and offers helpers that
/// are the canonical "spawn missing + despawn orphans" pattern. The
/// per-row mutation step is left to the caller — every list view
/// has its own visual update strategy.
pub struct KeyedList<K: std::hash::Hash + Eq, V = Entity> {
    map: std::collections::HashMap<K, V>,
}

impl<K: std::hash::Hash + Eq, V> Default for KeyedList<K, V> {
    fn default() -> Self {
        Self {
            map: std::collections::HashMap::new(),
        }
    }
}

/// Implemented by types that can tell `KeyedList::reconcile` which
/// entity to despawn when the key goes missing. `Entity` is the
/// trivial case; structs that store their own row entity can
/// implement this trait inline.
pub trait EntityContainer {
    fn entity(&self) -> Entity;
}

impl EntityContainer for Entity {
    fn entity(&self) -> Entity {
        *self
    }
}

impl<K: std::hash::Hash + Eq + Copy, V: Copy> KeyedList<K, V> {
    /// Iterate over `(key, value)` pairs in arbitrary order.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.map.iter()
    }

    /// Length of the tracked list.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Is the list empty?
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Look up the value for a key.
    pub fn get(&self, key: &K) -> Option<V> {
        self.map.get(key).copied()
    }

    /// Direct mutable access to the underlying map (for advanced
    /// callers that need to insert / remove outside the reconcile
    /// flow).
    pub fn map_mut(&mut self) -> &mut std::collections::HashMap<K, V> {
        &mut self.map
    }

    /// Drain the entire map, returning all `(K, V)` pairs.
    /// Use this for full-clear paths (e.g. "the menu closed, drop
    /// every row") where the caller will iterate and despawn.
    pub fn drain(&mut self) -> impl Iterator<Item = (K, V)> + '_ {
        self.map.drain()
    }

    /// Clear all entries without returning them. Faster than
    /// `drain().for_each(drop)` when the caller doesn't need the
    /// keys (e.g. the caller uses `commands.entity(...).despawn()`).
    pub fn clear(&mut self) {
        self.map.clear();
    }

    /// Reconcile the tracked list against a new set of live keys.
    ///
    /// - Keys that are in the map but NOT in `live_keys` are despawned
    ///   via `commands.entity(...).try_despawn()` and dropped from the map.
    /// - Keys that are in `live_keys` but NOT in the map are spawned
    ///   by calling `spawn_fn(commands, parent, key)` and inserted
    ///   into the map.
    /// - Keys that are in both are left untouched (the caller can
    ///   iterate `self.iter()` after to update their visual state).
    ///
    /// Only available for `V = Entity` (the common case where the
    /// value IS the row entity). For richer row types, callers can
    /// implement their own reconcile loop or store the row entity
    /// separately and query the rest via components / Children.
    pub fn reconcile<F>(
        &mut self,
        commands: &mut Commands,
        parent: Entity,
        live_keys: &[K],
        spawn_fn: F,
    ) where
        V: EntityContainer,
        F: Fn(&mut Commands, Entity, K) -> V,
    {
        let live_set: std::collections::HashSet<K> = live_keys.iter().copied().collect();
        // Despawn orphans.
        let to_remove: Vec<K> = self
            .map
            .keys()
            .filter(|k| !live_set.contains(k))
            .copied()
            .collect();
        for key in to_remove {
            if let Some(value) = self.map.remove(&key) {
                commands.entity(value.entity()).try_despawn();
            }
        }
        // Spawn missing.
        for key in live_keys {
            if self.map.contains_key(key) {
                continue;
            }
            let value = spawn_fn(commands, parent, *key);
            self.map.insert(*key, value);
        }
    }
}

// =====================================================================
// Tab body visibility (Phase 9: extracted from
// construction/mod.rs::tick_construction_body_visibility. Generic over
// the active-tab index resource + per-body marker, so any panel
// (construction, shipbuilding) can drive its tab bodies from a
// single system.)
// =====================================================================

/// Generic tab-body visibility system. Callers pass:
/// - the `active_index` value (typically read from a Resource like
///   `ConstructionUiState::selected_tab` or a dedicated `ActiveTabs`)
/// - a closure `body_matches(index)` that returns true for the body
///   entity that should be visible
///
/// For each `TabBody { index, group }` entity, sets Display::None
/// (and Visibility::Hidden) when `body_matches(index) == false`,
/// Display::Flex (and Visibility::Inherited) when true.
///
/// Panel-specific subsets (e.g. "show chrome on Build + Buildings
/// only") are intentionally not handled here — panels compose this
/// primitive with their own subset filters in their plugin module.
pub fn tick_tab_body_visibility<B, F>(
    mut bodies: Query<(&B, &mut Node, &mut Visibility)>,
    active_index: usize,
    body_matches: F,
) where
    B: Component,
    F: Fn(&B) -> bool,
{
    for (kind, mut node, mut visibility) in bodies.iter_mut() {
        let visible = body_matches(kind);
        node.display = if visible { Display::Flex } else { Display::None };
        *visibility = if visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

// =====================================================================
// Generic marquee primitive (Phase 10: extracted from
// construction/disabled.rs::tick_subtitle_marquee. Animate a
// horizontally-overflowing text track so the visible window
// scrolls left at a constant rate when the underlying text is wider
// than the clip container.)
// =====================================================================

/// Generic marquee track. Spawned with two copies of the text
/// side-by-side (no gap) — the second copy seamlessly takes over
/// when the first scrolls past the visible window.
#[derive(Component)]
pub struct Marquee {
    /// Entity of the first text copy (used for overflow measurement).
    pub text_node: Entity,
    /// Entity of the clip container (used for visible-width
    /// measurement).
    pub clip_container: Entity,
    /// Most recent text-content width (px).
    pub text_width: f32,
    /// Most recent clip-container width (px).
    pub container_width: f32,
    /// Current pixel offset of the track (leftward drift).
    pub phase: f32,
    /// Drift rate in pixels per second.
    pub rate_pixels_per_sec: f32,
}

/// Per-frame marquee driver. Reads `ComputedNode` on the text + clip
/// entities, advances `phase` while the text overflows, and writes
/// the offset back to the track's `UiTransform.translation.x`.
pub fn tick_marquee(
    time: Res<Time>,
    text_computed: Query<&ComputedNode>,
    mut tracks: Query<(Entity, &mut Marquee, &mut UiTransform)>,
) {
    let dt = time.delta_secs().min(0.1);
    for (_track_entity, mut marquee, mut ui_transform) in tracks.iter_mut() {
        let text_width = text_computed
            .get(marquee.text_node)
            .map(|c| c.content_size().x)
            .unwrap_or(0.0);
        let container_width = text_computed
            .get(marquee.clip_container)
            .map(|c| c.size().x)
            .unwrap_or(0.0);
        marquee.text_width = text_width;
        marquee.container_width = container_width;

        let overflows = text_width > container_width + 0.5;
        if overflows {
            marquee.phase += dt * marquee.rate_pixels_per_sec;
            if marquee.phase >= text_width {
                marquee.phase -= text_width;
            }
        }

        let dx = -marquee.phase;
        ui_transform.translation = Val2::px(dx, 0.0);
    }
}

// =====================================================================
// Progress fill primitive (Phase 10: the queue-row progress bar
// pattern shared between `QueuePanelRow` (sidebar queue list) and
// `OverviewQueueRow` (Overview tab). A `ProgressFill` Component
// holds the percentage; `tick_progress_fill` writes the width
// every frame.)
// =====================================================================

/// Marker on a foreground fill node whose width encodes a 0.0–1.0
/// progress value. Companion to a track node (just a `BackgroundColor`).
#[derive(Component)]
pub struct ProgressFill(pub f32);

/// Per-frame driver: writes `Node.width = Val::Percent(p * 100%)`
/// for every ProgressFill entity. Cheap; the system only writes the
/// field so layout reflow happens once per row per frame.
pub fn tick_progress_fill(mut q: Query<(&ProgressFill, &mut Node)>) {
    for (fill, mut node) in q.iter_mut() {
        let pct = fill.0.clamp(0.0, 1.0);
        node.width = Val::Percent(pct * 100.0);
    }
}
