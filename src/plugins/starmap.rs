//! Starmap view module
//!
//! When the camera zooms out past `STARMAP_TRANSITION_THRESHOLD`, the game
//! transitions from the detailed solar-system view to a sector/galaxy-level
//! starmap. In the starmap:
//!
//!  - Individual celestial bodies and orbit paths are hidden.
//!  - Each star system is represented by a single glowing icon/billboard.
//!  - Double-clicking a system icon anchors the camera and allows zoom-in.
//!
//! Currently only the Sol system exists; more systems will be added later.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy::math::DVec3;
use std::collections::HashMap;

use super::camera::{CameraAnchor, GameCamera, OrbitCamera, ViewMode};
use super::solar_system::{CelestialBody, Star, Planet, RADIUS_SCALE};
use super::solar_system_data::BodyType;
use crate::astronomy::components::{FloatingOrigin, CurrentStarSystem, SystemId, KeplerOrbit, SpaceCoordinates, OrbitCenter, OrbitPath};
use crate::astronomy::SCALING_FACTOR;
use crate::astronomy::nearby_stars::NearbyStarsData;
use rand::prelude::*;
use std::f64::consts::PI;

// Local constant for star scaling (matches solar_system.rs)
const STAR_RADIUS_SCALE: f32 = 0.00015;
const MIN_VISUAL_RADIUS: f32 = 5.0;

/// Default bounding radius for systems without calculated data (in AU).
/// Used for Sol system and as fallback. Sol extends to ~355 AU (Comet NEOWISE).
const DEFAULT_BOUNDING_RADIUS_AU: f64 = 400.0;

/// Default bounding radius for procedurally generated systems (in AU).
/// Most exoplanet systems have planets within ~10 AU; use conservative estimate.
const FALLBACK_BOUNDING_RADIUS_AU: f64 = 50.0;

/// Resource storing metadata about each star system, primarily their bounding radius.
/// This is used to calculate dynamic zoom thresholds.
#[derive(Resource, Default)]
pub struct SystemMetadata {
    /// Map from SystemId to bounding radius in AU
    pub bounding_radii: HashMap<usize, f64>,
}

impl SystemMetadata {
    pub fn set_bounding_radius(&mut self, system_id: usize, radius_au: f64) {
        self.bounding_radii.insert(system_id, radius_au);
    }
    
    pub fn get_bounding_radius(&self, system_id: usize) -> f64 {
        self.bounding_radii.get(&system_id).copied().unwrap_or(DEFAULT_BOUNDING_RADIUS_AU)
    }
}

/// Plugin that manages the starmap view layer.
pub struct StarmapPlugin;

impl Plugin for StarmapPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CurrentStarSystem>()
            .init_resource::<FloatingOrigin>()
            .init_resource::<SystemMetadata>()
            .add_systems(Startup, setup_starmap)
            .add_systems(
                Update,
                (
                    tag_sol_bodies,
                    spawn_system_bodies, // Handle spawning for non-Sol systems
                    toggle_system_view_entities,
                    update_starmap_visibility,
                    update_starmap_icon_scale,
                    update_starmap_coordinates,
                    handle_starmap_selection,
                    handle_system_transition,
                ),
            );
    }
}

// ── Components ──────────────────────────────────────────────────────────────

/// Marker for starmap-level star system icons.
#[derive(Component)]
pub struct StarSystemIcon {
    /// Unique ID of the system (index in the stars array)
    pub id: usize,
    /// Display name shown in the starmap
    pub name: String,
    /// Position in Universe space (AU) from Sol
    pub position: DVec3,
    /// Bounding radius of the system in AU (distance to outermost body)
    /// Used to determine appropriate zoom transition threshold
    pub bounding_radius_au: f64,
}

/// Tag for the Sol system's starmap icon (spawned once at startup).
#[derive(Component)]
pub struct SolSystemIcon;

/// Marker for the currently selected/anchored star system in starmap view.
#[derive(Component)]
pub struct SelectedStarSystem;

// ── Startup ─────────────────────────────────────────────────────────────────

// 1 Light Year in Astronomical Units
const LY_TO_AU: f64 = 63241.077;

struct NearbyStarData {
    name: &'static str,
    pos_ly: [f64; 3], // x, y, z in Light Years
    spectral_type: &'static str, // For color
}

