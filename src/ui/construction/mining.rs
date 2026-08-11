//! Mining tab body — per-resource extraction cards with [-] [+] buttons.
//!
//! Construction on the currently selected body — moon, planet,
//! asteroid, etc. There is no orbital section: orbital mining
//! (the legacy `Auto*Mine` buildings) is not exposed here and
//! will be reintroduced later via space stations rather than as
//! a duplicate mining grid.

use bevy::prelude::*;

use crate::ui::bevy_theme::*;
use super::cards::spawn_card;
use super::data::{
    apply_effect_cap, compute_mining_card_data, friendly_label, BuildCardData, MiningCardData,
};
use super::demolish::{spawn_demolish_button, DemolishMultiplierSource};
use super::markers::*;
use super::scrollbar::spawn_construction_scrollbar;
use super::state::{
    BuildingIcons, ConstructionTabBody, MiningGroupId,
    MINING_GROUPS_SURFACE,
};
use crate::ui::widgets::{spawn_scrollable_container, UiFonts};
use crate::colony::types::BuildingType;
use crate::economy::PlanetResources;
use crate::plugins::solar_system::CelestialBody;
use crate::plugins::solar_system_data::BodyType;

// Build the **Mining** body.
pub fn spawn_mining_body(commands: &mut Commands, parent: Entity) {
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
            ConstructionTabBody::Mining,
            Visibility::Hidden,
            Name::new("mining_body"),
        ))
        .id();
    commands.entity(parent).add_child(body);

    let content = spawn_scrollable_container(
        commands,
        body,
        "mining_content",
        SPACE_XS,
        MiningContent,
    );

    spawn_construction_scrollbar(
        commands,
        parent,
        content,
        "mining_scrollbar_track",
        98.0_f32,
        SPACE_SM,
        ConstructionTabBody::Mining,
    );
}

// Update the Mining tab body. Re-spawns the cards inside the
// `MiningContent` container every time it runs.
#[allow(clippy::too_many_arguments)]
pub fn update_mining_body(
    mut commands: Commands,
    fonts: Res<UiFonts>,
    ui_state: Res<super::state::ConstructionUiState>,
    buildings_data: Res<crate::colony::data::BuildingsData>,
    resource_icons: Option<Res<crate::ui::resource_icons::ResourceIcons>>,
    building_icons: Option<Res<BuildingIcons>>,
    colonies: Query<(Entity, &crate::colony::Colony)>,
    body_query: Query<(
        &CelestialBody,
        Option<&crate::astronomy::components::AtmosphereComposition>,
        Option<&crate::economy::PlanetResources>,
    )>,
    content_query: Query<Entity, With<MiningContent>>,
    mut spawned_rows: Local<Vec<Entity>>,
    mut last_fingerprint: Local<Option<MiningBodyFingerprint>>,
) {
    use crate::economy::budget::calculate_colony_power_totals;

    let Ok(content) = content_query.single() else {
        return;
    };

    if ui_state.selected_tab != super::state::ConstructionTab::Mining {
        return;
    }

    let active_colony_entity = ui_state.selected_colony;
    let colony_data: Option<(
        String,
        bool,
        Option<BodyType>,
        Option<&crate::economy::PlanetResources>,
        std::collections::HashMap<BuildingType, u32>,
    )> = active_colony_entity.and_then(|e| {
        colonies.get(e).ok().and_then(|(_, c)| {
            body_query.get(e).ok().map(|(body, atmo_opt, res_opt)| {
                let name = format!("{} (colony)", body.name);
                let breathable = atmo_opt.map(|a| a.breathable);
                (
                    name,
                    breathable.unwrap_or(false),
                    Some(body.body_type),
                    res_opt,
                    c.buildings.clone(),
                )
            })
        })
    });

    let fingerprint = match &colony_data {
        Some((_, breathable, body_type, planet_resources, counts)) => MiningBodyFingerprint {
            colony_entity: active_colony_entity,
            multiplier: ui_state.build_multiplier,
            collapsed: ui_state.mining_groups_collapsed.clone(),
            counts: counts.clone(),
            breathable: *breathable,
            body_type: *body_type,
            deposit_sig: mining_deposit_signature(*planet_resources),
        },
        None => MiningBodyFingerprint::no_colony(&ui_state),
    };
    if last_fingerprint.as_ref() == Some(&fingerprint) {
        return;
    }
    *last_fingerprint = Some(fingerprint);

    for entity in spawned_rows.drain(..) {
        commands.entity(entity).try_despawn();
    }

    let body_font: Handle<Font> = fonts.body.clone();
    let body_font_medium: Handle<Font> = fonts.medium.clone();
    let mono_font: Handle<Font> = fonts.mono.clone();
    let multiplier = ui_state.build_multiplier;
    let empty_resource_icons = crate::ui::resource_icons::ResourceIcons::default();
    let resource_icons: &crate::ui::resource_icons::ResourceIcons = resource_icons
        .as_ref()
        .map(|r: &Res<crate::ui::resource_icons::ResourceIcons>| -> &crate::ui::resource_icons::ResourceIcons { r.as_ref() })
        .unwrap_or(&empty_resource_icons);

    let spare_power_mw: f64 = if let Some(e) = active_colony_entity {
        colonies
            .get(e)
            .ok()
            .map(|(_, c)| calculate_colony_power_totals(c, Some(&buildings_data)))
            .map(|totals| (totals.produced_watts - totals.consumed_watts) / 1_000_000.0)
            .unwrap_or(f64::NAN)
    } else {
        f64::NAN
    };

    let Some((_colony_name, body_breathable, body_type, planet_resources, building_counts)) =
        colony_data
    else {
        let placeholder = commands
            .spawn((
                Text::new("(no colony selected)"),
                TextFont {
                    font: body_font.clone(),
                    font_size: BODY_SIZE,
                    ..default()
                },
                TextColor(TEXT_DIM),
                Name::new("mining_no_colony"),
            ))
            .id();
        commands.entity(content).add_child(placeholder);
        spawned_rows.push(placeholder);
        return;
    };

    let empty_building_icons = BuildingIcons::default();
    let building_icons_ref: &BuildingIcons = building_icons
        .as_ref()
        .map(|r: &Res<BuildingIcons>| -> &BuildingIcons { r.as_ref() })
        .unwrap_or(&empty_building_icons);

    for (group_id, group_label, group_buildings) in MINING_GROUPS_SURFACE {
        let group_collapsed = ui_state.mining_groups_collapsed.contains(group_id);
        let group_node = spawn_mining_group_section(
            &mut commands,
            content,
            *group_id,
            group_label,
            group_buildings,
            group_collapsed,
            body_breathable,
            body_type,
            planet_resources,
            &buildings_data,
            &building_counts,
            &body_font,
            &body_font_medium,
            &mono_font,
            multiplier,
            &resource_icons,
            building_icons_ref,
            spare_power_mw,
        );
        spawned_rows.push(group_node);
    }
}

