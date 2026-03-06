//! Procedural generation system for star systems
//!
//! This module implements the "gap-filler" logic to populate star systems with
//! procedurally generated planets, asteroid belts, and cometary clouds when
//! real data is incomplete or unavailable.

use bevy::prelude::*;
use rand::prelude::*;
use std::f64::consts::PI;

use super::components::KeplerOrbit;
use crate::plugins::solar_system_data::BodyType;

// ============================================================================
// PHYSICAL CONSTANTS FOR PROCEDURAL GENERATION
// ============================================================================

/// Solar mass in kg
const SOLAR_MASS_KG: f64 = 1.989e30;
/// Earth mass in kg
const EARTH_MASS_KG: f64 = 5.972e24;
/// Earth radius in km
const EARTH_RADIUS_KM: f64 = 6371.0;
/// Gravitational constant in m³/(kg·s²)
const G: f64 = 6.674e-11;
/// Astronomical unit in meters
const AU_M: f64 = 1.496e11;
/// Stefan-Boltzmann constant
const STEFAN_BOLTZMANN: f64 = 5.67e-8;

/// Calculate escape velocity from a body
/// v_esc = sqrt(2GM/R)
pub fn calculate_escape_velocity(mass_kg: f64, radius_m: f64) -> f64 {
    (2.0 * G * mass_kg / radius_m).sqrt()
}

/// Calculate instellation (stellar flux) at a given distance
/// I = L / d² (in Earth-equivalent units, where L_sun = 1, 1 AU = 1)
pub fn calculate_instellation(star_luminosity_sol: f64, distance_au: f64) -> f64 {
    star_luminosity_sol / distance_au.powf(2.0)
}

/// Calculate equilibrium temperature for a planet
/// T_eq = (L * (1 - albedo) / (16 * pi * d² * sigma))^(1/4)
/// Simplified: T_eq = 278.3 * L^(1/4) / d^(1/2)
pub fn calculate_equilibrium_temperature(star_luminosity_sol: f64, distance_au: f64, albedo: f32) -> f64 {
    let t_eq_no_albedo = 278.3 * star_luminosity_sol.powf(0.25) / distance_au.sqrt();
    // Apply albedo correction
    t_eq_no_albedo * (1.0 - albedo as f64).powf(0.25)
}

/// Calculate Hill sphere radius for a body orbiting a larger mass
/// R_H = a * (m / 3M)^(1/3)
/// Returns Hill sphere in AU
pub fn calculate_hill_sphere(planet_mass_kg: f64, star_mass_kg: f64, semi_major_axis_au: f64) -> f64 {
    let mass_ratio = planet_mass_kg / (3.0 * star_mass_kg);
    semi_major_axis_au * mass_ratio.cbrt()
}

/// Check if a planet can retain its atmosphere based on the Cosmic Shoreline
/// A planet retains atmosphere if v_esc^4 > K * I, where K is a baseline constant
/// For Earth-like retention, K ≈ 1.0
pub fn can_retain_atmosphere_cosmic_shoreline(
    escape_velocity_mps: f64,
    instellation: f64,
    baseline_k: f64,
) -> bool {
    escape_velocity_mps.powf(4.0) > baseline_k * instellation
}

/// Determine if a planet is in the habitable zone
pub fn is_in_habitable_zone(distance_au: f64, star_luminosity_sol: f64) -> bool {
    let (hz_inner, hz_outer) = calculate_habitable_zone(star_luminosity_sol);
    distance_au >= hz_inner && distance_au <= hz_outer
}

/// Calculate the stellar Hill sphere (approximate galactic bounds)
/// For a Sun-like star in typical galactic neighborhood, this is ~200,000 AU
pub fn calculate_stellar_hill_sphere(star_mass_sol: f64) -> f64 {
    // Approximate: stellar Hill sphere in typical galactic environment
    // M_star / M_galaxy ~ 10^-10, R_galaxy ~ 10^5 ly ~ 10^8 AU
    // R_H,star ~ 10^5 * (10^-10/3)^(1/3) ~ 2 × 10^5 AU
    200_000.0 * star_mass_sol.sqrt()
}

/// System architecture parameters for a star system
/// Defines the structure of rocky planets, gas giants, belts, and clouds
#[derive(Debug, Clone)]
pub struct SystemArchitecture {
    /// Distance of the frost line in Astronomical Units
    pub frost_line_au: f64,

    /// Inner system rocky planets (inside frost line)
    pub rocky_planets: Vec<ProceduralPlanet>,

    /// Asteroid belt (collection of entities with M, S, and V type resources)
    pub asteroid_belt: Option<AsteroidBelt>,

    /// Outer system gas/ice giants (outside frost line)
    pub gas_giants: Vec<ProceduralPlanet>,

    /// Cometary cloud (P and D type bodies high in Volatiles)
    pub cometary_cloud: Option<CometaryCloud>,

    /// Dwarf planets in the outer system (Pluto-like trans-Neptunian objects)
    pub dwarf_planets: Vec<ProceduralPlanet>,
}

/// Procedurally generated planet parameters
#[derive(Debug, Clone)]
pub struct ProceduralPlanet {
    /// Name of the planet (e.g., "Proxima b")
    pub name: String,

    /// Semi-major axis in AU
    pub semi_major_axis_au: f64,

    /// Orbital eccentricity (0-1)
    pub eccentricity: f64,

    /// Orbital inclination in radians
    pub inclination: f64,

    /// Longitude of ascending node in radians
    pub longitude_ascending_node: f64,

    /// Argument of periapsis in radians
    pub argument_of_periapsis: f64,

    /// Mean anomaly at epoch in radians
    pub mean_anomaly_epoch: f64,

    /// Orbital period in days
    pub period_days: f64,

    /// Mass in Earth masses
    pub mass_earth: f32,

    /// Radius in Earth radii
    pub radius_earth: f32,

    /// Planet type
    pub planet_type: PlanetType,

    /// Rotation period in Earth days (positive = prograde)
    pub rotation_period_days: f32,

    /// Axial tilt in degrees (obliquity)
    pub axial_tilt_deg: f32,
}

/// System architecture types - defines the overall structure of a star system
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemType {
    /// Standard system like our Solar System (inner rocky, outer gas giants)
    Standard,
    /// Compact system with many planets crammed close to the star (e.g., TRAPPIST-1)
    Compact,
    /// Jovian-heavy system with hot Jupiters inside the habitable zone
    JovianHeavy,
    /// Sparse system with few planets spread out
    Sparse,
}

impl SystemType {
    /// Determine system type based on star properties and random chance
    pub fn determine(_star_mass_solar: f64, luminosity_solar: f64, rng: &mut impl Rng) -> Self {
        // Red dwarf stars (M-type) often have compact systems
        let is_red_dwarf = luminosity_solar < 0.1;

        let roll = rng.random_range(0.0..1.0_f64);

        if is_red_dwarf {
            // M-dwarfs: 60% compact, 20% standard, 15% sparse, 5% jovian-heavy
            if roll < 0.60 {
                SystemType::Compact
            } else if roll < 0.80 {
                SystemType::Standard
            } else if roll < 0.95 {
                SystemType::Sparse
            } else {
                SystemType::JovianHeavy
            }
        } else if luminosity_solar < 0.5 {
            // K-type stars: 40% standard, 30% compact, 20% sparse, 10% jovian-heavy
            if roll < 0.40 {
                SystemType::Standard
            } else if roll < 0.70 {
                SystemType::Compact
            } else if roll < 0.90 {
                SystemType::Sparse
            } else {
                SystemType::JovianHeavy
            }
        } else {
            // G-type and brighter: 60% standard, 20% sparse, 15% compact, 5% jovian-heavy
            if roll < 0.60 {
                SystemType::Standard
            } else if roll < 0.80 {
                SystemType::Sparse
            } else if roll < 0.95 {
                SystemType::Compact
            } else {
                SystemType::JovianHeavy
            }
        }
    }

    /// Returns the typical number of orbital slots for this system type
    pub fn typical_slots(&self) -> (usize, usize) {
        match self {
            SystemType::Standard => (4, 6),   // 4-6 planets
            SystemType::Compact => (5, 8),   // 5-8 planets (crammed close, TRAPPIST-1 has 7)
            SystemType::JovianHeavy => (3, 7), // 3-7 planets with gas giants
            SystemType::Sparse => (2, 4),     // 2-4 planets
        }
    }
}

/// Type of procedurally generated planet
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanetType {
    Rocky,        // Standard terrestrial planet
    SuperEarth,  // 1.5-10 Earth masses, rocky
    MiniNeptune, // Gaseous but small (10-20 Earth masses)
    DesertWorld, // Hot, rocky, no water
    LavaWorld,   // Very hot (>1000K), thin silicate atmosphere
    IceGiant,    // Outer system, ice-rich
    GasGiant,    // Outer system, gas-rich
    WaterWorld,  // Terrestrial with deep ocean (>50% water by mass)
}

/// Asteroid belt configuration
#[derive(Debug, Clone)]
pub struct AsteroidBelt {
    /// Inner edge of the belt in AU
    pub inner_au: f64,

    /// Outer edge of the belt in AU
    pub outer_au: f64,

    /// Number of asteroids to spawn
    pub count: usize,

    /// Average inclination of the belt in radians
    pub inclination: f64,
}

/// Cometary cloud configuration
#[derive(Debug, Clone)]
pub struct CometaryCloud {
    /// Inner edge of the cloud in AU
    pub inner_au: f64,

    /// Outer edge of the cloud in AU
    pub outer_au: f64,

    /// Number of comets to spawn
    pub count: usize,

    /// Average inclination of the cloud in radians (highly inclined)
    pub inclination: f64,
}

