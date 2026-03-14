use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::types::ResourceType;
use crate::colony::{BuildingsData, Colony};
use crate::economy::{PowerGenerator, PowerSourceType};

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
    /// Each Warehouse/Resource-Depot contributes +5%, so:
    ///   0 depots → 1.0×, 10 depots → 1.5×, 20 depots → 2.0×.
    pub storage_multiplier: f64,
}

impl GlobalBudget {
    /// Create a new global budget with starting resources
    pub fn new() -> Self {
        let mut stockpiles = HashMap::new();

        // Initialize with starting resources representing approximately 3 months
        // of 2026 Earth annual production (what's naturally in industrial
        // circulation at any point in time).  All values in Megatons (Mt).
        //
        // Derivation: production_Mt_yr × 0.25 = 3-month buffer.
        // Sources: USGS Mineral Commodity Summaries 2025, IEA, UN FAO.
        //
        // NOTE: Food is in game units (population × 0.0001 Mt/yr consumption),
        //       calibrated so food_factor = 1.0 when ≥ 1 yr of consumption
        //       is in stock. The 1,000,000 Mt starting value ≈ 1.22 yr reserve.

        // Volatiles / gases (industrial production as liquid/compressed)
        stockpiles.insert(ResourceType::Water, 100.0);     // 400 Mt/yr processed → 3 mo
        stockpiles.insert(ResourceType::Oxygen, 25.0);     // 100 Mt/yr industrial O₂ → 3 mo
        stockpiles.insert(ResourceType::Hydrogen, 18.0);   // 70 Mt/yr industrial H₂ → 3 mo
        stockpiles.insert(ResourceType::Methane, 300.0);   // ~3,900 Mt/yr gas → 1 mo strategic
        stockpiles.insert(ResourceType::Nitrogen, 33.0);   // 130 Mt/yr → 3 mo
        stockpiles.insert(ResourceType::Ammonia, 45.0);    // 180 Mt/yr → 3 mo

        // Construction / common metals
        stockpiles.insert(ResourceType::Iron, 625.0);      // 2,500 Mt/yr → 3 mo
        stockpiles.insert(ResourceType::Copper, 5.5);      // 22 Mt/yr → 3 mo
        stockpiles.insert(ResourceType::Aluminum, 18.0);   // 70 Mt/yr → 3 mo
        stockpiles.insert(ResourceType::Silicates, 12_500.0); // 50,000 Mt/yr → 3 mo
        stockpiles.insert(ResourceType::Nickel, 0.8);      // 3.3 Mt/yr → 3 mo
        stockpiles.insert(ResourceType::Chromium, 11.0);   // 44 Mt/yr → 3 mo
        stockpiles.insert(ResourceType::Magnesium, 0.3);   // 1.2 Mt/yr → 3 mo
        stockpiles.insert(ResourceType::Cobalt, 0.05);     // 0.21 Mt/yr → 3 mo
        stockpiles.insert(ResourceType::Tungsten, 0.024);  // 0.094 Mt/yr → 3 mo
        stockpiles.insert(ResourceType::Titanium, 2.5);    // 10 Mt/yr → 3 mo

        // Non-metals / chemical industry
        stockpiles.insert(ResourceType::Carbon, 2_000.0);  // coal stockpiles ~2,000 Mt
        stockpiles.insert(ResourceType::Phosphorus, 60.0); // 240 Mt/yr phosphate rock → 3 mo
        stockpiles.insert(ResourceType::Sulfur, 16.0);     // 65 Mt/yr → 3 mo
        stockpiles.insert(ResourceType::Polymers, 100.0);  // 400 Mt/yr plastics → 3 mo
        stockpiles.insert(ResourceType::Fluorine, 0.5);    // 2 Mt/yr HF → 3 mo

        // Strategic / rare
        stockpiles.insert(ResourceType::Lithium, 0.23);    // 0.9 Mt/yr → 3 mo
        stockpiles.insert(ResourceType::RareEarths, 0.09); // 0.35 Mt/yr → 3 mo

        // Precious metals (accumulate more, 6-month buffer)
        stockpiles.insert(ResourceType::Gold, 0.0018);     // 0.0036 Mt/yr → 6 mo
        stockpiles.insert(ResourceType::Silver, 0.014);    // 0.028 Mt/yr → 6 mo
        stockpiles.insert(ResourceType::Platinum, 0.00012); // 0.00023 Mt/yr → 6 mo

        // Fissile / nuclear (strategic reserve)
        stockpiles.insert(ResourceType::Uranium, 0.15);    // large strategic reserve
        stockpiles.insert(ResourceType::Thorium, 0.001);   // minimal commercial use
        stockpiles.insert(ResourceType::Deuterium, 0.01);  // tiny production today
        stockpiles.insert(ResourceType::Helium3, 0.0);     // essentially none yet

        // Food — game units (8.2B pop × 0.0001 Mt/yr = 820,000 Mt/yr consumption)
        stockpiles.insert(ResourceType::Food, 1_000_000.0); // ~1.22yr reserve → food_factor=1.0

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
    /// Values are calibrated at **3 years of 2026 Earth annual production**
    /// (source: USGS Mineral Commodity Summaries 2025, IEA, UN FAO).
    /// Formula per resource: `base_cap = production_Mt_yr × 3`.
    ///
    /// **Food exception**: Food uses *game units* (population × 0.0001 Mt/yr),
    /// not real-world Mt.  Its cap is 2 years of game-unit consumption so the
    /// colony has a meaningful storage ceiling without being trivially easy to fill.
    ///
    /// Use `effective_stockpile_cap()` for the actual enforced cap which
    /// includes the `storage_multiplier` bonus from Warehouse / Resource Depot
    /// buildings.
    pub fn stockpile_cap(resource: ResourceType) -> f64 {
        match resource {
            // Food: 2 yr of Earth game-unit consumption (8.2B × 0.0001 × 2 = 1,640,000 Mt)
            // (Different from other resources; see doc comment above.)
            ResourceType::Food => 1_640_000.0,
            // Iron ore: 3 yr × 2,500 Mt/yr
            ResourceType::Iron => 7_500.0,
            // Copper: 3 yr × 22 Mt/yr
            ResourceType::Copper => 66.0,
            // Aluminium: 3 yr × 70 Mt/yr
            ResourceType::Aluminum => 210.0,
            // Nickel: 3 yr × 3.3 Mt/yr
            ResourceType::Nickel => 10.0,
            // Chromium: 3 yr × 44 Mt/yr
            ResourceType::Chromium => 132.0,
            // Magnesium: 3 yr × 1.2 Mt/yr
            ResourceType::Magnesium => 3.6,
            // Cobalt: 3 yr × 0.21 Mt/yr
            ResourceType::Cobalt => 0.63,
            // Tungsten: 3 yr × 0.094 Mt/yr
            ResourceType::Tungsten => 0.28,
            // Titanium: 3 yr × 10 Mt/yr
            ResourceType::Titanium => 30.0,
            // Silicates: 3 yr × 50,000 Mt/yr (construction aggregate)
            ResourceType::Silicates => 150_000.0,
            // Carbon (coal/charcoal): 3 yr × 1,000 Mt/yr carbon-equivalent
            ResourceType::Carbon => 3_000.0,
            // Sulfur: 3 yr × 65 Mt/yr
            ResourceType::Sulfur => 195.0,
            // Phosphorus (phosphate rock): 3 yr × 240 Mt/yr
            ResourceType::Phosphorus => 720.0,
            // Polymers (plastics): 3 yr × 400 Mt/yr
            ResourceType::Polymers => 1_200.0,
            // Fluorine: 3 yr × 2 Mt/yr
            ResourceType::Fluorine => 6.0,
            // Rare earths: 3 yr × 0.35 Mt/yr
            ResourceType::RareEarths => 1.05,
            // Lithium: 3 yr × 0.9 Mt/yr
            ResourceType::Lithium => 2.7,
            // Precious metals (3 yr production; also cumulative above-ground stocks)
            ResourceType::Gold => 0.011,
            ResourceType::Silver => 0.084,
            ResourceType::Platinum => 0.00069,
            // Water: processed freshwater 3 yr × 400 Mt/yr
            ResourceType::Water => 1_200.0,
            // Industrial gases
            ResourceType::Oxygen => 300.0,    // 3 yr × 100 Mt/yr
            ResourceType::Hydrogen => 210.0,  // 3 yr × 70 Mt/yr
            ResourceType::Nitrogen => 390.0,  // 3 yr × 130 Mt/yr
            ResourceType::Ammonia => 540.0,   // 3 yr × 180 Mt/yr
            // Hydrocarbons
            ResourceType::Methane => 11_700.0, // 3 yr × 3,900 Mt/yr nat.gas
            // Fissile / fusion fuels
            ResourceType::Uranium => 0.5,      // large but finite strategic reserve
            ResourceType::Thorium => 0.05,
            ResourceType::Deuterium => 100.0,  // room for future fusion programme
            ResourceType::Helium3 => 50.0,
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
        // Gold base cap is 0.011 Mt; set current stockpile to 0.009 Mt
        budget.stockpiles.insert(ResourceType::Gold, 0.009);
        let added = budget.add_resource_capped(ResourceType::Gold, 0.005);
        // Only 0.002 should fit (0.011 cap - 0.009 current)
        let expected_added = 0.011 - 0.009;
        assert!(
            (added - expected_added).abs() < 1e-9,
            "Should only add up to cap; got {added}"
        );
        assert!((budget.get_stockpile(&ResourceType::Gold) - 0.011).abs() < 1e-9);
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
        budget.storage_multiplier = 1.5; // 10 warehouses × 5%
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
    fn test_food_stockpile_initial_gives_full_food_factor() {
        let budget = GlobalBudget::new();
        let food = budget.get_stockpile(&ResourceType::Food);
        // Earth consumption ≈ 820,000 Mt/yr; initial stockpile should be ≥ 1 year
        let earth_annual_consumption = 8.2e9 * 0.0001; // 820,000 Mt
        assert!(
            food >= earth_annual_consumption,
            "Initial food stockpile ({food:.0} Mt) should cover >= 1yr Earth consumption ({earth_annual_consumption:.0} Mt)"
        );
    }
}

/// System that scans all colonies for `StorageCapacity` building modifiers and
/// updates `GlobalBudget.storage_multiplier`.
///
/// Each Warehouse / Resource Depot has `StorageCapacity = 0.05`, meaning it
/// adds +5% to ALL per-resource stockpile caps globally.
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
    // Assume building modifiers "PowerGeneration" is in GW (1e9 W)
    if let Some(data) = buildings_data {
        for colony in colonies.iter() {
            for (building_type, &count) in &colony.buildings {
                if count == 0 {
                    continue;
                }

                if let Some(def) = data.get(building_type) {
                    for modifier in &def.modifiers {
                        if modifier.modifier_type == "PowerGeneration" {
                            // Scale: 5.0 -> 5 GW
                            let power_gw = modifier.value * count as f64;
                            let power_watts = power_gw * 1_000_000_000.0;
                            total_produced += power_watts;

                            // Categorize based on PowerSourceType
                            *breakdown.entry(PowerSourceType::Planet).or_insert(0.0) += power_watts;
                        }
                    }
                }
            }
        }
    }

    // 3. Calculate consumption (Temporary: 400 MW per building)
    // TODO: Add power_consumption to building data
    let mut total_consumed = 0.0;
    for colony in colonies.iter() {
        let building_count: u32 = colony.buildings.values().sum();
        total_consumed += building_count as f64 * 400_000_000.0;
    }

    // Update grid
    budget.energy_grid.produced = total_produced;
    budget.energy_grid.consumed = total_consumed;
    budget.power_breakdown = breakdown;
}
