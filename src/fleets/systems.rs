//! ECS systems for fleet position updates and action processing.

use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::time::Real;

use super::components::{ActiveManeuver, Fleet, FleetOrbit, PendingFleetActions, ShipInfo};
use super::orbital_mechanics::AU_IN_METERS;
use super::types::{PropulsionType, ShipClass};
use super::visuals::predict_body_physics_pos;
use crate::astronomy::{orbit_position_from_mean_anomaly, KeplerOrbit, SpaceCoordinates};
use crate::plugins::solar_system::{CelestialBody, LogicalParent};
use crate::plugins::solar_system_data::BodyType;
use crate::ui::{SimulationTime, TimeScale};

// ── Position update systems ───────────────────────────────────────────────────

/// One full visual revolution every 40 real seconds — readable at any time scale.
const VISUAL_ORBIT_RATE: f64 = std::f64::consts::TAU / 40.0;

/// Update `SpaceCoordinates` for every fleet in a stable parking orbit.
///
/// The visual orbital angle advances at a gameplay-friendly fixed real-time rate
/// (1 rev per 120 s) that freezes when the simulation is paused.  The
/// `SpaceCoordinates` are updated from the angle for collision/range queries,
/// but the actual render position uses the body's visual `Transform` so moon
/// orbit amplification is handled correctly.
pub fn update_fleet_orbit_positions(
    real_time: Res<Time<Real>>,
    time_scale: Res<TimeScale>,
    mut fleet_query: Query<(&mut SpaceCoordinates, &mut FleetOrbit), With<Fleet>>,
    body_coords: Query<(&SpaceCoordinates, Option<&LogicalParent>), Without<Fleet>>,
) {
    // Freeze the visual orbit when the player has paused the simulation.
    let real_delta = if time_scale.is_paused() {
        0.0
    } else {
        real_time.delta_secs_f64()
    };

    for (mut fleet_sc, mut orbit) in fleet_query.iter_mut() {
        // Advance the visual orbital angle at a slow, legible rate.
        // `orbit.direction` is +1 (CCW/prograde) or -1 (CW/retrograde) and is set
        // at insertion to match the arrival arc's tangent direction.
        // direction == 0.0 marks an LP-stationed fleet whose angle is frozen at the
        // Lagrange-point angular position — do not advance it visually.
        if orbit.direction != 0.0 {
            orbit.angle_rad = (orbit.angle_rad + orbit.direction * VISUAL_ORBIT_RATE * real_delta)
                .rem_euclid(std::f64::consts::TAU);
        }

        if let Ok((body_sc, maybe_lp)) = body_coords.get(orbit.body) {
            let offset = DVec3::new(
                orbit.radius_au * orbit.angle_rad.cos(),
                orbit.radius_au * orbit.angle_rad.sin(),
                0.0,
            );
            // If the orbit body is a moon (has a LogicalParent), its SpaceCoordinates
            // store only a local offset from its parent planet.  Add the parent's
            // heliocentric position so the fleet's SpaceCoordinates are heliocentric,
            // which is required for correct departure-direction and range queries.
            let body_helio_pos = if let Some(lp) = maybe_lp {
                body_coords
                    .get(lp.0)
                    .map(|(sc, _)| sc.position)
                    .unwrap_or(DVec3::ZERO)
                    + body_sc.position
            } else {
                body_sc.position
            };
            fleet_sc.position = body_helio_pos + offset;
        }
    }
}

/// Update `SpaceCoordinates` for every fleet actively on a transfer arc.
///
/// The fleet follows a Keplerian ellipse (`ActiveManeuver.transfer_orbit`)
/// centred on `orbit_center`, advancing analytically from the departure time.

