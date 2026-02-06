use bevy::prelude::*;
use bevy::math::DVec3;
use bevy::window::PrimaryWindow;

use super::components::{
    HoverMarker, Hovered, KeplerOrbit, MarkerDot, MarkerOwner, OrbitPath, Selected,
    SelectionMarker, SpaceCoordinates,
};
use crate::plugins::solar_system::{CelestialBody, Moon, Planet, Star, RADIUS_SCALE};
use crate::plugins::camera::{CameraAnchor, GameCamera, OrbitCamera};

/// Scaling factor for converting astronomical units to Bevy rendering units
/// 1 AU = 1500.0 Bevy units ensures separation between planets and moons
pub const SCALING_FACTOR: f64 = 1500.0;

/// Distance threshold for showing moon orbits (in Bevy units)
/// Moon orbits only visible when camera is closer than this distance
const MOON_ORBIT_VISIBILITY_DISTANCE: f32 = 500.0;

/// Click radius for body selection (in Bevy units)
/// Bodies within this distance from the ray are considered clickable
const SELECTION_CLICK_RADIUS: f32 = 5.0;

/// Padding for the hover ring around celestial bodies (in Bevy units)
const HOVER_RING_PADDING: f32 = 8.0;  // Creates visible gap between marker and body

/// Maximum iterations for Kepler solver
const MAX_KEPLER_ITERATIONS: u32 = 50;

/// Convergence tolerance for Kepler solver
const KEPLER_TOLERANCE: f64 = 1e-10;

/// Minimum translation change threshold (in Bevy units squared)
/// Transform updates skip when squared distance change is below this threshold
/// Prevents unnecessary updates when changes are below f32 precision
/// Linear distance threshold: sqrt(1e-6) ≈ 0.001 Bevy units
/// Note: This threshold is safe even for slow-moving bodies because:
/// - At 1000x time acceleration, even distant asteroids move > 0.001 units/frame
/// - At normal speed, bodies with imperceptible motion don't need visual updates
/// - Orbital calculations still run at full precision (f64) regardless of this threshold
const MIN_TRANSLATION_CHANGE_THRESHOLD: f32 = 1e-6;

/// LOD (Level of Detail) reference distance for orbit trails (in Bevy units)
/// Orbits at this distance get intermediate detail scaling
const LOD_REFERENCE_DISTANCE: f32 = 3000.0;

/// LOD minimum distance for orbit trails (in Bevy units)
/// Orbits closer than this always get maximum detail
const LOD_MIN_DISTANCE: f32 = 300.0;

/// LOD minimum scaling factor for distant orbits
/// Even the most distant orbits get at least 25% of segments
const LOD_MIN_FACTOR: f32 = 0.25;

/// Minimum segment count for orbit trails
/// Even with maximum LOD reduction, orbits have at least this many segments
/// Note: For a 128-segment orbit with LOD_MIN_FACTOR (0.25), this ensures
/// we never go below 8 segments even if the calculation would produce fewer
/// Reduced from 16 to 8 for better performance at high time scales
const MIN_ORBIT_SEGMENTS: u32 = 8;

/// Solves Kepler's equation: M = E - e*sin(E) for eccentric anomaly E
/// Uses Newton-Raphson iteration for high accuracy
///
/// # Arguments
/// * `mean_anomaly` - Mean anomaly M in radians
/// * `eccentricity` - Orbital eccentricity e (0 <= e < 1 for elliptical orbits)
///
/// # Returns
/// Eccentric anomaly E in radians
pub fn solve_kepler(mean_anomaly: f64, eccentricity: f64) -> f64 {
    // For circular orbits, mean anomaly equals eccentric anomaly
    if eccentricity < 1e-10 {
        return mean_anomaly;
    }

    // Initial guess: mean anomaly is a good starting point
    let mut eccentric_anomaly = mean_anomaly;

    // Newton-Raphson iteration
    for _ in 0..MAX_KEPLER_ITERATIONS {
        // f(E) = E - e*sin(E) - M
        let f = eccentric_anomaly - eccentricity * eccentric_anomaly.sin() - mean_anomaly;
        
        // f'(E) = 1 - e*cos(E)
        let f_prime = 1.0 - eccentricity * eccentric_anomaly.cos();
        
        // Newton-Raphson step: E_new = E_old - f(E)/f'(E)
        let delta = f / f_prime;
        eccentric_anomaly -= delta;

        // Check for convergence
        if delta.abs() < KEPLER_TOLERANCE {
            break;
        }
    }

    eccentric_anomaly
}