/// Calculate the frost line based on stellar luminosity
/// Uses the formula: d_frost ≈ 4.85 × √(L/L_sun) AU
///
/// This is based on the equilibrium temperature for water ice sublimation (~170K)
/// at the distance where the stellar flux equals the threshold value.
///
/// # Arguments
/// * `luminosity_solar` - Luminosity of the star in solar luminosities (L☉)
///
/// # Returns
/// Frost line distance in Astronomical Units
pub fn calculate_frost_line(luminosity_solar: f64) -> f64 {
    4.85 * luminosity_solar.sqrt()
}

/// Calculate the habitable zone boundaries based on stellar luminosity
/// Uses simplified estimates: HZ ≈ √(L) AU for conservative inner edge
/// The "goldilocks" zone scales with the square root of luminosity
///
/// # Arguments
/// * `luminosity_solar` - Luminosity of the star in solar luminosities (L☉)
///
/// # Returns
/// (inner_edge_au, outer_edge_au) for the habitable zone
pub fn calculate_habitable_zone(luminosity_solar: f64) -> (f64, f64) {
    // Conservative estimates based on stellar irradiation
    // Inner edge: where water begins to evaporate (~340K equilibrium for runaway greenhouse)
    // Outer edge: where CO2 begins to condense (~170K equilibrium for maximum greenhouse)
    let inner_au = 0.75 * luminosity_solar.sqrt();
    let outer_au = 1.77 * luminosity_solar.sqrt();

    (inner_au, outer_au)
}

/// Generate orbital slots using a Titius-Bode-like log-spaced distribution
/// This creates more realistic planetary spacing than uniform random distribution
///
/// # Arguments
/// * `min_au` - Minimum orbital distance
/// * `max_au` - Maximum orbital distance
/// * `num_slots` - Target number of orbital slots
/// * `spacing_factor` - Controls spacing tightness (1.3 = loose, 2.0 = very spread)
/// * `existing_orbits_au` - Existing orbits to avoid
/// * `rng` - Random number generator
///
/// # Returns
/// Vector of semi-major axes in AU using Titius-Bode style geometric progression
/// with Hill Sphere stability checks
fn generate_log_spaced_orbits(
    min_au: f64,
    max_au: f64,
    num_slots: usize,
    spacing_factor: f64,
    existing_orbits_au: &[f64],
    rng: &mut impl Rng,
    star_mass_solar: f64,
    system_type: SystemType,
) -> Vec<f64> {
    let mut orbits = Vec::new();
    let mut all_orbits = existing_orbits_au.to_vec();

    // Star mass in kg for Hill sphere calculations
    let star_mass_kg = star_mass_solar * SOLAR_MASS_KG;

    // Use Titius-Bode style geometric progression: a_n = a_0 * k^n
    // where k is the spacing factor (1.4 - 2.0)
    let mut cursor = min_au;

    for _ in 0..num_slots {
        if cursor > max_au {
            break;
        }

        // Add small random jitter (±10% for more stable resonant systems)
        let jitter = rng.random_range(-0.10..0.10);
        let mut semi_major_axis = (cursor * (1.0 + jitter)).clamp(min_au, max_au);

        // ========================================================================
        // HILL SPHERE STABILITY CHECK
        // Ensure each planet is separated by at least 10 * R_H of the larger neighbor
        // Walk outward in small increments until the orbit is stable.
        // ========================================================================
        let mut attempts = 0;
        while attempts < 20 && !is_hill_stable(semi_major_axis, star_mass_kg, &all_orbits, 10.0) {
            semi_major_axis *= 1.12; // nudge outward by 12%
            attempts += 1;
        }

        if semi_major_axis > max_au || semi_major_axis < min_au {
            break;
        }

        all_orbits.push(semi_major_axis);
        orbits.push(semi_major_axis);

        // ========================================================================
        // TITIUS-BODE PROGRESSION
        // Advance cursor using geometric progression with random variation
        // ========================================================================
        cursor = semi_major_axis * rng.random_range(spacing_factor..(spacing_factor + 0.3));
    }

    // ========================================================================
    // GRAVITATIONAL CLEARING FOR JOVIAN HEAVY SYSTEMS
    // If JovianHeavy, remove any small planets within 5 Hill radii of gas giants
    // ========================================================================
    if system_type == SystemType::JovianHeavy {
        apply_jovian_clearing(&mut orbits, star_mass_kg, 5.0);
    }

    orbits
}

/// Check whether the proposed orbit satisfies Hill sphere stability against all
/// existing orbits.  Returns `true` if the orbit is stable (far enough from
/// all neighbours), `false` otherwise.
fn is_hill_stable(
    proposed_au: f64,
    star_mass_kg: f64,
    existing_orbits: &[f64],
    hill_multiplier: f64,
) -> bool {
    // Assume a representative planet mass for stability check
    let planet_mass_kg = 10.0 * EARTH_MASS_KG;

    for &existing_au in existing_orbits {
        let hill_radius_au = calculate_hill_sphere(planet_mass_kg, star_mass_kg, existing_au);
        let required_sep = hill_multiplier * hill_radius_au;
        let distance = (proposed_au - existing_au).abs();

        if distance < required_sep {
            return false;
        }
    }
    true
}

/// Apply gravitational clearing: remove small planets within Hill sphere of gas giants
fn apply_jovian_clearing(orbits: &mut Vec<f64>, star_mass_kg: f64, hill_multiplier: f64) {
    // For simplicity, we just note that in a full implementation,
    // any orbit that falls within 5 Hill radii of another gas giant would be cleared
    // In practice, this is handled during planet type determination
    // where JovianHeavy systems spawn gas giants that dominate their orbital zones

    // Sort orbits to check neighbors
    orbits.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let mut to_remove: Vec<usize> = Vec::new();

    for (i, &orbit) in orbits.iter().enumerate() {
        // Calculate this orbit's Hill sphere (assuming gas giant mass)
        let planet_mass_kg = 100.0 * EARTH_MASS_KG; // Jupiter-like
        let hill_radius_au = calculate_hill_sphere(planet_mass_kg, star_mass_kg, orbit);
        let clearing_zone = hill_multiplier * hill_radius_au;

        // Check if any smaller orbit falls within the clearing zone
        for (j, &inner_orbit) in orbits.iter().enumerate() {
            if j >= i {
                break;
            }
            if (orbit - inner_orbit) < clearing_zone {
                // Mark inner planet for removal
                if !to_remove.contains(&j) {
                    to_remove.push(j);
                }
            }
        }
    }

    // Remove cleared orbits (in reverse order to maintain indices)
    to_remove.sort();
    to_remove.reverse();
    for idx in to_remove {
        if idx < orbits.len() {
            orbits.remove(idx);
        }
    }
}

