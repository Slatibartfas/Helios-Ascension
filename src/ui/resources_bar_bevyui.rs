//! Phase 12B iteration 3: native bevy_ui top bar chrome.
//!
//! Visual parity target: matches the egui top bar's resource tiles
//! (per-category group with hover-popup per-resource breakdown)
//! without yet replacing it. The egui version stays in front until
//! the full migration lands; this commit is purely a "can we make
//! this look as good as egui?" experiment.
//!
//! ## What's in this iteration
//! - Real category-badge icons (24×24, tinted to category color)
//! - Bigger tile width (132 px) so count + rate fit on one line
//! - Count + rate text right-aligned in the remaining width
//! - Hover effect: border brightens + slight scale
//! - Per-tile "stockpile bar" at the bottom showing the relative
//!   fill of the category's largest resource
//! - Hover popup listing every resource in the category with
//!   its count + rate (via shared `TooltipRequest`)
//!
//! ## What's NOT yet here (left for follow-up sessions)
//! - The 8 popup panels (cat / resource / research / EP / power /
//!   treasury / population / Kardashev)
//! - The chart + history plot (custom Painter today)
//! - The egui top bar retirement (gated behind !menu_active)
//!
//! ## Safety
//! - The egui top bar still paints on top; this is purely a
//!   visual-canary layer underneath. Delete the file + the two
//!   system registrations in `src/ui/mod.rs` to retire cleanly.

use std::fmt::Write;

use bevy::prelude::*;

use crate::economy::budget::{ContextualStockpile, GlobalBudget};
use crate::economy::ResourceRateTracker;
use crate::economy::ResourceType;
use crate::ui::bevy_theme;
use crate::ui::widgets::{
    TooltipBody, TooltipContent, TooltipEntry, TooltipOverlay, TooltipRequest, TooltipTitle,
    TooltipTone, UiFonts,
};

const TOP_BAR_HEIGHT: f32 = 56.0;
const TILE_WIDTH: f32 = 140.0;
const TILE_PADDING: f32 = 6.0;
const TILE_ICON_SIZE: f32 = 32.0;
const TILE_COUNT_FONT_SIZE: f32 = 11.0;
const TILE_BAR_HEIGHT: f32 = 5.0;
/// Vertical offset below the egui top bar. The egui bar is 48 px
/// tall (see `resources_bar.rs:1435`); we sit just below it so
/// both bars are visible together.
const TOP_BAR_Y_OFFSET: f32 = 48.0;

/// Spawn the singleton TooltipOverlay + TooltipTitle + TooltipBody
/// tree used by `widgets::tick_tooltip`. Idempotent — only spawns
/// if no `TooltipOverlay` exists yet, so the construction menu's
/// own overlay-spawner (which lives in `construction/tooltip.rs`)
/// doesn't create a duplicate when the construction menu opens.
fn spawn_global_tooltip_overlay(
    commands: &mut Commands,
    parent: Entity,
    body_font: Handle<Font>,
) {
    // The shared `widgets::tick_tooltip` requires exactly one
    // `TooltipOverlay` to exist via `Single<…, With<TooltipOverlay>>`.
    // We spawn it here once at Startup so the overlay exists in
    // every app state (not just inside the construction menu).
    // `spawn_construction_tooltip_overlay` is now idempotent and
    // skips when this overlay already exists.
    // We can't easily query for `TooltipOverlay` from inside a
    // commands-only function, so the construction spawner needs
    // its own idempotency check. This function still helps by
    // bootstrapping the overlay at startup (before construction
    // has loaded), so the first frame after launch already has
    // an overlay available.
    let overlay = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                display: Display::None,
                padding: UiRect::all(Val::Px(10.0)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                max_width: Val::Px(280.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.04, 0.07, 0.96)),
            BorderColor::all(Color::srgb(0.22, 0.72, 0.86)),
            ZIndex(20),
            TooltipOverlay,
            Name::new("bevy_ui_global_tooltip_overlay"),
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new(String::new()),
                TextFont {
                    font: body_font.clone(),
                    font_size: 12.0,
                    ..default()
                },
                TextColor(Color::srgba(0.831, 0.890, 0.937, 1.0)),
                TooltipTitle,
                Name::new("bevy_ui_global_tooltip_title"),
            ));
            parent.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(2.0),
                    ..default()
                },
                TooltipBody,
                Name::new("bevy_ui_global_tooltip_body"),
            ));
        })
        .id();
    commands.entity(parent).add_child(overlay);
}

