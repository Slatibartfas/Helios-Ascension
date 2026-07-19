use bevy::ecs::system::SystemParam;
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use super::components::{
    CurrentStarSystem, FloatingOrigin, HoverMarker, Hovered, KeplerOrbit, LocalOrbitAmplification,
    MarkerDot, MarkerOwner, OrbitCenter, OrbitPath, Selected, SelectionMarker, SpaceCoordinates,
    SystemId,
};
use super::systems::{orbit_position_from_true_anomaly, SCALING_FACTOR, VISUAL_SPEED_BASE};
use crate::game_state::ActiveMenu;
use crate::plugins::camera::{CameraAnchor, EguiPanelBounds, GameCamera, OrbitCamera, ViewMode};
use crate::plugins::solar_system::{CelestialBody, ClickExcluded, LogicalParent, Moon, Star};
use crate::plugins::solar_system_data::{calculate_visual_radius, BodyType};
use crate::ui::{FleetUiState, TimeScale};

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

/// Base click tolerance for orbit ring selection (Bevy units).
/// Scaled by camera distance so orbits remain clickable at any zoom level.
const ORBIT_CLICK_RADIUS_BASE: f32 = 8.0;

/// Number of sample points along each orbit for click detection.
const ORBIT_CLICK_SAMPLES: u32 = 64;

/// Stored on a ring entity when it gains a selection/hover highlight so we can
/// restore the original emissive when the highlight ends.
#[derive(Component, Debug, Clone, Reflect)]
#[reflect(Component)]
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

#[derive(SystemParam)]
pub struct BodySelectionUiParams<'w, 's> {
    egui_contexts: bevy_egui::EguiContexts<'w, 's>,
    active_menu: Res<'w, ActiveMenu>,
    panel_bounds: Res<'w, EguiPanelBounds>,
    fleet_ui_state: ResMut<'w, FleetUiState>,
    floating_origin: Option<Res<'w, FloatingOrigin>>,
}

/// Minimum distance from a point on the ray (in front of camera) to a point
/// on the finite segment [A, B].  Returns `f32::MAX` if the segment is
/// entirely behind the camera.
fn ray_to_segment_distance(ray_origin: Vec3, ray_dir: Vec3, a: Vec3, b: Vec3) -> f32 {
    // Project both endpoints onto the ray to reject behind-camera segments
    let proj_a = (a - ray_origin).dot(ray_dir);
    let proj_b = (b - ray_origin).dot(ray_dir);
    if proj_a < 0.0 && proj_b < 0.0 {
        return f32::MAX;
    }

    let ab = b - a;
    let seg_len_sq = ab.dot(ab);

    // Degenerate segment
    if seg_len_sq < 1e-12 {
        let to_a = a - ray_origin;
        let t_ray = to_a.dot(ray_dir).max(0.0);
        return (a - (ray_origin + ray_dir * t_ray)).length();
    }

    // Closest point on the segment to each candidate on the ray.
    // Strategy: clamp segment param, recompute ray param, then distance.
    let ao = ray_origin - a;
    let dot_ab_dir = ab.dot(ray_dir);
    let dot_ab_ab = seg_len_sq;
    let dot_ao_ab = ao.dot(ab);
    let dot_ao_dir = ao.dot(ray_dir);
    let denom = dot_ab_ab - dot_ab_dir * dot_ab_dir;

    let mut best = f32::MAX;

    if denom.abs() > 1e-10 {
        // Unclamped closest approach parameters
        let t_seg_unc = (-dot_ao_ab * 1.0 + dot_ab_dir * dot_ao_dir) / denom;
        let t_seg = t_seg_unc.clamp(0.0, 1.0);

        // Re-derive ray param for the clamped segment point
        let seg_pt = a + ab * t_seg;
        let to_seg = seg_pt - ray_origin;
        let t_ray = to_seg.dot(ray_dir).max(0.0);
        let ray_pt = ray_origin + ray_dir * t_ray;
        let d = (seg_pt - ray_pt).length();
        if d < best {
            best = d;
        }
    }

    // Also check endpoints explicitly (covers edge cases)
    for &endpoint in &[a, b] {
        let to_ep = endpoint - ray_origin;
        let t_ray = to_ep.dot(ray_dir).max(0.0);
        let ray_pt = ray_origin + ray_dir * t_ray;
        let d = (endpoint - ray_pt).length();
        if d < best {
            best = d;
        }
    }

    best
}

