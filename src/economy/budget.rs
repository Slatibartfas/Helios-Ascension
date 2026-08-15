use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::types::ResourceType;
use crate::astronomy::components::{CurrentStarSystem, SystemId};
use crate::colony::{BuildingsData, Colony};
use crate::economy::{components::LocalStockpile, PowerGenerator, PowerSourceType};
use crate::plugins::camera::ViewMode;

/// Tracks per-month income/production rates for all resources
/// and research/engineering points for display in the resource bar.
///
/// Rates are stored as "amount per 30-day month" (2,592,000 seconds).
#[derive(Resource, Debug, Clone, Default, Reflect)]
#[reflect(Resource)]
pub struct ResourceRateTracker {
    /// Monthly production rate per resource type (Mt/month) — global total
    pub resource_rates: HashMap<ResourceType, f64>,
    /// Gross monthly production per resource type before consumption offsets (Mt/month)
    pub gross_production_rates: HashMap<ResourceType, f64>,
    /// Gross monthly consumption per resource type before production offsets (Mt/month)
    pub gross_consumption_rates: HashMap<ResourceType, f64>,
    /// Per-entity (body) monthly rates for resource tooltip breakdown
    pub per_entity_rates: HashMap<Entity, HashMap<ResourceType, f64>>,
    /// v3.8.11 (2026-08-07): per-resource per-capita draw
    /// (population × per-capita rate, in Mt/month). Used by the rate
    /// tooltip to break down "consumption" into population vs maintenance
    /// vs synthesis input. Prior to v3.8.11 this draw was only computed
    /// inside `deduct_population_consumption`, so the rate display missed
    /// the largest single component of consumer-resource draw.
    pub population_consumption: HashMap<ResourceType, f64>,
    /// v3.8.11: per-resource industrial-process input draw
    /// (e.g. Methane consumed by PolymerSynthesis, in Mt/month).
    pub synthesis_input: HashMap<ResourceType, f64>,
    /// Monthly research point generation
    pub research_rate_per_month: f64,
    /// Monthly engineering point generation
    pub engineering_rate_per_month: f64,
}

/// Seconds in one 30-day month (30 × 86400)
pub const SECONDS_PER_MONTH: f64 = 2_592_000.0;
/// Seconds in one year (365.25 × 86400)
pub const SECONDS_PER_YEAR: f64 = 31_557_600.0;

#[derive(Debug, Clone, Copy, Default, Reflect)]
pub struct ColonyPowerTotals {
    pub produced_watts: f64,
    pub consumed_watts: f64,
}

pub fn calculate_colony_power_totals(
    colony: &Colony,
    buildings_data: Option<&BuildingsData>,
) -> ColonyPowerTotals {
    let Some(data) = buildings_data else {
        let building_count: u32 = colony.buildings.values().sum();
        return ColonyPowerTotals {
            produced_watts: 0.0,
            consumed_watts: building_count as f64 * 400_000_000.0,
        };
    };

    let mut produced_watts = 0.0;
    let mut consumed_watts = 0.0;

    for (building_type, &count) in &colony.buildings {
        if count == 0 {
            continue;
        }

        let Some(def) = data.get(building_type) else {
            consumed_watts += count as f64 * 400_000_000.0;
            continue;
        };

        for modifier in &def.modifiers {
            if modifier.modifier_type == "PowerGeneration" {
                // RON `PowerGeneration` values are expressed in **GW per
                // unit** (matching the inline `// <value> GW` annotations
                // next to every entry in assets/data/buildings.ron and the
                // per-building display strings in src/colony/types.rs:
                // "+5 GW power output", "+20 GW power output", etc.).
                // The 1e9 factor converts GW → W, which lines up with the
                // EnergyGrid units (W) and the per-building
                // `power_demand_mw` field (converted the other way by
                // * 1e6 in the consumption loop below).
                produced_watts += modifier.value * count as f64 * 1_000_000_000.0;
            }
        }

        consumed_watts += def.power_demand_mw * count as f64 * 1_000_000.0;
    }

    ColonyPowerTotals {
        produced_watts,
        consumed_watts,
    }
}

impl ResourceRateTracker {
    /// Get the global monthly rate for a resource type
    pub fn get_resource_rate(&self, resource: &ResourceType) -> f64 {
        self.resource_rates.get(resource).copied().unwrap_or(0.0)
    }

    /// Get the gross monthly production rate for a resource type.
    pub fn get_resource_production_rate(&self, resource: &ResourceType) -> f64 {
        self.gross_production_rates
            .get(resource)
            .copied()
            .unwrap_or(0.0)
    }

    /// Get the gross monthly consumption rate for a resource type.
    pub fn get_resource_consumption_rate(&self, resource: &ResourceType) -> f64 {
        self.gross_consumption_rates
            .get(resource)
            .copied()
            .unwrap_or(0.0)
    }

    /// Get the monthly rate for a resource on a specific body
    pub fn get_entity_resource_rate(&self, entity: Entity, resource: &ResourceType) -> f64 {
        self.per_entity_rates
            .get(&entity)
            .and_then(|rates| rates.get(resource))
            .copied()
            .unwrap_or(0.0)
    }

