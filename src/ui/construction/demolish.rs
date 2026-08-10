//! Demolish button + confirmation dialog.
//!
//! Both Mining-tab and Buildings-tab cards carry a Demolish button;
//! the dialog is shared (a single centered modal).

use bevy::prelude::*;

use crate::ui::bevy_theme::*;
use super::markers::*;
pub use super::markers::DemolishMultiplierSource;
use super::state::*;
use crate::ui::widgets::UiFonts;
use crate::colony::components::PendingConstructionActions;

// ── Demolish button (per-card) ──────────────────────────────────────

// Spawn a Demolish button on a card. Pinned to the
// bottom-right of the card (opposite of the Queue button) via
// absolute positioning.
pub fn spawn_demolish_button(
    commands: &mut Commands,
    card: Entity,
    bt: crate::colony::types::BuildingType,
    count: u32,
    multiplier: u32,
    body_font_medium: &Handle<Font>,
    multiplier_source: DemolishMultiplierSource,
) {
    let label = if multiplier > 1 {
        format!("Demolish \u{2212}{}", multiplier)
    } else {
        "Demolish -1".to_string()
    };
    let dim_red = Color::srgba(0.353, 0.157, 0.169, 0.85);
    let dim_red_border = Color::srgba(0.847, 0.373, 0.392, 0.50);
    let demolish = commands
        .spawn((
            Button,
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                align_self: AlignSelf::FlexEnd,
                height: Val::Px(32.0),
                padding: UiRect::horizontal(Val::Px(SPACE_XL)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                position_type: PositionType::Absolute,
                bottom: Val::Px(SPACE_LG),
                right: Val::Px(SPACE_LG),
                ..default()
            },
            BackgroundColor(dim_red),
            BorderColor::all(dim_red_border),
            Name::new("card_demolish"),
            DemolishButton {
                building_type: bt,
                multiplier_source,
            },
            Pickable::default(),
        ))
        .id();
    if count == 0 {
        commands.entity(demolish).insert(DemolishDisabled);
    }
    commands.entity(card).add_child(demolish);

    let label_entity = commands
        .spawn((
            Text::new(label),
            TextFont {
                font: body_font_medium.clone(),
                font_size: CAPTION_SIZE,
                ..default()
            },
            TextColor(RED),
            Node {
                flex_grow: 1.0,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                display: Display::Flex,
                ..default()
            },
            Name::new("card_demolish_label"),
            DemolishButtonLabel,
        ))
        .id();
    commands.entity(demolish).add_child(label_entity);
}

// Click handler for the Demolish button. Opens the centered
// confirmation dialog (`DemolishConfirmDialog`).
pub fn tick_demolish_click(
    disabled: Query<Entity, With<DemolishDisabled>>,
    interactions: Query<(Entity, &Interaction, &DemolishButton), With<Button>>,
    ui_state: Res<ConstructionUiState>,
    mut confirm_state: ResMut<DemolishConfirmState>,
    mut prev: Local<std::collections::HashMap<Entity, Interaction>>,
) {
    let mut disabled_set: std::collections::HashSet<Entity> = std::collections::HashSet::new();
    for entity in disabled.iter() {
        disabled_set.insert(entity);
    }
    crate::ui::widgets::detect_rising_edges(&mut prev, &interactions, |entity, button| {
        if disabled_set.contains(&entity) {
            return;
        }
        if ui_state.selected_colony.is_none() {
            return;
        }
        let count = match button.multiplier_source {
            DemolishMultiplierSource::Mining => ui_state.build_multiplier.max(1),
            DemolishMultiplierSource::Build => ui_state.build_multiplier.max(1),
        };
        confirm_state.open = true;
        confirm_state.building_type = Some(button.building_type);
        confirm_state.count = count;
    });
}

