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
    /// Provides shelter on airless/hostile bodies
    UndergroundHabitat,

    // Mining & Industry
    /// Extracts minerals from the body surface
    Mine,
    /// Refines raw ores into usable materials
    Refinery,
    /// Manufactures goods and components
    Factory,

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
    /// Medical and cloning facilities to boost population growth
    MedicalCenter,

    // Research
    /// Scientific research laboratory
    ResearchLab,
    /// Engineering workshop for component development
    EngineeringBay,
}

impl BuildingType {
    /// Get all building types in display order
    pub fn all() -> &'static [BuildingType] {
        use BuildingType::*;
        &[
            LifeSupport,
            HabitatDome,
            UndergroundHabitat,
            Mine,
            Refinery,
            Factory,
            MassDriver,
            OrbitalLift,
            CargoTerminal,
            SolarPower,
            FissionReactor,
            FusionReactor,
            AgriDome,
            MedicalCenter,
            ResearchLab,
            EngineeringBay,
        ]
    }

    /// Display name for UI
    pub fn display_name(&self) -> &'static str {
        match self {
            BuildingType::LifeSupport => "Life Support",
            BuildingType::HabitatDome => "Habitat Dome",
            BuildingType::UndergroundHabitat => "Underground Habitat",
            BuildingType::Mine => "Mine",
            BuildingType::Refinery => "Refinery",
            BuildingType::Factory => "Factory",
            BuildingType::MassDriver => "Mass Driver",
            BuildingType::OrbitalLift => "Orbital Lift",
            BuildingType::CargoTerminal => "Cargo Terminal",
            BuildingType::SolarPower => "Solar Power Plant",
            BuildingType::FissionReactor => "Fission Reactor",
            BuildingType::FusionReactor => "Fusion Reactor",
            BuildingType::AgriDome => "Agricultural Dome",
            BuildingType::MedicalCenter => "Medical Center",
            BuildingType::ResearchLab => "Research Lab",
            BuildingType::EngineeringBay => "Engineering Bay",
        }
    }

    /// Short description for tooltips
    pub fn description(&self) -> &'static str {
        match self {
            BuildingType::LifeSupport => "Converts local volatiles into breathable atmosphere",
            BuildingType::HabitatDome => "Provides living and working space for colonists",
            BuildingType::UndergroundHabitat => "Shelter on airless or hostile bodies",
            BuildingType::Mine => "Extracts minerals from the body surface",
            BuildingType::Refinery => "Refines raw ores into usable materials",
            BuildingType::Factory => "Manufactures goods and components",
            BuildingType::MassDriver => "Electromagnetic launcher for bulk cargo between bodies",
            BuildingType::OrbitalLift => "Space elevator for efficient surface-to-orbit transport",
            BuildingType::CargoTerminal => "Ground-based cargo distribution hub",
            BuildingType::SolarPower => "Solar panel arrays for power generation",
            BuildingType::FissionReactor => "Nuclear fission reactor for reliable power",
            BuildingType::FusionReactor => "Advanced fusion power plant",
            BuildingType::AgriDome => "Agricultural facilities for food production",
            BuildingType::MedicalCenter => "Medical facilities to boost population growth",
            BuildingType::ResearchLab => "Scientific research laboratory",
            BuildingType::EngineeringBay => "Engineering workshop for component development",
        }
    }

    /// Icon/emoji for UI display
    pub fn icon(&self) -> &'static str {
        match self {
            BuildingType::LifeSupport => "🌬",
            BuildingType::HabitatDome => "🏠",
            BuildingType::UndergroundHabitat => "⛏",
            BuildingType::Mine => "⚒",
            BuildingType::Refinery => "🏭",
            BuildingType::Factory => "🔧",
            BuildingType::MassDriver => "🧲",
            BuildingType::OrbitalLift => "🚡",
            BuildingType::CargoTerminal => "📦",
            BuildingType::SolarPower => "☀",
            BuildingType::FissionReactor => "☢",
            BuildingType::FusionReactor => "⚡",
            BuildingType::AgriDome => "🌾",
            BuildingType::MedicalCenter => "🏥",
            BuildingType::ResearchLab => "🔬",
            BuildingType::EngineeringBay => "🔩",
        }
    }

    /// Category for grouping in UI
    pub fn category(&self) -> BuildingCategory {
        match self {
            BuildingType::LifeSupport
            | BuildingType::HabitatDome
            | BuildingType::UndergroundHabitat => BuildingCategory::Infrastructure,
            BuildingType::Mine | BuildingType::Refinery | BuildingType::Factory => {
                BuildingCategory::Industry
            }
            BuildingType::MassDriver | BuildingType::OrbitalLift | BuildingType::CargoTerminal => {
                BuildingCategory::Logistics
            }
            BuildingType::SolarPower
            | BuildingType::FissionReactor
            | BuildingType::FusionReactor => BuildingCategory::Power,
            BuildingType::AgriDome | BuildingType::MedicalCenter => BuildingCategory::Population,
            BuildingType::ResearchLab | BuildingType::EngineeringBay => BuildingCategory::Research,
        }
    }

    /// Construction cost in build points
    pub fn build_cost(&self) -> f64 {
        match self {
            BuildingType::LifeSupport => 500.0,
            BuildingType::HabitatDome => 800.0,
            BuildingType::UndergroundHabitat => 1200.0,
            BuildingType::Mine => 400.0,
            BuildingType::Refinery => 600.0,
            BuildingType::Factory => 1000.0,
            BuildingType::MassDriver => 2000.0,
            BuildingType::OrbitalLift => 5000.0,
            BuildingType::CargoTerminal => 300.0,
            BuildingType::SolarPower => 200.0,
            BuildingType::FissionReactor => 1500.0,
            BuildingType::FusionReactor => 4000.0,
            BuildingType::AgriDome => 600.0,
            BuildingType::MedicalCenter => 800.0,
            BuildingType::ResearchLab => 1000.0,
            BuildingType::EngineeringBay => 1200.0,
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
        assert_eq!(all.len(), 16, "Should have exactly 16 building types");
    }

    #[test]
    fn test_building_categories() {
        let categories = BuildingCategory::all();
        assert_eq!(categories.len(), 6, "Should have exactly 6 categories");

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
}
