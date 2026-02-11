use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

use super::types::BuildingType;

/// A single resource cost entry: (resource_name, amount)
pub type ResourceCostEntry = (String, f64);

/// A building modifier entry from data file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildingModifierDef {
    /// Type of modifier (matches ModifierType variant names)
    pub modifier_type: String,
    /// Numeric value of the modifier
    pub value: f64,
}

/// A building definition loaded from the data file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildingDefinition {
    /// ID that maps to a BuildingType variant (e.g. "Mine", "DeepDrill")
    pub id: String,
    /// Display name for UI
    pub display_name: String,
    /// Short description for tooltips
    pub description: String,
    /// Icon/emoji for UI
    pub icon: String,
    /// Category name (e.g. "Infrastructure", "Industry")
    pub category: String,
    /// Construction cost in build points
    pub build_points: f64,
    /// Workforce required to operate
    pub workforce: u32,
    /// Technology ID required (empty string = always available)
    pub required_tech: String,
    /// Resources consumed from stockpile on construction
    pub resource_costs: Vec<ResourceCostEntry>,
    /// Resources consumed per year for maintenance
    pub maintenance_resources: Vec<ResourceCostEntry>,
    /// Modifiers applied while this building is operational
    pub modifiers: Vec<BuildingModifierDef>,
}

impl BuildingDefinition {
    /// Returns the required tech as an Option (None if empty string)
    pub fn required_tech_opt(&self) -> Option<&str> {
        if self.required_tech.is_empty() {
            None
        } else {
            Some(&self.required_tech)
        }
    }
}

/// Data file format for buildings
#[derive(Debug, Clone, Serialize, Deserialize)]
struct BuildingsFile {
    buildings: Vec<BuildingDefinition>,
}

/// Resource that holds all building definitions loaded from data files
#[derive(Resource, Debug, Clone, Default)]
pub struct BuildingsData {
    /// Building definitions indexed by BuildingType
    pub definitions: HashMap<BuildingType, BuildingDefinition>,
}

impl BuildingsData {
    /// Get a building definition by type
    pub fn get(&self, building_type: &BuildingType) -> Option<&BuildingDefinition> {
        self.definitions.get(building_type)
    }

    /// Get the resource costs for a building type
    pub fn resource_costs(&self, building_type: &BuildingType) -> &[ResourceCostEntry] {
        self.definitions
            .get(building_type)
            .map(|d| d.resource_costs.as_slice())
            .unwrap_or(&[])
    }

    /// Get the maintenance resources for a building type
    pub fn maintenance_resources(&self, building_type: &BuildingType) -> &[ResourceCostEntry] {
        self.definitions
            .get(building_type)
            .map(|d| d.maintenance_resources.as_slice())
            .unwrap_or(&[])
    }

    /// Get the required tech for a building type (from data file)
    pub fn required_tech(&self, building_type: &BuildingType) -> Option<&str> {
        self.definitions
            .get(building_type)
            .and_then(|d| d.required_tech_opt())
    }
}

/// Parse a BuildingType from its variant name string
fn parse_building_type(id: &str) -> Option<BuildingType> {
    match id {
        "LifeSupport" => Some(BuildingType::LifeSupport),
        "HabitatDome" => Some(BuildingType::HabitatDome),
        "UndergroundHabitat" => Some(BuildingType::UndergroundHabitat),
        "Mine" => Some(BuildingType::Mine),
        "Refinery" => Some(BuildingType::Refinery),
        "Factory" => Some(BuildingType::Factory),
        "DeepDrill" => Some(BuildingType::DeepDrill),
        "LaserDrill" => Some(BuildingType::LaserDrill),
        "StripMine" => Some(BuildingType::StripMine),
        "MassDriver" => Some(BuildingType::MassDriver),
        "OrbitalLift" => Some(BuildingType::OrbitalLift),
        "CargoTerminal" => Some(BuildingType::CargoTerminal),
        "SolarPower" => Some(BuildingType::SolarPower),
        "FissionReactor" => Some(BuildingType::FissionReactor),
        "FusionReactor" => Some(BuildingType::FusionReactor),
        "AgriDome" => Some(BuildingType::AgriDome),
        "MedicalCenter" => Some(BuildingType::MedicalCenter),
        "ResearchLab" => Some(BuildingType::ResearchLab),
        "EngineeringBay" => Some(BuildingType::EngineeringBay),
        "AiCluster" => Some(BuildingType::AiCluster),
        "CommercialHub" => Some(BuildingType::CommercialHub),
        "FinancialCenter" => Some(BuildingType::FinancialCenter),
        "TradePort" => Some(BuildingType::TradePort),
        "Shipyard" => Some(BuildingType::Shipyard),
        "MissileSilo" => Some(BuildingType::MissileSilo),
        "LaunchSite" => Some(BuildingType::LaunchSite),
        _ => None,
    }
}

