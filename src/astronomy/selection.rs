use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use super::components::{
    CurrentStarSystem, HoverMarker, Hovered, MarkerDot, MarkerOwner, Selected, SelectionMarker,
    SystemId,
};
use super::systems::SCALING_FACTOR;
use crate::game_state::ActiveMenu;
use crate::plugins::camera::{CameraAnchor, EguiPanelBounds, GameCamera, OrbitCamera, ViewMode};
use crate::plugins::solar_system::{CelestialBody, ClickExcluded, LogicalParent, Moon, Star};
use crate::plugins::solar_system_data::{calculate_visual_radius, BodyType};
use crate::ui::FleetUiState;

/// Click radius for body selection (in Bevy units)
const SELECTION_CLICK_RADIUS: f32 = 45.0;

/// Padding for the hover ring around celestial bodies (in Bevy units)
const HOVER_RING_PADDING: f32 = 8.0;

/// Emissive highlight colour applied to ring meshes when selected or hovered.
const RING_HIGHLIGHT_COLOR: LinearRgba = LinearRgba::new(0.35, 0.75, 1.0, 1.0);
/// Emissive multiplier for a selected ring (brighter).
const RING_SELECTED_EMISSIVE: f32 = 3.5;
/// Emissive multiplier for a hovered ring (subtler).
const RING_HOVERED_EMISSIVE: f32 = 2.0;
/// Pulse speed (radians/sec) for the selected-ring glow animation.
const RING_PULSE_SPEED: f32 = 3.0;
/// Pulse amplitude (fraction of base emissive strength that oscillates ±).
const RING_PULSE_AMPLITUDE: f32 = 0.35;

/// Stored on a ring entity when it gains a selection/hover highlight so we can
/// restore the original emissive when the highlight ends.
#[derive(Component, Debug, Clone)]
pub struct RingHighlight {
    /// Original base color before we modified it.
    pub original_base_color: Color,
    /// Original emissive value before we modified it.
    pub original_emissive: LinearRgba,
    /// Base emissive strength applied (before pulse modulation).
    pub base_strength: f32,
    /// Whether this is a selection highlight (true) or hover highlight (false).
    pub is_selected: bool,
}

#[derive(Default)]
pub struct SelectionState {
    pub last_click_time: f64,
    pub last_clicked_entity: Option<Entity>,
}

