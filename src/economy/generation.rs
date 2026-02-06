use bevy::prelude::*;
use rand::Rng;

use super::components::{MineralDeposit, OrbitsBody, PlanetResources, StarSystem};
use super::types::ResourceType;
use crate::astronomy::SpaceCoordinates;
use crate::plugins::solar_system::{CelestialBody, Planet, DwarfPlanet, Moon, Asteroid, Comet};

/// Default frost line distance in Astronomical Units (for backwards compatibility)
/// Used when no StarSystem component is found (single-star legacy mode)
/// Beyond this distance, volatiles become more common
const DEFAULT_FROST_LINE_AU: f64 = 2.5;

/// System that generates resources for all celestial bodies on startup
/// Uses realistic accretion chemistry based on distance from parent star
/// Supports multiple star systems with different frost lines
pub fn generate_solar_system_resources(
    mut commands: Commands,
    // Query planets, dwarf planets, moons, asteroids, and comets without resources
    body_query: Query<
        (Entity, &CelestialBody, &SpaceCoordinates, Option<&OrbitsBody>),
        (
            Or<(With<Planet>, With<DwarfPlanet>, With<Moon>, With<Asteroid>, With<Comet>)>,
            Without<PlanetResources>,
        ),
    >,
    // Query for star systems to get frost line information
    star_query: Query<(&StarSystem, &SpaceCoordinates)>,
) {
    let mut rng = rand::thread_rng();

    for (entity, body, coords, orbits_body) in body_query.iter() {
        // Determine parent star and frost line
        let (distance_from_star, frost_line) = if let Some(orbits) = orbits_body {
            // Body orbits a specific parent - calculate distance from that parent
            if let Ok((star_system, star_coords)) = star_query.get(orbits.parent) {
                let distance = (coords.position - star_coords.position).length();
                (distance, star_system.frost_line_au)
            } else {
                // Parent entity exists but is not a star or doesn't have required components
                warn!(
                    "Parent star not found or invalid for {}, using origin distance and default frost line",
                    body.name
                );
                (coords.position.length(), DEFAULT_FROST_LINE_AU)
            }
        } else {
            // No parent specified - assume orbiting origin with default frost line
            // This maintains backwards compatibility with single-star systems
            (coords.position.length(), DEFAULT_FROST_LINE_AU)
        };

        info!(
            "Generating resources for {} at {:.2} AU (frost line: {:.2} AU)",
            body.name, distance_from_star, frost_line
        );

        // Generate resources based on distance from star and its frost line
        let resources = generate_resources_for_body(distance_from_star, frost_line, &mut rng);

        // Add resources component to entity
        commands.entity(entity).insert(resources);
    }
}

/// Generate resources for a celestial body based on its distance from parent star
/// Implements the frost line rule and realistic accretion chemistry
/// 
/// # Arguments
/// * `distance_au` - Distance from parent star in AU
/// * `frost_line_au` - Frost line distance for the parent star in AU
/// * `rng` - Random number generator for variability
fn generate_resources_for_body(distance_au: f64, frost_line_au: f64, rng: &mut impl Rng) -> PlanetResources {
    let mut resources = PlanetResources::new();

    // Determine if we're inside or outside the frost line
    let is_inner = distance_au < frost_line_au;

    // Generate each resource type
    for resource_type in ResourceType::all() {
        let deposit = generate_resource_deposit(*resource_type, distance_au, frost_line_au, is_inner, rng);
        
        // Only add deposits that have some presence
        if deposit.abundance > 0.001 || deposit.accessibility > 0.001 {
            resources.add_deposit(*resource_type, deposit);
        }
    }

    resources
}

