use bevy::prelude::*;
use rand::Rng;

use super::components::{MineralDeposit, OrbitsBody, PlanetResources, StarSystem};
use super::types::ResourceType;
use crate::astronomy::SpaceCoordinates;
use crate::plugins::solar_system::{Asteroid, CelestialBody, Comet, DwarfPlanet, Moon, Planet};
use crate::plugins::solar_system_data::{AsteroidClass, BodyType};

/// Default frost line distance in Astronomical Units (for backwards compatibility)
/// Used when no StarSystem component is found (single-star legacy mode)
/// Beyond this distance, volatiles become more common
const DEFAULT_FROST_LINE_AU: f64 = 2.5;

/// System that generates resources for all celestial bodies on startup
/// Uses realistic accretion chemistry based on distance from parent star
/// Supports multiple star systems with different frost lines
/// Applies metallicity bonuses from stellar composition
pub fn generate_solar_system_resources(
    mut commands: Commands,
    // Query planets, dwarf planets, moons, asteroids, and comets without resources
    body_query: Query<
        (
            Entity,
            &CelestialBody,
            &SpaceCoordinates,
            Option<&OrbitsBody>,
        ),
        (
            Or<(
                With<Planet>,
                With<DwarfPlanet>,
                With<Moon>,
                With<Asteroid>,
                With<Comet>,
            )>,
            Without<PlanetResources>,
        ),
    >,
    // Query for star systems to get frost line and metallicity information
    star_query: Query<(&StarSystem, &SpaceCoordinates)>,
    // Query to follow orbit parent chain (used to find the star entity for moons)
    parent_orbits_query: Query<&OrbitsBody>,
) {
    let mut rng = rand::rng();

    for (entity, body, coords, orbits_body) in body_query.iter() {
        // Determine parent star, frost line, and metallicity multiplier
        let (distance_from_star, frost_line, metallicity_multiplier) = if let Some(orbits) =
            orbits_body
        {
            // Follow the parent chain until we find an entity with a StarSystem
            let mut current_parent = orbits.parent;
            let mut found = false;
            let mut distance = coords.position.length();
            let mut frost = DEFAULT_FROST_LINE_AU;
            let mut metallicity_mult = 1.0_f32;

            loop {
                if let Ok((star_system, star_coords)) = star_query.get(current_parent) {
                    // Found the host star for this body (may be the immediate parent or higher up)
                    distance = (coords.position - star_coords.position).length();
                    frost = star_system.frost_line_au;
                    metallicity_mult = star_system.metallicity_multiplier();
                    found = true;
                    break;
                }

                // Step up one level in the orbit chain (parent might itself orbit another body)
                if let Ok(parent_orbits) = parent_orbits_query.get(current_parent) {
                    current_parent = parent_orbits.parent;
                } else {
                    // Reached an entity that doesn't orbit anything; stop searching
                    break;
                }
            }

            if !found {
                // Only warn when the orbit chain truly does not include a star
                warn!(
                    "Parent star lookup failed for {} (orbit chain did not include a star); using origin distance and default frost line",
                    body.name
                );
            }

            (distance, frost, metallicity_mult)
        } else {
            // No parent specified - assume orbiting origin with default frost line
            // This maintains backwards compatibility with single-star systems
            (coords.position.length(), DEFAULT_FROST_LINE_AU, 1.0)
        };

        info!(
            "Generating resources for {} at {:.2} AU (frost line: {:.2} AU, metallicity mult: {:.2}x)",
            body.name, distance_from_star, frost_line, metallicity_multiplier
        );

        // Generate resources based on distance from star, body characteristics, and frost line
        let mut resources = generate_resources_for_body(
            &body.name,
            body.body_type,
            body.mass,
            body.asteroid_class,
            distance_from_star,
            frost_line,
            &mut rng,
        );

        // Apply metallicity bonus to rare metals and fissile materials
        apply_metallicity_bonus(&mut resources, metallicity_multiplier);

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
    rng: &mut impl Rng,
) -> PlanetResources {
    let mut resources = PlanetResources::new();

    // Check for special body-specific resource profiles
    if let Some(special_resources) = apply_special_body_profile(body_name, body_mass, rng) {
        return special_resources;
    }

    // For asteroids with known spectral class, apply scientific resource mapping
    if matches!(
        body_type,
        crate::plugins::solar_system_data::BodyType::Asteroid
    ) {
        if let Some(class) = asteroid_class {
            if let Some(spectral_resources) = apply_spectral_class_profile(
                class,
                body_name,
                body_mass,
                distance_au,
                frost_line_au,
                rng,
            ) {
                return spectral_resources;
            }
        }
    }

    // Determine if we're inside or outside the frost line
    let is_inner = distance_au < frost_line_au;

    // For asteroids, determine if this is a specialized asteroid
    let asteroid_specialization = if matches!(
        body_type,
        crate::plugins::solar_system_data::BodyType::Asteroid
    ) {
        determine_asteroid_specialization(body_name, rng)
    } else {
        AsteroidSpecialization::None
    };

    // Generate each resource type
    for resource_type in ResourceType::all() {
        // Skip some resources randomly for variation (30-50% chance to skip non-critical resources)
        if !resource_type.is_critical() && rng.random_bool(0.4) {
            continue;
        }

        let mut deposit = generate_resource_deposit(
            *resource_type,
            body_mass,
            body_type,
            distance_au,
            frost_line_au,
            is_inner,
            rng,
        );

        // Apply asteroid specialization modifiers
        deposit = apply_asteroid_specialization(deposit, *resource_type, &asteroid_specialization);

        // Add deposits that are economically viable, or keep volatiles/atmospheric gases
        // even if below viability thresholds for small test bodies so tests and
        // physical presence are represented.
        let total_mt = deposit.reserve.total_mass();
        if deposit.is_viable()
            || (total_mt > 0.0 && (resource_type.is_volatile() || resource_type.is_atmospheric_gas()))
        {
            resources.add_deposit(*resource_type, deposit);
        }
    }

    resources
}

/// Apply metallicity bonus to rare metals and fissile materials.
/// Stars with higher metallicity ([Fe/H] > 0) have more heavy elements in their protoplanetary disk.
/// This affects the abundance of rare metals and fissiles according to the provided
/// `metallicity_multiplier` (currently derived in `StarSystem::metallicity_multiplier()` as
/// `1.0 + metallicity * 0.6`, with clamping applied there, resulting in approximately
/// +6% per +0.1 [Fe/H]).
///
/// # Arguments
/// * `resources` - Mutable reference to PlanetResources to modify
/// * `metallicity_multiplier` - Precomputed multiplier from the star's `metallicity_multiplier()` method
fn apply_metallicity_bonus(resources: &mut PlanetResources, metallicity_multiplier: f32) {
    // Only apply to rare metals and fissile materials
    let affected_resources = [
        ResourceType::Gold,
        ResourceType::Silver,
        ResourceType::Platinum,
        ResourceType::RareEarths,
        ResourceType::Uranium,
        ResourceType::Thorium,
    ];

    for resource_type in &affected_resources {
        if let Some(deposit) = resources.get_deposit_mut(*resource_type) {
            // Apply multiplier to all tiers of reserves
            deposit.reserve.proven_crustal *= metallicity_multiplier as f64;
            deposit.reserve.deep_deposits *= metallicity_multiplier as f64;
            deposit.reserve.planetary_bulk *= metallicity_multiplier as f64;
        }
    }
}

/// Helper to create a tiered deposit from legacy parameters
fn create_deposit_legacy(
    abundance: f64,
    accessibility: f32,
    body_mass_kg: f64,
    body_type: BodyType,
) -> MineralDeposit {
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
            (
                1.0e-10 + access_factor * 5.0e-10,
                1.0e-7 + access_factor * 5.0e-7,
            )
        }
        BodyType::Asteroid | BodyType::Comet => {
            // Asteroids are accessible throughout (can be fully stripped).
            // A large portion is considered "Proven" or "Deep" immediately.
            (0.3 + access_factor * 0.4, 0.2 + access_factor * 0.1)
        }
        // Rings/Stars/others
        _ => (0.0001, 0.001),
    };

    let proven = total_mass_mt * proven_factor;
    let deep = total_mass_mt * deep_factor;
    let bulk = (total_mass_mt - proven - deep).max(0.0);

    // Concentration roughly matches abundance or better
    let concentration = (abundance as f32).clamp(0.001, 1.0);

    MineralDeposit::new(proven, deep, bulk, concentration, accessibility)
}

/// Helper to create a deposit from absolute mass in Megatons (Mt) - scientifically verified values
/// Use this for bodies with known, measured resource amounts
///
/// # Arguments
/// * `total_mass_mt` - Total resource mass in Megatons (Mt), where 1 Mt = 10^6 metric tons = 10^9 kg
/// * `accessibility` - How easy to extract (0.0 to 1.0)
/// * `body_type` - Type of celestial body (affects tier distribution)
fn create_deposit_from_absolute_mass(
    total_mass_mt: f64,
    accessibility: f32,
    body_type: BodyType,
) -> MineralDeposit {
    let access_factor = accessibility as f64;

    let (proven_factor, deep_factor) = match body_type {
        BodyType::Planet | BodyType::Moon | BodyType::DwarfPlanet => {
            // For known measured deposits, use higher accessibility factors
            // Scientific estimates already account for extractable amounts
            (0.1 + access_factor * 0.3, 0.2 + access_factor * 0.4)
        }
        BodyType::Asteroid | BodyType::Comet => {
            // Asteroids are accessible throughout
            (0.3 + access_factor * 0.4, 0.2 + access_factor * 0.1)
        }
        _ => (0.0001, 0.001),
    };

    let proven = total_mass_mt * proven_factor;
    let deep = total_mass_mt * deep_factor;
    let bulk = (total_mass_mt - proven - deep).max(0.0);

    // For absolute mass deposits, concentration is based on how accessible it is
    let concentration = (accessibility * 0.8 + 0.1).clamp(0.001, 1.0);

    MineralDeposit::new(proven, deep, bulk, concentration, accessibility)
}

/// Helper to create a deposit for atmospheric gases
/// Gases in an atmosphere are directly harvestable - they don't require deep drilling.
/// The tier model for gases is:
/// - Proven ("Atmospheric"): Gas in the atmosphere, directly harvestable
/// - Deep ("Trapped/Dissolved"): Gas dissolved in oceans or trapped in rock pores
/// - Bulk ("Chemically Bound"): Gas locked in mineral bonds (e.g. carbonates for CO2)
///
/// # Arguments
/// * `atmospheric_mass_mt` - Mass of gas in the atmosphere in Megatons (Mt)
/// * `dissolved_fraction` - Additional fraction trapped/dissolved relative to atmospheric amount (e.g., 0.1 = 10% extra)
/// * `bound_fraction` - Additional fraction chemically bound in minerals (e.g., 0.05 = 5% extra)
/// * `accessibility` - How easy to harvest from atmosphere (0.0 to 1.0)
fn create_atmospheric_deposit(
    atmospheric_mass_mt: f64,
    dissolved_fraction: f64,
    bound_fraction: f64,
    accessibility: f32,
) -> MineralDeposit {
    let proven = atmospheric_mass_mt;
    let deep = atmospheric_mass_mt * dissolved_fraction;
    let bulk = atmospheric_mass_mt * bound_fraction;

    // High concentration for atmospheric gases (they're right there in the air)
    let concentration = (accessibility * 0.7 + 0.2).clamp(0.01, 1.0);

    let mut deposit = MineralDeposit::new(proven, deep, bulk, concentration, accessibility);
    deposit.is_atmospheric = true;
    deposit
}