/// Calculate the 3D orbital position from a mean anomaly.
///
/// # Arguments
/// * `orbit` - Keplerian orbital elements
/// * `mean_anomaly` - Mean anomaly in radians
///
/// # Returns
/// Position in AU in the orbit's reference frame
pub fn orbit_position_from_mean_anomaly(orbit: &KeplerOrbit, mean_anomaly: f64) -> DVec3 {
    // Solve Kepler's equation for eccentric anomaly
    let eccentric_anomaly = solve_kepler(mean_anomaly, orbit.eccentricity);

    // Convert to true anomaly
    let true_anomaly = eccentric_to_true_anomaly(eccentric_anomaly, orbit.eccentricity);

    // Calculate orbital radius
    let radius = orbital_radius(orbit.semi_major_axis, orbit.eccentricity, true_anomaly);

    // Position in the orbital plane
    let x_orbital = radius * true_anomaly.cos();
    let y_orbital = radius * true_anomaly.sin();

    // Apply argument of periapsis rotation
    let cos_w = orbit.argument_of_periapsis.cos();
    let sin_w = orbit.argument_of_periapsis.sin();
    let x_perifocal = x_orbital * cos_w - y_orbital * sin_w;
    let y_perifocal = x_orbital * sin_w + y_orbital * cos_w;

    // Apply inclination and longitude of ascending node rotations
    let cos_i = orbit.inclination.cos();
    let sin_i = orbit.inclination.sin();
    let cos_omega = orbit.longitude_ascending_node.cos();
    let sin_omega = orbit.longitude_ascending_node.sin();

    let x = x_perifocal * cos_omega - y_perifocal * cos_i * sin_omega;
    let y = x_perifocal * sin_omega + y_perifocal * cos_i * cos_omega;
    let z = y_perifocal * sin_i;

    DVec3::new(x, y, z)
}

/// Calculate true anomaly from eccentric anomaly
/// Uses the relationship: tan(ν/2) = sqrt((1+e)/(1-e)) * tan(E/2)
///
/// # Arguments
/// * `eccentric_anomaly` - Eccentric anomaly E in radians
/// * `eccentricity` - Orbital eccentricity e
///
/// # Returns
/// True anomaly ν in radians
fn eccentric_to_true_anomaly(eccentric_anomaly: f64, eccentricity: f64) -> f64 {
    // For circular orbits
    if eccentricity < 1e-10 {
        return eccentric_anomaly;
    }

    // Calculate true anomaly using the formula
    let sqrt_term = ((1.0 + eccentricity) / (1.0 - eccentricity)).sqrt();
    2.0 * (sqrt_term * (eccentric_anomaly / 2.0).tan()).atan()
}

/// Calculate the orbital radius at a given true anomaly
///
/// # Arguments
/// * `semi_major_axis` - Semi-major axis a in AU
/// * `eccentricity` - Orbital eccentricity e
/// * `true_anomaly` - True anomaly ν in radians
///
/// # Returns
/// Orbital radius r in AU
fn orbital_radius(semi_major_axis: f64, eccentricity: f64, true_anomaly: f64) -> f64 {
    // r = a(1 - e²) / (1 + e*cos(ν))
    let numerator = semi_major_axis * (1.0 - eccentricity * eccentricity);
    let denominator = 1.0 + eccentricity * true_anomaly.cos();
    numerator / denominator
}

