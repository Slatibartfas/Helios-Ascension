use bevy::prelude::*;
use rand::Rng;

use super::components::{MineralDeposit, OrbitsBody, PlanetResources, StarSystem};
use super::types::ResourceType;
use crate::astronomy::SpaceCoordinates;
use crate::plugins::solar_system::{CelestialBody, Planet, DwarfPlanet, Moon, Asteroid, Comet};
use crate::plugins::solar_system_data::{AsteroidClass, BodyType};

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
            body.mass,
            body.asteroid_class,
            distance_from_star, 
            frost_line, 
            &mut rng
        );

        // Add resources component to entity
        commands.entity(entity).insert(resources);
    }
}

/// Generate resources for a celestial body based on its distance from parent star
/// Implements the frost line rule, realistic accretion chemistry, body-specific profiles,
/// and scientific spectral class mapping for asteroids
/// 
/// # Arguments
/// * `body_name` - Name of the celestial body (for special cases like Europa, Mars)
/// * `body_type` - Type of the body (Planet, Moon, Asteroid, etc.)
/// * `body_mass` - Mass of the body in kg
/// * `asteroid_class` - Spectral class for asteroids (C, S, M, V, D, P types)
/// * `distance_au` - Distance from parent star in AU
/// * `frost_line_au` - Frost line distance for the parent star in AU
/// * `rng` - Random number generator for variability
fn generate_resources_for_body(
    body_name: &str,
    body_type: crate::plugins::solar_system_data::BodyType,
    body_mass: f64,
    asteroid_class: Option<AsteroidClass>,
    distance_au: f64, 
    frost_line_au: f64, 
    rng: &mut impl Rng
) -> PlanetResources {
    let mut resources = PlanetResources::new();

    // Check for special body-specific resource profiles
    if let Some(special_resources) = apply_special_body_profile(body_name, body_mass, rng) {
        return special_resources;
    }

    // For asteroids with known spectral class, apply scientific resource mapping
    if matches!(body_type, crate::plugins::solar_system_data::BodyType::Asteroid) {
        if let Some(class) = asteroid_class {
            if let Some(spectral_resources) = apply_spectral_class_profile(class, body_name, body_mass, distance_au, frost_line_au, rng) {
                return spectral_resources;
            }
        }
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
            body_mass,
            body_type,
            distance_au, 
            frost_line_au, 
            is_inner, 
            rng
        );

        // Apply asteroid specialization modifiers
        deposit = apply_asteroid_specialization(deposit, *resource_type, &asteroid_specialization);

        // Only add deposits that have some presence
        if deposit.is_viable() {
            resources.add_deposit(*resource_type, deposit);
        }
    }

    resources
}