// 50 Closest Star Systems to Sol (excluding Sol)
// Coordinates in Light Years (Equatorial J2000 Cartesian)
const NEARBY_STARS: &[NearbyStarData] = &[
    NearbyStarData { name: "Alpha Centauri", pos_ly: [-1.5477, -1.1846, -3.7728], spectral_type: "G2V" },
    NearbyStarData { name: "Barnard's Star", pos_ly: [-0.0568, -5.9426, 0.4879], spectral_type: "M4.0Ve" },
    NearbyStarData { name: "Luhman 16", pos_ly: [-3.7012, 1.1792, -5.2152], spectral_type: "L8" },
    NearbyStarData { name: "WISE 0855-0714", pos_ly: [-5.1011, 5.3203, -0.9371], spectral_type: "Y4" },
    NearbyStarData { name: "Wolf 359", pos_ly: [-7.4995, 2.1332, 0.9594], spectral_type: "M6.0V" },
    NearbyStarData { name: "Lalande 21185", pos_ly: [-6.5166, 1.6448, 4.8777], spectral_type: "M2.0V" },
    NearbyStarData { name: "Sirius", pos_ly: [-1.6326, 8.18, -2.5051], spectral_type: "A1V" },
    NearbyStarData { name: "Luyten 726-8", pos_ly: [7.5367, 3.4753, -2.6887], spectral_type: "M5.5Ve" },
    NearbyStarData { name: "Ross 154", pos_ly: [1.915, -8.6694, -3.9225], spectral_type: "M3.5Ve" },
    NearbyStarData { name: "Ross 248", pos_ly: [7.3684, -0.5828, 7.1815], spectral_type: "M5.5Ve" },
    NearbyStarData { name: "Epsilon Eridani", pos_ly: [6.1847, 8.2771, -1.7213], spectral_type: "K2V" },
    NearbyStarData { name: "Lacaille 9352", pos_ly: [8.4508, -2.0341, -6.2812], spectral_type: "M0.5V" },
    NearbyStarData { name: "Ross 128", pos_ly: [-10.9906, 0.5885, 0.1545], spectral_type: "M4.0Vn" },
    NearbyStarData { name: "EZ Aquarii", pos_ly: [10.0458, -3.7282, -2.9312], spectral_type: "M5.0Ve" },
    NearbyStarData { name: "61 Cygni", pos_ly: [6.4753, -6.0967, 7.1379], spectral_type: "K5.0V" },
    NearbyStarData { name: "Procyon", pos_ly: [-4.7928, 10.3605, 1.0439], spectral_type: "F5IV-V" },
    NearbyStarData { name: "Struve 2398", pos_ly: [1.0781, -5.7086, 9.914], spectral_type: "M3.0V" },
    NearbyStarData { name: "Groombridge 34", pos_ly: [8.328, 0.6694, 8.0747], spectral_type: "M1.5V" },
    NearbyStarData { name: "DX Cancri", pos_ly: [-6.3414, 8.2773, 5.2619], spectral_type: "M6.5Ve" },
    NearbyStarData { name: "Epsilon Indi", pos_ly: [5.6765, -3.1673, -9.9283], spectral_type: "K5Ve" },
    NearbyStarData { name: "Tau Ceti", pos_ly: [10.2932, 5.0241, -3.2708], spectral_type: "G8.5V" },
    NearbyStarData { name: "GJ 1061", pos_ly: [5.0232, 6.9135, -8.4015], spectral_type: "M5.5V" },
    NearbyStarData { name: "YZ Ceti", pos_ly: [11.0172, 3.6068, -3.544], spectral_type: "M4.5V" },
    NearbyStarData { name: "Luyten's Star", pos_ly: [-4.5772, 11.4136, 1.1247], spectral_type: "M3.5V" },
    NearbyStarData { name: "Teegarden's Star", pos_ly: [8.7097, 8.1943, 3.629], spectral_type: "M6.5V" },
    NearbyStarData { name: "Kapteyn's Star", pos_ly: [1.8982, 8.869, -9.0756], spectral_type: "M1.5V" },
    NearbyStarData { name: "Lacaille 8760", pos_ly: [7.6441, -6.5718, -8.1246], spectral_type: "M0.0V" },
    NearbyStarData { name: "SCR 1845-6357", pos_ly: [1.1209, -5.6237, -11.738], spectral_type: "M8.5V" },
    NearbyStarData { name: "Kruger 60", pos_ly: [6.4306, -2.7299, 11.0491], spectral_type: "M3.0V" },
    NearbyStarData { name: "DENIS J1048-3956", pos_ly: [-9.6244, 3.1158, -8.469], spectral_type: "M8.5V" },
    NearbyStarData { name: "Ross 614", pos_ly: [-1.7069, 13.2373, -0.656], spectral_type: "M4.5V" },
    NearbyStarData { name: "UGPS J0722-0540", pos_ly: [-4.7051, 12.5085, -1.328], spectral_type: "T9" },
    NearbyStarData { name: "Wolf 1061", pos_ly: [-5.2293, -12.6717, -3.0799], spectral_type: "M3.0V" },
    NearbyStarData { name: "Van Maanen's Star", pos_ly: [13.6885, 2.9824, 1.3215], spectral_type: "DZ7" },
    NearbyStarData { name: "Gliese 1", pos_ly: [11.2638, 0.2658, -8.601], spectral_type: "M1.5V" },
    NearbyStarData { name: "TZ Arietis", pos_ly: [12.2919, 7.1125, 3.2923], spectral_type: "M4.5V" },
    NearbyStarData { name: "Wolf 424", pos_ly: [-14.2627, -2.0862, 2.2884], spectral_type: "M5.5V" },
    NearbyStarData { name: "Gliese 687", pos_ly: [-0.5623, -5.4485, 13.7916], spectral_type: "M3.0V" },
    NearbyStarData { name: "Gliese 674", pos_ly: [-1.383, -10.0523, -10.8415], spectral_type: "M3.0V" },
    NearbyStarData { name: "LHS 292", pos_ly: [-13.8709, 4.4929, -2.9233], spectral_type: "M6.5V" },
    NearbyStarData { name: "Gliese 440", pos_ly: [-6.4165, 0.4005, -13.688], spectral_type: "DQ6" },
    NearbyStarData { name: "GJ 1245", pos_ly: [5.1766, -9.5437, 10.6378], spectral_type: "M5.5V" },
    NearbyStarData { name: "WISE 1741+2553", pos_ly: [-1.1098, -13.6475, 6.6454], spectral_type: "T9" },
    NearbyStarData { name: "Gliese 876", pos_ly: [14.147, -4.239, -3.7544], spectral_type: "M3.5V" },
    NearbyStarData { name: "WISE 1639-6847", pos_ly: [-1.9044, -5.2097, -14.2977], spectral_type: "Y0.5" },
    NearbyStarData { name: "LHS 288", pos_ly: [-7.1797, 2.4598, -13.8107], spectral_type: "M5.5V" },
    NearbyStarData { name: "GJ 1002", pos_ly: [15.6626, 0.4601, -2.0739], spectral_type: "M5.5V" },
    NearbyStarData { name: "DENIS 0255-4700", pos_ly: [7.8177, 7.4878, -11.6144], spectral_type: "L7.5V" },
    NearbyStarData { name: "Groombridge 1618", pos_ly: [-9.1881, 4.7135, 12.0713], spectral_type: "K7.0V" },
    NearbyStarData { name: "Gliese 412", pos_ly: [-11.2719, 2.7334, 11.0169], spectral_type: "M1.0V" },
];

