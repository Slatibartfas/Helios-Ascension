//! Integration tests for the GRA-153 transfer-planner HIGH/MEDIUM-severity fixes.
//!
//! Covers the 4 user-facing correctness bugs from the GRA-148 assessment:
//! - H-3: refresh `course_correction_sc` at action-queue time (not planner-open time).
//! - H-4: replace the parabolic abort-cost estimate with a real mid-flight ΔV.
//!   (Unit test in `orbital_mechanics.rs::tests::gra_153_h4_abort_dv_is_real_keplerian_not_parabolic`.)
//! - H-5: draw the two-leg gravity-assist preview, not just Leg-1.
//! - M-3: "Abort to Origin" parks the fleet at origin (not silently despawns it).
//!
//! All tests build a minimal `World` with the same body/spawn helpers used by
//! `planner_integration.rs`.  The H-3 test is a system test that runs
//! `process_fleet_actions` once and asserts the resulting `ActiveManeuver`
//! uses the live position, not the planner-open snapshot.  The M-3 test runs
//! the system with an `AbortToOriginAction` and asserts the fleet is preserved.

use bevy::math::DVec3;
use bevy::prelude::*;
use helios_ascension::astronomy::components::SystemId;
use helios_ascension::astronomy::{KeplerOrbit, SpaceCoordinates};
use helios_ascension::fleets::components::{
    AbortToOriginAction, ActiveManeuver, Fleet, PendingFleetActions, PlannedTransfer, ShipInfo,
    StartTransferAction,
};
use helios_ascension::fleets::TransferReferenceFrame;
use helios_ascension::plugins::solar_system::CelestialBody;
use helios_ascension::plugins::solar_system_data::BodyType;

// ── Test helpers ──────────────────────────────────────────────────────────────

fn test_body(name: &str, body_type: BodyType, mass: f64) -> CelestialBody {
    CelestialBody {
        name: name.to_string(),
        radius: 6_371.0,
        mass,
        body_type,
        visual_radius: 12.0,
        asteroid_class: None,
        star_approach_au: None,
    }
}

fn test_ship(name: &str, fuel_t: f32) -> ShipInfo {
    use helios_ascension::fleets::{PropulsionType, ShipClass};
    let mut info = ShipInfo::new(
        name.to_string(),
        ShipClass::ResearchVessel,
        PropulsionType::Chemical,
    );
    info.fuel_mass_t = fuel_t;
    info.max_fuel_t = fuel_t;
    info.dry_mass_t = 100.0;
    info.isp_s = 450.0;
    info
}

fn dummy_orbit(sma: f64, ecc: f64) -> KeplerOrbit {
    KeplerOrbit {
        semi_major_axis: sma,
        eccentricity: ecc,
        inclination: 0.0,
        longitude_ascending_node: 0.0,
        argument_of_periapsis: 0.0,
        mean_anomaly_epoch: 0.0,
        mean_motion: 1.0e-7,
    }
}

// ── H-3: refresh course_correction_sc at action-queue time ────────────────────