/// System that propagates all orbits based on Keplerian mechanics
/// Updates SpaceCoordinates based on KeplerOrbit elements and elapsed time
/// Uses virtual time to allow time scaling via UI controls
/// Implements adaptive update frequency for performance at high time scales
pub fn propagate_orbits(
    time: Res<Time<Virtual>>,
    time_scale: Res<crate::ui::TimeScale>,
    mut query: Query<(&KeplerOrbit, &mut SpaceCoordinates)>,
    mut frame_counter: Local<u32>,
) {
    // Adaptive update frequency based on time scale
    // At high time scales, we don't need every-frame updates
    let update_interval = if time_scale.scale < 1000.0 {
        1 // Update every frame for smooth animation at low speeds
    } else if time_scale.scale < 100000.0 {
        3 // Update every 3 frames for medium-high speeds
    } else if time_scale.scale < 1000000.0 {
        5 // Update every 5 frames for high speeds
    } else {
        10 // Update every 10 frames for extreme speeds
    };
    
    *frame_counter += 1;
    if *frame_counter % update_interval != 0 {
        return; // Skip this frame
    }
    
    // Get elapsed time in seconds since game start
    let elapsed_time = time.elapsed_seconds_f64();

    for (orbit, mut coords) in query.iter_mut() {
        // Calculate current mean anomaly: M = M₀ + n*t
        let mean_anomaly = orbit.mean_anomaly_epoch + orbit.mean_motion * elapsed_time;

        // Update space coordinates (in AU)
        coords.position = orbit_position_from_mean_anomaly(orbit, mean_anomaly);
    }
}

/// System that converts high-precision SpaceCoordinates to rendering Transform
/// Implements "floating origin" technique by scaling down coordinates and converting to f32
/// Uses change detection to only update when coordinates have changed
pub fn update_render_transform(
    mut query: Query<(&SpaceCoordinates, &mut Transform), Changed<SpaceCoordinates>>,
) {
    for (coords, mut transform) in query.iter_mut() {
        // Convert from AU to Bevy units using scaling factor
        let scaled_position = coords.position * SCALING_FACTOR;

        // Convert from f64 to f32 for rendering
        // This is safe because we've scaled coordinates to reasonable rendering range
        let new_translation = Vec3::new(
            scaled_position.x as f32,
            scaled_position.y as f32,
            scaled_position.z as f32,
        );
        
        // Only update if the translation has actually changed
        // This prevents unnecessary transform updates when position changes are below f32 precision
        if (new_translation - transform.translation).length_squared() > MIN_TRANSLATION_CHANGE_THRESHOLD {
            transform.translation = new_translation;
        }
    }
}