/// System that handles celestial body selection via mouse clicks
#[allow(clippy::too_many_arguments)]
pub fn handle_body_selection(
    view_mode: Res<ViewMode>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    body_query: Query<
        (
            Entity,
            &GlobalTransform,
            &CelestialBody,
            Option<&SystemId>,
            Option<&crate::plugins::solar_system::LogicalParent>,
            &Visibility,
        ),
        Without<ClickExcluded>,
    >,
    current_system: Res<CurrentStarSystem>,
    mut commands: Commands,
    selected_query: Query<Entity, With<Selected>>,
    mut anchor_query: Query<&mut CameraAnchor, With<GameCamera>>,
    mut orbit_query: Query<&mut OrbitCamera, With<GameCamera>>,
    time: Res<Time>,
    mut selection_state: Local<SelectionState>,
    mut egui_contexts: bevy_egui::EguiContexts,
    active_menu: Res<ActiveMenu>,
    panel_bounds: Res<EguiPanelBounds>,
    mut fleet_ui_state: ResMut<FleetUiState>,
) {
    // Disable body selection when a full-screen overlay menu is active
    if active_menu.current.blocks_world_interaction() {
        return;
    }

    // Disable body selection in starmap view
    if *view_mode == ViewMode::Starmap {
        return;
    }

    // Only process on mouse click
    let left_click = mouse_button.just_pressed(MouseButton::Left);
    let right_click = mouse_button.just_pressed(MouseButton::Right);
    if !left_click && !right_click {
        return;
    }

    // Don't process if egui is using the mouse (e.g., clicking on UI)
    if let Ok(ctx) = egui_contexts.ctx_mut() {
        let hover_pos = ctx.input(|i| i.pointer.hover_pos());
        let over_panel = if let Some(available) = panel_bounds.available_rect {
            hover_pos.map_or(false, |p| !available.contains(p))
        } else {
            false
        };
        if ctx.is_pointer_over_area()
            || ctx.is_using_pointer()
            || ctx.wants_pointer_input()
            || over_panel
        {
            return;
        }
    }

    let Ok(window) = windows.single() else {
        return;
    };

    let Ok((camera, camera_transform)) = camera_query.single() else {
        return;
    };

    // Get cursor position
    let Some(cursor_position) = window.cursor_position() else {
        return;
    };

    // Convert screen position to ray
    let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_position) else {
        return;
    };

    // Find the body whose center is closest to the mouse ray.
    // Using ray-distance (not camera-distance) prevents large bodies like
    // stars from stealing clicks away from smaller planets orbiting nearby.
    // Stores: (Entity, ray_distance, body name)
    let mut closest_body: Option<(Entity, f32, f32, String)> = None;

    for (entity, transform, body, system_id, _logical_parent, visibility) in body_query.iter() {
        // Only interact with bodies in the current star system
        let body_system = system_id.map(|s| s.0).unwrap_or(0);
        if body_system != current_system.0 {
            continue;
        }

        // Skip hidden bodies (e.g. moons whose parent is not anchored/selected)
        if *visibility == Visibility::Hidden {
            continue;
        }

        let body_pos = transform.translation();

        // Calculate distance from ray to body center
        let to_body = body_pos - ray.origin;
        let projection = to_body.dot(*ray.direction);

        // Skip if body is behind camera
        if projection < 0.0 {
            continue;
        }

        let closest_point = ray.origin + *ray.direction * projection;
        let distance = (body_pos - closest_point).length();

        // Check if click is within visual radius + margin
        let selection_radius = body.visual_radius + SELECTION_CLICK_RADIUS;

        if distance < selection_radius {
            match closest_body {
                None => closest_body = Some((entity, distance, projection, body.name.clone())),
                Some((_, prev_ray_dist, prev_proj, _))
                    if distance < prev_ray_dist
                        || (distance == prev_ray_dist && projection < prev_proj) =>
                {
                    closest_body = Some((entity, distance, projection, body.name.clone()));
                }
                _ => {}
            }
        }
    }

    // Deselect all currently selected bodies if left clicking
    if left_click {
        for entity in selected_query.iter() {
            commands.entity(entity).remove::<Selected>();
        }
    }

    // Select the clicked body if any
    if let Some((entity, _, _, name)) = closest_body {
        if left_click {
            commands.entity(entity).insert(Selected);
            info!("Selected celestial body: {} (entity {:?})", name, entity);

            let current_time = time.elapsed_secs_f64();
            if let Some(last_entity) = selection_state.last_clicked_entity {
                if last_entity == entity && (current_time - selection_state.last_click_time) < 0.5 {
                    info!("Double click on {}, setting anchor and recentering.", name);
                    if let Ok(mut anchor) = anchor_query.single_mut() {
                        anchor.0 = Some(entity);
                    }
                    // Also clear the pan offset to recenter the view
                    if let Ok(mut orbit) = orbit_query.single_mut() {
                        orbit.pan_offset = Vec3::ZERO;
                    }
                }
            }
            selection_state.last_click_time = current_time;
            selection_state.last_clicked_entity = Some(entity);
        } else if right_click {
            if fleet_ui_state.selected_fleet.is_some() {
                info!("Right clicked celestial body: {} (entity {:?}) with fleet selected, opening transfer planner", name, entity);
                fleet_ui_state.target_body = Some(entity);
                fleet_ui_state.target_lagrange = None;
                fleet_ui_state.target_fleet = None;
                fleet_ui_state.computed_options.clear();
                fleet_ui_state.planned_transfer = None;
                fleet_ui_state.selected_option = 0;
                fleet_ui_state.selected_gravity_assist = None;
                fleet_ui_state.show_transfer_popup = true;
                fleet_ui_state.departure_offset_days = -1.0; // Signal to auto-set to next window
            }
        }
    } else if left_click {
        selection_state.last_clicked_entity = None;
    }
}

