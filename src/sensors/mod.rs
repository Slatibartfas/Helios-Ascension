//! Sensor system for Helios Ascension.
//!
//! Handles ship signature calculation, detection mechanics, and contact tracking.
//!
//! ## Key Concepts
//!
//! - **Signature**: A ship's detectability across thermal, EM, visual, and neutrino bands.
//! - **Sensor Suite**: Equipped sensor hardware with detection/id ranges and strength.
//! - **Contact**: A detected target with tracking percentage and identification state.
//! - **Detection Formula**: `sensor_strength / (target_signature × distance²) × time_factor`

pub mod components;
pub mod data;
pub mod systems;

pub use components::*;
pub use data::*;
pub use systems::*;

use bevy::prelude::*;
pub use crate::fleets::components::ShipInfo;

/// Plugin that adds the sensor system to the Bevy app.
pub struct SensorPlugin;

impl Plugin for SensorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SensorData>();
        app.add_systems(
            Update,
            (
                systems::update_fleet_signatures,
                systems::sensor_detection_system,
                systems::active_sensor_ping_system,
                systems::jamming_reveal_system,
                systems::sensor_sweep_system,
                systems::consume_sensor_sweep,
                systems::decoy_expiration_system,
                systems::update_contact_tracking,
            )
                .chain(),
        );
    }
}