/// Spawn the starmap icon for the Sol system.
/// It starts hidden and becomes visible when `ViewMode::Starmap` is active.
fn setup_starmap(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut system_metadata: ResMut<SystemMetadata>,
) {
    // Initialize Sol's bounding radius
    system_metadata.set_bounding_radius(0, DEFAULT_BOUNDING_RADIUS_AU);
    
    // A bright glowing sphere representing the star system
    let icon_mesh = meshes.add(Sphere::new(1.0).mesh().uv(16, 8));
    
    // --- Sol System (ID: 0) ---
    let sol_material = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.95, 0.7),
        emissive: Color::srgb(6.0, 5.5, 3.5).into(), // Very bright for home system
        unlit: true,
        ..default()
    });

    // The icon is placed at the origin (same as the Sun) and scaled
    // dynamically based on camera distance.
    commands.spawn((
        PbrBundle {
            mesh: icon_mesh.clone(),
            material: sol_material,
            transform: Transform::from_translation(Vec3::ZERO),
            visibility: Visibility::Hidden, // starts hidden; shown in Starmap mode
            ..default()
        },
        StarSystemIcon {
            id: 0,
            name: "Sol System".to_string(),
            position: DVec3::ZERO,
            bounding_radius_au: DEFAULT_BOUNDING_RADIUS_AU,
        },
        SolSystemIcon,
    ));

    // --- Nearby Stars (ID: 1..50) ---
    for (i, star) in NEARBY_STARS.iter().enumerate() {
        let id = i + 1; // 0 is Sol

        // Determine color from spectral type
        let (r, g, b) = match star.spectral_type.chars().next().unwrap_or('G') {
            'O' => (0.6, 0.8, 1.0),      // Blue
            'B' => (0.7, 0.85, 1.0),     // Bluish White
            'A' => (0.9, 0.9, 1.0),      // White
            'F' => (1.0, 1.0, 0.9),      // Yellow-White
            'G' => (1.0, 0.95, 0.7),     // Yellow
            'K' => (1.0, 0.8, 0.6),      // Light Orange
            'M' => (1.0, 0.6, 0.4),      // Orange-Red
            'L' | 'T' | 'Y' => (0.8, 0.2, 0.2), // Brown/Dark Red
            _ => (1.0, 1.0, 1.0),        // Default White
        };

        let material = materials.add(StandardMaterial {
            base_color: Color::srgb(r, g, b),
            emissive: Color::srgb(r * 4.0, g * 4.0, b * 4.0).into(),
            unlit: true,
            ..default()
        });

        // Convert LY to AU
        let pos_au = DVec3::new(star.pos_ly[0], star.pos_ly[1], star.pos_ly[2]) * LY_TO_AU;
        
        // Initial transform assumes Origin is Sol (0,0,0)
        // Starmap Scale: 1 Unit = 1 AU.
        let spawn_pos = Vec3::new(pos_au.x as f32, pos_au.y as f32, pos_au.z as f32);

        // Estimate bounding radius for systems without detailed data
        // Most exoplanet systems discovered so far have planets within ~10 AU
        // Binary stars can extend much farther (hundreds to thousands of AU)
        // Use a conservative estimate for unknown systems
        let bounding_radius_au = FALLBACK_BOUNDING_RADIUS_AU;

        commands.spawn((
            PbrBundle {
                mesh: icon_mesh.clone(),
                material,
                transform: Transform::from_translation(spawn_pos),
                visibility: Visibility::Hidden,
                ..default()
            },
            StarSystemIcon {
                id,
                name: star.name.to_string(),
                position: pos_au,
                bounding_radius_au,
            },
        ));
    }
}


// ── Systems ─────────────────────────────────────────────────────────────────

/// Tag all celestial bodies spawned by solar_system.rs as belonging to System 0 (Sol).
/// We only tag the CelestialBody entity itself. Child entities (lights, clouds, etc)
/// may be added/removed asynchronously and inserting into them here can panic
/// if they are despawned before buffered commands are applied. Child entities'
/// ownership is inferred from their Parent during visibility logic.
fn tag_sol_bodies(
    mut commands: Commands,
    query: Query<Entity, (With<CelestialBody>, Without<SystemId>)>,
) {
    for entity in query.iter() {
        commands.entity(entity).insert(SystemId(0));
    }
}

