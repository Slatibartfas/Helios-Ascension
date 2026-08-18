use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::types::{BuildingCategory, BuildingType};

/// Default yield multipliers for each [`ColonyTier`].
///
/// A new colony starts at the Outpost tier and is upgraded by the player
/// (`investments` field on [`ColonyDevelopment`]) toward Civilisation parity
/// with the homeworld.  These multipliers are the **2026-anchored** values
/// agreed in the GRA-22 v2 plan: an outpost on a hostile body produces (and
/// costs to maintain) one-tenth of a civilisation-scale colony.
pub const OUTPOST_YIELD_MULTIPLIER: f64 = 0.10;
pub const SETTLEMENT_YIELD_MULTIPLIER: f64 = 0.40;
pub const CITY_YIELD_MULTIPLIER: f64 = 0.70;
pub const CIVILISATION_YIELD_MULTIPLIER: f64 = 1.00;

/// Development tier of a colony.
///
/// Drives the colony's `yield_multiplier`: production, population growth,
/// and maintenance all scale by this factor.  Tier upgrades are the player's
/// "this is no longer a tent city" decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub enum ColonyTier {
    /// A handful of pressurised buildings on a hostile world (×0.10).
    Outpost,
    /// A small, self-sufficient town (×0.40).
    Settlement,
    /// An industrial city with mature infrastructure (×0.70).
    City,
    /// A peer to the homeworld in output and consumption (×1.00).
    Civilisation,
}

impl ColonyTier {
    /// Canonical yield multiplier for this tier.
    pub fn yield_multiplier(self) -> f64 {
        match self {
            ColonyTier::Outpost => OUTPOST_YIELD_MULTIPLIER,
            ColonyTier::Settlement => SETTLEMENT_YIELD_MULTIPLIER,
            ColonyTier::City => CITY_YIELD_MULTIPLIER,
            ColonyTier::Civilisation => CIVILISATION_YIELD_MULTIPLIER,
        }
    }

    /// Display label for the UI (e.g. "Outpost × 0.10").
    pub fn display_name(self) -> &'static str {
        match self {
            ColonyTier::Outpost => "Outpost",
            ColonyTier::Settlement => "Settlement",
            ColonyTier::City => "City",
            ColonyTier::Civilisation => "Civilisation",
        }
    }
}

/// Colony development state — the multi-scale lens through which the colony
/// system reads "how industrialised is this place?".
///
/// `yield_multiplier` is a *data* field: it composes multiplicatively with
/// the base RON rate, with tech modifiers, and with any future maintenance
/// modifiers.  Type is `f64` to absorb the 2026 world production realism bar
/// (per GRA-23 + GRA-24 operator note).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Reflect)]
pub struct ColonyDevelopment {
    /// Current development tier.
    pub tier: ColonyTier,
    /// Multiplier applied to production, maintenance, and population growth.
    /// 0.10 for a brand-new Outpost, 1.00 for a Civilisation-tier world.
    pub yield_multiplier: f64,
    /// Number of tier-upgrade investments the player has applied.
    /// `tier` is *derived* from this in the upgrade action; `yield_multiplier`
    /// is the source of truth used by the systems.
    pub investments: u32,
}

impl Default for ColonyDevelopment {
    fn default() -> Self {
        Self {
            tier: ColonyTier::Outpost,
            yield_multiplier: OUTPOST_YIELD_MULTIPLIER,
            investments: 0,
        }
    }
}

/// Marker component for a colonised celestial body
#[derive(Component, Debug, Clone, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct Colony {
    /// Colony name (defaults to body name)
    pub name: String,
    /// Total population of the colony
    pub population: f64,
    /// Population growth rate modifier (1.0 = normal)
    pub growth_rate_modifier: f64,
    /// Number of completed buildings by type
    pub buildings: HashMap<BuildingType, u32>,
    /// Development state (tier, yield multiplier, upgrade count).
    /// Every founding colony starts at the Outpost tier with the Outpost
    /// yield multiplier; the homeworld is initialised to `Civilisation` at
    /// ×1.00 by the solar system populator.
    pub development: ColonyDevelopment,
}

impl Colony {
    /// Found a new colony at the **Outpost** tier with the Outpost yield
    /// multiplier (×0.10).  This is the path used by the
    /// "Establish Outpost" player action and by the bulk of test fixtures.
    pub fn new(name: String, initial_population: f64) -> Self {
        Self::new_with_tier(
            name,
            initial_population,
            ColonyTier::Outpost,
            OUTPOST_YIELD_MULTIPLIER,
        )
    }

    /// Construct a colony at an explicit tier and yield multiplier.
    /// Use this for the homeworld (Civilisation × 1.00) and for tests that
    /// need to assert yield-multiplier relationships.
    pub fn new_with_tier(
        name: String,
        initial_population: f64,
        tier: ColonyTier,
        yield_multiplier: f64,
    ) -> Self {
        Self {
            name,
            population: initial_population,
            growth_rate_modifier: 1.0,
            buildings: HashMap::new(),
            development: ColonyDevelopment {
                tier,
                yield_multiplier,
                investments: 0,
            },
        }
    }

    /// Convenience: a Civilisation-tier colony at ×1.00 yield.
    /// Used for the homeworld and for "Earth × 1.0" regression tests.
    pub fn new_civilisation(name: String, initial_population: f64) -> Self {
        Self::new_with_tier(
            name,
            initial_population,
            ColonyTier::Civilisation,
            CIVILISATION_YIELD_MULTIPLIER,
        )
    }

    /// Multiplier applied to production, maintenance, and population growth
    /// in the colony systems.  Source of truth is the `development` data
    /// field; the helper is the one-stop read for sim code and UI.
    pub fn effective_yield_multiplier(&self) -> f64 {
        self.development.yield_multiplier
    }

    /// Get the count of a specific building type
    pub fn building_count(&self, building_type: BuildingType) -> u32 {
        self.buildings.get(&building_type).copied().unwrap_or(0)
    }

    /// Add a completed building
    pub fn add_building(&mut self, building_type: BuildingType) {
        *self.buildings.entry(building_type).or_insert(0) += 1;
    }

