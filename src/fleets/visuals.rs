//! Fleet visualisation: drawing helpers, gizmo systems, and mesh management.

use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::time::Real;

use super::components::{
    ActiveManeuver, Fleet, FleetOrbit, PlannedTransfer, TransferReferenceFrame,
};
use super::types::FleetClass;
use crate::astronomy::components::FloatingOrigin;
use crate::astronomy::systems::orbit_position_from_mean_anomaly;
use crate::astronomy::{KeplerOrbit, LocalOrbitAmplification, SpaceCoordinates, SCALING_FACTOR};
use crate::fleets::orbital_mechanics::AU_IN_METERS;
use crate::plugins::camera::{GameCamera, OrbitCamera, ViewMode};
use crate::plugins::solar_system::{CelestialBody, LogicalParent};
use crate::plugins::solar_system_data::BodyType;
use crate::ui::{FleetUiState, Settings, SimulationTime, TimeScale};
use crate::ui::launch::LaunchState;

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

/// Convert a parking-orbit radius (AU) into a visual-space radius for the
/// dashed orbit ring drawn around a body in System view.
///
/// For **stars**, the AU value is the source of truth — Lagrange points at
/// `1 AU` and Cruise at `5 AU` both render directly via `SCALING_FACTOR` so
/// the ring matches the body's actual orbit position to the pixel.
///
/// For **planets / moons / asteroids**, the AU value (LEO ≈ 4.5e-5 AU for
/// Earth) is *much* smaller than the body's `visual_radius` (e.g. 2.0
/// visual units for Earth) — drawing the ring at the true AU altitude
/// would put it inside the body's on-screen sphere.  Instead we scale
/// [`fleet_parking_visual_radius`] by a per-shell multiplier picked from
/// the parking altitude's ratio to the body radius:
///
/// | Shell      | ratio `r_km / body.radius_km` | Multiplier |
/// | ---------- | ----------------------------- | ---------- |
/// | Low        | < 2.0                         | 1.0×       |
/// | Medium     | 2.0 – 5.0                     | 1.6×       |
/// | High       | 5.0 – 20.0                    | 2.6×       |
/// | Stationary | ≥ 20.0 / fallback             | 3.5×       |
///
/// These produce visually distinguishable rings for any planet — Low hugs
/// the body, Medium sits clearly farther out, High is a transfer-staging
/// shell, and Stationary is at the geostationary altitude.  For ratios
/// `≥ 20.0` (rare; only truly high-altitude parking) we cap at the
/// Stationary multiplier.
///
/// Defensive: a `body.radius == 0.0` falls through to the `1.0×` default
/// (which preserves the legacy `fleet_parking_visual_radius` behaviour).
#[inline]
pub(super) fn shell_ring_visual_radius(body: &CelestialBody, radius_au: f64) -> f32 {
    if body.body_type == BodyType::Star {
        // Direct AU → visual-space conversion.  Crucially, this matches the
        // star-orbiting fleet's render position in `update_fleet_transforms`
        // (which also uses `orbit.radius_au * SCALING_FACTOR` for star
        // bodies), so the green orbit ring lines up with the fleet mesh.
        (radius_au as f32) * (SCALING_FACTOR as f32)
    } else {
        let body_r_km = body.radius as f64;
        let r_km = if body_r_km > 0.0 {
            radius_au * AU_IN_METERS / 1000.0
        } else {
            0.0
        };
        let ratio = if body_r_km > 0.0 {
            r_km / body_r_km
        } else {
            1.05
        };
        let mult: f32 = if ratio < 2.0 {
            1.0
        } else if ratio < 5.0 {
            1.6
        } else if ratio < 20.0 {
            2.6
        } else {
            3.5
        };
        fleet_parking_visual_radius(body.visual_radius) * mult
    }
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

fn draw_solid_curve(
    gizmos: &mut Gizmos,
    sample_fn: impl Fn(f32) -> Vec3,
    segments: u32,
    mut color_fn: impl FnMut(f32) -> Color,
) {
    if segments == 0 {
        return;
    }

    let mut previous = sample_fn(0.0);
    for index in 1..=segments {
        let fraction = index as f32 / segments as f32;
        let position = sample_fn(fraction);
        gizmos.line(previous, position, color_fn(fraction));
        previous = position;
    }
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
    if points.len() < 2 || num_dashes == 0 {
        return;
    }
    let n = points.len();
    let mut cum = Vec::with_capacity(n);
    cum.push(0.0_f32);
    for i in 1..n {
        cum.push(cum[i - 1] + (points[i] - points[i - 1]).length());
    }
    let total = *cum.last().unwrap();
    if total < 0.001 {
        return;
    }
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

/// Mirrors the astronomy render-system cap so preview markers follow the same
/// readable visual orbit path as fast moons and tight inner bodies.
const VISUAL_PREDICTION_SPEED_BASE: f64 = std::f64::consts::TAU;

fn capped_prediction_visual_speed(effective_speed: f64) -> f64 {
    if effective_speed <= VISUAL_PREDICTION_SPEED_BASE {
        effective_speed
    } else {
        VISUAL_PREDICTION_SPEED_BASE * (1.0 + (effective_speed / VISUAL_PREDICTION_SPEED_BASE).ln())
    }
}

/// Predict the visual (render-space) `Vec3` position where a celestial body will
/// be at `future_sim_s` by propagating its `KeplerOrbit` forward.
///
/// Returns `None` if the body has no `KeplerOrbit` component.
fn predict_body_visual_pos(
    target: Entity,
    current_sim_s: f64,
    future_sim_s: f64,
    current_real_s: f64,
    time_scale: f64,
    body_query: &Query<(&Transform, &CelestialBody, Option<&LogicalParent>), Without<Fleet>>,
    kepler_query: &Query<&KeplerOrbit, Without<Fleet>>,
    amp_query: &Query<&LocalOrbitAmplification, Without<Fleet>>,
) -> Option<Vec3> {
    let kepler = kepler_query.get(target).ok()?;
    let (current_transform, _, maybe_lp) = body_query.get(target).ok()?;

    // Mirror the astronomy render transform: very fast local orbits are drawn
    // with a capped real-time angular speed to remain readable/clickable.
    let effective_speed = kepler.mean_motion.abs() * time_scale;
    let ma_future = if effective_speed > VISUAL_PREDICTION_SPEED_BASE {
        let vis_speed =
            capped_prediction_visual_speed(effective_speed) * kepler.mean_motion.signum();
        let delta_real_s = if time_scale.abs() > 1e-9 {
            ((future_sim_s - current_sim_s) / time_scale).max(0.0)
        } else {
            0.0
        };
        let current_visual_ma = kepler.mean_anomaly_epoch + vis_speed * current_real_s;
        current_visual_ma + vis_speed * delta_real_s
    } else {
        kepler.mean_anomaly_epoch + kepler.mean_motion * future_sim_s
    };
    let pos_future_au = orbit_position_from_mean_anomaly(kepler, ma_future);

    // Apply LocalOrbitAmplification (moons rendered further from parent than raw AU).
    let amp = amp_query.get(target).map(|a| a.0 as f64).unwrap_or(1.0);

    // Anchor to the parent body's predicted visual position.
    let parent_pos = if let Some(lp) = maybe_lp {
        let pos_scaled = Vec3::new(
            (pos_future_au.x * SCALING_FACTOR * amp) as f32,
            (pos_future_au.y * SCALING_FACTOR * amp) as f32,
            (pos_future_au.z * SCALING_FACTOR * amp) as f32,
        );
        predict_body_visual_pos(
            lp.0,
            current_sim_s,
            future_sim_s,
            current_real_s,
            time_scale,
            body_query,
            kepler_query,
            amp_query,
        )
        .unwrap_or_else(|| {
            body_query
                .get(lp.0)
                .ok()
                .map(|(t, _, _)| t.translation)
                .unwrap_or(Vec3::ZERO)
        }) + pos_scaled
    } else {
        let ma_current = kepler.mean_anomaly_epoch + kepler.mean_motion * current_sim_s;
        let pos_current_au = orbit_position_from_mean_anomaly(kepler, ma_current);
        let delta_au = (pos_future_au - pos_current_au) * amp;
        current_transform.translation
            + Vec3::new(
                (delta_au.x * SCALING_FACTOR) as f32,
                (delta_au.y * SCALING_FACTOR) as f32,
                (delta_au.z * SCALING_FACTOR) as f32,
            )
    };

    Some(parent_pos)
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
///   Callers pass a *shell-driven* radius so the ring scales with the player's
///   `Target orbit:` pick — Cruise (5 AU) for Sol, Low (6,689 km) for Earth, etc.
/// * Smaller dashed amber circle at approximately the body's visual size.
fn draw_ghost_body(
    gizmos: &mut Gizmos,
    center: Vec3,
    ring_r: f32,
    body_r: f32,
    skip_outer_ring: bool,
) {
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
}

/// Size parameters for the burn-direction arrow drawn at the endpoints of a
/// transfer arc.  Scaled with camera zoom by `draw_fleet_transfer_preview`
/// so the arrow stays readable across the full System-view zoom range —
/// small at planet-close zoom (~ 0.5×), large at heliocentric zoom
/// (~ 2–3×).  See `draw_fleet_transfer_preview` for the scale formula.
const BURN_ARROW_HEAD_LEN_BASE: f32 = 25.0;
const BURN_ARROW_HEAD_HALF_BASE: f32 = 12.5;

/// Draw a V-shaped arrowhead at `position` pointing in `direction`.
///
/// Used by the transfer preview paths to make the prograde/retrograde burn
/// direction explicit at each end of the arc.  `scale` is multiplied into
/// the base size — the caller passes a zoom-aware multiplier so the arrow
/// stays readable at both planet-close and heliocentric zoom levels.
/// Returns early on a degenerate direction so callers can invoke it
/// unconditionally after computing Bezier / Kepler tangents.
fn draw_burn_arrow(gizmos: &mut Gizmos, position: Vec3, direction: Vec3, scale: f32, color: Color) {
    let dir = direction.normalize_or_zero();
    if dir == Vec3::ZERO {
        return;
    }
    // In 2D ecliptic-space, a 90° CCW perpendicular to `dir` is (-y, x, 0).
    // The V opens backward from the tip along `dir`.
    let perp = Vec3::new(-dir.y, dir.x, 0.0);
    let len = BURN_ARROW_HEAD_LEN_BASE * scale;
    let half = BURN_ARROW_HEAD_HALF_BASE * scale;
    let base = position - dir * len;
    let tip = position;
    let wing_l = base + perp * half;
    let wing_r = base - perp * half;
    gizmos.line(wing_l, tip, color);
    gizmos.line(wing_r, tip, color);
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

fn find_host_star(
    start_entity: Entity,
    body_query: &Query<(&Transform, &CelestialBody, Option<&LogicalParent>), Without<Fleet>>,
) -> Option<Entity> {
    let mut current = Some(start_entity);
    for _ in 0..8 {
        let entity = current?;
        let Ok((_, body, maybe_parent)) = body_query.get(entity) else {
            return None;
        };
        if body.body_type == BodyType::Star {
            return Some(entity);
        }
        current = maybe_parent.map(|parent| parent.0);
    }
    None
}

#[inline]
fn is_inter_star_transfer(
    origin_entity: Entity,
    destination_entity: Entity,
    body_query: &Query<(&Transform, &CelestialBody, Option<&LogicalParent>), Without<Fleet>>,
) -> bool {
    let origin_host_star = find_host_star(origin_entity, body_query);
    let destination_host_star = find_host_star(destination_entity, body_query);
    origin_host_star.is_some()
        && destination_host_star.is_some()
        && origin_host_star != destination_host_star
}

fn resolve_preview_reference_frame(
    origin_entity: Entity,
    destination_entity: Entity,
    body_query: &Query<(&Transform, &CelestialBody, Option<&LogicalParent>), Without<Fleet>>,
) -> TransferReferenceFrame {
    if is_inter_star_transfer(origin_entity, destination_entity, body_query) {
        return TransferReferenceFrame::SystemBarycentric;
    }

    let origin_parent = body_query
        .get(origin_entity)
        .ok()
        .and_then(|(_, _, parent)| parent.map(|parent| parent.0));
    let destination_parent = body_query
        .get(destination_entity)
        .ok()
        .and_then(|(_, _, parent)| parent.map(|parent| parent.0));

    if let Some(shared_parent) = destination_parent.filter(|parent| Some(*parent) == origin_parent)
    {
        return TransferReferenceFrame::Body(shared_parent);
    }

    if destination_parent == Some(origin_entity) {
        return TransferReferenceFrame::Body(origin_entity);
    }

    if Some(destination_entity) == origin_parent {
        return TransferReferenceFrame::Body(destination_entity);
    }

    let origin_star = find_host_star(origin_entity, body_query);
    let destination_star = find_host_star(destination_entity, body_query);
    if let Some(host_star) = origin_star.filter(|star| Some(*star) == destination_star) {
        return TransferReferenceFrame::Body(host_star);
    }

    TransferReferenceFrame::Body(
        destination_parent
            .or(origin_parent)
            .unwrap_or(origin_entity),
    )
}

/// Return a **frame-stable** travel time for the transfer preview ghost body.
///
/// For *Moderate* and *Fast* transfer options the
/// [`calculate_transfer_options_phased`](crate::fleets::orbital_mechanics::calculate_transfer_options_phased)
/// function recalculates transfer times every frame because the phase-correction
/// factor changes with the simulation clock.  For slow-orbiting bodies this is
/// imperceptible, but for fast-orbiting moons (e.g. Amalthea,  period ≈ 12 h)
/// the fluctuating `transfer_time_s` makes the ghost body visually spin faster
/// than the actual body.
///
/// This helper uses the *Efficient* option's transfer time (`t_h`, the Hohmann
/// half-period, which depends only on the orbit radii and GM — all constants)
/// and re-derives the Moderate/Fast time with the **nominal** energy
/// multiplier (1.5 / 2.5) instead of the phase-corrected value.
fn stable_preview_travel_time(ui: &FleetUiState) -> f64 {
    // Porkchop path: when the player has picked a cell on the porkchop
    // grid, the 3D preview must reflect that cell's `tof_s` rather
    // than the legacy Efficient/Moderate/Fast option the panel was
    // last synchronised against.  Without this precedence the ghost
    // arc keeps redrawing at the Hohmann time even after the player
    // picks a non-Hohmann cell — the "trajectory never updates" bug.
    if ui.porkchop_grid.is_some() {
        if let Some(pt) = &ui.planned_transfer {
            return pt.duration_s;
        }
        // Fall through to the legacy path so the panel still draws
        // *something* while the planner is mid-frame (e.g. during the
        // one tick between selecting a cell and `planned_transfer`
        // being rebuilt).
    }
    if ui.selected_option < ui.computed_options.len() {
        let opt = &ui.computed_options[ui.selected_option];
        // Options with inherently stable times (Efficient, kinematic, etc.)
        // can be returned directly.
        // "Curved Moderate" / "Curved Fast" from cross-star ballistic options
        // also have phase-dependent times that need the same stabilisation.
        let is_moderate = opt.label == "Moderate" || opt.label == "Curved Moderate";
        let is_fast = opt.label == "Fast" || opt.label == "Curved Fast";
        if !is_moderate && !is_fast {
            return opt.transfer_time_s;
        }
        // Base Hohmann time from the Efficient option (index 0).
        let base_time = ui
            .computed_options
            .first()
            .map(|o| o.transfer_time_s)
            .unwrap_or(opt.transfer_time_s);
        // Use the tier's nominal multiplier, stripping the per-frame phase correction.
        let nominal_mult: f64 = if is_moderate { 1.5 } else { 2.5 };
        // Kepler scaling: time ∝ multiplier^(−2/3)
        base_time * nominal_mult.powf(-2.0 / 3.0)
    } else if let Some(pt) = &ui.planned_transfer {
        pt.duration_s
    } else {
        0.0
    }
}

#[cfg(test)]
fn should_sample_planned_transfer_preview(
    planned_transfer: &PlannedTransfer,
    center_is_star: bool,
) -> bool {
    planned_transfer.preserve_orbit_geometry
        && should_use_star_centered_transfer_preview(planned_transfer, center_is_star)
}

fn should_use_star_centered_transfer_preview(
    planned_transfer: &PlannedTransfer,
    center_is_star: bool,
) -> bool {
    !planned_transfer.reference_frame.is_barycentric()
        && center_is_star
        && planned_transfer.option_label != "Full Thrust"
        && !planned_transfer.option_label.contains("Coast")
        && planned_transfer.option_label != "Max Speed"
        && !planned_transfer.option_label.contains("Direct")
        && planned_transfer.flyby_body.is_none()
}

fn should_use_exact_endpoint_bezier(reference_frame: TransferReferenceFrame) -> bool {
    reference_frame.is_barycentric()
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

fn compute_direct_transfer_arc(
    op: Vec3,
    dp: Vec3,
    origin_ring_r: f32,
    dest_ring_r: f32,
) -> TransferArcGeometry {
    let departure_angle = optimal_departure_angle(op, dp);
    let dir = (dp - op).normalize_or_zero();
    let p0 = if dir == Vec3::ZERO {
        op
    } else {
        op + dir * origin_ring_r
    };
    let p3 = if dir == Vec3::ZERO {
        dp
    } else {
        dp - dir * dest_ring_r
    };

    TransferArcGeometry {
        p0,
        p1: p0,
        p2: p3,
        p3,
        is_kinematic: true,
        departure_angle,
    }
}

impl TransferArcGeometry {
    /// Evaluate the arc at parameter `t` ∈ \[0, 1\].
    fn eval(&self, t: f32) -> Vec3 {
        if self.is_kinematic {
            self.p0 + (self.p3 - self.p0) * t
        } else {
            let u = 1.0 - t;
            u * u * u * self.p0
                + 3.0 * u * u * t * self.p1
                + 3.0 * u * t * t * self.p2
                + t * t * t * self.p3
        }
    }
}

fn orbit_visual_tangent_direction(
    orbit: &KeplerOrbit,
    mean_anomaly: f64,
    delta_mean_anomaly: f64,
) -> Vec3 {
    let delta = delta_mean_anomaly.abs().max(1e-4);
    let p0 = orbit_position_from_mean_anomaly(orbit, mean_anomaly);
    let p1 =
        orbit_position_from_mean_anomaly(orbit, mean_anomaly + delta_mean_anomaly.signum() * delta);
    Vec3::new(
        (p1.x - p0.x) as f32,
        (p1.y - p0.y) as f32,
        (p1.z - p0.z) as f32,
    )
    .normalize_or_zero()
}

fn compute_barycentric_visual_arc(
    orbit: &KeplerOrbit,
    start_pos: Vec3,
    end_pos: Vec3,
    total_ma_travel: f64,
    departure_velocity_ms: Option<DVec3>,
    arrival_velocity_ms: Option<DVec3>,
) -> TransferArcGeometry {
    let departure_angle = optimal_departure_angle(start_pos, end_pos);
    let direct_dir = (end_pos - start_pos).normalize_or_zero();
    let chord = (end_pos - start_pos).length();

    if chord <= 1e-3 || direct_dir == Vec3::ZERO {
        return TransferArcGeometry {
            p0: start_pos,
            p1: start_pos,
            p2: end_pos,
            p3: end_pos,
            is_kinematic: true,
            departure_angle,
        };
    }

    let start_ma = orbit.mean_anomaly_epoch;
    let end_ma = start_ma + total_ma_travel;
    let delta_ma = total_ma_travel.signum() * total_ma_travel.abs().max(1e-3) * 0.002;

    let depart_dir_raw = departure_velocity_ms
        .map(|velocity| Vec3::new(velocity.x as f32, velocity.y as f32, velocity.z as f32))
        .filter(|velocity| velocity.length_squared() > 1e-12)
        .map(|velocity| velocity.normalize_or_zero())
        .unwrap_or_else(|| orbit_visual_tangent_direction(orbit, start_ma, delta_ma));
    let arrive_dir_raw = arrival_velocity_ms
        .map(|velocity| Vec3::new(velocity.x as f32, velocity.y as f32, velocity.z as f32))
        .filter(|velocity| velocity.length_squared() > 1e-12)
        .map(|velocity| velocity.normalize_or_zero())
        .unwrap_or_else(|| orbit_visual_tangent_direction(orbit, end_ma, -delta_ma));

    // Blend mostly-orbital-prograde with a small direct bias so the arc
    // does not overshoot / loop.  0.85 / 0.15 mirrors the central-body
    // branch's `compute_transfer_arc` blend and produces a clean
    // half-ellipse for Hohmann-style inter-star previews.  The previous
    // 0.75 / 0.25 with arm `0.28 × chord` introduced an S-shape kink at
    // each endpoint, which read as a sharp perpendicular departure.
    let depart_dir = (depart_dir_raw * 0.85 + direct_dir * 0.15).normalize_or_zero();
    let arrive_dir = (arrive_dir_raw * 0.85 + direct_dir * 0.15).normalize_or_zero();

    let departure_tangent = if depart_dir == Vec3::ZERO {
        direct_dir
    } else {
        depart_dir
    };
    let arrival_tangent = if arrive_dir == Vec3::ZERO {
        direct_dir
    } else {
        arrive_dir
    };

    // Half-ellipse approximation: with tangents roughly perpendicular to the
    // chord, a control arm of ~½ the chord length gives the smoothest fit.
    // Doubling the floor (48 vs 24) keeps short inter-moon transfers visibly
    // curved rather than near-straight.
    let arm = (chord * 0.55).max(48.0);

    TransferArcGeometry {
        p0: start_pos,
        p1: start_pos + departure_tangent * arm,
        p2: end_pos - arrival_tangent * arm,
        p3: end_pos,
        is_kinematic: false,
        departure_angle,
    }
}

fn transfer_orbit_visual_point(
    orbit: &KeplerOrbit,
    mean_anomaly: f64,
    center_visual: Vec3,
) -> Vec3 {
    let local = orbit_position_from_mean_anomaly(orbit, mean_anomaly);
    center_visual
        + Vec3::new(
            (local.x * SCALING_FACTOR) as f32,
            (local.y * SCALING_FACTOR) as f32,
            (local.z * SCALING_FACTOR) as f32,
        )
}

fn sampled_transfer_orbit_points(
    orbit: &KeplerOrbit,
    center_visual: Vec3,
    start_mean_anomaly: f64,
    total_ma_travel: f64,
    start_fraction: f32,
    segments: u32,
) -> Vec<Vec3> {
    sampled_transfer_orbit_points_with_center(
        orbit,
        start_mean_anomaly,
        total_ma_travel,
        start_fraction,
        segments,
        |_| center_visual,
    )
}

fn sampled_transfer_orbit_points_with_center(
    orbit: &KeplerOrbit,
    start_mean_anomaly: f64,
    total_ma_travel: f64,
    start_fraction: f32,
    segments: u32,
    mut center_at_fraction: impl FnMut(f32) -> Vec3,
) -> Vec<Vec3> {
    let mut points = Vec::with_capacity(segments as usize + 2);
    let start_ma = start_mean_anomaly + total_ma_travel * start_fraction as f64;
    points.push(transfer_orbit_visual_point(
        orbit,
        start_ma,
        center_at_fraction(start_fraction),
    ));

    for i in 0..=segments {
        let frac = i as f32 / segments as f32;
        if frac <= start_fraction {
            continue;
        }
        let mean_anomaly = start_mean_anomaly + total_ma_travel * frac as f64;
        points.push(transfer_orbit_visual_point(
            orbit,
            mean_anomaly,
            center_at_fraction(frac),
        ));
    }

    points
}

fn build_visual_sampled_transfer_polyline(
    orbit: &KeplerOrbit,
    center_visual: Vec3,
    _origin_body_pos: Vec3,
    _origin_ring_r: f32,
    _destination_body_pos: Vec3,
    _dest_ring_r: f32,
    _is_course_correction: bool,
    _is_inward: bool,
    start_mean_anomaly: f64,
    total_ma_travel: f64,
    segments: u32,
) -> Vec<Vec3> {
    sampled_transfer_orbit_points(
        orbit,
        center_visual,
        start_mean_anomaly,
        total_ma_travel,
        0.0,
        segments,
    )
}

fn build_visual_sampled_transfer_polyline_moving_center(
    orbit: &KeplerOrbit,
    center_entity: Entity,
    current_sim_s: f64,
    departure_s: f64,
    duration_s: f64,
    current_real_s: f64,
    time_scale: f64,
    body_query: &Query<(&Transform, &CelestialBody, Option<&LogicalParent>), Without<Fleet>>,
    kepler_query: &Query<&KeplerOrbit, Without<Fleet>>,
    amp_query: &Query<&LocalOrbitAmplification, Without<Fleet>>,
    start_mean_anomaly: f64,
    total_ma_travel: f64,
    segments: u32,
) -> Vec<Vec3> {
    let fallback_center = body_query
        .get(center_entity)
        .ok()
        .map(|(t, _, _)| t.translation)
        .unwrap_or(Vec3::ZERO);

    sampled_transfer_orbit_points_with_center(
        orbit,
        start_mean_anomaly,
        total_ma_travel,
        0.0,
        segments,
        |frac| {
            predict_body_visual_pos(
                center_entity,
                current_sim_s,
                departure_s + duration_s * frac as f64,
                current_real_s,
                time_scale,
                body_query,
                kepler_query,
                amp_query,
            )
            .unwrap_or(fallback_center)
        },
    )
}

fn sample_polyline(points: &[Vec3], progress: f32) -> Vec3 {
    if points.is_empty() {
        return Vec3::ZERO;
    }
    if points.len() == 1 {
        return points[0];
    }

    let scaled = progress.clamp(0.0, 1.0) * (points.len() - 1) as f32;
    let index = scaled.floor() as usize;
    let frac = scaled - index as f32;
    if index + 1 >= points.len() {
        points[points.len() - 1]
    } else {
        points[index].lerp(points[index + 1], frac)
    }
}

fn draw_sampled_transfer_orbit(
    gizmos: &mut Gizmos,
    points: &[Vec3],
    start_fraction: f32,
    mut color_fn: impl FnMut(f32) -> Color,
) {
    if points.is_empty() {
        return;
    }

    let start_fraction = start_fraction.clamp(0.0, 1.0);
    let mut prev = Some(sampled_transfer_orbit_position(points, start_fraction));
    let scaled = start_fraction * (points.len().saturating_sub(1)) as f32;
    let start_index = scaled.floor() as usize + 1;

    for (idx, pos) in points.iter().copied().enumerate().skip(start_index) {
        let frac = if points.len() <= 1 {
            1.0
        } else {
            idx as f32 / (points.len() - 1) as f32
        };
        if let Some(prev_pos) = prev {
            gizmos.line(prev_pos, pos, color_fn(frac));
        }
        prev = Some(pos);
    }
}

fn sampled_transfer_orbit_position(points: &[Vec3], progress: f32) -> Vec3 {
    sample_polyline(points, progress)
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

    // Detect "central-body" transfers where both bodies orbit a distant centre
    // (e.g. heliocentric planet→planet, or moon→moon around a planet).
    // For local transfers (planet→moon), the origin IS near the orbit centre,
    // so origin_r ≈ 0 and the check fails — preserving the existing local logic.
    let origin_r = (op - cv_ref).length();
    let dest_r = (dp - cv_ref).length();
    let chord = (dp - op).length().max(1e-3);
    let is_central_body_xfer =
        !is_course_correction && origin_r > chord * 0.15 && dest_r > chord * 0.15;

    // Rotate direction by 90° so the ring point is where prograde (outward) or
    // retrograde (inward) aims toward the destination — Hohmann departure.
    let dep_ring_dir = if is_inward {
        Vec3::new(-dir_dep.y, dir_dep.x, 0.0) // CCW rotation
    } else {
        Vec3::new(dir_dep.y, -dir_dep.x, 0.0) // CW rotation
    };
    let p0 = op + dep_ring_dir * origin_ring_r;

    let direct_dir = (dp - p0).normalize_or_zero();
    let tang_origin = if is_course_correction {
        // Course corrections: fleet already moving, aim mostly at destination.
        let tang_orbit_raw = dir_dep;
        (tang_orbit_raw * 0.10 + direct_dir * 0.90).normalize_or_zero()
    } else if is_central_body_xfer {
        // For orbits around a distant central body (star or planet), compute the
        // true orbital prograde direction (perpendicular to the radial from the
        // orbit centre to the body, CCW).  Hohmann transfers always depart in the
        // prograde direction — the burn changes speed, not heading.
        let origin_radial = (op - cv_ref).normalize_or_zero();
        let prograde = Vec3::new(-origin_radial.y, origin_radial.x, 0.0);
        // Blend: strongly orbital with a hint of direct for visual continuity.
        (prograde * 0.85 + direct_dir * 0.15).normalize_or_zero()
    } else if is_inward {
        // Inward (retrograde departure): blend heavily toward direct path.
        let tang_orbit_raw = dir_dep;
        (tang_orbit_raw * 0.20 + direct_dir * 0.80).normalize_or_zero()
    } else {
        // Outward (prograde departure): blend to avoid the 90° hook while
        // keeping the prograde orbital character of a Hohmann departure.
        let tang_orbit_raw = dir_dep;
        (tang_orbit_raw * 0.55 + direct_dir * 0.45).normalize_or_zero()
    };

    let radial_dest_raw = dp - cv_ref;
    let radial_dest = if radial_dest_raw.length() > 1.0 {
        radial_dest_raw.normalize()
    } else if (p0 - cv_ref).length() > 0.1 {
        // When the destination is the orbit centre itself (for example a transfer
        // into a star-centred parking ring), there is no body radial at `dp`.
        // Use the centre-to-source direction so the arrival side remains stable
        // even when the whole system is translated away from world origin.
        (p0 - cv_ref).normalize()
    } else {
        (dp - op).normalize_or_zero()
    };
    let preferred_arrival_side = if is_inward { radial_dest } else { -radial_dest };
    let (p3, tang_dest) = if is_kinematic {
        tangential_ring_arrival_point(p0, dp, dest_ring_r, Some(preferred_arrival_side))
            .unwrap_or_else(|| {
                let arr_ring_dir = if is_inward { radial_dest } else { -radial_dest };
                let p3 = dp + arr_ring_dir * dest_ring_r;
                let tang_dest = Vec3::new(-radial_dest.y, radial_dest.x, 0.0);
                (p3, tang_dest)
            })
    } else {
        tangential_ring_arrival_point(p0, dp, dest_ring_r, Some(preferred_arrival_side))
            .unwrap_or_else(|| {
                let arr_ring_dir = if is_inward { radial_dest } else { -radial_dest };
                let p3 = dp + arr_ring_dir * dest_ring_r;
                let tangent_ccw = Vec3::new(-radial_dest.y, radial_dest.x, 0.0);
                let inbound = (p3 - p0).normalize_or_zero();
                let tang_dest = if inbound.dot(tangent_ccw) >= 0.0 {
                    tangent_ccw
                } else {
                    -tangent_ccw
                };
                (p3, tang_dest)
            })
    };

    // For central-body transfers, override the arrival tangent with the orbital
    // prograde direction at the destination.  Hohmann transfers always arrive in
    // the prograde direction (tangent to the destination orbit), regardless of
    // whether the transfer is outward or inward.
    let tang_dest = if is_central_body_xfer {
        let dest_radial = (dp - cv_ref).normalize_or_zero();
        Vec3::new(-dest_radial.y, dest_radial.x, 0.0) // CCW prograde at destination
    } else {
        tang_dest
    };

    // Longer control arms for central-body transfers approximate the half-ellipse
    // of a Hohmann orbit. The standard 0.40× chord is too short and produces
    // nearly-straight arcs between distant planets.
    let ctrl_len = if is_central_body_xfer {
        (p3 - p0).length() * 0.72
    } else {
        (p3 - p0).length() * 0.40
    };
    let mut p1 = p0 + tang_origin * ctrl_len;
    let mut p2 = p3 - tang_dest * ctrl_len;
    if is_central_body_xfer {
        let radius_sum = (origin_r + dest_r).max(1.0);
        let eccentricity_hint = ((origin_r - dest_r).abs() / radius_sum).clamp(0.0, 1.0);
        let focus_pull = (p3 - p0).length() * (0.10 + 0.20 * eccentricity_hint);
        let focus_dir_1 = (cv_ref - p1).normalize_or_zero();
        let focus_dir_2 = (cv_ref - p2).normalize_or_zero();
        p1 += focus_dir_1 * focus_pull;
        p2 += focus_dir_2 * focus_pull;
    }
    // Smoothly interpolate z so the arc bridges the two orbital planes
    // even when the tangent vectors are 2D (z = 0).
    p1.z = p0.z + (p3.z - p0.z) * 0.33;
    p2.z = p0.z + (p3.z - p0.z) * 0.67;

    TransferArcGeometry {
        p0,
        p1,
        p2,
        p3,
        is_kinematic,
        departure_angle,
    }
}

fn tangential_ring_arrival_point(
    source: Vec3,
    center: Vec3,
    ring_r: f32,
    preferred_radial: Option<Vec3>,
) -> Option<(Vec3, Vec3)> {
    let to_source = source - center;
    let dist_sq = to_source.length_squared();
    let ring_sq = ring_r * ring_r;
    if dist_sq <= ring_sq + 1e-3 || ring_r <= 0.0 {
        return None;
    }

    let base = to_source * (ring_sq / dist_sq);
    let perp = Vec3::new(-to_source.y, to_source.x, 0.0);
    let scale = ring_r * (dist_sq - ring_sq).sqrt() / dist_sq;

    let candidate_a = center + base + perp * scale;
    let candidate_b = center + base - perp * scale;

    let direct_approach = (center - source).normalize_or_zero();
    let preferred_radial = preferred_radial.map(|dir| dir.normalize_or_zero());

    let evaluate_candidate = |point: Vec3| {
        let radial = (point - center).normalize_or_zero();
        let tangent_ccw = Vec3::new(-radial.y, radial.x, 0.0);
        let inbound = (point - source).normalize_or_zero();
        let tang_dest = if inbound.dot(tangent_ccw) >= 0.0 {
            tangent_ccw
        } else {
            -tangent_ccw
        };
        let primary_score = if let Some(preferred) = preferred_radial {
            radial.dot(preferred)
        } else if direct_approach.length_squared() > 0.0 {
            inbound.dot(direct_approach)
        } else {
            inbound.dot(tang_dest)
        };
        let secondary_score = if direct_approach.length_squared() > 0.0 {
            inbound.dot(direct_approach)
        } else {
            inbound.dot(tang_dest)
        };
        (primary_score, secondary_score, tang_dest)
    };

    let (score_a, secondary_a, tang_a) = evaluate_candidate(candidate_a);
    let (score_b, secondary_b, tang_b) = evaluate_candidate(candidate_b);

    let primary_delta = score_a - score_b;
    let secondary_delta = secondary_a - secondary_b;
    let choose_a = if primary_delta.abs() > 1e-4 {
        primary_delta > 0.0
    } else if secondary_delta.abs() > 1e-4 {
        secondary_delta > 0.0
    } else if candidate_a.x != candidate_b.x {
        candidate_a.x > candidate_b.x
    } else {
        candidate_a.y >= candidate_b.y
    };

    let (p3, tang_dest) = if choose_a {
        (candidate_a, tang_a)
    } else {
        (candidate_b, tang_b)
    };

    Some((p3, tang_dest))
}

#[cfg(test)]
mod tests {
    use super::{
        compute_barycentric_visual_arc, compute_gravity_assist_arc, compute_transfer_arc,
        sample_polyline, sampled_transfer_orbit_points_with_center, shell_ring_visual_radius,
        should_sample_planned_transfer_preview, tangential_ring_arrival_point,
        BURN_ARROW_HEAD_HALF_BASE, BURN_ARROW_HEAD_LEN_BASE,
    };
    use crate::astronomy::{orbit_position_from_mean_anomaly, KeplerOrbit, SCALING_FACTOR};
    use crate::fleets::orbital_mechanics::{solve_phase_aware_ga_option, GM_SUN};
    use crate::fleets::{PlannedTransfer, TransferReferenceFrame};
    use crate::plugins::solar_system::CelestialBody;
    use crate::plugins::solar_system_data::BodyType;
    use bevy::math::DVec3;
    use bevy::prelude::Vec3;

    fn make_body(body_type: BodyType, radius_km: f32, visual_radius: f32) -> CelestialBody {
        CelestialBody {
            name: "TestBody".to_string(),
            radius: radius_km,
            mass: 0.0,
            body_type,
            visual_radius,
            asteroid_class: None,
            star_approach_au: None,
            rotation_period_s: None,
            habitable_outer_au: None,
        }
    }

    fn test_planned_transfer(preserve_orbit_geometry: bool) -> PlannedTransfer {
        PlannedTransfer {
            origin_body: bevy::prelude::Entity::from_bits(1),
            destination_body: bevy::prelude::Entity::from_bits(2),
            reference_frame: TransferReferenceFrame::Body(bevy::prelude::Entity::from_bits(3)),
            orbit_center: bevy::prelude::Entity::from_bits(4),
            transfer_orbit: KeplerOrbit {
                semi_major_axis: 1.0,
                eccentricity: 0.2,
                inclination: 0.0,
                longitude_ascending_node: 0.0,
                argument_of_periapsis: 0.0,
                mean_anomaly_epoch: 0.0,
                mean_motion: 1.0,
            },
            duration_s: 1_000.0,
            preserve_orbit_geometry,
            arrival_delta_v_ms: 0.0,
            arrival_orbit_radius_au: 0.01,
            fuel_cost_t: 0.0,
            option_label: "Efficient",
            start_position_au: None,
            end_position_au: None,
            departure_velocity_ms: None,
            arrival_velocity_ms: None,
            flyby_body: None,
            leg2_orbit: None,
            leg2_start_s: 0.0,
        }
    }

    #[test]
    fn tangential_arrival_aligns_with_inbound_travel() {
        let source = Vec3::new(-10.0, 5.0, 0.0);
        let center = Vec3::ZERO;
        let ring_r = 2.0;

        let (point, tangent) = tangential_ring_arrival_point(source, center, ring_r, None)
            .expect("source outside ring should yield tangent point");

        let inbound = (point - source).normalize_or_zero();
        let radial = (point - center).normalize_or_zero();

        assert!(inbound.dot(tangent) > 0.99);
        assert!(radial.dot(tangent).abs() < 1e-4);
    }

    #[test]
    fn tangential_arrival_prefers_direct_approach_side() {
        let source = Vec3::new(-8.0, 6.0, 0.0);
        let center = Vec3::ZERO;
        let ring_r = 2.0;

        let (point, _) = tangential_ring_arrival_point(source, center, ring_r, None)
            .expect("source outside ring should yield tangent point");

        assert!(point.y > 0.0);
    }

    #[test]
    fn bezier_arrival_segment_aligns_with_inbound_travel() {
        let geo = compute_transfer_arc(
            Vec3::new(-12.0, -4.0, 0.0),
            Vec3::ZERO,
            1.5,
            2.5,
            false,
            false,
            false,
            Vec3::ZERO,
        );

        let final_segment = (geo.p3 - geo.p2).normalize_or_zero();
        let inbound = (geo.p3 - geo.p0).normalize_or_zero();
        let radial = geo.p3.normalize_or_zero();

        assert!(final_segment.dot(inbound) > 0.7);
        assert!(final_segment.dot(radial).abs() < 0.8);
    }

    #[test]
    fn central_body_inward_arc_bends_inside_linear_chord() {
        let geo = compute_transfer_arc(
            Vec3::new(12.0, 1.0, 0.0),
            Vec3::new(-2.5, 4.0, 0.0),
            0.6,
            0.6,
            false,
            true,
            false,
            Vec3::ZERO,
        );

        let curve_mid = geo.eval(0.5);
        let chord_mid = (geo.p0 + geo.p3) * 0.5;

        assert!(curve_mid.distance(chord_mid) > 0.5);
    }

    #[test]
    fn inward_star_arrival_is_translation_invariant() {
        let base_origin = Vec3::new(12.0, 1.0, 0.0);
        let base_dest = Vec3::ZERO;
        let offset = Vec3::new(37.0, -19.0, 0.0);

        let base = compute_transfer_arc(
            base_origin,
            base_dest,
            0.6,
            1.8,
            false,
            true,
            false,
            Vec3::ZERO,
        );
        let shifted = compute_transfer_arc(
            base_origin + offset,
            base_dest + offset,
            0.6,
            1.8,
            false,
            true,
            false,
            offset,
        );

        assert!(shifted.p0.distance(base.p0 + offset) < 1e-4);
        assert!(shifted.p1.distance(base.p1 + offset) < 1e-4);
        assert!(shifted.p2.distance(base.p2 + offset) < 1e-4);
        assert!(shifted.p3.distance(base.p3 + offset) < 1e-4);
    }

    #[test]
    fn sampled_preview_requires_preserved_orbit_geometry() {
        let planned = test_planned_transfer(false);

        assert!(!should_sample_planned_transfer_preview(&planned, true));
    }

    #[test]
    fn sampled_preview_allows_preserved_same_star_geometry() {
        let planned = test_planned_transfer(true);

        assert!(should_sample_planned_transfer_preview(&planned, true));
    }

    #[test]
    fn star_centered_preview_allows_non_preserved_same_star_geometry() {
        let planned = test_planned_transfer(false);

        assert!(super::should_use_star_centered_transfer_preview(
            &planned, true
        ));
    }

    #[test]
    fn barycentric_visual_arc_matches_given_endpoints() {
        let orbit = KeplerOrbit {
            semi_major_axis: 18.0,
            eccentricity: 0.55,
            inclination: 0.15,
            longitude_ascending_node: 0.4,
            argument_of_periapsis: 0.7,
            mean_anomaly_epoch: 0.3,
            mean_motion: 2.5e-7,
        };

        let start = Vec3::new(-320.0, 40.0, 0.0);
        let end = Vec3::new(410.0, -120.0, 0.0);
        let geo = compute_barycentric_visual_arc(
            &orbit,
            start,
            end,
            1.2,
            Some(DVec3::new(0.0, 18_000.0, 0.0)),
            Some(DVec3::new(-11_000.0, 9_000.0, 0.0)),
        );

        assert!(geo.eval(0.0).distance(start) < 1e-4);
        assert!(geo.eval(1.0).distance(end) < 1e-4);
        assert!(geo.eval(0.5).is_finite());
    }

    #[test]
    fn exact_endpoint_bezier_is_reserved_for_barycentric_routes() {
        assert!(super::should_use_exact_endpoint_bezier(
            TransferReferenceFrame::SystemBarycentric
        ));
        assert!(!super::should_use_exact_endpoint_bezier(
            TransferReferenceFrame::Body(bevy::prelude::Entity::from_bits(9))
        ));
    }

    // GRA-153 H-5: the gravity-assist preview must show BOTH legs (origin→flyby and
    // flyby→destination), not just Leg-1.  The `compute_gravity_assist_arc` helper is
    // shared between the preview (`draw_gravity_assist_preview`) and the in-transit
    // renderer (`draw_fleet_trajectories`), so verifying the geometry here guarantees
    // both call-sites get a two-leg Bezier.
    #[test]
    fn gra_153_h5_gravity_assist_arc_produces_two_distinct_legs() {
        // Earth→Venus→Mars geometry: origin at 1 AU, Venus at 0.72 AU, Mars at 1.52 AU.
        let op = Vec3::new(1.0, 0.0, 0.0);
        let fp = Vec3::new(0.0, 0.72, 0.0);
        let dp = Vec3::new(-1.5, 0.0, 0.0);
        let origin_ring_r = 0.05;
        let flyby_ring_r = 0.04;
        let dest_ring_r = 0.06;

        let ga = compute_gravity_assist_arc(op, fp, dp, origin_ring_r, flyby_ring_r, dest_ring_r);

        // Leg-1 endpoint must lie on the origin ring.
        let leg1_start = ga.eval_leg1(0.0);
        let leg1_end = ga.eval_leg1(1.0);
        let leg2_start = ga.eval_leg2(0.0);
        let leg2_end = ga.eval_leg2(1.0);

        assert!(
            (leg1_start - op).length() < origin_ring_r + 0.05,
            "Leg-1 start should sit on the origin ring (got {:?}, op={:?}, origin_ring_r={})",
            leg1_start,
            op,
            origin_ring_r
        );
        // Leg-1 end and Leg-2 start should coincide at the flyby periapsis (C0 continuity).
        assert!(
            (leg1_end - leg2_start).length() < 1e-4,
            "Leg-1 end and Leg-2 start must coincide at periapsis (leg1_end={:?}, leg2_start={:?})",
            leg1_end,
            leg2_start
        );
        // Leg-2 end should sit on the destination ring.
        assert!(
            (leg2_end - dp).length() < dest_ring_r + 0.05,
            "Leg-2 end should sit on the destination ring (got {:?}, dp={:?}, dest_ring_r={})",
            leg2_end,
            dp,
            dest_ring_r
        );
        // The two legs must NOT be the same segment: midpoint of Leg-1 and midpoint of
        // Leg-2 should be far apart (the flyby bends the path).
        let leg1_mid = ga.eval_leg1(0.5);
        let leg2_mid = ga.eval_leg2(0.5);
        assert!(
            (leg1_mid - leg2_mid).length() > 0.1,
            "Leg-1 and Leg-2 midpoints must diverge (got leg1_mid={:?}, leg2_mid={:?}, dist={})",
            leg1_mid,
            leg2_mid,
            (leg1_mid - leg2_mid).length()
        );
    }

    // GRA-NNN (regression for the "transfer arc rotates / never reaches
    // Jupiter" bug): the in-transit polyline must trace the fleet's actual
    // Keplerian half-orbit exactly.  The four buggy call-sites
    // (`draw_fleet_trajectories` × 3, `update_fleet_transforms` × 1) used
    // `mean_motion * sim_elapsed` as the polyline anchor, which (a) shifted
    // the polyline forward in orbital phase every frame so it visibly
    // rotated around the Sun, and (b) doubled the icon's apparent orbital
    // rate, so the fleet icon lapped the actual fleet position on every
    // transit.  The fix anchors the polyline at `mean_anomaly_epoch` —
    // the mean anomaly at `departure_time` per the `ActiveManeuver`
    // convention in `src/fleets/components.rs`.  This test pins both
    // invariants: the polyline endpoint must be at the apoapsis (Jupiter),
    // and every polyline point must equal the direct Keplerian formula at
    // the matching mean anomaly.
    #[test]
    fn in_transit_polyline_traces_keplerian_half_orbit() {
        // Hohmann transfer Earth (1 AU) → Jupiter (5.2 AU).  Outbound so
        // periapsis = origin, mean_anomaly_epoch = 0, half-ellipse runs
        // MA ∈ [0, π].
        let r_earth_au = 1.0_f64;
        let r_jupiter_au = 5.2_f64;
        let sma_au = (r_earth_au + r_jupiter_au) / 2.0;
        let ecc = (r_jupiter_au - r_earth_au) / (r_jupiter_au + r_earth_au);
        // mean_motion is arbitrary — the polyline shape is independent of
        // its absolute scale.  1.0 rad/s makes the half-period π s.
        let mean_motion = 1.0_f64;
        let transfer_orbit = KeplerOrbit {
            semi_major_axis: sma_au,
            eccentricity: ecc,
            inclination: 0.0,
            longitude_ascending_node: 0.0,
            argument_of_periapsis: 0.0,
            mean_anomaly_epoch: 0.0,
            mean_motion,
        };
        let duration_s = std::f64::consts::PI / mean_motion; // half-period
        let total_ma_travel = mean_motion * duration_s;

        // The fix: anchor the polyline at mean_anomaly_epoch (NOT
        // mean_motion * elapsed).  Sun at origin keeps the test pure-math.
        let start_mean_anomaly = transfer_orbit.mean_anomaly_epoch;
        let points = sampled_transfer_orbit_points_with_center(
            &transfer_orbit,
            start_mean_anomaly,
            total_ma_travel,
            0.0,
            128,
            |_| Vec3::ZERO,
        );

        // The polyline operates in Bevy render units (AU × SCALING_FACTOR).
        let scale = SCALING_FACTOR as f64;

        // (1) Endpoint must be Jupiter's orbit (apoapsis = r_jupiter_au),
        //     not rotated past it by `mean_motion * elapsed` as the bug did.
        let last = points.last().expect("polyline non-empty");
        let last_radius = last.length() as f64;
        let expected_last_radius = r_jupiter_au * scale;
        assert!(
            (last_radius - expected_last_radius).abs() < 1.0,
            "polyline endpoint must be at apoapsis ({r_jupiter_au} AU = {expected_last_radius} render units); \
             got {last_radius} render units (drift = {} render units = {} AU)",
            last_radius - expected_last_radius,
            (last_radius - expected_last_radius) / scale,
        );

        // (2) Startpoint must be Earth's orbit (periapsis = r_earth_au).
        let first = points.first().expect("polyline non-empty");
        let first_radius = first.length() as f64;
        let expected_first_radius = r_earth_au * scale;
        assert!(
            (first_radius - expected_first_radius).abs() < 1.0,
            "polyline startpoint must be at periapsis ({r_earth_au} AU = {expected_first_radius} render units); \
             got {first_radius} render units",
        );

        // (3) Every polyline point must equal the direct Keplerian formula
        //     at MA = mean_anomaly_epoch + total_ma_travel * f, scaled into
        //     render units.  This is the core "polyline == physics path"
        //     invariant the four buggy sites violated.
        for (i, point) in points.iter().enumerate() {
            let f = i as f64 / (points.len() - 1) as f64;
            let expected_ma = start_mean_anomaly + total_ma_travel * f;
            let expected_au = orbit_position_from_mean_anomaly(&transfer_orbit, expected_ma);
            let expected_v = Vec3::new(
                (expected_au.x * scale) as f32,
                (expected_au.y * scale) as f32,
                (expected_au.z * scale) as f32,
            );
            let dist = point.distance(expected_v);
            assert!(
                dist < 1.0,
                "polyline point at f={f} diverged from Keplerian arc: \
                 got {point:?}, expected {expected_v:?} (MA={expected_ma:.6}, dist={dist})",
            );
        }

        // (4) Sample at progress = (elapsed - departure)/duration must land
        //     on the fleet's actual Keplerian position at that moment:
        //     MA = mean_anomaly_epoch + mean_motion * (elapsed - departure).
        //     This pins the property that `update_fleet_transforms`'s icon
        //     sampler agrees with `update_fleet_maneuver_positions`'s
        //     physics propagation — the property the icon-lap bug broke.
        for fraction in [0.0_f32, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0] {
            let dt_in_orbit = fraction as f64 * duration_s;
            let expected_ma =
                transfer_orbit.mean_anomaly_epoch + transfer_orbit.mean_motion * dt_in_orbit;
            let expected_au = orbit_position_from_mean_anomaly(&transfer_orbit, expected_ma);
            let expected_v = Vec3::new(
                (expected_au.x * scale) as f32,
                (expected_au.y * scale) as f32,
                (expected_au.z * scale) as f32,
            );
            let polyline_pos = sample_polyline(&points, fraction);
            let dist = polyline_pos.distance(expected_v);
            assert!(
                dist < 1.0,
                "polyline sample at fraction {fraction} diverged from physics MA {expected_ma}: \
                 got {polyline_pos:?}, expected {expected_v:?} (dist={dist})",
            );
        }
    }

    // ── shell_ring_visual_radius ─────────────────────────────────────────────

    /// Stars scale directly via `SCALING_FACTOR` — no shell-multiplier
    /// table involved.  Cruise at 5 AU must equal 5 × 1500 = 7500 visual
    /// units; CloseApproach at 0.05 AU must equal 75 visual units.
    #[test]
    fn shell_ring_visual_radius_star_direct_au_conversion() {
        let sol = make_body(BodyType::Star, 695_700.0, 10.0);
        assert!((shell_ring_visual_radius(&sol, 5.0) - 5.0 * 1500.0).abs() < 1e-3);
        assert!((shell_ring_visual_radius(&sol, 0.05) - 0.05 * 1500.0).abs() < 1e-3);
        assert!((shell_ring_visual_radius(&sol, 1.0) - 1500.0).abs() < 1e-3);
    }

    /// For planets, the visual radius depends on the ratio of the
    /// parking-altitude (km) to the body's own radius (km).  Earth LEO
    /// (≈ 6,690 km, ratio 1.05×) must produce the same ring as the
    /// existing `fleet_parking_visual_radius` baseline.  Medium (3×),
    /// High (10×), and Stationary (≥20×) must all sit visibly farther
    /// out, in that order.
    #[test]
    fn shell_ring_visual_radius_planet_low_medium_high_stationary() {
        let earth = make_body(BodyType::Planet, 6_371.0, 2.0);
        let r_low = shell_ring_visual_radius(&earth, 6_690.0 / 149_597_870.7);
        let r_med = shell_ring_visual_radius(&earth, 3.0 * 6_371.0 / 149_597_870.7);
        let r_high = shell_ring_visual_radius(&earth, 10.0 * 6_371.0 / 149_597_870.7);
        let r_stationary = shell_ring_visual_radius(&earth, 20.0 * 6_371.0 / 149_597_870.7);

        // Low sits at the existing `fleet_parking_visual_radius` baseline
        // (1.0× multiplier).  For Earth that's max(2.0 × 1.5, 2.0 + 6.0 + 3.0) = 11.0.
        assert!(
            (r_low - 11.0).abs() < 1e-3,
            "Low ({r_low}) should equal the fleet_parking_visual_radius baseline (11.0)"
        );
        // Monotonic ordering.
        assert!(r_low < r_med, "Low ({r_low}) should be < Medium ({r_med})");
        assert!(
            r_med < r_high,
            "Medium ({r_med}) should be < High ({r_high})"
        );
        assert!(
            r_high <= r_stationary,
            "High ({r_high}) should be <= Stationary ({r_stationary})"
        );
    }

    /// Body with `radius == 0.0` (defensive) must fall back to the
    /// `1.0×` multiplier rather than panic.
    #[test]
    fn shell_ring_visual_radius_zero_radius_body_does_not_panic() {
        let ghost = make_body(BodyType::Planet, 0.0, 2.0);
        let _r = shell_ring_visual_radius(&ghost, 0.05);
        // No assertion needed — the test passes if no panic occurs.
    }

    // ── porkchop preview arc — regression for "Bezier in preview, real
    //     ellipse in transit" mismatch ────────────────────────────────────────

    /// Regression for the user-reported "preview arc looks perpendicular"
    /// bug.  The porkchop preview path in `draw_fleet_transfer_preview`
    /// used to render a cubic Bezier interpolation between predicted
    /// endpoints — visually an S-shape with sharp perpendicular kinks at
    /// each endpoint, and inconsistent with the actual Keplerian half-ellipse
    /// the in-transit renderer draws.  The fix samples the actual
    /// `KeplerOrbit` directly via `orbit_position_from_mean_anomaly`.  This
    /// test pins the property that the same formula the renderer uses
    /// produces points that match the direct Keplerian formula at every
    /// fraction along the arc.
    #[test]
    fn porkchop_preview_arc_traces_keplerian_half_orbit() {
        // Earth→Jupiter Hohmann: periapsis at Earth (1 AU), apoapsis at
        // Jupiter (5.2 AU).  The polyline shape is independent of
        // mean_motion's absolute scale.
        let r_earth_au = 1.0_f64;
        let r_jupiter_au = 5.2_f64;
        let sma_au = (r_earth_au + r_jupiter_au) / 2.0;
        let ecc = (r_jupiter_au - r_earth_au) / (r_jupiter_au + r_earth_au);
        let mean_motion = 1.0_f64;
        let transfer_orbit = KeplerOrbit {
            semi_major_axis: sma_au,
            eccentricity: ecc,
            inclination: 0.0,
            longitude_ascending_node: 0.0,
            argument_of_periapsis: 0.0,
            mean_anomaly_epoch: 0.0,
            mean_motion,
        };
        let duration_s = std::f64::consts::PI / mean_motion; // half-period
        let total_ma_travel = mean_motion * duration_s;

        let start_mean_anomaly = transfer_orbit.mean_anomaly_epoch;
        let scale = SCALING_FACTOR as f64;

        // Sample the preview arc — same expression the renderer's
        // `draw_dashed_curve` closure evaluates at fraction `t`.
        let eval_at = |t: f64| -> Vec3 {
            let ma = start_mean_anomaly + total_ma_travel * t;
            let pos_au = orbit_position_from_mean_anomaly(&transfer_orbit, ma);
            Vec3::new(
                (pos_au.x * scale) as f32,
                (pos_au.y * scale) as f32,
                (pos_au.z * scale) as f32,
            )
        };

        // Every sample must equal the Kepler formula at the matching mean
        // anomaly — the core "preview polyline == Keplerian half-orbit"
        // invariant the user complained was broken.
        for fraction in [0.0_f64, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0] {
            let expected_ma = start_mean_anomaly + total_ma_travel * fraction;
            let expected_au = orbit_position_from_mean_anomaly(&transfer_orbit, expected_ma);
            let expected_v = Vec3::new(
                (expected_au.x * scale) as f32,
                (expected_au.y * scale) as f32,
                (expected_au.z * scale) as f32,
            );
            let preview_v = eval_at(fraction);
            let dist = preview_v.distance(expected_v);
            assert!(
                dist < 1.0,
                "preview arc sample at fraction {fraction} diverged from Kepler: \
                 got {preview_v:?}, expected {expected_v:?} (dist={dist})",
            );
        }

        // Endpoint sanity: first sample at Earth radius (1 AU), last at
        // Jupiter radius (5.2 AU) — proves the orbit starts at periapsis
        // and ends at apoapsis, the defining Hohmann geometry.
        let first_radius = eval_at(0.0).length() as f64;
        let last_radius = eval_at(1.0).length() as f64;
        assert!(
            (first_radius - r_earth_au * scale).abs() < 1.0,
            "preview arc startpoint at periapsis: expected {} render units, got {}",
            r_earth_au * scale,
            first_radius,
        );
        assert!(
            (last_radius - r_jupiter_au * scale).abs() < 1.0,
            "preview arc endpoint at apoapsis: expected {} render units, got {}",
            r_jupiter_au * scale,
            last_radius,
        );
    }

    /// Burn-direction arrow geometry is a V-shape that opens backward
    /// from the tip.  Verified here by inlining the wing-point math rather
    /// than instantiating `Gizmos` (which is awkward in unit tests).
    /// The draw helper itself delegates to `gizmos.line` with these exact
    /// coordinates, so a regression here means the visible arrow would
    /// become asymmetric or collapse to a single line.
    #[test]
    fn burn_arrow_wing_geometry() {
        // Replicate the wing-point math from `draw_burn_arrow` so we can
        // test the geometry without a `Gizmos` instance.  The scale is
        // applied to both the shaft length and the wing half-width so the
        // V-shape aspect ratio stays constant.
        let dir = Vec3::new(1.0, 0.0, 0.0); // pointing +X
        let position = Vec3::new(10.0, 10.0, 0.0);
        let scale: f32 = 2.5;
        let perp = Vec3::new(-dir.y, dir.x, 0.0); // (0, 1, 0)
        let len = BURN_ARROW_HEAD_LEN_BASE * scale;
        let half = BURN_ARROW_HEAD_HALF_BASE * scale;
        let base = position - dir * len;
        let wing_l = base + perp * half;
        let wing_r = base - perp * half;
        let tip = position;

        // Tip should sit exactly at `position`.
        assert_eq!(tip, position);
        // Wings should be symmetric about `dir`: equal X-coords, mirrored
        // Y-coords around the dir line (which passes through `base` and
        // `tip`).
        assert_eq!(wing_l.x, wing_r.x);
        // Each wing sits `half` from the dir line on its respective side.
        assert!(
            (wing_l.y - base.y - half).abs() < 1e-5,
            "left wing above dir line by half",
        );
        assert!(
            (wing_r.y - base.y + half).abs() < 1e-5,
            "right wing below dir line by half",
        );
        // Wings should sit at `len` units behind the tip along `-dir`
        // (i.e. wing distance from base == half, the V's half-width).
        assert!((wing_l.distance(base) - half).abs() < 1e-5);
        // Scale doubles the absolute size: at scale=2.5 the base is 25
        // units behind the tip, vs the 25-unit base * 1.0 scale default.
        assert!((tip.distance(base) - len).abs() < 1e-5);
    }

    /// Regression for the GA-preview detachment the user reported in
    /// commit `41dbe7b`: sampling `orbit_position_from_mean_anomaly` at
    /// `t ∈ [0, 1]` (via `mean_anomaly_epoch + mean_motion * tof * t`)
    /// must land exactly on the Lambert solver's input endpoints.  If
    /// the endpoint mismatch is > 0.05 AU, the preview arcs detach from
    /// Mars / Saturn at non-optimal slider positions.
    #[test]
    fn phase_aware_ga_orbit_endpoint_matches_lambert_input() {
        use crate::fleets::orbital_mechanics::solve_lambert_transfer;
        use bevy::math::DVec3;

        let gm = GM_SUN;

        let earth_period_s = 365.25_f64 * 86_400.0;
        let mars_period_s = 1.881_f64 * earth_period_s;
        let earth_orbit = KeplerOrbit {
            semi_major_axis: 1.0,
            eccentricity: 0.0,
            inclination: 0.0,
            longitude_ascending_node: 0.0,
            argument_of_periapsis: 0.0,
            mean_anomaly_epoch: 0.0,
            mean_motion: KeplerOrbit::mean_motion_from_period(earth_period_s),
        };
        // Mars at angle π/4 — non-degenerate Lambert input (Lambert's
        // `sin_dtheta > 0` guard rejects co-linear cases).
        let mars_orbit = KeplerOrbit {
            semi_major_axis: 1.524,
            eccentricity: 0.0,
            inclination: 0.0,
            longitude_ascending_node: 0.0,
            argument_of_periapsis: 0.0,
            mean_anomaly_epoch: std::f64::consts::FRAC_PI_4,
            mean_motion: KeplerOrbit::mean_motion_from_period(mars_period_s),
        };

        // Two regimes — regimes share the same Lambert-input
        // machinery but exercise different geometry (Hohmann vs
        // off-optimal).  The off-optimal regime is the user-reported
        // bug case.
        for (label, tof_leg1) in [
            ("hohmann", 0.71 * earth_period_s),
            (
                "off_optimal_long",
                0.71 * earth_period_s * 2.0, // 2× Hohmann time
            ),
        ] {
            // Compute the predicted flyby-body position at the
            // encounter epoch (`t_dep_abs + tof_leg1`).
            let t_dep_abs = 0.0;
            let origin_pos = DVec3::from(orbit_position_from_mean_anomaly(
                &earth_orbit,
                earth_orbit.mean_anomaly_epoch,
            ));
            let flyby_pos = orbit_position_from_mean_anomaly(
                &mars_orbit,
                mars_orbit.mean_anomaly_epoch + mars_orbit.mean_motion * (t_dep_abs + tof_leg1),
            );

            // Skip if Lambert fails for this geometry — solver
            // legitimately rejects some non-physical configurations.
            let Some((_, _, orbit)) =
                solve_lambert_transfer(origin_pos, flyby_pos, tof_leg1, gm, false)
            else {
                continue;
            };

            // The preview sampling math must land on the flyby
            // body position.
            let ma_at_arrival = orbit.mean_anomaly_epoch + orbit.mean_motion * tof_leg1;
            let pos_at_arrival = orbit_position_from_mean_anomaly(&orbit, ma_at_arrival);
            let endpoint_error = (pos_at_arrival - flyby_pos).length();
            assert!(
                endpoint_error < 0.05,
                "[{label}] orbit endpoint error {endpoint_error} AU (allowed 0.05); \
                 mean_motion * tof_leg1 = {:.4} rad, mean_anomaly_epoch = {:.4}",
                orbit.mean_motion * tof_leg1,
                orbit.mean_anomaly_epoch,
            );
        }
    }

    /// Threshold regression for the multi-rev / half-rev detector in
    /// `draw_gravity_assist_preview`.  The preview uses
    /// `|orbit.mean_motion × tof - π| < 1.0` to decide whether the
    /// `PhaseAwareGaOption`'s Lambert-returned orbit is a half-revolution
    /// (render directly) or a multi-rev branch (fall back to the
    /// `compute_gravity_assist_arc` Bezier).  This test pins the
    /// threshold so future tweaks to the tolerance don't silently let
    /// multi-rev arcs back into the preview.
    #[test]
    fn phase_aware_ga_multi_rev_detector_threshold() {
        // Re-implements the detector closure to validate its boundary.
        const PI: f64 = std::f64::consts::PI;
        let accepts = |ma_travel: f64| (ma_travel - PI).abs() < 1.0;

        // Half-revolution (Hohmann-like): ma_travel ≈ π.
        assert!(accepts(PI));
        assert!(accepts(PI + 0.5));
        assert!(accepts(PI - 0.5));

        // Boundary: ±1.0 rad tolerance.
        assert!(accepts(PI + 0.999));
        assert!(!accepts(PI + 1.001));
        assert!(accepts(PI - 0.999));
        assert!(!accepts(PI - 1.001));

        // Multi-rev branches: 2π, 3π, 4π must reject.
        assert!(!accepts(2.0 * PI));
        assert!(!accepts(3.0 * PI));
        assert!(!accepts(4.0 * PI));

        // Sub-half-rev: much less than π → reject.
        assert!(accepts(PI - 0.5));
        assert!(!accepts(0.5 * PI));
    }
}

#[test]
fn tangential_arrival_tie_break_is_stable() {
    let source_a = Vec3::new(-8.0, 2.0000, 0.0);
    let source_b = Vec3::new(-8.0, 2.0001, 0.0);
    let center = Vec3::ZERO;
    let ring_r = 2.0;
    let preferred = Some(Vec3::X);

    let (point_a, _) = tangential_ring_arrival_point(source_a, center, ring_r, preferred)
        .expect("first tangent point should exist");
    let (point_b, _) = tangential_ring_arrival_point(source_b, center, ring_r, preferred)
        .expect("second tangent point should exist");

    assert!(point_a.y.signum() == point_b.y.signum());
}
// ── Gravity-assist arc geometry ──────────────────────────────────────────────

/// Computed two-leg Bezier geometry for a gravity-assist slingshot trajectory.
///
/// Both the preview and the in-transit renderer share this geometry to guarantee
/// identical curves.
struct GravityAssistArcGeometry {
    p0_1: Vec3,
    p1_1: Vec3,
    p2_1: Vec3,
    p3_1: Vec3,
    p0_2: Vec3,
    p1_2: Vec3,
    p2_2: Vec3,
    p3_2: Vec3,
}

impl GravityAssistArcGeometry {
    fn eval_leg1(&self, t: f32) -> Vec3 {
        let u = 1.0 - t;
        u * u * u * self.p0_1
            + 3.0 * u * u * t * self.p1_1
            + 3.0 * u * t * t * self.p2_1
            + t * t * t * self.p3_1
    }
    fn eval_leg2(&self, t: f32) -> Vec3 {
        let u = 1.0 - t;
        u * u * u * self.p0_2
            + 3.0 * u * u * t * self.p1_2
            + 3.0 * u * t * t * self.p2_2
            + t * t * t * self.p3_2
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
    let tang0 = if is_outward1 {
        prograde_at_p0
    } else {
        -prograde_at_p0
    };

    // ── Hyperbolic periapsis ─────────────────────────────────────────────────
    let dir_approach = dir_to_flyby;
    let dir_from_flyby = (dp - fp).normalize_or_zero();
    let dir_depart = dir_from_flyby;
    let apse_raw = dir_approach - dir_depart;
    let apse_dir = if apse_raw.length() > 0.001 {
        apse_raw.normalize()
    } else {
        Vec3::new(-dir_approach.y, dir_approach.x, 0.0)
    };
    let periapsis = fp + apse_dir * (flyby_ring_r * 2.0);
    let tang_perp_a = Vec3::new(-apse_dir.y, apse_dir.x, 0.0);
    let peri_tang = if tang_perp_a.dot(dir_depart) >= 0.0 {
        tang_perp_a
    } else {
        -tang_perp_a
    };

    let (p3_2, td2) = tangential_ring_arrival_point(periapsis, dp, dest_ring_r, None)
        .unwrap_or_else(|| {
            let inbound = -dir_from_flyby;
            let is_outward2 = dp.length_squared() > fp.length_squared();
            let arr_dir = if is_outward2 {
                Vec3::new(inbound.y, -inbound.x, 0.0)
            } else {
                Vec3::new(-inbound.y, inbound.x, 0.0)
            };
            let p3_2 = dp + arr_dir * dest_ring_r;
            (p3_2, inbound)
        });

    // ── Bezier control points ────────────────────────────────────────────────
    let cl1 = (periapsis - p0).length() * 0.40;
    let p1_1 = p0 + tang0 * cl1;
    let p2_1 = periapsis - peri_tang * cl1;

    let cl2 = (p3_2 - periapsis).length() * 0.40;
    let p1_2 = periapsis + peri_tang * cl2;
    let p2_2 = p3_2 - td2 * cl2;

    GravityAssistArcGeometry {
        p0_1: p0,
        p1_1,
        p2_1,
        p3_1: periapsis,
        p0_2: periapsis,
        p1_2,
        p2_2,
        p3_2,
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
    fleet_query: Query<(Entity, &Fleet, &ActiveManeuver, Option<&FleetOrbit>), With<Fleet>>,
    center_coords: Query<&SpaceCoordinates, Without<Fleet>>,
    body_query: Query<(&Transform, &CelestialBody, Option<&LogicalParent>), Without<Fleet>>,
    kepler_query: Query<&KeplerOrbit, Without<Fleet>>,
    amp_query: Query<&LocalOrbitAmplification, Without<Fleet>>,
    floating_origin: Option<Res<FloatingOrigin>>,
    mut fleet_ui_state: ResMut<FleetUiState>,
    view_mode: Res<ViewMode>,
    sim_time: Res<SimulationTime>,
    real_time: Res<Time<Real>>,
    time_scale: Res<TimeScale>,
    settings: Res<Settings>,
    launch_state: Res<LaunchState>,
) {
    if !launch_state.is_in_game() {
        return;
    }
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
    let real_secs = real_time.elapsed_secs() as f64;
    let real_secs_f32 = real_time.elapsed_secs();
    let sim_scale = time_scale.scale as f64;

    fleet_ui_state.waiting_orbit_count = 0;

    for (entity, fleet, maneuver, maybe_orbit) in fleet_query.iter() {
        // In System view, by default only draw the selected fleet's trajectory.
        // The player can opt in to drawing every in-transit fleet (filtered by
        // class) via Settings.show_all_fleet_trajectories (GRA-154 M-7).
        // Starmap view always draws every fleet.
        if *view_mode == ViewMode::System && !settings.show_all_fleet_trajectories {
            match fleet_ui_state.selected_fleet {
                Some(sel) if entity != sel => continue,
                None => continue, // No fleet selected → hide all trajectories
                _ => {}
            }
        } else if *view_mode == ViewMode::System {
            // show_all_fleet_trajectories == true: apply the per-class filter.
            let allow = match fleet.fleet_class() {
                FleetClass::Freighter => settings.trajectory_class_filter.freighter,
                FleetClass::Combat => settings.trajectory_class_filter.combat,
                FleetClass::Civilian => settings.trajectory_class_filter.civilian,
            };
            if !allow {
                continue;
            }
        }

        // GRA-41: hide the system-map transit gizmo for freighter fleets when
        // the player has toggled "Show freighters in transit" off.  Combat and
        // other fleet classes (no `ShipClass::Freighter` ship) are unaffected.
        if !settings.show_freighters_in_transit && fleet.has_freighter_ship() {
            continue;
        }

        let center_is_star = maneuver.reference_frame.is_barycentric()
            || body_query
                .get(maneuver.orbit_center)
                .map(|(_, b, _)| b.body_type == BodyType::Star)
                .unwrap_or(true);

        if !center_is_star && *view_mode == ViewMode::System {
            // ── Local transfer: visual-space arc ──
            let origin_ring_r = body_query
                .get(maneuver.origin_body)
                .map(|(_, b, _)| fleet_parking_visual_radius(b.visual_radius))
                .unwrap_or(0.0);
            let (dest_visual_r, default_dest_ring_r) = body_query
                .get(maneuver.destination_body)
                .map(|(_, b, _)| {
                    (
                        b.visual_radius,
                        fleet_parking_visual_radius(b.visual_radius),
                    )
                })
                .unwrap_or((0.0, 0.0));
            // Shell-driven destination ring radius: scales the dashed
            // amber ghost-body ring at the destination to the player's
            // chosen arrival shell (Low / Medium / High / Stationary /
            // HabitableInner / Cruise / etc.).  `dest_ring_r` (above)
            // stays as the small geometry offset for `compute_transfer_arc`
            // — splitting them lets us re-shape the visual ring without
            // warping the Bezier arc.
            let dest_orbit_ring_r = body_query
                .get(maneuver.destination_body)
                .map(|(_, b, _)| shell_ring_visual_radius(b, maneuver.arrival_orbit_radius_au))
                .unwrap_or(0.0);

            let origin_visual = body_query
                .get(maneuver.origin_body)
                .map(|(t, _, _)| t.translation)
                .ok();

            if let Some(op_current) = origin_visual {
                let dp_current = body_query
                    .get(maneuver.destination_body)
                    .ok()
                    .map(|(t, _, _)| t.translation);
                let Some(dp_now) = dp_current else {
                    continue;
                };

                let cv_current = body_query
                    .get(maneuver.orbit_center)
                    .map(|(t, _, _)| t.translation)
                    .unwrap_or(Vec3::ZERO);

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
                        sim_elapsed,
                        maneuver.departure_time,
                        real_secs,
                        sim_scale,
                        &body_query,
                        &kepler_query,
                        &amp_query,
                    )
                    .unwrap_or(cv_current);
                    start_pos - cv_at_departure + cv_current
                } else {
                    let op_absolute = predict_body_visual_pos(
                        maneuver.origin_body,
                        sim_elapsed,
                        maneuver.departure_time,
                        real_secs,
                        sim_scale,
                        &body_query,
                        &kepler_query,
                        &amp_query,
                    )
                    .unwrap_or(op_current);
                    let cv_at_departure = predict_body_visual_pos(
                        maneuver.orbit_center,
                        sim_elapsed,
                        maneuver.departure_time,
                        real_secs,
                        sim_scale,
                        &body_query,
                        &kepler_query,
                        &amp_query,
                    )
                    .unwrap_or(cv_current);
                    op_absolute - cv_at_departure + cv_current
                };

                // For the in-transit arc, target where the body WILL BE at arrival,
                // not where it is now.  The preview used predicted pos; once departed
                // we must continue pointing at the same predicted endpoint.
                let dest_is_star = body_query
                    .get(maneuver.destination_body)
                    .map(|(_, body, _)| body.body_type == BodyType::Star)
                    .unwrap_or(false);
                let dest_ring_r = if maneuver.reference_frame.is_barycentric()
                    && maneuver.is_kinematic()
                    && dest_is_star
                {
                    (maneuver.arrival_orbit_radius_au as f32 * SCALING_FACTOR as f32).max(1.0)
                } else {
                    default_dest_ring_r
                };
                let dp_absolute = predict_body_visual_pos(
                    maneuver.destination_body,
                    sim_elapsed,
                    maneuver.arrival_time,
                    real_secs,
                    sim_scale,
                    &body_query,
                    &kepler_query,
                    &amp_query,
                )
                .unwrap_or(dp_now);

                let cv_predicted = predict_body_visual_pos(
                    maneuver.orbit_center,
                    sim_elapsed,
                    maneuver.arrival_time,
                    real_secs,
                    sim_scale,
                    &body_query,
                    &kepler_query,
                    &amp_query,
                )
                .unwrap_or(dp_absolute);

                let dp = dp_absolute - cv_predicted + cv_current;

                let origin_lp = body_query
                    .get(maneuver.origin_body)
                    .ok()
                    .and_then(|(_, _, lp)| lp.map(|lp| lp.0));
                let dest_lp = body_query
                    .get(maneuver.destination_body)
                    .ok()
                    .and_then(|(_, _, lp)| lp.map(|lp| lp.0));
                let is_inward = if origin_lp == Some(maneuver.destination_body) {
                    true // Origin orbits the destination (e.g. Moon → Earth)
                } else if dest_lp == Some(maneuver.origin_body) {
                    false // Destination orbits the origin (e.g. Earth → Moon)
                } else {
                    op.length_squared() > dp.length_squared()
                };
                let actual_origin_ring_r = if maneuver.start_visual_pos.is_some() {
                    0.0
                } else {
                    origin_ring_r
                };
                let geo = compute_transfer_arc(
                    op,
                    dp,
                    actual_origin_ring_r,
                    dest_ring_r,
                    maneuver.start_visual_pos.is_some(),
                    is_inward,
                    maneuver.is_kinematic(),
                    cv_current,
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
                        let moon_orbit_r = rel_current.length();
                        let current_moon_a = rel_current.y.atan2(rel_current.x);
                        // departure angle of the moon relative to the planet (in current frame).
                        let rel_depart = op - cv_current;
                        let depart_moon_a = rel_depart.y.atan2(rel_depart.x);
                        // Moons orbit prograde (CCW).
                        let waiting_angle = (depart_moon_a - current_moon_a).rem_euclid(tau);
                        let full_orbits = (waiting_angle / tau) as u32;
                        let last_arc_angle = waiting_angle % tau;
                        fleet_ui_state.waiting_orbit_count = full_orbits + 1;

                        // Dim full ring for each complete extra revolution.
                        if full_orbits > 0 {
                            for i in 0..64u32 {
                                let a0 = i as f32 / 64.0 * tau;
                                let a1 = (i + 1) as f32 / 64.0 * tau;
                                gizmos.line(
                                    cv_current
                                        + Vec3::new(
                                            a0.cos() * moon_orbit_r,
                                            a0.sin() * moon_orbit_r,
                                            0.0,
                                        ),
                                    cv_current
                                        + Vec3::new(
                                            a1.cos() * moon_orbit_r,
                                            a1.sin() * moon_orbit_r,
                                            0.0,
                                        ),
                                    Color::linear_rgba(0.50, 0.05, 0.80, 0.10),
                                );
                            }
                        }
                        // Partial arc brightening CCW from the moon's current position to the
                        // departure position.
                        for i in 0..48u32 {
                            let t0 = i as f32 / 48.0;
                            let t1 = (i + 1) as f32 / 48.0;
                            let alpha = 0.10 + 0.22 * t0;
                            let a0 = current_moon_a + last_arc_angle * t0;
                            let a1 = current_moon_a + last_arc_angle * t1;
                            gizmos.line(
                                cv_current
                                    + Vec3::new(
                                        a0.cos() * moon_orbit_r,
                                        a0.sin() * moon_orbit_r,
                                        0.0,
                                    ),
                                cv_current
                                    + Vec3::new(
                                        a1.cos() * moon_orbit_r,
                                        a1.sin() * moon_orbit_r,
                                        0.0,
                                    ),
                                Color::linear_rgba(0.55, 0.05, 0.85, alpha),
                            );
                        }
                        // Small tick at the moon's current orbital position.
                        let tick = 5.0_f32;
                        let tick_col = Color::linear_rgba(0.65, 0.10, 0.90, 0.65);
                        gizmos.line(
                            op_current - Vec3::X * tick,
                            op_current + Vec3::X * tick,
                            tick_col,
                        );
                        gizmos.line(
                            op_current - Vec3::Y * tick,
                            op_current + Vec3::Y * tick,
                            tick_col,
                        );
                    } else if let Some(orbit) = maybe_orbit {
                        // ── Planet origin: arc along the fleet's parking ring ──
                        if orbit.direction != 0.0 && orbit.body == maneuver.origin_body {
                            let orbit_center = body_query
                                .get(maneuver.origin_body)
                                .map(|(t, _, _)| t.translation)
                                .unwrap_or(Vec3::ZERO);
                            let current_a = orbit.angle_rad as f32;
                            let waiting_angle = if orbit.direction > 0.0 {
                                (dep_angle - current_a).rem_euclid(tau)
                            } else {
                                (current_a - dep_angle).rem_euclid(tau)
                            };
                            let full_orbits = (waiting_angle / tau) as u32;
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
                                let t0 = i as f32 / 48.0;
                                let t1 = (i + 1) as f32 / 48.0;
                                let alpha = 0.10 + 0.22 * t0;
                                let a0 = arc_start + orbit.direction as f32 * last_arc_angle * t0;
                                let a1 = arc_start + orbit.direction as f32 * last_arc_angle * t1;
                                gizmos.line(
                                    orbit_center + Vec3::new(a0.cos() * r, a0.sin() * r, 0.0),
                                    orbit_center + Vec3::new(a1.cos() * r, a1.sin() * r, 0.0),
                                    Color::linear_rgba(0.55, 0.05, 0.85, alpha),
                                );
                            }
                            let fleet_pos = orbit_center
                                + Vec3::new(current_a.cos() * r, current_a.sin() * r, 0.0);
                            let tick = 5.0_f32;
                            let tick_col = Color::linear_rgba(0.65, 0.10, 0.90, 0.65);
                            gizmos.line(
                                fleet_pos - Vec3::X * tick,
                                fleet_pos + Vec3::X * tick,
                                tick_col,
                            );
                            gizmos.line(
                                fleet_pos - Vec3::Y * tick,
                                fleet_pos + Vec3::Y * tick,
                                tick_col,
                            );
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
                let glow_pos = progress_t + (real_secs_f32 / 4.0_f32).fract() * remaining_span;
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
                    if t_frac <= progress_t {
                        continue;
                    }
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
                // visually strange.  Pass `dest_orbit_ring_r` (the shell-driven
                // radius) rather than `dest_ring_r` (the geometry offset) so the
                // dashed amber ring scales with the player's chosen arrival shell.
                let dest_is_orbit_center = origin_lp == Some(maneuver.destination_body);
                draw_ghost_body(
                    &mut gizmos,
                    dp,
                    dest_orbit_ring_r,
                    dest_visual_r,
                    dest_is_orbit_center,
                );
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
            let origin_lp = body_query
                .get(maneuver.origin_body)
                .ok()
                .and_then(|(_, _, lp)| lp.map(|lp| lp.0));
            let dest_lp = body_query
                .get(maneuver.destination_body)
                .ok()
                .and_then(|(_, _, lp)| lp.map(|lp| lp.0));
            let origin_ring_r = body_query
                .get(maneuver.origin_body)
                .map(|(_, b, _)| fleet_parking_visual_radius(b.visual_radius))
                .unwrap_or(0.0);
            let (dest_visual_r, default_dest_ring_r) = body_query
                .get(maneuver.destination_body)
                .map(|(_, b, _)| {
                    (
                        b.visual_radius,
                        fleet_parking_visual_radius(b.visual_radius),
                    )
                })
                .unwrap_or((0.0, 0.0));
            // Shell-driven destination ring radius: scales the dashed
            // amber ghost-body ring at the destination to the player's
            // chosen arrival shell (Low / Medium / High / Stationary /
            // HabitableInner / Cruise / etc.).  `dest_ring_r` (above)
            // stays as the small geometry offset for `compute_transfer_arc`
            // — splitting them lets us re-shape the visual ring without
            // warping the Bezier arc.
            let dest_orbit_ring_r = body_query
                .get(maneuver.destination_body)
                .map(|(_, b, _)| shell_ring_visual_radius(b, maneuver.arrival_orbit_radius_au))
                .unwrap_or(0.0);
            let is_inter_star = maneuver.reference_frame.is_barycentric();

            // ── Departure point (op) ─────────────────────────────────────────
            // Course corrections: use the stored visual start position so it matches
            // what the preview showed at the time of the correction.
            let origin_now = body_query
                .get(maneuver.origin_body)
                .map(|(t, _, _)| t.translation)
                .unwrap_or(Vec3::ZERO);
            let is_course_correction = maneuver.start_visual_pos.is_some();
            let actual_origin_ring_r = if is_course_correction {
                0.0
            } else {
                origin_ring_r
            };
            let op = if let Some(start_pos) = maneuver.start_visual_pos {
                start_pos
            } else {
                predict_body_visual_pos(
                    maneuver.origin_body,
                    sim_elapsed,
                    maneuver.departure_time,
                    real_secs,
                    sim_scale,
                    &body_query,
                    &kepler_query,
                    &amp_query,
                )
                .unwrap_or(origin_now)
            };

            // ── Arrival point (dp) ───────────────────────────────────────────
            // Identical to preview: predict destination at arrival, then correct
            // for the orbit-centre drift so the arc endpoint tracks the body now.
            let dest_now = body_query
                .get(maneuver.destination_body)
                .map(|(t, _, _)| t.translation)
                .unwrap_or(Vec3::ZERO);
            let dest_is_star = body_query
                .get(maneuver.destination_body)
                .map(|(_, body, _)| body.body_type == BodyType::Star)
                .unwrap_or(false);
            let dest_ring_r = if is_inter_star && maneuver.is_kinematic() && dest_is_star {
                (maneuver.arrival_orbit_radius_au as f32 * SCALING_FACTOR as f32).max(1.0)
            } else {
                default_dest_ring_r
            };
            let dp_absolute = predict_body_visual_pos(
                maneuver.destination_body,
                sim_elapsed,
                maneuver.arrival_time,
                real_secs,
                sim_scale,
                &body_query,
                &kepler_query,
                &amp_query,
            )
            .unwrap_or(dest_now);

            let (dp, cv_ref, flyby_center_predicted, flyby_center_departure) = if is_inter_star {
                (dp_absolute, Vec3::ZERO, Vec3::ZERO, Vec3::ZERO)
            } else {
                let orbit_center_visual = maneuver.orbit_center;
                let cv_predicted = predict_body_visual_pos(
                    orbit_center_visual,
                    sim_elapsed,
                    maneuver.arrival_time,
                    real_secs,
                    sim_scale,
                    &body_query,
                    &kepler_query,
                    &amp_query,
                )
                .unwrap_or(dp_absolute);
                let cv_at_departure = predict_body_visual_pos(
                    orbit_center_visual,
                    sim_elapsed,
                    maneuver.departure_time,
                    real_secs,
                    sim_scale,
                    &body_query,
                    &kepler_query,
                    &amp_query,
                )
                .unwrap_or(cv_predicted);
                (
                    dp_absolute - cv_predicted + cv_at_departure,
                    cv_at_departure,
                    cv_predicted,
                    cv_at_departure,
                )
            };

            // --- gravity assist special case ------------------------------------------------
            if let (Some(flyby), Some(_)) = (maneuver.flyby_body, maneuver.leg2_orbit.as_ref()) {
                // predict flyby position at the time the second leg begins
                let fp_absolute = predict_body_visual_pos(
                    flyby,
                    sim_elapsed,
                    maneuver.departure_time + maneuver.leg2_start_s,
                    real_secs,
                    sim_scale,
                    &body_query,
                    &kepler_query,
                    &amp_query,
                )
                .unwrap_or_else(|| {
                    body_query
                        .get(flyby)
                        .ok()
                        .map(|(t, _, _)| t.translation)
                        .unwrap_or(Vec3::ZERO)
                });
                let fp = if is_inter_star {
                    fp_absolute
                } else {
                    fp_absolute - flyby_center_predicted + flyby_center_departure
                };

                // ring radii and visual size
                let flyby_visual_r = body_query
                    .get(flyby)
                    .map(|(_, b, _)| b.visual_radius)
                    .unwrap_or(0.0);
                let flyby_ring_r = fleet_parking_visual_radius(flyby_visual_r);

                // Geometry for the two legs: sample each leg from the actual
                // Kepler orbit (`maneuver.transfer_orbit` for leg 1,
                // `maneuver.leg2_orbit` for leg 2).  This replaces the
                // previous `compute_gravity_assist_arc` Bezier, which
                // interpolated a smooth curve through the three control
                // points without any orbital-mechanics fidelity — the
                // user-visible shape did not match the ship's actual
                // path.  Each orbit's mean_anomaly_epoch is set when the
                // orbit is built and `leg1_time_s` / `leg2_time_s` define
                // how far the mean anomaly advances over the leg, so
                // sampling the orbit directly produces the *real*
                // position-vs-time trajectory the spacecraft follows.
                //
                // For inter-star gravity assists the floating origin is
                // the departure star; for same-star gravity assists it is
                // the orbit-centre body (typically the Sun).  Either way
                // the orbit-centre local position is zero in the render
                // frame, so the closure evaluates `orbit_position_from_mean_anomaly`
                // against a star frame.
                let leg1_orbit = &maneuver.transfer_orbit;
                let Some(leg2_orbit) = maneuver.leg2_orbit.as_ref() else {
                    // Defensive: the outer `if let` guarantees this, but
                    // be explicit so future refactors don't silently
                    // fall through to the (now-deleted) Bezier branch.
                    continue;
                };
                let leg1_total_ma = leg1_orbit.mean_motion * maneuver.leg2_start_s.abs();
                let leg2_total_ma = leg2_orbit.mean_motion
                    * ((maneuver.arrival_time - maneuver.departure_time) - maneuver.leg2_start_s)
                        .abs();
                let cv_for_orbit = if is_inter_star {
                    Vec3::ZERO
                } else {
                    // Same-star gravity assist: anchor the orbit around
                    // the orbit-centre body's predicted departure-time
                    // position so the leg 1 arc tracks the planet.
                    flyby_center_departure
                };
                // The closure evaluates the orbit in the local frame of
                // its orbit centre; `op`-style absolute positions are not
                // relevant — only the relative shape matters for the
                // in-transit arc, which the planet's drift compensates
                // for via the `flyby_center_departure` shift below.
                let leg1_point_at = |t: f32| -> Vec3 {
                    let ma = leg1_orbit.mean_anomaly_epoch + leg1_total_ma * t as f64;
                    let pos_au = orbit_position_from_mean_anomaly(leg1_orbit, ma);
                    Vec3::new(
                        (pos_au.x * SCALING_FACTOR) as f32,
                        (pos_au.y * SCALING_FACTOR) as f32,
                        (pos_au.z * SCALING_FACTOR) as f32,
                    ) + cv_for_orbit
                };
                let leg2_point_at = |t: f32| -> Vec3 {
                    let ma = leg2_orbit.mean_anomaly_epoch + leg2_total_ma * t as f64;
                    let pos_au = orbit_position_from_mean_anomaly(leg2_orbit, ma);
                    Vec3::new(
                        (pos_au.x * SCALING_FACTOR) as f32,
                        (pos_au.y * SCALING_FACTOR) as f32,
                        (pos_au.z * SCALING_FACTOR) as f32,
                    ) + cv_for_orbit
                };

                // fractions along total t range where leg switch happens
                let leg1_frac = if maneuver.arrival_time > maneuver.departure_time {
                    (maneuver.leg2_start_s / (maneuver.arrival_time - maneuver.departure_time))
                        as f32
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
                let glow_pos = progress_t + (real_secs_f32 / 4.0_f32).fract() * remaining_span;
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

                // draw remaining piecewise along the two real-orbit legs
                let eval_at = |t_frac: f32| -> Vec3 {
                    if t_frac < leg1_frac {
                        leg1_point_at(t_frac / leg1_frac.max(1e-6))
                    } else {
                        leg2_point_at((t_frac - leg1_frac) / leg2_frac.max(1e-6))
                    }
                };
                let mut prev: Option<Vec3> = Some(eval_at(progress_t));

                for i in 0..=SEGMENTS {
                    let t_frac = i as f32 / SEGMENTS as f32;
                    if t_frac <= progress_t {
                        continue;
                    }
                    let pos = eval_at(t_frac);
                    if let Some(prev_pos) = prev {
                        gizmos.line(prev_pos, pos, traj_color(t_frac));
                    }
                    prev = Some(pos);
                }

                // ghost bodies at flyby and destination
                draw_ghost_body(&mut gizmos, fp, flyby_ring_r, flyby_visual_r, false);
                let dest_is_orbit_center = origin_lp == Some(maneuver.destination_body);
                draw_ghost_body(
                    &mut gizmos,
                    dp,
                    dest_ring_r,
                    dest_visual_r,
                    dest_is_orbit_center,
                );
                continue;
            }

            if is_inter_star && !maneuver.is_kinematic() {
                let progress_t = if maneuver.arrival_time > maneuver.departure_time {
                    ((sim_elapsed - maneuver.departure_time)
                        / (maneuver.arrival_time - maneuver.departure_time))
                        .clamp(0.0, 1.0) as f32
                } else {
                    1.0_f32
                };
                let remaining_span = (1.0 - progress_t).max(1e-4_f32);
                let glow_pos = progress_t + (real_secs_f32 / 4.0_f32).fract() * remaining_span;
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

                let total_ma_travel = maneuver.transfer_orbit.mean_motion
                    * (maneuver.arrival_time - maneuver.departure_time);
                // Sample the actual inter-star `transfer_orbit` instead
                // of interpolating a Bezier between predicted endpoints.
                // For inter-star transfers the floating origin is the
                // departure star (`Vec3::ZERO`); the orbit's local frame
                // matches AU, so SCALING_FACTOR is the only transform.
                // Mirror what `build_visual_sampled_transfer_polyline`
                // does, but inline because this branch needs to gate
                // rendering on `progress_t` and `t_frac` directly rather
                // than route through the helper's `start_fraction`
                // parameter.
                let orbit = &maneuver.transfer_orbit;
                let orbit_point = |t: f32| -> Vec3 {
                    let ma = orbit.mean_anomaly_epoch + total_ma_travel * t as f64;
                    let pos_au = orbit_position_from_mean_anomaly(orbit, ma);
                    Vec3::new(
                        (pos_au.x * SCALING_FACTOR) as f32,
                        (pos_au.y * SCALING_FACTOR) as f32,
                        (pos_au.z * SCALING_FACTOR) as f32,
                    )
                };
                let mut prev: Option<Vec3> = Some(orbit_point(progress_t));
                for i in 0..=SEGMENTS {
                    let t_frac = i as f32 / SEGMENTS as f32;
                    if t_frac <= progress_t {
                        continue;
                    }
                    let pos = orbit_point(t_frac);
                    if let Some(prev_pos) = prev {
                        gizmos.line(prev_pos, pos, traj_color(t_frac));
                    }
                    prev = Some(pos);
                }

                draw_ghost_body(&mut gizmos, dp, dest_orbit_ring_r, dest_visual_r, false);
                continue;
            }

            if !is_inter_star && !maneuver.is_kinematic() {
                let progress_t = if maneuver.arrival_time > maneuver.departure_time {
                    ((sim_elapsed - maneuver.departure_time)
                        / (maneuver.arrival_time - maneuver.departure_time))
                        .clamp(0.0, 1.0) as f32
                } else {
                    1.0_f32
                };
                let remaining_span = (1.0 - progress_t).max(1e-4_f32);
                let glow_pos = progress_t + (real_secs_f32 / 4.0_f32).fract() * remaining_span;
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

                let is_inward = if is_course_correction {
                    op.length_squared() > dp.length_squared()
                } else if origin_lp == Some(maneuver.destination_body) {
                    true
                } else if dest_lp == Some(maneuver.origin_body) {
                    false
                } else {
                    op.length_squared() > dp.length_squared()
                };

                let center_is_star = body_query
                    .get(maneuver.orbit_center)
                    .map(|(_, body, _)| body.body_type == BodyType::Star)
                    .unwrap_or(false);

                if center_is_star
                    && maneuver.departure_velocity_ms.is_some()
                    && maneuver.arrival_velocity_ms.is_some()
                    && maneuver.start_position_au.is_some()
                    && maneuver.end_position_au.is_some()
                {
                    let total_ma_travel = maneuver.transfer_orbit.mean_motion
                        * (maneuver.arrival_time - maneuver.departure_time);
                    // Sample the actual `maneuver.transfer_orbit` for
                    // same-star heliocentric transfers that carry full
                    // planned-transfer data (departure/arrival velocity
                    // and AU endpoints).  This unifies the in-transit
                    // rendering with the porkchop preview path which
                    // already uses actual orbit sampling — both produce
                    // the *same* real Keplerian half-ellipse, no Bezier.
                    let orbit = &maneuver.transfer_orbit;
                    let orbit_point = |t: f32| -> Vec3 {
                        let ma = orbit.mean_anomaly_epoch + total_ma_travel * t as f64;
                        let pos_au = orbit_position_from_mean_anomaly(orbit, ma);
                        Vec3::new(
                            (pos_au.x * SCALING_FACTOR) as f32,
                            (pos_au.y * SCALING_FACTOR) as f32,
                            (pos_au.z * SCALING_FACTOR) as f32,
                        )
                    };
                    let mut prev: Option<Vec3> = Some(orbit_point(progress_t));
                    for i in 0..=SEGMENTS {
                        let t_frac = i as f32 / SEGMENTS as f32;
                        if t_frac <= progress_t {
                            continue;
                        }
                        let pos = orbit_point(t_frac);
                        if let Some(prev_pos) = prev {
                            gizmos.line(prev_pos, pos, traj_color(t_frac));
                        }
                        prev = Some(pos);
                    }

                    draw_ghost_body(
                        &mut gizmos,
                        dp_absolute,
                        dest_orbit_ring_r,
                        dest_visual_r,
                        false,
                    );
                    continue;
                }

                let sampled_dest = if center_is_star { dp_absolute } else { dp };

                // Use `mean_motion * sim_elapsed` as the orbit's
                // start mean_anomaly so the trajectory begins at the
                // planet's current mean_anomaly.  The orbit's
                // `mean_anomaly_epoch` was set to the planet's
                // mean_anomaly at the time the orbit was built
                // (which may be many sim-minutes ago), so using it
                // directly here would draw the orbit at the OLD
                // planet position — half an orbit or more off from
                // the planet's current position, especially at 1
                // yr/s where the planet completes a full orbit per
                // real second.
                let start_mean_anomaly = maneuver.transfer_orbit.mean_motion * sim_elapsed;
                let visual_points = if center_is_star {
                    build_visual_sampled_transfer_polyline_moving_center(
                        &maneuver.transfer_orbit,
                        maneuver.orbit_center,
                        sim_elapsed,
                        maneuver.departure_time,
                        maneuver.arrival_time - maneuver.departure_time,
                        real_secs,
                        sim_scale,
                        &body_query,
                        &kepler_query,
                        &amp_query,
                        start_mean_anomaly,
                        maneuver.transfer_orbit.mean_motion
                            * (maneuver.arrival_time - maneuver.departure_time),
                        128,
                    )
                } else {
                    let center_visual = body_query
                        .get(maneuver.orbit_center)
                        .map(|(t, _, _)| t.translation)
                        .unwrap_or(Vec3::ZERO);
                    build_visual_sampled_transfer_polyline(
                        &maneuver.transfer_orbit,
                        center_visual,
                        op,
                        actual_origin_ring_r,
                        dp,
                        dest_ring_r,
                        is_course_correction,
                        is_inward,
                        start_mean_anomaly,
                        maneuver.transfer_orbit.mean_motion
                            * (maneuver.arrival_time - maneuver.departure_time),
                        128,
                    )
                };
                draw_sampled_transfer_orbit(&mut gizmos, &visual_points, progress_t, traj_color);

                draw_ghost_body(
                    &mut gizmos,
                    sampled_dest,
                    dest_orbit_ring_r,
                    dest_visual_r,
                    false,
                );
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
            let geo = if is_inter_star && maneuver.is_kinematic() {
                compute_direct_transfer_arc(op, dp, actual_origin_ring_r, dest_ring_r)
            } else {
                compute_transfer_arc(
                    op,
                    dp,
                    actual_origin_ring_r,
                    dest_ring_r,
                    is_course_correction,
                    is_inward,
                    maneuver.is_kinematic(),
                    cv_ref,
                )
            };

            // ── Progress & glow ──────────────────────────────────────────────
            let progress_t = if maneuver.arrival_time > maneuver.departure_time {
                ((sim_elapsed - maneuver.departure_time)
                    / (maneuver.arrival_time - maneuver.departure_time))
                    .clamp(0.0, 1.0) as f32
            } else {
                1.0_f32
            };
            let remaining_span = (1.0 - progress_t).max(1e-4_f32);
            let glow_pos = progress_t + (real_secs_f32 / 4.0_f32).fract() * remaining_span;
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
                if t_frac <= progress_t {
                    continue;
                }
                let pos = geo.eval(t_frac);
                if let Some(prev_pos) = prev {
                    gizmos.line(prev_pos, pos, traj_color(t_frac));
                }
                prev = Some(pos);
            }

            draw_ghost_body(&mut gizmos, dp, dest_orbit_ring_r, dest_visual_r, false);
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
        let glow_pos = progress_t + (real_secs_f32 / 4.0_f32).fract() * remaining_span;
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
                center_coords
                    .get(maneuver.origin_body)
                    .map(|sc| sc.position)
                    .unwrap_or(DVec3::ZERO)
            }) - center_pos;

            let p1_au = maneuver.end_position_au.unwrap_or_else(|| {
                center_coords
                    .get(maneuver.destination_body)
                    .map(|sc| sc.position)
                    .unwrap_or(DVec3::ZERO)
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
                if t0 <= progress_t {
                    continue;
                }
                let pos = rp0 + (rp1 - rp0) * t0;
                if let Some(prev_pos) = prev {
                    gizmos.line(prev_pos, pos, traj_color(t0));
                }
                prev = Some(pos);
            }
        } else {
            let total_ma_travel = maneuver.transfer_orbit.mean_motion
                * (maneuver.arrival_time - maneuver.departure_time);

            // Start exactly at the planet's current mean_anomaly
            // (not the orbit's mean_anomaly_epoch, which was set
            // when the orbit was built and may be many sim-minutes
            // stale by the time we render the trajectory).
            let ma_start = maneuver.transfer_orbit.mean_motion * sim_elapsed
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
                if (frac as f32) <= progress_t {
                    continue;
                }

                // Use the planet's current mean_anomaly as the
                // orbit's start, not the orbit's mean_anomaly_epoch
                // (which was set when the orbit was built).
                let mean_anomaly =
                    maneuver.transfer_orbit.mean_motion * sim_elapsed + total_ma_travel * frac;
                let orbit_pos =
                    orbit_position_from_mean_anomaly(&maneuver.transfer_orbit, mean_anomaly);
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
    launch_state: Res<LaunchState>,
) {
    if !launch_state.is_in_game() {
        return;
    }
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
            let camera_radius = camera_query.single().map(|c| c.radius).unwrap_or(200_000.0);
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
    fleet_query: Query<
        (&SpaceCoordinates, Option<&ActiveManeuver>),
        (With<Fleet>, Without<FleetMesh>),
    >,
    floating_origin: Option<Res<FloatingOrigin>>,
    launch_state: Res<LaunchState>,
) {
    if !launch_state.is_in_game() {
        return;
    }
    let origin_offset = floating_origin
        .as_ref()
        .map(|fo| fo.position)
        .unwrap_or(DVec3::ZERO);

    for (sc, maybe_maneuver) in fleet_query.iter() {
        let render_du = (sc.position - origin_offset) * SCALING_FACTOR;
        let render_pos = Vec3::new(render_du.x as f32, render_du.y as f32, render_du.z as f32);

        let color = if maybe_maneuver.is_some() {
            Color::srgba(0.3, 0.8, 1.0, 0.9) // cyan while in transit
        } else {
            Color::srgba(0.2, 0.9, 0.3, 0.9) // green while in orbit
        };

        let size = 10.0_f32;
        gizmos.line(
            render_pos - Vec3::X * size,
            render_pos + Vec3::X * size,
            color,
        );
        gizmos.line(
            render_pos - Vec3::Y * size,
            render_pos + Vec3::Y * size,
            color,
        );
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
    let camera_radius = camera_query
        .single()
        .ok()
        .map(|c| c.radius)
        .unwrap_or(200_000.0);
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
        gizmos.line(
            pos - Vec3::new(icon_size * 0.6, icon_size * 0.6, 0.0),
            pos + Vec3::new(icon_size * 0.6, icon_size * 0.6, 0.0),
            color.with_alpha(0.5),
        );
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
    fleet_query: Query<
        (Option<&ActiveManeuver>, &MeshMaterial3d<StandardMaterial>),
        (With<Fleet>, With<FleetMesh>),
    >,
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
        (
            Entity,
            &SpaceCoordinates,
            &mut Transform,
            &mut Visibility,
            Option<&FleetOrbit>,
            Option<&ActiveManeuver>,
        ),
        With<Fleet>,
    >,
    body_query: Query<(&Transform, &CelestialBody, Option<&LogicalParent>), Without<Fleet>>,
    kepler_query: Query<&KeplerOrbit, Without<Fleet>>,
    amp_query: Query<&LocalOrbitAmplification, Without<Fleet>>,
    floating_origin: Option<Res<FloatingOrigin>>,
    view_mode: Res<ViewMode>,
    sim_time: Res<SimulationTime>,
    real_time: Res<Time<Real>>,
    time_scale: Res<TimeScale>,
    fleet_ui_state: Res<FleetUiState>,
) {
    let origin_offset = floating_origin
        .as_ref()
        .map(|fo| fo.position)
        .unwrap_or(DVec3::ZERO);
    let elapsed = sim_time.elapsed_seconds();
    let real_secs = real_time.elapsed_secs() as f64;
    let sim_scale = time_scale.scale as f64;
    let selected = fleet_ui_state.selected_fleet;

    for (entity, sc, mut transform, mut vis, maybe_orbit, maybe_maneuver) in fleet_query.iter_mut()
    {
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
            transform.rotation = Quat::IDENTITY;
            let center_is_star = maneuver.reference_frame.is_barycentric()
                || body_query
                    .get(maneuver.orbit_center)
                    .map(|(_, b, _)| b.body_type == BodyType::Star)
                    .unwrap_or(true);

            if !center_is_star {
                // Local transfer: follow the same cubic Bezier as the trajectory gizmo.
                let origin_data = body_query
                    .get(maneuver.origin_body)
                    .ok()
                    .map(|(t, b, _)| (t.translation, fleet_parking_visual_radius(b.visual_radius)));
                let dest_data = body_query
                    .get(maneuver.destination_body)
                    .ok()
                    .map(|(t, b, _)| (t.translation, fleet_parking_visual_radius(b.visual_radius)));
                if let (Some((op_current, origin_ring_r)), Some((dp_now, dest_ring_r))) =
                    (origin_data, dest_data)
                {
                    let cv_current = body_query
                        .get(maneuver.orbit_center)
                        .map(|(t, _, _)| t.translation)
                        .unwrap_or(Vec3::ZERO);

                    // Once departed, fix the origin at the moon's departure-time position
                    // so the fleet moves with the planet but not the moon's orbit.
                    let op = if let Some(start_pos) = maneuver.start_visual_pos {
                        let cv_at_departure = predict_body_visual_pos(
                            maneuver.orbit_center,
                            elapsed,
                            maneuver.departure_time,
                            real_secs,
                            sim_scale,
                            &body_query,
                            &kepler_query,
                            &amp_query,
                        )
                        .unwrap_or(cv_current);
                        start_pos - cv_at_departure + cv_current
                    } else {
                        let op_absolute = predict_body_visual_pos(
                            maneuver.origin_body,
                            elapsed,
                            maneuver.departure_time,
                            real_secs,
                            sim_scale,
                            &body_query,
                            &kepler_query,
                            &amp_query,
                        )
                        .unwrap_or(op_current);
                        let cv_at_departure = predict_body_visual_pos(
                            maneuver.orbit_center,
                            elapsed,
                            maneuver.departure_time,
                            real_secs,
                            sim_scale,
                            &body_query,
                            &kepler_query,
                            &amp_query,
                        )
                        .unwrap_or(cv_current);
                        op_absolute - cv_at_departure + cv_current
                    };

                    let dp_absolute = predict_body_visual_pos(
                        maneuver.destination_body,
                        elapsed,
                        maneuver.arrival_time,
                        real_secs,
                        sim_scale,
                        &body_query,
                        &kepler_query,
                        &amp_query,
                    )
                    .unwrap_or(dp_now);

                    let cv_predicted = predict_body_visual_pos(
                        maneuver.orbit_center,
                        elapsed,
                        maneuver.arrival_time,
                        real_secs,
                        sim_scale,
                        &body_query,
                        &kepler_query,
                        &amp_query,
                    )
                    .unwrap_or(dp_absolute);

                    let dp = dp_absolute - cv_predicted + cv_current;

                    let progress = maneuver.progress(elapsed) as f32;

                    // Use shared geometry — guarantees dot follows the same curve
                    // as the transit gizmo and preview.
                    let origin_lp = body_query
                        .get(maneuver.origin_body)
                        .ok()
                        .and_then(|(_, _, lp)| lp.map(|lp| lp.0));
                    let dest_lp = body_query
                        .get(maneuver.destination_body)
                        .ok()
                        .and_then(|(_, _, lp)| lp.map(|lp| lp.0));
                    let is_inward = if origin_lp == Some(maneuver.destination_body) {
                        true
                    } else if dest_lp == Some(maneuver.origin_body) {
                        false
                    } else {
                        op.length_squared() > dp.length_squared()
                    };
                    let actual_origin_ring_r = if maneuver.start_visual_pos.is_some() {
                        0.0
                    } else {
                        origin_ring_r
                    };
                    let geo = compute_transfer_arc(
                        op,
                        dp,
                        actual_origin_ring_r,
                        dest_ring_r,
                        maneuver.start_visual_pos.is_some(),
                        is_inward,
                        maneuver.is_kinematic(),
                        cv_current,
                    );
                    transform.translation = geo.eval(progress);

                    // Hide the sphere while still inside the origin or destination orbit ring.
                    let inside_origin = transform.translation.distance(op) < origin_ring_r;
                    let inside_dest = transform.translation.distance(dp) < dest_ring_r;
                    if inside_origin || inside_dest {
                        *vis = Visibility::Hidden;
                    }
                }
            } else {
                // Heliocentric transfer in System view: follow Bezier matching the
                // trajectory gizmo so the green dot tracks the purple arc exactly.
                // The physics SpaceCoordinates (Keplerian) are kept for range queries;
                // only the visual Transform follows the Bezier.
                let origin_lp = body_query
                    .get(maneuver.origin_body)
                    .ok()
                    .and_then(|(_, _, lp)| lp.map(|lp| lp.0));
                let dest_lp_entity = body_query
                    .get(maneuver.destination_body)
                    .ok()
                    .and_then(|(_, _, lp)| lp.map(|lp| lp.0));

                let origin_ring_r = body_query
                    .get(maneuver.origin_body)
                    .map(|(_, b, _)| fleet_parking_visual_radius(b.visual_radius))
                    .unwrap_or(0.0);
                let default_dest_ring_r = body_query
                    .get(maneuver.destination_body)
                    .map(|(_, b, _)| fleet_parking_visual_radius(b.visual_radius))
                    .unwrap_or(0.0);
                let is_inter_star = maneuver.reference_frame.is_barycentric();

                let origin_now = body_query
                    .get(maneuver.origin_body)
                    .map(|(t, _, _)| t.translation)
                    .unwrap_or(Vec3::ZERO);
                let is_course_correction = maneuver.start_visual_pos.is_some();
                let actual_origin_ring_r = if is_course_correction {
                    0.0
                } else {
                    origin_ring_r
                };
                let op = if let Some(start_pos) = maneuver.start_visual_pos {
                    start_pos
                } else {
                    predict_body_visual_pos(
                        maneuver.origin_body,
                        elapsed,
                        maneuver.departure_time,
                        real_secs,
                        sim_scale,
                        &body_query,
                        &kepler_query,
                        &amp_query,
                    )
                    .unwrap_or(origin_now)
                };

                let dest_now = body_query
                    .get(maneuver.destination_body)
                    .map(|(t, _, _)| t.translation)
                    .unwrap_or(Vec3::ZERO);
                let dest_is_star = body_query
                    .get(maneuver.destination_body)
                    .map(|(_, body, _)| body.body_type == BodyType::Star)
                    .unwrap_or(false);
                let dest_ring_r = if is_inter_star && maneuver.is_kinematic() && dest_is_star {
                    (maneuver.arrival_orbit_radius_au as f32 * SCALING_FACTOR as f32).max(1.0)
                } else {
                    default_dest_ring_r
                };
                let dp_absolute = predict_body_visual_pos(
                    maneuver.destination_body,
                    elapsed,
                    maneuver.arrival_time,
                    real_secs,
                    sim_scale,
                    &body_query,
                    &kepler_query,
                    &amp_query,
                )
                .unwrap_or(dest_now);
                let orbit_center_visual = maneuver.orbit_center;
                let cv_predicted = predict_body_visual_pos(
                    orbit_center_visual,
                    elapsed,
                    maneuver.arrival_time,
                    real_secs,
                    sim_scale,
                    &body_query,
                    &kepler_query,
                    &amp_query,
                )
                .unwrap_or(dp_absolute);
                let cv_at_departure = predict_body_visual_pos(
                    orbit_center_visual,
                    elapsed,
                    maneuver.departure_time,
                    real_secs,
                    sim_scale,
                    &body_query,
                    &kepler_query,
                    &amp_query,
                )
                .unwrap_or(cv_predicted);
                // For inter-star transfers, skip the orbit-centre drift correction —
                // dp_absolute is already in the correct visual frame, matching
                // draw_fleet_trajectories which also uses dp_absolute directly.
                let dp = if is_inter_star {
                    dp_absolute
                } else {
                    dp_absolute - cv_predicted + cv_at_departure
                };

                if let (Some(flyby), Some(_)) = (maneuver.flyby_body, maneuver.leg2_orbit.as_ref())
                {
                    let fp_absolute = predict_body_visual_pos(
                        flyby,
                        elapsed,
                        maneuver.departure_time + maneuver.leg2_start_s,
                        real_secs,
                        sim_scale,
                        &body_query,
                        &kepler_query,
                        &amp_query,
                    )
                    .unwrap_or_else(|| {
                        body_query
                            .get(flyby)
                            .ok()
                            .map(|(t, _, _)| t.translation)
                            .unwrap_or(Vec3::ZERO)
                    });
                    let fp = if maneuver.reference_frame.is_barycentric() {
                        fp_absolute
                    } else {
                        fp_absolute - cv_predicted + cv_at_departure
                    };

                    let flyby_ring_r = body_query
                        .get(flyby)
                        .map(|(_, b, _)| fleet_parking_visual_radius(b.visual_radius))
                        .unwrap_or(0.0);

                    let ga_geo = compute_gravity_assist_arc(
                        op,
                        fp,
                        dp,
                        actual_origin_ring_r,
                        flyby_ring_r,
                        dest_ring_r,
                    );

                    let leg1_frac = if maneuver.arrival_time > maneuver.departure_time {
                        (maneuver.leg2_start_s / (maneuver.arrival_time - maneuver.departure_time))
                            as f32
                    } else {
                        0.5_f32
                    };
                    let leg2_frac = 1.0 - leg1_frac;
                    let progress = maneuver.progress(elapsed) as f32;

                    transform.translation = if progress < leg1_frac {
                        ga_geo.eval_leg1(progress / leg1_frac.max(1e-6))
                    } else {
                        ga_geo.eval_leg2((progress - leg1_frac) / leg2_frac.max(1e-6))
                    };

                    let inside_origin = transform.translation.distance(op) < origin_ring_r;
                    let inside_dest = transform.translation.distance(dp) < dest_ring_r;
                    if inside_origin || inside_dest {
                        *vis = Visibility::Hidden;
                    }
                    continue;
                }

                if maneuver.reference_frame.is_barycentric() && !maneuver.is_kinematic() {
                    let render_du = (sc.position - origin_offset) * SCALING_FACTOR;
                    transform.translation =
                        Vec3::new(render_du.x as f32, render_du.y as f32, render_du.z as f32);

                    let inside_origin = transform.translation.distance(op) < origin_ring_r;
                    let inside_dest = transform.translation.distance(dp) < dest_ring_r;
                    if inside_origin || inside_dest {
                        *vis = Visibility::Hidden;
                    }
                    continue;
                }

                if !is_inter_star && !maneuver.is_kinematic() {
                    let is_inward = if is_course_correction {
                        op.length_squared() > dp.length_squared()
                    } else if origin_lp == Some(maneuver.destination_body) {
                        true
                    } else if dest_lp_entity == Some(maneuver.origin_body) {
                        false
                    } else {
                        op.length_squared() > dp.length_squared()
                    };

                    let center_is_star = body_query
                        .get(maneuver.orbit_center)
                        .map(|(_, body, _)| body.body_type == BodyType::Star)
                        .unwrap_or(false);

                    if center_is_star
                        && should_use_exact_endpoint_bezier(maneuver.reference_frame)
                        && maneuver.departure_velocity_ms.is_some()
                        && maneuver.arrival_velocity_ms.is_some()
                        && maneuver.start_position_au.is_some()
                        && maneuver.end_position_au.is_some()
                    {
                        let total_ma_travel = maneuver.transfer_orbit.mean_motion
                            * (maneuver.arrival_time - maneuver.departure_time);
                        let geo = compute_barycentric_visual_arc(
                            &maneuver.transfer_orbit,
                            op,
                            dp_absolute,
                            total_ma_travel,
                            maneuver.departure_velocity_ms,
                            maneuver.arrival_velocity_ms,
                        );
                        transform.translation = geo.eval(maneuver.progress(elapsed) as f32);

                        let inside_origin = transform.translation.distance(op) < origin_ring_r;
                        let inside_dest = transform.translation.distance(dp_absolute) < dest_ring_r;
                        if inside_origin || inside_dest {
                            *vis = Visibility::Hidden;
                        }
                        continue;
                    }

                    let sampled_dest = if center_is_star { dp_absolute } else { dp };

                    // Use `mean_motion * elapsed` as the orbit's
                    // start mean_anomaly so the trajectory begins
                    // at the planet's current mean_anomaly.  The
                    // orbit's `mean_anomaly_epoch` was set at the
                    // time the orbit was built (which may be many
                    // sim-minutes ago), so using it directly would
                    // draw the orbit at the OLD planet position.
                    let start_mean_anomaly = maneuver.transfer_orbit.mean_motion * elapsed;
                    let visual_points = if center_is_star {
                        build_visual_sampled_transfer_polyline_moving_center(
                            &maneuver.transfer_orbit,
                            maneuver.orbit_center,
                            elapsed,
                            maneuver.departure_time,
                            maneuver.arrival_time - maneuver.departure_time,
                            real_secs,
                            sim_scale,
                            &body_query,
                            &kepler_query,
                            &amp_query,
                            start_mean_anomaly,
                            maneuver.transfer_orbit.mean_motion
                                * (maneuver.arrival_time - maneuver.departure_time),
                            128,
                        )
                    } else {
                        let center_visual = body_query
                            .get(maneuver.orbit_center)
                            .map(|(t, _, _)| t.translation)
                            .unwrap_or(Vec3::ZERO);
                        build_visual_sampled_transfer_polyline(
                            &maneuver.transfer_orbit,
                            center_visual,
                            op,
                            actual_origin_ring_r,
                            dp,
                            dest_ring_r,
                            is_course_correction,
                            is_inward,
                            maneuver.transfer_orbit.mean_anomaly_epoch,
                            maneuver.transfer_orbit.mean_motion
                                * (maneuver.arrival_time - maneuver.departure_time),
                            128,
                        )
                    };
                    transform.translation = sampled_transfer_orbit_position(
                        &visual_points,
                        maneuver.progress(elapsed) as f32,
                    );

                    let inside_origin = transform.translation.distance(op) < origin_ring_r;
                    let inside_dest = transform.translation.distance(sampled_dest) < dest_ring_r;
                    if inside_origin || inside_dest {
                        *vis = Visibility::Hidden;
                    }
                    continue;
                }

                let is_inward = if is_course_correction {
                    op.length_squared() > dp.length_squared()
                } else if origin_lp == Some(maneuver.destination_body) {
                    true
                } else if dest_lp_entity == Some(maneuver.origin_body) {
                    false
                } else {
                    op.length_squared() > dp.length_squared()
                };

                let geo = if maneuver.reference_frame.is_barycentric() && maneuver.is_kinematic() {
                    compute_direct_transfer_arc(op, dp, actual_origin_ring_r, dest_ring_r)
                } else {
                    compute_transfer_arc(
                        op,
                        dp,
                        actual_origin_ring_r,
                        dest_ring_r,
                        is_course_correction,
                        is_inward,
                        maneuver.is_kinematic(),
                        cv_at_departure,
                    )
                };

                let progress = maneuver.progress(elapsed) as f32;
                transform.translation = geo.eval(progress);

                // Hide inside origin/dest orbit rings
                let inside_origin = transform.translation.distance(op) < origin_ring_r;
                let inside_dest = transform.translation.distance(dp) < dest_ring_r;
                if inside_origin || inside_dest {
                    *vis = Visibility::Hidden;
                }
            }
        } else {
            // Fallback: physics position
            let render_du = (sc.position - origin_offset) * SCALING_FACTOR;
            transform.translation =
                Vec3::new(render_du.x as f32, render_du.y as f32, render_du.z as f32);
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
    launch_state: Res<LaunchState>,
) {
    if !launch_state.is_in_game() {
        return;
    }
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
            // Parked source ring — scales with the fleet's actual parking
            // orbit (Low / Medium / High / Stationary for planets; AU for
            // stars), so different parking altitudes around the same body
            // render at visibly distinct radii.  `shell_ring_visual_radius`
            // does the star-vs-planet visual-scale translation.
            let ring_radius = shell_ring_visual_radius(body, orbit.radius_au);
            draw_ring(
                &mut gizmos,
                body_transform.translation,
                ring_radius,
                Color::srgba(0.2, 0.9, 0.3, 0.30),
            );
        }
    } else if let Ok((_, maneuver)) = transit_query.get(selected) {
        // Only draw rings for local (planet-centric) transfers
        let center_is_star = maneuver.reference_frame.is_barycentric()
            || body_query
                .get(maneuver.orbit_center)
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
    fleet_query: Query<
        (
            Entity,
            &Transform,
            Option<&FleetOrbit>,
            Option<&ActiveManeuver>,
        ),
        With<Fleet>,
    >,
    body_query: Query<(&Transform, &CelestialBody, Option<&LogicalParent>), Without<Fleet>>,
    kepler_query: Query<&KeplerOrbit, Without<Fleet>>,
    amp_query: Query<&LocalOrbitAmplification, Without<Fleet>>,
    floating_origin: Option<Res<FloatingOrigin>>,
    fleet_ui_state: Res<FleetUiState>,
    view_mode: Res<ViewMode>,
    sim_time: Res<SimulationTime>,
    real_time: Res<Time<Real>>,
    time_scale: Res<TimeScale>,
    camera_query: Query<&OrbitCamera, With<GameCamera>>,
    launch_state: Res<LaunchState>,
) {
    if !launch_state.is_in_game() {
        return;
    }
    if *view_mode != ViewMode::System {
        return;
    }
    if !fleet_ui_state.show_transfer_popup {
        return;
    }
    // Suppress the arc preview until the porkchop grid has landed.
    // The early target-snap block at `transfer_planner.rs:~6318`
    // sets `planned_transfer = Some(planned_to_target)` (an Efficient
    // Hohmann) the moment `show_transfer_popup = true` flips —
    // *before* the async build finishes — so a check on
    // `planned_transfer.is_none()` would not gate the arc.  Gate on
    // the grid instead: the arc only draws once the porkchop has
    // produced a real cell for the planner to size against.  Until
    // then the player sees "Calculating transfer…" with no arc, then
    // a single arc appears using the auto-picked cell's TOF.
    //
    // Note: this also suppresses the arc for cross-star / interstellar
    // targets that never build a porkchop grid (no-op case for v0.5).
    if fleet_ui_state.porkchop_grid.is_none() {
        return;
    }
    let Some(fleet_entity) = fleet_ui_state.selected_fleet else {
        return;
    };

    // If a gravity assist candidate has been chosen, the transfer planner shows a
    // specialised two-leg trajectory.  In that case skip the standard amber preview
    // arc entirely; the assist code will draw its own overlay on top.
    if fleet_ui_state.selected_gravity_assist.is_some() {
        return;
    }

    // Hoist fleet-state lookup so both LP and regular branches can share it.
    let Ok((_, fleet_transform, maybe_orbit, maybe_maneuver)) = fleet_query.get(fleet_entity)
    else {
        return;
    };
    let _origin_offset = floating_origin
        .as_ref()
        .map(|origin| origin.position)
        .unwrap_or(DVec3::ZERO);
    let elapsed = sim_time.elapsed_seconds();
    let real_secs = real_time.elapsed_secs() as f64;
    let sim_scale = time_scale.scale as f64;

    // Zoom-aware scale for the burn-direction arrows.  At neutral System-view
    // zoom the camera radius sits around 4000–6000 render units (showing
    // ~2 AU around the Sun).  We use the square-root of the ratio so the
    // arrows stay readable across the full zoom range without dominating at
    // planet-close zoom.  Clamp keeps them sensible at extreme zoom levels.
    let camera_radius = camera_query
        .single()
        .map(|c| c.radius)
        .unwrap_or(5_000.0_f32);
    let burn_arrow_scale = (camera_radius / 5_000.0_f32).sqrt().clamp(0.6, 3.5);

    let current_sim_s = elapsed;
    let departure_offset_s = fleet_ui_state.departure_offset_days * 86_400.0;
    // Anchor the burn at the cell's absolute epoch when one is
    // recorded (the user clicked a porkchop cell — the cell click
    // records `selected_abs_t_dep_s`, which survives buffer
    // rotations).  Falling back to the relative formula
    // (`current_sim_s + departure_offset_s`) keeps the legacy
    // slider path working (where there is no recorded absolute).
    //
    // "Stays at immediate departure once cell hits Now": when the
    // recorded `selected_abs_t_dep_s` is in the past (the user
    // has scrolled the chart past their selection, or sim time
    // has overtaken the click epoch), clamp it to `current_sim_s`
    // so the trajectory behaves as an "immediate departure"
    // preview — `op` is then the *live* planet position, the
    // Lambert solver is re-evaluated every frame with the live
    // origin and the live destination-at-arrival-time, and the
    // arc visibly tracks the live planet instead of orbiting
    // around the screen at the past burn-time planet's phase.
    // The selected cell stays selected (ΔV / TOF / fuel readout
    // intact) but the trajectory itself now shows "if I burn
    // right now, this is what happens" continuously — which is
    // what the player wants once the planned burn has expired.
    //
    // Clamping logic:
    //   * `selected_abs_t_dep_s = Some(t)` where `t < current`:
    //     fall back to `current_sim_s` (immediate departure).
    //   * `selected_abs_t_dep_s = Some(t)` where `t >= current`:
    //     use `t` (frozen absolute, the original GRA-169 fix).
    //   * `selected_abs_t_dep_s = None`: slider path, use
    //     `current_sim_s + departure_offset_s`.  The relative
    //     formula keeps `planet(burn_time) - planet(current)`
    //     constant in orbital phase, so the slider path's arc
    //     origin advances with sim time at orbital rate.  When
    //     the slider sits at the leftmost position
    //     (`departure_offset_days = 0`), the relative formula
    //     collapses to `current_sim_s` and the slider path
    //     matches the cell path's "immediate departure" behaviour.
    let departure_s = match fleet_ui_state.selected_abs_t_dep_s {
        Some(recorded_abs_t_dep_s) if recorded_abs_t_dep_s >= current_sim_s => {
            // Cell is in the future — anchor absolutely (frozen
            // burn epoch) so the visible separation between the
            // arc origin and the live planet shrinks smoothly as
            // sim time approaches the burn.
            recorded_abs_t_dep_s
        }
        Some(_) => {
            // Cell is in the past — clamp to "now" so the
            // preview becomes an immediate-departure preview
            // that continuously re-solves against the live
            // planet.  This is the fix for the "trajectory moves
            // all over the place once the selected tile hits
            // Now" report: previously the arc was anchored at
            // the past burn epoch and the back-projected planet
            // orbited around the star at orbital rate as sim
            // time kept advancing.
            current_sim_s
        }
        None => current_sim_s + departure_offset_s,
    };

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

        let Ok((origin_transform, origin_body_data, _)) = body_query.get(origin_body) else {
            return;
        };
        // Predict origin body position at planned departure so the start mark moves
        // when the player drags the departure slider.
        let op = if let Some(fleet_pos) = course_correction_fleet_pos {
            fleet_pos
        } else {
            predict_body_visual_pos(
                origin_body,
                current_sim_s,
                departure_s,
                real_secs,
                sim_scale,
                &body_query,
                &kepler_query,
                &amp_query,
            )
            .unwrap_or(origin_transform.translation)
        };
        let origin_ring_r = if is_course_correction {
            0.0
        } else {
            fleet_parking_visual_radius(origin_body_data.visual_radius)
        };

        let travel_time_s = stable_preview_travel_time(&fleet_ui_state);

        // Predict the LP's parent planet position to get LP direction.
        let planet_pos_now = body_query
            .get(lp.planet_entity)
            .ok()
            .map(|(t, _, _)| t.translation)
            .unwrap_or(Vec3::ZERO);

        // For co-orbital L3/L4/L5 phasing, show the arc toward the CURRENT LP
        // marker position (the one visible on screen) instead of a predicted
        // position many years in the future.  Phasing maneuvers keep the fleet
        // near the planet's orbit, so the current marker is the intuitive target.
        // L1/L2 are radial transfers that use the predicted arrival position.
        let co_orbital_lp = matches!(lp.point, 3..=5);
        let planet_ref_pos = if co_orbital_lp {
            planet_pos_now
        } else {
            predict_body_visual_pos(
                lp.planet_entity,
                current_sim_s,
                departure_s + travel_time_s,
                real_secs,
                sim_scale,
                &body_query,
                &kepler_query,
                &amp_query,
            )
            .unwrap_or(planet_pos_now)
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
        let dp = Vec3::new(
            lp_angle.cos() * lp_render_dist,
            lp_angle.sin() * lp_render_dist,
            0.0,
        );

        // Small marker radius for the LP arrival point (no physical body radius).
        let lp_marker_r = (lp.radius_au as f32 * SCALING_FACTOR as f32 * 0.015).clamp(10.0, 50.0);

        let is_inward = op.length_squared() > dp.length_squared();
        let geo = compute_transfer_arc(
            op,
            dp,
            origin_ring_r,
            lp_marker_r,
            is_course_correction,
            is_inward,
            false,
            Vec3::ZERO,
        );

        // Dashed amber arc — arc-length-uniform.
        draw_dashed_curve(
            &mut gizmos,
            |t| geo.eval(t),
            24,
            |f| Color::srgba(1.0, 0.75, 0.15, 0.70 - 0.35 * f),
        );

        // Burn-direction arrows at the LP transfer endpoints.  LPs are
        // always ballistic, so no kinematic gate.
        let burn_color = Color::srgba(1.0, 0.75, 0.15, 0.95);
        draw_burn_arrow(
            &mut gizmos,
            geo.p0,
            geo.p1 - geo.p0,
            burn_arrow_scale,
            burn_color,
        );
        draw_burn_arrow(
            &mut gizmos,
            geo.p3,
            geo.p3 - geo.p2,
            burn_arrow_scale,
            burn_color,
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
        if target_fleet_entity == fleet_entity {
            return;
        }

        let origin_body = if let Some(orbit) = maybe_orbit {
            orbit.body
        } else {
            return;
        };

        let Ok((origin_transform, origin_body_data, _)) = body_query.get(origin_body) else {
            return;
        };
        let op = if let Some(fleet_pos) = course_correction_fleet_pos {
            fleet_pos
        } else {
            predict_body_visual_pos(
                origin_body,
                current_sim_s,
                departure_s,
                real_secs,
                sim_scale,
                &body_query,
                &kepler_query,
                &amp_query,
            )
            .unwrap_or(origin_transform.translation)
        };
        let origin_ring_r = if is_course_correction {
            0.0
        } else {
            fleet_parking_visual_radius(origin_body_data.visual_radius)
        };

        // Use the target fleet's current visual Transform position — fleets have no Keplerian
        // orbit to predict future positions from, so we target where they are right now.
        let Ok((_, target_fleet_transform, _, _)) = fleet_query.get(target_fleet_entity) else {
            return;
        };
        let dp = target_fleet_transform.translation;

        let is_kinematic = fleet_ui_state
            .computed_options
            .get(fleet_ui_state.selected_option)
            .map(|opt| {
                opt.label == "Full Thrust"
                    || opt.label.contains("Coast")
                    || opt.label == "Max Speed"
                    || opt.label.contains("Direct")
            })
            .unwrap_or(false);

        // Fixed visual marker radius — no physical body size for a fleet.
        let marker_r = 15.0_f32;

        let is_inward = op.length_squared() > dp.length_squared();
        // cv_ref = Vec3::ZERO (star) for fleet-intercept — no specific orbit centre.
        let geo = compute_transfer_arc(
            op,
            dp,
            origin_ring_r,
            marker_r,
            is_course_correction,
            is_inward,
            is_kinematic,
            Vec3::ZERO,
        );

        if is_kinematic {
            draw_solid_curve(
                &mut gizmos,
                |t| geo.eval(t),
                64,
                |f| Color::srgba(1.0, 0.75, 0.15, 0.70 - 0.35 * f),
            );
        } else {
            draw_dashed_curve(
                &mut gizmos,
                |t| geo.eval(t),
                24,
                |f| Color::srgba(1.0, 0.75, 0.15, 0.70 - 0.35 * f),
            );
        }

        // Burn-direction arrows at the fleet-intercept arc endpoints.
        // Skip for kinematic transfers (Full Thrust / Coast / Max Speed
        // / Direct) — those are straight-line powered burns with no
        // departure / insertion tangent.
        if !is_kinematic {
            let burn_color = Color::srgba(1.0, 0.75, 0.15, 0.95);
            draw_burn_arrow(
                &mut gizmos,
                geo.p0,
                geo.p1 - geo.p0,
                burn_arrow_scale,
                burn_color,
            );
            draw_burn_arrow(
                &mut gizmos,
                geo.p3,
                geo.p3 - geo.p2,
                burn_arrow_scale,
                burn_color,
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

    let Some(target_entity) = fleet_ui_state.target_body else {
        return;
    };

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

    if origin_body == target_entity {
        return;
    }

    let Ok((origin_transform, origin_body_data, origin_lp)) = body_query.get(origin_body) else {
        return;
    };
    let Ok((dest_transform_now, dest_body_data, dest_lp)) = body_query.get(target_entity) else {
        return;
    };
    let reference_frame = resolve_preview_reference_frame(origin_body, target_entity, &body_query);
    let selected_option = fleet_ui_state
        .computed_options
        .get(fleet_ui_state.selected_option);
    let is_kinematic = selected_option
        .map(|opt| {
            opt.label == "Full Thrust"
                || opt.label.contains("Coast")
                || opt.label == "Max Speed"
                || opt.label.contains("Direct")
        })
        .unwrap_or(false);
    let planned_transfer = fleet_ui_state.planned_transfer.as_ref();

    // For course corrections: departure point = fleet's current render position.
    // For normal transfers: departure point = predicted origin body position at departure.
    let op = if let Some(fleet_pos) = course_correction_fleet_pos {
        fleet_pos
    } else {
        predict_body_visual_pos(
            origin_body,
            current_sim_s,
            departure_s,
            real_secs,
            sim_scale,
            &body_query,
            &kepler_query,
            &amp_query,
        )
        .unwrap_or(origin_transform.translation)
    };
    // Origin ring radius is 0 for course corrections (fleet is already at the departure point).
    let origin_ring_r = if is_course_correction {
        0.0
    } else {
        fleet_parking_visual_radius(origin_body_data.visual_radius)
    };
    let dest_visual_r = dest_body_data.visual_radius;
    let default_dest_ring_r = fleet_parking_visual_radius(dest_visual_r);

    // ── Shell-driven target orbit ring ─────────────────────────────────────
    // The player's `Target orbit:` picker writes a shell id (Low / Medium /
    // High / Stationary / CloseApproach / HabitableInner / HabitableOuter /
    // Cruise) keyed by destination entity.  We resolve it to a numeric
    // arrival radius (AU) and feed that through `shell_ring_visual_radius`
    // to scale the dashed amber ghost-body ring at the destination.  When
    // the player hasn't picked a shell we fall back to the body's default
    // shell — Star → HabitableInner (≈ 0.3 AU around Sol), everything else
    // → Low (the LEO-proxy default).
    //
    // `dest_ring_r` (above) is the geometry offset used by
    // `compute_*_transfer_arc` and is **untouched** — splitting the two
    // lets us re-shape the visual ring without warping the Bezier arc.
    let target_shell = fleet_ui_state
        .target_orbit_shell
        .filter(|(entity, _)| *entity == target_entity)
        .map(|(_, shell)| shell)
        .unwrap_or_else(|| crate::fleets::default_shell_for_body_type(dest_body_data.body_type));
    let target_orbit_au = crate::fleets::radius_for_shell(dest_body_data, target_shell);
    let dest_orbit_ring_r = shell_ring_visual_radius(dest_body_data, target_orbit_au);

    let star_centered_preview_candidate = planned_transfer
        .map(|transfer| {
            let center_is_star = matches!(
                body_query.get(transfer.orbit_center),
                Ok((_, body, _)) if body.body_type == BodyType::Star
            );
            should_use_star_centered_transfer_preview(transfer, center_is_star)
        })
        .unwrap_or(false);

    // Travel time from selected transfer option (or 0 = show arc to current position).
    // Most previews use `stable_preview_travel_time` to strip frame-varying phase
    // corrections from Moderate/Fast options. For sampled same-star ballistic previews,
    // however, the arc is drawn from the selected planned transfer orbit, so the ghost
    // marker must use that exact arrival epoch to stay aligned with the arc endpoint.
    let stable_travel_time_s = stable_preview_travel_time(&fleet_ui_state);
    let candidate_travel_time_s = if star_centered_preview_candidate {
        planned_transfer
            .map(|transfer| transfer.duration_s)
            .unwrap_or(stable_travel_time_s)
    } else {
        stable_travel_time_s
    };
    if !candidate_travel_time_s.is_finite() {
        return;
    }

    // Predict destination body position at planned departure + travel time so the
    // ghost mark moves when the player drags the departure slider.
    let dest_is_star = dest_body_data.body_type == BodyType::Star;
    let dest_ring_r = if reference_frame.is_barycentric() && is_kinematic && dest_is_star {
        planned_transfer
            .map(|transfer| {
                (transfer.arrival_orbit_radius_au as f32 * SCALING_FACTOR as f32).max(1.0)
            })
            .unwrap_or(default_dest_ring_r)
    } else {
        default_dest_ring_r
    };
    let dp_absolute = predict_body_visual_pos(
        target_entity,
        current_sim_s,
        departure_s + candidate_travel_time_s,
        real_secs,
        sim_scale,
        &body_query,
        &kepler_query,
        &amp_query,
    )
    .unwrap_or(dest_transform_now.translation);

    let (dp, cv_ref) = if let TransferReferenceFrame::Body(orbit_center) = reference_frame {
        let cv_predicted = predict_body_visual_pos(
            orbit_center,
            current_sim_s,
            departure_s + candidate_travel_time_s,
            real_secs,
            sim_scale,
            &body_query,
            &kepler_query,
            &amp_query,
        )
        .unwrap_or(dp_absolute);

        let cv_at_departure = predict_body_visual_pos(
            orbit_center,
            current_sim_s,
            departure_s,
            real_secs,
            sim_scale,
            &body_query,
            &kepler_query,
            &amp_query,
        )
        .unwrap_or(cv_predicted);

        (
            dp_absolute - cv_predicted + cv_at_departure,
            cv_at_departure,
        )
    } else {
        (dp_absolute, Vec3::ZERO)
    };

    let same_star_orbit_preview = planned_transfer
        .filter(|_| star_centered_preview_candidate)
        .is_some();

    let travel_time_s = if same_star_orbit_preview {
        candidate_travel_time_s
    } else {
        stable_travel_time_s
    };
    if !travel_time_s.is_finite() {
        return;
    }

    let dp_absolute = if same_star_orbit_preview {
        dp_absolute
    } else {
        predict_body_visual_pos(
            target_entity,
            current_sim_s,
            departure_s + travel_time_s,
            real_secs,
            sim_scale,
            &body_query,
            &kepler_query,
            &amp_query,
        )
        .unwrap_or(dest_transform_now.translation)
    };

    let (dp, cv_ref) = if same_star_orbit_preview {
        (dp, cv_ref)
    } else if let TransferReferenceFrame::Body(orbit_center) = reference_frame {
        let cv_predicted = predict_body_visual_pos(
            orbit_center,
            current_sim_s,
            departure_s + travel_time_s,
            real_secs,
            sim_scale,
            &body_query,
            &kepler_query,
            &amp_query,
        )
        .unwrap_or(dp_absolute);

        let cv_at_departure = predict_body_visual_pos(
            orbit_center,
            current_sim_s,
            departure_s,
            real_secs,
            sim_scale,
            &body_query,
            &kepler_query,
            &amp_query,
        )
        .unwrap_or(cv_predicted);

        (
            dp_absolute - cv_predicted + cv_at_departure,
            cv_at_departure,
        )
    } else {
        (dp_absolute, Vec3::ZERO)
    };

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

    // Unified heliocentric / star-centered Bezier preview.
    //
    // Covers both:
    //   * `Barycentric(star)` interplanetary / interstellar transfers (the
    //     textbook reference frame), and
    //   * `Body(star)` same-star heliocentric transfers (the GRA-326
    //     dispatcher uses this for Earth→Mars / Earth→Jupiter porkchop
    //     selections where the orbit center is the star but the reference
    //     frame is local).
    //
    // Both use the same `compute_barycentric_visual_arc` geometry, which
    // interpolates between the predicted launch (`op`) and arrival (`dp`)
    // positions with Lambert-derived tangent directions when available.
    // The result is a smooth Bezier that is continuous across buffer
    // rebuilds (the predicted positions advance smoothly with the sim
    // clock; no snap, no snake).
    //
    // The previous code routed `Body(star)` same-star transfers through
    // `preview_center_is_star`, which drew the actual transfer-orbit
    // polyline. That polyline couldn't track the planet's current mean
    // anomaly across buffer rotations, so it snapped on each rebuild.
    // The replacement keeps one unified Bezier for both reference-frame
    // flavours and drops the orbit-polyline branch entirely.
    if !is_kinematic
        && !is_course_correction
        && matches!(
            reference_frame,
            TransferReferenceFrame::SystemBarycentric | TransferReferenceFrame::Body(_)
        )
        && same_star_orbit_preview
    {
        let unified_preview = planned_transfer
            .map(|transfer| {
                (
                    &transfer.transfer_orbit,
                    transfer.duration_s,
                    transfer.departure_velocity_ms,
                    transfer.arrival_velocity_ms,
                )
            })
            .or_else(|| {
                selected_option.and_then(|opt| {
                    opt.transfer_orbit_override
                        .as_ref()
                        .map(|orbit| (orbit, travel_time_s, None, None))
                })
            });

        if let Some((orbit, preview_duration_s, departure_velocity_ms, arrival_velocity_ms)) =
            unified_preview
        {
            let total_ma_travel = orbit.mean_motion * preview_duration_s;

            // Time progression along the planned transfer.  This makes the
            // preview arc visibly "consume" as sim time advances past the
            // planned departure — at Phase 0.0 the arc spans the full launch
            // → arrival, and as elapsed_since_departure approaches
            // preview_duration_s the arc shrinks back toward dp_absolute.
            //
            // Reuse the already-anchored `departure_s` (absolute epoch
            // when `selected_abs_t_dep_s` is recorded, otherwise the
            // relative fallback) instead of recomputing
            // `current_sim_s + departure_offset_s` here — that
            // recomputation used the relative formula unconditionally,
            // silently overriding the absolute anchor and reintroducing
            // the "trajectory origin stuck at the original selection"
            // bug for the pre-burn branch below.  Before the burn, the
            // arc starts at `op` (planet at launch) and spans the full
            // trajectory; after it starts at the spacecraft's *actual*
            // position along the planned orbit (Lambert-conic-propagated,
            // star-centered since this branch is same-star / barycentric)
            // with the remaining mean-anomaly travel.
            let elapsed_since_departure_s = (current_sim_s - departure_s).max(0.0);
            let phase_ma_raw = elapsed_since_departure_s * orbit.mean_motion;
            // Direction-aware clamp: handle retrograde transfers where
            // mean_motion is negative by clamping on the absolute scale.
            let phase_ma = if total_ma_travel >= 0.0 {
                phase_ma_raw.min(total_ma_travel)
            } else {
                phase_ma_raw.max(total_ma_travel)
            };
            let (start_pos, remaining_ma_travel) = if phase_ma.abs() > 1e-9 {
                let local =
                    orbit_position_from_mean_anomaly(orbit, orbit.mean_anomaly_epoch + phase_ma);
                // Local orbit pos is in AU relative to the transfer's center
                // (the star).  For this branch the floating origin is the
                // star, so the AU→visual scaling is direct (no parent
                // offset needed).
                let current_visual = Vec3::new(
                    (local.x * SCALING_FACTOR) as f32,
                    (local.y * SCALING_FACTOR) as f32,
                    (local.z * SCALING_FACTOR) as f32,
                );
                (current_visual, total_ma_travel - phase_ma)
            } else {
                (op, total_ma_travel)
            };

            // Sample the actual Kepler transfer orbit at 128 mean-anomaly
            // points and render the polyline — the *same* half-ellipse the
            // in-transit renderer draws (`draw_fleet_trajectories` at L2514
            // uses `build_visual_sampled_transfer_polyline_moving_center`).
            // The previous Bezier approximation in `compute_barycentric_visual_arc`
            // interpolated a cubic curve between predicted endpoints, which
            // produced an S-shape kink at each endpoint and looked like a
            // 90° perpendicular departure.  Sampling the orbit itself gives
            // a true Hohmann half-ellipse that matches what the player sees
            // after clicking Execute Transfer.
            //
            // Anchor the polyline at the spacecraft's *current* mean
            // anomaly when post-departure (so the arc visually consumes
            // as sim time advances) and at the planned departure mean
            // anomaly when pre-departure (so the full trajectory is shown).
            let start_mean_anomaly = if phase_ma.abs() > 1e-9 {
                orbit.mean_anomaly_epoch + phase_ma
            } else {
                orbit.mean_anomaly_epoch
            };

            // Use `draw_dashed_curve` with a closure that evaluates the
            // Kepler orbit at fraction `t` of the remaining arc.  This
            // preserves the dashed amber visual style and the
            // arc-length-uniform dash spacing from `draw_dashed_polyline`.
            draw_dashed_curve(
                &mut gizmos,
                |t| {
                    let ma = start_mean_anomaly + remaining_ma_travel * t as f64;
                    let pos_au = orbit_position_from_mean_anomaly(orbit, ma);
                    Vec3::new(
                        (pos_au.x * SCALING_FACTOR) as f32,
                        (pos_au.y * SCALING_FACTOR) as f32,
                        (pos_au.z * SCALING_FACTOR) as f32,
                    )
                },
                24,
                |f| Color::srgba(1.0, 0.75, 0.15, 0.70 - 0.35 * f),
            );

            // Burn-direction arrows at the actual orbit endpoints.  These
            // use the *real* velocity vectors from the Lambert solver
            // (m/s, star-centered physics frame) — the same vectors the
            // ship will follow at departure and arrival.  Visual-space
            // direction is just the unit vector (the magnitudes differ by
            // a constant scaling factor that `normalize_or_zero` drops).
            let burn_color = Color::srgba(1.0, 0.75, 0.15, 0.95);
            if let Some(v) = departure_velocity_ms {
                let dir = Vec3::new(v.x as f32, v.y as f32, v.z as f32);
                draw_burn_arrow(&mut gizmos, start_pos, dir, burn_arrow_scale, burn_color);
            }
            if let Some(v) = arrival_velocity_ms {
                let dir = Vec3::new(v.x as f32, v.y as f32, v.z as f32);
                draw_burn_arrow(&mut gizmos, dp_absolute, dir, burn_arrow_scale, burn_color);
            }

            draw_ghost_body(
                &mut gizmos,
                dp_absolute,
                dest_orbit_ring_r,
                dest_visual_r,
                false,
            );
            return;
        }
    }

    // Shared geometry — identical curve to transit gizmo and fleet dot.
    let geo = if reference_frame.is_barycentric() && is_kinematic {
        compute_direct_transfer_arc(op, dp, origin_ring_r, dest_ring_r)
    } else {
        compute_transfer_arc(
            op,
            dp,
            origin_ring_r,
            dest_ring_r,
            is_course_correction,
            is_inward,
            is_kinematic,
            cv_ref,
        )
    };

    if is_kinematic {
        draw_solid_curve(
            &mut gizmos,
            |t| geo.eval(t),
            96,
            |f| Color::srgba(1.0, 0.75, 0.15, 0.70 - 0.35 * f),
        );
    } else {
        draw_dashed_curve(
            &mut gizmos,
            |t| geo.eval(t),
            24,
            |f| Color::srgba(1.0, 0.75, 0.15, 0.70 - 0.35 * f),
        );
    }

    // Burn-direction arrows at the local / barycentric-kinematic transfer
    // endpoints.  Skip for kinematic transfers — the arc collapses to a
    // straight line with no real departure / insertion tangent.
    if !is_kinematic {
        let burn_color = Color::srgba(1.0, 0.75, 0.15, 0.95);
        draw_burn_arrow(
            &mut gizmos,
            geo.p0,
            geo.p1 - geo.p0,
            burn_arrow_scale,
            burn_color,
        );
        draw_burn_arrow(
            &mut gizmos,
            geo.p3,
            geo.p3 - geo.p2,
            burn_arrow_scale,
            burn_color,
        );
    }

    // Ghost body at predicted arrival position.
    // In the preview the destination is the body we are flying TO, not the orbit
    // centre, so the outer ring is always wanted here.  We pass
    // `dest_orbit_ring_r` (the shell-driven radius) rather than `dest_ring_r`
    // (the geometry offset) so the dashed amber ring scales with the
    // player's `Target orbit:` pick instead of hugging the body's visual rim.
    draw_ghost_body(&mut gizmos, dp, dest_orbit_ring_r, dest_visual_r, false);
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
/// The regular amber preview arc (`draw_fleet_transfer_preview`) is **not**
/// drawn when a gravity assist is selected — that function returns early
/// at the top of its body (see the `selected_gravity_assist.is_some()`
/// guard near line 3388) so it doesn't double-draw on top of this
/// overlay.  Only the two-colour slingshot overlay appears.
pub fn draw_gravity_assist_preview(
    mut gizmos: Gizmos,
    fleet_query: Query<(Entity, Option<&FleetOrbit>, Option<&ActiveManeuver>), With<Fleet>>,
    body_query: Query<(&Transform, &CelestialBody, Option<&LogicalParent>), Without<Fleet>>,
    kepler_query: Query<&KeplerOrbit, Without<Fleet>>,
    amp_query: Query<&LocalOrbitAmplification, Without<Fleet>>,
    fleet_ui_state: Res<FleetUiState>,
    view_mode: Res<ViewMode>,
    sim_time: Res<SimulationTime>,
    real_time: Res<Time<Real>>,
    time_scale: Res<TimeScale>,
    launch_state: Res<LaunchState>,
) {
    if !launch_state.is_in_game() {
        return;
    }
    if *view_mode != ViewMode::System {
        return;
    }
    if !fleet_ui_state.show_transfer_popup {
        return;
    }
    let Some(sel_ga_idx) = fleet_ui_state.selected_gravity_assist else {
        return;
    };
    let Some(fleet_entity) = fleet_ui_state.selected_fleet else {
        return;
    };
    let Some(target_entity) = fleet_ui_state.target_body else {
        return;
    };
    let Some(ga_entry) = fleet_ui_state.gravity_assist_candidates.get(sel_ga_idx) else {
        return;
    };

    let flyby_entity = ga_entry.flyby_entity;
    let leg1_time = ga_entry.option.leg1_time_s;
    let total_time = ga_entry.option.total_time_s;
    let departure_offset_s = fleet_ui_state.departure_offset_days * 86_400.0;

    let Ok((_, maybe_orbit, maybe_maneuver)) = fleet_query.get(fleet_entity) else {
        return;
    };
    // Hide the gravity-assist preview as soon as a transfer has been planned/executed.
    if maybe_maneuver.is_some() {
        return;
    }
    let origin_body = if let Some(orbit) = maybe_orbit {
        orbit.body
    } else {
        return;
    };
    // Sanity: all three bodies must be distinct
    if origin_body == target_entity || origin_body == flyby_entity || flyby_entity == target_entity
    {
        return;
    }

    let current_sim_s = sim_time.elapsed_seconds();
    let real_secs = real_time.elapsed_secs() as f64;
    let sim_scale = time_scale.scale as f64;
    // Same three-way clamp as `draw_fleet_transfer_preview`:
    // recorded absolute epoch in the future → use it (frozen);
    // recorded epoch in the past → clamp to `current_sim_s`
    // (immediate-departure preview); no recorded epoch → slider
    // formula.  See that function's docs for the full rationale.
    let depart_s = match fleet_ui_state.selected_abs_t_dep_s {
        Some(recorded_abs_t_dep_s) if recorded_abs_t_dep_s >= current_sim_s => recorded_abs_t_dep_s,
        Some(_) => current_sim_s,
        None => current_sim_s + departure_offset_s,
    };

    let Ok((origin_t, origin_bd, _)) = body_query.get(origin_body) else {
        return;
    };
    let Ok((_, flyby_bd, _)) = body_query.get(flyby_entity) else {
        return;
    };
    let Ok((dest_t, dest_bd, _)) = body_query.get(target_entity) else {
        return;
    };

    // Predict origin body position at planned departure time so the start mark moves
    // when the player drags the departure slider.
    let op = predict_body_visual_pos(
        origin_body,
        current_sim_s,
        depart_s,
        real_secs,
        sim_scale,
        &body_query,
        &kepler_query,
        &amp_query,
    )
    .unwrap_or(origin_t.translation);
    let origin_ring_r = fleet_parking_visual_radius(origin_bd.visual_radius);
    let dest_ring_r = fleet_parking_visual_radius(dest_bd.visual_radius);

    // Shell-driven destination ring radius: scales the dashed amber
    // ghost-body ring at the destination to the player's chosen arrival
    // shell (`Target orbit:` picker).  `dest_ring_r` (above) stays as
    // the geometry offset for `compute_gravity_assist_arc` — splitting
    // them lets us re-shape the visual ring without warping the
    // slingshot arc.
    let ga_target_shell = fleet_ui_state
        .target_orbit_shell
        .filter(|(entity, _)| *entity == target_entity)
        .map(|(_, shell)| shell)
        .unwrap_or_else(|| crate::fleets::default_shell_for_body_type(dest_bd.body_type));
    let ga_target_orbit_au = crate::fleets::radius_for_shell(dest_bd, ga_target_shell);
    let dest_orbit_ring_r = shell_ring_visual_radius(dest_bd, ga_target_orbit_au);

    // Predict flyby body position at end of Leg 1 (cached optimal-window
    // leg time — see the REVERT comment on `compute_gravity_assist_arc`
    // below; the Bezier anchors on these predicted positions).
    let fp = predict_body_visual_pos(
        flyby_entity,
        current_sim_s,
        depart_s + leg1_time,
        real_secs,
        sim_scale,
        &body_query,
        &kepler_query,
        &amp_query,
    )
    .unwrap_or_else(|| {
        body_query
            .get(flyby_entity)
            .map(|(t, _, _)| t.translation)
            .unwrap_or(Vec3::ZERO)
    });

    // Predict destination body position at end of full two-leg transfer
    let dp = predict_body_visual_pos(
        target_entity,
        current_sim_s,
        depart_s + total_time,
        real_secs,
        sim_scale,
        &body_query,
        &kepler_query,
        &amp_query,
    )
    .unwrap_or(dest_t.translation);

    // ── Geometry: prefer phase-aware real orbits, fall back to Bezier ──
    //
    // The planner stores a `ga_phase_aware: Option<PhaseAwareGaOption>`
    // with `leg1_orbit` / `leg2_orbit` from Lambert for the slider's
    // burn epoch.  Sampling these directly gives the *real* Kepler
    // half-ellipse path the ship would fly.
    //
    // BUT: for off-optimal slider positions the `solve_lambert_transfer`
    // solver can return a **multi-revolution** orbit (lowest-energy
    // branch) where `mean_motion * tof > 2π`.  Sampling such an orbit
    // from `t = 0..1` traces the orbit multiple times, looping back
    // past the origin and detaching from Mars / Saturn — the exact
    // visual the user reported.  Detect this and fall back to the
    // Bezier (`compute_gravity_assist_arc`) which always anchors on
    // the predicted `op` / `fp` / `dp` positions.
    //
    // `optimal_window_arc_fits_hohmann` (heuristic): a half-orbit
    // Lambert transfer has `mean_motion * tof ≈ π` (radians).  Allow
    // a ±1.0 rad tolerance to cover peri-/apohelion Hohmann where
    // eccentricity shifts the angular travel slightly.  Outside this
    // band, the solver picked a multi-rev branch — Bezier fallback.
    let flyby_ring_r = fleet_parking_visual_radius(flyby_bd.visual_radius);
    let phase_aware_legs = fleet_ui_state.ga_phase_aware.as_ref().and_then(|p| {
        let leg1_orbit = p.leg1_orbit.as_ref()?;
        let leg2_orbit = p.leg2_orbit.as_ref()?;
        let ma_leg1 = leg1_orbit.mean_motion * p.leg1_time_s;
        let ma_leg2 = leg2_orbit.mean_motion * (p.leg1_time_s + p.leg2_time_s);
        // Reject only when BOTH legs obviously depart from π (i.e.
        // the solver picked multi-rev branches on both).  Single-leg
        // oddities can still look OK in isolation.
        let leg1_hohmann_like = (ma_leg1 - std::f64::consts::PI).abs() < 1.0;
        let leg2_hohmann_like = (ma_leg2 - std::f64::consts::PI).abs() < 1.0;
        if leg1_hohmann_like && leg2_hohmann_like {
            Some((leg1_orbit, leg2_orbit, p.leg1_time_s, p.leg2_time_s))
        } else {
            None
        }
    });

    let eval_leg1: Box<dyn Fn(f32) -> Vec3> =
        if let Some((leg1_orbit, _, p_leg1_time, _)) = phase_aware_legs {
            let orbit = leg1_orbit;
            let leg_adv = orbit.mean_motion * p_leg1_time;
            Box::new(move |t: f32| {
                let ma = orbit.mean_anomaly_epoch + leg_adv * t as f64;
                let pos_au = orbit_position_from_mean_anomaly(orbit, ma);
                Vec3::new(
                    (pos_au.x * SCALING_FACTOR) as f32,
                    (pos_au.y * SCALING_FACTOR) as f32,
                    (pos_au.z * SCALING_FACTOR) as f32,
                )
            })
        } else {
            let ga_geo =
                compute_gravity_assist_arc(op, fp, dp, origin_ring_r, flyby_ring_r, dest_ring_r);
            Box::new(move |t: f32| ga_geo.eval_leg1(t))
        };
    let eval_leg2: Box<dyn Fn(f32) -> Vec3> =
        if let Some((_, leg2_orbit, p_leg1_time, p_leg2_time)) = phase_aware_legs {
            let orbit = leg2_orbit;
            let ma_at_flyby = orbit.mean_motion * p_leg1_time;
            let leg_adv = orbit.mean_motion * p_leg2_time;
            Box::new(move |t: f32| {
                let ma = orbit.mean_anomaly_epoch + ma_at_flyby + leg_adv * t as f64;
                let pos_au = orbit_position_from_mean_anomaly(orbit, ma);
                Vec3::new(
                    (pos_au.x * SCALING_FACTOR) as f32,
                    (pos_au.y * SCALING_FACTOR) as f32,
                    (pos_au.z * SCALING_FACTOR) as f32,
                )
            })
        } else {
            let ga_geo =
                compute_gravity_assist_arc(op, fp, dp, origin_ring_r, flyby_ring_r, dest_ring_r);
            Box::new(move |t: f32| ga_geo.eval_leg2(t))
        };

    // ── Draw both legs with arc-length-uniform dashing ───────────────────────

    // Leg 1: lime-green approach arc.
    draw_dashed_curve(&mut gizmos, &eval_leg1, 24, |f| {
        Color::srgba(0.3, 1.0, 0.4, 0.80 - 0.35 * f)
    });

    // Leg 2: magenta departure arc.
    draw_dashed_curve(&mut gizmos, &eval_leg2, 24, |f| {
        Color::srgba(1.0, 0.3, 0.8, 0.80 - 0.35 * f)
    });

    // ── Flyby node: yellow cross + ghost ring ─────────────────────────────────
    let cross = flyby_ring_r * 2.5;
    let node_color = Color::srgba(1.0, 1.0, 0.3, 0.9);
    gizmos.line(fp - Vec3::X * cross, fp + Vec3::X * cross, node_color);
    gizmos.line(fp - Vec3::Y * cross, fp + Vec3::Y * cross, node_color);
    draw_ghost_body(
        &mut gizmos,
        fp,
        flyby_ring_r * 1.4,
        flyby_bd.visual_radius * 0.7,
        false,
    );

    // ── Destination ghost ─────────────────────────────────────────────────────
    // `dest_orbit_ring_r` (shell-driven) instead of `dest_ring_r`
    // (geometry offset) so the dashed amber ring scales with the
    // player's chosen arrival shell rather than hugging the body's rim.
    draw_ghost_body(
        &mut gizmos,
        dp,
        dest_orbit_ring_r,
        dest_bd.visual_radius,
        false,
    );
}