    /// Get the total monthly rate for a category of resources
    pub fn get_category_rate(&self, resources: &[ResourceType]) -> f64 {
        resources.iter().map(|r| self.get_resource_rate(r)).sum()
    }

    /// v3.8.11: per-capita (population) draw for a resource, in Mt/month.
    /// Returns 0.0 if not tracked.
    pub fn get_population_consumption(&self, resource: &ResourceType) -> f64 {
        self.population_consumption
            .get(resource)
            .copied()
            .unwrap_or(0.0)
    }

    /// v3.8.11: industrial-process input draw for a resource, in Mt/month.
    /// Returns 0.0 if not tracked.
    pub fn get_synthesis_input(&self, resource: &ResourceType) -> f64 {
        self.synthesis_input.get(resource).copied().unwrap_or(0.0)
    }
}

/// Global economic budget and resource management
/// Tracks civilization-wide stockpiles and power generation
#[derive(Resource, Debug, Clone, Serialize, Deserialize, Reflect)]
#[reflect(Resource)]
pub struct GlobalBudget {
    /// Global stockpiles of each resource type (in arbitrary units)
    pub stockpiles: HashMap<ResourceType, f64>,

    /// Energy grid status
    pub energy_grid: EnergyGrid,

    /// Civilization score based on power generation
    pub civilization_score: f64,

    /// Breakdown of power production by source
    pub power_breakdown: HashMap<PowerSourceType, f64>,

    /// Treasury: total accumulated wealth (Mega-Credits, MC)
    pub treasury: f64,

    /// Income per year from all colonies (MC/year)
    pub income_per_year: f64,

    /// Expenses per year from all colonies (MC/year)
    pub expenses_per_year: f64,

    /// Storage multiplier applied on top of the base per-resource cap.
    ///
    /// Updated each frame by `update_storage_capacity` by summing
    /// `StorageCapacity` building modifiers across all colonies.
    /// Each Warehouse/Resource-Depot contributes +2.5%, so:
    ///   0 depots → 1.0×, 4 depots → 1.1×, 20 depots → 1.5×.
    pub storage_multiplier: f64,
}

impl GlobalBudget {
    /// Create a new global budget with starting resources
    pub fn new() -> Self {
        let mut stockpiles = HashMap::new();

        // v3.8.4 (2026-08-07): starting stockpiles are 50% of the
        // realistic 2026 active-storage cap (mid-cycle inventory).
        // Previously calibrated at 6 months of world demand, which
        // exceeded the new active-storage cap.  This matches the
        // player's real-world mental model: the cap *is* the
        // warehouse / port / tank, and at game start the
        // warehouses are half-full.  The per-capita + maintenance
        // demand drains them down to a steady-state below the cap.

        // Volatiles / gases (cap is industrial-tank / atmospheric storage)
        stockpiles.insert(ResourceType::Water, 300.0); // cap 600
        stockpiles.insert(ResourceType::Oxygen, 10.0); // cap 20
        stockpiles.insert(ResourceType::Hydrogen, 2.5); // cap 5
        stockpiles.insert(ResourceType::Methane, 25.0); // cap 50
        stockpiles.insert(ResourceType::Nitrogen, 15.0); // cap 30
        stockpiles.insert(ResourceType::Ammonia, 15.0); // cap 30

        // Construction / common metals (cap is LME / port / mine stockpile)
        stockpiles.insert(ResourceType::Iron, 50.0); // cap 100
        stockpiles.insert(ResourceType::Copper, 15.0); // cap 30
        stockpiles.insert(ResourceType::Aluminum, 5.0); // cap 10
        stockpiles.insert(ResourceType::Silicates, 2_500.0); // cap 5,000
        stockpiles.insert(ResourceType::Nickel, 0.5); // cap 1
        stockpiles.insert(ResourceType::Chromium, 2.5); // cap 5
        stockpiles.insert(ResourceType::Magnesium, 0.25); // cap 0.5
        stockpiles.insert(ResourceType::Cobalt, 0.025); // cap 0.05
        stockpiles.insert(ResourceType::Tungsten, 0.025); // cap 0.05
        stockpiles.insert(ResourceType::Titanium, 2.5); // cap 5

        // Non-metals / chemical industry
        stockpiles.insert(ResourceType::Carbon, 100.0); // cap 200
        stockpiles.insert(ResourceType::Phosphorus, 15.0); // cap 30
        stockpiles.insert(ResourceType::Sulfur, 15.0); // cap 30
        stockpiles.insert(ResourceType::Polymers, 25.0); // cap 50
        stockpiles.insert(ResourceType::Fluorine, 0.5); // cap 1

        // Strategic / rare
        stockpiles.insert(ResourceType::Lithium, 0.1); // cap 0.2
        stockpiles.insert(ResourceType::RareEarths, 0.05); // cap 0.1

        // Precious metals
        stockpiles.insert(ResourceType::Gold, 0.0005); // cap 0.001
        stockpiles.insert(ResourceType::Silver, 0.0025); // cap 0.005
        stockpiles.insert(ResourceType::Platinum, 0.00005); // cap 0.0001

        // Fissile / fusion fuels
        stockpiles.insert(ResourceType::Uranium, 0.025); // cap 0.05
        stockpiles.insert(ResourceType::Thorium, 0.0025); // cap 0.005
        stockpiles.insert(ResourceType::Plutonium, 0.0); // manufactured in breeder reactors
        stockpiles.insert(ResourceType::Deuterium, 0.05); // cap 0.1
        stockpiles.insert(ResourceType::Tritium, 0.0); // bred from lithium blankets
        stockpiles.insert(ResourceType::Helium3, 0.0); // essentially none yet

        // Food — 50% of cap (FAO 2024 grain reserves ~800 Mt)
        stockpiles.insert(ResourceType::Food, 400.0); // cap 800

        Self {
            stockpiles,
            energy_grid: EnergyGrid::default(),
            civilization_score: 0.0,
            power_breakdown: HashMap::new(),
            treasury: 1_000_000.0, // Starting treasury: 1M MC (global industrial base)
            income_per_year: 0.0,
            expenses_per_year: 0.0,
            storage_multiplier: 1.0,
        }
    }

