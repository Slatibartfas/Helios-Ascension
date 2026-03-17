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
#[derive(Resource, Debug, Clone, Default)]
pub struct ResourceRateTracker {
    /// Monthly production rate per resource type (Mt/month) — global total
    pub resource_rates: HashMap<ResourceType, f64>,
    /// Per-entity (body) monthly rates for resource tooltip breakdown
    pub per_entity_rates: HashMap<Entity, HashMap<ResourceType, f64>>,
    /// Monthly research point generation
    pub research_rate_per_month: f64,
    /// Monthly engineering point generation
    pub engineering_rate_per_month: f64,
}

/// Seconds in one 30-day month (30 × 86400)
pub const SECONDS_PER_MONTH: f64 = 2_592_000.0;
/// Seconds in one year (365.25 × 86400)
pub const SECONDS_PER_YEAR: f64 = 31_557_600.0;

#[derive(Debug, Clone, Copy, Default)]
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
}

/// Global economic budget and resource management
/// Tracks civilization-wide stockpiles and power generation
#[derive(Resource, Debug, Clone, Serialize, Deserialize)]
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

        // Initialize with starting resources representing roughly half a year
        // of 2026 Earth throughput/consumption. Earth's four starting depots add
        // +10% storage headroom, so the effective cap lands at ~1.1 years.
        // All values are in Megatons (Mt).

        // Volatiles / gases
        stockpiles.insert(ResourceType::Water, 300.0);      // 600 Mt/yr → 6 mo
        stockpiles.insert(ResourceType::Oxygen, 50.0);      // 100 Mt/yr → 6 mo
        stockpiles.insert(ResourceType::Hydrogen, 50.0);    // 100 Mt/yr → 6 mo
        stockpiles.insert(ResourceType::Methane, 1_950.0);  // 3,900 Mt/yr → 6 mo
        stockpiles.insert(ResourceType::Nitrogen, 65.0);    // 130 Mt/yr → 6 mo
        stockpiles.insert(ResourceType::Ammonia, 95.0);     // 190 Mt/yr → 6 mo

        // Construction / common metals
        stockpiles.insert(ResourceType::Iron, 1_250.0);     // 2,500 Mt/yr → 6 mo
        stockpiles.insert(ResourceType::Copper, 13.0);      // 26 Mt/yr → 6 mo
        stockpiles.insert(ResourceType::Aluminum, 35.0);    // 70 Mt/yr → 6 mo
        stockpiles.insert(ResourceType::Silicates, 25_000.0); // 50,000 Mt/yr → 6 mo
        stockpiles.insert(ResourceType::Nickel, 1.65);      // 3.3 Mt/yr → 6 mo
        stockpiles.insert(ResourceType::Chromium, 22.0);    // 44 Mt/yr → 6 mo
        stockpiles.insert(ResourceType::Magnesium, 0.6);    // 1.2 Mt/yr → 6 mo
        stockpiles.insert(ResourceType::Cobalt, 0.105);     // 0.21 Mt/yr → 6 mo
        stockpiles.insert(ResourceType::Tungsten, 0.047);   // 0.094 Mt/yr → 6 mo
        stockpiles.insert(ResourceType::Titanium, 5.0);     // 10 Mt/yr → 6 mo

        // Non-metals / chemical industry
        stockpiles.insert(ResourceType::Carbon, 2_150.0);   // 4,300 Mt/yr → 6 mo
        stockpiles.insert(ResourceType::Phosphorus, 120.0); // 240 Mt/yr → 6 mo
        stockpiles.insert(ResourceType::Sulfur, 32.5);      // 65 Mt/yr → 6 mo
        stockpiles.insert(ResourceType::Polymers, 217.5);   // 435 Mt/yr → 6 mo
        stockpiles.insert(ResourceType::Fluorine, 1.0);     // 2 Mt/yr → 6 mo

        // Strategic / rare
        stockpiles.insert(ResourceType::Lithium, 0.45);     // 0.9 Mt/yr → 6 mo
        stockpiles.insert(ResourceType::RareEarths, 0.175); // 0.35 Mt/yr → 6 mo

        // Precious metals
        stockpiles.insert(ResourceType::Gold, 0.0018);      // 0.0036 Mt/yr → 6 mo
        stockpiles.insert(ResourceType::Silver, 0.014);     // 0.028 Mt/yr → 6 mo
        stockpiles.insert(ResourceType::Platinum, 0.000115); // 0.00023 Mt/yr → 6 mo

        // Fissile / fusion fuels
        stockpiles.insert(ResourceType::Uranium, 0.029);    // 0.058 Mt/yr → 6 mo
        stockpiles.insert(ResourceType::Thorium, 0.005);    // 0.01 Mt/yr → 6 mo
        stockpiles.insert(ResourceType::Plutonium, 0.0);    // manufactured in breeder reactors
        stockpiles.insert(ResourceType::Deuterium, 0.005);  // 0.01 Mt/yr → 6 mo
        stockpiles.insert(ResourceType::Tritium, 0.0);      // bred from lithium blankets
        stockpiles.insert(ResourceType::Helium3, 0.0);      // essentially none yet

        // Food — game units (8.2B pop × 0.0001 Mt/yr = 820,000 Mt/yr consumption)
        stockpiles.insert(ResourceType::Food, 450_000.0);   // ~0.55yr reserve

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
            amount, resource
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
    /// Values are calibrated at **1 year of 2026 Earth annual throughput**.
    /// Earth's starting four Warehouses add +10% capacity, which yields the
    /// requested one-year baseline plus a small construction margin.
    ///
    /// Use `effective_stockpile_cap()` for the actual enforced cap which
    /// includes the `storage_multiplier` bonus from Warehouse / Resource Depot
    /// buildings.
    pub fn stockpile_cap(resource: ResourceType) -> f64 {
        match resource {
            ResourceType::Food => 820_000.0,
            ResourceType::Iron => 2_500.0,
            ResourceType::Copper => 26.0,
            ResourceType::Aluminum => 70.0,
            ResourceType::Nickel => 3.3,
            ResourceType::Chromium => 44.0,
            ResourceType::Magnesium => 1.2,
            ResourceType::Cobalt => 0.21,
            ResourceType::Tungsten => 0.094,
            ResourceType::Titanium => 10.0,
            ResourceType::Silicates => 50_000.0,
            ResourceType::Carbon => 4_300.0,
            ResourceType::Sulfur => 65.0,
            ResourceType::Phosphorus => 240.0,
            ResourceType::Polymers => 435.0,
            ResourceType::Fluorine => 2.0,
            ResourceType::RareEarths => 0.35,
            ResourceType::Lithium => 0.9,
            ResourceType::Gold => 0.0036,
            ResourceType::Silver => 0.028,
            ResourceType::Platinum => 0.00023,
            ResourceType::Water => 600.0,
            ResourceType::Oxygen => 100.0,
            ResourceType::Hydrogen => 100.0,
            ResourceType::Nitrogen => 130.0,
            ResourceType::Ammonia => 190.0,
            ResourceType::Methane => 3_900.0,
            ResourceType::Uranium => 0.058,
            ResourceType::Thorium => 0.01,
            ResourceType::Plutonium => 0.02,
            ResourceType::Deuterium => 0.01,
            ResourceType::Tritium => 0.05,
            ResourceType::Helium3 => 10.0,
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
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
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

/// Format power value in human-readable units (W, kW, MW, GW, TW)
pub fn format_power(watts: f64) -> String {
    if watts >= 1e12 {
        format!("{:.2} TW", watts / 1e12)
    } else if watts >= 1e9 {
        format!("{:.2} GW", watts / 1e9)
    } else if watts >= 1e6 {
        format!("{:.2} MW", watts / 1e6)
    } else if watts >= 1e3 {
        format!("{:.2} kW", watts / 1e3)
    } else {
        format!("{:.2} W", watts)
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
#[derive(Resource, Debug, Clone, Default)]
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
/// Each Warehouse / Resource Depot has `StorageCapacity = 0.025`, meaning it
/// adds +2.5% to ALL per-resource stockpile caps globally.
/// `storage_multiplier = 1.0 + Σ(modifier.value × count)`.
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
        // Gold base cap is 0.0036 Mt; set current stockpile just below it.
        budget.stockpiles.insert(ResourceType::Gold, 0.002);
        let added = budget.add_resource_capped(ResourceType::Gold, 0.005);
        let expected_added = 0.0036 - 0.002;
        assert!(
            (added - expected_added).abs() < 1e-9,
            "Should only add up to cap; got {added}"
        );
        assert!((budget.get_stockpile(&ResourceType::Gold) - 0.0036).abs() < 1e-9);
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
        assert!((eff_cap - base_cap * 1.5).abs() < 1e-6, "Effective cap should be 1.5× base");
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
        assert!(added > 0.0, "Should be able to add beyond base cap when multiplier > 1");
    }

    #[test]
    fn test_food_stockpile_initial_stays_within_one_year_margin() {
        let budget = GlobalBudget::new();
        let food = budget.get_stockpile(&ResourceType::Food);
        let earth_annual_consumption = 8.2e9 * 0.0001;
        assert!(
            food >= earth_annual_consumption * 0.5,
            "Initial food stockpile ({food:.0} Mt) should cover at least half a year of Earth consumption ({earth_annual_consumption:.0} Mt/yr)"
        );
        assert!(
            food <= GlobalBudget::stockpile_cap(ResourceType::Food),
            "Initial food stockpile ({food:.0} Mt) should not exceed the one-year base cap"
        );
    }

    #[test]
    fn test_stockpile_caps_are_one_year_baselines() {
        assert_eq!(GlobalBudget::stockpile_cap(ResourceType::Iron), 2_500.0);
        assert_eq!(GlobalBudget::stockpile_cap(ResourceType::Copper), 26.0);
        assert_eq!(GlobalBudget::stockpile_cap(ResourceType::Carbon), 4_300.0);
    }
}