/// Find the closest orbit ring to a camera ray.
///
/// Samples points along each visible orbit, transforms them into Bevy world
/// coordinates (matching `draw_orbit_paths`), and returns the entity with
/// the smallest ray-to-segment distance if within the click radius.
///
/// The click radius scales with `camera_distance` so orbits remain clickable
/// at any zoom level.
///
/// Returns `(entity, ray_distance)`.
fn find_closest_orbit_to_ray<'a>(
    ray: &Ray3d,
    camera_distance: f32,
    bodies: impl Iterator<
        Item = (
            Entity,
            &'a KeplerOrbit,
            &'a OrbitPath,
            Option<&'a OrbitCenter>,
            Option<&'a LogicalParent>,
            Option<&'a LocalOrbitAmplification>,
            &'a Visibility,
        ),
    >,
    get_parent_coords: &impl Fn(Entity) -> Option<DVec3>,
    origin_offset: DVec3,
) -> Option<(Entity, f32)> {
    let ray_origin = ray.origin;
    let ray_dir = *ray.direction;

    // Scale click radius with camera distance so orbits are always clickable.
    // At 1000 Bevy units distance, radius ~8. At 10000, ~80.
    let click_radius = ORBIT_CLICK_RADIUS_BASE * (camera_distance / 1000.0).max(1.0);

    let mut closest: Option<(Entity, f32)> = None;

    for (entity, orbit, path, orbit_center, logical_parent, amplification, visibility) in bodies {
        if !path.visible {
            continue;
        }
        if *visibility == Visibility::Hidden {
            continue;
        }

        let amp = amplification.map(|a| a.0 as f64).unwrap_or(1.0);
        let parent_offset = orbit_center
            .map(|center| center.0)
            .or_else(|| logical_parent.map(|parent| parent.0))
            .and_then(get_parent_coords)
            .map(|pos| {
                let scaled = (pos - origin_offset) * SCALING_FACTOR;
                Vec3::new(scaled.x as f32, scaled.y as f32, scaled.z as f32)
            })
            .unwrap_or(Vec3::ZERO);

        // Use more samples for eccentric orbits to avoid gaps
        let samples = if orbit.eccentricity > 0.6 {
            (ORBIT_CLICK_SAMPLES as f64 * (1.0 + orbit.eccentricity * 2.0)) as u32
        } else {
            ORBIT_CLICK_SAMPLES
        };
        let orbit_step = std::f64::consts::TAU / samples as f64;

        let mut prev_point: Option<Vec3> = None;
        let mut min_dist = f32::MAX;

        for i in 0..=samples {
            let true_anomaly = i as f64 * orbit_step;
            let pos_au = orbit_position_from_true_anomaly(orbit, true_anomaly);

            let scaled_x = (pos_au.x * SCALING_FACTOR * amp) as f32;
            let scaled_y = (pos_au.y * SCALING_FACTOR * amp) as f32;
            let scaled_z = (pos_au.z * SCALING_FACTOR * amp) as f32;
            let point = Vec3::new(scaled_x, scaled_y, scaled_z) + parent_offset;

            if let Some(prev) = prev_point {
                let dist = ray_to_segment_distance(ray_origin, ray_dir, prev, point);
                if dist < min_dist {
                    min_dist = dist;
                }
            }
            prev_point = Some(point);
        }

        if min_dist < click_radius {
            match closest {
                None => closest = Some((entity, min_dist)),
                Some((_, prev_dist)) if min_dist < prev_dist => {
                    closest = Some((entity, min_dist));
                }
                _ => {}
            }
        }
    }

    closest
}