// Per-frame system: re-evaluate which Demolish buttons should be
// disabled based on the current mine count.
pub fn tick_demolish_disabled(
    mut commands: Commands,
    ui_state: Res<ConstructionUiState>,
    colonies: Query<&crate::colony::Colony>,
    demolish_buttons: Query<(Entity, &DemolishButton, Has<DemolishDisabled>)>,
) {
    let Some(colony_entity) = ui_state.selected_colony else {
        return;
    };
    let Ok(colony) = colonies.get(colony_entity) else {
        return;
    };
    for (entity, button, is_disabled) in demolish_buttons.iter() {
        let count = colony
            .buildings
            .get(&button.building_type)
            .copied()
            .unwrap_or(0);
        if count == 0 && !is_disabled {
            commands
                .entity(entity)
                .queue_silenced(InsertDemolishDisabled);
        } else if count > 0 && is_disabled {
            commands
                .entity(entity)
                .queue_silenced(RemoveDemolishDisabled);
        }
    }
}

// v0.5.2 (2026-08-06): rewrite every Demolish button's label from the
// live `build_multiplier` each frame.
pub fn update_demolish_button_labels(
    ui_state: Res<ConstructionUiState>,
    mut labels: Query<&mut Text, With<DemolishButtonLabel>>,
) {
    let mult = ui_state.build_multiplier.max(1);
    let label = if mult > 1 {
        format!("Demolish \u{2212}{}", mult)
    } else {
        "Demolish -1".to_string()
    };
    for mut text in labels.iter_mut() {
        if text.0 != label {
            **text = label.clone();
        }
    }
}

// Hover / press effect system for the Demolish buttons.
pub fn tick_demolish_hover(
    mut params: ParamSet<(
        Query<
            (
                Entity,
                &Interaction,
                &mut BackgroundColor,
                &mut BorderColor,
                &mut UiTransform,
            ),
            With<DemolishButton>,
        >,
        Query<Entity, With<DemolishDisabled>>,
    )>,
    mut prev_state: Local<std::collections::HashMap<Entity, (Interaction, bool)>>,
) {
    let rest_fill = Color::srgba(0.353, 0.157, 0.169, 0.85);
    let hover_fill = Color::srgba(0.55, 0.235, 0.255, 1.0);
    let rest_border = Color::srgba(0.847, 0.373, 0.392, 0.50);
    let hover_border = Color::srgba(0.95, 0.45, 0.45, 1.0);
    let mut disabled_set: std::collections::HashSet<Entity> =
        std::collections::HashSet::new();
    for entity in params.p1().iter() {
        disabled_set.insert(entity);
    }
    for (entity, interaction, mut bg, mut border, mut ui_transform) in params.p0().iter_mut() {
        let is_disabled = disabled_set.contains(&entity);
        let prev = prev_state.get(&entity).copied();
        if let Some((prev_int, prev_disabled)) = prev {
            if prev_int == *interaction && prev_disabled == is_disabled {
                continue;
            }
        }
        match interaction {
            Interaction::Pressed if !is_disabled => {
                *bg = BackgroundColor(hover_fill);
                *border = BorderColor::all(hover_border);
                ui_transform.scale = Vec2::splat(0.98);
            }
            Interaction::Hovered if !is_disabled => {
                *bg = BackgroundColor(hover_fill);
                *border = BorderColor::all(hover_border);
                ui_transform.scale = Vec2::splat(1.02);
            }
            Interaction::None => {
                *bg = BackgroundColor(rest_fill);
                *border = BorderColor::all(rest_border);
                ui_transform.scale = Vec2::splat(1.00);
            }
            _ => {
                *bg = BackgroundColor(rest_fill);
                *border = BorderColor::all(rest_border);
                ui_transform.scale = Vec2::splat(1.00);
            }
        }
        prev_state.insert(entity, (*interaction, is_disabled));
    }
    let live: std::collections::HashSet<Entity> = params.p0().iter().map(|(e, ..)| e).collect();
    prev_state.retain(|e, _| live.contains(e));
}

// `EntityCommand` that inserts `DemolishDisabled`.
struct InsertDemolishDisabled;

impl bevy::ecs::system::EntityCommand for InsertDemolishDisabled {
    fn apply(self, mut entity: bevy::ecs::world::EntityWorldMut) {
        entity.insert(DemolishDisabled);
    }
}

// `EntityCommand` that removes `DemolishDisabled`.
struct RemoveDemolishDisabled;

impl bevy::ecs::system::EntityCommand for RemoveDemolishDisabled {
    fn apply(self, mut entity: bevy::ecs::world::EntityWorldMut) {
        entity.remove::<DemolishDisabled>();
    }
}

// ── Demolish confirmation modal ───────────────────────────────

