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
