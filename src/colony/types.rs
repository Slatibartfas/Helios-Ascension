use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Types of buildings that can be constructed on a colony
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub enum BuildingType {
    // Infrastructure - Basic colony infrastructure
    /// Converts local volatiles into breathable atmosphere
    LifeSupport,
    /// Provides living and working space
    HabitatDome,
    /// Standard housing for habitable worlds
    Housing,
    /// Provides shelter on airless/hostile bodies
    UndergroundHabitat,
    /// v3.2 canary 12 (2026-08-07): starter-tier inflatable
    /// shelter, 1,000 residents. The smallest housing in the
    /// catalog — first building on a new colony. Material cost:
    /// 3 Fe + 5 Si (buildable from the new-colony bootstrap).
    /// See `BALANCE_PATCHES_v0.5.md` §0.I (v3.2).
    HabitatTent,
    /// v3.2 canary 12 (2026-08-07): starter-tier prefab
    /// habitat, 10,000 residents. Second-tier starter for
    /// growing colonies. Material cost: 10 Fe + 15 Si + 1 Cu +
    /// 3 Al. See `BALANCE_PATCHES_v0.5.md` §0.I (v3.2).
    HabitatModule,
    /// Off-world water extraction (atmospheric condenser / ice miner).
    /// Required for non-breathable colony life support; body-restricted
    /// to `[None]` atmospheres (see `BALANCE_PATCHES_v0.5.md` §8.2.1).
    /// v0.5 canary 2.
    WaterProcessor,

    // Mining & Industry — per-resource dedicated base mines
    // (v0.5.2: replaced the legacy generic `Mine`/`Refinery`/`DeepDrill`/
    // `LaserDrill`/`StripMine`/`HydrocarbonExtractor`/`RecyclingCenter`
    // buildings + `MiningEfficiency`/`DeepMiningEfficiency`/
    // `BulkMiningEfficiency` modifier + share-fold. Each base mine below
    // produces ONE resource at `base_yield × deposit.accessibility ×
    // yield_mult`. The `line = "Mine"` field groups them for tier-based
    // tech upgrades that apply to the whole line (see `data.rs`).
    //
    // Calibration target: 25 mines × base_yield × 0.6 (Earth accessibility)
    // ≈ world demand (USGS 2024/2026). See BALANCE_PATCHES_v0.5.md §5.
    //
    // Construction materials (9):
    /// Iron ore extraction (open-pit / underground). 120 Mt/yr per build.
    IronMine,
    /// Aluminum (bauxite) extraction. 5 Mt/yr per build.
    AluminumMine,
    /// Titanium (rutile / ilmenite) extraction. 0.02 Mt/yr per build.
    TitaniumMine,
    /// Silicates (granite / basalt / quartz) aggregate quarry. 700 Mt/yr per build.
    SilicatesMine,
    /// Nickel ore extraction. 0.2 Mt/yr per build.
    NickelMine,
    /// Tungsten (wolframite / scheelite) extraction. 0.005 Mt/yr per build.
    TungstenMine,
    /// Carbon (coal / graphite) extraction. 350 Mt/yr per build.
    CarbonMine,
    /// Chromium (chromite) extraction. 2 Mt/yr per build.
    ChromiumMine,
    /// Magnesium (magnesite / dolomite / seawater) extraction. 0.07 Mt/yr per build.
    MagnesiumMine,
    // Precious metals (3 — v0.5.1):
    /// Placer / lode gold extraction (cyanidation). 0.0001 Mt/yr per build.
    GoldMine,
    /// Lead-zinc byproduct-style silver extraction. 0.001 Mt/yr per build.
    SilverMine,
    /// Platinum-group-metal extraction from layered intrusions. 0.00001 Mt/yr per build.
    PlatinumMine,
    // Strategic materials (6):
    /// Copper ore extraction (chalcopyrite / porphyry). 1.5 Mt/yr per build.
    CopperMine,
    /// Rare-earth element extraction (bastnäsite / monazite). 0.025 Mt/yr per build.
    RareEarthsMine,
    /// Lithium (spodumene / brine) extraction. 0.012 Mt/yr per build.
    LithiumMine,
    /// Sulfur (Frasch / pyrite roasting) extraction. 5 Mt/yr per build.
    SulfurMine,
    /// Phosphate rock (apatite) extraction. 0.003 Mt/yr per build.
    PhosphorusMine,
    /// Cobalt ore extraction. 0.015 Mt/yr per build.
    CobaltMine,
    /// Fluorite (CaF₂) extraction / fluorospar mining. 0.2 Mt/yr per build.
    FluorineMine,
    // Fissile (2):
    /// Uranium (U₃O₈) extraction. 0.003 Mt/yr per build.
    UraniumMine,
    /// Thorium (monazite) extraction. 0.0007 Mt/yr per build.
    ThoriumMine,
    // Hydrocarbons (1):
    /// Methane (natural gas / clathrate / Titan lakes) extraction. 270 Mt/yr per build.
    MethaneExtractor,
    // Heavy water (1):
    /// Deuterium (heavy water) extraction from seawater / ice. 0.5 Mt/yr per build.
    DeuteriumExtractor,
    // Helium-3 (canary 3, body-restricted):
    /// Helium-3 mining from solar-wind-implanted regolith (Moon, asteroids)
    /// or primordial gas-giant atmospheres. 0.5 Mt/yr per build. Body-restricted
    /// to `[Moon, GasGiant, Asteroid]`; requires `lunar_colony` tech.
    He3Mine,
    // Manufactures goods and components
    Factory,

    // Atmospheric Harvesting
    /// Collects gases from a body's atmosphere
    AtmosphericProcessor,

    // Advanced Mining (tech-gated) — REMOVED in v0.5.2
    // (DeepDrill/LaserDrill/StripMine/HydrocarbonExtractor/RecyclingCenter
    // are obsolete; their per-resource functionality is captured in the
    // dedicated base mines above + the `tier/line` system in
    // `BuildingDefinition` which lets future tech upgrades apply to the
    // whole `line = "Mine"` line.)

    // AutoMines — per-resource orbital / asteroidic extraction rigs
    // (v0.5.2: dedicated asteroid-mining buildings, body-restricted to
    // `[Asteroid, Moon, GasGiant]`, yields calibrated at ~1/10 of the
    // surface base mine (orbital extraction is harder than surface).
    // Each AutoMine reads the body's bulk deposit and applies its
    // accessibility × a small flat per-build yield. They are NOT
    // generic — one AutoMine per resource — because the operator bar
    // demands predictable per-build numbers and ore body composition
    // varies wildly between asteroids. A "generic" AutoMine would
    // need a heuristic dispatcher and obscure the player's build
    // decisions. Body restriction: see `BuildingDefinition.
    // allowed_body_types`. Requires `asteroid_mining` tech.
    AutoIronMine,
    AutoAluminumMine,
    AutoTitaniumMine,
    AutoSilicatesMine,
    AutoNickelMine,
    AutoTungstenMine,
    AutoCarbonMine,
    AutoChromiumMine,
    AutoMagnesiumMine,
    AutoGoldMine,
    AutoSilverMine,
    AutoPlatinumMine,
    AutoCopperMine,
    AutoRareEarthsMine,
    AutoLithiumMine,
    AutoSulfurMine,
    AutoPhosphorusMine,
    AutoCobaltMine,
    AutoFluorineMine,
    AutoUraniumMine,
    AutoThoriumMine,
    AutoMethaneExtractor,
    AutoDeuteriumExtractor,
    AutoHe3Mine,
    AutoWaterProcessor,

    // Logistics - Reduce logistics penalty
    /// Electromagnetic launcher for bulk cargo between bodies
    MassDriver,
    /// Space elevator for efficient surface-to-orbit transport
    OrbitalLift,
    /// Ground-based cargo distribution
    CargoTerminal,

    // Power
    /// Solar panel arrays
    SolarPower,
    /// Nuclear fission reactor
    FissionReactor,
    /// Advanced fusion power plant
    FusionReactor,
    /// Deuterium-tritium fusion plant with lithium breeding support
    DTFusionReactor,
    /// Deuterium-helium-3 fusion plant for premium low-neutron power
    DHe3FusionReactor,
    /// Thorium molten-salt reactor for long-duration baseload generation
    ThoriumReactor,
    /// Fast breeder reactor that creates plutonium while generating power
    BreederReactor,

    // Population & Growth
    /// Agricultural facilities for food production
    AgriDome,
    /// Standard farming for habitable worlds
    Farm,
    /// Medical and cloning facilities to boost population growth
    MedicalCenter,

    // Research
    /// Scientific research laboratory
    ResearchLab,
    /// Engineering workshop for component development
    EngineeringBay,
    /// AI computation cluster (requires neural_networks tech)
    AiCluster,

    // Financial & Commerce
    /// Commercial hub generating wealth from trade
    CommercialHub,
    /// Financial centre for banking and investment
    FinancialCenter,
    // v3.10 (GRA-22c Phase 4C-2): `TradePort` removed. The
    // trade mechanic moved to `LaunchSite` (a launch resource;
    // see `LaunchCapacity` in `economy::components`). The
    // maintenance draw that TradePort used to carry is now
    // absorbed by `CommercialHub` (Iron +0.05, Cu +0.0007,
    // Ti +0.0005, Poly +0.0001, Water +0.005 — the per-build
    // bump that lets 500 CommercialHubs absorb the 50 TradePorts
    // worth of maintenance on Earth-start).

    // Military & Shipbuilding
    /// Orbital shipyard for constructing vessels (requires orbital_construction tech)
    Shipyard,
    /// Ground-based missile silo (requires missile_systems tech)
    MissileSilo,
    /// Rocket launch site for orbital access
    LaunchSite,

    // Advanced Industry
    /// Chemical plant for synthesizing volatiles and polymers
    ChemicalPlant,

    // Advanced Power Generation
    /// Wind turbine farm
    WindFarm,
    /// Hydroelectric power dam
    HydroelectricDam,
    /// Geothermal energy plant (requires geothermal_energy tech)
    GeothermalPlant,
    /// Fossil-fuel coal power plant
    CoalPowerPlant,
    /// Natural gas combustion turbine plant
    NaturalGasPlant,

    // Advanced Industry (new)
    /// High-precision microchip and electronics industry (requires semiconductor_manufacturing tech)
    SemiconductorFab,
    /// Pharmaceutical and biomedical production sector
    PharmaceuticalPlant,

    // Water & Environment
    /// Purifies contaminated water supplies
    WaterTreatmentPlant,
    /// Extracts fresh water from oceans / brine (requires desalination tech)
    DesalinationPlant,

    // Advanced Agriculture
    /// Controlled-environment crop growing
    Greenhouse,
    /// Fish, shellfish, and aquatic protein farming
    AquacultureFacility,

    // Digital Infrastructure
    /// Planetary-scale computation and data-storage infrastructure
    DataCenter,

    // Advanced Space
    /// Advanced multi-pad orbital launch complex
    SpacePort,
    /// Ground-based anti-orbital / anti-missile defense battery
    GroundDefenseBattery,

    // Storage
    /// Bulk resource depot expanding civilisation-wide stockpile capacity
    Warehouse,

    // Survey (v0.5.0, GRA-83 PR-E)
    /// Permanent orbital installation that continuously surveys the host
    /// body and grants a mining yield bonus to local mines. Per-body:
    /// effect does not transfer to moons or parent planets.
    OrbitalSurveyStation,
}

