//! Phase 12B iteration 2: native bevy_ui top bar chrome.
//!
//! Visual parity target: matches the egui top bar's resource tiles
//! (per-category group with hover-popup per-resource breakdown)
//! without yet replacing it. The egui version stays in front until
//! the full migration lands; this commit is purely a "can we make
//! this look as good as egui?" experiment.
//!
//! ## What's new vs the MVP scaffold (commit cd26527)
//! - Bigger tiles (96 px wide) so labels fit on one line.
//! - A real resource icon (representative resource) per tile.
//! - Per-tile `Pickable` + hover observer that writes a
//!   `TooltipRequest` listing the resources in the category.
//! - The per-frame text update now writes both count and rate.
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
use crate::ui::resource_icons::{get_resource_icon_handle_bevy, ResourceIcons};
use crate::ui::widgets::{
    spawn_text_label, TooltipContent, TooltipEntry, TooltipRequest, TooltipTone, UiFonts,
};

const TOP_BAR_HEIGHT: f32 = 48.0;
const TILE_WIDTH: f32 = 96.0;
const TILE_PADDING: f32 = 6.0;

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
    resource_icons: Option<Res<ResourceIcons>>,
) {
    if !root_query.is_empty() {
        return;
    }
    let Some(fonts) = ui_fonts else {
        return;
    };
    let empty_icons = ResourceIcons::default();
    let icons: &ResourceIcons = resource_icons.as_deref().unwrap_or(&empty_icons);

    let root = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(0.0),
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

    for (idx, (category_name, resources)) in ResourceType::by_category().into_iter().enumerate() {
        let tint = bevy_theme::category_color(category_name);
        let representative = resources.first().copied().unwrap_or(ResourceType::Food);

        let tile = commands
            .spawn((
                Button,
                Node {
                    width: Val::Px(TILE_WIDTH),
                    height: Val::Px(TOP_BAR_HEIGHT - 2.0 * TILE_PADDING),
                    display: Display::Flex,
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    padding: UiRect::all(Val::Px(4.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(4.0)),
                    column_gap: Val::Px(4.0),
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

        // Representative resource icon (or tinted placeholder square).
        let icon_entity = match get_resource_icon_handle_bevy(icons, representative) {
            Some(handle) => commands
                .spawn((
                    ImageNode::new(handle.clone()).with_color(tint),
                    Node {
                        width: Val::Px(20.0),
                        height: Val::Px(20.0),
                        ..default()
                    },
                    Name::new("top_bar_tile_icon"),
                ))
                .id(),
            None => commands
                .spawn((
                    Node {
                        width: Val::Px(20.0),
                        height: Val::Px(20.0),
                        border_radius: BorderRadius::all(Val::Px(2.0)),
                        ..default()
                    },
                    BackgroundColor(tint.with_alpha(0.85)),
                    Name::new("top_bar_tile_icon_placeholder"),
                ))
                .id(),
        };
        commands.entity(tile).add_child(icon_entity);

        // Category label
        let label = spawn_text_label(
            &mut commands,
            tile,
            category_name,
            fonts.medium.clone(),
            9.0,
            Color::srgba(0.831, 0.890, 0.937, 1.0),
            (),
        );
        commands.entity(label).insert(Node {
            flex_grow: 1.0,
            ..default()
        });

        // Count text node (will be updated each frame)
        let count_text = commands
            .spawn((
                Text::new("..."),
                TextFont {
                    font: fonts.mono.clone(),
                    font_size: 11.0,
                    ..default()
                },
                TextColor(tint),
                Node {
                    width: Val::Px(0.0), // auto-size
                    ..default()
                },
                Name::new("top_bar_count"),
                BevyUiTopBarCountText {
                    category: category_name,
                },
            ))
            .id();
        commands.entity(tile).add_child(count_text);

        // Hover observers: write the per-resource tooltip on Over,
        // clear it on Out. Uses the shared `TooltipRequest` resource
        // + `tick_tooltip` system for the latency / clamping logic.
        commands.entity(tile).observe(on_tile_hover);
        commands.entity(tile).observe(on_tile_hover_out);
    }
}

/// Hover observer: build a `TooltipContent` listing every resource
/// in the hovered tile's category. The rate column is filled in by
/// `update_bevy_ui_top_bar` per frame so the player sees current
/// production rates without waiting for a tooltip rebuild.
fn on_tile_hover(
    on: On<Pointer<Over>>,
    mut tooltip: ResMut<TooltipRequest>,
    tile_query: Query<&BevyUiTopBarTile>,
) {
    let Ok(tile) = tile_query.get(on.entity) else {
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
}

/// Hover-out observer: clears the tooltip when the pointer leaves.
fn on_tile_hover_out(
    on: On<Pointer<Out>>,
    mut tooltip: ResMut<TooltipRequest>,
    tile_query: Query<&BevyUiTopBarTile>,
) {
    let Ok(tile) = tile_query.get(on.entity) else {
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
}

/// Per-frame update: writes the contextual stockpile count + rate
/// for each tile's category. Also refreshes the live tooltip
/// entries so the rate column stays current while hovering.
pub fn update_bevy_ui_top_bar(
    contextual: Res<ContextualStockpile>,
    rate_tracker: Option<Res<ResourceRateTracker>>,
    _budget: Res<GlobalBudget>,
    mut text_query: Query<(&mut Text, &BevyUiTopBarCountText)>,
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