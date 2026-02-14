use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::types::BuildingType;

/// Marker component for a colonised celestial body
#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct Colony {
    /// Colony name (defaults to body name)
    pub name: String,
    /// Total population of the colony
    pub population: f64,
    /// Population growth rate modifier (1.0 = normal)
    pub growth_rate_modifier: f64,
    /// Number of completed buildings by type
    pub buildings: HashMap<BuildingType, u32>,
}

impl Colony {
    /// Create a new colony with the given name and initial population
    pub fn new(name: String, initial_population: f64) -> Self {
        Self {
            name,
            population: initial_population,
            growth_rate_modifier: 1.0,
            buildings: HashMap::new(),
        }
    }

    /// Get the count of a specific building type
    pub fn building_count(&self, building_type: BuildingType) -> u32 {
        self.buildings.get(&building_type).copied().unwrap_or(0)
    }

    /// Add a completed building
    pub fn add_building(&mut self, building_type: BuildingType) {
        *self.buildings.entry(building_type).or_insert(0) += 1;
    }

    /// Get total number of buildings
    pub fn total_buildings(&self) -> u32 {
        self.buildings.values().sum()
    }

    /// Calculate the logistics capacity of this colony.
    ///
    /// Each logistics building contributes a set amount of capacity:
    /// - Mass Driver: 5,000 units
    /// - Orbital Lift: 20,000 units
    /// - Cargo Terminal: 2,000 units
    pub fn logistics_capacity(&self) -> f64 {
        let mass_drivers = self.building_count(BuildingType::MassDriver) as f64;
        let orbital_lifts = self.building_count(BuildingType::OrbitalLift) as f64;
        let cargo_terminals = self.building_count(BuildingType::CargoTerminal) as f64;

        mass_drivers * 5_000.0 + orbital_lifts * 20_000.0 + cargo_terminals * 2_000.0
    }

    /// Calculate the logistics demand based on colony industry.
    ///
    /// Demand scales with total industrial buildings (mines, refineries, factories,
    /// deep drills, laser drills, strip mines).
    /// A colony with no industry has zero logistics demand and thus no penalty.
    pub fn logistics_demand(&self) -> f64 {
        let industrial_buildings = (self.building_count(BuildingType::Mine)
            + self.building_count(BuildingType::Refinery)
            + self.building_count(BuildingType::Factory)
            + self.building_count(BuildingType::DeepDrill)
            + self.building_count(BuildingType::LaserDrill)
            + self.building_count(BuildingType::StripMine)) as f64;

        // 1,000 units of logistics demand per industrial building
        industrial_buildings * 1_000.0
    }

    /// Calculate the logistics efficiency factor (0.0 to 1.0).
    ///
    /// When capacity >= demand, efficiency is 1.0 (no penalty).
    /// When capacity < demand, the ratio drops, penalising mining output,
    /// research speed and population growth.
    ///
    /// A colony with no demand has 1.0 efficiency (no penalty needed).
    pub fn logistics_efficiency(&self) -> f64 {
        let demand = self.logistics_demand();
        if demand <= 0.0 {
            return 1.0;
        }
        let capacity = self.logistics_capacity();
        (capacity / demand).min(1.0)
    }

    /// Calculate housing capacity from habitat buildings.
    ///
    /// Habitat Dome houses 1,000,000 colonists, Underground Habitat houses 600,000.
    /// Values are scaled for civilization-level populations (millions to billions).
    /// Multiple domes/habitats are needed for large colony populations.
    pub fn housing_capacity(&self) -> f64 {
        let domes = self.building_count(BuildingType::HabitatDome) as f64;
        let housing_complexes = self.building_count(BuildingType::Housing) as f64;
        let underground = self.building_count(BuildingType::UndergroundHabitat) as f64;

        domes * 1_000_000.0 + housing_complexes * 250_000.0 + underground * 600_000.0
    }