/// System that draws orbit paths as fading trails (Terra Invicta style).
/// The trail is brightest at the body's current position and fades out
/// behind it, creating a comet-tail effect along the orbit.
/// Uses LOD (Level of Detail) to reduce segment count for distant/less important orbits.
/// Implements adaptive update frequency for performance at high time scales.
pub fn draw_orbit_paths(
    mut gizmos: Gizmos,
    time: Res<Time<Virtual>>,
    time_scale: Res<crate::ui::TimeScale>,
    camera_query: Query<&Transform, With<GameCamera>>,
    query: Query<(&KeplerOrbit, &OrbitPath, Option<&Parent>, Option<&Selected>)>,
    parent_query: Query<&GlobalTransform>,
    mut frame_counter: Local<u32>,
) {
    // Adaptive rendering frequency based on time scale
    // At very high time scales, skip some frames to improve performance
    let render_interval = if time_scale.scale < 100000.0 {
        1 // Render every frame for smooth trails at normal/medium speeds
    } else if time_scale.scale < 1000000.0 {
        2 // Render every 2 frames at high speeds
    } else {
        3 // Render every 3 frames at extreme speeds
    };
    
    *frame_counter += 1;
    if *frame_counter % render_interval != 0 {
        return; // Skip this frame
    }
    
    let elapsed_time = time.elapsed_seconds_f64();
    
    // Get camera position for LOD calculations
    let camera_pos = camera_query
        .get_single()
        .map(|t| t.translation)
        .unwrap_or(Vec3::ZERO);

    for (orbit, path, parent, selected) in query.iter() {
        if !path.visible {
            continue;
        }

        let parent_offset = parent
            .and_then(|parent| parent_query.get(parent.get()).ok())
            .map(|transform| transform.translation())
            .unwrap_or(Vec3::ZERO);

        // Calculate orbit center for LOD
        let orbit_center = parent_offset;
        let distance_to_camera = (orbit_center - camera_pos).length();
        
        // LOD: Reduce segment count for distant orbits
        // Selected orbits always get full detail
        let segments = if selected.is_some() {
            path.segments // Full detail for selected
        } else {
            // Scale segments based on distance using smooth interpolation
            // This provides gradual detail reduction rather than sharp transitions
            let lod_factor = if distance_to_camera < LOD_MIN_DISTANCE {
                1.0 // Full detail when close
            } else if distance_to_camera > LOD_REFERENCE_DISTANCE {
                LOD_MIN_FACTOR // Minimum detail when far
            } else {
                // Smooth interpolation between min and reference distance
                // Lerp from 1.0 (at LOD_MIN_DISTANCE) to LOD_MIN_FACTOR (at LOD_REFERENCE_DISTANCE)
                let distance_range = LOD_REFERENCE_DISTANCE - LOD_MIN_DISTANCE;
                debug_assert!(distance_range > 0.0, "LOD_REFERENCE_DISTANCE must be > LOD_MIN_DISTANCE");
                let t = (distance_to_camera - LOD_MIN_DISTANCE) / distance_range;
                1.0 - t * (1.0 - LOD_MIN_FACTOR)
            };
            ((path.segments as f32 * lod_factor) as u32).max(MIN_ORBIT_SEGMENTS)
        };

        // Current mean anomaly of the body (where it actually is now)
        let current_mean_anomaly = (orbit.mean_anomaly_epoch + orbit.mean_motion * elapsed_time)
            .rem_euclid(std::f64::consts::TAU);

        let mean_anomaly_step = std::f64::consts::TAU / (segments as f64);

        // Extract base color channels from path color
        let base = path.color.to_srgba();

        // Trail covers the full orbit but fades from current position backwards.
        // Segment 0 is the body's current position (brightest).
        // Segment N is the point just before the body (dimmest / invisible).
        let mut prev_point: Option<Vec3> = None;

        for i in 0..=segments {
            // Walk backwards from the current position
            let mean_anomaly = current_mean_anomaly - (i as f64) * mean_anomaly_step;
            let position_au = orbit_position_from_mean_anomaly(orbit, mean_anomaly);

            let scaled_x = (position_au.x * SCALING_FACTOR) as f32;
            let scaled_y = (position_au.y * SCALING_FACTOR) as f32;
            let scaled_z = (position_au.z * SCALING_FACTOR) as f32;

            let point = Vec3::new(scaled_x, scaled_y, scaled_z) + parent_offset;

            if let Some(prev) = prev_point {
                // t goes from 0.0 (at the body) to 1.0 (full orbit behind)
                let t = i as f32 / segments as f32;

                // Fade curve: bright near the body, fading to near-zero
                // Use a smooth power curve for a natural look
                let alpha = base.alpha * (1.0 - t).powf(1.8);

                // Glow boost near the head of the trail
                let glow = if t < 0.08 { 1.3 } else { 1.0 };

                if alpha > 0.01 {
                    let segment_color = Color::srgba(
                        (base.red * glow).min(1.0),
                        (base.green * glow).min(1.0),
                        (base.blue * glow).min(1.0),
                        alpha,
                    );
                    gizmos.line(prev, point, segment_color);
                }
            }

            prev_point = Some(point);
        }
    }
}

