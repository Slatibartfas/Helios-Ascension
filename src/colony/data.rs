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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub enum AtmosphereKind {
    Breathable,
    None,
}

/// A single resource cost entry: (resource_name, amount)
pub type ResourceCostEntry = (String, f64);

/// A building modifier entry from data file
#[derive(Debug, Clone, Serialize, Deserialize, Reflect)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Reflect)]
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
#[derive(Debug, Clone, Serialize, Deserialize, Reflect)]
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
    /// v0.5.2 canary 3: `BodyType` filter for buildings that are
    /// only constructible on specific body classes — e.g. `He3Mine`
    /// is restricted to `[Moon, GasGiant, Asteroid]` (the three body
    /// classes with real solar-wind-implanted or primordial He-3
    /// deposits), and all `AutoXxx` asteroid-mining buildings are
    /// restricted to `[Asteroid, Moon, GasGiant]`. An empty list
    /// means "any body type". Defaults to empty so existing RON
    /// entries without the field continue to parse.
    #[serde(default)]
    pub allowed_body_types: Vec<crate::plugins::solar_system_data::BodyType>,
    /// v0.5.2: when a future tier-1+ upgrade building wants to
    /// replace ANY building in a given `line` (rather than a single
    /// specific `replaces` id), set this to the line name. The
    /// construction system decrements the colony's count of the
    /// lowest-tier building in that line by one when the new
    /// building is added. Most buildings leave this as `None`.
    #[serde(default)]
    pub replaces_in_line: Option<String>,
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
    /// Colony-level tuning parameters (v3.6: food consumption, growth
    /// rates, workforce fraction, operating-cost fraction). Defaults to
    /// the v3.5 hard-coded values if the field is missing.
    #[serde(default)]
    colony_constants: ColonyConstants,
    buildings: Vec<BuildingDefinition>,
}

fn default_population_scale_multiplier() -> f64 {
    100.0
}

/// Resource that holds all building definitions loaded from data files
/// Colony-level tuning parameters. v3.6 moved these from
/// `src/colony/components.rs` hard-coded constants into the RON
/// data file so the calibration file is the single source of truth.
#[derive(Debug, Clone, Serialize, Deserialize, Reflect)]
pub struct ColonyConstants {
    /// Per-capita food consumption in Mt/person/yr. FAO 2024 SOFA:
    /// 1,100 kg/person/yr = 0.0000011 Mt/person/yr.
    pub food_consumption_per_capita_mt_per_year: f64,
    /// Base annual population growth rate (Earth 2026 baseline = 0.9%/yr).
    pub base_growth_rate: f64,
    /// Per-MedicalCenter additive growth bonus.
    pub medical_growth_per_center: f64,
    /// Cap on total MedicalCenter growth bonus.
    pub max_medical_growth_bonus: f64,
    /// Housing utilisation penalty (0.8 → at 100% full, growth = 0.2×).
    pub housing_utilization_penalty: f64,
    /// Working-age fraction of population.
    pub available_workforce_fraction: f64,
    /// Operating cost as fraction of build cost per year.
    pub operating_cost_fraction: f64,
    /// v3.7: food-driven growth threshold. When food production /
    /// consumption ratio drops below this, mortality kicks in.
    pub food_decline_threshold: f64,
    /// v3.7: max mortality rate at food_ratio=0. (0.005 = 0.5%/yr.)
    pub food_decline_max_mortality: f64,
    /// v3.7: per-capita consumer consumption rates (Mt/person/yr).
    /// Calibrated so 8.2B people consume ~70% of USGS 2024 /
    /// OECD 2024 / worldsteel 2024 world demand; the remaining ~30%
    /// goes to industry, maintenance, feedstock, and power gen.
    #[serde(default)]
    pub per_capita_consumption: PerCapitaConsumption,
}