const SECONDS_PER_DAY: f64 = 86400.0;

/// Spawns minimal celestial bodies (Star) for non-Sol systems when visited.
fn spawn_system_bodies(
    mut commands: Commands,
    current_system: Res<CurrentStarSystem>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    existing_bodies: Query<&SystemId, With<CelestialBody>>,
    nearby_stars: Res<NearbyStarsData>,
    mut system_metadata: ResMut<SystemMetadata>,
) {
    if !current_system.is_changed() { return; }
    
    let sys_id = current_system.0;
    if sys_id == 0 { return; } // Sol is handled by solar_system.rs

    // Check if bodies for this system already exist
    if existing_bodies.iter().any(|id| id.0 == sys_id) {
        return; 
    }

    // Determine star data index
    // SystemId is 1-based index into NEARBY_STARS + Sol (0)
    let star_idx = sys_id - 1;
    if star_idx >= NEARBY_STARS.len() { 
        warn!("System ID {} is out of range for nearby stars", sys_id);
        return; 
    }
    
    let star_data = &NEARBY_STARS[star_idx];
    let position_ly = Vec3::from_array([star_data.pos_ly[0] as f32, star_data.pos_ly[1] as f32, star_data.pos_ly[2] as f32]);
    let system_offset = DVec3::new(
        position_ly.x as f64 * LY_TO_AU, 
        position_ly.y as f64 * LY_TO_AU, 
        position_ly.z as f64 * LY_TO_AU
    );
    
    // Check if we have detailed data for this system
    if let Some(detailed_data) = nearby_stars.get_by_name(star_data.name) {
        info!("Spawning detailed system: {}", star_data.name);
        spawn_detailed_system(&mut commands, sys_id, system_offset, detailed_data, &mut meshes, &mut materials, &mut system_metadata);
        return;
    }

    // --- Fallback: Procedural fallback ---
    info!("Spawning fallback system: {}", star_data.name);
    spawn_fallback_system(&mut commands, sys_id, system_offset, star_data, &mut meshes, &mut materials, &mut system_metadata);
}