/// Return the active Keplerian orbit and the time elapsed within that orbit
/// for a given maneuver and simulation time.
///
/// For gravity-assist transfers the fleet switches from Leg-1 (`transfer_orbit`)
/// to Leg-2 (`leg2_orbit`) after `leg2_start_s` seconds; for all other
/// transfers this always returns `(&transfer_orbit, dt)`.
fn active_orbit_at(maneuver: &ActiveManeuver, dt: f64) -> (&KeplerOrbit, f64) {
    match &maneuver.leg2_orbit {
        Some(leg2) if dt >= maneuver.leg2_start_s => (leg2, dt - maneuver.leg2_start_s),
        _ => (&maneuver.transfer_orbit, dt),
    }
}

pub fn update_fleet_maneuver_positions(
    sim_time: Res<SimulationTime>,
    mut fleet_query: Query<(&mut SpaceCoordinates, &ActiveManeuver), With<Fleet>>,
    center_coords: Query<&SpaceCoordinates, Without<Fleet>>,
) {
    let elapsed = sim_time.elapsed_seconds();

    for (mut fleet_sc, maneuver) in fleet_query.iter_mut() {
        // Skip pre-departure fleets — they are still handled by update_fleet_orbit_positions.
        if elapsed < maneuver.departure_time {
            continue;
        }

        if maneuver.is_kinematic() {
            let progress = maneuver.progress(elapsed);

            // Get origin position at departure
            let origin_pos = maneuver.start_position_au.unwrap_or_else(|| {
                center_coords
                    .get(maneuver.origin_body)
                    .map(|sc| sc.position)
                    .unwrap_or(DVec3::ZERO)
            });

            // Get destination position at arrival
            let dest_pos = maneuver.end_position_au.unwrap_or_else(|| {
                center_coords
                    .get(maneuver.destination_body)
                    .map(|sc| sc.position)
                    .unwrap_or(DVec3::ZERO)
            });

            fleet_sc.position = origin_pos + (dest_pos - origin_pos) * progress;
        } else {
            let dt = elapsed - maneuver.departure_time;

            // For gravity-assist transfers, stitch Leg-1 and Leg-2 Keplerian arcs:
            // follow transfer_orbit until leg2_start_s, then switch to leg2_orbit.
            let (active_orbit, dt_in_orbit) = active_orbit_at(maneuver, dt);

            let mean_anomaly =
                active_orbit.mean_anomaly_epoch + active_orbit.mean_motion * dt_in_orbit;

            let orbit_pos_au = orbit_position_from_mean_anomaly(active_orbit, mean_anomaly);

            let center_pos = center_coords
                .get(maneuver.orbit_center)
                .map(|sc| sc.position)
                .unwrap_or(DVec3::ZERO);

            fleet_sc.position = center_pos + orbit_pos_au;
        }
    }
}

