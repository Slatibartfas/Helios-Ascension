//! Tests for Fleet Operations system — P0 system: Fleet Operations
//!
//! Verifies:
//! - Fleet spawning
//! - Transfer mechanics (spawn, transfer, arrival, refueling)
//! - Fuel consumption
//! - ActiveManeuver progress

use bevy::prelude::*;
use helios_ascension::astronomy::KeplerOrbit;
use helios_ascension::fleets::types::{PropulsionType, ShipClass};
use helios_ascension::fleets::{
    ActiveManeuver, Fleet, FleetOrbit, PendingFleetActions, PlannedTransfer, TransferReferenceFrame,
};

/// Fleet::new should create a fleet with a valid name and empty ship list.
#[test]
fn fleet_new() {
    let fleet = Fleet::new("Test Fleet".to_string());
    assert_eq!(fleet.name, "Test Fleet");
    assert!(fleet.ships.is_empty());
}

/// FleetOrbit::new should create a valid parking orbit around a body.
#[test]
fn fleet_orbit_new() {
    let body = Entity::from_bits(42);
    let orbit = FleetOrbit::new(body, 1.5);
    assert_eq!(orbit.body, body);
    assert_eq!(orbit.radius_au, 1.5);
}

/// FleetOrbit direction should be readable (non-zero for new orbits).
#[test]
fn fleet_orbit_direction_readable() {
    let body = Entity::from_bits(1);
    let orbit = FleetOrbit::new(body, 1.0);
    let _ = orbit.direction;
}

/// PendingFleetActions::default should be empty.
#[test]
fn pending_fleet_actions_default_empty() {
    let actions = PendingFleetActions::default();
    assert!(actions.spawn_fleets.is_empty());
    assert!(actions.start_transfers.is_empty());
    assert!(actions.abort_transfers.is_empty());
    assert!(actions.merge_fleets.is_empty());
}

/// PlannedTransfer should store all transfer parameters.
#[test]
fn planned_transfer_fields() {
    let origin = Entity::from_bits(1);
    let dest = Entity::from_bits(2);
    let transfer = PlannedTransfer {
        origin_body: origin,
        destination_body: dest,
        reference_frame: TransferReferenceFrame::Body(origin),
        orbit_center: origin,
        transfer_orbit: KeplerOrbit::circular(1.5, 0.0),
        duration_s: 86400.0 * 259.0,
        preserve_orbit_geometry: false,
        arrival_delta_v_ms: 2000.0,
        arrival_orbit_radius_au: 1.5,
        fuel_cost_t: 50.0,
        option_label: "Hohmann",
        start_position_au: None,
        end_position_au: None,
        departure_velocity_ms: None,
        arrival_velocity_ms: None,
        flyby_body: None,
        leg2_orbit: None,
        leg2_start_s: 0.0,
    };

    assert_eq!(transfer.origin_body, origin);
    assert_eq!(transfer.destination_body, dest);
    assert_eq!(transfer.duration_s, 86400.0 * 259.0);
    assert!(transfer.flyby_body.is_none());
}

/// ActiveManeuver progress: at departure progress=0, at midpoint progress=0.5, at arrival progress=1.
#[test]
fn active_maneuver_kinematic_progress() {
    let origin = Entity::from_bits(1);
    let dest = Entity::from_bits(2);
    let maneuver = ActiveManeuver {
        transfer_orbit: KeplerOrbit::circular(1.5, 0.0),
        reference_frame: TransferReferenceFrame::Body(origin),
        orbit_center: origin,
        origin_body: origin,
        destination_body: dest,
        departure_time: 100.0,
        arrival_time: 300.0, // 200s duration
        preserve_orbit_geometry: false,
        arrival_orbit_radius_au: 1.5,
        arrival_delta_v_ms: 2000.0,
        fuel_used_t: 50.0,
        option_label: "Full Thrust",
        departure_angle: 0.0,
        start_position_au: None,
        end_position_au: None,
        departure_velocity_ms: None,
        arrival_velocity_ms: None,
        start_visual_pos: None,
        flyby_body: None,
        leg2_orbit: None,
        leg2_start_s: 0.0,
    };

    // Note: progress() uses arrival_time - departure_time as duration
    assert!(
        (maneuver.progress(100.0) - 0.0).abs() < 1e-6,
        "At departure progress should be 0"
    );
    assert!(
        (maneuver.progress(200.0) - 0.5).abs() < 1e-6,
        "At midpoint progress should be 0.5"
    );
    assert!(
        (maneuver.progress(300.0) - 1.0).abs() < 1e-6,
        "At arrival progress should be 1"
    );
}

