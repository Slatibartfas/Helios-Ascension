//! Buildings tab body — list of constructed buildings for the active colony.

use bevy::prelude::*;

use super::data::{category_from_index, parse_category};
use super::demolish::{spawn_demolish_button, DemolishMultiplierSource};
use super::markers::*;
use super::scrollbar::spawn_construction_scrollbar;
use super::state::*;
use crate::colony::types::BuildingCategory;
use crate::colony::types::BuildingType;
use crate::ui::bevy_theme::*;
use crate::ui::widgets::UiFonts;

// Build the **Buildings** body. A persistent container with a header
// + a content scroll area.
pub fn spawn_buildings_body(
    commands: &mut Commands,
    parent: Entity,
    body_font_medium: &Handle<Font>,
) {
    let body = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                width: Val::Percent(100.0),
                flex_grow: 1.0,
                min_height: Val::Px(0.0),
                padding: UiRect::all(Val::Px(SPACE_LG)),
                row_gap: Val::Px(SPACE_SM),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
            ConstructionTabBody::Buildings,
            Visibility::Hidden,
            Name::new("buildings_body"),
        ))
        .id();
    commands.entity(parent).add_child(body);

    let header = commands
        .spawn((
            Text::new("Constructed Buildings"),
            TextFont {
                font: body_font_medium.clone(),
                font_size: BODY_SIZE,
                ..default()
            },
            TextColor(CYAN),
            Name::new("buildings_header"),
            BuildingsHeader,
        ))
        .id();
    commands.entity(body).add_child(header);

    let content = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                align_content: AlignContent::Start,
                align_items: AlignItems::Stretch,
                width: Val::Percent(100.0),
                flex_grow: 1.0,
                min_height: Val::Px(0.0),
                row_gap: Val::Px(SPACE_LG),
                column_gap: Val::Px(SPACE_LG),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
            Visibility::Inherited,
            Name::new("buildings_content"),
            BuildingsContent,
        ))
        .id();
    commands.entity(body).add_child(content);

    spawn_construction_scrollbar(
        commands,
        parent,
        content,
        "buildings_scrollbar_track",
        138.0_f32,
        SPACE_SM,
        ConstructionTabBody::Buildings,
    );
}

// Marker on the Buildings body's header text.
#[derive(Component)]
pub struct BuildingsHeader;

// Marker on the Buildings body's content container.
#[derive(Component)]
pub struct BuildingsContent;

