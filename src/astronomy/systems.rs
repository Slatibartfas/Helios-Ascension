use bevy::prelude::*;
use bevy::math::DVec3;

use super::components::{KeplerOrbit, OrbitPath, SpaceCoordinates};

/// Scaling factor for converting astronomical units to Bevy rendering units
/// 1 AU = 100.0 Bevy units keeps planets within reasonable camera frustum
pub const SCALING_FACTOR: f64 = 100.0;

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
    let true_anomaly = 2.0 * (sqrt_term * (eccentric_anomaly / 2.0).tan()).atan();
    
    true_anomaly
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
pub fn propagate_orbits(
    time: Res<Time>,
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

        for i in 0..=segments {
            // Uniformly sample mean anomaly (which represents time)
            let mean_anomaly = (i as f64) * std::f64::consts::TAU / (segments as f64);
            
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
        app.init_resource::<Time>();
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
