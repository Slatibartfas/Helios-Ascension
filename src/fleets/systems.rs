//! ECS systems for fleet position updates, trajectory rendering, and startup.

use bevy::math::DVec3;
use bevy::prelude::*;

use super::components::{ActiveManeuver, Fleet, FleetOrbit, PendingFleetActions, ShipInfo};
use super::orbital_mechanics::AU_IN_METERS;
use super::types::{PropulsionType, ShipClass};
use crate::astronomy::components::FloatingOrigin;
use crate::astronomy::{orbit_position_from_mean_anomaly, SpaceCoordinates, SCALING_FACTOR};
use crate::ui::SimulationTime;

// ── Position update systems ───────────────────────────────────────────────────

/// Update `SpaceCoordinates` for every fleet in a stable parking orbit.
///
/// The fleet's world position equals its parent body's position
/// plus a small circular offset at `FleetOrbit.radius_au`.
pub fn update_fleet_orbit_positions(
    sim_time: Res<SimulationTime>,
    mut fleet_query: Query<
        (&mut SpaceCoordinates, &mut FleetOrbit),
        (With<Fleet>, Without<ActiveManeuver>),
    >,
    body_coords: Query<&SpaceCoordinates, Without<Fleet>>,
) {
    let elapsed = sim_time.elapsed_seconds();

    for (mut fleet_sc, mut orbit) in fleet_query.iter_mut() {
        // Advance the visual orbital angle
        orbit.angle_rad = (orbit.angular_velocity * elapsed).rem_euclid(std::f64::consts::TAU);

        if let Ok(body_sc) = body_coords.get(orbit.body) {
            let offset = DVec3::new(
                orbit.radius_au * orbit.angle_rad.cos(),
                orbit.radius_au * orbit.angle_rad.sin(),
                0.0,
            );
            fleet_sc.position = body_sc.position + offset;
        }
    }
}

/// Update `SpaceCoordinates` for every fleet actively on a transfer arc.
///
/// The fleet follows a Keplerian ellipse (`ActiveManeuver.transfer_orbit`)
/// centred on `orbit_center`, advancing analytically from the departure time.
pub fn update_fleet_maneuver_positions(
    sim_time: Res<SimulationTime>,
    mut fleet_query: Query<(&mut SpaceCoordinates, &ActiveManeuver), With<Fleet>>,
    center_coords: Query<&SpaceCoordinates, Without<Fleet>>,
) {
    let elapsed = sim_time.elapsed_seconds();

    for (mut fleet_sc, maneuver) in fleet_query.iter_mut() {
        let dt = (elapsed - maneuver.departure_time).max(0.0);
        let mean_anomaly = maneuver.transfer_orbit.mean_anomaly_epoch
            + maneuver.transfer_orbit.mean_motion * dt;

        let orbit_pos_au = orbit_position_from_mean_anomaly(&maneuver.transfer_orbit, mean_anomaly);

        let center_pos = center_coords
            .get(maneuver.orbit_center)
            .map(|sc| sc.position)
            .unwrap_or(DVec3::ZERO);

        fleet_sc.position = center_pos + orbit_pos_au;
    }
}

/// Detect completed maneuvers and transition the fleet into a parking orbit
/// around its destination body.
pub fn complete_fleet_maneuvers(
    mut commands: Commands,
    sim_time: Res<SimulationTime>,
    mut fleet_query: Query<(Entity, &mut Fleet, &ActiveManeuver)>,
) {
    let elapsed = sim_time.elapsed_seconds();

    for (entity, mut fleet, maneuver) in fleet_query.iter_mut() {
        if !maneuver.is_complete(elapsed) {
            continue;
        }

        let destination = maneuver.destination_body;
        let radius_au = maneuver.arrival_orbit_radius_au;
        let fuel_used = maneuver.fuel_used_t;

        // Deduct propellant equally across ships (simplified)
        let per_ship = if fleet.ships.is_empty() {
            0.0
        } else {
            fuel_used / fleet.ships.len() as f32
        };
        for ship in fleet.ships.iter_mut() {
            ship.fuel_mass_t = (ship.fuel_mass_t - per_ship).max(0.0);
        }

        // Swap maneuver for a stable parking orbit
        let new_orbit = FleetOrbit::new(destination, radius_au);
        commands.entity(entity).remove::<ActiveManeuver>().insert(new_orbit);
    }
}

// ── Action processing ─────────────────────────────────────────────────────────

/// Process fleet actions queued by the UI in the previous frame.
pub fn process_fleet_actions(
    mut commands: Commands,
    mut actions: ResMut<PendingFleetActions>,
    sim_time: Res<SimulationTime>,
    fleet_query: Query<&FleetOrbit, (With<Fleet>, Without<ActiveManeuver>)>,
) {
    let elapsed = sim_time.elapsed_seconds();

    // Spawn new fleets
    for action in actions.spawn_fleets.drain(..) {
        let orbit = FleetOrbit::new(action.orbit_body, action.orbit_radius_au);
        let mut fleet = Fleet::new(action.name);
        fleet.ships = action.ships;
        commands.spawn((fleet, orbit, SpaceCoordinates::default()));
    }

    // Start transfers
    for action in actions.start_transfers.drain(..) {
        if fleet_query.get(action.fleet).is_ok() {
            let t = &action.transfer;
            let maneuver = ActiveManeuver {
                transfer_orbit: t.transfer_orbit,
                orbit_center: t.orbit_center,
                departure_time: elapsed,
                arrival_time: elapsed + t.duration_s,
                destination_body: t.destination_body,
                arrival_orbit_radius_au: t.arrival_orbit_radius_au,
                arrival_delta_v_ms: t.arrival_delta_v_ms,
                fuel_used_t: t.fuel_cost_t,
                option_label: t.option_label,
            };
            commands
                .entity(action.fleet)
                .remove::<FleetOrbit>()
                .insert(maneuver);
        }
    }

    // Cancel maneuvers — park the fleet in place (no orbit body available, so skip for now)
    for entity in actions.cancel_maneuvers.drain(..) {
        commands.entity(entity).remove::<ActiveManeuver>();
    }
}

