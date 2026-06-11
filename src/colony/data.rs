use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

use super::types::BuildingType;

/// 2026-calibrated population scale multiplier.
///
/// Mirrors the `population_scale_multiplier: 100.0` field on the
/// `buildings.ron` root introduced by GRA-22a (LGD, PR #84 draft).
/// One Rust constant here is the single source of truth that the
/// colony founding path, building-derived per-person rates, and
/// the GRA-25/26 system work all read; the LGD RON field falls
/// back to this value when the field is absent.
///
/// Per the operator's 2026-06-06 realism bar: types are wide (f64),
/// scale is exposed explicitly, no hardcoded multiplication of
/// pre-scale numbers anywhere in the sim.
pub const POPULATION_SCALE_MULTIPLIER: f64 = 100.0;

/// Atmosphere availability for a building.  Used to hide or grey out
/// cross-atmosphere buildings in the construction panel (GRA-27).
/// `Breathable` matches a body whose `AtmosphereComposition.breathable`
/// is `true`; `None` matches a body with no atmosphere (vacuum /
/// trace gases only).  Buildings that should be buildable on every
/// kind of body include both variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AtmosphereKind {
    Breathable,
    None,
}

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

/// A synergy rule: when the colony has at least `count` buildings whose
/// `line` matches `requires_line`, the listed `effect` gets a flat additive
/// `bonus` (e.g. `0.10` = +10%).  Civ-VI-style adjacency, data-only —
/// activated/deactivated purely by colony composition at recompute time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SynergyRule {
    /// Line of buildings that must be present in the colony (e.g. "Refinery").
    /// Counted across the *colony*, not the same line as the building that
    /// owns the rule.
    pub requires_line: String,
    /// Minimum count of that line required to activate the bonus.
    pub count: u8,
    /// Effect name to bump.  Multiple rules targeting the same effect
    /// sum additively (e.g. two Mine buildings each with a +5% mining
    /// synergy give +10% total).
    pub effect: String,
    /// Additive bonus applied per qualifying rule, in the same units as
    /// the consuming system reads (e.g. 0.10 = +10% mining efficiency).
    pub bonus: f64,
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
    /// Resources consumed per year for maintenance.  GRA-22c audit
    /// enforces 4–6 entries per building — see [`audit_buildings`].
    pub maintenance_resources: Vec<ResourceCostEntry>,
    /// Modifiers applied while this building is operational
    pub modifiers: Vec<BuildingModifierDef>,
    /// Power consumed by this building in MW (megawatts).
    /// Defaults to 0 if not specified in the data file.
    #[serde(default)]
    pub power_demand_mw: f64,
    /// Tier within the building's line.  0 = base, 1+ = upgrades that
    /// replace a predecessor in the same line (see `replaces`).
    /// Default 0 so existing RON entries without this field continue
    /// to deserialize.
    #[serde(default)]
    pub tier: u8,
    /// Optional line name (e.g. `Some("Farm")`).  All buildings that
    /// share a `line` belong to the same upgrade path; `tier` orders
    /// them, `replaces` declares the predecessor.
    #[serde(default)]
    pub line: Option<String>,
    /// Optional predecessor `id` (BuildingType variant name) that this
    /// building replaces in the colony.  When non-None and the colony
    /// has at least one of the predecessor, building this one decrements
    /// the predecessor by one.  Tier-0 buildings leave this as `None`.
    #[serde(default)]
    pub replaces: Option<String>,
    /// Synergy rules granted by this building when active.  Summed
    /// across the colony at recompute time.  Default empty for
    /// buildings without adjacency (the common case).
    #[serde(default)]
    pub synergy: Vec<SynergyRule>,
    /// Atmosphere kinds this building can be constructed under.
    /// The construction panel filters the available and locked
    /// lists against the currently selected body's
    /// `AtmosphereComposition.breathable` (GRA-27).  Defaults to
    /// `[Breathable, None]` so existing RON entries without the
    /// field continue to parse.
    #[serde(default = "default_available_atmospheres")]
    pub available_atmospheres: Vec<AtmosphereKind>,
    /// RON ids of anomalies that must be `Verified` on the body
    /// before this building is available. Empty = no anomaly gate.
    /// PR-C introduced this for the `DHe3FusionReactor` /
    /// `magnetic_anomaly` pair.  Defaults to empty so existing
    /// RON entries without the field continue to parse.
    #[serde(default)]
    pub required_anomalies: Vec<String>,
}