// Per-frame: mirror `DemolishConfirmState::open` into the dialog
// root's `Visibility`.
pub fn tick_demolish_dialog_visibility(
    state: Option<Res<DemolishConfirmState>>,
    mut dialog_query: Query<&mut Visibility, With<DemolishConfirmDialog>>,
) {
    let is_open = state.as_ref().map(|s| s.open).unwrap_or(false);
    for mut visibility in dialog_query.iter_mut() {
        *visibility = if is_open {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

// Per-frame: re-read the live colony's building count for the
// dialog's target `BuildingType` and clamp the dialog's
// `count` down.
pub fn update_demolish_dialog_text(
    ui_state: Res<ConstructionUiState>,
    confirm_state: Option<ResMut<DemolishConfirmState>>,
    colonies: Query<&crate::colony::Colony>,
    mut texts: ParamSet<(
        Query<&mut Text, With<DemolishConfirmTitle>>,
        Query<&mut Text, With<DemolishConfirmSubtitle>>,
    )>,
) {
    let Some(mut confirm) = confirm_state else {
        return;
    };
    if !confirm.open {
        return;
    }
    let Some(bt) = confirm.building_type else {
        return;
    };
    let Some(colony_entity) = ui_state.selected_colony else {
        return;
    };
    let Ok(colony) = colonies.get(colony_entity) else {
        return;
    };
    let live_count = colony.buildings.get(&bt).copied().unwrap_or(0);
    let clamped_count = confirm.count.min(live_count);
    if clamped_count != confirm.count {
        confirm.count = clamped_count;
    }
    let title = format!("Demolish {} {}?", clamped_count, bt);
    let subtitle = format!(
        "You currently have {} on {}.",
        live_count, colony.name
    );
    for mut text in texts.p0().iter_mut() {
        **text = title.clone();
    }
    for mut text in texts.p1().iter_mut() {
        **text = subtitle.clone();
    }
}

// Yes button click: rising-edge `Interaction::Pressed` on a
// `DemolishConfirmYes` entity.
pub fn tick_demolish_confirm_yes_click(
    yes_query: Query<(Entity, &Interaction), (With<DemolishConfirmYes>, With<Button>)>,
    mut confirm_state: ResMut<DemolishConfirmState>,
    ui_state: Res<ConstructionUiState>,
    mut pending: ResMut<PendingConstructionActions>,
    mut prev: Local<std::collections::HashMap<Entity, Interaction>>,
) {
    crate::ui::widgets::detect_rising_edges_no_marker(&mut prev, &yes_query, |_entity| {
        if !confirm_state.open {
            return;
        }
        let (Some(bt), Some(colony_entity)) =
            (confirm_state.building_type, ui_state.selected_colony)
        else {
            *confirm_state = DemolishConfirmState::default();
            return;
        };
        let count = confirm_state.count as i32;
        if count > 0 {
            pending.mining_edits.push((colony_entity, bt, -count));
        }
        *confirm_state = DemolishConfirmState::default();
    });
}

// No button click + backdrop click.
pub fn tick_demolish_confirm_no_click(
    no_query: Query<(Entity, &Interaction), (With<DemolishConfirmNo>, With<Button>)>,
    backdrop_query: Query<(Entity, &Interaction), (With<DemolishConfirmDialog>, With<Button>)>,
    mut confirm_state: ResMut<DemolishConfirmState>,
    mut prev_no: Local<std::collections::HashMap<Entity, Interaction>>,
    mut prev_backdrop: Local<std::collections::HashMap<Entity, Interaction>>,
) {
    crate::ui::widgets::detect_rising_edges_no_marker(&mut prev_no, &no_query, |_entity| {
        *confirm_state = DemolishConfirmState::default();
    });
    crate::ui::widgets::detect_rising_edges_no_marker(&mut prev_backdrop, &backdrop_query, |_entity| {
        *confirm_state = DemolishConfirmState::default();
    });
}

// When the player switches tabs while the dialog is open, reset
// the dialog state.
pub fn tick_demolish_dialog_close_on_tab_switch(
    ui_state: Res<ConstructionUiState>,
    mut confirm_state: ResMut<DemolishConfirmState>,
    mut prev_tab: Local<Option<ConstructionTab>>,
) {
    if let Some(prev) = *prev_tab {
        if prev != ui_state.selected_tab && confirm_state.open {
            *confirm_state = DemolishConfirmState::default();
        }
    }
    *prev_tab = Some(ui_state.selected_tab);
}

// When the player switches colonies while the dialog is open,
// reset the dialog state.
pub fn tick_demolish_dialog_close_on_colony_change(
    ui_state: Res<ConstructionUiState>,
    mut confirm_state: ResMut<DemolishConfirmState>,
    mut prev_colony: Local<Option<Entity>>,
) {
    if *prev_colony != ui_state.selected_colony && confirm_state.open {
        *confirm_state = DemolishConfirmState::default();
    }
    *prev_colony = ui_state.selected_colony;
}

// Spawn the Demolish confirmation dialog as a single `Display::None`
// child of the Construction menu root.
pub fn spawn_demolish_confirm_dialog(
    commands: &mut Commands,
    parent: Entity,
    body_font: &Handle<Font>,
    body_font_medium: &Handle<Font>,
) {
    let dialog = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(0.0),
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                bottom: Val::Px(0.0),
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.70)),
            Pickable::default(),
            Button,
            GlobalZIndex(150),
            Visibility::Hidden,
            DemolishConfirmDialog,
            Name::new("demolish_confirm_dialog"),
        ))
        .id();
    commands.entity(parent).add_child(dialog);

    let card = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(SPACE_LG),
                padding: UiRect::all(Val::Px(SPACE_XL)),
                width: Val::Px(480.0),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(8.0)),
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
            Name::new("demolish_confirm_card"),
        ))
        .id();
    commands.entity(dialog).add_child(card);

    let title = commands
        .spawn((
            Text::new("Demolish"),
            TextFont {
                font: body_font_medium.clone(),
                font_size: BODY_SIZE,
                ..default()
            },
            TextColor(RED),
            DemolishConfirmTitle,
            Name::new("demolish_confirm_title"),
        ))
        .id();
    commands.entity(card).add_child(title);

    let subtitle = commands
        .spawn((
            Text::new(""),
            TextFont {
                font: body_font.clone(),
                font_size: CAPTION_SIZE,
                ..default()
            },
            TextColor(TEXT_BODY),
            DemolishConfirmSubtitle,
            Name::new("demolish_confirm_subtitle"),
        ))
        .id();
    commands.entity(card).add_child(subtitle);

    let button_row = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(SPACE_LG),
                ..default()
            },
            Name::new("demolish_confirm_buttons"),
        ))
        .id();
    commands.entity(card).add_child(button_row);

    let yes_button = commands
        .spawn((
            Button,
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                flex_grow: 1.0,
                height: Val::Px(36.0),
                padding: UiRect::horizontal(Val::Px(SPACE_XL)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.353, 0.157, 0.169, 0.85)),
            BorderColor::all(Color::srgba(0.847, 0.373, 0.392, 0.50)),
            DemolishConfirmYes,
            Pickable::default(),
            Name::new("demolish_confirm_yes"),
        ))
        .id();
    commands.entity(button_row).add_child(yes_button);
    let yes_label = commands
        .spawn((
            Text::new("Demolish"),
            TextFont {
                font: body_font_medium.clone(),
                font_size: BODY_SIZE,
                ..default()
            },
            TextColor(RED),
            Node {
                flex_grow: 1.0,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                display: Display::Flex,
                ..default()
            },
            Name::new("demolish_confirm_yes_label"),
        ))
        .id();
    commands.entity(yes_button).add_child(yes_label);

    let no_button = commands
        .spawn((
            Button,
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                flex_grow: 1.0,
                height: Val::Px(36.0),
                padding: UiRect::horizontal(Val::Px(SPACE_XL)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.094, 0.298, 0.353, 0.85)),
            BorderColor::all(CYAN_BORDER_STRONG),
            DemolishConfirmNo,
            Pickable::default(),
            Name::new("demolish_confirm_no"),
        ))
        .id();
    commands.entity(button_row).add_child(no_button);
    let no_label = commands
        .spawn((
            Text::new("Cancel"),
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
            Name::new("demolish_confirm_no_label"),
        ))
        .id();
    commands.entity(no_button).add_child(no_label);
}
