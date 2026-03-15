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
    ///
    /// The starting colony (Earth) has effectively infinite logistics capacity
    /// as it represents a fully developed planetary economy that doesn't
    /// primarily rely on space-based logistics for surface operations.
    pub fn logistics_capacity(&self) -> f64 {
        if self.name == "Earth" {
            return 1_000_000_000.0;
        }

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
            + self.building_count(BuildingType::StripMine))
            as f64;

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
    /// Each building represents a district-level installation:
    /// - Housing Complex:      25,000,000 residents  (scaled for meaningful per-build impact)
    /// - Habitat Dome:         50,000,000 residents  (pressurised premium dome)
    /// - Underground Habitat:  30,000,000 residents  (buried habitat, airless worlds)
    ///
    /// At this scale Earth needs ~335 Housing Complexes (not 33,500), so
    /// each newly-built complex is a visible +0.3% capacity improvement.
    pub fn housing_capacity(&self) -> f64 {
        let domes = self.building_count(BuildingType::HabitatDome) as f64;
        let housing_complexes = self.building_count(BuildingType::Housing) as f64;
        let underground = self.building_count(BuildingType::UndergroundHabitat) as f64;

        domes * 50_000_000.0 + housing_complexes * 25_000_000.0 + underground * 30_000_000.0
    }

    /// Calculate food production rate (Mt/year) from agricultural buildings.
    ///
    /// Each building is scaled for civilisation-level throughput:
    /// - Farm:                1,000 Mt/yr  → feeds ~10M people
    /// - AgriDome:              4   Mt/yr  → feeds ~40K people (enclosed)
    /// - Greenhouse:          500   Mt/yr  → feeds ~5M people (controlled-env)
    /// - AquacultureFacility: 750   Mt/yr  → feeds ~7.5M people
    ///
    /// Per-capita food consumption: 0.0001 Mt/person/yr (100 t/person/yr).
    pub fn food_production_per_year(&self) -> f64 {
        let farm_count = self.building_count(BuildingType::Farm) as f64;
        let agri_count = self.building_count(BuildingType::AgriDome) as f64;
        let greenhouse_count = self.building_count(BuildingType::Greenhouse) as f64;
        let aquaculture_count = self.building_count(BuildingType::AquacultureFacility) as f64;
        farm_count * 1_000.0
            + agri_count * 4.0
            + greenhouse_count * 500.0
            + aquaculture_count * 750.0
    }

    /// Calculate food consumption rate (Mt/year) based on population.
    ///
    /// Per-capita consumption: 0.0001 Mt/person/year (100 tonnes/person/year).
    /// At this scale 1 Farm (1,000 Mt/yr) feeds ~10M people.
    pub fn food_consumption_per_year(&self) -> f64 {
        self.population * 0.0001
    }

    /// Calculate base population growth rate per year.
    ///
    /// Base growth: 0.9% per year (Earth 2026 demographic baseline).
    /// Medical centres add up to +0.9% bonus (capped).
    /// Growth slows as housing fills. Logistics also applies.
    ///
    /// # Arguments
    /// * `food_factor` - Food adequacy ratio (0.5 = ship supply only, 1.0 = fully fed).
    ///   Derived from the Food resource stockpile by the growth system.
    pub fn population_growth_per_year(&self, food_factor: f64) -> f64 {
        if self.population <= 0.0 {
            return 0.0;
        }

        let housing = self.housing_capacity();
        if housing <= 0.0 {
            return 0.0;
        }

        // Base growth rate: 0.9%/yr — matches Earth's 2026 demographic rate.
        // Medical centres can double this for well-served colonies.
        const BASE_GROWTH_RATE: f64 = 0.009;

        // Medical centres add 0.03% per centre, capped at +0.9% total.
        // 30 centres → full bonus.
        const MEDICAL_GROWTH_PER_CENTER: f64 = 0.0003;
        const MAX_MEDICAL_GROWTH_BONUS: f64 = 0.009;
        let medical_bonus = (self.building_count(BuildingType::MedicalCenter) as f64
            * MEDICAL_GROWTH_PER_CENTER)
            .min(MAX_MEDICAL_GROWTH_BONUS);

        // Housing utilisation factor – growth slows as housing fills
        let utilisation = (self.population / housing).min(1.0);
        let housing_factor = 1.0 - utilisation * 0.8; // at 100% full → 0.2× growth

        // Logistics efficiency penalty
        let logistics = self.logistics_efficiency();

        let effective_rate = (BASE_GROWTH_RATE + medical_bonus)
            * food_factor
            * housing_factor
            * logistics
            * self.growth_rate_modifier;

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
    /// - CommercialHub: 500 MC/year per building (local economy)
    /// - FinancialCenter: 2,000 MC/year per building (investment returns)
    /// - TradePort: 5,000 MC/year per building (interplanetary trade)
    /// - Factories also generate 100 MC/year each (manufactured goods)
    ///
    /// Scaled by workforce efficiency (understaffed buildings produce less).
    pub fn wealth_generation_per_year(&self) -> f64 {
        let commercial = self.building_count(BuildingType::CommercialHub) as f64 * 500.0;
        let financial = self.building_count(BuildingType::FinancialCenter) as f64 * 2_000.0;
        let trade = self.building_count(BuildingType::TradePort) as f64 * 5_000.0;
        let factories = self.building_count(BuildingType::Factory) as f64 * 100.0;

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
    /// When `true` the project is waiting for a `ResourceRequest` to be fulfilled
    /// before construction can begin.  Set by `process_construction_actions` when
    /// the local stockpile cannot afford the costs; cleared by `complete_deliveries`
    /// when the delivery arrives.
    #[serde(default)]
    pub awaiting_resources: bool,
    /// ID of the `ResourceRequest` that is blocking this project.
    /// `None` when the project is not blocked.
    #[serde(skip)]
    pub blocking_request_id: Option<u64>,
}

impl ConstructionProject {
    /// Create a new construction project
    pub fn new(building_type: BuildingType, colony_entity: Entity) -> Self {
        Self {
            building_type,
            progress: 0.0,
            required: building_type.build_cost(),
            colony_entity,
            awaiting_resources: false,
            blocking_request_id: None,
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
    /// Requests to establish a new outpost colony on a body.
    pub establish_outpost: Vec<EstablishOutpostRequest>,
}

/// Parameters carried from the UI when the player clicks "Establish Outpost".
#[derive(Debug, Clone)]
pub struct EstablishOutpostRequest {
    /// The celestial-body entity to colonise.
    pub body_entity: Entity,
    /// Name to give the new colony (usually the body name).
    pub colony_name: String,
    /// True when the body has no breathable atmosphere — adds O₂ maintenance.
    pub needs_oxygen: bool,
}

/// Per-colony continuous resource drain from the environment, driven by
/// population.  Used for outposts that need to import oxygen (no breathable
/// atmosphere) and/or recycle water.
///
/// Attached by `process_construction_actions` when an outpost is established.
#[derive(Component, Debug, Clone)]
pub struct ColonyEnvironmentCosts {
    /// Oxygen consumed per person per year (Mt).
    /// Set to 0.0 when the body has a breathable atmosphere.
    pub oxygen_per_person_per_year: f64,
    /// Water consumed per person per year (Mt).
    pub water_per_person_per_year: f64,
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
        assert_eq!(colony.housing_capacity(), 50_000_000.0);

        colony.add_building(BuildingType::UndergroundHabitat);
        assert_eq!(colony.housing_capacity(), 80_000_000.0);
    }

    #[test]
    fn test_population_growth_no_housing() {
        let colony = Colony::new("Test".to_string(), 1000.0);
        assert_eq!(colony.population_growth_per_year(1.0), 0.0);
    }

    #[test]
    fn test_population_growth_with_housing() {
        let mut colony = Colony::new("Test".to_string(), 100_000.0);
        colony.add_building(BuildingType::HabitatDome); // 50,000,000 capacity
        colony.add_building(BuildingType::AgriDome); // food for ~40K people

        let growth = colony.population_growth_per_year(1.0);
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
        let entity = Entity::from_raw_u32(1).unwrap();
        let project = ConstructionProject::new(BuildingType::Mine, entity);

        assert_eq!(project.building_type, BuildingType::Mine);
        assert_eq!(project.progress, 0.0);
        assert_eq!(project.required, BuildingType::Mine.build_cost());
        assert!(!project.is_complete());
        assert_eq!(project.progress_percent(), 0.0);
    }

    #[test]
    fn test_construction_project_completion() {
        let entity = Entity::from_raw_u32(1).unwrap();
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

        colony.add_building(BuildingType::CommercialHub); // 500 MC/year
        assert!(colony.wealth_generation_per_year() > 0.0);

        colony.add_building(BuildingType::FinancialCenter); // 2,000 MC/year
        let wealth = colony.wealth_generation_per_year();
        assert!(wealth > 500.0, "Should have substantial wealth: {}", wealth);
    }

    #[test]
    fn test_operating_cost() {
        let mut colony = Colony::new("Test".to_string(), 1_000_000.0);
        assert_eq!(colony.operating_cost_per_year(), 0.0);

        colony.add_building(BuildingType::Mine); // cost 400, maint = 400*0.05 = 20
        assert!((colony.operating_cost_per_year() - 20.0).abs() < 0.001);
    }
}
