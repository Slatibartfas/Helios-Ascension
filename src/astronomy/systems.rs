use bevy::prelude::*;
use bevy::math::DVec3;
use bevy::window::PrimaryWindow;

use super::components::{
    CometTail, Destroyed, HoverMarker, Hovered, KeplerOrbit, LocalOrbitAmplification, MarkerDot,
    MarkerOwner, OrbitPath, Selected, SelectionMarker, SpaceCoordinates,
};
use crate::plugins::solar_system::{CelestialBody, Comet, LogicalParent, Moon, Planet, Star, RADIUS_SCALE};
use crate::plugins::camera::{CameraAnchor, GameCamera, OrbitCamera, ViewMode};
use crate::ui::SimulationTime;

/// Scaling factor for converting astronomical units to Bevy rendering units
/// 1 AU = 1500.0 Bevy units ensures separation between planets and moons
pub const SCALING_FACTOR: f64 = 1500.0;



/// Click radius for body selection (in Bevy units)
/// Bodies within this distance from the ray are considered clickable
const SELECTION_CLICK_RADIUS: f32 = 5.0;

/// Padding for the hover ring around celestial bodies (in Bevy units)
const HOVER_RING_PADDING: f32 = 8.0;  // Creates visible gap between marker and body

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

/// Calculate the 3D orbital position directly from a true anomaly.
/// Unlike `orbit_position_from_mean_anomaly`, this skips the Kepler solver
/// and is used for drawing orbit paths with uniform geometric spacing.
fn orbit_position_from_true_anomaly(orbit: &KeplerOrbit, true_anomaly: f64) -> DVec3 {
    let radius = orbital_radius(orbit.semi_major_axis, orbit.eccentricity, true_anomaly);

    let x_orbital = radius * true_anomaly.cos();
    let y_orbital = radius * true_anomaly.sin();

    let cos_w = orbit.argument_of_periapsis.cos();
    let sin_w = orbit.argument_of_periapsis.sin();
    let x_perifocal = x_orbital * cos_w - y_orbital * sin_w;
    let y_perifocal = x_orbital * sin_w + y_orbital * cos_w;

    let cos_i = orbit.inclination.cos();
    let sin_i = orbit.inclination.sin();
    let cos_omega = orbit.longitude_ascending_node.cos();
    let sin_omega = orbit.longitude_ascending_node.sin();

    let x = x_perifocal * cos_omega - y_perifocal * cos_i * sin_omega;
    let y = x_perifocal * sin_omega + y_perifocal * cos_i * cos_omega;
    let z = y_perifocal * sin_i;

    DVec3::new(x, y, z)
}

/// Convert mean anomaly to true anomaly via the Kepler solver
fn mean_anomaly_to_true_anomaly(mean_anomaly: f64, eccentricity: f64) -> f64 {
    let e_anom = solve_kepler(mean_anomaly, eccentricity);
    eccentric_to_true_anomaly(e_anom, eccentricity)
}

/// System that propagates all orbits based on Keplerian mechanics
/// Updates SpaceCoordinates based on KeplerOrbit elements and elapsed time
/// Uses SimulationTime to allow time scaling via UI controls
pub fn propagate_orbits(
    sim_time: Res<SimulationTime>,
    mut query: Query<(&KeplerOrbit, &mut SpaceCoordinates)>,
) {
    // Get elapsed simulation time in seconds
    let elapsed_time = sim_time.elapsed_seconds();

    for (orbit, mut coords) in query.iter_mut() {
        // Calculate current mean anomaly: M = M₀ + n*t
        let mean_anomaly = orbit.mean_anomaly_epoch + orbit.mean_motion * elapsed_time;

        // Update space coordinates (in AU)
        coords.position = orbit_position_from_mean_anomaly(orbit, mean_anomaly);
    }
}

