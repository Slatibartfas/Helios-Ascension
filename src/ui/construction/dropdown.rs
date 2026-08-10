//! Active Colony dropdown — picker + menu + selection system.

use bevy::prelude::*;

use crate::ui::bevy_theme::*;
use super::markers::*;
use super::state::*;
use crate::ui::widgets::UiFonts;

// Auto-select the first available colony if `selected_colony` is None
// or points at a despawned entity. This makes the Queue button work
// out of the box without requiring the player to manually pick a
// colony from the dropdown, and gracefully recovers when the selected
// colony has been removed (e.g. dissolved mid-game).
pub fn auto_select_first_colony(
    mut ui_state: ResMut<ConstructionUiState>,
    colonies: Query<Entity, With<crate::colony::Colony>>,
) {
    let needs_pick = match ui_state.selected_colony {
        None => true,
        Some(e) => colonies.get(e).is_err(),
    };
    if needs_pick {
        if let Some(first) = colonies.iter().next() {
            ui_state.selected_colony = Some(first);
        }
    }
}

// Click handler for the "Active Colony" picker. Toggles
// `ColonyDropdownState::open` so the floating dropdown appears /
// disappears.
pub fn tick_colony_picker_click(
    interactions: Query<(Entity, &Interaction), (With<ColonyPicker>, With<Button>)>,
    mut state: Option<ResMut<ColonyDropdownState>>,
    mut prev: Local<std::collections::HashMap<Entity, Interaction>>,
) {
    crate::ui::widgets::detect_rising_edges_no_marker(&mut prev, &interactions, |_entity| {
        if let Some(ref mut s) = state {
            s.open = !s.open;
        }
    });
}

// Click handler for a single colony option inside the dropdown.
pub fn tick_colony_option_click(
    interactions: Query<(Entity, &Interaction, &ColonyDropdownOption), With<Button>>,
    mut ui_state: ResMut<ConstructionUiState>,
    mut state: Option<ResMut<ColonyDropdownState>>,
    mut prev: Local<std::collections::HashMap<Entity, Interaction>>,
) {
    crate::ui::widgets::detect_rising_edges(&mut prev, &interactions, |_entity, option| {
        ui_state.selected_colony = Some(option.colony_entity);
        if let Some(ref mut s) = state {
            s.open = false;
        }
    });
}

