//! ECS systems for fleet position updates, trajectory rendering, and startup.

use bevy::math::DVec3;
use bevy::prelude::*;

use super::components::{ActiveManeuver, Fleet, FleetOrbit, PendingFleetActions, ShipInfo};
use super::orbital_mechanics::AU_IN_METERS;
use super::types::{PropulsionType, ShipClass};
use bevy::time::Real;
use crate::astronomy::components::FloatingOrigin;
use crate::astronomy::{orbit_position_from_mean_anomaly, SpaceCoordinates, SCALING_FACTOR};
use crate::plugins::camera::{GameCamera, OrbitCamera, ViewMode};
use crate::plugins::solar_system::CelestialBody;
use crate::plugins::solar_system_data::BodyType;
use crate::ui::{FleetUiState, SimulationTime, TimeScale};

/// Marker component for entities that have a fleet mesh sphere.
#[derive(Component)]
pub struct FleetMesh;

// ── Position update systems ───────────────────────────────────────────────────

/// One full visual revolution every 120 real seconds — readable at any time scale.
const VISUAL_ORBIT_RATE: f64 = std::f64::consts::TAU / 120.0;

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
        orbit.angle_rad = (orbit.angle_rad + VISUAL_ORBIT_RATE * real_delta)
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
}

// ── Rendering systems ─────────────────────────────────────────────────────────

