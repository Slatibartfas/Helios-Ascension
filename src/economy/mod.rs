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
pub mod types;

pub use budget::{format_power, update_civilization_score, EnergyGrid, GlobalBudget};
pub use components::{MineralDeposit, PlanetResources};
pub use generation::generate_solar_system_resources;
pub use types::ResourceType;

/// Plugin that adds the economy system to the Bevy app
pub struct EconomyPlugin;

impl Plugin for EconomyPlugin {
    fn build(&self, app: &mut App) {
        app
            // Resources
            .init_resource::<GlobalBudget>()
            // Startup systems
            .add_systems(Startup, generate_solar_system_resources.after(
                // Run after solar system is set up
                crate::plugins::solar_system::setup_solar_system,
            ))
            // Update systems
            .add_systems(Update, update_civilization_score);
    }
}