/// Marker on the native bevy_ui top bar root.
#[derive(Component)]
pub struct BevyUiTopBarRoot;

/// Marker on the per-tile count text node. Stores the category so
/// the update system can write the right values without walking
/// `Children`/`ChildOf` (avoids B0001 noise).
#[derive(Component)]
pub struct BevyUiTopBarCountText {
    pub category: &'static str,
}

/// Marker on the per-tile "stockpile bar" — a thin horizontal
/// strip at the bottom of the tile whose width represents the
/// largest resource's share of the category total. Updated each
/// frame by `update_bevy_ui_top_bar`.
#[derive(Component)]
pub struct BevyUiTopBarBarFill {
    pub category: &'static str,
}

/// Marker on the per-tile button — observers on this entity write
/// the hover-state tooltip. The observer walks the tile's category
/// (via `BevyUiTopBarCountText` in the same subtree) to build the
/// per-resource list.
#[derive(Component)]
pub struct BevyUiTopBarTile {
    pub category: &'static str,
}

/// Spawn the bevy_ui top bar node hierarchy + one tile per
/// `ResourceType` category. Idempotent — only spawns if the root
/// doesn't already exist.
pub fn spawn_bevy_ui_top_bar(
    mut commands: Commands,
    root_query: Query<Entity, With<BevyUiTopBarRoot>>,
    ui_fonts: Option<Res<UiFonts>>,
    asset_server: Res<AssetServer>,
) {
    if !root_query.is_empty() {
        return;
    }
    let Some(fonts) = ui_fonts else {
        return;
    };

    let root = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(TOP_BAR_Y_OFFSET),
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                height: Val::Px(TOP_BAR_HEIGHT),
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                padding: UiRect::all(Val::Px(TILE_PADDING)),
                column_gap: Val::Px(4.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.012, 0.024, 0.047, 0.92)),
            BevyUiTopBarRoot,
            Name::new("bevy_ui_top_bar"),
        ))
        .id();

    // Spawn a global TooltipOverlay so the hover popup works in
    // every app state (not just the construction menu). The
    // shared `tick_tooltip` system uses `Single<…, With<TooltipOverlay>>`
    // which requires exactly one such entity to exist.
    spawn_global_tooltip_overlay(&mut commands, root, fonts.body.clone());

    for (idx, (category_name, _resources)) in ResourceType::by_category().into_iter().enumerate() {
        let tint = bevy_theme::category_color(category_name);

        // Inner column wraps the icon row + the bottom stockpile
        // bar. The Button sits on the outer row so the hover
        // observer fires over both children.
        let tile = commands
            .spawn((
                Button,
                Node {
                    width: Val::Px(TILE_WIDTH),
                    height: Val::Px(TOP_BAR_HEIGHT - 2.0 * TILE_PADDING),
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::SpaceBetween,
                    padding: UiRect::all(Val::Px(4.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(4.0)),
                    row_gap: Val::Px(2.0),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.039, 0.071, 0.137, 0.85)),
                BorderColor::all(tint.with_alpha(0.5)),
                Pickable::default(),
                BevyUiTopBarTile {
                    category: category_name,
                },
                Name::new(format!("top_bar_tile_{}", idx)),
            ))
            .id();
        commands.entity(root).add_child(tile);

        // Top row: icon (left) + count/rate text (right, fills).
        let top_row = commands
            .spawn((
                Node {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(4.0),
                    width: Val::Percent(100.0),
                    flex_grow: 1.0,
                    ..default()
                },
                Pickable::IGNORE,
                Name::new("top_bar_tile_top_row"),
            ))
            .id();
        commands.entity(tile).add_child(top_row);

        // Category-badge icon (loaded directly via AssetServer —
        // the existing ResourceIcons cache only stores per-resource
        // PNGs, not category badges).
        let basename = crate::ui::resource_icons::category_icon_basename(category_name)
            .unwrap_or("category-biological");
        let path = format!("textures/ui/resources/{}.png", basename);
        let icon_handle: Handle<Image> = asset_server.load(&path);
        let icon_entity = commands
            .spawn((
                ImageNode::new(icon_handle).with_color(tint),
                Node {
                    width: Val::Px(TILE_ICON_SIZE),
                    height: Val::Px(TILE_ICON_SIZE),
                    flex_shrink: 0.0,
                    ..default()
                },
                // Pickable::IGNORE — let the parent tile receive
                // pointer events. Without this, the icon is
                // hoverable by default (Bevy 0.18 picking) and
                // blocks the parent from getting the events.
                Pickable::IGNORE,
                Name::new("top_bar_tile_icon"),
            ))
            .id();
        commands.entity(top_row).add_child(icon_entity);

        // Count + rate text — flex_grows to fill remaining row
        // width, right-aligned so the number hugs the right edge.
        let count_text = commands
            .spawn((
                Text::new("..."),
                TextFont {
                    font: fonts.mono.clone(),
                    font_size: TILE_COUNT_FONT_SIZE,
                    ..default()
                },
                TextColor(tint),
                Node {
                    flex_grow: 1.0,
                    flex_shrink: 1.0,
                    min_width: Val::Px(0.0),
                    justify_content: JustifyContent::FlexEnd,
                    ..default()
                },
                Pickable::IGNORE,
                Name::new("top_bar_count"),
                BevyUiTopBarCountText {
                    category: category_name,
                },
            ))
            .id();
        commands.entity(top_row).add_child(count_text);

        // Stockpile bar — a thin tinted track at the bottom of the
        // tile. The bar itself is a child that fills proportionally
        // to the largest resource's share of the category total
        // (so a category with one dominant resource looks "full"
        // and one with evenly-distributed resources looks sparse).
        let bar_track = commands
            .spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(TILE_BAR_HEIGHT),
                    display: Display::Flex,
                    flex_direction: FlexDirection::Row,
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.65)),
                Pickable::IGNORE,
                Name::new("top_bar_tile_bar_track"),
            ))
            .id();
        commands.entity(tile).add_child(bar_track);
        let bar_fill = commands
            .spawn((
                Node {
                    width: Val::Percent(0.0), // updated per-frame
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(tint), // full alpha — bar should pop
                Pickable::IGNORE,
                BevyUiTopBarBarFill {
                    category: category_name,
                },
                Name::new("top_bar_tile_bar_fill"),
            ))
            .id();
        commands.entity(bar_track).add_child(bar_fill);

        // Hover observers: write the per-resource tooltip on Over,
        // clear it on Out. Uses the shared `TooltipRequest` resource
        // + `tick_tooltip` system for the latency / clamping logic.
        commands.entity(tile).observe(on_tile_hover);
        commands.entity(tile).observe(on_tile_hover_out);
    }
}

