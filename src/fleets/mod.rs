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
pub use systems::{
    activate_scheduled_departures, FleetMesh,
};
pub use orbital_mechanics::{
    apply_thrust_limits, brachistochrone_option, calculate_transfer_options,
    calculate_transfer_options_phased, compute_burn_time_s, compute_transfer_window,
    estimate_fuel_cost_tonnes, format_delta_v, format_duration,
    hohmann_transfer, phase_dv_factor, rocket_equation_fuel_fraction,
    GravityAssistOption, TransferOption, TransferWindowInfo,
    AU_IN_METERS, G_CONST, GM_SUN,
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
                    systems::activate_scheduled_departures
                        .after(systems::process_fleet_actions),
                    systems::update_fleet_orbit_positions
                        .after(systems::activate_scheduled_departures),
                    systems::update_fleet_maneuver_positions
                        .after(systems::activate_scheduled_departures)
                        .after(systems::update_fleet_orbit_positions),
                    systems::complete_fleet_maneuvers
                        .after(systems::update_fleet_maneuver_positions),
                    systems::draw_fleet_trajectories
                        .after(systems::update_fleet_maneuver_positions),
                    systems::draw_fleet_icons
                        .after(systems::update_fleet_orbit_positions),
                    systems::draw_fleet_starmap_icons,
                    systems::draw_fleet_selection_reticule
                        .after(systems::update_fleet_transforms),
                    systems::draw_fleet_orbit_rings
                        .after(systems::update_fleet_transforms),
                    systems::draw_fleet_transfer_preview
                        .after(systems::update_fleet_transforms),
                    systems::draw_gravity_assist_preview
                        .after(systems::update_fleet_transforms),
                    systems::ensure_fleet_meshes,
                    systems::update_fleet_transforms
                        .after(systems::update_fleet_orbit_positions)
                        .after(systems::update_fleet_maneuver_positions),
                ),
            );
    }
}