/// Build a fleet mid-transit, queue a `StartTransferAction` from the planner
/// (with a stale `start_position_au` snapshot from popup-open), then run
/// `process_fleet_actions` and assert the resulting `ActiveManeuver`'s
/// `start_position_au` matches the fleet's CURRENT position (p1), not the
/// stale snapshot (p0).
#[test]
fn gra_153_h3_refreshes_start_position_at_action_queue_time() {
    use helios_ascension::fleets::systems::process_fleet_actions;

    let mut world = World::new();
    // Sun at origin (for the barycentric reference frame).
    let _sun = world
        .spawn((
            test_body("Sol", BodyType::Star, 1.9e30),
            SpaceCoordinates::new(DVec3::ZERO),
            SystemId(0),
        ))
        .id();
    // Mars at 1.524 AU (heliocentric).
    let mars = world
        .spawn((
            test_body("Mars", BodyType::Planet, 6.4e23),
            SpaceCoordinates::new(DVec3::new(1.524, 0.0, 0.0)),
            dummy_orbit(1.524, 0.1),
            SystemId(0),
        ))
        .id();

    // Fleet mid-transit at the OLD position p0 (planner-open snapshot).
    let p0 = DVec3::new(1.0, 0.05, 0.0);
    let fleet_entity = world
        .spawn((
            Fleet::new("Test Fleet".to_string()),
            SpaceCoordinates::new(p0),
            ActiveManeuver {
                transfer_orbit: dummy_orbit(1.26, 0.2),
                reference_frame: TransferReferenceFrame::SystemBarycentric,
                orbit_center: Entity::PLACEHOLDER,
                origin_body: Entity::PLACEHOLDER,
                departure_time: 0.0,
                arrival_time: 86_400.0 * 300.0,
                preserve_orbit_geometry: false,
                destination_body: mars,
                arrival_orbit_radius_au: 0.05,
                arrival_delta_v_ms: 0.0,
                fuel_used_t: 100.0,
                option_label: "Hohmann",
                departure_angle: 0.0,
                start_position_au: Some(p0),
                end_position_au: None,
                departure_velocity_ms: None,
                arrival_velocity_ms: None,
                start_visual_pos: None,
                flyby_body: None,
                leg2_orbit: None,
                leg2_start_s: 0.0,
            },
        ))
        .id();

    // Update the fleet's SpaceCoordinates to the NEW position p1 (the
    // simulation has moved the fleet since the planner was opened).
    let p1 = DVec3::new(1.1, 0.10, 0.0);
    {
        let mut sc = world.get_mut::<SpaceCoordinates>(fleet_entity).unwrap();
        sc.position = p1;
    }

    // Build a PlannedTransfer with the stale p0 snapshot in start_position_au.
    let planned = PlannedTransfer {
        origin_body: Entity::PLACEHOLDER,
        destination_body: mars,
        reference_frame: TransferReferenceFrame::SystemBarycentric,
        orbit_center: Entity::PLACEHOLDER,
        transfer_orbit: dummy_orbit(1.26, 0.2),
        duration_s: 86_400.0 * 300.0,
        preserve_orbit_geometry: false,
        arrival_delta_v_ms: 0.0,
        arrival_orbit_radius_au: 0.05,
        fuel_cost_t: 100.0,
        option_label: "Hohmann",
        start_position_au: Some(p0),
        end_position_au: None,
        departure_velocity_ms: None,
        arrival_velocity_ms: None,
        flyby_body: None,
        leg2_orbit: None,
        leg2_start_s: 0.0,
    };
    // Queue the start_transfers action.
    let mut pending = world.resource_mut::<PendingFleetActions>();
    pending.start_transfers.push(StartTransferAction {
        fleet: fleet_entity,
        transfer: planned,
        abort_cost_t: 0.0,
        departure_offset_s: 0.0,
    });
    let _ = pending;

    // Insert SimulationTime resource.
    world.insert_resource(helios_ascension::ui::SimulationTime::new());
    {
        let mut sim_time = world.resource_mut::<helios_ascension::ui::SimulationTime>();
        sim_time.elapsed = 86_400.0 * 30.0;
    }

    // Run process_fleet_actions once.
    let mut schedule = Schedule::default();
    schedule.add_systems(process_fleet_actions);
    schedule.run(&mut world);

    // Assert the resulting ActiveManeuver has start_position_au == p1, NOT p0.
    let maneuver = world.get::<ActiveManeuver>(fleet_entity).unwrap();
    let new_start = maneuver
        .start_position_au
        .expect("GRA-153 H-3: start_position_au must be set after the fix");
    let diff_p0 = (new_start - p0).length();
    let diff_p1 = (new_start - p1).length();
    assert!(
        diff_p1 < 1e-6,
        "GRA-153 H-3: ActiveManeuver.start_position_au should match the live p1 \
         (got {:?}, p1 = {:?}, distance to p1 = {:.2e})",
        new_start,
        p1,
        diff_p1
    );
    assert!(
        diff_p0 > 1e-3,
        "GRA-153 H-3: ActiveManeuver.start_position_au should NOT match the stale p0 \
         (got {:?}, p0 = {:?}, distance to p0 = {:.2e})",
        new_start,
        p0,
        diff_p0
    );
    // Sanity: distance to p1 is much smaller than distance to p0.
    assert!(
        diff_p1 < diff_p0 / 100.0,
        "GRA-153 H-3: |new - p1| ({:.2e}) should be much smaller than |new - p0| ({:.2e})",
        diff_p1,
        diff_p0
    );
}

// ── M-3: Abort to Origin preserves the fleet entity ──────────────────────────

