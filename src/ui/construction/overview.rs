//! Overview tab body — colony summary + active construction queue.

use bevy::prelude::*;

use crate::ui::bevy_theme::*;
use super::markers::*;
use super::state::*;
use crate::ui::widgets::{spawn_scrollable_container, UiFonts};

// Build the **Overview** body. Read-only summary of the active colony:
// name, population, BP/yr, queue count.
pub fn spawn_overview_body(
    commands: &mut Commands,
    parent: Entity,
    body_font: &Handle<Font>,
    body_font_medium: &Handle<Font>,
    mono_font: &Handle<Font>,
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
            ConstructionTabBody::Overview,
            Visibility::Hidden,
            Name::new("overview_body"),
        ))
        .id();
    commands.entity(parent).add_child(body);

    let header = commands
        .spawn((
            Text::new("Colony Overview"),
            TextFont {
                font: body_font_medium.clone(),
                font_size: BODY_SIZE,
                ..default()
            },
            TextColor(CYAN),
            Name::new("overview_header"),
        ))
        .id();
    commands.entity(body).add_child(header);

    let mut row = |label: &str, marker: OverviewRowKind, initial: &str, color: Color| {
        let row_node = commands
            .spawn((
                Node {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(SPACE_MD),
                    padding: UiRect::vertical(Val::Px(SPACE_XS)),
                    width: Val::Percent(100.0),
                    ..default()
                },
                Name::new("overview_row"),
            ))
            .id();
        commands.entity(body).add_child(row_node);
        let l = commands
            .spawn((
                Text::new(label.to_string()),
                TextFont {
                    font: body_font.clone(),
                    font_size: BODY_SIZE,
                    ..default()
                },
                TextColor(TEXT_DIM),
                Node {
                    width: Val::Px(180.0),
                    ..default()
                },
                Name::new("overview_row_label"),
            ))
            .id();
        commands.entity(row_node).add_child(l);
        let v = commands
            .spawn((
                Text::new(initial.to_string()),
                TextFont {
                    font: mono_font.clone(),
                    font_size: BODY_SIZE,
                    ..default()
                },
                TextColor(color),
                Name::new("overview_row_value"),
                OverviewRowValue { kind: marker },
            ))
            .id();
        commands.entity(row_node).add_child(v);
    };

    row("Colony", OverviewRowKind::Colony, "(none)", ORANGE_ORE);
    row("Population", OverviewRowKind::Population, "—", TEXT_BODY);
    row(
        "Active Construction",
        OverviewRowKind::ActiveConstruction,
        "—",
        TEXT_BODY,
    );
    row(
        "Unique Building Types",
        OverviewRowKind::UniqueBuildingTypes,
        "—",
        TEXT_BODY,
    );

    let queue_section_header = commands
        .spawn((
            Text::new("Construction Queue"),
            TextFont {
                font: body_font_medium.clone(),
                font_size: BODY_SIZE,
                ..default()
            },
            TextColor(CYAN),
            Node {
                margin: UiRect::top(Val::Px(SPACE_MD)),
                ..default()
            },
            Name::new("overview_queue_header"),
        ))
        .id();
    commands.entity(body).add_child(queue_section_header);

    spawn_scrollable_container(
        commands,
        body,
        "overview_queue_content",
        SPACE_XS,
        OverviewQueueContent,
    );

    let help = commands
        .spawn((
            Text::new(
                "Tip: switch to the Build tab to queue new structures, or open the Queue panel from the AppBar to track progress.",
            ),
            TextFont {
                font: body_font.clone(),
                font_size: CAPTION_SIZE,
                ..default()
            },
            TextColor(TEXT_DIM),
            Node {
                margin: UiRect::top(Val::Px(SPACE_LG)),
                width: Val::Percent(100.0),
                ..default()
            },
            Name::new("overview_help"),
        ))
        .id();
    commands.entity(body).add_child(help);
}