// Build a `BuildCardData` for a mine / AutoMine.
#[allow(clippy::too_many_arguments)]
pub fn build_mine_card_data(
    bt: BuildingType,
    def: &crate::colony::data::BuildingDefinition,
    count: u32,
    body_breathable: bool,
    body_type: Option<BodyType>,
    planet_resources: Option<&crate::economy::PlanetResources>,
    multiplier: u32,
    spare_power_mw: f64,
) -> BuildCardData {
    let mult = multiplier.max(1) as f64;
    let card_data = compute_mining_card_data(def, planet_resources);
    let body_blocked =
        !crate::colony::data::building_is_available_on(def, Some(body_breathable), body_type);

    let current_rate = card_data.production_mt_per_year(count);
    let count_label = if current_rate > 0.0 {
        format!(
            "\u{00d7}{} \u{2502} {}",
            count,
            super::data::format_mining_rate(current_rate)
        )
    } else {
        format!("\u{00d7}{}", count)
    };
    let acc_label = if card_data.accessibility > 0.0 {
        format!("Acc: {:.0}%", card_data.accessibility * 100.0)
    } else {
        "Acc: -".to_string()
    };
    let bp_str = count_label;
    let cost_str = acc_label;

    let mut effects: Vec<(super::data::EffectTone, String)> = Vec::new();
    let power_chip = super::data::build_power_chip_data(
        def,
        mult,
        if spare_power_mw.is_nan() {
            None
        } else {
            Some(spare_power_mw)
        },
    );
    let reserve_label = if card_data.reserve_mt > 0.0 {
        format!(
            "Available Deposits: {}",
            super::data::format_mining_reserve(card_data.reserve_mt)
        )
    } else if planet_resources.is_none() {
        "Survey the body to see deposits".to_string()
    } else {
        "no deposit".to_string()
    };
    effects.push((super::data::EffectTone::Neutral, reserve_label));
    for m in def.modifiers.iter() {
        if m.modifier_type == "BuildPointsProduction" {
            if let Some((tone, label)) = friendly_label(m) {
                effects.push((tone, label));
            }
            continue;
        }
        if m.modifier_type.ends_with("Production") {
            continue;
        }
        if let Some((tone, label)) = friendly_label(m) {
            effects.push((tone, label));
        }
    }
    apply_effect_cap(&mut effects);

    let mut resource_costs: Vec<super::data::ResourceCostRow> = Vec::new();
    for (name, amt) in def.resource_costs.iter().take(6) {
        let total = amt * mult;
        resource_costs.push(super::data::ResourceCostRow {
            name: name.clone(),
            amount: total,
            resource: crate::colony::data::parse_resource_type(name),
        });
    }
    if body_blocked {
        let gate_label = match def.allowed_body_types.first() {
            Some(bt_value) => format!("\u{26a0} body - requires {:?}", bt_value),
            None => "\u{26a0} body - unavailable".to_string(),
        };
        effects.push((super::data::EffectTone::Negative, gate_label));
    }

    let power_insufficient = body_blocked || power_chip.insufficient;

    BuildCardData {
        name: def.display_name.clone(),
        subtitle: super::data::clamp_subtitle_two_lines(&def.description),
        building_type: bt,
        icon: def.icon.clone(),
        multiplier: multiplier.max(1),
        stat_a: ("\u{00d7}N", bp_str),
        stat_b: ("ACC", cost_str),
        stat_c: ("", String::new()),
        effects,
        resource_costs,
        power_chip,
        queue_label: if multiplier > 1 {
            format!("Build \u{002b}{}", multiplier)
        } else {
            "Build +1".to_string()
        },
        build_points: def.build_points,
        power_insufficient,
        body_blocked,
        constructed: false,
    }
}

