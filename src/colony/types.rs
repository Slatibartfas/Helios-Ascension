use serde::{Deserialize, Serialize};
use std::fmt;

/// Types of buildings that can be constructed on a colony
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

    // Mining & Industry
    /// Extracts minerals from the body surface
    Mine,
    /// Refines raw ores into usable materials
    Refinery,
    /// Manufactures goods and components
    Factory,

    // Atmospheric Harvesting
    /// Collects gases from a body's atmosphere
    AtmosphericProcessor,

    // Advanced Mining (tech-gated)
    /// Deep drilling into planetary crust (requires deep_drilling tech)
    DeepDrill,
    /// Laser-based deep mining (requires laser_drilling tech)
    LaserDrill,
    /// Strip mining entire surface layers (requires strip_mining tech)
    StripMine,

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
    /// Interplanetary trade port
    TradePort,

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
    /// Extraction facility for hydrocarbons (oil/gas)
    HydrocarbonExtractor,

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
    /// Recovers and re-processes waste materials
    RecyclingCenter,

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
}

impl BuildingType {
    /// Get all building types in display order
    pub fn all() -> &'static [BuildingType] {
        use BuildingType::*;
        &[
            Housing,
            LifeSupport,
            HabitatDome,
            UndergroundHabitat,
            Mine,
            Refinery,
            Factory,
            AtmosphericProcessor,
            ChemicalPlant,
            HydrocarbonExtractor,
            DeepDrill,
            LaserDrill,
            StripMine,
            MassDriver,
            OrbitalLift,
            CargoTerminal,
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
            TradePort,
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
            WaterTreatmentPlant,
            DesalinationPlant,
            RecyclingCenter,
            Greenhouse,
            AquacultureFacility,
            DataCenter,
            SpacePort,
            GroundDefenseBattery,
            Warehouse,
        ]
    }

    /// Display name for UI
    pub fn display_name(&self) -> &'static str {
        match self {
            BuildingType::LifeSupport => "Life Support",
            BuildingType::HabitatDome => "Habitat Dome",
            BuildingType::Housing => "Housing Complex",
            BuildingType::UndergroundHabitat => "Underground Habitat",
            BuildingType::Mine => "Mine",
            BuildingType::Refinery => "Refinery",
            BuildingType::Factory => "Factory",
            BuildingType::ChemicalPlant => "Chemical Plant",
            BuildingType::HydrocarbonExtractor => "Hydrocarbon Extractor",
            BuildingType::AtmosphericProcessor => "Atmospheric Processor",
            BuildingType::DeepDrill => "Deep Drill",
            BuildingType::LaserDrill => "Laser Drill",
            BuildingType::StripMine => "Strip Mine",
            BuildingType::MassDriver => "Mass Driver",
            BuildingType::OrbitalLift => "Orbital Lift",
            BuildingType::CargoTerminal => "Cargo Terminal",
            BuildingType::SolarPower => "Solar Power Plant",
            BuildingType::FissionReactor => "Fission Reactor",
            BuildingType::FusionReactor => "Fusion Reactor",
            BuildingType::DTFusionReactor => "D-T Fusion Reactor",
            BuildingType::DHe3FusionReactor => "D-He3 Fusion Reactor",
            BuildingType::ThoriumReactor => "Thorium Reactor",
            BuildingType::BreederReactor => "Breeder Reactor",
            BuildingType::AgriDome => "Agricultural Dome",
            BuildingType::Farm => "Farm",
            BuildingType::MedicalCenter => "Medical Center",
            BuildingType::ResearchLab => "Research Lab",
            BuildingType::EngineeringBay => "Engineering Bay",
            BuildingType::AiCluster => "AI Cluster",
            BuildingType::CommercialHub => "Commercial Hub",
            BuildingType::FinancialCenter => "Financial Center",
            BuildingType::TradePort => "Trade Port",
            BuildingType::Shipyard => "Shipyard",
            BuildingType::MissileSilo => "Missile Silo",
            BuildingType::LaunchSite => "Launch Site",
            BuildingType::WindFarm => "Wind Farm",
            BuildingType::HydroelectricDam => "Hydroelectric Dam",
            BuildingType::GeothermalPlant => "Geothermal Plant",
            BuildingType::CoalPowerPlant => "Coal Power Sector",
            BuildingType::NaturalGasPlant => "Gas Power Sector",
            BuildingType::SemiconductorFab => "Electronics Industry",
            BuildingType::PharmaceuticalPlant => "Pharmaceutical Sector",
            BuildingType::WaterTreatmentPlant => "Water Management Complex",
            BuildingType::DesalinationPlant => "Desalination Complex",
            BuildingType::RecyclingCenter => "Industrial Recycling Complex",
            BuildingType::Greenhouse => "Greenhouse Complex",
            BuildingType::AquacultureFacility => "Aquaculture Complex",
            BuildingType::DataCenter => "Computation Hub",
            BuildingType::SpacePort => "Space Port",
            BuildingType::GroundDefenseBattery => "Ground Defense Battery",
            BuildingType::Warehouse => "Resource Depot",
        }
    }
    pub fn description(&self) -> &'static str {
        match self {
            BuildingType::LifeSupport => "Converts local volatiles into breathable atmosphere",
            BuildingType::Housing => "Standard residential buildings for habitable worlds",
            BuildingType::HabitatDome => "Provides living and working space for colonists",
            BuildingType::UndergroundHabitat => "Shelter on airless or hostile bodies",
            BuildingType::Mine => "Extracts minerals from the body surface",
            BuildingType::Refinery => "Refines raw ores into usable materials",
            BuildingType::Factory => "Manufactures goods and components",
            BuildingType::ChemicalPlant => "Processes volatiles into useful chemical products",
            BuildingType::HydrocarbonExtractor => {
                "Extracts hydrocarbons (oil/gas) from crustal deposits"
            }
            BuildingType::AtmosphericProcessor => "Harvests gases from the atmosphere",
            BuildingType::DeepDrill => "Deep drilling into planetary crust for hidden deposits",
            BuildingType::LaserDrill => "Laser-based deep mining for maximum extraction",
            BuildingType::StripMine => "Strip mining entire surface layers at massive scale",
            BuildingType::MassDriver => "Electromagnetic launcher for bulk cargo between bodies",
            BuildingType::OrbitalLift => "Space elevator for efficient surface-to-orbit transport",
            BuildingType::CargoTerminal => "Ground-based cargo distribution hub",
            BuildingType::SolarPower => "Solar panel arrays for power generation",
            BuildingType::FissionReactor => "Nuclear fission reactor for reliable power",
            BuildingType::Farm => "Open-air food production",
            BuildingType::FusionReactor => "Advanced fusion power plant",
            BuildingType::DTFusionReactor => "High-output fusion plant using deuterium-tritium fuel",
            BuildingType::DHe3FusionReactor => "Premium fusion plant using deuterium and helium-3",
            BuildingType::ThoriumReactor => "Molten-salt thorium reactor for safe baseload power",
            BuildingType::BreederReactor => "Fast breeder reactor that produces plutonium from uranium",
            BuildingType::AgriDome => "Agricultural facilities for food production",
            BuildingType::MedicalCenter => "Medical facilities to boost population growth",
            BuildingType::ResearchLab => "Scientific research laboratory",
            BuildingType::EngineeringBay => "Engineering workshop for component development",
            BuildingType::AiCluster => "AI computation cluster boosting research and engineering",
            BuildingType::CommercialHub => "Commercial centre generating wealth from trade",
            BuildingType::FinancialCenter => "Banking and investment for wealth generation",
            BuildingType::TradePort => "Interplanetary trade port for import/export revenue",
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
            BuildingType::RecyclingCenter => "Industrial-scale material recovery and resource reprocessing",
            BuildingType::Greenhouse => "Vast network of climate-controlled crop production facilities",
            BuildingType::AquacultureFacility => "Planetary aquatic protein farming across oceans and inland seas",
            BuildingType::DataCenter => "Planetary-scale computation, AI processing, and data storage infrastructure",
            BuildingType::SpacePort => "High-throughput orbital launch complex with multiple pads",
            BuildingType::GroundDefenseBattery => "Anti-orbital and anti-missile ground defense installation",
            BuildingType::Warehouse => "Bulk storage depot that expands global resource stockpile capacity by 2.5% per depot",
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
            // ── Mining & Industry ────────────────────────────────────────
            BuildingType::Mine => &["+24.4% mining efficiency"],
            BuildingType::Refinery => &["+13% mining efficiency"],
            BuildingType::Factory => &["+10 BP/yr construction speed", "-5% construction costs"],
            BuildingType::ChemicalPlant => &[
                "+0.15 Mt/yr hydrogen",
                "+0.14 Mt/yr ammonia",
                "+0.01 Mt/yr polymers",
            ],
            BuildingType::HydrocarbonExtractor => &["+16.2% mining efficiency"],
            // ── Atmospheric Harvesting ───────────────────────────────────
            BuildingType::AtmosphericProcessor => &["+0.9 Mt/yr atmospheric harvest"],
            // ── Advanced Mining ──────────────────────────────────────────
            BuildingType::DeepDrill => &["+40.7% deep mining efficiency"],
            BuildingType::LaserDrill => &["+81.3% deep mining efficiency"],
            BuildingType::StripMine => &["+162.7% bulk mining efficiency"],
            // ── Logistics ────────────────────────────────────────────────
            BuildingType::MassDriver => &["+5,000 logistics capacity"],
            BuildingType::OrbitalLift => &["+20,000 logistics capacity"],
            BuildingType::CargoTerminal => &["+2,000 logistics capacity"],
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
            BuildingType::TradePort => &["+credits income from import/export"],
            // ── Military & Shipbuilding ──────────────────────────────────
            BuildingType::Shipyard => &[
                "Enables ship construction",
                "+10% ship efficiency, -10% build costs",
            ],
            BuildingType::MissileSilo => &["Planetary anti-orbital defense"],
            BuildingType::LaunchSite => &["Surface-to-orbit launch access"],
            BuildingType::SpacePort => &["High-throughput orbital access"],
            BuildingType::GroundDefenseBattery => &["Anti-orbital / anti-missile defense"],
            // ── Advanced Industry ────────────────────────────────────────
            BuildingType::RecyclingCenter => &["+3.2% mining efficiency", "Reduces waste"],
            BuildingType::Warehouse => &["+2.5% global stockpile capacity"],
        }
    }

    /// Icon/emoji for UI display
    pub fn icon(&self) -> &'static str {
        match self {
            BuildingType::Housing => "🏙",
            BuildingType::LifeSupport => "🌬",
            BuildingType::HabitatDome => "🏠",
            BuildingType::UndergroundHabitat => "⛏",
            BuildingType::Mine => "⚒",
            BuildingType::Refinery => "🏭",
            BuildingType::ChemicalPlant => "⚗️",
            BuildingType::HydrocarbonExtractor => "🛢️",
            BuildingType::Factory => "🏭",
            BuildingType::AtmosphericProcessor => "☁️",
            BuildingType::DeepDrill => "🕳",
            BuildingType::LaserDrill => "🔦",
            BuildingType::StripMine => "🗻",
            BuildingType::MassDriver => "🧲",
            BuildingType::OrbitalLift => "🚡",
            BuildingType::CargoTerminal => "📦",
            BuildingType::SolarPower => "☀",
            BuildingType::Farm => "🐄",
            BuildingType::FissionReactor => "☢",
            BuildingType::FusionReactor => "⚡",
            BuildingType::DTFusionReactor => "⚛",
            BuildingType::DHe3FusionReactor => "☀",
            BuildingType::ThoriumReactor => "♨️",
            BuildingType::BreederReactor => "☢️",
            BuildingType::AgriDome => "🌾",
            BuildingType::MedicalCenter => "🏥",
            BuildingType::ResearchLab => "🔬",
            BuildingType::EngineeringBay => "🔩",
            BuildingType::AiCluster => "🤖",
            BuildingType::CommercialHub => "🏪",
            BuildingType::FinancialCenter => "🏦",
            BuildingType::TradePort => "🚢",
            BuildingType::Shipyard => "⚓",
            BuildingType::MissileSilo => "🚀",
            BuildingType::LaunchSite => "🛫",
            BuildingType::WindFarm => "💨",
            BuildingType::HydroelectricDam => "🌊",
            BuildingType::GeothermalPlant => "🌋",
            BuildingType::CoalPowerPlant => "🏭",
            BuildingType::NaturalGasPlant => "🔥",
            BuildingType::SemiconductorFab => "💾",
            BuildingType::PharmaceuticalPlant => "💊",
            BuildingType::WaterTreatmentPlant => "💧",
            BuildingType::DesalinationPlant => "🧂",
            BuildingType::RecyclingCenter => "♻️",
            BuildingType::Greenhouse => "🌿",
            BuildingType::AquacultureFacility => "🐟",
            BuildingType::DataCenter => "🖥️",
            BuildingType::SpacePort => "🚀",
            BuildingType::GroundDefenseBattery => "🛡️",
            BuildingType::Warehouse => "🏗",
        }
    }

    /// Category for grouping in UI
    pub fn category(&self) -> BuildingCategory {
        match self {
            BuildingType::LifeSupport
            | BuildingType::HabitatDome
            | BuildingType::Housing
            | BuildingType::UndergroundHabitat
            | BuildingType::WaterTreatmentPlant
            | BuildingType::DesalinationPlant
            | BuildingType::RecyclingCenter => BuildingCategory::Infrastructure,
            BuildingType::Mine
            | BuildingType::Refinery
            | BuildingType::Factory
            | BuildingType::AtmosphericProcessor
            | BuildingType::ChemicalPlant
            | BuildingType::HydrocarbonExtractor
            | BuildingType::DeepDrill
            | BuildingType::LaserDrill
            | BuildingType::StripMine
            | BuildingType::SemiconductorFab
            | BuildingType::PharmaceuticalPlant => BuildingCategory::Industry,
            BuildingType::MassDriver | BuildingType::OrbitalLift | BuildingType::CargoTerminal => {
                BuildingCategory::Logistics
            }
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
            BuildingType::AgriDome
            | BuildingType::Farm
            | BuildingType::MedicalCenter
            | BuildingType::Greenhouse
            | BuildingType::AquacultureFacility => BuildingCategory::Population,
            BuildingType::ResearchLab
            | BuildingType::EngineeringBay
            | BuildingType::AiCluster
            | BuildingType::DataCenter => BuildingCategory::Research,
            BuildingType::CommercialHub
            | BuildingType::FinancialCenter
            | BuildingType::TradePort => BuildingCategory::Financial,
            BuildingType::Shipyard
            | BuildingType::MissileSilo
            | BuildingType::LaunchSite
            | BuildingType::SpacePort
            | BuildingType::GroundDefenseBattery => BuildingCategory::Military,
            BuildingType::Warehouse => BuildingCategory::Logistics,
        }
    }

    /// Construction cost in build points
    pub fn build_cost(&self) -> f64 {
        match self {
            BuildingType::LifeSupport => 500.0,
            BuildingType::HabitatDome => 800.0,
            BuildingType::Housing => 200.0,
            BuildingType::UndergroundHabitat => 1200.0,
            BuildingType::Mine => 400.0,
            BuildingType::Refinery => 600.0,
            BuildingType::Factory => 1000.0,
            BuildingType::AtmosphericProcessor => 600.0,
            BuildingType::ChemicalPlant => 800.0,
            BuildingType::HydrocarbonExtractor => 1200.0,
            BuildingType::DeepDrill => 2000.0,
            BuildingType::LaserDrill => 6000.0,
            BuildingType::StripMine => 12000.0,
            BuildingType::MassDriver => 2000.0,
            BuildingType::OrbitalLift => 5000.0,
            BuildingType::Farm => 100.0,
            BuildingType::CargoTerminal => 300.0,
            BuildingType::SolarPower => 200.0,
            BuildingType::FissionReactor => 1500.0,
            BuildingType::FusionReactor => 5000.0,
            BuildingType::DTFusionReactor => 6000.0,
            BuildingType::DHe3FusionReactor => 7000.0,
            BuildingType::ThoriumReactor => 1800.0,
            BuildingType::BreederReactor => 2600.0,
            BuildingType::AgriDome => 600.0,
            BuildingType::MedicalCenter => 800.0,
            BuildingType::ResearchLab => 1000.0,
            BuildingType::EngineeringBay => 1200.0,
            BuildingType::AiCluster => 4000.0,
            BuildingType::CommercialHub => 500.0,
            BuildingType::FinancialCenter => 1500.0,
            BuildingType::TradePort => 2500.0,
            BuildingType::Shipyard => 10000.0,
            BuildingType::MissileSilo => 3000.0,
            BuildingType::LaunchSite => 2000.0,
            BuildingType::WindFarm => 300.0,
            BuildingType::HydroelectricDam => 2500.0,
            BuildingType::GeothermalPlant => 1800.0,
            BuildingType::CoalPowerPlant => 800.0,
            BuildingType::NaturalGasPlant => 600.0,
            BuildingType::SemiconductorFab => 5000.0,
            BuildingType::PharmaceuticalPlant => 800.0,
            BuildingType::WaterTreatmentPlant => 400.0,
            BuildingType::DesalinationPlant => 600.0,
            BuildingType::RecyclingCenter => 300.0,
            BuildingType::Greenhouse => 400.0,
            BuildingType::AquacultureFacility => 500.0,
            BuildingType::DataCenter => 2000.0,
            BuildingType::SpacePort => 4000.0,
            BuildingType::GroundDefenseBattery => 2500.0,
            BuildingType::Warehouse => 300.0,
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
            // Basic industry
            BuildingType::Mine => 5_000,
            BuildingType::Refinery => 6_000,
            BuildingType::Factory => 12_000,
            BuildingType::ChemicalPlant => 4_000,
            BuildingType::HydrocarbonExtractor => 2_500,
            BuildingType::AtmosphericProcessor => 3_000, // Advanced mining – mid/late game scale
            BuildingType::DeepDrill => 10_000,
            BuildingType::LaserDrill => 4_000,
            BuildingType::StripMine => 50_000,
            // Logistics
            BuildingType::MassDriver => 2_500,
            BuildingType::OrbitalLift => 6_000,
            BuildingType::CargoTerminal => 3_000,
            // Power – largely automated
            BuildingType::SolarPower => 500,
            BuildingType::FissionReactor => 4_000,
            BuildingType::FusionReactor => 8_000,
            BuildingType::DTFusionReactor => 9_000,
            BuildingType::DHe3FusionReactor => 9_500,
            BuildingType::ThoriumReactor => 4_500,
            BuildingType::BreederReactor => 5_000,
            // Population support
            BuildingType::AgriDome => 4_000,
            BuildingType::Farm => 1_000,
            BuildingType::MedicalCenter => 6_000,
            // Research
            BuildingType::ResearchLab => 8_000,
            BuildingType::EngineeringBay => 10_000,
            BuildingType::AiCluster => 2_000,
            // Financial
            BuildingType::CommercialHub => 8_000,
            BuildingType::FinancialCenter => 10_000,
            BuildingType::TradePort => 15_000,
            // Military – large installations
            BuildingType::Shipyard => 80_000,
            BuildingType::MissileSilo => 5_000,
            BuildingType::LaunchSite => 12_000,
            // Advanced power
            BuildingType::WindFarm => 200,
            BuildingType::HydroelectricDam => 1_000,
            BuildingType::GeothermalPlant => 800,
            BuildingType::CoalPowerPlant => 2_000,
            BuildingType::NaturalGasPlant => 1_500,
            // Advanced industry
            BuildingType::SemiconductorFab => 5_000,
            BuildingType::PharmaceuticalPlant => 4_000,
            // Water & environment
            BuildingType::WaterTreatmentPlant => 500,
            BuildingType::DesalinationPlant => 400,
            BuildingType::RecyclingCenter => 1_000,
            // Advanced agriculture
            BuildingType::Greenhouse => 2_000,
            BuildingType::AquacultureFacility => 1_500,
            // Digital infrastructure
            BuildingType::DataCenter => 1_000,
            // Advanced space
            BuildingType::SpacePort => 20_000,
            BuildingType::GroundDefenseBattery => 3_000,
            // Storage
            BuildingType::Warehouse => 1_000,
        }
    }

    /// Technology ID required to unlock this building, if any.
    ///
    /// Returns `None` for base-game buildings available from the start.
    pub fn required_tech(&self) -> Option<&'static str> {
        match self {
            BuildingType::DeepDrill => Some("deep_drilling"),
            BuildingType::LaserDrill => Some("laser_drilling"),
            BuildingType::StripMine => Some("strip_mining"),
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BuildingCategory {
    Infrastructure,
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
            BuildingCategory::Industry => "Mining & Industry",
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
        assert_eq!(all.len(), 51, "Should have exactly 51 building types");
    }

    #[test]
    fn test_building_categories() {
        let categories = BuildingCategory::all();
        assert_eq!(categories.len(), 8, "Should have exactly 8 categories");

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
        assert_eq!(BuildingType::Mine.display_name(), "Mine");
        assert_eq!(BuildingType::MassDriver.display_name(), "Mass Driver");
        assert_eq!(BuildingType::FusionReactor.display_name(), "Fusion Reactor");
        assert_eq!(
            BuildingType::DTFusionReactor.display_name(),
            "D-T Fusion Reactor"
        );
        assert_eq!(BuildingType::DeepDrill.display_name(), "Deep Drill");
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
        assert!(!BuildingType::Mine.is_logistics());
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
        let early_buildings = [
            BuildingType::LifeSupport, // 2,000
            BuildingType::HabitatDome, // 1,000
            BuildingType::SolarPower,  // 500
            BuildingType::Mine,        // 5,000
            BuildingType::Mine,        // 5,000
            BuildingType::AgriDome,    // 4,000
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
        // Base buildings have no tech requirement
        assert!(BuildingType::Mine.required_tech().is_none());
        assert!(BuildingType::Factory.required_tech().is_none());

        // Advanced buildings require tech
        assert_eq!(
            BuildingType::DeepDrill.required_tech(),
            Some("deep_drilling")
        );
        assert_eq!(
            BuildingType::LaserDrill.required_tech(),
            Some("laser_drilling")
        );
        assert_eq!(
            BuildingType::StripMine.required_tech(),
            Some("strip_mining")
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
        assert_eq!(
            BuildingType::TradePort.category(),
            BuildingCategory::Financial
        );
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