/// System that converts high-precision SpaceCoordinates to rendering Transform.
/// Implements "floating origin" technique by scaling down coordinates and converting to f32.
///
/// For moons with a [`LocalOrbitAmplification`] component the local position is
/// additionally scaled so that the moon renders outside the parent's visual mesh.
pub fn update_render_transform(
    mut query: Query<
        (
            &SpaceCoordinates,
            &mut Transform,
            Option<&LocalOrbitAmplification>,
            Option<&LogicalParent>,
        ),
        Changed<SpaceCoordinates>,
    >,
    parent_coords: Query<&SpaceCoordinates>,
    floating_origin: Option<Res<crate::astronomy::components::FloatingOrigin>>,
) {
    let origin_offset = floating_origin.map(|fo| fo.position).unwrap_or(DVec3::ZERO);

    for (coords, mut transform, amplification, logical_parent) in query.iter_mut() {
        let amp = amplification.map(|a| a.0 as f64).unwrap_or(1.0);

        // Convert from AU to Bevy units, applying local amplification for moons
        // Shift by floating origin BEFORE scaling
        let scaled_position = (coords.position - origin_offset) * SCALING_FACTOR * amp;

        // For moons (with orbit amplification) add the parent's world position,
        // since moons are NOT spatial children of their parent planet.
        // This avoids the planet's spin rotation being applied to the moon.
        let parent_offset = if amplification.is_some() {
            logical_parent
                .and_then(|lp| parent_coords.get(lp.0).ok())
                .map(|parent_sc| {
                    let pos = (parent_sc.position - origin_offset) * SCALING_FACTOR;
                    Vec3::new(pos.x as f32, pos.y as f32, pos.z as f32)
                })
                .unwrap_or(Vec3::ZERO)
        } else {
            Vec3::ZERO
        };

        // Convert from f64 to f32 for rendering
        transform.translation = Vec3::new(
            scaled_position.x as f32,
            scaled_position.y as f32,
            scaled_position.z as f32,
        ) + parent_offset;
    }
}