    /// Calculate base population growth rate per year.
    ///
    /// Base growth: 5% per year (viable gameplay pacing at 1wk/s).
    /// At 1wk/s game speed, a 100K-pop colony reaches ~1M in ~5 real minutes.
    /// Medical centres add 1% each (up to meaningful bonus).
    /// AgriDome supports 500,000 population each (food).
    /// Growth slows as housing fills. Logistics also applies.
    pub fn population_growth_per_year(&self) -> f64 {
        if self.population <= 0.0 {
            return 0.0;
        }

        let housing = self.housing_capacity();
        if housing <= 0.0 {
            return 0.0;
        }

        // Base growth rate: 5% per year
        let base_rate = 0.05;

        // Medical centres add 1% each
        let medical_bonus =
            self.building_count(BuildingType::MedicalCenter) as f64 * 0.01;

        // Agri domes/Farms contribute to food – without them growth is halved
        let agri_count = self.building_count(BuildingType::AgriDome) as f64;
        let farm_count = self.building_count(BuildingType::Farm) as f64;
        let food_capacity = agri_count * 500_000.0 + farm_count * 1_000_000.0;
        
        let food_factor = if food_capacity > 0.0 {
            (food_capacity / self.population).min(1.0)
        } else {
            0.5 // Ship-based supply can sustain half rate
        };

        // Housing utilisation factor – growth slows as housing fills
        let utilisation = (self.population / housing).min(1.0);
        let housing_factor = 1.0 - utilisation * 0.8; // at 100% full → 0.2× growth

        // Logistics efficiency penalty
        let logistics = self.logistics_efficiency();

        let effective_rate =
            (base_rate + medical_bonus) * food_factor * housing_factor * logistics * self.growth_rate_modifier;

        self.population * effective_rate
    }

    /// Calculate mining output multiplier (affected by logistics)
    pub fn mining_output_multiplier(&self) -> f64 {
        self.logistics_efficiency()
    }

    /// Calculate research output multiplier (affected by logistics)
    pub fn research_output_multiplier(&self) -> f64 {
        // Research is less affected by logistics than mining (minimum 50%)
        let efficiency = self.logistics_efficiency();
        0.5 + 0.5 * efficiency
    }

    /// Total workforce demand across all buildings
    pub fn total_workforce_demand(&self) -> u32 {
        self.buildings
            .iter()
            .map(|(bt, count)| bt.workforce_required() * count)
            .sum()
    }

    /// Available workforce from population.
    ///
    /// Roughly 40% of the population is of working age and willing to work.
    pub fn available_workforce(&self) -> u32 {
        (self.population * 0.4) as u32
    }

    /// Workforce efficiency factor (0.0 to 1.0).
    ///
    /// When available workers >= demand, all buildings operate at full efficiency.
    /// When understaffed, output scales proportionally.
    /// A colony with zero demand has 1.0 efficiency.
    pub fn workforce_efficiency(&self) -> f64 {
        let demand = self.total_workforce_demand() as f64;
        if demand <= 0.0 {
            return 1.0;
        }
        let available = self.available_workforce() as f64;
        (available / demand).min(1.0)
    }

    /// Wealth generated per year by financial/commercial buildings.
    ///
    /// - CommercialHub: 2,000 MC/year per building (local economy)
    /// - FinancialCenter: 8,000 MC/year per building (investment returns)
    /// - TradePort: 20,000 MC/year per building (interplanetary trade)
    /// - Factories also generate 1,000 MC/year each (manufactured goods)
    ///
    /// Scaled by workforce efficiency (understaffed buildings produce less).
    pub fn wealth_generation_per_year(&self) -> f64 {
        let commercial = self.building_count(BuildingType::CommercialHub) as f64 * 2_000.0;
        let financial = self.building_count(BuildingType::FinancialCenter) as f64 * 8_000.0;
        let trade = self.building_count(BuildingType::TradePort) as f64 * 20_000.0;
        let factories = self.building_count(BuildingType::Factory) as f64 * 1_000.0;

        (commercial + financial + trade + factories) * self.workforce_efficiency()
    }

    /// Operating cost per year for all buildings.
    ///
    /// Each building has a maintenance cost proportional to its build cost.
    /// Base rate: 5% of build cost per year.
    pub fn operating_cost_per_year(&self) -> f64 {
        self.buildings
            .iter()
            .map(|(bt, count)| bt.build_cost() * 0.05 * (*count as f64))
            .sum()
    }

    /// Format population for display
    pub fn format_population(pop: f64) -> String {
        if pop >= 1_000_000_000.0 {
            format!("{:.2}B", pop / 1_000_000_000.0)
        } else if pop >= 1_000_000.0 {
            format!("{:.2}M", pop / 1_000_000.0)
        } else if pop >= 1_000.0 {
            format!("{:.1}K", pop / 1_000.0)
        } else {
            format!("{:.0}", pop)
        }
    }
}

/// An entry in the construction queue for a colony
#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct ConstructionProject {
    /// The type of building being constructed
    pub building_type: BuildingType,
    /// Build points accumulated so far
    pub progress: f64,
    /// Total build points required
    pub required: f64,
    /// The colony entity this project belongs to
    pub colony_entity: Entity,
}