/// Detect completed maneuvers and transition the fleet into a parking orbit
/// around its destination body.
pub fn complete_fleet_maneuvers(
    mut commands: Commands,
    sim_time: Res<SimulationTime>,
    mut fleet_query: Query<(Entity, &mut Fleet, &ActiveManeuver, &SpaceCoordinates)>,
    center_coords: Query<&SpaceCoordinates, Without<Fleet>>,
    body_type_query: Query<&CelestialBody, Without<Fleet>>,
) {
    let elapsed = sim_time.elapsed_seconds();

    for (entity, mut fleet, maneuver, fleet_sc) in fleet_query.iter_mut() {
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

        // Compute the initial parking orbit angle from the fleet's current
        // physics position relative to the destination body's centre.
        // For L3/L4/L5 transfers the destination is the star (always at DVec3::ZERO);
        // for L1/L2 transfers it is the parent planet (has real SpaceCoordinates).
        let (initial_angle, orbit_direction) = {
            let center_pos = center_coords
                .get(destination)
                .map(|sc| sc.position)
                .unwrap_or(DVec3::ZERO); // star sits at the heliocentric origin
            let rel = fleet_sc.position - center_pos;
            let pos_angle = rel.y.atan2(rel.x);

            let direction = if maneuver.is_kinematic() {
                let start_pos = maneuver.start_position_au.unwrap_or(DVec3::ZERO);
                let end_pos = maneuver.end_position_au.unwrap_or(DVec3::ZERO);
                let vel_dir = end_pos - start_pos;
                let cross_z = rel.x * vel_dir.y - rel.y * vel_dir.x;
                if cross_z >= 0.0 {
                    1.0
                } else {
                    -1.0
                }
            } else {
                // Determine whether the arrival was prograde (CCW) or retrograde (CW)
                // by computing the Keplerian velocity direction at the moment of arrival.
                // For gravity-assist transfers, use the Leg-2 orbit at arrival (if present).
                let dt = elapsed - maneuver.departure_time;
                let (arrival_orbit, dt_in_orbit) = active_orbit_at(maneuver, dt);
                let mean_anomaly_arrival =
                    arrival_orbit.mean_anomaly_epoch + arrival_orbit.mean_motion * dt_in_orbit;
                let small_dt = 1.0_f64; // 1 second step
                let ma_before = mean_anomaly_arrival - arrival_orbit.mean_motion * small_dt;
                let pos_before = orbit_position_from_mean_anomaly(arrival_orbit, ma_before);
                let pos_now = orbit_position_from_mean_anomaly(arrival_orbit, mean_anomaly_arrival);
                let vel_dir = pos_now - pos_before; // proportional to velocity
                                                    // 2-D cross product (z-component): rel × vel_dir
                let cross_z = rel.x * vel_dir.y - rel.y * vel_dir.x;
                if cross_z >= 0.0 {
                    1.0
                } else {
                    -1.0
                }
            };

            (pos_angle, direction)
        };

        // LP-stationed (L3/L4/L5) arrivals: a kinematic arc ending at a heliocentric
        // position (star destination) should freeze the fleet at the Lagrange-point
        // angular position instead of freely orbiting the Sun at 1 AU every 40 s.
        let orbit_direction = if maneuver.is_kinematic()
            && body_type_query
                .get(destination)
                .map(|b| b.body_type == BodyType::Star)
                .unwrap_or(false)
        {
            0.0 // sentinel: LP-stationed, angle frozen
        } else {
            orbit_direction
        };

        // Swap maneuver for a stable parking orbit
        let new_orbit = FleetOrbit {
            body: destination,
            radius_au,
            angle_rad: initial_angle,
            direction: orbit_direction,
        };
        commands
            .entity(entity)
            .remove::<ActiveManeuver>()
            .insert(new_orbit);
    }
}