/// System that controls orbit visibility based on body type, selection, and camera distance
pub fn update_orbit_visibility(
    camera_query: Query<&Transform, With<GameCamera>>,
    mut orbit_query: Query<(
        &mut OrbitPath,
        Option<&Selected>,
        Option<&Planet>,
        Option<&Moon>,
    )>,
) {
    // Get camera position
    let Ok(camera_transform) = camera_query.get_single() else {
        return;
    };

    // Calculate distance from camera to origin (solar system center)
    let camera_distance = camera_transform.translation.length();

    for (mut orbit_path, selected, planet, moon) in orbit_query.iter_mut() {
        if selected.is_some() {
            // Selected bodies always show their orbit
            orbit_path.visible = true;
        } else if planet.is_some() {
            // Planets always show their orbit
            orbit_path.visible = true;
        } else if moon.is_some() {
            // Show moon orbits only when zoomed in close enough
            orbit_path.visible = camera_distance < MOON_ORBIT_VISIBILITY_DISTANCE;
        } else {
            // Asteroids, Comets, DwarfPlanets are hidden by default
            orbit_path.visible = false;
        }
    }
}

#[derive(Default)]
pub struct SelectionState {
    pub last_click_time: f64,
    pub last_clicked_entity: Option<Entity>,
}

/// System that handles celestial body selection via mouse clicks
#[allow(clippy::too_many_arguments)]
pub fn handle_body_selection(
    mouse_button: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    body_query: Query<(Entity, &GlobalTransform, &CelestialBody)>,
    mut commands: Commands,
    selected_query: Query<Entity, With<Selected>>,
    mut anchor_query: Query<&mut CameraAnchor, With<GameCamera>>,
    time: Res<Time>,
    mut selection_state: Local<SelectionState>,
    mut egui_contexts: bevy_egui::EguiContexts,
) {
    // Only process on mouse click
    if !mouse_button.just_pressed(MouseButton::Left) {
        return;
    }

    // Don't process if egui is using the mouse (e.g., clicking on UI)
    if egui_contexts.ctx_mut().wants_pointer_input() {
        return;
    }

    let Ok(window) = windows.get_single() else {
        return;
    };

    let Ok((camera, camera_transform)) = camera_query.get_single() else {
        return;
    };

    // Get cursor position
    let Some(cursor_position) = window.cursor_position() else {
        return;
    };

    // Convert screen position to ray
    let Some(ray) = camera.viewport_to_world(camera_transform, cursor_position) else {
        return;
    };

    // Find the closest body to the ray
    // Stores: (Entity, distance from camera, body name)
    let mut closest_body: Option<(Entity, f32, String)> = None;
    
    for (entity, transform, body) in body_query.iter() {
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
        // This allows clicking on the visible surface of large bodies, and provides
        // a generous margin for small bodies
        let selection_radius = body.visual_radius + SELECTION_CLICK_RADIUS;

        if distance < selection_radius {
            match closest_body {
                None => closest_body = Some((entity, projection, body.name.clone())),
                Some((_, prev_dist, _)) if projection < prev_dist => {
                    closest_body = Some((entity, projection, body.name.clone()));
                }
                _ => {}
            }
        }
    }

    // Deselect all currently selected bodies
    for entity in selected_query.iter() {
        commands.entity(entity).remove::<Selected>();
    }

    // Select the clicked body if any
    if let Some((entity, _, name)) = closest_body {
        commands.entity(entity).insert(Selected);
        info!("Selected celestial body: {} (entity {:?})", name, entity);
        
        let current_time = time.elapsed_seconds_f64();
        if let Some(last_entity) = selection_state.last_clicked_entity {
             if last_entity == entity && (current_time - selection_state.last_click_time) < 0.5 {
                 info!("Double click on {}, setting anchor.", name);
                 if let Ok(mut anchor) = anchor_query.get_single_mut() {
                     anchor.0 = Some(entity);
                 }
             }
        }
        selection_state.last_click_time = current_time;
        selection_state.last_clicked_entity = Some(entity);
    } else {
        selection_state.last_clicked_entity = None;
    }
}

