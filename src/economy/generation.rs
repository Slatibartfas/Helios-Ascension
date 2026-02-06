use bevy::prelude::*;
use rand::Rng;

use super::components::{MineralDeposit, OrbitsBody, PlanetResources, StarSystem};
use super::types::ResourceType;
use crate::astronomy::SpaceCoordinates;
use crate::plugins::solar_system::{CelestialBody, Planet, DwarfPlanet, Moon, Asteroid, Comet};
use crate::plugins::solar_system_data::AsteroidClass;

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

        // Generate resources based on distance from star, body characteristics, and frost line
        let resources = generate_resources_for_body(
            &body.name, 
            body.body_type,
            distance_from_star, 
            frost_line, 
            &mut rng
        );

        // Add resources component to entity
        commands.entity(entity).insert(resources);
    }
}

/// Generate resources for a celestial body based on its distance from parent star
/// Implements the frost line rule, realistic accretion chemistry, and body-specific profiles
/// 
/// # Arguments
/// * `body_name` - Name of the celestial body (for special cases like Europa, Mars)
/// * `body_type` - Type of the body (Planet, Moon, Asteroid, etc.)
/// * `distance_au` - Distance from parent star in AU
/// * `frost_line_au` - Frost line distance for the parent star in AU
/// * `rng` - Random number generator for variability
fn generate_resources_for_body(
    body_name: &str,
    body_type: crate::plugins::solar_system_data::BodyType,
    distance_au: f64, 
    frost_line_au: f64, 
    rng: &mut impl Rng
) -> PlanetResources {
    let mut resources = PlanetResources::new();

    // Check for special body-specific resource profiles
    if let Some(special_resources) = apply_special_body_profile(body_name, rng) {
        return special_resources;
    }

    // Determine if we're inside or outside the frost line
    let is_inner = distance_au < frost_line_au;

    // For asteroids, determine if this is a specialized asteroid
    let asteroid_specialization = if matches!(body_type, crate::plugins::solar_system_data::BodyType::Asteroid) {
        determine_asteroid_specialization(body_name, rng)
    } else {
        AsteroidSpecialization::None
    };

    // Generate each resource type
    for resource_type in ResourceType::all() {
        // Skip some resources randomly for variation (30-50% chance to skip non-critical resources)
        if !resource_type.is_critical() && rng.gen_bool(0.4) {
            continue;
        }

        let mut deposit = generate_resource_deposit(
            *resource_type, 
            distance_au, 
            frost_line_au, 
            is_inner, 
            rng
        );

        // Apply asteroid specialization modifiers
        deposit = apply_asteroid_specialization(deposit, *resource_type, &asteroid_specialization);

        // Only add deposits that have some presence
        if deposit.abundance > 0.0001 || deposit.accessibility > 0.001 {
            resources.add_deposit(*resource_type, deposit);
        }
    }

    resources
}

/// Asteroid specialization types for concentrated resource deposits
#[derive(Debug, Clone, Copy)]
enum AsteroidSpecialization {
    None,
    PlatinumRich,  // Mostly platinum group metals
    GoldRich,      // High gold and silver content
    IronNickel,    // Metallic asteroid with iron/nickel
    Carbonaceous,  // High volatiles even in inner system
}

