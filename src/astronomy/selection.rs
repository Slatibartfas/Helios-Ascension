use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use super::components::{
    CurrentStarSystem, HoverMarker, Hovered, MarkerDot, MarkerOwner, Selected,
    SelectionMarker, SystemId,
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

/// Selection-marker scale controls for ring bodies.
/// Ring `visual_radius` often encodes orbital span and can be much larger than planet radii,
/// so marker sizing uses a dedicated clamped range.
const RING_MARKER_RADIUS_SCALE: f32 = 0.28;
const RING_MARKER_RADIUS_MIN: f32 = 45.0;
const RING_MARKER_RADIUS_MAX: f32 = 140.0;

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
    body_query: Query<(Entity, &GlobalTransform, &CelestialBody, Option<&SystemId>, Option<&crate::plugins::solar_system::LogicalParent>, &Visibility), Without<ClickExcluded>>,
    current_system: Res<CurrentStarSystem>,
    mut commands: Commands,
    selected_query: Query<Entity, With<Selected>>,
    mut anchor_query: Query<&mut CameraAnchor, With<GameCamera>>,
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
        if ctx.is_pointer_over_area() || ctx.is_using_pointer() || ctx.wants_pointer_input() || over_panel {
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
                    info!("Double click on {}, setting anchor.", name);
                    if let Ok(mut anchor) = anchor_query.single_mut() {
                        anchor.0 = Some(entity);
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
    body_query: Query<(Entity, &GlobalTransform, &CelestialBody, Option<&SystemId>, &Visibility), Without<ClickExcluded>>,
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
pub fn spawn_selection_markers(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    selected_query: Query<(Entity, &CelestialBody, &GlobalTransform), Added<Selected>>,
    hover_markers: Query<(Entity, &MarkerOwner), With<HoverMarker>>,
    camera_query: Query<&GlobalTransform, With<GameCamera>>,
    orbit_camera_query: Query<&OrbitCamera, With<GameCamera>>,
) {
    let camera_pos = camera_query.single().ok().map(|t| t.translation()).unwrap_or(Vec3::ZERO);
    let zoom_scale = orbit_camera_query.single().ok()
        .map(|oc| (oc.radius / 1000.0_f32).clamp(1.0, 3.0))
        .unwrap_or(1.0);

    for (entity, body, gtransform) in selected_query.iter() {
        // Remove hover marker if it exists
        for (marker_entity, owner) in hover_markers.iter() {
            if owner.0 == entity {
                commands.entity(marker_entity).despawn();
            }
        }

        let marker_radius = marker_radius_for_body(body);
        spawn_marker(
            &mut commands,
            &mut meshes,
            &mut materials,
            entity,
            gtransform.translation(),
            camera_pos,
            zoom_scale,
            marker_radius,
            body.body_type == BodyType::Ring,
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
    let camera_pos = camera_query.single().ok().map(|t| t.translation()).unwrap_or(Vec3::ZERO);
    let zoom_scale = orbit_camera_query.single().ok()
        .map(|oc| (oc.radius / 1000.0_f32).clamp(1.0, 3.0))
        .unwrap_or(1.0);

    for entity in removed_selected.read() {
        for (marker_entity, owner) in marker_query.iter() {
            if owner.0 == entity {
                commands.entity(marker_entity).despawn();
            }
        }

        // If still hovered, add a hover marker
        if let Ok((body, Some(_), gtransform)) = body_query.get(entity) {
            let marker_radius = marker_radius_for_body(body);
            spawn_marker(
                &mut commands,
                &mut meshes,
                &mut materials,
                entity,
                gtransform.translation(),
                camera_pos,
                zoom_scale,
                marker_radius,
                body.body_type == BodyType::Ring,
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
    hovered_query: Query<(Entity, &CelestialBody, &GlobalTransform), (Added<Hovered>, Without<Selected>)>,
    camera_query: Query<&GlobalTransform, With<GameCamera>>,
    orbit_camera_query: Query<&OrbitCamera, With<GameCamera>>,
) {
    let camera_pos = camera_query.single().ok().map(|t| t.translation()).unwrap_or(Vec3::ZERO);
    let zoom_scale = orbit_camera_query.single().ok()
        .map(|oc| (oc.radius / 1000.0_f32).clamp(1.0, 3.0))
        .unwrap_or(1.0);

    for (entity, body, gtransform) in hovered_query.iter() {
        let marker_radius = marker_radius_for_body(body);
        spawn_marker(
            &mut commands,
            &mut meshes,
            &mut materials,
            entity,
            gtransform.translation(),
            camera_pos,
            zoom_scale,
            marker_radius,
            body.body_type == BodyType::Ring,
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

/// System that animates marker dots around selection/hover rings.
pub fn animate_marker_dots(time: Res<Time>, mut query: Query<(&mut Transform, &mut MarkerDot)>) {
    for (mut transform, mut dot) in query.iter_mut() {
        dot.angle = (dot.angle + dot.angular_speed * time.delta_secs())
            .rem_euclid(std::f32::consts::TAU);
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
    is_ring: bool,
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

    if is_ring {
        spawn_ring_glow_marker(commands, meshes, bracket_material.clone(), marker_entity, radius, is_selected);
    } else {
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

fn marker_radius_for_body(body: &CelestialBody) -> f32 {
    if body.body_type == BodyType::Ring {
        (body.visual_radius * RING_MARKER_RADIUS_SCALE)
            .clamp(RING_MARKER_RADIUS_MIN, RING_MARKER_RADIUS_MAX)
    } else {
        body.visual_radius + HOVER_RING_PADDING
    }
}

fn spawn_ring_glow_marker(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    ring_material: Handle<StandardMaterial>,
    marker_entity: Entity,
    radius: f32,
    is_selected: bool,
) {
    let segment_count = 20;
    let segment_thickness = (radius * 0.045).max(1.8);
    let segment_length = (radius * 0.18).max(7.0);

    for index in 0..segment_count {
        let angle = (index as f32 / segment_count as f32) * std::f32::consts::TAU;
        let pos = Vec3::new(radius * angle.cos(), 0.0, radius * angle.sin());
        let rot = Quat::from_rotation_y(-angle + std::f32::consts::FRAC_PI_2);

        let seg_mesh = meshes.add(Cuboid::new(segment_length, segment_thickness, segment_thickness));
        commands.entity(marker_entity).with_children(|parent| {
            parent.spawn((
                Mesh3d(seg_mesh),
                MeshMaterial3d(ring_material.clone()),
                Transform {
                    translation: pos,
                    rotation: rot,
                    ..default()
                },
            ));
        });
    }

    if is_selected {
        let dot_radius = (radius * 0.11).max(2.8);
        let dot_mesh = meshes.add(Sphere::new(dot_radius).mesh().uv(16, 8));

        commands.entity(marker_entity).with_children(|parent| {
            parent.spawn((
                Mesh3d(dot_mesh),
                MeshMaterial3d(ring_material),
                Transform::from_translation(Vec3::new(radius, 0.0, 0.0)),
                MarkerDot {
                    angle: 0.0,
                    angular_speed: 1.8,
                    radius,
                },
            ));
        });
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