/// System that handles celestial body hover detection via mouse position
pub fn handle_body_hover(
    view_mode: Res<ViewMode>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    body_query: Query<
        (
            Entity,
            &GlobalTransform,
            &CelestialBody,
            Option<&SystemId>,
            &Visibility,
        ),
        Without<ClickExcluded>,
    >,
    current_system: Res<CurrentStarSystem>,
    mut commands: Commands,
    hovered_query: Query<Entity, With<Hovered>>,
    mut egui_contexts: bevy_egui::EguiContexts,
    active_menu: Res<ActiveMenu>,
    panel_bounds: Res<EguiPanelBounds>,
    fleet_ui_state: Res<FleetUiState>,
) {
    // Disable hover when a full-screen menu overlay is active (Research, etc.)
    if active_menu.current.blocks_world_interaction() {
        for entity in hovered_query.iter() {
            commands.entity(entity).remove::<Hovered>();
        }
        return;
    }

    // Disable hover in starmap view
    if *view_mode == ViewMode::Starmap {
        for entity in hovered_query.iter() {
            commands.entity(entity).remove::<Hovered>();
        }
        return;
    }

    // Safety check: ensure we have access to egui context
    // If the cursor is over a UI element, don't perform world picking
    if let Ok(ctx) = egui_contexts.ctx_mut() {
        let hover_pos = ctx.input(|i| i.pointer.hover_pos());
        let over_panel = if let Some(available) = panel_bounds.available_rect {
            hover_pos.map_or(false, |p| !available.contains(p))
        } else {
            false
        };
        if ctx.is_pointer_over_area() || ctx.is_using_pointer() || over_panel {
            // Clear all hovers if we are over UI
            for entity in hovered_query.iter() {
                commands.entity(entity).remove::<Hovered>();
            }
            return;
        }
    }

    let Ok(window) = windows.single() else {
        return;
    };

    let Ok((camera, camera_transform)) = camera_query.single() else {
        return;
    };

    // Get cursor position
    let Some(cursor_position) = window.cursor_position() else {
        // No cursor, clear all hovers
        for entity in hovered_query.iter() {
            commands.entity(entity).remove::<Hovered>();
        }
        return;
    };

    // Convert screen position to ray
    let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_position) else {
        return;
    };

    // Find the body whose center is closest to the mouse ray.
    // Using ray-distance (not camera-distance) prevents large bodies like
    // stars from stealing hovers away from smaller planets orbiting nearby.
    let mut closest_body: Option<(Entity, f32, f32)> = None;

    for (entity, transform, body, system_id, visibility) in body_query.iter() {
        // Only interact with bodies in the current star system
        let body_system = system_id.map(|s| s.0).unwrap_or(0);
        if body_system != current_system.0 {
            continue;
        }

        // Skip hidden bodies (e.g. moons whose parent is not anchored/selected)
        if *visibility == Visibility::Hidden {
            continue;
        }

        let body_pos = transform.translation();

        // Calculate distance from ray to body center
        let to_body = body_pos - ray.origin;
        let projection = to_body.dot(*ray.direction);

        // Skip if body is behind camera
        if projection < 0.0 {
            continue;
        }

        let closest_point = ray.origin + *ray.direction * projection;
        let distance = (body_pos - closest_point).length();

        // Check if cursor is within hover radius (visual radius + margin)
        let selection_radius = body.visual_radius + SELECTION_CLICK_RADIUS;
        if distance < selection_radius {
            match closest_body {
                None => closest_body = Some((entity, distance, projection)),
                Some((_, prev_ray_dist, prev_proj))
                    if distance < prev_ray_dist
                        || (distance == prev_ray_dist && projection < prev_proj) =>
                {
                    closest_body = Some((entity, distance, projection));
                }
                _ => {}
            }
        }
    }

    // Only change Hovered when the target entity actually changes.
    // Unconditionally removing+re-inserting every frame triggers Added<Hovered>
    // each frame, which spawns a fresh marker at Transform::default() (the star's
    // origin position) before scale_markers_with_zoom can reposition it.
    let new_hover = closest_body.map(|(e, _, _)| e);
    let hover_is_body = new_hover.is_some();
    // Use crosshair only while the transfer planner popup is open.
    // A selected fleet that is merely being inspected (no active planning) keeps the default cursor.
    let planner_mode_active = fleet_ui_state.show_transfer_popup;
    let currently_hovered: Vec<Entity> = hovered_query.iter().collect();

    // Remove Hovered from entities no longer under the cursor
    for entity in &currently_hovered {
        if new_hover != Some(*entity) {
            commands.entity(*entity).remove::<Hovered>();
        }
    }

    // Insert Hovered only on a newly-hovered entity
    if let Some(entity) = new_hover {
        if !currently_hovered.contains(&entity) {
            commands.entity(entity).insert(Hovered);
        }
    }

    if let Ok(ctx) = egui_contexts.ctx_mut() {
        ctx.output_mut(|o| {
            o.cursor_icon = if planner_mode_active {
                bevy_egui::egui::CursorIcon::Crosshair
            } else if hover_is_body {
                bevy_egui::egui::CursorIcon::PointingHand
            } else {
                bevy_egui::egui::CursorIcon::Default
            };
        });
    }
}

