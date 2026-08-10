//! Construction scrollbar — shared chrome for all scrollable tabs
//! (Build + Mining + Buildings).

use bevy::input::mouse::MouseWheel;
use bevy::picking::events::{Press, Release};
use bevy::picking::hover::HoverMap;
use bevy::picking::pointer::{PointerButton, PointerId};
use bevy::prelude::*;
use bevy::ui::RelativeCursorPosition;
use bevy::window::CursorMoved;

use crate::ui::bevy_theme::*;
use super::markers::*;
use super::state::*;

// Drive the always-visible scrollbar overlay on the card grid.
pub fn tick_construction_scrollbar(
    mut params: ParamSet<(
        Query<(&ComputedNode, &ConstructionScrollbarTrack)>,
        Query<&ComputedNode>,
        Query<&ScrollPosition>,
        Query<&mut Node, With<ConstructionScrollbarThumb>>,
    )>,
    ui_state: Res<ConstructionUiState>,
    mut metrics: ResMut<ConstructionScrollbarMetrics>,
) {
    let track_data: Option<(f32, Entity)> = {
        let p0 = params.p0();
        let active = ConstructionTabBody::from_tab(ui_state.selected_tab);
        p0.iter()
            .find(|(_, track)| track.tab == active)
            .map(|(c, track)| (c.size().y, track.target))
    };
    let Some((track_height, scrollable_entity)) = track_data else {
        return;
    };
    let (grid_size, grid_content_height, scroll_y) = {
        let p1 = params.p1();
        let Some(grid_computed) = p1.get(scrollable_entity).ok() else {
            return;
        };
        let grid_size = grid_computed.size();
        let grid_content_height = grid_computed.content_size().y;
        let scroll_y = {
            let p2 = params.p2();
            let Some(scroll_pos) = p2.get(scrollable_entity).ok() else {
                return;
            };
            scroll_pos.y
        };
        (grid_size, grid_content_height, scroll_y)
    };
    let usable_track = (track_height - 16.0).max(0.0);
    if grid_content_height <= grid_size.y || grid_size.y <= 0.0 {
        for mut node in params.p3().iter_mut() {
            node.height = Val::Px(0.0);
            node.top = Val::Px(0.0);
        }
        metrics.usable_track_height = usable_track;
        metrics.thumb_height = 0.0;
        metrics.max_scroll = 0.0;
        return;
    }
    let ratio = (grid_size.y / grid_content_height).clamp(0.0, 1.0);
    let thumb_height = (usable_track * ratio).max(12.0);
    let max_scroll_y = (grid_content_height - grid_size.y).max(0.0);
    let scroll_progress = if max_scroll_y > 0.0 {
        (scroll_y / max_scroll_y).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let thumb_y = scroll_progress * (usable_track - thumb_height);
    metrics.usable_track_height = usable_track;
    metrics.thumb_height = thumb_height;
    metrics.max_scroll = max_scroll_y;
    for mut node in params.p3().iter_mut() {
        node.height = Val::Px(thumb_height);
        node.top = Val::Px(thumb_y);
    }
}

// Drag-to-scroll for the construction card grid scrollbar.
#[derive(Resource, Default)]
pub(crate) struct ScrollbarDragState {
    pub(crate) active: bool,
    pub(crate) started_on_track: bool,
    pub(crate) press_track_y: f32,
}

// On-press observer for the `ConstructionScrollbarThumb`.
fn on_thumb_press(on: On<Pointer<Press>>, mut drag: ResMut<ScrollbarDragState>) {
    if on.event.button != PointerButton::Primary {
        return;
    }
    drag.active = true;
    drag.started_on_track = false;
    drag.press_track_y = 0.0;
}

// On-release observer for the `ConstructionScrollbarThumb`.
fn on_thumb_release(on: On<Pointer<Release>>, mut drag: ResMut<ScrollbarDragState>) {
    if on.event.button != PointerButton::Primary {
        return;
    }
    drag.active = false;
    drag.started_on_track = false;
}

// On-press observer for the `ConstructionScrollbarTrack`.
fn on_track_press(
    on: On<Pointer<Press>>,
    mut drag: ResMut<ScrollbarDragState>,
    track_query: Query<&RelativeCursorPosition, With<ConstructionScrollbarTrack>>,
) {
    if on.event.button != PointerButton::Primary {
        return;
    }
    drag.active = true;
    drag.started_on_track = true;
    let y = track_query
        .get(on.entity)
        .ok()
        .and_then(|rcp| rcp.normalized)
        .map(|n| n.y)
        .unwrap_or(0.0);
    drag.press_track_y = y;
}

// On-release observer for the `ConstructionScrollbarTrack`.
fn on_track_release(on: On<Pointer<Release>>, mut drag: ResMut<ScrollbarDragState>) {
    if on.event.button != PointerButton::Primary {
        return;
    }
    drag.active = false;
    drag.started_on_track = false;
}

pub fn tick_construction_scrollbar_drag(
    mut cursor_events: MessageReader<CursorMoved>,
    ui_state: Res<ConstructionUiState>,
    tracks: Query<&ConstructionScrollbarTrack>,
    mut scrollable_query: Query<&mut ScrollPosition>,
    metrics: Res<ConstructionScrollbarMetrics>,
    mut drag: ResMut<ScrollbarDragState>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
) {
    if !drag.active {
        cursor_events.clear();
        return;
    }
    if !mouse_buttons.pressed(MouseButton::Left) {
        drag.active = false;
        drag.started_on_track = false;
        cursor_events.clear();
        return;
    }

    let active = ConstructionTabBody::from_tab(ui_state.selected_tab);
    let scrollable = tracks
        .iter()
        .find(|t| t.tab == active)
        .map(|t| t.target);
    let Some(scrollable_entity) = scrollable else {
        drag.active = false;
        drag.started_on_track = false;
        cursor_events.clear();
        return;
    };

    let travel = (metrics.usable_track_height - metrics.thumb_height).max(1.0);
    let factor = metrics.max_scroll / travel;

    if drag.started_on_track {
        let click_y_px: f32 = drag.press_track_y * metrics.usable_track_height;
        let target_thumb_y: f32 = (click_y_px - metrics.thumb_height * 0.5).clamp(
            0.0,
            (metrics.usable_track_height - metrics.thumb_height).max(0.0),
        );
        let target_scroll: f32 = target_thumb_y * factor;
        if let Ok(mut pos) = scrollable_query.get_mut(scrollable_entity) {
            pos.y = target_scroll.clamp(0.0_f32, metrics.max_scroll);
        }
        cursor_events.clear();
        drag.started_on_track = false;
        return;
    }

    for event in cursor_events.read() {
        let Some(delta) = event.delta else { continue };
        let dy = delta.y;
        if let Ok(mut pos) = scrollable_query.get_mut(scrollable_entity) {
            pos.y = (pos.y + dy * factor).clamp(0.0, metrics.max_scroll);
        }
    }
}

// Shared layout numbers from `tick_construction_scrollbar`.
#[derive(Resource, Default, Debug)]
pub struct ConstructionScrollbarMetrics {
    pub usable_track_height: f32,
    pub thumb_height: f32,
    pub max_scroll: f32,
}

// Wheel-scroll handler for `Overflow::scroll_y` containers.
pub fn tick_ui_scroll_on_wheel(
    mut wheel_events: MessageReader<MouseWheel>,
    hover_map: Res<HoverMap>,
    mut nodes: ParamSet<(
        Query<(Entity, &Node, &ComputedNode)>,
        Query<(Entity, &mut ScrollPosition, &ComputedNode)>,
    )>,
    parents: Query<&ChildOf>,
    tracks: Query<&ConstructionScrollbarTrack>,
    ui_state: Res<ConstructionUiState>,
    card_grids: Query<Entity, (With<CardGrid>, With<Node>)>,
    mining_contents: Query<Entity, (With<MiningContent>, With<Node>)>,
    buildings_contents: Query<Entity, (With<BuildingsContent>, With<Node>)>,
) {
    for event in wheel_events.read() {
        if event.y == 0.0 {
            continue;
        }
        let pointer_id = PointerId::Mouse;
        let Some(hovered_entities) = hover_map.0.get(&pointer_id) else {
            continue;
        };
        if hovered_entities.is_empty() {
            continue;
        };
        let start_entity = *hovered_entities
            .keys()
            .next()
            .expect("non-empty checked above");
        let mut scrollable: Option<Entity> = None;
        if let Ok(track) = tracks.get(start_entity) {
            scrollable = Some(track.target);
        } else {
            let mut cursor = start_entity;
            for _ in 0..10 {
                if let Ok(track) = tracks.get(cursor) {
                    scrollable = Some(track.target);
                    break;
                }
                let Ok(parent) = parents.get(cursor) else {
                    break;
                };
                cursor = parent.0;
            }
        }
        if scrollable.is_none() {
            let mut cursor = start_entity;
            loop {
                if let Ok((_entity, node, _computed)) = nodes.p0().get(cursor) {
                    if matches!(node.overflow.y, OverflowAxis::Scroll) {
                        scrollable = Some(cursor);
                        break;
                    }
                }
                let Ok(parent) = parents.get(cursor) else {
                    break;
                };
                cursor = parent.0;
            }
        }
        if scrollable.is_none() {
            scrollable = match ui_state.selected_tab {
                ConstructionTab::Mining => mining_contents.iter().next(),
                ConstructionTab::Build => card_grids.iter().next(),
                ConstructionTab::Buildings => buildings_contents.iter().next(),
                _ => None,
            };
        }
        let Some(scrollable_entity) = scrollable else {
            continue;
        };
        if let Ok((_, mut pos, computed)) = nodes.p1().get_mut(scrollable_entity) {
            let max_y = (computed.content_size().y - computed.size().y).max(0.0);
            let new_y = (pos.0.y - event.y * 24.0).clamp(0.0, max_y);
            pos.0.y = new_y;
        }
    }
}

// Shared helper: spawn the always-visible vertical scrollbar
// (track + thumb + observers) parented to a panel root, aimed
// at a specific scrollable body.
pub fn spawn_construction_scrollbar(
    commands: &mut Commands,
    root: Entity,
    target: Entity,
    track_name: &'static str,
    track_top_px: f32,
    track_bottom_px: f32,
    tab: ConstructionTabBody,
) {
    let scrollbar_track = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(2.0),
                top: Val::Px(track_top_px),
                bottom: Val::Px(track_bottom_px),
                width: Val::Px(12.0),
                border_radius: BorderRadius::all(Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.06)),
            ZIndex(10),
            Pickable::default(),
            RelativeCursorPosition::default(),
            Name::new(track_name),
            ConstructionScrollbarTrack { target, tab },
        ))
        .id();
    commands.entity(root).add_child(scrollbar_track);
    commands.entity(scrollbar_track).observe(on_track_press);
    commands.entity(scrollbar_track).observe(on_track_release);

    let scrollbar_thumb = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                height: Val::Px(0.0),
                border_radius: BorderRadius::all(Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(CYAN.with_alpha(0.6)),
            ZIndex(11),
            Pickable::default(),
            Name::new("construction_scrollbar_thumb"),
            ConstructionScrollbarThumb,
        ))
        .id();
    commands.entity(scrollbar_track).add_child(scrollbar_thumb);
    commands.entity(scrollbar_thumb).observe(on_thumb_press);
    commands.entity(scrollbar_thumb).observe(on_thumb_release);
}

// BuildingsContent marker import — referenced by tick_ui_scroll_on_wheel
// and re-exported here for convenience.
use super::buildings::BuildingsContent;
