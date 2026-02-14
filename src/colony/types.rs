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
            BuildingType::DeepDrill => "Deep Drill",
            BuildingType::LaserDrill => "Laser Drill",
            BuildingType::StripMine => "Strip Mine",
            BuildingType::MassDriver => "Mass Driver",
            BuildingType::OrbitalLift => "Orbital Lift",
            BuildingType::CargoTerminal => "Cargo Terminal",
            BuildingType::SolarPower => "Solar Power Plant",
            BuildingType::FissionReactor => "Fission Reactor",
            BuildingType::FusionReactor => "Fusion Reactor",
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
        }
    }

    /// Short description for tooltips
    pub fn description(&self) -> &'static str {
        match self {
            BuildingType::LifeSupport => "Converts local volatiles into breathable atmosphere",
            BuildingType::Housing => "Standard residential buildings for habitable worlds",
            BuildingType::HabitatDome => "Provides living and working space for colonists",
            BuildingType::UndergroundHabitat => "Shelter on airless or hostile bodies",
            BuildingType::Mine => "Extracts minerals from the body surface",
            BuildingType::Refinery => "Refines raw ores into usable materials",
            BuildingType::Factory => "Manufactures goods and components",
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
            BuildingType::Factory => "🔧",
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
        }
    }

    /// Category for grouping in UI
    pub fn category(&self) -> BuildingCategory {
        match self {
            BuildingType::LifeSupport
            | BuildingType::HabitatDome
            | BuildingType::Housing
            | BuildingType::UndergroundHabitat => BuildingCategory::Infrastructure,
            BuildingType::Mine
            | BuildingType::Refinery
            | BuildingType::Factory
            | BuildingType::DeepDrill
            | BuildingType::LaserDrill
            | BuildingType::StripMine => BuildingCategory::Industry,
            BuildingType::MassDriver | BuildingType::OrbitalLift | BuildingType::CargoTerminal => {
                BuildingCategory::Logistics
            }
            BuildingType::SolarPower
            | BuildingType::FissionReactor
            | BuildingType::FusionReactor => BuildingCategory::Power,
            BuildingType::AgriDome | BuildingType::Farm | BuildingType::MedicalCenter => BuildingCategory::Population,
            BuildingType::ResearchLab
            | BuildingType::EngineeringBay
            | BuildingType::AiCluster => BuildingCategory::Research,
            BuildingType::CommercialHub
            | BuildingType::FinancialCenter
            | BuildingType::TradePort => BuildingCategory::Financial,
            BuildingType::Shipyard
            | BuildingType::MissileSilo
            | BuildingType::LaunchSite => BuildingCategory::Military,
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
            // Advanced mining – mid/late game scale
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
            BuildingType::FusionReactor => Some("fusion_power"),
            BuildingType::AiCluster => Some("neural_networks"),
            BuildingType::Shipyard => Some("orbital_construction"),
            BuildingType::MissileSilo => Some("missile_systems"),
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
        assert_eq!(all.len(), 28, "Should have exactly 28 building types");
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
            BuildingType::LifeSupport,  // 2,000
            BuildingType::HabitatDome,  // 1,000
            BuildingType::SolarPower,   // 500
            BuildingType::Mine,         // 5,000
            BuildingType::Mine,         // 5,000
            BuildingType::AgriDome,     // 4,000
        ];
        let total: u32 = early_buildings.iter().map(|b| b.workforce_required()).sum();
        assert!(total <= 40_000, "Early colony buildings should fit in 40,000 workers, got {}", total);
    }

    #[test]
    fn test_tech_gated_buildings() {
        // Base buildings have no tech requirement
        assert!(BuildingType::Mine.required_tech().is_none());
        assert!(BuildingType::Factory.required_tech().is_none());

        // Advanced buildings require tech
        assert_eq!(BuildingType::DeepDrill.required_tech(), Some("deep_drilling"));
        assert_eq!(BuildingType::LaserDrill.required_tech(), Some("laser_drilling"));
        assert_eq!(BuildingType::StripMine.required_tech(), Some("strip_mining"));
        assert_eq!(BuildingType::AiCluster.required_tech(), Some("neural_networks"));
        assert_eq!(BuildingType::Shipyard.required_tech(), Some("orbital_construction"));
        assert_eq!(BuildingType::MissileSilo.required_tech(), Some("missile_systems"));
    }

    #[test]
    fn test_financial_category() {
        assert_eq!(BuildingType::CommercialHub.category(), BuildingCategory::Financial);
        assert_eq!(BuildingType::FinancialCenter.category(), BuildingCategory::Financial);
        assert_eq!(BuildingType::TradePort.category(), BuildingCategory::Financial);
    }

    #[test]
    fn test_military_category() {
        assert_eq!(BuildingType::Shipyard.category(), BuildingCategory::Military);
        assert_eq!(BuildingType::MissileSilo.category(), BuildingCategory::Military);
        assert_eq!(BuildingType::LaunchSite.category(), BuildingCategory::Military);
    }
}
