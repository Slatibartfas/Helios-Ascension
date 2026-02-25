//! ECS systems for fleet position updates, trajectory rendering, and startup.

use bevy::math::DVec3;
use bevy::prelude::*;

use super::components::{ActiveManeuver, Fleet, FleetOrbit, PendingFleetActions, ShipInfo};
use super::orbital_mechanics::AU_IN_METERS;
use super::types::{PropulsionType, ShipClass};
use bevy::time::Real;
use crate::astronomy::components::FloatingOrigin;
use crate::astronomy::{orbit_position_from_mean_anomaly, KeplerOrbit, LocalOrbitAmplification, SpaceCoordinates, SCALING_FACTOR};
use crate::plugins::camera::{GameCamera, OrbitCamera, ViewMode};
use crate::plugins::solar_system::{CelestialBody, LogicalParent};
use crate::plugins::solar_system_data::BodyType;
use crate::ui::{FleetUiState, SimulationTime, TimeScale};

/// Marker component for entities that have a fleet mesh sphere.
#[derive(Component)]
pub struct FleetMesh;

// ── Position update systems ───────────────────────────────────────────────────

/// One full visual revolution every 40 real seconds — readable at any time scale.
const VISUAL_ORBIT_RATE: f64 = std::f64::consts::TAU / 40.0;

/// Multiplier applied to a body's visual radius to determine the orbit ring
/// radius and fleet icon parking distance.  1.5× keeps the marker just outside
/// the body's glow without the ring dominating the view.
const FLEET_ORBIT_RADIUS_MULT: f32 = 1.5;

/// Radius of the sphere mesh spawned for each fleet icon (must match `ensure_fleet_meshes`).
const FLEET_SPHERE_RADIUS: f32 = 6.0;

/// Minimum clearance gap between the fleet sphere's surface and the body's visual surface.
const FLEET_ORBIT_MIN_GAP: f32 = 3.0;

/// Compute the visual-space parking orbit radius for a fleet around a body.
///
/// Uses `body_visual_radius * FLEET_ORBIT_RADIUS_MULT` as default, but enforces
/// a minimum of `body_visual_radius + FLEET_SPHERE_RADIUS + FLEET_ORBIT_MIN_GAP`
/// so the fleet marker never intersects the body for very small or inflated bodies.
#[inline]
fn fleet_parking_visual_radius(body_visual_radius: f32) -> f32 {
    let proportional = body_visual_radius * FLEET_ORBIT_RADIUS_MULT;
    let minimum = body_visual_radius + FLEET_SPHERE_RADIUS + FLEET_ORBIT_MIN_GAP;
    proportional.max(minimum)
}

/// Number of sub-samples used to approximate arc length for dashed curves.
const ARC_SAMPLES: usize = 256;
/// Fraction of each dash-gap cycle occupied by the visible dash (0.6 = 60%).
const DASH_RATIO: f32 = 0.6;

/// Draw a dashed curve with *proportional*, arc-length-uniform spacing.
///
/// The curve is densely over-sampled, then `num_dashes` dash-gap cycles are
/// placed at uniform arc-length intervals.  The result looks the same
/// regardless of the curve's total world-space length.
///
/// * `sample_fn` — maps `t ∈ [0, 1]` to a world-space position on the curve.
/// * `num_dashes` — how many visible dash segments the curve should contain.
/// * `color_fn` — maps arc-length fraction `∈ [0, 1]` to the dash colour.
fn draw_dashed_curve(
    gizmos: &mut Gizmos,
    sample_fn: impl Fn(f32) -> Vec3,
    num_dashes: u32,
    color_fn: impl Fn(f32) -> Color,
) {
    let mut points = Vec::with_capacity(ARC_SAMPLES + 1);
    for i in 0..=ARC_SAMPLES {
        points.push(sample_fn(i as f32 / ARC_SAMPLES as f32));
    }
    draw_dashed_polyline(gizmos, &points, num_dashes, &color_fn);
}