/// Generate a single resource deposit based on location and resource type
/// 
/// # Arguments
/// * `resource` - The type of resource to generate
/// * `distance_au` - Distance from parent star in AU
/// * `frost_line_au` - Frost line distance for the parent star in AU
/// * `is_inner` - Whether the body is inside the frost line
/// * `rng` - Random number generator
fn generate_resource_deposit(
    resource: ResourceType,
    distance_au: f64,
    frost_line_au: f64,
    is_inner: bool,
    rng: &mut impl Rng,
) -> MineralDeposit {
    // Base probabilities and parameters
    let (base_abundance, base_accessibility) = match (resource, is_inner) {
        // Volatiles - HIGH in outer system, VERY LOW in inner system
        (r, false) if r.is_volatile() => (
            rng.gen_range(0.6..0.95),  // High abundance beyond frost line
            rng.gen_range(0.5..0.9),   // Good accessibility (ice on surface)
        ),
        (r, true) if r.is_volatile() => (
            rng.gen_range(0.0..0.05),  // Almost none in inner system
            rng.gen_range(0.0..0.1),   // Poor accessibility if any
        ),

        // Construction materials - HIGH in inner system, LOW accessibility in outer
        (r, true) if r.is_construction() => (
            rng.gen_range(0.5..0.9),   // High abundance in rocky planets
            rng.gen_range(0.6..0.95),  // Good accessibility (near surface)
        ),
        (r, false) if r.is_construction() => (
            rng.gen_range(0.2..0.5),   // Present but less concentrated
            rng.gen_range(0.1..0.3),   // Poor accessibility (buried under ice)
        ),

        // Noble gases - HIGH in outer system, trace in inner
        (r, false) if r.is_noble_gas() => (
            rng.gen_range(0.4..0.8),   // Good amounts in outer system
            rng.gen_range(0.3..0.7),   // Moderate accessibility (atmospheres)
        ),
        (r, true) if r.is_noble_gas() => (
            rng.gen_range(0.0..0.1),   // Trace amounts only
            rng.gen_range(0.1..0.3),   // Poor accessibility
        ),

        // Fissile materials - Rare everywhere, slightly better in inner system
        (r, true) if r.is_fissile() => (
            rng.gen_range(0.05..0.25), // Rare but present
            rng.gen_range(0.3..0.6),   // Moderate accessibility
        ),
        (r, false) if r.is_fissile() => (
            rng.gen_range(0.01..0.15), // Very rare
            rng.gen_range(0.1..0.3),   // Poor accessibility
        ),

        // Specialty materials - Varied distribution, slightly favor inner system
        (r, true) if r.is_specialty() => (
            rng.gen_range(0.1..0.4),   // Moderate abundance
            rng.gen_range(0.3..0.7),   // Moderate accessibility
        ),
        (r, false) if r.is_specialty() => (
            rng.gen_range(0.05..0.3),  // Lower abundance
            rng.gen_range(0.2..0.5),   // Harder to access
        ),

        // Fallback (shouldn't happen)
        _ => (0.0, 0.0),
    };

    // Apply distance modifiers for more nuanced distribution
    let distance_factor = calculate_distance_modifier(resource, distance_au, frost_line_au);
    let final_abundance = (base_abundance * distance_factor).clamp(0.0, 1.0);
    let final_accessibility = (base_accessibility * distance_factor as f32).clamp(0.0, 1.0);

    MineralDeposit::new(final_abundance, final_accessibility)
}

