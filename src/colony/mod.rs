//! Colony and Construction System
//!
//! Provides a comprehensive colony management system including:
//! - Colony establishment on celestial bodies (planets, moons, asteroids)
//! - Building construction with queue management and resource costs
//! - Logistics penalty system (mass drivers, orbital lifts, cargo terminals)
//! - Population growth with housing, food and medical modifiers
//! - Data-driven building definitions loaded from assets/data/buildings.ron
//! - Integration with the global resource budget
//! - F12 debug menu for development

use bevy::prelude::*;

pub mod components;
pub mod data;
pub mod systems;
pub mod types;

pub use components::{Colony, ConstructionProject, PendingConstructionActions};
pub use data::{BuildingDefinition, BuildingsData};
pub use systems::{advance_construction, process_construction_actions, update_colony_growth, update_treasury};
pub use types::{BuildingCategory, BuildingType};

/// Debug settings for construction system (toggled with F12 on Construction menu)
#[derive(Resource, Debug, Clone)]
pub struct ConstructionDebugSettings {
    /// Whether debug mode is enabled
    pub enabled: bool,
    /// Free construction: bypass resource costs
    pub free_construction: bool,
    /// Instant build: complete construction immediately
    pub instant_build: bool,
    /// Bypass tech prerequisites
    pub bypass_tech_requirements: bool,
}

impl Default for ConstructionDebugSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            free_construction: false,
            instant_build: false,
            bypass_tech_requirements: false,
        }
    }
}

/// Plugin that adds the colony and construction system to the Bevy app
pub struct ColonyPlugin;

impl Plugin for ColonyPlugin {
    fn build(&self, app: &mut App) {
        app
            // Resources
            .init_resource::<PendingConstructionActions>()
            .init_resource::<ConstructionDebugSettings>()
            // Startup systems
            .add_systems(Startup, data::load_buildings)
            // Update systems
            .add_systems(
                Update,
                (
                    process_construction_actions,
                    advance_construction,
                    update_colony_growth,
                    update_treasury,
                    systems::deduct_maintenance_resources,
                )
                    .chain(),
            );
    }
}