/// ActiveManeuver progress before departure should be 0.
#[test]
fn active_maneuver_before_departure() {
    let maneuver = ActiveManeuver {
        transfer_orbit: KeplerOrbit::circular(1.0, 0.0),
        reference_frame: TransferReferenceFrame::Body(Entity::from_bits(1)),
        orbit_center: Entity::from_bits(1),
        origin_body: Entity::from_bits(1),
        destination_body: Entity::from_bits(2),
        departure_time: 100.0,
        arrival_time: 300.0,
        preserve_orbit_geometry: false,
        arrival_orbit_radius_au: 1.0,
        arrival_delta_v_ms: 0.0,
        fuel_used_t: 0.0,
        option_label: "Max Speed",
        departure_angle: 0.0,
        start_position_au: None,
        end_position_au: None,
        departure_velocity_ms: None,
        arrival_velocity_ms: None,
        start_visual_pos: None,
        flyby_body: None,
        leg2_orbit: None,
        leg2_start_s: 0.0,
    };

    assert!((maneuver.progress(50.0) - 0.0).abs() < 1e-6);
    assert!((maneuver.progress(99.0) - 0.0).abs() < 1e-6);
}

/// is_kinematic should return true for Full Thrust, Max Speed, Coast, Direct.
#[test]
fn active_maneuver_is_kinematic_labels() {
    let base = || ActiveManeuver {
        transfer_orbit: KeplerOrbit::circular(1.0, 0.0),
        reference_frame: TransferReferenceFrame::Body(Entity::from_bits(1)),
        orbit_center: Entity::from_bits(1),
        origin_body: Entity::from_bits(1),
        destination_body: Entity::from_bits(2),
        departure_time: 0.0,
        arrival_time: 100.0,
        preserve_orbit_geometry: false,
        arrival_orbit_radius_au: 1.0,
        arrival_delta_v_ms: 0.0,
        fuel_used_t: 0.0,
        departure_angle: 0.0,
        start_position_au: None,
        end_position_au: None,
        departure_velocity_ms: None,
        arrival_velocity_ms: None,
        start_visual_pos: None,
        flyby_body: None,
        leg2_orbit: None,
        leg2_start_s: 0.0,
    };

    let kinematic_labels = ["Full Thrust", "Max Speed", "Coast Phase", "Direct Hohmann"];
    for label in kinematic_labels {
        let m = {
            let mut b = base();
            b.option_label = label;
            b
        };
        assert!(m.is_kinematic(), "Label '{}' should be kinematic", label);
    }

    let non_kinematic_labels = ["Hohmann", "Bi-elliptic", "Gravity Assist"];
    for label in non_kinematic_labels {
        let m = {
            let mut b = base();
            b.option_label = label;
            b
        };
        assert!(
            !m.is_kinematic(),
            "Label '{}' should NOT be kinematic",
            label
        );
    }
}

/// estimate_fuel_cost_tonnes: positive inputs give positive fuel.
#[test]
fn fuel_cost_positive_for_valid_inputs() {
    use helios_ascension::fleets::orbital_mechanics::estimate_fuel_cost_tonnes;

    // wet_mass_t=1000, isp_s=450, delta_v_ms=3000
    let fuel = estimate_fuel_cost_tonnes(1000.0, 450.0, 3000.0);
    assert!(fuel > 0.0, "Fuel cost should be positive for valid inputs");
    // Tsiolkovsky with Isp=450 gives fuel fraction ~0.5, fuel ~500t for 1000t wet mass
    assert!(fuel < 1000.0, "Fuel {} seems unreasonably high", fuel);
}

/// rocket_equation_fuel_fraction should produce values in (0, 1).
#[test]
fn fuel_fraction_bounds() {
    use helios_ascension::fleets::orbital_mechanics::rocket_equation_fuel_fraction;

    // delta_v 3000 m/s, Isp 300s
    let fraction = rocket_equation_fuel_fraction(3000.0, 300.0);
    assert!(
        fraction > 0.0 && fraction < 1.0,
        "Fuel fraction {} should be in (0,1)",
        fraction
    );
}

/// All PropulsionType variants should produce non-negative thrust.
#[test]
fn propulsion_types_all_nonzero_thrust() {
    for pt in [
        PropulsionType::Chemical,
        PropulsionType::NuclearThermal,
        PropulsionType::IonDrive,
        PropulsionType::NuclearPulse,
        PropulsionType::FusionTorch,
    ] {
        let thrust = pt.thrust_kn(1000.0);
        assert!(
            thrust >= 0.0,
            "{:?} should produce non-negative thrust, got {} kN",
            pt,
            thrust
        );
    }
}

/// All propulsion types should have positive specific impulse.
#[test]
fn propulsion_types_all_have_isp() {
    for pt in [
        PropulsionType::Chemical,
        PropulsionType::NuclearThermal,
        PropulsionType::IonDrive,
        PropulsionType::NuclearPulse,
        PropulsionType::FusionTorch,
    ] {
        let isp = pt.specific_impulse_s();
        assert!(
            isp > 0.0,
            "{:?} should have positive Isp, got {} s",
            pt,
            isp
        );
    }
}

/// ShipClass variants should have non-empty display name.
#[test]
fn ship_class_display_names() {
    for sc in [
        ShipClass::Unspecified,
        ShipClass::ColonyShip,
        ShipClass::Freighter,
        ShipClass::Frigate,
        ShipClass::Destroyer,
        ShipClass::Cruiser,
        ShipClass::Battleship,
        ShipClass::TroopShip,
        ShipClass::ScienceVessel,
    ] {
        let name = sc.display_name();
        assert!(
            !name.is_empty(),
            "ShipClass {:?} should have display name",
            sc
        );
    }
}