/// System that spawns glossy selection markers for newly selected bodies.
///
/// Also performs defensive cleanup: when a new selection is detected, any
/// existing `SelectionMarker` whose owner is NOT the newly selected body
/// is despawned immediately. This prevents stale markers from remaining
/// visible when `RemovedComponents<Selected>` misses an event (e.g. due
/// to same-frame remove+re-add across different schedules).
pub fn spawn_selection_markers(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    selected_query: Query<(Entity, &CelestialBody, &GlobalTransform), Added<Selected>>,
    hover_markers: Query<(Entity, &MarkerOwner), With<HoverMarker>>,
    existing_selection_markers: Query<(Entity, &MarkerOwner), With<SelectionMarker>>,
    all_selected: Query<(), With<Selected>>,
    camera_query: Query<&GlobalTransform, With<GameCamera>>,
    orbit_camera_query: Query<&OrbitCamera, With<GameCamera>>,
) {
    // Early exit: nothing newly selected
    if selected_query.is_empty() {
        return;
    }

    let camera_pos = camera_query
        .single()
        .ok()
        .map(|t| t.translation())
        .unwrap_or(Vec3::ZERO);
    let zoom_scale = orbit_camera_query
        .single()
        .ok()
        .map(|oc| (oc.radius / 1000.0_f32).clamp(1.0, 3.0))
        .unwrap_or(1.0);

    // Defensive cleanup: despawn any existing selection markers whose owner
    // no longer has Selected. This catches stale markers that
    // despawn_selection_markers might miss due to schedule timing.
    for (marker_entity, owner) in existing_selection_markers.iter() {
        if all_selected.get(owner.0).is_err() {
            commands.entity(marker_entity).despawn();
        }
    }

    for (entity, body, gtransform) in selected_query.iter() {
        // Remove hover marker if it exists
        for (marker_entity, owner) in hover_markers.iter() {
            if owner.0 == entity {
                commands.entity(marker_entity).despawn();
            }
        }

        // Also remove any existing selection marker for this entity
        // to avoid duplicates if Selected was removed and re-added.
        for (marker_entity, owner) in existing_selection_markers.iter() {
            if owner.0 == entity {
                commands.entity(marker_entity).despawn();
            }
        }

        // Rings are highlighted by modifying their own material emissive
        // (handled by `apply_ring_highlight` system) — no 3D marker needed.
        if body.body_type == BodyType::Ring {
            continue;
        }

        let marker_radius = body.visual_radius + HOVER_RING_PADDING;
        spawn_marker(
            &mut commands,
            &mut meshes,
            &mut materials,
            entity,
            gtransform.translation(),
            camera_pos,
            zoom_scale,
            marker_radius,
            true,
        );
    }
}

/// System that removes selection markers when selection is cleared.
pub fn despawn_selection_markers(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut removed_selected: RemovedComponents<Selected>,
    marker_query: Query<(Entity, &MarkerOwner), With<SelectionMarker>>,
    body_query: Query<(&CelestialBody, Option<&Hovered>, &GlobalTransform)>,
    camera_query: Query<&GlobalTransform, With<GameCamera>>,
    orbit_camera_query: Query<&OrbitCamera, With<GameCamera>>,
) {
    let camera_pos = camera_query
        .single()
        .ok()
        .map(|t| t.translation())
        .unwrap_or(Vec3::ZERO);
    let zoom_scale = orbit_camera_query
        .single()
        .ok()
        .map(|oc| (oc.radius / 1000.0_f32).clamp(1.0, 3.0))
        .unwrap_or(1.0);

    for entity in removed_selected.read() {
        for (marker_entity, owner) in marker_query.iter() {
            if owner.0 == entity {
                commands.entity(marker_entity).despawn();
            }
        }

        // If still hovered, add a hover marker (rings are handled separately)
        if let Ok((body, Some(_), gtransform)) = body_query.get(entity) {
            if body.body_type == BodyType::Ring {
                continue;
            }
            let marker_radius = body.visual_radius + HOVER_RING_PADDING;
            spawn_marker(
                &mut commands,
                &mut meshes,
                &mut materials,
                entity,
                gtransform.translation(),
                camera_pos,
                zoom_scale,
                marker_radius,
                false,
            );
        }
    }
}

