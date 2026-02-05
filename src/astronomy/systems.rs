use bevy::prelude::*;
use bevy::math::DVec3;
use bevy::window::PrimaryWindow;

use super::components::{KeplerOrbit, OrbitPath, Selected, Hovered, SpaceCoordinates};
use crate::plugins::solar_system::{CelestialBody, Moon, Star};
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

/// Maximum iterations for Kepler solver
const MAX_KEPLER_ITERATIONS: u32 = 50;

/// Convergence tolerance for Kepler solver
const KEPLER_TOLERANCE: f64 = 1e-10;

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
pub fn propagate_orbits(
    time: Res<Time<Virtual>>,
    mut query: Query<(&KeplerOrbit, &mut SpaceCoordinates)>,
) {
    // Get elapsed time in seconds since game start
    let elapsed_time = time.elapsed_seconds_f64();

    for (orbit, mut coords) in query.iter_mut() {
        // Calculate current mean anomaly: M = M₀ + n*t
        let mean_anomaly = orbit.mean_anomaly_epoch + orbit.mean_motion * elapsed_time;

        // Solve Kepler's equation for eccentric anomaly
        let eccentric_anomaly = solve_kepler(mean_anomaly, orbit.eccentricity);

        // Convert to true anomaly
        let true_anomaly = eccentric_to_true_anomaly(eccentric_anomaly, orbit.eccentricity);

        // Calculate orbital radius
        let radius = orbital_radius(orbit.semi_major_axis, orbit.eccentricity, true_anomaly);

        // Calculate position in the orbital plane
        // In orbital frame: x-axis points to periapsis, z-axis is orbit normal
        let x_orbital = radius * true_anomaly.cos();
        let y_orbital = radius * true_anomaly.sin();

        // Apply argument of periapsis rotation (rotation in orbital plane)
        let cos_w = orbit.argument_of_periapsis.cos();
        let sin_w = orbit.argument_of_periapsis.sin();
        let x_perifocal = x_orbital * cos_w - y_orbital * sin_w;
        let y_perifocal = x_orbital * sin_w + y_orbital * cos_w;

        // Apply inclination and longitude of ascending node rotations
        // to transform from perifocal to heliocentric coordinates
        let cos_i = orbit.inclination.cos();
        let sin_i = orbit.inclination.sin();
        let cos_omega = orbit.longitude_ascending_node.cos();
        let sin_omega = orbit.longitude_ascending_node.sin();

        // Heliocentric position using rotation matrices
        let x = x_perifocal * cos_omega - y_perifocal * cos_i * sin_omega;
        let y = x_perifocal * sin_omega + y_perifocal * cos_i * cos_omega;
        let z = y_perifocal * sin_i;

        // Update space coordinates (in AU)
        coords.position = DVec3::new(x, y, z);
    }
}

/// System that converts high-precision SpaceCoordinates to rendering Transform
/// Implements "floating origin" technique by scaling down coordinates and converting to f32
pub fn update_render_transform(
    mut query: Query<(&SpaceCoordinates, &mut Transform), Changed<SpaceCoordinates>>,
) {
    for (coords, mut transform) in query.iter_mut() {
        // Convert from AU to Bevy units using scaling factor
        let scaled_position = coords.position * SCALING_FACTOR;

        // Convert from f64 to f32 for rendering
        // This is safe because we've scaled coordinates to reasonable rendering range
        transform.translation = Vec3::new(
            scaled_position.x as f32,
            scaled_position.y as f32,
            scaled_position.z as f32,
        );
    }
}

/// System that draws orbit paths using gizmos
/// Visualizes Keplerian orbits as ellipses
pub fn draw_orbit_paths(
    mut gizmos: Gizmos,
    query: Query<(&KeplerOrbit, &OrbitPath)>,
) {
    for (orbit, path) in query.iter() {
        if !path.visible {
            continue;
        }

        // Generate points along the orbit by sampling mean anomaly uniformly
        let segments = path.segments;
        let mut points = Vec::with_capacity(segments as usize + 1);
        
        // Pre-calculate step size for mean anomaly sampling
        let mean_anomaly_step = std::f64::consts::TAU / (segments as f64);

        for i in 0..=segments {
            // Uniformly sample mean anomaly (which represents time)
            let mean_anomaly = (i as f64) * mean_anomaly_step;
            
            // Solve for eccentric anomaly
            let eccentric_anomaly = solve_kepler(mean_anomaly, orbit.eccentricity);
            
            // Convert to true anomaly
            let true_anomaly = eccentric_to_true_anomaly(eccentric_anomaly, orbit.eccentricity);
            
            // Calculate radius at this true anomaly
            let radius = orbital_radius(orbit.semi_major_axis, orbit.eccentricity, true_anomaly);
            
            // Position in orbital plane (relative to focus)
            let x_orbital = radius * true_anomaly.cos();
            let y_orbital = radius * true_anomaly.sin();
            
            // Apply argument of periapsis rotation
            let cos_w = orbit.argument_of_periapsis.cos();
            let sin_w = orbit.argument_of_periapsis.sin();
            let x_perifocal = x_orbital * cos_w - y_orbital * sin_w;
            let y_perifocal = x_orbital * sin_w + y_orbital * cos_w;
            
            // Apply inclination and longitude of ascending node
            let cos_i = orbit.inclination.cos();
            let sin_i = orbit.inclination.sin();
            let cos_omega = orbit.longitude_ascending_node.cos();
            let sin_omega = orbit.longitude_ascending_node.sin();
            
            let x = x_perifocal * cos_omega - y_perifocal * cos_i * sin_omega;
            let y = x_perifocal * sin_omega + y_perifocal * cos_i * cos_omega;
            let z = y_perifocal * sin_i;
            
            // Apply scaling to Bevy units
            let scaled_x = (x * SCALING_FACTOR) as f32;
            let scaled_y = (y * SCALING_FACTOR) as f32;
            let scaled_z = (z * SCALING_FACTOR) as f32;
            
            points.push(Vec3::new(scaled_x, scaled_y, scaled_z));
        }

        // Draw the orbit as a line loop
        for i in 0..points.len() - 1 {
            gizmos.line(points[i], points[i + 1], path.color);
        }
    }
}