impl ConstructionProject {
    /// Create a new construction project
    pub fn new(building_type: BuildingType, colony_entity: Entity) -> Self {
        Self {
            building_type,
            progress: 0.0,
            required: building_type.build_cost(),
            colony_entity,
        }
    }

    /// Get progress percentage (0.0 to 1.0)
    pub fn progress_percent(&self) -> f32 {
        if self.required <= 0.0 {
            return 1.0;
        }
        (self.progress / self.required).min(1.0) as f32
    }

    /// Check if the project is complete
    pub fn is_complete(&self) -> bool {
        self.progress >= self.required
    }
}

/// Resource that holds pending construction actions from the UI
#[derive(Resource, Debug, Clone, Default)]
pub struct PendingConstructionActions {
    /// (colony_entity, building_type) pairs to start constructing
    pub start_construction: Vec<(Entity, BuildingType)>,
    /// Construction project entities to cancel
    pub cancel_construction: Vec<Entity>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_colony_creation() {
        let colony = Colony::new("Mars Base".to_string(), 1000.0);
        assert_eq!(colony.name, "Mars Base");
        assert_eq!(colony.population, 1000.0);
        assert_eq!(colony.total_buildings(), 0);
    }

    #[test]
    fn test_colony_add_building() {
        let mut colony = Colony::new("Test".to_string(), 100.0);
        colony.add_building(BuildingType::Mine);
        colony.add_building(BuildingType::Mine);
        colony.add_building(BuildingType::Factory);

        assert_eq!(colony.building_count(BuildingType::Mine), 2);
        assert_eq!(colony.building_count(BuildingType::Factory), 1);
        assert_eq!(colony.building_count(BuildingType::Refinery), 0);
        assert_eq!(colony.total_buildings(), 3);
    }

    #[test]
    fn test_logistics_capacity() {
        let mut colony = Colony::new("Test".to_string(), 100.0);
        assert_eq!(colony.logistics_capacity(), 0.0);

        colony.add_building(BuildingType::MassDriver);
        assert_eq!(colony.logistics_capacity(), 5_000.0);

        colony.add_building(BuildingType::OrbitalLift);
        assert_eq!(colony.logistics_capacity(), 25_000.0);

        colony.add_building(BuildingType::CargoTerminal);
        assert_eq!(colony.logistics_capacity(), 27_000.0);
    }

    #[test]
    fn test_logistics_demand() {
        let mut colony = Colony::new("Test".to_string(), 100_000.0);
        // No industrial buildings → zero demand
        assert_eq!(colony.logistics_demand(), 0.0);

        colony.add_building(BuildingType::Mine);
        // 1 mine × 1000 = 1000
        assert!((colony.logistics_demand() - 1_000.0).abs() < 0.001);
    }

    #[test]
    fn test_logistics_efficiency_no_demand() {
        let colony = Colony::new("Test".to_string(), 0.0);
        assert_eq!(colony.logistics_efficiency(), 1.0);
    }

    #[test]
    fn test_logistics_efficiency_sufficient() {
        let mut colony = Colony::new("Test".to_string(), 1_000_000.0);
        colony.add_building(BuildingType::Mine); // demand: 1000
        colony.add_building(BuildingType::MassDriver); // capacity: 5000
        // 5000 / 1000 > 1.0 → clamped to 1.0
        assert_eq!(colony.logistics_efficiency(), 1.0);
    }

    #[test]
    fn test_logistics_efficiency_insufficient() {
        let mut colony = Colony::new("Test".to_string(), 10_000_000.0);
        // Add many mines without logistics
        for _ in 0..10 {
            colony.add_building(BuildingType::Mine);
        }
        // demand: 10*1000 = 10000, capacity: 0
        assert_eq!(colony.logistics_efficiency(), 0.0);
    }

    #[test]
    fn test_housing_capacity() {
        let mut colony = Colony::new("Test".to_string(), 100.0);
        assert_eq!(colony.housing_capacity(), 0.0);

        colony.add_building(BuildingType::HabitatDome);
        assert_eq!(colony.housing_capacity(), 1_000_000.0);

        colony.add_building(BuildingType::UndergroundHabitat);
        assert_eq!(colony.housing_capacity(), 1_600_000.0);
    }

    #[test]
    fn test_population_growth_no_housing() {
        let colony = Colony::new("Test".to_string(), 1000.0);
        assert_eq!(colony.population_growth_per_year(), 0.0);
    }