/// System that spawns glossy hover markers for newly hovered bodies.
pub fn spawn_hover_markers(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    hovered_query: Query<
        (Entity, &CelestialBody, &GlobalTransform),
        (Added<Hovered>, Without<Selected>),
    >,
    camera_query: Query<&GlobalTransform, With<GameCamera>>,
    orbit_camera_query: Query<&OrbitCamera, With<GameCamera>>,
) {
    let camera_pos = camera_query
        .single()
        .ok()
        .map(|t| t.translation())
        .unwrap_or(Vec3::ZERO);
    let zoom_scale = orbit_camera_query
        .single()
        .ok()
        .map(|oc| (oc.radius / 1000.0_f32).clamp(1.0, 3.0))
        .unwrap_or(1.0);

    for (entity, body, gtransform) in hovered_query.iter() {
        // Rings are highlighted via material emissive, not a 3D marker.
        if body.body_type == BodyType::Ring {
            continue;
        }

        let marker_radius = body.visual_radius + HOVER_RING_PADDING;
        spawn_marker(
            &mut commands,
            &mut meshes,
            &mut materials,
            entity,
            gtransform.translation(),
            camera_pos,
            zoom_scale,
            marker_radius,
            false,
        );
    }
}

/// System that removes hover markers when hover ends.
pub fn despawn_hover_markers(
    mut commands: Commands,
    mut removed_hovered: RemovedComponents<Hovered>,
    marker_query: Query<(Entity, &MarkerOwner), With<HoverMarker>>,
    selected_query: Query<(), With<Selected>>,
) {
    for entity in removed_hovered.read() {
        // Skip if the entity is now selected - spawn_selection_markers already handles it
        if selected_query.get(entity).is_ok() {
            continue;
        }

        for (marker_entity, owner) in marker_query.iter() {
            if owner.0 == entity {
                commands.entity(marker_entity).despawn();
            }
        }
    }
}

/// Safety cleanup: despawn any `SelectionMarker` whose owner no longer has `Selected`.
///
/// `RemovedComponents` can miss stale markers when the same entity loses and regains
/// `Selected` in one frame (e.g. the egui tree fires `clicked()` + `double_clicked()`
/// simultaneously while `handle_body_selection` also deselects-then-reselects).  This
/// system runs after `despawn_selection_markers` to catch any leftovers.
pub fn cleanup_stale_selection_markers(
    mut commands: Commands,
    marker_query: Query<(Entity, &MarkerOwner), With<SelectionMarker>>,
    selected_query: Query<(), With<Selected>>,
) {
    for (marker_entity, owner) in marker_query.iter() {
        if selected_query.get(owner.0).is_err() {
            commands.entity(marker_entity).despawn();
        }
    }
}

/// System that animates marker dots around selection/hover rings.
pub fn animate_marker_dots(time: Res<Time>, mut query: Query<(&mut Transform, &mut MarkerDot)>) {
    for (mut transform, mut dot) in query.iter_mut() {
        dot.angle =
            (dot.angle + dot.angular_speed * time.delta_secs()).rem_euclid(std::f32::consts::TAU);
        transform.translation = Vec3::new(
            dot.radius * dot.angle.cos(),
            0.0,
            dot.radius * dot.angle.sin(),
        );
    }
}