/// System that draws orbit paths as fading trails (Terra Invicta style).
/// The trail is brightest at the body's current position and fades out
/// behind it, creating a comet-tail effect along the orbit.
///
/// Samples uniformly in **true anomaly** so that highly eccentric orbits
/// (comets, long-period objects) get even point density along the geometric
/// ellipse rather than clustering near apoapsis.
pub fn draw_orbit_paths(
    mut gizmos: Gizmos,
    sim_time: Res<SimulationTime>,
    query: Query<(
        &KeplerOrbit,
        &OrbitPath,
        Option<&LogicalParent>,
        Option<&LocalOrbitAmplification>,
    )>,
    parent_coords: Query<&SpaceCoordinates>,
    floating_origin: Option<Res<crate::astronomy::components::FloatingOrigin>>,
) {
    let elapsed_time = sim_time.elapsed_seconds();
    let origin_offset = floating_origin.map(|fo| fo.position).unwrap_or(DVec3::ZERO);

    for (orbit, path, logical_parent, amplification) in query.iter() {
        if !path.visible {
            continue;
        }

        let amp = amplification.map(|a| a.0 as f64).unwrap_or(1.0);

        let parent_offset = logical_parent
            .and_then(|lp| parent_coords.get(lp.0).ok())
            .map(|sc| {
                let pos = (sc.position - origin_offset) * SCALING_FACTOR;
                Vec3::new(pos.x as f32, pos.y as f32, pos.z as f32)
            })
            .unwrap_or(Vec3::ZERO);

        // Current true anomaly of the body
        let current_mean_anomaly = orbit.mean_anomaly_epoch + orbit.mean_motion * elapsed_time;
        let current_true_anomaly = mean_anomaly_to_true_anomaly(
            current_mean_anomaly.rem_euclid(std::f64::consts::TAU),
            orbit.eccentricity,
        );

        // Use more segments for eccentric orbits to keep the periapsis region smooth
        let segments = if orbit.eccentricity > 0.6 {
            (path.segments as f64 * (1.0 + orbit.eccentricity * 2.0)) as u32
        } else {
            path.segments
        };

        let true_anomaly_step = std::f64::consts::TAU / (segments as f64);

        // Extract base color channels from path color
        let base = path.color.to_srgba();

        // Trail covers the full orbit but fades from current position backwards.
        // Segment 0 is the body's current position (brightest).
        // Segment N is the point just before the body (dimmest / invisible).
        let mut prev_point: Option<Vec3> = None;

        for i in 0..=segments {
            // Walk backwards from the current position in true anomaly
            let true_anomaly = current_true_anomaly - (i as f64) * true_anomaly_step;
            let position_au = orbit_position_from_true_anomaly(orbit, true_anomaly);

            let scaled_x = (position_au.x * SCALING_FACTOR * amp) as f32;
            let scaled_y = (position_au.y * SCALING_FACTOR * amp) as f32;
            let scaled_z = (position_au.z * SCALING_FACTOR * amp) as f32;

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

/// Distance in AU within which a comet tail becomes visible.
/// Real comets start developing tails around 3-5 AU from the Sun.
const COMET_TAIL_ONSET_AU: f64 = 5.0;

/// Minimum distance from sun (in AU) to render tail - avoids rendering inside sun
const COMET_TAIL_MIN_DISTANCE_AU: f64 = 0.02;

/// Maximum visual tail length in Bevy units at perihelion
const COMET_TAIL_MAX_LENGTH: f32 = 300.0;

/// Number of radial segments around the tail cone
const TAIL_RADIAL_SEGMENTS: u32 = 16;

/// Number of length segments for smooth gradients
const TAIL_LENGTH_SEGMENTS: u32 = 32;

/// Number of volumetric strands for ion/dust tail gizmo lines
const TAIL_VOLUME_STRANDS: u32 = 6;

/// Number of line segments per individual comet-tail strand
const COMET_TAIL_SEGMENTS: u32 = 24;

/// Creates a tapered cone mesh with vertex colors for gradient transparency.
/// Used for volumetric comet tails with smooth fade from base to tip.
fn create_tail_cone_mesh(
    length: f32,
    base_radius: f32,
    tip_radius: f32,
    base_color: Color,
    tip_color: Color,
) -> Mesh {
    use bevy::render::mesh::{Indices, PrimitiveTopology};
    use bevy::render::render_asset::RenderAssetUsages;
    
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut colors = Vec::new();
    let mut indices = Vec::new();

    // Generate vertices along the cone length
    for length_i in 0..=TAIL_LENGTH_SEGMENTS {
        let t = length_i as f32 / TAIL_LENGTH_SEGMENTS as f32;
        let z = length * t;
        let radius = base_radius + (tip_radius - base_radius) * t;
        
        // Interpolate color
        let base_rgba = base_color.to_srgba();
        let tip_rgba = tip_color.to_srgba();
        let color = Color::srgba(
            base_rgba.red + (tip_rgba.red - base_rgba.red) * t,
            base_rgba.green + (tip_rgba.green - base_rgba.green) * t,
            base_rgba.blue + (tip_rgba.blue - base_rgba.blue) * t,
            base_rgba.alpha + (tip_rgba.alpha - base_rgba.alpha) * t,
        );

        // Create ring of vertices
        for radial_i in 0..TAIL_RADIAL_SEGMENTS {
            let theta = (radial_i as f32 / TAIL_RADIAL_SEGMENTS as f32) * std::f32::consts::TAU;
            let (sin_theta, cos_theta) = theta.sin_cos();
            
            let x = radius * cos_theta;
            let y = radius * sin_theta;
            
            positions.push([x, y, z]);
            
            // Normal points outward from cone surface
            let normal = Vec3::new(cos_theta, sin_theta, 0.0).normalize();
            normals.push(normal.to_array());
            
            colors.push(color.to_linear().to_f32_array());
        }
    }

    // Generate indices for triangle strip
    for length_i in 0..TAIL_LENGTH_SEGMENTS {
        for radial_i in 0..TAIL_RADIAL_SEGMENTS {
            let next_radial = (radial_i + 1) % TAIL_RADIAL_SEGMENTS;
            
            let current_ring = length_i * TAIL_RADIAL_SEGMENTS;
            let next_ring = (length_i + 1) * TAIL_RADIAL_SEGMENTS;
            
            let i0 = current_ring + radial_i;
            let i1 = current_ring + next_radial;
            let i2 = next_ring + radial_i;
            let i3 = next_ring + next_radial;
            
            // Two triangles per quad
            indices.push(i0);
            indices.push(i2);
            indices.push(i1);
            
            indices.push(i1);
            indices.push(i2);
            indices.push(i3);
        }
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    
    mesh
}

/// System that spawns and manages volumetric 3D mesh-based comet tails.
/// Creates true geometry with gradient transparency for realistic appearance.
pub fn manage_comet_tail_meshes(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    comet_query: Query<(Entity, &CelestialBody, &KeplerOrbit, &SpaceCoordinates), (With<Comet>, Without<Destroyed>)>,
    tail_query: Query<(Entity, &CometTail)>,
    existing_tails: Query<&CometTail>,
) {
    // Track which comets should have tails
    let mut comets_needing_tails = std::collections::HashSet::new();
    
    for (entity, body, _orbit, coords) in comet_query.iter() {
        let distance_au = coords.position.length();

        // Check if tail should be visible
        if distance_au <= COMET_TAIL_ONSET_AU 
            && distance_au >= COMET_TAIL_MIN_DISTANCE_AU 
            && distance_au > 1e-6 {
            comets_needing_tails.insert(entity);
            
            // Check if this comet already has tails
            let has_tails = existing_tails.iter().any(|t| t.comet_entity == entity);
            
            if !has_tails {
                // Spawn tail meshes for this comet
                spawn_comet_tail_meshes(
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    entity,
                    body,
                    coords,
                    distance_au,
                );
            }
        }
    }
    
    // Despawn tails for comets that no longer need them
    for (tail_entity, tail) in tail_query.iter() {
        if !comets_needing_tails.contains(&tail.comet_entity) {
            commands.entity(tail_entity).despawn_recursive();
        }
    }
}

/// Spawns ion and dust tail meshes for a comet
fn spawn_comet_tail_meshes(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    comet_entity: Entity,
    body: &CelestialBody,
    _coords: &SpaceCoordinates,
    distance_au: f64,
) {
    // Calculate tail parameters
    let intensity = ((1.0 - distance_au / COMET_TAIL_ONSET_AU) as f32).clamp(0.0, 1.0);
    let proximity_boost = (0.5 / distance_au.max(0.1)) as f32;
    let brightness = (intensity * proximity_boost.min(3.0)).clamp(0.0, 1.0);
    let tail_length = COMET_TAIL_MAX_LENGTH * intensity * proximity_boost.min(2.5);

    // Seed for procedural variation
    let mut seed = 0u32;
    for byte in body.name.bytes() {
        seed = seed.wrapping_mul(31).wrapping_add(byte as u32);
    }

    // === ION TAIL (Type I): narrow, bluish-white ===
    // Use fixed small radii, slightly larger as requested
    let ion_base_radius = 1.0; 
    let ion_tip_radius = 0.2;
    let ion_base_color = Color::srgba(0.7, 0.85, 1.0, brightness * 0.6);
    let ion_tip_color = Color::srgba(0.5, 0.75, 1.0, 0.0);
    
    let ion_mesh = meshes.add(create_tail_cone_mesh(
        tail_length,
        ion_base_radius,
        ion_tip_radius,
        ion_base_color,
        ion_tip_color,
    ));
    
    let ion_material = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        emissive: LinearRgba::new(0.5, 0.7, 1.0, 0.0) * brightness,
        alpha_mode: AlphaMode::Add,
        unlit: true,
        cull_mode: None, // Double-sided
        ..default()
    });
    
    commands.spawn((
        PbrBundle {
            mesh: ion_mesh,
            material: ion_material,
            transform: Transform::default(), // Will be updated by update_tail_transforms
            ..default()
        },
        CometTail {
            comet_entity,
            is_ion_tail: true,
        },
    ));

    // === DUST TAIL (Type II): wider, yellowish ===
    // Fixed radii, wider than ion tail and enclosing it at base
    let dust_base_radius = 1.6;
    let dust_tip_radius = 0.4;
    let dust_base_color = Color::srgba(1.0, 0.85, 0.4, brightness * 0.5);
    let dust_tip_color = Color::srgba(1.0, 0.7, 0.2, 0.0);
    
    let dust_mesh = meshes.add(create_tail_cone_mesh(
        tail_length * 0.7, // Dust tail is shorter
        dust_base_radius,
        dust_tip_radius,
        dust_base_color,
        dust_tip_color,
    ));
    
    let dust_material = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        emissive: LinearRgba::new(1.0, 0.75, 0.3, 0.0) * brightness * 0.8,
        alpha_mode: AlphaMode::Add,
        unlit: true,
        cull_mode: None,
        ..default()
    });
    
    commands.spawn((
        PbrBundle {
            mesh: dust_mesh,
            material: dust_material,
            transform: Transform::default(),
            ..default()
        },
        CometTail {
            comet_entity,
            is_ion_tail: false,
        },
    ));
}