fn spawn_detailed_system(
    commands: &mut Commands,
    sys_id: usize,
    system_offset: DVec3,
    data: &crate::astronomy::nearby_stars::StarSystemData,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    system_metadata: &mut ResMut<SystemMetadata>,
) {
    let mut rng = rand::thread_rng();
    let seconds_per_year: f64 = 365.25 * SECONDS_PER_DAY;

    // Calculate bounding radius: maximum of planet orbits + binary star orbits
    let mut max_radius_au: f64 = 10.0; // Default minimum
    
    // Check planet orbits
    for star_data in &data.stars {
        for planet in &star_data.planets {
            let aphelion = (planet.semi_major_axis_au * (1.0 + planet.eccentricity)) as f64;
            max_radius_au = max_radius_au.max(aphelion);
        }
    }
    
    // Check binary star orbits
    for binary in &data.binary_orbits {
        let binary_extent = binary.semi_major_axis_au * (1.0 + binary.eccentricity);
        max_radius_au = max_radius_au.max(binary_extent);
    }
    
    // Add 50% margin for safety
    max_radius_au *= 1.5;
    
    info!("System {} bounding radius: {:.1} AU", data.system_name, max_radius_au);

    // --- Phase 1: Spawn a virtual barycenter entity for the system ---
    let barycenter = commands.spawn((
        TransformBundle::from_transform(Transform::IDENTITY),
        VisibilityBundle { visibility: Visibility::Hidden, ..default() },
        SpaceCoordinates { position: system_offset },
        SystemId(sys_id),
    )).id();

    // --- Phase 2: Spawn all stars ---
    let mut star_entities: Vec<Entity> = Vec::new();
    
    for (_star_idx, star_data) in data.stars.iter().enumerate() {
        let color = get_color_from_spectral_type(&star_data.spectral_type);
        let visual_radius = (696340.0 * star_data.radius_sol * STAR_RADIUS_SCALE).max(MIN_VISUAL_RADIUS);
        
        let star_entity = commands.spawn((
            PbrBundle {
                mesh: meshes.add(Sphere::new(visual_radius).mesh().uv(64, 32)),
                material: materials.add(StandardMaterial {
                    base_color: color,
                    emissive: LinearRgba::from(color).into(),
                    unlit: true,
                    ..default()
                }),
                transform: Transform::IDENTITY, // Will be set by propagate_orbits + update_render_transform
               ..default()
            },
            CelestialBody {
                name: star_data.name.clone(),
                radius: 696340.0 * star_data.radius_sol,
                mass: 1.989e30 * star_data.mass_sol as f64,
                body_type: BodyType::Star,
                visual_radius,
                asteroid_class: None,
            },
            SystemId(sys_id),
            Star,
            // Initial position at barycenter; will be updated if it has a binary orbit
            SpaceCoordinates { position: system_offset },
        )).with_children(|parent| {
            let intensity = 2.8e11 * star_data.luminosity_sol.sqrt();
            parent.spawn((
                PointLightBundle {
                    point_light: PointLight {
                        intensity,
                        range: 2.0e9,
                        shadows_enabled: false,
                        color,
                        ..default()
                    },
                    ..default()
                },
                SystemId(sys_id),
            ));
        }).id();
        
        star_entities.push(star_entity);
    }

    // --- Phase 3: Set up binary/multiple star orbits ---
    for binary in &data.binary_orbits {
        if binary.primary_idx >= star_entities.len() || binary.secondary_idx >= star_entities.len() {
            warn!("Binary orbit indices out of range for system {}", data.system_name);
            continue;
        }
        
        let primary_mass = data.stars[binary.primary_idx].mass_sol as f64;
        let secondary_mass = data.stars[binary.secondary_idx].mass_sol as f64;
        let total_mass = primary_mass + secondary_mass;
        
        // For a binary, both stars orbit the barycenter.
        // The secondary orbits at distance: a * M_primary / (M_primary + M_secondary)
        // The primary orbits at distance: a * M_secondary / (M_primary + M_secondary)
        let period_seconds = binary.period_years * seconds_per_year;
        let mean_motion = 2.0 * PI / period_seconds;
        let incl_rad = binary.inclination_deg.to_radians();
        let arg_peri_rad = binary.arg_periastron_deg.to_radians();
        let initial_anomaly: f64 = rng.gen_range(0.0..2.0*PI);
        
        // Secondary star orbit around barycenter
        let secondary_sma = binary.semi_major_axis_au * primary_mass / total_mass;
        let secondary_orbit = KeplerOrbit {
            eccentricity: binary.eccentricity,
            semi_major_axis: secondary_sma,
            inclination: incl_rad,
            longitude_ascending_node: rng.gen_range(0.0..2.0*PI),
            argument_of_periapsis: arg_peri_rad,
            mean_anomaly_epoch: initial_anomaly,
            mean_motion,
        };
        commands.entity(star_entities[binary.secondary_idx])
            .insert((secondary_orbit, OrbitCenter(barycenter)));
        
        // Primary star orbit around barycenter (opposite phase)
        let primary_sma = binary.semi_major_axis_au * secondary_mass / total_mass;
        let primary_orbit = KeplerOrbit {
            eccentricity: binary.eccentricity,
            semi_major_axis: primary_sma,
            inclination: incl_rad,
            longitude_ascending_node: rng.gen_range(0.0..2.0*PI),
            argument_of_periapsis: arg_peri_rad + PI, // Opposite side
            mean_anomaly_epoch: initial_anomaly,
            mean_motion,
        };
        commands.entity(star_entities[binary.primary_idx])
            .insert((primary_orbit, OrbitCenter(barycenter)));
    }
    
    // For single stars with no binary orbit defined, they stay at barycenter
    // (their SpaceCoordinates = system_offset, no KeplerOrbit)
    // If only 1 star, no binary orbits → star sits at center. Perfect.
    // If multiple stars but some have no binary_orbit entry (e.g. Proxima),
    // place them statically far out with an OrbitCenter:
    if data.stars.len() > 1 {
        let has_binary: Vec<bool> = (0..data.stars.len()).map(|i| {
            data.binary_orbits.iter().any(|b| b.primary_idx == i || b.secondary_idx == i)
        }).collect();
        
        for (i, &has_orbit) in has_binary.iter().enumerate() {
            if !has_orbit && i > 0 {
                // This is a wide companion (like Proxima) with no defined binary orbit
                // Place it at a static offset
                let offset = DVec3::new(100.0 * i as f64, 0.0, 50.0 * i as f64);
                commands.entity(star_entities[i]).insert(
                    SpaceCoordinates { position: system_offset + offset }
                );
            }
        }
    }
        
    // --- Phase 4: Spawn planets for each star ---
    for (star_idx, star_data) in data.stars.iter().enumerate() {
        let parent_star = star_entities[star_idx];
        
        for planet in &star_data.planets {
            let orbit = KeplerOrbit {
                eccentricity: planet.eccentricity as f64,
                semi_major_axis: planet.semi_major_axis_au as f64,
                inclination: rng.gen_range(0.0..0.15),
                longitude_ascending_node: rng.gen_range(0.0..2.0*PI),
                argument_of_periapsis: rng.gen_range(0.0..2.0*PI),
                mean_anomaly_epoch: rng.gen_range(0.0..2.0*PI),
                mean_motion: 2.0 * PI / (planet.period_days as f64 * SECONDS_PER_DAY),
            };
            
            let planet_radius_km = if let Some(r) = planet.radius_earth { 
                r * 6371.0 
            } else { 
                estimate_planet_radius_km(planet.mass_earth)
            };
            
            // Adjust visual radius for very close orbits (like Proxima b)
            // Prevent the planet from visually engulfing the star
            let orbit_dist_bu = planet.semi_major_axis_au as f32 * SCALING_FACTOR as f32;
            let max_visual_radius = orbit_dist_bu * 0.3; // Max 30% of orbit distance
            
            let nominal_visual_radius = (planet_radius_km * RADIUS_SCALE).max(MIN_VISUAL_RADIUS);
            let planet_visual_radius = nominal_visual_radius.min(max_visual_radius);

            let p_color = planet_type_to_color(&planet.planet_type);
            
            commands.spawn((
                PbrBundle {
                    mesh: meshes.add(Sphere::new(planet_visual_radius).mesh().uv(32, 16)),
                    material: materials.add(StandardMaterial {
                        base_color: p_color,
                        perceptual_roughness: 0.8,
                        reflectance: 0.1,
                        // Add slight emissive so it's visible against dark space
                        emissive: LinearRgba::from(p_color) * 0.05,
                        ..default()
                    }),
                    transform: Transform::IDENTITY,
                    ..default()
                },
                CelestialBody {
                    name: planet.name.clone(),
                    radius: planet_radius_km,
                    mass: 5.972e24 * planet.mass_earth as f64,
                    body_type: BodyType::Planet,
                    visual_radius: planet_visual_radius,
                    asteroid_class: None,
                },
                SystemId(sys_id),
                Planet,
                orbit,
                OrbitPath {
                   color: Color::srgba(0.4, 0.75, 1.0, 0.85),
                   visible: true,
                   segments: 128,
                },
                OrbitCenter(parent_star),
                // Initial position will be computed by propagate_orbits
                SpaceCoordinates { position: system_offset },
            ));
        }
    }
    
    // Store the calculated bounding radius in system metadata
    system_metadata.set_bounding_radius(sys_id, max_radius_au);
}