/// Draw the trajectory arc for the selected fleet.
/// In System view uses SCALING_FACTOR; in Starmap view uses raw AU (1 unit = 1 AU).
///
/// For local (non-heliocentric) transfers the trajectory is drawn in **visual space**
/// by interpolating between the origin and destination body render positions.
/// Heliocentric transfers continue to use the physics-accurate Keplerian arc.
pub fn draw_fleet_trajectories(
    mut gizmos: Gizmos,
    sim_time: Res<SimulationTime>,
    fleet_query: Query<(Entity, &ActiveManeuver), With<Fleet>>,
    center_coords: Query<&SpaceCoordinates, Without<Fleet>>,
    body_query: Query<(&Transform, &CelestialBody), Without<Fleet>>,
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
    let elapsed = sim_time.elapsed_seconds();

    for (entity, maneuver) in fleet_query.iter() {
        // In System view only draw for the selected fleet, in Starmap always draw.
        if *view_mode == ViewMode::System {
            if let Some(sel) = fleet_ui_state.selected_fleet {
                if entity != sel {
                    continue;
                }
            }
        }

        // Determine if this is a local (planet-centric) transfer by checking whether
        // the orbit center is a star. For local transfers we draw in visual space.
        let center_is_star = body_query.get(maneuver.orbit_center)
            .map(|(_, b)| b.body_type == BodyType::Star)
            .unwrap_or(true); // default to heliocentric treatment if unknown

        if !center_is_star && *view_mode == ViewMode::System {
            // ── Local transfer: visual-space arc clipped to orbit ring boundaries ──
            let origin_ring_r = body_query.get(maneuver.origin_body)
                .map(|(_, b)| b.visual_radius * 2.0)
                .unwrap_or(0.0);
            let dest_ring_r = body_query.get(maneuver.destination_body)
                .map(|(_, b)| b.visual_radius * 2.0)
                .unwrap_or(0.0);
            let origin_visual = body_query.get(maneuver.origin_body).map(|(t, _)| t.translation).ok();
            let dest_visual = body_query.get(maneuver.destination_body).map(|(t, _)| t.translation).ok();

            if let (Some(op), Some(dp)) = (origin_visual, dest_visual) {
                let forward = dp - op;
                let arc_height = forward.length() * 0.3;
                let perp = Vec3::new(-forward.y, forward.x, 0.0).normalize_or_zero();

                // Helper: evaluate arc position at parameter t ∈ [0,1]
                let arc_pos = |t: f32| -> Vec3 {
                    let base = op.lerp(dp, t);
                    let bulge = perp * arc_height * (t * std::f32::consts::PI).sin();
                    base + bulge
                };

                // Only draw segments that lie outside both orbit rings.
                // This naturally clips the arc at the ring boundaries.
                let mut prev: Option<Vec3> = None;
                for i in 0..=SEGMENTS {
                    let t_frac = i as f32 / SEGMENTS as f32;
                    let pos = arc_pos(t_frac);
                    let inside_origin = pos.distance(op) < origin_ring_r;
                    let inside_dest   = pos.distance(dp) < dest_ring_r;

                    if inside_origin || inside_dest {
                        prev = None; // lift the pen — restart outside the ring
                        continue;
                    }

                    if let Some(prev_pos) = prev {
                        let alpha = 0.85 - 0.35 * t_frac;
                        gizmos.line(prev_pos, pos, Color::srgba(0.3, 0.8, 1.0, alpha));
                    }
                    prev = Some(pos);
                }
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
            // ── Orbiting fleet: place just outside the parent body's visual sphere ──
            if let Ok((body_transform, body)) = body_query.get(orbit.body) {
                let dir = Vec3::new(
                    orbit.angle_rad.cos() as f32,
                    orbit.angle_rad.sin() as f32,
                    0.0,
                );
                // Position at 2× visual radius so the marker sits clearly outside
                let visual_orbit = body.visual_radius * 2.0;
                transform.translation = body_transform.translation + dir * visual_orbit;
            }
        } else if let Some(maneuver) = maybe_maneuver {
            // ── In-transit: check whether this is a local or heliocentric transfer ──
            let center_is_star = body_query.get(maneuver.orbit_center)
                .map(|(_, b)| b.body_type == BodyType::Star)
                .unwrap_or(true);

            if !center_is_star {
                // Local transfer: interpolate visually between origin and destination
                let origin_visual = body_query.get(maneuver.origin_body).map(|(t, _)| t.translation).ok();
                let dest_visual = body_query.get(maneuver.destination_body).map(|(t, _)| t.translation).ok();
                if let (Some(op), Some(dp)) = (origin_visual, dest_visual) {
                    let progress = maneuver.progress(elapsed) as f32;
                    let forward = dp - op;
                    let arc_height = forward.length() * 0.3;
                    let perp = Vec3::new(-forward.y, forward.x, 0.0).normalize_or_zero();
                    let base = op.lerp(dp, progress);
                    let bulge = perp * arc_height * (progress * std::f32::consts::PI).sin();
                    transform.translation = base + bulge;
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

/// Draw a thin dashed orbit ring around the body that the selected fleet is
/// parked around.  Only drawn in System view when a fleet is selected and in
/// orbit (not in transit).
pub fn draw_fleet_orbit_rings(
    mut gizmos: Gizmos,
    fleet_ui_state: Res<FleetUiState>,
    fleet_query: Query<(Entity, &FleetOrbit), With<Fleet>>,
    body_query: Query<(&Transform, &CelestialBody), Without<Fleet>>,
    view_mode: Res<ViewMode>,
) {
    if *view_mode != ViewMode::System {
        return;
    }
    let Some(selected) = fleet_ui_state.selected_fleet else {
        return;
    };
    let Ok((_, orbit)) = fleet_query.get(selected) else {
        return; // selected fleet is in transit, no orbit ring
    };
    let Ok((body_transform, body)) = body_query.get(orbit.body) else {
        return;
    };

    let center = body_transform.translation;
    let radius = body.visual_radius * 2.0;
    // Dashed effect: draw every other segment out of TOTAL_SEGMENTS
    const TOTAL_SEGMENTS: u32 = 64;
    let color = Color::srgba(0.2, 0.9, 0.3, 0.30);

    for i in 0..TOTAL_SEGMENTS {
        if i % 2 == 1 {
            continue; // gap
        }
        let a1 = (i as f32 / TOTAL_SEGMENTS as f32) * std::f32::consts::TAU;
        let a2 = ((i + 1) as f32 / TOTAL_SEGMENTS as f32) * std::f32::consts::TAU;
        let p1 = center + Vec3::new(a1.cos() * radius, a1.sin() * radius, 0.0);
        let p2 = center + Vec3::new(a2.cos() * radius, a2.sin() * radius, 0.0);
        gizmos.line(p1, p2, color);
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
