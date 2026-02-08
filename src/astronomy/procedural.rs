//! Procedural generation system for star systems
//! 
//! This module implements the "gap-filler" logic to populate star systems with
//! procedurally generated planets, asteroid belts, and cometary clouds when
//! real data is incomplete or unavailable.

use bevy::prelude::*;
use rand::prelude::*;
use std::f64::consts::PI;

use super::components::{KeplerOrbit, SpaceCoordinates};
use crate::plugins::solar_system_data::{AsteroidClass, BodyType};

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
    Rocky,      // Inner system, terrestrial composition
    IceGiant,   // Outer system, ice-rich
    GasGiant,   // Outer system, gas-rich
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
    
    info!(
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
        // Inner system: 2-4 rocky planets
        // Outer system: 1-3 gas/ice giants
        
        let inner_count = rng.gen_range(2..=4.min(planets_needed));
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
        gas_giants = generate_gas_giants(
            star_name,
            outer_count,
            frost_line_au,
            existing_orbits_au,
            rng,
        );
    }
    
    // Generate asteroid belt (inside or near frost line)
    let asteroid_belt = if rng.gen_bool(0.8) {
        // 80% chance of asteroid belt
        Some(generate_asteroid_belt(frost_line_au, existing_orbits_au, rng))
    } else {
        None
    };
    
    // Generate cometary cloud (far outer system)
    let cometary_cloud = if rng.gen_bool(0.7) {
        // 70% chance of cometary cloud
        Some(generate_cometary_cloud(frost_line_au, rng))
    } else {
        None
    };
    
    SystemArchitecture {
        frost_line_au,
        rocky_planets,
        asteroid_belt,
        gas_giants,
        cometary_cloud,
    }
}

/// Generate rocky planets for the inner system
fn generate_rocky_planets(
    star_name: &str,
    count: usize,
    frost_line_au: f64,
    existing_orbits_au: &[f64],
    rng: &mut impl Rng,
) -> Vec<ProceduralPlanet> {
    let mut planets = Vec::new();
    
    // Inner system range: 0.3 AU to frost line
    let inner_min = 0.3;
    let inner_max = frost_line_au * 0.95; // Stay just inside frost line
    
    for i in 0..count {
        // Space planets roughly evenly, with some randomness
        let base_orbit = inner_min + (inner_max - inner_min) * (i as f64 + 0.5) / (count as f64);
        let variation = rng.gen_range(-0.15..0.15);
        let mut semi_major_axis = base_orbit * (1.0 + variation);
        
        // Avoid collisions with existing planets (need at least 0.1 AU separation)
        while is_too_close_to_existing(semi_major_axis, existing_orbits_au, 0.1) {
            semi_major_axis += rng.gen_range(0.05..0.15);
        }
        
        // Calculate orbital period using Kepler's third law
        // T² = a³ (for solar masses)
        let period_years = semi_major_axis.powf(1.5);
        let period_days = period_years * 365.25;
        
        let planet = ProceduralPlanet {
            name: format!("{} {}", star_name, char::from_u32('b' as u32 + i as u32).unwrap_or('?')),
            semi_major_axis_au: semi_major_axis,
            eccentricity: rng.gen_range(0.0..0.15), // Rocky planets tend to have low eccentricity
            inclination: rng.gen_range(-0.05..0.05), // Low inclination
            longitude_ascending_node: rng.gen_range(0.0..std::f64::consts::TAU),
            argument_of_periapsis: rng.gen_range(0.0..std::f64::consts::TAU),
            mean_anomaly_epoch: rng.gen_range(0.0..std::f64::consts::TAU),
            period_days,
            mass_earth: rng.gen_range(0.3..3.5), // Sub-Earth to Super-Earth
            radius_earth: rng.gen_range(0.7..1.8),
            planet_type: PlanetType::Rocky,
        };
        
        planets.push(planet);
    }
    
    planets
}