    /// Decrement the count of `predecessor` by one, removing the entry
    /// at zero.  Returns `true` if a building was actually removed.
    ///
    /// Used by GRA-22c tier-replacement: when a player queues a tier-N
    /// building (e.g. `HydroponicsFarm`) while the colony has the
    /// predecessor (e.g. `Farm`), the predecessor count drops by one.
    pub fn remove_one_building(&mut self, predecessor: BuildingType) -> bool {
        match self.buildings.entry(predecessor) {
            std::collections::hash_map::Entry::Occupied(mut o) => {
                let v = o.get_mut();
                *v = v.saturating_sub(1);
                if *v == 0 {
                    o.remove();
                }
                true
            }
            std::collections::hash_map::Entry::Vacant(_) => false,
        }
    }

    /// v0.5.2: decrement the count of `building_type` by `n` (clamped
    /// to current count, no underflow).  Used by the Mining tab's
    /// [-] buttons for direct inventory-style edits.
    /// Returns the actual number removed.
    pub fn remove_buildings(&mut self, building_type: BuildingType, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        match self.buildings.entry(building_type) {
            std::collections::hash_map::Entry::Occupied(mut o) => {
                let v = o.get_mut();
                let actual = (*v).min(n);
                *v = v.saturating_sub(actual);
                if *v == 0 {
                    o.remove();
                }
                actual
            }
            std::collections::hash_map::Entry::Vacant(_) => 0,
        }
    }

    /// Get total number of buildings
    pub fn total_buildings(&self) -> u32 {
        self.buildings.values().sum()
    }

    /// Calculate the logistics capacity of this colony.
    ///
    /// Each logistics building contributes a set amount of capacity:
    /// v3.6: per-build `LogisticsCapacity` values are read from the RON
    /// `LogisticsCapacity` modifier on each logistics building.
    /// Hard-coded values removed.
    /// - MassDriver:    5,000 t/yr  (Earth-to-orbit bulk cargo)
    /// - OrbitalLift:  20,000 t/yr  (space elevator; 4× MassDriver)
    /// - CargoTerminal: 2,000 t/yr  (ground distribution; 0.4× MassDriver)
    ///
    /// The starting colony (Earth) has effectively infinite logistics
    /// capacity as it represents a fully developed planetary economy.
    pub fn logistics_capacity(&self, data: &super::data::BuildingsData) -> f64 {
        if self.name == "Earth" {
            return 1_000_000_000.0;
        }

        [
            BuildingType::MassDriver,
            BuildingType::OrbitalLift,
            BuildingType::CargoTerminal,
        ]
        .iter()
        .map(|bt| self.building_count(*bt) as f64 * data.logistics_capacity_for(*bt))
        .sum()
    }