    #[test]
    fn test_population_growth_with_housing() {
        let mut colony = Colony::new("Test".to_string(), 100_000.0);
        colony.add_building(BuildingType::HabitatDome); // 1,000,000 capacity
        colony.add_building(BuildingType::AgriDome); // food for 500,000

        let growth = colony.population_growth_per_year();
        // Should be positive with housing and food
        assert!(growth > 0.0, "Growth should be positive: {}", growth);
    }

    #[test]
    fn test_mining_output_multiplier() {
        let mut colony = Colony::new("Test".to_string(), 1_000_000.0);
        colony.add_building(BuildingType::Mine);
        colony.add_building(BuildingType::MassDriver);

        let multiplier = colony.mining_output_multiplier();
        assert!(multiplier > 0.0 && multiplier <= 1.0);
    }

    #[test]
    fn test_research_output_multiplier_minimum() {
        let mut colony = Colony::new("Test".to_string(), 10_000_000.0);
        for _ in 0..10 {
            colony.add_building(BuildingType::Mine);
        }
        // No logistics → efficiency = 0 → research multiplier = 0.5 (minimum)
        assert!((colony.research_output_multiplier() - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_format_population() {
        assert_eq!(Colony::format_population(500.0), "500");
        assert_eq!(Colony::format_population(1_500.0), "1.5K");
        assert_eq!(Colony::format_population(2_500_000.0), "2.50M");
        assert_eq!(Colony::format_population(7_800_000_000.0), "7.80B");
    }

    #[test]
    fn test_construction_project() {
        let entity = Entity::from_raw(1);
        let project = ConstructionProject::new(BuildingType::Mine, entity);

        assert_eq!(project.building_type, BuildingType::Mine);
        assert_eq!(project.progress, 0.0);
        assert_eq!(project.required, BuildingType::Mine.build_cost());
        assert!(!project.is_complete());
        assert_eq!(project.progress_percent(), 0.0);
    }

    #[test]
    fn test_construction_project_completion() {
        let entity = Entity::from_raw(1);
        let mut project = ConstructionProject::new(BuildingType::Mine, entity);

        project.progress = project.required;
        assert!(project.is_complete());
        assert_eq!(project.progress_percent(), 1.0);
    }

    #[test]
    fn test_workforce_demand() {
        let mut colony = Colony::new("Test".to_string(), 10_000_000.0);
        assert_eq!(colony.total_workforce_demand(), 0);

        colony.add_building(BuildingType::Mine); // 5,000 workers
        assert_eq!(colony.total_workforce_demand(), 5_000);

        colony.add_building(BuildingType::Factory); // 12,000 workers
        assert_eq!(colony.total_workforce_demand(), 17_000);
    }

    #[test]
    fn test_workforce_efficiency() {
        // Large population, few buildings → full efficiency
        let mut colony = Colony::new("Test".to_string(), 10_000_000.0);
        colony.add_building(BuildingType::Mine);
        assert_eq!(colony.workforce_efficiency(), 1.0);

        // Small population, many buildings → understaffed
        let mut colony2 = Colony::new("Test".to_string(), 10_000.0);
        colony2.add_building(BuildingType::Factory); // needs 12,000 workers, has 4,000
        assert!(colony2.workforce_efficiency() < 1.0);
    }

    #[test]
    fn test_workforce_efficiency_no_buildings() {
        let colony = Colony::new("Test".to_string(), 1000.0);
        assert_eq!(colony.workforce_efficiency(), 1.0);
    }

    #[test]
    fn test_wealth_generation() {
        let mut colony = Colony::new("Test".to_string(), 10_000_000.0);
        assert_eq!(colony.wealth_generation_per_year(), 0.0);

        colony.add_building(BuildingType::CommercialHub); // 2,000 MC/year
        assert!(colony.wealth_generation_per_year() > 0.0);

        colony.add_building(BuildingType::FinancialCenter); // 8,000 MC/year
        let wealth = colony.wealth_generation_per_year();
        assert!(wealth > 2_000.0, "Should have substantial wealth: {}", wealth);
    }

    #[test]
    fn test_operating_cost() {
        let mut colony = Colony::new("Test".to_string(), 1_000_000.0);
        assert_eq!(colony.operating_cost_per_year(), 0.0);

        colony.add_building(BuildingType::Mine); // cost 400, maint = 400*0.05 = 20
        assert!((colony.operating_cost_per_year() - 20.0).abs() < 0.001);
    }
}