/// Draw a dashed polyline with proportional, arc-length-uniform spacing.
///
/// `num_dashes` visible dash segments are distributed evenly along the total
/// arc length.  Each sub-segment is drawn when its midpoint falls inside a
/// dash phase.  With ≥ 256 samples the transition error is sub-pixel.
fn draw_dashed_polyline(
    gizmos: &mut Gizmos,
    points: &[Vec3],
    num_dashes: u32,
    color_fn: &dyn Fn(f32) -> Color,
) {
    if points.len() < 2 || num_dashes == 0 { return; }
    let n = points.len();
    let mut cum = Vec::with_capacity(n);
    cum.push(0.0_f32);
    for i in 1..n {
        cum.push(cum[i - 1] + (points[i] - points[i - 1]).length());
    }
    let total = *cum.last().unwrap();
    if total < 0.001 { return; }
    let cycle = total / num_dashes as f32;
    let dash_len = cycle * DASH_RATIO;
    for i in 1..n {
        let mid_arc = (cum[i - 1] + cum[i]) * 0.5;
        let phase = mid_arc % cycle;
        if phase < dash_len {
            let frac = mid_arc / total;
            gizmos.line(points[i - 1], points[i], color_fn(frac));
        }
    }
}

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
    mut fleet_query: Query<
        (&mut SpaceCoordinates, &mut FleetOrbit),
        With<Fleet>,
    >,
    body_coords: Query<&SpaceCoordinates, Without<Fleet>>,
) {
    // Freeze the visual orbit when the player has paused the simulation.
    let real_delta = if time_scale.is_paused() { 0.0 } else { real_time.delta_secs_f64() };

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
        // Skip pre-departure fleets — they are still handled by update_fleet_orbit_positions.
        if elapsed < maneuver.departure_time {
            continue;
        }
        
        if maneuver.is_kinematic() {
            let progress = maneuver.progress(elapsed);
            
            // Get origin position at departure
            let origin_pos = maneuver.start_position_au.unwrap_or_else(|| {
                center_coords.get(maneuver.origin_body).map(|sc| sc.position).unwrap_or(DVec3::ZERO)
            });
            
            // Get destination position at arrival
            let dest_pos = maneuver.end_position_au.unwrap_or_else(|| {
                center_coords.get(maneuver.destination_body).map(|sc| sc.position).unwrap_or(DVec3::ZERO)
            });
            
            fleet_sc.position = origin_pos + (dest_pos - origin_pos) * progress;
        } else {
            let dt = elapsed - maneuver.departure_time;
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
            let center_pos = center_coords.get(destination)
                .map(|sc| sc.position)
                .unwrap_or(DVec3::ZERO); // star sits at the heliocentric origin
            let rel = fleet_sc.position - center_pos;
            let pos_angle = rel.y.atan2(rel.x);

            let direction = if maneuver.is_kinematic() {
                let start_pos = maneuver.start_position_au.unwrap_or(DVec3::ZERO);
                let end_pos = maneuver.end_position_au.unwrap_or(DVec3::ZERO);
                let vel_dir = end_pos - start_pos;
                let cross_z = rel.x * vel_dir.y - rel.y * vel_dir.x;
                if cross_z >= 0.0 { 1.0 } else { -1.0 }
            } else {
                // Determine whether the arrival was prograde (CCW) or retrograde (CW)
                // by computing the Keplerian velocity direction at the moment of arrival
                // and taking its cross product with the position vector.
                let mean_anomaly_arrival = maneuver.transfer_orbit.mean_anomaly_epoch
                    + maneuver.transfer_orbit.mean_motion * (elapsed - maneuver.departure_time);
                let small_dt = 1.0_f64; // 1 second step
                let ma_before = mean_anomaly_arrival
                    - maneuver.transfer_orbit.mean_motion * small_dt;
                let pos_before = orbit_position_from_mean_anomaly(
                    &maneuver.transfer_orbit, ma_before);
                let pos_now = orbit_position_from_mean_anomaly(
                    &maneuver.transfer_orbit, mean_anomaly_arrival);
                let vel_dir = pos_now - pos_before; // proportional to velocity
                // 2-D cross product (z-component): rel × vel_dir
                let cross_z = rel.x * vel_dir.y - rel.y * vel_dir.x;
                if cross_z >= 0.0 { 1.0 } else { -1.0 }
            };

            (pos_angle, direction)
        };

        // LP-stationed (L3/L4/L5) arrivals: a kinematic arc ending at a heliocentric
        // position (star destination) should freeze the fleet at the Lagrange-point
        // angular position instead of freely orbiting the Sun at 1 AU every 40 s.
        let orbit_direction = if maneuver.is_kinematic()
            && body_type_query.get(destination)
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
        commands.entity(entity).remove::<ActiveManeuver>().insert(new_orbit);
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
        // the origin body's angle relative to the orbit center at the actual departure moment.
        if let Ok(origin_sc) = body_coords.get(maneuver.origin_body) {
            let center_pos = body_coords.get(maneuver.orbit_center)
                .map(|sc| sc.position)
                .unwrap_or(DVec3::ZERO);
            
            let rel_pos = origin_sc.position - center_pos;
            let theta = rel_pos.y.atan2(rel_pos.x);
            maneuver.transfer_orbit.argument_of_periapsis =
                theta - maneuver.transfer_orbit.mean_anomaly_epoch;
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
        let departure_angle = orbit_query.get(action.fleet)
            .map(|o| o.angle_rad as f32)
            .unwrap_or(0.0);
            
        let is_kinematic = t.option_label == "Full Thrust" || t.option_label.contains("Coast") || t.option_label == "Max Speed" || t.option_label.contains("Direct");
        let (start_position_au, end_position_au) = if is_kinematic {
            // For course corrections (mid-transit), always use the fleet's actual current
            // physics position as departure — the pre-computed start from the planner may
            // reference a planet body (e.g. Jupiter) rather than where the fleet actually is.
            let start_pos = if is_in_transit {
                fleet_sc_query.get(action.fleet)
                    .map(|sc| sc.position)
                    .unwrap_or_else(|_| {
                        // Fallback: predict origin body
                        predict_body_physics_pos(t.origin_body, departure_s, &body_query, &kepler_query)
                            .unwrap_or_else(|| center_coords.get(t.origin_body).map(|sc| sc.position).unwrap_or(DVec3::ZERO))
                    })
            } else {
                t.start_position_au.unwrap_or_else(|| {
                    predict_body_physics_pos(
                        t.origin_body,
                        departure_s,
                        &body_query,
                        &kepler_query,
                    ).unwrap_or_else(|| {
                        center_coords.get(t.origin_body).map(|sc| sc.position).unwrap_or(DVec3::ZERO)
                    })
                })
            };
            
            let end_pos = t.end_position_au.unwrap_or_else(|| {
                predict_body_physics_pos(
                    t.destination_body,
                    arrival_s,
                    &body_query,
                    &kepler_query,
                ).unwrap_or_else(|| {
                    center_coords.get(t.destination_body).map(|sc| sc.position).unwrap_or(DVec3::ZERO)
                })
            });
            
            (Some(start_pos), Some(end_pos))
        } else {
            (None, None)
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
                if let Ok([mut src_fleet, mut dst_fleet]) = fleet_query.get_many_mut([action.source_fleet, action.destination_fleet]) {
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
            orbit_query.get(src).map(|o| o.body == target_body).unwrap_or(false)
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

// ── Rendering systems ─────────────────────────────────────────────────────────

/// Predict the visual (render-space) `Vec3` position where a celestial body will
/// be at `future_sim_s` by propagating its `KeplerOrbit` forward.
///
/// Returns `None` if the body has no `KeplerOrbit` component.
fn predict_body_visual_pos(
    target: Entity,
    future_sim_s: f64,
    body_query: &Query<(&Transform, &CelestialBody, Option<&LogicalParent>), Without<Fleet>>,
    kepler_query: &Query<&KeplerOrbit, Without<Fleet>>,
    amp_query: &Query<&LocalOrbitAmplification, Without<Fleet>>,
) -> Option<Vec3> {
    let kepler = kepler_query.get(target).ok()?;
    let (_, _, maybe_lp) = body_query.get(target).ok()?;

    // Advance mean anomaly to future simulation time.
    let ma = kepler.mean_anomaly_epoch + kepler.mean_motion * future_sim_s;
    let pos_au = orbit_position_from_mean_anomaly(kepler, ma);

    // Apply LocalOrbitAmplification (moons rendered further from parent than raw AU).
    let amp = amp_query.get(target).map(|a| a.0 as f64).unwrap_or(1.0);

    let pos_scaled = Vec3::new(
        (pos_au.x * SCALING_FACTOR * amp) as f32,
        (pos_au.y * SCALING_FACTOR * amp) as f32,
        (pos_au.z * SCALING_FACTOR * amp) as f32,
    );

    // Anchor to the parent body's predicted visual position.
    let parent_pos = if let Some(lp) = maybe_lp {
        predict_body_visual_pos(lp.0, future_sim_s, body_query, kepler_query, amp_query)
            .unwrap_or_else(|| body_query.get(lp.0).ok().map(|(t, _, _)| t.translation).unwrap_or(Vec3::ZERO))
    } else {
        Vec3::ZERO // star at render origin
    };

    Some(parent_pos + pos_scaled)
}

/// Predict the physics (AU) `DVec3` position where a celestial body will
/// be at `future_sim_s` by propagating its `KeplerOrbit` forward.
///
/// Returns `None` if the body has no `KeplerOrbit` component.
fn predict_body_physics_pos(
    target: Entity,
    future_sim_s: f64,
    body_query: &Query<(&Transform, &CelestialBody, Option<&LogicalParent>), Without<Fleet>>,
    kepler_query: &Query<&KeplerOrbit, Without<Fleet>>,
) -> Option<DVec3> {
    let kepler = kepler_query.get(target).ok()?;
    let (_, _, maybe_lp) = body_query.get(target).ok()?;

    // Advance mean anomaly to future simulation time.
    let ma = kepler.mean_anomaly_epoch + kepler.mean_motion * future_sim_s;
    let pos_au = orbit_position_from_mean_anomaly(kepler, ma);

    // Anchor to the parent body's predicted physics position.
    let parent_pos = if let Some(lp) = maybe_lp {
        predict_body_physics_pos(lp.0, future_sim_s, body_query, kepler_query)
            .unwrap_or(DVec3::ZERO)
    } else {
        DVec3::ZERO // star at origin
    };

    Some(parent_pos + pos_au)
}

/// Draw a KSP-style "ghost" body gizmo at `center` showing predicted arrival position.
///
/// * Dashed amber ring at `ring_r` — the arrival orbit ring (skipped when
///   `skip_outer_ring` is true, e.g. when the destination is the orbit centre and
///   `draw_fleet_orbit_rings` already draws a cyan arrival ring at the same radius).
/// * Smaller dashed amber circle at approximately the body's visual size.
/// * Crosshair in the centre.
fn draw_ghost_body(gizmos: &mut Gizmos, center: Vec3, ring_r: f32, body_r: f32, skip_outer_ring: bool) {
    let tau = std::f32::consts::TAU;

    // Arrival orbit ring — dashed amber, 50 % alpha.
    if !skip_outer_ring {
        draw_dashed_curve(
            gizmos,
            |t| {
                let a = t * tau;
                center + Vec3::new(a.cos() * ring_r, a.sin() * ring_r, 0.0)
            },
            16,
            |_| Color::srgba(1.0, 0.75, 0.15, 0.50),
        );
    }

    // Ghost body outline — slightly smaller, 28 % alpha.
    let ghost_r = (body_r * 0.85).max(ring_r * 0.35);
    draw_dashed_curve(
        gizmos,
        |t| {
            let a = t * tau;
            center + Vec3::new(a.cos() * ghost_r, a.sin() * ghost_r, 0.0)
        },
        12,
        |_| Color::srgba(1.0, 0.75, 0.15, 0.28),
    );

    // Centre crosshair.
    let cs = ring_r * 0.18;
    let cross_color = Color::srgba(1.0, 0.75, 0.15, 0.65);
    gizmos.line(center - Vec3::X * cs, center + Vec3::X * cs, cross_color);
    gizmos.line(center - Vec3::Y * cs, center + Vec3::Y * cs, cross_color);
}

/// Compute the stable, optimal departure angle for a local transfer arc.
///
/// Returns the angle (radians) in the ecliptic plane from the origin body position
/// toward the destination position, so the arc departure is in the direction
/// the fleet must be facing to begin the transfer.
///
/// Fallback: when `origin` and `destination` are at nearly the same render position
/// (distance ≤ 0.1 render units), returns the angle of `origin` relative to the
/// coordinate origin.  In System view the central star is always at `Vec3::ZERO`, so
/// this fallback yields the radially-outward direction from the star — a physically
/// sensible departure direction for that degenerate case.
///
/// # Why not use the fleet's orbit phase?
/// The fleet is assumed to wait in its parking orbit until it reaches this
/// angle before firing, spending only a few hours to guarantee the ΔV-optimal
/// departure geometry.  This keeps the arc stable across the fast visual orbit rate.
fn optimal_departure_angle(origin: Vec3, destination: Vec3) -> f32 {
    let to_dest = destination - origin;
    if to_dest.length() > 0.1 {
        to_dest.y.atan2(to_dest.x)
    } else {
        // Fallback: radially outward from the star (star is at Vec3::ZERO in System view).
        origin.y.atan2(origin.x)
    }
}

/// Draw the trajectory arc for the selected fleet.
/// In System view uses SCALING_FACTOR; in Starmap view uses raw AU (1 unit = 1 AU).
///
/// For local (non-heliocentric) transfers the trajectory is drawn in **visual space**
/// by interpolating between the origin and destination body render positions.
/// Heliocentric transfers continue to use the physics-accurate Keplerian arc.
pub fn draw_fleet_trajectories(
    mut gizmos: Gizmos,
    fleet_query: Query<(Entity, &ActiveManeuver), With<Fleet>>,
    center_coords: Query<&SpaceCoordinates, Without<Fleet>>,
    body_query: Query<(&Transform, &CelestialBody, Option<&LogicalParent>), Without<Fleet>>,
    kepler_query: Query<&KeplerOrbit, Without<Fleet>>,
    amp_query: Query<&LocalOrbitAmplification, Without<Fleet>>,
    floating_origin: Option<Res<FloatingOrigin>>,
    fleet_ui_state: Res<FleetUiState>,
    view_mode: Res<ViewMode>,
    sim_time: Res<SimulationTime>,
    real_time: Res<Time<Real>>,
) {
    let origin_offset = floating_origin
        .as_ref()
        .map(|fo| fo.position)
        .unwrap_or(DVec3::ZERO);

    let scale: f64 = match *view_mode {
        ViewMode::System => SCALING_FACTOR,
        ViewMode::Starmap => 1.0,
    };

    const SEGMENTS: u32 = 64;
    let sim_elapsed = sim_time.elapsed_seconds();
    let real_secs = real_time.elapsed_secs();

    for (entity, maneuver) in fleet_query.iter() {
        // In System view only draw for the selected fleet, in Starmap always draw.
        if *view_mode == ViewMode::System {
            match fleet_ui_state.selected_fleet {
                Some(sel) if entity != sel => continue,
                None => continue, // No fleet selected → hide all trajectories
                _ => {}
            }
        }

        let center_is_star = body_query.get(maneuver.orbit_center)
            .map(|(_, b, _)| b.body_type == BodyType::Star)
            .unwrap_or(true);

        if !center_is_star && *view_mode == ViewMode::System {
            // ── Local transfer: visual-space arc ──
            let origin_ring_r = body_query.get(maneuver.origin_body)
                .map(|(_, b, _)| fleet_parking_visual_radius(b.visual_radius))
                .unwrap_or(0.0);
            let (dest_visual_r, dest_ring_r) = body_query.get(maneuver.destination_body)
                .map(|(_, b, _)| (b.visual_radius, fleet_parking_visual_radius(b.visual_radius)))
                .unwrap_or((0.0, 0.0));

            let origin_visual = body_query.get(maneuver.origin_body)
                .map(|(t, _, _)| t.translation).ok();

            if let Some(op_current) = origin_visual {
                let dp_current = body_query.get(maneuver.destination_body)
                    .ok().map(|(t, _, _)| t.translation);
                let Some(dp_now) = dp_current else { continue; };

                let cv_current = body_query.get(maneuver.orbit_center)
                    .map(|(t, _, _)| t.translation).unwrap_or(Vec3::ZERO);

                // Once departed, fix the origin at the moon's departure-time position
                // so it moves with the planet but not with the moon's orbit.
                let is_departed = sim_elapsed >= maneuver.departure_time;
                let op = if is_departed {
                    let op_absolute = predict_body_visual_pos(
                        maneuver.origin_body,
                        maneuver.departure_time,
                        &body_query,
                        &kepler_query,
                        &amp_query,
                    ).unwrap_or(op_current);
                    let cv_at_departure = predict_body_visual_pos(
                        maneuver.orbit_center,
                        maneuver.departure_time,
                        &body_query,
                        &kepler_query,
                        &amp_query,
                    ).unwrap_or(cv_current);
                    op_absolute - cv_at_departure + cv_current
                } else {
                    op_current
                };

                // For the in-transit arc, target where the body WILL BE at arrival,
                // not where it is now.  The preview used predicted pos; once departed
                // we must continue pointing at the same predicted endpoint.
                let dp_absolute = predict_body_visual_pos(
                    maneuver.destination_body,
                    maneuver.arrival_time,
                    &body_query,
                    &kepler_query,
                    &amp_query,
                ).unwrap_or(dp_now);

                let cv_predicted = predict_body_visual_pos(
                    maneuver.orbit_center,
                    maneuver.arrival_time,
                    &body_query,
                    &kepler_query,
                    &amp_query,
                ).unwrap_or(dp_absolute);

                let dp = dp_absolute - cv_predicted + cv_current;

                // Optimal departure angle: direction from origin body toward the predicted
                // arrival position — same logic as the preview arc (stable, prograde-optimal).
                let dep_angle = optimal_departure_angle(op, dp);
                let dir_dep = Vec3::new(dep_angle.cos(), dep_angle.sin(), 0.0);
                let p0 = op + dir_dep * origin_ring_r;

                // Determine if this is an inward (orbit-lowering) transfer so
                // the departure tangent matches the preview arc direction.
                let origin_lp = body_query.get(maneuver.origin_body)
                    .ok().and_then(|(_, _, lp)| lp.map(|lp| lp.0));
                let dest_lp = body_query.get(maneuver.destination_body)
                    .ok().and_then(|(_, _, lp)| lp.map(|lp| lp.0));
                let is_inward = if origin_lp == Some(maneuver.destination_body) {
                    true  // Origin orbits the destination (e.g. Moon → Earth)
                } else if dest_lp == Some(maneuver.origin_body) {
                    false // Destination orbits the origin (e.g. Earth → Moon)
                } else {
                    op.length_squared() > dp.length_squared()
                };
                // Departure tangent: prograde (CCW) for outward, retrograde (CW) for inward.
                let tang_origin = if is_inward {
                    Vec3::new(dep_angle.sin(), -dep_angle.cos(), 0.0)  // CW
                } else {
                    Vec3::new(-dep_angle.sin(), dep_angle.cos(), 0.0)  // CCW
                };

                // Arrival: radial direction of destination relative to its orbital centre.
                let radial_dest_raw = dp - cv_current;
                let radial_dest = if radial_dest_raw.length() > 1.0 {
                    radial_dest_raw.normalize()
                } else {
                    (dp - op).normalize_or_zero()
                };
                // p3 is on the arrival ring, on the side closest to the origin.
                let inward = (op - dp).normalize_or_zero();
                let p3 = dp + inward * dest_ring_r;
                // Always use the CCW (prograde) arrival tangent so the direction
                // matches the parking orbit that follows insertion.
                let tang_d_a = Vec3::new(-radial_dest.y, radial_dest.x, 0.0);
                let tang_dest = if tang_d_a.dot(tang_origin) >= 0.0 { tang_d_a } else { -tang_d_a };

                // ── Semi-transparent purple glow: show only the remaining arc ─────────
                let progress_t = if maneuver.arrival_time > maneuver.departure_time {
                    ((sim_elapsed - maneuver.departure_time)
                        / (maneuver.arrival_time - maneuver.departure_time))
                        .clamp(0.0, 1.0) as f32
                } else {
                    1.0_f32
                };
                let remaining_span = (1.0_f32 - progress_t).max(1e-4_f32);
                // Glow pulse travels from fleet toward target, one cycle per 4 real seconds.
                let glow_pos = progress_t + (real_secs / 4.0_f32).fract() * remaining_span;
                let traj_color = |t: f32| -> Color {
                    let arc_frac = ((t - progress_t) / remaining_span).clamp(0.0, 1.0);
                    let base_a = 0.50 - 0.22 * arc_frac;
                    let dist = (t - glow_pos).abs();
                    let glow = (1.0 - (dist / 0.09_f32).min(1.0)).powi(2);
                    // Deep purple base; bright lavender-white at pulse peak (triggers bloom).
                    Color::linear_rgba(
                        0.55 + glow * 1.15,
                        0.08 + glow * 0.50,
                        0.85 + glow * 1.05,
                        (base_a + glow * 0.45).min(1.0),
                    )
                };

                if maneuver.is_kinematic() {
                    // Kinematic transfer: straight powered line — draw only the remaining arc.
                    let start_pos = p0 + (p3 - p0) * progress_t;
                    let mut prev = Some(start_pos);
                    for i in 0..=SEGMENTS {
                        let t0 = i as f32 / SEGMENTS as f32;
                        if t0 <= progress_t { continue; }
                        let pos = p0 + (p3 - p0) * t0;
                        if let Some(prev_pos) = prev {
                            gizmos.line(prev_pos, pos, traj_color(t0));
                        }
                        prev = Some(pos);
                    }
                } else {
                    let ctrl_len = (p3 - p0).length() * 0.40;
                    let mut p1 = p0 + tang_origin * ctrl_len;
                    let mut p2 = p3 - tang_dest   * ctrl_len;
                    // Smoothly interpolate z so the arc bridges the two orbital planes
                    // even when the tangent vectors are 2D (z = 0).
                    p1.z = p0.z + (p3.z - p0.z) * 0.33;
                    p2.z = p0.z + (p3.z - p0.z) * 0.67;

                    let bezier = |t: f32| -> Vec3 {
                        let u = 1.0 - t;
                        u*u*u*p0 + 3.0*u*u*t*p1 + 3.0*u*t*t*p2 + t*t*t*p3
                    };

                    // Start exactly at the fleet's current position on the Bezier arc.
                    let mut prev: Option<Vec3> = Some(bezier(progress_t));
                    for i in 0..=SEGMENTS {
                        let t_frac = i as f32 / SEGMENTS as f32;
                        if t_frac <= progress_t { continue; }
                        let pos = bezier(t_frac);
                        if let Some(prev_pos) = prev {
                            gizmos.line(prev_pos, pos, traj_color(t_frac));
                        }
                        prev = Some(pos);
                    }
                }

                // Ghost body at predicted arrival position (same as arc target).
                // Suppress the outer ring when the destination IS the orbit centre
                // (e.g. Moon → Earth): draw_fleet_orbit_rings already draws a cyan
                // arrival ring at that same radius, and the two rings overlapping looks
                // visually strange.
                let dest_is_orbit_center = origin_lp == Some(maneuver.destination_body);
                draw_ghost_body(&mut gizmos, dp, dest_ring_r, dest_visual_r, dest_is_orbit_center);
            }
            continue;
        }

        // ── Heliocentric / Starmap: physics-accurate Keplerian arc ──
        let center_pos = center_coords
            .get(maneuver.orbit_center)
            .map(|sc| sc.position)
            .unwrap_or(DVec3::ZERO);

        // ── Semi-transparent purple glow: show only the remaining arc ─────────
        let progress_t = if maneuver.arrival_time > maneuver.departure_time {
            ((sim_elapsed - maneuver.departure_time)
                / (maneuver.arrival_time - maneuver.departure_time))
                .clamp(0.0, 1.0) as f32
        } else {
            1.0_f32
        };
        let remaining_span = (1.0_f32 - progress_t).max(1e-4_f32);
        let glow_pos = progress_t + (real_secs / 4.0_f32).fract() * remaining_span;
        let traj_color = |t: f32| -> Color {
            let arc_frac = ((t - progress_t) / remaining_span).clamp(0.0, 1.0);
            let base_a = 0.50 - 0.22 * arc_frac;
            let dist = (t - glow_pos).abs();
            let glow = (1.0 - (dist / 0.09_f32).min(1.0)).powi(2);
            Color::linear_rgba(
                0.55 + glow * 1.15,
                0.08 + glow * 0.50,
                0.85 + glow * 1.05,
                (base_a + glow * 0.45).min(1.0),
            )
        };

        if maneuver.is_kinematic() {
            // Kinematic transfer: straight powered line — draw only the remaining arc.
            let p0_au = maneuver.start_position_au.unwrap_or_else(|| {
                center_coords.get(maneuver.origin_body).map(|sc| sc.position).unwrap_or(DVec3::ZERO)
            }) - center_pos;

            let p1_au = maneuver.end_position_au.unwrap_or_else(|| {
                center_coords.get(maneuver.destination_body).map(|sc| sc.position).unwrap_or(DVec3::ZERO)
            }) - center_pos;

            let rp0 = Vec3::new(
                ((center_pos.x + p0_au.x - origin_offset.x) * scale) as f32,
                ((center_pos.y + p0_au.y - origin_offset.y) * scale) as f32,
                ((center_pos.z + p0_au.z - origin_offset.z) * scale) as f32,
            );
            let rp1 = Vec3::new(
                ((center_pos.x + p1_au.x - origin_offset.x) * scale) as f32,
                ((center_pos.y + p1_au.y - origin_offset.y) * scale) as f32,
                ((center_pos.z + p1_au.z - origin_offset.z) * scale) as f32,
            );
            let start_pos = rp0 + (rp1 - rp0) * progress_t;
            let mut prev = Some(start_pos);
            for i in 0..=SEGMENTS {
                let t0 = i as f32 / SEGMENTS as f32;
                if t0 <= progress_t { continue; }
                let pos = rp0 + (rp1 - rp0) * t0;
                if let Some(prev_pos) = prev {
                    gizmos.line(prev_pos, pos, traj_color(t0));
                }
                prev = Some(pos);
            }
        } else {
            let total_ma_travel = maneuver.transfer_orbit.mean_motion
                * (maneuver.arrival_time - maneuver.departure_time);

            // Start exactly at the fleet's current Keplerian position on the arc.
            let ma_start = maneuver.transfer_orbit.mean_anomaly_epoch
                + total_ma_travel * progress_t as f64;
            let orbit_start = orbit_position_from_mean_anomaly(&maneuver.transfer_orbit, ma_start);
            let world_start = center_pos + orbit_start - origin_offset;
            let mut prev: Option<Vec3> = Some(Vec3::new(
                (world_start.x * scale) as f32,
                (world_start.y * scale) as f32,
                (world_start.z * scale) as f32,
            ));

            for i in 0..=SEGMENTS {
                let frac = i as f64 / SEGMENTS as f64;
                if (frac as f32) <= progress_t { continue; }

                let mean_anomaly =
                    maneuver.transfer_orbit.mean_anomaly_epoch + total_ma_travel * frac;
                let orbit_pos = orbit_position_from_mean_anomaly(&maneuver.transfer_orbit, mean_anomaly);
                let world_au = center_pos + orbit_pos - origin_offset;
                let render_pos = Vec3::new(
                    (world_au.x * scale) as f32,
                    (world_au.y * scale) as f32,
                    (world_au.z * scale) as f32,
                );

                if let Some(prev_pos) = prev {
                    gizmos.line(prev_pos, render_pos, traj_color(frac as f32));
                }
                prev = Some(render_pos);
            }
        }
    }
}

/// Draw a green corner-bracket selection reticule around the currently selected fleet.
///
/// Draws 4 L-shaped brackets in System view using gizmos.  In Starmap view the
/// reticule is drawn at AU-scale.  The reticule is only shown while a fleet is
/// selected (`FleetUiState.selected_fleet`).
pub fn draw_fleet_selection_reticule(
    mut gizmos: Gizmos,
    fleet_ui_state: Res<FleetUiState>,
    fleet_query: Query<(&SpaceCoordinates, Option<&Transform>), With<Fleet>>,
    floating_origin: Option<Res<FloatingOrigin>>,
    view_mode: Res<ViewMode>,
    camera_query: Query<&OrbitCamera, With<GameCamera>>,
) {
    let Some(selected) = fleet_ui_state.selected_fleet else {
        return;
    };
    let Ok((sc, maybe_transform)) = fleet_query.get(selected) else {
        return;
    };

    let origin_offset = floating_origin
        .as_ref()
        .map(|fo| fo.position)
        .unwrap_or(DVec3::ZERO);

    let (center, arm) = match *view_mode {
        ViewMode::System => {
            // Use the fleet's already-computed visual Transform if available,
            // so the reticule tracks the visual position (accounts for moon amplification).
            let pos = if let Some(t) = maybe_transform {
                t.translation
            } else {
                let du = (sc.position - origin_offset) * SCALING_FACTOR;
                Vec3::new(du.x as f32, du.y as f32, du.z as f32)
            };
            (pos, 22.0_f32)
        }
        ViewMode::Starmap => {
            let camera_radius = camera_query
                .single()
                .map(|c| c.radius as f32)
                .unwrap_or(200_000.0);
            let icon_size = 280.0 * (camera_radius / 100_000.0).sqrt().max(0.5);
            let raw = sc.position - origin_offset;
            let pos = Vec3::new(raw.x as f32, raw.y as f32, raw.z as f32);
            (pos, icon_size)
        }
    };

    // Bright green for friendly-fleet selection
    let color = Color::srgba(0.15, 1.0, 0.35, 1.0);
    let dim = Color::srgba(0.15, 1.0, 0.35, 0.35);
    let gap = arm * 0.35; // gap between centre and bracket start
    let len = arm * 0.55; // length of each bracket arm

    // Draw 4 L-shaped corner brackets in the XY plane (ecliptic plane).
    // Each corner: two short lines meeting at a right angle.
    for &(sx, sy) in &[(1.0_f32, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)] {
        let corner = center + Vec3::new(sx * arm, sy * arm, 0.0);

        // Horizontal arm (towards centre along X)
        let h_start = corner;
        let h_end = corner - Vec3::new(sx * len, 0.0, 0.0);
        gizmos.line(h_start, h_end, color);

        // Vertical arm (towards centre along Y)
        let v_end = corner - Vec3::new(0.0, sy * len, 0.0);
        gizmos.line(h_start, v_end, color);
    }

    // Cross hair: faint diagonal lines through centre for readability
    gizmos.line(
        center - Vec3::new(gap, 0.0, 0.0),
        center + Vec3::new(gap, 0.0, 0.0),
        dim,
    );
    gizmos.line(
        center - Vec3::new(0.0, gap, 0.0),
        center + Vec3::new(0.0, gap, 0.0),
        dim,
    );
}

/// Draw a small cross marker at each fleet's current render position.
/// This fallback only draws for fleets that somehow lack a mesh.
pub fn draw_fleet_icons(
    mut gizmos: Gizmos,
    fleet_query: Query<(&SpaceCoordinates, Option<&ActiveManeuver>), (With<Fleet>, Without<FleetMesh>)>,
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

/// Draw fleet position markers (cross gizmos) in Starmap view at AU scale.
/// Called every frame; early-exits in System view.
pub fn draw_fleet_starmap_icons(
    mut gizmos: Gizmos,
    fleet_query: Query<(&SpaceCoordinates, Option<&ActiveManeuver>), With<Fleet>>,
    floating_origin: Option<Res<FloatingOrigin>>,
    view_mode: Res<ViewMode>,
    camera_query: Query<&OrbitCamera, With<GameCamera>>,
) {
    if *view_mode != ViewMode::Starmap {
        return;
    }

    let origin_offset = floating_origin
        .as_ref()
        .map(|fo| fo.position)
        .unwrap_or(DVec3::ZERO);

    // Scale cross size proportionally to camera distance (matches star-icon scale logic).
    let camera_radius = camera_query.single().ok().map(|c| c.radius as f32).unwrap_or(200_000.0);
    let icon_size = 200.0 * (camera_radius / 100_000.0).sqrt().max(0.5);

    for (sc, maybe_maneuver) in fleet_query.iter() {
        // Starmap uses raw AU (1 unit = 1 AU); no SCALING_FACTOR.
        let raw_au = sc.position - origin_offset;
        let pos = Vec3::new(raw_au.x as f32, raw_au.y as f32, raw_au.z as f32);

        let color = if maybe_maneuver.is_some() {
            Color::srgba(0.3, 0.8, 1.0, 1.0) // cyan in transit
        } else {
            Color::srgba(0.2, 0.9, 0.3, 1.0) // green in orbit
        };

        gizmos.line(pos - Vec3::X * icon_size, pos + Vec3::X * icon_size, color);
        gizmos.line(pos - Vec3::Y * icon_size, pos + Vec3::Y * icon_size, color);
        // Small diagonal for diamond shape
        gizmos.line(pos - Vec3::new(icon_size * 0.6, icon_size * 0.6, 0.0), pos + Vec3::new(icon_size * 0.6, icon_size * 0.6, 0.0), color.with_alpha(0.5));
    }
}

/// Lazily add a sphere mesh + emissive material to fleet entities that don't have one yet.
pub fn ensure_fleet_meshes(
    mut commands: Commands,
    fleets_without_mesh: Query<Entity, (With<Fleet>, Without<FleetMesh>)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for entity in fleets_without_mesh.iter() {
        let mesh = meshes.add(Sphere::new(6.0).mesh().uv(16, 8));
        let material = materials.add(StandardMaterial {
            base_color: Color::srgb(0.3, 0.9, 0.4),
            emissive: LinearRgba::new(0.6, 1.8, 0.8, 1.0),
            unlit: true,
            ..default()
        });
        commands.entity(entity).insert((
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::default(),
            // Start hidden; update_fleet_transforms sets correct position + visibility
            // the very next frame, preventing a one-frame flash at world origin.
            Visibility::Hidden,
            FleetMesh,
        ));
    }
}

/// Update fleet mesh sphere colours based on transit state.
///
/// In-transit fleets travel along the **cyan** trajectory arc, so the default green
/// blends in.  Switching to **bright yellow** while in transit gives strong contrast
/// at all zoom levels.  Runs every frame (fleet count is small, so the overhead is
/// negligible; running unconditionally avoids `RemovedComponents` bookkeeping).
pub fn update_fleet_mesh_materials(
    fleet_query: Query<(Option<&ActiveManeuver>, &MeshMaterial3d<StandardMaterial>), (With<Fleet>, With<FleetMesh>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (maybe_maneuver, mat_handle) in fleet_query.iter() {
        if let Some(mat) = materials.get_mut(&mat_handle.0) {
            if maybe_maneuver.is_some() {
                // In transit: bright yellow — high contrast against the cyan arc.
                mat.base_color = Color::srgb(1.0, 0.92, 0.2);
                mat.emissive = LinearRgba::new(3.5, 2.8, 0.4, 1.0);
            } else {
                // In orbit: standard green.
                mat.base_color = Color::srgb(0.3, 0.9, 0.4);
                mat.emissive = LinearRgba::new(0.6, 1.8, 0.8, 1.0);
            }
        }
    }
}

/// Keep each fleet entity's `Transform` in sync with its position, and control
/// mesh visibility:
///
/// - **Orbiting + unselected**: hidden (the orbit ring gizmo provides context).
/// - **Orbiting + selected**: shown just outside the parent body's visual sphere.
/// - **In-transit (local)**: follows a visual arc between bodies.
/// - **In-transit (heliocentric)**: computed from `SpaceCoordinates`.
///
/// The mesh sphere is also hidden in Starmap view.
pub fn update_fleet_transforms(
    mut fleet_query: Query<
        (Entity, &SpaceCoordinates, &mut Transform, &mut Visibility, Option<&FleetOrbit>, Option<&ActiveManeuver>),
        With<Fleet>,
    >,
    body_query: Query<(&Transform, &CelestialBody, Option<&LogicalParent>), Without<Fleet>>,
    kepler_query: Query<&KeplerOrbit, Without<Fleet>>,
    amp_query: Query<&LocalOrbitAmplification, Without<Fleet>>,
    floating_origin: Option<Res<FloatingOrigin>>,
    view_mode: Res<ViewMode>,
    sim_time: Res<SimulationTime>,
    fleet_ui_state: Res<FleetUiState>,
) {
    let origin_offset = floating_origin
        .as_ref()
        .map(|fo| fo.position)
        .unwrap_or(DVec3::ZERO);
    let elapsed = sim_time.elapsed_seconds();
    let selected = fleet_ui_state.selected_fleet;

    for (entity, sc, mut transform, mut vis, maybe_orbit, maybe_maneuver) in fleet_query.iter_mut() {
        // Starmap: hide all fleet spheres (gizmos handle that view).
        if *view_mode == ViewMode::Starmap {
            *vis = Visibility::Hidden;
            continue;
        }

        let is_selected = selected == Some(entity);
        let is_in_transit = maybe_maneuver.is_some();

        // Hide parked (non-transiting) fleets that are not selected.
        if !is_in_transit && !is_selected {
            *vis = Visibility::Hidden;
            // Still update position so the reticule and orbit ring are accurate.
        } else {
            *vis = Visibility::Inherited;
        }

        if let Some(orbit) = maybe_orbit {
            // ── Orbiting fleet: place at visual orbit position ──
            if let Ok((body_transform, body, _)) = body_query.get(orbit.body) {
                let dir = Vec3::new(
                    orbit.angle_rad.cos() as f32,
                    orbit.angle_rad.sin() as f32,
                    0.0,
                );
                // For star-orbiting fleets (Lagrange points), the orbit radius is
                // heliocentric AU — convert to visual units with SCALING_FACTOR.
                // For all other bodies the fleet parks just outside the visual sphere.
                let visual_orbit = if body.body_type == BodyType::Star {
                    orbit.radius_au as f32 * SCALING_FACTOR as f32
                } else {
                    fleet_parking_visual_radius(body.visual_radius)
                };
                transform.translation = body_transform.translation + dir * visual_orbit;
            }
        } else if let Some(maneuver) = maybe_maneuver {
            // ── In-transit: check whether this is a local or heliocentric transfer ──
            let center_is_star = body_query.get(maneuver.orbit_center)
                .map(|(_, b, _)| b.body_type == BodyType::Star)
                .unwrap_or(true);

            if !center_is_star {
                // Local transfer: follow the same cubic Bezier as the trajectory gizmo.
                let origin_data = body_query.get(maneuver.origin_body).ok().map(|(t, b, _)| (t.translation, fleet_parking_visual_radius(b.visual_radius)));
                let dest_data   = body_query.get(maneuver.destination_body).ok().map(|(t, b, _)| (t.translation, fleet_parking_visual_radius(b.visual_radius)));
                if let (Some((op_current, origin_ring_r)), Some((dp_now, dest_ring_r))) = (origin_data, dest_data) {
                    let cv_current = body_query.get(maneuver.orbit_center)
                        .map(|(t, _, _)| t.translation).unwrap_or(Vec3::ZERO);

                    // Once departed, fix the origin at the moon's departure-time position
                    // so the fleet moves with the planet but not the moon's orbit.
                    let op = {
                        let op_absolute = predict_body_visual_pos(
                            maneuver.origin_body,
                            maneuver.departure_time,
                            &body_query,
                            &kepler_query,
                            &amp_query,
                        ).unwrap_or(op_current);
                        let cv_at_departure = predict_body_visual_pos(
                            maneuver.orbit_center,
                            maneuver.departure_time,
                            &body_query,
                            &kepler_query,
                            &amp_query,
                        ).unwrap_or(cv_current);
                        op_absolute - cv_at_departure + cv_current
                    };

                    let dp_absolute = predict_body_visual_pos(
                        maneuver.destination_body,
                        maneuver.arrival_time,
                        &body_query,
                        &kepler_query,
                        &amp_query,
                    ).unwrap_or(dp_now);

                    let cv_predicted = predict_body_visual_pos(
                        maneuver.orbit_center,
                        maneuver.arrival_time,
                        &body_query,
                        &kepler_query,
                        &amp_query,
                    ).unwrap_or(dp_absolute);

                    let dp = dp_absolute - cv_predicted + cv_current;

                    let progress = maneuver.progress(elapsed) as f32;

                    // Reproduce the EXACT same Bezier as draw_fleet_trajectories.
                    // Use optimal departure angle (direction toward destination) for consistency
                    // with the preview arc — avoids the arc shape changing based on the orbital
                    // phase when Execute was clicked.
                    let dep_angle = optimal_departure_angle(op, dp);
                    let dir_dep = Vec3::new(dep_angle.cos(), dep_angle.sin(), 0.0);
                    let p0 = op + dir_dep * origin_ring_r;

                    // Determine if this is an inward (orbit-lowering) transfer so
                    // the fleet follows the same arc shape as the preview/trajectory.
                    let origin_lp = body_query.get(maneuver.origin_body)
                        .ok().and_then(|(_, _, lp)| lp.map(|lp| lp.0));
                    let dest_lp = body_query.get(maneuver.destination_body)
                        .ok().and_then(|(_, _, lp)| lp.map(|lp| lp.0));
                    let is_inward = if origin_lp == Some(maneuver.destination_body) {
                        true  // Origin orbits the destination (e.g. Moon → Earth)
                    } else if dest_lp == Some(maneuver.origin_body) {
                        false // Destination orbits the origin (e.g. Earth → Moon)
                    } else {
                        op.length_squared() > dp.length_squared()
                    };
                    // Departure tangent: prograde (CCW) for outward, retrograde (CW) for inward.
                    let tang_origin = if is_inward {
                        Vec3::new(dep_angle.sin(), -dep_angle.cos(), 0.0)  // CW
                    } else {
                        Vec3::new(-dep_angle.sin(), dep_angle.cos(), 0.0)  // CCW
                    };

                    let radial_raw = dp - cv_current;
                    let radial = if radial_raw.length() > 1.0 {
                        radial_raw.normalize()
                    } else {
                        (dp - op).normalize_or_zero()
                    };
                    let inward = (op - dp).normalize_or_zero();
                    let p3 = dp + inward * dest_ring_r;
                    let tang_d_a = Vec3::new(-radial.y, radial.x, 0.0);
                    let tang_dest = if tang_d_a.dot(tang_origin) >= 0.0 { tang_d_a } else { -tang_d_a };

                    let ctrl_len = (p3 - p0).length() * 0.40;
                    let p1 = p0 + tang_origin * ctrl_len;
                    let p2 = p3 - tang_dest   * ctrl_len;

                    let t = progress;
                    if maneuver.is_kinematic() {
                        // Kinematic (brachistochrone): straight-line interpolation.
                        transform.translation = p0 + (p3 - p0) * t;
                    } else {
                        // Ballistic (Hohmann): cubic Bézier matching the trajectory gizmo.
                        let u = 1.0 - t;
                        transform.translation = u*u*u*p0 + 3.0*u*u*t*p1 + 3.0*u*t*t*p2 + t*t*t*p3;
                    }

                    // Hide the sphere while still inside the origin or destination orbit ring.
                    let inside_origin = transform.translation.distance(op) < origin_ring_r;
                    let inside_dest   = transform.translation.distance(dp) < dest_ring_r;
                    if inside_origin || inside_dest {
                        *vis = Visibility::Hidden;
                    }
                }
            } else {
                // Heliocentric transfer: physics-based position
                let render_du = (sc.position - origin_offset) * SCALING_FACTOR;
                transform.translation = Vec3::new(
                    render_du.x as f32,
                    render_du.y as f32,
                    render_du.z as f32,
                );
            }
        } else {
            // Fallback: physics position
            let render_du = (sc.position - origin_offset) * SCALING_FACTOR;
            transform.translation = Vec3::new(
                render_du.x as f32,
                render_du.y as f32,
                render_du.z as f32,
            );
        }
    }
}

/// Draw dashed orbit rings in System view:
///
/// - **Parked** fleet (selected): one ring around the orbit body.
/// - **In-transit** fleet (selected, local transfer): departure ring around `origin_body`
///   and arrival ring around `destination_body`.
///
/// Ring radius = `body.visual_radius × 2.0`, matching the fleet's parking orbit
/// visual position and the arc clip boundary used by `draw_fleet_trajectories`.
pub fn draw_fleet_orbit_rings(
    mut gizmos: Gizmos,
    fleet_ui_state: Res<FleetUiState>,
    parked_query: Query<(Entity, &FleetOrbit), With<Fleet>>,
    transit_query: Query<(Entity, &ActiveManeuver), With<Fleet>>,
    body_query: Query<(&Transform, &CelestialBody), Without<Fleet>>,
    view_mode: Res<ViewMode>,
) {
    if *view_mode != ViewMode::System {
        return;
    }
    let Some(selected) = fleet_ui_state.selected_fleet else {
        return;
    };

    let draw_ring = |gizmos: &mut Gizmos, center: Vec3, radius: f32, color: Color| {
        let tau = std::f32::consts::TAU;
        draw_dashed_curve(
            gizmos,
            |t| {
                let a = t * tau;
                center + Vec3::new(a.cos() * radius, a.sin() * radius, 0.0)
            },
            32,
            |_| color,
        );
    };

    if let Ok((_, orbit)) = parked_query.get(selected) {
        // ── Parked: single green ring around orbit body ──
        if let Ok((body_transform, body)) = body_query.get(orbit.body) {
            // For LP-stationed fleets (direction == 0.0) the fleet is frozen at a
            // heliocentric Lagrange-point position; skip the 1-AU orbit ring that
            // would otherwise dominate the view.  The green selection reticule from
            // `draw_fleet_selection_reticule` already marks the fleet's position.
            if body.body_type == BodyType::Star && orbit.direction == 0.0 {
                return;
            }

            // For star-orbiting fleets (Lagrange points), the orbit radius is
            // heliocentric AU — convert to visual units.  This draws a large
            // ring matching the fleet's actual heliocentric orbital path.
            let ring_radius = if body.body_type == BodyType::Star {
                orbit.radius_au as f32 * SCALING_FACTOR as f32
            } else {
                fleet_parking_visual_radius(body.visual_radius)
            };
            draw_ring(
                &mut gizmos,
                body_transform.translation,
                ring_radius,
                Color::srgba(0.2, 0.9, 0.3, 0.30),
            );
        }
    } else if let Ok((_, maneuver)) = transit_query.get(selected) {
        // Only draw rings for local (planet-centric) transfers
        let center_is_star = body_query.get(maneuver.orbit_center)
            .map(|(_, b)| b.body_type == BodyType::Star)
            .unwrap_or(true);
        if center_is_star {
            // Heliocentric transfer: draw a dim departure ring near origin and a
            // brighter cyan arrival ring near the destination so the user has visual
            // anchors at both ends regardless of camera position.
            if let Ok((body_transform, body)) = body_query.get(maneuver.origin_body) {
                draw_ring(
                    &mut gizmos,
                    body_transform.translation,
                    fleet_parking_visual_radius(body.visual_radius),
                    Color::srgba(0.2, 0.9, 0.3, 0.15),
                );
            }
            if let Ok((body_transform, body)) = body_query.get(maneuver.destination_body) {
                draw_ring(
                    &mut gizmos,
                    body_transform.translation,
                    fleet_parking_visual_radius(body.visual_radius),
                    Color::srgba(0.3, 0.8, 1.0, 0.35),
                );
            }
            return;
        }

        // Departure ring — dim green
        if let Ok((body_transform, body)) = body_query.get(maneuver.origin_body) {
            draw_ring(
                &mut gizmos,
                body_transform.translation,
                fleet_parking_visual_radius(body.visual_radius),
                Color::srgba(0.2, 0.9, 0.3, 0.20),
            );
        }
        // Arrival ring — brighter cyan
        if let Ok((body_transform, body)) = body_query.get(maneuver.destination_body) {
            draw_ring(
                &mut gizmos,
                body_transform.translation,
                fleet_parking_visual_radius(body.visual_radius),
                Color::srgba(0.3, 0.8, 1.0, 0.35),
            );
        }
    }
}
/// Draw a dashed amber preview arc when a destination is selected in the Transfer Planner popup.
///
/// * Predicts where the destination body will be at current_time + transfer_time.
/// * Draws a KSP-style ghost body (amber dashed circles) at the predicted intercept.
/// * Arc departure is anchored to the optimal firing position (direction toward predicted
///   destination), not the fleet's rotating local orbit angle, so the preview is stable.
pub fn draw_fleet_transfer_preview(
    mut gizmos: Gizmos,
    fleet_query: Query<(Entity, &Transform, Option<&FleetOrbit>, Option<&ActiveManeuver>), With<Fleet>>,
    body_query: Query<(&Transform, &CelestialBody, Option<&LogicalParent>), Without<Fleet>>,
    kepler_query: Query<&KeplerOrbit, Without<Fleet>>,
    amp_query: Query<&LocalOrbitAmplification, Without<Fleet>>,
    fleet_ui_state: Res<FleetUiState>,
    view_mode: Res<ViewMode>,
    sim_time: Res<SimulationTime>,
) {
    if *view_mode != ViewMode::System { return; }
    if !fleet_ui_state.show_transfer_popup { return; }
    let Some(fleet_entity) = fleet_ui_state.selected_fleet else { return; };

    // Hoist fleet-state lookup so both LP and regular branches can share it.
    let Ok((_, _, maybe_orbit, maybe_maneuver)) = fleet_query.get(fleet_entity) else { return; };
    // Hide the preview as soon as a transfer has been planned/executed — the real
    // trajectory arc (drawn by draw_fleet_orbit_rings / maneuver gizmos) takes over.
    if maybe_maneuver.is_some() { return; }
    let elapsed = sim_time.elapsed_seconds();

    let current_sim_s      = elapsed;
    let departure_offset_s = fleet_ui_state.departure_offset_days * 86_400.0;
    let departure_s        = current_sim_s + departure_offset_s;

    // ── Lagrange-point target preview ─────────────────────────────────────────
    // LP transfers have no body entity; draw an arc to the predicted LP position.
    if let Some(lp) = &fleet_ui_state.target_lagrange {
        let origin_body = if let Some(orbit) = maybe_orbit {
            orbit.body
        } else {
            return;
        };

        let Ok((origin_transform, origin_body_data, _)) = body_query.get(origin_body) else { return; };
        // Predict origin body position at planned departure so the start mark moves
        // when the player drags the departure slider.
        let op = predict_body_visual_pos(origin_body, departure_s, &body_query, &kepler_query, &amp_query)
            .unwrap_or(origin_transform.translation);
        let origin_ring_r = fleet_parking_visual_radius(origin_body_data.visual_radius);

        let travel_time_s = if fleet_ui_state.selected_option < fleet_ui_state.computed_options.len() {
            fleet_ui_state.computed_options[fleet_ui_state.selected_option].transfer_time_s
        } else if let Some(pt) = &fleet_ui_state.planned_transfer {
            pt.duration_s
        } else {
            0.0
        };

        // Predict the LP's parent planet position to get LP direction.
        let planet_pos_now = body_query.get(lp.planet_entity)
            .ok().map(|(t, _, _)| t.translation).unwrap_or(Vec3::ZERO);

        // For co-orbital L3/L4/L5 phasing, show the arc toward the CURRENT LP
        // marker position (the one visible on screen) instead of a predicted
        // position many years in the future.  Phasing maneuvers keep the fleet
        // near the planet's orbit, so the current marker is the intuitive target.
        // L1/L2 are radial transfers that use the predicted arrival position.
        let co_orbital_lp = matches!(lp.point, 3 | 4 | 5);
        let planet_ref_pos = if co_orbital_lp {
            planet_pos_now
        } else {
            predict_body_visual_pos(
                lp.planet_entity,
                departure_s + travel_time_s,
                &body_query,
                &kepler_query,
                &amp_query,
            ).unwrap_or(planet_pos_now)
        };

        // L3 is opposite the planet; L4/L5 are ±60°; L1/L2 are along the planet axis.
        let planet_angle = planet_ref_pos.y.atan2(planet_ref_pos.x) as f64;
        let lp_angle = match lp.point {
            3 => planet_angle + std::f64::consts::PI,
            4 => planet_angle + std::f64::consts::FRAC_PI_3,
            5 => planet_angle - std::f64::consts::FRAC_PI_3,
            _ => planet_angle,
        } as f32;
        let lp_render_dist = lp.radius_au as f32 * SCALING_FACTOR as f32;
        let dp = Vec3::new(lp_angle.cos() * lp_render_dist, lp_angle.sin() * lp_render_dist, 0.0);

        // Small marker radius for the LP arrival point (no physical body radius).
        let lp_marker_r = (lp.radius_au as f32 * SCALING_FACTOR as f32 * 0.015).clamp(10.0, 50.0);

        let departure_angle = optimal_departure_angle(op, dp);
        let dir_dep = Vec3::new(departure_angle.cos(), departure_angle.sin(), 0.0);
        let p0 = op + dir_dep * origin_ring_r;
        let tang_origin = Vec3::new(-departure_angle.sin(), departure_angle.cos(), 0.0);

        let inward = (op - dp).normalize_or_zero();
        let p3 = dp + inward * lp_marker_r;
        // Arrival tangent: perpendicular to the heliocentric radial direction (prograde).
        let radial_dest = dp.normalize_or_zero();
        let tang_dest = Vec3::new(-radial_dest.y, radial_dest.x, 0.0);

        let ctrl_len = (p3 - p0).length() * 0.40;
        let p1 = p0 + tang_origin * ctrl_len;
        let p2 = p3 - tang_dest   * ctrl_len;
        let lp_bezier = |t: f32| -> Vec3 {
            let u = 1.0 - t;
            u*u*u*p0 + 3.0*u*u*t*p1 + 3.0*u*t*t*p2 + t*t*t*p3
        };

        // Dashed amber arc — arc-length-uniform.
        draw_dashed_curve(
            &mut gizmos, &lp_bezier, 24,
            |f| Color::srgba(1.0, 0.75, 0.15, 0.70 - 0.35 * f),
        );

        // LP marker: crosshair + dashed circle in cyan-blue.
        let lp_color = Color::srgba(0.5, 0.85, 1.0, 0.85);
        let cs = lp_marker_r * 1.4;
        gizmos.line(dp - Vec3::X * cs, dp + Vec3::X * cs, lp_color);
        gizmos.line(dp - Vec3::Y * cs, dp + Vec3::Y * cs, lp_color);
        {
            let tau = std::f32::consts::TAU;
            draw_dashed_curve(
                &mut gizmos,
                |t| {
                    let a = t * tau;
                    dp + Vec3::new(a.cos() * lp_marker_r, a.sin() * lp_marker_r, 0.0)
                },
                12,
                |_| lp_color,
            );
        }

        return;
    }

    // ── Fleet-intercept target preview ──────────────────────────────────────
    if let Some(target_fleet_entity) = fleet_ui_state.target_fleet {
        if target_fleet_entity == fleet_entity { return; }

        let origin_body = if let Some(orbit) = maybe_orbit {
            orbit.body
        } else {
            return;
        };

        let Ok((origin_transform, origin_body_data, _)) = body_query.get(origin_body) else { return; };
        let op = predict_body_visual_pos(origin_body, departure_s, &body_query, &kepler_query, &amp_query)
            .unwrap_or(origin_transform.translation);
        let origin_ring_r = fleet_parking_visual_radius(origin_body_data.visual_radius);

        // Use the target fleet's current visual Transform position — fleets have no Keplerian
        // orbit to predict future positions from, so we target where they are right now.
        let Ok((_, target_fleet_transform, _, _)) = fleet_query.get(target_fleet_entity) else { return; };
        let dp = target_fleet_transform.translation;

        let is_kinematic = fleet_ui_state.computed_options
            .get(fleet_ui_state.selected_option)
            .map(|opt| opt.label == "Full Thrust" || opt.label.contains("Coast") || opt.label == "Max Speed" || opt.label.contains("Direct"))
            .unwrap_or(false);

        // Fixed visual marker radius — no physical body size for a fleet.
        let marker_r = 15.0_f32;

        let is_inward = op.length_squared() > dp.length_squared();
        let departure_angle = optimal_departure_angle(op, dp);
        let dir_dep = Vec3::new(departure_angle.cos(), departure_angle.sin(), 0.0);
        let p0 = op + dir_dep * origin_ring_r;
        let tang_origin = if is_inward {
            Vec3::new(departure_angle.sin(), -departure_angle.cos(), 0.0)
        } else {
            Vec3::new(-departure_angle.sin(), departure_angle.cos(), 0.0)
        };

        let inward = (op - dp).normalize_or_zero();
        let p3 = dp + inward * marker_r;
        let radial_dest = dp.normalize_or_zero();
        let tang_d_a = Vec3::new(-radial_dest.y, radial_dest.x, 0.0);
        let tang_dest = if tang_d_a.dot(tang_origin) >= 0.0 { tang_d_a } else { -tang_d_a };

        if is_kinematic {
            draw_dashed_curve(
                &mut gizmos,
                |t| p0 + (p3 - p0) * t,
                24,
                |f| Color::srgba(1.0, 0.75, 0.15, 0.70 - 0.35 * f),
            );
        } else {
            let ctrl_len = (p3 - p0).length() * 0.40;
            let p1 = p0 + tang_origin * ctrl_len;
            let p2 = p3 - tang_dest * ctrl_len;
            let bezier = move |t: f32| -> Vec3 {
                let u = 1.0 - t;
                u*u*u*p0 + 3.0*u*u*t*p1 + 3.0*u*t*t*p2 + t*t*t*p3
            };
            draw_dashed_curve(
                &mut gizmos, &bezier, 24,
                |f| Color::srgba(1.0, 0.75, 0.15, 0.70 - 0.35 * f),
            );
        }

        // Fleet-target marker: crosshair + dashed circle in orange-red.
        let fleet_color = Color::srgba(1.0, 0.4, 0.1, 0.9);
        let cs = marker_r * 1.5;
        gizmos.line(dp - Vec3::X * cs, dp + Vec3::X * cs, fleet_color);
        gizmos.line(dp - Vec3::Y * cs, dp + Vec3::Y * cs, fleet_color);
        {
            let tau = std::f32::consts::TAU;
            draw_dashed_curve(
                &mut gizmos,
                |t| {
                    let a = t * tau;
                    dp + Vec3::new(a.cos() * marker_r, a.sin() * marker_r, 0.0)
                },
                10,
                |_| fleet_color,
            );
        }

        return;
    }

    let Some(target_entity) = fleet_ui_state.target_body   else { return; };

    let origin_body = if let Some(orbit) = maybe_orbit {
        orbit.body
    } else {
        return;
    };

    if origin_body == target_entity { return; }

    let Ok((origin_transform, origin_body_data, origin_lp)) = body_query.get(origin_body)   else { return; };
    let Ok((dest_transform_now, dest_body_data, dest_lp))  = body_query.get(target_entity) else { return; };

    // Predict origin body position at planned departure time so the start mark moves
    // when the player drags the departure slider.
    let op            = predict_body_visual_pos(origin_body, departure_s, &body_query, &kepler_query, &amp_query)
        .unwrap_or(origin_transform.translation);
    let origin_ring_r = fleet_parking_visual_radius(origin_body_data.visual_radius);
    let dest_visual_r = dest_body_data.visual_radius;
    let dest_ring_r   = fleet_parking_visual_radius(dest_visual_r);

    // Travel time from selected transfer option (or 0 = show arc to current position).
    let travel_time_s = if fleet_ui_state.selected_option < fleet_ui_state.computed_options.len() {
        fleet_ui_state.computed_options[fleet_ui_state.selected_option].transfer_time_s
    } else if let Some(pt) = &fleet_ui_state.planned_transfer {
        pt.duration_s
    } else {
        0.0
    };

    // Predict destination body position at planned departure + travel time so the
    // ghost mark moves when the player drags the departure slider.
    let dp_absolute = predict_body_visual_pos(
        target_entity,
        departure_s + travel_time_s,
        &body_query,
        &kepler_query,
        &amp_query,
    ).unwrap_or(dest_transform_now.translation);

    let orbit_center = dest_lp.map(|lp| lp.0).unwrap_or(target_entity);
    let cv_predicted = predict_body_visual_pos(
        orbit_center,
        departure_s + travel_time_s,
        &body_query,
        &kepler_query,
        &amp_query,
    ).unwrap_or(dp_absolute);

    let cv_at_departure = predict_body_visual_pos(
        orbit_center,
        departure_s,
        &body_query,
        &kepler_query,
        &amp_query,
    ).unwrap_or(cv_predicted);

    let dp = dp_absolute - cv_predicted + cv_at_departure;

    // ── Determine if this is an inward (orbit-lowering) transfer ─────────────
    // Inward transfers require a retrograde departure burn (CW tangent), while
    // outward transfers use a prograde departure burn (CCW tangent).
    let is_inward = if origin_lp.map(|lp| lp.0) == Some(target_entity) {
        // Origin orbits the destination (e.g. Moon → Earth): always inward.
        true
    } else if dest_lp.map(|lp| lp.0) == Some(origin_body) {
        // Destination orbits the origin (e.g. Earth → Moon): always outward.
        false
    } else {
        // Same parent (e.g. Mars → Earth): compare distances from the star.
        op.length_squared() > dp.length_squared()
    };

    // Stable optimal departure angle: direction from the origin body toward the predicted
    // destination position.  The fleet waits in its parking orbit until it reaches this
    // angular position, then fires — this is always the ΔV-optimal departure point.
    // Using this (instead of the fleet's rotating orbit.angle_rad) keeps the preview arc
    // anchored; it only drifts slowly as the target body advances in its own orbit.
    let departure_angle = optimal_departure_angle(op, dp);

    // Departure point on the origin ring, facing the destination.
    let dir_dep     = Vec3::new(departure_angle.cos(), departure_angle.sin(), 0.0);
    let p0          = op + dir_dep * origin_ring_r;
    // Departure tangent: prograde (CCW) for outward, retrograde (CW) for inward.
    let tang_origin = if is_inward {
        Vec3::new(departure_angle.sin(), -departure_angle.cos(), 0.0)  // CW
    } else {
        Vec3::new(-departure_angle.sin(), departure_angle.cos(), 0.0)  // CCW
    };

    // Arrival: point on the ring approached from the origin-facing (inward) side.
    let radial_dest_raw = dp - cv_at_departure;
    let radial_dest = if radial_dest_raw.length() > 1.0 {
        radial_dest_raw.normalize()
    } else {
        (dp - op).normalize_or_zero()
    };
    let inward = (op - dp).normalize_or_zero();
    let p3 = dp + inward * dest_ring_r;
    let tang_d_a  = Vec3::new(-radial_dest.y, radial_dest.x, 0.0);
    let tang_dest = if tang_d_a.dot(tang_origin) >= 0.0 { tang_d_a } else { -tang_d_a };

    // Check whether the selected option is a kinematic transfer.
    let is_kinematic = fleet_ui_state.computed_options
        .get(fleet_ui_state.selected_option)
        .map(|opt| opt.label == "Full Thrust" || opt.label.contains("Coast") || opt.label == "Max Speed" || opt.label.contains("Direct"))
        .unwrap_or(false);

    // Dashed amber arc — straight line for kinematic transfers, cubic Bezier for ballistic arcs.
    if is_kinematic {
        draw_dashed_curve(
            &mut gizmos,
            |t| p0 + (p3 - p0) * t,
            24,
            |f| Color::srgba(1.0, 0.75, 0.15, 0.70 - 0.35 * f),
        );
    } else {
        let ctrl_len = (p3 - p0).length() * 0.40;
        let mut p1 = p0 + tang_origin * ctrl_len;
        let mut p2 = p3 - tang_dest   * ctrl_len;
        // Smoothly interpolate z so the arc bridges the two orbital planes
        // even when the tangent vectors are 2D (z = 0).
        p1.z = p0.z + (p3.z - p0.z) * 0.33;
        p2.z = p0.z + (p3.z - p0.z) * 0.67;
        let bezier = move |t: f32| -> Vec3 {
            let u = 1.0 - t;
            u*u*u*p0 + 3.0*u*u*t*p1 + 3.0*u*t*t*p2 + t*t*t*p3
        };
        draw_dashed_curve(
            &mut gizmos, &bezier, 24,
            |f| Color::srgba(1.0, 0.75, 0.15, 0.70 - 0.35 * f),
        );
    }

    // Ghost body at predicted arrival position.
    // In the preview the destination is the body we are flying TO, not the orbit
    // centre, so the outer ring is always wanted here.
    draw_ghost_body(&mut gizmos, dp, dest_ring_r, dest_visual_r, false);

    // ── Arrival highlight arc on target body's orbit ring ─────────────────────
    // When the predicted arrival position is far off-screen (e.g. a distant moon
    // like Fornjot after a long transfer window), the bezier arc and ghost body
    // become invisible.  To keep the player informed, we draw a thin bright-orange
    // arc segment directly on the target body's **orbit ring** (centered on the
    // orbit parent, which typically remains on-screen) at the angular position
    // where the body will be at arrival time.  This is always visible as long as
    // the orbit parent is in view.
    //
    // The arc spans ±30° of mean anomaly around the arrival point and fades
    // toward its ends, creating a clear "arrival window" marker on the orbit ring.
    if let Ok(kepler) = kepler_query.get(target_entity) {
        let amp = amp_query.get(target_entity).map(|a| a.0 as f64).unwrap_or(1.0);
        let arrival_time = departure_s + travel_time_s;
        let ma_arrival = kepler.mean_anomaly_epoch + kepler.mean_motion * arrival_time;

        // Sweep ±30° of mean anomaly around the arrival point.
        const ARRIVAL_ARC_SPAN: f64 = std::f64::consts::PI / 6.0; // 30°
        const ARRIVAL_ARC_STEPS: usize = 32;

        let mut prev: Option<Vec3> = None;
        for i in 0..=ARRIVAL_ARC_STEPS {
            let frac = i as f64 / ARRIVAL_ARC_STEPS as f64;
            let ma = ma_arrival - ARRIVAL_ARC_SPAN + frac * 2.0 * ARRIVAL_ARC_SPAN;
            let pos_au = orbit_position_from_mean_anomaly(kepler, ma);
            let pos_scaled = Vec3::new(
                (pos_au.x * SCALING_FACTOR * amp) as f32,
                (pos_au.y * SCALING_FACTOR * amp) as f32,
                (pos_au.z * SCALING_FACTOR * amp) as f32,
            );
            // Arc is anchored to the orbit center at departure time — the same
            // reference frame used for `dp`, so the arc midpoint (frac=0.5) sits
            // exactly at the ghost body marker when it is on-screen.
            let arc_pos = cv_at_departure + pos_scaled;

            // Fade to transparent at the arc ends, full brightness at the midpoint.
            let edge_fade = 1.0_f32 - (2.0 * frac as f32 - 1.0).abs();
            let alpha = 0.18 + 0.72 * edge_fade;
            let arc_color = Color::srgba(1.0, 0.55, 0.1, alpha);

            if let Some(p) = prev {
                gizmos.line(p, arc_pos, arc_color);
            }
            prev = Some(arc_pos);
        }
    }
}

/// Draw the two-leg slingshot arc when a gravity-assist flyby is selected.
///
/// The trajectory is drawn as a **single continuous curve** that passes through
/// the flyby body position, with a visible "bend" showing the gravitational
/// deflection.  Colour transitions from lime-green (approach leg) to magenta
/// (departure leg) at the flyby point.
///
/// - A bright yellow cross marker is drawn at the predicted flyby intercept.
/// - A ghost-body circle is drawn at the predicted destination position.
///
/// The regular amber preview arc (`draw_fleet_transfer_preview`) continues to
/// be drawn; this system adds the two-colour slingshot overlay on top.
pub fn draw_gravity_assist_preview(
    mut gizmos: Gizmos,
    fleet_query: Query<(Entity, Option<&FleetOrbit>, Option<&ActiveManeuver>), With<Fleet>>,
    body_query: Query<(&Transform, &CelestialBody, Option<&LogicalParent>), Without<Fleet>>,
    kepler_query: Query<&KeplerOrbit, Without<Fleet>>,
    amp_query: Query<&LocalOrbitAmplification, Without<Fleet>>,
    fleet_ui_state: Res<FleetUiState>,
    view_mode: Res<ViewMode>,
    sim_time: Res<SimulationTime>,
) {
    if *view_mode != ViewMode::System { return; }
    if !fleet_ui_state.show_transfer_popup { return; }
    let Some(sel_ga_idx) = fleet_ui_state.selected_gravity_assist else { return; };
    let Some(fleet_entity) = fleet_ui_state.selected_fleet else { return; };
    let Some(target_entity) = fleet_ui_state.target_body else { return; };
    let Some(ga_entry) = fleet_ui_state.gravity_assist_candidates.get(sel_ga_idx) else { return; };

    let flyby_entity = ga_entry.flyby_entity;
    let leg1_time = ga_entry.option.leg1_time_s;
    let total_time = ga_entry.option.total_time_s;
    let departure_offset_s = fleet_ui_state.departure_offset_days * 86_400.0;

    let Ok((_, maybe_orbit, maybe_maneuver)) = fleet_query.get(fleet_entity) else { return; };
    // Hide the gravity-assist preview as soon as a transfer has been planned/executed.
    if maybe_maneuver.is_some() { return; }
    let origin_body = if let Some(orbit) = maybe_orbit {
        orbit.body
    } else {
        return;
    };
    // Sanity: all three bodies must be distinct
    if origin_body == target_entity || origin_body == flyby_entity || flyby_entity == target_entity {
        return;
    }

    let current_sim_s = sim_time.elapsed_seconds();
    let depart_s = current_sim_s + departure_offset_s;

    let Ok((origin_t, origin_bd, _))  = body_query.get(origin_body)  else { return; };
    let Ok((_, flyby_bd, _))           = body_query.get(flyby_entity) else { return; };
    let Ok((dest_t,   dest_bd,   _))   = body_query.get(target_entity) else { return; };

    // Predict origin body position at planned departure time so the start mark moves
    // when the player drags the departure slider.
    let op = predict_body_visual_pos(origin_body, depart_s, &body_query, &kepler_query, &amp_query)
        .unwrap_or(origin_t.translation);
    let origin_ring_r = fleet_parking_visual_radius(origin_bd.visual_radius);
    let dest_ring_r   = fleet_parking_visual_radius(dest_bd.visual_radius);

    // Predict flyby body position at end of Leg 1
    let fp = predict_body_visual_pos(
        flyby_entity, depart_s + leg1_time,
        &body_query, &kepler_query, &amp_query,
    ).unwrap_or_else(|| body_query.get(flyby_entity).map(|(t, _, _)| t.translation).unwrap_or(Vec3::ZERO));

    // Predict destination body position at end of full two-leg transfer
    let dp = predict_body_visual_pos(
        target_entity, depart_s + total_time,
        &body_query, &kepler_query, &amp_query,
    ).unwrap_or(dest_t.translation);

    // ── Compute a smooth hyperbolic-flyby trajectory ──────────────────────────
    //
    // Instead of meeting at the planet centre (which creates a sharp corner),
    // the two Bezier legs meet at a *periapsis* point offset from the planet
    // centre.  The offset direction is derived from the hyperbola axis of
    // symmetry: the bisector of the incoming asymptote (away from planet) and
    // the outgoing asymptote (away from planet).  Both legs share the same
    // tangent at the periapsis, producing a C1-continuous curve with a clear
    // smooth gravitational deflection.

    // Departure point on origin ring (same as the regular transfer preview).
    let is_outward1 = fp.length_squared() > op.length_squared();
    let rad_op = op.normalize_or_zero();
    let prograde_op = Vec3::new(-rad_op.y, rad_op.x, 0.0);
    let tang0 = if is_outward1 { prograde_op } else { -prograde_op };
    let dir_dep1 = if is_outward1 { rad_op } else { -rad_op };
    let p0 = op + dir_dep1 * origin_ring_r;

    // Arrival point on destination ring.
    let is_outward2 = dp.length_squared() > fp.length_squared();
    let rad_dp = dp.normalize_or_zero();
    let prograde_dp = Vec3::new(-rad_dp.y, rad_dp.x, 0.0);
    let td2 = if is_outward2 { prograde_dp } else { -prograde_dp };
    let dir_arr2 = if is_outward2 { -rad_dp } else { rad_dp };
    let p3_2 = dp + dir_arr2 * dest_ring_r;

    // ── Hyperbolic periapsis computation ─────────────────────────────────────
    // Approach direction: from origin toward flyby body.
    let dir_approach = (fp - op).normalize_or_zero();
    // Departure direction: from flyby body toward destination.
    let dir_depart  = (dp - fp).normalize_or_zero();

    // The asymptote directions from the focus (planet) are:
    //   incoming:  -dir_approach   (toward where the spacecraft came from)
    //   outgoing:   dir_depart     (toward where the spacecraft is going)
    // Their vector sum bisects the NARROW angle between them, but the
    // periapsis of the hyperbolic trajectory is on the WIDE side (the
    // 360°−δ arc that the spacecraft actually traverses).  So we negate
    // the bisector to get the correct periapsis direction.
    let apse_raw = dir_approach - dir_depart;
    let apse_dir = if apse_raw.length() > 0.001 {
        apse_raw.normalize()
    } else {
        // Near-zero deflection (straight-through): offset perpendicular to approach.
        Vec3::new(-dir_approach.y, dir_approach.x, 0.0)
    };

    // Periapsis offset distance: just outside the flyby ring for visual clarity.
    let flyby_ring_r = fleet_parking_visual_radius(flyby_bd.visual_radius);
    let periapsis_dist = flyby_ring_r * 2.0;
    let periapsis = fp + apse_dir * periapsis_dist;

    // Tangent at periapsis: perpendicular to the apse line — this is the velocity
    // direction at closest approach on a hyperbola.
    let tang_perp_a = Vec3::new(-apse_dir.y, apse_dir.x, 0.0);
    // Choose the sign so it flows from approach to departure direction.
    let peri_tang = if tang_perp_a.dot(dir_depart) >= 0.0 { tang_perp_a } else { -tang_perp_a };

    // ── Build two C1-continuous Bezier legs meeting at periapsis ──────────────
    let cl1 = (periapsis - p0).length() * 0.40;
    let p1_1 = p0       + tang0     * cl1;
    let p2_1 = periapsis - peri_tang * cl1;

    let cl2 = (p3_2 - periapsis).length() * 0.40;
    let p1_2 = periapsis + peri_tang * cl2;
    let p2_2 = p3_2      - td2      * cl2;

    let peri = periapsis; // copy for closures
    let bez1 = move |t: f32| -> Vec3 {
        let u = 1.0 - t;
        u*u*u*p0 + 3.0*u*u*t*p1_1 + 3.0*u*t*t*p2_1 + t*t*t*peri
    };
    let bez2 = move |t: f32| -> Vec3 {
        let u = 1.0 - t;
        u*u*u*peri + 3.0*u*u*t*p1_2 + 3.0*u*t*t*p2_2 + t*t*t*p3_2
    };

    // ── Draw both legs with arc-length-uniform dashing ───────────────────────

    // Leg 1: lime-green approach arc.
    draw_dashed_curve(
        &mut gizmos, &bez1, 24,
        |f| Color::srgba(0.3, 1.0, 0.4, 0.80 - 0.35 * f),
    );

    // Leg 2: magenta departure arc.
    draw_dashed_curve(
        &mut gizmos, &bez2, 24,
        |f| Color::srgba(1.0, 0.3, 0.8, 0.80 - 0.35 * f),
    );

    // ── Flyby node: yellow cross + ghost ring ─────────────────────────────────
    let cross = flyby_ring_r * 2.5;
    let node_color = Color::srgba(1.0, 1.0, 0.3, 0.9);
    gizmos.line(fp - Vec3::X * cross, fp + Vec3::X * cross, node_color);
    gizmos.line(fp - Vec3::Y * cross, fp + Vec3::Y * cross, node_color);
    draw_ghost_body(&mut gizmos, fp, flyby_ring_r * 1.4, flyby_bd.visual_radius * 0.7, false);

    // ── Destination ghost ─────────────────────────────────────────────────────
    draw_ghost_body(&mut gizmos, dp, dest_ring_r, dest_bd.visual_radius, false);
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
        body_query.iter().find(|(_, b)| b.name == name).map(|(e, _)| e)
    };

    // ── Earth Defense Squadron (Nuclear Thermal, Earth orbit) ─────────────────
    if let Some(earth) = find_body("Earth") {
        let radius_au = 6_771.0_f64 * 1_000.0 / AU_IN_METERS;
        let mut fleet = Fleet::new("Earth Defense Squadron".to_string());
        fleet.ships.push(ShipInfo::new("EDS Helios".to_string(),
            ShipClass::Frigate, PropulsionType::NuclearThermal));
        fleet.ships.push(ShipInfo::new("EDS Aurora".to_string(),
            ShipClass::Destroyer, PropulsionType::NuclearThermal));
        commands.spawn((fleet, FleetOrbit::new(earth, radius_au), SpaceCoordinates::default()));
    } else {
        bevy::log::warn!("spawn_initial_fleet: Earth not found");
    }

    // ── Chemical Strike Force (Chemical, Venus orbit) ─────────────────────────
    if let Some(venus) = find_body("Venus") {
        // Venus radius ≈ 6052 km; 400 km altitude orbit
        let radius_au = 6_452.0_f64 * 1_000.0 / AU_IN_METERS;
        let mut fleet = Fleet::new("Chemical Strike Force".to_string());
        fleet.ships.push(ShipInfo::new("CSV Pyrrhus".to_string(),
            ShipClass::Frigate, PropulsionType::Chemical));
        fleet.ships.push(ShipInfo::new("CSV Ares".to_string(),
            ShipClass::Frigate, PropulsionType::Chemical));
        fleet.ships.push(ShipInfo::new("CSV Hammer".to_string(),
            ShipClass::Destroyer, PropulsionType::Chemical));
        commands.spawn((fleet, FleetOrbit::new(venus, radius_au), SpaceCoordinates::default()));
    } else {
        bevy::log::warn!("spawn_initial_fleet: Venus not found");
    }

    // ── Ion Research Fleet (Ion Drive, Mars orbit) ────────────────────────────
    if let Some(mars) = find_body("Mars") {
        // Mars radius ≈ 3390 km; 400 km altitude orbit
        let radius_au = 3_790.0_f64 * 1_000.0 / AU_IN_METERS;
        let mut fleet = Fleet::new("Ion Research Fleet".to_string());
        fleet.ships.push(ShipInfo::new("IRS Odyssey".to_string(),
            ShipClass::ResearchVessel, PropulsionType::IonDrive));
        fleet.ships.push(ShipInfo::new("IRS Pathfinder".to_string(),
            ShipClass::Freighter, PropulsionType::IonDrive));
        commands.spawn((fleet, FleetOrbit::new(mars, radius_au), SpaceCoordinates::default()));
    } else {
        bevy::log::warn!("spawn_initial_fleet: Mars not found");
    }

    // ── Fusion Expeditionary Corps (Fusion Torch, Jupiter orbit) ─────────────
    if let Some(jupiter) = find_body("Jupiter") {
        // Jupiter radius ≈ 71 492 km; 5 000 km altitude orbit
        let radius_au = 76_492.0_f64 * 1_000.0 / AU_IN_METERS;
        let mut fleet = Fleet::new("Fusion Expeditionary Corps".to_string());
        fleet.ships.push(ShipInfo::new("FEC Prometheus".to_string(),
            ShipClass::Frigate, PropulsionType::FusionTorch));
        fleet.ships.push(ShipInfo::new("FEC Titan".to_string(),
            ShipClass::Cruiser, PropulsionType::FusionTorch));
        commands.spawn((fleet, FleetOrbit::new(jupiter, radius_au), SpaceCoordinates::default()));
    } else {
        bevy::log::warn!("spawn_initial_fleet: Jupiter not found");
    }

    // ── Antimatter Vanguard (Antimatter Drive, Saturn orbit) ──────────────────
    if let Some(saturn) = find_body("Saturn") {
        // Saturn radius ≈ 60 268 km; 5 000 km altitude orbit
        let radius_au = 65_268.0_f64 * 1_000.0 / AU_IN_METERS;
        let mut fleet = Fleet::new("Antimatter Vanguard".to_string());
        fleet.ships.push(ShipInfo::new("AMV Singularity".to_string(),
            ShipClass::Destroyer, PropulsionType::AntimatterDrive));
        fleet.ships.push(ShipInfo::new("AMV Horizon".to_string(),
            ShipClass::Frigate, PropulsionType::AntimatterDrive));
        commands.spawn((fleet, FleetOrbit::new(saturn, radius_au), SpaceCoordinates::default()));
    } else {
        bevy::log::warn!("spawn_initial_fleet: Saturn not found");
    }
}