/// Map a star to a system architecture based on its properties
/// This is the main entry point for procedural system generation
///
/// # Arguments
/// * `star_name` - Name of the star (for naming generated bodies)
/// * `star_mass_solar` - Mass of the star in solar masses (for system type determination)
/// * `luminosity_solar` - Luminosity in solar units
/// * `existing_planet_count` - Number of confirmed planets already in the system
/// * `existing_orbits_au` - Semi-major axes of existing planets (to avoid collisions)
/// * `rng` - Random number generator for variability
///
/// # Returns
/// SystemArchitecture containing all procedurally generated bodies
pub fn map_star_to_system_architecture(
    star_name: &str,
    star_mass_solar: f64,
    luminosity_solar: f64,
    existing_planet_count: usize,
    existing_orbits_au: &[f64],
    rng: &mut impl Rng,
) -> SystemArchitecture {
    // Calculate frost line and habitable zone
    let frost_line_au = calculate_frost_line(luminosity_solar);
    let (hz_inner, hz_outer) = calculate_habitable_zone(luminosity_solar);

    // ========================================================================
    // DYNAMIC GALAXY LIMITS: Stellar Hill Sphere
    // Use the Hill sphere of the star to set absolute edge of procedural systems
    // For a Sun-like star in typical galactic neighborhood: ~200,000 AU
    // Anything beyond that is "Interstellar Space"
    // ========================================================================
    let stellar_hill_sphere = calculate_stellar_hill_sphere(star_mass_solar);

    // Filter existing orbits to only those within stellar Hill sphere
    let valid_existing_orbits: Vec<f64> = existing_orbits_au
        .iter()
        .filter(|&&o| o < stellar_hill_sphere)
        .cloned()
        .collect();

    // If all existing orbits are outside the Hill sphere, return empty system
    if valid_existing_orbits.len() != existing_orbits_au.len() {
        debug!(
            "Warning: {} existing orbits are outside stellar Hill sphere ({:.0} AU)",
            existing_orbits_au.len() - valid_existing_orbits.len(),
            stellar_hill_sphere
        );
    }

    // Determine system architecture type based on star properties
    let system_type = SystemType::determine(star_mass_solar, luminosity_solar, rng);

    debug!(
        "Generating system architecture for {} ({:?}, L={:.3}L☉, frost line={:.2}AU, HZ={:.2}-{:.2}AU)",
        star_name, system_type, luminosity_solar, frost_line_au, hz_inner, hz_outer
    );

    // Determine number of orbital slots based on system type
    let (_min_slots, max_slots) = system_type.typical_slots();
    let total_slots = if existing_planet_count < max_slots {
        max_slots.max(existing_planet_count + 1)
    } else {
        existing_planet_count
    };
    let slots_to_generate = total_slots.saturating_sub(existing_planet_count);

    let mut all_planets = Vec::new();

    // Generate planets if we need more
    if slots_to_generate > 0 {
        // Generate orbital slots using log-spaced distribution
        let (min_orbit, max_orbit) = match system_type {
            SystemType::Compact => (0.02, (frost_line_au * 1.5).max(0.5)),
            SystemType::Standard => (0.08, 40.0),
            SystemType::JovianHeavy => (0.03, (frost_line_au * 3.0).max(5.0)),
            SystemType::Sparse => (0.1, 50.0),
        };

        // Spacing factor: compact systems need tighter spacing
        let spacing_factor = match system_type {
            SystemType::Compact => 1.2,
            SystemType::Standard => 1.35,
            SystemType::JovianHeavy => 1.25,
            SystemType::Sparse => 1.5,
        };

        // Clamp max_orbit to stellar Hill sphere (system boundary)
        let system_boundary = stellar_hill_sphere.min(max_orbit);

        let orbital_slots = generate_log_spaced_orbits(
            min_orbit,
            system_boundary,
            slots_to_generate,
            spacing_factor,
            &valid_existing_orbits,
            rng,
            star_mass_solar,
            system_type,
        );

        // Decide once per system whether a hot Jupiter migrated inward.
        // Real occurrence rate is ~1% of FGK stars; slightly higher for more
        // massive stars and JovianHeavy architectures.
        let has_migrated_giant = match system_type {
            SystemType::JovianHeavy => rng.random_bool(0.15),
            _ => rng.random_bool(0.02),
        };
        let mut migrated_giant_placed = false;

        // For each orbital slot, determine planet type
        for (i, &semi_major_axis) in orbital_slots.iter().enumerate() {
            let planet = generate_planet_for_slot(
                star_name,
                i,
                semi_major_axis,
                frost_line_au,
                hz_inner,
                hz_outer,
                system_type,
                star_mass_solar,
                luminosity_solar,
                has_migrated_giant && !migrated_giant_placed,
                rng,
            );
            if matches!(planet.planet_type, PlanetType::GasGiant | PlanetType::MiniNeptune)
                && planet.semi_major_axis_au < frost_line_au
            {
                migrated_giant_placed = true;
            }
            all_planets.push(planet);
        }
    }

    // Separate planets into rocky and gas giants based on type
    let rocky_planets: Vec<_> = all_planets
        .iter()
        .filter(|p| {
            matches!(
                p.planet_type,
                PlanetType::Rocky
                    | PlanetType::SuperEarth
                    | PlanetType::DesertWorld
                    | PlanetType::LavaWorld
                    | PlanetType::WaterWorld
            )
        })
        .cloned()
        .collect();

    let gas_giants: Vec<_> = all_planets
        .iter()
        .filter(|p| {
            matches!(
                p.planet_type,
                PlanetType::MiniNeptune | PlanetType::IceGiant | PlanetType::GasGiant
            )
        })
        .cloned()
        .collect();

    // ========================================================================
    // EFFECTIVE FROST LINE FOR MINOR BODY PLACEMENT
    // When planets are packed much closer than the frost line suggests
    // (e.g. compact systems around luminous stars), scale minor body
    // distances down so asteroid belts, cometary clouds, and dwarf planets
    // form a cohesive system rather than orbiting at implausible distances.
    // ========================================================================
    let mut all_orbits_for_belt = valid_existing_orbits.to_vec();
    all_orbits_for_belt.extend(all_planets.iter().map(|p| p.semi_major_axis_au));
    let outermost_planet_au = all_orbits_for_belt
        .iter()
        .copied()
        .fold(0.0_f64, f64::max);

    // If the outermost planet is well inside the frost line, use a scaled
    // "effective frost line" so that minor bodies form just beyond the
    // planetary zone rather than at unrealistically distant orbits.
    let effective_frost_line = if outermost_planet_au > 0.0 && outermost_planet_au < frost_line_au * 0.5 {
        // Belt should start just beyond the outermost planet
        (outermost_planet_au * 2.5).min(frost_line_au)
    } else {
        frost_line_au
    };

    // Generate asteroid belt (inside or near frost line)
    let asteroid_belt = if rng.random_bool(0.8) {
        Some(generate_asteroid_belt(effective_frost_line, &all_orbits_for_belt, rng))
    } else {
        None
    };

    // Generate cometary cloud (far outer system, but within stellar Hill sphere)
    let cometary_cloud = if rng.random_bool(0.7) {
        Some(generate_cometary_cloud(effective_frost_line, rng))
    } else {
        None
    };

    // Generate dwarf planets in the trans-Neptunian region
    let dwarf_planets = if rng.random_bool(0.75) {
        let name_offset = valid_existing_orbits.len() + all_planets.len();
        generate_dwarf_planets(star_name, effective_frost_line, &all_orbits_for_belt, name_offset, star_mass_solar, rng)
    } else {
        Vec::new()
    };

    SystemArchitecture {
        frost_line_au,
        rocky_planets,
        asteroid_belt,
        gas_giants,
        cometary_cloud,
        dwarf_planets,
    }
}

/// Generate a planet for a specific orbital slot based on distance and system type
fn generate_planet_for_slot(
    star_name: &str,
    slot_index: usize,
    semi_major_axis_au: f64,
    frost_line_au: f64,
    hz_inner: f64,
    hz_outer: f64,
    system_type: SystemType,
    star_mass_solar: f64,
    star_luminosity_solar: f64,
    allow_migration: bool,
    rng: &mut impl Rng,
) -> ProceduralPlanet {
    // Calculate orbital period using Kepler's third law: P² = a³ / M
    let period_years = semi_major_axis_au.powf(1.5) / star_mass_solar.sqrt();
    let period_days = period_years * 365.25;

    // Calculate equilibrium temperature: T_eq = 278.3 × L^(1/4) / d^(1/2)
    let equilibrium_temp = 278.3 * star_luminosity_solar.powf(0.25) / semi_major_axis_au.sqrt();

    // Determine planet type based on distance, temperature, and system type
    let planet_type = determine_planet_type(
        semi_major_axis_au,
        frost_line_au,
        hz_inner,
        hz_outer,
        equilibrium_temp,
        system_type,
        allow_migration,
        rng,
    );

    // Generate mass and radius based on planet type
    let (mass_earth, radius_earth) = generate_mass_radius(planet_type, semi_major_axis_au, frost_line_au, rng);

    // Generate visual properties based on composition
    let (_color, _albedo) = generate_visual_properties(
        planet_type,
        semi_major_axis_au,
        equilibrium_temp,
        rng,
    );

    // Generate rotation based on planet type and distance
    let rotation_period_days = match planet_type {
        PlanetType::GasGiant | PlanetType::IceGiant | PlanetType::MiniNeptune => {
            rng.random_range(0.3..0.9_f32)
        }
        _ => {
            if semi_major_axis_au < 0.15 {
                period_days as f32 // tidally locked
            } else if semi_major_axis_au < 0.3 {
                rng.random_range(10.0..60.0_f32)
            } else {
                let log_p = rng.random_range((-0.5_f32)..(1.0));
                10.0_f32.powf(log_p)
            }
        }
    };
    let axial_tilt_deg = rng.random_range(0.0_f32..1.0).powf(1.5) * 45.0;

    ProceduralPlanet {
        name: format!(
            "{} {}",
            star_name,
            char::from_u32('b' as u32 + slot_index as u32).unwrap_or('?')
        ),
        semi_major_axis_au,
        // Eccentricity based on planet type
        eccentricity: match planet_type {
            PlanetType::GasGiant | PlanetType::IceGiant => {
                rng.random_range(0.0_f64..1.0).powf(2.0) * 0.35
            }
            _ => rng.random_range(0.0_f64..1.0).powf(2.5) * 0.25,
        },
        // Inclination varies by system type
        inclination: rng.random_range(-0.13..0.13),
        longitude_ascending_node: rng.random_range(0.0..std::f64::consts::TAU),
        argument_of_periapsis: rng.random_range(0.0..std::f64::consts::TAU),
        mean_anomaly_epoch: rng.random_range(0.0..std::f64::consts::TAU),
        period_days,
        mass_earth,
        radius_earth,
        planet_type,
        rotation_period_days,
        axial_tilt_deg,
    }
}

/// Determine planet type based on orbital distance and conditions
fn determine_planet_type(
    distance_au: f64,
    frost_line_au: f64,
    hz_inner: f64,
    hz_outer: f64,
    equilibrium_temp: f64,
    system_type: SystemType,
    allow_migration: bool,
    rng: &mut impl Rng,
) -> PlanetType {
    // Check for lava world (very hot)
    if equilibrium_temp > 1000.0 {
        return PlanetType::LavaWorld;
    }

    // Check for desert world (hot, inside HZ inner edge)
    if distance_au < hz_inner && equilibrium_temp > 400.0 && equilibrium_temp < 1000.0 {
        if rng.random_bool(0.3) {
            return PlanetType::DesertWorld;
        }
    }

    // Check for hot Jupiter ( JovianHeavy system or migration)
    if system_type == SystemType::JovianHeavy && distance_au < hz_inner {
        if rng.random_bool(0.7) {
            return PlanetType::GasGiant;
        }
    }

    // Check for migration: gas giant inside frost line (decided once per system)
    if allow_migration && distance_au < frost_line_au {
        if rng.random_bool(0.6) {
            return PlanetType::GasGiant;
        } else {
            return PlanetType::MiniNeptune;
        }
    }

    // Inside frost line: rocky/terrestrial planets
    if distance_au < frost_line_au {
        // Check if in habitable zone
        let in_hz = distance_au >= hz_inner && distance_au <= hz_outer;

        if in_hz {
            // Habitable zone planet: water world, super earth, or rocky
            let roll = rng.random_range(0.0..1.0_f64);
            if roll < 0.3 {
                return PlanetType::WaterWorld;
            } else if roll < 0.7 {
                return PlanetType::SuperEarth;
            } else {
                return PlanetType::Rocky;
            }
        } else if distance_au < hz_inner {
            // Inside HZ but too hot: desert or rocky
            if rng.random_bool(0.4) {
                return PlanetType::DesertWorld;
            } else {
                return PlanetType::Rocky;
            }
        } else {
            // Outside HZ but inside frost line: could be super earth or rocky
            if rng.random_bool(0.5) {
                return PlanetType::SuperEarth;
            } else {
                return PlanetType::Rocky;
            }
        }
    }

    // Outside frost line: gas/ice giants
    let distance_ratio = distance_au / frost_line_au;

    if distance_ratio < 2.0 {
        // Very close to frost line: gas giant likely
        if rng.random_bool(0.7) {
            PlanetType::GasGiant
        } else {
            PlanetType::MiniNeptune
        }
    } else if distance_ratio < 4.0 {
        // Mid outer region: transition zone
        if rng.random_bool(0.5) {
            PlanetType::GasGiant
        } else {
            PlanetType::IceGiant
        }
    } else {
        // Far outer: ice giant dominant
        if rng.random_bool(0.3) {
            PlanetType::IceGiant
        } else {
            // Could also be a smaller ice world
            PlanetType::MiniNeptune
        }
    }
}

