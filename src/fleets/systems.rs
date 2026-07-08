//! ECS systems for fleet position updates and action processing.

use std::collections::HashMap;

use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::time::Real;

use super::components::{
    ActiveManeuver, Fleet, FleetOrbit, PendingFleetActions, ShipInfo, ShipInstance,
    TransferPlan,
};
use super::orbital_mechanics::AU_IN_METERS;
use super::types::{PropulsionType, ShipClass};
use super::visuals::predict_body_physics_pos;
use crate::astronomy::{orbit_position_from_mean_anomaly, KeplerOrbit, SpaceCoordinates};
use crate::plugins::solar_system::{CelestialBody, LogicalParent};
use crate::plugins::solar_system_data::BodyType;
use crate::shipbuilding::ShipbuildingData;
use crate::ships::{
    freighter_cargo_capacity_t_for_components, FreighterSlots, FreighterTemplateRegistry,
    ShipTemplateRef,
};
use crate::ui::{SimulationTime, TimeScale};

// ── Position update systems ───────────────────────────────────────────────────

/// One full visual revolution every 40 real seconds — readable at any time scale.
/// The rate is **independent of the simulation time scale** so the fleet icon
/// doesn't blur past the player at high game speeds (1 year/s would otherwise
/// scale the visual orbit to >3000 rev/min — unusable UX) and doesn't crawl
/// when the simulation is paused.
///
/// 1 rev / 40 s is fast enough to read at 60 fps (~9°/frame on a 30 fps loop)
/// but slow enough that a 50-pixel orbit ring is traversed at ~13 px/sec —
/// readable, not strobing.  The fleet's analytical `SpaceCoordinates` are still
/// computed from this angle for collision/range queries, while the actual
/// render position uses the body's visual `Transform` so moon orbit
/// amplification is handled correctly.
const VISUAL_ORBIT_RATE: f64 = std::f64::consts::TAU / 40.0;

fn ordered_ship_entities_for_fleet(
    ships: &Query<(Entity, &ShipInstance)>,
    fleet_entity: Entity,
) -> Vec<Entity> {
    let mut rows: Vec<_> = ships
        .iter()
        .filter(|(_, ship)| ship.assigned_fleet == Some(fleet_entity))
        .map(|(entity, ship)| (entity, ship.sort_order, ship.info.name.clone()))
        .collect();
    rows.sort_by(|left, right| left.1.cmp(&right.1).then_with(|| left.2.cmp(&right.2)));
    rows.into_iter().map(|(entity, _, _)| entity).collect()
}

fn next_sort_order_for_fleet(ships: &Query<(Entity, &ShipInstance)>, fleet_entity: Entity) -> i32 {
    ships
        .iter()
        .filter(|(_, ship)| ship.assigned_fleet == Some(fleet_entity))
        .map(|(_, ship)| ship.sort_order)
        .max()
        .unwrap_or(-1)
        + 1
}

fn fleet_has_assigned_ships(ships: &Query<(Entity, &ShipInstance)>, fleet_entity: Entity) -> bool {
    ships
        .iter()
        .any(|(_, ship)| ship.assigned_fleet == Some(fleet_entity))
}

fn spawn_fleet_with_ship_entities(
    commands: &mut Commands,
    name: String,
    ships: Vec<ShipInfo>,
    orbit_body: Entity,
    orbit_radius_au: f64,
    stationary: bool,
) -> Entity {
    let mut orbit = FleetOrbit::new(orbit_body, orbit_radius_au);
    if stationary {
        orbit.direction = 0.0;
    }

    let mut fleet = Fleet::new(name);
    fleet.ships = ships.clone();
    let fleet_entity = commands
        .spawn((fleet, orbit, SpaceCoordinates::default()))
        .id();

    for (index, ship) in ships.into_iter().enumerate() {
        commands.spawn(ShipInstance::new(
            ship,
            orbit_body,
            orbit_radius_au,
            stationary,
            Some(fleet_entity),
            index as i32,
        ));
    }

    fleet_entity
}