/// v3.7: per-capita consumer consumption. Each field is Mt/person/year.
/// Population × field = colony's per-year draw on that resource.
#[derive(Debug, Clone, Serialize, Deserialize, Reflect)]
pub struct PerCapitaConsumption {
    pub iron_mt_per_year: f64,
    pub copper_mt_per_year: f64,
    pub aluminum_mt_per_year: f64,
    pub silicates_mt_per_year: f64,
    pub titanium_mt_per_year: f64,
    pub polymers_mt_per_year: f64,
    pub phosphorus_mt_per_year: f64,
    pub sulfur_mt_per_year: f64,
    pub nitrogen_mt_per_year: f64,
    pub methane_mt_per_year: f64,
    pub uranium_mt_per_year: f64,
    pub carbon_mt_per_year: f64,
}

impl Default for PerCapitaConsumption {
    fn default() -> Self {
        // Defaults match the RON values documented in §0.M.
        // (1 kg = 1e-9 Mt; values in Mt/person/year.)
        Self {
            iron_mt_per_year: 0.000000213,
            copper_mt_per_year: 0.0000000019,
            aluminum_mt_per_year: 0.000000006,
            silicates_mt_per_year: 0.00000041,
            titanium_mt_per_year: 0.0000000011,
            polymers_mt_per_year: 0.000000038,
            phosphorus_mt_per_year: 0.0000000188,
            sulfur_mt_per_year: 0.000000006,
            nitrogen_mt_per_year: 0.000000019,
            methane_mt_per_year: 0.00000025,
            uranium_mt_per_year: 0.0,
            carbon_mt_per_year: 0.0000007,
        }
    }
}

impl Default for ColonyConstants {
    fn default() -> Self {
        Self {
            food_consumption_per_capita_mt_per_year: 0.0000011,
            base_growth_rate: 0.009,
            medical_growth_per_center: 0.0003,
            max_medical_growth_bonus: 0.009,
            housing_utilization_penalty: 0.8,
            available_workforce_fraction: 0.4,
            operating_cost_fraction: 0.05,
            food_decline_threshold: 0.95,
            food_decline_max_mortality: 0.03,
            per_capita_consumption: PerCapitaConsumption::default(),
        }
    }
}

#[derive(Resource, Debug, Clone, Default, Reflect)]
#[reflect(Resource)]
pub struct BuildingsData {
    /// Building definitions indexed by BuildingType
    pub definitions: HashMap<BuildingType, BuildingDefinition>,
    /// Colony-level tuning parameters (v3.6: moved from
    /// `src/colony/components.rs` hard-coded constants).
    pub colony_constants: ColonyConstants,
}

impl BuildingsData {
    /// Look up the value of a specific modifier on a building, returning
    /// `0.0` if the building or modifier is absent. v3.6 helper used by
    /// the `Colony` methods that previously hard-coded per-build values.
    pub fn per_build_value(&self, building_type: BuildingType, modifier_type: &str) -> f64 {
        self.definitions
            .get(&building_type)
            .and_then(|d| d.modifiers.iter().find(|m| m.modifier_type == modifier_type))
            .map(|m| m.value)
            .unwrap_or(0.0)
    }

    /// Per-build `HousingCapacity` (residents).
    pub fn housing_capacity_for(&self, bt: BuildingType) -> f64 {
        self.per_build_value(bt, "HousingCapacity")
    }

    /// Per-build `FoodProduction` (Mt/yr).
    pub fn food_production_for(&self, bt: BuildingType) -> f64 {
        self.per_build_value(bt, "FoodProduction")
    }

    /// Per-build `WealthGeneration` (MC/yr).
    pub fn wealth_generation_for(&self, bt: BuildingType) -> f64 {
        self.per_build_value(bt, "WealthGeneration")
    }

    /// Per-build `LogisticsCapacity` (t/yr surface-to-orbit).
    pub fn logistics_capacity_for(&self, bt: BuildingType) -> f64 {
        self.per_build_value(bt, "LogisticsCapacity")
    }