impl BuildingType {
    /// Get all building types in display order
    pub fn all() -> &'static [BuildingType] {
        use BuildingType::*;
        &[
            // ── Infrastructure ───────────────────────────────────────────
            Housing,
            LifeSupport,
            HabitatDome,
            UndergroundHabitat,
            // v3.2 canary 12 (2026-08-07): starter-tier housing
            HabitatTent,
            HabitatModule,
            WaterProcessor,
            WaterTreatmentPlant,
            DesalinationPlant,
            // ── Mining & Industry (v0.5.2: per-resource dedicated mines) ─
            // Construction materials (9)
            IronMine,
            AluminumMine,
            TitaniumMine,
            SilicatesMine,
            NickelMine,
            TungstenMine,
            CarbonMine,
            ChromiumMine,
            MagnesiumMine,
            // Precious metals (3 — v0.5.1)
            GoldMine,
            SilverMine,
            PlatinumMine,
            // Strategic materials (6)
            CopperMine,
            RareEarthsMine,
            LithiumMine,
            SulfurMine,
            PhosphorusMine,
            CobaltMine,
            FluorineMine,
            // Fissile (2)
            UraniumMine,
            ThoriumMine,
            // Hydrocarbons (1)
            MethaneExtractor,
            // Heavy water (1)
            DeuteriumExtractor,
            // He-3 (1 — canary 3, body-restricted to [Moon, GasGiant, Asteroid])
            He3Mine,
            // AutoMines (22 — orbital/asteroid mining, body-restricted)
            AutoIronMine,
            AutoAluminumMine,
            AutoTitaniumMine,
            AutoSilicatesMine,
            AutoNickelMine,
            AutoTungstenMine,
            AutoCarbonMine,
            AutoChromiumMine,
            AutoMagnesiumMine,
            AutoGoldMine,
            AutoSilverMine,
            AutoPlatinumMine,
            AutoCopperMine,
            AutoRareEarthsMine,
            AutoLithiumMine,
            AutoSulfurMine,
            AutoPhosphorusMine,
            AutoCobaltMine,
            AutoFluorineMine,
            AutoUraniumMine,
            AutoThoriumMine,
            AutoMethaneExtractor,
            AutoDeuteriumExtractor,
            AutoHe3Mine,
            AutoWaterProcessor,
            // Generic industry / refining
            Factory,
            AtmosphericProcessor,
            ChemicalPlant,
            // ── Logistics ────────────────────────────────────────────────
            MassDriver,
            OrbitalLift,
            CargoTerminal,
            Warehouse,
            // ── Power ────────────────────────────────────────────────────
            SolarPower,
            Farm,
            FissionReactor,
            FusionReactor,
            DTFusionReactor,
            DHe3FusionReactor,
            ThoriumReactor,
            BreederReactor,
            AgriDome,
            MedicalCenter,
            ResearchLab,
            EngineeringBay,
            AiCluster,
            CommercialHub,
            FinancialCenter,
            // v3.10 (GRA-22c Phase 4C-2): TradePort removed —
            // its maintenance draw is reassigned to CommercialHub
            // and its wealth-generation role is gone (trade
            // revenue flows through FinancialCenter +
            // CommercialHub + future LaunchSite transfer fees).
            Shipyard,
            MissileSilo,
            LaunchSite,
            WindFarm,
            HydroelectricDam,
            GeothermalPlant,
            CoalPowerPlant,
            NaturalGasPlant,
            SemiconductorFab,
            PharmaceuticalPlant,
            Greenhouse,
            AquacultureFacility,
            DataCenter,
            SpacePort,
            GroundDefenseBattery,
            OrbitalSurveyStation,
        ]
    }

    /// Display name for UI
    pub fn display_name(&self) -> &'static str {
        match self {
            BuildingType::LifeSupport => "Life Support",
            BuildingType::HabitatDome => "Habitat Dome",
            BuildingType::Housing => "Housing Complex",
            BuildingType::UndergroundHabitat => "Underground Habitat",
            // v3.2 canary 12 (2026-08-07): starter-tier housing
            BuildingType::HabitatTent => "Habitat Tent",
            BuildingType::HabitatModule => "Habitat Module",
            BuildingType::WaterProcessor => "Water Processor",
            // Construction (9)
            BuildingType::IronMine => "Iron Mine",
            BuildingType::AluminumMine => "Aluminum Mine",
            BuildingType::TitaniumMine => "Titanium Mine",
            BuildingType::SilicatesMine => "Silicates Quarry",
            BuildingType::NickelMine => "Nickel Mine",
            BuildingType::TungstenMine => "Tungsten Mine",
            BuildingType::CarbonMine => "Carbon Mine",
            BuildingType::ChromiumMine => "Chromium Mine",
            BuildingType::MagnesiumMine => "Magnesium Mine",
            // Precious (3 — v0.5.1)
            BuildingType::GoldMine => "Gold Mine",
            BuildingType::SilverMine => "Silver Mine",
            BuildingType::PlatinumMine => "Platinum Mine",
            // Strategic (6)
            BuildingType::CopperMine => "Copper Mine",
            BuildingType::RareEarthsMine => "Rare Earths Mine",
            BuildingType::LithiumMine => "Lithium Mine",
            BuildingType::SulfurMine => "Sulfur Mine",
            BuildingType::PhosphorusMine => "Phosphorus Mine",
            BuildingType::CobaltMine => "Cobalt Mine",
            BuildingType::FluorineMine => "Fluorine Mine",
            // Fissile (2)
            BuildingType::UraniumMine => "Uranium Mine",
            BuildingType::ThoriumMine => "Thorium Mine",
            // Hydrocarbons (1)
            BuildingType::MethaneExtractor => "Methane Extractor",
            // Heavy water (1)
            BuildingType::DeuteriumExtractor => "Deuterium Extractor",
            // He-3 (1 — body-restricted)
            BuildingType::He3Mine => "Helium-3 Mine",
            // AutoMines (22)
            BuildingType::AutoIronMine => "Auto Iron Mine",
            BuildingType::AutoAluminumMine => "Auto Aluminum Mine",
            BuildingType::AutoTitaniumMine => "Auto Titanium Mine",
            BuildingType::AutoSilicatesMine => "Auto Silicates Quarry",
            BuildingType::AutoNickelMine => "Auto Nickel Mine",
            BuildingType::AutoTungstenMine => "Auto Tungsten Mine",
            BuildingType::AutoCarbonMine => "Auto Carbon Mine",
            BuildingType::AutoChromiumMine => "Auto Chromium Mine",
            BuildingType::AutoMagnesiumMine => "Auto Magnesium Mine",
            BuildingType::AutoGoldMine => "Auto Gold Mine",
            BuildingType::AutoSilverMine => "Auto Silver Mine",
            BuildingType::AutoPlatinumMine => "Auto Platinum Mine",
            BuildingType::AutoCopperMine => "Auto Copper Mine",
            BuildingType::AutoRareEarthsMine => "Auto Rare Earths Mine",
            BuildingType::AutoLithiumMine => "Auto Lithium Mine",
            BuildingType::AutoSulfurMine => "Auto Sulfur Mine",
            BuildingType::AutoPhosphorusMine => "Auto Phosphorus Mine",
            BuildingType::AutoCobaltMine => "Auto Cobalt Mine",
            BuildingType::AutoFluorineMine => "Auto Fluorine Mine",
            BuildingType::AutoUraniumMine => "Auto Uranium Mine",
            BuildingType::AutoThoriumMine => "Auto Thorium Mine",
            BuildingType::AutoMethaneExtractor => "Auto Methane Extractor",
            BuildingType::AutoDeuteriumExtractor => "Auto Deuterium Extractor",
            BuildingType::AutoHe3Mine => "Auto He-3 Mine",
            BuildingType::AutoWaterProcessor => "Auto Water Extractor",
            // Generic industry / refining
            BuildingType::Factory => "Factory",
            BuildingType::AtmosphericProcessor => "Atmospheric Processor",
            BuildingType::ChemicalPlant => "Chemical Plant",
            // Logistics
            BuildingType::MassDriver => "Mass Driver",
            BuildingType::OrbitalLift => "Orbital Lift",
            BuildingType::CargoTerminal => "Cargo Terminal",
            BuildingType::Warehouse => "Resource Depot",
            // Power
            BuildingType::SolarPower => "Solar Power Plant",
            BuildingType::FissionReactor => "Fission Reactor",
            BuildingType::FusionReactor => "Fusion Reactor",
            BuildingType::DTFusionReactor => "D-T Fusion Reactor",
            BuildingType::DHe3FusionReactor => "D-He3 Fusion Reactor",
            BuildingType::ThoriumReactor => "Thorium Reactor",
            BuildingType::BreederReactor => "Breeder Reactor",
            BuildingType::WindFarm => "Wind Farm",
            BuildingType::HydroelectricDam => "Hydroelectric Dam",
            BuildingType::GeothermalPlant => "Geothermal Plant",
            BuildingType::CoalPowerPlant => "Coal Power Sector",
            BuildingType::NaturalGasPlant => "Gas Power Sector",
            // Population / Research / Industry
            BuildingType::AgriDome => "Agricultural Dome",
            BuildingType::Farm => "Farm",
            BuildingType::Greenhouse => "Greenhouse Complex",
            BuildingType::AquacultureFacility => "Aquaculture Complex",
            BuildingType::MedicalCenter => "Medical Center",
            BuildingType::ResearchLab => "Research Lab",
            BuildingType::EngineeringBay => "Engineering Bay",
            BuildingType::AiCluster => "AI Cluster",
            BuildingType::SemiconductorFab => "Electronics Industry",
            BuildingType::PharmaceuticalPlant => "Pharmaceutical Sector",
            BuildingType::DataCenter => "Computation Hub",
            // Financial / Commerce / Military
            BuildingType::CommercialHub => "Commercial Hub",
            BuildingType::FinancialCenter => "Financial Center",
            // v3.10 (GRA-22c Phase 4C-2): TradePort removed.
            BuildingType::Shipyard => "Shipyard",
            BuildingType::MissileSilo => "Missile Silo",
            BuildingType::LaunchSite => "Launch Site",
            BuildingType::SpacePort => "Space Port",
            BuildingType::GroundDefenseBattery => "Ground Defense Battery",
            // Water / environment
            BuildingType::WaterTreatmentPlant => "Water Management Complex",
            BuildingType::DesalinationPlant => "Desalination Complex",
            // Survey
            BuildingType::OrbitalSurveyStation => "Orbital Survey Station",
        }
    }
    pub fn description(&self) -> &'static str {
        match self {
            BuildingType::LifeSupport => "Converts local volatiles into breathable atmosphere",
            BuildingType::Housing => "Standard residential buildings for habitable worlds",
            BuildingType::HabitatDome => "Provides living and working space for colonists",
            BuildingType::UndergroundHabitat => "Shelter on airless or hostile bodies",
            // v3.2 canary 12 (2026-08-07): starter-tier housing
            BuildingType::HabitatTent => "Inflatable emergency shelter for 1,000 residents. v3.2 starter-tier — first building on a new colony.",
            BuildingType::HabitatModule => "Prefab habitat module for 10,000 residents. v3.2 starter-tier — second-tier for growing colonies.",
            BuildingType::WaterProcessor => "Atmospheric condenser / regolith ice miner. Extracts 16 Mt/yr water for non-breathable colony life support; body-restricted to non-breathable atmospheres (buildable on Moon, Mars, asteroids, gas-giant moons — not on Earth-like worlds).",
            // Construction mines (9)
            BuildingType::IronMine => "Iron ore extraction (open-pit / underground). 120 Mt/yr per build, scaled by deposit.accessibility.",
            BuildingType::AluminumMine => "Aluminum (bauxite) extraction. 5 Mt/yr per build, scaled by deposit.accessibility.",
            BuildingType::TitaniumMine => "Titanium (rutile / ilmenite) extraction. 0.02 Mt/yr per build, scaled by deposit.accessibility.",
            BuildingType::SilicatesMine => "Silicates (granite / basalt / quartz) aggregate quarry. 700 Mt/yr per build, scaled by deposit.accessibility.",
            BuildingType::NickelMine => "Nickel ore extraction. 0.2 Mt/yr per build, scaled by deposit.accessibility.",
            BuildingType::TungstenMine => "Tungsten (wolframite / scheelite) extraction. 0.005 Mt/yr per build, scaled by deposit.accessibility.",
            BuildingType::CarbonMine => "Carbon (coal / graphite) extraction. 350 Mt/yr per build, scaled by deposit.accessibility.",
            BuildingType::ChromiumMine => "Chromium (chromite) extraction. 2 Mt/yr per build, scaled by deposit.accessibility.",
            BuildingType::MagnesiumMine => "Magnesium (magnesite / dolomite / seawater) extraction. 0.07 Mt/yr per build, scaled by deposit.accessibility.",
            // Precious metals (3 — v0.5.1)
            BuildingType::GoldMine => "Placer / lode gold extraction with cyanidation. 0.0001 Mt/yr Au per mine (USGS 2026 real-world scale; ~3,200 troy oz/yr — small-mine scale). Direct deposit (not via share-fold).",
            BuildingType::SilverMine => "Lead-zinc byproduct-style silver extraction. 0.001 Mt/yr Ag per mine (USGS 2026 real-world; ~1,000 t/yr — large-mine scale). Direct deposit (not via share-fold).",
            BuildingType::PlatinumMine => "Platinum-group-metal extraction from layered intrusions (Bushveld / Norilsk analog). 0.00001 Mt/yr Pt per mine (USGS 2026 real-world; ~10 t/yr — realistic PGM output). Direct deposit (not via share-fold).",
            // Strategic (6)
            BuildingType::CopperMine => "Copper ore extraction (chalcopyrite / porphyry). 1.5 Mt/yr per build, scaled by deposit.accessibility.",
            BuildingType::RareEarthsMine => "Rare-earth element extraction (bastnäsite / monazite). 0.025 Mt/yr per build, scaled by deposit.accessibility.",
            BuildingType::LithiumMine => "Lithium (spodumene / brine) extraction. 0.012 Mt/yr per build, scaled by deposit.accessibility.",
            BuildingType::SulfurMine => "Sulfur (Frasch / pyrite roasting) extraction. 5 Mt/yr per build, scaled by deposit.accessibility.",
            BuildingType::PhosphorusMine => "Phosphate rock (apatite) extraction. 0.003 Mt/yr per build, scaled by deposit.accessibility.",
            BuildingType::CobaltMine => "Cobalt ore extraction. 0.015 Mt/yr per build, scaled by deposit.accessibility.",
            BuildingType::FluorineMine => "Fluorite (CaF₂) extraction / fluorospar mining. 0.2 Mt/yr per build, scaled by deposit.accessibility.",
            // Fissile (2)
            BuildingType::UraniumMine => "Uranium (U₃O₈) extraction. 0.003 Mt/yr per build, scaled by deposit.accessibility.",
            BuildingType::ThoriumMine => "Thorium (monazite) extraction. 0.0007 Mt/yr per build, scaled by deposit.accessibility.",
            // Hydrocarbons (1)
            BuildingType::MethaneExtractor => "Methane (natural gas / clathrate / Titan lakes) extraction. 270 Mt/yr per build, scaled by deposit.accessibility.",
            // Heavy water (1)
            BuildingType::DeuteriumExtractor => "Deuterium (heavy water) extraction from seawater / ice. 0.5 Mt/yr per build, scaled by deposit.accessibility.",
            // He-3 (1 — body-restricted)
            BuildingType::He3Mine => "Helium-3 mining from solar-wind-implanted regolith (Moon, asteroids) or primordial gas-giant atmospheres. 0.5 Mt/yr per build, scaled by deposit.accessibility. Body-restricted to [Moon, GasGiant, Asteroid]; requires lunar_colony tech.",
            // AutoMines (22) — orbital/asteroid mining, body-restricted
            BuildingType::AutoIronMine => "Orbital iron extraction rig for asteroids. 12 Mt/yr per build, scaled by deposit.accessibility. Body-restricted to [Asteroid, Moon, GasGiant]; requires asteroid_mining tech.",
            BuildingType::AutoAluminumMine => "Orbital aluminum extraction rig for asteroids. 0.5 Mt/yr per build. Body-restricted to [Asteroid, Moon, GasGiant].",
            BuildingType::AutoTitaniumMine => "Orbital titanium extraction rig. 0.002 Mt/yr per build. Body-restricted to [Asteroid, Moon, GasGiant].",
            BuildingType::AutoSilicatesMine => "Orbital silicates quarry rig. 70 Mt/yr per build. Body-restricted to [Asteroid, Moon, GasGiant].",
            BuildingType::AutoNickelMine => "Orbital nickel extraction rig (key for M-type asteroids). 0.02 Mt/yr per build. Body-restricted to [Asteroid, Moon, GasGiant].",
            BuildingType::AutoTungstenMine => "Orbital tungsten extraction rig. 0.0005 Mt/yr per build. Body-restricted to [Asteroid, Moon, GasGiant].",
            BuildingType::AutoCarbonMine => "Orbital carbon extraction rig (carbonaceous chondrites). 35 Mt/yr per build. Body-restricted to [Asteroid, Moon, GasGiant].",
            BuildingType::AutoChromiumMine => "Orbital chromium extraction rig. 0.2 Mt/yr per build. Body-restricted to [Asteroid, Moon, GasGiant].",
            BuildingType::AutoMagnesiumMine => "Orbital magnesium extraction rig. 0.007 Mt/yr per build. Body-restricted to [Asteroid, Moon, GasGiant].",
            BuildingType::AutoGoldMine => "Orbital gold extraction rig (rare, mostly M-type asteroids). 0.00001 Mt/yr per build. Body-restricted to [Asteroid, Moon, GasGiant].",
            BuildingType::AutoSilverMine => "Orbital silver extraction rig. 0.0001 Mt/yr per build. Body-restricted to [Asteroid, Moon, GasGiant].",
            BuildingType::AutoPlatinumMine => "Orbital platinum-group-metal extraction rig (mostly M-type asteroids). 0.000001 Mt/yr per build. Body-restricted to [Asteroid, Moon, GasGiant].",
            BuildingType::AutoCopperMine => "Orbital copper extraction rig. 0.15 Mt/yr per build. Body-restricted to [Asteroid, Moon, GasGiant].",
            BuildingType::AutoRareEarthsMine => "Orbital rare-earths extraction rig. 0.0025 Mt/yr per build. Body-restricted to [Asteroid, Moon, GasGiant].",
            BuildingType::AutoLithiumMine => "Orbital lithium extraction rig. 0.0012 Mt/yr per build. Body-restricted to [Asteroid, Moon, GasGiant].",
            BuildingType::AutoSulfurMine => "Orbital sulfur extraction rig (carbonaceous chondrites). 0.5 Mt/yr per build. Body-restricted to [Asteroid, Moon, GasGiant].",
            BuildingType::AutoPhosphorusMine => "Orbital phosphate extraction rig. 0.0003 Mt/yr per build. Body-restricted to [Asteroid, Moon, GasGiant].",
            BuildingType::AutoCobaltMine => "Orbital cobalt extraction rig. 0.0015 Mt/yr per build. Body-restricted to [Asteroid, Moon, GasGiant].",
            BuildingType::AutoFluorineMine => "Orbital fluorine extraction rig (fluorite / apatite). 0.02 Mt/yr per build. Body-restricted to [Asteroid, Moon, GasGiant].",
            BuildingType::AutoUraniumMine => "Orbital uranium extraction rig (rare). 0.0003 Mt/yr per build. Body-restricted to [Asteroid, Moon, GasGiant].",
            BuildingType::AutoThoriumMine => "Orbital thorium extraction rig. 0.00007 Mt/yr per build. Body-restricted to [Asteroid, Moon, GasGiant].",
            BuildingType::AutoMethaneExtractor => "Orbital methane extraction rig (Titan lakes, gas-giant moons). 27 Mt/yr per build. Body-restricted to [Asteroid, Moon, GasGiant].",
            BuildingType::AutoDeuteriumExtractor => "Orbital heavy-water extraction rig (carbonaceous chondrite ice). 0.05 Mt/yr per build. Body-restricted to [Asteroid, Moon, GasGiant].",
            BuildingType::AutoHe3Mine => "Orbital He-3 sweeper for solar-wind-implanted asteroid / lunar regolith. 0.05 Mt/yr per build. Body-restricted to [Asteroid, Moon, GasGiant]; requires asteroid_mining + lunar_colony tech.",
            BuildingType::AutoWaterProcessor => "Orbital water extractor for carbonaceous chondrites / icy moons. 1.6 Mt/yr per build. Body-restricted to [Asteroid, Moon, GasGiant].",
            // Generic industry / refining
            BuildingType::Factory => "Manufactures goods and components",
            BuildingType::ChemicalPlant => "Processes volatiles into useful chemical products",
            BuildingType::AtmosphericProcessor => "Harvests gases from the atmosphere",
            // Logistics
            BuildingType::MassDriver => "Electromagnetic launcher for bulk cargo between bodies",
            BuildingType::OrbitalLift => "Space elevator for efficient surface-to-orbit transport",
            BuildingType::CargoTerminal => "Ground-based cargo distribution hub",
            // Power
            BuildingType::SolarPower => "Solar panel arrays for power generation",
            BuildingType::FissionReactor => "Nuclear fission reactor for reliable power",
            BuildingType::Farm => "Open-air food production",
            BuildingType::FusionReactor => "Advanced fusion power plant",
            BuildingType::DTFusionReactor => "High-output fusion plant using deuterium-tritium fuel",
            BuildingType::DHe3FusionReactor => "Premium fusion plant using deuterium and helium-3",
            BuildingType::ThoriumReactor => "Molten-salt thorium reactor for safe baseload power",
            BuildingType::BreederReactor => "Fast breeder reactor that produces plutonium from uranium",
            // Population / Research / Industry
            BuildingType::AgriDome => "Agricultural facilities for food production",
            BuildingType::MedicalCenter => "Medical facilities to boost population growth",
            BuildingType::ResearchLab => "Scientific research laboratory",
            BuildingType::EngineeringBay => "Engineering workshop for component development",
            BuildingType::AiCluster => "AI computation cluster boosting research and engineering",
            BuildingType::CommercialHub => "Commercial centre generating wealth from trade",
            BuildingType::FinancialCenter => "Banking and investment for wealth generation",
            // v3.10 (GRA-22c Phase 4C-2): TradePort removed.
            BuildingType::Shipyard => "Orbital shipyard for constructing vessels",
            BuildingType::MissileSilo => "Ground-based missile silo for planetary defence",
            BuildingType::LaunchSite => "Rocket launch site for orbital access",
            BuildingType::WindFarm => "Generates clean energy from wind currents",
            BuildingType::HydroelectricDam => "Harnesses river flow for reliable base-load power",
            BuildingType::GeothermalPlant => "Taps planetary heat for continuous power generation",
            BuildingType::CoalPowerPlant => "Massive coal-fired power sector; a major industrial base-load source",
            BuildingType::NaturalGasPlant => "Gas turbine power sector for fast-ramping base-load generation",
            BuildingType::SemiconductorFab => "Planetary electronics and microchip manufacturing industry",
            BuildingType::PharmaceuticalPlant => "Civilisation-scale pharmaceutical and biomedical production",
            BuildingType::WaterTreatmentPlant => "Planetary water purification and distribution network",
            BuildingType::DesalinationPlant => "Large-scale ocean desalination infrastructure for water-scarce worlds",
            BuildingType::Greenhouse => "Vast network of climate-controlled crop production facilities",
            BuildingType::AquacultureFacility => "Planetary aquatic protein farming across oceans and inland seas",
            BuildingType::DataCenter => "Planetary-scale computation, AI processing, and data storage infrastructure",
            BuildingType::SpacePort => "High-throughput orbital launch complex with multiple pads",
            BuildingType::GroundDefenseBattery => "Anti-orbital and anti-missile ground defense installation",
            BuildingType::Warehouse => "Bulk storage depot that expands global resource stockpile capacity by 2.5% per depot",
            BuildingType::OrbitalSurveyStation => "Permanent orbital installation that continuously surveys the host body and boosts local mining yield. Place in orbit of a single body; effect does not transfer to moons or parent planets.",
        }
    }

    /// Short, pre-formatted effect strings shown on the construction card.
    ///
    /// Each entry is one line such as "+25M housing capacity" or
    /// "+1,000 Mt/yr food (feeds ~10M ppl)".  Returns an empty slice for
    /// buildings whose effects are opaque from the UI (e.g. pure military).
    pub fn effects_summary(&self) -> &'static [&'static str] {
        match self {
            // ── Infrastructure ───────────────────────────────────────────
            BuildingType::LifeSupport => &[
                "Enables habitation on vacuum / hostile worlds",
                "Recycles air and water for pressurised habitats",
            ],
            BuildingType::HabitatDome => &[
                "+50M housing capacity",
                "Pressurised dome, works on any body",
            ],
            BuildingType::Housing => &["+25M housing capacity"],
            BuildingType::UndergroundHabitat => &[
                "+30M housing capacity",
                "Buried structure; ideal for airless bodies",
            ],
            // v3.2 canary 12 (2026-08-07): starter-tier housing
            BuildingType::HabitatTent => &[
                "+1k housing capacity",
                "Inflatable emergency shelter; v3.2 starter-tier",
            ],
            BuildingType::HabitatModule => &[
                "+10k housing capacity",
                "Prefab habitat; v3.2 starter-tier for growing colonies",
            ],
            BuildingType::WaterProcessor => &[
                "+16 Mt/yr water per processor",
                "Required for non-breathable colony life support",
            ],
            // ── Mining & Industry (v0.5.2: per-resource dedicated mines) ─
            // Construction (9)
            BuildingType::IronMine => {
                &["+120 Mt/yr iron per mine", "Yield × deposit.accessibility"]
            }
            BuildingType::AluminumMine => &[
                "+5 Mt/yr aluminum per mine",
                "Yield × deposit.accessibility",
            ],
            BuildingType::TitaniumMine => &[
                "+0.02 Mt/yr titanium per mine",
                "Yield × deposit.accessibility",
            ],
            BuildingType::SilicatesMine => &[
                "+700 Mt/yr silicates per quarry",
                "Yield × deposit.accessibility",
            ],
            BuildingType::NickelMine => &[
                "+0.2 Mt/yr nickel per mine",
                "Yield × deposit.accessibility",
            ],
            BuildingType::TungstenMine => &[
                "+0.005 Mt/yr tungsten per mine",
                "Yield × deposit.accessibility",
            ],
            BuildingType::CarbonMine => &[
                "+350 Mt/yr carbon per mine",
                "Yield × deposit.accessibility",
            ],
            BuildingType::ChromiumMine => &[
                "+2 Mt/yr chromium per mine",
                "Yield × deposit.accessibility",
            ],
            BuildingType::MagnesiumMine => &[
                "+0.07 Mt/yr magnesium per mine",
                "Yield × deposit.accessibility",
            ],
            // Precious (3 — v0.5.1)
            BuildingType::GoldMine => &[
                "+0.0001 Mt/yr gold per mine",
                "Direct deposit (not share-fold)",
            ],
            BuildingType::SilverMine => &[
                "+0.001 Mt/yr silver per mine",
                "Direct deposit (not share-fold)",
            ],
            BuildingType::PlatinumMine => &[
                "+0.00001 Mt/yr platinum per mine",
                "Direct deposit (not share-fold)",
            ],
            // Strategic (6)
            BuildingType::CopperMine => &[
                "+1.5 Mt/yr copper per mine",
                "Yield × deposit.accessibility",
            ],
            BuildingType::RareEarthsMine => &[
                "+0.025 Mt/yr rare earths per mine",
                "Yield × deposit.accessibility",
            ],
            BuildingType::LithiumMine => &[
                "+0.012 Mt/yr lithium per mine",
                "Yield × deposit.accessibility",
            ],
            BuildingType::SulfurMine => {
                &["+5 Mt/yr sulfur per mine", "Yield × deposit.accessibility"]
            }
            BuildingType::PhosphorusMine => &[
                "+0.003 Mt/yr phosphorus per mine",
                "Yield × deposit.accessibility",
            ],
            BuildingType::CobaltMine => &[
                "+0.015 Mt/yr cobalt per mine",
                "Yield × deposit.accessibility",
            ],
            BuildingType::FluorineMine => &[
                "+0.2 Mt/yr fluorine per mine",
                "Yield × deposit.accessibility",
            ],
            // Fissile (2)
            BuildingType::UraniumMine => &[
                "+0.003 Mt/yr uranium per mine",
                "Yield × deposit.accessibility",
            ],
            BuildingType::ThoriumMine => &[
                "+0.0007 Mt/yr thorium per mine",
                "Yield × deposit.accessibility",
            ],
            // Hydrocarbons (1)
            BuildingType::MethaneExtractor => &[
                "+270 Mt/yr methane per extractor",
                "Yield × deposit.accessibility",
            ],
            // Heavy water (1)
            BuildingType::DeuteriumExtractor => &[
                "+0.5 Mt/yr deuterium per extractor",
                "Yield × deposit.accessibility",
            ],
            // He-3 (1 — body-restricted)
            BuildingType::He3Mine => &[
                "+0.5 Mt/yr He-3 per mine",
                "Body-restricted: [Moon, GasGiant, Asteroid]",
                "Requires lunar_colony tech",
            ],
            // AutoMines (22) — orbital/asteroid mining
            BuildingType::AutoIronMine => &[
                "+12 Mt/yr iron per rig (asteroid)",
                "Body: [Asteroid, Moon, GasGiant]",
            ],
            BuildingType::AutoAluminumMine => &[
                "+0.5 Mt/yr aluminum per rig",
                "Body: [Asteroid, Moon, GasGiant]",
            ],
            BuildingType::AutoTitaniumMine => &[
                "+0.002 Mt/yr titanium per rig",
                "Body: [Asteroid, Moon, GasGiant]",
            ],
            BuildingType::AutoSilicatesMine => &[
                "+70 Mt/yr silicates per rig",
                "Body: [Asteroid, Moon, GasGiant]",
            ],
            BuildingType::AutoNickelMine => &[
                "+0.02 Mt/yr nickel per rig (M-type)",
                "Body: [Asteroid, Moon, GasGiant]",
            ],
            BuildingType::AutoTungstenMine => &[
                "+0.0005 Mt/yr tungsten per rig",
                "Body: [Asteroid, Moon, GasGiant]",
            ],
            BuildingType::AutoCarbonMine => &[
                "+35 Mt/yr carbon per rig (C-type)",
                "Body: [Asteroid, Moon, GasGiant]",
            ],
            BuildingType::AutoChromiumMine => &[
                "+0.2 Mt/yr chromium per rig",
                "Body: [Asteroid, Moon, GasGiant]",
            ],
            BuildingType::AutoMagnesiumMine => &[
                "+0.007 Mt/yr magnesium per rig",
                "Body: [Asteroid, Moon, GasGiant]",
            ],
            BuildingType::AutoGoldMine => &[
                "+0.00001 Mt/yr gold per rig",
                "Body: [Asteroid, Moon, GasGiant]",
            ],
            BuildingType::AutoSilverMine => &[
                "+0.0001 Mt/yr silver per rig",
                "Body: [Asteroid, Moon, GasGiant]",
            ],
            BuildingType::AutoPlatinumMine => &[
                "+0.000001 Mt/yr platinum per rig",
                "Body: [Asteroid, Moon, GasGiant]",
            ],
            BuildingType::AutoCopperMine => &[
                "+0.15 Mt/yr copper per rig",
                "Body: [Asteroid, Moon, GasGiant]",
            ],
            BuildingType::AutoRareEarthsMine => &[
                "+0.0025 Mt/yr rare earths per rig",
                "Body: [Asteroid, Moon, GasGiant]",
            ],
            BuildingType::AutoLithiumMine => &[
                "+0.0012 Mt/yr lithium per rig",
                "Body: [Asteroid, Moon, GasGiant]",
            ],
            BuildingType::AutoSulfurMine => &[
                "+0.5 Mt/yr sulfur per rig",
                "Body: [Asteroid, Moon, GasGiant]",
            ],
            BuildingType::AutoPhosphorusMine => &[
                "+0.0003 Mt/yr phosphorus per rig",
                "Body: [Asteroid, Moon, GasGiant]",
            ],
            BuildingType::AutoCobaltMine => &[
                "+0.0015 Mt/yr cobalt per rig",
                "Body: [Asteroid, Moon, GasGiant]",
            ],
            BuildingType::AutoFluorineMine => &[
                "+0.02 Mt/yr fluorine per rig",
                "Body: [Asteroid, Moon, GasGiant]",
            ],
            BuildingType::AutoUraniumMine => &[
                "+0.0003 Mt/yr uranium per rig",
                "Body: [Asteroid, Moon, GasGiant]",
            ],
            BuildingType::AutoThoriumMine => &[
                "+0.00007 Mt/yr thorium per rig",
                "Body: [Asteroid, Moon, GasGiant]",
            ],
            BuildingType::AutoMethaneExtractor => &[
                "+27 Mt/yr methane per rig (Titan)",
                "Body: [Asteroid, Moon, GasGiant]",
            ],
            BuildingType::AutoDeuteriumExtractor => &[
                "+0.05 Mt/yr deuterium per rig",
                "Body: [Asteroid, Moon, GasGiant]",
            ],
            BuildingType::AutoHe3Mine => &[
                "+0.05 Mt/yr He-3 per rig (lunar regolith / asteroids)",
                "Body: [Asteroid, Moon, GasGiant]",
            ],
            BuildingType::AutoWaterProcessor => &[
                "+1.6 Mt/yr water per rig (C-type ice)",
                "Body: [Asteroid, Moon, GasGiant]",
            ],
            // Generic industry
            BuildingType::Factory => &["+10 BP/yr construction speed", "-5% construction costs"],
            BuildingType::ChemicalPlant => &[
                "+0.15 Mt/yr hydrogen",
                "+0.14 Mt/yr ammonia",
                "+0.01 Mt/yr polymers",
            ],
            BuildingType::AtmosphericProcessor => &["+0.9 Mt/yr atmospheric harvest"],
            // ── Logistics ────────────────────────────────────────────────
            BuildingType::MassDriver => &["+5,000 logistics capacity"],
            BuildingType::OrbitalLift => &["+20,000 logistics capacity"],
            BuildingType::CargoTerminal => &["+2,000 logistics capacity"],
            BuildingType::Warehouse => &["+2.5% global stockpile capacity"],
            // ── Power ───────────────────────────────────────────────────
            BuildingType::SolarPower => &["+5 GW power output"],
            BuildingType::FissionReactor => &["+20 GW power output", "Fuel: Uranium"],
            BuildingType::FusionReactor => &["+40 GW power output", "Fuel: He-3 + Deuterium"],
            BuildingType::DTFusionReactor => &["+50 GW power output", "Fuel: Deuterium + Tritium"],
            BuildingType::DHe3FusionReactor => {
                &["+45 GW power output", "Fuel: Deuterium + Helium-3"]
            }
            BuildingType::ThoriumReactor => &["+24 GW power output", "Fuel: Thorium"],
            BuildingType::BreederReactor => &["+22 GW power output", "+Plutonium bred/yr"],
            BuildingType::WindFarm => &["+3 GW power output"],
            BuildingType::HydroelectricDam => &["+15 GW power output"],
            BuildingType::GeothermalPlant => &["+18 GW power output"],
            BuildingType::CoalPowerPlant => &["+10 GW power output", "Burns: Coal"],
            BuildingType::NaturalGasPlant => &["+12 GW power output", "Burns: Natural Gas"],
            // ── Population & Growth ──────────────────────────────────────
            BuildingType::Farm => &["+1,000 Mt/yr food", "Feeds ~10M people"],
            BuildingType::AgriDome => &["+4 Mt/yr food", "Feeds ~40K people (enclosed)"],
            BuildingType::Greenhouse => &["+500 Mt/yr food", "Feeds ~5M people"],
            BuildingType::AquacultureFacility => &["+750 Mt/yr food", "Feeds ~7.5M people"],
            BuildingType::MedicalCenter => &["+0.03% population growth rate per centre"],
            BuildingType::WaterTreatmentPlant => &["+2% population growth rate"],
            BuildingType::DesalinationPlant => &["+1% population growth rate"],
            BuildingType::PharmaceuticalPlant => &["+3% population growth rate"],
            // ── Research ─────────────────────────────────────────────────
            BuildingType::ResearchLab => &["+5% research speed"],
            BuildingType::EngineeringBay => &["+5% engineering speed"],
            BuildingType::AiCluster => &["+15% research speed", "+10% engineering speed"],
            BuildingType::SemiconductorFab => &["+8% research speed", "+5% engineering speed"],
            BuildingType::DataCenter => &["+10% research speed", "+8% engineering speed"],
            // ── Financial & Commerce ─────────────────────────────────────
            BuildingType::CommercialHub => &["+credits income from trade"],
            BuildingType::FinancialCenter => &["+credits income from banking"],
            // v3.10 (GRA-22c Phase 4C-2): TradePort removed.
            // ── Military & Shipbuilding ──────────────────────────────────
            BuildingType::Shipyard => &[
                "Enables ship construction",
                "+10% ship efficiency, -10% build costs",
            ],
            BuildingType::MissileSilo => &["Planetary anti-orbital defense"],
            BuildingType::LaunchSite => &["Surface-to-orbit launch access"],
            BuildingType::SpacePort => &["High-throughput orbital access"],
            BuildingType::GroundDefenseBattery => &["Anti-orbital / anti-missile defense"],
            // ── Survey ───────────────────────────────────────────────────
            BuildingType::OrbitalSurveyStation => &[
                "Continuous low-yield survey of the host body",
                "+5/10/15% mining yield (tier 1/2/3) on local mines",
                "Per-body isolation — does not affect moons or parent planet",
            ],
        }
    }

    /// Icon/emoji for UI display
    pub fn icon(&self) -> &'static str {
        match self {
            BuildingType::Housing => "🏙",
            BuildingType::LifeSupport => "🌬",
            BuildingType::HabitatDome => "🏠",
            BuildingType::UndergroundHabitat => "⛏",
            // v3.2 canary 12 (2026-08-07): starter-tier housing
            BuildingType::HabitatTent => "⛺",
            BuildingType::HabitatModule => "🏗",
            BuildingType::WaterProcessor => "🧊",
            // Construction mines (9)
            BuildingType::IronMine => "⛓", // chain link / iron symbol
            BuildingType::AluminumMine => "🪨", // rock
            BuildingType::TitaniumMine => "🔩", // bolt (strong alloy)
            BuildingType::SilicatesMine => "🪨", // stone
            BuildingType::NickelMine => "🟘", // brown circle (nickel hue)
            BuildingType::TungstenMine => "🟧", // orange square (wolframite)
            BuildingType::CarbonMine => "⬛", // black square (coal)
            BuildingType::ChromiumMine => "🟢", // green circle (chromite green hue)
            BuildingType::MagnesiumMine => "⬜", // white square (magnesia)
            // Precious metals (3 — v0.5.1)
            BuildingType::GoldMine => "🥇",
            BuildingType::SilverMine => "🥈",
            BuildingType::PlatinumMine => "💍",
            // Strategic (6)
            BuildingType::CopperMine => "🟠", // orange circle (copper hue)
            BuildingType::RareEarthsMine => "🌐", // globe (rare elements)
            BuildingType::LithiumMine => "🔋", // battery
            BuildingType::SulfurMine => "🟡", // yellow circle (sulfur)
            BuildingType::PhosphorusMine => "🟣", // purple circle (phosphorus glow)
            BuildingType::CobaltMine => "🔵", // blue circle (cobalt)
            BuildingType::FluorineMine => "🟩", // green square (fluorite)
            // Fissile (2)
            BuildingType::UraniumMine => "☢",  // radioactive symbol
            BuildingType::ThoriumMine => "🟤", // brown square (monazite)
            // Hydrocarbons (1)
            BuildingType::MethaneExtractor => "🔥", // flame (methane combustion)
            // Heavy water (1)
            BuildingType::DeuteriumExtractor => "💧", // water drop
            // He-3 (1 — body-restricted)
            BuildingType::He3Mine => "☀", // sun (solar-wind-implanted He-3)
            // AutoMines (22) — orbital/asteroid mining (use '🛰' prefix to denote orbital)
            BuildingType::AutoIronMine => "🛰⛓",
            BuildingType::AutoAluminumMine => "🛰🪨",
            BuildingType::AutoTitaniumMine => "🛰🔩",
            BuildingType::AutoSilicatesMine => "🛰🪨",
            BuildingType::AutoNickelMine => "🛰🟘",
            BuildingType::AutoTungstenMine => "🛰🟧",
            BuildingType::AutoCarbonMine => "🛰⬛",
            BuildingType::AutoChromiumMine => "🛰🟢",
            BuildingType::AutoMagnesiumMine => "🛰⬜",
            BuildingType::AutoGoldMine => "🛰🥇",
            BuildingType::AutoSilverMine => "🛰🥈",
            BuildingType::AutoPlatinumMine => "🛰💍",
            BuildingType::AutoCopperMine => "🛰🟠",
            BuildingType::AutoRareEarthsMine => "🛰🌐",
            BuildingType::AutoLithiumMine => "🛰🔋",
            BuildingType::AutoSulfurMine => "🛰🟡",
            BuildingType::AutoPhosphorusMine => "🛰🟣",
            BuildingType::AutoCobaltMine => "🛰🔵",
            BuildingType::AutoFluorineMine => "🛰🟩",
            BuildingType::AutoUraniumMine => "🛰☢",
            BuildingType::AutoThoriumMine => "🛰🟤",
            BuildingType::AutoMethaneExtractor => "🛰🔥",
            BuildingType::AutoDeuteriumExtractor => "🛰💧",
            BuildingType::AutoHe3Mine => "🛰☀",
            BuildingType::AutoWaterProcessor => "🛰🧊",
            // Generic industry
            BuildingType::Factory => "🏭",
            BuildingType::ChemicalPlant => "⚗️",
            BuildingType::AtmosphericProcessor => "☁️",
            BuildingType::SemiconductorFab => "💾",
            BuildingType::PharmaceuticalPlant => "💊",
            // Logistics
            BuildingType::MassDriver => "🧲",
            BuildingType::OrbitalLift => "🚡",
            BuildingType::CargoTerminal => "📦",
            BuildingType::Warehouse => "🏗",
            // Power
            BuildingType::SolarPower => "☀",
            BuildingType::FissionReactor => "☢",
            BuildingType::FusionReactor => "⚡",
            BuildingType::DTFusionReactor => "⚛",
            BuildingType::DHe3FusionReactor => "☀",
            BuildingType::ThoriumReactor => "♨️",
            BuildingType::BreederReactor => "☢️",
            BuildingType::WindFarm => "💨",
            BuildingType::HydroelectricDam => "🌊",
            BuildingType::GeothermalPlant => "🌋",
            BuildingType::CoalPowerPlant => "🏭",
            BuildingType::NaturalGasPlant => "🔥",
            // Population
            BuildingType::AgriDome => "🌾",
            BuildingType::Farm => "🐄",
            BuildingType::Greenhouse => "🌿",
            BuildingType::AquacultureFacility => "🐟",
            BuildingType::MedicalCenter => "🏥",
            BuildingType::WaterTreatmentPlant => "💧",
            BuildingType::DesalinationPlant => "🧂",
            // Research
            BuildingType::ResearchLab => "🔬",
            BuildingType::EngineeringBay => "🔩",
            BuildingType::AiCluster => "🤖",
            BuildingType::DataCenter => "🖥️",
            // Financial
            BuildingType::CommercialHub => "🏪",
            BuildingType::FinancialCenter => "🏦",
            // v3.10 (GRA-22c Phase 4C-2): TradePort removed.
            // Military
            BuildingType::Shipyard => "⚓",
            BuildingType::MissileSilo => "🚀",
            BuildingType::LaunchSite => "🛫",
            BuildingType::SpacePort => "🚀",
            BuildingType::GroundDefenseBattery => "🛡️",
            // Survey
            BuildingType::OrbitalSurveyStation => "🛰",
        }
    }

    /// Category for grouping in UI
    pub fn category(&self) -> BuildingCategory {
        match self {
            // Infrastructure
            BuildingType::LifeSupport
            | BuildingType::HabitatDome
            | BuildingType::Housing
            | BuildingType::UndergroundHabitat
            // v3.2 canary 12 (2026-08-07): starter-tier housing
            | BuildingType::HabitatTent
            | BuildingType::HabitatModule
            | BuildingType::WaterProcessor
            | BuildingType::WaterTreatmentPlant
            | BuildingType::DesalinationPlant => BuildingCategory::Infrastructure,
            // Mining (v0.5.2: split out of Industry). 22 base mines +
            // 25 AutoMines, all routed to the dedicated Mining
            // category. Both surface and orbital share the same
            // category — the orbital body-gate is enforced by
            // `building_is_available_on` and the Mining tab UI
            // (the AutoMine card is dimmed/disabled on Earth/Mars/
            // Venus).
            BuildingType::IronMine
            | BuildingType::AluminumMine
            | BuildingType::TitaniumMine
            | BuildingType::SilicatesMine
            | BuildingType::NickelMine
            | BuildingType::TungstenMine
            | BuildingType::CarbonMine
            | BuildingType::ChromiumMine
            | BuildingType::MagnesiumMine
            | BuildingType::GoldMine
            | BuildingType::SilverMine
            | BuildingType::PlatinumMine
            | BuildingType::CopperMine
            | BuildingType::RareEarthsMine
            | BuildingType::LithiumMine
            | BuildingType::SulfurMine
            | BuildingType::PhosphorusMine
            | BuildingType::CobaltMine
            | BuildingType::FluorineMine
            | BuildingType::UraniumMine
            | BuildingType::ThoriumMine
            | BuildingType::MethaneExtractor
            | BuildingType::DeuteriumExtractor
            | BuildingType::He3Mine
            | BuildingType::AutoIronMine
            | BuildingType::AutoAluminumMine
            | BuildingType::AutoTitaniumMine
            | BuildingType::AutoSilicatesMine
            | BuildingType::AutoNickelMine
            | BuildingType::AutoTungstenMine
            | BuildingType::AutoCarbonMine
            | BuildingType::AutoChromiumMine
            | BuildingType::AutoMagnesiumMine
            | BuildingType::AutoGoldMine
            | BuildingType::AutoSilverMine
            | BuildingType::AutoPlatinumMine
            | BuildingType::AutoCopperMine
            | BuildingType::AutoRareEarthsMine
            | BuildingType::AutoLithiumMine
            | BuildingType::AutoSulfurMine
            | BuildingType::AutoPhosphorusMine
            | BuildingType::AutoCobaltMine
            | BuildingType::AutoFluorineMine
            | BuildingType::AutoUraniumMine
            | BuildingType::AutoThoriumMine
            | BuildingType::AutoMethaneExtractor
            | BuildingType::AutoDeuteriumExtractor
            | BuildingType::AutoHe3Mine
            | BuildingType::AutoWaterProcessor => BuildingCategory::Mining,
            // Industry (v0.5.2: mines moved out — this is now pure
            // processing/manufacturing: chemical, semiconductor,
            // pharmaceutical, atmospheric harvesting, factories).
            BuildingType::Factory
            | BuildingType::AtmosphericProcessor
            | BuildingType::ChemicalPlant
            | BuildingType::SemiconductorFab
            | BuildingType::PharmaceuticalPlant => BuildingCategory::Industry,
            // Logistics
            BuildingType::MassDriver
            | BuildingType::OrbitalLift
            | BuildingType::CargoTerminal
            | BuildingType::Warehouse => BuildingCategory::Logistics,
            // Power
            BuildingType::SolarPower
            | BuildingType::FissionReactor
            | BuildingType::FusionReactor
            | BuildingType::DTFusionReactor
            | BuildingType::DHe3FusionReactor
            | BuildingType::ThoriumReactor
            | BuildingType::BreederReactor
            | BuildingType::WindFarm
            | BuildingType::HydroelectricDam
            | BuildingType::GeothermalPlant
            | BuildingType::CoalPowerPlant
            | BuildingType::NaturalGasPlant => BuildingCategory::Power,
            // Population
            BuildingType::AgriDome
            | BuildingType::Farm
            | BuildingType::MedicalCenter
            | BuildingType::Greenhouse
            | BuildingType::AquacultureFacility => BuildingCategory::Population,
            // Research
            BuildingType::ResearchLab
            | BuildingType::EngineeringBay
            | BuildingType::AiCluster
            | BuildingType::DataCenter
            | BuildingType::OrbitalSurveyStation => BuildingCategory::Research,
            // Financial
            BuildingType::CommercialHub
            | BuildingType::FinancialCenter => BuildingCategory::Financial,
            // v3.10 (GRA-22c Phase 4C-2): TradePort removed.
            // Military
            BuildingType::Shipyard
            | BuildingType::MissileSilo
            | BuildingType::LaunchSite
            | BuildingType::SpacePort
            | BuildingType::GroundDefenseBattery => BuildingCategory::Military,
        }
    }

    /// Construction cost in build points
    pub fn build_cost(&self) -> f64 {
        match self {
            BuildingType::LifeSupport => 500.0,
            BuildingType::HabitatDome => 800.0,
            BuildingType::Housing => 200.0,
            BuildingType::UndergroundHabitat => 1200.0,
            // v3.2 canary 12 (2026-08-07): starter-tier housing
            BuildingType::HabitatTent => 50.0,
            BuildingType::HabitatModule => 200.0,
            BuildingType::WaterProcessor => 600.0,
            // Construction mines (9) — scaled to per-build yield
            BuildingType::IronMine => 1500.0,
            BuildingType::AluminumMine => 1200.0,
            BuildingType::TitaniumMine => 1800.0,
            BuildingType::SilicatesMine => 300.0,
            BuildingType::NickelMine => 1100.0,
            BuildingType::TungstenMine => 2200.0,
            BuildingType::CarbonMine => 900.0,
            BuildingType::ChromiumMine => 1300.0,
            BuildingType::MagnesiumMine => 1400.0,
            // Precious metals (3 — v0.5.1)
            BuildingType::GoldMine => 1200.0,
            BuildingType::SilverMine => 1500.0,
            BuildingType::PlatinumMine => 2000.0,
            // Strategic (6)
            BuildingType::CopperMine => 1300.0,
            BuildingType::RareEarthsMine => 2200.0,
            BuildingType::LithiumMine => 1900.0,
            BuildingType::SulfurMine => 1000.0,
            BuildingType::PhosphorusMine => 1500.0,
            BuildingType::CobaltMine => 1700.0,
            BuildingType::FluorineMine => 1400.0,
            // Fissile (2)
            BuildingType::UraniumMine => 2500.0,
            BuildingType::ThoriumMine => 2100.0,
            // Hydrocarbons (1)
            BuildingType::MethaneExtractor => 1100.0,
            // Heavy water (1)
            BuildingType::DeuteriumExtractor => 1800.0,
            // He-3 (1)
            BuildingType::He3Mine => 3500.0,
            // AutoMines (22) — orbital/asteroid mining (more expensive due to space-grade hardware)
            BuildingType::AutoIronMine => 2500.0,
            BuildingType::AutoAluminumMine => 2200.0,
            BuildingType::AutoTitaniumMine => 2800.0,
            BuildingType::AutoSilicatesMine => 1500.0,
            BuildingType::AutoNickelMine => 2400.0,
            BuildingType::AutoTungstenMine => 3200.0,
            BuildingType::AutoCarbonMine => 2000.0,
            BuildingType::AutoChromiumMine => 2500.0,
            BuildingType::AutoMagnesiumMine => 2600.0,
            BuildingType::AutoGoldMine => 3500.0,
            BuildingType::AutoSilverMine => 3200.0,
            BuildingType::AutoPlatinumMine => 4000.0,
            BuildingType::AutoCopperMine => 2400.0,
            BuildingType::AutoRareEarthsMine => 3500.0,
            BuildingType::AutoLithiumMine => 3200.0,
            BuildingType::AutoSulfurMine => 2000.0,
            BuildingType::AutoPhosphorusMine => 2700.0,
            BuildingType::AutoCobaltMine => 2900.0,
            BuildingType::AutoFluorineMine => 2500.0,
            BuildingType::AutoUraniumMine => 3800.0,
            BuildingType::AutoThoriumMine => 3500.0,
            BuildingType::AutoMethaneExtractor => 2500.0,
            BuildingType::AutoDeuteriumExtractor => 3000.0,
            BuildingType::AutoHe3Mine => 5000.0,
            BuildingType::AutoWaterProcessor => 2000.0,
            // Generic industry
            BuildingType::Factory => 1000.0,
            BuildingType::AtmosphericProcessor => 600.0,
            BuildingType::ChemicalPlant => 800.0,
            BuildingType::SemiconductorFab => 5000.0,
            BuildingType::PharmaceuticalPlant => 800.0,
            // Logistics
            BuildingType::MassDriver => 2000.0,
            BuildingType::OrbitalLift => 5000.0,
            BuildingType::CargoTerminal => 300.0,
            BuildingType::Warehouse => 300.0,
            // Power
            BuildingType::SolarPower => 200.0,
            BuildingType::FissionReactor => 1500.0,
            BuildingType::FusionReactor => 5000.0,
            BuildingType::DTFusionReactor => 6000.0,
            BuildingType::DHe3FusionReactor => 7000.0,
            BuildingType::ThoriumReactor => 1800.0,
            BuildingType::BreederReactor => 2600.0,
            BuildingType::WindFarm => 300.0,
            BuildingType::HydroelectricDam => 2500.0,
            BuildingType::GeothermalPlant => 1800.0,
            BuildingType::CoalPowerPlant => 800.0,
            BuildingType::NaturalGasPlant => 600.0,
            // Population
            BuildingType::AgriDome => 600.0,
            BuildingType::Farm => 100.0,
            BuildingType::Greenhouse => 400.0,
            BuildingType::AquacultureFacility => 500.0,
            BuildingType::MedicalCenter => 800.0,
            BuildingType::WaterTreatmentPlant => 400.0,
            BuildingType::DesalinationPlant => 600.0,
            // Research
            BuildingType::ResearchLab => 1000.0,
            BuildingType::EngineeringBay => 1200.0,
            BuildingType::AiCluster => 4000.0,
            BuildingType::DataCenter => 2000.0,
            // Financial
            BuildingType::CommercialHub => 500.0,
            BuildingType::FinancialCenter => 1500.0,
            // v3.10 (GRA-22c Phase 4C-2): TradePort removed.
            // Military
            BuildingType::Shipyard => 10000.0,
            BuildingType::MissileSilo => 3000.0,
            BuildingType::LaunchSite => 2000.0,
            BuildingType::SpacePort => 4000.0,
            BuildingType::GroundDefenseBattery => 2500.0,
            // Survey
            BuildingType::OrbitalSurveyStation => 1200.0,
        }
    }

    /// Workforce required to operate this building (number of workers).
    ///
    /// Values are scaled to match civilization-level operations: deposits are
    /// measured in Megatons, populations in millions to billions.  A starting
    /// colony of 100,000 people (40,000 workers) can operate several basic
    /// buildings.  Advanced/large installations need tens of thousands of
    /// workers, encouraging population growth before scaling up.
    pub fn workforce_required(&self) -> u32 {
        match self {
            // Infrastructure – essential services
            BuildingType::LifeSupport => 2_000,
            BuildingType::HabitatDome => 1_000,
            BuildingType::Housing => 500,
            BuildingType::UndergroundHabitat => 1_500,
            // v3.2 canary 12 (2026-08-07): starter-tier housing.
            // Tent is 5 workers (small inflatable, minimal crew).
            // Module is 50 workers (prefab habitat, small staff).
            BuildingType::HabitatTent => 5,
            BuildingType::HabitatModule => 50,
            BuildingType::WaterProcessor => 2_000,
            // Construction mines (9) — surface-scale operations
            BuildingType::IronMine => 5_000,
            BuildingType::AluminumMine => 4_500,
            BuildingType::TitaniumMine => 5_500,
            BuildingType::SilicatesMine => 1_500,
            BuildingType::NickelMine => 4_000,
            BuildingType::TungstenMine => 6_000,
            BuildingType::CarbonMine => 3_500,
            BuildingType::ChromiumMine => 4_500,
            BuildingType::MagnesiumMine => 5_000,
            // Precious metals (3 — v0.5.1)
            BuildingType::GoldMine => 4_000,
            BuildingType::SilverMine => 5_000,
            BuildingType::PlatinumMine => 6_000,
            // Strategic (6)
            BuildingType::CopperMine => 4_500,
            BuildingType::RareEarthsMine => 6_000,
            BuildingType::LithiumMine => 5_500,
            BuildingType::SulfurMine => 3_500,
            BuildingType::PhosphorusMine => 5_000,
            BuildingType::CobaltMine => 5_500,
            BuildingType::FluorineMine => 4_500,
            // Fissile (2)
            BuildingType::UraniumMine => 6_500,
            BuildingType::ThoriumMine => 6_000,
            // Hydrocarbons (1)
            BuildingType::MethaneExtractor => 3_500,
            // Heavy water (1)
            BuildingType::DeuteriumExtractor => 5_500,
            // He-3 (1)
            BuildingType::He3Mine => 8_000,
            // AutoMines (22) — orbital crews, smaller workforces (more automation)
            BuildingType::AutoIronMine => 800,
            BuildingType::AutoAluminumMine => 700,
            BuildingType::AutoTitaniumMine => 900,
            BuildingType::AutoSilicatesMine => 300,
            BuildingType::AutoNickelMine => 800,
            BuildingType::AutoTungstenMine => 1_000,
            BuildingType::AutoCarbonMine => 600,
            BuildingType::AutoChromiumMine => 800,
            BuildingType::AutoMagnesiumMine => 800,
            BuildingType::AutoGoldMine => 1_200,
            BuildingType::AutoSilverMine => 1_000,
            BuildingType::AutoPlatinumMine => 1_500,
            BuildingType::AutoCopperMine => 800,
            BuildingType::AutoRareEarthsMine => 1_200,
            BuildingType::AutoLithiumMine => 1_000,
            BuildingType::AutoSulfurMine => 600,
            BuildingType::AutoPhosphorusMine => 900,
            BuildingType::AutoCobaltMine => 1_000,
            BuildingType::AutoFluorineMine => 800,
            BuildingType::AutoUraniumMine => 1_300,
            BuildingType::AutoThoriumMine => 1_200,
            BuildingType::AutoMethaneExtractor => 800,
            BuildingType::AutoDeuteriumExtractor => 1_000,
            BuildingType::AutoHe3Mine => 1_500,
            BuildingType::AutoWaterProcessor => 600,
            // Generic industry
            BuildingType::Factory => 12_000,
            BuildingType::ChemicalPlant => 4_000,
            BuildingType::AtmosphericProcessor => 3_000,
            // Logistics
            BuildingType::MassDriver => 2_500,
            BuildingType::OrbitalLift => 6_000,
            BuildingType::CargoTerminal => 3_000,
            BuildingType::Warehouse => 1_000,
            // Power – largely automated
            BuildingType::SolarPower => 500,
            BuildingType::FissionReactor => 4_000,
            BuildingType::FusionReactor => 8_000,
            BuildingType::DTFusionReactor => 9_000,
            BuildingType::DHe3FusionReactor => 9_500,
            BuildingType::ThoriumReactor => 4_500,
            BuildingType::BreederReactor => 5_000,
            BuildingType::WindFarm => 200,
            BuildingType::HydroelectricDam => 1_000,
            BuildingType::GeothermalPlant => 800,
            BuildingType::CoalPowerPlant => 2_000,
            BuildingType::NaturalGasPlant => 1_500,
            // Population support
            BuildingType::AgriDome => 4_000,
            BuildingType::Farm => 1_000,
            BuildingType::Greenhouse => 2_000,
            BuildingType::AquacultureFacility => 1_500,
            BuildingType::MedicalCenter => 6_000,
            // Research
            BuildingType::ResearchLab => 8_000,
            BuildingType::EngineeringBay => 10_000,
            BuildingType::AiCluster => 2_000,
            BuildingType::SemiconductorFab => 5_000,
            BuildingType::DataCenter => 1_000,
            // Financial
            BuildingType::CommercialHub => 8_000,
            BuildingType::FinancialCenter => 10_000,
            // v3.10 (GRA-22c Phase 4C-2): TradePort removed.
            // Military
            BuildingType::Shipyard => 80_000,
            BuildingType::MissileSilo => 5_000,
            BuildingType::LaunchSite => 12_000,
            BuildingType::SpacePort => 20_000,
            BuildingType::GroundDefenseBattery => 3_000,
            // Survey
            BuildingType::OrbitalSurveyStation => 500,
            // Water & environment
            BuildingType::WaterTreatmentPlant => 500,
            BuildingType::DesalinationPlant => 400,
            // Advanced industry
            BuildingType::PharmaceuticalPlant => 4_000,
        }
    }

    /// Technology ID required to unlock this building, if any.
    ///
    /// Returns `None` for base-game buildings available from the start.
    pub fn required_tech(&self) -> Option<&'static str> {
        match self {
            BuildingType::AtmosphericProcessor => None,
            BuildingType::FusionReactor => Some("fusion_power"),
            BuildingType::DTFusionReactor => Some("fusion_power"),
            BuildingType::DHe3FusionReactor => Some("helium3_fusion"),
            BuildingType::ThoriumReactor => Some("molten_salt_fission"),
            BuildingType::BreederReactor => Some("breeder_reactors"),
            BuildingType::AiCluster => Some("neural_networks"),
            BuildingType::Shipyard => Some("orbital_construction"),
            BuildingType::MissileSilo => Some("missile_systems"),
            BuildingType::GeothermalPlant => Some("geothermal_energy"),
            BuildingType::SemiconductorFab => Some("semiconductor_manufacturing"),
            BuildingType::DesalinationPlant => Some("desalination"),
            BuildingType::DataCenter => Some("neural_networks"),
            BuildingType::GroundDefenseBattery => Some("missile_systems"),
            BuildingType::OrbitalSurveyStation => Some("advanced_radar"),
            // He-3 mine: requires lunar_colony (canary 3)
            BuildingType::He3Mine => Some("lunar_colony"),
            // AutoMines: all require asteroid_mining
            BuildingType::AutoIronMine
            | BuildingType::AutoAluminumMine
            | BuildingType::AutoTitaniumMine
            | BuildingType::AutoSilicatesMine
            | BuildingType::AutoNickelMine
            | BuildingType::AutoTungstenMine
            | BuildingType::AutoCarbonMine
            | BuildingType::AutoChromiumMine
            | BuildingType::AutoMagnesiumMine
            | BuildingType::AutoGoldMine
            | BuildingType::AutoSilverMine
            | BuildingType::AutoPlatinumMine
            | BuildingType::AutoCopperMine
            | BuildingType::AutoRareEarthsMine
            | BuildingType::AutoLithiumMine
            | BuildingType::AutoSulfurMine
            | BuildingType::AutoPhosphorusMine
            | BuildingType::AutoCobaltMine
            | BuildingType::AutoFluorineMine
            | BuildingType::AutoUraniumMine
            | BuildingType::AutoThoriumMine
            | BuildingType::AutoMethaneExtractor
            | BuildingType::AutoDeuteriumExtractor
            | BuildingType::AutoHe3Mine
            | BuildingType::AutoWaterProcessor => Some("asteroid_mining"),
            _ => None,
        }
    }

    /// Whether this building type contributes to logistics capacity
    pub fn is_logistics(&self) -> bool {
        matches!(
            self,
            BuildingType::MassDriver | BuildingType::OrbitalLift | BuildingType::CargoTerminal
        )
    }
}