pub fn sync_fleet_cache_from_ship_entities(
    ships: Query<(
        Entity,
        &ShipInstance,
        Option<&ShipTemplateRef>,
        Option<&FreighterSlots>,
    )>,
    mut fleets: Query<(Entity, &mut Fleet)>,
    registry: Res<FreighterTemplateRegistry>,
    shipbuilding_data: Res<ShipbuildingData>,
) {
    let mut grouped: HashMap<Entity, Vec<(i32, String, ShipInfo)>> = HashMap::new();

    for (_, ship, template_ref, freighter_slots) in ships.iter() {
        let Some(fleet_entity) = ship.assigned_fleet else {
            continue;
        };

        let mut info = ship.as_ship_info();
        // GRA-119: stamp `cargo_capacity_t` from the ship's resolved
        // template + slot list.  Non-freighter entities (no
        // `ShipTemplateRef` or no `FreighterSlots`) keep the default 0.0
        // — the dispatch sites use the fleet-level getter which sums to
        // zero for those ships, so the cap never applies.  This means
        // pre-migration saves / hand-spawned test freighters without
        // the template components degrade gracefully (the auto-freight
        // loop simply won't pick them up — same as today's behaviour
        // for non-freighter classes).
        if let (Some(template_ref), Some(slots)) = (template_ref, freighter_slots) {
            info.cargo_capacity_t = freighter_cargo_capacity_t_for_components(
                &registry,
                &shipbuilding_data,
                template_ref,
                slots,
            );
        }

        grouped.entry(fleet_entity).or_default().push((
            ship.sort_order,
            ship.info.name.clone(),
            info,
        ));
    }

    for (fleet_entity, mut fleet) in fleets.iter_mut() {
        let mut ships_for_fleet = grouped.remove(&fleet_entity).unwrap_or_default();
        ships_for_fleet
            .sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
        fleet.ships = ships_for_fleet
            .into_iter()
            .map(|(_, _, ship_info)| ship_info)
            .collect();
    }
}

pub fn sync_ship_instance_locations(
    fleets: Query<(Option<&FleetOrbit>, Option<&ActiveManeuver>), With<Fleet>>,
    mut ships: Query<&mut ShipInstance>,
) {
    for mut ship in ships.iter_mut() {
        let Some(fleet_entity) = ship.assigned_fleet else {
            continue;
        };

        match fleets.get(fleet_entity) {
            Ok((Some(orbit), _)) => {
                ship.parked_body = orbit.body;
                ship.parked_orbit_radius_au = orbit.radius_au;
                ship.stationary = orbit.direction == 0.0;
            }
            Ok((None, Some(_))) => {}
            Ok((None, None)) | Err(_) => {
                ship.assigned_fleet = None;
            }
        }
    }
}

