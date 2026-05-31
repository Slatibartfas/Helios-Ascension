//! Economy module for resource management and planetary economics
//!
//! This module provides a comprehensive economic system including:
//! - 15 different resource types (volatiles, construction, noble gases, fissiles, specialty)
//! - Planetary resource deposits with abundance and accessibility
//! - Realistic resource generation based on distance from sun (frost line)
//! - Global budget and stockpile management
//! - Energy grid tracking and civilization scoring
//! - Logistics network: resource requests, minimum stockpiles, private shipping companies

use bevy::prelude::*;

pub mod budget;
pub mod company;
pub mod components;
pub mod generation;
pub mod logistics;
pub mod mining;
pub(crate) mod profiles;
pub mod pool;
pub mod types;

pub use budget::{
    calculate_colony_power_totals, format_currency, format_power, update_civilization_score,
    update_contextual_stockpile, update_power_grid, update_storage_capacity,
    ColonyPowerTotals, ContextualStockpile, EnergyGrid, GlobalBudget, ResourceRateTracker,
    SECONDS_PER_MONTH, SECONDS_PER_YEAR,
};
pub use company::{ShippingCompanies, ShippingCompany};
pub use components::{
    LocalStockpile, MineralDeposit, OrbitsBody, PlanetResources, PowerGenerator, PowerSourceType,
    SpectralClass, StarSystem,
};
pub use generation::{generate_ring_resources, generate_solar_system_resources};
pub use logistics::{
    check_minimum_stockpile_requests, complete_deliveries, prune_old_requests, MinimumStockpile,
    PendingResourceRequests, RequestPriority, RequestState, ResourceRequest,
};
pub use mining::{extract_resources, update_resource_rates, MiningOperation};
pub use types::{ResourcePhase, ResourceType};
pub use pool::ResourcePool;

/// Plugin that adds the economy system to the Bevy app
pub struct EconomyPlugin;

impl Plugin for EconomyPlugin {
    fn build(&self, app: &mut App) {
        app
            // Resources
            .init_resource::<GlobalBudget>()
            .init_resource::<ResourceRateTracker>()
            .init_resource::<ContextualStockpile>()
            // Logistics resources
            .init_resource::<PendingResourceRequests>()
            .init_resource::<ShippingCompanies>()
            // Startup systems
            .add_systems(
                PostStartup,
                (generate_solar_system_resources, generate_ring_resources)
                    .chain()
                    .before(generation::stamp_resource_phases),
            )
            .add_systems(PostStartup, generation::stamp_resource_phases)
            // Update systems
            .add_systems(
                Update,
                (
                    update_storage_capacity,
                    update_power_grid,
                    update_civilization_score.after(update_power_grid),
                    extract_resources,
                    update_resource_rates,
                    // Context-aware aggregation: must run after mining/production
                    update_contextual_stockpile.after(extract_resources),
                    // Logistics: check minimums → company AI → deliver → return freighters
                    // check_minimum_stockpile_requests must run after extraction/drains
                    // so it reads up-to-date stockpile values.
                    check_minimum_stockpile_requests.after(extract_resources),
                    company::process_company_ai.after(check_minimum_stockpile_requests),
                    complete_deliveries.after(company::process_company_ai),
                    company::update_company_fleets.after(complete_deliveries),
                    prune_old_requests.after(complete_deliveries),
                ),
            );
    }
}