    /// Get the stockpile amount for a specific resource
    pub fn get_stockpile(&self, resource: &ResourceType) -> f64 {
        self.stockpiles.get(resource).copied().unwrap_or(0.0)
    }

    /// Add resources to the stockpile
    ///
    /// # Arguments
    /// * `resource` - The type of resource to add
    /// * `amount` - The amount to add (must be non-negative)
    ///
    /// # Panics
    /// Panics if amount is negative
    pub fn add_resource(&mut self, resource: ResourceType, amount: f64) {
        assert!(
            amount >= 0.0,
            "Cannot add negative resource amount: {}",
            amount
        );
        let current = self.get_stockpile(&resource);
        self.stockpiles.insert(resource, current + amount);
    }

    /// Remove resources from the stockpile (returns true if successful)
    ///
    /// # Arguments
    /// * `resource` - The type of resource to consume
    /// * `amount` - The amount to consume (must be non-negative)
    ///
    /// # Returns
    /// `true` if the resource was successfully consumed, `false` if insufficient stockpile
    ///
    /// # Panics
    /// Panics if amount is negative
    pub fn consume_resource(&mut self, resource: ResourceType, amount: f64) -> bool {
        assert!(
            amount >= 0.0,
            "Cannot consume negative resource amount: {}",
            amount
        );
        let current = self.get_stockpile(&resource);
        if current >= amount {
            self.stockpiles.insert(resource, current - amount);
            true
        } else {
            false
        }
    }

    /// Add resources to the stockpile, capped at the per-resource limit.
    ///
    /// Production stops silently when the stockpile is full.  Use this instead
    /// of `add_resource` for all ongoing production (mining, food, atmospheric
    /// harvesting) so that stockpiles have finite capacity.
    ///
    /// The effective cap = `stockpile_cap(resource) * self.storage_multiplier`.
    ///
    /// Returns the amount actually added (may be less than `amount` if near cap).
    pub fn add_resource_capped(&mut self, resource: ResourceType, amount: f64) -> f64 {
        assert!(
            amount >= 0.0,
            "Cannot add negative resource amount {} to {:?}",
            amount,
            resource
        );
        let cap = self.effective_stockpile_cap(resource);
        let current = self.get_stockpile(&resource);
        let headroom = (cap - current).max(0.0);
        let added = amount.min(headroom);
        if added > 0.0 {
            self.stockpiles.insert(resource, current + added);
        }
        added
    }