/// Remove `FleetOrbit` from a pre-departure fleet exactly at its scheduled departure time.
///
/// When a transfer is executed with a non-zero departure offset the fleet retains its
/// `FleetOrbit` (so it keeps orbiting visually) alongside the queued `ActiveManeuver`.
/// This system detects when `elapsed >= maneuver.departure_time` and:
/// 1. Re-orients the transfer orbit's `argument_of_periapsis` to the origin body's
///    **actual** heliocentric angle at the moment of departure.  This corrects for
///    planet drift between the time the player clicked Execute and the real departure.
/// 2. Removes `FleetOrbit` so `update_fleet_maneuver_positions` takes over.
pub fn activate_scheduled_departures(
    mut commands: Commands,
    sim_time: Res<SimulationTime>,
    mut query: Query<(Entity, &FleetOrbit, &mut ActiveManeuver), With<Fleet>>,
    body_coords: Query<&SpaceCoordinates, Without<Fleet>>,
    fleet_sc_query: Query<&SpaceCoordinates, With<Fleet>>,
) {
    let elapsed = sim_time.elapsed_seconds();
    for (entity, _orbit, mut maneuver) in query.iter_mut() {
        if elapsed < maneuver.departure_time {
            continue;
        }
        // Correct the transfer-orbit orientation: the argument of periapsis must match
        // the departure position relative to the orbit center at the actual departure moment.
        // Use the fleet's own (heliocentric) SpaceCoordinates rather than the origin body's
        // SpaceCoordinates: moons only store a local offset from their parent planet, so
        // querying the moon entity directly would give the wrong departure direction.
        // For local transfers (planet <-> moon), the orbit_center is the planet whose
        // SpaceCoordinates are heliocentric, but we need planet-centric (DVec3::ZERO).
        let is_local_transfer = maneuver.orbit_center == maneuver.origin_body
            || maneuver.orbit_center == maneuver.destination_body;
        let center_pos = if is_local_transfer {
            DVec3::ZERO
        } else {
            body_coords
                .get(maneuver.orbit_center)
                .map(|sc| sc.position)
                .unwrap_or(DVec3::ZERO)
        };

        let rel_pos = if let Ok(fleet_sc) = fleet_sc_query.get(entity) {
            fleet_sc.position - center_pos
        } else if let Ok(origin_sc) = body_coords.get(maneuver.origin_body) {
            origin_sc.position - center_pos
        } else {
            DVec3::ZERO
        };

        if rel_pos.length_squared() > 1e-30 {
            let lan = maneuver.transfer_orbit.longitude_ascending_node;
            let incl = maneuver.transfer_orbit.inclination;

            if incl > 1e-10 {
                let n = bevy::math::DVec3::new(
                    incl.sin() * lan.sin(),
                    -incl.sin() * lan.cos(),
                    incl.cos(),
                );
                let node = bevy::math::DVec3::new(lan.cos(), lan.sin(), 0.0);
                let peri_dir = rel_pos.normalize_or_zero();
                let cos_w = node.dot(peri_dir);
                let sin_w = n.dot(node.cross(peri_dir));
                let omega = sin_w.atan2(cos_w);
                maneuver.transfer_orbit.argument_of_periapsis =
                    omega - maneuver.transfer_orbit.mean_anomaly_epoch;
            } else {
                let theta = rel_pos.y.atan2(rel_pos.x);
                maneuver.transfer_orbit.argument_of_periapsis =
                    theta - maneuver.transfer_orbit.mean_anomaly_epoch;
            }
        }

        // For kinematic LP transfers with a departure offset, the start_position_au
        // was set at planning time — update it to the fleet's actual physics position now.
        if maneuver.is_kinematic() && maneuver.start_position_au.is_some() {
            if let Ok(fleet_sc) = fleet_sc_query.get(entity) {
                maneuver.start_position_au = Some(fleet_sc.position);
            }
        }

        commands.entity(entity).remove::<FleetOrbit>();
    }
}

// ── Action processing ─────────────────────────────────────────────────────────