fn spawn_marker(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    owner: Entity,
    initial_position: Vec3,
    camera_position: Vec3,
    zoom_scale: f32,
    radius: f32,
    is_selected: bool,
) {
    // Make hovered markers slightly brighter; selected/anchored markers a bit darker
    let ring_color = if is_selected {
        // Darker, more subdued color for selected/anchored
        Color::srgb(0.3, 0.65, 0.85)
    } else {
        // Brighter color for hover to indicate immediacy
        Color::srgb(0.5, 0.9, 1.0)
    };

    let emissive_strength = if is_selected { 2.5 } else { 4.0 };
    let bracket_material = materials.add(StandardMaterial {
        base_color: ring_color,
        emissive: LinearRgba::from(ring_color) * emissive_strength,
        unlit: true,
        ..default()
    });

    // Create a parent entity for the reticle (no mesh, just a transform anchor).
    // Spawn with the correct position, billboard rotation, and zoom scale immediately
    // so there is no one-frame flash with wrong orientation or size.
    let initial_rotation = {
        let dir = (camera_position - initial_position).normalize_or_zero();
        if dir != Vec3::ZERO {
            Quat::from_rotation_arc(Vec3::Y, dir)
        } else {
            Quat::IDENTITY
        }
    };
    let initial_transform = Transform {
        translation: initial_position,
        rotation: initial_rotation,
        scale: Vec3::splat(zoom_scale),
    };
    let marker_entity = commands
        .spawn((initial_transform, Visibility::default(), MarkerOwner(owner)))
        .id();

    if is_selected {
        commands.entity(marker_entity).insert(SelectionMarker);
    } else {
        commands.entity(marker_entity).insert(HoverMarker);
    }

    // IMPORTANT: Do NOT parent the marker to the owner.
    // We want the marker to be:
    // 1. "Billboarded" (always facing the camera)
    // 2. "Stationary" relative to rotation (not spinning with the planet)
    //
    // Trying to billboard a child of a rotating parent is complex because standard billboarding
    // is overridden by parent rotation. Instead, we keep the marker detached and
    // manually sync its position every frame in `scale_markers_with_zoom`.
    // commands.entity(marker_entity).set_parent(owner);

    {
        // Create corner brackets using boxes
        // Each corner has two bars forming an L-shape
        let bracket_thickness = (radius * 0.08).max(2.0); // Scale with body size, minimum 2.0
        let bracket_length = radius * 0.30; // Length of each bracket arm
                                            // Corner sits at exactly the ring radius so arms are always outside the body sphere:
                                            // the perpendicular distance of each arm from center equals `radius` > visual_radius.
        let bracket_offset = radius;

        // Define four corners and create L-shaped brackets at each
        let corners = [
            // Top-right (positive X, positive Z)
            (1.0, 1.0),
            // Top-left (negative X, positive Z)
            (-1.0, 1.0),
            // Bottom-left (negative X, negative Z)
            (-1.0, -1.0),
            // Bottom-right (positive X, negative Z)
            (1.0, -1.0),
        ];

        for (x_sign, z_sign) in corners {
            let corner_x = bracket_offset * x_sign;
            let corner_z = bracket_offset * z_sign;

            // Horizontal bar extending inward from corner (along X axis)
            // Add bracket_thickness so the bar extends half-a-thickness past the
            // corner point, filling the outer-corner gap where the two arms meet.
            let h_bar_mesh = meshes.add(Cuboid::new(
                bracket_length + bracket_thickness,
                bracket_thickness,
                bracket_thickness,
            ));
            let h_bar_pos = Vec3::new(corner_x - x_sign * bracket_length * 0.5, 0.0, corner_z);

            commands.entity(marker_entity).with_children(|parent| {
                parent.spawn((
                    Mesh3d(h_bar_mesh),
                    MeshMaterial3d(bracket_material.clone()),
                    Transform::from_translation(h_bar_pos),
                ));
            });

            // Vertical bar extending inward from corner (along Z axis)
            let v_bar_mesh = meshes.add(Cuboid::new(
                bracket_thickness,
                bracket_thickness,
                bracket_length + bracket_thickness,
            ));
            let v_bar_pos = Vec3::new(corner_x, 0.0, corner_z - z_sign * bracket_length * 0.5);

            commands.entity(marker_entity).with_children(|parent| {
                parent.spawn((
                    Mesh3d(v_bar_mesh),
                    MeshMaterial3d(bracket_material.clone()),
                    Transform::from_translation(v_bar_pos),
                ));
            });
        }
    }
}