// Spawn a single mine / AutoMine card.
#[allow(clippy::too_many_arguments)]
fn spawn_mining_card(
    commands: &mut Commands,
    parent: Entity,
    bt: BuildingType,
    body_breathable: bool,
    body_type: Option<BodyType>,
    planet_resources: Option<&crate::economy::PlanetResources>,
    buildings_data: &crate::colony::data::BuildingsData,
    building_counts: &std::collections::HashMap<BuildingType, u32>,
    body_font: &Handle<Font>,
    body_font_medium: &Handle<Font>,
    mono_font: &Handle<Font>,
    icon: Option<&Handle<Image>>,
    multiplier: u32,
    resource_icons: &crate::ui::resource_icons::ResourceIcons,
    spare_power_mw: f64,
) -> Entity {
    let def = match buildings_data.get(&bt) {
        Some(d) => d,
        None => {
            let card = commands
                .spawn((
                    Node {
                        display: Display::Flex,
                        flex_direction: FlexDirection::Column,
                        width: Val::Px(320.0),
                        min_height: Val::Px(320.0),
                        padding: UiRect::all(Val::Px(SPACE_LG)),
                        border: UiRect::all(Val::Px(1.0)),
                        border_radius: BorderRadius::all(Val::Px(8.0)),
                        ..default()
                    },
                    BackgroundColor(CARD_BG),
                    BorderColor::all(CARD_BORDER),
                    MiningCard { building_type: bt },
                    Name::new("mining_card_unknown"),
                ))
                .id();
            commands.entity(parent).add_child(card);
            return card;
        }
    };

    let count = building_counts.get(&bt).copied().unwrap_or(0);
    let data = build_mine_card_data(
        bt,
        def,
        count,
        body_breathable,
        body_type,
        planet_resources,
        multiplier,
        spare_power_mw,
    );

    let card = spawn_card(
        commands,
        parent,
        &data,
        bt,
        body_font,
        body_font_medium,
        mono_font,
        icon,
        resource_icons,
    );

    spawn_demolish_button(
        commands,
        card,
        bt,
        count,
        multiplier,
        body_font_medium,
        DemolishMultiplierSource::Mining,
    );

    card
}