/// System that updates tail mesh positions and orientations each frame.
/// Tails always point away from the sun and follow their parent comet.
pub fn update_tail_transforms(
    comet_query: Query<(&SpaceCoordinates, &KeplerOrbit, &CelestialBody), With<Comet>>,
    mut tail_query: Query<(&mut Transform, &CometTail)>,
) {
    for (mut transform, tail) in tail_query.iter_mut() {
        if let Ok((coords, orbit, body)) = comet_query.get(tail.comet_entity) {
            // Convert comet position to rendering coordinates
            let comet_pos_scaled = coords.position * SCALING_FACTOR;
            let comet_pos = Vec3::new(
                comet_pos_scaled.x as f32,
                comet_pos_scaled.y as f32,
                comet_pos_scaled.z as f32,
            );

            // Anti-sunward direction (sun at origin)
            let to_sun = -comet_pos;
            let sun_distance = to_sun.length();
            if sun_distance < 1e-6 {
                continue;
            }
            let anti_sun_dir = -to_sun.normalize();

            // Offset tail to start at comet surface
            // Both tails start at the same point to avoid dual-cone effect
            let surface_offset = body.visual_radius * anti_sun_dir;
            
            transform.translation = comet_pos + surface_offset;

            // Orient tail to point away from sun
            // Cone extends along +Z axis, so look along anti-sunward direction
            if tail.is_ion_tail {
                // Ion tail points straight away from sun
                transform.rotation = Quat::from_rotation_arc(Vec3::Z, anti_sun_dir);
            } else {
                // Dust tail has slight curve based on orbit
                let orbit_normal = Vec3::new(
                    orbit.longitude_ascending_node.sin() as f32 * orbit.inclination.sin() as f32,
                    orbit.inclination.cos() as f32,
                    orbit.longitude_ascending_node.cos() as f32 * orbit.inclination.sin() as f32,
                );
                let velocity_dir = anti_sun_dir.cross(orbit_normal).normalize_or_zero();
                let curved_dir = (anti_sun_dir + velocity_dir * 0.15).normalize();
                
                transform.rotation = Quat::from_rotation_arc(Vec3::Z, curved_dir);
            }
        }
    }
}
///
/// The tail always points away from the Sun and grows longer + brighter
/// as the comet approaches perihelion. Two visual tails are drawn:
/// - **Ion tail**: straight, narrow, bluish — points directly anti-sunward
/// - **Dust tail**: slightly curved, broader, yellowish — trails behind
///
/// Uses SpaceCoordinates directly for smooth rendering during time acceleration.
pub fn draw_comet_tails(
    view_mode: Res<ViewMode>,
    mut gizmos: Gizmos,
    query: Query<(&CelestialBody, &KeplerOrbit, &SpaceCoordinates), (With<Comet>, Without<Destroyed>)>,
) {
    // Skip comet tails in starmap view
    if *view_mode == ViewMode::Starmap {
        return;
    }

    for (body, orbit, coords) in query.iter() {
        // Current heliocentric distance in AU
        let distance_au = coords.position.length();

        // Only draw tail if within the onset distance and not too close to sun
        if distance_au > COMET_TAIL_ONSET_AU 
            || distance_au < COMET_TAIL_MIN_DISTANCE_AU 
            || distance_au < 1e-6 {
            continue;
        }

        // Convert high-precision position to rendering coordinates
        let body_pos_scaled = coords.position * SCALING_FACTOR;
        let body_pos = Vec3::new(
            body_pos_scaled.x as f32,
            body_pos_scaled.y as f32,
            body_pos_scaled.z as f32,
        );

        // Anti-sunward direction (sun is at origin)
        let to_sun = -body_pos;
        let sun_distance = to_sun.length();
        if sun_distance < 1e-6 {
            continue;
        }
        let anti_sun_dir = -to_sun.normalize();

        // Tail intensity scales inversely with distance squared (solar radiation)
        // Normalized: 1.0 at 0.5 AU, fading to 0 at onset distance
        let intensity = ((1.0 - distance_au / COMET_TAIL_ONSET_AU) as f32).clamp(0.0, 1.0);
        let proximity_boost = (0.5 / distance_au.max(0.1)) as f32; // Brighter when closer
        let brightness = (intensity * proximity_boost.min(3.0)).clamp(0.0, 1.0);

        // Tail length scales with proximity - longer when closer to sun
        let tail_length = COMET_TAIL_MAX_LENGTH * intensity * proximity_boost.min(2.5);

        if tail_length < 1.0 || brightness < 0.01 {
            continue;
        }

        // Use body name hash for consistent slight curl direction on the dust tail
        let mut seed = 0u32;
        for byte in body.name.bytes() {
            seed = seed.wrapping_mul(31).wrapping_add(byte as u32);
        }
        let curl_angle = ((seed % 1000) as f32 / 1000.0 - 0.5) * 0.3; // slight random curl

        // Find a perpendicular vector for the dust tail curve
        let up = if anti_sun_dir.y.abs() > 0.9 {
            Vec3::X
        } else {
            Vec3::Y
        };
        let perp = anti_sun_dir.cross(up).normalize();
        let perp2 = anti_sun_dir.cross(perp).normalize();

        // Compute orbit velocity direction for a more realistic dust tail curve
        // Dust tail curves slightly in the direction opposite to orbital motion
        let orbit_normal = Vec3::new(
            orbit.longitude_ascending_node.sin() as f32 * orbit.inclination.sin() as f32,
            orbit.inclination.cos() as f32,
            orbit.longitude_ascending_node.cos() as f32 * orbit.inclination.sin() as f32,
        );
        let velocity_approx = anti_sun_dir.cross(orbit_normal).normalize_or_zero();

        // === ION TAIL (Type I): straight, narrow, bluish-white ===
        // Draw multiple strands with Fibonacci spiral distribution for natural appearance
        for strand in 0..TAIL_VOLUME_STRANDS {
            // Fibonacci spiral for even distribution (better than uniform circle)
            let golden_angle = std::f32::consts::PI * (3.0 - 5.0_f32.sqrt());
            let angle = golden_angle * (strand as f32);
            let radius_factor = ((strand as f32 + 0.5) / TAIL_VOLUME_STRANDS as f32).sqrt();
            
            let (sin_a, cos_a) = angle.sin_cos();
            
            // Offset perpendicular to tail direction - varies by strand
            let base_offset_radius = tail_length * 0.02 * radius_factor; // 0-2% of tail length
            let base_offset = (perp * cos_a + perp2 * sin_a) * base_offset_radius;
            
            // Procedural variation per strand for natural look
            let strand_seed = seed.wrapping_add(strand * 997);
            let strand_var = ((strand_seed % 1000) as f32) / 1000.0;
            let wiggle_phase = strand_var * std::f32::consts::TAU;
            
            let mut prev = body_pos;
            for i in 1..=COMET_TAIL_SEGMENTS {
                let t = i as f32 / COMET_TAIL_SEGMENTS as f32;
                
                // Gentle expansion and wiggle along the tail
                let expanding = 1.0 + t * 0.4;
                let wiggle = (t * 8.0 + wiggle_phase).sin() * 0.15 * t;
                let offset = base_offset * expanding + perp2 * wiggle * base_offset_radius;
                
                let pos = body_pos + anti_sun_dir * tail_length * t + offset;

                // Fade from bright near body to transparent at tip
                // Use per-strand brightness variation for natural look
                let strand_brightness = 0.8 + strand_var * 0.4;
                let alpha = brightness * 0.5 * strand_brightness * (1.0 - t).powf(1.5) / (TAIL_VOLUME_STRANDS as f32 * 0.7);
                
                if alpha > 0.005 {
                    // Slight color variation per strand
                    let blue_var = 0.95 + strand_var * 0.05;
                    let color = Color::srgba(
                        0.5 + 0.3 * (1.0 - t), // slight white near head
                        0.65 + 0.2 * (1.0 - t),
                        blue_var,
                        alpha,
                    );
                    gizmos.line(prev, pos, color);
                }
                prev = pos;
            }
        }

        // === DUST TAIL (Type II): curved, broader, yellowish ===
        // Draw multiple strands with more variation and curvature
        for strand in 0..TAIL_VOLUME_STRANDS {
            // Fibonacci spiral distribution
            let golden_angle = std::f32::consts::PI * (3.0 - 5.0_f32.sqrt());
            let angle = golden_angle * (strand as f32) + 0.5; // offset from ion tail
            let radius_factor = ((strand as f32 + 0.5) / TAIL_VOLUME_STRANDS as f32).sqrt();
            
            let (sin_a, cos_a) = angle.sin_cos();
            
            // Wider cone for dust tail
            let base_offset_radius = tail_length * 0.045 * radius_factor; // 0-4.5% of tail length
            let base_offset = (perp * cos_a + perp2 * sin_a) * base_offset_radius;
            
            // More variation for dust particles
            let strand_seed = seed.wrapping_add(strand * 1009);
            let strand_var = ((strand_seed % 1000) as f32) / 1000.0;
            let wiggle_phase = strand_var * std::f32::consts::TAU;
            
            let mut prev = body_pos;
            for i in 1..=COMET_TAIL_SEGMENTS {
                let t = i as f32 / COMET_TAIL_SEGMENTS as f32;

                // Dust tail is shorter and curves away from orbit direction
                let dust_length = tail_length * 0.7;
                let curve = t * t * 0.3; // quadratic curve
                
                // More expansion and wiggle for dust
                let expanding = 1.0 + t * 1.5;
                let wiggle = ((t * 6.0 + wiggle_phase).sin() * 0.2 + (t * 3.5).cos() * 0.15) * t;
                let offset = base_offset * expanding + perp2 * wiggle * base_offset_radius;

                let pos = body_pos
                    + anti_sun_dir * dust_length * t
                    + (perp * curl_angle + velocity_approx * 0.15) * dust_length * curve
                    + offset;

                // More variation in dust brightness
                let strand_brightness = 0.7 + strand_var * 0.5;
                let alpha = brightness * 0.4 * strand_brightness * (1.0 - t).powf(1.3) / (TAIL_VOLUME_STRANDS as f32 * 0.7);
                
                if alpha > 0.005 {
                    // Color varies more along dust tail
                    let yellow_var = 0.92 + strand_var * 0.08;
                    let color = Color::srgba(
                        1.0,
                        yellow_var - 0.15 * t, // yellower at tip
                        0.4 - 0.2 * t,         // orange tint at tip
                        alpha,
                    );
                    gizmos.line(prev, pos, color);
                }
                prev = pos;
            }
        }

        // === COMA (fuzzy glow around the nucleus) ===
        // Draw a small radial starburst around the body
        {
            let coma_radius = body.visual_radius * 2.5 * brightness.max(0.3);
            let coma_alpha = brightness * 0.35;
            if coma_alpha > 0.01 {
                let num_rays = 12;
                for i in 0..num_rays {
                    let angle = (i as f32 / num_rays as f32) * std::f32::consts::TAU;
                    let (sin_a, cos_a) = angle.sin_cos();

                    // Use perpendicular vectors to create rays in a plane
                    let ray_dir = (perp * cos_a + perp2 * sin_a).normalize();
                    let tip = body_pos + ray_dir * coma_radius;

                    let color = Color::srgba(0.9, 0.95, 1.0, coma_alpha * 0.5);
                    gizmos.line(body_pos, tip, color);
                }

                // Sunward jet (brighter toward sun)
                let jet_length = coma_radius * 1.5;
                let jet_tip = body_pos - anti_sun_dir * jet_length; // toward sun
                let jet_color = Color::srgba(0.95, 0.95, 1.0, coma_alpha * 0.7);
                gizmos.line(body_pos, jet_tip, jet_color);
            }
        }
    }
}