/// Build a fleet mid-transit with 3 ships assigned, queue an
/// `AbortToOriginAction`, run `process_fleet_actions`, and assert:
/// - the fleet entity still exists;
/// - `ActiveManeuver` is set with `destination_body == origin_body`;
/// - all 3 ships still have `assigned_fleet = Some(fleet_entity)`.
#[test]
fn gra_153_m3_abort_to_origin_preserves_fleet_and_ships() {
    use helios_ascension::fleets::components::ShipInstance;
    use helios_ascension::fleets::systems::process_fleet_actions;

    let mut world = World::new();
    let _sun = world
        .spawn((
            test_body("Sol", BodyType::Star, 1.9e30),
            SpaceCoordinates::new(DVec3::ZERO),
            SystemId(0),
        ))
        .id();
    let mars = world
        .spawn((
            test_body("Mars", BodyType::Planet, 6.4e23),
            SpaceCoordinates::new(DVec3::new(1.524, 0.0, 0.0)),
            dummy_orbit(1.524, 0.1),
            SystemId(0),
        ))
        .id();
    // Earth as origin body.
    let earth = world
        .spawn((
            test_body("Earth", BodyType::Planet, 5.97e24),
            SpaceCoordinates::new(DVec3::new(1.0, 0.0, 0.0)),
            dummy_orbit(1.0, 0.01),
            SystemId(0),
        ))
        .id();

    // Fleet mid-transit (ActiveManeuver, no FleetOrbit) at p0 = 1.2 AU.
    let mut fleet = Fleet::new("Abort Test Fleet".to_string());
    fleet.ships = vec![
        test_ship("Ship A", 500.0),
        test_ship("Ship B", 500.0),
        test_ship("Ship C", 500.0),
    ];
    let fleet_entity = world
        .spawn((
            fleet,
            SpaceCoordinates::new(DVec3::new(1.2, 0.0, 0.0)),
            ActiveManeuver {
                transfer_orbit: dummy_orbit(1.26, 0.2),
                reference_frame: TransferReferenceFrame::SystemBarycentric,
                orbit_center: Entity::PLACEHOLDER,
                origin_body: earth,
                departure_time: 0.0,
                arrival_time: 86_400.0 * 300.0,
                preserve_orbit_geometry: false,
                destination_body: mars,
                arrival_orbit_radius_au: 0.05,
                arrival_delta_v_ms: 0.0,
                fuel_used_t: 100.0,
                option_label: "Hohmann",
                departure_angle: 0.0,
                start_position_au: Some(DVec3::new(1.0, 0.0, 0.0)),
                end_position_au: None,
                departure_velocity_ms: None,
                arrival_velocity_ms: None,
                start_visual_pos: None,
                flyby_body: None,
                leg2_orbit: None,
                leg2_start_s: 0.0,
            },
        ))
        .id();

    // Create 3 ship instances assigned to the fleet.
    let ship_entities: Vec<Entity> = (0..3)
        .map(|i| {
            let si = ShipInstance::new(
                test_ship(&format!("Ship {i}"), 500.0),
                earth,
                0.0001,
                false,
                Some(fleet_entity),
                i,
            );
            world.spawn(si).id()
        })
        .collect();

    // Queue AbortToOriginAction.
    let mut pending = world.resource_mut::<PendingFleetActions>();
    pending.abort_to_origin.push(AbortToOriginAction {
        fleet: fleet_entity,
        abort_cost_t: 50.0,
    });
    let _ = pending;

    // Insert SimulationTime.
    world.insert_resource(helios_ascension::ui::SimulationTime::new());
    {
        let mut sim_time = world.resource_mut::<helios_ascension::ui::SimulationTime>();
        sim_time.elapsed = 86_400.0 * 30.0;
    }

    // Run process_fleet_actions.
    let mut schedule = Schedule::default();
    schedule.add_systems(process_fleet_actions);
    schedule.run(&mut world);

    // Assert 1: fleet entity still exists.
    assert!(
        world.get_entity(fleet_entity).is_ok(),
        "GRA-153 M-3: fleet entity should still exist after Abort to Origin"
    );

    // Assert 2: fleet has ActiveManeuver with destination_body == origin_body.
    let maneuver = world
        .get::<ActiveManeuver>(fleet_entity)
        .expect("GRA-153 M-3: ActiveManeuver should be set after Abort to Origin");
    assert_eq!(
        maneuver.destination_body, earth,
        "GRA-153 M-3: destination_body should be the origin body (Earth)"
    );
    assert_eq!(
        maneuver.origin_body, earth,
        "GRA-153 M-3: origin_body should still be Earth"
    );

    // Assert 3: all 3 ships still have assigned_fleet = Some(fleet_entity).
    for (i, ship_entity) in ship_entities.iter().enumerate() {
        let si = world
            .get::<ShipInstance>(*ship_entity)
            .unwrap_or_else(|| panic!("GRA-153 M-3: ship {i} should still exist"));
        assert_eq!(
            si.assigned_fleet,
            Some(fleet_entity),
            "GRA-153 M-3: ship {i} should still be assigned to the fleet"
        );
    }

    // Assert 4: fleet fuel was deducted by the abort cost.
    let fleet_data = world.get::<Fleet>(fleet_entity).unwrap();
    let total_fuel_after: f32 = fleet_data.ships.iter().map(|s| s.fuel_mass_t).sum();
    let total_fuel_before: f32 = 500.0 * 3.0; // 1500.0
    let per_ship_burn = 50.0 / 3.0;
    let expected = (total_fuel_before - per_ship_burn * 3.0).max(0.0);
    assert!(
        (total_fuel_after - expected).abs() < 1.0,
        "GRA-153 M-3: total fuel should be {:.1} (got {:.1}) after the abort burn",
        expected,
        total_fuel_after
    );

    // Reference is intentional: this is the Abort-to-Origin test.
}