/// Default atmosphere availability: buildable on every body kind.
fn default_available_atmospheres() -> Vec<AtmosphereKind> {
    vec![AtmosphereKind::Breathable, AtmosphereKind::None]
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
    /// Colony-population / per-build worker-unit conversion factor. Read by Rust
    /// in GRA-22b as `pub const POPULATION_SCALE_MULTIPLIER: f64`. Defaults to
    /// `100.0` so existing RON files written before GRA-22a still parse.
    #[serde(default = "default_population_scale_multiplier")]
    population_scale_multiplier: f64,
    buildings: Vec<BuildingDefinition>,
}

fn default_population_scale_multiplier() -> f64 {
    100.0
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

    /// Is the given building constructible on a body with the given
    /// atmosphere?  `body_breathable = None` means the body's
    /// atmosphere is unknown (e.g. before the body has been
    /// spawned) — pass-through: all buildings available.  The free
    /// function [`building_is_available_on`] is the source of truth;
    /// this method just threads the `&self` for convenience.
    pub fn is_available_on(
        &self,
        building_type: &BuildingType,
        body_breathable: Option<bool>,
    ) -> bool {
        let Some(def) = self.definitions.get(building_type) else {
            return true;
        };
        building_is_available_on(def, body_breathable)
    }

    /// Sum of `line == name` buildings across the colony, using the
    /// per-building `line` field on the definition.  Returns 0 if
    /// `BuildingsData` is not present.  Used by `recompute_synergies`
    /// and exposed for the unit tests.
    pub fn count_in_line(
        &self,
        colony_buildings: &std::collections::HashMap<BuildingType, u32>,
        name: &str,
    ) -> u32 {
        colony_buildings
            .iter()
            .filter_map(|(bt, count)| {
                self.definitions
                    .get(bt)
                    .and_then(|d| d.line.as_deref())
                    .and_then(|line| (line == name).then_some(*count))
            })
            .sum()
    }
}

/// Pure-logic predicate for the GRA-27 atmosphere filter.  Given a
/// building's `available_atmospheres` list and a body's
/// `breathable` flag, return `true` when the building is constructible
/// on that body.  `body_breathable = None` is the "atmosphere
/// unknown" pass-through: all buildings available (used during
/// initial UI bootstrap before the body has an
/// `AtmosphereComposition`).
///
/// A building with an empty `available_atmospheres` list is
/// deliberately hidden on every body — useful for build-cancel /
/// event-driven buildings, but not used in the current RON.
pub fn building_is_available_on(def: &BuildingDefinition, body_breathable: Option<bool>) -> bool {
    let Some(breathable) = body_breathable else {
        return true;
    };
    def.available_atmospheres.iter().any(|a| match a {
        AtmosphereKind::Breathable => breathable,
        AtmosphereKind::None => !breathable,
    })
}

/// GRA-22c maintenance audit: every building must consume 4–6 distinct
/// resources for upkeep (per plan §4.7).  Buildings outside this range
/// indicate a balance regression that should be caught at load time.
pub const MAINTENANCE_AUDIT_MIN: usize = 4;
pub const MAINTENANCE_AUDIT_MAX: usize = 6;

/// Run the 4–6 maintenance audit across a building set.  Returns the
/// list of violations (empty == pass).  Distinct resources are checked
/// — duplicate entries in `maintenance_resources` count as one.
pub fn audit_buildings(buildings: &[BuildingDefinition]) -> Vec<String> {
    let mut errors = Vec::new();
    for def in buildings {
        let mut seen: Vec<&str> = Vec::new();
        for (name, _) in &def.maintenance_resources {
            if !seen.contains(&name.as_str()) {
                seen.push(name.as_str());
            }
        }
        let n = seen.len();
        if !(MAINTENANCE_AUDIT_MIN..=MAINTENANCE_AUDIT_MAX).contains(&n) {
            errors.push(format!(
                "{}: maintenance has {} distinct resource(s), expected [{}, {}]",
                def.id, n, MAINTENANCE_AUDIT_MIN, MAINTENANCE_AUDIT_MAX
            ));
        }
    }
    errors
}