/// Helper to create a deposit for trace atmospheric gases (very small amounts)
/// Used for gases present at ppb/ppm levels in an atmosphere
#[cfg(test)]
fn create_trace_atmospheric_deposit(
    mass_mt: f64,
    accessibility: f32,
) -> MineralDeposit {
    // Trace gases are entirely atmospheric, no significant trapped/bound amounts
    let concentration = (accessibility * 0.3).clamp(0.001, 0.1);
    let mut deposit = MineralDeposit::new(mass_mt, 0.0, 0.0, concentration, accessibility);
    deposit.is_atmospheric = true;
    deposit
}

/// Helper to create a deposit for solar-wind implanted resources (e.g. He-3)
/// These exist only in the top few meters of regolith on airless bodies.
/// No deep deposits or planetary bulk — it's purely a surface phenomenon.
///
/// # Arguments
/// * `total_mass_mt` - Total implanted mass in Megatons
/// * `accessibility` - How easy to harvest (0.0 to 1.0)
fn create_solar_wind_deposit(
    total_mass_mt: f64,
    accessibility: f32,
) -> MineralDeposit {
    // 90% in proven (surface regolith, directly harvestable)
    // 10% in deep (slightly deeper regolith layers)
    // 0% bulk (solar wind doesn't penetrate deep)
    let proven = total_mass_mt * 0.9;
    let deep = total_mass_mt * 0.1;
    let concentration = (accessibility * 0.5).clamp(0.001, 1.0);
    MineralDeposit::new(proven, deep, 0.0, concentration, accessibility)
}

/// Asteroid specialization types for concentrated resource deposits
#[derive(Debug, Clone, Copy)]
enum AsteroidSpecialization {
    None,
    PlatinumRich, // Mostly platinum group metals
    GoldRich,     // High gold and silver content
    IronNickel,   // Metallic asteroid with iron/nickel
    Carbonaceous, // High volatiles even in inner system
}