/// Apply special resource profiles for known celestial bodies
/// Returns Some(resources) for special bodies, None for normal generation
fn apply_special_body_profile(body_name: &str, rng: &mut impl Rng) -> Option<PlanetResources> {
    let mut resources = PlanetResources::new();
    
    match body_name {
        // Europa: Massive subsurface ocean (80-90% water)
        "Europa" => {
            resources.add_deposit(ResourceType::Water, MineralDeposit::new(0.85, 0.4)); // Massive water, but under ice
            resources.add_deposit(ResourceType::Oxygen, MineralDeposit::new(0.05, 0.3)); // Some O2 in ice
            resources.add_deposit(ResourceType::Silicates, MineralDeposit::new(0.08, 0.2)); // Rocky core
            resources.add_deposit(ResourceType::Iron, MineralDeposit::new(0.02, 0.1)); // Small iron core
            info!("Applied Europa special profile: massive water ocean");
            Some(resources)
        }
        
        // Mars: Polar ice caps and subsurface ice
        "Mars" => {
            resources.add_deposit(ResourceType::Water, MineralDeposit::new(0.15, 0.5)); // Significant ice caps
            resources.add_deposit(ResourceType::CarbonDioxide, MineralDeposit::new(0.08, 0.7)); // CO2 ice caps
            resources.add_deposit(ResourceType::Iron, MineralDeposit::new(0.25, 0.7)); // Oxidized iron (rust)
            resources.add_deposit(ResourceType::Silicates, MineralDeposit::new(0.35, 0.8));
            resources.add_deposit(ResourceType::Aluminum, MineralDeposit::new(0.08, 0.6));
            resources.add_deposit(ResourceType::Nitrogen, MineralDeposit::new(0.02, 0.4)); // Thin atmosphere
            info!("Applied Mars special profile: ice caps and oxidized surface");
            Some(resources)
        }
        
        // Moon (Earth's): Water ice in permanently shadowed craters
        "Moon" => {
            resources.add_deposit(ResourceType::Water, MineralDeposit::new(0.05, 0.3)); // Polar ice in craters
            resources.add_deposit(ResourceType::Oxygen, MineralDeposit::new(0.02, 0.4)); // Bound in regolith
            resources.add_deposit(ResourceType::Silicates, MineralDeposit::new(0.45, 0.8));
            resources.add_deposit(ResourceType::Iron, MineralDeposit::new(0.15, 0.6));
            resources.add_deposit(ResourceType::Aluminum, MineralDeposit::new(0.10, 0.7));
            resources.add_deposit(ResourceType::Titanium, MineralDeposit::new(0.008, 0.5));
            resources.add_deposit(ResourceType::Helium3, MineralDeposit::new(0.00000001, 0.8)); // Solar wind implanted
            info!("Applied Moon special profile: polar ice and regolith resources");
            Some(resources)
        }
        
        // Titan: Hydrocarbon-rich moon
        "Titan" => {
            resources.add_deposit(ResourceType::Methane, MineralDeposit::new(0.45, 0.9)); // Methane lakes
            resources.add_deposit(ResourceType::Nitrogen, MineralDeposit::new(0.35, 0.8)); // Thick N2 atmosphere
            resources.add_deposit(ResourceType::Ammonia, MineralDeposit::new(0.08, 0.6));
            resources.add_deposit(ResourceType::Water, MineralDeposit::new(0.10, 0.3)); // Water ice crust
            resources.add_deposit(ResourceType::Silicates, MineralDeposit::new(0.02, 0.2));
            info!("Applied Titan special profile: hydrocarbon lakes and thick atmosphere");
            Some(resources)
        }
        
        // Enceladus: Water geysers
        "Enceladus" => {
            resources.add_deposit(ResourceType::Water, MineralDeposit::new(0.75, 0.9)); // Active geysers make water very accessible
            resources.add_deposit(ResourceType::Nitrogen, MineralDeposit::new(0.05, 0.7));
            resources.add_deposit(ResourceType::Ammonia, MineralDeposit::new(0.03, 0.6));
            resources.add_deposit(ResourceType::Silicates, MineralDeposit::new(0.15, 0.4));
            resources.add_deposit(ResourceType::Iron, MineralDeposit::new(0.02, 0.3));
            info!("Applied Enceladus special profile: water geysers");
            Some(resources)
        }
        
        // Ceres: Dwarf planet with significant water ice
        "Ceres" => {
            resources.add_deposit(ResourceType::Water, MineralDeposit::new(0.40, 0.6));
            resources.add_deposit(ResourceType::Ammonia, MineralDeposit::new(0.08, 0.5));
            resources.add_deposit(ResourceType::Silicates, MineralDeposit::new(0.35, 0.7));
            resources.add_deposit(ResourceType::Iron, MineralDeposit::new(0.12, 0.5));
            resources.add_deposit(ResourceType::Copper, MineralDeposit::new(0.0001, 0.4));
            info!("Applied Ceres special profile: water-rich dwarf planet");
            Some(resources)
        }
        
        _ => None, // Normal generation for other bodies
    }
}

/// Determine if an asteroid should have specialized resources
fn determine_asteroid_specialization(body_name: &str, rng: &mut impl Rng) -> AsteroidSpecialization {
    // Check for M-type asteroids (metallic) - should be rich in metals
    if body_name.contains("Psyche") || (body_name.contains("M-type") || body_name.contains("metallic")) {
        return AsteroidSpecialization::IronNickel;
    }
    
    // 15% chance for any asteroid to be specialized
    let roll = rng.gen_range(0.0..1.0);
    if roll < 0.03 {
        AsteroidSpecialization::PlatinumRich
    } else if roll < 0.06 {
        AsteroidSpecialization::GoldRich
    } else if roll < 0.10 {
        AsteroidSpecialization::IronNickel
    } else if roll < 0.15 {
        AsteroidSpecialization::Carbonaceous
    } else {
        AsteroidSpecialization::None
    }
}