/// Generate mass and radius based on planet type
/// Implements Radius Valley logic: planets with 1.6-2.0 R_earth are differentiated
/// based on whether they're inside or outside the frost line
fn generate_mass_radius(planet_type: PlanetType, distance_au: f64, frost_line_au: f64, rng: &mut impl Rng) -> (f32, f32) {
    match planet_type {
        PlanetType::Rocky => {
            // 0.05 - 2.0 Earth masses
            let log_mass = rng.random_range(-1.3_f64..0.3);
            let mass = 10.0_f64.powf(log_mass) as f32;
            // Rocky: M = R^3.7 (dense rocky composition)
            let radius = (mass as f64).powf(1.0/3.7) * rng.random_range(0.90..1.10);
            (mass, radius as f32)
        }
        PlanetType::SuperEarth => {
            // 1.5 - 10 Earth masses
            let log_mass = rng.random_range(0.18_f64..1.0);
            let mass = 10.0_f64.powf(log_mass) as f32;

            // ========================================================================
            // RADIUS VALLEY LOGIC: Inside frost line = SuperEarth (dense, rocky)
            // ========================================================================
            let radius = if distance_au < frost_line_au {
                // Inside frost line: SuperEarth (high density, rocky interior)
                // M = R^3.5 for rocky SuperEarths
                (mass as f64).powf(1.0/3.5) * rng.random_range(0.90..1.10)
            } else {
                // Outside frost line: Mini-Neptune (low density, gas shroud)
                // M = R^2.0 for gaseous dwarfs
                (mass as f64).powf(0.5) * rng.random_range(1.5..2.5)
            };
            (mass, radius as f32)
        }
        PlanetType::MiniNeptune => {
            // 10 - 20 Earth masses - always gaseous
            let mass = rng.random_range(10.0..20.0_f32);
            // Mini-Neptunes: R ∝ M^0.25 (puffy atmospheres)
            let radius = (mass as f64).powf(0.25) * rng.random_range(2.0..3.5);
            (mass, radius as f32)
        }
        PlanetType::DesertWorld => {
            // 0.3 - 4.0 Earth masses (drier, denser)
            let log_mass = rng.random_range(-0.52_f64..0.6);
            let mass = 10.0_f64.powf(log_mass) as f32;
            // Denser than water worlds: R ∝ M^0.27
            let radius = (mass as f64).powf(0.27) * rng.random_range(0.85..1.0);
            (mass, radius as f32)
        }
        PlanetType::LavaWorld => {
            // 0.3 - 3.0 Earth masses (tidally heated or very close)
            let log_mass = rng.random_range(-0.52_f64..0.48);
            let mass = 10.0_f64.powf(log_mass) as f32;
            let radius = (mass as f64).powf(0.28) * rng.random_range(0.80..0.95); // Slightly smaller due to high temp
            (mass, radius as f32)
        }
        PlanetType::WaterWorld => {
            // 1.0 - 5.0 Earth masses (lots of water/ice)
            let log_mass = rng.random_range(0.0_f64..0.7);
            let mass = 10.0_f64.powf(log_mass) as f32;
            // Water worlds are slightly larger: R ∝ M^0.30
            let radius = (mass as f64).powf(0.30) * rng.random_range(1.0..1.2);
            (mass, radius as f32)
        }
        PlanetType::IceGiant => {
            // 8 - 30 Earth masses
            let log_mass = rng.random_range(0.9_f64..1.48);
            let mass = 10.0_f64.powf(log_mass) as f32;
            let radius = 3.5 + (mass / 15.0 - 1.0) * 0.5 * rng.random_range(0.8..1.2);
            (mass, radius.clamp(3.0, 5.0))
        }
        PlanetType::GasGiant => {
            // 30 - 800 Earth masses
            let log_mass = rng.random_range(1.48_f64..2.9);
            let mass = 10.0_f64.powf(log_mass) as f32;
            let base_radius = 9.0 + rng.random_range(-1.0..2.5_f32);
            let radius = base_radius * rng.random_range(0.9..1.1);
            (mass, radius.clamp(7.0, 13.0))
        }
    }
}

/// Generate visual properties (color and albedo) based on planet composition
fn generate_visual_properties(
    planet_type: PlanetType,
    _distance_au: f64,
    equilibrium_temp: f64,
    rng: &mut impl Rng,
) -> (Vec3, f32) {
    // Default albedo range: 0.02 (dark) to 0.95 (bright/icy)
    let (base_color, base_albedo) = match planet_type {
        PlanetType::Rocky => {
            // Grey/brown rocky planets
            let albedo = rng.random_range(0.1..0.4);
            let r = rng.random_range(0.3..0.6);
            let g = rng.random_range(0.25..0.5);
            let b = rng.random_range(0.2..0.4);
            (Vec3::new(r, g, b), albedo)
        }
        PlanetType::SuperEarth => {
            // Could be rocky or water worlds
            if rng.random_bool(0.5) {
                // Rocky super earth: grey/brown
                let albedo = rng.random_range(0.15..0.35);
                (Vec3::new(0.4, 0.35, 0.3), albedo)
            } else {
                // Water/terrestrial: blue-green
                let albedo = rng.random_range(0.3..0.6);
                let r = rng.random_range(0.2..0.4);
                let g = rng.random_range(0.4..0.6);
                let b = rng.random_range(0.5..0.7);
                (Vec3::new(r, g, b), albedo)
            }
        }
        PlanetType::MiniNeptune => {
            // Hydrogen/helium atmosphere: pale blue/white
            let albedo = rng.random_range(0.4..0.7);
            let b = rng.random_range(0.6..0.9);
            (Vec3::new(0.5, 0.6, b), albedo)
        }
        PlanetType::DesertWorld => {
            // Orange/red deserts (like Mars but hotter)
            let albedo = rng.random_range(0.15..0.35);
            let r = rng.random_range(0.6..0.9);
            let g = rng.random_range(0.3..0.5);
            let b = rng.random_range(0.1..0.3);
            (Vec3::new(r, g, b), albedo)
        }
        PlanetType::LavaWorld => {
            // Bright orange/red glowing
            let albedo = rng.random_range(0.1..0.25);
            let r = rng.random_range(0.8..1.0);
            let g = rng.random_range(0.3..0.6);
            let b = rng.random_range(0.05..0.2);
            (Vec3::new(r, g, b), albedo)
        }
        PlanetType::WaterWorld => {
            // Deep blue oceans
            let albedo = rng.random_range(0.4..0.7);
            let r = rng.random_range(0.1..0.3);
            let g = rng.random_range(0.3..0.5);
            let b = rng.random_range(0.6..0.9);
            (Vec3::new(r, g, b), albedo)
        }
        PlanetType::IceGiant => {
            // Cyan/blue with bands
            let albedo = rng.random_range(0.5..0.8);
            let g = rng.random_range(0.5..0.8);
            let b = rng.random_range(0.7..1.0);
            (Vec3::new(0.4, g, b), albedo)
        }
        PlanetType::GasGiant => {
            // Jupiter/Saturn-like: orange/tan with bands
            let albedo = rng.random_range(0.3..0.6);
            let r = rng.random_range(0.7..0.9);
            let g = rng.random_range(0.5..0.7);
            let b = rng.random_range(0.3..0.5);
            (Vec3::new(r, g, b), albedo)
        }
    };

    // Adjust based on distance/temperature (volatiles condense farther out)
    let mut color = base_color;
    let mut albedo: f32 = base_albedo;

    // If very hot, darken (lava) or redden (desert)
    if equilibrium_temp > 800.0 {
        albedo *= 0.8; // Darker due to molten surface
    }

    // If very cold, brighten (ice)
    if equilibrium_temp < 100.0 {
        albedo = (albedo + 0.2_f32).min(0.95);
        // Add slight blue tint
        color.z = (color.z + 0.1_f32).min(1.0);
    }

    (color, albedo)
}

