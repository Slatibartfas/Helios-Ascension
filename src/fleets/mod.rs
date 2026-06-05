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
    ShipInfo, SpawnFleetAction, StartTransferAction, TransferReferenceFrame,
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

#[cfg(test)]
mod fleet_thrust_tests {
    use super::types::PropulsionType;

    /// Verifies that the thrust calculation produces reasonable, unit-correct values.
    ///
    /// This test guards against the previous bug where the tonne-to-kilogram
    /// conversion was applied incorrectly, resulting in thrust figures that were
    /// three orders of magnitude too small.
    #[test]
    fn thrust_calculation_units() {
        // a 2 000-tonne frigate with a chemical engine (TWR = 10) should produce
        // roughly 2 000 × 10 × 9.81 = 196_200 kN of thrust.
        let dry = 2_000.0_f32;
        let expected = dry * 10.0 * 9.81;
        let chemical = PropulsionType::Chemical.thrust_kn(dry);
        assert!(
            (chemical - expected).abs() < 1.0,
            "chemical thrust was {} kN, expected {} kN",
            chemical,
            expected
        );

        // an ion drive on a 3 000-tonne ship should be very low thrust but still
        // correctly scaled (TWR = 0.001 → ≈ 29.43 kN).
        let dry2 = 3_000.0_f32;
        let expected2 = dry2 * 0.001 * 9.81;
        let ion = PropulsionType::IonDrive.thrust_kn(dry2);
        assert!(
            (ion - expected2).abs() < 1.0,
            "ion drive thrust was {} kN, expected {} kN",
            ion,
            expected2
        );

        // Check that every propulsion type returns a non-negative value for a
        // nominal mass; this serves as a basic sanity check.
        for prop in [
            PropulsionType::Chemical,
            PropulsionType::NuclearThermal,
            PropulsionType::IonDrive,
            PropulsionType::NuclearPulse,
            PropulsionType::FusionTorch,
        ] {
            let thrust = prop.thrust_kn(1_000.0);
            assert!(thrust >= 0.0, "negative thrust for {:?}", prop);
        }
    }
}

#[cfg(test)]
mod ga_transfer_tests {
    use super::systems::process_fleet_actions;
    use super::{
        ActiveManeuver, Fleet, FleetOrbit, PendingFleetActions, PlannedTransfer,
        StartTransferAction, TransferReferenceFrame,
    };
    use crate::astronomy::{KeplerOrbit, SpaceCoordinates};
    use crate::plugins::solar_system::CelestialBody;
    use crate::plugins::solar_system_data::BodyType;
    use crate::ui::SimulationTime;
    use bevy::prelude::*;

    /// Simple sanity check that the `PlannedTransfer` struct has a flyby field
    /// and that it can be assigned and read back.
    #[test]
    fn test_planned_transfer_records_flyby() {
        // create a raw entity value; exact index doesn't matter for unit test
        let dummy = Entity::from_bits(123);
        let pt = PlannedTransfer {
            origin_body: dummy,
            destination_body: dummy,
            reference_frame: TransferReferenceFrame::Body(dummy),
            orbit_center: dummy,
            transfer_orbit: KeplerOrbit::circular(1.0, 1.0),
            duration_s: 0.0,
            preserve_orbit_geometry: false,
            arrival_delta_v_ms: 0.0,
            arrival_orbit_radius_au: 0.0,
            fuel_cost_t: 0.0,
            option_label: "",
            start_position_au: None,
            end_position_au: None,
            departure_velocity_ms: None,
            arrival_velocity_ms: None,
            flyby_body: Some(dummy),
            leg2_orbit: None,
            leg2_start_s: 0.0,
        };

        assert_eq!(pt.flyby_body, Some(dummy));
    }

    /// Verify that `process_fleet_actions` propagates the flyby entity from a
    /// queued `StartTransferAction` into the resulting `ActiveManeuver`.
    #[test]
    fn test_active_maneuver_inherits_flyby() {
        let mut app = App::new();
        // Instead of registering the entire plugin (which drags in visual
        // systems and gizmconfig dependencies), we only insert the resources
        // needed by `process_fleet_actions` and add that system directly.
        app.insert_resource(PendingFleetActions::default());
        app.add_systems(Update, process_fleet_actions);
        // simulation time resource required by the fleet systems
        app.insert_resource(SimulationTime::new());

        // spawn a couple of dummy celestial bodies that satisfy the queries used
        // in `process_fleet_actions`.
        let origin = app
            .world_mut()
            .spawn((
                Transform::default(),
                CelestialBody {
                    name: "origin".to_string(),
                    radius: 1.0,
                    mass: 1.0,
                    body_type: BodyType::Planet,
                    visual_radius: 1.0,
                    asteroid_class: None,
                },
            ))
            .id();
        let destination = app
            .world_mut()
            .spawn((
                Transform::default(),
                CelestialBody {
                    name: "dest".to_string(),
                    radius: 1.0,
                    mass: 1.0,
                    body_type: BodyType::Planet,
                    visual_radius: 1.0,
                    asteroid_class: None,
                },
            ))
            .id();

        // create a fleet parked around the origin body
        let fleet_entity = app
            .world_mut()
            .spawn((
                Fleet::new("test".to_string()),
                FleetOrbit::new(origin, 1.0),
                // fleets also expect a SpaceCoordinates component when they exist in
                // the world, but the spawn helper above will automatically insert a
                // default one via bundle inference, so we just add one explicitly.
                SpaceCoordinates::default(),
            ))
            .id();

        // schedule a transfer with a flyby
        let flyby = origin; // arbitrary
        let planned = PlannedTransfer {
            origin_body: origin,
            destination_body: destination,
            reference_frame: TransferReferenceFrame::Body(origin),
            orbit_center: origin,
            transfer_orbit: KeplerOrbit::circular(1.0, 1.0),
            duration_s: 1.0,
            preserve_orbit_geometry: false,
            arrival_delta_v_ms: 0.0,
            arrival_orbit_radius_au: 0.0,
            fuel_cost_t: 0.0,
            option_label: "option",
            start_position_au: None,
            end_position_au: None,
            departure_velocity_ms: None,
            arrival_velocity_ms: None,
            flyby_body: Some(flyby),
            leg2_orbit: None,
            leg2_start_s: 0.0,
        };
        app.world_mut()
            .resource_mut::<PendingFleetActions>()
            .start_transfers
            .push(StartTransferAction {
                fleet: fleet_entity,
                transfer: planned.clone(),
                abort_cost_t: 0.0,
                departure_offset_s: 0.0,
            });

        // run one update tick so the action is processed
        app.update();

        // after the tick, the fleet entity should have an ActiveManeuver with the
        // same flyby body we set above
        let maneuver = app
            .world()
            .get::<ActiveManeuver>(fleet_entity)
            .expect("fleet should have maneuver");
        assert_eq!(maneuver.flyby_body, Some(flyby));
    }
}
