//! Queue panel — AppBar queue chip + slide-out panel of active projects.

use bevy::prelude::*;

use crate::ui::bevy_theme::*;
use super::markers::*;
use super::state::*;
use crate::ui::widgets::{spawn_scrollable_container, UiFonts};
use crate::colony::components::PendingConstructionActions;

// Click handler for the AppBar "OPEN QUEUE" chip.
pub fn tick_open_queue_chip_click(
    interactions: Query<(Entity, &Interaction), (With<OpenQueueChip>, With<Button>)>,
    mut state: Option<ResMut<QueuePanelState>>,
    mut prev: Local<std::collections::HashMap<Entity, Interaction>>,
) {
    crate::ui::widgets::detect_rising_edges_no_marker(&mut prev, &interactions, |_entity| {
        if let Some(ref mut s) = state {
            s.open = !s.open;
        }
    });
}

// Click handler for the QueuePanel close button.
pub fn tick_queue_panel_close_click(
    interactions: Query<(Entity, &Interaction), (With<QueuePanelClose>, With<Button>)>,
    mut state: Option<ResMut<QueuePanelState>>,
    mut prev: Local<std::collections::HashMap<Entity, Interaction>>,
) {
    crate::ui::widgets::detect_rising_edges_no_marker(&mut prev, &interactions, |_entity| {
        if let Some(ref mut s) = state {
            s.open = false;
        }
    });
}

// Esc-to-close for the QueuePanel (Phase 8). The panel is a slide-in
// drawer; pressing Escape closes it without committing.
pub fn tick_queue_panel_close_on_esc(
    keys: Res<ButtonInput<KeyCode>>,
    mut state: Option<ResMut<QueuePanelState>>,
) {
    let is_open = state.as_ref().map(|s| s.open).unwrap_or(false);
    if is_open && keys.just_pressed(KeyCode::Escape) {
        if let Some(ref mut s) = state {
            s.open = false;
        }
    }
}