    /// Maximum stockpile capacity for a given resource *before* the storage
    /// multiplier is applied (in Megatons).
    ///
    /// v3.8.4 (2026-08-07): values are **2026 active storage estimates**
    /// (warehouses, ports, strategic reserves, LME-bonded stocks) — not
    /// 1 year of world demand.  Stockpile = "actual storage facilities",
    /// not "stuff lying in the ground waiting to be mined".  Earth as a
    /// single body holds the global 2026 stock; the per-body cap is
    /// scaled by the storage multiplier for Warehouse/Depot buildings.
    ///
    /// Citations (rough active-storage estimates, 2024-2026):
    /// - Food: FAO 2024 grain reserves ≈ 800 Mt (~30% annual consumption)
    /// - Iron: USGS 2024 iron-ore stockpiles + LME bonded ≈ 100 Mt
    /// - Silicates: port stocks + construction-aggregate terminals ≈ 5 Gt
    /// - Carbon (coal): strategic reserves + port stocks ≈ 200 Mt
    /// - Methane: gas-storage facilities (US/EU/Russia) ≈ 50 Mt
    /// - Water: industrial reservoir / desalinated buffer ≈ 600 Mt
    /// - Polymers: plastics-resin warehouses ≈ 50 Mt
    /// - Copper: LME + bonded warehouses ≈ 30 Mt
    /// - Aluminum, Nickel, etc: LME-bonded stocks
    /// - Atmospheric gases: industrial gas-storage tanks
    ///   (the atmosphere itself is the geological deposit; the
    ///   storage cap reflects *tank* capacity, not air volume)
    ///
    /// Use `effective_stockpile_cap()` for the actual enforced cap which
    /// includes the `storage_multiplier` bonus from Warehouse / Resource
    /// Depot buildings.
    pub fn stockpile_cap(resource: ResourceType) -> f64 {
        match resource {
            // Construction materials — LME-bonded + port + strategic
            ResourceType::Iron => 100.0, // 100 Mt iron-ore stockpiles
            ResourceType::Silicates => 5_000.0, // 5 Gt construction aggregate
            ResourceType::Copper => 30.0, // 30 Mt LME-bonded Cu
            ResourceType::Aluminum => 10.0, // 10 Mt LME Al
            ResourceType::Titanium => 5.0, // 5 Mt Ti mineral stockpiles
            ResourceType::Nickel => 1.0, // 1 Mt LME Ni
            ResourceType::Tungsten => 0.05, // 50 kt W APT stockpiles
            ResourceType::Chromium => 5.0, // 5 Mt chromite stockpiles
            ResourceType::Magnesium => 0.5, // 500 kt Mg
            ResourceType::Cobalt => 0.05, // 50 kt Co
            // Industrial / energy
            ResourceType::Carbon => 200.0,    // 200 Mt coal stockpiles
            ResourceType::Methane => 50.0,    // 50 Mt gas-storage facilities
            ResourceType::Water => 600.0,     // 600 Mt industrial reservoir
            ResourceType::Sulfur => 30.0,     // 30 Mt elemental S stockpiles
            ResourceType::Phosphorus => 30.0, // 30 Mt phosphate rock
            ResourceType::Polymers => 50.0,   // 50 Mt plastics-resin tanks
            ResourceType::Fluorine => 1.0,    // 1 Mt fluorspar
            ResourceType::RareEarths => 0.1,  // 100 kt REO stockpiles
            ResourceType::Lithium => 0.2,     // 200 kt Li carbonate
            // Precious metals — small tank / vault storage
            ResourceType::Gold => 0.001,   // 1,000 t central-bank+LBMA
            ResourceType::Silver => 0.005, // 5,000 t silver stockpiles
            ResourceType::Platinum => 0.0001, // 100 t Pt stockpiles
            // Atmospheric / industrial gases — tank storage
            ResourceType::Nitrogen => 30.0, // 30 Mt N₂ tank storage
            ResourceType::Oxygen => 20.0,   // 20 Mt O₂ tank storage
            ResourceType::Argon => 1.0,     // 1 Mt Ar tank storage
            ResourceType::Hydrogen => 5.0,  // 5 Mt H₂ tank storage
            ResourceType::Ammonia => 30.0,  // 30 Mt NH₃ refrigerated tanks
            // v3.8.16: CarbonDioxide previously fell through to the
            // `_ => f64::MAX` catch-all, so its stockpile grew forever
            // (production 153 Mt/yr vs 150 cons, no throttle) and the
            // Atmospheric Gases / Volatiles row showed an always-empty
            // fill bar — "seemingly infinite storage". Industrial CO₂
            // tank storage (beverage carbonation, dry ice, urea
            // feedstock, EOR) is a few months of the game's 153 Mt/yr
            // throughput: 50 Mt ≈ 0.33 yr, consistent with Nitrogen
            // (30 Mt / 117 Mt/yr = 0.26 yr).
            ResourceType::CarbonDioxide => 50.0, // 50 Mt CO₂ tank storage
            // Fissile / fusion
            ResourceType::Uranium => 0.05,    // 50 kt U₃O₈
            ResourceType::Thorium => 0.005,   // 5 kt Th
            ResourceType::Plutonium => 0.001, // 1 t Pu stockpiles
            ResourceType::Helium3 => 0.0001,  // 100 kg He-3 strategic reserve
            ResourceType::Deuterium => 0.1,   // 100 kt D₂O stockpiles
            ResourceType::Tritium => 0.05,    // 50 kg T₂ reserve
            // Food
            ResourceType::Food => 800.0, // 800 Mt grain reserves
            // Exotic / late-game (effectively uncapped)
            _ => f64::MAX,
        }
    }

    /// Effective stockpile cap for `resource`, accounting for storage buildings.
    ///
    /// = `stockpile_cap(resource) * self.storage_multiplier`
    pub fn effective_stockpile_cap(&self, resource: ResourceType) -> f64 {
        let base = Self::stockpile_cap(resource);
        if base >= f64::MAX {
            return f64::MAX;
        }
        base * self.storage_multiplier
    }

    /// Returns true if the stockpile for `resource` has reached its effective cap.
    pub fn is_stockpile_full(&self, resource: ResourceType) -> bool {
        let cap = self.effective_stockpile_cap(resource);
        if cap >= f64::MAX {
            return false;
        }
        self.get_stockpile(&resource) >= cap
    }

    /// Update civilization score based on power generation
    /// Score = log10(total_watts) * 10
    /// This gives a Kardashev-like scale
    pub fn update_civilization_score(&mut self) {
        let total_watts = self.energy_grid.produced;
        if total_watts > 0.0 {
            // Logarithmic scale: log10(watts) * 10
            // Example: 1 GW (10^9 W) = 90 points
            // Example: 1 TW (10^12 W) = 120 points
            self.civilization_score = total_watts.log10() * 10.0;
        } else {
            self.civilization_score = 0.0;
        }
    }