/// Perihelion distance (in AU) at which ISON disintegrates
/// Historical: ISON broke apart around 730,000 km from sun surface (0.0049 AU from center)
const ISON_DESTRUCTION_DISTANCE_AU: f64 = 0.005;

/// System that checks for natural destruction events (e.g., Comet ISON solar disintegration).
/// This system monitors comets approaching the sun and triggers destruction for historically
/// accurate events like ISON's breakup.
pub fn check_natural_destruction(
    mut commands: Commands,
    sim_time: Res<SimulationTime>,
    query: Query<(Entity, &CelestialBody, &SpaceCoordinates), (With<Comet>, Without<Destroyed>)>,
) {
    for (entity, body, coords) in query.iter() {
        let distance_au = coords.position.length();
        
        // Check for ISON specifically - historically disintegrated near perihelion in Nov 2013
        if body.name == "Comet ISON" && distance_au < ISON_DESTRUCTION_DISTANCE_AU {
            info!("Comet ISON disintegrating due to solar proximity at {:.4} AU", distance_au);
            commands.entity(entity).insert(Destroyed::new(
                sim_time.elapsed_seconds(),
                2.0, // 2 second fade-out
            ));
        }
        
        // Additional destruction checks can be added here for other scenarios:
        // - Mining operations completing
        // - Weapon impacts
        // - Orbital decay into planets
        // - Collision events
    }
}

