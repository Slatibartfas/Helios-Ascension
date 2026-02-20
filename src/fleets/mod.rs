//! Fleet management and orbital transfer system.
//!
//! Provides:
//! - Fleet ECS components (`Fleet`, `FleetOrbit`, `ActiveManeuver`)
//! - Orbital mechanics for transfer planning (Hohmann, moderate, fast options)
//! - Fleet position update and trajectory visualisation systems
//! - Integration with launch sites on colonised bodies

use bevy::prelude::*;

pub mod components;
pub mod orbital_mechanics;
pub mod systems;
pub mod types;

pub use components::{
    ActiveManeuver, Fleet, FleetOrbit, PendingFleetActions, PlannedTransfer, ShipInfo,
    SpawnFleetAction, StartTransferAction,
};
pub use orbital_mechanics::{
    calculate_transfer_options, estimate_fuel_cost_tonnes, format_delta_v, format_duration,
    hohmann_transfer, rocket_equation_fuel_fraction, TransferOption, AU_IN_METERS, GM_SUN,
};
pub use types::{PropulsionType, ShipClass};

/// Plugin that adds the fleet management system to the Bevy app.
pub struct FleetPlugin;

impl Plugin for FleetPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PendingFleetActions>()
            .add_systems(PostStartup, systems::spawn_initial_fleet)
            .add_systems(
                Update,
                (
                    systems::process_fleet_actions,
                    systems::update_fleet_orbit_positions
                        .after(systems::process_fleet_actions),
                    systems::update_fleet_maneuver_positions
                        .after(systems::process_fleet_actions),
                    systems::complete_fleet_maneuvers
                        .after(systems::update_fleet_maneuver_positions),
                    systems::draw_fleet_trajectories
                        .after(systems::update_fleet_maneuver_positions),
                    systems::draw_fleet_icons
                        .after(systems::update_fleet_orbit_positions),
                ),
            );
    }
}