impl fmt::Display for BuildingType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Categories for grouping buildings in the UI
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub enum BuildingCategory {
    Infrastructure,
    /// v0.5.2: dedicated Mining category. 22 base mines (one per
    /// mineable resource) + 25 AutoMines. Split out of `Industry` so
    /// the player can pivot to mining management without scrolling
    /// through the chemical/pharma/semiconductor plants.
    Mining,
    Industry,
    Logistics,
    Power,
    Population,
    Research,
    Financial,
    Military,
}

impl BuildingCategory {
    /// Get all categories in display order
    pub fn all() -> &'static [BuildingCategory] {
        &[
            BuildingCategory::Infrastructure,
            BuildingCategory::Mining,
            BuildingCategory::Industry,
            BuildingCategory::Logistics,
            BuildingCategory::Power,
            BuildingCategory::Population,
            BuildingCategory::Research,
            BuildingCategory::Financial,
            BuildingCategory::Military,
        ]
    }

    /// Display name for UI
    pub fn display_name(&self) -> &'static str {
        match self {
            BuildingCategory::Infrastructure => "Infrastructure",
            BuildingCategory::Mining => "Mining",
            BuildingCategory::Industry => "Industry",
            BuildingCategory::Logistics => "Logistics",
            BuildingCategory::Power => "Power Generation",
            BuildingCategory::Population => "Population & Growth",
            BuildingCategory::Research => "Research & Engineering",
            BuildingCategory::Financial => "Financial & Commerce",
            BuildingCategory::Military => "Military & Shipbuilding",
        }
    }

    /// Get all building types in this category
    pub fn buildings(&self) -> Vec<BuildingType> {
        BuildingType::all()
            .iter()
            .filter(|b| b.category() == *self)
            .copied()
            .collect()
    }
}

