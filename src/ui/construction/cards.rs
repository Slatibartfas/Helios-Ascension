//! Build card chrome — `spawn_card` (used by Build + Mining + Buildings tabs)
//! and `spawn_constructed_card` (Buildings-tab variant).

use bevy::prelude::*;

use crate::ui::bevy_theme::*;
use super::data::{clamp_subtitle_two_lines, BuildCardData};
use super::markers::*;
use super::state::*;
use super::tooltip::on_power_chip_hover_over;
use super::tooltip::on_power_chip_hover_out;
use crate::ui::widgets::{
    card_shadow, card_shadow_hover, HoverElevation, UiFonts,
};
use crate::ui::bevy_theme::HairlineBundle;
use crate::ui::resource_icons::{
    get_energy_icon_handle_bevy, get_resource_icon_handle_bevy, ResourceIcons,
};

// Spawn a single build / mine / constructed-building card.
//
// The `data` argument drives all chrome (header, stats, power chip,
// effects, ETA, resource-cost strip, CTA). When `data.constructed`
// is true (Buildings-tab cards), the CTA + ETA row are skipped
// and the caller (`spawn_constructed_card`) is expected to attach
// a Demolish button after spawn.
pub fn spawn_card(
    commands: &mut Commands,
    parent: Entity,
    data: &BuildCardData,
    building_type: crate::colony::types::BuildingType,
    body_font: &Handle<Font>,
    body_font_medium: &Handle<Font>,
    mono_font: &Handle<Font>,
    icon: Option<&Handle<Image>>,
    resource_icons: &ResourceIcons,
) -> Entity {
    let card = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                padding: UiRect {
                    top: Val::Px(SPACE_LG),
                    right: Val::Px(SPACE_LG),
                    bottom: Val::Px(if data.constructed { SPACE_LG } else { CTA_FOOTPRINT }),
                    left: Val::Px(SPACE_LG),
                },
                row_gap: Val::Px(SPACE_SM),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(8.0)),
                width: Val::Px(320.0),
                flex_shrink: 0.0,
                height: Val::Px(320.0),
                flex_grow: 0.0,
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(CARD_BG),
            BorderColor {
                top: CARD_BORDER_HIGHLIGHT,
                bottom: CARD_BORDER_SHADOW,
                left: CARD_BORDER_HIGHLIGHT,
                right: CARD_BORDER_SHADOW,
            },
            card_shadow(),
            Pickable::default(),
            Name::new("build_card"),
            ConstructionCard {
                name: data.name.clone(),
            },
            HoverElevation {
                hover_scale: Vec2::splat(1.02),
                press_scale: Vec2::splat(0.98),
                border_rest: Some(BorderColor {
                    top: CARD_BORDER_HIGHLIGHT,
                    bottom: CARD_BORDER_SHADOW,
                    left: CARD_BORDER_HIGHLIGHT,
                    right: CARD_BORDER_SHADOW,
                }),
                border_hover: Some(CYAN),
                bg: Some(CARD_BG),
                bg_hover: Some(CARD_BG_HOVER),
                shadow: Some(card_shadow()),
                shadow_hover: Some(card_shadow_hover()),
                z_lift: true,
                ..default()
            },
        ))
        .id();
    commands.entity(parent).add_child(card);

    let header_row = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(SPACE_SM),
                ..default()
            },
            Name::new("card_header_row"),
        ))
        .id();
    commands.entity(card).add_child(header_row);

    let card_icon = match icon {
        Some(handle) => commands
            .spawn((
                Node {
                    width: Val::Px(36.0),
                    height: Val::Px(36.0),
                    flex_shrink: 0.0,
                    border_radius: BorderRadius::all(Val::Px(4.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BorderColor::all(CYAN_BORDER),
                ImageNode::new(handle.clone()).with_color(CYAN),
                Name::new("card_icon"),
            ))
            .id(),
        None => commands
            .spawn((
                Node {
                    width: Val::Px(36.0),
                    height: Val::Px(36.0),
                    flex_shrink: 0.0,
                    border_radius: BorderRadius::all(Val::Px(4.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BorderColor::all(CYAN_BORDER),
                BackgroundColor(Color::srgba(0.373, 0.784, 0.847, 0.60)),
                Name::new("card_icon_placeholder"),
            ))
            .id(),
    };
    commands.entity(header_row).add_child(card_icon);

    let title_col = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                min_width: Val::Px(0.0),
                row_gap: Val::Px(SPACE_XS),
                ..default()
            },
            Name::new("card_title_col"),
        ))
        .id();
    commands.entity(header_row).add_child(title_col);

    let title = commands
        .spawn((
            Text::new(data.name.clone()),
            TextFont {
                font: body_font_medium.clone(),
                font_size: BODY_SIZE,
                ..default()
            },
            TextColor(CYAN),
            Name::new("card_title"),
        ))
        .id();
    commands.entity(title_col).add_child(title);

    let subtitle_clip = commands
        .spawn((
            Node {
                height: Val::Px(20.0),
                width: Val::Percent(100.0),
                min_width: Val::Px(0.0),
                overflow: Overflow::clip(),
                ..default()
            },
            Name::new("card_subtitle_clip"),
        ))
        .id();
    commands.entity(title_col).add_child(subtitle_clip);

    let subtitle_text_a = commands
        .spawn((
            Text::new(data.subtitle.clone()),
            TextFont {
                font: body_font.clone(),
                font_size: CAPTION_SIZE,
                ..default()
            },
            TextColor(TEXT_DIM),
            TextLayout::new_with_no_wrap(),
            Name::new("card_subtitle_text"),
        ))
        .id();

    let subtitle_text_b = commands
        .spawn((
            Text::new(data.subtitle.clone()),
            TextFont {
                font: body_font.clone(),
                font_size: CAPTION_SIZE,
                ..default()
            },
            TextColor(TEXT_DIM),
            TextLayout::new_with_no_wrap(),
            Name::new("card_subtitle_text"),
        ))
        .id();

    let subtitle_track = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                flex_shrink: 0.0,
                align_items: AlignItems::Center,
                ..default()
            },
            UiTransform::default(),
            SubtitleMarquee {
                card,
                text_node: subtitle_text_a,
                clip_container: subtitle_clip,
                text_width: 0.0,
                container_width: 0.0,
                phase: 0.0,
            },
            Name::new("card_subtitle_track"),
        ))
        .id();
    commands.entity(subtitle_clip).add_child(subtitle_track);
    commands.entity(subtitle_track).add_child(subtitle_text_a);
    commands.entity(subtitle_track).add_child(subtitle_text_b);

    let stats_row = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                width: Val::Percent(100.0),
                justify_content: JustifyContent::SpaceBetween,
                ..default()
            },
            Name::new("card_stats"),
        ))
        .id();
    commands.entity(card).add_child(stats_row);

    for (_label, value) in [&data.stat_a, &data.stat_b] {
        let stat = commands
            .spawn((
                Node {
                    align_items: AlignItems::Center,
                    ..default()
                },
                Text::new(value.clone()),
                TextFont {
                    font: mono_font.clone(),
                    font_size: CAPTION_SIZE,
                    ..default()
                },
                TextColor(CYAN),
                Name::new("card_stat"),
            ))
            .id();
        commands.entity(stats_row).add_child(stat);
    }

    let hairline = commands.spawn(HairlineBundle::default()).id();
    commands.entity(card).add_child(hairline);

    let power_chrome = YELLOW_ENERGY;
    let power_text_color = if data.power_chip.per_unit_mw < -0.01 {
        RED
    } else if data.power_chip.per_unit_mw > 0.01 {
        GREEN_FIN
    } else {
        TEXT_BODY
    };
    let sign_prefix = if data.power_chip.per_unit_mw < -0.01 {
        "-"
    } else if data.power_chip.per_unit_mw > 0.01 {
        "+"
    } else {
        ""
    };
    let power_chip_label = format!(
        "{}{}",
        sign_prefix, data.power_chip.amount
    );
    let power_chip = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(4.0),
                padding: UiRect::horizontal(Val::Px(6.0)),
                height: Val::Px(32.0),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(power_chrome.with_alpha(0.12)),
            BorderColor::all(power_chrome.with_alpha(0.35)),
            Pickable::default(),
            PowerChip {
                tooltip_lines: data.power_chip.tooltip_lines.clone(),
                tone: power_text_color,
                card,
            },
            Name::new("power_chip"),
        ))
        .id();
    commands.entity(card).add_child(power_chip);
    commands.entity(power_chip).observe(on_power_chip_hover_over);
    commands.entity(power_chip).observe(on_power_chip_hover_out);

    let power_icon_node = match get_energy_icon_handle_bevy(resource_icons) {
        Some(handle) => commands
            .spawn((
                Node {
                    width: Val::Px(24.0),
                    height: Val::Px(24.0),
                    ..default()
                },
                ImageNode::new(handle.clone()).with_color(power_chrome),
                Name::new("power_chip_icon"),
            ))
            .id(),
        None => commands
            .spawn((
                Node {
                    width: Val::Px(24.0),
                    height: Val::Px(24.0),
                    border_radius: BorderRadius::all(Val::Px(2.0)),
                    ..default()
                },
                BackgroundColor(power_chrome.with_alpha(0.85)),
                Name::new("power_chip_icon_placeholder"),
            ))
            .id(),
    };
    commands.entity(power_chip).add_child(power_icon_node);

    let label = commands
        .spawn((
            Text::new(power_chip_label),
            TextFont {
                font: mono_font.clone(),
                font_size: CAPTION_SIZE,
                ..default()
            },
            TextColor(power_text_color),
            Name::new("power_chip_label"),
        ))
        .id();
    commands.entity(power_chip).add_child(label);

    for (tone, line) in &data.effects {
        let color = match tone {
            super::data::EffectTone::Positive => GREEN_FIN,
            super::data::EffectTone::Negative => ORANGE_ORE,
            super::data::EffectTone::Neutral => TEXT_BODY,
            super::data::EffectTone::Cost => ORANGE_ORE,
            super::data::EffectTone::Throughput => GREEN_FIN,
        };
        let bullet = commands
            .spawn((
                Text::new(line.clone()),
                TextFont {
                    font: mono_font.clone(),
                    font_size: CAPTION_SIZE,
                    ..default()
                },
                TextColor(color),
                Name::new("effect_bullet"),
            ))
            .id();
        commands.entity(card).add_child(bullet);
    }

    let strip = if data.resource_costs.is_empty() {
        None
    } else {
        let s = commands
            .spawn((
                Node {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(6.0),
                    row_gap: Val::Px(4.0),
                    width: Val::Percent(100.0),
                    ..default()
                },
                Name::new("resource_cost_strip"),
            ))
            .id();
        commands.entity(card).add_child(s);
        Some(s)
    };

    for cost in &data.resource_costs {
        let category = cost
            .resource
            .map(|r| crate::ui::bevy_theme::category_color_for_resource(&r))
            .unwrap_or(TEXT_BODY);
        let amount_str = super::data::format_mining_reserve(cost.amount);

        let display_name: String = cost
            .resource
            .map(|r| r.display_name().to_string())
            .unwrap_or_else(|| cost.name.clone());
        let chip = commands
            .spawn((
                Node {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(4.0),
                    padding: UiRect::horizontal(Val::Px(6.0)),
                    height: Val::Px(28.0),
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(4.0)),
                    ..default()
                },
                BackgroundColor(category.with_alpha(0.12)),
                BorderColor::all(category.with_alpha(0.35)),
                Pickable::default(),
                ResourceCostChip {
                    name: display_name,
                    amount: amount_str.clone(),
                    category,
                    card,
                },
                Name::new("resource_cost_chip"),
            ))
            .id();
        commands.entity(strip.unwrap()).add_child(chip);
        commands.entity(chip).observe(super::tooltip::on_chip_hover_over);
        commands.entity(chip).observe(super::tooltip::on_chip_hover_out);

        let icon_node = match cost
            .resource
            .and_then(|r| get_resource_icon_handle_bevy(resource_icons, r))
        {
            Some(handle) => commands
                .spawn((
                    Node {
                        width: Val::Px(20.0),
                        height: Val::Px(20.0),
                        ..default()
                    },
                    ImageNode::new(handle.clone()).with_color(category),
                    Name::new("resource_cost_chip_icon"),
                ))
                .id(),
            None => {
                let placeholder = commands
                    .spawn((
                        Node {
                            width: Val::Px(20.0),
                            height: Val::Px(20.0),
                            border_radius: BorderRadius::all(Val::Px(2.0)),
                            ..default()
                        },
                        BackgroundColor(category.with_alpha(0.85)),
                        Name::new("resource_cost_chip_icon_placeholder"),
                    ))
                    .id();
                placeholder
            }
        };
        commands.entity(chip).add_child(icon_node);

        let label = commands
            .spawn((
                Text::new(amount_str),
                TextFont {
                    font: mono_font.clone(),
                    font_size: CAPTION_SIZE,
                    ..default()
                },
                TextColor(category),
                Name::new("resource_cost_chip_label"),
            ))
            .id();
        commands.entity(chip).add_child(label);
    }

    if !data.constructed {
        let eta_hairline = commands.spawn(HairlineBundle::default()).id();
        commands.entity(card).add_child(eta_hairline);

        let unit_bp = data.build_points;
        let batch_bp = unit_bp * data.multiplier.max(1) as f64;
        let eta_seconds = batch_bp / 12_001.0 * 365.25 * 24.0 * 3600.0;
        let eta_str = format_duration_compact(eta_seconds);
        let eta_row = commands
            .spawn((
                Node {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    width: Val::Percent(100.0),
                    padding: UiRect::horizontal(Val::Px(SPACE_XS)),
                    column_gap: Val::Px(SPACE_SM),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
                Name::new("card_eta_row"),
            ))
            .id();
        commands.entity(card).add_child(eta_row);
        let eta_label = commands
            .spawn((
                Text::new("ETA: "),
                TextFont {
                    font: mono_font.clone(),
                    font_size: CAPTION_SIZE,
                    ..default()
                },
                TextColor(TEXT_DIM),
                Name::new("card_eta_label"),
            ))
            .id();
        commands.entity(eta_row).add_child(eta_label);
        let eta_value = commands
            .spawn((
                Text::new(eta_str),
                TextFont {
                    font: mono_font.clone(),
                    font_size: CAPTION_SIZE,
                    ..default()
                },
                TextColor(YELLOW_ETA),
                Name::new("card_eta_value"),
            ))
            .id();
        commands.entity(eta_row).add_child(eta_value);
    }

    if !data.constructed {
        let cta = commands
            .spawn((
                Button,
                Node {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    align_self: AlignSelf::FlexStart,
                    height: Val::Px(32.0),
                    padding: UiRect::horizontal(Val::Px(SPACE_XL)),
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(4.0)),
                    position_type: PositionType::Absolute,
                    bottom: Val::Px(SPACE_LG),
                    left: Val::Px(SPACE_LG),
                    ..default()
                },
                BackgroundColor(CTA_FILL),
                BorderColor::all(CYAN_BORDER_STRONG),
                Name::new("card_cta"),
                ConstructionCta { building_type },
                ConstructionCtaDisabled,
                Pickable::default(),
            ))
            .id();
        if !data.power_insufficient {
            commands.entity(cta).remove::<ConstructionCtaDisabled>();
        }
        if data.body_blocked {
            commands.entity(cta).insert(ConstructionCtaBodyBlocked);
        }
        commands.entity(card).add_child(cta);

        let cta_label = commands
            .spawn((
                Text::new(data.queue_label.clone()),
                TextFont {
                    font: body_font_medium.clone(),
                    font_size: BODY_SIZE,
                    ..default()
                },
                TextColor(CYAN),
                Node {
                    flex_grow: 1.0,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    display: Display::Flex,
                    ..default()
                },
                ConstructionCtaLabelMarker,
                Name::new("card_cta_label"),
            ))
            .id();
        commands.entity(cta).add_child(cta_label);
    }

    card
}