/// System to load building definitions from data file at startup
pub fn load_buildings(mut commands: Commands) {
    info!("Loading building definitions...");

    let path = "assets/data/buildings.ron";

    match fs::read_to_string(path) {
        Ok(contents) => match ron::from_str::<BuildingsFile>(&contents) {
            Ok(data) => {
                let count = data.buildings.len();
                let mut buildings_data = BuildingsData::default();

                for def in data.buildings {
                    if let Some(bt) = parse_building_type(&def.id) {
                        buildings_data.definitions.insert(bt, def);
                    } else {
                        warn!("Unknown building type ID in data file: {}", def.id);
                    }
                }

                info!(
                    "Loaded {} building definitions ({} matched)",
                    count,
                    buildings_data.definitions.len()
                );

                commands.insert_resource(buildings_data);
            }
            Err(e) => {
                error!("Failed to parse building data file: {}", e);
                commands.insert_resource(BuildingsData::default());
            }
        },
        Err(e) => {
            warn!(
                "Building data file not found at {}: {}. Using defaults.",
                path, e
            );
            commands.insert_resource(BuildingsData::default());
        }
    }
}

/// Parse a ResourceType from its display name string
pub fn parse_resource_type(name: &str) -> Option<crate::economy::ResourceType> {
    use crate::economy::ResourceType;
    match name {
        "Water" => Some(ResourceType::Water),
        "Hydrogen" => Some(ResourceType::Hydrogen),
        "Ammonia" => Some(ResourceType::Ammonia),
        "Methane" => Some(ResourceType::Methane),
        "Nitrogen" => Some(ResourceType::Nitrogen),
        "Oxygen" => Some(ResourceType::Oxygen),
        "CarbonDioxide" => Some(ResourceType::CarbonDioxide),
        "Argon" => Some(ResourceType::Argon),
        "Iron" => Some(ResourceType::Iron),
        "Aluminum" => Some(ResourceType::Aluminum),
        "Titanium" => Some(ResourceType::Titanium),
        "Silicates" => Some(ResourceType::Silicates),
        "Helium3" => Some(ResourceType::Helium3),
        "Uranium" => Some(ResourceType::Uranium),
        "Thorium" => Some(ResourceType::Thorium),
        "Gold" => Some(ResourceType::Gold),
        "Silver" => Some(ResourceType::Silver),
        "Platinum" => Some(ResourceType::Platinum),
        "Copper" => Some(ResourceType::Copper),
        "RareEarths" => Some(ResourceType::RareEarths),
        _ => None,
    }
}

/// Check if all resource costs can be paid from the global budget
pub fn can_afford_resources(
    budget: &crate::economy::GlobalBudget,
    costs: &[ResourceCostEntry],
) -> bool {
    for (name, amount) in costs {
        if let Some(rt) = parse_resource_type(name) {
            if budget.get_stockpile(&rt) < *amount {
                return false;
            }
        }
    }
    true
}