/// Generate rocky planets for the inner system.
///
/// Uses **sequential multiplicative spacing**: each planet is placed at a
/// position that is at least `MIN_SPACING_FACTOR` times greater than the
/// previous orbit. This guarantees visually distinct, non-overlapping orbits
/// across all system scales — including ultra-compact brown-dwarf systems.
fn generate_rocky_planets(
    star_name: &str,
    count: usize,
    frost_line_au: f64,
    existing_orbits_au: &[f64],
    rng: &mut impl Rng,
) -> Vec<ProceduralPlanet> {
    let mut planets = Vec::new();

    // Inner system range: scaled with frost line but with minimum extents.
    let inner_min = (frost_line_au * 0.5).max(0.08); // At least 0.08 AU
    let inner_max = (frost_line_au * 0.95).max(inner_min + 0.25); // At least 0.25 AU range

    if inner_max <= inner_min {
        return planets;
    }

    // Minimum multiplicative gap between consecutive orbits.
    // A factor of 1.30 means each planet must orbit at least 30% farther
    // out than its inner neighbour — derived loosely from mutual Hill-sphere
    // stability criteria and ensures clear visual separation at all scales.
    const MIN_SPACING_FACTOR: f64 = 1.30;

    // Find the furthest existing orbit that lies within the inner zone so we
    // know where to start placing new planets.
    let last_existing_inner = existing_orbits_au
        .iter()
        .filter(|&&a| a <= inner_max)
        .cloned()
        .fold(f64::NAN, f64::max);

    // Current placement cursor: start from inner_min, or just past the last
    // existing inner-zone planet (whichever is further out).
    let mut cursor = if last_existing_inner.is_finite() {
        (last_existing_inner * MIN_SPACING_FACTOR).max(inner_min)
    } else {
        inner_min
    };

    // All orbits used for the final uniqueness check (existing + new).
    let mut all_orbits = existing_orbits_au.to_vec();

    for i in 0..count {
        if cursor > inner_max {
            // No room for more planets in the inner zone.
            break;
        }

        // Apply a small random jitter around the cursor position.
        let jitter = rng.random_range(-0.06..0.06);
        let mut semi_major_axis = (cursor * (1.0 + jitter)).clamp(inner_min, inner_max);

        // Safety pass: if jitter pushed us into an existing orbit, walk outward
        // in small relative steps until we are clear (max 16 iterations).
        for _ in 0..16 {
            let min_sep = semi_major_axis * (MIN_SPACING_FACTOR - 1.0);
            if !is_too_close_to_existing(semi_major_axis, &all_orbits, min_sep) {
                break;
            }
            semi_major_axis = (semi_major_axis * 1.08).min(inner_max);
        }

        // If still too close after the safety pass (e.g., no room left), skip.
        let min_sep = semi_major_axis * (MIN_SPACING_FACTOR - 1.0);
        if is_too_close_to_existing(semi_major_axis, &all_orbits, min_sep) {
            break;
        }

        // Calculate orbital period using Kepler's third law: T² = a³
        let period_years = semi_major_axis.powf(1.5);
        let period_days = period_years * 365.25;

        // Rocky planet rotation periods:
        // Earth 1.0 d, Mars 1.03 d, Mercury 58.6 d (3:2 resonance)
        // Most rocky planets rotate in ~0.3-3 days; close-in ones can be tidally locked
        let rotation_period_days = if semi_major_axis < 0.15 {
            // Very close-in: likely tidally locked (period ≈ orbital period)
            period_days as f32
        } else if semi_major_axis < 0.3 {
            // Tidal braking zone: real planets here rotate in ~2-8 days, not 10-60.
            // The previous 10-60 day range produced unrealistically extreme temperature
            // differentials (±65% of average) via adjust_temperature_for_rotation.
            rng.random_range(2.0..8.0_f32)
        } else {
            // Normal: log-uniform from ~0.3 to ~5 days
            let log_p = rng.random_range((-0.5_f32)..(0.7));
            10.0_f32.powf(log_p)
        };

        // Axial tilt: most rocky planets 0-30°, occasional high obliquity
        // Earth 23.4°, Mars 25.2°, Venus 177° (retrograde), Uranus 97.8°
        let axial_tilt_deg = rng.random_range(0.0_f32..1.0).powf(1.5) * 45.0;

        let planet = ProceduralPlanet {
            name: format!(
                "{} {}",
                star_name,
                char::from_u32('b' as u32 + existing_orbits_au.len() as u32 + i as u32)
                    .unwrap_or('?')
            ),
            semi_major_axis_au: semi_major_axis,
            // Eccentricity: most rocky planets are near-circular, but a few
            // can be quite eccentric (Mercury 0.206, HD 80606 b 0.93).
            // Use a power-law-biased distribution: low values common, high rare.
            eccentricity: rng.random_range(0.0_f64..1.0).powf(2.5) * 0.25,
            // Inclination: real planets 0-7° (Mercury 7°, Venus 3.4°, Mars 1.85°)
            inclination: rng.random_range(-0.13..0.13), // ±~7.5°
            longitude_ascending_node: rng.random_range(0.0..std::f64::consts::TAU),
            argument_of_periapsis: rng.random_range(0.0..std::f64::consts::TAU),
            mean_anomaly_epoch: rng.random_range(0.0..std::f64::consts::TAU),
            period_days,
            // Mass-radius correlation (Chen & Kipping 2017):
            // For rocky planets, R ∝ M^0.28 (Terran regime)
            mass_earth: {
                // Log-uniform from 0.05 (sub-Mercury) to 5.0 (super-Earth)
                let log_mass = rng.random_range((-1.3_f64)..(0.7_f64)); // 10^-1.3 ≈ 0.05, 10^0.7 ≈ 5.0
                10.0_f64.powf(log_mass) as f32
            },
            radius_earth: 0.0, // placeholder, set below
            planet_type: PlanetType::Rocky,
            rotation_period_days,
            axial_tilt_deg,
        };

        // Derive radius from mass using empirical power law + scatter
        // R ≈ M^0.28 for rocky planets (Chen & Kipping 2017)
        let radius = (planet.mass_earth as f64).powf(0.28) * rng.random_range(0.90..1.10_f64);
        let planet = ProceduralPlanet {
            radius_earth: radius as f32,
            ..planet
        };

        all_orbits.push(semi_major_axis);
        planets.push(planet);

        // Advance the cursor: next planet must be at least MIN_SPACING_FACTOR
        // times the current orbit, with an additional random spread.
        cursor = semi_major_axis * rng.random_range(MIN_SPACING_FACTOR..1.60);
    }

    planets
}

/// Generate gas and ice giants for the outer system.
///
/// Uses **sequential multiplicative spacing** (same principle as rocky planets)
/// so orbits are guaranteed to be distinct at every system scale.
fn generate_gas_giants(
    star_name: &str,
    count: usize,
    frost_line_au: f64,
    existing_orbits_au: &[f64],
    name_offset: usize,
    rng: &mut impl Rng,
) -> Vec<ProceduralPlanet> {
    let mut planets = Vec::new();

    // Outer system range: frost line to ~30 AU
    let outer_min = (frost_line_au * 1.2).max(0.5);
    let outer_max = 30.0;

    // Gas giants need even larger relative gaps than rocky planets.
    const MIN_SPACING_FACTOR: f64 = 1.40;

    // Find the furthest existing orbit in the outer zone.
    let last_existing_outer = existing_orbits_au
        .iter()
        .filter(|&&a| a >= outer_min * 0.8)
        .cloned()
        .fold(f64::NAN, f64::max);

    let mut cursor = if last_existing_outer.is_finite() {
        (last_existing_outer * MIN_SPACING_FACTOR).max(outer_min)
    } else {
        outer_min
    };

    // All orbits used for uniqueness check.
    let mut all_orbits = existing_orbits_au.to_vec();

    for i in 0..count {
        if cursor > outer_max {
            break;
        }

        // Small jitter around cursor.
        let jitter = rng.random_range(-0.08..0.08);
        let mut semi_major_axis = (cursor * (1.0 + jitter)).clamp(outer_min, outer_max);

        // Safety: walk outward if we land on an existing orbit (max 16 steps).
        for _ in 0..16 {
            let min_sep = semi_major_axis * (MIN_SPACING_FACTOR - 1.0);
            if !is_too_close_to_existing(semi_major_axis, &all_orbits, min_sep) {
                break;
            }
            semi_major_axis = (semi_major_axis * 1.10).min(outer_max);
        }

        let min_sep = semi_major_axis * (MIN_SPACING_FACTOR - 1.0);
        if is_too_close_to_existing(semi_major_axis, &all_orbits, min_sep) {
            break;
        }

        // Calculate orbital period using Kepler's third law
        let period_years = semi_major_axis.powf(1.5);
        let period_days = period_years * 365.25;

        // Determine if this is a gas giant or ice giant
        // Gas giants form closer to frost line where more material is available;
        // ice giants dominate further out where accretion is slower.
        // Jupiter (5.2AU ≈ 1.07× frost line), Saturn (9.5AU ≈ 1.96×),
        // Uranus (19.2AU ≈ 3.96×), Neptune (30.0AU ≈ 6.19×)
        let distance_ratio = semi_major_axis / frost_line_au;
        let planet_type = if distance_ratio < 2.5 && rng.random_bool(0.7) {
            PlanetType::GasGiant // More likely near frost line
        } else if distance_ratio > 4.0 {
            PlanetType::IceGiant // Dominant far out
        } else {
            // Transition zone: either type
            if rng.random_bool(0.4) {
                PlanetType::GasGiant
            } else {
                PlanetType::IceGiant
            }
        };

        // Mass-radius with realistic variance
        // Gas giants: Jupiter 317.8 M⊕ / 11.2 R⊕, Saturn 95.2 M⊕ / 9.4 R⊕
        // Ice giants: Uranus 14.5 M⊕ / 4.0 R⊕, Neptune 17.1 M⊕ / 3.9 R⊕
        let (mass_earth, radius_earth) = match planet_type {
            PlanetType::IceGiant => {
                // Log-uniform mass: 8-30 M⊕ (Uranus 14.5, Neptune 17.1)
                let log_mass = rng.random_range(0.9_f64..1.48); // 10^0.9 ≈ 8, 10^1.48 ≈ 30
                let mass = 10.0_f64.powf(log_mass) as f32;
                // R ∝ M^0.06 for Neptunian regime (Chen & Kipping 2017) + scatter
                let radius = 3.5 + (mass / 15.0 - 1.0) * 0.5 * rng.random_range(0.8..1.2);
                (mass, radius.clamp(3.0, 5.0))
            }
            PlanetType::GasGiant => {
                // Log-uniform mass: 30-800 M⊕ (Saturn 95, Jupiter 318, some up to 2× Jupiter)
                let log_mass = rng.random_range(1.48_f64..2.9); // 10^1.48 ≈ 30, 10^2.9 ≈ 800
                let mass = 10.0_f64.powf(log_mass) as f32;
                // Gas giants have roughly constant radius (~9-12 R⊕)
                // due to degeneracy pressure; more massive ones can be smaller (hot/dense)
                let base_radius = 9.0 + rng.random_range(-1.0..2.5_f32);
                let radius = base_radius * rng.random_range(0.9..1.1);
                (mass, radius.clamp(7.0, 13.0))
            }
            _ => unreachable!(),
        };

        // Gas/ice giant rotation periods:
        // Jupiter 0.41 d, Saturn 0.44 d, Neptune 0.67 d, Uranus 0.72 d
        // Gas giants are fast rotators: ~0.3-0.8 days typically
        let rotation_period_days = rng.random_range(0.3..0.9_f32);

        // Gas giant axial tilts: Jupiter 3.1°, Saturn 26.7°, Neptune 28.3°, Uranus 97.8°
        let axial_tilt_deg = rng.random_range(0.0_f32..1.0).powf(1.2) * 40.0;

        let planet = ProceduralPlanet {
            name: format!(
                "{} {}",
                star_name,
                char::from_u32('b' as u32 + name_offset as u32 + i as u32).unwrap_or('?')
            ),
            semi_major_axis_au: semi_major_axis,
            // Gas giants: most near-circular but some significantly eccentric
            // Jupiter 0.049, Saturn 0.054, HD 80606 b 0.93 (extreme)
            eccentricity: rng.random_range(0.0_f64..1.0).powf(2.0) * 0.35,
            // Inclination: Jupiter 1.3°, Saturn 2.5°, up to ~5-10° for exoplanets
            inclination: rng.random_range(-0.15..0.15), // ±~8.6°
            longitude_ascending_node: rng.random_range(0.0..std::f64::consts::TAU),
            argument_of_periapsis: rng.random_range(0.0..std::f64::consts::TAU),
            mean_anomaly_epoch: rng.random_range(0.0..std::f64::consts::TAU),
            period_days,
            mass_earth,
            radius_earth,
            planet_type,
            rotation_period_days,
            axial_tilt_deg,
        };

        // Advance cursor with multiplicative spacing + extra random spread.
        all_orbits.push(semi_major_axis);
        planets.push(planet);

        cursor = semi_major_axis * rng.random_range(MIN_SPACING_FACTOR..1.80);
    }

    planets
}