/// Helper to create a tiered deposit from legacy parameters
fn create_deposit_legacy(abundance: f64, accessibility: f32, body_mass_kg: f64, body_type: BodyType) -> MineralDeposit {
    // 1 Mt = 1e9 kg
    let total_mass_mt = (body_mass_kg * abundance) / 1e9;
    
    // Split into tiers based on accessibility.
    // Logic updated to prevent unrealistically massive "Proven" reserves on planets (Exatons).
    // Earth's Iron reserves are Gigatons (1e-9 of Earth mass), not Exatons.
    let access_factor = accessibility as f64;
    
    let (proven_factor, deep_factor) = match body_type {
        BodyType::Planet | BodyType::Moon | BodyType::DwarfPlanet => {
            // For planetary bodies, proven reserves are a tiny fraction of total composition (Crust is <1%).
            // Base factor 1e-10 ensures Proven reserves are in Gigaton range for Earth-sized bodies.
            (1.0e-10 + access_factor * 5.0e-10, 1.0e-7 + access_factor * 5.0e-7)
        },
        BodyType::Asteroid | BodyType::Comet => {
            // Asteroids are accessible throughout (can be fully stripped).
            // A large portion is considered "Proven" or "Deep" immediately.
            (0.3 + access_factor * 0.4, 0.2 + access_factor * 0.1)
        },
         // Rings/Stars/others
        _ => (0.0001, 0.001)
    };
    
    let proven = total_mass_mt * proven_factor; 
    let deep = total_mass_mt * deep_factor; 
    let bulk = (total_mass_mt - proven - deep).max(0.0);
    
    // Concentration roughly matches abundance or better
    let concentration = (abundance as f32).clamp(0.001, 1.0);
    
    MineralDeposit::new(proven, deep, bulk, concentration, accessibility)
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
fn apply_special_body_profile(body_name: &str, body_mass: f64, _rng: &mut impl Rng) -> Option<PlanetResources> {
    let mut resources = PlanetResources::new();
    
    match body_name {
        // Europa: Massive subsurface ocean (80-90% water)
        "Europa" => {
            resources.add_deposit(ResourceType::Water, create_deposit_legacy(0.85, 0.4, body_mass, BodyType::Moon)); // Massive water, but under ice
            resources.add_deposit(ResourceType::Oxygen, create_deposit_legacy(0.05, 0.3, body_mass, BodyType::Moon)); // Some O2 in ice
            resources.add_deposit(ResourceType::Silicates, create_deposit_legacy(0.08, 0.2, body_mass, BodyType::Moon)); // Rocky core
            resources.add_deposit(ResourceType::Iron, create_deposit_legacy(0.02, 0.1, body_mass, BodyType::Moon)); // Small iron core
            info!("Applied Europa special profile: massive water ocean");
            Some(resources)
        }
        
        // Mars: Polar ice caps and subsurface ice
        "Mars" => {
            resources.add_deposit(ResourceType::Water, create_deposit_legacy(0.15, 0.5, body_mass, BodyType::Planet)); // Significant ice caps
            resources.add_deposit(ResourceType::CarbonDioxide, create_deposit_legacy(0.08, 0.7, body_mass, BodyType::Planet)); // CO2 ice caps
            resources.add_deposit(ResourceType::Iron, create_deposit_legacy(0.25, 0.7, body_mass, BodyType::Planet)); // Oxidized iron (rust)
            resources.add_deposit(ResourceType::Silicates, create_deposit_legacy(0.35, 0.8, body_mass, BodyType::Planet));
            resources.add_deposit(ResourceType::Aluminum, create_deposit_legacy(0.08, 0.6, body_mass, BodyType::Planet));
            resources.add_deposit(ResourceType::Nitrogen, create_deposit_legacy(0.02, 0.4, body_mass, BodyType::Planet)); // Thin atmosphere
            info!("Applied Mars special profile: ice caps and oxidized surface");
            Some(resources)
        }
        
        // Moon (Earth's): Water ice in permanently shadowed craters
        "Moon" => {
            resources.add_deposit(ResourceType::Water, create_deposit_legacy(0.05, 0.3, body_mass, BodyType::Moon)); // Polar ice in craters
            resources.add_deposit(ResourceType::Oxygen, create_deposit_legacy(0.02, 0.4, body_mass, BodyType::Moon)); // Bound in regolith
            resources.add_deposit(ResourceType::Silicates, create_deposit_legacy(0.45, 0.8, body_mass, BodyType::Moon));
            resources.add_deposit(ResourceType::Iron, create_deposit_legacy(0.15, 0.6, body_mass, BodyType::Moon));
            resources.add_deposit(ResourceType::Aluminum, create_deposit_legacy(0.10, 0.7, body_mass, BodyType::Moon));
            resources.add_deposit(ResourceType::Titanium, create_deposit_legacy(0.008, 0.5, body_mass, BodyType::Moon));
            resources.add_deposit(ResourceType::Helium3, create_deposit_legacy(0.00000001, 0.8, body_mass, BodyType::Moon)); // Solar wind implanted
            info!("Applied Moon special profile: polar ice and regolith resources");
            Some(resources)
        }
        
        // Titan: Hydrocarbon-rich moon
        "Titan" => {
            resources.add_deposit(ResourceType::Methane, create_deposit_legacy(0.45, 0.9, body_mass, BodyType::Moon)); // Methane lakes
            resources.add_deposit(ResourceType::Nitrogen, create_deposit_legacy(0.35, 0.8, body_mass, BodyType::Moon)); // Thick N2 atmosphere
            resources.add_deposit(ResourceType::Ammonia, create_deposit_legacy(0.08, 0.6, body_mass, BodyType::Moon));
            resources.add_deposit(ResourceType::Water, create_deposit_legacy(0.10, 0.3, body_mass, BodyType::Moon)); // Water ice crust
            resources.add_deposit(ResourceType::Silicates, create_deposit_legacy(0.02, 0.2, body_mass, BodyType::Moon));
            info!("Applied Titan special profile: hydrocarbon lakes and thick atmosphere");
            Some(resources)
        }
        
        // Enceladus: Water geysers
        "Enceladus" => {
            resources.add_deposit(ResourceType::Water, create_deposit_legacy(0.75, 0.9, body_mass, BodyType::Moon)); // Active geysers make water very accessible
            resources.add_deposit(ResourceType::Nitrogen, create_deposit_legacy(0.05, 0.7, body_mass, BodyType::Moon));
            resources.add_deposit(ResourceType::Ammonia, create_deposit_legacy(0.03, 0.6, body_mass, BodyType::Moon));
            resources.add_deposit(ResourceType::Silicates, create_deposit_legacy(0.15, 0.4, body_mass, BodyType::Moon));
            resources.add_deposit(ResourceType::Iron, create_deposit_legacy(0.02, 0.3, body_mass, BodyType::Moon));
            info!("Applied Enceladus special profile: water geysers");
            Some(resources)
        }
        
        // Ceres: Dwarf planet with significant water ice
        "Ceres" => {
            resources.add_deposit(ResourceType::Water, create_deposit_legacy(0.40, 0.6, body_mass, BodyType::DwarfPlanet));
            resources.add_deposit(ResourceType::Ammonia, create_deposit_legacy(0.08, 0.5, body_mass, BodyType::DwarfPlanet));
            resources.add_deposit(ResourceType::Silicates, create_deposit_legacy(0.35, 0.7, body_mass, BodyType::DwarfPlanet));
            resources.add_deposit(ResourceType::Iron, create_deposit_legacy(0.12, 0.5, body_mass, BodyType::DwarfPlanet));
            resources.add_deposit(ResourceType::Copper, create_deposit_legacy(0.0001, 0.4, body_mass, BodyType::DwarfPlanet));
            info!("Applied Ceres special profile: water-rich dwarf planet");
            Some(resources)
        }
        
        _ => None, // Normal generation for other bodies
    }
}

/// Apply scientifically-based resource profiles based on asteroid spectral class
/// Based on data from NASA, JPL, Asterank, and asteroid taxonomy research
fn apply_spectral_class_profile(
    class: AsteroidClass,
    body_name: &str,
    body_mass: f64,
    distance_au: f64,
    frost_line_au: f64,
    rng: &mut impl Rng
) -> Option<PlanetResources> {
    let mut resources = PlanetResources::new();
    let is_beyond_frost_line = distance_au > frost_line_au;
    
    match class {
        // C-Type: Carbonaceous - High volatiles
        AsteroidClass::CType => {
            // Volatiles are primary composition
            let water_abundance = if is_beyond_frost_line {
                rng.gen_range(0.25..0.45)
            } else {
                rng.gen_range(0.10..0.25)
            };
            resources.add_deposit(ResourceType::Water, create_deposit_legacy(water_abundance, rng.gen_range(0.6..0.85), body_mass, BodyType::Asteroid));
            resources.add_deposit(ResourceType::Hydrogen, create_deposit_legacy(rng.gen_range(0.08..0.15), rng.gen_range(0.5..0.75), body_mass, BodyType::Asteroid));
            resources.add_deposit(ResourceType::Ammonia, create_deposit_legacy(rng.gen_range(0.05..0.12), rng.gen_range(0.5..0.75), body_mass, BodyType::Asteroid));
            resources.add_deposit(ResourceType::Methane, create_deposit_legacy(rng.gen_range(0.03..0.08), rng.gen_range(0.4..0.7), body_mass, BodyType::Asteroid));
            resources.add_deposit(ResourceType::CarbonDioxide, create_deposit_legacy(rng.gen_range(0.04..0.10), rng.gen_range(0.5..0.75), body_mass, BodyType::Asteroid));
            
            // Low metals but present
            resources.add_deposit(ResourceType::Iron, create_deposit_legacy(rng.gen_range(0.05..0.15), rng.gen_range(0.4..0.65), body_mass, BodyType::Asteroid));
            resources.add_deposit(ResourceType::Silicates, create_deposit_legacy(rng.gen_range(0.15..0.25), rng.gen_range(0.5..0.7), body_mass, BodyType::Asteroid));
            
            info!("Applied C-Type spectral profile to {}: High volatiles", body_name);
        }
        
        // S-Type: Silicaceous - Stony, high silicates and metals
        AsteroidClass::SType => {
            // Silicates dominant
            resources.add_deposit(ResourceType::Silicates, create_deposit_legacy(rng.gen_range(0.35..0.50), rng.gen_range(0.7..0.9), body_mass, BodyType::Asteroid));
            resources.add_deposit(ResourceType::Iron, create_deposit_legacy(rng.gen_range(0.20..0.35), rng.gen_range(0.7..0.9), body_mass, BodyType::Asteroid));
            resources.add_deposit(ResourceType::Aluminum, create_deposit_legacy(rng.gen_range(0.08..0.15), rng.gen_range(0.6..0.85), body_mass, BodyType::Asteroid));
            
            // Some metals
            resources.add_deposit(ResourceType::Copper, create_deposit_legacy(rng.gen_range(0.0002..0.0008), rng.gen_range(0.5..0.75), body_mass, BodyType::Asteroid));
            resources.add_deposit(ResourceType::RareEarths, create_deposit_legacy(rng.gen_range(0.0001..0.0004), rng.gen_range(0.4..0.7), body_mass, BodyType::Asteroid));
            
            // Low volatiles
            if is_beyond_frost_line {
                resources.add_deposit(ResourceType::Water, create_deposit_legacy(rng.gen_range(0.02..0.08), rng.gen_range(0.3..0.6), body_mass, BodyType::Asteroid));
            }
            
            info!("Applied S-Type spectral profile to {}: High silicates and iron", body_name);
        }
        
        // M-Type: Metallic - Almost pure metal, nickel-iron
        AsteroidClass::MType => {
            // Dominated by iron and nickel
            resources.add_deposit(ResourceType::Iron, create_deposit_legacy(rng.gen_range(0.70..0.85), rng.gen_range(0.85..0.98), body_mass, BodyType::Asteroid));
            
            // High precious metals (10-20x normal)
            resources.add_deposit(ResourceType::Platinum, create_deposit_legacy(rng.gen_range(0.00001..0.0001), rng.gen_range(0.7..0.9), body_mass, BodyType::Asteroid));
            resources.add_deposit(ResourceType::Gold, create_deposit_legacy(rng.gen_range(0.000005..0.00005), rng.gen_range(0.7..0.9), body_mass, BodyType::Asteroid));
            resources.add_deposit(ResourceType::Silver, create_deposit_legacy(rng.gen_range(0.00001..0.00008), rng.gen_range(0.7..0.9), body_mass, BodyType::Asteroid));
            
            // Some copper and rare earths
            resources.add_deposit(ResourceType::Copper, create_deposit_legacy(rng.gen_range(0.001..0.005), rng.gen_range(0.7..0.9), body_mass, BodyType::Asteroid));
            resources.add_deposit(ResourceType::RareEarths, create_deposit_legacy(rng.gen_range(0.0002..0.001), rng.gen_range(0.6..0.85), body_mass, BodyType::Asteroid));
            
            // Minimal silicates and volatiles
            resources.add_deposit(ResourceType::Silicates, create_deposit_legacy(rng.gen_range(0.02..0.08), rng.gen_range(0.3..0.6), body_mass, BodyType::Asteroid));
            
            info!("Applied M-Type spectral profile to {}: Metallic, high iron and precious metals", body_name);
        }
        
        // V-Type: Vestoid - Basaltic, from differentiated bodies
        AsteroidClass::VType => {
            // Basaltic composition - high silicates and titanium
            resources.add_deposit(ResourceType::Silicates, create_deposit_legacy(rng.gen_range(0.40..0.55), rng.gen_range(0.75..0.92), body_mass, BodyType::Asteroid));
            resources.add_deposit(ResourceType::Titanium, create_deposit_legacy(rng.gen_range(0.02..0.05), rng.gen_range(0.7..0.9), body_mass, BodyType::Asteroid));
            resources.add_deposit(ResourceType::Iron, create_deposit_legacy(rng.gen_range(0.15..0.28), rng.gen_range(0.7..0.88), body_mass, BodyType::Asteroid));
            resources.add_deposit(ResourceType::Aluminum, create_deposit_legacy(rng.gen_range(0.10..0.18), rng.gen_range(0.65..0.85), body_mass, BodyType::Asteroid));
            
            // Some pyroxene-related metals
            resources.add_deposit(ResourceType::Copper, create_deposit_legacy(rng.gen_range(0.0001..0.0005), rng.gen_range(0.5..0.75), body_mass, BodyType::Asteroid));
            
            info!("Applied V-Type spectral profile to {}: Basaltic, high titanium", body_name);
        }
        
        // D-Type: Dark primitive - Very high volatiles, organic-rich
        AsteroidClass::DType => {
            // Extremely high volatiles
            resources.add_deposit(ResourceType::Water, create_deposit_legacy(rng.gen_range(0.35..0.55), rng.gen_range(0.7..0.9), body_mass, BodyType::Asteroid));
            resources.add_deposit(ResourceType::Methane, create_deposit_legacy(rng.gen_range(0.15..0.30), rng.gen_range(0.6..0.85), body_mass, BodyType::Asteroid));
            resources.add_deposit(ResourceType::Ammonia, create_deposit_legacy(rng.gen_range(0.12..0.25), rng.gen_range(0.6..0.85), body_mass, BodyType::Asteroid));
            resources.add_deposit(ResourceType::Hydrogen, create_deposit_legacy(rng.gen_range(0.10..0.20), rng.gen_range(0.6..0.8), body_mass, BodyType::Asteroid));
            resources.add_deposit(ResourceType::Nitrogen, create_deposit_legacy(rng.gen_range(0.05..0.12), rng.gen_range(0.5..0.75), body_mass, BodyType::Asteroid));
            
            // Very low metals
            resources.add_deposit(ResourceType::Silicates, create_deposit_legacy(rng.gen_range(0.05..0.15), rng.gen_range(0.4..0.6), body_mass, BodyType::Asteroid));
            resources.add_deposit(ResourceType::Iron, create_deposit_legacy(rng.gen_range(0.02..0.08), rng.gen_range(0.3..0.55), body_mass, BodyType::Asteroid));
            
            info!("Applied D-Type spectral profile to {}: Primitive, extremely high volatiles", body_name);
        }
        
        // P-Type: Primitive - Similar to D-type, outer belt
        AsteroidClass::PType => {
            // Very high volatiles (slightly less than D-type)
            resources.add_deposit(ResourceType::Water, create_deposit_legacy(rng.gen_range(0.30..0.48), rng.gen_range(0.65..0.88), body_mass, BodyType::Asteroid));
            resources.add_deposit(ResourceType::Ammonia, create_deposit_legacy(rng.gen_range(0.10..0.22), rng.gen_range(0.6..0.82), body_mass, BodyType::Asteroid));
            resources.add_deposit(ResourceType::Methane, create_deposit_legacy(rng.gen_range(0.12..0.25), rng.gen_range(0.55..0.8), body_mass, BodyType::Asteroid));
            resources.add_deposit(ResourceType::Hydrogen, create_deposit_legacy(rng.gen_range(0.08..0.18), rng.gen_range(0.55..0.78), body_mass, BodyType::Asteroid));
            resources.add_deposit(ResourceType::CarbonDioxide, create_deposit_legacy(rng.gen_range(0.06..0.15), rng.gen_range(0.5..0.75), body_mass, BodyType::Asteroid));
            
            // Low metals
            resources.add_deposit(ResourceType::Silicates, create_deposit_legacy(rng.gen_range(0.08..0.18), rng.gen_range(0.45..0.65), body_mass, BodyType::Asteroid));
            resources.add_deposit(ResourceType::Iron, create_deposit_legacy(rng.gen_range(0.03..0.10), rng.gen_range(0.35..0.6), body_mass, BodyType::Asteroid));
            
            info!("Applied P-Type spectral profile to {}: Primitive, very high volatiles", body_name);
        }
        
        AsteroidClass::Unknown => {
            // No spectral profile, fall through to normal generation
            return None;
        }
    }
    
    Some(resources)
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

// Helpers for modifying legacy MineralDeposit assumptions
fn scale_deposit(deposit: &mut MineralDeposit, factor: f64) {
    deposit.reserve.proven_crustal *= factor;
    deposit.reserve.deep_deposits *= factor;
    deposit.reserve.planetary_bulk *= factor;
    // Keep concentration consistent (implied relationship)
    deposit.reserve.concentration = (deposit.reserve.concentration * factor as f32).clamp(0.0001, 1.0);
}

fn cap_deposit_conc(deposit: &mut MineralDeposit, max_conc: f32) {
    if deposit.reserve.concentration > max_conc {
        let factor = (max_conc / deposit.reserve.concentration) as f64;
        scale_deposit(deposit, factor);
    }
}

fn min_deposit_conc(deposit: &mut MineralDeposit, min_conc: f32) {
    if deposit.reserve.concentration < min_conc {
        let factor = (min_conc / deposit.reserve.concentration) as f64;
        scale_deposit(deposit, factor);
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
                    scale_deposit(&mut deposit, 250.0);
                    cap_deposit_conc(&mut deposit, 0.05);
                    deposit.accessibility = (deposit.accessibility * 1.5).min(0.95);
                }
                ResourceType::Silver | ResourceType::Gold => {
                    scale_deposit(&mut deposit, 20.0);
                    cap_deposit_conc(&mut deposit, 0.01);
                }
                _ if !resource.is_precious_metal() => {
                    scale_deposit(&mut deposit, 0.2);
                }
                _ => {}
            }
        }
        
        AsteroidSpecialization::GoldRich => {
            match resource {
                ResourceType::Gold => {
                    scale_deposit(&mut deposit, 200.0);
                    cap_deposit_conc(&mut deposit, 0.03);
                    deposit.accessibility = (deposit.accessibility * 1.5).min(0.95);
                }
                ResourceType::Silver => {
                    scale_deposit(&mut deposit, 50.0);
                    cap_deposit_conc(&mut deposit, 0.015);
                }
                ResourceType::Copper => {
                    scale_deposit(&mut deposit, 10.0);
                    cap_deposit_conc(&mut deposit, 0.005);
                }
                _ if !resource.is_precious_metal() && !resource.is_specialty() => {
                    scale_deposit(&mut deposit, 0.2);
                }
                _ => {}
            }
        }
        
        AsteroidSpecialization::IronNickel => {
            match resource {
                ResourceType::Iron => {
                    min_deposit_conc(&mut deposit, 0.70);
                    deposit.accessibility = (deposit.accessibility * 1.3).min(0.98);
                }
                ResourceType::Platinum | ResourceType::Gold | ResourceType::Silver => {
                    scale_deposit(&mut deposit, 15.0);
                    cap_deposit_conc(&mut deposit, 0.005);
                }
                ResourceType::Copper => {
                    scale_deposit(&mut deposit, 5.0);
                }
                _ if resource.is_volatile() || resource.is_atmospheric_gas() => {
                    scale_deposit(&mut deposit, 0.01);
                }
                _ => {}
            }
        }
        
        AsteroidSpecialization::Carbonaceous => {
            match resource {
                ResourceType::Water | ResourceType::Ammonia | ResourceType::Methane => {
                    min_deposit_conc(&mut deposit, 0.20);
                    deposit.accessibility = (deposit.accessibility * 1.2).min(0.90);
                }
                ResourceType::Hydrogen => {
                    min_deposit_conc(&mut deposit, 0.15);
                }
                ResourceType::CarbonDioxide | ResourceType::Nitrogen => {
                    scale_deposit(&mut deposit, 3.0);
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
/// * `body_mass` - Mass of the body in kg
/// * `distance_au` - Distance from parent star in AU
/// * `frost_line_au` - Frost line distance for the parent star in AU
/// * `is_inner` - Whether the body is inside the frost line
/// * `rng` - Random number generator
fn generate_resource_deposit(
    resource: ResourceType,
    body_mass: f64,
    body_type: BodyType,
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

    create_deposit_legacy(final_abundance, final_accessibility, body_mass, body_type)
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

    // Use 1.0e9 kg (1 Mt) as standard test body mass so that 
    // returned Megatons values are equivalent to fractions (0.0-1.0)
    const TEST_BODY_MASS: f64 = 1.0e9;

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
            TEST_BODY_MASS,
            None,
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
            TEST_BODY_MASS,
            None,
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
            TEST_BODY_MASS,
            None,
            5.2, 
            2.5, 
            &mut rng
        );

        // Europa should have massive water abundance
        let water = resources.get_abundance(&ResourceType::Water);
        assert!(water > 0.5, "Europa should have >50% water (found {})", water);
    }

    #[test]
    fn test_mars_special_profile() {
        let mut rng = rand::thread_rng();
        let resources = generate_resources_for_body(
            "Mars",
            crate::plugins::solar_system_data::BodyType::Planet,
            TEST_BODY_MASS,
            None,
            1.52, 
            2.5, 
            &mut rng
        );

        // Mars should have ice caps and CO2
        let water = resources.get_abundance(&ResourceType::Water);
        let co2 = resources.get_abundance(&ResourceType::CarbonDioxide);
        
        assert!(water > 0.05, "Mars should have significant water ice");
        assert!(co2 > 0.02, "Mars should have CO2 ice caps");
    }

    #[test]
    fn test_atmospheric_gases_present() {
        let mut rng = rand::thread_rng();
        let frost_line = 2.5;
        let resources = generate_resources_for_body(
            "TestBody",
            crate::plugins::solar_system_data::BodyType::Planet,
            TEST_BODY_MASS,
            None,
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
            TEST_BODY_MASS,
            None,
            1.0, 
            frost_line, 
            &mut rng
        );

        // Precious metals should be very rare (when present)
        let gold = resources.get_abundance(&ResourceType::Gold);
        let platinum = resources.get_abundance(&ResourceType::Platinum);
        
        if gold > 0.0 {
            assert!(gold < 0.001, "Gold should be extremely rare");
        }
        if platinum > 0.0 {
            assert!(platinum < 0.0001, "Platinum should be extremely rare");
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
        let deposit = generate_resource_deposit(ResourceType::Iron, TEST_BODY_MASS, BodyType::Planet, 1.0, frost_line, true, &mut rng);
        
        // Values should be within valid ranges (Mass should be >= 0)
        assert!(deposit.reserve.proven_crustal >= 0.0);
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
            TEST_BODY_MASS,
            None,
            1.0, 
            m_star_frost_line, 
            &mut rng
        );
        
        // At 1.0 AU from an M-star, we're well beyond the frost line
        // So we should have some volatiles present
        let water = resources_m.get_abundance(&ResourceType::Water);
        if water > 0.0 {
            assert!(water > 0.1, "Should have decent water abundance beyond frost line");
        }
        
        // Test with a hotter star (A-type) with farther frost line
        let a_star_frost_line = 5.0;
        let resources_a = generate_resources_for_body(
            "TestBody",
            crate::plugins::solar_system_data::BodyType::Planet,
            TEST_BODY_MASS,
            None,
            1.0, 
            a_star_frost_line, 
            &mut rng
        );
        
        // At 1.0 AU from an A-star, we're well inside the frost line
        // So we should have construction materials when present
        let iron = resources_a.get_abundance(&ResourceType::Iron);
        if iron > 0.0 {
            assert!(iron > 0.05, "Should have decent iron abundance inside frost line");
        }
    }

    #[test]
    fn test_c_type_asteroid_spectral_profile() {
        let mut rng = rand::thread_rng();
        let resources = generate_resources_for_body(
            "TestAsteroid",
            crate::plugins::solar_system_data::BodyType::Asteroid,
            TEST_BODY_MASS,
            Some(AsteroidClass::CType),
            2.8,
            2.5,
            &mut rng
        );

        // C-Type should have high volatiles
        let water = resources.get_abundance(&ResourceType::Water);
        assert!(water > 0.10, "C-Type should have >10% water");
    }

    #[test]
    fn test_s_type_asteroid_spectral_profile() {
        let mut rng = rand::thread_rng();
        let resources = generate_resources_for_body(
            "TestAsteroid",
            crate::plugins::solar_system_data::BodyType::Asteroid,
            TEST_BODY_MASS,
            Some(AsteroidClass::SType),
            2.8,
            2.5,
            &mut rng
        );

        // S-Type should have high silicates and iron
        let silicates = resources.get_abundance(&ResourceType::Silicates);
        let iron = resources.get_abundance(&ResourceType::Iron);
        assert!(silicates > 0.20, "S-Type should have >20% silicates");
        assert!(iron > 0.10, "S-Type should have >10% iron");
    }

    #[test]
    fn test_m_type_asteroid_spectral_profile() {
        let mut rng = rand::thread_rng();
        let resources = generate_resources_for_body(
            "TestAsteroid",
            crate::plugins::solar_system_data::BodyType::Asteroid,
            TEST_BODY_MASS,
            Some(AsteroidClass::MType),
            2.8,
            2.5,
            &mut rng
        );

        // M-Type should be dominated by iron
        let iron = resources.get_abundance(&ResourceType::Iron);
        assert!(iron > 0.50, "M-Type should have >50% iron");

        // Should have elevated precious metals
        let platinum = resources.get_abundance(&ResourceType::Platinum);
        if platinum > 0.0 {
            assert!(platinum > 0.000001, "M-Type should have elevated platinum");
        }
    }

    #[test]
    fn test_v_type_asteroid_spectral_profile() {
        let mut rng = rand::thread_rng();
        let resources = generate_resources_for_body(
            "TestAsteroid",
            crate::plugins::solar_system_data::BodyType::Asteroid,
            TEST_BODY_MASS,
            Some(AsteroidClass::VType),
            2.8,
            2.5,
            &mut rng
        );

        // V-Type should have high titanium
        let titanium = resources.get_abundance(&ResourceType::Titanium);
        assert!(titanium > 0.010, "V-Type should have >1.0% titanium");
    }

    #[test]
    fn test_d_type_asteroid_spectral_profile() {
        let mut rng = rand::thread_rng();
        let resources = generate_resources_for_body(
            "TestAsteroid",
            crate::plugins::solar_system_data::BodyType::Asteroid,
            TEST_BODY_MASS,
            Some(AsteroidClass::DType),
            5.2,
            2.5,
            &mut rng
        );

        // D-Type should have extremely high volatiles
        let water = resources.get_abundance(&ResourceType::Water);
        let methane = resources.get_abundance(&ResourceType::Methane);
        assert!(water > 0.20, "D-Type should have >20% water");
        assert!(methane > 0.05, "D-Type should have >5% methane");
    }
}
