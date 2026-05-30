//! Victory conditions system — detection and endgame state management.
//!
//! Tracks progress toward four victory conditions:
//! - **Scientific**: complete all technologies in the tech tree
//! - **Military**: capture all AI faction homeworlds or eliminate all factions
//! - **Economic**: achieve GDP and trade volume thresholds
//! - **Diplomatic**: form alliance with all AI factions simultaneously
//!
//! Also handles partial victory (first-to-achieve wins) and the endgame
//! screen with campaign statistics.

use bevy::prelude::*;

// Re-export for use by other modules
pub use self::types::{VictoryState, VictoryType};

pub mod types;
pub mod systems;

/// Plugin that registers the victory condition system
pub struct VictoryPlugin;

impl Plugin for VictoryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VictoryState>()
            .add_systems(
                Update, // Runs every frame — victory check is cheap
                systems::check_victory_conditions,
            );
    }
}