fn spawn_fallback_system(
    commands: &mut Commands,
    sys_id: usize,
    system_offset: DVec3,
    star_data: &NearbyStarData,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    system_metadata: &mut ResMut<SystemMetadata>,
) {
    let spectral = star_data.spectral_type;
    let color = get_color_from_spectral_type(spectral);
    let radius_mult = estimate_radius_from_spectral(spectral);
    let visual_radius = (696340.0 * radius_mult * STAR_RADIUS_SCALE).max(MIN_VISUAL_RADIUS);

    commands.spawn((
        PbrBundle {
            mesh: meshes.add(Sphere::new(visual_radius).mesh().uv(64, 32)),
            material: materials.add(StandardMaterial {
                base_color: color,
                emissive: LinearRgba::from(color).into(),
                unlit: true,
                ..default()
            }),
            transform: Transform::IDENTITY,
            ..default()
        },
        CelestialBody {
            name: star_data.name.to_string(),
            radius: 696340.0 * radius_mult,
            mass: 1.989e30 * radius_mult as f64, 
            body_type: BodyType::Star,
            visual_radius,
            asteroid_class: None,
        },
        SystemId(sys_id),
        Star,
        SpaceCoordinates { position: system_offset },
    )).with_children(|parent| {
        let intensity = 2.8e11 * radius_mult; 
        parent.spawn((
            PointLightBundle {
                point_light: PointLight {
                    intensity,
                    range: 2.0e9,
                    shadows_enabled: false,
                    color: color,
                    ..default()
                },
                ..default()
            },
            SystemId(sys_id),
        ));
    });
    
    // For fallback systems without detailed data, use default bounding radius
    system_metadata.set_bounding_radius(sys_id, FALLBACK_BOUNDING_RADIUS_AU);
}

fn get_color_from_spectral_type(spectral: &str) -> Color {
    if spectral.starts_with('O') { Color::srgb(0.6, 0.8, 1.0) }
    else if spectral.starts_with('B') { Color::srgb(0.7, 0.85, 1.0) }
    else if spectral.starts_with('A') { Color::WHITE }
    else if spectral.starts_with('F') { Color::srgb(1.0, 1.0, 0.9) }
    else if spectral.starts_with('G') { Color::srgb(1.0, 0.95, 0.8) }
    else if spectral.starts_with('K') { Color::srgb(1.0, 0.7, 0.4) }
    else if spectral.starts_with('M') { Color::srgb(1.0, 0.4, 0.2) }
    else if spectral.starts_with('L') || spectral.starts_with('T') || spectral.starts_with('Y') { 
        Color::srgb(0.8, 0.2, 0.1) 
    } else { 
        Color::WHITE 
    }
}

fn estimate_radius_from_spectral(spectral: &str) -> f32 {
    let spectral = spectral.trim(); // Just in case
    if spectral.starts_with('M') { 0.3 }
    else if spectral.starts_with('K') { 0.7 }
    else if spectral.starts_with('G') { 1.0 }
    else if spectral.starts_with('F') { 1.3 }
    else if spectral.starts_with('A') { 1.8 }
    else if spectral.starts_with('B') { 3.0 }
    else if spectral.starts_with('O') { 10.0 }
    else { 0.1 }
}

fn planet_type_to_color(ptype: &str) -> Color {
    let lower = ptype.to_lowercase();
    if lower.contains("gas") || lower.contains("jupiter") { Color::srgb(0.9, 0.8, 0.6) } 
    else if lower.contains("telluric") || lower.contains("terrestrial") || lower.contains("earth") { Color::srgb(0.2, 0.5, 0.8) }
    else if lower.contains("super-earth") || lower.contains("super_earth") { Color::srgb(0.4, 0.6, 0.7) }
    else if lower.contains("neptun") { Color::srgb(0.3, 0.3, 0.9) }
    else if lower.contains("sub-earth") || lower.contains("sub_earth") { Color::srgb(0.6, 0.55, 0.5) }
    else if lower.contains("rocky") { Color::srgb(0.5, 0.45, 0.4) }
    else if lower.contains("mars") { Color::srgb(0.8, 0.3, 0.1) } 
    else { Color::srgb(0.5, 0.5, 0.5) }
}