// ── Ring highlight via material emissive ──────────────────────────────────────

/// Applies an emissive glow to a ring's own `StandardMaterial` when it gains
/// `Selected` or `Hovered`.  Stores the original emissive in a [`RingHighlight`]
/// component so it can be restored later.
pub fn apply_ring_highlight(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    selected_rings: Query<
        (Entity, &MeshMaterial3d<StandardMaterial>),
        (Added<Selected>, With<crate::plugins::solar_system::Ring>),
    >,
    hovered_rings: Query<
        (Entity, &MeshMaterial3d<StandardMaterial>),
        (
            Added<Hovered>,
            Without<Selected>,
            With<crate::plugins::solar_system::Ring>,
        ),
    >,
    existing_highlight: Query<&RingHighlight>,
) {
    for (entity, mat_handle) in selected_rings.iter().chain(hovered_rings.iter()) {
        let is_selected = selected_rings.get(entity).is_ok();
        let strength = if is_selected {
            RING_SELECTED_EMISSIVE
        } else {
            RING_HOVERED_EMISSIVE
        };

        if let Some(mat) = materials.get_mut(&mat_handle.0) {
            // Only store the original if we haven't already (hover→select upgrade)
            let (original_base, original_emissive) =
                if let Ok(prev) = existing_highlight.get(entity) {
                    (prev.original_base_color, prev.original_emissive)
                } else {
                    (mat.base_color, mat.emissive)
                };

            let highlight_lin = RING_HIGHLIGHT_COLOR * strength;

            // Unlit materials use base_color, PBR use emissive. Set both.
            mat.base_color = Color::from(highlight_lin);
            mat.emissive = highlight_lin;

            commands.entity(entity).insert(RingHighlight {
                original_base_color: original_base,
                original_emissive,
                base_strength: strength,
                is_selected,
            });
        }
    }
}

/// Removes the emissive glow from a ring when `Selected` / `Hovered` is removed,
/// restoring the original emissive value.
pub fn remove_ring_highlight(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut removed_selected: RemovedComponents<Selected>,
    mut removed_hovered: RemovedComponents<Hovered>,
    ring_query: Query<
        (&MeshMaterial3d<StandardMaterial>, &RingHighlight),
        With<crate::plugins::solar_system::Ring>,
    >,
    // If the ring just lost Selected but is still Hovered, downgrade to hover glow.
    hovered_check: Query<(), With<Hovered>>,
) {
    let mut restore = |entity: Entity| {
        if let Ok((mat_handle, highlight)) = ring_query.get(entity) {
            // If this was a selection removal but we're still hovered, downgrade.
            if highlight.is_selected && hovered_check.get(entity).is_ok() {
                if let Some(mat) = materials.get_mut(&mat_handle.0) {
                    let highlight_lin = RING_HIGHLIGHT_COLOR * RING_HOVERED_EMISSIVE;
                    mat.base_color = Color::from(highlight_lin);
                    mat.emissive = highlight_lin;
                }
                commands.entity(entity).insert(RingHighlight {
                    original_base_color: highlight.original_base_color,
                    original_emissive: highlight.original_emissive,
                    base_strength: RING_HOVERED_EMISSIVE,
                    is_selected: false,
                });
                return;
            }

            if let Some(mat) = materials.get_mut(&mat_handle.0) {
                mat.base_color = highlight.original_base_color;
                mat.emissive = highlight.original_emissive;
            }
            commands.entity(entity).remove::<RingHighlight>();
        }
    };

    for entity in removed_selected.read() {
        restore(entity);
    }
    for entity in removed_hovered.read() {
        // Only fully restore if not still selected.
        if ring_query.get(entity).is_ok() {
            let is_sel = ring_query
                .get(entity)
                .map(|(_, h)| h.is_selected)
                .unwrap_or(false);
            if !is_sel {
                restore(entity);
            }
        }
    }
}