/// System that fades out and despawns destroyed celestial bodies.
/// Bodies fade out over their specified duration, then are removed from the simulation along
/// with any child entities (markers, trails, etc.).
pub fn fade_destroyed_bodies(
    mut commands: Commands,
    sim_time: Res<SimulationTime>,
    mut query: Query<(
        Entity,
        &Destroyed,
        Option<&mut Visibility>,
        &Children,
    ), With<CelestialBody>>,
    child_query: Query<Entity>,
) {
    let current_time = sim_time.elapsed_seconds();
    
    for (entity, destroyed, visibility, children) in query.iter_mut() {
        let elapsed = current_time - destroyed.destruction_time;
        
        if destroyed.fade_duration <= 0.0 || elapsed >= destroyed.fade_duration {
            // Fade complete or instant destruction - despawn the entity and all children
            info!("Despawning destroyed celestial body (entity {:?})", entity);
            
            // Despawn all children first (markers, trails, etc.)
            for child in children.iter() {
                if let Ok(child_entity) = child_query.get(*child) {
                    commands.entity(child_entity).despawn_recursive();
                }
            }
            
            // Despawn the body itself
            commands.entity(entity).despawn_recursive();
        } else if let Some(mut vis) = visibility {
            // During fade-out, gradually hide the body
            // Could also modify alpha/emissive here if we add that capability
            let fade_progress = elapsed / destroyed.fade_duration;
            if fade_progress > 0.8 {
                *vis = Visibility::Hidden;
            }
        }
    }
}