/// Apply special resource profiles for known celestial bodies
/// Returns Some(resources) for special bodies, None for normal generation
fn apply_special_body_profile(
    body_name: &str,
    body_mass: f64,
    _rng: &mut impl Rng,
) -> Option<PlanetResources> {
    let mut resources = PlanetResources::new();

    match body_name {
        // GAS GIANTS - Atmospheric composition only, NO solid ice reserves
        // Jupiter: 0.25% atmospheric water vapor (NOT mineable ice)
        "Jupiter" => {
            // Jupiter is a gas giant - only atmospheric hydrogen and helium
            // Small amounts of other gases, but NO solid ice deposits
            resources.add_deposit(
                ResourceType::Hydrogen,
                create_deposit_legacy(0.90, 0.02, body_mass, BodyType::Planet),
            ); // 90% H2, but very low accessibility
            resources.add_deposit(
                ResourceType::Helium3,
                create_atmospheric_deposit(
                    (body_mass * 0.00002) / 1e9, // Trace He3 in atmosphere
                    0.1,  // small dissolved fraction
                    0.0,  // no bound fraction
                    0.05, // very low accessibility (deep atmosphere)
                ),
            );
               // Note: Water exists as atmospheric vapor (~0.25%), not as mineable solid ice
            info!("Applied Jupiter special profile: gas giant atmosphere (no solid resources)");
            Some(resources)
        }

        // Saturn: Similar to Jupiter but slightly less massive atmosphere
        "Saturn" => {
            resources.add_deposit(
                ResourceType::Hydrogen,
                create_deposit_legacy(0.96, 0.02, body_mass, BodyType::Planet),
            ); // 96% H2
            resources.add_deposit(
                ResourceType::Helium3,
                create_atmospheric_deposit(
                    (body_mass * 0.00001) / 1e9, // Trace He3
                    0.1,
                    0.0,
                    0.05,
                ),
            );
            info!("Applied Saturn special profile: gas giant atmosphere (no solid resources)");
            Some(resources)
        }

        // Uranus: Ice giant with more volatiles than Jupiter/Saturn
        "Uranus" => {
            resources.add_deposit(
                ResourceType::Hydrogen,
                create_deposit_legacy(0.83, 0.02, body_mass, BodyType::Planet),
            );
            resources.add_deposit(
                ResourceType::Helium3,
                create_atmospheric_deposit(
                    (body_mass * 0.000015) / 1e9,
                    0.1,
                    0.0,
                    0.05,
                ),
            );
            resources.add_deposit(
                ResourceType::Methane,
                create_deposit_legacy(0.02, 0.03, body_mass, BodyType::Planet),
            ); // Atmospheric methane
            info!("Applied Uranus special profile: ice giant atmosphere (minimal solid resources)");
            Some(resources)
        }

        // Neptune: Similar to Uranus
        "Neptune" => {
            resources.add_deposit(
                ResourceType::Hydrogen,
                create_deposit_legacy(0.80, 0.02, body_mass, BodyType::Planet),
            );
            resources.add_deposit(
                ResourceType::Helium3,
                create_atmospheric_deposit(
                    (body_mass * 0.000019) / 1e9,
                    0.1,
                    0.0,
                    0.05,
                ),
            );
            resources.add_deposit(
                ResourceType::Methane,
                create_deposit_legacy(0.025, 0.03, body_mass, BodyType::Planet),
            );
            info!(
                "Applied Neptune special profile: ice giant atmosphere (minimal solid resources)"
            );
            Some(resources)
        }

        // Earth: Biosphere-rich and technologically active world
        // Mass: ~5.97e24 kg = 5.97e15 Mt
        //
        // Deposit tiers follow a consistent model based on geological surveys:
        //   Proven  = USGS "Reserves" — economically extractable at current prices/tech
        //   Deep    = USGS "Resources" — identified but sub-economic, or undiscovered
        //             Typically 3-10× reserves for most minerals
        //   Bulk    = Total crustal/mantle abundance — planet-scale, essentially infinite
        //             Only accessible with far-future technology
        //
        // Concentrations are set proportional to 2026 real-world annual production
        // so that the mining system distributes output realistically:
        //   Solid: Silicates ~50,000  Methane ~2,900  Iron ~2,500  Water ~5,000
        //          Aluminum ~76  Copper ~26.0  RareEarths ~0.24  Uranium ~0.058 Mt/yr
        //   Atmo:  N₂ ~175  O₂ ~150  CO₂ ~35  Ar ~4.5 Mt/yr
        "Earth" => {
            // === NON-ATMOSPHERIC (SOLID/LIQUID MINING) ===
            // Concentration determines share of total mining output.
            // Values below are proportional to global annual production,
            // normalised so Silicates (most-mined material) = 1.0.

            // Silicates: Sand, Gravel, Crushed Stone — ~50,000 Mt/yr
            // Proven: Practically unlimited surface material (~200 yr supply at current rate)
            // Deep: Upper crustal silicates accessible with effort
            // Bulk: ~45% of crust by mass
            resources.add_deposit(
                ResourceType::Silicates,
                MineralDeposit::new(10_000_000.0, 1_600_000_000.0, 2.69e15, 1.0, 1.0),
            );

            // Water: Oceans + Freshwater — industrial use ~5,000 Mt/yr
            // Proven: 1.35 billion Gt oceans (effectively inexhaustible)
            // Deep: 50M Mt ice caps + groundwater
            resources.add_deposit(
                ResourceType::Water,
                MineralDeposit::new(1_350_000_000.0, 50_000_000.0, 0.0, 0.10, 1.0),
            );

            // Methane (Natural Gas) — ~2,900 Mt/yr (~4,000 bcm)
            // Proven: ~188,000 Mt (USGS+EIA 2024: ~188 Tcm proven reserves)
            // Deep: ~800,000 Mt (unconventional: shale, tight gas, hydrates)
            resources.add_deposit(
                ResourceType::Methane,
                MineralDeposit::new(188_000.0, 800_000.0, 0.0, 0.058, 0.3),
            );

            // Iron — ~2,500 Mt/yr (crude ore + DRI + steel)
            // Proven: ~180,000 Mt (USGS 2024: 180 Gt iron content in ore reserves)
            // Deep: ~800,000 Mt (USGS identified resources, lower-grade deposits)
            // Bulk: Core+mantle iron (~32% of Earth by mass)
            resources.add_deposit(
                ResourceType::Iron,
                MineralDeposit::new(180_000.0, 800_000.0, 1.91e15, 0.05, 0.9),
            );

            // Aluminum — ~76 Mt/yr primary production
            // Proven: ~5,500 Mt (USGS 2024: ~32 Gt bauxite ≈ 5.5 Gt aluminum metal)
            // Deep: ~40,000 Mt (USGS identified resources, non-bauxite alumina sources)
            // Bulk: ~8% of crust by mass
            resources.add_deposit(
                ResourceType::Aluminum,
                MineralDeposit::new(5_500.0, 40_000.0, 4.78e14, 0.00152, 0.8),
            );

            // Copper — ~26.0 Mt/yr (updated to match 2026 demand)
            // Proven: ~890 Mt (USGS 2024 reserves)
            // Deep: ~3,500 Mt (USGS 2024 identified + undiscovered resources ~5.6 Gt)
            // Bulk: Crustal average ~60 ppm
            resources.add_deposit(
                ResourceType::Copper,
                MineralDeposit::new(890.0, 3_500.0, 1.43e12, 0.00184, 0.5),
            );

            // Rare Earths — ~0.24 Mt/yr (240,000 tonnes REO)
            // Proven: ~110 Mt REO (USGS 2024 reserves)
            // Deep: ~500 Mt (USGS total resources estimate)
            // Bulk: Crustal average ~150-200 ppm
            resources.add_deposit(
                ResourceType::RareEarths,
                MineralDeposit::new(110.0, 500.0, 1.19e12, 0.000017, 0.4),
            );

            // Uranium — ~0.058 Mt/yr (58,000 tonnes)
            // Proven: ~6.1 Mt (IAEA 2024: RAR <$130/kg U)
            // Deep: ~22 Mt (IAEA total identified + speculative resources)
            // Bulk: Crustal average ~2.7 ppm
            resources.add_deposit(
                ResourceType::Uranium,
                MineralDeposit::new(6.1, 22.0, 1.61e10, 0.0000012, 0.3),
            );

            // Thorium — ~0.01 Mt/yr (byproduct of rare earth mining)
            // Proven: ~6.3 Mt (USGS/IAEA 2024 identified resources)
            // Deep: ~25 Mt (speculative; thorium largely unexplored)
            // Bulk: Crustal average ~8.1 ppm — 3× more abundant than uranium
            resources.add_deposit(
                ResourceType::Thorium,
                MineralDeposit::new(6.3, 25.0, 4.84e10, 0.0000002, 0.35),
            );

            // Titanium — ~0.26 Mt/yr sponge metal (~9 Mt/yr mineral concentrates)
            // Proven: ~700 Mt (USGS 2024: ilmenite + rutile reserves)
            // Deep: ~2,000 Mt (USGS total resources)
            // Bulk: Crustal average ~0.57% — 9th most abundant element
            resources.add_deposit(
                ResourceType::Titanium,
                MineralDeposit::new(700.0, 2_000.0, 3.40e13, 0.0000052, 0.6),
            );

            // Gold — ~0.0031 Mt/yr (3,100 tonnes)
            // Proven: ~0.059 Mt (USGS 2024: 59,000 tonnes reserves)
            // Deep: ~0.20 Mt (USGS identified resources + ocean dissolved)
            // Bulk: Crustal average ~0.004 ppm (most in core, inaccessible)
            resources.add_deposit(
                ResourceType::Gold,
                MineralDeposit::new(0.059, 0.20, 2.39e4, 0.000000062, 0.3),
            );

            // Silver — ~0.026 Mt/yr (26,000 tonnes)
            // Proven: ~0.53 Mt (USGS 2024: 530,000 tonnes reserves)
            // Deep: ~1.7 Mt (USGS identified resources)
            // Bulk: Crustal average ~0.075 ppm
            resources.add_deposit(
                ResourceType::Silver,
                MineralDeposit::new(0.53, 1.7, 4.48e5, 0.00000052, 0.3),
            );

            // Platinum — ~0.00019 Mt/yr (190 tonnes)
            // Proven: ~0.069 Mt (USGS 2024: 69,000 tonnes reserves)
            // Deep: ~0.10 Mt (USGS total resources, mainly Bushveld Complex)
            // Bulk: Crustal average ~0.005 ppm
            resources.add_deposit(
                ResourceType::Platinum,
                MineralDeposit::new(0.069, 0.10, 2.99e4, 0.0000000038, 0.2),
            );

            // === ATMOSPHERIC GASES ===
            // Concentrations proportional to 2026 industrial gas production.
            // N₂ (highest industrial output) = 1.0 reference.

            // Nitrogen: 78% of atmosphere — ~175 Mt/yr industrial
            {
                let mut dep = MineralDeposit::new(4_000_000_000.0, 4_000_000.0, 0.0, 1.0, 0.78);
                dep.is_atmospheric = true;
                resources.add_deposit(ResourceType::Nitrogen, dep);
            }

            // Oxygen: 21% of atmosphere — ~150 Mt/yr industrial
            {
                let mut dep = MineralDeposit::new(1_100_000_000.0, 7_700_000.0, 0.0, 0.857, 0.21);
                dep.is_atmospheric = true;
                resources.add_deposit(ResourceType::Oxygen, dep);
            }

            // Carbon Dioxide: 0.04% of atmosphere — ~35 Mt/yr industrial
            {
                let mut dep = MineralDeposit::new(2_200_000.0, 2_200_000.0, 0.0, 0.200, 0.0004);
                dep.is_atmospheric = true;
                resources.add_deposit(ResourceType::CarbonDioxide, dep);
            }

            // Argon: 0.93% of atmosphere — ~4.5 Mt/yr industrial
            {
                let mut dep = MineralDeposit::new(50_000_000.0, 0.0, 0.0, 0.026, 0.009);
                dep.is_atmospheric = true;
                resources.add_deposit(ResourceType::Argon, dep);
            }

            info!("Applied Earth special profile: production-calibrated concentrations (2026 baseline)");
            Some(resources)
        }

        // Europa: Massive subsurface ocean (2-3x Earth's oceans)
        // Scientific estimate: 2.6-3.2×10^18 metric tons
        // Europa mass: ~4.8×10^22 kg = (4.8×10^22 ÷ 10^9) = 4.8×10^13 Mt
        // 85% water = 0.85 × 4.8×10^13 Mt = 4.08×10^13 Mt = ~40 trillion Mt
        // This represents 2-3× Earth's oceans (realistic!)
        "Europa" => {
            resources.add_deposit(
                ResourceType::Water,
                create_deposit_legacy(0.85, 0.4, body_mass, BodyType::Moon),
            );
            resources.add_deposit(
                ResourceType::Oxygen,
                create_deposit_legacy(0.05, 0.3, body_mass, BodyType::Moon),
            );
            resources.add_deposit(
                ResourceType::Silicates,
                create_deposit_legacy(0.08, 0.2, body_mass, BodyType::Moon),
            );
            resources.add_deposit(
                ResourceType::Iron,
                create_deposit_legacy(0.02, 0.1, body_mass, BodyType::Moon),
            );
            info!("Applied Europa special profile: massive subsurface ocean (2-3× Earth's oceans)");
            Some(resources)
        }

        // Mars: Polar ice caps and subsurface ice
        // Scientific estimate: 5 million km³ = 5×10^6 km³ × 920 kg/m³ × 10^9 m³/km³
        //                     = 4.6×10^18 kg = 4.6×10^15 metric tons = 4.6×10^9 Mt
        // Mars mass: 6.4171×10²³ kg = 6.4171×10^14 Mt
        "Mars" => {
            // Water: 5M km³ ice = 4.6×10^9 Mt (4.6 billion Mt)
            resources.add_deposit(
                ResourceType::Water,
                create_deposit_from_absolute_mass(4.6e9, 0.5, BodyType::Planet),
            );

            // Mars regolith composition (from rover data):
            // SiO2: 44-46%, FeO: 16-22%, Al2O3: 9-10%
            // Using crustal abundance approach for realistic extraction
            resources.add_deposit(
                ResourceType::Iron,
                create_deposit_legacy(0.18, 0.7, body_mass, BodyType::Planet),
            ); // ~18% iron oxide
            resources.add_deposit(
                ResourceType::Silicates,
                create_deposit_legacy(0.45, 0.8, body_mass, BodyType::Planet),
            ); // ~45% silicates
            resources.add_deposit(
                ResourceType::Aluminum,
                create_deposit_legacy(0.095, 0.6, body_mass, BodyType::Planet),
            ); // ~9.5% aluminum oxide
            resources.add_deposit(
                ResourceType::CarbonDioxide,
                create_deposit_legacy(0.08, 0.7, body_mass, BodyType::Planet),
            ); // CO2 ice caps
            resources.add_deposit(
                ResourceType::Nitrogen,
                create_deposit_legacy(0.02, 0.4, body_mass, BodyType::Planet),
            ); // Thin atmosphere
            info!("Applied Mars special profile: 4.6 billion Mt water ice, basaltic regolith");
            Some(resources)
        }

        // Moon (Earth's): Water ice in permanently shadowed craters
        // Scientific estimate: 600 million metric tons = 6×10^8 metric tons = 600 Mt
        // Moon mass: 7.342×10²² kg = 7.342×10^13 Mt
        "Moon" => {
            // Water: 600 Mt in permanently shadowed polar craters
            // Mostly surface/near-surface ice, not underground aquifers
            // Proven = surface ice in craters (directly accessible): ~540 Mt
            // Deep = subsurface permafrost: ~60 Mt
            // Bulk = 0 (no hidden deep water on Moon)
            resources.add_deposit(
                ResourceType::Water,
                MineralDeposit::new(540.0, 60.0, 0.0, 0.3, 0.3),
            );

            // Moon regolith composition (Apollo samples):
            // Highlands: SiO2 ~45%, Al2O3 ~24%, FeO ~6%, TiO2 ~0.6%
            // Maria: SiO2 ~45%, Al2O3 ~15%, FeO ~14%, TiO2 ~7.5%
            // Using average composition
            resources.add_deposit(
                ResourceType::Oxygen,
                create_deposit_legacy(0.43, 0.4, body_mass, BodyType::Moon),
            ); // ~43% oxygen in oxides
            resources.add_deposit(
                ResourceType::Silicates,
                create_deposit_legacy(0.45, 0.8, body_mass, BodyType::Moon),
            ); // ~45% silicates
            resources.add_deposit(
                ResourceType::Iron,
                create_deposit_legacy(0.10, 0.6, body_mass, BodyType::Moon),
            ); // ~10% iron (average)
            resources.add_deposit(
                ResourceType::Aluminum,
                create_deposit_legacy(0.08, 0.7, body_mass, BodyType::Moon),
            ); // ~8% aluminum
            resources.add_deposit(
                ResourceType::Titanium,
                create_deposit_legacy(0.04, 0.5, body_mass, BodyType::Moon),
            ); // ~4% titanium (generous)
            // He-3: Solar wind implanted into surface regolith only (top 2-3 meters)
            // Estimated ~1 million metric tons total (Wittenberg et al. 1986)
            // 1 Mt in game units. Entirely surface — no deep deposits.
            resources.add_deposit(
                ResourceType::Helium3,
                MineralDeposit::new(0.9, 0.1, 0.0, 0.8, 0.8),
            );
            info!("Applied Moon special profile: 600 Mt water ice in polar craters");
            Some(resources)
        }

        // Titan: Hydrocarbon-rich moon
        "Titan" => {
            resources.add_deposit(
                ResourceType::Methane,
                create_deposit_legacy(0.45, 0.9, body_mass, BodyType::Moon),
            );
            resources.add_deposit(
                ResourceType::Nitrogen,
                create_deposit_legacy(0.35, 0.8, body_mass, BodyType::Moon),
            );
            resources.add_deposit(
                ResourceType::Ammonia,
                create_deposit_legacy(0.08, 0.6, body_mass, BodyType::Moon),
            );
            resources.add_deposit(
                ResourceType::Water,
                create_deposit_legacy(0.10, 0.3, body_mass, BodyType::Moon),
            );
            resources.add_deposit(
                ResourceType::Silicates,
                create_deposit_legacy(0.02, 0.2, body_mass, BodyType::Moon),
            );
            info!("Applied Titan special profile: hydrocarbon lakes and thick N2 atmosphere");
            Some(resources)
        }

        // Enceladus: Water geysers
        "Enceladus" => {
            resources.add_deposit(
                ResourceType::Water,
                create_deposit_legacy(0.75, 0.9, body_mass, BodyType::Moon),
            );
            resources.add_deposit(
                ResourceType::Nitrogen,
                create_deposit_legacy(0.05, 0.7, body_mass, BodyType::Moon),
            );
            resources.add_deposit(
                ResourceType::Ammonia,
                create_deposit_legacy(0.03, 0.6, body_mass, BodyType::Moon),
            );
            resources.add_deposit(
                ResourceType::Silicates,
                create_deposit_legacy(0.15, 0.4, body_mass, BodyType::Moon),
            );
            resources.add_deposit(
                ResourceType::Iron,
                create_deposit_legacy(0.02, 0.3, body_mass, BodyType::Moon),
            );
            info!("Applied Enceladus special profile: active water geysers");
            Some(resources)
        }

        // Ceres: Dwarf planet with significant water ice
        "Ceres" => {
            resources.add_deposit(
                ResourceType::Water,
                create_deposit_legacy(0.40, 0.6, body_mass, BodyType::DwarfPlanet),
            );
            resources.add_deposit(
                ResourceType::Ammonia,
                create_deposit_legacy(0.08, 0.5, body_mass, BodyType::DwarfPlanet),
            );
            resources.add_deposit(
                ResourceType::Silicates,
                create_deposit_legacy(0.35, 0.7, body_mass, BodyType::DwarfPlanet),
            );
            resources.add_deposit(
                ResourceType::Iron,
                create_deposit_legacy(0.12, 0.5, body_mass, BodyType::DwarfPlanet),
            );
            resources.add_deposit(
                ResourceType::Copper,
                create_deposit_legacy(0.0001, 0.4, body_mass, BodyType::DwarfPlanet),
            );
            info!("Applied Ceres special profile: water-rich dwarf planet");
            Some(resources)
        }



        // Venus: Dense CO2 atmosphere, volcanic surface, no water
        // Venus mass: 4.867×10^24 kg
        // Atmosphere mass: 4.8×10^20 kg (100× Earth's, 96.5% CO2, 3.5% N2)
        // Surface pressure: 92 bar
        "Venus" => {
            // === ATMOSPHERIC GASES ===
            // CO2: 96.5% of atmosphere = 4.63×10^20 kg = 4.63×10^11 Mt
            resources.add_deposit(
                ResourceType::CarbonDioxide,
                create_atmospheric_deposit(4.63e11, 0.0, 0.0, 0.6),
            );

            // Nitrogen: 3.5% of atmosphere = 1.68×10^19 kg = 1.68×10^10 Mt
            resources.add_deposit(
                ResourceType::Nitrogen,
                create_atmospheric_deposit(1.68e10, 0.0, 0.0, 0.6),
            );

            // Argon: 70 ppm = 3.36×10^16 kg = 3.36×10^7 Mt
            resources.add_deposit(
                ResourceType::Argon,
                create_atmospheric_deposit(3.36e7, 0.0, 0.0, 0.5),
            );

            // SO2: ~150 ppm - we don't model sulfur compounds, but it's notable

            // === NO WATER ===
            // Venus has essentially no water (lost to space via hydrogen escape)

            // === CONSTRUCTION MATERIALS ===
            // Similar rocky composition to Earth but low accessibility (extreme surface conditions)
            resources.add_deposit(
                ResourceType::Iron,
                create_deposit_legacy(0.31, 0.2, body_mass, BodyType::Planet),
            ); // Similar to Earth, very low accessibility (462°C, 92 bar)
            resources.add_deposit(
                ResourceType::Silicates,
                create_deposit_legacy(0.30, 0.25, body_mass, BodyType::Planet),
            );
            resources.add_deposit(
                ResourceType::Aluminum,
                create_deposit_legacy(0.015, 0.2, body_mass, BodyType::Planet),
            );
            resources.add_deposit(
                ResourceType::Titanium,
                create_deposit_legacy(0.0004, 0.15, body_mass, BodyType::Planet),
            );

            // === FISSILE ===
            resources.add_deposit(
                ResourceType::Uranium,
                create_deposit_legacy(0.000001, 0.15, body_mass, BodyType::Planet),
            );
            resources.add_deposit(
                ResourceType::Thorium,
                create_deposit_legacy(0.000004, 0.12, body_mass, BodyType::Planet),
            );

            // === PRECIOUS/SPECIALTY ===
            resources.add_deposit(
                ResourceType::Gold,
                create_deposit_legacy(0.0000000005, 0.1, body_mass, BodyType::Planet),
            );
            resources.add_deposit(
                ResourceType::Copper,
                create_deposit_legacy(0.00003, 0.15, body_mass, BodyType::Planet),
            );

            info!("Applied Venus special profile: massive CO2 atmosphere, no water, extreme surface");
            Some(resources)
        }

        // Mercury: Dense metallic core, no atmosphere, extreme temperature
        // Mercury mass: 3.301×10^23 kg, ~70% metallic core
        "Mercury" => {
            // No atmosphere, no volatiles

            // === CONSTRUCTION MATERIALS ===
            // Mercury has an oversized iron core (~70% of radius, ~60% by mass)
            resources.add_deposit(
                ResourceType::Iron,
                create_deposit_legacy(0.60, 0.7, body_mass, BodyType::Planet),
            ); // Much higher iron fraction than other rocky planets
            resources.add_deposit(
                ResourceType::Silicates,
                create_deposit_legacy(0.25, 0.8, body_mass, BodyType::Planet),
            );
            resources.add_deposit(
                ResourceType::Aluminum,
                create_deposit_legacy(0.007, 0.7, body_mass, BodyType::Planet),
            );
            resources.add_deposit(
                ResourceType::Titanium,
                create_deposit_legacy(0.0003, 0.6, body_mass, BodyType::Planet),
            );

            // Water: tiny amounts of ice in permanently shadowed polar craters
            // MESSENGER data: ~100 billion to 1 trillion kg = 100-1000 Mt
            resources.add_deposit(
                ResourceType::Water,
                create_deposit_from_absolute_mass(500.0, 0.3, BodyType::Planet),
            );

            // === PRECIOUS METALS ===
            // Large metallic core means elevated precious and siderophile metals
            resources.add_deposit(
                ResourceType::Gold,
                create_deposit_legacy(0.000000002, 0.4, body_mass, BodyType::Planet),
            );
            resources.add_deposit(
                ResourceType::Platinum,
                create_deposit_legacy(0.000000005, 0.4, body_mass, BodyType::Planet),
            );
            resources.add_deposit(
                ResourceType::Silver,
                create_deposit_legacy(0.00000005, 0.35, body_mass, BodyType::Planet),
            );

            // === SPECIALTY ===
            resources.add_deposit(
                ResourceType::Copper,
                create_deposit_legacy(0.00005, 0.5, body_mass, BodyType::Planet),
            );

            // === FISSILE ===
            resources.add_deposit(
                ResourceType::Uranium,
                create_deposit_legacy(0.0000005, 0.3, body_mass, BodyType::Planet),
            );

            // === FUSION FUEL ===
            // Solar wind implanted He-3 (no atmosphere to shield surface)
            resources.add_deposit(
                ResourceType::Helium3,
                create_solar_wind_deposit((body_mass * 0.000000001) / 1e9, 0.6),
            );

            info!("Applied Mercury special profile: massive iron core, polar ice, solar wind He-3");
            Some(resources)
        }

        _ => None, // Normal generation for other bodies
    }
}