impl fmt::Display for BuildingCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_building_type_all() {
        let all = BuildingType::all();
        // v0.5.2: 56 → 95 (added 23 base mines, 25 AutoMines; removed
        // 7 legacy generic mines: Mine, Refinery, DeepDrill, LaserDrill,
        // StripMine, HydrocarbonExtractor, RecyclingCenter).
        //   56 + 23 + 25 - 7 - 2 (GoldMine/SilverMine/PlatinumMine
        //   were already in v0.5.1; He3Mine was the v0.5.1 canary 3 that
        //   finally lands) = 95.
        // v3.2 canary 12 (2026-08-07): 95 → 97 (added 2 starter-tier
        // housing: HabitatTent 1k, HabitatModule 10k).
        // v3.10 (GRA-22c Phase 4C-2): 97 → 96 (TradePort removed;
        // maintenance draw reassigned to CommercialHub).
        assert_eq!(
            all.len(),
            96,
            "Should have exactly 96 building types (v0.5.2: 95 base + v3.2: 2 starter-tier housing − v3.10: 1 TradePort removed)"
        );
    }

    #[test]
    fn test_building_categories() {
        let categories = BuildingCategory::all();
        // v0.5.2: 8 → 9 — Mining split out of Industry.
        assert_eq!(categories.len(), 9, "Should have exactly 9 categories");

        // Every building should belong to a category
        for building in BuildingType::all() {
            let _cat = building.category();
        }
    }

    #[test]
    fn test_category_buildings_complete() {
        let total: usize = BuildingCategory::all()
            .iter()
            .map(|c| c.buildings().len())
            .sum();
        assert_eq!(
            total,
            BuildingType::all().len(),
            "All buildings should be in exactly one category"
        );
    }

    #[test]
    fn test_building_display_names() {
        assert_eq!(BuildingType::IronMine.display_name(), "Iron Mine");
        assert_eq!(BuildingType::MassDriver.display_name(), "Mass Driver");
        assert_eq!(BuildingType::FusionReactor.display_name(), "Fusion Reactor");
        assert_eq!(
            BuildingType::DTFusionReactor.display_name(),
            "D-T Fusion Reactor"
        );
        assert_eq!(BuildingType::He3Mine.display_name(), "Helium-3 Mine");
        assert_eq!(BuildingType::Shipyard.display_name(), "Shipyard");
    }

    #[test]
    fn test_building_costs_positive() {
        for building in BuildingType::all() {
            assert!(
                building.build_cost() > 0.0,
                "{} should have positive build cost",
                building.display_name()
            );
        }
    }

    #[test]
    fn test_logistics_buildings() {
        assert!(BuildingType::MassDriver.is_logistics());
        assert!(BuildingType::OrbitalLift.is_logistics());
        assert!(BuildingType::CargoTerminal.is_logistics());
        assert!(!BuildingType::IronMine.is_logistics());
        assert!(!BuildingType::Factory.is_logistics());
    }

    #[test]
    fn test_workforce_positive() {
        for building in BuildingType::all() {
            assert!(
                building.workforce_required() > 0,
                "{} should require workforce",
                building.display_name()
            );
        }
    }

    #[test]
    fn test_early_colony_workforce_feasible() {
        // A starting colony (100K pop, 40K workers) should be able to run
        // several basic buildings without hitting workforce limits immediately.
        // v0.5.2: replaced generic Mine with IronMine (5,000 workers).
        // v3.2: replaced HabitatDome with the new starter-tier
        // HabitatTent + HabitatModule — the actual first buildings
        // on a new colony (the v0.5.0 50M-per-dome is "metropolitan
        // tier" and a 100k colony can't use it).
        let early_buildings = [
            BuildingType::LifeSupport,   // 2,000
            BuildingType::HabitatTent,   // 5 (v3.2 starter-tier)
            BuildingType::HabitatModule, // 50 (v3.2 starter-tier)
            BuildingType::SolarPower,    // 500
            BuildingType::IronMine,      // 5,000 (v0.5.2: was Mine)
            BuildingType::Farm,          // 2,000 (v3.1 canary 9)
            BuildingType::AgriDome,      // 3,000
        ];
        let total: u32 = early_buildings.iter().map(|b| b.workforce_required()).sum();
        assert!(
            total <= 40_000,
            "Early colony buildings should fit in 40,000 workers, got {}",
            total
        );
    }

    #[test]
    fn test_tech_gated_buildings() {
        // v0.5.2: legacy Mine → IronMine; DeepDrill/LaserDrill/StripMine
        // removed; He3Mine requires lunar_colony; AutoMines require
        // asteroid_mining.
        // Base buildings have no tech requirement
        assert!(BuildingType::IronMine.required_tech().is_none());
        assert!(BuildingType::Factory.required_tech().is_none());

        // Advanced buildings require tech
        assert_eq!(
            BuildingType::He3Mine.required_tech(),
            Some("lunar_colony"),
            "He3Mine must be gated by lunar_colony (canary 3)"
        );
        assert_eq!(
            BuildingType::AutoIronMine.required_tech(),
            Some("asteroid_mining"),
            "AutoIronMine must be gated by asteroid_mining (v0.5.2)"
        );
        assert_eq!(
            BuildingType::AiCluster.required_tech(),
            Some("neural_networks")
        );
        assert_eq!(
            BuildingType::DHe3FusionReactor.required_tech(),
            Some("helium3_fusion")
        );
        assert_eq!(
            BuildingType::ThoriumReactor.required_tech(),
            Some("molten_salt_fission")
        );
        assert_eq!(
            BuildingType::BreederReactor.required_tech(),
            Some("breeder_reactors")
        );
        assert_eq!(
            BuildingType::Shipyard.required_tech(),
            Some("orbital_construction")
        );
        assert_eq!(
            BuildingType::MissileSilo.required_tech(),
            Some("missile_systems")
        );
    }

    #[test]
    fn test_financial_category() {
        assert_eq!(
            BuildingType::CommercialHub.category(),
            BuildingCategory::Financial
        );
        assert_eq!(
            BuildingType::FinancialCenter.category(),
            BuildingCategory::Financial
        );
        // v3.10 (GRA-22c Phase 4C-2): TradePort removed.
    }

    #[test]
    fn test_military_category() {
        assert_eq!(
            BuildingType::Shipyard.category(),
            BuildingCategory::Military
        );
        assert_eq!(
            BuildingType::MissileSilo.category(),
            BuildingCategory::Military
        );
        assert_eq!(
            BuildingType::LaunchSite.category(),
            BuildingCategory::Military
        );
    }
}