/// Generate an asteroid belt
fn generate_asteroid_belt(
    frost_line_au: f64,
    existing_orbits_au: &[f64],
    rng: &mut impl Rng,
) -> AsteroidBelt {
    // For ultra-dim stars with near-zero frost lines, use a minimum belt center
    let base_center = (frost_line_au * 2.0).max(0.5);

    // Belt width: ±25% of center, with a minimum width of 0.5 AU so that
    // dim-star belts don't collapse into a dense ring of overlapping bodies.
    let half_width = (base_center * 0.25).max(0.25);
    let mut inner = base_center - half_width;
    let mut outer = base_center + half_width;

    // Adjust if too close to existing planets
    for &orbit in existing_orbits_au {
        if (orbit - base_center).abs() < 1.0 {
            // Shift the belt while keeping width
            if orbit < base_center {
                inner = orbit + 0.3;
                outer = inner + half_width * 2.0;
            } else {
                outer = orbit - 0.3;
                inner = outer - half_width * 2.0;
            }
        }
    }

    // Ensure valid range: inner must be positive and less than outer
    inner = inner.max(0.01);
    if outer <= inner {
        outer = inner + 0.5;
    }

    AsteroidBelt {
        inner_au: inner,
        outer_au: outer,
        count: rng.random_range(50..200), // Number of asteroids to spawn
        inclination: rng.random_range(0.0..0.1), // Low to moderate inclination
    }
}

/// Generate a cometary cloud
fn generate_cometary_cloud(frost_line_au: f64, rng: &mut impl Rng) -> CometaryCloud {
    // Cloud at outer reaches of system, scaled to the effective frost line.
    // For compact systems (small effective frost line), this places comets
    // proportionally closer rather than always at 20-50 AU.
    let inner = (frost_line_au * 4.0).max(1.0);
    let mut outer = (frost_line_au * 10.0).max(inner * 2.5);

    // Ensure valid range
    if outer <= inner {
        outer = inner + frost_line_au.max(1.0);
    }

    CometaryCloud {
        inner_au: inner,
        outer_au: outer,
        count: rng.random_range(20..80), // Fewer but more visible comets
        inclination: rng.random_range(0.0..PI / 3.0), // High inclination (spherical distribution)
    }
}

/// Check if a proposed orbit is too close to any existing orbit.
///
/// The `min_separation` is an **absolute** AU floor. In addition, orbits
/// closer than 15 % of the smaller radius are always considered too close,
/// which naturally enforces tighter gaps in the inner system without
/// requiring per-callsite tuning.
fn is_too_close_to_existing(
    proposed_au: f64,
    existing_orbits_au: &[f64],
    min_separation: f64,
) -> bool {
    for &existing in existing_orbits_au {
        let abs_gap = (proposed_au - existing).abs();
        // Relative gap as a fraction of the inner orbit's radius.
        let rel_gap = abs_gap / proposed_au.min(existing).max(1e-9);
        if abs_gap < min_separation || rel_gap < 0.15 {
            return true;
        }
    }
    false
}

/// Generate dwarf planets in the trans-Neptunian region.
///
/// Models bodies like Pluto (39.5 AU, e=0.25, i=17°), Eris (67.7 AU, e=0.44, i=44°),
/// Makemake (45.4 AU, e=0.16, i=29°), Haumea (43.1 AU, e=0.19, i=28°).
/// These bodies occupy high-eccentricity, high-inclination orbits in the
/// Kuiper belt and scattered disk.
fn generate_dwarf_planets(
    star_name: &str,
    frost_line_au: f64,
    existing_orbits_au: &[f64],
    name_offset: usize,
    star_mass_solar: f64,
    rng: &mut impl Rng,
) -> Vec<ProceduralPlanet> {
    let mut planets = Vec::new();

    // Trans-Neptunian region: 6× frost line to 20× frost line
    // Scaled with the (effective) frost line so compact systems get
    // proportionally closer dwarf planets instead of always 10-150 AU.
    let inner = (frost_line_au * 6.0).max(0.5);
    let outer = (frost_line_au * 20.0).max(inner * 3.0).min(150.0);

    // 1-4 dwarf planets (Sol has ~5 officially, likely hundreds undiscovered)
    let count = rng.random_range(1..=4_usize);

    let mut all_orbits = existing_orbits_au.to_vec();

    for i in 0..count {
        // Log-uniform distribution across the region
        let log_inner = inner.ln();
        let log_outer = outer.ln();
        let semi_major_axis =
            (log_inner + rng.random_range(0.0..1.0_f64) * (log_outer - log_inner)).exp();

        // Check spacing
        let min_sep = semi_major_axis * 0.1;
        if is_too_close_to_existing(semi_major_axis, &all_orbits, min_sep) {
            continue;
        }

        // Eccentricity: wide range, many TNOs have significant eccentricity
        // Pluto 0.25, Eris 0.44, Makemake 0.16, Sedna 0.84
        // Biased toward moderate values with some high-e scattered disk bodies
        let eccentricity = if rng.random_range(0.0..1.0_f64) < 0.2 {
            // Scattered disk: high eccentricity (Sedna-like)
            rng.random_range(0.5..0.85_f64)
        } else {
            // Classical/resonant Kuiper belt
            rng.random_range(0.03..0.35_f64)
        };

        // Inclination: TNOs have wide range
        // Classical belt: 0-5°, hot population: up to 30°+
        // Scattered disk: up to 45°+
        // Pluto 17°, Eris 44°, Makemake 29°, Haumea 28°
        let inclination = rng.random_range(0.0_f64..1.0).powf(0.5) * 0.87; // up to ~50°, biased toward moderate

        let period_years = semi_major_axis.powf(1.5) / star_mass_solar.sqrt();
        let period_days = period_years * 365.25;

        // Mass: dwarf planets range from ~0.00015 M⊕ (Ceres) to ~0.003 M⊕ (Eris)
        // Log-uniform distribution
        let log_mass = rng.random_range(-4.0..-2.5_f64); // 10^-4 to 10^-2.5 M⊕
        let mass_earth = 10.0_f64.powf(log_mass) as f32;

        // Radius from mass using icy body density (~1800-2200 kg/m³)
        // R ∝ M^(1/3) for constant density
        let density = rng.random_range(1600.0..2400.0_f64);
        let mass_kg = (mass_earth as f64) * 5.972e24;
        let volume_m3 = mass_kg / density;
        let radius_m = (volume_m3 * 3.0 / (4.0 * PI)).powf(1.0 / 3.0);
        let radius_earth = (radius_m / 6.371e6) as f32;

        // Dwarf planet rotation:
        // Pluto 6.39 d, Eris ~15.8 d, Ceres 0.38 d, Makemake 0.95 d, Haumea 0.16 d
        // Wide range from fast rotators to slow ones
        let rotation_period_days = {
            let log_p = rng.random_range((-0.8_f32)..(1.2));
            10.0_f32.powf(log_p) // ~0.16 to ~16 days
        };

        // TNO axial tilts can be extreme: Pluto 122.5°, Ceres 4°, Eris ~78°
        let axial_tilt_deg = rng.random_range(0.0_f32..60.0);

        let planet = ProceduralPlanet {
            name: format!(
                "{} {}",
                star_name,
                char::from_u32('b' as u32 + name_offset as u32 + i as u32).unwrap_or('?')
            ),
            semi_major_axis_au: semi_major_axis,
            eccentricity,
            inclination,
            longitude_ascending_node: rng.random_range(0.0..std::f64::consts::TAU),
            argument_of_periapsis: rng.random_range(0.0..std::f64::consts::TAU),
            mean_anomaly_epoch: rng.random_range(0.0..std::f64::consts::TAU),
            period_days,
            mass_earth,
            radius_earth,
            planet_type: PlanetType::Rocky, // Dwarf planets are rocky/icy
            rotation_period_days,
            axial_tilt_deg,
        };

        all_orbits.push(semi_major_axis);
        planets.push(planet);
    }

    planets
}

impl ProceduralPlanet {
    /// Convert to a KeplerOrbit component
    pub fn to_kepler_orbit(&self) -> KeplerOrbit {
        let period_seconds = self.period_days * 86400.0;
        let mean_motion = std::f64::consts::TAU / period_seconds;

        KeplerOrbit::new(
            self.eccentricity,
            self.semi_major_axis_au,
            self.inclination,
            self.longitude_ascending_node,
            self.argument_of_periapsis,
            self.mean_anomaly_epoch,
            mean_motion,
        )
    }