/// Estimate planet radius in km from mass in Earth masses
fn estimate_planet_radius_km(mass_earth: f32) -> f32 {
    if mass_earth > 100.0 {
        // Gas giant territory
        71492.0 * (mass_earth / 318.0).powf(0.06) // Jupiter-like, weak mass-radius
    } else if mass_earth > 10.0 {
        // Neptune-like
        24764.0 * (mass_earth / 17.0).powf(0.3)
    } else if mass_earth > 1.0 {
        // Super-Earth
        6371.0 * mass_earth.powf(0.28) // Rocky scaling
    } else {
        // Sub-Earth
        6371.0 * mass_earth.powf(0.33)
    }
}


/// Hide all celestial bodies and their orbit gizmos when in Starmap mode.
/// Also handles hiding bodies from other systems when in System mode.
fn toggle_system_view_entities(
    view_mode: Res<ViewMode>,
    current_system: Res<CurrentStarSystem>,
    mut body_query: Query<(&mut Visibility, Option<&SystemId>), With<CelestialBody>>,
    mut light_query: Query<(&mut Visibility, Option<&SystemId>, Option<&Parent>), (With<PointLight>, Without<CelestialBody>, Without<StarSystemIcon>)>,
    parent_sys_query: Query<&SystemId>,
    newly_spawned_bodies: Query<Entity, Added<CelestialBody>>,
) {
    // Run if view mode changed, current system changed, OR new bodies were spawned
    if !view_mode.is_changed() && !current_system.is_changed() && newly_spawned_bodies.is_empty() {
        return;
    }

    match *view_mode {
        ViewMode::System => {
            // Show bodies only for the current system
            for (mut vis, sys_id) in body_query.iter_mut() {
                let id = sys_id.map(|s| s.0).unwrap_or(0); // Default to Sol if untagged
                if id == current_system.0 {
                    *vis = Visibility::Inherited;
                } else {
                    *vis = Visibility::Hidden;
                }
            }
            // Update lights and other child entities by checking their own SystemId,
            // falling back to their Parent's SystemId if available. This avoids
            // inserting components into children (which can panic if they are
            // despawned before command application).
            for (mut vis, sys_id, parent) in light_query.iter_mut() {
                let id = if let Some(s) = sys_id {
                    s.0
                } else if let Some(parent) = parent {
                    parent_sys_query.get(parent.get()).map(|s| s.0).unwrap_or(0)
                } else {
                    0
                };

                if id == current_system.0 {
                    *vis = Visibility::Inherited;
                } else {
                    *vis = Visibility::Hidden;
                }
            }
        },
        ViewMode::Starmap => {
            // Hide everything in Starmap mode (except Icons)
            for (mut vis, _) in body_query.iter_mut() {
                *vis = Visibility::Hidden;
            }
            for (mut vis, _, _) in light_query.iter_mut() {
                *vis = Visibility::Hidden;
            }
        }
    }
}

/// Update starmap icon positions relative to the floating origin
fn update_starmap_coordinates(
    floating_origin: Res<FloatingOrigin>,
    mut query: Query<(&mut Transform, &StarSystemIcon)>,
) {
    if !floating_origin.is_changed() {
         // Optimization: usually only update if origin changes, 
         // BUT finding if new icons spawned is hard. 
         // For 50 items, running every frame is cheap.
    }
    
    let origin = floating_origin.position;
    
    // Starmap scale: We render icons at 1 Unit = 1 AU relative to origin.
    // This makes the starmap "miniature" compared to the System View (1500 Units = 1 AU).
    // This allows the camera to see the starmap within reasonable Z-range.
    
    for (mut transform, icon) in query.iter_mut() {
        // Calculate position in AU relative to origin
        let relative_au = icon.position - origin;
        
        // Map to Bevy units: 1 AU = 1.0 Unit (Starmap Scale)
        transform.translation = Vec3::new(
            relative_au.x as f32,
            relative_au.y as f32,
            relative_au.z as f32,
        );
    }
}

/// Show/hide starmap icons based on current `ViewMode`.
fn update_starmap_visibility(
    view_mode: Res<ViewMode>,
    current_system: Res<CurrentStarSystem>,
    mut icon_query: Query<(&mut Visibility, &StarSystemIcon)>,
) {
    if !view_mode.is_changed() && !current_system.is_changed() {
        return;
    }

    match *view_mode {
        ViewMode::System => {
            for (mut vis, icon) in icon_query.iter_mut() {
                // For Sol (0), we have a real model, so hide the icon.
                // For others, show the icon as a placeholder star until we implement real loading.
                if icon.id == current_system.0 && icon.id != 0 {
                    *vis = Visibility::Inherited;
                } else {
                    *vis = Visibility::Hidden;
                }
            }
        },
        ViewMode::Starmap => {
            for (mut vis, _) in icon_query.iter_mut() {
                *vis = Visibility::Inherited;
            }
        }
    };
}

/// Scale the starmap icon so it remains a comfortable visual size regardless of
/// how far the camera is zoomed out.
fn update_starmap_icon_scale(
    view_mode: Res<ViewMode>,
    current_system: Res<CurrentStarSystem>,
    camera_query: Query<&OrbitCamera, With<GameCamera>>,
    mut icon_query: Query<(&mut Transform, &StarSystemIcon)>,
) {
    let Ok(orbit) = camera_query.get_single() else {
        return;
    };

    // Calculate desired radius
    let icon_radius = (orbit.radius * 0.012).max(50.0);
    let scale = Vec3::splat(icon_radius);

    match *view_mode {
        ViewMode::Starmap => {
             for (mut transform, _) in icon_query.iter_mut() {
                transform.scale = scale;
            }
        },
        ViewMode::System => {
            // Only update the active system icon so it looks good as a placeholder
            // But skip Sol, as it's hidden anyway
            if current_system.0 != 0 {
                for (mut transform, icon) in icon_query.iter_mut() {
                    if icon.id == current_system.0 {
                        transform.scale = scale;
                    }
                }
            }
        }
    }
}

