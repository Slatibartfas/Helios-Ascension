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
pub mod visuals;

pub use components::{
    ActiveManeuver, Fleet, FleetOrbit, MergeFleetAction, PendingFleetActions, PlannedTransfer,
    ShipInfo, SpawnFleetAction, StartTransferAction,
};
pub use orbital_mechanics::{
    apply_thrust_limits, calculate_transfer_options, calculate_transfer_options_phased,
    compute_burn_time_s, compute_transfer_window, estimate_fuel_cost_tonnes, format_delta_v,
    format_duration, hohmann_transfer, kinematic_transfer_options, phase_dv_factor,
    rocket_equation_fuel_fraction, GravityAssistOption, TransferOption, TransferWindowInfo,
    AU_IN_METERS, GM_SUN, G_CONST,
};
pub use systems::activate_scheduled_departures;
pub use types::{FleetRole, PropulsionType, ShipClass};
pub use visuals::FleetMesh;

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
                    systems::activate_scheduled_departures.after(systems::process_fleet_actions),
                    systems::update_fleet_orbit_positions
                        .after(systems::activate_scheduled_departures),
                    systems::update_fleet_maneuver_positions
                        .after(systems::activate_scheduled_departures)
                        .after(systems::update_fleet_orbit_positions),
                    systems::complete_fleet_maneuvers
                        .after(systems::update_fleet_maneuver_positions),
                    visuals::draw_fleet_trajectories
                        .after(systems::update_fleet_maneuver_positions),
                    visuals::draw_fleet_icons.after(systems::update_fleet_orbit_positions),
                    visuals::draw_fleet_starmap_icons,
                    visuals::draw_fleet_selection_reticule.after(visuals::update_fleet_transforms),
                    visuals::draw_fleet_orbit_rings.after(visuals::update_fleet_transforms),
                    visuals::draw_fleet_transfer_preview.after(visuals::update_fleet_transforms),
                    visuals::draw_gravity_assist_preview.after(visuals::update_fleet_transforms),
                    visuals::ensure_fleet_meshes,
                    visuals::update_fleet_mesh_materials.after(visuals::ensure_fleet_meshes),
                    visuals::update_fleet_transforms
                        .after(systems::update_fleet_orbit_positions)
                        .after(systems::update_fleet_maneuver_positions),
                ),
            );
    }
}