/// System that controls orbit visibility based on camera distance
/// Moon orbits are only visible when camera is close enough
pub fn update_orbit_visibility_by_zoom(
    camera_query: Query<&Transform, With<GameCamera>>,
    mut orbit_query: Query<(&mut OrbitPath, Option<&Selected>), With<Moon>>,
) {
    // Get camera position
    let Ok(camera_transform) = camera_query.get_single() else {
        return;
    };

    // Calculate distance from camera to origin (solar system center)
    let camera_distance = camera_transform.translation.length();

    // Update visibility for moon orbits based on camera distance
    for (mut orbit_path, selected) in orbit_query.iter_mut() {
        // Always show orbits for selected bodies
        if selected.is_some() {
            orbit_path.visible = true;
        } else {
            // Show moon orbits only when zoomed in close enough
            orbit_path.visible = camera_distance < MOON_ORBIT_VISIBILITY_DISTANCE;
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
) {
    // Only process on mouse click
    if !mouse_button.just_pressed(MouseButton::Left) {
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
        
        // Check if click is within selection radius
        if distance < SELECTION_CLICK_RADIUS {
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

/// System that ensures selected bodies have visible orbits
pub fn update_selected_orbit_visibility(
    selected_query: Query<Entity, (With<Selected>, With<OrbitPath>)>,
    mut orbit_query: Query<&mut OrbitPath>,
) {
    for entity in selected_query.iter() {
        if let Ok(mut orbit_path) = orbit_query.get_mut(entity) {
            orbit_path.visible = true;
        }
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
    
    for (entity, transform, _body) in body_query.iter() {
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
        
        // Check if cursor is within hover radius
        if distance < SELECTION_CLICK_RADIUS {
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

/// System that draws glowing rings around hovered celestial bodies
pub fn draw_hover_effects(
    mut gizmos: Gizmos,
    query: Query<(&GlobalTransform, &CelestialBody), With<Hovered>>,
) {
    for (transform, body) in query.iter() {
        let pos = transform.translation();
        let radius = body.radius * 0.002 + 8.0; // Slightly larger than the body
        
        // Draw glowing ring effect using multiple circles with varying opacity
        let ring_color = Color::srgba(0.4, 0.8, 1.0, 0.8);
        
        // Main ring
        gizmos.circle(pos, Dir3::Y, radius, ring_color);
        
        // Outer glow
        let glow_color = Color::srgba(0.4, 0.8, 1.0, 0.4);
        gizmos.circle(pos, Dir3::Y, radius + 2.0, glow_color);
        
        // Inner highlight
        let highlight_color = Color::srgba(0.6, 0.9, 1.0, 0.6);
        gizmos.circle(pos, Dir3::Y, radius - 2.0, highlight_color);
    }
}

/// System that automatically zooms camera when anchoring to a body
pub fn zoom_camera_to_anchored_body(
    body_query: Query<(&CelestialBody, Option<&Star>), Changed<Selected>>,
    selected_query: Query<Entity, (With<Selected>, With<CelestialBody>)>,
    mut camera_query: Query<(&mut OrbitCamera, &CameraAnchor), With<GameCamera>>,
) {
    // Only trigger when selection changes
    let Ok((mut orbit_camera, anchor)) = camera_query.get_single_mut() else {
        return;
    };
    
    // Check if we have an anchored body that was just selected
    if let Some(anchored_entity) = anchor.0 {
        if let Ok(entity) = selected_query.get_single() {
            if entity == anchored_entity {
                if let Ok((body, is_star)) = body_query.get(entity) {
                    // Calculate appropriate zoom distance
                    let zoom_distance = if is_star.is_some() {
                        // For the Sun, show the entire solar system
                        // Approximately 40 AU should show out to Neptune
                        40.0 * 1500.0 // 1500 is the SCALING_FACTOR
                    } else {
                        // For other bodies, make them fill about 10% of the screen
                        // Assuming a 60° FOV, we need distance = radius * 10
                        let visual_radius = body.radius * 0.002; // RADIUS_SCALE
                        let target_distance = visual_radius * 20.0; // Fill ~10% of screen
                        target_distance.clamp(50.0, 10000.0) // Clamp to reasonable range
                    };
                    
                    orbit_camera.radius = zoom_distance;
                }
            }
        }
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