/// System that handles celestial body selection via mouse clicks.
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
            Option<&LogicalParent>,
            &Visibility,
            Option<&KeplerOrbit>,
            Option<&OrbitPath>,
            Option<&LocalOrbitAmplification>,
            Option<&OrbitCenter>,
            Option<&SpaceCoordinates>,
        ),
        Without<ClickExcluded>,
    >,
    space_coords_query: Query<&SpaceCoordinates>,
    current_system: Res<CurrentStarSystem>,
    mut commands: Commands,
    selected_query: Query<Entity, With<Selected>>,
    mut anchor_query: Query<(&mut CameraAnchor, &mut OrbitCamera), With<GameCamera>>,
    time: Res<Time>,
    mut selection_state: Local<SelectionState>,
    mut ui: BodySelectionUiParams,
) {
    // Disable body selection when a full-screen overlay menu is active
    if ui.active_menu.current.blocks_world_interaction() {
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
    if let Ok(ctx) = ui.egui_contexts.ctx_mut() {
        let hover_pos = ctx.input(|i| i.pointer.hover_pos());
        let over_panel = if let Some(available) = ui.panel_bounds.available_rect {
            hover_pos.is_some_and(|p| !available.contains(p))
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

    for (
        entity,
        transform,
        body,
        system_id,
        _logical_parent,
        visibility,
        _kepler,
        _orbit_path,
        _amp,
        _orbit_center,
        _coords,
    ) in body_query.iter()
    {
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

    // Fallback: if no body mesh was directly clicked, check orbit rings.
    // Bodies always take priority over orbit ring clicks.
    let selected_entity = if let Some((entity, _, _, name)) = closest_body {
        Some((entity, name))
    } else {
        let origin_offset = ui
            .floating_origin
            .as_ref()
            .map(|fo| fo.position)
            .unwrap_or(DVec3::ZERO);
        let orbit_iter = body_query.iter().filter_map(
            |(
                entity,
                _gt,
                _body,
                system_id,
                logical_parent,
                visibility,
                kepler,
                orbit_path,
                amp,
                orbit_center,
                _coords,
            )| {
                let body_system = system_id.map(|s| s.0).unwrap_or(0);
                if body_system != current_system.0 {
                    return None;
                }
                let orbit = kepler?;
                let path = orbit_path?;
                Some((
                    entity,
                    orbit,
                    path,
                    orbit_center,
                    logical_parent,
                    amp,
                    visibility,
                ))
            },
        );
        let get_parent_coords = |parent: Entity| -> Option<DVec3> {
            match body_query.get(parent) {
                Ok(t) => t.10.map(|sc| sc.position),
                // Fallback for orbit-anchor entities that have SpaceCoordinates but no CelestialBody
                Err(_) => space_coords_query.get(parent).ok().map(|sc| sc.position),
            }
        };
        let camera_distance = camera_transform.translation().length();
        find_closest_orbit_to_ray(
            &ray,
            camera_distance,
            orbit_iter,
            &get_parent_coords,
            origin_offset,
        )
        .and_then(|(entity, _dist)| {
            body_query
                .get(entity)
                .ok()
                .map(|(_, _, body, _, _, _, _, _, _, _, _)| (entity, body.name.clone()))
        })
    };

    // Deselect all currently selected bodies if left clicking
    if left_click {
        for entity in selected_query.iter() {
            commands.entity(entity).remove::<Selected>();
        }
    }

    // Select the clicked body if any
    if let Some((entity, name)) = selected_entity {
        if left_click {
            commands.entity(entity).insert(Selected);
            info!("Selected celestial body: {} (entity {:?})", name, entity);

            let current_time = time.elapsed_secs_f64();
            if let Some(last_entity) = selection_state.last_clicked_entity {
                if last_entity == entity && (current_time - selection_state.last_click_time) < 0.5 {
                    info!("Double click on {}, setting anchor and recentering.", name);
                    if let Ok((mut anchor, mut orbit_cam)) = anchor_query.single_mut() {
                        anchor.0 = Some(entity);
                        orbit_cam.pan_offset = Vec3::ZERO;
                    }
                }
            }
            selection_state.last_click_time = current_time;
            selection_state.last_clicked_entity = Some(entity);
        } else if right_click && ui.fleet_ui_state.selected_fleet.is_some() {
            info!("Right clicked celestial body: {} (entity {:?}) with fleet selected, opening transfer planner", name, entity);
            // GRA-388: route the right-click through
            // `apply_body_right_click_target` so the porkchop-grid
            // cache (`porkchop_grid`, `porkchop_built_at_s`,
            // `porkchop_built_for`, `selected_porkchop_cell`,
            // `selected_abs_t_dep_s`, `porkchop_texture`,
            // `cross_system_grid`, …) is dropped atomically with the
            // other per-target state.  The previous hand-rolled
            // field clears only touched the per-target slots and
            // left the grid anchored to the *previous* sim epoch,
            // so re-clicking the same body surfaced a stale grid
            // whose "Now" tick no longer aligned with the current
            // sim time and whose min-cell highlight pointed at a
            // launch in the past (the "weird porkchop" report).
            // The new build path is fired on the next frame by the
            // standard `porkchop_built_for != target_body` check
            // (which now always sees a mismatch after `clear_target`).
            apply_body_right_click_target(&mut ui.fleet_ui_state, entity);
        }
    } else if left_click {
        selection_state.last_clicked_entity = None;
    }
}

/// GRA-388: mutate [`FleetUiState`] for a 3D-scene right-click on a
/// celestial body with a fleet selected.  Free function so the
/// mutation contract is unit-testable without standing up the
/// full click-handling system (camera, mouse input, ray cast, …).
///
/// Behaviour contract:
/// - All per-target state — `target_body` / `target_lagrange` /
///   `target_fleet` / `target_star_system` / `computed_options` /
///   `planned_transfer` / `selected_option` /
///   `selected_gravity_assist` / `porkchop_grid` /
///   `porkchop_built_for` / `porkchop_built_at_s` /
///   `porkchop_last_real_build_s` / `porkchop_grid_pending_rebuild` /
///   `porkchop_build_in_flight` / `porkchop_build_result_rx` /
///   `porkchop_texture` / `porkchop_texture_built_for` /
///   `selected_porkchop_cell` / `selected_abs_t_dep_s` /
///   `selected_abs_tof_s` / `cross_system_grid` /
///   `cross_system_grid_built_for` — is dropped via
///   [`FleetUiState::clear_target`] so re-clicking the *same* body
///   also rebuilds the grid (the stale-grid bug).
/// - `target_body` is then re-set to the clicked entity.
/// - `show_transfer_popup` flips on so the popup renders.
/// - `departure_offset_days = -1.0` is the existing "auto-set to
///   next window" sentinel consumed in
///   `src/ui/transfer_planner.rs` near line 4719.
///
/// Fleet selection (`selected_fleet`, `spawn_location_body`,
/// `editing_fleet_name`, `disband_confirm_fleet`, …) is preserved —
/// the right-click is a destination-pick, not a fleet-pick.
pub fn apply_body_right_click_target(state: &mut FleetUiState, entity: Entity) {
    state.clear_target();
    state.target_body = Some(entity);
    state.show_transfer_popup = true;
    // Signal to auto-set to next window.
    state.departure_offset_days = -1.0;
}

/// System that handles celestial body hover detection via mouse position.
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
            Option<&LogicalParent>,
            &Visibility,
            Option<&KeplerOrbit>,
            Option<&OrbitPath>,
            Option<&LocalOrbitAmplification>,
            Option<&OrbitCenter>,
            Option<&SpaceCoordinates>,
        ),
        Without<ClickExcluded>,
    >,
    space_coords_query: Query<&SpaceCoordinates>,
    current_system: Res<CurrentStarSystem>,
    mut commands: Commands,
    hovered_query: Query<Entity, With<Hovered>>,
    mut egui_contexts: bevy_egui::EguiContexts,
    active_menu: Res<ActiveMenu>,
    panel_bounds: Res<EguiPanelBounds>,
    fleet_ui_state: Res<FleetUiState>,
    floating_origin: Option<Res<FloatingOrigin>>,
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
            hover_pos.is_some_and(|p| !available.contains(p))
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

    for (
        entity,
        transform,
        body,
        system_id,
        _logical_parent,
        visibility,
        _kepler,
        _orbit_path,
        _amp,
        _orbit_center,
        _coords,
    ) in body_query.iter()
    {
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

    // Fallback: if no body mesh is hovered, check orbit rings.
    let new_hover = if let Some((e, _, _)) = closest_body {
        Some(e)
    } else {
        let origin_offset = floating_origin
            .as_ref()
            .map(|fo| fo.position)
            .unwrap_or(DVec3::ZERO);
        let orbit_iter = body_query.iter().filter_map(
            |(
                entity,
                _gt,
                _body,
                system_id,
                logical_parent,
                visibility,
                kepler,
                orbit_path,
                amp,
                orbit_center,
                _coords,
            )| {
                let body_system = system_id.map(|s| s.0).unwrap_or(0);
                if body_system != current_system.0 {
                    return None;
                }
                let orbit = kepler?;
                let path = orbit_path?;
                Some((
                    entity,
                    orbit,
                    path,
                    orbit_center,
                    logical_parent,
                    amp,
                    visibility,
                ))
            },
        );
        let get_parent_coords = |parent: Entity| -> Option<DVec3> {
            match body_query.get(parent) {
                Ok(t) => t.10.map(|sc| sc.position),
                // Fallback for orbit-anchor entities that have SpaceCoordinates but no CelestialBody
                Err(_) => space_coords_query.get(parent).ok().map(|sc| sc.position),
            }
        };
        let camera_distance = camera_transform.translation().length();
        find_closest_orbit_to_ray(
            &ray,
            camera_distance,
            orbit_iter,
            &get_parent_coords,
            origin_offset,
        )
        .map(|(entity, _dist)| entity)
    };
    let hover_is_body = new_hover.is_some();
    // Use crosshair only while the transfer planner popup is open.
    // A selected fleet that is merely being inspected (no active planning) keeps the default cursor.
    let planner_open = fleet_ui_state.show_transfer_popup;
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
            o.cursor_icon = if planner_open {
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
    selected_query: Query<
        (
            Entity,
            &CelestialBody,
            &GlobalTransform,
            Option<&KeplerOrbit>,
        ),
        Added<Selected>,
    >,
    hover_markers: Query<(Entity, &MarkerOwner), With<HoverMarker>>,
    existing_selection_markers: Query<(Entity, &MarkerOwner), With<SelectionMarker>>,
    all_selected: Query<(), With<Selected>>,
    camera_query: Query<&GlobalTransform, With<GameCamera>>,
    orbit_camera_query: Query<&OrbitCamera, With<GameCamera>>,
    time_scale: Res<TimeScale>,
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

    for (entity, body, gtransform, kepler) in selected_query.iter() {
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

        // Skip bracket markers for speed-capped bodies — their orbit path
        // is highlighted instead, avoiding strobing at high time-scales.
        if let Some(orbit) = kepler {
            let effective_speed = orbit.mean_motion.abs() * time_scale.scale as f64;
            if effective_speed > VISUAL_SPEED_BASE {
                continue;
            }
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
    body_query: Query<(&CelestialBody, Option<&Hovered>, &GlobalTransform), Without<Selected>>,
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

        // If still hovered and no longer selected, add a hover marker
        // (rings are handled separately).
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
        (
            Entity,
            &CelestialBody,
            &GlobalTransform,
            Option<&KeplerOrbit>,
        ),
        (Added<Hovered>, Without<Selected>),
    >,
    camera_query: Query<&GlobalTransform, With<GameCamera>>,
    orbit_camera_query: Query<&OrbitCamera, With<GameCamera>>,
    time_scale: Res<TimeScale>,
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

    for (entity, body, gtransform, kepler) in hovered_query.iter() {
        // Rings are highlighted via material emissive, not a 3D marker.
        if body.body_type == BodyType::Ring {
            continue;
        }

        // Skip bracket markers for speed-capped bodies — orbit highlighting
        // provides feedback instead, avoiding strobing at high time-scales.
        if let Some(orbit) = kepler {
            let effective_speed = orbit.mean_motion.abs() * time_scale.scale as f64;
            if effective_speed > VISUAL_SPEED_BASE {
                continue;
            }
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

/// Re-spawns bracket markers for selected/hovered bodies when the time scale
/// drops back below the suppression threshold.
///
/// Markers are despawned in `scale_markers_with_zoom` when the effective orbital
/// speed exceeds `VISUAL_SPEED_BASE`.  Once the user slows time back down the
/// `Added<Selected>` / `Added<Hovered>` filters in the spawn systems no longer
/// fire (the component was never removed), so this system fills the gap.
pub fn restore_suppressed_markers(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    time_scale: Res<TimeScale>,
    selected_query: Query<
        (
            Entity,
            &CelestialBody,
            &GlobalTransform,
            Option<&KeplerOrbit>,
        ),
        With<Selected>,
    >,
    hovered_query: Query<
        (
            Entity,
            &CelestialBody,
            &GlobalTransform,
            Option<&KeplerOrbit>,
        ),
        (With<Hovered>, Without<Selected>),
    >,
    selection_markers: Query<&MarkerOwner, With<SelectionMarker>>,
    hover_markers: Query<&MarkerOwner, With<HoverMarker>>,
    camera_query: Query<&GlobalTransform, With<GameCamera>>,
    orbit_camera_query: Query<&OrbitCamera, With<GameCamera>>,
) {
    // Only run when TimeScale actually changed, to avoid per-frame cost.
    if !time_scale.is_changed() {
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

    let scale = time_scale.scale as f64;

    // Collect owners that already have markers to avoid duplicates.
    let has_selection_marker: std::collections::HashSet<Entity> =
        selection_markers.iter().map(|o| o.0).collect();
    let has_hover_marker: std::collections::HashSet<Entity> =
        hover_markers.iter().map(|o| o.0).collect();

    for (entity, body, gtransform, kepler) in selected_query.iter() {
        if body.body_type == BodyType::Ring {
            continue;
        }
        // Only act when the body is now below the threshold and has no marker.
        let speed_capped = kepler
            .map(|o| o.mean_motion.abs() * scale > VISUAL_SPEED_BASE)
            .unwrap_or(false);
        if speed_capped || has_selection_marker.contains(&entity) {
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

    for (entity, body, gtransform, kepler) in hovered_query.iter() {
        if body.body_type == BodyType::Ring {
            continue;
        }
        let speed_capped = kepler
            .map(|o| o.mean_motion.abs() * scale > VISUAL_SPEED_BASE)
            .unwrap_or(false);
        if speed_capped || has_hover_marker.contains(&entity) {
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
    owner_query: Query<(&Transform, Option<&KeplerOrbit>), Without<MarkerOwner>>,
    time_scale: Res<TimeScale>,
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
        if let Ok((owner_transform, kepler)) = owner_query.get(owner.0) {
            // Despawn markers for speed-capped bodies — orbit highlighting
            // provides visual feedback instead, avoiding strobing.
            if let Some(orbit) = kepler {
                let effective_speed = orbit.mean_motion.abs() * time_scale.scale as f64;
                if effective_speed > VISUAL_SPEED_BASE {
                    commands.entity(entity).despawn();
                    continue;
                }
            }

            // 1. Match owner position (read from Transform, set in same Update schedule)
            let owner_position = owner_transform.translation;
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
