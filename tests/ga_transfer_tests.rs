use bevy::prelude::*;
use helios_ascension::astronomy::KeplerOrbit;
use helios_ascension::fleets::{
    ActiveManeuver, Fleet, FleetOrbit, PendingFleetActions, PlannedTransfer, StartTransferAction,
    TransferReferenceFrame,
};
use helios_ascension::plugins::solar_system::CelestialBody;
use helios_ascension::plugins::solar_system_data::BodyType;
use helios_ascension::ui::SimulationTime;

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
    app.add_systems(
        Update,
        helios_ascension::fleets::systems::process_fleet_actions,
    );
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
            helios_ascension::astronomy::SpaceCoordinates::default(),
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