// ── Rendering systems ─────────────────────────────────────────────────────────

/// Draw the planned trajectory arc for every fleet in transit using gizmos.
pub fn draw_fleet_trajectories(
    mut gizmos: Gizmos,
    fleet_query: Query<&ActiveManeuver, With<Fleet>>,
    center_coords: Query<&SpaceCoordinates, Without<Fleet>>,
    floating_origin: Option<Res<FloatingOrigin>>,
) {
    let origin_offset = floating_origin
        .as_ref()
        .map(|fo| fo.position)
        .unwrap_or(DVec3::ZERO);

    const SEGMENTS: u32 = 64;

    for maneuver in fleet_query.iter() {
        let center_pos = center_coords
            .get(maneuver.orbit_center)
            .map(|sc| sc.position)
            .unwrap_or(DVec3::ZERO);

        let total_ma_travel = maneuver.transfer_orbit.mean_motion
            * (maneuver.arrival_time - maneuver.departure_time);

        let mut prev: Option<Vec3> = None;

        for i in 0..=SEGMENTS {
            let frac = i as f64 / SEGMENTS as f64;
            let mean_anomaly =
                maneuver.transfer_orbit.mean_anomaly_epoch + total_ma_travel * frac;

            let orbit_pos = orbit_position_from_mean_anomaly(&maneuver.transfer_orbit, mean_anomaly);
            let world_au = center_pos + orbit_pos - origin_offset;
            let render_pos = Vec3::new(
                (world_au.x * SCALING_FACTOR) as f32,
                (world_au.y * SCALING_FACTOR) as f32,
                (world_au.z * SCALING_FACTOR) as f32,
            );

            if let Some(prev_pos) = prev {
                // Fade slightly toward the destination end
                let alpha = 0.8 * (1.0 - 0.4 * frac as f32);
                gizmos.line(prev_pos, render_pos, Color::srgba(0.3, 0.8, 1.0, alpha));
            }
            prev = Some(render_pos);
        }
    }
}

/// Draw a small cross marker at each fleet's current render position.
pub fn draw_fleet_icons(
    mut gizmos: Gizmos,
    fleet_query: Query<(&SpaceCoordinates, Option<&ActiveManeuver>), With<Fleet>>,
    floating_origin: Option<Res<FloatingOrigin>>,
) {
    let origin_offset = floating_origin
        .as_ref()
        .map(|fo| fo.position)
        .unwrap_or(DVec3::ZERO);

    for (sc, maybe_maneuver) in fleet_query.iter() {
        let render_du = (sc.position - origin_offset) * SCALING_FACTOR;
        let render_pos = Vec3::new(
            render_du.x as f32,
            render_du.y as f32,
            render_du.z as f32,
        );

        let color = if maybe_maneuver.is_some() {
            Color::srgba(0.3, 0.8, 1.0, 0.9) // cyan while in transit
        } else {
            Color::srgba(0.2, 0.9, 0.3, 0.9) // green while in orbit
        };

        let size = 10.0_f32;
        gizmos.line(render_pos - Vec3::X * size, render_pos + Vec3::X * size, color);
        gizmos.line(render_pos - Vec3::Y * size, render_pos + Vec3::Y * size, color);
    }
}

// ── Startup ───────────────────────────────────────────────────────────────────

/// Spawn a sample fleet in Earth's orbit at game start for demonstration.
pub fn spawn_initial_fleet(
    mut commands: Commands,
    body_query: Query<(Entity, &crate::plugins::solar_system::CelestialBody)>,
) {
    // Find Earth by name
    let Some((earth_entity, _)) = body_query.iter().find(|(_, b)| b.name == "Earth") else {
        return;
    };

    // Parking orbit at ~400 km above Earth's surface: r ≈ 6771 km = 4.52e-5 AU
    let orbit_radius_au = 6_771.0_f64 * 1_000.0 / AU_IN_METERS; // km → m → AU

    let orbit = FleetOrbit::new(earth_entity, orbit_radius_au);

    let mut fleet = Fleet::new("Earth Defense Squadron".to_string());
    fleet.ships.push(ShipInfo::new(
        "EDS Helios".to_string(),
        ShipClass::Frigate,
        PropulsionType::NuclearThermal,
    ));
    fleet.ships.push(ShipInfo::new(
        "EDS Aurora".to_string(),
        ShipClass::Destroyer,
        PropulsionType::NuclearThermal,
    ));

    commands.spawn((fleet, orbit, SpaceCoordinates::default()));
}