/// Parse a BuildingType from its variant name string (as used in buildings.ron)
pub(super) fn parse_building_type(id: &str) -> Option<BuildingType> {
    match id {
        "LifeSupport" => Some(BuildingType::LifeSupport),
        "HabitatDome" => Some(BuildingType::HabitatDome),
        "UndergroundHabitat" => Some(BuildingType::UndergroundHabitat),
        "Mine" => Some(BuildingType::Mine),
        "Refinery" => Some(BuildingType::Refinery),
        "Factory" => Some(BuildingType::Factory),
        "ChemicalPlant" => Some(BuildingType::ChemicalPlant),
        "AtmosphericProcessor" => Some(BuildingType::AtmosphericProcessor),
        "HydrocarbonExtractor" => Some(BuildingType::HydrocarbonExtractor),
        "DeepDrill" => Some(BuildingType::DeepDrill),
        "LaserDrill" => Some(BuildingType::LaserDrill),
        "StripMine" => Some(BuildingType::StripMine),
        "MassDriver" => Some(BuildingType::MassDriver),
        "OrbitalLift" => Some(BuildingType::OrbitalLift),
        "CargoTerminal" => Some(BuildingType::CargoTerminal),
        "SolarPower" => Some(BuildingType::SolarPower),
        "FissionReactor" => Some(BuildingType::FissionReactor),
        "FusionReactor" => Some(BuildingType::FusionReactor),
        "DTFusionReactor" => Some(BuildingType::DTFusionReactor),
        "DHe3FusionReactor" => Some(BuildingType::DHe3FusionReactor),
        "ThoriumReactor" => Some(BuildingType::ThoriumReactor),
        "BreederReactor" => Some(BuildingType::BreederReactor),
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
        "Housing" => Some(BuildingType::Housing),
        "Farm" => Some(BuildingType::Farm),
        "WindFarm" => Some(BuildingType::WindFarm),
        "HydroelectricDam" => Some(BuildingType::HydroelectricDam),
        "GeothermalPlant" => Some(BuildingType::GeothermalPlant),
        "CoalPowerPlant" => Some(BuildingType::CoalPowerPlant),
        "NaturalGasPlant" => Some(BuildingType::NaturalGasPlant),
        "SemiconductorFab" => Some(BuildingType::SemiconductorFab),
        "PharmaceuticalPlant" => Some(BuildingType::PharmaceuticalPlant),
        "WaterTreatmentPlant" => Some(BuildingType::WaterTreatmentPlant),
        "DesalinationPlant" => Some(BuildingType::DesalinationPlant),
        "RecyclingCenter" => Some(BuildingType::RecyclingCenter),
        "Greenhouse" => Some(BuildingType::Greenhouse),
        "AquacultureFacility" => Some(BuildingType::AquacultureFacility),
        "DataCenter" => Some(BuildingType::DataCenter),
        "SpacePort" => Some(BuildingType::SpacePort),
        "GroundDefenseBattery" => Some(BuildingType::GroundDefenseBattery),
        "Warehouse" => Some(BuildingType::Warehouse),
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

                // GRA-22c: load-time maintenance audit (4–6 distinct
                // resources per building, per plan §4.7).  We *warn* on
                // violations rather than panic, so debug builds still
                // run and the operator can see the data regressions
                // without a hard crash.  The accompanying unit test
                // (`test_audit_buildings_*`) is the strict pass/fail gate.
                let violations = audit_buildings(
                    buildings_data
                        .definitions
                        .values()
                        .cloned()
                        .collect::<Vec<_>>()
                        .as_slice(),
                );
                if !violations.is_empty() {
                    for v in &violations {
                        warn!("buildings.ron audit: {}", v);
                    }
                    warn!(
                        "buildings.ron audit: {} maintenance-range violation(s) (expected {}–{} distinct resources per building)",
                        violations.len(),
                        MAINTENANCE_AUDIT_MIN,
                        MAINTENANCE_AUDIT_MAX,
                    );
                } else {
                    info!(
                        "buildings.ron audit: all {} buildings have {}-{} maintenance resources",
                        count, MAINTENANCE_AUDIT_MIN, MAINTENANCE_AUDIT_MAX
                    );
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

/// Parse a ResourceType from its variant name string (as used in buildings.ron data file).
///
/// Returns `None` for unrecognised names.
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
        "Tritium" => Some(ResourceType::Tritium),
        "Uranium" => Some(ResourceType::Uranium),
        "Thorium" => Some(ResourceType::Thorium),
        "Plutonium" => Some(ResourceType::Plutonium),
        "Gold" => Some(ResourceType::Gold),
        "Silver" => Some(ResourceType::Silver),
        "Platinum" => Some(ResourceType::Platinum),
        "Copper" => Some(ResourceType::Copper),
        "RareEarths" => Some(ResourceType::RareEarths),
        "Phosphorus" => Some(ResourceType::Phosphorus),
        "Nickel" => Some(ResourceType::Nickel),
        "Tungsten" => Some(ResourceType::Tungsten),
        "Carbon" => Some(ResourceType::Carbon),
        "Deuterium" => Some(ResourceType::Deuterium),
        "Lithium" => Some(ResourceType::Lithium),
        "Sulfur" => Some(ResourceType::Sulfur),
        "Food" => Some(ResourceType::Food),
        "Chromium" => Some(ResourceType::Chromium),
        "Magnesium" => Some(ResourceType::Magnesium),
        "Cobalt" => Some(ResourceType::Cobalt),
        "Fluorine" => Some(ResourceType::Fluorine),
        "Polymers" => Some(ResourceType::Polymers),
        "Antimatter" => Some(ResourceType::Antimatter),
        "ExoticMatter" => Some(ResourceType::ExoticMatter),
        "Metamaterials" => Some(ResourceType::Metamaterials),
        "Computronium" => Some(ResourceType::Computronium),
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
        // Budget starts with Iron ≈ 625 Mt (3-month 2026 production buffer)
        let costs = vec![("Iron".to_string(), 10.0)];
        assert!(can_afford_resources(&budget, &costs));

        let too_expensive = vec![("Iron".to_string(), 100_000.0)];
        assert!(!can_afford_resources(&budget, &too_expensive));
    }

    #[test]
    fn test_deduct_resources() {
        let mut budget = crate::economy::GlobalBudget::new();
        let initial_iron = budget.get_stockpile(&crate::economy::ResourceType::Iron);
        let costs = vec![("Iron".to_string(), 5.0)];

        assert!(deduct_resources(&mut budget, &costs));
        assert!(
            (budget.get_stockpile(&crate::economy::ResourceType::Iron) - (initial_iron - 5.0))
                .abs()
                < 0.001
        );
    }

    #[test]
    fn test_deduct_resources_insufficient() {
        let mut budget = crate::economy::GlobalBudget::new();
        let costs = vec![("Iron".to_string(), 999_999.0)];

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
            power_demand_mw: 0.0,
            tier: 0,
            line: None,
            replaces: None,
            synergy: vec![],
            available_atmospheres: default_available_atmospheres(),
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
                power_demand_mw: 250.0,
                tier: 0,
                line: Some("Mine".to_string()),
                replaces: None,
                synergy: vec![],
                available_atmospheres: default_available_atmospheres(),
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

    #[test]
    fn test_every_building_has_4_to_6_maintenance_resources() {
        // GRA-22a acceptance criterion: every building must have 4–6
        // maintenance resources, so disabling a resource type noticeably
        // weakens or shuts down the building. Full audit (synergy/tiers) is
        // GRA-22c; this is the data-only seed.
        let path = "assets/data/buildings.ron";
        let contents = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return, // File not available in test env; skip.
        };
        let data = match ron::from_str::<BuildingsFile>(&contents) {
            Ok(d) => d,
            Err(_) => panic!("buildings.ron should parse (RON schema may be invalid)"),
        };
        assert!(
            data.population_scale_multiplier > 0.0,
            "population_scale_multiplier must be positive, got {}",
            data.population_scale_multiplier
        );
        for def in &data.buildings {
            let n = def.maintenance_resources.len();
            assert!(
                (4..=6).contains(&n),
                "{} has {} maintenance resources (expected 4–6): {:?}",
                def.id,
                n,
                def.maintenance_resources
            );
        }
    }

    #[test]
    fn test_production_ron_cross_atmosphere_values() {
        // GRA-27 acceptance: the four cross-atmosphere buildings in the
        // production RON must carry the values the CTO plan (and LGD
        // brief) committed to.  If LGD changes a value, this test must
        // be updated *with* the data change so the test suite
        // exercises the production RON (and not the `#[serde(default)]`
        // fallback).  Buildings *not* in this list should fall back to
        // `[Breathable, None]`.
        let path = "assets/data/buildings.ron";
        let contents = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return, // File not available in test env; skip.
        };
        let data = match ron::from_str::<BuildingsFile>(&contents) {
            Ok(d) => d,
            Err(_) => panic!("buildings.ron should parse (RON schema may be invalid)"),
        };

        let by_id: std::collections::HashMap<&str, &BuildingDefinition> =
            data.buildings.iter().map(|d| (d.id.as_str(), d)).collect();

        // Off-world only (closed environment).
        assert_eq!(
            by_id["AgriDome"].available_atmospheres,
            vec![AtmosphereKind::None],
            "AgriDome must be [None] per the CTO plan"
        );
        assert_eq!(
            by_id["UndergroundHabitat"].available_atmospheres,
            vec![AtmosphereKind::None],
            "UndergroundHabitat must be [None] per the CTO plan"
        );

        // Atmosphere-only (open-air).
        assert_eq!(
            by_id["Farm"].available_atmospheres,
            vec![AtmosphereKind::Breathable],
            "Farm must be [Breathable] per the CTO plan"
        );
        assert_eq!(
            by_id["Greenhouse"].available_atmospheres,
            vec![AtmosphereKind::Breathable],
            "Greenhouse must be [Breathable] per the CTO plan"
        );

        // A default building: confirm the serde default kicks in for
        // entries that don't declare the field.  `Mine` is a safe
        // choice — it's been on main since GRA-22a and does not need
        // an atmosphere constraint.
        assert_eq!(
            by_id["Mine"].available_atmospheres,
            vec![AtmosphereKind::Breathable, AtmosphereKind::None],
            "Mine must default to both kinds (no atmosphere gate)"
        );
    }

    // ── GRA-22c: tier / line / replaces / synergy fields + audit helper ─

    fn make_def(id: &str, maint: &[&str]) -> BuildingDefinition {
        BuildingDefinition {
            id: id.to_string(),
            display_name: id.to_string(),
            description: "test".to_string(),
            icon: "?".to_string(),
            category: "Test".to_string(),
            build_points: 100.0,
            workforce: 10,
            required_tech: "".to_string(),
            resource_costs: vec![],
            maintenance_resources: maint.iter().map(|n| ((*n).to_string(), 0.1)).collect(),
            modifiers: vec![],
            power_demand_mw: 0.0,
            tier: 0,
            line: None,
            replaces: None,
            synergy: vec![],
            available_atmospheres: default_available_atmospheres(),
        }
    }

    #[test]
    fn test_audit_buildings_pass_at_four() {
        // Lower bound: 4 distinct maintenance resources is accepted.
        let defs = vec![make_def("OK", &["Iron", "Copper", "Water", "Polymers"])];
        assert!(audit_buildings(&defs).is_empty());
    }

    #[test]
    fn test_audit_buildings_pass_at_six() {
        // Upper bound: 6 distinct maintenance resources is accepted.
        let defs = vec![make_def(
            "OK",
            &[
                "Iron",
                "Copper",
                "Water",
                "Polymers",
                "Sulfur",
                "RareEarths",
            ],
        )];
        assert!(audit_buildings(&defs).is_empty());
    }

    #[test]
    fn test_audit_buildings_fail_below_four() {
        // 3 distinct maintenance resources — must report a violation.
        let defs = vec![make_def("Bad", &["Iron", "Copper", "Water"])];
        let errs = audit_buildings(&defs);
        assert_eq!(errs.len(), 1, "expected 1 violation, got {:?}", errs);
        assert!(errs[0].contains("Bad"), "error should name the building");
        assert!(errs[0].contains("3"), "error should report the count");
    }

    #[test]
    fn test_audit_buildings_fail_above_six() {
        // 7 distinct maintenance resources — must report a violation.
        let defs = vec![make_def(
            "Bad",
            &[
                "Iron",
                "Copper",
                "Water",
                "Polymers",
                "Sulfur",
                "RareEarths",
                "Lithium",
            ],
        )];
        let errs = audit_buildings(&defs);
        assert_eq!(errs.len(), 1, "expected 1 violation, got {:?}", errs);
        assert!(errs[0].contains("Bad"), "error should name the building");
        assert!(errs[0].contains("7"), "error should report the count");
    }

    #[test]
    fn test_audit_buildings_dedupes_repeats() {
        // Duplicate resource names in the maintenance list count as one.
        // 5 entries with a duplicate Iron = 4 distinct → still passes.
        let def = BuildingDefinition {
            id: "DupIron".to_string(),
            maintenance_resources: vec![
                ("Iron".to_string(), 0.1),
                ("Iron".to_string(), 0.05), // duplicate
                ("Copper".to_string(), 0.1),
                ("Water".to_string(), 0.1),
                ("Polymers".to_string(), 0.1),
            ],
            ..make_def("DupIron", &["Iron", "Copper", "Water", "Polymers"])
        };
        assert!(audit_buildings(&[def]).is_empty());
    }

    #[test]
    fn test_audit_buildings_cumulative() {
        // Multiple violations across multiple buildings are reported in one pass.
        let defs = vec![
            make_def("A", &["Iron", "Copper", "Water"]), // 3 — too few
            make_def(
                "B",
                &[
                    "Iron",
                    "Copper",
                    "Water",
                    "Polymers",
                    "Sulfur",
                    "RareEarths",
                    "Lithium",
                ],
            ), // 7 — too many
            make_def("C", &["Iron", "Copper", "Water", "Polymers"]), // 4 — pass
        ];
        let errs = audit_buildings(&defs);
        assert_eq!(errs.len(), 2, "expected 2 violations, got {:?}", errs);
        assert!(errs.iter().any(|e| e.starts_with("A:")));
        assert!(errs.iter().any(|e| e.starts_with("B:")));
    }

    #[test]
    fn test_synergy_rule_roundtrip() {
        // SynergyRule serialises + deserialises with the same field order
        // the RON entries will use.
        let rule = SynergyRule {
            requires_line: "Refinery".to_string(),
            count: 2,
            effect: "MiningEfficiency".to_string(),
            bonus: 0.10,
        };
        let ron = ron::to_string(&rule).expect("serialize");
        let back: SynergyRule = ron::from_str(&ron).expect("deserialize");
        assert_eq!(rule, back);
    }

    #[test]
    fn test_building_definition_default_serde() {
        // The new fields (tier/line/replaces/synergy/available_atmospheres)
        // must all default so existing RON entries that lack them keep
        // deserialising.
        let minimal = r#"(
            id: "X",
            display_name: "X",
            description: "x",
            icon: "?",
            category: "Test",
            build_points: 1.0,
            workforce: 1,
            required_tech: "",
            resource_costs: [],
            maintenance_resources: [("Iron", 0.1), ("Copper", 0.1), ("Water", 0.1), ("Polymers", 0.1)],
            modifiers: [],
        )"#;
        let def: BuildingDefinition = ron::from_str(minimal).expect("parse minimal RON");
        assert_eq!(def.tier, 0);
        assert_eq!(def.line, None);
        assert_eq!(def.replaces, None);
        assert!(def.synergy.is_empty());
        assert_eq!(def.power_demand_mw, 0.0);
        assert_eq!(
            def.available_atmospheres,
            vec![AtmosphereKind::Breathable, AtmosphereKind::None],
            "missing field must default to both kinds"
        );
    }

    // ── GRA-27: atmosphere-availability field & filter logic ───────────

    #[test]
    fn test_available_atmospheres_default_is_both() {
        // Minimal RON with no `available_atmospheres` field → the
        // serde default kicks in, giving both kinds so existing
        // RON keeps parsing.
        let minimal = r#"(
            id: "X",
            display_name: "X",
            description: "x",
            icon: "?",
            category: "Test",
            build_points: 1.0,
            workforce: 1,
            required_tech: "",
            resource_costs: [],
            maintenance_resources: [("Iron", 0.1), ("Copper", 0.1), ("Water", 0.1), ("Polymers", 0.1)],
            modifiers: [],
        )"#;
        let def: BuildingDefinition = ron::from_str(minimal).expect("parse minimal RON");
        assert_eq!(
            def.available_atmospheres,
            vec![AtmosphereKind::Breathable, AtmosphereKind::None]
        );
    }

    #[test]
    fn test_available_atmospheres_specific_breathable() {
        // RON with `[Breathable]` parses to a single-element list.
        let ron = r#"(
            id: "Farm",
            display_name: "Farm",
            description: "open-air",
            icon: "F",
            category: "Food",
            build_points: 100.0,
            workforce: 50,
            required_tech: "",
            resource_costs: [],
            maintenance_resources: [("Iron", 0.1), ("Copper", 0.1), ("Water", 0.1), ("Polymers", 0.1)],
            modifiers: [],
            available_atmospheres: [Breathable],
        )"#;
        let def: BuildingDefinition = ron::from_str(ron).expect("parse Farm RON");
        assert_eq!(def.available_atmospheres, vec![AtmosphereKind::Breathable]);
    }

    #[test]
    fn test_available_atmospheres_specific_none() {
        // RON with `[None]` parses to a single-element list.  This is
        // the canonical "off-world only" building (AgriDome,
        // UndergroundHabitat).
        let ron = r#"(
            id: "AgriDome",
            display_name: "AgriDome",
            description: "closed env",
            icon: "A",
            category: "Food",
            build_points: 100.0,
            workforce: 50,
            required_tech: "hydroponics",
            resource_costs: [],
            maintenance_resources: [("Iron", 0.1), ("Copper", 0.1), ("Water", 0.1), ("Polymers", 0.1)],
            modifiers: [],
            available_atmospheres: [None],
        )"#;
        let def: BuildingDefinition = ron::from_str(ron).expect("parse AgriDome RON");
        assert_eq!(def.available_atmospheres, vec![AtmosphereKind::None]);
    }

    #[test]
    fn test_atmosphere_filter_passes_when_match() {
        // Farm is `[Breathable]`.  Earth (breathable) → available.
        let def = def_with_availability("Farm", vec![AtmosphereKind::Breathable]);
        assert!(building_is_available_on(&def, Some(true)));
        // Pass-through when the body's atmosphere is not known yet.
        assert!(building_is_available_on(&def, None));

        // Both kinds → always available regardless of the body.
        let any = def_with_availability(
            "Any",
            vec![AtmosphereKind::Breathable, AtmosphereKind::None],
        );
        assert!(building_is_available_on(&any, Some(true)));
        assert!(building_is_available_on(&any, Some(false)));
    }

    #[test]
    fn test_atmosphere_filter_fails_when_mismatch() {
        // Farm is `[Breathable]`.  Moon (not breathable) → unavailable.
        let farm = def_with_availability("Farm", vec![AtmosphereKind::Breathable]);
        assert!(!building_is_available_on(&farm, Some(false)));

        // And the symmetric case: AgriDome is `[None]`, so it must
        // be hidden on Earth.
        let agri = def_with_availability("AgriDome", vec![AtmosphereKind::None]);
        assert!(!building_is_available_on(&agri, Some(true)));
        assert!(building_is_available_on(&agri, Some(false)));
    }

    // Helper used by the failure-case test to keep it focused.
    fn def_with_availability(id: &str, atms: Vec<AtmosphereKind>) -> BuildingDefinition {
        BuildingDefinition {
            id: id.to_string(),
            display_name: id.to_string(),
            description: "test".to_string(),
            icon: "?".to_string(),
            category: "Test".to_string(),
            build_points: 100.0,
            workforce: 10,
            required_tech: "".to_string(),
            resource_costs: vec![],
            maintenance_resources: vec![],
            modifiers: vec![],
            power_demand_mw: 0.0,
            tier: 0,
            line: None,
            replaces: None,
            synergy: vec![],
            available_atmospheres: atms,
        }
    }

    // ── GRA-27: Earth vs Moon integration (construction-panel view) ────
    //
    // These tests build a `BuildingsData` populated with the four
    // cross-atmosphere buildings (Farm/Greenhouse = [Breathable],
    // AgriDome/UndergroundHabitat = [None]) and exercise the
    // `is_available_on` accessor the UI calls.  The intent is to
    // mirror what the player sees in the construction panel:
    //
    //   - On Earth (breathable): Farm shows up, AgriDome is hidden.
    //   - On the Moon (no atmosphere): AgriDome shows up, Farm is
    //     hidden.
    //
    // `hydroponics` is a hard gate for AgriDome, so it is not in
    // "available" on Earth *both* because of atmosphere AND because
    // of tech.  We don't try to model the tech-gate here — that is
    // covered by the existing `required_tech` plumbing — we only
    // check the atmosphere axis.

    fn make_cross_atmosphere_data() -> BuildingsData {
        let mut data = BuildingsData::default();
        let mut add = |bt: BuildingType, atms: Vec<AtmosphereKind>| {
            data.definitions.insert(
                bt,
                BuildingDefinition {
                    id: format!("{:?}", bt),
                    display_name: format!("{:?}", bt),
                    description: "cross-atmo test".to_string(),
                    icon: "?".to_string(),
                    category: "Test".to_string(),
                    build_points: 100.0,
                    workforce: 10,
                    required_tech: "".to_string(),
                    resource_costs: vec![],
                    maintenance_resources: vec![],
                    modifiers: vec![],
                    power_demand_mw: 0.0,
                    tier: 0,
                    line: None,
                    replaces: None,
                    synergy: vec![],
                    available_atmospheres: atms,
                },
            );
        };
        add(BuildingType::Farm, vec![AtmosphereKind::Breathable]);
        add(BuildingType::Greenhouse, vec![AtmosphereKind::Breathable]);
        add(BuildingType::AgriDome, vec![AtmosphereKind::None]);
        add(BuildingType::UndergroundHabitat, vec![AtmosphereKind::None]);
        // A default building for control: buildable everywhere.
        add(BuildingType::Mine, default_available_atmospheres());
        data
    }

    #[test]
    fn test_earth_simulation_breathable_body() {
        // body_breathable = true (Earth).
        let data = make_cross_atmosphere_data();

        // Farm: buildable on Earth.
        assert!(data.is_available_on(&BuildingType::Farm, Some(true)));
        // Greenhouse: buildable on Earth.
        assert!(data.is_available_on(&BuildingType::Greenhouse, Some(true)));
        // AgriDome: closed-env, must be hidden on Earth.
        assert!(!data.is_available_on(&BuildingType::AgriDome, Some(true)));
        // UndergroundHabitat: must be hidden on Earth.
        assert!(!data.is_available_on(&BuildingType::UndergroundHabitat, Some(true)));
        // Mine: default both, always available.
        assert!(data.is_available_on(&BuildingType::Mine, Some(true)));
    }

    #[test]
    fn test_moon_simulation_vacuum_body() {
        // body_breathable = false (Moon).
        let data = make_cross_atmosphere_data();

        // Farm: open-air, must be hidden on the Moon.
        assert!(!data.is_available_on(&BuildingType::Farm, Some(false)));
        // Greenhouse: open-air, must be hidden on the Moon.
        assert!(!data.is_available_on(&BuildingType::Greenhouse, Some(false)));
        // AgriDome: buildable on the Moon (with hydroponics unlocked —
        // tech gate is checked separately by the UI).
        assert!(data.is_available_on(&BuildingType::AgriDome, Some(false)));
        // UndergroundHabitat: buildable on the Moon.
        assert!(data.is_available_on(&BuildingType::UndergroundHabitat, Some(false)));
        // Mine: default both, always available.
        assert!(data.is_available_on(&BuildingType::Mine, Some(false)));
    }

    #[test]
    fn test_atmosphere_filter_passthrough_when_unknown() {
        // When the body has no `AtmosphereComposition` yet, every
        // building is "available" — the UI's pre-spawn bootstrap
        // view must not be empty.
        let data = make_cross_atmosphere_data();
        assert!(data.is_available_on(&BuildingType::Farm, None));
        assert!(data.is_available_on(&BuildingType::AgriDome, None));
        assert!(data.is_available_on(&BuildingType::Mine, None));
    }
}