/// Calculate a distance-based modifier for resource abundance
/// Provides smooth transitions rather than sharp cutoffs at the frost line
/// 
/// # Arguments
/// * `resource` - The type of resource
/// * `distance_au` - Distance from parent star in AU
/// * `frost_line_au` - Frost line distance for the parent star in AU
fn calculate_distance_modifier(resource: ResourceType, distance_au: f64, frost_line_au: f64) -> f64 {
    match resource {
        // Volatiles increase with distance beyond frost line
        r if r.is_volatile() => {
            if distance_au > frost_line_au {
                (1.0 + (distance_au - frost_line_au) * 0.2).min(1.5)
            } else {
                (distance_au / frost_line_au).powf(2.0) // Sharp drop-off inside frost line
            }
        }

        // Construction materials decrease with distance
        r if r.is_construction() => {
            if distance_au < frost_line_au {
                1.0
            } else {
                (frost_line_au / distance_au).powf(0.5) // Gradual decrease
            }
        }

        // Noble gases favor outer system
        r if r.is_noble_gas() => {
            if distance_au > frost_line_au {
                1.0 + (distance_au - frost_line_au) * 0.1
            } else {
                0.3
            }
        }

        // Fissile materials slightly favor inner system
        r if r.is_fissile() => {
            if distance_au < frost_line_au {
                1.0
            } else {
                0.8
            }
        }

        // Specialty materials have complex distribution
        r if r.is_specialty() => {
            // Peak around optimal distance (scaled by frost line)
            // For Sun-like stars (frost_line ~2.5), optimal is ~1.5 AU
            let optimal_distance = frost_line_au * 0.6;
            1.0 - ((distance_au - optimal_distance).abs() * 0.15).min(0.6)
        }

        _ => 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_frost_line_constant() {
        assert_eq!(DEFAULT_FROST_LINE_AU, 2.5);
    }

    #[test]
    fn test_generate_resources_inner_system() {
        let mut rng = rand::thread_rng();
        let frost_line = 2.5;
        let resources = generate_resources_for_body(1.0, frost_line, &mut rng);

        // Inner system should have high construction materials
        assert!(resources.get_abundance(&ResourceType::Iron) > 0.3);
        // Inner system should have low volatiles
        assert!(resources.get_abundance(&ResourceType::Water) < 0.2);
    }

    #[test]
    fn test_generate_resources_outer_system() {
        let mut rng = rand::thread_rng();
        let frost_line = 2.5;
        let resources = generate_resources_for_body(5.0, frost_line, &mut rng);

        // Outer system should have high volatiles
        assert!(resources.get_abundance(&ResourceType::Water) > 0.4);
        assert!(resources.get_abundance(&ResourceType::Methane) > 0.4);
    }

    #[test]
    fn test_distance_modifier_volatiles() {
        let frost_line = 2.5;
        // Volatiles should increase beyond frost line
        let inner_modifier = calculate_distance_modifier(ResourceType::Water, 1.0, frost_line);
        let outer_modifier = calculate_distance_modifier(ResourceType::Water, 5.0, frost_line);
        assert!(outer_modifier > inner_modifier);
    }

    #[test]
    fn test_distance_modifier_construction() {
        let frost_line = 2.5;
        // Construction materials should decrease beyond frost line
        let inner_modifier = calculate_distance_modifier(ResourceType::Iron, 1.0, frost_line);
        let outer_modifier = calculate_distance_modifier(ResourceType::Iron, 5.0, frost_line);
        assert!(inner_modifier > outer_modifier);
    }

    #[test]
    fn test_resource_deposit_bounds() {
        let mut rng = rand::thread_rng();
        let frost_line = 2.5;
        let deposit = generate_resource_deposit(ResourceType::Iron, 1.0, frost_line, true, &mut rng);
        
        // Values should be within valid ranges
        assert!(deposit.abundance >= 0.0 && deposit.abundance <= 1.0);
        assert!(deposit.accessibility >= 0.0 && deposit.accessibility <= 1.0);
    }
    
    #[test]
    fn test_different_frost_lines() {
        let mut rng = rand::thread_rng();
        
        // Test with a cooler star (M-type) with closer frost line
        let m_star_frost_line = 0.5;
        let resources_m = generate_resources_for_body(1.0, m_star_frost_line, &mut rng);
        
        // At 1.0 AU from an M-star, we're well beyond the frost line
        // So we should have high volatiles
        assert!(resources_m.get_abundance(&ResourceType::Water) > 0.3);
        
        // Test with a hotter star (A-type) with farther frost line
        let a_star_frost_line = 5.0;
        let resources_a = generate_resources_for_body(1.0, a_star_frost_line, &mut rng);
        
        // At 1.0 AU from an A-star, we're well inside the frost line
        // So we should have high construction materials
        assert!(resources_a.get_abundance(&ResourceType::Iron) > 0.3);
    }
}