// Marker component on the value text of a single Overview row.
#[derive(Component)]
pub struct OverviewRowValue {
    pub kind: OverviewRowKind,
}

// Identifies which semantic row the value text belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverviewRowKind {
    Colony,
    Population,
    ActiveConstruction,
    UniqueBuildingTypes,
}

// Marker on the Overview body's queue content container.
#[derive(Component)]
pub struct OverviewQueueContent;

// Marker component on the `Text` node that holds the building
// display name within a queued row.
#[derive(Component)]
pub struct OverviewQueueRowNameChild;

// Marker component on the `Text` node that holds the human-readable
// status ("Building" / "Awaiting delivery") within a queued row.
#[derive(Component)]
pub struct OverviewQueueRowStatusChild;

// Marker component on the `Text` node that holds the formatted
// "{:.0}%" progress label within a queued row.
#[derive(Component)]
pub struct OverviewQueueRowProgressChild;

// Phase 10: alias for `widgets::ProgressFill`. The construction-side
// overview queue row progress bar now goes through the generic
// `tick_progress_fill` primitive. The old bare marker
// `OverviewQueueRowFillChild` (a `Node` whose width was set every
// frame) is gone; the percentage lives on `ProgressFill` instead.
pub use crate::ui::widgets::ProgressFill as OverviewQueueRowFillChild;

#[derive(Component)]
pub struct OverviewQueueRow {
    pub project_entity: Entity,
    pub row: Entity,
    pub name_text: Entity,
    pub status_text: Entity,
    pub progress_text: Entity,
    pub progress_fill: Entity,
}

