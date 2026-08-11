//! Setup system — spawns the entire Construction canary UI tree at startup.

use bevy::prelude::*;

use crate::ui::bevy_theme::*;
use super::cards::spawn_card;
use super::data::{compute_colony_spare_power_mw_opt, visible_cards};
use super::demolish::spawn_demolish_confirm_dialog;
use super::disabled::refresh_card_grid;
use super::dropdown::auto_select_first_colony;
use super::markers::*;
use super::mining::{spawn_mining_body, update_mining_body};
use super::overview::{spawn_overview_body, update_overview_body, update_overview_queue};
use super::buildings::{spawn_buildings_body, update_buildings_body};
use super::queue::*;
use super::scrollbar::spawn_construction_scrollbar;
use super::state::*;
use super::tooltip::*;
use crate::ui::widgets::{
    card_shadow, spawn_scrollable_container, UiFonts, HoverElevation, ChipGroup,
};
use crate::ui::bevy_theme::{
    ChipButtonBundle, ChipRowContainerBundle, HairlineBundle, spawn_chip_text,
};
use crate::research::systems::ResearchState;

// Setup system: spawns the Construction panel entities once at startup.
pub fn setup_construction(
    mut commands: Commands,
    fonts: Res<UiFonts>,
    buildings_data_opt: Option<Res<crate::colony::data::BuildingsData>>,
    research_state: Res<ResearchState>,
    ui_state: Res<ConstructionUiState>,
    building_icons: Option<Res<BuildingIcons>>,
    resource_icons: Option<Res<crate::ui::resource_icons::ResourceIcons>>,
    colonies: Query<(Entity, &crate::colony::Colony)>,
) {
    let body_font = fonts.body.clone();
    let body_font_medium = fonts.medium.clone();
    let mono_font = fonts.mono.clone();

    let root = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(126.0),
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                bottom: Val::Px(72.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::FlexStart,
                align_items: AlignItems::Start,
                row_gap: Val::Px(SPACE_SM),
                padding: UiRect::all(Val::Px(SPACE_LG)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.012, 0.024, 0.047, 0.97)),
            ZIndex(1),
            Visibility::Hidden,
            Name::new("construction_canary_root"),
            ConstructionRoot,
        ))
        .id();

    let tab_strip = commands
        .spawn(ChipRowContainerBundle::new("tabs", TAB_STRIP_H))
        .id();
    commands.entity(root).add_child(tab_strip);

    for (i, (label, is_active)) in [
        ("Overview", false),
        ("Buildings", false),
        ("Build", true),
        ("Mining", false),
    ]
    .iter()
    .enumerate()
    {
        let chip = ChipButtonBundle::new(label, *is_active);
        let mut entity_commands = commands.spawn(chip);
        entity_commands.insert(ChipGroup::Tab(i));
        let tab = entity_commands.id();
        commands.entity(tab_strip).add_child(tab);
        spawn_chip_text(
            &mut commands,
            tab,
            label,
            body_font.clone(),
            *is_active,
            TAB_FONT_SIZE,
        );
    }

    let shared_chrome = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                width: Val::Percent(100.0),
                align_items: AlignItems::Start,
                row_gap: Val::Px(SPACE_SM),
                ..default()
            },
            ZIndex(1),
            Name::new("shared_chrome"),
        ))
        .id();
    commands.entity(root).add_child(shared_chrome);

    let output_row = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                height: Val::Px(32.0),
                padding: UiRect::horizontal(Val::Px(SPACE_LG)),
                column_gap: Val::Px(SPACE_LG),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
            Name::new("output_row"),
        ))
        .id();
    commands.entity(shared_chrome).add_child(output_row);

    let output_icon = commands
        .spawn((
            Node {
                width: Val::Px(20.0),
                height: Val::Px(20.0),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(3.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.196, 0.529, 0.612, 0.30)),
            BorderColor::all(CYAN_BORDER),
            Name::new("output_icon"),
        ))
        .id();
    commands.entity(output_row).add_child(output_icon);
    let output_label = commands
        .spawn((
            Text::new("Output: "),
            TextFont {
                font: body_font.clone(),
                font_size: BODY_SIZE,
                ..default()
            },
            TextColor(TEXT_DIM),
            Name::new("output_label"),
        ))
        .id();
    commands.entity(output_row).add_child(output_label);
    let output_value = commands
        .spawn((
            Text::new("12001.0 BP/year"),
            TextFont {
                font: mono_font.clone(),
                font_size: BODY_SIZE,
                ..default()
            },
            TextColor(CYAN),
            Name::new("output_value"),
        ))
        .id();
    commands.entity(output_row).add_child(output_value);
    let queue_label = commands
        .spawn((
            Text::new("    │   Queue: "),
            TextFont {
                font: body_font.clone(),
                font_size: BODY_SIZE,
                ..default()
            },
            TextColor(TEXT_DIM),
            Name::new("queue_label"),
        ))
        .id();
    commands.entity(output_row).add_child(queue_label);
    let queue_value = commands
        .spawn((
            Text::new("6d 2h"),
            TextFont {
                font: mono_font.clone(),
                font_size: BODY_SIZE,
                ..default()
            },
            TextColor(YELLOW_ETA),
            Name::new("queue_value"),
            QueuePanelSummaryText,
        ))
        .id();
    commands.entity(output_row).add_child(queue_value);

    let queue_chip = commands
        .spawn((
            Button,
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                height: Val::Px(20.0),
                padding: UiRect::horizontal(Val::Px(SPACE_MD)),
                column_gap: Val::Px(SPACE_XS),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(3.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
            BorderColor::all(CYAN_BORDER),
            Pickable::default(),
            Name::new("open_queue_chip"),
            OpenQueueChip,
        ))
        .id();
    commands.entity(output_row).add_child(queue_chip);
    let queue_chip_label = commands
        .spawn((
            Text::new("OPEN QUEUE"),
            TextFont {
                font: body_font_medium.clone(),
                font_size: CAPTION_SIZE,
                ..default()
            },
            TextColor(CYAN),
            Name::new("open_queue_chip_label"),
        ))
        .id();
    commands.entity(queue_chip).add_child(queue_chip_label);

    let picker = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                height: Val::Px(24.0),
                padding: UiRect::horizontal(Val::Px(SPACE_MD)),
                column_gap: Val::Px(SPACE_SM),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.012, 0.039, 0.078, 0.40)),
            BorderColor::all(CYAN_BORDER),
            Pickable::default(),
            Button,
            Name::new("active_colony_picker"),
            ColonyPicker,
        ))
        .id();
    commands.entity(shared_chrome).add_child(picker);
    let colony_icon = commands
        .spawn((
            Node {
                width: Val::Px(14.0),
                height: Val::Px(14.0),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.373, 0.784, 0.847, 0.40)),
            BorderColor::all(CYAN_BORDER),
            Name::new("colony_icon"),
        ))
        .id();
    commands.entity(picker).add_child(colony_icon);
    let colony_label = commands
        .spawn((
            Text::new("Active Colony: "),
            TextFont {
                font: body_font.clone(),
                font_size: CAPTION_SIZE,
                ..default()
            },
            TextColor(TEXT_DIM),
            Name::new("colony_label"),
        ))
        .id();
    commands.entity(picker).add_child(colony_label);
    let colony_value = commands
        .spawn((
            Text::new("(no colony)"),
            TextFont {
                font: body_font_medium.clone(),
                font_size: CAPTION_SIZE,
                ..default()
            },
            TextColor(CYAN),
            Name::new("colony_value"),
            ColonyPickerText,
        ))
        .id();
    commands.entity(picker).add_child(colony_value);
    let chevron = commands
        .spawn((
            Text::new("▾"),
            TextFont {
                font: body_font.clone(),
                font_size: CAPTION_SIZE,
                ..default()
            },
            TextColor(CYAN),
            Name::new("colony_chevron"),
        ))
        .id();
    commands.entity(picker).add_child(chevron);

    let dropdown = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(28.0),
                left: Val::Px(0.0),
                min_width: Val::Px(220.0),
                max_width: Val::Px(320.0),
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(SPACE_XS)),
                row_gap: Val::Px(SPACE_XS),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.008, 0.039, 0.094, 0.96)),
            BorderColor::all(CYAN_BORDER_STRONG),
            BoxShadow::new(
                Color::srgba(0.0, 0.0, 0.0, 0.65),
                Val::Px(0.0),
                Val::Px(4.0),
                Val::Px(2.0),
                Val::Px(12.0),
            ),
            GlobalZIndex(100),
            Visibility::Hidden,
            Pickable::default(),
            Name::new("active_colony_dropdown"),
            ColonyDropdownMenu,
        ))
        .id();
    commands.entity(picker).add_child(dropdown);
    // Outside-click dismissal: a Pointer<Click> on the menu's empty
    // area (not on an option) closes it. The observer ignores clicks
    // that land on option rows so the selection handler stays the
    // source of truth.
    commands
        .entity(dropdown)
        .observe(super::dropdown::on_colony_dropdown_outside_click);

    let build_qty_row = commands
        .spawn(ChipRowContainerBundle::new("build_qty", 28.0))
        .id();
    commands.entity(shared_chrome).add_child(build_qty_row);

    let build_qty_label = commands
        .spawn((
            Text::new("Build qty: "),
            TextFont {
                font: body_font.clone(),
                font_size: CAPTION_SIZE,
                ..default()
            },
            TextColor(TEXT_DIM),
            Name::new("build_qty_label"),
        ))
        .id();
    commands.entity(build_qty_row).add_child(build_qty_label);

    for (i, (label, qty)) in [
        ("x1", 1u32),
        ("x5", 5),
        ("x10", 10),
        ("x25", 25),
        ("x50", 50),
        ("x100", 100),
    ]
    .iter()
    .enumerate()
    {
        let is_active = i == 0;
        let btn = commands.spawn(ChipButtonBundle::new(label, is_active)).id();
        commands.entity(btn).insert(ChipGroup::Qty(*qty));
        commands.entity(build_qty_row).add_child(btn);
        spawn_chip_text(
            &mut commands,
            btn,
            label,
            mono_font.clone(),
            is_active,
            16.0,
        );
    }

    let filter_row = commands
        .spawn((
            ChipRowContainerBundle::new("filter", 28.0),
            ShowOnBuildOrBuildings,
        ))
        .id();
    commands.entity(shared_chrome).add_child(filter_row);

    let filter_label = commands
        .spawn((
            Text::new("Filter: "),
            TextFont {
                font: body_font.clone(),
                font_size: CAPTION_SIZE,
                ..default()
            },
            TextColor(TEXT_DIM),
            Name::new("filter_label"),
        ))
        .id();
    commands.entity(filter_row).add_child(filter_label);

    for (i, label) in [
        "Infrastructure",
        "Industry",
        "Logistics",
        "Power",
        "Population",
        "Research",
        "Financial",
        "Military",
        "All",
    ]
    .iter()
    .copied()
    .enumerate()
    {
        let is_active = i == 8;
        let chip = commands.spawn(ChipButtonBundle::new(label, is_active)).id();
        commands.entity(chip).insert(ChipGroup::Category(i));
        commands.entity(filter_row).add_child(chip);
        spawn_chip_text(
            &mut commands,
            chip,
            label,
            body_font.clone(),
            is_active,
            16.0,
        );
    }

    let card_grid = commands
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
                padding: UiRect::all(Val::Px(SPACE_LG)),
                row_gap: Val::Px(SPACE_LG),
                column_gap: Val::Px(SPACE_LG),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
            ConstructionTabBody::Build,
            Visibility::Hidden,
            Name::new("card_grid"),
        ))
        .id();
    commands.entity(root).add_child(card_grid);
    commands.entity(card_grid).insert(CardGrid);

    let track_top_px = 138.0_f32;
    let track_bottom_px = SPACE_SM;
    spawn_construction_scrollbar(
        &mut commands,
        root,
        card_grid,
        "card_grid_scrollbar_track",
        track_top_px,
        track_bottom_px,
        ConstructionTabBody::Build,
    );

    let buildings_data = match buildings_data_opt {
        Some(data) => data,
        None => return,
    };
    let category_idx = ui_state.selected_build_tab;
    let filter = ui_state.selected_filter;
    let multiplier = ui_state.build_multiplier;
    let spare_power_mw = compute_colony_spare_power_mw_opt(&ui_state, &colonies, Some(&buildings_data));
    for (building_type, card_data) in visible_cards(
        &buildings_data,
        &research_state,
        category_idx,
        filter,
        multiplier,
        spare_power_mw,
    ) {
        let icon_handle: Option<&Handle<Image>> = building_icons
            .as_ref()
            .and_then(|icons| icons.handles.get(&building_type));
        let empty_resource_icons = crate::ui::resource_icons::ResourceIcons::default();
        let resource_icons_ref: &crate::ui::resource_icons::ResourceIcons = resource_icons
            .as_ref()
            .map(|r: &Res<crate::ui::resource_icons::ResourceIcons>| -> &crate::ui::resource_icons::ResourceIcons { r.as_ref() })
            .unwrap_or(&empty_resource_icons);
        let card = spawn_card(
            &mut commands,
            card_grid,
            &card_data,
            building_type,
            &body_font,
            &body_font_medium,
            &mono_font,
            icon_handle,
            resource_icons_ref,
        );
        commands.entity(card).insert(BuildCard);
    }

    spawn_overview_body(
        &mut commands,
        root,
        &body_font,
        &body_font_medium,
        &mono_font,
    );
    spawn_buildings_body(&mut commands, root, &body_font_medium);
    spawn_mining_body(&mut commands, root);

    // Phase 4: spawn the singleton cursor-following tooltip overlay
    // tree (TooltipOverlay + TooltipTitle + TooltipBody) used by
    // every construction tooltip surface. Replaces the three
    // per-overlay trees that used to live here.
    spawn_construction_tooltip_overlay(&mut commands, root, body_font.clone());

    let queue_panel = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(0.0),
                right: Val::Px(0.0),
                bottom: Val::Px(0.0),
                width: Val::Px(360.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(SPACE_LG)),
                row_gap: Val::Px(SPACE_SM),
                border: UiRect::left(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.012, 0.024, 0.047, 0.96)),
            BorderColor::all(CYAN_BORDER),
            ZIndex(2),
            Visibility::Hidden,
            Pickable::default(),
            Name::new("queue_panel"),
            QueuePanelRoot,
        ))
        .id();
    commands.entity(root).add_child(queue_panel);

    let queue_header = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                width: Val::Percent(100.0),
                column_gap: Val::Px(SPACE_MD),
                ..default()
            },
            Name::new("queue_panel_header"),
        ))
        .id();
    commands.entity(queue_panel).add_child(queue_header);

    let title = commands
        .spawn((
            Text::new("CONSTRUCTION QUEUE"),
            TextFont {
                font: body_font_medium.clone(),
                font_size: BODY_SIZE,
                ..default()
            },
            TextColor(CYAN),
            Node {
                flex_grow: 1.0,
                ..default()
            },
            Name::new("queue_panel_title"),
        ))
        .id();
    commands.entity(queue_header).add_child(title);

    let close_btn = commands
        .spawn((
            Button,
            Node {
                display: Display::Flex,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                width: Val::Px(24.0),
                height: Val::Px(24.0),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(3.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
            BorderColor::all(CYAN_BORDER),
            Pickable::default(),
            Name::new("queue_panel_close"),
            QueuePanelClose,
        ))
        .id();
    commands.entity(queue_header).add_child(close_btn);
    let close_label = commands
        .spawn((
            Text::new("×"),
            TextFont {
                font: body_font_medium.clone(),
                font_size: BODY_SIZE,
                ..default()
            },
            TextColor(CYAN),
            Name::new("queue_panel_close_label"),
        ))
        .id();
    commands.entity(close_btn).add_child(close_label);

    spawn_scrollable_container(
        &mut commands,
        queue_panel,
        "queue_panel_body",
        SPACE_SM,
        QueuePanelBody,
    );

    spawn_demolish_confirm_dialog(&mut commands, root, &body_font, &body_font_medium);
}
