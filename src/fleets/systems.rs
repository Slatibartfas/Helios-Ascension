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

/// One full visual revolution every 120 real seconds — readable at any time scale.
const VISUAL_ORBIT_RATE: f64 = std::f64::consts::TAU / 40.0;

/// Multiplier applied to a body's visual radius to determine the orbit ring
/// radius and fleet icon parking distance.  1.5× keeps the marker just outside
/// the body's glow without the ring dominating the view.
const FLEET_ORBIT_RADIUS_MULT: f32 = 1.5;

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
        (With<Fleet>, Without<ActiveManeuver>),
    >,
    body_coords: Query<&SpaceCoordinates, Without<Fleet>>,
) {
    // Freeze the visual orbit when the player has paused the simulation.
    let real_delta = if time_scale.is_paused() { 0.0 } else { real_time.delta_secs_f64() };

    for (mut fleet_sc, mut orbit) in fleet_query.iter_mut() {
        // Advance the visual orbital angle at a slow, legible rate.
        // `orbit.direction` is +1 (CCW/prograde) or -1 (CW/retrograde) and is set
        // at insertion to match the arrival arc's tangent direction.
        orbit.angle_rad = (orbit.angle_rad + orbit.direction * VISUAL_ORBIT_RATE * real_delta)
            .rem_euclid(std::f64::consts::TAU);

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
    mut fleet_query: Query<(Entity, &mut Fleet, &ActiveManeuver, &SpaceCoordinates)>,
    center_coords: Query<&SpaceCoordinates, Without<Fleet>>,
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
        // For Lagrange-point transfers the destination is the star, which lacks
        // SpaceCoordinates (it is always at the heliocentric origin DVec3::ZERO).
        let (initial_angle, orbit_direction) = {
            let center_pos = center_coords.get(destination)
                .map(|sc| sc.position)
                .unwrap_or(DVec3::ZERO); // star sits at the heliocentric origin
            let rel = fleet_sc.position - center_pos;
            let pos_angle = rel.y.atan2(rel.x);

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
            let direction = if cross_z >= 0.0 { 1.0 } else { -1.0 };

            (pos_angle, direction)
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

// ── Action processing ─────────────────────────────────────────────────────────

/// Process fleet actions queued by the UI in the previous frame.
pub fn process_fleet_actions(
    mut commands: Commands,
    mut actions: ResMut<PendingFleetActions>,
    sim_time: Res<SimulationTime>,
    orbit_query: Query<&FleetOrbit, With<Fleet>>,
    maneuver_query: Query<(), (With<Fleet>, With<ActiveManeuver>)>,
    mut fleet_query: Query<&mut Fleet>,
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
        let departure_angle = orbit_query.get(action.fleet)
            .map(|o| o.angle_rad as f32)
            .unwrap_or(0.0);
        let maneuver = ActiveManeuver {
            transfer_orbit: t.transfer_orbit,
            orbit_center: t.orbit_center,
            origin_body: t.origin_body,
            departure_time: elapsed,
            arrival_time: elapsed + t.duration_s,
            destination_body: t.destination_body,
            arrival_orbit_radius_au: t.arrival_orbit_radius_au,
            arrival_delta_v_ms: t.arrival_delta_v_ms,
            fuel_used_t: t.fuel_cost_t,
            option_label: t.option_label,
            departure_angle,
        };
        // Remove whatever the fleet currently has (FleetOrbit or ActiveManeuver) and insert new maneuver
        commands
            .entity(action.fleet)
            .remove::<FleetOrbit>()
            .remove::<ActiveManeuver>()
            .insert(maneuver);
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

    // Anchor to the parent body's current visual position.
    let parent_pos = if let Some(lp) = maybe_lp {
        body_query.get(lp.0).ok().map(|(t, _, _)| t.translation).unwrap_or(Vec3::ZERO)
    } else {
        Vec3::ZERO // star at render origin
    };

    Some(parent_pos + pos_scaled)
}

/// Draw a KSP-style "ghost" body gizmo at `center` showing predicted arrival position.
///
/// * Dashed amber ring at `ring_r` — the arrival orbit ring.
/// * Smaller dashed amber circle at approximately the body's visual size.
/// * Crosshair in the centre.
fn draw_ghost_body(gizmos: &mut Gizmos, center: Vec3, ring_r: f32, body_r: f32) {
    const N: u32 = 32;
    let tau = std::f32::consts::TAU;

    // Arrival orbit ring — dashed amber, 50 % alpha.
    for i in 0..N {
        if i % 2 == 1 { continue; }
        let a1 = (i as f32 / N as f32) * tau;
        let a2 = ((i + 1) as f32 / N as f32) * tau;
        let p1 = center + Vec3::new(a1.cos() * ring_r, a1.sin() * ring_r, 0.0);
        let p2 = center + Vec3::new(a2.cos() * ring_r, a2.sin() * ring_r, 0.0);
        gizmos.line(p1, p2, Color::srgba(1.0, 0.75, 0.15, 0.50));
    }

    // Ghost body outline — slightly smaller, 28 % alpha.
    let ghost_r = (body_r * 0.85).max(ring_r * 0.35);
    for i in 0..N {
        if i % 2 == 1 { continue; }
        let a1 = (i as f32 / N as f32) * tau;
        let a2 = ((i + 1) as f32 / N as f32) * tau;
        let p1 = center + Vec3::new(a1.cos() * ghost_r, a1.sin() * ghost_r, 0.0);
        let p2 = center + Vec3::new(a2.cos() * ghost_r, a2.sin() * ghost_r, 0.0);
        gizmos.line(p1, p2, Color::srgba(1.0, 0.75, 0.15, 0.28));
    }

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

    for (entity, maneuver) in fleet_query.iter() {
        // In System view only draw for the selected fleet, in Starmap always draw.
        if *view_mode == ViewMode::System {
            if let Some(sel) = fleet_ui_state.selected_fleet {
                if entity != sel {
                    continue;
                }
            }
        }

        let center_is_star = body_query.get(maneuver.orbit_center)
            .map(|(_, b, _)| b.body_type == BodyType::Star)
            .unwrap_or(true);

        if !center_is_star && *view_mode == ViewMode::System {
            // ── Local transfer: visual-space arc ──
            let origin_ring_r = body_query.get(maneuver.origin_body)
                .map(|(_, b, _)| b.visual_radius * FLEET_ORBIT_RADIUS_MULT)
                .unwrap_or(0.0);
            let (dest_visual_r, dest_ring_r) = body_query.get(maneuver.destination_body)
                .map(|(_, b, _)| (b.visual_radius, b.visual_radius * FLEET_ORBIT_RADIUS_MULT))
                .unwrap_or((0.0, 0.0));

            let origin_visual = body_query.get(maneuver.origin_body)
                .map(|(t, _, _)| t.translation).ok();
            let center_visual = body_query.get(maneuver.orbit_center)
                .map(|(t, _, _)| t.translation).ok();

            if let (Some(op), Some(cv)) = (origin_visual, center_visual) {
                let dp_current = body_query.get(maneuver.destination_body)
                    .ok().map(|(t, _, _)| t.translation);
                let Some(dp_now) = dp_current else { continue; };

                // For the in-transit arc, target where the body WILL BE at arrival,
                // not where it is now.  The preview used predicted pos; once departed
                // we must continue pointing at the same predicted endpoint.
                let dp = predict_body_visual_pos(
                    maneuver.destination_body,
                    maneuver.arrival_time,
                    &body_query,
                    &kepler_query,
                    &amp_query,
                ).unwrap_or(dp_now);

                // Optimal departure angle: direction from origin body toward the predicted
                // arrival position — same logic as the preview arc (stable, prograde-optimal).
                let dep_angle = optimal_departure_angle(op, dp);
                let dir_dep = Vec3::new(dep_angle.cos(), dep_angle.sin(), 0.0);
                let p0 = op + dir_dep * origin_ring_r;
                // Always CCW (prograde) departure tangent.
                let tang_origin = Vec3::new(-dep_angle.sin(), dep_angle.cos(), 0.0);

                // Arrival: radial direction of destination relative to its orbital centre.
                let radial_dest_raw = dp - cv;
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
                let tang_dest = Vec3::new(-radial_dest.y, radial_dest.x, 0.0);

                let ctrl_len = (p3 - p0).length() * 0.40;
                let p1 = p0 + tang_origin * ctrl_len;
                let p2 = p3 - tang_dest   * ctrl_len;

                let bezier = |t: f32| -> Vec3 {
                    let u = 1.0 - t;
                    u*u*u*p0 + 3.0*u*u*t*p1 + 3.0*u*t*t*p2 + t*t*t*p3
                };

                let mut prev: Option<Vec3> = None;
                for i in 0..=SEGMENTS {
                    let t_frac = i as f32 / SEGMENTS as f32;
                    let pos = bezier(t_frac);
                    if let Some(prev_pos) = prev {
                        let alpha = 0.85 - 0.35 * t_frac;
                        gizmos.line(prev_pos, pos, Color::srgba(0.3, 0.8, 1.0, alpha));
                    }
                    prev = Some(pos);
                }

                // Ghost body at predicted arrival position (same as arc target).
                draw_ghost_body(&mut gizmos, dp, dest_ring_r, dest_visual_r);
            }
            continue;
        }

        // ── Heliocentric / Starmap: physics-accurate Keplerian arc ──
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
                (world_au.x * scale) as f32,
                (world_au.y * scale) as f32,
                (world_au.z * scale) as f32,
            );

            if let Some(prev_pos) = prev {
                let alpha = 0.8 * (1.0 - 0.4 * frac as f32);
                gizmos.line(prev_pos, render_pos, Color::srgba(0.3, 0.8, 1.0, alpha));
            }
            prev = Some(render_pos);
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
    body_query: Query<(&Transform, &CelestialBody), Without<Fleet>>,
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
            if let Ok((body_transform, body)) = body_query.get(orbit.body) {
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
                    body.visual_radius * FLEET_ORBIT_RADIUS_MULT
                };
                transform.translation = body_transform.translation + dir * visual_orbit;
            }
        } else if let Some(maneuver) = maybe_maneuver {
            // ── In-transit: check whether this is a local or heliocentric transfer ──
            let center_is_star = body_query.get(maneuver.orbit_center)
                .map(|(_, b)| b.body_type == BodyType::Star)
                .unwrap_or(true);

            if !center_is_star {
                // Local transfer: follow the same cubic Bezier as the trajectory gizmo.
                let origin_data = body_query.get(maneuver.origin_body).ok().map(|(t, b)| (t.translation, b.visual_radius * FLEET_ORBIT_RADIUS_MULT));
                let dest_data   = body_query.get(maneuver.destination_body).ok().map(|(t, b)| (t.translation, b.visual_radius * FLEET_ORBIT_RADIUS_MULT));
                if let (Some((op, origin_ring_r)), Some((dp, dest_ring_r))) = (origin_data, dest_data) {
                    let progress = maneuver.progress(elapsed) as f32;

                    // Reproduce the EXACT same Bezier as draw_fleet_trajectories.
                    // Use optimal departure angle (direction toward destination) for consistency
                    // with the preview arc — avoids the arc shape changing based on the orbital
                    // phase when Execute was clicked.
                    let dep_angle = optimal_departure_angle(op, dp);
                    let dir_dep = Vec3::new(dep_angle.cos(), dep_angle.sin(), 0.0);
                    let p0 = op + dir_dep * origin_ring_r;
                    // Always prograde (CCW), no flip.
                    let tang_origin = Vec3::new(-dep_angle.sin(), dep_angle.cos(), 0.0);

                    let cv = body_query.get(maneuver.orbit_center)
                        .map(|(t, _)| t.translation).unwrap_or(dp);
                    let radial_raw = dp - cv;
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
                    let u = 1.0 - t;
                    transform.translation = u*u*u*p0 + 3.0*u*u*t*p1 + 3.0*u*t*t*p2 + t*t*t*p3;

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

    const TOTAL_SEGMENTS: u32 = 64;

    let draw_ring = |gizmos: &mut Gizmos, center: Vec3, radius: f32, color: Color| {
        for i in 0..TOTAL_SEGMENTS {
            if i % 2 == 1 { continue; } // gap — dashed
            let a1 = (i as f32 / TOTAL_SEGMENTS as f32) * std::f32::consts::TAU;
            let a2 = ((i + 1) as f32 / TOTAL_SEGMENTS as f32) * std::f32::consts::TAU;
            let p1 = center + Vec3::new(a1.cos() * radius, a1.sin() * radius, 0.0);
            let p2 = center + Vec3::new(a2.cos() * radius, a2.sin() * radius, 0.0);
            gizmos.line(p1, p2, color);
        }
    };

    if let Ok((_, orbit)) = parked_query.get(selected) {
        // ── Parked: single green ring around orbit body ──
        if let Ok((body_transform, body)) = body_query.get(orbit.body) {
            // For star-orbiting fleets (Lagrange points), the orbit radius is
            // heliocentric AU — convert to visual units.  This draws a large
            // ring matching the fleet's actual heliocentric orbital path.
            let ring_radius = if body.body_type == BodyType::Star {
                orbit.radius_au as f32 * SCALING_FACTOR as f32
            } else {
                body.visual_radius * FLEET_ORBIT_RADIUS_MULT
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
            // Heliocentric (LP) transfer: draw a dim departure ring so the user can
            // see where the fleet left from.  The trajectory arc (draw_fleet_trajectories)
            // shows the full heliocentric path when the fleet is selected.
            if let Ok((body_transform, body)) = body_query.get(maneuver.origin_body) {
                draw_ring(
                    &mut gizmos,
                    body_transform.translation,
                    body.visual_radius * FLEET_ORBIT_RADIUS_MULT,
                    Color::srgba(0.2, 0.9, 0.3, 0.15),
                );
            }
            return;
        }

        // Departure ring — dim green
        if let Ok((body_transform, body)) = body_query.get(maneuver.origin_body) {
            draw_ring(
                &mut gizmos,
                body_transform.translation,
                body.visual_radius * FLEET_ORBIT_RADIUS_MULT,
                Color::srgba(0.2, 0.9, 0.3, 0.20),
            );
        }
        // Arrival ring — brighter cyan
        if let Ok((body_transform, body)) = body_query.get(maneuver.destination_body) {
            draw_ring(
                &mut gizmos,
                body_transform.translation,
                body.visual_radius * FLEET_ORBIT_RADIUS_MULT,
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
    let Some(fleet_entity) = fleet_ui_state.selected_fleet else { return; };

    // ── Lagrange-point target preview ─────────────────────────────────────────
    // LP transfers have no body entity; draw an arc to the predicted LP position.
    if let Some(lp) = &fleet_ui_state.target_lagrange {
        let Ok((_, maybe_orbit, maybe_maneuver)) = fleet_query.get(fleet_entity) else { return; };
        let origin_body = if let Some(orbit) = maybe_orbit {
            orbit.body
        } else if let Some(maneuver) = maybe_maneuver {
            maneuver.destination_body
        } else {
            return;
        };

        let Ok((origin_transform, origin_body_data, _)) = body_query.get(origin_body) else { return; };
        let op = origin_transform.translation;
        let origin_ring_r = origin_body_data.visual_radius * FLEET_ORBIT_RADIUS_MULT;

        let current_sim_s = sim_time.elapsed_seconds();
        let travel_time_s = if fleet_ui_state.selected_option < fleet_ui_state.computed_options.len() {
            fleet_ui_state.computed_options[fleet_ui_state.selected_option].transfer_time_s
        } else if let Some(pt) = &fleet_ui_state.planned_transfer {
            pt.duration_s
        } else {
            0.0
        };

        // Predict the LP's parent planet position at arrival to get LP direction.
        let planet_pos_now = body_query.get(lp.planet_entity)
            .ok().map(|(t, _, _)| t.translation).unwrap_or(Vec3::ZERO);
        let planet_pos_arrival = predict_body_visual_pos(
            lp.planet_entity,
            current_sim_s + travel_time_s,
            &body_query,
            &kepler_query,
            &amp_query,
        ).unwrap_or(planet_pos_now);

        // L3 is opposite the planet; L4/L5 are ±60°; L1/L2 are along the planet axis.
        let planet_angle = planet_pos_arrival.y.atan2(planet_pos_arrival.x) as f64;
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

        // Dashed amber arc.
        const LP_SEGS: u32 = 48;
        for i in 0..LP_SEGS {
            if i % 2 == 1 { continue; }
            let t0 = i as f32 / LP_SEGS as f32;
            let t1 = (i + 1) as f32 / LP_SEGS as f32;
            let alpha = 0.70 - 0.35 * t0;
            gizmos.line(lp_bezier(t0), lp_bezier(t1), Color::srgba(1.0, 0.75, 0.15, alpha));
        }

        // LP marker: crosshair + dashed circle in cyan-blue.
        let lp_color = Color::srgba(0.5, 0.85, 1.0, 0.85);
        let cs = lp_marker_r * 1.4;
        gizmos.line(dp - Vec3::X * cs, dp + Vec3::X * cs, lp_color);
        gizmos.line(dp - Vec3::Y * cs, dp + Vec3::Y * cs, lp_color);
        const LP_N: u32 = 24;
        for i in 0..LP_N {
            if i % 2 == 1 { continue; }
            let a1 = (i as f32 / LP_N as f32) * std::f32::consts::TAU;
            let a2 = ((i + 1) as f32 / LP_N as f32) * std::f32::consts::TAU;
            let q1 = dp + Vec3::new(a1.cos() * lp_marker_r, a1.sin() * lp_marker_r, 0.0);
            let q2 = dp + Vec3::new(a2.cos() * lp_marker_r, a2.sin() * lp_marker_r, 0.0);
            gizmos.line(q1, q2, lp_color);
        }

        return;
    }

    let Some(target_entity) = fleet_ui_state.target_body   else { return; };

    let Ok((_, maybe_orbit, maybe_maneuver)) = fleet_query.get(fleet_entity) else { return; };

    let origin_body = if let Some(orbit) = maybe_orbit {
        orbit.body
    } else if let Some(maneuver) = maybe_maneuver {
        maneuver.destination_body
    } else {
        return;
    };

    if origin_body == target_entity { return; }

    let Ok((origin_transform, origin_body_data, _))      = body_query.get(origin_body)   else { return; };
    let Ok((dest_transform_now, dest_body_data, dest_lp)) = body_query.get(target_entity) else { return; };

    let op            = origin_transform.translation;
    let origin_ring_r = origin_body_data.visual_radius * FLEET_ORBIT_RADIUS_MULT;
    let dest_visual_r = dest_body_data.visual_radius;
    let dest_ring_r   = dest_visual_r * FLEET_ORBIT_RADIUS_MULT;

    // Travel time from selected transfer option (or 0 = show arc to current position).
    let current_sim_s = sim_time.elapsed_seconds();
    let travel_time_s = if fleet_ui_state.selected_option < fleet_ui_state.computed_options.len() {
        fleet_ui_state.computed_options[fleet_ui_state.selected_option].transfer_time_s
    } else if let Some(pt) = &fleet_ui_state.planned_transfer {
        pt.duration_s
    } else {
        0.0
    };

    // Predict destination body position at estimated arrival time.
    let dp = predict_body_visual_pos(
        target_entity,
        current_sim_s + travel_time_s,
        &body_query,
        &kepler_query,
        &amp_query,
    ).unwrap_or(dest_transform_now.translation);

    // Stable optimal departure angle: direction from the origin body toward the predicted
    // destination position.  The fleet waits in its parking orbit until it reaches this
    // angular position, then fires — this is always the ΔV-optimal departure point.
    // Using this (instead of the fleet's rotating orbit.angle_rad) keeps the preview arc
    // anchored; it only drifts slowly as the target body advances in its own orbit.
    let departure_angle = optimal_departure_angle(op, dp);

    // Departure: prograde tangent at the optimal departure angle (always CCW, no flip).
    let dir_dep     = Vec3::new(departure_angle.cos(), departure_angle.sin(), 0.0);
    let p0          = op + dir_dep * origin_ring_r;
    let tang_origin = Vec3::new(-departure_angle.sin(), departure_angle.cos(), 0.0);

    // Arrival: point on the ring approached from the origin-facing (inward) side.
    let dest_center_pos = if dest_lp.map(|lp| lp.0) == Some(origin_body) {
        op
    } else if let Some(lp) = dest_lp {
        body_query.get(lp.0).ok().map(|(t, _, _)| t.translation).unwrap_or(op)
    } else {
        op
    };
    let radial_dest_raw = dp - dest_center_pos;
    let radial_dest = if radial_dest_raw.length() > 1.0 {
        radial_dest_raw.normalize()
    } else {
        (dp - op).normalize_or_zero()
    };
    let inward = (op - dp).normalize_or_zero();
    let p3 = dp + inward * dest_ring_r;
    let tang_d_a  = Vec3::new(-radial_dest.y, radial_dest.x, 0.0);
    let tang_dest = if tang_d_a.dot(tang_origin) >= 0.0 { tang_d_a } else { -tang_d_a };

    // Cubic Bezier.
    let ctrl_len = (p3 - p0).length() * 0.40;
    let p1 = p0 + tang_origin * ctrl_len;
    let p2 = p3 - tang_dest   * ctrl_len;
    let bezier = |t: f32| -> Vec3 {
        let u = 1.0 - t;
        u*u*u*p0 + 3.0*u*u*t*p1 + 3.0*u*t*t*p2 + t*t*t*p3
    };

    // Dashed amber arc.
    const SEGMENTS: u32 = 48;
    for i in 0..SEGMENTS {
        if i % 2 == 1 { continue; }
        let t0 = i as f32 / SEGMENTS as f32;
        let t1 = (i + 1) as f32 / SEGMENTS as f32;
        let alpha = 0.70 - 0.35 * t0;
        gizmos.line(bezier(t0), bezier(t1), Color::srgba(1.0, 0.75, 0.15, alpha));
    }

    // Ghost body at predicted arrival position.
    draw_ghost_body(&mut gizmos, dp, dest_ring_r, dest_visual_r);
}

/// Draw the two-leg slingshot arc when a gravity-assist flyby is selected.
///
/// - **Leg 1** (origin → flyby body): lime-green dashed arc.
/// - **Leg 2** (flyby body → destination): magenta dashed arc.
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
    let origin_body = if let Some(orbit) = maybe_orbit {
        orbit.body
    } else if let Some(maneuver) = maybe_maneuver {
        maneuver.destination_body
    } else {
        return;
    };
    // Sanity: all three bodies must be distinct
    if origin_body == target_entity || origin_body == flyby_entity || flyby_entity == target_entity {
        return;
    }

    let Ok((origin_t, origin_bd, _))  = body_query.get(origin_body)  else { return; };
    let Ok((_, flyby_bd, _))           = body_query.get(flyby_entity) else { return; };
    let Ok((dest_t,   dest_bd,   _))   = body_query.get(target_entity) else { return; };

    let op = origin_t.translation;
    let origin_ring_r = origin_bd.visual_radius * FLEET_ORBIT_RADIUS_MULT;
    let flyby_ring_r  = flyby_bd.visual_radius  * FLEET_ORBIT_RADIUS_MULT;
    let dest_ring_r   = dest_bd.visual_radius   * FLEET_ORBIT_RADIUS_MULT;

    let current_sim_s = sim_time.elapsed_seconds();
    let depart_s = current_sim_s + departure_offset_s;

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

    const SEGS: u32 = 48;

    // ── Leg 1: origin → flyby (lime-green dashes) ────────────────────────────
    let dep1 = optimal_departure_angle(op, fp);
    let dir1 = Vec3::new(dep1.cos(), dep1.sin(), 0.0);
    let p0 = op + dir1 * origin_ring_r;
    let tang0 = Vec3::new(-dep1.sin(), dep1.cos(), 0.0);
    let inward1 = (op - fp).normalize_or_zero();
    let p3_1 = fp + inward1 * flyby_ring_r;
    let rad1 = (fp - op).normalize_or_zero();
    let td1 = {
        let a = Vec3::new(-rad1.y, rad1.x, 0.0);
        if a.dot(tang0) >= 0.0 { a } else { -a }
    };
    let cl1 = (p3_1 - p0).length() * 0.40;
    let p1_1 = p0     + tang0 * cl1;
    let p2_1 = p3_1   - td1   * cl1;
    let bez1 = |t: f32| -> Vec3 {
        let u = 1.0 - t;
        u*u*u*p0 + 3.0*u*u*t*p1_1 + 3.0*u*t*t*p2_1 + t*t*t*p3_1
    };
    for i in 0..SEGS {
        if i % 2 == 1 { continue; }
        let t0 = i as f32 / SEGS as f32;
        let t1 = (i + 1) as f32 / SEGS as f32;
        let alpha = 0.80 - 0.35 * t0;
        gizmos.line(bez1(t0), bez1(t1), Color::srgba(0.3, 1.0, 0.4, alpha));
    }

    // ── Leg 2: flyby → destination (magenta dashes) ──────────────────────────
    let dep2 = optimal_departure_angle(fp, dp);
    let dir2 = Vec3::new(dep2.cos(), dep2.sin(), 0.0);
    let p0_2 = fp + dir2 * flyby_ring_r;
    let tang0_2 = Vec3::new(-dep2.sin(), dep2.cos(), 0.0);
    let inward2 = (fp - dp).normalize_or_zero();
    let p3_2 = dp + inward2 * dest_ring_r;
    let rad2 = (dp - fp).normalize_or_zero();
    let td2 = {
        let a = Vec3::new(-rad2.y, rad2.x, 0.0);
        if a.dot(tang0_2) >= 0.0 { a } else { -a }
    };
    let cl2 = (p3_2 - p0_2).length() * 0.40;
    let p1_2 = p0_2 + tang0_2 * cl2;
    let p2_2 = p3_2 - td2     * cl2;
    let bez2 = |t: f32| -> Vec3 {
        let u = 1.0 - t;
        u*u*u*p0_2 + 3.0*u*u*t*p1_2 + 3.0*u*t*t*p2_2 + t*t*t*p3_2
    };
    for i in 0..SEGS {
        if i % 2 == 1 { continue; }
        let t0 = i as f32 / SEGS as f32;
        let t1 = (i + 1) as f32 / SEGS as f32;
        let alpha = 0.80 - 0.35 * t0;
        gizmos.line(bez2(t0), bez2(t1), Color::srgba(1.0, 0.3, 0.8, alpha));
    }

    // ── Flyby node: yellow cross + ghost ring ─────────────────────────────────
    let cross = flyby_ring_r * 2.5;
    let node_color = Color::srgba(1.0, 1.0, 0.3, 0.9);
    gizmos.line(fp - Vec3::X * cross, fp + Vec3::X * cross, node_color);
    gizmos.line(fp - Vec3::Y * cross, fp + Vec3::Y * cross, node_color);
    draw_ghost_body(&mut gizmos, fp, flyby_ring_r * 1.4, flyby_bd.visual_radius * 0.7);

    // ── Destination ghost ─────────────────────────────────────────────────────
    draw_ghost_body(&mut gizmos, dp, dest_ring_r, dest_bd.visual_radius);
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