// Toggle the colony dropdown menu visibility based on
// `ColonyDropdownState::open`.
pub fn tick_colony_dropdown_visibility(
    state: Option<Res<ColonyDropdownState>>,
    mut menu_query: Query<&mut Visibility, With<ColonyDropdownMenu>>,
) {
    let is_open = state.as_ref().map(|s| s.open).unwrap_or(false);
    let target = if is_open {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    for mut v in menu_query.iter_mut() {
        *v = target;
    }
}

// Update the picker's value text every frame based on the active
// selection.
pub fn update_colony_picker_text(
    ui_state: Res<ConstructionUiState>,
    colonies: Query<(Entity, &crate::colony::Colony)>,
    mut text_query: Query<&mut Text, With<ColonyPickerText>>,
) {
    let label = match ui_state.selected_colony.and_then(|e| colonies.get(e).ok()) {
        Some((_, colony)) => format!(
            "{} ({})",
            colony.name,
            crate::colony::Colony::format_population(colony.population)
        ),
        None => "(no colony)".to_string(),
    };
    for mut text in text_query.iter_mut() {
        **text = label.clone();
    }
}

// Update the colony dropdown menu's rows. Spawn-once-update-many:
// rows persist across `ColonyDropdownState::open` toggles and across
// tab visibility changes.
pub fn refresh_colony_dropdown(
    mut commands: Commands,
    fonts: Res<UiFonts>,
    colonies: Query<(Entity, &crate::colony::Colony)>,
    menu_query: Query<Entity, With<ColonyDropdownMenu>>,
    ui_state: Res<ConstructionUiState>,
    mut spawned_rows: Local<
        std::collections::HashMap<bevy::ecs::entity::Entity, bevy::ecs::entity::Entity>,
    >,
    mut row_bg_query: Query<
        (&ColonyDropdownOption, &mut BackgroundColor),
        Without<ColonyDropdownOptionText>,
    >,
    mut text_query: Query<(&ChildOf, &mut Text, &mut TextColor), With<ColonyDropdownOptionText>>,
) {
    let Ok(menu) = menu_query.single() else {
        return;
    };

    let body_font_medium: Handle<Font> = fonts.medium.clone();

    let mut live_colonies: Vec<(bevy::ecs::entity::Entity, String)> = colonies
        .iter()
        .map(|(e, c)| {
            (
                e,
                format!(
                    "{} ({})",
                    c.name,
                    crate::colony::Colony::format_population(c.population)
                ),
            )
        })
        .collect();
    live_colonies.sort_by(|a, b| a.1.cmp(&b.1));
    let live_keys: std::collections::HashSet<bevy::ecs::entity::Entity> =
        live_colonies.iter().map(|(e, _)| *e).collect();

    let to_remove: Vec<bevy::ecs::entity::Entity> = spawned_rows
        .keys()
        .filter(|k| !live_keys.contains(k))
        .copied()
        .collect();
    for key in to_remove {
        if let Some(row_entity) = spawned_rows.remove(&key) {
            commands.entity(row_entity).try_despawn();
        }
    }

    for (colony_entity, row_entity) in spawned_rows.iter() {
        let is_selected = ui_state.selected_colony == Some(*colony_entity);
        if let Ok((_, mut bg)) = row_bg_query.get_mut(*row_entity) {
            *bg = BackgroundColor(if is_selected {
                Color::srgba(0.196, 0.529, 0.612, 0.78)
            } else {
                Color::srgba(0.0, 0.0, 0.0, 0.0)
            });
        }
        let label = colonies
            .get(*colony_entity)
            .map(|(_, c)| {
                format!(
                    "{} ({})",
                    c.name,
                    crate::colony::Colony::format_population(c.population)
                )
            })
            .unwrap_or_else(|_| "(unknown)".to_string());
        let text_color = if is_selected {
            ACTIVE_CHIP_TEXT
        } else {
            TEXT_BODY
        };
        for (parent, mut text, mut color) in text_query.iter_mut() {
            if parent.0 == *row_entity {
                **text = label.clone();
                *color = TextColor(text_color);
                break;
            }
        }
    }

    for (colony_entity, label) in live_colonies {
        if spawned_rows.contains_key(&colony_entity) {
            continue;
        }
        let is_selected = ui_state.selected_colony == Some(colony_entity);
        let row = commands
            .spawn((
                Button,
                Node {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    width: Val::Percent(100.0),
                    height: Val::Px(22.0),
                    padding: UiRect::horizontal(Val::Px(SPACE_SM)),
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(3.0)),
                    ..default()
                },
                BackgroundColor(if is_selected {
                    Color::srgba(0.196, 0.529, 0.612, 0.78)
                } else {
                    Color::srgba(0.0, 0.0, 0.0, 0.0)
                }),
                BorderColor::all(Color::NONE),
                Pickable::default(),
                Name::new("colony_dropdown_option"),
                ColonyDropdownOption { colony_entity },
            ))
            .id();
        commands.entity(menu).add_child(row);
        let text_color = if is_selected {
            ACTIVE_CHIP_TEXT
        } else {
            TEXT_BODY
        };
        let label_text = commands
            .spawn((
                Text::new(label),
                TextFont {
                    font: body_font_medium.clone(),
                    font_size: CAPTION_SIZE,
                    ..default()
                },
                TextColor(text_color),
                Name::new("colony_dropdown_option_text"),
                ColonyDropdownOptionText,
            ))
            .id();
        commands.entity(row).add_child(label_text);
        spawned_rows.insert(colony_entity, row);
    }
}