// Build a `BuildCardData` for a building already constructed in the
// active colony.
pub fn build_constructed_card_data(
    bt: crate::colony::types::BuildingType,
    def: &crate::colony::data::BuildingDefinition,
    count: u32,
    multiplier: u32,
) -> BuildCardData {
    let _ = multiplier;

    let count_label = format!("\u{00d7}{}", count);
    let bp_str = count_label;

    let mut effects: Vec<(super::data::EffectTone, String)> = Vec::new();
    for m in def.modifiers.iter() {
        if m.modifier_type == "BuildPointsProduction" {
            if let Some((tone, label)) = super::data::friendly_label(m) {
                effects.push((tone, label));
            }
            continue;
        }
        if let Some(res_name) = m.modifier_type.strip_suffix("Production") {
            if m.value > 0.0 {
                let per_unit = m.value;
                let total = per_unit * count as f64;
                let line = if count > 1 {
                    format!(
                        "Produces {} \u{00d7} {} = {} {}",
                        super::data::format_mining_rate(per_unit),
                        count,
                        super::data::format_mining_rate(total),
                        res_name
                    )
                } else {
                    format!("Produces {} {}", super::data::format_mining_rate(per_unit), res_name)
                };
                effects.push((super::data::EffectTone::Positive, line));
            }
        } else if let Some((tone, label)) = super::data::friendly_label(m) {
            effects.push((tone, label));
        }
    }
    super::data::apply_effect_cap(&mut effects);

    let power_output_gw_per_unit: f64 = def
        .modifiers
        .iter()
        .filter(|m| m.modifier_type == "PowerGeneration")
        .map(|m| m.value)
        .sum();
    let power_chip = if power_output_gw_per_unit > 0.0 {
        let per_unit_mw = power_output_gw_per_unit * 1_000.0;
        let total_mw = per_unit_mw * count as f64;
        super::data::PowerChipData {
            verb: "Produces",
            amount: super::data::format_power(total_mw),
            per_unit_mw: total_mw,
            multiplier: count.max(1) as u32,
            spare_mw: None,
            insufficient: false,
            tooltip_lines: vec![
                format!("Total generation: {}", super::data::format_power(total_mw)),
                format!("{} built \u{00d7} {} each", count, super::data::format_power(per_unit_mw)),
                "Net surplus to the grid".to_string(),
            ],
        }
    } else if def.power_demand_mw.abs() < 0.01 {
        super::data::PowerChipData {
            verb: "Power",
            amount: "0 W".to_string(),
            per_unit_mw: 0.0,
            multiplier: count.max(1) as u32,
            spare_mw: None,
            insufficient: false,
            tooltip_lines: vec!["No grid interaction".to_string()],
        }
    } else {
        let per_unit_mw = def.power_demand_mw;
        let total_mw = per_unit_mw * count as f64;
        super::data::PowerChipData {
            verb: "Demand",
            amount: super::data::format_power(total_mw),
            per_unit_mw: -total_mw,
            multiplier: count.max(1) as u32,
            spare_mw: None,
            insufficient: false,
            tooltip_lines: vec![
                format!("Total demand: {}", super::data::format_power(total_mw)),
                format!("{} built \u{00d7} {} each", count, super::data::format_power(per_unit_mw)),
            ],
        }
    };

    BuildCardData {
        name: def.display_name.clone(),
        subtitle: clamp_subtitle_two_lines(&def.description),
        building_type: bt,
        icon: def.icon.clone(),
        multiplier: multiplier.max(1),
        stat_a: ("\u{00d7}N", bp_str),
        stat_b: ("", String::new()),
        stat_c: ("", String::new()),
        effects,
        resource_costs: Vec::new(),
        power_chip,
        queue_label: String::new(),
        build_points: 0.0,
        power_insufficient: false,
        body_blocked: false,
        constructed: true,
    }
}

// Spawn a single constructed-buildings card on the Buildings tab.
#[allow(clippy::too_many_arguments)]
pub fn spawn_constructed_card(
    commands: &mut Commands,
    parent: Entity,
    bt: crate::colony::types::BuildingType,
    def: &crate::colony::data::BuildingDefinition,
    count: u32,
    multiplier: u32,
    body_font: &Handle<Font>,
    body_font_medium: &Handle<Font>,
    mono_font: &Handle<Font>,
    icon: Option<&Handle<Image>>,
    resource_icons: &ResourceIcons,
) -> Entity {
    let data = build_constructed_card_data(bt, def, count, multiplier);
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
    super::demolish::spawn_demolish_button(
        commands,
        card,
        bt,
        count,
        multiplier,
        body_font_medium,
        super::markers::DemolishMultiplierSource::Build,
    );
    card
}