/// System that handles celestial body hover detection via mouse position
pub fn handle_body_hover(
    windows: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    body_query: Query<(Entity, &GlobalTransform, &CelestialBody)>,
    mut commands: Commands,
    hovered_query: Query<Entity, With<Hovered>>,
) {
    let Ok(window) = windows.get_single() else {
        return;
    };

    let Ok((camera, camera_transform)) = camera_query.get_single() else {
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
    let Some(ray) = camera.viewport_to_world(camera_transform, cursor_position) else {
        return;
    };

    // Find the closest body to the ray
    let mut closest_body: Option<(Entity, f32)> = None;
    
    for (entity, transform, body) in body_query.iter() {
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
                None => closest_body = Some((entity, projection)),
                Some((_, prev_dist)) if projection < prev_dist => {
                    closest_body = Some((entity, projection));
                }
                _ => {}
            }
        }
    }

    // Clear all hovers first
    for entity in hovered_query.iter() {
        commands.entity(entity).remove::<Hovered>();
    }

    // Set the hovered body if any
    if let Some((entity, _)) = closest_body {
        commands.entity(entity).insert(Hovered);
    }
}

/// System that spawns glossy selection markers for newly selected bodies.
pub fn spawn_selection_markers(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    selected_query: Query<(Entity, &CelestialBody), Added<Selected>>,
    hover_markers: Query<(Entity, &MarkerOwner), With<HoverMarker>>,
) {
    for (entity, body) in selected_query.iter() {
        // Remove hover marker if it exists
        for (marker_entity, owner) in hover_markers.iter() {
            if owner.0 == entity {
                commands.entity(marker_entity).despawn_recursive();
            }
        }

        let marker_radius = body.visual_radius + HOVER_RING_PADDING;
        spawn_marker(
            &mut commands,
            &mut meshes,
            &mut materials,
            entity,
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
    body_query: Query<(&CelestialBody, Option<&Hovered>)>,
) {
    for entity in removed_selected.read() {
        for (marker_entity, owner) in marker_query.iter() {
            if owner.0 == entity {
                commands.entity(marker_entity).despawn_recursive();
            }
        }

        // If still hovered, add a hover marker
        if let Ok((body, Some(_))) = body_query.get(entity) {
            let marker_radius = body.visual_radius + HOVER_RING_PADDING;
            spawn_marker(
                &mut commands,
                &mut meshes,
                &mut materials,
                entity,
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
    hovered_query: Query<(Entity, &CelestialBody), (Added<Hovered>, Without<Selected>)>,
) {
    for (entity, body) in hovered_query.iter() {
        let marker_radius = body.visual_radius + HOVER_RING_PADDING;
        spawn_marker(
            &mut commands,
            &mut meshes,
            &mut materials,
            entity,
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
) {
    for entity in removed_hovered.read() {
        for (marker_entity, owner) in marker_query.iter() {
            if owner.0 == entity {
                commands.entity(marker_entity).despawn_recursive();
            }
        }
    }
}

/// System that animates marker dots around selection/hover rings.
pub fn animate_marker_dots(time: Res<Time>, mut query: Query<(&mut Transform, &mut MarkerDot)>) {
    for (mut transform, mut dot) in query.iter_mut() {
        dot.angle = (dot.angle + dot.angular_speed * time.delta_seconds())
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
    radius: f32,
    is_selected: bool,
) {
    let ring_color = if is_selected {
        Color::srgb(0.45, 0.85, 1.0)
    } else {
        Color::srgb(0.35, 0.7, 0.9)
    };

    let emissive = if is_selected { 0.25 } else { 0.12 };
    let ring_material = materials.add(StandardMaterial {
        base_color: ring_color,
        emissive: LinearRgba::from(ring_color) * emissive,
        metallic: 0.6,
        perceptual_roughness: 0.15,
        reflectance: 0.8,
        ..default()
    });

    let ring_mesh = meshes.add(Torus {
        minor_radius: 0.6,
        major_radius: radius,
        ..default()
    });

    let marker_entity = commands
        .spawn((
            PbrBundle {
                mesh: ring_mesh,
                material: ring_material,
                transform: Transform::default(),
                ..default()
            },
            MarkerOwner(owner),
        ))
        .id();

    if is_selected {
        commands.entity(marker_entity).insert(SelectionMarker);
    } else {
        commands.entity(marker_entity).insert(HoverMarker);
    }

    commands.entity(marker_entity).set_parent(owner);

    let dot_color = if is_selected {
        Color::srgb(0.9, 0.95, 1.0)
    } else {
        Color::srgb(0.7, 0.85, 1.0)
    };

    // Create glowing transparent material for the marker dot
    let dot_material = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 1.0, 1.0, 0.6),
        emissive: LinearRgba::from(dot_color) * 3.0,  // Strong glow
        alpha_mode: AlphaMode::Blend,
        metallic: 0.0,
        perceptual_roughness: 0.1,
        unlit: true,  // Pure glow effect
        ..default()
    });

    let dot_mesh = meshes.add(Sphere::new(1.2));

    commands.entity(marker_entity).with_children(|parent| {
        parent.spawn((
            PbrBundle {
                mesh: dot_mesh,
                material: dot_material,
                transform: Transform::from_translation(Vec3::new(radius, 0.0, 0.0)),
                ..default()
            },
            MarkerDot {
                angle: 0.0,
                angular_speed: if is_selected { 0.3 } else { 0.2 },
                radius,
            },
        ));
    });
}

/// System that automatically zooms camera when anchoring to a body
pub fn zoom_camera_to_anchored_body(
    body_query: Query<(&CelestialBody, Option<&Star>)>,
    mut camera_query: Query<(&mut OrbitCamera, &CameraAnchor), (With<GameCamera>, Changed<CameraAnchor>)>,
) {
    // Only trigger when camera anchor changes
    let Ok((mut orbit_camera, anchor)) = camera_query.get_single_mut() else {
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
                // For other bodies, make them fill about 10% of the screen
                // Assuming a 60° FOV, we need distance = radius * 10
                let visual_radius = body.radius * RADIUS_SCALE;
                let target_distance = visual_radius * 20.0; // Fill ~10% of screen
                target_distance.clamp(50.0, 10000.0) // Clamp to reasonable range
            };
            
            orbit_camera.radius = zoom_distance;
        }
    }
}

/// System that scales selection and hover markers based on camera zoom distance.
///
/// This ensures markers remain a consistent visual size regardless of how far
/// the camera is from the target body. Markers scale linearly with camera distance,
/// with a reference distance of 200 Bevy units where scale is 1.0.
pub fn scale_markers_with_zoom(
    camera_query: Query<&OrbitCamera, With<GameCamera>>,
    mut marker_query: Query<
        &mut Transform,
        Or<(With<SelectionMarker>, With<HoverMarker>)>,
    >,
) {
    let Ok(orbit_camera) = camera_query.get_single() else {
        return;
    };

    // Reference distance where markers appear at their base size
    let reference_distance = 1000.0_f32;
    // Scale factor: markers grow with camera distance when zoomed out
    // Never shrink below 1.0 to prevent rings from going inside the body
    let zoom_scale = (orbit_camera.radius / reference_distance).clamp(1.0, 3.0);

    for mut transform in marker_query.iter_mut() {
        transform.scale = Vec3::splat(zoom_scale);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_solve_kepler_circular_orbit() {
        // For circular orbit (e=0), eccentric anomaly should equal mean anomaly
        let mean_anomaly = std::f64::consts::PI / 4.0; // 45 degrees
        let eccentricity = 0.0;
        let result = solve_kepler(mean_anomaly, eccentricity);
        assert!((result - mean_anomaly).abs() < 1e-10);
    }

    #[test]
    fn test_solve_kepler_eccentric_orbit() {
        // Test with Earth's eccentricity (e ≈ 0.0167)
        let mean_anomaly = std::f64::consts::PI / 2.0; // 90 degrees
        let eccentricity = 0.0167;
        let eccentric_anomaly = solve_kepler(mean_anomaly, eccentricity);
        
        // Verify Kepler's equation: M = E - e*sin(E)
        let calculated_mean = eccentric_anomaly - eccentricity * eccentric_anomaly.sin();
        assert!((calculated_mean - mean_anomaly).abs() < KEPLER_TOLERANCE);
    }

    #[test]
    fn test_solve_kepler_high_eccentricity() {
        // Test with higher eccentricity (e = 0.8)
        let mean_anomaly = std::f64::consts::PI;
        let eccentricity = 0.8;
        let eccentric_anomaly = solve_kepler(mean_anomaly, eccentricity);
        
        // Verify Kepler's equation
        let calculated_mean = eccentric_anomaly - eccentricity * eccentric_anomaly.sin();
        assert!((calculated_mean - mean_anomaly).abs() < KEPLER_TOLERANCE);
    }

    #[test]
    fn test_eccentric_to_true_anomaly_circular() {
        // For circular orbit, true anomaly should equal eccentric anomaly
        let eccentric_anomaly = std::f64::consts::PI / 3.0;
        let eccentricity = 0.0;
        let true_anomaly = eccentric_to_true_anomaly(eccentric_anomaly, eccentricity);
        assert!((true_anomaly - eccentric_anomaly).abs() < 1e-10);
    }

    #[test]
    fn test_orbital_radius_circular() {
        // For circular orbit at any true anomaly, radius should equal semi-major axis
        let semi_major_axis = 1.0;
        let eccentricity = 0.0;
        let true_anomaly = std::f64::consts::PI / 4.0;
        let radius = orbital_radius(semi_major_axis, eccentricity, true_anomaly);
        assert!((radius - semi_major_axis).abs() < 1e-10);
    }

    #[test]
    fn test_orbital_radius_periapsis_apoapsis() {
        // Test periapsis and apoapsis distances
        let semi_major_axis = 1.0;
        let eccentricity = 0.5;
        
        // At periapsis (true anomaly = 0), r = a(1-e)
        let periapsis_distance = orbital_radius(semi_major_axis, eccentricity, 0.0);
        let expected_periapsis = semi_major_axis * (1.0 - eccentricity);
        assert!((periapsis_distance - expected_periapsis).abs() < 1e-10);
        
        // At apoapsis (true anomaly = π), r = a(1+e)
        let apoapsis_distance = orbital_radius(semi_major_axis, eccentricity, std::f64::consts::PI);
        let expected_apoapsis = semi_major_axis * (1.0 + eccentricity);
        assert!((apoapsis_distance - expected_apoapsis).abs() < 1e-10);
    }

    #[test]
    fn test_propagate_orbits_system() {
        // Create a test app
        let mut app = App::new();
        app.init_resource::<Time<Virtual>>();
        app.add_systems(Update, propagate_orbits);

        // Spawn an entity with circular orbit
        let orbit = KeplerOrbit::circular(1.0, std::f64::consts::TAU); // 1 AU, 1 radian/second
        let coords = SpaceCoordinates::default();
        app.world_mut().spawn((orbit, coords));

        // Run one update
        app.update();

        // Verify the entity was processed (coordinates should be updated)
        let mut query = app.world_mut().query::<&SpaceCoordinates>();
        let coords = query.iter(app.world()).next().unwrap();
        // For a circular orbit starting at mean anomaly 0, should be at (a, 0, 0)
        assert!(coords.position.x > 0.0);
    }

    #[test]
    fn test_update_render_transform_scaling() {
        // Test that the transform system correctly scales coordinates
        let mut app = App::new();
        app.add_systems(Update, update_render_transform);

        // Spawn entity with known space coordinates
        let coords = SpaceCoordinates::new(DVec3::new(1.0, 2.0, 3.0)); // In AU
        let transform = Transform::default();
        app.world_mut().spawn((coords, transform));

        // Run one update
        app.update();

        // Verify transform was updated with scaled values
        let mut query = app.world_mut().query::<&Transform>();
        let transform = query.iter(app.world()).next().unwrap();
        
        // Should be scaled by SCALING_FACTOR
        let expected = Vec3::new(
            (1.0 * SCALING_FACTOR) as f32,
            (2.0 * SCALING_FACTOR) as f32,
            (3.0 * SCALING_FACTOR) as f32,
        );
        assert!((transform.translation - expected).length() < 1e-5);
    }
}
