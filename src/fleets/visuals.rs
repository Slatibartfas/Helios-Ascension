//! Fleet visualisation: drawing helpers, gizmo systems, and mesh management.

use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::time::Real;

use super::components::{ActiveManeuver, Fleet, FleetOrbit};
use crate::astronomy::components::FloatingOrigin;
use crate::astronomy::{
    orbit_position_from_mean_anomaly, KeplerOrbit, LocalOrbitAmplification, SpaceCoordinates,
    SCALING_FACTOR,
};
use crate::plugins::camera::{GameCamera, OrbitCamera, ViewMode};
use crate::plugins::solar_system::{CelestialBody, LogicalParent};
use crate::plugins::solar_system_data::BodyType;
use crate::ui::{FleetUiState, SimulationTime};

/// Marker component for entities that have a fleet mesh sphere.
#[derive(Component)]
pub struct FleetMesh;

// ── Visual radius helpers ─────────────────────────────────────────────────────

/// Multiplier applied to a body's visual radius to determine the orbit ring
/// radius and fleet icon parking distance.  1.5× keeps the marker just outside
/// the body's glow without the ring dominating the view.
pub(super) const FLEET_ORBIT_RADIUS_MULT: f32 = 1.5;

/// Radius of the sphere mesh spawned for each fleet icon (must match `ensure_fleet_meshes`).
pub(super) const FLEET_SPHERE_RADIUS: f32 = 6.0;

/// Minimum clearance gap between the fleet sphere's surface and the body's visual surface.
pub(super) const FLEET_ORBIT_MIN_GAP: f32 = 3.0;

/// Compute the visual-space parking orbit radius for a fleet around a body.
///
/// Uses `body_visual_radius * FLEET_ORBIT_RADIUS_MULT` as default, but enforces
/// a minimum of `body_visual_radius + FLEET_SPHERE_RADIUS + FLEET_ORBIT_MIN_GAP`
/// so the fleet marker never intersects the body for very small or inflated bodies.
#[inline]
pub(super) fn fleet_parking_visual_radius(body_visual_radius: f32) -> f32 {
    let proportional = body_visual_radius * FLEET_ORBIT_RADIUS_MULT;
    let minimum = body_visual_radius + FLEET_SPHERE_RADIUS + FLEET_ORBIT_MIN_GAP;
    proportional.max(minimum)
}

// ── Dashed-curve helpers ──────────────────────────────────────────────────────

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

