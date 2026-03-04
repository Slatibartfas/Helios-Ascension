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
}

/// Type of procedurally generated planet
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanetType {
    Rocky,    // Inner system, terrestrial composition
    IceGiant, // Outer system, ice-rich
    GasGiant, // Outer system, gas-rich
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

/// Map a star to a system architecture based on its properties
/// This is the main entry point for procedural system generation
///
/// # Arguments
/// * `star_name` - Name of the star (for naming generated bodies)
/// * `luminosity_solar` - Luminosity in solar units
/// * `existing_planet_count` - Number of confirmed planets already in the system
/// * `existing_orbits_au` - Semi-major axes of existing planets (to avoid collisions)
/// * `rng` - Random number generator for variability
///
/// # Returns
/// SystemArchitecture containing all procedurally generated bodies
pub fn map_star_to_system_architecture(
    star_name: &str,
    luminosity_solar: f64,
    existing_planet_count: usize,
    existing_orbits_au: &[f64],
    rng: &mut impl Rng,
) -> SystemArchitecture {
    // Calculate frost line
    let frost_line_au = calculate_frost_line(luminosity_solar);

    debug!(
        "Generating system architecture for {} (L={:.3}L☉, frost line={:.2}AU)",
        star_name, luminosity_solar, frost_line_au
    );

    // Determine how many planets to add (aim for at least 5 total)
    let target_planet_count = 5;
    let planets_needed = if existing_planet_count < target_planet_count {
        target_planet_count - existing_planet_count
    } else {
        0
    };

    let mut rocky_planets = Vec::new();
    let mut gas_giants = Vec::new();

    // Generate planets if needed
    if planets_needed > 0 {
        // Determine distribution: inner vs outer
        // Inner system: 2-4 rocky planets (when adding 2+ planets)
        // Outer system: 1-3 gas/ice giants

        let inner_count = match planets_needed {
            1 => rng.random_range(0..=1),
            _ => rng.random_range(2..=4.min(planets_needed)),
        };
        let outer_count = (planets_needed - inner_count).min(3);

        // Generate inner system rocky planets
        rocky_planets = generate_rocky_planets(
            star_name,
            inner_count,
            frost_line_au,
            existing_orbits_au,
            rng,
        );

        // Generate outer system gas/ice giants
        // Offset the name index past confirmed + rocky-procedural planets
        let gas_name_offset = existing_orbits_au.len() + rocky_planets.len();

        // Combine existing orbits with newly generated rocky planets so gas giants avoid them
        let mut all_orbits = existing_orbits_au.to_vec();
        all_orbits.extend(rocky_planets.iter().map(|p| p.semi_major_axis_au));

        gas_giants = generate_gas_giants(
            star_name,
            outer_count,
            frost_line_au,
            &all_orbits,
            gas_name_offset,
            rng,
        );
    }

    // Generate asteroid belt (inside or near frost line)
    let asteroid_belt = if rng.random_bool(0.8) {
        // 80% chance of asteroid belt
        Some(generate_asteroid_belt(
            frost_line_au,
            existing_orbits_au,
            rng,
        ))
    } else {
        None
    };

    // Generate cometary cloud (far outer system)
    let cometary_cloud = if rng.random_bool(0.7) {
        // 70% chance of cometary cloud
        Some(generate_cometary_cloud(frost_line_au, rng))
    } else {
        None
    };

    // Generate dwarf planets in the trans-Neptunian region
    // Most systems with outer planets likely have a Kuiper belt analog
    // with a few dwarf-planet-scale bodies
    let dwarf_planets = if rng.random_bool(0.75) {
        // Combine all orbits for collision avoidance
        let mut all_orbits = existing_orbits_au.to_vec();
        all_orbits.extend(rocky_planets.iter().map(|p| p.semi_major_axis_au));
        all_orbits.extend(gas_giants.iter().map(|p| p.semi_major_axis_au));
        let name_offset = existing_orbits_au.len() + rocky_planets.len() + gas_giants.len();
        generate_dwarf_planets(star_name, frost_line_au, &all_orbits, name_offset, rng)
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
    // Cloud at outer reaches of system (20-50 AU)
    let inner = 20.0_f64.max(frost_line_au * 4.0);
    let mut outer = 50.0;

    // Ensure valid range
    if outer <= inner {
        outer = inner + 10.0;
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
    rng: &mut impl Rng,
) -> Vec<ProceduralPlanet> {
    let mut planets = Vec::new();

    // Trans-Neptunian region: 6× frost line to 100 AU
    // (For Sol, this is ~30-100 AU, matching the Kuiper belt + scattered disk)
    let inner = (frost_line_au * 6.0).max(10.0);
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

        let period_years = semi_major_axis.powf(1.5);
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
            PlanetType::Rocky => BodyType::Planet,
            PlanetType::IceGiant | PlanetType::GasGiant => BodyType::GasGiant,
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

    const EARTH_MASS_KG: f64 = 5.972e24;
    const EARTH_RADIUS_KM: f32 = 6371.0;

    let mass_kg = (planet_mass_earth as f64) * EARTH_MASS_KG;
    let radius_km = planet_radius_earth * EARTH_RADIUS_KM;

    // Check if planet can retain atmosphere
    if !AtmosphereComposition::can_retain_atmosphere(mass_kg, radius_km) {
        return None;
    }

    // Define habitable zone (conservative estimate: 0.75 - 1.5 AU scaled by luminosity)
    let hz_inner = 0.75 * (star_luminosity_sol as f64).sqrt();
    let hz_outer = 1.5 * (star_luminosity_sol as f64).sqrt();
    let in_habitable_zone = distance_au >= hz_inner && distance_au <= hz_outer;

    // Planet must be terrestrial-sized (0.3 - 3.0 Earth masses for rocky planets with atmospheres)
    // Larger planets become mini-Neptunes with thick H/He envelopes
    let is_terrestrial_size = planet_mass_earth >= 0.3 && planet_mass_earth <= 3.0;

    if !is_terrestrial_size {
        return None; // Too small or too large for Earth-like atmosphere
    }

    // Probability of having atmosphere increases for:
    // - Planets in habitable zone
    // - More massive planets (better retention)
    // - Planets not too close to star (atmospheric erosion)
    let base_probability = if in_habitable_zone { 0.8 } else { 0.5 };
    let mass_factor = (planet_mass_earth - 0.3) / 2.7; // 0 to 1 for 0.3-3.0 Earth masses
    let distance_factor = if distance_au < 0.3 {
        0.1 // Very close, strong stellar wind strips atmosphere
    } else if distance_au < 0.5 {
        0.4
    } else {
        1.0
    };

    let probability = base_probability * (0.5 + 0.5 * mass_factor) * distance_factor;

    if rng.random::<f32>() > probability {
        return None; // No atmosphere generated
    }

    // Generate atmospheric composition based on temperature and distance
    let (gases, pressure_mbar, greenhouse_factor) =
        if in_habitable_zone && equilibrium_temp_k > 250.0 && equilibrium_temp_k < 320.0 {
            // Earth-like atmosphere (20-50% chance for breathable)
            if rng.random::<f32>() < 0.35 {
                // Breathable atmosphere (Earth-like)
                (
                    vec![
                        AtmosphericGas::new("N2", 78.0 + rng.random_range(-3.0..3.0)),
                        AtmosphericGas::new("O2", 21.0 + rng.random_range(-2.0..2.0)),
                        AtmosphericGas::new("Ar", 0.93),
                        AtmosphericGas::new("CO2", 0.04 + rng.random_range(-0.02..0.1)),
                    ],
                    rng.random_range(800.0..1200.0), // Near Earth pressure
                    1.3,                             // Moderate greenhouse effect (~33K warming)
                )
            } else {
                // Thin atmosphere (Mars-like) or thick (Venus-lite)
                let is_thick = rng.random::<f32>() > 0.6;
                if is_thick {
                    // Thick CO2 atmosphere
                    (
                        vec![
                            AtmosphericGas::new("CO2", 95.0 + rng.random_range(-5.0..3.0)),
                            AtmosphericGas::new("N2", 3.0 + rng.random_range(-1.0..2.0)),
                            AtmosphericGas::new("Ar", 1.6),
                        ],
                        rng.random_range(2000.0..10000.0), // Thick atmosphere
                        1.8,                               // Strong greenhouse effect
                    )
                } else {
                    // Thin CO2 atmosphere (Mars-like)
                    (
                        vec![
                            AtmosphericGas::new("CO2", 95.0),
                            AtmosphericGas::new("N2", 2.7),
                            AtmosphericGas::new("Ar", 1.6),
                        ],
                        rng.random_range(5.0..15.0), // Very thin
                        1.05,                        // Minimal greenhouse effect
                    )
                }
            }
        } else if equilibrium_temp_k > 320.0 {
            // Hot planet - thick CO2/sulfur atmosphere
            (
                vec![
                    AtmosphericGas::new("CO2", 96.5),
                    AtmosphericGas::new("N2", 3.5),
                ],
                rng.random_range(5000.0..50000.0), // Very thick (Venus-like possible)
                2.0,                               // Very strong greenhouse effect
            )
        } else {
            // Cold planet - thin atmosphere
            (
                vec![
                    AtmosphericGas::new("N2", 80.0),
                    AtmosphericGas::new("CH4", 15.0 + rng.random_range(-5.0..5.0)),
                    AtmosphericGas::new("Ar", 5.0),
                ],
                rng.random_range(10.0..100.0), // Thin
                1.1,                           // Small greenhouse effect
            )
        };

    // Calculate surface temperature with greenhouse effect
    let surface_temp_k = equilibrium_temp_k * greenhouse_factor;
    let surface_temp_c = (surface_temp_k - 273.15) as f32;

    let atmosphere = AtmosphereComposition::new_with_body_data(
        pressure_mbar,
        surface_temp_c,
        gases,
        mass_kg,
        radius_km,
        false, // Not a reference pressure
    );

    Some((atmosphere, surface_temp_c))
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
            1.0,
            0, // No existing planets
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
            map_star_to_system_architecture("Test Star", 1.0, 2, &existing, &mut rng);

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
            // Rocky planets should have reasonable masses
            assert!(planet.mass_earth > 0.1 && planet.mass_earth < 10.0);
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

        // Too small: below 0.3 M⊕
        let result_small = generate_procedural_atmosphere(
            0.2,   // Below 0.3 M⊕ threshold
            0.6,   // Earth-like radius
            1.0,   // 1 AU
            1.0,   // Solar luminosity
            300.0, // Equilibrium temp
            &mut rng,
        );
        assert!(
            result_small.is_none(),
            "Planet below 0.3 M⊕ should not get atmosphere"
        );

        // Too large: above 3.0 M⊕
        let result_large = generate_procedural_atmosphere(
            3.5,   // Above 3.0 M⊕ threshold
            1.5,   // Larger radius
            1.0,   // 1 AU
            1.0,   // Solar luminosity
            300.0, // Equilibrium temp
            &mut rng,
        );
        assert!(
            result_large.is_none(),
            "Planet above 3.0 M⊕ should not get atmosphere"
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