#[derive(Default)]
struct StarmapSelectionState {
    last_click_time: f64,
    last_clicked_entity: Option<Entity>,
}

/// Handle double-click selection of star system icons in starmap view.
/// Double-clicking anchors the camera to the system's position.
fn handle_starmap_selection(
    view_mode: Res<ViewMode>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    icon_query: Query<(Entity, &GlobalTransform, &StarSystemIcon)>,
    mut commands: Commands,
    selected_query: Query<Entity, With<SelectedStarSystem>>,
    mut anchor_query: Query<&mut CameraAnchor, With<GameCamera>>,
    time: Res<Time>,
    mut selection_state: Local<StarmapSelectionState>,
    mut egui_contexts: bevy_egui::EguiContexts,
) {
    // Only active in starmap view
    if *view_mode != ViewMode::Starmap {
        return;
    }

    // Only process on mouse click
    if !mouse_button.just_pressed(MouseButton::Left) {
        return;
    }

    // Don't process if egui is using the mouse
    let ctx = egui_contexts.ctx_mut();
    if ctx.is_pointer_over_area() || ctx.wants_pointer_input() || ctx.is_using_pointer() {
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

    // Find the closest star system icon to the ray
    let mut closest_icon: Option<(Entity, f32, String)> = None;
    
    for (entity, transform, icon) in icon_query.iter() {
        let icon_pos = transform.translation();
        
        // Calculate distance from ray to icon center
        let to_icon = icon_pos - ray.origin;
        let projection = to_icon.dot(*ray.direction);
        
        if projection < 0.0 {
            continue; // Icon is behind camera
        }
        
        let closest_point = ray.origin + *ray.direction * projection;
        let distance_to_ray = (icon_pos - closest_point).length();
        
        // Icon scale determines its clickable radius
        let icon_scale = transform.compute_transform().scale.x;
        let click_radius = icon_scale * 1.5; // 50% larger for easier clicking
        
        if distance_to_ray < click_radius {
            let distance_from_camera = (icon_pos - ray.origin).length();
            
            if closest_icon.is_none() || distance_from_camera < closest_icon.as_ref().unwrap().1 {
                closest_icon = Some((entity, distance_from_camera, icon.name.clone()));
            }
        }
    }
    
    // If we found an icon, check for double-click
    if let Some((entity, _, name)) = closest_icon {
        let current_time = time.elapsed_seconds_f64();
        let is_double_click = selection_state.last_clicked_entity == Some(entity)
            && (current_time - selection_state.last_click_time) < 0.3; // 300ms window
        
        selection_state.last_clicked_entity = Some(entity);
        selection_state.last_click_time = current_time;
        
        if is_double_click {
            info!("Double-clicked star system: {}", name);
            
            // Clear previous selection
            for selected_entity in selected_query.iter() {
                commands.entity(selected_entity).remove::<SelectedStarSystem>();
            }
            
            // Mark this system as selected/anchored
            commands.entity(entity).insert(SelectedStarSystem);
            
            // Anchor camera to this system icon's position
            // Note: We anchor to the entity itself so the camera follows it
            if let Ok(mut anchor) = anchor_query.get_single_mut() {
                anchor.0 = Some(entity);
                info!("Camera anchored to {}", name);
            }
        }
    }
}

/// Handle transition from Starmap to System view.
/// This updates the floating origin and current system if we were anchored to a star.
fn handle_system_transition(
    view_mode: Res<ViewMode>,
    mut current_system: ResMut<CurrentStarSystem>,
    mut floating_origin: ResMut<FloatingOrigin>,
    mut anchor_query: Query<&mut CameraAnchor, With<GameCamera>>,
    icon_query: Query<&StarSystemIcon>,
    selected_query: Query<Entity, With<SelectedStarSystem>>,
    mut commands: Commands,
) {
    if !view_mode.is_changed() || *view_mode != ViewMode::System {
        return;
    }

    // Identify which star we are anchored to
    if let Ok(mut anchor) = anchor_query.get_single_mut() {
        if let Some(anchored_entity) = anchor.0 {
            // Check if the anchored entity is a star system icon
            if let Ok(icon) = icon_query.get(anchored_entity) {
                // We are zooming into this system!
                
                // Update Current System
                current_system.0 = icon.id;
                
                // Update Floating Origin to center on this star
                floating_origin.position = icon.position;
                
                info!("Transitioned to system: {} (Origin: {:?})", icon.name, floating_origin.position);

                // Clear the anchor so the camera is free to move in the new system
                // But wait! If we clear the anchor, the camera target_center stays where it was (at the icon).
                // Since the Floating Origin shifted, the Icon moved to (0,0,0).
                // So target_center should be (0,0,0).
                // And OrbitCamera will naturally look at (0,0,0).
                // So this is correct.
                anchor.0 = None;
            }
        }
    }

    // Clear all starmap selections (visual rings etc)
    for entity in selected_query.iter() {
        commands.entity(entity).remove::<SelectedStarSystem>();
    }
}