// Update the Overview body's queue section every frame.
//
// Spawn-once-update-many (v0.5.2): rows persist across frames.
pub fn update_overview_queue(
    mut commands: Commands,
    fonts: Res<UiFonts>,
    ui_state: Res<ConstructionUiState>,
    buildings_data: Res<crate::colony::data::BuildingsData>,
    projects: Query<(Entity, &crate::colony::ConstructionProject)>,
    content_query: Query<Entity, With<OverviewQueueContent>>,
    mut spawned_rows: Local<crate::ui::widgets::KeyedList<Entity>>,
    row_query: Query<&OverviewQueueRow>,
    mut text_params: ParamSet<(
        Query<&mut Text, With<OverviewQueueRowNameChild>>,
        Query<&mut Text, With<OverviewQueueRowProgressChild>>,
        Query<(&mut Text, &mut TextColor), With<OverviewQueueRowStatusChild>>,
    )>,
    mut progress_fill_query: Query<&mut Node, With<OverviewQueueRowFillChild>>,
    mut empty_placeholder: Local<Option<Entity>>,
) {
    let Ok(content) = content_query.single() else {
        return;
    };

    let body_font: Handle<Font> = fonts.body.clone();
    let body_font_medium: Handle<Font> = fonts.medium.clone();
    let mono_font: Handle<Font> = fonts.mono.clone();

    let selected_colony = ui_state.selected_colony;
    let colony_projects: Vec<(Entity, crate::colony::ConstructionProject)> = selected_colony
        .map(|colony_entity| {
            projects
                .iter()
                .filter(|(_, p)| p.colony_entity == colony_entity)
                .map(|(e, p)| (e, p.clone()))
                .collect()
        })
        .unwrap_or_default();
    // Phase 6: KeyedList::reconcile handles orphan despawn + missing
    // spawn. The spawn closure builds the row + its four child text/fill
    // entities and stores the OverviewQueueRow struct as a Component on
    // the row (queried below via row_query).
    let live_keys: Vec<Entity> = colony_projects.iter().map(|(e, _)| *e).collect();
    let projects_for_spawn = colony_projects.clone();
    spawned_rows.reconcile(
        &mut commands,
        content,
        &live_keys,
        |commands, parent, project_entity| {
            // Look up the project data for this key.
            let project = projects_for_spawn
                .iter()
                .find(|(e, _)| *e == project_entity)
                .map(|(_, p)| p.clone());
            let Some(project) = project else {
                // Defensive: spawn an empty placeholder row so the
                // KeyedList invariant (every live key has an entity)
                // is preserved. Should not happen in practice — the
                // live_keys set came from the same source.
                return commands.spawn(Node::default()).id();
            };
            let display_name = buildings_data
                .get(&project.building_type)
                .map(|d| d.display_name.as_str())
                .unwrap_or("(unknown)");
            let progress = project.progress_percent();
            let status = if project.awaiting_resources {
                "Awaiting delivery"
            } else {
                "Building"
            };
            let row = commands
                .spawn((
                    Node {
                        display: Display::Flex,
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::all(Val::Px(SPACE_MD)),
                        row_gap: Val::Px(SPACE_XS),
                        border: UiRect::all(Val::Px(1.0)),
                        border_radius: BorderRadius::all(Val::Px(4.0)),
                        width: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(CARD_BG),
                    BorderColor::all(CYAN_BORDER),
                    Name::new("overview_queue_row"),
                ))
                .id();
            commands.entity(parent).add_child(row);

            let header = commands
                .spawn((
                    Node {
                        display: Display::Flex,
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        column_gap: Val::Px(SPACE_MD),
                        width: Val::Percent(100.0),
                        ..default()
                    },
                    Name::new("overview_queue_row_header"),
                ))
                .id();
            commands.entity(row).add_child(header);

            let name_text = commands
                .spawn((
                    Text::new(display_name.to_string()),
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
                    Name::new("overview_queue_row_name"),
                    OverviewQueueRowNameChild,
                ))
                .id();
            commands.entity(header).add_child(name_text);

            let status_text = commands
                .spawn((
                    Text::new(status.to_string()),
                    TextFont {
                        font: body_font.clone(),
                        font_size: CAPTION_SIZE,
                        ..default()
                    },
                    TextColor(if project.awaiting_resources {
                        ORANGE_ORE
                    } else {
                        GREEN_FIN
                    }),
                    Name::new("overview_queue_row_status"),
                    OverviewQueueRowStatusChild,
                ))
                .id();
            commands.entity(header).add_child(status_text);

            let progress_text = commands
                .spawn((
                    Text::new(format!("{:.0}%", (progress as f64) * 100.0)),
                    TextFont {
                        font: mono_font.clone(),
                        font_size: CAPTION_SIZE,
                        ..default()
                    },
                    TextColor(CYAN),
                    Name::new("overview_queue_row_progress"),
                    OverviewQueueRowProgressChild,
                ))
                .id();
            commands.entity(header).add_child(progress_text);

            let track = commands
                .spawn((
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(4.0),
                        border_radius: BorderRadius::all(Val::Px(2.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.196, 0.529, 0.612, 0.30)),
                    Name::new("overview_queue_row_track"),
                ))
                .id();
            commands.entity(row).add_child(track);
            let progress_fill = commands
                .spawn((
                    Node {
                        width: Val::Percent(0.0), // set by tick_progress_fill
                        height: Val::Percent(100.0),
                        border_radius: BorderRadius::all(Val::Px(2.0)),
                        ..default()
                    },
                    BackgroundColor(CYAN),
                    Name::new("overview_queue_row_fill"),
                    OverviewQueueRowFillChild(progress.clamp(0.0, 1.0)),
                ))
                .id();
            commands.entity(track).add_child(progress_fill);

            // Store the row + child handles as a Component so the
            // per-frame update below can resolve them without walking
            // Children / ChildrenOf chains.
            commands.entity(row).insert(OverviewQueueRow {
                project_entity,
                row,
                name_text,
                status_text,
                progress_text,
                progress_fill,
            });
            row
        },
    );

    if colony_projects.is_empty() {
        let need_spawn = match *empty_placeholder {
            Some(p) => commands.get_entity(p).is_err(),
            None => true,
        };
        if need_spawn {
            let placeholder = commands
                .spawn((
                    Text::new(
                        "No active construction projects. Switch to the Build tab to queue a building.",
                    ),
                    TextFont {
                        font: body_font.clone(),
                        font_size: BODY_SIZE,
                        ..default()
                    },
                    TextColor(TEXT_DIM),
                    Name::new("overview_queue_empty"),
                ))
                .id();
            commands.entity(content).add_child(placeholder);
            *empty_placeholder = Some(placeholder);
        }
        return;
    } else if let Some(placeholder) = empty_placeholder.take() {
        commands.entity(placeholder).try_despawn();
    }

    for (project_entity, project) in &colony_projects {
        let Some(row_entity) = spawned_rows.get(project_entity) else {
            continue;
        };
        let Ok(row) = row_query.get(row_entity) else {
            continue;
        };
        let progress = project.progress_percent();
        let status = if project.awaiting_resources {
            "Awaiting delivery"
        } else {
            "Building"
        };
        let display_name = buildings_data
            .get(&project.building_type)
            .map(|d| d.display_name.as_str())
            .unwrap_or("(unknown)");
        let new_text = display_name.to_string();
        {
            if let Ok(mut text) = text_params.p0().get_mut(row.name_text) {
                **text = new_text;
            }
        }
        let status_text = status.to_string();
        let status_color = if project.awaiting_resources {
            ORANGE_ORE
        } else {
            GREEN_FIN
        };
        {
            if let Ok((mut text, mut color)) = text_params.p2().get_mut(row.status_text) {
                **text = status_text;
                *color = TextColor(status_color);
            }
        }
        let progress_text = format!("{:.0}%", (progress as f64) * 100.0);
        {
            if let Ok(mut text) = text_params.p1().get_mut(row.progress_text) {
                **text = progress_text;
            }
        }
        if let Ok(mut node) = progress_fill_query.get_mut(row.progress_fill) {
            node.width = Val::Percent(progress.clamp(0.0, 1.0) * 100.0);
        }
    }
}

// Update the Overview body's four value rows every frame.
pub fn update_overview_body(
    ui_state: Res<ConstructionUiState>,
    colonies: Query<(Entity, &crate::colony::Colony)>,
    projects: Query<&crate::colony::ConstructionProject>,
    mut value_query: Query<(&OverviewRowValue, &mut Text, &mut TextColor)>,
) {
    let selected_colony = ui_state.selected_colony;
    let colony_data = selected_colony.and_then(|e| {
        colonies
            .iter()
            .find(|(ce, _)| *ce == e)
            .map(|(_, c)| c.clone())
    });

    let project_count: u32 = selected_colony
        .map(|colony_entity| {
            projects
                .iter()
                .filter(|p| p.colony_entity == colony_entity)
                .count() as u32
        })
        .unwrap_or(0);

    for (marker, mut text, mut color) in value_query.iter_mut() {
        let (new_text, new_color) = match marker.kind {
            OverviewRowKind::Colony => match &colony_data {
                Some(c) => (c.name.clone(), CYAN),
                None => ("(no colony selected)".to_string(), ORANGE_ORE),
            },
            OverviewRowKind::Population => match &colony_data {
                Some(c) => (format!("{:.0}", c.population), TEXT_BODY),
                None => ("—".to_string(), TEXT_DIM),
            },
            OverviewRowKind::ActiveConstruction => {
                let c = if project_count == 0 {
                    GREEN_OK
                } else {
                    YELLOW_ETA
                };
                (format!("{}", project_count), c)
            }
            OverviewRowKind::UniqueBuildingTypes => match &colony_data {
                Some(c) => (format!("{}", c.buildings.len()), TEXT_BODY),
                None => ("—".to_string(), TEXT_DIM),
            },
        };
        **text = new_text;
        *color = TextColor(new_color);
    }
}