// Spawn one surface group section (header + collapsible body of
// cards).
#[allow(clippy::too_many_arguments)]
fn spawn_mining_group_section(
    commands: &mut Commands,
    parent: Entity,
    group_id: MiningGroupId,
    group_label: &str,
    group_buildings: &[BuildingType],
    collapsed: bool,
    body_breathable: bool,
    body_type: Option<BodyType>,
    planet_resources: Option<&crate::economy::PlanetResources>,
    buildings_data: &crate::colony::data::BuildingsData,
    building_counts: &std::collections::HashMap<BuildingType, u32>,
    body_font: &Handle<Font>,
    body_font_medium: &Handle<Font>,
    mono_font: &Handle<Font>,
    multiplier: u32,
    resource_icons: &crate::ui::resource_icons::ResourceIcons,
    building_icons: &BuildingIcons,
    spare_power_mw: f64,
) -> Entity {
    let group_container = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                width: Val::Percent(100.0),
                row_gap: Val::Px(SPACE_XS),
                padding: UiRect::all(Val::Px(SPACE_SM)),
                ..default()
            },
            BackgroundColor(CARD_BG),
            Name::new(format!("mining_group_{:?}", group_id)),
        ))
        .id();
    commands.entity(parent).add_child(group_container);

    let header = commands
        .spawn((
            Button,
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(SPACE_SM),
                padding: UiRect::all(Val::Px(SPACE_XS)),
                width: Val::Percent(100.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
            MiningGroupHeader { group_id },
            Name::new("mining_group_header"),
        ))
        .id();
    commands.entity(group_container).add_child(header);

    let chevron = if collapsed { "▶" } else { "▼" };
    let chevron_text = commands
        .spawn((
            Text::new(chevron),
            TextFont {
                font: body_font.clone(),
                font_size: BODY_SIZE,
                ..default()
            },
            TextColor(CYAN),
            Name::new("mining_group_chevron"),
        ))
        .id();
    commands.entity(header).add_child(chevron_text);

    let label_text = commands
        .spawn((
            Text::new(format!("{} ({})", group_label, group_buildings.len())),
            TextFont {
                font: body_font.clone(),
                font_size: SECTION_SIZE,
                ..default()
            },
            TextColor(CYAN),
            Node {
                flex_grow: 1.0,
                ..default()
            },
            Name::new("mining_group_label"),
        ))
        .id();
    commands.entity(header).add_child(label_text);

    let body_node = commands
        .spawn((
            Node {
                display: if collapsed {
                    Display::None
                } else {
                    Display::Flex
                },
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                align_items: AlignItems::FlexStart,
                column_gap: Val::Px(SPACE_SM),
                row_gap: Val::Px(SPACE_SM),
                width: Val::Percent(100.0),
                padding: UiRect::all(Val::Px(SPACE_XS)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
            MiningGroupBody { group_id },
            Name::new("mining_group_body"),
        ))
        .id();
    commands.entity(group_container).add_child(body_node);

    if !collapsed {
        for bt in group_buildings {
            let icon_handle: Option<&Handle<Image>> = building_icons.handles.get(bt);
            spawn_mining_card(
                commands,
                body_node,
                *bt,
                body_breathable,
                body_type,
                planet_resources,
                buildings_data,
                building_counts,
                body_font,
                body_font_medium,
                mono_font,
                icon_handle,
                multiplier,
                resource_icons,
                spare_power_mw,
            );
        }
    }

    group_container
}

// Fingerprint of the inputs that determine the Mining body's card content.
#[derive(PartialEq)]
pub struct MiningBodyFingerprint {
    colony_entity: Option<bevy::ecs::entity::Entity>,
    multiplier: u32,
    collapsed: std::collections::HashSet<MiningGroupId>,
    counts: std::collections::HashMap<BuildingType, u32>,
    breathable: bool,
    body_type: Option<BodyType>,
    deposit_sig: u64,
}

impl MiningBodyFingerprint {
    fn no_colony(ui_state: &super::state::ConstructionUiState) -> Self {
        Self {
            colony_entity: None,
            multiplier: ui_state.build_multiplier,
            collapsed: ui_state.mining_groups_collapsed.clone(),
            counts: std::collections::HashMap::new(),
            breathable: false,
            body_type: None,
            deposit_sig: 0,
        }
    }
}

fn mining_deposit_signature(resources: Option<&crate::economy::PlanetResources>) -> u64 {
    let Some(res) = resources else {
        return 0;
    };
    let mut h: u64 = 0xcbf29ce484222325;
    for (rt, dep) in res.deposits.iter() {
        h ^= *rt as u64;
        h = h.wrapping_mul(0x100000001b3);
        let bits = dep.accessibility.to_bits() as u64;
        h ^= bits;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

// Group-visibility toggle.
pub fn tick_mining_group_visibility(
    mut ui_state: ResMut<super::state::ConstructionUiState>,
    headers: Query<(Entity, &Interaction, &MiningGroupHeader), With<Button>>,
    mut prev: Local<std::collections::HashMap<Entity, Interaction>>,
) {
    crate::ui::widgets::detect_rising_edges(&mut prev, &headers, |_entity, header| {
        let id = header.group_id;
        if ui_state.mining_groups_collapsed.contains(&id) {
            ui_state.mining_groups_collapsed.remove(&id);
        } else {
            ui_state.mining_groups_collapsed.insert(id);
        }
    });
}
