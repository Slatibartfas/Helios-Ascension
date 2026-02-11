//! Colony and Construction System
//!
//! Provides a comprehensive colony management system including:
//! - Colony establishment on celestial bodies (planets, moons, asteroids)
//! - Building construction with queue management
//! - Logistics penalty system (mass drivers, orbital lifts, cargo terminals)
//! - Population growth with housing, food and medical modifiers
//! - Integration with the global resource budget

use bevy::prelude::*;

pub mod components;
pub mod systems;
pub mod types;

pub use components::{Colony, ConstructionProject, PendingConstructionActions};
pub use systems::{advance_construction, process_construction_actions, update_colony_growth};
pub use types::{BuildingCategory, BuildingType};

/// Plugin that adds the colony and construction system to the Bevy app
pub struct ColonyPlugin;

impl Plugin for ColonyPlugin {
    fn build(&self, app: &mut App) {
        app
            // Resources
            .init_resource::<PendingConstructionActions>()
            // Update systems
            .add_systems(
                Update,
                (
                    process_construction_actions,
                    advance_construction,
                    update_colony_growth,
                )
                    .chain(),
            );
    }
}