/// Apply specialization modifiers to a resource deposit
fn apply_asteroid_specialization(
    mut deposit: MineralDeposit, 
    resource: ResourceType,
    specialization: &AsteroidSpecialization
) -> MineralDeposit {
    match specialization {
        AsteroidSpecialization::PlatinumRich => {
            match resource {
                ResourceType::Platinum => {
                    // 100-500x normal abundance for platinum-rich asteroids
                    deposit.abundance *= 250.0;
                    deposit.abundance = deposit.abundance.min(0.05); // Cap at 5%
                    deposit.accessibility = (deposit.accessibility * 1.5).min(0.95);
                }
                ResourceType::Silver | ResourceType::Gold => {
                    // 10-50x for other precious metals
                    deposit.abundance *= 20.0;
                    deposit.abundance = deposit.abundance.min(0.01);
                }
                // Reduce non-precious metal resources
                _ if !resource.is_precious_metal() => {
                    deposit.abundance *= 0.2;
                }
                _ => {}
            }
        }
        
        AsteroidSpecialization::GoldRich => {
            match resource {
                ResourceType::Gold => {
                    // 100-500x normal abundance
                    deposit.abundance *= 200.0;
                    deposit.abundance = deposit.abundance.min(0.03); // Cap at 3%
                    deposit.accessibility = (deposit.accessibility * 1.5).min(0.95);
                }
                ResourceType::Silver => {
                    // 50x for silver
                    deposit.abundance *= 50.0;
                    deposit.abundance = deposit.abundance.min(0.015);
                }
                ResourceType::Copper => {
                    // 10x for copper
                    deposit.abundance *= 10.0;
                    deposit.abundance = deposit.abundance.min(0.005);
                }
                _ if !resource.is_precious_metal() && !resource.is_specialty() => {
                    deposit.abundance *= 0.2;
                }
                _ => {}
            }
        }
        
        AsteroidSpecialization::IronNickel => {
            match resource {
                ResourceType::Iron => {
                    // Metallic asteroids are 70-90% iron
                    deposit.abundance = deposit.abundance.max(0.70);
                    deposit.accessibility = (deposit.accessibility * 1.3).min(0.98);
                }
                ResourceType::Platinum | ResourceType::Gold | ResourceType::Silver => {
                    // 10-20x precious metals in metallic asteroids
                    deposit.abundance *= 15.0;
                    deposit.abundance = deposit.abundance.min(0.005);
                }
                ResourceType::Copper => {
                    // 5x copper
                    deposit.abundance *= 5.0;
                }
                // Very little of everything else
                _ if resource.is_volatile() || resource.is_atmospheric_gas() => {
                    deposit.abundance *= 0.01;
                }
                _ => {}
            }
        }
        
        AsteroidSpecialization::Carbonaceous => {
            match resource {
                ResourceType::Water | ResourceType::Ammonia | ResourceType::Methane => {
                    // High volatiles even in inner system
                    deposit.abundance = deposit.abundance.max(0.20);
                    deposit.accessibility = (deposit.accessibility * 1.2).min(0.90);
                }
                ResourceType::Hydrogen => {
                    deposit.abundance = deposit.abundance.max(0.15);
                }
                ResourceType::CarbonDioxide | ResourceType::Nitrogen => {
                    deposit.abundance *= 3.0;
                }
                _ => {}
            }
        }
        
        AsteroidSpecialization::None => {
            // No modifications
        }
    }
    
    deposit
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
    // Note: Abundance values are more realistic now to support mining depletion
    // Values represent fraction of body composition, not absolute amounts
    let (base_abundance, base_accessibility) = match (resource, is_inner) {
        // Volatiles - HIGH in outer system, VERY LOW in inner system
        (r, false) if r.is_volatile() => (
            rng.gen_range(0.3..0.7),   // Realistic ice composition
            rng.gen_range(0.5..0.9),   // Good accessibility (ice on surface)
        ),
        (r, true) if r.is_volatile() => (
            rng.gen_range(0.0..0.02),  // Almost none in inner system
            rng.gen_range(0.0..0.1),   // Poor accessibility if any
        ),

        // Atmospheric gases - Present in atmospheres and trapped in ice
        (r, false) if r.is_atmospheric_gas() => (
            rng.gen_range(0.1..0.4),   // Moderate in outer system (trapped in ice)
            rng.gen_range(0.4..0.8),   // Moderate-good accessibility
        ),
        (r, true) if r.is_atmospheric_gas() => (
            rng.gen_range(0.0..0.15),  // Trace to moderate in atmospheres
            rng.gen_range(0.2..0.6),   // Variable accessibility (atmospheric mining)
        ),

        // Construction materials - HIGH in inner system, present in outer
        // More realistic abundances based on actual planetary composition
        (r, true) if r.is_construction() => {
            let abundance = match resource {
                ResourceType::Iron => rng.gen_range(0.15..0.35),      // ~30% of Earth's composition
                ResourceType::Silicates => rng.gen_range(0.25..0.45), // Major component
                ResourceType::Aluminum => rng.gen_range(0.05..0.12),  // ~8% of crust
                ResourceType::Titanium => rng.gen_range(0.003..0.01), // ~0.6% of crust
                _ => rng.gen_range(0.1..0.3),
            };
            (abundance, rng.gen_range(0.6..0.95)) // Good accessibility (near surface)
        },
        (r, false) if r.is_construction() => (
            rng.gen_range(0.05..0.2),  // Present but less concentrated
            rng.gen_range(0.1..0.3),   // Poor accessibility (buried under ice)
        ),

        // Noble gases - He3 is very rare but valuable for fusion
        (r, false) if r.is_noble_gas() => (
            rng.gen_range(0.00001..0.0001), // Extremely rare He3
            rng.gen_range(0.3..0.7),        // Moderate accessibility
        ),
        (r, true) if r.is_noble_gas() => (
            rng.gen_range(0.000001..0.00001), // Trace He3 in inner system
            rng.gen_range(0.1..0.3),          // Poor accessibility
        ),

        // Fissile materials - Rare everywhere, more realistic abundances
        (r, true) if r.is_fissile() => {
            let abundance = match resource {
                ResourceType::Uranium => rng.gen_range(0.000001..0.00001),  // ~3 ppm in Earth's crust
                ResourceType::Thorium => rng.gen_range(0.000003..0.00003),  // ~12 ppm in Earth's crust
                _ => rng.gen_range(0.00001..0.0001),
            };
            (abundance, rng.gen_range(0.3..0.6)) // Moderate accessibility
        },
        (r, false) if r.is_fissile() => (
            rng.gen_range(0.0000001..0.000001), // Very rare in outer system
            rng.gen_range(0.1..0.3),            // Poor accessibility
        ),

        // Precious metals - Very rare but valuable
        (r, true) if r.is_precious_metal() => {
            let abundance = match resource {
                ResourceType::Gold => rng.gen_range(0.0000001..0.000001),     // ~0.004 ppm in crust
                ResourceType::Silver => rng.gen_range(0.0000003..0.000003),   // ~0.08 ppm in crust
                ResourceType::Platinum => rng.gen_range(0.00000001..0.0000001), // ~0.005 ppb in crust
                _ => rng.gen_range(0.0000001..0.000001),
            };
            (abundance, rng.gen_range(0.2..0.5)) // Harder to access (concentrated deposits)
        },
        (r, false) if r.is_precious_metal() => (
            rng.gen_range(0.0000001..0.000001), // Rare in outer system too
            rng.gen_range(0.1..0.3),            // Poor accessibility
        ),

        // Specialty materials - Moderate rarity
        (r, true) if r.is_specialty() => {
            let abundance = match resource {
                ResourceType::Copper => rng.gen_range(0.00003..0.0001),    // ~60 ppm in crust
                ResourceType::RareEarths => rng.gen_range(0.00005..0.0002), // Variable, ~200 ppm combined
                _ => rng.gen_range(0.0001..0.001),
            };
            (abundance, rng.gen_range(0.3..0.7)) // Moderate accessibility
        },
        (r, false) if r.is_specialty() => (
            rng.gen_range(0.00001..0.0001), // Lower abundance
            rng.gen_range(0.2..0.5),        // Harder to access
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

        // Atmospheric gases are present everywhere but more in outer system
        r if r.is_atmospheric_gas() => {
            if distance_au > frost_line_au {
                1.0 + (distance_au - frost_line_au) * 0.15
            } else {
                0.8 // Still present in inner system atmospheres
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

        // Noble gases (He3) favor outer system but very rare
        r if r.is_noble_gas() => {
            if distance_au > frost_line_au {
                1.0 + (distance_au - frost_line_au) * 0.1
            } else {
                0.2
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

        // Precious metals have complex distribution - can be concentrated in asteroids
        r if r.is_precious_metal() => {
            // Peak in asteroid belt region (around frost line)
            let optimal_distance = frost_line_au * 1.2;
            let distance_diff = (distance_au - optimal_distance).abs();
            1.0 - (distance_diff * 0.1).min(0.5) // Less penalty for distance
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
        let resources = generate_resources_for_body(
            "TestBody", 
            crate::plugins::solar_system_data::BodyType::Planet,
            1.0, 
            frost_line, 
            &mut rng
        );

        // Inner system should have construction materials (when present)
        // Note: Due to random resource skipping, not all resources may be present
        let iron = resources.get_abundance(&ResourceType::Iron);
        if iron > 0.0 {
            assert!(iron > 0.05);
        }
    }

    #[test]
    fn test_generate_resources_outer_system() {
        let mut rng = rand::thread_rng();
        let frost_line = 2.5;
        let resources = generate_resources_for_body(
            "TestBody",
            crate::plugins::solar_system_data::BodyType::Planet,
            5.0, 
            frost_line, 
            &mut rng
        );

        // Outer system should have volatiles (when present)
        let water = resources.get_abundance(&ResourceType::Water);
        let methane = resources.get_abundance(&ResourceType::Methane);
        
        // At least one volatile should be present with decent abundance
        assert!(water > 0.1 || methane > 0.1);
    }

    #[test]
    fn test_europa_special_profile() {
        let mut rng = rand::thread_rng();
        let resources = generate_resources_for_body(
            "Europa",
            crate::plugins::solar_system_data::BodyType::Moon,
            5.2, 
            2.5, 
            &mut rng
        );

        // Europa should have massive water abundance
        let water = resources.get_abundance(&ResourceType::Water);
        assert!(water > 0.7, "Europa should have >70% water");
    }

    #[test]
    fn test_mars_special_profile() {
        let mut rng = rand::thread_rng();
        let resources = generate_resources_for_body(
            "Mars",
            crate::plugins::solar_system_data::BodyType::Planet,
            1.52, 
            2.5, 
            &mut rng
        );

        // Mars should have ice caps and CO2
        let water = resources.get_abundance(&ResourceType::Water);
        let co2 = resources.get_abundance(&ResourceType::CarbonDioxide);
        
        assert!(water > 0.1, "Mars should have significant water ice");
        assert!(co2 > 0.05, "Mars should have CO2 ice caps");
    }

    #[test]
    fn test_atmospheric_gases_present() {
        let mut rng = rand::thread_rng();
        let frost_line = 2.5;
        let resources = generate_resources_for_body(
            "TestBody",
            crate::plugins::solar_system_data::BodyType::Planet,
            1.0, 
            frost_line, 
            &mut rng
        );

        // At least some atmospheric gases should be present (not all skipped)
        let total_atm = resources.get_abundance(&ResourceType::Nitrogen)
            + resources.get_abundance(&ResourceType::Oxygen)
            + resources.get_abundance(&ResourceType::CarbonDioxide)
            + resources.get_abundance(&ResourceType::Argon);
        
        // Should have at least one atmospheric gas
        assert!(total_atm > 0.0);
    }

    #[test]
    fn test_precious_metals_rare() {
        let mut rng = rand::thread_rng();
        let frost_line = 2.5;
        let resources = generate_resources_for_body(
            "TestBody",
            crate::plugins::solar_system_data::BodyType::Planet,
            1.0, 
            frost_line, 
            &mut rng
        );

        // Precious metals should be very rare (when present)
        let gold = resources.get_abundance(&ResourceType::Gold);
        let platinum = resources.get_abundance(&ResourceType::Platinum);
        
        if gold > 0.0 {
            assert!(gold < 0.00001, "Gold should be extremely rare");
        }
        if platinum > 0.0 {
            assert!(platinum < 0.000001, "Platinum should be extremely rare");
        }
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
        let resources_m = generate_resources_for_body(
            "TestBody",
            crate::plugins::solar_system_data::BodyType::Planet,
            1.0, 
            m_star_frost_line, 
            &mut rng
        );
        
        // At 1.0 AU from an M-star, we're well beyond the frost line
        // So we should have some volatiles present
        let water = resources_m.get_abundance(&ResourceType::Water);
        if water > 0.0 {
            assert!(water > 0.2, "Should have decent water abundance beyond frost line");
        }
        
        // Test with a hotter star (A-type) with farther frost line
        let a_star_frost_line = 5.0;
        let resources_a = generate_resources_for_body(
            "TestBody",
            crate::plugins::solar_system_data::BodyType::Planet,
            1.0, 
            a_star_frost_line, 
            &mut rng
        );
        
        // At 1.0 AU from an A-star, we're well inside the frost line
        // So we should have construction materials when present
        let iron = resources_a.get_abundance(&ResourceType::Iron);
        if iron > 0.0 {
            assert!(iron > 0.1, "Should have decent iron abundance inside frost line");
        }
    }
}