// Update the Buildings body every frame.
//
// Spawn-once-update-many: cards persist across frames.
pub fn update_buildings_body(
    mut commands: Commands,
    fonts: Res<UiFonts>,
    ui_state: Res<ConstructionUiState>,
    buildings_data: Res<crate::colony::data::BuildingsData>,
    building_icons: Option<Res<BuildingIcons>>,
    resource_icons: Option<Res<crate::ui::resource_icons::ResourceIcons>>,
    colonies: Query<(Entity, &crate::colony::Colony)>,
    content_query: Query<Entity, With<BuildingsContent>>,
    mut spawned_cards: Local<crate::ui::widgets::KeyedList<BuildingType>>,
    mut header_query: Query<&mut Text, With<BuildingsHeader>>,
    mut empty_placeholder: Local<Option<Entity>>,
    mut no_colony_placeholder: Local<Option<Entity>>,
    mut last_colony: Local<Option<bevy::ecs::entity::Entity>>,
    mut last_buildings_sig: Local<Option<u64>>,
) {
    let Ok(content) = content_query.single() else {
        return;
    };

    let body_font: Handle<Font> = fonts.body.clone();
    let body_font_medium: Handle<Font> = fonts.medium.clone();
    let mono_font: Handle<Font> = fonts.mono.clone();

    let colony = ui_state
        .selected_colony
        .and_then(|e| colonies.iter().find(|(ce, _)| *ce == e));

    let header_text = match &colony {
        Some((_, c)) => format!("Constructed Buildings ({})", c.buildings.len()),
        None => "Constructed Buildings".to_string(),
    };
    for mut text in header_query.iter_mut() {
        **text = header_text.clone();
    }

    let empty_resource_icons = crate::ui::resource_icons::ResourceIcons::default();
    let resource_icons: &crate::ui::resource_icons::ResourceIcons = resource_icons
        .as_ref()
        .map(|r: &Res<crate::ui::resource_icons::ResourceIcons>| -> &crate::ui::resource_icons::ResourceIcons { r.as_ref() })
        .unwrap_or(&empty_resource_icons);

    fn spawn_no_colony_placeholder(
        commands: &mut Commands,
        content: Entity,
        body_font: Handle<Font>,
        existing: Option<Entity>,
    ) -> Option<Entity> {
        if let Some(p) = existing {
            if commands.get_entity(p).is_ok() {
                return Some(p);
            }
        }
        let placeholder = commands
            .spawn((
                Text::new("(no colony selected)"),
                TextFont {
                    font: body_font,
                    font_size: BODY_SIZE,
                    ..default()
                },
                TextColor(TEXT_DIM),
                Name::new("buildings_no_colony"),
            ))
            .id();
        commands.entity(content).add_child(placeholder);
        Some(placeholder)
    }

    fn spawn_empty_placeholder(
        commands: &mut Commands,
        content: Entity,
        body_font: Handle<Font>,
        existing: Option<Entity>,
    ) -> Option<Entity> {
        if let Some(p) = existing {
            if commands.get_entity(p).is_ok() {
                return Some(p);
            }
        }
        let placeholder = commands
            .spawn((
                Text::new(
                    "No buildings yet. Switch to the Build tab to queue your first structure.",
                ),
                TextFont {
                    font: body_font,
                    font_size: BODY_SIZE,
                    ..default()
                },
                TextColor(TEXT_DIM),
                Name::new("buildings_empty"),
            ))
            .id();
        commands.entity(content).add_child(placeholder);
        Some(placeholder)
    }

    let Some((_, colony)) = colony else {
        for (_, card_entity) in spawned_cards.drain() {
            commands.entity(card_entity).try_despawn();
        }
        if let Some(p) = no_colony_placeholder.take() {
            commands.entity(p).try_despawn();
        }
        if let Some(p) = empty_placeholder.take() {
            commands.entity(p).try_despawn();
        }
        *last_colony = None;
        *no_colony_placeholder =
            spawn_no_colony_placeholder(&mut commands, content, body_font.clone(), None);
        return;
    };

    if *last_colony != ui_state.selected_colony {
        for (_, card_entity) in spawned_cards.drain() {
            commands.entity(card_entity).try_despawn();
        }
        *last_colony = ui_state.selected_colony;
        *last_buildings_sig = None;
    }

    let mut sig: u64 = 0xcbf29ce484222325;
    for (bt, count) in colony.buildings.iter() {
        sig ^= *bt as u64;
        sig = sig.wrapping_mul(0x100000001b3);
        sig ^= *count as u64;
        sig = sig.wrapping_mul(0x100000001b3);
    }
    if *last_buildings_sig != Some(sig) {
        for (_, card_entity) in spawned_cards.drain() {
            commands.entity(card_entity).try_despawn();
        }
        *last_buildings_sig = Some(sig);
    }

    if colony.buildings.is_empty() {
        for (_, card_entity) in spawned_cards.drain() {
            commands.entity(card_entity).try_despawn();
        }
        if let Some(p) = no_colony_placeholder.take() {
            commands.entity(p).try_despawn();
        }
        if let Some(p) = empty_placeholder.take() {
            commands.entity(p).try_despawn();
        }
        *empty_placeholder =
            spawn_empty_placeholder(&mut commands, content, body_font.clone(), None);
        return;
    }

    if let Some(p) = no_colony_placeholder.take() {
        commands.entity(p).try_despawn();
    }
    if let Some(p) = empty_placeholder.take() {
        commands.entity(p).try_despawn();
    }

    let active_category = category_from_index(ui_state.selected_build_tab);
    let filtered: Vec<(BuildingType, u32)> = colony
        .buildings
        .iter()
        .filter(|(bt, _count)| {
            let def = buildings_data.get(bt);
            let cat = def.and_then(|d| parse_category(d.category.as_str()));
            if cat == Some(BuildingCategory::Mining) {
                return false;
            }
            match active_category {
                Some(c) => cat == Some(c),
                None => true,
            }
        })
        .map(|(bt, count)| (*bt, *count))
        .collect();

    let mut entries: Vec<BuildingType> = filtered.iter().map(|(bt, _)| *bt).collect::<Vec<_>>();
    entries.sort_by(|a, b| {
        let an = buildings_data
            .get(a)
            .map(|d| d.display_name.as_str())
            .unwrap_or("");
        let bn = buildings_data
            .get(b)
            .map(|d| d.display_name.as_str())
            .unwrap_or("");
        an.cmp(bn)
    });

    let multiplier = ui_state.build_multiplier;
    spawned_cards.reconcile(
        &mut commands,
        content,
        &entries,
        |commands, parent, building_type| {
            let Some(def) = buildings_data.get(&building_type) else {
                return Entity::PLACEHOLDER;
            };
            let icon_handle = building_icons
                .as_ref()
                .and_then(|bi| bi.handles.get(&building_type).cloned());
            let count = colony.buildings.get(&building_type).copied().unwrap_or(0);
            super::cards::spawn_constructed_card(
                commands,
                parent,
                building_type,
                def,
                count,
                multiplier,
                &body_font,
                &body_font_medium,
                &mono_font,
                icon_handle.as_ref(),
                resource_icons,
            )
        },
    );
}
