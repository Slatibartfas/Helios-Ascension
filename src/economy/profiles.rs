//! Resource generation profiles for known celestial bodies and asteroid spectral classes.
//!
//! This module contains the large data-table functions that assign scientifically-grounded
//! resource inventories to specific named bodies and asteroid types.

use bevy::prelude::*;
use rand::Rng;

use super::components::{MineralDeposit, PlanetResources};
use super::generation::{
    create_atmospheric_deposit, create_deposit_from_absolute_mass, create_deposit_legacy,
    create_solar_wind_deposit,
};
use super::types::ResourceType;
use crate::plugins::solar_system_data::{AsteroidClass, BodyType};

/// Apply special resource profiles for known celestial bodies
/// Returns Some(resources) for special bodies, None for normal generation
pub(super) fn apply_special_body_profile(
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
            // Deuterium: D/H ratio ~2.5×10⁻⁵ in Jupiter's hydrogen
            resources.add_deposit(
                ResourceType::Deuterium,
                create_atmospheric_deposit(
                    (body_mass * 0.000025) / 1e9,
                    0.1,
                    0.0,
                    0.03, // very hard to extract from deep atmosphere
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
            // Deuterium: D/H ratio ~2.25×10⁻⁵ in Saturn's hydrogen
            resources.add_deposit(
                ResourceType::Deuterium,
                create_atmospheric_deposit(
                    (body_mass * 0.0000225) / 1e9,
                    0.1,
                    0.0,
                    0.03,
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
            // Deuterium in ice giant hydrogen
            resources.add_deposit(
                ResourceType::Deuterium,
                create_atmospheric_deposit(
                    (body_mass * 0.000022) / 1e9,
                    0.1,
                    0.0,
                    0.04,
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
            // Deuterium in ice giant hydrogen
            resources.add_deposit(
                ResourceType::Deuterium,
                create_atmospheric_deposit(
                    (body_mass * 0.000025) / 1e9,
                    0.1,
                    0.0,
                    0.04,
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

            // === NEW RESOURCES (2026 baseline) ===

            // Nickel — ~2.7 Mt/yr (refined)
            // Proven: ~95 Mt (USGS 2024 reserves)
            // Deep: ~350 Mt (USGS identified resources + laterite)
            // Bulk: ~1.8×10¹³ Mt (core is ~5% Ni, mantle ~0.2%)
            resources.add_deposit(
                ResourceType::Nickel,
                MineralDeposit::new(95.0, 350.0, 1.8e13, 0.0054, 0.7),
            );

            // Tungsten — ~0.084 Mt/yr (84,000 tonnes)
            // Proven: ~3.8 Mt (USGS 2024 reserves)
            // Deep: ~12 Mt (USGS identified resources)
            // Bulk: Crustal average ~1.3 ppm
            resources.add_deposit(
                ResourceType::Tungsten,
                MineralDeposit::new(3.8, 12.0, 7.76e9, 0.0000168, 0.4),
            );

            // Carbon — ~9,000 Mt/yr (coal equiv + industrial)
            // Proven: ~1,100,000 Mt (proven coal reserves, USGS/WEC 2024)
            // Deep: ~10,000,000 Mt (total coal resources + carbonate rocks)
            // Bulk: Crustal average ~200 ppm (mostly in carbonate sediment)
            resources.add_deposit(
                ResourceType::Carbon,
                MineralDeposit::new(1_100_000.0, 10_000_000.0, 1.19e12, 0.18, 0.8),
            );

            // Phosphorus — ~0.22 Mt/yr (220,000 tonnes elemental P)
            // Proven: ~71,000 Mt phosphate rock (USGS 2024; ~13% P content = ~9,200 Mt P)
            // Deep: ~300,000 Mt (USGS total resources)
            // Bulk: Crustal average ~1,050 ppm
            resources.add_deposit(
                ResourceType::Phosphorus,
                MineralDeposit::new(9_200.0, 39_000.0, 6.27e12, 0.000044, 0.5),
            );

            // Deuterium — ocean D/H ratio 1.5576×10⁻⁴
            // Total ocean hydrogen mass: ~1.11×10¹¹ Mt
            // Deuterium: 0.01558% = ~1.73×10⁷ Mt (practically inexhaustible)
            // Proven: 2.1×10⁷ Mt (extractable from seawater)
            resources.add_deposit(
                ResourceType::Deuterium,
                MineralDeposit::new(21_000_000.0, 0.0, 0.0, 0.0002, 0.6),
            );

            // Lithium — ~0.13 Mt/yr (130,000 tonnes)
            // Proven: ~28 Mt (USGS 2024 reserves)
            // Deep: ~98 Mt (USGS identified resources)
            // Bulk: Crustal average ~20 ppm
            resources.add_deposit(
                ResourceType::Lithium,
                MineralDeposit::new(28.0, 98.0, 1.19e11, 0.0000026, 0.35),
            );

            // Sulfur — ~80 Mt/yr (elemental + recovered)
            // Proven: ~600 Mt (USGS 2024 reserves, mostly from oil/gas)
            // Deep: ~5,000 Mt (volcanic + evaporite deposits)
            // Bulk: Crustal average ~350 ppm
            resources.add_deposit(
                ResourceType::Sulfur,
                MineralDeposit::new(600.0, 5_000.0, 2.09e12, 0.016, 0.6),
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
            // Deuterium from subsurface ocean (D/H ~1.5×10⁻⁴ in water)
            resources.add_deposit(
                ResourceType::Deuterium,
                create_deposit_legacy(0.000013, 0.3, body_mass, BodyType::Moon),
            );
            // Sulfur from irradiated surface (from Io's volcanic output)
            resources.add_deposit(
                ResourceType::Sulfur,
                create_deposit_legacy(0.002, 0.5, body_mass, BodyType::Moon),
            );
            // Nickel in rocky interior
            resources.add_deposit(
                ResourceType::Nickel,
                create_deposit_legacy(0.005, 0.1, body_mass, BodyType::Moon),
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
            // Nickel: meteoritic enrichment of regolith (~1.5% from meteorites)
            resources.add_deposit(
                ResourceType::Nickel,
                create_deposit_legacy(0.01, 0.5, body_mass, BodyType::Planet),
            );
            // Sulfur: sulfate minerals in regolith (MgSO4 etc. from Spirit/Opportunity)
            resources.add_deposit(
                ResourceType::Sulfur,
                create_deposit_legacy(0.003, 0.6, body_mass, BodyType::Planet),
            );
            // Carbon: as carbonates in martian soil (~0.5% estimate)
            resources.add_deposit(
                ResourceType::Carbon,
                create_deposit_legacy(0.005, 0.4, body_mass, BodyType::Planet),
            );
            // Phosphorus: trace in basaltic rock (~0.1%)
            resources.add_deposit(
                ResourceType::Phosphorus,
                create_deposit_legacy(0.001, 0.3, body_mass, BodyType::Planet),
            );
            // Deuterium: from polar ice (D/H enriched ~5× vs Earth on Mars)
            resources.add_deposit(
                ResourceType::Deuterium,
                create_deposit_from_absolute_mass(700.0, 0.3, BodyType::Planet),
            );
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
            // Nickel: trace in lunar regolith from meteoritic input (~0.3%)
            resources.add_deposit(
                ResourceType::Nickel,
                create_deposit_legacy(0.003, 0.4, body_mass, BodyType::Moon),
            );
            // Sulfur: volcanic glasses contain ~0.05% S
            resources.add_deposit(
                ResourceType::Sulfur,
                create_deposit_legacy(0.0005, 0.3, body_mass, BodyType::Moon),
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
            // Carbon: abundant in hydrocarbon form (ethane, propane, etc.)
            resources.add_deposit(
                ResourceType::Carbon,
                create_deposit_legacy(0.08, 0.8, body_mass, BodyType::Moon),
            );
            // Phosphorus: trace in rocky interior
            resources.add_deposit(
                ResourceType::Phosphorus,
                create_deposit_legacy(0.0005, 0.2, body_mass, BodyType::Moon),
            );
            // Deuterium: from water ice subsurface
            resources.add_deposit(
                ResourceType::Deuterium,
                create_deposit_legacy(0.000015, 0.2, body_mass, BodyType::Moon),
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
            // Deuterium from subsurface ocean
            resources.add_deposit(
                ResourceType::Deuterium,
                create_deposit_legacy(0.000012, 0.6, body_mass, BodyType::Moon),
            );
            // Phosphorus: detected in plume material
            resources.add_deposit(
                ResourceType::Phosphorus,
                create_deposit_legacy(0.0003, 0.7, body_mass, BodyType::Moon),
            );
            // Sulfur: trace in hydrothermal fluids
            resources.add_deposit(
                ResourceType::Sulfur,
                create_deposit_legacy(0.001, 0.5, body_mass, BodyType::Moon),
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
            // Carbon: C-type body, abundant carbonaceous material
            resources.add_deposit(
                ResourceType::Carbon,
                create_deposit_legacy(0.03, 0.5, body_mass, BodyType::DwarfPlanet),
            );
            // Phosphorus: present in aqueously altered minerals
            resources.add_deposit(
                ResourceType::Phosphorus,
                create_deposit_legacy(0.002, 0.4, body_mass, BodyType::DwarfPlanet),
            );
            // Sulfur: sulfate/sulfide minerals
            resources.add_deposit(
                ResourceType::Sulfur,
                create_deposit_legacy(0.005, 0.4, body_mass, BodyType::DwarfPlanet),
            );
            // Nickel: in chondritic material
            resources.add_deposit(
                ResourceType::Nickel,
                create_deposit_legacy(0.01, 0.4, body_mass, BodyType::DwarfPlanet),
            );
            // Deuterium: from water ice
            resources.add_deposit(
                ResourceType::Deuterium,
                create_deposit_legacy(0.00006, 0.3, body_mass, BodyType::DwarfPlanet),
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
            // Nickel: similar to Earth's bulk composition
            resources.add_deposit(
                ResourceType::Nickel,
                create_deposit_legacy(0.018, 0.15, body_mass, BodyType::Planet),
            );
            // Tungsten: refractory, present in crust
            resources.add_deposit(
                ResourceType::Tungsten,
                create_deposit_legacy(0.0000013, 0.10, body_mass, BodyType::Planet),
            );
            // Carbon: CO2 atmosphere contains carbon (as the CO2 is already counted,
            // this represents crustal carbonate rocks)
            resources.add_deposit(
                ResourceType::Carbon,
                create_deposit_legacy(0.0001, 0.1, body_mass, BodyType::Planet),
            );
            // Sulfur: SO2 in atmosphere (~150 ppm) and volcanic surface deposits
            resources.add_deposit(
                ResourceType::Sulfur,
                create_deposit_legacy(0.001, 0.3, body_mass, BodyType::Planet),
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
            // Nickel: large metallic core, high nickel content (~5% of core)
            resources.add_deposit(
                ResourceType::Nickel,
                create_deposit_legacy(0.03, 0.6, body_mass, BodyType::Planet),
            );
            // Sulfur: MESSENGER detected volatile-rich surface (~4% S by weight)
            resources.add_deposit(
                ResourceType::Sulfur,
                create_deposit_legacy(0.04, 0.7, body_mass, BodyType::Planet),
            );
            // Tungsten: refractory siderophile, concentrated in metallic core
            resources.add_deposit(
                ResourceType::Tungsten,
                create_deposit_legacy(0.00001, 0.4, body_mass, BodyType::Planet),
            );
            // Carbon: MESSENGER found ~3-4% carbon on surface (graphite)
            resources.add_deposit(
                ResourceType::Carbon,
                create_deposit_legacy(0.035, 0.6, body_mass, BodyType::Planet),
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
pub(super) fn apply_spectral_class_profile(
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

            // Carbon: 3-8% in carbonaceous chondrites — the best Carbon source
            resources.add_deposit(
                ResourceType::Carbon,
                create_deposit_legacy(
                    rng.random_range(0.03..0.08),
                    rng.random_range(0.6..0.85),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );
            // Phosphorus: 0.1-0.3% — the *only* reliable source outside Earth
            resources.add_deposit(
                ResourceType::Phosphorus,
                create_deposit_legacy(
                    rng.random_range(0.001..0.003),
                    rng.random_range(0.5..0.7),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );
            // Sulfur: 1-3% in sulfide minerals
            resources.add_deposit(
                ResourceType::Sulfur,
                create_deposit_legacy(
                    rng.random_range(0.01..0.03),
                    rng.random_range(0.5..0.75),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );
            // Nickel: 1-2% (moderate, in silicate matrix)
            resources.add_deposit(
                ResourceType::Nickel,
                create_deposit_legacy(
                    rng.random_range(0.01..0.02),
                    rng.random_range(0.4..0.65),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );
            // Deuterium: trace, bound in hydrated minerals (D/H ~1.5×10⁻⁴)
            resources.add_deposit(
                ResourceType::Deuterium,
                create_deposit_legacy(
                    rng.random_range(0.000006..0.00001),
                    rng.random_range(0.4..0.6),
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
            // Nickel: 0.5-2% in metallic grains within silicate matrix
            resources.add_deposit(
                ResourceType::Nickel,
                create_deposit_legacy(
                    rng.random_range(0.005..0.02),
                    rng.random_range(0.6..0.8),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );
            // Sulfur: 0.1-0.5% in sulfide inclusions
            resources.add_deposit(
                ResourceType::Sulfur,
                create_deposit_legacy(
                    rng.random_range(0.001..0.005),
                    rng.random_range(0.4..0.65),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );
            // Carbon: trace only in S-type
            resources.add_deposit(
                ResourceType::Carbon,
                create_deposit_legacy(
                    rng.random_range(0.0001..0.001),
                    rng.random_range(0.3..0.5),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );
            // Tungsten: trace in refractory silicates
            resources.add_deposit(
                ResourceType::Tungsten,
                create_deposit_legacy(
                    rng.random_range(0.00001..0.0001),
                    rng.random_range(0.4..0.6),
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
            // Nickel: 5-15% — the *primary* Nickel source (nickel-iron alloy)
            resources.add_deposit(
                ResourceType::Nickel,
                create_deposit_legacy(
                    rng.random_range(0.05..0.15),
                    rng.random_range(0.85..0.98),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );
            // Tungsten: 0.01-0.1% — concentrated in metallic bodies
            resources.add_deposit(
                ResourceType::Tungsten,
                create_deposit_legacy(
                    rng.random_range(0.0001..0.001),
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
            // Nickel: 1-3% in differentiated body
            resources.add_deposit(
                ResourceType::Nickel,
                create_deposit_legacy(
                    rng.random_range(0.01..0.03),
                    rng.random_range(0.6..0.8),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );
            // Tungsten: trace in basaltic material
            resources.add_deposit(
                ResourceType::Tungsten,
                create_deposit_legacy(
                    rng.random_range(0.00001..0.00005),
                    rng.random_range(0.5..0.7),
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
            // Carbon: 5-12% — organic-rich primitive material
            resources.add_deposit(
                ResourceType::Carbon,
                create_deposit_legacy(
                    rng.random_range(0.05..0.12),
                    rng.random_range(0.6..0.85),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );
            // Phosphorus: 0.05-0.2% in primitive organics
            resources.add_deposit(
                ResourceType::Phosphorus,
                create_deposit_legacy(
                    rng.random_range(0.0005..0.002),
                    rng.random_range(0.5..0.7),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );
            // Sulfur: 2-5% in primitive volatile-rich material
            resources.add_deposit(
                ResourceType::Sulfur,
                create_deposit_legacy(
                    rng.random_range(0.02..0.05),
                    rng.random_range(0.55..0.8),
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
            // Carbon: 4-10% in primitive material
            resources.add_deposit(
                ResourceType::Carbon,
                create_deposit_legacy(
                    rng.random_range(0.04..0.10),
                    rng.random_range(0.55..0.8),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );
            // Phosphorus: 0.03-0.15% in primitive organics
            resources.add_deposit(
                ResourceType::Phosphorus,
                create_deposit_legacy(
                    rng.random_range(0.0003..0.0015),
                    rng.random_range(0.45..0.65),
                    body_mass,
                    BodyType::Asteroid,
                ),
            );
            // Sulfur: 1.5-4% in volatile-rich primitive material
            resources.add_deposit(
                ResourceType::Sulfur,
                create_deposit_legacy(
                    rng.random_range(0.015..0.04),
                    rng.random_range(0.5..0.75),
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
