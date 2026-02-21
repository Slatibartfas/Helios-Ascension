//! ECS systems for fleet position updates, trajectory rendering, and startup.

use bevy::math::DVec3;
use bevy::prelude::*;

use super::components::{ActiveManeuver, Fleet, FleetOrbit, PendingFleetActions, ShipInfo};
use super::orbital_mechanics::AU_IN_METERS;
use super::types::{PropulsionType, ShipClass};
use crate::astronomy::components::FloatingOrigin;
use crate::astronomy::{orbit_position_from_mean_anomaly, SpaceCoordinates, SCALING_FACTOR};
use crate::plugins::camera::{GameCamera, OrbitCamera, ViewMode};
use crate::ui::{FleetUiState, SimulationTime};

/// Marker component for entities that have a fleet mesh sphere.
#[derive(Component)]
pub struct FleetMesh;

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
pub fn draw_fleet_trajectories(
    mut gizmos: Gizmos,
    fleet_query: Query<(Entity, &ActiveManeuver), With<Fleet>>,
    center_coords: Query<&SpaceCoordinates, Without<Fleet>>,
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
    fleet_query: Query<&SpaceCoordinates, With<Fleet>>,
    floating_origin: Option<Res<FloatingOrigin>>,
    view_mode: Res<ViewMode>,
    camera_query: Query<&OrbitCamera, With<GameCamera>>,
) {
    let Some(selected) = fleet_ui_state.selected_fleet else {
        return;
    };
    let Ok(sc) = fleet_query.get(selected) else {
        return;
    };

    let origin_offset = floating_origin
        .as_ref()
        .map(|fo| fo.position)
        .unwrap_or(DVec3::ZERO);

    let (center, arm) = match *view_mode {
        ViewMode::System => {
            let du = (sc.position - origin_offset) * SCALING_FACTOR;
            let pos = Vec3::new(du.x as f32, du.y as f32, du.z as f32);
            (pos, 22.0_f32)
        }
        ViewMode::Starmap => {
            let camera_radius = camera_query
                .single()
                .ok()
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

/// Keep each fleet entity's `Transform` in sync with its `SpaceCoordinates`.
/// In System view positions use SCALING_FACTOR; the mesh is hidden in Starmap view
/// (starmap rendering is handled by `draw_fleet_trajectories` gizmos at AU scale).
pub fn update_fleet_transforms(
    mut fleet_query: Query<(&SpaceCoordinates, &mut Transform, &mut Visibility), With<Fleet>>,
    floating_origin: Option<Res<FloatingOrigin>>,
    view_mode: Res<ViewMode>,
) {
    let origin_offset = floating_origin
        .as_ref()
        .map(|fo| fo.position)
        .unwrap_or(DVec3::ZERO);

    for (sc, mut transform, mut vis) in fleet_query.iter_mut() {
        match *view_mode {
            ViewMode::System => {
                *vis = Visibility::Inherited;
                let render_du = (sc.position - origin_offset) * SCALING_FACTOR;
                transform.translation = Vec3::new(
                    render_du.x as f32,
                    render_du.y as f32,
                    render_du.z as f32,
                );
            }
            ViewMode::Starmap => {
                // Hide mesh sphere in starmap; trajectory is drawn by gizmos at AU scale.
                *vis = Visibility::Hidden;
            }
        }
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