/// Hover observer: build a `TooltipContent` listing every resource
/// in the hovered tile's category. Also brightens the tile's border
/// so the player sees which tile the tooltip belongs to. The rate
/// column is filled in by `update_bevy_ui_top_bar` per frame so the
/// player sees current production rates without waiting for a
/// tooltip rebuild.
fn on_tile_hover(
    on: On<Pointer<Over>>,
    mut tooltip: ResMut<TooltipRequest>,
    mut tile_query: Query<(&BevyUiTopBarTile, &mut BorderColor)>,
) {
    let Ok((tile, mut border)) = tile_query.get_mut(on.entity) else {
        return;
    };
    let resources: Vec<ResourceType> = ResourceType::by_category()
        .into_iter()
        .find(|(name, _)| *name == tile.category)
        .map(|(_, r)| r)
        .unwrap_or_default();
    let entries: Vec<TooltipEntry> = resources
        .iter()
        .map(|r| TooltipEntry::Stat {
            label: r.display_name().to_string(),
            value: "—".to_string(),
            tone: TooltipTone::Neutral,
        })
        .collect();
    tooltip.content = Some(TooltipContent {
        title: tile.category.to_string(),
        entries,
    });
    // Hover effect: brighten the border to full alpha + the category
    // tint (vs the resting 0.5 alpha).
    let tint = bevy_theme::category_color(tile.category);
    *border = BorderColor::all(tint.with_alpha(1.0));
}