/// System that controls orbit visibility based on body type and camera anchor.
///
/// Moon orbits are only shown when their parent planet is the camera's anchor,
/// preventing overlapping moon systems from cluttering the view.
pub fn update_orbit_visibility(
    view_mode: Res<ViewMode>,
    camera_query: Query<&CameraAnchor, With<GameCamera>>,
    mut orbit_query: Query<(
        &mut OrbitPath,
        Option<&Selected>,
        Option<&Planet>,
        Option<&Moon>,
        Option<&LogicalParent>,
    )>,
) {
    let Ok(anchor) = camera_query.get_single() else {
        return;
    };

    for (mut orbit_path, selected, planet, moon, logical_parent) in orbit_query.iter_mut() {
        // Hide all orbits in starmap view
        if *view_mode == ViewMode::Starmap {
            orbit_path.visible = false;
            continue;
        }

        if selected.is_some() {
            // Selected bodies always show their orbit
            orbit_path.visible = true;
        } else if planet.is_some() {
            // Planets always show their orbit
            orbit_path.visible = true;
        } else if moon.is_some() {
            // Show moon orbits only when the parent planet is the camera anchor
            orbit_path.visible = anchor.0.is_some()
                && logical_parent.map(|lp| Some(lp.0) == anchor.0).unwrap_or(false);
        } else {
            // Asteroids, Comets, DwarfPlanets are hidden by default
            orbit_path.visible = false;
        }
    }
}