/// Process fleet actions queued by the UI in the previous frame.
pub fn process_fleet_actions(
    mut commands: Commands,
    mut actions: ResMut<PendingFleetActions>,
    sim_time: Res<SimulationTime>,
    orbit_query: Query<&FleetOrbit, With<Fleet>>,
    maneuver_query: Query<(), (With<Fleet>, With<ActiveManeuver>)>,
    mut fleet_query: Query<&mut Fleet>,
    body_query: Query<(&Transform, &CelestialBody, Option<&LogicalParent>), Without<Fleet>>,
    kepler_query: Query<&KeplerOrbit, Without<Fleet>>,
    center_coords: Query<&SpaceCoordinates, Without<Fleet>>,
    fleet_sc_query: Query<&SpaceCoordinates, With<Fleet>>,
    fleet_transform_query: Query<&Transform, With<Fleet>>,
) {
    let elapsed = sim_time.elapsed_seconds();

    // Spawn new fleets
    for action in actions.spawn_fleets.drain(..) {
        let orbit = FleetOrbit::new(action.orbit_body, action.orbit_radius_au);
        let mut fleet = Fleet::new(action.name);
        fleet.ships = action.ships;
        commands.spawn((fleet, orbit, SpaceCoordinates::default()));
        // NOTE: mesh is added lazily by ensure_fleet_meshes (needs asset access)
    }

    // Start transfers (works for both parked and in-transit fleets)
    for action in actions.start_transfers.drain(..) {
        let is_parked = orbit_query.get(action.fleet).is_ok();
        let is_in_transit = maneuver_query.get(action.fleet).is_ok();

        if !is_parked && !is_in_transit {
            continue;
        }

        // Deduct abort burn cost from fleet fuel (course corrections only)
        if action.abort_cost_t > 0.0 {
            if let Ok(mut fleet) = fleet_query.get_mut(action.fleet) {
                let per_ship = if fleet.ships.is_empty() {
                    0.0
                } else {
                    action.abort_cost_t / fleet.ships.len() as f32
                };
                for ship in fleet.ships.iter_mut() {
                    ship.fuel_mass_t = (ship.fuel_mass_t - per_ship).max(0.0);
                }
            }
        }

        let t = &action.transfer;
        let departure_s = elapsed + action.departure_offset_s;
        let arrival_s = departure_s + t.duration_s;
        let departure_angle = orbit_query
            .get(action.fleet)
            .map(|o| o.angle_rad as f32)
            .unwrap_or(0.0);

        // Course corrections (is_in_transit) for truly kinematic option types still use
        // kinematic interpolation.  Efficient/Moderate/Fast Hohmann-style options now use
        // proper Keplerian arcs — the transfer orbit elements are computed from the
        // fleet's actual position by build_planned_transfer.
        let is_kinematic = t.option_label == "Full Thrust"
            || t.option_label.contains("Coast")
            || t.option_label == "Max Speed"
            || t.option_label.contains("Direct");
        let (start_position_au, end_position_au) = if is_kinematic {
            // For course corrections (mid-transit), always use the fleet's actual current
            // physics position as departure — the pre-computed start from the planner may
            // reference a planet body (e.g. Jupiter) rather than where the fleet actually is.
            let start_pos = if is_in_transit {
                fleet_sc_query
                    .get(action.fleet)
                    .map(|sc| sc.position)
                    .unwrap_or_else(|_| {
                        // Fallback: predict origin body
                        predict_body_physics_pos(
                            t.origin_body,
                            departure_s,
                            &body_query,
                            &kepler_query,
                        )
                        .unwrap_or_else(|| {
                            center_coords
                                .get(t.origin_body)
                                .map(|sc| sc.position)
                                .unwrap_or(DVec3::ZERO)
                        })
                    })
            } else {
                t.start_position_au.unwrap_or_else(|| {
                    predict_body_physics_pos(t.origin_body, departure_s, &body_query, &kepler_query)
                        .unwrap_or_else(|| {
                            center_coords
                                .get(t.origin_body)
                                .map(|sc| sc.position)
                                .unwrap_or(DVec3::ZERO)
                        })
                })
            };

            let end_pos = t.end_position_au.unwrap_or_else(|| {
                predict_body_physics_pos(t.destination_body, arrival_s, &body_query, &kepler_query)
                    .unwrap_or_else(|| {
                        center_coords
                            .get(t.destination_body)
                            .map(|sc| sc.position)
                            .unwrap_or(DVec3::ZERO)
                    })
            });

            (Some(start_pos), Some(end_pos))
        } else {
            (None, None)
        };

        let start_visual_pos = if is_in_transit {
            fleet_transform_query
                .get(action.fleet)
                .ok()
                .map(|t| t.translation)
        } else {
            None
        };

        let maneuver = ActiveManeuver {
            transfer_orbit: t.transfer_orbit,
            orbit_center: t.orbit_center,
            origin_body: t.origin_body,
            departure_time: departure_s,
            arrival_time: arrival_s,
            destination_body: t.destination_body,
            arrival_orbit_radius_au: t.arrival_orbit_radius_au,
            arrival_delta_v_ms: t.arrival_delta_v_ms,
            fuel_used_t: t.fuel_cost_t,
            option_label: t.option_label,
            departure_angle,
            start_position_au,
            end_position_au,
            start_visual_pos,
            flyby_body: t.flyby_body,
            leg2_orbit: t.leg2_orbit,
            leg2_start_s: t.leg2_start_s,
        };
        if is_in_transit {
            // Course correction: swap immediately (no parking orbit to preserve).
            commands
                .entity(action.fleet)
                .remove::<FleetOrbit>()
                .remove::<ActiveManeuver>()
                .insert(maneuver);
        } else {
            // Parked fleet: keep FleetOrbit so the fleet continues its parking orbit
            // animation until departure_time.  `activate_scheduled_departures` will
            // remove FleetOrbit and refit the transfer-orbit geometry at departure.
            commands
                .entity(action.fleet)
                .remove::<ActiveManeuver>()
                .insert(maneuver);
            // FleetOrbit intentionally kept.
        }
    }

    // Cancel maneuvers — park the fleet in place (no orbit body available, so skip for now)
    for entity in actions.cancel_maneuvers.drain(..) {
        commands.entity(entity).remove::<ActiveManeuver>();
    }

    // Refuel fleets — fill every ship to max propellant capacity.
    // Only processes fleets in a stable orbit (not in transit).
    // In the future this will draw propellant from the orbited body's resource stockpile.
    for entity in actions.refuel_fleets.drain(..) {
        // Only refuel if the fleet is in a stable orbit, not mid-transfer.
        if orbit_query.get(entity).is_ok() {
            if let Ok(mut fleet) = fleet_query.get_mut(entity) {
                for ship in fleet.ships.iter_mut() {
                    ship.fuel_mass_t = ship.max_fuel_t;
                }
            }
        }
    }

    // Refuel individual ships — only while in a stable orbit.
    for (entity, ship_idx) in actions.refuel_ships.drain(..) {
        if orbit_query.get(entity).is_ok() {
            if let Ok(mut fleet) = fleet_query.get_mut(entity) {
                if let Some(ship) = fleet.ships.get_mut(ship_idx) {
                    ship.fuel_mass_t = ship.max_fuel_t;
                }
            }
        }
    }

    // Rename fleets
    for (entity, new_name) in actions.rename_fleets.drain(..) {
        if let Ok(mut fleet) = fleet_query.get_mut(entity) {
            fleet.name = new_name;
        }
    }

    // Change fleet roles
    for (entity, new_role) in actions.change_fleet_roles.drain(..) {
        if let Ok(mut fleet) = fleet_query.get_mut(entity) {
            fleet.role = new_role;
        }
    }

    // Transfer ships between fleets
    for action in actions.transfer_ships.drain(..) {
        // Ensure both fleets exist and are in the same location
        let source_orbit = orbit_query.get(action.source_fleet).ok().cloned();
        let dest_orbit = orbit_query.get(action.destination_fleet).ok().cloned();

        // Only allow transfer if both are parked at the same body
        if let (Some(src_orbit), Some(dst_orbit)) = (source_orbit, dest_orbit) {
            if src_orbit.body == dst_orbit.body {
                let mut despawn_source = false;
                if let Ok([mut src_fleet, mut dst_fleet]) =
                    fleet_query.get_many_mut([action.source_fleet, action.destination_fleet])
                {
                    // Sort indices in descending order so we can remove them without shifting issues
                    let mut indices = action.ship_indices.clone();
                    indices.sort_unstable_by(|a, b| b.cmp(a));

                    for idx in indices {
                        if idx < src_fleet.ships.len() {
                            let ship = src_fleet.ships.remove(idx);
                            dst_fleet.ships.push(ship);
                        }
                    }

                    if src_fleet.ships.is_empty() {
                        despawn_source = true;
                    }
                }
                if despawn_source {
                    commands.entity(action.source_fleet).despawn();
                }
            }
        }
    }

    // Scrap individual ships. If the last ship is removed, despawn the fleet.
    for (entity, ship_idx) in actions.scrap_ships.drain(..) {
        let mut despawn_fleet = false;
        if let Ok(mut fleet) = fleet_query.get_mut(entity) {
            if ship_idx < fleet.ships.len() {
                fleet.ships.remove(ship_idx);
                despawn_fleet = fleet.ships.is_empty();
            }
        }
        if despawn_fleet {
            commands.entity(entity).despawn();
        }
    }

    // Disband fleets (already confirmed by the player in the UI).
    for entity in actions.disband_fleets.drain(..) {
        if fleet_query.get(entity).is_ok() {
            commands.entity(entity).despawn();
        }
    }

    // Merge fleets: move all ships into the target fleet, despawn sources.
    // All fleets must be in orbit (not in transit) at the same body.
    for action in actions.merge_fleets.drain(..) {
        // Verify target is in a stable orbit.
        let target_body = match orbit_query.get(action.target_fleet) {
            Ok(o) => o.body,
            Err(_) => continue, // target is in transit — reject
        };
        // Verify every source is in orbit at the same body.
        let all_valid = action.source_fleets.iter().all(|&src| {
            orbit_query
                .get(src)
                .map(|o| o.body == target_body)
                .unwrap_or(false)
        });
        if !all_valid {
            continue;
        }
        // Collect all ships from sources first to satisfy the borrow checker.
        let mut collected_ships: Vec<ShipInfo> = Vec::new();
        let mut to_despawn: Vec<Entity> = Vec::new();
        for src_entity in &action.source_fleets {
            if let Ok(mut src) = fleet_query.get_mut(*src_entity) {
                collected_ships.append(&mut src.ships);
                to_despawn.push(*src_entity);
            }
        }
        if let Ok(mut target) = fleet_query.get_mut(action.target_fleet) {
            target.ships.append(&mut collected_ships);
        }
        for e in to_despawn {
            commands.entity(e).despawn();
        }
    }
}