/// Apply scientifically-based resource profiles based on asteroid spectral class
/// Based on data from NASA, JPL, Asterank, and asteroid taxonomy research
/// Scientific estimates: C-type (4-7% water), S-type (<1% water), M-type (negligible water)
fn apply_spectral_class_profile(
    class: AsteroidClass,
    body_name: &str,
    body_mass: f64,
    distance_au: f64,
    frost_line_au: f64,
    rng: &mut impl Rng,
) -> Option<PlanetResources> {
    let mut resources = PlanetResources::new();
    let is_beyond_frost_line = distance_au > frost_line_au;

    match class {
        // C-Type: Carbonaceous - High volatiles (4-7% water content scientifically)
        // About 75% of all asteroids
        AsteroidClass::CType => {
            // Scientific water content: 4-7 wt%, with up to 10.5% in some CM chondrites
            let water_abundance = if is_beyond_frost_line {
                rng.random_range(0.045..0.07) // 4.5-7% water by weight
            } else {
                rng.random_range(0.04..0.055) // 4-5.5% in inner belt
            };
            resources.add_deposit(
                ResourceType::Water,
                create_deposit_legacy(
                    water_abundance,
                    rng.random_range(0.6..0.85),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );
            resources.add_deposit(
                ResourceType::Hydrogen,
                create_deposit_legacy(
                    rng.random_range(0.01..0.02),
                    rng.random_range(0.5..0.75),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );
            resources.add_deposit(
                ResourceType::Ammonia,
                create_deposit_legacy(
                    rng.random_range(0.01..0.025),
                    rng.random_range(0.5..0.75),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );
            resources.add_deposit(
                ResourceType::Methane,
                create_deposit_legacy(
                    rng.random_range(0.005..0.015),
                    rng.random_range(0.4..0.7),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );
            resources.add_deposit(
                ResourceType::CarbonDioxide,
                create_deposit_legacy(
                    rng.random_range(0.01..0.03),
                    rng.random_range(0.5..0.75),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );

            // Moderate metals and silicates
            resources.add_deposit(
                ResourceType::Iron,
                create_deposit_legacy(
                    rng.random_range(0.10..0.20),
                    rng.random_range(0.4..0.65),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );
            resources.add_deposit(
                ResourceType::Silicates,
                create_deposit_legacy(
                    rng.random_range(0.40..0.60),
                    rng.random_range(0.5..0.7),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );

            // Rare earth elements and transition metals (higher in C-types)
            resources.add_deposit(
                ResourceType::RareEarths,
                create_deposit_legacy(
                    rng.random_range(0.0002..0.0005),
                    rng.random_range(0.5..0.7),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );

            info!(
                "Applied C-Type spectral profile to {}: 4-7% water content",
                body_name
            );
        }

        // S-Type: Silicaceous - Stony, high silicates and metals
        // About 17% of main belt asteroids
        // Very low water content (<1%), mostly bound in minerals
        AsteroidClass::SType => {
            // Silicates dominant (olivine and pyroxene)
            resources.add_deposit(
                ResourceType::Silicates,
                create_deposit_legacy(
                    rng.random_range(0.45..0.65),
                    rng.random_range(0.7..0.9),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );
            resources.add_deposit(
                ResourceType::Iron,
                create_deposit_legacy(
                    rng.random_range(0.18..0.30),
                    rng.random_range(0.7..0.9),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );
            resources.add_deposit(
                ResourceType::Aluminum,
                create_deposit_legacy(
                    rng.random_range(0.04..0.08),
                    rng.random_range(0.6..0.85),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );

            // Some metals
            resources.add_deposit(
                ResourceType::Copper,
                create_deposit_legacy(
                    rng.random_range(0.0001..0.0004),
                    rng.random_range(0.5..0.75),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );
            resources.add_deposit(
                ResourceType::RareEarths,
                create_deposit_legacy(
                    rng.random_range(0.00005..0.0002),
                    rng.random_range(0.4..0.7),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );

            // Very low volatiles (<1% water scientifically, as hydroxyl in minerals)
            if is_beyond_frost_line {
                resources.add_deposit(
                    ResourceType::Water,
                    create_deposit_legacy(
                        rng.random_range(0.005..0.01),
                        rng.random_range(0.3..0.6),
                        body_mass,
                        BodyType::Asteroid,
                    ),
                );
            } else {
                // Even less in inner belt
                resources.add_deposit(
                    ResourceType::Water,
                    create_deposit_legacy(
                        rng.random_range(0.002..0.007),
                        rng.random_range(0.2..0.5),
                        body_mass,
                        BodyType::Asteroid,
                    ),
                );
            }

            info!(
                "Applied S-Type spectral profile to {}: <1% water, high silicates",
                body_name
            );
        }

        // M-Type: Metallic - Almost pure metal, nickel-iron
        // About 8% of main belt asteroids
        // Negligible water content (anhydrous, remnant metallic cores)
        AsteroidClass::MType => {
            // Dominated by iron and nickel (70-85% iron)
            resources.add_deposit(
                ResourceType::Iron,
                create_deposit_legacy(
                    rng.random_range(0.70..0.85),
                    rng.random_range(0.85..0.98),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );

            // Platinum-group metals (higher concentrations than Earth's crust)
            resources.add_deposit(
                ResourceType::Platinum,
                create_deposit_legacy(
                    rng.random_range(0.00001..0.0001),
                    rng.random_range(0.7..0.9),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );
            resources.add_deposit(
                ResourceType::Gold,
                create_deposit_legacy(
                    rng.random_range(0.000005..0.00005),
                    rng.random_range(0.7..0.9),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );
            resources.add_deposit(
                ResourceType::Silver,
                create_deposit_legacy(
                    rng.random_range(0.00001..0.00008),
                    rng.random_range(0.7..0.9),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );

            // Some copper
            resources.add_deposit(
                ResourceType::Copper,
                create_deposit_legacy(
                    rng.random_range(0.001..0.005),
                    rng.random_range(0.7..0.9),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );

            // Minimal silicates, NO significant volatiles (anhydrous)
            resources.add_deposit(
                ResourceType::Silicates,
                create_deposit_legacy(
                    rng.random_range(0.02..0.08),
                    rng.random_range(0.3..0.6),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );

            info!(
                "Applied M-Type spectral profile to {}: Metallic, negligible water",
                body_name
            );
        }

        // V-Type: Vestoid - Basaltic, from differentiated bodies
        AsteroidClass::VType => {
            // Basaltic composition - high silicates and titanium
            resources.add_deposit(
                ResourceType::Silicates,
                create_deposit_legacy(
                    rng.random_range(0.40..0.55),
                    rng.random_range(0.75..0.92),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );
            resources.add_deposit(
                ResourceType::Titanium,
                create_deposit_legacy(
                    rng.random_range(0.02..0.05),
                    rng.random_range(0.7..0.9),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );
            resources.add_deposit(
                ResourceType::Iron,
                create_deposit_legacy(
                    rng.random_range(0.15..0.28),
                    rng.random_range(0.7..0.88),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );
            resources.add_deposit(
                ResourceType::Aluminum,
                create_deposit_legacy(
                    rng.random_range(0.10..0.18),
                    rng.random_range(0.65..0.85),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );

            // Some pyroxene-related metals
            resources.add_deposit(
                ResourceType::Copper,
                create_deposit_legacy(
                    rng.random_range(0.0001..0.0005),
                    rng.random_range(0.5..0.75),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );

            info!(
                "Applied V-Type spectral profile to {}: Basaltic, high titanium",
                body_name
            );
        }

        // D-Type: Dark primitive - Very high volatiles, organic-rich
        AsteroidClass::DType => {
            // Extremely high volatiles
            resources.add_deposit(
                ResourceType::Water,
                create_deposit_legacy(
                    rng.random_range(0.35..0.55),
                    rng.random_range(0.7..0.9),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );
            resources.add_deposit(
                ResourceType::Methane,
                create_deposit_legacy(
                    rng.random_range(0.15..0.30),
                    rng.random_range(0.6..0.85),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );
            resources.add_deposit(
                ResourceType::Ammonia,
                create_deposit_legacy(
                    rng.random_range(0.12..0.25),
                    rng.random_range(0.6..0.85),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );
            resources.add_deposit(
                ResourceType::Hydrogen,
                create_deposit_legacy(
                    rng.random_range(0.10..0.20),
                    rng.random_range(0.6..0.8),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );
            resources.add_deposit(
                ResourceType::Nitrogen,
                create_deposit_legacy(
                    rng.random_range(0.05..0.12),
                    rng.random_range(0.5..0.75),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );

            // Very low metals
            resources.add_deposit(
                ResourceType::Silicates,
                create_deposit_legacy(
                    rng.random_range(0.05..0.15),
                    rng.random_range(0.4..0.6),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );
            resources.add_deposit(
                ResourceType::Iron,
                create_deposit_legacy(
                    rng.random_range(0.02..0.08),
                    rng.random_range(0.3..0.55),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );

            info!(
                "Applied D-Type spectral profile to {}: Primitive, extremely high volatiles",
                body_name
            );
        }

        // P-Type: Primitive - Similar to D-type, outer belt
        AsteroidClass::PType => {
            // Very high volatiles (slightly less than D-type)
            resources.add_deposit(
                ResourceType::Water,
                create_deposit_legacy(
                    rng.random_range(0.30..0.48),
                    rng.random_range(0.65..0.88),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );
            resources.add_deposit(
                ResourceType::Ammonia,
                create_deposit_legacy(
                    rng.random_range(0.10..0.22),
                    rng.random_range(0.6..0.82),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );
            resources.add_deposit(
                ResourceType::Methane,
                create_deposit_legacy(
                    rng.random_range(0.12..0.25),
                    rng.random_range(0.55..0.8),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );
            resources.add_deposit(
                ResourceType::Hydrogen,
                create_deposit_legacy(
                    rng.random_range(0.08..0.18),
                    rng.random_range(0.55..0.78),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );
            resources.add_deposit(
                ResourceType::CarbonDioxide,
                create_deposit_legacy(
                    rng.random_range(0.06..0.15),
                    rng.random_range(0.5..0.75),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );

            // Low metals
            resources.add_deposit(
                ResourceType::Silicates,
                create_deposit_legacy(
                    rng.random_range(0.08..0.18),
                    rng.random_range(0.45..0.65),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );
            resources.add_deposit(
                ResourceType::Iron,
                create_deposit_legacy(
                    rng.random_range(0.03..0.10),
                    rng.random_range(0.35..0.6),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );

            info!(
                "Applied P-Type spectral profile to {}: Primitive, very high volatiles",
                body_name
            );
        }

        AsteroidClass::Unknown => {
            // No spectral profile, fall through to normal generation
            return None;
        }
    }

    Some(resources)
}

/// Determine if an asteroid should have specialized resources
fn determine_asteroid_specialization(
    body_name: &str,
    rng: &mut impl Rng,
) -> AsteroidSpecialization {
    // Check for M-type asteroids (metallic) - should be rich in metals
    if body_name.contains("Psyche")
        || (body_name.contains("M-type") || body_name.contains("metallic"))
    {
        return AsteroidSpecialization::IronNickel;
    }

    // 15% chance for any asteroid to be specialized
    let roll = rng.random_range(0.0..1.0);
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
    deposit.reserve.concentration =
        (deposit.reserve.concentration * factor as f32).clamp(0.0001, 1.0);
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
    specialization: &AsteroidSpecialization,
) -> MineralDeposit {
    match specialization {
        AsteroidSpecialization::PlatinumRich => match resource {
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
        },

        AsteroidSpecialization::GoldRich => match resource {
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
        },

        AsteroidSpecialization::IronNickel => match resource {
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
        },

        AsteroidSpecialization::Carbonaceous => match resource {
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
        },

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
            rng.random_range(0.3..0.7), // Realistic ice composition
            rng.random_range(0.5..0.9), // Good accessibility (ice on surface)
        ),
        (r, true) if r.is_volatile() => (
            rng.random_range(0.0..0.02), // Almost none in inner system
            rng.random_range(0.0..0.1),  // Poor accessibility if any
        ),

        // Atmospheric gases - Present in atmospheres and trapped in ice
        // Outer system: gases trapped as ices in body composition (significant fraction)
        (r, false) if r.is_atmospheric_gas() => (
            rng.random_range(0.1..0.4), // Moderate in outer system (trapped in ice)
            rng.random_range(0.4..0.8), // Moderate-good accessibility
        ),
        // Inner system: atmospheres are a TINY fraction of planetary mass
        // Earth's entire atmosphere is only 0.000087% of its mass
        // Rocky planets can have 0% (Mercury) to ~0.01% (Venus is an outlier at ~0.008%)
        (r, true) if r.is_atmospheric_gas() => (
            rng.random_range(0.0..0.0001), // Realistic: atmosphere is <0.01% of planet mass
            rng.random_range(0.5..0.9),    // Gas is easy to collect from atmosphere
        ),

        // Construction materials - HIGH in inner system, present in outer
        // More realistic abundances based on actual planetary composition
        (r, true) if r.is_construction() => {
            let abundance = match resource {
                ResourceType::Iron => rng.random_range(0.15..0.35), // ~30% of Earth's composition
                ResourceType::Silicates => rng.random_range(0.25..0.45), // Major component
                ResourceType::Aluminum => rng.random_range(0.05..0.12), // ~8% of crust
                ResourceType::Titanium => rng.random_range(0.003..0.01), // ~0.6% of crust
                _ => rng.random_range(0.1..0.3),
            };
            (abundance, rng.random_range(0.6..0.95)) // Good accessibility (near surface)
        }
        (r, false) if r.is_construction() => (
            rng.random_range(0.05..0.2), // Present but less concentrated
            rng.random_range(0.1..0.3),  // Poor accessibility (buried under ice)
        ),

        // Noble gases - He3 is very rare but valuable for fusion
        (r, false) if r.is_noble_gas() => (
            rng.random_range(0.00001..0.0001), // Extremely rare He3
            rng.random_range(0.3..0.7),        // Moderate accessibility
        ),
        (r, true) if r.is_noble_gas() => (
            rng.random_range(0.000001..0.00001), // Trace He3 in inner system
            rng.random_range(0.1..0.3),          // Poor accessibility
        ),

        // Fissile materials - Rare everywhere, more realistic abundances
        (r, true) if r.is_fissile() => {
            let abundance = match resource {
                ResourceType::Uranium => rng.random_range(0.000001..0.00001), // ~3 ppm in Earth's crust
                ResourceType::Thorium => rng.random_range(0.000003..0.00003), // ~12 ppm in Earth's crust
                _ => rng.random_range(0.00001..0.0001),
            };
            (abundance, rng.random_range(0.3..0.6)) // Moderate accessibility
        }
        (r, false) if r.is_fissile() => (
            rng.random_range(0.0000001..0.000001), // Very rare in outer system
            rng.random_range(0.1..0.3),            // Poor accessibility
        ),

        // Precious metals - Very rare but valuable
        (r, true) if r.is_precious_metal() => {
            let abundance = match resource {
                ResourceType::Gold => rng.random_range(0.0000001..0.000001), // ~0.004 ppm in crust
                ResourceType::Silver => rng.random_range(0.0000003..0.000003), // ~0.08 ppm in crust
                ResourceType::Platinum => rng.random_range(0.00000001..0.0000001), // ~0.005 ppb in crust
                _ => rng.random_range(0.0000001..0.000001),
            };
            (abundance, rng.random_range(0.2..0.5)) // Harder to access (concentrated deposits)
        }
        (r, false) if r.is_precious_metal() => (
            rng.random_range(0.0000001..0.000001), // Rare in outer system too
            rng.random_range(0.1..0.3),            // Poor accessibility
        ),

        // Specialty materials - Moderate rarity
        (r, true) if r.is_specialty() => {
            let abundance = match resource {
                ResourceType::Copper => rng.random_range(0.00003..0.0001), // ~60 ppm in crust
                ResourceType::RareEarths => rng.random_range(0.00005..0.0002), // Variable, ~200 ppm combined
                _ => rng.random_range(0.0001..0.001),
            };
            (abundance, rng.random_range(0.3..0.7)) // Moderate accessibility
        }
        (r, false) if r.is_specialty() => (
            rng.random_range(0.00001..0.0001), // Lower abundance
            rng.random_range(0.2..0.5),        // Harder to access
        ),

        // Fallback (shouldn't happen)
        _ => (0.0, 0.0),
    };

    // Apply distance modifiers for more nuanced distribution
    let distance_factor = calculate_distance_modifier(resource, distance_au, frost_line_au);
    let mut final_abundance = (base_abundance * distance_factor).clamp(0.0, 1.0);
    let final_accessibility = (base_accessibility * distance_factor as f32).clamp(0.0, 1.0);

    // Cap volatile abundances to reasonable maximums to avoid unrealistic 100%+ volatiles
    if resource.is_volatile() {
        final_abundance = final_abundance.min(0.7);
    }

    // For atmospheric gases on planets/moons, use atmospheric deposit model
    // instead of the mineral deep-drilling model.
    // Only for bodies massive enough to gravitationally retain an atmosphere.
    // Bodies below ~10²¹ kg (smaller than Ceres) cannot hold gases — any O2/CO2/Ar
    // would be chemically bound in minerals, not atmospheric.
    if resource.is_atmospheric_gas()
        && matches!(body_type, BodyType::Planet | BodyType::Moon | BodyType::DwarfPlanet)
        && body_mass > 1.0e21
    {
        let atmospheric_mass_mt = (body_mass * final_abundance) / 1e9;
        // Inner system: gas is mostly atmospheric; small dissolved fraction
        // Outer system: gas is mostly trapped in ice ("dissolved" fraction is higher)
        let dissolved_frac = if is_inner { 0.1 } else { 0.5 };
        let bound_frac = if is_inner { 0.0 } else { 0.2 };
        return create_atmospheric_deposit(atmospheric_mass_mt, dissolved_frac, bound_frac, final_accessibility);
    }

    // Noble gases (He-3) on solid bodies: solar wind implantation model.
    // He-3 is deposited by solar wind into the top few meters of regolith.
    // On airless bodies (asteroids, small moons) this is the primary source.
    // On larger bodies with atmospheres, the atmosphere shields the surface,
    // so He-3 is negligible (handled by special profiles or skipped).
    if resource.is_noble_gas() {
        let total_mass_mt = (body_mass * final_abundance) / 1e9;
        return create_solar_wind_deposit(total_mass_mt, final_accessibility);
    }

    create_deposit_legacy(final_abundance, final_accessibility, body_mass, body_type)
}

/// Calculate a distance-based modifier for resource abundance
/// Provides smooth transitions rather than sharp cutoffs at the frost line
///
/// # Arguments
/// * `resource` - The type of resource
/// * `distance_au` - Distance from parent star in AU
/// * `frost_line_au` - Frost line distance for the parent star in AU
fn calculate_distance_modifier(
    resource: ResourceType,
    distance_au: f64,
    frost_line_au: f64,
) -> f64 {
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
    use rand::SeedableRng;

    // Use 1.0e9 kg (1 Mt) as standard test body mass so that
    // returned Megatons values are equivalent to fractions (0.0-1.0)
    const TEST_BODY_MASS: f64 = 1.0e9;

    #[test]
    fn test_default_frost_line_constant() {
        assert_eq!(DEFAULT_FROST_LINE_AU, 2.5);
    }

    #[test]
    fn test_generate_resources_inner_system() {
        let mut rng = rand::rng();
        let frost_line = 2.5;
        let resources = generate_resources_for_body(
            "TestBody",
            crate::plugins::solar_system_data::BodyType::Planet,
            TEST_BODY_MASS,
            None,
            1.0,
            frost_line,
            &mut rng,
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
        let mut rng = rand::rng();
        let frost_line = 2.5;
        let resources = generate_resources_for_body(
            "TestBody",
            crate::plugins::solar_system_data::BodyType::Planet,
            TEST_BODY_MASS,
            None,
            5.0,
            frost_line,
            &mut rng,
        );

        // Outer system should have volatiles (when present)
        let water = resources.get_abundance(&ResourceType::Water);
        let methane = resources.get_abundance(&ResourceType::Methane);

        // At least one volatile should be present with decent abundance
        assert!(water > 0.1 || methane > 0.1);
    }

    #[test]
    fn test_europa_special_profile() {
        let mut rng = rand::rng();
        let resources = generate_resources_for_body(
            "Europa",
            crate::plugins::solar_system_data::BodyType::Moon,
            TEST_BODY_MASS,
            None,
            5.2,
            2.5,
            &mut rng,
        );

        // Europa should have massive water abundance
        let water = resources.get_abundance(&ResourceType::Water);
        assert!(
            water > 0.5,
            "Europa should have >50% water (found {})",
            water
        );
    }

    #[test]
    fn test_mars_special_profile() {
        let mut rng = rand::rng();
        let resources = generate_resources_for_body(
            "Mars",
            crate::plugins::solar_system_data::BodyType::Planet,
            TEST_BODY_MASS,
            None,
            1.52,
            2.5,
            &mut rng,
        );

        // Mars should have ice caps and CO2
        let water = resources.get_abundance(&ResourceType::Water);
        let co2 = resources.get_abundance(&ResourceType::CarbonDioxide);

        assert!(water > 0.05, "Mars should have significant water ice");
        assert!(co2 > 0.02, "Mars should have CO2 ice caps");
    }

    #[test]
    fn test_atmospheric_gases_present() {
        let mut rng = rand::rng();
        let frost_line = 2.5;
        let resources = generate_resources_for_body(
            "TestBody",
            crate::plugins::solar_system_data::BodyType::Planet,
            TEST_BODY_MASS,
            None,
            1.0,
            frost_line,
            &mut rng,
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
        let mut rng = rand::rng();
        let frost_line = 2.5;
        let resources = generate_resources_for_body(
            "TestBody",
            crate::plugins::solar_system_data::BodyType::Planet,
            TEST_BODY_MASS,
            None,
            1.0,
            frost_line,
            &mut rng,
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
        let mut rng = rand::rng();
        let frost_line = 2.5;
        let deposit = generate_resource_deposit(
            ResourceType::Iron,
            TEST_BODY_MASS,
            BodyType::Planet,
            1.0,
            frost_line,
            true,
            &mut rng,
        );

        // Values should be within valid ranges (Mass should be >= 0)
        assert!(deposit.reserve.proven_crustal >= 0.0);
        assert!(deposit.accessibility >= 0.0 && deposit.accessibility <= 1.0);
    }

    #[test]
    fn test_different_frost_lines() {
        let mut rng = rand::rng();

        // Test with a cooler star (M-type) with closer frost line
        let m_star_frost_line = 0.5;
        let resources_m = generate_resources_for_body(
            "TestBody",
            crate::plugins::solar_system_data::BodyType::Planet,
            TEST_BODY_MASS,
            None,
            1.0,
            m_star_frost_line,
            &mut rng,
        );

        // At 1.0 AU from an M-star, we're well beyond the frost line
        // So we should have some volatiles present
        let water = resources_m.get_abundance(&ResourceType::Water);
        if water > 0.0 {
            assert!(
                water > 0.1,
                "Should have decent water abundance beyond frost line"
            );
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
            &mut rng,
        );

        // At 1.0 AU from an A-star, we're well inside the frost line
        // So we should have construction materials when present
        let iron = resources_a.get_abundance(&ResourceType::Iron);
        if iron > 0.0 {
            assert!(
                iron > 0.05,
                "Should have decent iron abundance inside frost line"
            );
        }
    }

    #[test]
    fn test_c_type_asteroid_spectral_profile() {
        let mut rng = rand::rng();
        let resources = generate_resources_for_body(
            "TestAsteroid",
            crate::plugins::solar_system_data::BodyType::Asteroid,
            TEST_BODY_MASS,
            Some(AsteroidClass::CType),
            2.8,
            2.5,
            &mut rng,
        );

        // C-Type should have 4-7% water (scientifically validated)
        let water = resources.get_abundance(&ResourceType::Water);
        assert!(
            water >= 0.04 && water <= 0.08,
            "C-Type should have 4-7% water (scientific range), found: {:.1}%",
            water * 100.0
        );
    }

    #[test]
    fn test_s_type_asteroid_spectral_profile() {
        let mut rng = rand::rng();
        let resources = generate_resources_for_body(
            "TestAsteroid",
            crate::plugins::solar_system_data::BodyType::Asteroid,
            TEST_BODY_MASS,
            Some(AsteroidClass::SType),
            2.8,
            2.5,
            &mut rng,
        );

        // S-Type should have high silicates and iron
        let silicates = resources.get_abundance(&ResourceType::Silicates);
        let iron = resources.get_abundance(&ResourceType::Iron);
        assert!(silicates > 0.20, "S-Type should have >20% silicates");
        assert!(iron > 0.10, "S-Type should have >10% iron");
    }

    #[test]
    fn test_m_type_asteroid_spectral_profile() {
        let mut rng = rand::rng();
        let resources = generate_resources_for_body(
            "TestAsteroid",
            crate::plugins::solar_system_data::BodyType::Asteroid,
            TEST_BODY_MASS,
            Some(AsteroidClass::MType),
            2.8,
            2.5,
            &mut rng,
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
        let mut rng = rand::rng();
        let resources = generate_resources_for_body(
            "TestAsteroid",
            crate::plugins::solar_system_data::BodyType::Asteroid,
            TEST_BODY_MASS,
            Some(AsteroidClass::VType),
            2.8,
            2.5,
            &mut rng,
        );

        // V-Type should have high titanium
        let titanium = resources.get_abundance(&ResourceType::Titanium);
        assert!(titanium > 0.010, "V-Type should have >1.0% titanium");
    }

    #[test]
    fn test_d_type_asteroid_spectral_profile() {
        let mut rng = rand::rng();
        let resources = generate_resources_for_body(
            "TestAsteroid",
            crate::plugins::solar_system_data::BodyType::Asteroid,
            TEST_BODY_MASS,
            Some(AsteroidClass::DType),
            5.2,
            2.5,
            &mut rng,
        );

        // D-Type should have extremely high volatiles
        let water = resources.get_abundance(&ResourceType::Water);
        let methane = resources.get_abundance(&ResourceType::Methane);
        assert!(water > 0.20, "D-Type should have >20% water");
        assert!(methane > 0.05, "D-Type should have >5% methane");
    }

    #[test]
    fn test_gas_giants_no_solid_ice() {
        let mut rng = rand::rng();

        // Test Jupiter - should NOT have water deposits (only atmospheric hydrogen/helium)
        let jupiter_mass = 1.8982e27; // kg
        let jupiter_resources = generate_resources_for_body(
            "Jupiter",
            crate::plugins::solar_system_data::BodyType::Planet,
            jupiter_mass,
            None,
            5.2,
            2.5,
            &mut rng,
        );

        // Jupiter should have hydrogen but NO water (gas giant, not ice giant)
        let jupiter_water = jupiter_resources.get_abundance(&ResourceType::Water);
        let jupiter_hydrogen = jupiter_resources.get_abundance(&ResourceType::Hydrogen);
        let jupiter_total_mt = jupiter_mass / 1e9; // Convert kg to Mt

        assert_eq!(
            jupiter_water, 0.0,
            "Jupiter should have NO solid water deposits (gas giant)"
        );

        // Hydrogen should be a large fraction of Jupiter's mass
        if jupiter_hydrogen > 0.0 {
            let hydrogen_fraction = jupiter_hydrogen / jupiter_total_mt;
            assert!(
                hydrogen_fraction > 0.5,
                "Jupiter should have hydrogen (>50% of mass), found: {:.1}%",
                hydrogen_fraction * 100.0
            );
        }

        // Test Saturn
        let saturn_mass = 5.6834e26; // kg
        let saturn_resources = generate_resources_for_body(
            "Saturn",
            crate::plugins::solar_system_data::BodyType::Planet,
            saturn_mass,
            None,
            9.5,
            2.5,
            &mut rng,
        );

        let saturn_water = saturn_resources.get_abundance(&ResourceType::Water);
        let saturn_hydrogen = saturn_resources.get_abundance(&ResourceType::Hydrogen);
        let saturn_total_mt = saturn_mass / 1e9; // Convert kg to Mt

        assert_eq!(
            saturn_water, 0.0,
            "Saturn should have NO solid water deposits (gas giant)"
        );

        // Hydrogen should be a large fraction of Saturn's mass
        if saturn_hydrogen > 0.0 {
            let hydrogen_fraction = saturn_hydrogen / saturn_total_mt;
            assert!(
                hydrogen_fraction > 0.5,
                "Saturn should have hydrogen (>50% of mass), found: {:.1}%",
                hydrogen_fraction * 100.0
            );
        }
    }

    #[test]
    fn test_mars_realistic_water() {
        let mut rng = rand::rng();

        // Mars mass: 6.4171×10²³ kg = 6.4171×10^14 Mt
        // Scientific estimate: 5M km³ × 920 kg/m³ × 10^9 m³/km³ = 4.6×10^18 kg = 4.6×10^9 Mt
        let mars_mass = 6.4171e23;
        let mars_resources = generate_resources_for_body(
            "Mars",
            crate::plugins::solar_system_data::BodyType::Planet,
            mars_mass,
            None,
            1.52,
            2.5,
            &mut rng,
        );

        let water_deposits = mars_resources.get_deposit(&ResourceType::Water);
        assert!(water_deposits.is_some(), "Mars should have water deposits");

        if let Some(deposit) = water_deposits {
            let total_water_mt = deposit.reserve.proven_crustal
                + deposit.reserve.deep_deposits
                + deposit.reserve.planetary_bulk;

            // Should be around 4.6 billion Mt (4.6×10^9 Mt), allow some variance
            assert!(
                total_water_mt > 1e9,
                "Mars should have at least 1 billion Mt of water"
            );
            assert!(
                total_water_mt < 1e11,
                "Mars should NOT have excessive water (> 100 billion Mt)"
            );

            // More specific: should be in the billions of Mt range
            assert!(total_water_mt > 1e9 && total_water_mt < 1e10, 
                "Mars water should be in billions of Mt range (scientific: 4.6×10^9 Mt), found: {:.2e} Mt", 
                total_water_mt);
        }
    }

    #[test]
    fn test_moon_realistic_water() {
        let mut rng = rand::rng();

        // Moon mass: 7.342×10²² kg = 7.342×10^13 Mt
        // Scientific estimate: 600 million metric tons = 6×10^8 metric tons = 600 Mt (NOT 6×10^8 Mt!)
        let moon_mass = 7.342e22;
        let moon_resources = generate_resources_for_body(
            "Moon",
            crate::plugins::solar_system_data::BodyType::Moon,
            moon_mass,
            None,
            1.0,
            2.5,
            &mut rng,
        );

        let water_deposits = moon_resources.get_deposit(&ResourceType::Water);
        assert!(
            water_deposits.is_some(),
            "Moon should have water deposits in polar craters"
        );

        if let Some(deposit) = water_deposits {
            let total_water_mt = deposit.reserve.proven_crustal
                + deposit.reserve.deep_deposits
                + deposit.reserve.planetary_bulk;

            // Should be around 600 Mt, allow some variance
            assert!(
                total_water_mt > 100.0,
                "Moon should have at least 100 Mt of water"
            );
            assert!(
                total_water_mt < 10000.0,
                "Moon should NOT have excessive water (> 10,000 Mt)"
            );

            // More specific: should be in the hundreds of Mt range
            assert!(total_water_mt > 200.0 && total_water_mt < 2000.0, 
                "Moon water should be in hundreds of Mt range (scientific: 600 Mt), found: {:.2e} Mt", 
                total_water_mt);
        }
    }

    #[test]
    fn test_moon_he3_surface_only() {
        let mut rng = rand::rng();
        let moon_mass = 7.342e22;
        let moon_resources = generate_resources_for_body(
            "Moon",
            crate::plugins::solar_system_data::BodyType::Moon,
            moon_mass,
            None,
            1.0,
            2.5,
            &mut rng,
        );

        let he3 = moon_resources.get_deposit(&ResourceType::Helium3);
        assert!(he3.is_some(), "Moon should have He-3");
        let he3 = he3.unwrap();

        // He-3 is solar-wind implanted in surface regolith only (~1 Mt total)
        let total = he3.reserve.proven_crustal + he3.reserve.deep_deposits + he3.reserve.planetary_bulk;
        assert!(total < 10.0, "Moon He-3 should be ~1 Mt, not {:.1} Mt", total);
        assert!(total > 0.1, "Moon He-3 should exist");

        // Almost all should be in proven (surface regolith), no bulk
        assert!(he3.reserve.proven_crustal > 0.5, "He-3 should be mostly surface");
        assert!(he3.reserve.planetary_bulk < 0.01, "He-3 should have no deep bulk deposits");
        assert!(!he3.is_atmospheric, "He-3 on Moon is not atmospheric");
    }

    #[test]
    fn test_small_moon_no_atmospheric_gases() {
        let mut rng = rand::rng();
        // Phobos-like small moon (mass < 1e21 kg) should NOT get atmospheric gas model
        let phobos_mass = 1.07e16;
        let resources = generate_resources_for_body(
            "TestSmallMoon",
            crate::plugins::solar_system_data::BodyType::Moon,
            phobos_mass,
            None,
            1.5, // Inner system
            2.5,
            &mut rng,
        );

        // Any gases that exist should NOT be marked as atmospheric
        for resource_type in ResourceType::all() {
            if resource_type.is_atmospheric_gas() {
                if let Some(deposit) = resources.get_deposit(resource_type) {
                    assert!(!deposit.is_atmospheric,
                        "Small moon should not have atmospheric {} - too small to hold atmosphere",
                        resource_type.display_name());
                }
            }
        }
    }

    #[test]
    fn test_c_type_asteroid_water_realistic() {
        let mut rng = rand::rng();

        // C-type asteroids should have 4-7% water by weight (scientifically validated)
        let resources = generate_resources_for_body(
            "TestCTypeAsteroid",
            crate::plugins::solar_system_data::BodyType::Asteroid,
            TEST_BODY_MASS,
            Some(AsteroidClass::CType),
            2.8,
            2.5,
            &mut rng,
        );

        let water_abundance = resources.get_abundance(&ResourceType::Water);
        assert!(
            water_abundance >= 0.04 && water_abundance <= 0.08,
            "C-type asteroids should have 4-7% water by weight (scientific range), found: {:.1}%",
            water_abundance * 100.0
        );
    }

    #[test]
    fn test_s_type_asteroid_low_water() {
        let mut rng = rand::rng();

        // S-type asteroids should have <1% water (mostly as hydroxyl in minerals)
        let resources = generate_resources_for_body(
            "TestSTypeAsteroid",
            crate::plugins::solar_system_data::BodyType::Asteroid,
            TEST_BODY_MASS,
            Some(AsteroidClass::SType),
            2.8,
            2.5,
            &mut rng,
        );

        let water_abundance = resources.get_abundance(&ResourceType::Water);
        // Should be less than 1%, likely in the 0.2-0.7% range
        if water_abundance > 0.0 {
            assert!(
                water_abundance < 0.01,
                "S-type asteroids should have <1% water (scientific), found: {:.2}%",
                water_abundance * 100.0
            );
        }
    }

    #[test]
    fn test_m_type_asteroid_negligible_water() {
        let mut rng = rand::rng();

        // M-type asteroids should have negligible/no water (anhydrous metallic cores)
        let resources = generate_resources_for_body(
            "TestMTypeAsteroid",
            crate::plugins::solar_system_data::BodyType::Asteroid,
            TEST_BODY_MASS,
            Some(AsteroidClass::MType),
            2.8,
            2.5,
            &mut rng,
        );

        let water_abundance = resources.get_abundance(&ResourceType::Water);
        // M-types are anhydrous - should have essentially no water
        assert_eq!(
            water_abundance,
            0.0,
            "M-type asteroids should have negligible water (anhydrous), found: {:.3}%",
            water_abundance * 100.0
        );
    }

    #[test]
    fn test_procedural_generation_realistic_all_resources() {
        let mut rng = rand::rng();

        // Test an Earth-like planet (inner system, rocky)
        // Earth mass: 5.972×10^24 kg
        let earth_mass = 5.972e24;
        let resources = generate_resources_for_body(
            "TestEarthLike",
            crate::plugins::solar_system_data::BodyType::Planet,
            earth_mass,
            None,
            1.0, // 1 AU
            2.5, // Sun-like frost line
            &mut rng,
        );

        // Total body mass in megatons (Mt); resource abundances are stored as Mt
        let total_mass_mt = earth_mass / 1.0e9;

        // Verify construction materials are present and realistic for inner planet
        let iron = resources.get_abundance(&ResourceType::Iron);
        let silicates = resources.get_abundance(&ResourceType::Silicates);
        let aluminum = resources.get_abundance(&ResourceType::Aluminum);

        let iron_fraction = if total_mass_mt > 0.0 {
            iron / total_mass_mt
        } else {
            0.0
        };
        let silicates_fraction = if total_mass_mt > 0.0 {
            silicates / total_mass_mt
        } else {
            0.0
        };
        let aluminum_fraction = if total_mass_mt > 0.0 {
            aluminum / total_mass_mt
        } else {
            0.0
        };

        if iron_fraction > 0.0 {
            assert!(
                iron_fraction >= 0.15 && iron_fraction <= 0.35,
                "Inner planet iron should be 15-35% (realistic crustal abundance), found: {:.1}%",
                iron_fraction * 100.0
            );
        }

        if silicates_fraction > 0.0 {
            assert!(
                silicates_fraction >= 0.25 && silicates_fraction <= 0.45,
                "Inner planet silicates should be 25-45% (major crustal component), found: {:.1}%",
                silicates_fraction * 100.0
            );
        }

        if aluminum_fraction > 0.0 {
            assert!(aluminum_fraction >= 0.05 && aluminum_fraction <= 0.12, 
                "Inner planet aluminum should be 5-12% (realistic crustal abundance), found: {:.1}%", 
                aluminum_fraction * 100.0);
        }

        // Volatiles should be very low or absent in inner system
        let water = resources.get_abundance(&ResourceType::Water);
        let water_fraction = if total_mass_mt > 0.0 {
            water / total_mass_mt
        } else {
            0.0
        };
        if water_fraction > 0.0 {
            assert!(
                water_fraction < 0.02,
                "Inner planet water should be <2% (very low volatiles), found: {:.2}%",
                water_fraction * 100.0
            );
        }
    }

    #[test]
    fn test_procedural_outer_system_volatiles() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);

        // Test an outer system body (beyond frost line)
        let test_mass = 1.0e21; // Small icy body
        let total_mass_mt = test_mass / 1.0e9;

        let resources = generate_resources_for_body(
            "TestIcyBody",
            crate::plugins::solar_system_data::BodyType::Moon,
            test_mass,
            None,
            10.0, // 10 AU (well beyond frost line)
            2.5,  // Sun-like frost line
            &mut rng,
        );

        // Outer system bodies should have high volatiles
        let water = resources.get_abundance(&ResourceType::Water);
        let water_fraction = if total_mass_mt > 0.0 {
            water / total_mass_mt
        } else {
            0.0
        };



        if water_fraction > 0.0 {
            assert!(
                water_fraction >= 0.3 && water_fraction <= 0.7,
                "Outer system body should have 30-70% water ice, found: {:.1}%",
                water_fraction * 100.0
            );
        }

        // Construction materials should be low
        let iron = resources.get_abundance(&ResourceType::Iron);
        let iron_fraction = if total_mass_mt > 0.0 {
            iron / total_mass_mt
        } else {
            0.0
        };

        if iron_fraction > 0.0 {
            assert!(
                iron_fraction < 0.2,
                "Outer system body should have <20% iron, found: {:.1}%",
                iron_fraction * 100.0
            );
        }
    }

    #[test]
    fn test_tier_calculations_realistic() {
        let mut rng = rand::rng();

        // Test that tier calculations (proven/deep/bulk) are realistic
        // Earth mass for reference
        let earth_mass = 5.972e24;
        let resources = generate_resources_for_body(
            "TestTierCalc",
            crate::plugins::solar_system_data::BodyType::Planet,
            earth_mass,
            None,
            1.0,
            2.5,
            &mut rng,
        );

        // Check iron deposits if present
        if let Some(iron_deposit) = resources.get_deposit(&ResourceType::Iron) {
            let proven = iron_deposit.reserve.proven_crustal;
            let deep = iron_deposit.reserve.deep_deposits;
            let bulk = iron_deposit.reserve.planetary_bulk;
            let total = proven + deep + bulk;

            // For planets, proven should be tiny fraction of total (crustal accessibility)
            if total > 0.0 {
                let proven_fraction = proven / total;
                let deep_fraction = deep / total;

                // Proven should be << 1% for planets (Earth's proven iron ~200 Gt out of millions of Gt total)
                assert!(proven_fraction < 0.01, 
                    "Planetary proven reserves should be <1% of total (realistic crustal access), found: {:.4}%", 
                    proven_fraction * 100.0);

                // Deep should be larger than proven but still small
                assert!(
                    deep_fraction > proven_fraction,
                    "Deep reserves should be larger than proven"
                );
                assert!(
                    deep_fraction < 0.1,
                    "Deep reserves should still be <10% of total for planets"
                );

                // Bulk should be the vast majority
                let bulk_fraction = bulk / total;
                assert!(bulk_fraction > 0.89, 
                    "Bulk reserves should be >89% of total for planets (most inaccessible), found: {:.1}%", 
                    bulk_fraction * 100.0);
            }
        }
    }

    #[test]
    fn test_asteroid_tier_calculations() {
        let mut rng = rand::rng();

        // Asteroids should have much higher proven/deep fractions (fully accessible)
        let asteroid_mass = 1.0e15; // ~1 km asteroid
        let resources = generate_resources_for_body(
            "TestAsteroidTier",
            crate::plugins::solar_system_data::BodyType::Asteroid,
            asteroid_mass,
            Some(AsteroidClass::MType), // Metallic asteroid
            2.8,
            2.5,
            &mut rng,
        );

        if let Some(iron_deposit) = resources.get_deposit(&ResourceType::Iron) {
            let proven = iron_deposit.reserve.proven_crustal;
            let deep = iron_deposit.reserve.deep_deposits;
            let bulk = iron_deposit.reserve.planetary_bulk;
            let total = proven + deep + bulk;

            if total > 0.0 {
                let proven_fraction = proven / total;

                // Asteroids should have much higher proven fraction (30-70%)
                assert!(proven_fraction > 0.25, 
                    "Asteroid proven reserves should be >25% of total (fully accessible), found: {:.1}%", 
                    proven_fraction * 100.0);
                assert!(
                    proven_fraction < 0.75,
                    "Asteroid proven reserves should be <75% of total, found: {:.1}%",
                    proven_fraction * 100.0
                );
            }
        }
    }

    #[test]
    fn test_earth_special_profile() {
        let mut rng = rand::rng();

        let earth_mass = 5.972e24; // kg
        let resources = generate_resources_for_body(
            "Earth",
            crate::plugins::solar_system_data::BodyType::Planet,
            earth_mass,
            None,
            1.0,
            2.5,
            &mut rng,
        );

        // Earth should NOT have ammonia (essentially zero in nature)
        let ammonia = resources.get_deposit(&ResourceType::Ammonia);
        assert!(
            ammonia.is_none(),
            "Earth should NOT have ammonia deposits"
        );

        // Earth should have water (oceans) - mostly surface accessible
        let water = resources.get_deposit(&ResourceType::Water);
        assert!(water.is_some(), "Earth should have water");
        if let Some(w) = water {
            let total_water = w.total_megatons();
            // ~1.4 billion Mt (oceans + freshwater)
            assert!(total_water > 1e9 && total_water < 2e9,
                "Earth water should be ~1.4 billion Mt, found: {:.2e}", total_water);
            // Water should be overwhelmingly in proven (oceans), not deep/bulk
            let proven_fraction = w.reserve.proven_crustal / total_water;
            assert!(proven_fraction > 0.95,
                "Earth water should be >95% proven (oceans), found: {:.1}%", proven_fraction * 100.0);
            // Water is NOT atmospheric
            assert!(!w.is_atmospheric, "Earth water should not be flagged as atmospheric");
        }

        // Earth should have nitrogen (atmospheric)
        let n2 = resources.get_deposit(&ResourceType::Nitrogen);
        assert!(n2.is_some(), "Earth should have nitrogen");
        if let Some(n) = n2 {
            let total_n2 = n.total_megatons();
            // ~4.02×10^9 Mt atmospheric + small dissolved
            assert!(total_n2 > 3e9 && total_n2 < 5e9,
                "Earth N2 should be ~4 billion Mt, found: {:.2e}", total_n2);
            // For atmospheric gases, proven (atmospheric) should be the dominant tier
            let atm_fraction = n.reserve.proven_crustal / total_n2;
            assert!(atm_fraction > 0.9,
                "Earth N2 should be >90% atmospheric, found: {:.1}%", atm_fraction * 100.0);
            // N2 IS atmospheric
            assert!(n.is_atmospheric, "Earth N2 should be flagged as atmospheric");
        }

        // Earth should have all major metals
        assert!(resources.get_deposit(&ResourceType::Iron).is_some(), "Earth should have iron");
        assert!(resources.get_deposit(&ResourceType::Aluminum).is_some(), "Earth should have aluminum");
        assert!(resources.get_deposit(&ResourceType::Titanium).is_some(), "Earth should have titanium");

        // Precious metals should have visible proven reserves (USGS data)
        let gold = resources.get_deposit(&ResourceType::Gold);
        assert!(gold.is_some(), "Earth should have gold");
        if let Some(g) = gold {
            assert!(g.reserve.proven_crustal > 0.01,
                "Earth gold proven should be >0.01 Mt (~54kt USGS), found: {:.4}", g.reserve.proven_crustal);
            assert!(!g.is_atmospheric, "Gold should not be atmospheric");
        }

        let platinum = resources.get_deposit(&ResourceType::Platinum);
        assert!(platinum.is_some(), "Earth should have platinum");
        if let Some(p) = platinum {
            assert!(p.reserve.proven_crustal > 0.01,
                "Earth platinum proven should be >0.01 Mt (~69kt USGS), found: {:.4}", p.reserve.proven_crustal);
        }

        assert!(resources.get_deposit(&ResourceType::Silver).is_some(), "Earth should have silver");
        assert!(resources.get_deposit(&ResourceType::Copper).is_some(), "Earth should have copper");
        assert!(resources.get_deposit(&ResourceType::RareEarths).is_some(), "Earth should have rare earths");
        assert!(resources.get_deposit(&ResourceType::Uranium).is_some(), "Earth should have uranium");
    }

    #[test]
    fn test_venus_special_profile() {
        let mut rng = rand::rng();

        let venus_mass = 4.867e24;
        let resources = generate_resources_for_body(
            "Venus",
            crate::plugins::solar_system_data::BodyType::Planet,
            venus_mass,
            None,
            0.72,
            2.5,
            &mut rng,
        );

        // Venus should have massive CO2 atmosphere
        let co2 = resources.get_deposit(&ResourceType::CarbonDioxide);
        assert!(co2.is_some(), "Venus should have CO2");
        if let Some(c) = co2 {
            let total_co2 = c.total_megatons();
            assert!(total_co2 > 4e11,
                "Venus CO2 should be >400 billion Mt, found: {:.2e}", total_co2);
        }

        // Venus should NOT have water
        let water = resources.get_deposit(&ResourceType::Water);
        assert!(water.is_none(), "Venus should NOT have water");
    }

    #[test]
    fn test_mercury_special_profile() {
        let mut rng = rand::rng();

        let mercury_mass = 3.301e23;
        let resources = generate_resources_for_body(
            "Mercury",
            crate::plugins::solar_system_data::BodyType::Planet,
            mercury_mass,
            None,
            0.39,
            2.5,
            &mut rng,
        );

        // Mercury should have high iron (60% by mass)
        let iron = resources.get_deposit(&ResourceType::Iron);
        assert!(iron.is_some(), "Mercury should have iron");

        // Mercury should have polar ice (small amount)
        let water = resources.get_deposit(&ResourceType::Water);
        assert!(water.is_some(), "Mercury should have polar ice");
        if let Some(w) = water {
            assert!(w.total_megatons() < 2000.0,
                "Mercury water should be <2000 Mt (polar craters only)");
        }

        // Mercury should have He-3 (solar wind implanted)
        assert!(resources.get_deposit(&ResourceType::Helium3).is_some(), "Mercury should have He-3");
    }

    #[test]
    fn test_atmospheric_gas_tier_distribution() {
        // Verify that atmospheric deposits use the atmospheric model
        // (proven tier is dominant, not deep/bulk)
        let deposit = create_atmospheric_deposit(1000.0, 0.1, 0.0, 0.9);

        assert_eq!(deposit.reserve.proven_crustal, 1000.0,
            "Atmospheric tier should equal input atmospheric mass");
        assert!((deposit.reserve.deep_deposits - 100.0).abs() < 0.01,
            "Trapped/dissolved should be 10% of atmospheric");
        assert_eq!(deposit.reserve.planetary_bulk, 0.0,
            "No chemically bound tier when bound_fraction is 0");
        assert!(deposit.is_atmospheric, "Atmospheric deposit should have is_atmospheric = true");

        // Verify trace deposit
        let trace = create_trace_atmospheric_deposit(50.0, 0.3);
        assert_eq!(trace.reserve.proven_crustal, 50.0);
        assert_eq!(trace.reserve.deep_deposits, 0.0);
        assert_eq!(trace.reserve.planetary_bulk, 0.0);
        assert!(trace.is_atmospheric, "Trace atmospheric deposit should have is_atmospheric = true");

        // Verify mineral deposit does NOT have is_atmospheric
        let mineral = create_deposit_legacy(0.1, 0.5, 1e20, BodyType::Planet);
        assert!(!mineral.is_atmospheric, "Mineral deposit should have is_atmospheric = false");
    }
}