/// System that toggles moon mesh visibility based on camera anchor.
///
/// Moons are only visible when their parent planet is the camera's anchor.
/// This prevents overlapping moon systems from different planets.
pub fn update_body_lod_visibility(
    camera_query: Query<&CameraAnchor, With<GameCamera>>,
    mut body_query: Query<
        (
            &mut Visibility,
            Option<&LogicalParent>,
            Option<&Moon>,
            Option<&Selected>,
        ),
        With<CelestialBody>,
    >,
) {
    let Ok(anchor) = camera_query.get_single() else {
        return;
    };

    for (mut visibility, logical_parent, moon, selected) in body_query.iter_mut() {
        // Selected bodies are always visible
        if selected.is_some() {
            *visibility = Visibility::Inherited;
            continue;
        }

        if moon.is_some() {
            // Moon visibility: only when parent planet is the camera anchor
            let parent_anchored = anchor.0.is_some()
                && logical_parent.map(|lp| Some(lp.0) == anchor.0).unwrap_or(false);
            *visibility = if parent_anchored {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
        // Planets, Stars, Asteroids, Dwarf Planets: always visible
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
    view_mode: Res<ViewMode>,
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
    // Disable body selection in starmap view
    if *view_mode == ViewMode::Starmap {
        return;
    }

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
    view_mode: Res<ViewMode>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    body_query: Query<(Entity, &GlobalTransform, &CelestialBody)>,
    mut commands: Commands,
    hovered_query: Query<Entity, With<Hovered>>,
    mut egui_contexts: bevy_egui::EguiContexts,
) {
    // Disable hover in starmap view
    if *view_mode == ViewMode::Starmap {
        for entity in hovered_query.iter() {
            commands.entity(entity).remove::<Hovered>();
        }
        return;
    }

    // Safety check: ensure we have access to egui context
    // If the cursor is over a UI element, don't perform world picking
    let ctx = egui_contexts.ctx_mut();
    if ctx.is_pointer_over_area() || ctx.wants_pointer_input() {
        // Clear all hovers if we are over UI
        for entity in hovered_query.iter() {
            commands.entity(entity).remove::<Hovered>();
        }
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
    moon_parent_query: Query<&LogicalParent, With<Moon>>,
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
                let visual_radius = (body.radius * RADIUS_SCALE).max(5.0);
                
                // Check if any moon has this body as its logical parent
                let has_moons = moon_parent_query
                    .iter()
                    .any(|lp| lp.0 == anchored_entity);
                
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
        app.init_resource::<SimulationTime>();
        app.add_systems(Update, propagate_orbits);

        // Spawn an entity with circular orbit
        let orbit = KeplerOrbit::circular(1.0, std::f64::consts::TAU); // 1 AU, 1 radian/second
        let coords = SpaceCoordinates::default();
        app.world_mut().spawn((orbit, coords));

        // Advance simulation time so orbit has moved
        app.world_mut().resource_mut::<SimulationTime>().elapsed = 0.1;

        // Run one update
        app.update();

        // Verify the entity was processed (coordinates should be updated)
        let mut query = app.world_mut().query::<&SpaceCoordinates>();
        let coords = query.iter(app.world()).next().unwrap();
        // For a circular orbit with elapsed > 0, position should have moved from origin
        assert!(coords.position.x.abs() > 0.0 || coords.position.y.abs() > 0.0);
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