    /// Get the body type for this planet
    pub fn body_type(&self) -> BodyType {
        match self.planet_type {
            PlanetType::Rocky
            | PlanetType::SuperEarth
            | PlanetType::DesertWorld
            | PlanetType::LavaWorld
            | PlanetType::WaterWorld => BodyType::Planet,
            PlanetType::MiniNeptune | PlanetType::IceGiant | PlanetType::GasGiant => {
                BodyType::GasGiant
            }
        }
    }

    /// Calculate mass in kilograms
    pub fn mass_kg(&self) -> f64 {
        const EARTH_MASS_KG: f64 = 5.972e24;
        (self.mass_earth as f64) * EARTH_MASS_KG
    }

    /// Calculate radius in kilometers
    pub fn radius_km(&self) -> f32 {
        const EARTH_RADIUS_KM: f32 = 6371.0;
        self.radius_earth * EARTH_RADIUS_KM
    }
}

/// Generate a procedural atmosphere for a planet based on its properties
/// Uses physics-based calculations (Cosmic Shoreline) to determine atmosphere retention
/// Returns (AtmosphereComposition, adjusted_temperature) if the planet should have an atmosphere
pub fn generate_procedural_atmosphere(
    planet_mass_earth: f32,
    planet_radius_earth: f32,
    distance_au: f64,
    star_luminosity_sol: f32,
    equilibrium_temp_k: f64,
    rng: &mut impl rand::Rng,
) -> Option<(crate::astronomy::AtmosphereComposition, f32)> {
    use crate::astronomy::{AtmosphereComposition, AtmosphericGas};

    let mass_kg = (planet_mass_earth as f64) * EARTH_MASS_KG;
    let radius_km = (planet_radius_earth as f64) * EARTH_RADIUS_KM;

    // ========================================================================
    // STEP 1: Calculate escape velocity (v_esc = sqrt(2GM/R))
    // ========================================================================
    let escape_velocity = calculate_escape_velocity(mass_kg, radius_km * 1000.0); // Convert km to m

    // ========================================================================
    // STEP 2: Calculate instellation (I = L / d²)
    // ========================================================================
    let instellation = calculate_instellation(star_luminosity_sol as f64, distance_au);

    // ========================================================================
    // STEP 3: Apply Cosmic Shoreline - physics-based atmosphere retention
    // v_esc^4 > K * I determines if atmosphere is retained
    // K = 1.0 is baseline for Earth-like retention
    // ========================================================================
    let cosmic_shoreline_pass = can_retain_atmosphere_cosmic_shoreline(
        escape_velocity,
        instellation,
        1.0, // Baseline constant
    );

    // Also check with the existing physics-based check
    if !AtmosphereComposition::can_retain_atmosphere(mass_kg, radius_km as f32) {
        return None;
    }

    // If cosmic shoreline check fails, planet becomes a Vacuum world (like Mercury)
    if !cosmic_shoreline_pass {
        return None;
    }

    // ========================================================================
    // STEP 4: Determine habitable zone membership
    // ========================================================================
    let in_habitable_zone = is_in_habitable_zone(distance_au, star_luminosity_sol as f64);

    // Planet must be terrestrial-sized (0.5 - 5.0 Earth masses for rocky planets with atmospheres)
    // Larger planets become mini-Neptunes with thick H/He envelopes
    let is_terrestrial_size = planet_mass_earth >= 0.5 && planet_mass_earth <= 5.0;

    if !is_terrestrial_size {
        return None; // Too small or too large for Earth-like atmosphere
    }

    // ========================================================================
    // STEP 5: Generate atmospheric composition based on temperature (Recipes)
    // ========================================================================
    let (gases, pressure_mbar, greenhouse_factor) = generate_atmosphere_recipe(
        equilibrium_temp_k,
        in_habitable_zone,
        planet_mass_earth,
        distance_au,
        rng,
    );

    // Calculate surface temperature with greenhouse effect
    let surface_temp_k = equilibrium_temp_k * greenhouse_factor;
    let surface_temp_c = (surface_temp_k - 273.15) as f32;

    let atmosphere = AtmosphereComposition::new_with_body_data(
        pressure_mbar as f32,
        surface_temp_c,
        gases,
        mass_kg,
        radius_km as f32,
        false, // Not a reference pressure
    );

    Some((atmosphere, surface_temp_c))
}