// Toggle the QueuePanel visibility based on `QueuePanelState::open`.
pub fn tick_queue_panel_visibility(
    state: Option<Res<QueuePanelState>>,
    mut panel_query: Query<&mut Visibility, With<QueuePanelRoot>>,
) {
    let is_open = state.as_ref().map(|s| s.open).unwrap_or(false);
    let target = if is_open {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    for mut v in panel_query.iter_mut() {
        *v = target;
    }
}

// Update the live AppBar queue summary text.
pub fn update_queue_summary(
    mut text_query: Query<&mut Text, With<QueuePanelSummaryText>>,
    ui_state: Res<ConstructionUiState>,
    projects: Query<&crate::colony::ConstructionProject>,
    output_bp_per_year: Res<ConstructionQueue>,
) {
    let bp_per_sec = (output_bp_per_year.output_bp_per_year / 365.25 / 24.0 / 3600.0).max(1e-9);
    let total_remaining_bp: f64 = ui_state
        .selected_colony
        .map(|colony_entity| {
            projects
                .iter()
                .filter(|p| p.colony_entity == colony_entity)
                .map(|p| (p.required - p.progress).max(0.0))
                .sum()
        })
        .unwrap_or(0.0);
    let eta_seconds = total_remaining_bp / bp_per_sec;
    let text = if total_remaining_bp <= 0.0 {
        "Empty Queue".to_string()
    } else {
        format_duration_padded(eta_seconds)
    };
    for mut t in text_query.iter_mut() {
        **t = text.clone();
    }
}

// Diff-based queue row management.
pub fn update_queue_panel(
    mut commands: Commands,
    fonts: Res<UiFonts>,
    buildings_data: Res<crate::colony::data::BuildingsData>,
    ui_state: Res<ConstructionUiState>,
    output_bp_per_year: Res<ConstructionQueue>,
    projects: Query<(Entity, &crate::colony::ConstructionProject)>,
    existing_rows: Query<(Entity, &QueuePanelRow)>,
    root_query: Query<Entity, With<QueuePanelRoot>>,
    body_query: Query<Entity, With<QueuePanelBody>>,
) {
    let Ok(panel_root) = root_query.single() else {
        return;
    };
    let _ = panel_root;
    let Ok(body_root) = body_query.single() else {
        return;
    };

    let desired: std::collections::HashMap<Entity, crate::colony::ConstructionProject> = ui_state
        .selected_colony
        .map(|colony_entity| {
            projects
                .iter()
                .filter(|(_, p)| p.colony_entity == colony_entity)
                .map(|(entity, p)| (entity, p.clone()))
                .collect()
        })
        .unwrap_or_default();

    let mut existing: std::collections::HashMap<Entity, Entity> = std::collections::HashMap::new();
    for (row_entity, row) in existing_rows.iter() {
        existing.insert(row.project_entity, row_entity);
    }

    for (project_entity, row_entity) in existing.iter() {
        if !desired.contains_key(project_entity) {
            commands.entity(*row_entity).try_despawn();
        }
    }

    for (project_entity, project) in desired.iter() {
        if existing.contains_key(project_entity) {
            continue;
        }
        let display_name = buildings_data
            .get(&project.building_type)
            .map(|d| d.display_name.as_str())
            .unwrap_or("(unknown)");
        let row = spawn_queue_row(
            &mut commands,
            body_root,
            *project_entity,
            display_name,
            project,
            &fonts,
            &output_bp_per_year,
        );
        let _ = row;
    }
}

// Update the ETA text on every existing queue row every frame.
pub fn update_queue_row_eta(
    projects: Query<(Entity, &crate::colony::ConstructionProject)>,
    output_bp_per_year: Res<ConstructionQueue>,
    mut eta_text_query: Query<(&QueuePanelRowEta, &mut Text, &mut TextColor)>,
) {
    let by_entity: std::collections::HashMap<Entity, &crate::colony::ConstructionProject> =
        projects.iter().map(|(e, p)| (e, p)).collect();
    let bp_per_sec = (output_bp_per_year.output_bp_per_year / 365.25 / 24.0 / 3600.0).max(1e-9);
    for (eta_marker, mut text, mut color) in eta_text_query.iter_mut() {
        let Some(project) = by_entity.get(&eta_marker.project_entity) else {
            continue;
        };
        let remaining_bp = (project.required - project.progress).max(0.0);
        let eta_seconds = remaining_bp / bp_per_sec;
        if project.awaiting_resources {
            **text = "⏳ Awaiting".to_string();
            *color = TextColor(ORANGE_ORE);
        } else if remaining_bp <= 0.0 {
            **text = "Done".to_string();
            *color = TextColor(GREEN_OK);
        } else {
            **text = format_duration_padded(eta_seconds);
            *color = TextColor(YELLOW_ETA);
        }
    }
}

// Update the progress-bar fill width on every existing queue row
// every frame. Phase 10: now a thin shim over the generic
// `widgets::tick_progress_fill`. We pre-compute the per-entity
// progress percentages so the generic system (which only knows the
// `f32` value, not the project lookup) can write the width.
pub fn update_queue_row_progress(
    projects: Query<(Entity, &crate::colony::ConstructionProject)>,
    mut fill_query: Query<(Entity, &mut QueuePanelRowFill, &mut Node)>,
) {
    for (entity, mut fill, mut node) in fill_query.iter_mut() {
        let Ok((_, project)) = projects.get(entity) else { continue };
        fill.0 = project.progress_percent().clamp(0.0, 1.0) as f32;
        // The actual Node.width write happens in tick_progress_fill;
        // we only set the percentage here.
        let _ = node;
    }
}

// Spawn a single row in the queue panel for a given
// `ConstructionProject`.
fn spawn_queue_row(
    commands: &mut Commands,
    parent: Entity,
    project_entity: Entity,
    display_name: &str,
    project: &crate::colony::ConstructionProject,
    fonts: &UiFonts,
    output_bp_per_year: &Res<ConstructionQueue>,
) -> Entity {
    let body_font: Handle<Font> = fonts.body.clone();
    let body_font_medium: Handle<Font> = fonts.medium.clone();
    let mono_font: Handle<Font> = fonts.mono.clone();
    let _ = (body_font.clone(), body_font_medium.clone());

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
            Name::new("queue_row"),
            QueuePanelRow { project_entity },
        ))
        .id();
    commands.entity(parent).add_child(row);

    let header = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                width: Val::Percent(100.0),
                column_gap: Val::Px(SPACE_SM),
                ..default()
            },
            Name::new("queue_row_header"),
        ))
        .id();
    commands.entity(row).add_child(header);

    let name = commands
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
            Name::new("queue_row_name"),
        ))
        .id();
    commands.entity(header).add_child(name);

    let bp_per_sec = (output_bp_per_year.output_bp_per_year / 365.25 / 24.0 / 3600.0).max(1e-9);
    let remaining_bp = (project.required - project.progress).max(0.0);
    let eta_seconds = remaining_bp / bp_per_sec;
    let eta_text = if project.awaiting_resources {
        "⏳ Awaiting".to_string()
    } else {
        format_duration_compact(eta_seconds)
    };
    let eta_color = if project.awaiting_resources {
        ORANGE_ORE
    } else {
        YELLOW_ETA
    };
    let eta = commands
        .spawn((
            Text::new(eta_text),
            TextFont {
                font: mono_font.clone(),
                font_size: CAPTION_SIZE,
                ..default()
            },
            TextColor(eta_color),
            Name::new("queue_row_eta"),
            QueuePanelRowEta { project_entity },
        ))
        .id();
    commands.entity(header).add_child(eta);

    let cancel = commands
        .spawn((
            Button,
            Node {
                display: Display::Flex,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                width: Val::Px(20.0),
                height: Val::Px(20.0),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
            BorderColor::all(CYAN_BORDER),
            Pickable::default(),
            Name::new("queue_row_cancel"),
            QueuePanelRowCancel { project_entity },
        ))
        .id();
    commands.entity(header).add_child(cancel);
    let cancel_label = commands
        .spawn((
            Text::new("×"),
            TextFont {
                font: mono_font.clone(),
                font_size: CAPTION_SIZE,
                ..default()
            },
            TextColor(CYAN),
            Name::new("queue_row_cancel_label"),
        ))
        .id();
    commands.entity(cancel).add_child(cancel_label);

    let track = commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Px(6.0),
                border_radius: BorderRadius::all(Val::Px(3.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.196, 0.529, 0.612, 0.30)),
            Name::new("queue_row_progress_track"),
        ))
        .id();
    commands.entity(row).add_child(track);
    let fill = commands
        .spawn((
            Node {
                width: Val::Percent(project.progress_percent().clamp(0.0, 1.0) * 100.0),
                height: Val::Percent(100.0),
                border_radius: BorderRadius::all(Val::Px(3.0)),
                ..default()
            },
            BackgroundColor(CYAN),
            Name::new("queue_row_progress_fill"),
            QueuePanelRowFill(0.0), // set by tick_progress_fill each frame
        ))
        .id();
    commands.entity(track).add_child(fill);

    row
}

// Click handler for the cancel button on each queue row.
pub fn tick_queue_panel_row_cancel_click(
    interactions: Query<(Entity, &Interaction, &QueuePanelRowCancel), With<QueuePanelRowCancel>>,
    mut pending: ResMut<PendingConstructionActions>,
    mut prev: Local<std::collections::HashMap<Entity, Interaction>>,
) {
    crate::ui::widgets::detect_rising_edges(&mut prev, &interactions, |_entity, cancel| {
        pending.cancel_construction.push(cancel.project_entity);
    });
}

// Re-export format_duration helpers from bevy_theme for local use.
use crate::ui::bevy_theme::format_duration_compact;
use crate::ui::bevy_theme::format_duration_padded;

// Re-export spawn_scrollable_container so callers can reference it.
pub(super) use crate::ui::widgets::spawn_scrollable_container as _spawn_scrollable_container_export;
