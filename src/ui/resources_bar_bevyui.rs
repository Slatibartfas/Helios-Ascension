//! Phase 12B MVP scaffold: native bevy_ui top bar chrome.
//!
//! This is a **foundation** for migrating the always-visible top
//! resource count strip from egui (in `src/ui/resources_bar.rs`,
//! ~4,314 LOC) to bevy_ui. The full migration is a 5–8 day effort
//! because the egui surface owns:
//! - The 8 popup panels (cat / resource / research / EP / power /
//!   treasury / population / Kardashev)
//! - A custom `Painter` chart for the Kardashev trend
//! - The per-resource Kardashev plot history
//! - The hover-state preview pips
//!
//! ## What's scaffolded here (MVP)
//! - `spawn_bevy_ui_top_bar` — startup system that spawns one tile
//!   per `ResourceType` category, with chevron + count text.
//! - `update_bevy_ui_top_bar` — per-frame update that writes the
//!   count + rate into each tile's count text.
//!
//! ## What this does NOT yet do
//! - It is not registered in `src/ui/mod.rs` yet (WIP).
//! - It does not displace the egui top bar (the egui one stays in
//!   front until the full migration lands).
//! - It does not show any of the 8 popup panels.
//! - It does not include the chart / history plot.
//!
//! ## Migration path (next sessions)
//! 1. Wire the spawn system into `Startup` and the update system
//!    into `Update` (or `EguiPrimaryContextPass` for parity).
//! 2. Migrate one popup at a time (cat → resource → research → ...).
//! 3. Once the last popup is on bevy_ui, gate the egui top bar
//!    behind `!menu_active` to retire it.

use std::fmt::Write;

use bevy::prelude::*;

use crate::economy::budget::{ContextualStockpile, GlobalBudget};
use crate::economy::ResourceRateTracker;
use crate::economy::ResourceType;
use crate::ui::bevy_theme;
use crate::ui::widgets::{spawn_text_label, UiFonts};

const TOP_BAR_HEIGHT: f32 = 48.0;
const TILE_WIDTH: f32 = 70.0;
const TILE_PADDING: f32 = 6.0;

/// Marker on the native bevy_ui top bar root. Exists so the
/// per-frame update system can find it via `Query<&mut …, With<…>>`.
#[derive(Component)]
pub struct BevyUiTopBarRoot;

/// Marker on the per-tile count text node. Stores the category so
/// the update system can write the right values without walking
/// `Children`/`ChildOf` (avoids B0001 noise).
#[derive(Component)]
pub struct BevyUiTopBarCountText {
    pub category: &'static str,
}

/// Spawn the bevy_ui top bar node hierarchy + one tile per
/// `ResourceType` category. Idempotent — only spawns if the root
/// doesn't already exist.
pub fn spawn_bevy_ui_top_bar(
    mut commands: Commands,
    root_query: Query<Entity, With<BevyUiTopBarRoot>>,
    ui_fonts: Option<Res<UiFonts>>,
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

    for (idx, (category_name, _resources)) in ResourceType::by_category().into_iter().enumerate() {
        let tint = bevy_theme::category_color(category_name);
        let tile = commands
            .spawn((
                Node {
                    width: Val::Px(TILE_WIDTH),
                    height: Val::Px(TOP_BAR_HEIGHT - 2.0 * TILE_PADDING),
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(Val::Px(4.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(4.0)),
                    row_gap: Val::Px(2.0),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.039, 0.071, 0.137, 0.6)),
                BorderColor::all(tint.with_alpha(0.5)),
                Name::new(format!("top_bar_tile_{}", idx)),
            ))
            .id();
        commands.entity(root).add_child(tile);

        // Category label (top row)
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
            width: Val::Percent(100.0),
            ..default()
        });

        // Count + rate placeholder (bottom row)
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
                    width: Val::Percent(100.0),
                    ..default()
                },
                Name::new("top_bar_count"),
                BevyUiTopBarCountText {
                    category: category_name,
                },
            ))
            .id();
        commands.entity(tile).add_child(count_text);
    }
}

/// Per-frame update: writes the contextual stockpile count + rate
/// for each tile's category. Idempotent.
pub fn update_bevy_ui_top_bar(
    contextual: Res<ContextualStockpile>,
    rate_tracker: Option<Res<ResourceRateTracker>>,
    _budget: Res<GlobalBudget>,
    mut text_query: Query<(&mut Text, &BevyUiTopBarCountText)>,
) {
    let Some(_rate_tracker) = rate_tracker else {
        return;
    };
    for (mut text, marker) in text_query.iter_mut() {
        let resources: Vec<ResourceType> = ResourceType::by_category()
            .into_iter()
            .find(|(name, _)| *name == marker.category)
            .map(|(_, r)| r)
            .unwrap_or_default();
        let total: f64 = resources.iter().map(|r| contextual.get(r)).sum();
        let mut buf = String::new();
        let _ = write!(buf, "{:.1}", total);
        **text = buf;
    }
}