// ── Startup ───────────────────────────────────────────────────────────────────

/// Spawn demonstration fleets at game start covering all four propulsion archetypes.
///
/// | Fleet                     | Location | Propulsion         |
/// |---------------------------|----------|--------------------|
/// | Earth Defense Squadron    | Earth    | Nuclear Thermal    |
/// | Chemical Strike Force     | Venus    | Chemical           |
/// | Ion Research Fleet        | Mars     | Ion Drive          |
/// | Fusion Expeditionary Corps| Jupiter  | Fusion Torch       |
/// | Antimatter Vanguard       | Saturn   | Antimatter Drive   |
pub fn spawn_initial_fleet(
    mut commands: Commands,
    body_query: Query<(Entity, &crate::plugins::solar_system::CelestialBody)>,
) {
    // Helper: find a body by name, log a warning if missing.
    let find_body = |name: &str| -> Option<Entity> {
        body_query
            .iter()
            .find(|(_, b)| b.name == name)
            .map(|(e, _)| e)
    };

    // ── Earth Defense Squadron (Nuclear Thermal, Earth orbit) ─────────────────
    if let Some(earth) = find_body("Earth") {
        let radius_au = 6_771.0_f64 * 1_000.0 / AU_IN_METERS;
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
        commands.spawn((
            fleet,
            FleetOrbit::new(earth, radius_au),
            SpaceCoordinates::default(),
        ));
    } else {
        bevy::log::warn!("spawn_initial_fleet: Earth not found");
    }

    // ── Chemical Strike Force (Chemical, Venus orbit) ─────────────────────────
    if let Some(venus) = find_body("Venus") {
        // Venus radius ≈ 6052 km; 400 km altitude orbit
        let radius_au = 6_452.0_f64 * 1_000.0 / AU_IN_METERS;
        let mut fleet = Fleet::new("Chemical Strike Force".to_string());
        fleet.ships.push(ShipInfo::new(
            "CSV Pyrrhus".to_string(),
            ShipClass::Frigate,
            PropulsionType::Chemical,
        ));
        fleet.ships.push(ShipInfo::new(
            "CSV Ares".to_string(),
            ShipClass::Frigate,
            PropulsionType::Chemical,
        ));
        fleet.ships.push(ShipInfo::new(
            "CSV Hammer".to_string(),
            ShipClass::Destroyer,
            PropulsionType::Chemical,
        ));
        commands.spawn((
            fleet,
            FleetOrbit::new(venus, radius_au),
            SpaceCoordinates::default(),
        ));
    } else {
        bevy::log::warn!("spawn_initial_fleet: Venus not found");
    }

    // ── Ion Research Fleet (Ion Drive, Mars orbit) ────────────────────────────
    if let Some(mars) = find_body("Mars") {
        // Mars radius ≈ 3390 km; 400 km altitude orbit
        let radius_au = 3_790.0_f64 * 1_000.0 / AU_IN_METERS;
        let mut fleet = Fleet::new("Ion Research Fleet".to_string());
        fleet.ships.push(ShipInfo::new(
            "IRS Odyssey".to_string(),
            ShipClass::ResearchVessel,
            PropulsionType::IonDrive,
        ));
        fleet.ships.push(ShipInfo::new(
            "IRS Pathfinder".to_string(),
            ShipClass::Freighter,
            PropulsionType::IonDrive,
        ));
        commands.spawn((
            fleet,
            FleetOrbit::new(mars, radius_au),
            SpaceCoordinates::default(),
        ));
    } else {
        bevy::log::warn!("spawn_initial_fleet: Mars not found");
    }

    // ── Fusion Expeditionary Corps (Fusion Torch, Jupiter orbit) ─────────────
    if let Some(jupiter) = find_body("Jupiter") {
        // Jupiter radius ≈ 71 492 km; 5 000 km altitude orbit
        let radius_au = 76_492.0_f64 * 1_000.0 / AU_IN_METERS;
        let mut fleet = Fleet::new("Fusion Expeditionary Corps".to_string());
        fleet.ships.push(ShipInfo::new(
            "FEC Prometheus".to_string(),
            ShipClass::Frigate,
            PropulsionType::FusionTorch,
        ));
        fleet.ships.push(ShipInfo::new(
            "FEC Titan".to_string(),
            ShipClass::Cruiser,
            PropulsionType::FusionTorch,
        ));
        commands.spawn((
            fleet,
            FleetOrbit::new(jupiter, radius_au),
            SpaceCoordinates::default(),
        ));
    } else {
        bevy::log::warn!("spawn_initial_fleet: Jupiter not found");
    }

    // ── Antimatter Vanguard (Antimatter Drive, Saturn orbit) ──────────────────
    if let Some(saturn) = find_body("Saturn") {
        // Saturn radius ≈ 60 268 km; 5 000 km altitude orbit
        let radius_au = 65_268.0_f64 * 1_000.0 / AU_IN_METERS;
        let mut fleet = Fleet::new("Antimatter Vanguard".to_string());
        fleet.ships.push(ShipInfo::new(
            "AMV Singularity".to_string(),
            ShipClass::Destroyer,
            PropulsionType::AntimatterDrive,
        ));
        fleet.ships.push(ShipInfo::new(
            "AMV Horizon".to_string(),
            ShipClass::Frigate,
            PropulsionType::AntimatterDrive,
        ));
        commands.spawn((
            fleet,
            FleetOrbit::new(saturn, radius_au),
            SpaceCoordinates::default(),
        ));
    } else {
        bevy::log::warn!("spawn_initial_fleet: Saturn not found");
    }
}