/// Generate atmospheric composition based on physics-based recipes
fn generate_atmosphere_recipe(
    equilibrium_temp_k: f64,
    in_habitable_zone: bool,
    planet_mass_earth: f32,
    distance_au: f64,
    rng: &mut impl rand::Rng,
) -> (Vec<crate::astronomy::AtmosphericGas>, f64, f64) {
    use crate::astronomy::AtmosphericGas;

    // ========================================================================
    // RECIPE 1: LavaWorld - T_eq > 1500K
    // Atmosphere: Sodium (Na) and Silicate Vapor (SiO)
    // ========================================================================
    if equilibrium_temp_k > 1500.0 {
        return (
            vec![
                AtmosphericGas::new("Na", 60.0 + rng.random_range(-10.0..20.0)),
                AtmosphericGas::new("SiO", 25.0 + rng.random_range(-5.0..10.0)),
                AtmosphericGas::new("O2", 10.0 + rng.random_range(-3.0..5.0)),
                AtmosphericGas::new("K", 5.0),
            ],
            rng.random_range(0.1..5.0), // Very thin, high-altitude haze
            1.0,                        // No greenhouse effect from these gases
        );
    }

    // ========================================================================
    // RECIPE 2: Hot Planet - 1000K < T_eq < 1500K
    // Thick CO2/SO2 atmosphere (Venus-like)
    // ========================================================================
    if equilibrium_temp_k > 1000.0 {
        return (
            vec![
                AtmosphericGas::new("CO2", 96.0 + rng.random_range(-2.0..3.0)),
                AtmosphericGas::new("N2", 3.0 + rng.random_range(-1.0..1.0)),
                AtmosphericGas::new("SO2", 1.0 + rng.random_range(0.0..0.5)),
            ],
            rng.random_range(5000.0..90000.0), // Very thick
            2.0,                               // Strong greenhouse
        );
    }

    // ========================================================================
    // RECIPE 3: Cold terrestrial - T_eq < 150K (outside frost line)
    // Titan-like: N2-dominated with CH4 (terrestrial bodies can't retain
    // primordial H2/He at these masses; H2/He atmospheres are for ice/gas giants)
    // ========================================================================
    if equilibrium_temp_k < 150.0 && !in_habitable_zone {
        return (
            vec![
                AtmosphericGas::new("N2", 90.0 + rng.random_range(-5.0..5.0)),
                AtmosphericGas::new("CH4", 5.0 + rng.random_range(-2.0..3.0)),
                AtmosphericGas::new("Ar", 3.0 + rng.random_range(-1.0..1.0)),
                AtmosphericGas::new("CO", 2.0),
            ],
            rng.random_range(500.0..3000.0), // Substantial but not giant-planet thick
            1.15,                            // Moderate greenhouse from CH4
        );
    }

    // ========================================================================
    // RECIPE 4: Habitable Zone - 250K < T_eq < 350K
    // ========================================================================
    if in_habitable_zone && equilibrium_temp_k > 250.0 && equilibrium_temp_k < 350.0 {
        // Oxygen only allowed if mass > 0.5 Earth masses (for photosynthesis)
        let allow_oxygen = planet_mass_earth > 0.5;

        if allow_oxygen && rng.random::<f32>() < 0.35 {
            // Breathable Earth-like atmosphere
            return (
                vec![
                    AtmosphericGas::new("N2", 78.0 + rng.random_range(-3.0..3.0)),
                    AtmosphericGas::new("O2", 21.0 + rng.random_range(-2.0..2.0)),
                    AtmosphericGas::new("Ar", 0.93),
                    AtmosphericGas::new("CO2", 0.04 + rng.random_range(-0.02..0.1)),
                ],
                rng.random_range(800.0..1200.0),
                1.3,
            );
        } else if rng.random::<f32>() > 0.5 {
            // Thick CO2 (Venus-lite in HZ)
            return (
                vec![
                    AtmosphericGas::new("CO2", 95.0 + rng.random_range(-5.0..3.0)),
                    AtmosphericGas::new("N2", 3.0 + rng.random_range(-1.0..2.0)),
                    AtmosphericGas::new("Ar", 1.6),
                ],
                rng.random_range(2000.0..10000.0),
                1.8,
            );
        } else {
            // Thin Mars-like
            return (
                vec![
                    AtmosphericGas::new("CO2", 95.0),
                    AtmosphericGas::new("N2", 2.7),
                    AtmosphericGas::new("Ar", 1.6),
                ],
                rng.random_range(5.0..15.0),
                1.05,
            );
        }
    }

    // ========================================================================
    // RECIPE 5: Hot but not in HZ - 350K < T_eq < 1000K
    // ========================================================================
    if equilibrium_temp_k > 350.0 && equilibrium_temp_k <= 1000.0 {
        return (
            vec![
                AtmosphericGas::new("CO2", 90.0 + rng.random_range(-5.0..8.0)),
                AtmosphericGas::new("N2", 5.0 + rng.random_range(-2.0..3.0)),
                AtmosphericGas::new("H2O", 5.0 + rng.random_range(-3.0..5.0)),
            ],
            rng.random_range(500.0..5000.0),
            1.5,
        );
    }

    // ========================================================================
    // RECIPE 6: Cold - 150K < T_eq < 250K (outside HZ, but not frozen)
    // ========================================================================
    if equilibrium_temp_k > 150.0 && equilibrium_temp_k <= 250.0 {
        return (
            vec![
                AtmosphericGas::new("N2", 75.0 + rng.random_range(-10.0..15.0)),
                AtmosphericGas::new("CH4", 15.0 + rng.random_range(-5.0..8.0)),
                AtmosphericGas::new("Ar", 5.0),
                AtmosphericGas::new("CO2", 5.0),
            ],
            rng.random_range(10.0..200.0),
            1.15,
        );
    }

    // ========================================================================
    // DEFAULT: Very thin atmosphere
    // ========================================================================
    (
        vec![
            AtmosphericGas::new("N2", 95.0),
            AtmosphericGas::new("Ar", 3.0),
            AtmosphericGas::new("CO2", 2.0),
        ],
        rng.random_range(0.5..5.0),
        1.0,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    #[test]
    fn test_frost_line_calculation() {
        // Sun: L = 1.0 L☉, frost line should be ~4.85 AU
        let sun_frost_line = calculate_frost_line(1.0);
        assert!((sun_frost_line - 4.85).abs() < 0.01);

        // Alpha Centauri A: L = 1.519 L☉
        let alpha_cen_a_frost_line = calculate_frost_line(1.519);
        assert!(alpha_cen_a_frost_line > 5.0 && alpha_cen_a_frost_line < 7.0);

        // Proxima Centauri: L = 0.0017 L☉
        let proxima_frost_line = calculate_frost_line(0.0017);
        assert!(proxima_frost_line < 0.5);
    }

    #[test]
    fn test_system_generation_empty_system() {
        let mut rng = StdRng::seed_from_u64(42);

        let architecture = map_star_to_system_architecture(
            "Test Star",
            1.0, // 1.0 solar mass
            1.0, // 1.0 solar luminosity
            0,   // No existing planets
            &[],
            &mut rng,
        );

        // Should generate planets to reach target of 5
        assert!(architecture.rocky_planets.len() + architecture.gas_giants.len() >= 4);
        assert!(architecture.frost_line_au > 4.0 && architecture.frost_line_au < 5.5);
    }

    #[test]
    fn test_system_generation_partial_system() {
        let mut rng = StdRng::seed_from_u64(123);

        // System with 2 existing planets
        let existing = vec![0.5, 1.2];

        let architecture =
            map_star_to_system_architecture("Test Star", 1.0, 1.0, 2, &existing, &mut rng);

        // Should generate fewer planets since we already have some
        assert!(architecture.rocky_planets.len() + architecture.gas_giants.len() <= 5);

        // Generated planets should not overlap with existing ones
        for planet in &architecture.rocky_planets {
            for &existing_orbit in &existing {
                assert!((planet.semi_major_axis_au - existing_orbit).abs() > 0.1);
            }
        }
    }

    #[test]
    fn test_rocky_planets_inside_frost_line() {
        let mut rng = StdRng::seed_from_u64(456);
        let frost_line = 4.85;

        let planets = generate_rocky_planets("Test", 3, frost_line, &[], &mut rng);

        assert!(planets.len() <= 3);
        assert!(!planets.is_empty());
        for planet in &planets {
            // All rocky planets should be inside the frost line
            assert!(planet.semi_major_axis_au < frost_line);
            assert_eq!(planet.planet_type, PlanetType::Rocky);
            // Rocky planets should have reasonable masses (0.05 - 5.0 M⊕ range)
            assert!(planet.mass_earth > 0.01 && planet.mass_earth < 10.0);
        }
    }

    #[test]
    fn test_gas_giants_outside_frost_line() {
        let mut rng = StdRng::seed_from_u64(789);
        let frost_line = 4.85;

        let planets = generate_gas_giants("Test", 2, frost_line, &[], 0, &mut rng);

        assert_eq!(planets.len(), 2);
        for planet in &planets {
            // All giants should be outside the frost line
            assert!(planet.semi_major_axis_au > frost_line);
            assert!(
                planet.planet_type == PlanetType::GasGiant
                    || planet.planet_type == PlanetType::IceGiant
            );
            // Giants should have significant mass
            assert!(planet.mass_earth > 10.0);
        }
    }

    #[test]
    fn test_kepler_orbit_conversion() {
        let mut rng = StdRng::seed_from_u64(999);
        let planets = generate_rocky_planets("Test", 1, 4.85, &[], &mut rng);

        let kepler = planets[0].to_kepler_orbit();
        assert_eq!(kepler.semi_major_axis, planets[0].semi_major_axis_au);
        assert_eq!(kepler.eccentricity, planets[0].eccentricity);
        assert!(kepler.mean_motion > 0.0);
    }

    #[test]
    fn test_asteroid_belt_minimum_width() {
        let mut rng = StdRng::seed_from_u64(42);

        // Very dim star (brown dwarf): frost line near zero
        let belt = generate_asteroid_belt(0.02, &[], &mut rng);
        let width = belt.outer_au - belt.inner_au;
        // Belt width should be at least 0.5 AU
        assert!(
            width >= 0.49,
            "Belt too narrow for dim star: {:.3} AU",
            width
        );
        assert!(belt.inner_au > 0.0);

        // Sun-like star: frost line ~4.85 AU
        let belt_sun = generate_asteroid_belt(4.85, &[], &mut rng);
        let width_sun = belt_sun.outer_au - belt_sun.inner_au;
        // Should be at least 0.5 AU wide
        assert!(
            width_sun >= 0.49,
            "Belt too narrow for sun-like star: {:.3} AU",
            width_sun
        );
    }

    #[test]
    fn test_atmosphere_retention_failure() {
        let mut rng = StdRng::seed_from_u64(42);

        // Small planet that cannot retain atmosphere (< 2.0 km/s escape velocity)
        // Mars-like: 0.107 M⊕, 0.53 R⊕ → escape velocity ~5 km/s (can retain)
        // Mercury-like: 0.055 M⊕, 0.38 R⊕ → escape velocity ~4.3 km/s (can retain)
        // Moon-like: 0.012 M⊕, 0.27 R⊕ → escape velocity ~2.4 km/s (borderline)
        // Very small: 0.01 M⊕, 0.25 R⊕ → escape velocity ~2.3 km/s (borderline)
        let result = generate_procedural_atmosphere(
            0.01,  // 0.01 Earth masses
            0.25,  // 0.25 Earth radii
            1.0,   // 1 AU
            1.0,   // Solar luminosity
            300.0, // Equilibrium temp
            &mut rng,
        );

        // Should return None for bodies too small to retain atmosphere
        assert!(result.is_none(), "Small body should not retain atmosphere");
    }

    #[test]
    fn test_atmosphere_outside_mass_range() {
        let mut rng = StdRng::seed_from_u64(123);

        // Too small: below 0.5 M⊕ terrestrial threshold
        let result_small = generate_procedural_atmosphere(
            0.2,   // Below 0.5 M⊕ threshold
            0.6,   // Earth-like radius
            1.0,   // 1 AU
            1.0,   // Solar luminosity
            300.0, // Equilibrium temp
            &mut rng,
        );
        assert!(
            result_small.is_none(),
            "Planet below 0.5 M⊕ should not get atmosphere"
        );

        // Too large: above 5.0 M⊕ (becomes mini-Neptune, not terrestrial)
        let result_large = generate_procedural_atmosphere(
            6.0,   // Above 5.0 M⊕ threshold
            2.0,   // Larger radius
            1.0,   // 1 AU
            1.0,   // Solar luminosity
            300.0, // Equilibrium temp
            &mut rng,
        );
        assert!(
            result_large.is_none(),
            "Planet above 5.0 M⊕ should not get atmosphere"
        );
    }

    #[test]
    fn test_atmosphere_deterministic_with_seed() {
        // Test that same seed produces same result
        let mut rng1 = StdRng::seed_from_u64(999);
        let mut rng2 = StdRng::seed_from_u64(999);

        let result1 = generate_procedural_atmosphere(
            1.0,   // Earth-like mass
            1.0,   // Earth-like radius
            1.0,   // 1 AU
            1.0,   // Solar luminosity
            288.0, // Earth-like temp
            &mut rng1,
        );

        let result2 = generate_procedural_atmosphere(
            1.0,   // Earth-like mass
            1.0,   // Earth-like radius
            1.0,   // 1 AU
            1.0,   // Solar luminosity
            288.0, // Earth-like temp
            &mut rng2,
        );

        // Both should produce the same result (either both Some or both None)
        assert_eq!(
            result1.is_some(),
            result2.is_some(),
            "RNG should be deterministic"
        );

        if let (Some((atm1, temp1)), Some((atm2, temp2))) = (result1, result2) {
            assert_eq!(atm1.surface_pressure_mbar, atm2.surface_pressure_mbar);
            assert_eq!(temp1, temp2);
            assert_eq!(atm1.gases.len(), atm2.gases.len());
        }
    }

    #[test]
    fn test_atmosphere_has_valid_composition() {
        let mut rng = StdRng::seed_from_u64(456);

        // Earth-like planet in habitable zone
        let result = generate_procedural_atmosphere(
            1.0,   // Earth mass
            1.0,   // Earth radius
            1.0,   // 1 AU (habitable zone)
            1.0,   // Solar luminosity
            288.0, // Earth-like equilibrium temp
            &mut rng,
        );

        // May or may not have atmosphere due to probability, but if it does:
        if let Some((atmosphere, _temp)) = result {
            // Check that gases sum to approximately 100%
            let total_percentage: f32 = atmosphere.gases.iter().map(|g| g.percentage).sum();
            assert!(
                (total_percentage - 100.0).abs() < 2.0,
                "Gas percentages should sum to ~100%, got {}",
                total_percentage
            );

            // Check that at least one gas is present
            assert!(
                !atmosphere.gases.is_empty(),
                "Atmosphere should have at least one gas"
            );

            // Pressure should be reasonable (not negative or absurdly high)
            assert!(atmosphere.surface_pressure_mbar > 0.0);
            assert!(atmosphere.surface_pressure_mbar < 100000.0); // Less than 100 bar
        }
    }
}