/// Gentle pulsing glow on selected rings so the highlight feels alive.
pub fn animate_ring_highlight(
    time: Res<Time>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    query: Query<
        (&MeshMaterial3d<StandardMaterial>, &RingHighlight),
        With<crate::plugins::solar_system::Ring>,
    >,
) {
    let t = time.elapsed_secs();
    for (mat_handle, highlight) in query.iter() {
        if !highlight.is_selected {
            continue; // Only pulse for selection, hover is static.
        }
        let pulse = 1.0 + RING_PULSE_AMPLITUDE * (t * RING_PULSE_SPEED).sin();
        if let Some(mat) = materials.get_mut(&mat_handle.0) {
            let highlight_lin = RING_HIGHLIGHT_COLOR * (highlight.base_strength * pulse);
            mat.base_color = Color::from(highlight_lin);
            mat.emissive = highlight_lin;
        }
    }
}

/// System that automatically zooms camera when anchoring to a body
pub fn zoom_camera_to_anchored_body(
    body_query: Query<(&CelestialBody, Option<&Star>)>,
    moon_parent_query: Query<&LogicalParent, With<Moon>>,
    mut camera_query: Query<
        (&mut OrbitCamera, &CameraAnchor),
        (With<GameCamera>, Changed<CameraAnchor>),
    >,
) {
    // Only trigger when camera anchor changes
    let Ok((mut orbit_camera, anchor)) = camera_query.single_mut() else {
        return;
    };

    // Check if we have an anchored body
    if let Some(anchored_entity) = anchor.0 {
        if let Ok((body, is_star)) = body_query.get(anchored_entity) {
            // Calculate appropriate zoom distance
            let zoom_distance = if is_star.is_some() {
                // For the Sun, show the entire solar system
                // Approximately 40 AU should show out to Neptune
                40.0 * SCALING_FACTOR as f32
            } else {
                let visual_radius = calculate_visual_radius(body.body_type, body.radius);

                // Check if any moon has this body as its logical parent
                let has_moons = moon_parent_query.iter().any(|lp| lp.0 == anchored_entity);

                if has_moons {
                    // Zoom to show the entire moon system
                    // Outermost moon is at ~6× parent visual radius (OUTER_MOON_MULTIPLIER),
                    // so zoom to ~2.5× that for comfortable framing
                    let target_distance = visual_radius * 15.0;
                    target_distance.clamp(200.0, 50000.0)
                } else {
                    // No moons: zoom to show the body itself
                    let target_distance = visual_radius * 20.0;
                    target_distance.clamp(50.0, 10000.0)
                }
            };

            orbit_camera.radius = zoom_distance;
        }
    }
}

/// System that updates selection and hover markers:
/// 1. Updates position to match the owner (since markers are not parented).
/// 2. Scales based on camera zoom distance.
/// 3. Billboards the marker (makes it face the camera).
pub fn scale_markers_with_zoom(
    mut commands: Commands,
    camera_query: Query<&GlobalTransform, With<GameCamera>>,
    orbit_camera_query: Query<&OrbitCamera, With<GameCamera>>,
    mut marker_query: Query<
        (Entity, &mut Transform, &MarkerOwner),
        Or<(With<SelectionMarker>, With<HoverMarker>)>,
    >,
    owner_query: Query<&GlobalTransform, Without<MarkerOwner>>,
) {
    let Ok(orbit_camera) = orbit_camera_query.single() else {
        return;
    };

    let Ok(camera_global_transform) = camera_query.single() else {
        return;
    };

    let camera_position = camera_global_transform.translation();

    // Reference distance where markers appear at their base size
    let reference_distance = 1000.0_f32;
    // Scale factor: markers grow with camera distance when zoomed out
    // Never shrink below 1.0 to prevent rings from going inside the body
    let zoom_scale = (orbit_camera.radius / reference_distance).clamp(1.0, 3.0);

    for (entity, mut transform, owner) in marker_query.iter_mut() {
        if let Ok(owner_transform) = owner_query.get(owner.0) {
            // 1. Match owner position
            let owner_position = owner_transform.translation();
            transform.translation = owner_position;

            // 2. Apply zoom scaling
            transform.scale = Vec3::splat(zoom_scale);

            // 3. Make marker face the camera (billboard effect)
            // Align local Y (plane normal) to point at camera
            let direction = (camera_position - owner_position).normalize_or_zero();
            if direction != Vec3::ZERO {
                transform.rotation = Quat::from_rotation_arc(Vec3::Y, direction);
            }
        } else {
            // Owner doesn't exist anymore (destroyed?), clean up marker
            commands.entity(entity).despawn();
        }
    }
}