    /// Get the net power (produced - consumed)
    pub fn net_power(&self) -> f64 {
        self.energy_grid.produced - self.energy_grid.consumed
    }

    /// Get the power efficiency (consumed / produced)
    pub fn power_efficiency(&self) -> f64 {
        if self.energy_grid.produced > 0.0 {
            self.energy_grid.consumed / self.energy_grid.produced
        } else {
            0.0
        }
    }

    /// Get the yearly financial balance (income - expenses)
    pub fn balance_per_year(&self) -> f64 {
        self.income_per_year - self.expenses_per_year
    }
}

impl Default for GlobalBudget {
    fn default() -> Self {
        Self::new()
    }
}

/// Energy grid status tracking power generation and consumption
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Reflect)]
pub struct EnergyGrid {
    /// Total power produced (in Watts)
    pub produced: f64,

    /// Total power consumed (in Watts)
    pub consumed: f64,
}

impl EnergyGrid {
    /// Create a new energy grid with specified values
    pub fn new(produced: f64, consumed: f64) -> Self {
        Self { produced, consumed }
    }

    /// Get the surplus or deficit
    pub fn surplus(&self) -> f64 {
        self.produced - self.consumed
    }

    /// Returns true if the grid has sufficient power
    pub fn is_sufficient(&self) -> bool {
        self.produced >= self.consumed
    }

    /// Get the load factor (consumed / produced)
    pub fn load_factor(&self) -> f64 {
        if self.produced > 0.0 {
            (self.consumed / self.produced).min(1.0)
        } else {
            0.0
        }
    }
}

impl Default for EnergyGrid {
    fn default() -> Self {
        Self {
            produced: 1_000_000_000.0, // Start with 1 GW
            consumed: 500_000_000.0,   // Consuming 500 MW
        }
    }
}

/// System that updates the civilization score based on power generation
/// Uses Local state to track previous energy grid values for efficient change detection
///
/// Note: Uses direct equality comparison for f64 values. This is safe here because
/// energy grid values are set directly (not computed), so no floating-point precision
/// issues occur. If values were computed through arithmetic, an epsilon comparison
/// would be needed.
pub fn update_civilization_score(
    mut budget: ResMut<GlobalBudget>,
    mut last_produced: Local<f64>,
    mut last_consumed: Local<f64>,
) {
    // Only recalculate if energy grid values have changed
    // Direct equality is safe here since values are assigned, not computed
    let current_produced = budget.energy_grid.produced;
    let current_consumed = budget.energy_grid.consumed;

    if current_produced != *last_produced || current_consumed != *last_consumed {
        budget.update_civilization_score();
        *last_produced = current_produced;
        *last_consumed = current_consumed;
    }
}

/// Format power value in human-readable units (W, kW, MW, GW, TW).
///
/// Handles negative values (deficits) by stripping the sign before
/// picking the unit scale, then re-prefixing it. The previous
/// `>=` cascade fell through to the `W` branch for every negative
/// input, producing `"-960000000000.00 W"` instead of `"-960.00 GW"`
/// for the body Net column in the Economy Power tab.
pub fn format_power(watts: f64) -> String {
    let sign = if watts < 0.0 { "-" } else { "" };
    let abs = watts.abs();
    if abs >= 1e12 {
        format!("{}{:.2} TW", sign, abs / 1e12)
    } else if abs >= 1e9 {
        format!("{}{:.2} GW", sign, abs / 1e9)
    } else if abs >= 1e6 {
        format!("{}{:.2} MW", sign, abs / 1e6)
    } else if abs >= 1e3 {
        format!("{}{:.2} kW", sign, abs / 1e3)
    } else {
        format!("{}{:.2} W", sign, abs)
    }
}

/// Format currency value in human-readable units (MC)
pub fn format_currency(mc: f64) -> String {
    let abs = mc.abs();
    let sign = if mc < 0.0 { "-" } else { "" };
    if abs >= 1_000_000.0 {
        format!("{}{:.1}M MC", sign, abs / 1_000_000.0)
    } else if abs >= 1_000.0 {
        format!("{}{:.1}K MC", sign, abs / 1_000.0)
    } else {
        format!("{}{:.0} MC", sign, abs)
    }
}

// ============================================================
// ContextualStockpile — view-scoped aggregate for the UI
// ============================================================

/// View-scoped aggregate of all `LocalStockpile` components.
///
/// Updated each frame by `update_contextual_stockpile`:
/// - **System view**: sums stockpiles of every body in `CurrentStarSystem`.
/// - **Starmap view**: sums stockpiles of every body across all systems.
///
/// The resource bar and construction affordability checks read from this
/// resource so the displayed numbers match the player's current context.
#[derive(Resource, Debug, Clone, Default, Reflect)]
#[reflect(Resource)]
pub struct ContextualStockpile {
    /// Aggregated stockpiles for the current view context (Mt).
    pub stockpiles: HashMap<ResourceType, f64>,
    /// Human-readable label for the current context (e.g. "Sol System").
    pub context_label: String,
    /// The system ID being shown, or `None` if showing all systems.
    pub active_system_id: Option<usize>,
}