    /// Load the BuildingsData from the RON file, falling back to
    /// `Default::default()` if the file is missing or malformed. Use
    /// this in tests where the Bevy Startup system hasn't run yet but
    /// you need the real per-build values.
    pub fn load_for_tests() -> Self {
        use std::fs;
        let path = "assets/data/buildings.ron";
        if let Ok(contents) = fs::read_to_string(path) {
            if let Ok(file) = ron::from_str::<BuildingsFile>(&contents) {
                let mut data = BuildingsData {
                    colony_constants: file.colony_constants,
                    ..Default::default()
                };
                for def in file.buildings {
                    if let Some(bt) = parse_building_type(&def.id) {
                        data.definitions.insert(bt, def);
                    }
                }
                return data;
            }
        }
        BuildingsData::default()
    }
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
    /// atmosphere + body type?  `body_breathable = None` means the
    /// body's atmosphere is unknown (e.g. before the body has been
    /// spawned) — pass-through: all buildings available.  `body_type
    /// = None` likewise means "body type unknown" (e.g. before the
    /// body has been spawned). The free function
    /// [`building_is_available_on`] is the source of truth; this
    /// method just threads the `&self` for convenience.
    pub fn is_available_on(
        &self,
        building_type: &BuildingType,
        body_breathable: Option<bool>,
        body_type: Option<crate::plugins::solar_system_data::BodyType>,
    ) -> bool {
        let Some(def) = self.definitions.get(building_type) else {
            return true;
        };
        building_is_available_on(def, body_breathable, body_type)
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
/// v0.5.2 canary 3: also accepts `body_type` and filters by the
/// building's `allowed_body_types` (empty = any body type).
/// `body_type = None` is the "body type unknown" pass-through.
///
/// A building with an empty `available_atmospheres` list is
/// deliberately hidden on every body — useful for build-cancel /
/// event-driven buildings, but not used in the current RON.
pub fn building_is_available_on(
    def: &BuildingDefinition,
    body_breathable: Option<bool>,
    body_type: Option<crate::plugins::solar_system_data::BodyType>,
) -> bool {
    // Atmosphere gate (v0.5.1 GRA-27)
    if let Some(breathable) = body_breathable {
        let atmosphere_ok = def.available_atmospheres.iter().any(|a| match a {
            AtmosphereKind::Breathable => breathable,
            AtmosphereKind::None => !breathable,
        });
        if !atmosphere_ok {
            return false;
        }
    }
    // Body type gate (v0.5.2 canary 3)
    if let Some(bt) = body_type {
        if !def.allowed_body_types.is_empty() && !def.allowed_body_types.contains(&bt) {
            return false;
        }
    }
    true
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
        // v0.5.2: removed legacy generic mines (Mine/Refinery/DeepDrill/
        // LaserDrill/StripMine/HydrocarbonExtractor/RecyclingCenter) in
        // favour of per-resource dedicated mines + AutoMines.
        "LifeSupport" => Some(BuildingType::LifeSupport),
        "HabitatDome" => Some(BuildingType::HabitatDome),
        "UndergroundHabitat" => Some(BuildingType::UndergroundHabitat),
        // v3.2 canary 12 (2026-08-07): starter-tier housing for new
        // colonies. See `BALANCE_PATCHES_v0.5.md` §0.I (v3.2).
        "HabitatTent" => Some(BuildingType::HabitatTent),
        "HabitatModule" => Some(BuildingType::HabitatModule),
        "WaterProcessor" => Some(BuildingType::WaterProcessor),
        // Construction mines (9)
        "IronMine" => Some(BuildingType::IronMine),
        "AluminumMine" => Some(BuildingType::AluminumMine),
        "TitaniumMine" => Some(BuildingType::TitaniumMine),
        "SilicatesMine" => Some(BuildingType::SilicatesMine),
        "NickelMine" => Some(BuildingType::NickelMine),
        "TungstenMine" => Some(BuildingType::TungstenMine),
        "CarbonMine" => Some(BuildingType::CarbonMine),
        "ChromiumMine" => Some(BuildingType::ChromiumMine),
        "MagnesiumMine" => Some(BuildingType::MagnesiumMine),
        // Precious metals (3 — v0.5.1)
        "GoldMine" => Some(BuildingType::GoldMine),
        "SilverMine" => Some(BuildingType::SilverMine),
        "PlatinumMine" => Some(BuildingType::PlatinumMine),
        // Strategic (6)
        "CopperMine" => Some(BuildingType::CopperMine),
        "RareEarthsMine" => Some(BuildingType::RareEarthsMine),
        "LithiumMine" => Some(BuildingType::LithiumMine),
        "SulfurMine" => Some(BuildingType::SulfurMine),
        "PhosphorusMine" => Some(BuildingType::PhosphorusMine),
        "CobaltMine" => Some(BuildingType::CobaltMine),
        "FluorineMine" => Some(BuildingType::FluorineMine),
        // Fissile (2)
        "UraniumMine" => Some(BuildingType::UraniumMine),
        "ThoriumMine" => Some(BuildingType::ThoriumMine),
        // Hydrocarbons (1)
        "MethaneExtractor" => Some(BuildingType::MethaneExtractor),
        // Heavy water (1)
        "DeuteriumExtractor" => Some(BuildingType::DeuteriumExtractor),
        // He-3 (1 — canary 3)
        "He3Mine" => Some(BuildingType::He3Mine),
        // AutoMines (22 — orbital/asteroid mining)
        "AutoIronMine" => Some(BuildingType::AutoIronMine),
        "AutoAluminumMine" => Some(BuildingType::AutoAluminumMine),
        "AutoTitaniumMine" => Some(BuildingType::AutoTitaniumMine),
        "AutoSilicatesMine" => Some(BuildingType::AutoSilicatesMine),
        "AutoNickelMine" => Some(BuildingType::AutoNickelMine),
        "AutoTungstenMine" => Some(BuildingType::AutoTungstenMine),
        "AutoCarbonMine" => Some(BuildingType::AutoCarbonMine),
        "AutoChromiumMine" => Some(BuildingType::AutoChromiumMine),
        "AutoMagnesiumMine" => Some(BuildingType::AutoMagnesiumMine),
        "AutoGoldMine" => Some(BuildingType::AutoGoldMine),
        "AutoSilverMine" => Some(BuildingType::AutoSilverMine),
        "AutoPlatinumMine" => Some(BuildingType::AutoPlatinumMine),
        "AutoCopperMine" => Some(BuildingType::AutoCopperMine),
        "AutoRareEarthsMine" => Some(BuildingType::AutoRareEarthsMine),
        "AutoLithiumMine" => Some(BuildingType::AutoLithiumMine),
        "AutoSulfurMine" => Some(BuildingType::AutoSulfurMine),
        "AutoPhosphorusMine" => Some(BuildingType::AutoPhosphorusMine),
        "AutoCobaltMine" => Some(BuildingType::AutoCobaltMine),
        "AutoFluorineMine" => Some(BuildingType::AutoFluorineMine),
        "AutoUraniumMine" => Some(BuildingType::AutoUraniumMine),
        "AutoThoriumMine" => Some(BuildingType::AutoThoriumMine),
        "AutoMethaneExtractor" => Some(BuildingType::AutoMethaneExtractor),
        "AutoDeuteriumExtractor" => Some(BuildingType::AutoDeuteriumExtractor),
        "AutoHe3Mine" => Some(BuildingType::AutoHe3Mine),
        "AutoWaterProcessor" => Some(BuildingType::AutoWaterProcessor),
        // Generic industry / refining
        "Factory" => Some(BuildingType::Factory),
        "ChemicalPlant" => Some(BuildingType::ChemicalPlant),
        "AtmosphericProcessor" => Some(BuildingType::AtmosphericProcessor),
        // Logistics
        "MassDriver" => Some(BuildingType::MassDriver),
        "OrbitalLift" => Some(BuildingType::OrbitalLift),
        "CargoTerminal" => Some(BuildingType::CargoTerminal),
        "Warehouse" => Some(BuildingType::Warehouse),
        // Power
        "SolarPower" => Some(BuildingType::SolarPower),
        "FissionReactor" => Some(BuildingType::FissionReactor),
        "FusionReactor" => Some(BuildingType::FusionReactor),
        "DTFusionReactor" => Some(BuildingType::DTFusionReactor),
        "DHe3FusionReactor" => Some(BuildingType::DHe3FusionReactor),
        "ThoriumReactor" => Some(BuildingType::ThoriumReactor),
        "BreederReactor" => Some(BuildingType::BreederReactor),
        "WindFarm" => Some(BuildingType::WindFarm),
        "HydroelectricDam" => Some(BuildingType::HydroelectricDam),
        "GeothermalPlant" => Some(BuildingType::GeothermalPlant),
        "CoalPowerPlant" => Some(BuildingType::CoalPowerPlant),
        "NaturalGasPlant" => Some(BuildingType::NaturalGasPlant),
        // Population
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
        "SemiconductorFab" => Some(BuildingType::SemiconductorFab),
        "PharmaceuticalPlant" => Some(BuildingType::PharmaceuticalPlant),
        "WaterTreatmentPlant" => Some(BuildingType::WaterTreatmentPlant),
        "DesalinationPlant" => Some(BuildingType::DesalinationPlant),
        "Greenhouse" => Some(BuildingType::Greenhouse),
        "AquacultureFacility" => Some(BuildingType::AquacultureFacility),
        "DataCenter" => Some(BuildingType::DataCenter),
        "SpacePort" => Some(BuildingType::SpacePort),
        "GroundDefenseBattery" => Some(BuildingType::GroundDefenseBattery),
        "OrbitalSurveyStation" => Some(BuildingType::OrbitalSurveyStation),
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
                let mut buildings_data = BuildingsData {
                    colony_constants: data.colony_constants,
                    ..Default::default()
                };

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
        // v0.5.2: replaced legacy Mine/DeepDrill with v0.5.1/v0.5.2
        // variants. (Mine, DeepDrill, Refinery, LaserDrill, StripMine,
        // HydrocarbonExtractor, RecyclingCenter were all removed.)
        assert_eq!(
            parse_building_type("IronMine"),
            Some(BuildingType::IronMine)
        );
        assert_eq!(parse_building_type("He3Mine"), Some(BuildingType::He3Mine));
        assert_eq!(
            parse_building_type("WaterProcessor"),
            Some(BuildingType::WaterProcessor)
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
            replaces_in_line: None,
            synergy: vec![],
            available_atmospheres: default_available_atmospheres(),
            required_anomalies: vec![],
            allowed_body_types: vec![],
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
        assert!(data.get(&BuildingType::IronMine).is_none());
        assert!(data.resource_costs(&BuildingType::IronMine).is_empty());
        assert!(data
            .maintenance_resources(&BuildingType::IronMine)
            .is_empty());
        assert!(data.required_tech(&BuildingType::IronMine).is_none());

        data.definitions.insert(
            BuildingType::IronMine,
            BuildingDefinition {
                id: "IronMine".to_string(),
                display_name: "Iron Mine".to_string(),
                description: "Test mine".to_string(),
                icon: "⛓".to_string(),
                category: "Industry".to_string(),
                build_points: 1500.0,
                workforce: 5000,
                required_tech: "".to_string(),
                resource_costs: vec![("Iron".to_string(), 5.0)],
                maintenance_resources: vec![("Iron".to_string(), 0.1)],
                modifiers: vec![],
                power_demand_mw: 250.0,
                tier: 0,
                line: Some("Mine".to_string()),
                replaces: None,
                replaces_in_line: None,
                synergy: vec![],
                available_atmospheres: default_available_atmospheres(),
                required_anomalies: vec![],
                allowed_body_types: vec![],
            },
        );

        assert!(data.get(&BuildingType::IronMine).is_some());
        assert_eq!(data.resource_costs(&BuildingType::IronMine).len(), 1);
        assert_eq!(data.maintenance_resources(&BuildingType::IronMine).len(), 1);
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
        // entries that don't declare the field.  `IronMine` is a safe
        // choice — it's a per-resource base mine added in the v0.5.2
        // refactor and (like every mine) does not need an atmosphere
        // constraint. v0.5.2's RON always has it; pre-v0.5.2 RON
        // would have only `Mine` (now stripped) and the test would
        // skip the assertion.
        if let Some(iron_mine) = by_id.get("IronMine") {
            assert_eq!(
                iron_mine.available_atmospheres,
                vec![AtmosphereKind::Breathable, AtmosphereKind::None],
                "IronMine must default to both kinds (no atmosphere gate)"
            );
        }
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
            replaces_in_line: None,
            synergy: vec![],
            available_atmospheres: default_available_atmospheres(),
            required_anomalies: vec![],
            allowed_body_types: vec![],
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
        // v0.5.2: signature now takes body_type (None = pass-through).
        assert!(building_is_available_on(&def, Some(true), None));
        // Pass-through when the body's atmosphere is not known yet.
        assert!(building_is_available_on(&def, None, None));

        // Both kinds → always available regardless of the body.
        let any = def_with_availability(
            "Any",
            vec![AtmosphereKind::Breathable, AtmosphereKind::None],
        );
        assert!(building_is_available_on(&any, Some(true), None));
        assert!(building_is_available_on(&any, Some(false), None));
    }

    #[test]
    fn test_atmosphere_filter_fails_when_mismatch() {
        // Farm is `[Breathable]`.  Moon (not breathable) → unavailable.
        let farm = def_with_availability("Farm", vec![AtmosphereKind::Breathable]);
        assert!(!building_is_available_on(&farm, Some(false), None));

        // And the symmetric case: AgriDome is `[None]`, so it must
        // be hidden on Earth.
        let agri = def_with_availability("AgriDome", vec![AtmosphereKind::None]);
        assert!(!building_is_available_on(&agri, Some(true), None));
        assert!(building_is_available_on(&agri, Some(false), None));
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
            replaces_in_line: None,
            synergy: vec![],
            available_atmospheres: atms,
            required_anomalies: vec![],
            allowed_body_types: vec![],
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
                    replaces_in_line: None,
                    synergy: vec![],
                    available_atmospheres: atms,
                    required_anomalies: vec![],
                    allowed_body_types: vec![],
                },
            );
        };
        add(BuildingType::Farm, vec![AtmosphereKind::Breathable]);
        add(BuildingType::Greenhouse, vec![AtmosphereKind::Breathable]);
        add(BuildingType::AgriDome, vec![AtmosphereKind::None]);
        add(BuildingType::UndergroundHabitat, vec![AtmosphereKind::None]);
        // v0.5.2: was BuildingType::Mine; replaced with IronMine
        // (the new generic default).
        add(BuildingType::IronMine, default_available_atmospheres());
        data
    }

    #[test]
    fn test_earth_simulation_breathable_body() {
        // body_breathable = true (Earth).
        let data = make_cross_atmosphere_data();
        // v0.5.2: signature now takes body_type. Tests don't
        // exercise the body-type gate (default empty list), so we
        // pass `None` to use the pass-through.
        let earth_body: Option<crate::plugins::solar_system_data::BodyType> = None;

        // Farm: buildable on Earth.
        assert!(data.is_available_on(&BuildingType::Farm, Some(true), earth_body));
        // Greenhouse: buildable on Earth.
        assert!(data.is_available_on(&BuildingType::Greenhouse, Some(true), earth_body));
        // AgriDome: closed-env, must be hidden on Earth.
        assert!(!data.is_available_on(&BuildingType::AgriDome, Some(true), earth_body));
        // UndergroundHabitat: must be hidden on Earth.
        assert!(!data.is_available_on(&BuildingType::UndergroundHabitat, Some(true), earth_body));
        // IronMine (v0.5.2: was Mine): default both, always available.
        assert!(data.is_available_on(&BuildingType::IronMine, Some(true), earth_body));
    }

    #[test]
    fn test_moon_simulation_vacuum_body() {
        // body_breathable = false (Moon).
        let data = make_cross_atmosphere_data();
        let moon_body: Option<crate::plugins::solar_system_data::BodyType> = None;

        // Farm: open-air, must be hidden on the Moon.
        assert!(!data.is_available_on(&BuildingType::Farm, Some(false), moon_body));
        // Greenhouse: open-air, must be hidden on the Moon.
        assert!(!data.is_available_on(&BuildingType::Greenhouse, Some(false), moon_body));
        // AgriDome: buildable on the Moon (with hydroponics unlocked —
        // tech gate is checked separately by the UI).
        assert!(data.is_available_on(&BuildingType::AgriDome, Some(false), moon_body));
        // UndergroundHabitat: buildable on the Moon.
        assert!(data.is_available_on(&BuildingType::UndergroundHabitat, Some(false), moon_body));
        // IronMine (v0.5.2: was Mine): default both, always available.
        assert!(data.is_available_on(&BuildingType::IronMine, Some(false), moon_body));
    }

    #[test]
    fn test_atmosphere_filter_passthrough_when_unknown() {
        // When the body has no `AtmosphereComposition` yet, every
        // building is "available" — the UI's pre-spawn bootstrap
        // view must not be empty.
        let data = make_cross_atmosphere_data();
        let unknown: Option<crate::plugins::solar_system_data::BodyType> = None;
        assert!(data.is_available_on(&BuildingType::Farm, None, unknown));
        assert!(data.is_available_on(&BuildingType::AgriDome, None, unknown));
        assert!(data.is_available_on(&BuildingType::IronMine, None, unknown));
    }

    #[test]
    fn test_body_type_filter_canary_3() {
        // v0.5.2 canary 3: He3Mine restricted to
        // [Moon, GasGiant, Asteroid].
        use crate::plugins::solar_system_data::BodyType::*;
        let mut data = BuildingsData::default();
        data.definitions.insert(
            BuildingType::He3Mine,
            BuildingDefinition {
                id: "He3Mine".to_string(),
                display_name: "He3 Mine".to_string(),
                description: "solar-wind regolith / gas-giant atmosphere".to_string(),
                icon: "☀".to_string(),
                category: "Industry".to_string(),
                build_points: 3500.0,
                workforce: 8000,
                required_tech: "lunar_colony".to_string(),
                resource_costs: vec![],
                maintenance_resources: vec![],
                modifiers: vec![],
                power_demand_mw: 100.0,
                tier: 0,
                line: Some("Mine".to_string()),
                replaces: None,
                replaces_in_line: None,
                synergy: vec![],
                available_atmospheres: default_available_atmospheres(),
                required_anomalies: vec![],
                allowed_body_types: vec![Moon, GasGiant, Asteroid],
            },
        );

        // He3Mine on Moon: available.
        assert!(data.is_available_on(&BuildingType::He3Mine, Some(true), Some(Moon)));
        // He3Mine on GasGiant: available.
        assert!(data.is_available_on(&BuildingType::He3Mine, Some(true), Some(GasGiant)));
        // He3Mine on Asteroid: available.
        assert!(data.is_available_on(&BuildingType::He3Mine, Some(true), Some(Asteroid)));
        // He3Mine on Earth (Planet): NOT available (not in allowed list).
        assert!(!data.is_available_on(&BuildingType::He3Mine, Some(true), Some(Planet)));
        // He3Mine on Star: NOT available.
        assert!(!data.is_available_on(&BuildingType::He3Mine, Some(true), Some(Star)));
        // He3Mine with body_type unknown: pass-through (buildable).
        assert!(data.is_available_on(&BuildingType::He3Mine, Some(true), None));
    }
}
