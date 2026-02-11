//! Economy module for resource management and planetary economics
//!
//! This module provides a comprehensive economic system including:
//! - 15 different resource types (volatiles, construction, noble gases, fissiles, specialty)
//! - Planetary resource deposits with abundance and accessibility
//! - Realistic resource generation based on distance from sun (frost line)
//! - Global budget and stockpile management
//! - Energy grid tracking and civilization scoring

use bevy::prelude::*;

pub mod budget;
pub mod components;
pub mod generation;
pub mod mining;
pub mod types;

pub use budget::{
    format_currency, format_power, update_civilization_score, update_power_grid, EnergyGrid,
    GlobalBudget, ResourceRateTracker, SECONDS_PER_MONTH, SECONDS_PER_YEAR,
};
pub use components::{
    MineralDeposit, OrbitsBody, PlanetResources, PowerGenerator, PowerSourceType, SpectralClass,
    StarSystem,
};
pub use generation::generate_solar_system_resources;
pub use mining::{extract_resources, update_resource_rates, MiningOperation};
pub use types::ResourceType;

/// Plugin that adds the economy system to the Bevy app
pub struct EconomyPlugin;

impl Plugin for EconomyPlugin {
    fn build(&self, app: &mut App) {
        app
            // Resources
            .init_resource::<GlobalBudget>()
            .init_resource::<ResourceRateTracker>()
            // Startup systems
            .add_systems(
                Startup,
                generate_solar_system_resources.after(
                    // Run after solar system is set up
                    crate::plugins::solar_system::setup_solar_system,
                ),
            )
            // Update systems
            .add_systems(
                Update,
                (
                    update_power_grid,
                    update_civilization_score.after(update_power_grid),
                    extract_resources,
                    update_resource_rates,
                ),
            );
    }
}
