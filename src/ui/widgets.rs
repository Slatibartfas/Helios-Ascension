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

use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::prelude::*;

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
}