/// Update `SpaceCoordinates` for every fleet in a stable parking orbit.
///
/// The visual orbital angle advances at a **constant** real-time rate
/// (1 rev per `VISUAL_ORBIT_RATE` — currently 60 s) and freezes when
/// the simulation is paused.  The rate is **independent of the
/// simulation time scale** so the fleet icon doesn't blur past the
/// player at high game speeds (1 year/s would otherwise scale the
/// visual orbit to >3000 rev/min — unusable UX).  The previous
/// behaviour scaled the parking-orbit rate by `time_scale.scale` and
/// applied the same logarithmic cap used for orbital bodies, which
/// produced a fast, strobing fleet icon at every game-speed tier
/// above the visual-speed base.
///
/// The `SpaceCoordinates` are updated from the angle for collision /
/// range queries, but the actual render position uses the body's
/// visual `Transform` so moon orbit amplification is handled
/// correctly.
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
        // Advance the visual orbital angle at the constant
        // `VISUAL_ORBIT_RATE` (no time-scale multiplier, no logarithmic
        // cap — the fleet icon must move at the same readable pace at
        // every game speed).
        //
        // `orbit.direction` is +1 (CCW/prograde) or -1 (CW/retrograde)
        // and is set at insertion to match the arrival arc's tangent
        // direction.  direction == 0.0 marks an LP-stationed fleet
        // whose angle is frozen.
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

            let center_pos = if maneuver.reference_frame.is_barycentric() {
                DVec3::ZERO
            } else {
                center_coords
                    .get(maneuver.orbit_center)
                    .map(|sc| sc.position)
                    .unwrap_or(DVec3::ZERO)
            };

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
    body_types: Query<&CelestialBody, Without<Fleet>>,
    fleet_sc_query: Query<&SpaceCoordinates, With<Fleet>>,
) {
    let elapsed = sim_time.elapsed_seconds();
    for (entity, _orbit, mut maneuver) in query.iter_mut() {
        if elapsed < maneuver.departure_time {
            continue;
        }
        if maneuver.preserve_orbit_geometry {
            if maneuver.is_kinematic() && maneuver.start_position_au.is_some() {
                if let Ok(fleet_sc) = fleet_sc_query.get(entity) {
                    maneuver.start_position_au = Some(fleet_sc.position);
                }
            }

            commands.entity(entity).remove::<FleetOrbit>();
            continue;
        }
        // Correct the transfer-orbit orientation: the argument of periapsis must match
        // the departure position relative to the orbit center at the actual departure moment.
        // Use the fleet's own (heliocentric) SpaceCoordinates rather than the origin body's
        // SpaceCoordinates: moons only store a local offset from their parent planet, so
        // querying the moon entity directly would give the wrong departure direction.
        // For local transfers (planet <-> moon), the orbit_center is the planet whose
        // SpaceCoordinates are heliocentric, but we need planet-centric (DVec3::ZERO).
        let center_pos = match maneuver.reference_frame {
            crate::fleets::TransferReferenceFrame::SystemBarycentric => DVec3::ZERO,
            crate::fleets::TransferReferenceFrame::Body(center_entity) => {
                let is_local_transfer = center_entity == maneuver.origin_body
                    || center_entity == maneuver.destination_body;
                let orbit_center_is_star = body_types
                    .get(center_entity)
                    .map(|body| body.body_type == BodyType::Star)
                    .unwrap_or(false);
                if is_local_transfer && !orbit_center_is_star {
                    DVec3::ZERO
                } else {
                    body_coords
                        .get(center_entity)
                        .map(|sc| sc.position)
                        .unwrap_or(DVec3::ZERO)
                }
            }
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
    // GRA-153 M-3: typed access to `ActiveManeuver` for the Abort-to-Origin
    // handler.  Separate from the existing `maneuver_query` (which is
    // `Query<(), ...>` used for the parked/in-transit boolean check).
    active_maneuver_query: Query<&ActiveManeuver, With<Fleet>>,
    mut ship_queries: ParamSet<(
        Query<(Entity, &ShipInstance)>,
        Query<(Entity, &mut ShipInstance)>,
    )>,
) {
    let elapsed = sim_time.elapsed_seconds();

    // Spawn new fleets
    for action in actions.spawn_fleets.drain(..) {
        spawn_fleet_with_ship_entities(
            &mut commands,
            action.name,
            action.ships,
            action.orbit_body,
            action.orbit_radius_au,
            action.stationary,
        );
    }

    for action in actions.create_fleets_from_ships.drain(..) {
        let fleet_entity = spawn_fleet_with_ship_entities(
            &mut commands,
            action.name,
            Vec::new(),
            action.orbit_body,
            action.orbit_radius_au,
            action.stationary,
        );

        for (index, ship_entity) in action.ship_entities.into_iter().enumerate() {
            if let Ok((_, mut ship)) = ship_queries.p1().get_mut(ship_entity) {
                ship.assigned_fleet = Some(fleet_entity);
                ship.parked_body = action.orbit_body;
                ship.parked_orbit_radius_au = action.orbit_radius_au;
                ship.stationary = action.stationary;
                ship.sort_order = index as i32;
            }
        }
    }

    for action in actions.assign_ships.drain(..) {
        let source_fleets: Vec<_> = action
            .ship_entities
            .iter()
            .filter_map(|entity| {
                ship_queries
                    .p0()
                    .get(*entity)
                    .ok()
                    .and_then(|(_, ship)| ship.assigned_fleet)
            })
            .collect();

        let destination_state = action.destination_fleet.and_then(|fleet_entity| {
            orbit_query.get(fleet_entity).ok().map(|orbit| {
                (
                    fleet_entity,
                    orbit.body,
                    orbit.radius_au,
                    orbit.direction == 0.0,
                    next_sort_order_for_fleet(&ship_queries.p0(), fleet_entity),
                )
            })
        });

        let mut next_sort_order = destination_state.map(|(_, _, _, _, sort_order)| sort_order);

        for ship_entity in action.ship_entities {
            if let Ok((_, mut ship)) = ship_queries.p1().get_mut(ship_entity) {
                if let Some((fleet_entity, body, radius_au, stationary, _)) = destination_state {
                    ship.assigned_fleet = Some(fleet_entity);
                    ship.parked_body = body;
                    ship.parked_orbit_radius_au = radius_au;
                    ship.stationary = stationary;
                    ship.sort_order = next_sort_order.unwrap_or(0);
                    next_sort_order = next_sort_order.map(|value| value + 1);
                } else {
                    ship.assigned_fleet = None;
                }
            }
        }

        for source_fleet in source_fleets {
            if action.destination_fleet == Some(source_fleet) {
                continue;
            }
            if !fleet_has_assigned_ships(&ship_queries.p0(), source_fleet) {
                commands.entity(source_fleet).despawn();
            }
        }
    }

    // Start transfers (works for both parked and in-transit fleets)
    for action in actions.start_transfers.drain(..) {
        if let Ok(fleet) = fleet_query.get(action.fleet) {
            if fleet
                .ships
                .iter()
                .any(|ship| ship.class == ShipClass::Station)
            {
                continue;
            }
        }

        let is_parked = orbit_query.get(action.fleet).is_ok();
        let is_in_transit = maneuver_query.get(action.fleet).is_ok();

        if !is_parked && !is_in_transit {
            continue;
        }

        // Deduct abort burn cost from fleet fuel (course corrections only)
        if action.abort_cost_t > 0.0 {
            let per_ship_abort_cost = if let Ok(fleet) = fleet_query.get(action.fleet) {
                if fleet.ships.is_empty() {
                    0.0
                } else {
                    action.abort_cost_t / fleet.ships.len() as f32
                }
            } else {
                0.0
            };
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
            for (_, mut ship) in ship_queries.p1().iter_mut() {
                if ship.assigned_fleet == Some(action.fleet) {
                    ship.info.fuel_mass_t = (ship.info.fuel_mass_t - per_ship_abort_cost).max(0.0);
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
        //
        // GRA-153 H-3 (Kilo CRITICAL 2): mid-transit course corrections must
        // ALSO re-anchor the transfer orbit, not just `start_position_au`.
        // For a non-kinematic Keplerian propagation, `update_fleet_maneuver_positions`
        // follows `transfer_orbit` from its current state and ignores
        // `start_position_au` — so refreshing only the start position is
        // decorative when the planner built a Keplerian orbit from a stale
        // snapshot.  Force in-transit course corrections to be kinematic so
        // the propagation actually uses the refreshed start.
        let is_kinematic = t.option_label == "Full Thrust"
            || t.option_label.contains("Coast")
            || t.option_label == "Max Speed"
            || t.option_label.contains("Direct")
            || is_in_transit;
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
            reference_frame: t.reference_frame,
            orbit_center: t.orbit_center,
            origin_body: t.origin_body,
            departure_time: departure_s,
            arrival_time: arrival_s,
            preserve_orbit_geometry: t.preserve_orbit_geometry,
            destination_body: t.destination_body,
            arrival_orbit_radius_au: t.arrival_orbit_radius_au,
            arrival_delta_v_ms: t.arrival_delta_v_ms,
            fuel_used_t: t.fuel_cost_t,
            option_label: t.option_label,
            // GRA-153 H-3 (Kilo CRITICAL 2): force kinematic propagation for
            // in-transit course corrections so the refreshed start/end
            // positions actually drive fleet position.
            kinematic_override: is_in_transit,
            departure_angle,
            start_position_au,
            end_position_au,
            departure_velocity_ms: t.departure_velocity_ms,
            arrival_velocity_ms: t.arrival_velocity_ms,
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

    // GRA-153 M-3: "Abort to Origin" — overwrite the active maneuver with a
    // return-to-origin transfer.  The fleet entity, its ships' `assigned_fleet`,
    // and the visible render position are all preserved (only `ActiveManeuver`
    // is replaced).  This avoids the silent despawn that the legacy
    // `cancel_maneuvers` path produced when the resulting fleet had neither
    // `FleetOrbit` nor `ActiveManeuver`.
    for action in actions.abort_to_origin.drain(..) {
        // Skip if the fleet is no longer in transit (e.g. the maneuver already
        // completed between action-queue and process-tick).
        if maneuver_query.get(action.fleet).is_err() {
            continue;
        }

        // Deduct the abort burn fuel cost (same per-ship split as
        // `start_transfers`).
        if action.abort_cost_t > 0.0 {
            let per_ship_abort = if let Ok(fleet) = fleet_query.get(action.fleet) {
                if fleet.ships.is_empty() {
                    0.0
                } else {
                    action.abort_cost_t / fleet.ships.len() as f32
                }
            } else {
                0.0
            };
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
            for (_, mut ship) in ship_queries.p1().iter_mut() {
                if ship.assigned_fleet == Some(action.fleet) {
                    ship.info.fuel_mass_t = (ship.info.fuel_mass_t - per_ship_abort).max(0.0);
                }
            }
        }

        // Look up the current maneuver via a fresh typed access.  We can't
        // reuse the existing `maneuver_query` (it's `Query<(), ...>`) so we
        // re-query via the typed `active_maneuver_query` added to the system
        // params below.
        let Some(current_man) = active_maneuver_query.get(action.fleet).ok() else {
            continue;
        };

        // The fleet's current heliocentric position is the abort start.
        let start_pos = fleet_sc_query
            .get(action.fleet)
            .map(|sc| sc.position)
            .unwrap_or(DVec3::ZERO);
        // Origin body parking radius — prefer a body-type-aware default.
        // GRA-149 (C-2) added `star_approach_au` for stars; for non-stellar
        // bodies we use a conservative low-orbit default (0.001 AU ≈ 150k km,
        // well inside the SOI of any planet).  GRA-153 Kilo WARNING: the
        // destination must be the ORIGIN body (where the fleet started), not
        // the orbit center (typically the Sun).
        let origin_body = current_man.origin_body;
        let parking_r_au = body_query
            .get(origin_body)
            .map(|(_, body, _)| match body.body_type {
                BodyType::Star => body.star_approach_au.unwrap_or(0.3),
                BodyType::Planet | BodyType::GasGiant | BodyType::DwarfPlanet => 0.001_f64,
                BodyType::Moon => 0.0001_f64,
                BodyType::Asteroid | BodyType::Comet | BodyType::Ring => 0.0005_f64,
            })
            .unwrap_or(0.001_f64);
        let origin_pos = center_coords
            .get(origin_body)
            .map(|sc| sc.position)
            .unwrap_or(DVec3::ZERO);
        // Park the abort destination at `origin_pos + offset` where the
        // offset is `parking_r_au` along the heliocentric radial direction
        // (away from the Sun, so it doesn't end up inside the star).
        let radial = if origin_pos.length() > 1e-9 {
            origin_pos.normalize()
        } else {
            DVec3::new(1.0, 0.0, 0.0)
        };
        let dest_pos = origin_pos + radial * parking_r_au;

        // Build a minimal kinematic ActiveManeuver pointing the fleet back to
        // the origin body.  Kinematic mode (no Kepler orbit) so the existing
        // `update_fleet_maneuver_positions` linear-interpolates between start
        // and end positions; the duration is set to a small fraction of the
        // original remaining transfer time so the abort arrives quickly.
        let remaining_s = (current_man.arrival_time - elapsed).max(0.0);
        let abort_duration_s = (remaining_s * 0.5).max(86_400.0); // half the remaining, min 1 day
        let maneuver = ActiveManeuver {
            // Reuse the current orbit (orientation will be re-anchored by
            // `update_fleet_maneuver_positions`).
            transfer_orbit: current_man.transfer_orbit,
            reference_frame: current_man.reference_frame,
            orbit_center: current_man.origin_body,
            origin_body: current_man.origin_body,
            departure_time: elapsed,
            arrival_time: elapsed + abort_duration_s,
            preserve_orbit_geometry: true,
            destination_body: current_man.origin_body,
            // Park at the origin body's parking radius (a reasonable default).
            arrival_orbit_radius_au: parking_r_au,
            arrival_delta_v_ms: 0.0,
            fuel_used_t: action.abort_cost_t,
            option_label: "Abort to Origin",
            departure_angle: 0.0,
            start_position_au: Some(start_pos),
            end_position_au: Some(dest_pos),
            departure_velocity_ms: None,
            arrival_velocity_ms: None,
            start_visual_pos: fleet_transform_query
                .get(action.fleet)
                .ok()
                .map(|tr| tr.translation),
            flyby_body: None,
            leg2_orbit: None,
            leg2_start_s: 0.0,
            // GRA-153 M-3 / Kilo CRITICAL 1: belt-and-braces — set the
            // override flag in addition to the "Abort to Origin" label so
            // `is_kinematic()` returns true even if the label is later
            // changed for UX reasons.
            kinematic_override: true,
        };
        commands
            .entity(action.fleet)
            .remove::<FleetOrbit>()
            .remove::<ActiveManeuver>()
            .insert(maneuver);
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
            for (_, mut ship) in ship_queries.p1().iter_mut() {
                if ship.assigned_fleet == Some(entity) {
                    ship.info.fuel_mass_t = ship.info.max_fuel_t;
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
            if let Some(ship_entity) = ordered_ship_entities_for_fleet(&ship_queries.p0(), entity)
                .get(ship_idx)
                .copied()
            {
                if let Ok((_, mut ship)) = ship_queries.p1().get_mut(ship_entity) {
                    ship.info.fuel_mass_t = ship.info.max_fuel_t;
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
                let ordered_ships =
                    ordered_ship_entities_for_fleet(&ship_queries.p0(), action.source_fleet);
                let mut next_sort_order =
                    next_sort_order_for_fleet(&ship_queries.p0(), action.destination_fleet);
                for idx in action.ship_indices {
                    if let Some(ship_entity) = ordered_ships.get(idx).copied() {
                        if let Ok((_, mut ship)) = ship_queries.p1().get_mut(ship_entity) {
                            ship.assigned_fleet = Some(action.destination_fleet);
                            ship.parked_body = dst_orbit.body;
                            ship.parked_orbit_radius_au = dst_orbit.radius_au;
                            ship.stationary = dst_orbit.direction == 0.0;
                            ship.sort_order = next_sort_order;
                            next_sort_order += 1;
                        }
                    }
                }

                if !fleet_has_assigned_ships(&ship_queries.p0(), action.source_fleet) {
                    commands.entity(action.source_fleet).despawn();
                }
            }
        }
    }

    // Scrap individual ships. If the last ship is removed, despawn the fleet.
    for (entity, ship_idx) in actions.scrap_ships.drain(..) {
        let ordered_ships = ordered_ship_entities_for_fleet(&ship_queries.p0(), entity);
        if let Some(ship_entity) = ordered_ships.get(ship_idx).copied() {
            commands.entity(ship_entity).despawn();
        }

        if !fleet_has_assigned_ships(&ship_queries.p0(), entity) {
            commands.entity(entity).despawn();
        }
    }

    // Disband fleets (already confirmed by the player in the UI).
    for entity in actions.disband_fleets.drain(..) {
        if fleet_query.get(entity).is_ok() {
            let orbit_state = orbit_query
                .get(entity)
                .ok()
                .map(|orbit| (orbit.body, orbit.radius_au, orbit.direction == 0.0));

            for (_, mut ship) in ship_queries.p1().iter_mut() {
                if ship.assigned_fleet == Some(entity) {
                    ship.assigned_fleet = None;
                    if let Some((body, radius_au, stationary)) = orbit_state {
                        ship.parked_body = body;
                        ship.parked_orbit_radius_au = radius_au;
                        ship.stationary = stationary;
                    }
                }
            }
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
        let mut next_sort_order =
            next_sort_order_for_fleet(&ship_queries.p0(), action.target_fleet);
        for src_entity in &action.source_fleets {
            let ship_entities = ordered_ship_entities_for_fleet(&ship_queries.p0(), *src_entity);
            for ship_entity in ship_entities {
                if let Ok((_, mut ship)) = ship_queries.p1().get_mut(ship_entity) {
                    ship.assigned_fleet = Some(action.target_fleet);
                    ship.sort_order = next_sort_order;
                    next_sort_order += 1;
                }
            }
        }
        for e in action.source_fleets {
            commands.entity(e).despawn();
        }
    }
}

// ── Startup ───────────────────────────────────────────────────────────────────

/// Marker resource recording that the Day-1 constellation has been spawned.
///
/// `spawn_initial_fleet` short-circuits when this resource is present so a
/// save/load that re-uses the same `World` (e.g. a future save-restore path
/// that rehydrates `World` and re-runs `PostStartup`) does not duplicate
/// the constellation.  When a future "fresh save" / "new game" path lands it
/// must remove this resource (and the existing Day-1 fleet entities) before
/// the next `PostStartup` tick.  GRA-128.
#[derive(Resource, Debug, Clone, Copy, Default, Reflect)]
#[reflect(Resource)]
pub struct DayOneFleetSpawned;

/// Resolve a `ShipHullDefinition`'s dry mass from the `ShipbuildingData`
/// registry, logging a warning and falling back to the class default when
/// the hull id is missing.  Used by `spawn_initial_fleet` so the Day-1
/// ships honour the RON `base_dry_mass_t` instead of the class default
/// (which is calibrated for tier-2/3/4 ships and is wildly off for
/// 0.08-9.5 t probes).  GRA-128.
fn resolve_hull_dry_mass_t(
    hull_id: &str,
    class: ShipClass,
    shipbuilding_data: &ShipbuildingData,
) -> f32 {
    match shipbuilding_data.get_hull(hull_id) {
        Some(hull) => hull.base_dry_mass_t as f32,
        None => {
            bevy::log::warn!(
                "spawn_initial_fleet: hull '{}' not found in ShipbuildingData; \
                 falling back to class default for {:?}",
                hull_id,
                class
            );
            class.default_dry_mass_t()
        }
    }
}

/// Spawn the Day-1 Earth-orbit constellation described by the LGD design
/// contract (GRA-127 comment 6d35dea0-4c8d-4d2a-bb58-129ad00d06ae).
///
/// Replaces the previous 6-fleet demo (Earth Defense Squadron, Chemical
/// Strike Force at Venus, Ion Research Fleet at Mars, Fusion Expeditionary
/// Corps at Jupiter, Antimatter Vanguard at Saturn, Alpha Centauri Test
/// Fleet).  Those fleets have been removed because they skipped 4 tiers of
/// research — pre-spawning a fusion-torch cruiser at Jupiter makes the
/// chemical-spaceframes hull gate meaningless.  Each of those fleets is
/// now an **unlock target** the player must earn via research + slipway
/// construction, not a pre-spawned starting asset.
///
/// The new constellation is a single 5 (or 6, with the optional Mars flyby
/// probe) ship Earth-orbit fleet, all `tier: 1` hulls unlocked by
/// `chemical_spaceframes`.  Roster:
///
/// | # | Hull id             | Class           | Propulsion | Role                                        |
/// |---:|---------------------|-----------------|------------|---------------------------------------------|
/// | 1 | `micro_probe_frame` | `ResearchVessel` | `Chemical` | Cislunar hosted-payload bus, comms anchor   |
/// | 2 | `small_probe_frame` | `ResearchVessel` | `Chemical` | Inner-system survey probe (Venus transfer)  |
/// | 3 | `courier_frame`     | `ResearchVessel` | `Chemical` | Lunar-surface resupply courier              |
/// | 4 | `lander_frame`      | `ResearchVessel` | `Chemical` | Lunar / Mars lander bus (parked)            |
/// | 5 | `freighter_frame`   | `Freighter`     | `Chemical` | Cislunar logistics tug                      |
/// | 6 (opt.) | `small_probe_frame` | `ResearchVessel` | `IonDrive` | Mars flyby probe (MarCO-class)          |
///
/// All 5 (or 6) ships are parked in a 400 km altitude Earth orbit
/// (radius_au = `(6_371 km + 400 km) / AU_IN_METERS`); the optional 6th
/// is parked in a circular Mars parking orbit as a stand-in for the
/// planned Hohmann transfer (see the CTO review note in the PR for why
/// we do not wire a `KeplerOrbit` transfer here yet).
///
/// Idempotency: gated on `DayOneFleetSpawned`.  Future save/load
/// implementations must remove the resource (and the existing fleet
/// entities) before re-running `PostStartup` to re-spawn.
pub fn spawn_initial_fleet(
    mut commands: Commands,
    body_query: Query<(Entity, &crate::plugins::solar_system::CelestialBody)>,
    shipbuilding_data: Res<ShipbuildingData>,
    day_one_marker: Option<Res<DayOneFleetSpawned>>,
) {
    if day_one_marker.is_some() {
        // Idempotency: do not re-spawn.  The marker must be removed by a
        // future "new game" / "fresh save load" path before this branch
        // is taken.
        return;
    }

    // Helper: find a body by name.
    let find_body = |name: &str| -> Option<Entity> {
        body_query
            .iter()
            .find(|(_, b)| b.name == name)
            .map(|(e, _)| e)
    };

    // ── Roster ─────────────────────────────────────────────────────────────
    // (name, hull_id, class, propulsion) for each Day-1 ship.  Ship
    // `name` is human-readable; `hull_id` is the RON `ShipHullDefinition`
    // key used to resolve `base_dry_mass_t`.  Class and propulsion are the
    // ECS `ShipClass` / `PropulsionType` used for fuel / thrust / Isp math.
    const DAY_ONE_ROSTER: &[(&str, &str, ShipClass, PropulsionType)] = &[
        (
            "DOC-1 Cislunar Relay",
            "micro_probe_frame",
            ShipClass::ResearchVessel,
            PropulsionType::Chemical,
        ),
        (
            "DOC-2 Inner Surveyor",
            "small_probe_frame",
            ShipClass::ResearchVessel,
            PropulsionType::Chemical,
        ),
        (
            "DOC-3 Lunar Courier",
            "courier_frame",
            ShipClass::ResearchVessel,
            PropulsionType::Chemical,
        ),
        (
            "DOC-4 Lander Bus",
            "lander_frame",
            ShipClass::ResearchVessel,
            PropulsionType::Chemical,
        ),
        (
            "DOC-5 Cislunar Tug",
            "freighter_frame",
            ShipClass::Freighter,
            PropulsionType::Chemical,
        ),
    ];

    // ── Day-One Constellation (Earth 400 km orbit) ──────────────────────────
    let Some(earth) = find_body("Earth") else {
        bevy::log::warn!(
            "spawn_initial_fleet: Earth not found; skipping Day-One \
             Constellation spawn (this leaves the game with zero fleets \
             on Day 1 — investigate the solar system seed)"
        );
        // Mark spawned so we don't retry every tick if Earth is missing.
        commands.init_resource::<DayOneFleetSpawned>();
        return;
    };
    let earth_orbit_radius_au = (6_371.0_f64 + 400.0) * 1_000.0 / AU_IN_METERS;

    let ships: Vec<ShipInfo> = DAY_ONE_ROSTER
        .iter()
        .map(|(name, hull_id, class, propulsion)| {
            let dry_mass_t = resolve_hull_dry_mass_t(hull_id, *class, &shipbuilding_data);
            ShipInfo::new_with_dry_mass(
                (*name).to_string(),
                Some(*hull_id),
                *class,
                *propulsion,
                dry_mass_t,
            )
        })
        .collect();

    spawn_fleet_with_ship_entities(
        &mut commands,
        "Day-One Constellation".to_string(),
        ships,
        earth,
        earth_orbit_radius_au,
        false,
    );

    // ── Optional Mars Flyby Probe (parked in Mars orbit) ────────────────────
    // The LGD contract recommends YES for an early-game science probe
    // (MarCO-class).  For Day-1 we park the probe in a circular Mars
    // parking orbit at 400 km altitude; a follow-up can wire it to a real
    // `KeplerOrbit` transfer arc if/when the LGD wants the on-screen
    // trajectory to be a Hohmann ellipse from Day 1.  Skipped entirely if
    // Mars is missing from the seed (the 5-ship constellation still ships).
    if let Some(mars) = find_body("Mars") {
        let mars_orbit_radius_au = (3_390.0_f64 + 400.0) * 1_000.0 / AU_IN_METERS;
        let dry_mass_t = resolve_hull_dry_mass_t(
            "small_probe_frame",
            ShipClass::ResearchVessel,
            &shipbuilding_data,
        );
        let probe = ShipInfo::new_with_dry_mass(
            "DOC-6 Mars Flyby".to_string(),
            Some("small_probe_frame"),
            ShipClass::ResearchVessel,
            PropulsionType::IonDrive,
            dry_mass_t,
        );
        spawn_fleet_with_ship_entities(
            &mut commands,
            "Mars Flyby Probe".to_string(),
            vec![probe],
            mars,
            mars_orbit_radius_au,
            true,
        );
    } else {
        bevy::log::info!(
            "spawn_initial_fleet: Mars not found; skipping optional \
             Mars flyby probe (5-ship Day-One Constellation still spawned)"
        );
    }

    // Mark spawned so re-runs of `PostStartup` (e.g. after a future
    // save-load that rehydrates the `World`) do not duplicate the
    // constellation.
    commands.init_resource::<DayOneFleetSpawned>();
}

// ── GRA-371 Phase 1: TransferPlan shadow-sync ────────────────────────────────
//
// Phase 1 of the GRA-367 harmonisation plan shadows `FleetUiState`'s six
// coexisting option surfaces into a single `TransferPlan` resource.  The
// system below is the write-through path: every frame, it observes the
// `FleetUiState` fields that the planner render branch reads from and writes
// the matching `TransferPlan` representation.  Nothing *renders* off
// `TransferPlan` yet — Phases 2-6 migrate rendering branch-by-branch — so the
// only Phase-1 consumer is the 1-line reference-frame indicator in
// `render_transfer_planner` (`src/ui/transfer_planner.rs`), which reads
// `TransferPlan.frame`.  Every other `TransferPlan` field is populated for
// future phases to adopt without an API break.
//
// Behaviour-change audit (Phase 1 contract):
//   * System runs after `process_fleet_actions` so commits reflected in
//     `FleetUiState.planned_transfer` are mirrored this frame.
//   * System is idempotent — running it twice leaves `TransferPlan` in the
//     same state, so it can run in any schedule position relative to itself.
//   * Reads no ECS components, only `Res<FleetUiState>` + `ResMut<TransferPlan>`,
//     so registering it does not change the existing per-frame system graph
//     cost beyond a single in-place shadow write.
//   * Does not mutate `FleetUiState`.  Phase 2+ will introduce a reverse-sync
//     path; Phase 1 deliberately avoids that to keep the no-behaviour-change
//     guarantee.
pub fn sync_transfer_plan_from_ui_state(
    fleet_ui_state: Res<crate::ui::FleetUiState>,
    mut transfer_plan: ResMut<TransferPlan>,
) {
    use super::components::{
        CrossSystemGridSnapshot, DepWindowSnapshot, GravityAssistEntrySnapshot, PlanPreview,
        PorkchopGridSnapshot, SelectionSource, TransferOptionSnapshot,
    };

    // No target → `SelectionSource::Empty`.
    if fleet_ui_state.target_body.is_none()
        && fleet_ui_state.target_lagrange.is_none()
        && fleet_ui_state.target_fleet.is_none()
        && fleet_ui_state.target_star_system.is_none()
    {
        transfer_plan.source = SelectionSource::Empty;
        transfer_plan.selected = None;
        transfer_plan.preview = None;
        transfer_plan.commit = None;
        return;
    }

    let empty_window = DepWindowSnapshot {
        center_s: 0.0,
        half_width_s: 0.0,
    };

    // Cross-star / interstellar system target → `CrossStar`.
    if let Some((_system_id, _name, _distance_ly)) = fleet_ui_state.target_star_system {
        let cell_count = fleet_ui_state
            .cross_system_grid
            .as_ref()
            .map(|g| g.cols * g.rows)
            .unwrap_or(1);
        transfer_plan.source = SelectionSource::CrossStar {
            grid: CrossSystemGridSnapshot { cell_count },
            selected: None,
        };
    } else if fleet_ui_state.target_lagrange.is_some() {
        // Lagrange targets route through the same 3-option row as moons today.
        transfer_plan.source = SelectionSource::BodyHohmann {
            option: TransferOptionSnapshot {
                index: fleet_ui_state.selected_option,
                delta_v_ms: fleet_ui_state
                    .computed_options
                    .get(fleet_ui_state.selected_option)
                    .map(|o| o.total_dv_ms)
                    .unwrap_or(0.0),
            },
            dep_window: empty_window,
        };
    } else if let Some(idx) = fleet_ui_state.selected_gravity_assist {
        // Gravity-assist branch: GA candidate + the dep window of the parent
        // porkchop grid (mirrors the planner's GA-selected render path).
        transfer_plan.source = SelectionSource::GravityAssist {
            candidate: GravityAssistEntrySnapshot { index: Some(idx) },
            dep_window: empty_window,
        };
    } else if let Some(grid) = fleet_ui_state.porkchop_grid.as_ref() {
        let (cols, rows) = grid.resolution;
        transfer_plan.source = SelectionSource::Porkchop {
            grid: PorkchopGridSnapshot {
                cell_count: cols * rows,
                cols,
                rows,
            },
            selected: fleet_ui_state.selected_porkchop_cell,
        };
    } else if !fleet_ui_state.computed_options.is_empty() {
        transfer_plan.source = SelectionSource::BodyHohmann {
            option: TransferOptionSnapshot {
                index: fleet_ui_state.selected_option,
                delta_v_ms: fleet_ui_state
                    .computed_options
                    .get(fleet_ui_state.selected_option)
                    .map(|o| o.total_dv_ms)
                    .unwrap_or(0.0),
            },
            dep_window: empty_window,
        };
    } else {
        transfer_plan.source = SelectionSource::Empty;
    }

    transfer_plan.preview = Some(PlanPreview {
        option_count: fleet_ui_state.computed_options.len(),
        recommended: fleet_ui_state.selected_porkchop_cell,
    });

    transfer_plan.commit = fleet_ui_state.planned_transfer.clone();
}