/// Generate gas and ice giants for the outer system
fn generate_gas_giants(
    star_name: &str,
    count: usize,
    frost_line_au: f64,
    existing_orbits_au: &[f64],
    rng: &mut impl Rng,
) -> Vec<ProceduralPlanet> {
    let mut planets = Vec::new();
    
    // Outer system range: frost line to ~30 AU
    let outer_min = frost_line_au * 1.2; // Start just beyond frost line
    let outer_max = 30.0;
    
    for i in 0..count {
        // Space planets with increasing separation (logarithmic spacing)
        let t = (i as f64 + 0.5) / (count as f64);
        let base_orbit = outer_min * (outer_max / outer_min).powf(t);
        let variation = rng.gen_range(-0.15..0.15);
        let mut semi_major_axis = base_orbit * (1.0 + variation);
        
        // Avoid collisions with existing planets (need at least 0.5 AU separation for giants)
        while is_too_close_to_existing(semi_major_axis, existing_orbits_au, 0.5) {
            semi_major_axis += rng.gen_range(0.3..0.8);
        }
        
        // Calculate orbital period using Kepler's third law
        let period_years = semi_major_axis.powf(1.5);
        let period_days = period_years * 365.25;
        
        // Determine if this is a gas giant or ice giant
        // Ice giants are more common at moderate distances, gas giants further out
        let planet_type = if semi_major_axis < frost_line_au * 3.0 && rng.gen_bool(0.6) {
            PlanetType::IceGiant
        } else {
            PlanetType::GasGiant
        };
        
        let (mass_range, radius_range) = match planet_type {
            PlanetType::IceGiant => ((10.0, 25.0), (3.5, 4.5)), // Neptune-like
            PlanetType::GasGiant => ((50.0, 400.0), (8.0, 12.0)), // Jupiter-like
            _ => unreachable!(),
        };
        
        let planet = ProceduralPlanet {
            name: format!("{} {}", star_name, char::from_u32('b' as u32 + existing_orbits_au.len() as u32 + i as u32).unwrap_or('?')),
            semi_major_axis_au: semi_major_axis,
            eccentricity: rng.gen_range(0.0..0.25), // Giants can have moderate eccentricity
            inclination: rng.gen_range(-0.08..0.08), // Moderate inclination
            longitude_ascending_node: rng.gen_range(0.0..std::f64::consts::TAU),
            argument_of_periapsis: rng.gen_range(0.0..std::f64::consts::TAU),
            mean_anomaly_epoch: rng.gen_range(0.0..std::f64::consts::TAU),
            period_days,
            mass_earth: rng.gen_range(mass_range.0..mass_range.1),
            radius_earth: rng.gen_range(radius_range.0..radius_range.1),
            planet_type,
        };
        
        planets.push(planet);
    }
    
    planets
}

/// Generate an asteroid belt
fn generate_asteroid_belt(
    frost_line_au: f64,
    existing_orbits_au: &[f64],
    rng: &mut impl Rng,
) -> AsteroidBelt {
    // Belt typically at 1.5-2.5 × frost line distance
    let base_center = frost_line_au * 2.0;
    
    // Find a clear zone for the belt
    let mut inner = base_center * 0.7;
    let mut outer = base_center * 1.3;
    
    // Adjust if too close to existing planets
    for &orbit in existing_orbits_au {
        if (orbit - base_center).abs() < 1.0 {
            // Shift the belt
            if orbit < base_center {
                inner = orbit + 0.3;
                outer = inner + (base_center * 0.6);
            } else {
                outer = orbit - 0.3;
                inner = outer - (base_center * 0.6);
            }
        }
    }
    
    AsteroidBelt {
        inner_au: inner,
        outer_au: outer,
        count: rng.gen_range(50..200), // Number of asteroids to spawn
        inclination: rng.gen_range(0.0..0.1), // Low to moderate inclination
    }
}

/// Generate a cometary cloud
fn generate_cometary_cloud(frost_line_au: f64, rng: &mut impl Rng) -> CometaryCloud {
    // Cloud at outer reaches of system (20-50 AU)
    let inner = 20.0_f64.max(frost_line_au * 4.0);
    let outer = 50.0;
    
    CometaryCloud {
        inner_au: inner,
        outer_au: outer,
        count: rng.gen_range(20..80), // Fewer but more visible comets
        inclination: rng.gen_range(0.0..PI / 3.0), // High inclination (spherical distribution)
    }
}

/// Check if a proposed orbit is too close to existing planets
fn is_too_close_to_existing(
    proposed_au: f64,
    existing_orbits_au: &[f64],
    min_separation: f64,
) -> bool {
    for &existing in existing_orbits_au {
        if (proposed_au - existing).abs() < min_separation {
            return true;
        }
    }
    false
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

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

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
        
        let architecture = map_star_to_system_architecture(
            "Test Star",
            1.0,
            2,
            &existing,
            &mut rng,
        );
        
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
        
        assert_eq!(planets.len(), 3);
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
        
        let planets = generate_gas_giants("Test", 2, frost_line, &[], &mut rng);
        
        assert_eq!(planets.len(), 2);
        for planet in &planets {
            // All giants should be outside the frost line
            assert!(planet.semi_major_axis_au > frost_line);
            assert!(planet.planet_type == PlanetType::GasGiant || planet.planet_type == PlanetType::IceGiant);
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
}