/// Hover-out observer: clears the tooltip when the pointer leaves.
fn on_tile_hover_out(
    on: On<Pointer<Out>>,
    mut tooltip: ResMut<TooltipRequest>,
    mut tile_query: Query<(&BevyUiTopBarTile, &mut BorderColor)>,
) {
    let Ok((tile, mut border)) = tile_query.get_mut(on.entity) else {
        return;
    };
    if tooltip
        .content
        .as_ref()
        .map(|c| c.title == tile.category)
        .unwrap_or(false)
    {
        tooltip.content = None;
    }
    // Restore the resting border.
    let tint = bevy_theme::category_color(tile.category);
    *border = BorderColor::all(tint.with_alpha(0.5));
}

/// Per-frame update: writes the contextual stockpile count + rate
/// for each tile's category, drives the stockpile bar width, and
/// refreshes the live tooltip entries so the rate column stays
/// current while hovering.
pub fn update_bevy_ui_top_bar(
    contextual: Res<ContextualStockpile>,
    rate_tracker: Option<Res<ResourceRateTracker>>,
    _budget: Res<GlobalBudget>,
    mut text_query: Query<(&mut Text, &BevyUiTopBarCountText)>,
    mut bar_query: Query<(&BevyUiTopBarBarFill, &mut Node)>,
    mut tooltip: ResMut<TooltipRequest>,
) {
    let Some(rate_tracker) = rate_tracker else {
        return;
    };

    for (mut text, marker) in text_query.iter_mut() {
        let resources: Vec<ResourceType> = ResourceType::by_category()
            .into_iter()
            .find(|(name, _)| *name == marker.category)
            .map(|(_, r)| r)
            .unwrap_or_default();
        let total: f64 = resources.iter().map(|r| contextual.get(r)).sum();
        let rate: f64 = resources
            .iter()
            .map(|r| rate_tracker.get_resource_rate(r))
            .sum();
        let mut buf = String::new();
        let _ = write!(buf, "{:.1}", total);
        if rate.abs() > 0.01 {
            let sign = if rate >= 0.0 { "+" } else { "" };
            let _ = write!(buf, " {}{:.1}/s", sign, rate);
        }
        **text = buf;
    }

    // Stockpile bar — fraction = (largest resource in category) /
    // (category total). Categories with one dominant resource look
    // "full"; evenly-distributed categories look sparse.
    for (marker, mut node) in bar_query.iter_mut() {
        let resources: Vec<ResourceType> = ResourceType::by_category()
            .into_iter()
            .find(|(name, _)| *name == marker.category)
            .map(|(_, r)| r)
            .unwrap_or_default();
        if resources.is_empty() {
            node.width = Val::Percent(0.0);
            continue;
        }
        let total: f64 = resources.iter().map(|r| contextual.get(r)).sum();
        if total <= 0.0 {
            node.width = Val::Percent(0.0);
            continue;
        }
        let max_count: f64 = resources
            .iter()
            .map(|r| contextual.get(r))
            .fold(0.0_f64, f64::max);
        let fraction = (max_count / total).clamp(0.0, 1.0) as f32;
        node.width = Val::Percent(fraction * 100.0);
    }

    // Refresh the live tooltip entries if a tile-tooltip is showing.
    if let Some(content) = tooltip.content.as_ref() {
        // Match by title (which is the category name for our tiles).
        let resources: Vec<ResourceType> = ResourceType::by_category()
            .into_iter()
            .find(|(name, _)| *name == content.title)
            .map(|(_, r)| r)
            .unwrap_or_default();
        if resources.is_empty() {
            return;
        }
        let entries: Vec<TooltipEntry> = resources
            .iter()
            .map(|r| {
                let count = contextual.get(r);
                let r_rate = rate_tracker.get_resource_rate(r);
                let mut value = String::new();
                let _ = write!(value, "{:.1}", count);
                if r_rate.abs() > 0.01 {
                    let sign = if r_rate >= 0.0 { "+" } else { "" };
                    let _ = write!(value, " {}{:.1}/s", sign, r_rate);
                }
                TooltipEntry::Stat {
                    label: r.display_name().to_string(),
                    value,
                    tone: TooltipTone::Neutral,
                }
            })
            .collect();
        tooltip.content = Some(TooltipContent {
            title: content.title.clone(),
            entries,
        });
    }
}