    /// Calculate the logistics demand based on colony industry.
    ///
    /// Demand scales with total industrial buildings (mines,
    /// refineries, factories, chemical plants, etc.). A colony with
    /// no industry has zero logistics demand and thus no penalty.
    ///
    /// v0.5.2: the canary's `Mining` split (24 base + 25 Auto) put
    /// mines in their own category separate from `Industry`. But
    /// mines still consume logistics in the game world (they
    /// produce materials that need to be shipped off-body, and they
    /// draw power from the industrial grid), so they still count
    /// toward logistics demand. The function now counts buildings
    /// in **both** `Mining` and `Industry` — the union of the two
    /// categories covers every "industrial" structure on the body.
    pub fn logistics_demand(&self) -> f64 {
        let industrial_buildings: f64 = self
            .buildings
            .iter()
            .filter(|(bt, _)| {
                let cat = bt.category();
                cat == BuildingCategory::Mining || cat == BuildingCategory::Industry
            })
            .map(|(_, count)| *count as f64)
            .sum();

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
    pub fn logistics_efficiency(&self, data: &super::data::BuildingsData) -> f64 {
        let demand = self.logistics_demand();
        if demand <= 0.0 {
            return 1.0;
        }
        let capacity = self.logistics_capacity(data);
        (capacity / demand).min(1.0)
    }

    /// Calculate housing capacity from habitat buildings.
    ///
    /// Five tiers, from starter (1k) to metropolitan (25M):
    /// - Habitat Tent:           1,000 residents  (v3.2 starter — first building on a new colony)
    /// - Habitat Module:        10,000 residents  (v3.2 starter — second-tier for growing colonies)
    /// - Habitat Dome:        5,000,000 residents  (v3.10 — small dome, post-starter)
    /// - Underground Habitat: 8,000,000 residents  (v3.10 — buried habitat, airless worlds)
    /// - Housing Complex:    25,000,000 residents  (metropolitan workhorse, requires terraforming)
    ///
    /// v3.10 (GRA-22c Phase 4A): the gradient was Tent 1k → Module
    /// 10k → 50M (5,000× jump) → 30M (other branch). Now: 1k → 10k
    /// → 5M / 8M → 25M. The 10k → 5M step is 500×, manageable;
    /// the 5M → 25M step is 5× once the colony has city-scale
    /// infrastructure to warrant housing districts.
    ///
    /// At metropolitan scale Earth needs ~328 Housing Complexes to
    /// reach its 8.2B seed. At post-starter scale a 5M colony
    /// needs ~1 HabitatDome; at starter scale a fresh 100k-population
    /// outpost needs ~10 Habitat Tents + 5 Habitat Modules — within
    /// the v2 manageable-count band (10–50).
    ///
    /// v3.6: per-build `HousingCapacity` values are now read from the
    /// RON `HousingCapacity` modifier on each building, not hard-coded.
    /// The RON is the single source of truth.
    /// Calibration: docs/design/BALANCE_PATCHES_v0.5.md §0.I.2.
    pub fn housing_capacity(&self, data: &super::data::BuildingsData) -> f64 {
        [
            BuildingType::HabitatTent,
            BuildingType::HabitatModule,
            BuildingType::HabitatDome,
            BuildingType::Housing,
            BuildingType::UndergroundHabitat,
        ]
        .iter()
        .map(|bt| self.building_count(*bt) as f64 * data.housing_capacity_for(*bt))
        .sum()
    }

    /// v3.6: per-build `FoodProduction` values are read from the RON
    /// `FoodProduction` modifier on each food building. Hard-coded
    /// values removed (they were 25× wrong in the RON documentation
    /// but the Rust code was correct at 360/200/200/4 Mt/yr per build).
    pub fn food_production_per_year(&self, data: &super::data::BuildingsData) -> f64 {
        [
            BuildingType::Farm,
            BuildingType::AgriDome,
            BuildingType::Greenhouse,
            BuildingType::AquacultureFacility,
        ]
        .iter()
        .map(|bt| self.building_count(*bt) as f64 * data.food_production_for(*bt))
        .sum()
    }

    /// v3.6: per-capita food consumption is read from the RON
    /// `colony_constants.food_consumption_per_capita_mt_per_year`.
    /// FAO 2024 SOFA: 1,100 kg/person/yr = 0.0000011 Mt/person/yr.
    pub fn food_consumption_per_year(&self, data: &super::data::BuildingsData) -> f64 {
        self.population
            * data
                .colony_constants
                .food_consumption_per_capita_mt_per_year
    }

    /// v3.7: per-capita consumer consumption as a HashMap of
    /// (ResourceType, Mt/yr) so the maintenance system can deduct
    /// it from the local stockpile / global budget. Each value is
    /// `population × per_capita` from the RON `colony_constants.
    /// per_capita_consumption` block.
    ///
    /// Calibrated so 8.2B people consume ~70% of USGS 2024 /
    /// worldsteel 2024 / OECD 2024 / WNA 2024 / IFA 2024 / NMA 2024
    /// world demand; the remaining ~30% goes to industry,
    /// maintenance, feedstock, and power generation.
    pub fn per_capita_consumption_per_year(
        &self,
        data: &super::data::BuildingsData,
    ) -> std::collections::HashMap<crate::economy::ResourceType, f64> {
        use crate::economy::ResourceType;
        use std::collections::HashMap;
        let pcc = &data.colony_constants.per_capita_consumption;
        let pop = self.population;
        let mut out = HashMap::new();
        out.insert(ResourceType::Iron, pop * pcc.iron_mt_per_year);
        out.insert(ResourceType::Copper, pop * pcc.copper_mt_per_year);
        out.insert(ResourceType::Aluminum, pop * pcc.aluminum_mt_per_year);
        out.insert(ResourceType::Silicates, pop * pcc.silicates_mt_per_year);
        out.insert(ResourceType::Titanium, pop * pcc.titanium_mt_per_year);
        out.insert(ResourceType::Polymers, pop * pcc.polymers_mt_per_year);
        out.insert(ResourceType::Phosphorus, pop * pcc.phosphorus_mt_per_year);
        out.insert(ResourceType::Sulfur, pop * pcc.sulfur_mt_per_year);
        out.insert(ResourceType::Nitrogen, pop * pcc.nitrogen_mt_per_year);
        out.insert(ResourceType::Methane, pop * pcc.methane_mt_per_year);
        out.insert(ResourceType::Uranium, pop * pcc.uranium_mt_per_year);
        out.insert(ResourceType::Carbon, pop * pcc.carbon_mt_per_year);
        out
    }

    /// v3.8: per-body annual consumption of a single resource, summing
    /// the per-capita biological draw and the yield-scaled building
    /// maintenance draw. Used by `economy::mining::throttle_production`
    /// to compute the "consumption floor" that a body always produces
    /// to keep itself supplied, even when the local stockpile is at cap.
    ///
    /// Industrial process *inputs* (e.g. Iron consumed by SteelMill)
    /// are intentionally excluded: those are downstream of the
    /// `feasible_output_amount` input-availability throttle and would
    /// create a circular reference (the input draw depends on the
    /// output rate, which is itself throttled by the output cap).
    ///
    /// Returns Mt/yr.
    pub fn annual_resource_consumption(
        &self,
        resource: crate::economy::ResourceType,
        data: &super::data::BuildingsData,
    ) -> f64 {
        let pcc = &data.colony_constants.per_capita_consumption;
        let pop = self.population;
        let yield_mult = self.effective_yield_multiplier();

        // Per-capita (biological) draw — same per-resource map the
        // per-capita HashMap uses, just looked up by enum so we don't
        // have to allocate per call.
        let per_capita = match resource {
            crate::economy::ResourceType::Iron => pcc.iron_mt_per_year,
            crate::economy::ResourceType::Copper => pcc.copper_mt_per_year,
            crate::economy::ResourceType::Aluminum => pcc.aluminum_mt_per_year,
            crate::economy::ResourceType::Silicates => pcc.silicates_mt_per_year,
            crate::economy::ResourceType::Titanium => pcc.titanium_mt_per_year,
            crate::economy::ResourceType::Polymers => pcc.polymers_mt_per_year,
            crate::economy::ResourceType::Phosphorus => pcc.phosphorus_mt_per_year,
            crate::economy::ResourceType::Sulfur => pcc.sulfur_mt_per_year,
            crate::economy::ResourceType::Nitrogen => pcc.nitrogen_mt_per_year,
            crate::economy::ResourceType::Methane => pcc.methane_mt_per_year,
            crate::economy::ResourceType::Uranium => pcc.uranium_mt_per_year,
            crate::economy::ResourceType::Carbon => pcc.carbon_mt_per_year,
            _ => 0.0,
        };
        let population_draw = pop * per_capita;

        // Maintenance draw (yield-scaled per GRA-22 §4.7 — matches the
        // loop in `deduct_maintenance_resources`).
        let mut maintenance_draw = 0.0;
        for (bt, count) in &self.buildings {
            if *count == 0 {
                continue;
            }
            let maintenance = data.maintenance_resources(bt);
            for (res_name, amt) in maintenance {
                if let Some(rt) = super::data::parse_resource_type(res_name) {
                    if rt == resource {
                        maintenance_draw += amt * (*count as f64) * yield_mult;
                    }
                }
            }
        }

        population_draw + maintenance_draw
    }

    /// Calculate base population growth rate per year.
    ///
    /// Base growth: 0.9% per year (Earth 2026 demographic baseline).
    /// Medical centres add up to +0.9% bonus (capped).
    /// Growth slows as housing fills. Logistics also applies.
    ///
    /// v3.6: base growth, medical bonus, and housing-utilisation penalty
    /// are read from `colony_constants` (v3.5 had them as `const`s in
    /// this file).
    ///
    /// v3.7 (steeper curve, v3.7.1): food growth factor is
    /// `(2*ratio - 1)^1.5` clamped to [0, 1] for ratio in [0.5, 1.0].
    /// Below 0.5 the factor is 0 (no growth). Mortality kicks in
    /// when ratio < `food_decline_threshold` (default 0.95) and
    /// scales linearly to `food_decline_max_mortality` (default 3%/yr)
    /// at ratio=0. The earlier threshold + steeper curve + higher
    /// max mortality reflect real-world famine data (IPC levels,
    /// Ó Gráda 2009, Sen 1981): IPC level 2 (Stressed) at 0.95,
    /// level 3 (Crisis) at 0.85, level 4 (Emergency) at 0.7, level 5
    /// (Famine) at 0.5. The player should never see <0.5 because
    /// the game gives clear feedback from 0.95 downward.
    ///
    /// # Arguments
    /// * `food_ratio` - Food production / consumption ratio. Pass
    ///   `1.0` for "fully fed" and `< 1.0` for deficit.
    /// * `data` - The colony-tuning parameters and building definitions.
    pub fn population_growth_per_year(
        &self,
        food_ratio: f64,
        data: &super::data::BuildingsData,
    ) -> f64 {
        if self.population <= 0.0 {
            return 0.0;
        }

        let housing = self.housing_capacity(data);
        if housing <= 0.0 {
            return 0.0;
        }

        let cc = &data.colony_constants;

        // Population-growth bonus: sum every building's `PopulationGrowth`
        // modifier (PER BUILD) across the colony, then clamp to the
        // `max_medical_growth_bonus` ceiling. The clamp preserves the
        // MedicalCenter-led design (`medical_growth_per_center = 0.0003`
        // × N centers, capped at `0.009`) while also honouring the
        // parse-but-until-now-unused PopGrowth modifiers on
        // PharmaceuticalPlant / WaterTreatmentPlant / DesalinationPlant
        // (GRA-22c building-economy audit Phase 1). Earth-start still
        // saturates at the cap (200 MedicalCenters >> 30 needed) so
        // the migration is a no-op for existing saves and the
        // upcoming outpost will now get the bonus the card advertised.
        let raw_growth_bonus: f64 = self
            .buildings
            .iter()
            .map(|(bt, count)| {
                let per_build = data.population_growth_for(*bt);
                per_build * *count as f64
            })
            .sum();
        let medical_bonus = raw_growth_bonus.min(cc.max_medical_growth_bonus);

        // Housing utilisation factor – growth slows as housing fills
        let utilisation = (self.population / housing).min(1.0);
        let housing_factor = 1.0 - utilisation * cc.housing_utilization_penalty;

        // Logistics efficiency penalty
        let logistics = self.logistics_efficiency(data);

        // v3.7.1 (steeper) food-driven growth.
        // Growth factor: power-1.5 curve so the player feels
        // pressure EARLY (at 0.95 ratio the factor is already 0.85)
        // and the curve steepens sharply as famine approaches.
        let food_ratio_clamped = food_ratio.max(0.0);
        let food_growth_factor = if food_ratio_clamped >= 1.0 {
            1.0
        } else if food_ratio_clamped >= 0.5 {
            // (2 * ratio - 1)^1.5
            let t = 2.0 * food_ratio_clamped - 1.0;
            (t * t * t.sqrt()).min(1.0)
        } else {
            0.0
        };
        // Mortality: linear from 0 at threshold (default 0.95) to
        // max_mortality (default 3%/yr) at ratio=0. Threshold is
        // high so the player gets feedback from any deficit, not
        // just severe famine.
        let mortality_rate = if food_ratio_clamped < cc.food_decline_threshold {
            let deficit_frac =
                (cc.food_decline_threshold - food_ratio_clamped) / cc.food_decline_threshold;
            cc.food_decline_max_mortality * deficit_frac
        } else {
            0.0
        };

        let gross_growth_rate = (cc.base_growth_rate + medical_bonus)
            * food_growth_factor
            * housing_factor
            * logistics
            * self.growth_rate_modifier;

        let net_growth_rate = gross_growth_rate - mortality_rate;

        self.population * net_growth_rate
    }

    /// Calculate mining output multiplier (affected by logistics)
    pub fn mining_output_multiplier(&self, data: &super::data::BuildingsData) -> f64 {
        self.logistics_efficiency(data)
    }

    /// Calculate research output multiplier (affected by logistics)
    pub fn research_output_multiplier(&self, data: &super::data::BuildingsData) -> f64 {
        // Research is less affected by logistics than mining (minimum 50%)
        let efficiency = self.logistics_efficiency(data);
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
    /// v3.6: the working-age fraction is read from
    /// `colony_constants.available_workforce_fraction` instead of
    /// being hard-coded at 0.4. (v3.5 default = 0.4 = 40% of
    /// population is working-age and willing to work.)
    pub fn available_workforce(&self, data: &super::data::BuildingsData) -> u32 {
        (self.population * data.colony_constants.available_workforce_fraction) as u32
    }

    /// Workforce efficiency factor (0.0 to 1.0).
    ///
    /// When available workers >= demand, all buildings operate at full efficiency.
    /// When understaffed, output scales proportionally.
    /// A colony with zero demand has 1.0 efficiency.
    pub fn workforce_efficiency(&self, data: &super::data::BuildingsData) -> f64 {
        let demand = self.total_workforce_demand() as f64;
        if demand <= 0.0 {
            return 1.0;
        }
        let available = self.available_workforce(data) as f64;
        (available / demand).min(1.0)
    }

    /// Wealth generated per year by financial/commercial buildings.
    ///
    /// v3.6: per-build `WealthGeneration` values are read from the RON
    /// `WealthGeneration` modifier on each building. Hard-coded values
    /// removed.
    /// - CommercialHub:    500 MC/yr  (local economy)
    /// - FinancialCenter: 2,000 MC/yr  (investment returns)
    /// - TradePort:       5,000 MC/yr  (interplanetary trade)
    /// - Factory:           100 MC/yr  (manufactured-goods revenue)
    ///
    /// Scaled by workforce efficiency (understaffed buildings produce less).
    pub fn wealth_generation_per_year(&self, data: &super::data::BuildingsData) -> f64 {
        let sum: f64 = [
            BuildingType::CommercialHub,
            BuildingType::FinancialCenter,
            BuildingType::TradePort,
            BuildingType::Factory,
        ]
        .iter()
        .map(|bt| self.building_count(*bt) as f64 * data.wealth_generation_for(*bt))
        .sum();
        sum * self.workforce_efficiency(data)
    }

    /// Operating cost per year for all buildings.
    ///
    /// v3.6: the per-year cost fraction (5%) is read from
    /// `colony_constants.operating_cost_fraction` instead of being
    /// hard-coded.
    ///
    /// v3.10 (GRA-22c Phase 4C): when a building has an explicit
    /// `money_cost_mc_per_year` (in the RON data file), use that
    /// directly. Otherwise fall back to the legacy
    /// `build_cost() × operating_cost_fraction` formula. The new
    /// path is the user-facing one — the cost is no longer
    /// derived from the build cost in BP, but from a real MC
    /// estimate (staff payroll + capital + maintenance
    /// contracts). Power plants, financial centres, and shipyards
    /// pay the most; mines and farms pay the least.
    pub fn operating_cost_per_year(&self, data: &super::data::BuildingsData) -> f64 {
        let rate = data.colony_constants.operating_cost_fraction;
        self.buildings
            .iter()
            .map(|(bt, count)| {
                let per_build = data
                    .get(bt)
                    .map(|d| d.money_cost_mc_per_year)
                    .unwrap_or(0.0);
                let cost = if per_build > 0.0 {
                    per_build
                } else {
                    bt.build_cost() * rate
                };
                cost * (*count as f64)
            })
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
#[derive(Component, Debug, Clone, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
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
#[derive(Resource, Debug, Clone, Default, Reflect)]
#[reflect(Resource)]
pub struct PendingConstructionActions {
    /// (colony_entity, building_type) pairs to start constructing
    pub start_construction: Vec<(Entity, BuildingType)>,
    /// v0.5.2: (colony_entity, building_type, delta) tuples for the
    /// Mining tab's direct inventory edits (positive = add, negative
    /// = remove).  Processed in the same system as `start_construction`
    /// but applied immediately (no BP / build-time), so the UI
    /// shows the new count on the next frame.
    pub mining_edits: Vec<(Entity, BuildingType, i32)>,
    /// Construction project entities to cancel
    pub cancel_construction: Vec<Entity>,
    /// Requests to establish a new outpost colony on a body.
    pub establish_outpost: Vec<EstablishOutpostRequest>,
}

/// Parameters carried from the UI when the player clicks "Establish Outpost".
#[derive(Debug, Clone, Reflect)]
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
#[derive(Component, Debug, Clone, Reflect)]
#[reflect(Component)]
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
    use crate::colony::data::BuildingsData;

    /// v3.6: every test in this module calls a Colony method that takes
    /// `&BuildingsData`. Loading the RON file once keeps the tests in
    /// sync with the actual per-build values (Farm=360, HabitatDome=50M,
    /// etc.) and surfaces any RON-data regressions as a test failure.
    fn data() -> BuildingsData {
        BuildingsData::load_for_tests()
    }

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
        colony.add_building(BuildingType::IronMine);
        colony.add_building(BuildingType::IronMine);
        colony.add_building(BuildingType::Factory);

        assert_eq!(colony.building_count(BuildingType::IronMine), 2);
        assert_eq!(colony.building_count(BuildingType::Factory), 1);
        assert_eq!(colony.building_count(BuildingType::CopperMine), 0);
        assert_eq!(colony.total_buildings(), 3);
    }

    #[test]
    fn test_logistics_capacity() {
        let mut colony = Colony::new("Test".to_string(), 100.0);
        assert_eq!(colony.logistics_capacity(&data()), 0.0);

        colony.add_building(BuildingType::MassDriver);
        assert_eq!(colony.logistics_capacity(&data()), 5_000.0);

        colony.add_building(BuildingType::OrbitalLift);
        assert_eq!(colony.logistics_capacity(&data()), 25_000.0);

        colony.add_building(BuildingType::CargoTerminal);
        assert_eq!(colony.logistics_capacity(&data()), 27_000.0);
    }

    #[test]
    fn test_logistics_demand() {
        let mut colony = Colony::new("Test".to_string(), 100_000.0);
        // No industrial buildings → zero demand
        assert_eq!(colony.logistics_demand(), 0.0);

        colony.add_building(BuildingType::IronMine);
        // 1 mine × 1000 = 1000
        assert!((colony.logistics_demand() - 1_000.0).abs() < 0.001);
    }

    #[test]
    fn test_logistics_efficiency_no_demand() {
        let colony = Colony::new("Test".to_string(), 0.0);
        assert_eq!(colony.logistics_efficiency(&data()), 1.0);
    }

    #[test]
    fn test_logistics_efficiency_sufficient() {
        let mut colony = Colony::new("Test".to_string(), 1_000_000.0);
        colony.add_building(BuildingType::IronMine); // demand: 1000
        colony.add_building(BuildingType::MassDriver); // capacity: 5000
                                                       // 5000 / 1000 > 1.0 → clamped to 1.0
        assert_eq!(colony.logistics_efficiency(&data()), 1.0);
    }

    #[test]
    fn test_logistics_efficiency_insufficient() {
        let mut colony = Colony::new("Test".to_string(), 10_000_000.0);
        // Add many mines without logistics
        for _ in 0..10 {
            colony.add_building(BuildingType::IronMine);
        }
        // demand: 10*1000 = 10000, capacity: 0
        assert_eq!(colony.logistics_efficiency(&data()), 0.0);
    }

    #[test]
    fn test_housing_capacity() {
        // v3.10 (GRA-22c Phase 4A): HabitatDome 50M → 5M,
        // UndergroundHabitat 30M → 8M. Asserts updated.
        let mut colony = Colony::new("Test".to_string(), 100.0);
        assert_eq!(colony.housing_capacity(&data()), 0.0);

        colony.add_building(BuildingType::HabitatDome);
        assert_eq!(colony.housing_capacity(&data()), 5_000_000.0);

        colony.add_building(BuildingType::UndergroundHabitat);
        assert_eq!(colony.housing_capacity(&data()), 13_000_000.0);
    }

    #[test]
    fn test_population_growth_no_housing() {
        let colony = Colony::new("Test".to_string(), 1000.0);
        assert_eq!(colony.population_growth_per_year(1.0, &data()), 0.0);
    }

    #[test]
    fn test_population_growth_with_housing() {
        let mut colony = Colony::new("Test".to_string(), 100_000.0);
        colony.add_building(BuildingType::HabitatDome); // 50,000,000 capacity
        colony.add_building(BuildingType::AgriDome); // food for ~40K people

        let growth = colony.population_growth_per_year(1.0, &data());
        // Should be positive with housing and food
        assert!(growth > 0.0, "Growth should be positive: {}", growth);
    }

    // v3.9 (GRA-22c Phase 1.2): regression guards for the generic
    // PopulationGrowth accumulation. The pre-v3.9 hard-code only
    // counted `MedicalCenter` × `medical_growth_per_center`; now
    // every building with a `PopulationGrowth` modifier contributes,
    // clamped at `max_medical_growth_bonus`. These tests pin both
    // the per-building contribution and the cap behaviour so future
    // edits to `Colony::population_growth_per_year` cannot silently
    // re-break the wiring.
    #[test]
    fn test_population_growth_bonus_sums_all_contributors() {
        let data = data();
        // Spot-check the live RON values the test relies on.
        let per_pharma = data.population_growth_for(BuildingType::PharmaceuticalPlant);
        let per_water = data.population_growth_for(BuildingType::WaterTreatmentPlant);
        let per_desal = data.population_growth_for(BuildingType::DesalinationPlant);
        let per_medical = data.population_growth_for(BuildingType::MedicalCenter);
        assert!(
            per_pharma > 0.0 && per_water > 0.0 && per_desal > 0.0 && per_medical > 0.0,
            "Each PopGrowth modifier must be wired (got pharma={pharma}, water={water}, desal={desal}, medical={medical})",
            pharma = per_pharma,
            water = per_water,
            desal = per_desal,
            medical = per_medical,
        );
    }

    #[test]
    fn test_population_growth_bonus_cap_holds_across_categories() {
        // With housing, the bonus comes from the colony's buildings.
        // 200 MedicalCenters saturate the cap; the bonus must NOT
        // scale linearly beyond it.
        let mut colony = Colony::new("Cap".to_string(), 200_000.0);
        colony.add_building(BuildingType::HabitatDome);
        colony.add_building(BuildingType::AgriDome);
        for _ in 0..200 {
            colony.add_building(BuildingType::MedicalCenter);
        }
        // Compare to a baseline with just 30 centers (also saturates cap).
        let mut baseline = Colony::new("Baseline".to_string(), 200_000.0);
        baseline.add_building(BuildingType::HabitatDome);
        baseline.add_building(BuildingType::AgriDome);
        for _ in 0..30 {
            baseline.add_building(BuildingType::MedicalCenter);
        }

        let data = data();
        let g_cap = colony.population_growth_per_year(1.0, &data);
        let g_base = baseline.population_growth_per_year(1.0, &data);
        assert!(
            (g_cap - g_base).abs() < 1e-6,
            "200 MedicalCenters must clamp to the same cap as 30: \
             200-colony grew {g_cap} vs 30-colony grew {g_base}"
        );
    }

    /// Phase 1.5 (GRA-22c): the construction UI gates the queue
    /// button on `raw_growth_bonus >= max_medical_growth_bonus`. To
    /// keep that gate honest, this test exercises the same arithmetic
    /// the UI uses — `Colony::buildings` summed by RON
    /// `PopulationGrowth` per-build values vs. `BuildingsData::colony_constants.max_medical_growth_bonus`.
    /// The CTA should be disabled when the *current* raw sum is at
    /// or above the cap (a new building would be pure waste).
    #[test]
    fn test_population_growth_cap_gate_predicate() {
        let data = data();
        let cap = data.colony_constants.max_medical_growth_bonus;
        assert!(cap > 0.0, "cap must be positive");

        // Build a colony with 30 MedicalCenters (1 × 0.0003 × 30 =
        // 0.009, exactly at the cap). Adding one more would push the
        // raw sum to 0.012 and the clamped bonus would stay at 0.009
        // → marginal benefit = 0.
        let mut c = Colony::new("Saturated".to_string(), 100.0);
        let centers_for_cap =
            (cap / data.population_growth_for(BuildingType::MedicalCenter)).ceil() as i32;
        for _ in 0..centers_for_cap {
            c.add_building(BuildingType::MedicalCenter);
        }
        let raw_now: f64 = c
            .buildings
            .iter()
            .map(|(bt, n)| data.population_growth_for(*bt) * *n as f64)
            .sum();
        assert!(
            raw_now >= cap,
            "After {centers_for_cap} MedicalCenters raw={raw_now} should be >= cap={cap}"
        );

        // Same colony with 1 fewer MedicalCenter — raw sum should be
        // strictly below the cap, so the gate is NOT active.
        let mut under = Colony::new("Under".to_string(), 100.0);
        for _ in 0..(centers_for_cap - 1).max(0) {
            under.add_building(BuildingType::MedicalCenter);
        }
        let raw_under: f64 = under
            .buildings
            .iter()
            .map(|(bt, n)| data.population_growth_for(*bt) * *n as f64)
            .sum();
        assert!(
            raw_under < cap,
            "raw_under={raw_under} should be < cap={cap}"
        );
    }

    #[test]
    fn test_mining_output_multiplier() {
        let mut colony = Colony::new("Test".to_string(), 1_000_000.0);
        colony.add_building(BuildingType::IronMine);
        colony.add_building(BuildingType::MassDriver);

        let multiplier = colony.mining_output_multiplier(&data());
        assert!(multiplier > 0.0 && multiplier <= 1.0);
    }

    #[test]
    fn test_research_output_multiplier_minimum() {
        let mut colony = Colony::new("Test".to_string(), 10_000_000.0);
        for _ in 0..10 {
            colony.add_building(BuildingType::IronMine);
        }
        // No logistics → efficiency = 0 → research multiplier = 0.5 (minimum)
        assert!((colony.research_output_multiplier(&data()) - 0.5).abs() < 0.001);
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
        let project = ConstructionProject::new(BuildingType::IronMine, entity);

        assert_eq!(project.building_type, BuildingType::IronMine);
        assert_eq!(project.progress, 0.0);
        assert_eq!(project.required, BuildingType::IronMine.build_cost());
        assert!(!project.is_complete());
        assert_eq!(project.progress_percent(), 0.0);
    }

    #[test]
    fn test_construction_project_completion() {
        let entity = Entity::from_raw_u32(1).unwrap();
        let mut project = ConstructionProject::new(BuildingType::IronMine, entity);

        project.progress = project.required;
        assert!(project.is_complete());
        assert_eq!(project.progress_percent(), 1.0);
    }

    #[test]
    fn test_workforce_demand() {
        let mut colony = Colony::new("Test".to_string(), 10_000_000.0);
        assert_eq!(colony.total_workforce_demand(), 0);

        colony.add_building(BuildingType::IronMine); // 5,000 workers
        assert_eq!(colony.total_workforce_demand(), 5_000);

        colony.add_building(BuildingType::Factory); // 12,000 workers
        assert_eq!(colony.total_workforce_demand(), 17_000);
    }

    #[test]
    fn test_workforce_efficiency() {
        // Large population, few buildings → full efficiency
        let mut colony = Colony::new("Test".to_string(), 10_000_000.0);
        colony.add_building(BuildingType::IronMine);
        assert_eq!(colony.workforce_efficiency(&data()), 1.0);

        // Small population, many buildings → understaffed
        let mut colony2 = Colony::new("Test".to_string(), 10_000.0);
        colony2.add_building(BuildingType::Factory); // needs 12,000 workers, has 4,000
        assert!(colony2.workforce_efficiency(&data()) < 1.0);
    }

    #[test]
    fn test_workforce_efficiency_no_buildings() {
        let colony = Colony::new("Test".to_string(), 1000.0);
        assert_eq!(colony.workforce_efficiency(&data()), 1.0);
    }

    #[test]
    fn test_wealth_generation() {
        let mut colony = Colony::new("Test".to_string(), 10_000_000.0);
        assert_eq!(colony.wealth_generation_per_year(&data()), 0.0);

        colony.add_building(BuildingType::CommercialHub); // 500 MC/year
        assert!(colony.wealth_generation_per_year(&data()) > 0.0);

        colony.add_building(BuildingType::FinancialCenter); // 2,000 MC/year
        let wealth = colony.wealth_generation_per_year(&data());
        assert!(wealth > 500.0, "Should have substantial wealth: {}", wealth);
    }

    #[test]
    fn test_operating_cost() {
        let mut colony = Colony::new("Test".to_string(), 1_000_000.0);
        assert_eq!(colony.operating_cost_per_year(&data()), 0.0);

        // v0.5.2: IronMine build cost is 1500 BP, so 5%/yr = 75 MC/yr.
        colony.add_building(BuildingType::IronMine);
        assert!((colony.operating_cost_per_year(&data()) - 75.0).abs() < 0.001);
    }

    // ── ColonyDevelopment / yield-multiplier helpers ───────────────────

    #[test]
    fn test_colony_founding_starts_at_outpost() {
        // Per GRA-22 §4.5: every colony created via the founding path
        // (`Colony::new`) is an Outpost at × 0.10.  This is what the
        // "Establish Outpost" UI action spawns and what test fixtures get
        // by default.
        let colony = Colony::new("Luna".to_string(), 5_000.0);
        assert_eq!(colony.development.tier, ColonyTier::Outpost);
        assert!((colony.effective_yield_multiplier() - OUTPOST_YIELD_MULTIPLIER).abs() < 1e-9);
        assert!((colony.effective_yield_multiplier() - 0.10).abs() < 1e-9);
        assert_eq!(colony.development.investments, 0);
    }

    #[test]
    fn test_civilisation_colony_full_yield() {
        // Per GRA-22 §4.5: a Civilisation-tier colony operates at × 1.00.
        // This is the homeworld initial state and the "Earth × 1.0 yield
        // produces same totals as today" regression baseline.
        let earth = Colony::new_civilisation("Earth".to_string(), 8.2e9);
        assert_eq!(earth.development.tier, ColonyTier::Civilisation);
        assert!((earth.effective_yield_multiplier() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_settlement_and_city_intermediate_tiers() {
        let settlement = Colony::new_with_tier(
            "Mars".to_string(),
            1_000_000.0,
            ColonyTier::Settlement,
            SETTLEMENT_YIELD_MULTIPLIER,
        );
        assert_eq!(settlement.development.tier, ColonyTier::Settlement);
        assert!((settlement.effective_yield_multiplier() - 0.40).abs() < 1e-9);

        let city = Colony::new_with_tier(
            "Mars2".to_string(),
            5_000_000.0,
            ColonyTier::City,
            CITY_YIELD_MULTIPLIER,
        );
        assert_eq!(city.development.tier, ColonyTier::City);
        assert!((city.effective_yield_multiplier() - 0.70).abs() < 1e-9);
    }

    #[test]
    fn test_outpost_yield_is_ten_percent_of_civilisation() {
        // Binding acceptance criterion: a new colony's effective yield is
        // 10× less than a civilisation's, on every output and on
        // maintenance.  Asserted at the helper level — systems multiply
        // the helper output by `effective_yield_multiplier()` at the call
        // site, so the 10× relationship flows through unchanged.
        //
        // Both colonies share the same population so that workforce-driven
        // multipliers (e.g. `workforce_efficiency` in wealth generation)
        // cancel out and the yield multiplier is the *only* multiplier
        // under test.
        let pop = 1_000_000.0_f64;
        let mut outpost = Colony::new_with_tier(
            "Moon".to_string(),
            pop,
            ColonyTier::Outpost,
            OUTPOST_YIELD_MULTIPLIER,
        );
        let mut earth = Colony::new_with_tier(
            "Earth-test".to_string(),
            pop,
            ColonyTier::Civilisation,
            CIVILISATION_YIELD_MULTIPLIER,
        );

        // Same single building, same base helper output.
        outpost.add_building(BuildingType::Farm);
        earth.add_building(BuildingType::Farm);

        let outpost_food =
            outpost.food_production_per_year(&data()) * outpost.effective_yield_multiplier();
        let earth_food =
            earth.food_production_per_year(&data()) * earth.effective_yield_multiplier();
        assert!(
            (earth_food / outpost_food - 10.0).abs() < 1e-6,
            "Earth food / Outpost food should be 10×, got {:.4}×",
            earth_food / outpost_food,
        );

        // Wealth has the same relationship (same population, so the only
        // multiplier is the yield multiplier).
        outpost.add_building(BuildingType::CommercialHub);
        earth.add_building(BuildingType::CommercialHub);
        let outpost_wealth =
            outpost.wealth_generation_per_year(&data()) * outpost.effective_yield_multiplier();
        let earth_wealth =
            earth.wealth_generation_per_year(&data()) * earth.effective_yield_multiplier();
        assert!(
            (earth_wealth / outpost_wealth - 10.0).abs() < 1e-6,
            "Earth wealth / Outpost wealth should be 10×, got {:.4}×",
            earth_wealth / outpost_wealth,
        );

        // Operating cost has the same relationship.  The IronMine build
        // cost is 1500 BP (v0.5.2), so 5%/yr = 75 MC/yr base.
        // Earth: 75 × 1.0; Outpost: 75 × 0.10.
        outpost.add_building(BuildingType::IronMine);
        earth.add_building(BuildingType::IronMine);
        let outpost_op =
            outpost.operating_cost_per_year(&data()) * outpost.effective_yield_multiplier();
        let earth_op = earth.operating_cost_per_year(&data()) * earth.effective_yield_multiplier();
        assert!(
            (earth_op / outpost_op - 10.0).abs() < 1e-6,
            "Earth op-cost / Outpost op-cost should be 10×, got {:.4}×",
            earth_op / outpost_op,
        );
    }

    #[test]
    fn test_earth_yield_one_unchanged() {
        // Regression check (per GRA-22 §8 / GRA-24 acceptance criteria):
        // a Civilisation-tier colony must produce the *same* base helper
        // totals as today's calibration.  The yield multiplier is
        // multiplied at the system call site, not inside the helper, so
        // the helper itself stays at × 1.00.
        //
        // v0.5.1: Farm per-build was 1,000 Mt/yr, calibrated against the
        // 0.0001 Mt/p/yr per-capita (which was 91× over real-world 1,100 kg
        // = 0.0000011 Mt). v0.5.1 corrects both: per-capita → 0.0000011,
        // per-build → 360. 25 Farms × 360 = 9,000 Mt/yr ≈ 8.2B ×
        // 0.0000011 = 9,020 Mt/yr (FAO 2024 SOFA, balanced).
        // Reference: docs/design/BALANCE_PATCHES_v0.5.md §4.1 + §8.1.
        let mut earth = Colony::new_civilisation("Earth".to_string(), 8.2e9);
        earth.add_building(BuildingType::Farm); // 360 Mt/yr at × 1.00 (v0.5.1)
        assert!(
            (earth.food_production_per_year(&data()) - 360.0).abs() < 0.001,
            "Earth × 1.0 must keep today's Farm output (360 Mt/yr in v0.5.1)",
        );
        assert!(
            (earth.effective_yield_multiplier() - 1.0).abs() < 1e-9,
            "Earth's effective yield must be 1.00",
        );
    }

    // ============================================================
    // v3.8: per-resource annual-consumption helper (used by
    // economy::mining::throttle_production as the "consumption
    // floor" at cap).
    // ============================================================

    #[test]
    fn test_annual_resource_consumption_zero_for_empty_colony() {
        // No population, no buildings → no draw.
        let colony = Colony::new("Luna".to_string(), 0.0);
        let draw = colony.annual_resource_consumption(crate::economy::ResourceType::Iron, &data());
        assert_eq!(draw, 0.0);
    }

    #[test]
    fn test_annual_resource_consumption_per_capita_scales_with_pop() {
        // 8.2B people × iron_mt_per_year (default 0.000000213 Mt/p/yr
        // = 213 kg/p/yr per worldsteel 2024 finished-steel demand) =
        // 1,747 Mt/yr. Sanity check: ~70% of USGS 2024 world iron-ore
        // demand (~2,500 Mt/yr finished-steel equivalent).
        let colony = Colony::new_civilisation("Earth".to_string(), 8.2e9);
        let draw = colony.annual_resource_consumption(crate::economy::ResourceType::Iron, &data());
        // 0.000000213 Mt/p/yr × 8.2e9 = 1,746.6 Mt/yr
        assert!(
            (draw - 1_746.6).abs() < 5.0,
            "8.2B × 213 kg/p/yr should be ~1,747 Mt/yr, got {draw}",
        );
    }

    #[test]
    fn test_annual_resource_consumption_zero_for_unmet_resource() {
        // Tungsten is NOT in the per-capita block, so population
        // contributes 0. (Industrial maintenance is the only path
        // for non-per-capita resources.)
        let colony = Colony::new_civilisation("Earth".to_string(), 8.2e9);
        let draw =
            colony.annual_resource_consumption(crate::economy::ResourceType::Tungsten, &data());
        assert_eq!(draw, 0.0);
    }
}