impl ContextualStockpile {
    /// Get the aggregated stockpile for a resource in the current context.
    pub fn get(&self, resource: &ResourceType) -> f64 {
        self.stockpiles.get(resource).copied().unwrap_or(0.0)
    }

    /// Sum of all stockpiles for a slice of resource types.
    pub fn category_total(&self, resources: &[ResourceType]) -> f64 {
        resources.iter().map(|r| self.get(r)).sum()
    }
}

/// System that builds `ContextualStockpile` from `LocalStockpile` components.
///
/// Runs every frame in `Update`.  Reads `ViewMode` and `CurrentStarSystem`
/// to decide which bodies to aggregate.
pub fn update_contextual_stockpile(
    view_mode: Res<ViewMode>,
    current_system: Res<CurrentStarSystem>,
    local_query: Query<(Option<&SystemId>, &LocalStockpile)>,
    // With<Star> ensures we only find the star body, not planets/moons like Pluto
    star_query: Query<
        (&crate::plugins::solar_system::CelestialBody, &SystemId),
        With<crate::plugins::solar_system::Star>,
    >,
    mut contextual: ResMut<ContextualStockpile>,
) {
    contextual.stockpiles.clear();

    match *view_mode {
        ViewMode::System => {
            // Aggregate only bodies in the active star system
            let sys_id = current_system.0;
            contextual.active_system_id = Some(sys_id);
            for (sid_opt, stockpile) in local_query.iter() {
                let body_sys = sid_opt.map(|s| s.0).unwrap_or(0);
                if body_sys == sys_id {
                    for (rt, &amount) in &stockpile.stockpiles {
                        *contextual.stockpiles.entry(*rt).or_insert(0.0) += amount;
                    }
                }
            }
            // Find the star name for this system
            let star_name = star_query
                .iter()
                .find(|(_, sid)| sid.0 == sys_id)
                .map(|(body, _)| body.name.clone())
                .unwrap_or_else(|| format!("System {sys_id}"));
            contextual.context_label = format!("{star_name} System");
        }
        ViewMode::Starmap => {
            // Aggregate all bodies
            contextual.active_system_id = None;
            for (_, stockpile) in local_query.iter() {
                for (rt, &amount) in &stockpile.stockpiles {
                    *contextual.stockpiles.entry(*rt).or_insert(0.0) += amount;
                }
            }
            contextual.context_label = "All Systems".to_string();
        }
    }
}

/// System that scans all colonies for `StorageCapacity` building modifiers and
/// updates `GlobalBudget.storage_multiplier`.
///
/// Each Warehouse / Resource Depot has `StorageCapacity = 0.25`, meaning it
/// adds +25% to ALL per-resource stockpile caps globally.
/// `storage_multiplier = 1.0 + Σ(modifier.value × count)`.
///
/// v3.8.16: raised from 0.10 → 0.25 per depot. At +10% the player needed
/// ten depots to double storage ("gigantic storage farms"); at +25% the
/// four starting depots already give a 2.0× multiplier and a handful more
/// makes storage a meaningful strategic lever without spamming depots.
pub fn update_storage_capacity(
    mut budget: ResMut<GlobalBudget>,
    colonies: Query<&Colony>,
    buildings_data: Option<Res<BuildingsData>>,
) {
    let data = match buildings_data {
        Some(ref d) if !d.definitions.is_empty() => d,
        _ => return,
    };

    let mut total_bonus = 0.0_f64;
    for colony in colonies.iter() {
        for (building_type, &count) in &colony.buildings {
            if count == 0 {
                continue;
            }
            if let Some(def) = data.get(building_type) {
                for modifier in &def.modifiers {
                    if modifier.modifier_type == "StorageCapacity" {
                        total_bonus += modifier.value * count as f64;
                    }
                }
            }
        }
    }
    budget.storage_multiplier = 1.0 + total_bonus;
}