// ── Body-prediction helpers ───────────────────────────────────────────────────

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
pub(super) fn predict_body_physics_pos(
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

// ── Ghost-body / departure helpers ───────────────────────────────────────────

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

// ── Shared transfer-arc geometry ──────────────────────────────────────────────

/// Computed Bezier control points for a transfer arc.
///
/// All three rendering paths (preview, transit gizmo, fleet dot) share this
/// geometry via [`compute_transfer_arc`] to guarantee identical curves.
struct TransferArcGeometry {
    /// Bezier start point (on origin ring, or fleet pos for course corrections).
    p0: Vec3,
    /// Bezier control point near departure.
    p1: Vec3,
    /// Bezier control point near arrival.
    p2: Vec3,
    /// Bezier end point (on destination ring).
    p3: Vec3,
    /// Whether this is a kinematic (straight-line) transfer.
    is_kinematic: bool,
    /// Departure angle computed by `optimal_departure_angle`.
    departure_angle: f32,
}

impl TransferArcGeometry {
    /// Evaluate the arc at parameter `t` ∈ \[0, 1\].
    fn eval(&self, t: f32) -> Vec3 {
        if self.is_kinematic {
            self.p0 + (self.p3 - self.p0) * t
        } else {
            let u = 1.0 - t;
            u*u*u*self.p0 + 3.0*u*u*t*self.p1 + 3.0*u*t*t*self.p2 + t*t*t*self.p3
        }
    }
}

/// Compute the visual Bezier geometry for a transfer arc.
///
/// # Parameters
/// - `op` / `dp`: departure / arrival body positions (frame-corrected by the caller).
/// - `origin_ring_r`: fleet parking radius at origin (0 for course corrections).
/// - `dest_ring_r`: fleet parking radius at destination.
/// - `is_course_correction`: use tighter tangent blend (10/90 instead of 20/80).
/// - `is_inward`: orbit-lowering transfer (CW departure tangent).
/// - `is_kinematic`: straight line instead of cubic Bezier.
/// - `cv_ref`: orbit-centre position for computing arrival tangent direction.
///   Use `cv_current` for local transfers, `cv_at_departure` for heliocentric / preview.
fn compute_transfer_arc(
    op: Vec3,
    dp: Vec3,
    origin_ring_r: f32,
    dest_ring_r: f32,
    is_course_correction: bool,
    is_inward: bool,
    is_kinematic: bool,
    cv_ref: Vec3,
) -> TransferArcGeometry {
    let departure_angle = optimal_departure_angle(op, dp);
    let dir_dep = Vec3::new(departure_angle.cos(), departure_angle.sin(), 0.0);

    // Rotate direction by 90° so the ring point is where prograde (outward) or
    // retrograde (inward) aims toward the destination — Hohmann departure.
    let dep_ring_dir = if is_inward {
        Vec3::new(-dir_dep.y, dir_dep.x, 0.0)  // CCW rotation
    } else {
        Vec3::new(dir_dep.y, -dir_dep.x, 0.0)  // CW rotation
    };
    let p0 = op + dep_ring_dir * origin_ring_r;

    // Prograde/retrograde at the rotated ring point now points along dir_dep.
    let tang_orbit_raw = dir_dep;
    let direct_dir = (dp - p0).normalize_or_zero();
    let tang_origin = if is_course_correction {
        // Course corrections: fleet already moving, aim mostly at destination.
        (tang_orbit_raw * 0.10 + direct_dir * 0.90).normalize_or_zero()
    } else if is_inward {
        // Inward (retrograde departure): blend heavily toward direct path.
        (tang_orbit_raw * 0.20 + direct_dir * 0.80).normalize_or_zero()
    } else {
        // Outward (prograde departure): blend to avoid the 90° hook while
        // keeping the prograde orbital character of a Hohmann departure.
        (tang_orbit_raw * 0.55 + direct_dir * 0.45).normalize_or_zero()
    };

    let radial_dest_raw = dp - cv_ref;
    let radial_dest = if radial_dest_raw.length() > 1.0 {
        radial_dest_raw.normalize()
    } else {
        (dp - op).normalize_or_zero()
    };
    // Arrival ring point: rotate inbound direction by 90° so the fleet arrives
    // where prograde/retrograde aligns with the incoming trajectory.
    let inbound = (dp - op).normalize_or_zero();
    let arr_ring_dir = if is_inward {
        Vec3::new(-inbound.y, inbound.x, 0.0)  // CCW rotation
    } else {
        Vec3::new(inbound.y, -inbound.x, 0.0)  // CW rotation
    };
    let p3 = dp + arr_ring_dir * dest_ring_r;
    let tang_d_a = Vec3::new(-radial_dest.y, radial_dest.x, 0.0);
    let tang_dest = if is_inward {
        if tang_d_a.dot(tang_origin) < 0.0 { tang_d_a } else { -tang_d_a }
    } else {
        if tang_d_a.dot(tang_origin) >= 0.0 { tang_d_a } else { -tang_d_a }
    };

    let ctrl_len = (p3 - p0).length() * 0.40;
    let mut p1 = p0 + tang_origin * ctrl_len;
    let mut p2 = p3 - tang_dest * ctrl_len;
    // Smoothly interpolate z so the arc bridges the two orbital planes
    // even when the tangent vectors are 2D (z = 0).
    p1.z = p0.z + (p3.z - p0.z) * 0.33;
    p2.z = p0.z + (p3.z - p0.z) * 0.67;

    TransferArcGeometry { p0, p1, p2, p3, is_kinematic, departure_angle }
}

// ── Gravity-assist arc geometry ──────────────────────────────────────────────

/// Computed two-leg Bezier geometry for a gravity-assist slingshot trajectory.
///
/// Both the preview and the in-transit renderer share this geometry to guarantee
/// identical curves.
struct GravityAssistArcGeometry {
    p0_1: Vec3, p1_1: Vec3, p2_1: Vec3, p3_1: Vec3,
    p0_2: Vec3, p1_2: Vec3, p2_2: Vec3, p3_2: Vec3,
}

impl GravityAssistArcGeometry {
    fn eval_leg1(&self, t: f32) -> Vec3 {
        let u = 1.0 - t;
        u*u*u*self.p0_1 + 3.0*u*u*t*self.p1_1 + 3.0*u*t*t*self.p2_1 + t*t*t*self.p3_1
    }
    fn eval_leg2(&self, t: f32) -> Vec3 {
        let u = 1.0 - t;
        u*u*u*self.p0_2 + 3.0*u*u*t*self.p1_2 + 3.0*u*t*t*self.p2_2 + t*t*t*self.p3_2
    }
}

/// Build two C1-continuous Bezier legs for a gravity-assist flyby trajectory.
///
/// The departure tangent is blended 80% toward the flyby body and 20%
/// orbital-prograde, eliminating the 90° hook produced by a pure prograde
/// departure tangent.  The two legs meet at a hyperbolic periapsis point offset
/// from the flyby body, producing a realistic gravitational-deflection bend.
fn compute_gravity_assist_arc(
    op: Vec3,
    fp: Vec3,
    dp: Vec3,
    origin_ring_r: f32,
    flyby_ring_r: f32,
    dest_ring_r: f32,
) -> GravityAssistArcGeometry {
    // ── Departure ────────────────────────────────────────────────────────────
    let dir_to_flyby = (fp - op).normalize_or_zero();

    // Is the fleet transferring outward (prograde burn) or inward (retrograde)?
    let is_outward1 = fp.length_squared() > op.length_squared();

    // Departure ring point: rotate dir_to_flyby by 90° so the prograde (or
    // retrograde) direction at that ring point aims toward the flyby body.
    // For an outward/prograde departure: rotate -90° (CW).
    // For an inward/retrograde departure: rotate +90° (CCW).
    let dep_dir = if is_outward1 {
        Vec3::new(dir_to_flyby.y, -dir_to_flyby.x, 0.0)
    } else {
        Vec3::new(-dir_to_flyby.y, dir_to_flyby.x, 0.0)
    };
    let p0 = op + dep_dir * origin_ring_r;

    // Departure tangent: the prograde direction at that ring point.
    let rad_at_p0 = (p0 - op).normalize_or_zero();
    let prograde_at_p0 = Vec3::new(-rad_at_p0.y, rad_at_p0.x, 0.0);
    let tang0 = if is_outward1 { prograde_at_p0 } else { -prograde_at_p0 };

    // ── Arrival ──────────────────────────────────────────────────────────────
    let dir_from_flyby = (dp - fp).normalize_or_zero();

    // Is the destination further from the sun than the flyby body?
    let is_outward2 = dp.length_squared() > fp.length_squared();

    // Arrival ring point: rotate inbound direction by 90° so the fleet arrives
    // where prograde/retrograde aligns with the incoming trajectory.
    // Same rotation convention as departure: CW for outward, CCW for inward.
    let inbound = -dir_from_flyby;
    let arr_dir = if is_outward2 {
        Vec3::new(inbound.y, -inbound.x, 0.0)   // CW for outer arrival
    } else {
        Vec3::new(-inbound.y, inbound.x, 0.0)   // CCW for inner arrival
    };
    let p3_2 = dp + arr_dir * dest_ring_r;

    // Arrival tangent: the inbound direction (reversed from dir_from_flyby),
    // so the Bezier arrives aligned with the incoming leg.
    let td2 = inbound;

    // ── Hyperbolic periapsis ─────────────────────────────────────────────────
    let dir_approach = dir_to_flyby;
    let dir_depart   = dir_from_flyby;
    let apse_raw = dir_approach - dir_depart;
    let apse_dir = if apse_raw.length() > 0.001 {
        apse_raw.normalize()
    } else {
        Vec3::new(-dir_approach.y, dir_approach.x, 0.0)
    };
    let periapsis = fp + apse_dir * (flyby_ring_r * 2.0);
    let tang_perp_a = Vec3::new(-apse_dir.y, apse_dir.x, 0.0);
    let peri_tang = if tang_perp_a.dot(dir_depart) >= 0.0 { tang_perp_a } else { -tang_perp_a };

    // ── Bezier control points ────────────────────────────────────────────────
    let cl1 = (periapsis - p0).length() * 0.40;
    let p1_1 = p0        + tang0     * cl1;
    let p2_1 = periapsis - peri_tang * cl1;

    let cl2 = (p3_2 - periapsis).length() * 0.40;
    let p1_2 = periapsis + peri_tang * cl2;
    let p2_2 = p3_2      - td2       * cl2;

    GravityAssistArcGeometry {
        p0_1: p0, p1_1, p2_1, p3_1: periapsis,
        p0_2: periapsis, p1_2, p2_2, p3_2,
    }
}

// ── Public drawing / mesh systems ─────────────────────────────────────────────

/// Draw the trajectory arc for the selected fleet.
/// In System view uses SCALING_FACTOR; in Starmap view uses raw AU (1 unit = 1 AU).
///
/// For local (non-heliocentric) transfers the trajectory is drawn in **visual space**
/// by interpolating between the origin and destination body render positions.
/// Heliocentric transfers continue to use the physics-accurate Keplerian arc.
pub fn draw_fleet_trajectories(
    mut gizmos: Gizmos,
    fleet_query: Query<(Entity, &ActiveManeuver, Option<&FleetOrbit>), With<Fleet>>,
    center_coords: Query<&SpaceCoordinates, Without<Fleet>>,
    body_query: Query<(&Transform, &CelestialBody, Option<&LogicalParent>), Without<Fleet>>,
    kepler_query: Query<&KeplerOrbit, Without<Fleet>>,
    amp_query: Query<&LocalOrbitAmplification, Without<Fleet>>,
    floating_origin: Option<Res<FloatingOrigin>>,
    mut fleet_ui_state: ResMut<FleetUiState>,
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

    fleet_ui_state.waiting_orbit_count = 0;

    for (entity, maneuver, maybe_orbit) in fleet_query.iter() {
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

                // Pin the arc origin to the predicted departure position (works both
                // before and after departure).  Before departure this shows where the
                // origin body *will be* at departure time; after departure it fixes the
                // arc start at the historical departure position, anchored to the current
                // render position of the orbit-centre body so the arc follows the
                // planet's visual drift in the system view.
                let op = if let Some(start_pos) = maneuver.start_visual_pos {
                    // For course corrections, the fleet started mid-transit.
                    // Anchor its historical start position to the orbit centre's current position.
                    let cv_at_departure = predict_body_visual_pos(
                        maneuver.orbit_center,
                        maneuver.departure_time,
                        &body_query,
                        &kepler_query,
                        &amp_query,
                    ).unwrap_or(cv_current);
                    start_pos - cv_at_departure + cv_current
                } else {
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
                let actual_origin_ring_r = if maneuver.start_visual_pos.is_some() { 0.0 } else { origin_ring_r };
                let geo = compute_transfer_arc(
                    op, dp, actual_origin_ring_r, dest_ring_r,
                    maneuver.start_visual_pos.is_some(), is_inward, maneuver.is_kinematic(), cv_current,
                );
                let dep_angle = geo.departure_angle;

                // ── Waiting arc: shown when departure is scheduled in the future ────────
                // When the origin body is a moon (has a logical parent), draw the arc along
                // the moon's orbit around its parent planet — this is far more readable than
                // the tiny fleet-parking ring.  For planet-origin transfers fall back to the
                // parking ring arc around the origin body.
                let is_pre_departure = sim_elapsed < maneuver.departure_time;
                if is_pre_departure {
                    let tau = std::f32::consts::TAU;

                    if origin_lp.is_some() {
                        // ── Moon origin: arc along the moon's orbit around the planet ──
                        // current moon angle and orbit radius come from op_current vs planet.
                        let rel_current = op_current - cv_current;
                        let moon_orbit_r   = rel_current.length();
                        let current_moon_a = rel_current.y.atan2(rel_current.x);
                        // departure angle of the moon relative to the planet (in current frame).
                        let rel_depart     = op - cv_current;
                        let depart_moon_a  = rel_depart.y.atan2(rel_depart.x);
                        // Moons orbit prograde (CCW).
                        let waiting_angle  = (depart_moon_a - current_moon_a).rem_euclid(tau);
                        let full_orbits    = (waiting_angle / tau) as u32;
                        let last_arc_angle = waiting_angle % tau;
                        fleet_ui_state.waiting_orbit_count = full_orbits + 1;

                        // Dim full ring for each complete extra revolution.
                        if full_orbits > 0 {
                            for i in 0..64u32 {
                                let a0 = i as f32 / 64.0 * tau;
                                let a1 = (i + 1) as f32 / 64.0 * tau;
                                gizmos.line(
                                    cv_current + Vec3::new(a0.cos() * moon_orbit_r, a0.sin() * moon_orbit_r, 0.0),
                                    cv_current + Vec3::new(a1.cos() * moon_orbit_r, a1.sin() * moon_orbit_r, 0.0),
                                    Color::linear_rgba(0.50, 0.05, 0.80, 0.10),
                                );
                            }
                        }
                        // Partial arc brightening CCW from the moon's current position to the
                        // departure position.
                        for i in 0..48u32 {
                            let t0    = i as f32 / 48.0;
                            let t1    = (i + 1) as f32 / 48.0;
                            let alpha = 0.10 + 0.22 * t0;
                            let a0 = current_moon_a + last_arc_angle * t0;
                            let a1 = current_moon_a + last_arc_angle * t1;
                            gizmos.line(
                                cv_current + Vec3::new(a0.cos() * moon_orbit_r, a0.sin() * moon_orbit_r, 0.0),
                                cv_current + Vec3::new(a1.cos() * moon_orbit_r, a1.sin() * moon_orbit_r, 0.0),
                                Color::linear_rgba(0.55, 0.05, 0.85, alpha),
                            );
                        }
                        // Small tick at the moon's current orbital position.
                        let tick = 5.0_f32;
                        let tick_col = Color::linear_rgba(0.65, 0.10, 0.90, 0.65);
                        gizmos.line(op_current - Vec3::X * tick, op_current + Vec3::X * tick, tick_col);
                        gizmos.line(op_current - Vec3::Y * tick, op_current + Vec3::Y * tick, tick_col);
                    } else if let Some(orbit) = maybe_orbit {
                        // ── Planet origin: arc along the fleet's parking ring ──
                        if orbit.direction != 0.0 && orbit.body == maneuver.origin_body {
                            let orbit_center = body_query.get(maneuver.origin_body)
                                .map(|(t, _, _)| t.translation).unwrap_or(Vec3::ZERO);
                            let current_a = orbit.angle_rad as f32;
                            let waiting_angle = if orbit.direction > 0.0 {
                                (dep_angle - current_a).rem_euclid(tau)
                            } else {
                                (current_a - dep_angle).rem_euclid(tau)
                            };
                            let full_orbits    = (waiting_angle / tau) as u32;
                            let last_arc_angle = waiting_angle % tau;
                            fleet_ui_state.waiting_orbit_count = full_orbits + 1;

                            let r = origin_ring_r;
                            if full_orbits > 0 {
                                for i in 0..64u32 {
                                    let a0 = i as f32 / 64.0 * tau;
                                    let a1 = (i + 1) as f32 / 64.0 * tau;
                                    gizmos.line(
                                        orbit_center + Vec3::new(a0.cos() * r, a0.sin() * r, 0.0),
                                        orbit_center + Vec3::new(a1.cos() * r, a1.sin() * r, 0.0),
                                        Color::linear_rgba(0.50, 0.05, 0.80, 0.10),
                                    );
                                }
                            }
                            let arc_start = dep_angle - orbit.direction as f32 * last_arc_angle;
                            for i in 0..48u32 {
                                let t0    = i as f32 / 48.0;
                                let t1    = (i + 1) as f32 / 48.0;
                                let alpha = 0.10 + 0.22 * t0;
                                let a0 = arc_start + orbit.direction as f32 * last_arc_angle * t0;
                                let a1 = arc_start + orbit.direction as f32 * last_arc_angle * t1;
                                gizmos.line(
                                    orbit_center + Vec3::new(a0.cos() * r, a0.sin() * r, 0.0),
                                    orbit_center + Vec3::new(a1.cos() * r, a1.sin() * r, 0.0),
                                    Color::linear_rgba(0.55, 0.05, 0.85, alpha),
                                );
                            }
                            let fleet_pos = orbit_center + Vec3::new(current_a.cos() * r, current_a.sin() * r, 0.0);
                            let tick = 5.0_f32;
                            let tick_col = Color::linear_rgba(0.65, 0.10, 0.90, 0.65);
                            gizmos.line(fleet_pos - Vec3::X * tick, fleet_pos + Vec3::X * tick, tick_col);
                            gizmos.line(fleet_pos - Vec3::Y * tick, fleet_pos + Vec3::Y * tick, tick_col);
                        }
                    }
                }

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

                // Draw remaining arc with shared geometry.
                let mut prev: Option<Vec3> = Some(geo.eval(progress_t));
                for i in 0..=SEGMENTS {
                    let t_frac = i as f32 / SEGMENTS as f32;
                    if t_frac <= progress_t { continue; }
                    let pos = geo.eval(t_frac);
                    if let Some(prev_pos) = prev {
                        gizmos.line(prev_pos, pos, traj_color(t_frac));
                    }
                    prev = Some(pos);
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

        // ── Heliocentric System view: Bezier arc matching the preview style ────────────────
        // Even though the physics position is Keplerian, we draw the in-transit arc as the
        // same cubic Bezier used by the preview so it looks consistent and starts from the
        // origin body's visual position (e.g. the moon) rather than the planet orbit centre.
        // All geometry logic is copied verbatim from `draw_fleet_transfer_preview` to ensure
        // preview == actual.
        if center_is_star && *view_mode == ViewMode::System {
            let origin_lp = body_query.get(maneuver.origin_body)
                .ok().and_then(|(_, _, lp)| lp.map(|lp| lp.0));
            let dest_lp = body_query.get(maneuver.destination_body)
                .ok().and_then(|(_, _, lp)| lp.map(|lp| lp.0));

            let origin_ring_r = body_query.get(maneuver.origin_body)
                .map(|(_, b, _)| fleet_parking_visual_radius(b.visual_radius))
                .unwrap_or(0.0);
            let (dest_visual_r, dest_ring_r) = body_query.get(maneuver.destination_body)
                .map(|(_, b, _)| (b.visual_radius, fleet_parking_visual_radius(b.visual_radius)))
                .unwrap_or((0.0, 0.0));

            // ── Departure point (op) ─────────────────────────────────────────
            // Course corrections: use the stored visual start position so it matches
            // what the preview showed at the time of the correction.
            let origin_now = body_query.get(maneuver.origin_body)
                .map(|(t, _, _)| t.translation).unwrap_or(Vec3::ZERO);
            let is_course_correction = maneuver.start_visual_pos.is_some();
            let actual_origin_ring_r = if is_course_correction { 0.0 } else { origin_ring_r };
            let op = if let Some(start_pos) = maneuver.start_visual_pos {
                start_pos
            } else {
                predict_body_visual_pos(
                    maneuver.origin_body, maneuver.departure_time,
                    &body_query, &kepler_query, &amp_query,
                ).unwrap_or(origin_now)
            };

            // ── Arrival point (dp) ───────────────────────────────────────────
            // Identical to preview: predict destination at arrival, then correct
            // for the orbit-centre drift so the arc endpoint tracks the body now.
            let dest_now = body_query.get(maneuver.destination_body)
                .map(|(t, _, _)| t.translation).unwrap_or(Vec3::ZERO);
            let dp_absolute = predict_body_visual_pos(
                maneuver.destination_body, maneuver.arrival_time,
                &body_query, &kepler_query, &amp_query,
            ).unwrap_or(dest_now);

            let orbit_center_visual = dest_lp.unwrap_or(maneuver.destination_body);
            let cv_predicted = predict_body_visual_pos(
                orbit_center_visual, maneuver.arrival_time,
                &body_query, &kepler_query, &amp_query,
            ).unwrap_or(dp_absolute);
            let cv_at_departure = predict_body_visual_pos(
                orbit_center_visual, maneuver.departure_time,
                &body_query, &kepler_query, &amp_query,
            ).unwrap_or(cv_predicted);
            let dp = dp_absolute - cv_predicted + cv_at_departure;

            // --- gravity assist special case ------------------------------------------------
            if let (Some(flyby), Some(_)) = (maneuver.flyby_body, maneuver.leg2_orbit.as_ref()) {
                // predict flyby position at the time the second leg begins
                let fp_absolute = predict_body_visual_pos(
                    flyby,
                    maneuver.departure_time + maneuver.leg2_start_s,
                    &body_query,
                    &kepler_query,
                    &amp_query,
                ).unwrap_or_else(|| {
                    body_query
                        .get(flyby)
                        .ok()
                        .map(|(t, _, _)| t.translation)
                        .unwrap_or(Vec3::ZERO)
                });
                let fp = fp_absolute - cv_predicted + cv_at_departure;

                // ring radii and visual size
                let flyby_visual_r = body_query
                    .get(flyby)
                    .map(|(_, b, _)| b.visual_radius)
                    .unwrap_or(0.0);
                let flyby_ring_r = fleet_parking_visual_radius(flyby_visual_r);

                // geometry for the two legs — shared helper produces C1-continuous slingshot
                let ga_geo = compute_gravity_assist_arc(
                    op, fp, dp,
                    actual_origin_ring_r,
                    flyby_ring_r,
                    dest_ring_r,
                );

                // fractions along total t range where leg switch happens
                let leg1_frac = if maneuver.arrival_time > maneuver.departure_time {
                    (maneuver.leg2_start_s
                        / (maneuver.arrival_time - maneuver.departure_time)) as f32
                } else {
                    0.5_f32
                };
                let leg2_frac = 1.0 - leg1_frac;

                // progress / glow etc reuse below
                let progress_t = if maneuver.arrival_time > maneuver.departure_time {
                    ((sim_elapsed - maneuver.departure_time)
                        / (maneuver.arrival_time - maneuver.departure_time))
                        .clamp(0.0, 1.0) as f32
                } else {
                    1.0_f32
                };
                let remaining_span = (1.0 - progress_t).max(1e-4_f32);
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

                // draw remaining piecewise along ga_geo leg1/leg2
                let eval_at = |t_frac: f32| -> Vec3 {
                    if t_frac < leg1_frac {
                        ga_geo.eval_leg1(t_frac / leg1_frac)
                    } else {
                        ga_geo.eval_leg2((t_frac - leg1_frac) / leg2_frac.max(1e-6))
                    }
                };
                let mut prev: Option<Vec3> = Some(eval_at(progress_t));

                for i in 0..=SEGMENTS {
                    let t_frac = i as f32 / SEGMENTS as f32;
                    if t_frac <= progress_t { continue; }
                    let pos = eval_at(t_frac);
                    if let Some(prev_pos) = prev {
                        gizmos.line(prev_pos, pos, traj_color(t_frac));
                    }
                    prev = Some(pos);
                }

                // ghost bodies at flyby and destination
                draw_ghost_body(&mut gizmos, fp, flyby_ring_r, flyby_visual_r, false);
                let dest_is_orbit_center = origin_lp == Some(maneuver.destination_body);
                draw_ghost_body(&mut gizmos, dp, dest_ring_r, dest_visual_r, dest_is_orbit_center);
                continue;
            }

            // ── Inward / outward ─────────────────────────────────────────────
            let is_inward = if is_course_correction {
                op.length_squared() > dp.length_squared()
            } else if origin_lp == Some(maneuver.destination_body) {
                true
            } else if dest_lp == Some(maneuver.origin_body) {
                false
            } else {
                op.length_squared() > dp.length_squared()
            };

            // ── Shared geometry ───────────────────────────────────────────
            let geo = compute_transfer_arc(
                op, dp, actual_origin_ring_r, dest_ring_r,
                is_course_correction, is_inward, maneuver.is_kinematic(), cv_at_departure,
            );

            // ── Progress & glow ──────────────────────────────────────────────
            let progress_t = if maneuver.arrival_time > maneuver.departure_time {
                ((sim_elapsed - maneuver.departure_time)
                    / (maneuver.arrival_time - maneuver.departure_time))
                    .clamp(0.0, 1.0) as f32
            } else {
                1.0_f32
            };
            let remaining_span = (1.0 - progress_t).max(1e-4_f32);
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

            // Draw remaining arc with shared geometry.
            let mut prev: Option<Vec3> = Some(geo.eval(progress_t));
            for i in 0..=SEGMENTS {
                let t_frac = i as f32 / SEGMENTS as f32;
                if t_frac <= progress_t { continue; }
                let pos = geo.eval(t_frac);
                if let Some(prev_pos) = prev {
                    gizmos.line(prev_pos, pos, traj_color(t_frac));
                }
                prev = Some(pos);
            }

            draw_ghost_body(&mut gizmos, dp, dest_ring_r, dest_visual_r, false);
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
        // A fleet that has both FleetOrbit AND ActiveManeuver is in the departure frame —
        // the FleetOrbit removal is still pending (deferred command). Treat it as in-transit
        // once the maneuver's departure_time has been reached so we use Keplerian positions
        // rather than the moon's amplified visual Transform.
        let maneuver_started = maybe_maneuver
            .map(|m| elapsed >= m.departure_time)
            .unwrap_or(false);
        let is_in_transit = maybe_maneuver.is_some();

        // Hide parked (non-transiting) fleets that are not selected.
        if !is_in_transit && !is_selected {
            *vis = Visibility::Hidden;
            // Still update position so the reticule and orbit ring are accurate.
        } else {
            *vis = Visibility::Inherited;
        }

        if let Some(orbit) = maybe_orbit.filter(|_| !maneuver_started) {
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
                    let op = if let Some(start_pos) = maneuver.start_visual_pos {
                        let cv_at_departure = predict_body_visual_pos(
                            maneuver.orbit_center,
                            maneuver.departure_time,
                            &body_query,
                            &kepler_query,
                            &amp_query,
                        ).unwrap_or(cv_current);
                        start_pos - cv_at_departure + cv_current
                    } else {
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

                    // Use shared geometry — guarantees dot follows the same curve
                    // as the transit gizmo and preview.
                    let origin_lp = body_query.get(maneuver.origin_body)
                        .ok().and_then(|(_, _, lp)| lp.map(|lp| lp.0));
                    let dest_lp = body_query.get(maneuver.destination_body)
                        .ok().and_then(|(_, _, lp)| lp.map(|lp| lp.0));
                    let is_inward = if origin_lp == Some(maneuver.destination_body) {
                        true
                    } else if dest_lp == Some(maneuver.origin_body) {
                        false
                    } else {
                        op.length_squared() > dp.length_squared()
                    };
                    let actual_origin_ring_r = if maneuver.start_visual_pos.is_some() { 0.0 } else { origin_ring_r };
                    let geo = compute_transfer_arc(
                        op, dp, actual_origin_ring_r, dest_ring_r,
                        maneuver.start_visual_pos.is_some(), is_inward, maneuver.is_kinematic(), cv_current,
                    );
                    transform.translation = geo.eval(progress);

                    // Hide the sphere while still inside the origin or destination orbit ring.
                    let inside_origin = transform.translation.distance(op) < origin_ring_r;
                    let inside_dest   = transform.translation.distance(dp) < dest_ring_r;
                    if inside_origin || inside_dest {
                        *vis = Visibility::Hidden;
                    }
                }
            } else {
                // Heliocentric transfer in System view: follow Bezier matching the
                // trajectory gizmo so the green dot tracks the purple arc exactly.
                // The physics SpaceCoordinates (Keplerian) are kept for range queries;
                // only the visual Transform follows the Bezier.
                let origin_lp = body_query.get(maneuver.origin_body)
                    .ok().and_then(|(_, _, lp)| lp.map(|lp| lp.0));
                let dest_lp_entity = body_query.get(maneuver.destination_body)
                    .ok().and_then(|(_, _, lp)| lp.map(|lp| lp.0));

                let origin_ring_r = body_query.get(maneuver.origin_body)
                    .map(|(_, b, _)| fleet_parking_visual_radius(b.visual_radius))
                    .unwrap_or(0.0);
                let dest_ring_r = body_query.get(maneuver.destination_body)
                    .map(|(_, b, _)| fleet_parking_visual_radius(b.visual_radius))
                    .unwrap_or(0.0);

                let origin_now = body_query.get(maneuver.origin_body)
                    .map(|(t, _, _)| t.translation).unwrap_or(Vec3::ZERO);
                let is_course_correction = maneuver.start_visual_pos.is_some();
                let actual_origin_ring_r = if is_course_correction { 0.0 } else { origin_ring_r };
                let op = if let Some(start_pos) = maneuver.start_visual_pos {
                    start_pos
                } else {
                    predict_body_visual_pos(
                        maneuver.origin_body, maneuver.departure_time,
                        &body_query, &kepler_query, &amp_query,
                    ).unwrap_or(origin_now)
                };

                let dest_now = body_query.get(maneuver.destination_body)
                    .map(|(t, _, _)| t.translation).unwrap_or(Vec3::ZERO);
                let dp_absolute = predict_body_visual_pos(
                    maneuver.destination_body, maneuver.arrival_time,
                    &body_query, &kepler_query, &amp_query,
                ).unwrap_or(dest_now);

                let orbit_center_visual = dest_lp_entity.unwrap_or(maneuver.destination_body);
                let cv_predicted = predict_body_visual_pos(
                    orbit_center_visual, maneuver.arrival_time,
                    &body_query, &kepler_query, &amp_query,
                ).unwrap_or(dp_absolute);
                let cv_at_departure = predict_body_visual_pos(
                    orbit_center_visual, maneuver.departure_time,
                    &body_query, &kepler_query, &amp_query,
                ).unwrap_or(cv_predicted);
                let dp = dp_absolute - cv_predicted + cv_at_departure;

                let is_inward = if is_course_correction {
                    op.length_squared() > dp.length_squared()
                } else if origin_lp == Some(maneuver.destination_body) {
                    true
                } else if dest_lp_entity == Some(maneuver.origin_body) {
                    false
                } else {
                    op.length_squared() > dp.length_squared()
                };

                let geo = compute_transfer_arc(
                    op, dp, actual_origin_ring_r, dest_ring_r,
                    is_course_correction, is_inward, maneuver.is_kinematic(), cv_at_departure,
                );

                let progress = maneuver.progress(elapsed) as f32;
                transform.translation = geo.eval(progress);

                // Hide inside origin/dest orbit rings
                let inside_origin = transform.translation.distance(op) < origin_ring_r;
                let inside_dest   = transform.translation.distance(dp) < dest_ring_r;
                if inside_origin || inside_dest {
                    *vis = Visibility::Hidden;
                }
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

    // If a gravity assist candidate has been chosen, the transfer planner shows a
    // specialised two-leg trajectory.  In that case skip the standard amber preview
    // arc entirely; the assist code will draw its own overlay on top.
    if fleet_ui_state.selected_gravity_assist.is_some() {
        return;
    }

    // Hoist fleet-state lookup so both LP and regular branches can share it.
    let Ok((_, fleet_transform, maybe_orbit, maybe_maneuver)) = fleet_query.get(fleet_entity) else { return; };
    let elapsed = sim_time.elapsed_seconds();

    let current_sim_s      = elapsed;
    let departure_offset_s = fleet_ui_state.departure_offset_days * 86_400.0;
    let departure_s        = current_sim_s + departure_offset_s;

    // Course correction: fleet is actively mid-transit but the transfer planner is open.
    // Instead of returning early with a straight line, we override the departure point
    // (p0) to be the fleet's current render position and fall through to the normal
    // Bezier/kinematic preview arc so the player sees the actual trajectory shape.
    let is_course_correction = maybe_maneuver
        .map(|man| elapsed >= man.departure_time)
        .unwrap_or(false);
    // For course corrections the fleet's current Transform IS the departure point;
    // for normal transfers it's the predicted origin body position at departure time.
    let course_correction_fleet_pos: Option<Vec3> = if is_course_correction {
        Some(fleet_transform.translation)
    } else {
        None
    };

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
        let op = if let Some(fleet_pos) = course_correction_fleet_pos {
            fleet_pos
        } else {
            predict_body_visual_pos(origin_body, departure_s, &body_query, &kepler_query, &amp_query)
                .unwrap_or(origin_transform.translation)
        };
        let origin_ring_r = if is_course_correction {
            0.0
        } else {
            fleet_parking_visual_radius(origin_body_data.visual_radius)
        };

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

        let is_inward = op.length_squared() > dp.length_squared();
        let geo = compute_transfer_arc(
            op, dp, origin_ring_r, lp_marker_r,
            is_course_correction, is_inward, false, Vec3::ZERO,
        );

        // Dashed amber arc — arc-length-uniform.
        draw_dashed_curve(
            &mut gizmos, |t| geo.eval(t), 24,
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
        let op = if let Some(fleet_pos) = course_correction_fleet_pos {
            fleet_pos
        } else {
            predict_body_visual_pos(origin_body, departure_s, &body_query, &kepler_query, &amp_query)
                .unwrap_or(origin_transform.translation)
        };
        let origin_ring_r = if is_course_correction {
            0.0
        } else {
            fleet_parking_visual_radius(origin_body_data.visual_radius)
        };

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
        // cv_ref = Vec3::ZERO (star) for fleet-intercept — no specific orbit centre.
        let geo = compute_transfer_arc(
            op, dp, origin_ring_r, marker_r,
            is_course_correction, is_inward, is_kinematic, Vec3::ZERO,
        );

        draw_dashed_curve(
            &mut gizmos,
            |t| geo.eval(t),
            24,
            |f| Color::srgba(1.0, 0.75, 0.15, 0.70 - 0.35 * f),
        );

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

    let origin_body = if is_course_correction {
        // During course corrections, FleetOrbit may not exist — the fleet is mid-transit.
        // Use the active maneuver's origin body as the conceptual origin.
        if let Some(orbit) = maybe_orbit {
            orbit.body
        } else if let Some(man) = maybe_maneuver {
            man.origin_body
        } else {
            return;
        }
    } else if let Some(orbit) = maybe_orbit {
        orbit.body
    } else {
        return;
    };

    if origin_body == target_entity { return; }

    let Ok((origin_transform, origin_body_data, origin_lp)) = body_query.get(origin_body)   else { return; };
    let Ok((dest_transform_now, dest_body_data, dest_lp))  = body_query.get(target_entity) else { return; };

    // For course corrections: departure point = fleet's current render position.
    // For normal transfers: departure point = predicted origin body position at departure.
    let op = if let Some(fleet_pos) = course_correction_fleet_pos {
        fleet_pos
    } else {
        predict_body_visual_pos(origin_body, departure_s, &body_query, &kepler_query, &amp_query)
            .unwrap_or(origin_transform.translation)
    };
    // Origin ring radius is 0 for course corrections (fleet is already at the departure point).
    let origin_ring_r = if is_course_correction {
        0.0
    } else {
        fleet_parking_visual_radius(origin_body_data.visual_radius)
    };
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
    let is_inward = if is_course_correction {
        // For course corrections, compare the fleet's current distance to the destination distance.
        op.length_squared() > dp.length_squared()
    } else if origin_lp.map(|lp| lp.0) == Some(target_entity) {
        // Origin orbits the destination (e.g. Moon → Earth): always inward.
        true
    } else if dest_lp.map(|lp| lp.0) == Some(origin_body) {
        // Destination orbits the origin (e.g. Earth → Moon): always outward.
        false
    } else {
        // Same parent (e.g. Mars → Earth): compare distances from the star.
        op.length_squared() > dp.length_squared()
    };

    // Check whether the selected option is a kinematic transfer.
    let is_kinematic = fleet_ui_state.computed_options
        .get(fleet_ui_state.selected_option)
        .map(|opt| opt.label == "Full Thrust" || opt.label.contains("Coast") || opt.label == "Max Speed" || opt.label.contains("Direct"))
        .unwrap_or(false);

    // Shared geometry — identical curve to transit gizmo and fleet dot.
    let geo = compute_transfer_arc(
        op, dp, origin_ring_r, dest_ring_r,
        is_course_correction, is_inward, is_kinematic, cv_at_departure,
    );

    // Dashed amber arc.
    draw_dashed_curve(
        &mut gizmos,
        |t| geo.eval(t),
        24,
        |f| Color::srgba(1.0, 0.75, 0.15, 0.70 - 0.35 * f),
    );

    // Ghost body at predicted arrival position.
    // In the preview the destination is the body we are flying TO, not the orbit
    // centre, so the outer ring is always wanted here.
    draw_ghost_body(&mut gizmos, dp, dest_ring_r, dest_visual_r, false);
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

    // ── Shared geometry via the same helper used during active transit ───────
    let flyby_ring_r = fleet_parking_visual_radius(flyby_bd.visual_radius);
    let ga_geo = compute_gravity_assist_arc(op, fp, dp, origin_ring_r, flyby_ring_r, dest_ring_r);

    let leg1 = |t: f32| ga_geo.eval_leg1(t);
    let leg2 = |t: f32| ga_geo.eval_leg2(t);

    // ── Draw both legs with arc-length-uniform dashing ───────────────────────

    // Leg 1: lime-green approach arc.
    draw_dashed_curve(
        &mut gizmos, &leg1, 24,
        |f| Color::srgba(0.3, 1.0, 0.4, 0.80 - 0.35 * f),
    );

    // Leg 2: magenta departure arc.
    draw_dashed_curve(
        &mut gizmos, &leg2, 24,
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