/// Deduct resource costs from the global budget. Returns true if successful.
pub fn deduct_resources(
    budget: &mut crate::economy::GlobalBudget,
    costs: &[ResourceCostEntry],
) -> bool {
    // First verify all resources are available
    if !can_afford_resources(budget, costs) {
        return false;
    }
    // Then deduct
    for (name, amount) in costs {
        if let Some(rt) = parse_resource_type(name) {
            budget.consume_resource(rt, *amount);
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_building_type() {
        assert_eq!(parse_building_type("Mine"), Some(BuildingType::Mine));
        assert_eq!(
            parse_building_type("DeepDrill"),
            Some(BuildingType::DeepDrill)
        );
        assert_eq!(
            parse_building_type("Shipyard"),
            Some(BuildingType::Shipyard)
        );
        assert_eq!(parse_building_type("Unknown"), None);
    }

    #[test]
    fn test_parse_building_type_all_variants() {
        for bt in BuildingType::all() {
            let name = format!("{:?}", bt);
            assert!(
                parse_building_type(&name).is_some(),
                "parse_building_type should handle {:?}",
                bt
            );
        }
    }

    #[test]
    fn test_parse_resource_type() {
        assert_eq!(
            parse_resource_type("Iron"),
            Some(crate::economy::ResourceType::Iron)
        );
        assert_eq!(
            parse_resource_type("RareEarths"),
            Some(crate::economy::ResourceType::RareEarths)
        );
        assert_eq!(parse_resource_type("FakeResource"), None);
    }

    #[test]
    fn test_can_afford_resources() {
        let budget = crate::economy::GlobalBudget::new();
        // Budget starts with Iron=50
        let costs = vec![("Iron".to_string(), 10.0)];
        assert!(can_afford_resources(&budget, &costs));

        let too_expensive = vec![("Iron".to_string(), 1000.0)];
        assert!(!can_afford_resources(&budget, &too_expensive));
    }

    #[test]
    fn test_deduct_resources() {
        let mut budget = crate::economy::GlobalBudget::new();
        let initial_iron = budget.get_stockpile(&crate::economy::ResourceType::Iron);
        let costs = vec![("Iron".to_string(), 5.0)];

        assert!(deduct_resources(&mut budget, &costs));
        assert!(
            (budget.get_stockpile(&crate::economy::ResourceType::Iron)
                - (initial_iron - 5.0))
                .abs()
                < 0.001
        );
    }

    #[test]
    fn test_deduct_resources_insufficient() {
        let mut budget = crate::economy::GlobalBudget::new();
        let costs = vec![("Iron".to_string(), 9999.0)];

        assert!(!deduct_resources(&mut budget, &costs));
        // Stockpile unchanged
        assert!(budget.get_stockpile(&crate::economy::ResourceType::Iron) > 0.0);
    }

    #[test]
    fn test_building_definition_required_tech() {
        let def = BuildingDefinition {
            id: "Test".to_string(),
            display_name: "Test".to_string(),
            description: "Test".to_string(),
            icon: "T".to_string(),
            category: "Test".to_string(),
            build_points: 100.0,
            workforce: 10,
            required_tech: "".to_string(),
            resource_costs: vec![],
            maintenance_resources: vec![],
            modifiers: vec![],
        };
        assert!(def.required_tech_opt().is_none());

        let def2 = BuildingDefinition {
            required_tech: "fusion_power".to_string(),
            ..def
        };
        assert_eq!(def2.required_tech_opt(), Some("fusion_power"));
    }

    #[test]
    fn test_buildings_data_accessors() {
        let mut data = BuildingsData::default();
        assert!(data.get(&BuildingType::Mine).is_none());
        assert!(data.resource_costs(&BuildingType::Mine).is_empty());
        assert!(data.maintenance_resources(&BuildingType::Mine).is_empty());
        assert!(data.required_tech(&BuildingType::Mine).is_none());

        data.definitions.insert(
            BuildingType::Mine,
            BuildingDefinition {
                id: "Mine".to_string(),
                display_name: "Mine".to_string(),
                description: "Test mine".to_string(),
                icon: "⚒".to_string(),
                category: "Industry".to_string(),
                build_points: 400.0,
                workforce: 200,
                required_tech: "".to_string(),
                resource_costs: vec![("Iron".to_string(), 5.0)],
                maintenance_resources: vec![("Iron".to_string(), 0.1)],
                modifiers: vec![],
            },
        );

        assert!(data.get(&BuildingType::Mine).is_some());
        assert_eq!(data.resource_costs(&BuildingType::Mine).len(), 1);
        assert_eq!(data.maintenance_resources(&BuildingType::Mine).len(), 1);
    }

    #[test]
    fn test_load_buildings_data_file() {
        // Test that the actual data file can be parsed
        let path = "assets/data/buildings.ron";
        if let Ok(contents) = std::fs::read_to_string(path) {
            let result = ron::from_str::<BuildingsFile>(&contents);
            assert!(
                result.is_ok(),
                "buildings.ron should parse: {:?}",
                result.err()
            );
            let data = result.unwrap();
            assert!(
                data.buildings.len() >= 26,
                "Should have at least 26 buildings, got {}",
                data.buildings.len()
            );
        }
        // If file doesn't exist in test env, that's OK
    }
}