/// System to aggregate power from all generators and update global budget
pub fn update_power_grid(
    mut budget: ResMut<GlobalBudget>,
    query: Query<&PowerGenerator>,
    colonies: Query<&Colony>,
    buildings_data: Option<Res<BuildingsData>>,
) {
    let mut total_produced = 0.0;
    let mut breakdown = HashMap::new();

    // 1. Existing PowerGenerator entities
    for generator in query.iter() {
        total_produced += generator.output;
        *breakdown.entry(generator.source_type).or_insert(0.0) += generator.output;
    }

    // 2. Colony buildings
    if let Some(ref data) = buildings_data {
        for colony in colonies.iter() {
            let totals = calculate_colony_power_totals(colony, Some(data));
            total_produced += totals.produced_watts;
            *breakdown.entry(PowerSourceType::Planet).or_insert(0.0) += totals.produced_watts;
        }
    }

    // 3. Calculate consumption using per-building power_demand_mw from data.
    //    Falls back to a generic 400 MW estimate when BuildingsData is unavailable.
    let mut total_consumed = 0.0;
    for colony in colonies.iter() {
        let totals = calculate_colony_power_totals(colony, buildings_data.as_deref());
        total_consumed += totals.consumed_watts;
    }

    // Update grid
    budget.energy_grid.produced = total_produced;
    budget.energy_grid.consumed = total_consumed;
    budget.power_breakdown = breakdown;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_global_budget_creation() {
        let budget = GlobalBudget::new();
        assert!(budget.get_stockpile(&ResourceType::Water) > 0.0);
        assert!(
            budget.get_stockpile(&ResourceType::Uranium) > 0.0,
            "Uranium should have starting stockpile"
        );
    }

    #[test]
    fn test_add_resource() {
        let mut budget = GlobalBudget::new();
        let initial = budget.get_stockpile(&ResourceType::Iron);

        budget.add_resource(ResourceType::Iron, 100.0);

        assert_eq!(budget.get_stockpile(&ResourceType::Iron), initial + 100.0);
    }

    #[test]
    fn test_consume_resource_success() {
        let mut budget = GlobalBudget::new();
        budget.add_resource(ResourceType::Iron, 100.0);

        let success = budget.consume_resource(ResourceType::Iron, 50.0);

        assert!(success);
    }

    #[test]
    fn test_consume_resource_insufficient() {
        let mut budget = GlobalBudget::new();
        // Set a specific amount
        budget.stockpiles.insert(ResourceType::Titanium, 10.0);

        let success = budget.consume_resource(ResourceType::Titanium, 50.0);

        assert!(!success);
        assert_eq!(budget.get_stockpile(&ResourceType::Titanium), 10.0); // Unchanged
    }

    #[test]
    fn test_civilization_score_calculation() {
        let mut budget = GlobalBudget::new();
        budget.energy_grid.produced = 1e9; // 1 GW

        budget.update_civilization_score();

        // log10(1e9) * 10 = 90
        assert!((budget.civilization_score - 90.0).abs() < 0.1);
    }

    #[test]
    fn test_civilization_score_zero_power() {
        let mut budget = GlobalBudget::new();
        budget.energy_grid.produced = 0.0;

        budget.update_civilization_score();

        assert_eq!(budget.civilization_score, 0.0);
    }

    #[test]
    fn test_energy_grid_surplus() {
        let grid = EnergyGrid::new(1000.0, 600.0);
        assert_eq!(grid.surplus(), 400.0);
    }

    #[test]
    fn test_energy_grid_deficit() {
        let grid = EnergyGrid::new(500.0, 800.0);
        assert_eq!(grid.surplus(), -300.0);
        assert!(!grid.is_sufficient());
    }

    #[test]
    fn test_energy_grid_load_factor() {
        let grid = EnergyGrid::new(1000.0, 750.0);
        assert!((grid.load_factor() - 0.75).abs() < 0.001);
    }

    #[test]
    fn test_power_formatting() {
        assert_eq!(format_power(500.0), "500.00 W");
        assert_eq!(format_power(1500.0), "1.50 kW");
        assert_eq!(format_power(2_500_000.0), "2.50 MW");
        assert_eq!(format_power(3_500_000_000.0), "3.50 GW");
        assert_eq!(format_power(4_500_000_000_000.0), "4.50 TW");
    }

    #[test]
    fn test_power_formatting_negative() {
        // Negative values must pick the same unit scale as positives,
        // not fall through to the raw-W branch.
        assert_eq!(format_power(-500.0), "-500.00 W");
        assert_eq!(format_power(-1500.0), "-1.50 kW");
        assert_eq!(format_power(-2_500_000.0), "-2.50 MW");
        assert_eq!(format_power(-3_500_000_000.0), "-3.50 GW");
        assert_eq!(format_power(-4_500_000_000_000.0), "-4.50 TW");
        // Exact zero stays unsigned.
        assert_eq!(format_power(0.0), "0.00 W");
    }

    #[test]
    fn test_net_power() {
        let mut budget = GlobalBudget::new();
        budget.energy_grid.produced = 1000.0;
        budget.energy_grid.consumed = 600.0;

        assert_eq!(budget.net_power(), 400.0);
    }

    #[test]
    fn test_power_efficiency() {
        let mut budget = GlobalBudget::new();
        budget.energy_grid.produced = 1000.0;
        budget.energy_grid.consumed = 800.0;

        assert!((budget.power_efficiency() - 0.8).abs() < 0.001);
    }

    #[test]
    #[should_panic(expected = "Cannot add negative resource amount")]
    fn test_add_resource_negative_panics() {
        let mut budget = GlobalBudget::new();
        budget.add_resource(ResourceType::Iron, -100.0);
    }

    #[test]
    #[should_panic(expected = "Cannot consume negative resource amount")]
    fn test_consume_resource_negative_panics() {
        let mut budget = GlobalBudget::new();
        budget.consume_resource(ResourceType::Iron, -50.0);
    }

    #[test]
    fn test_treasury_initial() {
        let budget = GlobalBudget::new();
        assert_eq!(budget.treasury, 1_000_000.0);
    }

    #[test]
    fn test_balance_calculation() {
        let mut budget = GlobalBudget::new();
        budget.income_per_year = 500.0;
        budget.expenses_per_year = 200.0;
        assert_eq!(budget.balance_per_year(), 300.0);
    }

    #[test]
    fn test_format_currency() {
        assert_eq!(format_currency(500.0), "500 MC");
        assert_eq!(format_currency(1500.0), "1.5K MC");
        assert_eq!(format_currency(2_500_000.0), "2.5M MC");
        assert_eq!(format_currency(-500.0), "-500 MC");
    }

    #[test]
    fn test_add_resource_capped_respects_limit() {
        let mut budget = GlobalBudget::new();
        // v3.8.4: Gold base cap = 0.001 Mt (2026 vault/central-bank stock);
        // set current stockpile just below it.
        budget.stockpiles.insert(ResourceType::Gold, 0.0005);
        let added = budget.add_resource_capped(ResourceType::Gold, 0.005);
        let expected_added = 0.001 - 0.0005;
        assert!(
            (added - expected_added).abs() < 1e-9,
            "Should only add up to cap; got {added}"
        );
        assert!((budget.get_stockpile(&ResourceType::Gold) - 0.001).abs() < 1e-9);
    }

    #[test]
    fn test_add_resource_capped_when_full() {
        let mut budget = GlobalBudget::new();
        // Manually set gold to exactly the cap
        let cap = GlobalBudget::stockpile_cap(ResourceType::Gold);
        budget.stockpiles.insert(ResourceType::Gold, cap);
        let added = budget.add_resource_capped(ResourceType::Gold, 0.001);
        assert_eq!(added, 0.0, "Nothing should be added when at cap");
        assert!((budget.get_stockpile(&ResourceType::Gold) - cap).abs() < 1e-12);
    }

    #[test]
    fn test_is_stockpile_full() {
        let mut budget = GlobalBudget::new();
        let cap = GlobalBudget::stockpile_cap(ResourceType::Gold);
        budget.stockpiles.insert(ResourceType::Gold, cap);
        assert!(budget.is_stockpile_full(ResourceType::Gold));
        budget.stockpiles.insert(ResourceType::Gold, cap - 0.001);
        assert!(!budget.is_stockpile_full(ResourceType::Gold));
    }

    #[test]
    fn test_storage_multiplier_increases_effective_cap() {
        let mut budget = GlobalBudget::new();
        budget.storage_multiplier = 1.5; // 20 warehouses × 2.5%
        let base_cap = GlobalBudget::stockpile_cap(ResourceType::Iron);
        let eff_cap = budget.effective_stockpile_cap(ResourceType::Iron);
        assert!(
            (eff_cap - base_cap * 1.5).abs() < 1e-6,
            "Effective cap should be 1.5× base"
        );
    }

    #[test]
    fn test_storage_multiplier_allows_adding_beyond_base_cap() {
        let mut budget = GlobalBudget::new();
        budget.storage_multiplier = 2.0;
        let base_cap = GlobalBudget::stockpile_cap(ResourceType::Iron);
        // Fill to exactly the base cap
        budget.stockpiles.insert(ResourceType::Iron, base_cap);
        // With 2× multiplier we should still have headroom
        let added = budget.add_resource_capped(ResourceType::Iron, base_cap);
        assert!(
            added > 0.0,
            "Should be able to add beyond base cap when multiplier > 1"
        );
    }

    #[test]
    fn test_food_stockpile_initial_stays_within_one_year_margin() {
        let budget = GlobalBudget::new();
        let food = budget.get_stockpile(&ResourceType::Food);
        // v3.8.4: per-capita food = 0.0000011 Mt/p/yr (FAO 2024 SOFA
        // 1,100 kg/p/yr).  Earth (8.2B) consumes 9,020 Mt/yr.
        // 50% of 800 Mt cap = 400 Mt ≈ 16 days of supply.
        let earth_annual_consumption = 8.2e9 * 0.0000011;
        assert!(
            food <= GlobalBudget::stockpile_cap(ResourceType::Food),
            "Initial food stockpile ({food:.0} Mt) should not exceed the cap"
        );
        // Sanity: at least 10 days of supply at the new per-capita rate.
        let ten_days_supply = earth_annual_consumption / 36.5;
        assert!(
            food >= ten_days_supply,
            "Initial food stockpile ({food:.0} Mt) should cover at least 10 days ({ten_days_supply:.0} Mt)"
        );
    }

    #[test]
    fn test_stockpile_caps_are_realistic_2026_active_storage() {
        // v3.8.4: stockpile caps are 2026 active storage (warehouses,
        // ports, LME-bonded, strategic reserves), NOT 1 year of demand.
        // These values are anchors; if they change the test catches
        // a regression in the cap-recalibration pass.
        assert_eq!(GlobalBudget::stockpile_cap(ResourceType::Iron), 100.0);
        assert_eq!(GlobalBudget::stockpile_cap(ResourceType::Copper), 30.0);
        assert_eq!(GlobalBudget::stockpile_cap(ResourceType::Carbon), 200.0);
        assert_eq!(GlobalBudget::stockpile_cap(ResourceType::Food), 800.0);
        // Spot-check the post-recalibration values
        assert!(GlobalBudget::stockpile_cap(ResourceType::Silicates) > 0.0);
        assert!(GlobalBudget::stockpile_cap(ResourceType::Methane) > 0.0);
    }
}
