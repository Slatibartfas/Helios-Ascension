use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::types::ResourceType;
use crate::economy::{PowerGenerator, PowerSourceType};

/// Tracks per-month income/production rates for all resources
/// and research/engineering points for display in the resource bar.
///
/// Rates are stored as "amount per 30-day month" (2,592,000 seconds).
#[derive(Resource, Debug, Clone, Default)]
pub struct ResourceRateTracker {
    /// Monthly production rate per resource type (Mt/month)
    pub resource_rates: HashMap<ResourceType, f64>,
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
    /// Get the monthly rate for a resource type
    pub fn get_resource_rate(&self, resource: &ResourceType) -> f64 {
        self.resource_rates.get(resource).copied().unwrap_or(0.0)
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
}

impl GlobalBudget {
    /// Create a new global budget with starting resources
    pub fn new() -> Self {
        let mut stockpiles = HashMap::new();

        // Initialize with some starting resources for gameplay
        stockpiles.insert(ResourceType::Water, 100.0);
        stockpiles.insert(ResourceType::Oxygen, 50.0);
        stockpiles.insert(ResourceType::Iron, 50.0);
        stockpiles.insert(ResourceType::Copper, 20.0);

        Self {
            stockpiles,
            energy_grid: EnergyGrid::default(),
            civilization_score: 0.0,
            power_breakdown: HashMap::new(),
            treasury: 1000.0, // Starting treasury: 1000 MC
            income_per_year: 0.0,
            expenses_per_year: 0.0,
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
        assert_eq!(budget.get_stockpile(&ResourceType::Uranium), 0.0);
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
        assert_eq!(budget.treasury, 1000.0);
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
}

/// System to aggregate power from all generators and update global budget
pub fn update_power_grid(
    mut budget: ResMut<GlobalBudget>,
    query: Query<&PowerGenerator>,
) {
    let mut total_produced = 0.0;
    let mut breakdown = HashMap::new();

    for generator in query.iter() {
        total_produced += generator.output;
        *breakdown.entry(generator.source_type).or_insert(0.0) += generator.output;
    }

    // Update grid production
    budget.energy_grid.produced = total_produced;
    budget.power_breakdown = breakdown;
}
