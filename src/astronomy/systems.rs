use std::collections::HashMap;

use bevy::math::DVec3;
use bevy::prelude::*;

use super::components::{
    CometTail, CurrentStarSystem, Destroyed, FloatingOrigin, Hovered, HyperbolicTrajectory,
    KeplerOrbit, LocalOrbitAmplification, OrbitCenter, OrbitPath, Selected, SpaceCoordinates,
    SystemId,
};
use crate::plugins::camera::{CameraAnchor, GameCamera, ViewMode};
use crate::plugins::solar_system::{CelestialBody, Comet, LogicalParent, Moon, Planet, Star};
use crate::plugins::solar_system_data::BodyType;
use crate::ui::launch::LaunchState;
use crate::ui::{SimulationTime, TimeScale};

/// Scaling factor for converting astronomical units to Bevy rendering units
/// 1 AU = 1500.0 Bevy units ensures separation between planets and moons
pub const SCALING_FACTOR: f64 = 1500.0;

/// Maximum iterations for Kepler solver
const MAX_KEPLER_ITERATIONS: u32 = 50;

/// Convergence tolerance for Kepler solver
const KEPLER_TOLERANCE: f64 = 1e-10;

/// Maximum eccentricity for the elliptical Kepler solver.
/// Orbits with e >= this are clamped to avoid numerical singularities.
const MAX_ELLIPTICAL_ECCENTRICITY: f64 = 0.99999;

/// Desired approximate line-segment chord length for orbit gizmos, in render units.
/// Large stellar orbits need more samples than planet-scale orbits to avoid a faceted look.
const ORBIT_PATH_TARGET_CHORD_LENGTH: f64 = 300.0;

/// Hard cap to keep orbit rendering cost bounded even for extremely wide systems.
const MAX_ORBIT_PATH_SEGMENTS: u32 = 1536;

/// Solves Kepler's equation: M = E - e*sin(E) for eccentric anomaly E
/// Uses Newton-Raphson iteration for high accuracy.
///
/// For near-parabolic or hyperbolic orbits (e >= 0.99999), eccentricity is
/// clamped to avoid numerical singularities in the elliptical solver.
///
/// # Arguments
/// * `mean_anomaly` - Mean anomaly M in radians
/// * `eccentricity` - Orbital eccentricity e (0 <= e < 1 for elliptical orbits)
///
/// # Returns
/// Eccentric anomaly E in radians
pub fn solve_kepler(mean_anomaly: f64, eccentricity: f64) -> f64 {
    // For circular orbits, mean anomaly equals eccentric anomaly
    if eccentricity < 1e-10 {
        return mean_anomaly;
    }

    // Clamp eccentricity to avoid numerical singularities
    let e = eccentricity.min(MAX_ELLIPTICAL_ECCENTRICITY);

    // For high eccentricity, use a better initial guess
    let mut eccentric_anomaly = if e > 0.8 {
        // For high eccentricity, M + e is a better starting point
        mean_anomaly + e * mean_anomaly.sin()
    } else {
        mean_anomaly
    };

    // Newton-Raphson iteration
    for _ in 0..MAX_KEPLER_ITERATIONS {
        // f(E) = E - e*sin(E) - M
        let f = eccentric_anomaly - e * eccentric_anomaly.sin() - mean_anomaly;

        // f'(E) = 1 - e*cos(E)
        let f_prime = 1.0 - e * eccentric_anomaly.cos();

        // Prevent division by near-zero
        if f_prime.abs() < 1e-15 {
            break;
        }

        // Newton-Raphson step: E_new = E_old - f(E)/f'(E)
        let delta = f / f_prime;
        eccentric_anomaly -= delta;

        // Check for convergence
        if delta.abs() < KEPLER_TOLERANCE {
            break;
        }
    }

    eccentric_anomaly
}

/// Solve the **hyperbolic** Kepler equation: `M = e * sinh(H) - H` for the
/// hyperbolic anomaly `H`.
///
/// Used by the propagation branch for entities with `KeplerOrbit.eccentricity > 1.0`
/// that carry a `HyperbolicTrajectory` companion.  The mean anomaly `M` for a
/// hyperbolic orbit is unbounded (not modulo 2π like the elliptic case) and
/// is integrated linearly over time once the asymptote velocity is known.
///
/// Newton-Raphson converges quadratically for `|H| < ~50`, which covers every
/// real probe at 2026-01-01 (the brief's probes are all in `H ∈ [3, 4]`).
///
/// # Arguments
/// * `mean_anomaly` - Hyperbolic mean anomaly `M = e sinh(H) - H`
/// * `eccentricity` - Eccentricity `e` (must be `> 1.0` for a valid result)
///
/// # Returns
/// Hyperbolic anomaly `H` in radians.
pub fn solve_hyperbolic_kepler(mean_anomaly: f64, eccentricity: f64) -> f64 {
    if eccentricity <= 1.0 {
        // Not a hyperbolic orbit; the caller should not invoke this path.
        return 0.0;
    }
    // Initial guess: H ≈ M / (e - 1) for large e (Curtis §3.5).
    let mut h = mean_anomaly / (eccentricity - 1.0).max(1e-9);
    for _ in 0..MAX_KEPLER_ITERATIONS {
        let sinh_h = h.sinh();
        let f = eccentricity * sinh_h - h - mean_anomaly;
        let f_prime = eccentricity * h.cosh() - 1.0;
        if f_prime.abs() < 1e-15 {
            break;
        }
        let delta = f / f_prime;
        h -= delta;
        if delta.abs() < KEPLER_TOLERANCE {
            break;
        }
    }
    h
}

/// Convert hyperbolic anomaly `H` to true anomaly `ν` for `e > 1.0`.
///
/// `tan(ν/2) = sqrt((e+1)/(e-1)) * tanh(H/2)`
pub fn hyperbolic_to_true_anomaly(hyperbolic_anomaly: f64, eccentricity: f64) -> f64 {
    if eccentricity <= 1.0 {
        return 0.0;
    }
    let ratio = ((eccentricity + 1.0) / (eccentricity - 1.0)).sqrt();
    let tan_nu_half = ratio * (hyperbolic_anomaly * 0.5).tanh();
    2.0 * tan_nu_half.atan()
}

/// Calculate the 3D orbital position from a mean anomaly.
///
/// # Arguments
/// * `orbit` - Keplerian orbital elements
/// * `mean_anomaly` - Mean anomaly in radians (elliptic: 0..2π;
///   hyperbolic: unbounded — the time-since-periapsis in radians
///   at mean-motion rate, allowed to grow past 2π once `t > t_period`).
///
/// # Returns
/// Position in AU in the orbit's reference frame
pub fn orbit_position_from_mean_anomaly(orbit: &KeplerOrbit, mean_anomaly: f64) -> DVec3 {
    // Branch on orbit class.  Hyperbolic orbits (e > 1) need
    // hyperbolic Kepler solving + hyperbolic true-anomaly conversion;
    // the elliptic solver's eccentricity clamp would otherwise
    // collapse the orbit to a degenerate ellipse.
    let (true_anomaly, radius) = if orbit.eccentricity > 1.0 {
        let h = solve_hyperbolic_kepler(mean_anomaly, orbit.eccentricity);
        let nu = hyperbolic_to_true_anomaly(h, orbit.eccentricity);
        // r = a * (1 - e²) / (1 + e*cos(ν)).  For hyperbolic
        // orbits `a` is negative and `1 - e²` is negative (since
        // e² > 1), so the product is positive — the formula is
        // valid without sign flips.
        let denom = 1.0 + orbit.eccentricity * nu.cos();
        let r = if denom.abs() < 1e-10 {
            // ν ≈ π (apoapsis = ∞); fall back to perihelion radius.
            orbit.semi_major_axis.abs() * (1.0 - orbit.eccentricity)
        } else {
            (orbit.semi_major_axis * (1.0 - orbit.eccentricity * orbit.eccentricity) / denom)
                .max(0.0)
        };
        (nu, r)
    } else {
        // Solve Kepler's equation for eccentric anomaly.
        let eccentric_anomaly = solve_kepler(mean_anomaly, orbit.eccentricity);
        // Convert to true anomaly.
        let true_anomaly = eccentric_to_true_anomaly(eccentric_anomaly, orbit.eccentricity);
        // Elliptic / circular orbit radius (clamps eccentricity
        // to < 1 internally to avoid the division-by-zero near
        // parabolic limit).
        let radius = orbital_radius(orbit.semi_major_axis, orbit.eccentricity, true_anomaly);
        (true_anomaly, radius)
    };

    // Position in the orbital plane
    let x_orbital = radius * true_anomaly.cos();
    let y_orbital = radius * true_anomaly.sin();

    // Apply argument of periapsis rotation
    let cos_w = orbit.argument_of_periapsis.cos();
    let sin_w = orbit.argument_of_periapsis.sin();
    let x_perifocal = x_orbital * cos_w - y_orbital * sin_w;
    let y_perifocal = x_orbital * sin_w + y_orbital * cos_w;

    // Apply inclination and longitude of ascending node rotations
    let cos_i = orbit.inclination.cos();
    let sin_i = orbit.inclination.sin();
    let cos_omega = orbit.longitude_ascending_node.cos();
    let sin_omega = orbit.longitude_ascending_node.sin();

    let x = x_perifocal * cos_omega - y_perifocal * cos_i * sin_omega;
    let y = x_perifocal * sin_omega + y_perifocal * cos_i * cos_omega;
    let z = y_perifocal * sin_i;

    DVec3::new(x, y, z)
}

/// Calculate true anomaly from eccentric anomaly
/// Uses the relationship: tan(ν/2) = sqrt((1+e)/(1-e)) * tan(E/2)
///
/// For near-parabolic/hyperbolic orbits, eccentricity is clamped to avoid
/// taking sqrt of a negative number.
///
/// # Arguments
/// * `eccentric_anomaly` - Eccentric anomaly E in radians
/// * `eccentricity` - Orbital eccentricity e
///
/// # Returns
/// True anomaly ν in radians
pub fn eccentric_to_true_anomaly(eccentric_anomaly: f64, eccentricity: f64) -> f64 {
    // For circular orbits
    if eccentricity < 1e-10 {
        return eccentric_anomaly;
    }

    // Clamp eccentricity to keep the sqrt term valid (1-e must be > 0)
    let e = eccentricity.min(MAX_ELLIPTICAL_ECCENTRICITY);

    // Recover the correct quadrant with atan2; a plain atan folds angles back
    // into (-pi, pi) and breaks transfers that propagate past apoapsis.
    let denom = 1.0 - e * eccentric_anomaly.cos();
    let cos_nu = ((eccentric_anomaly.cos() - e) / denom).clamp(-1.0, 1.0);
    let sin_nu = ((1.0 - e * e).max(0.0)).sqrt() * eccentric_anomaly.sin() / denom;
    sin_nu.atan2(cos_nu)
}

/// Calculate the orbital radius at a given true anomaly
///
/// # Arguments
/// * `semi_major_axis` - Semi-major axis a in AU
/// * `eccentricity` - Orbital eccentricity e
/// * `true_anomaly` - True anomaly ν in radians
///
/// # Returns
/// Orbital radius r in AU
fn orbital_radius(semi_major_axis: f64, eccentricity: f64, true_anomaly: f64) -> f64 {
    // Clamp eccentricity for safety
    let e = eccentricity.min(MAX_ELLIPTICAL_ECCENTRICITY);

    // r = a(1 - e²) / (1 + e*cos(ν))
    let numerator = semi_major_axis * (1.0 - e * e);
    let denominator = 1.0 + e * true_anomaly.cos();

    // Prevent division by zero or negative radius
    if denominator.abs() < 1e-10 {
        return semi_major_axis; // Fall back to semi-major axis
    }

    let r = numerator / denominator;
    r.max(0.0) // Ensure non-negative
}

/// Calculate the 3D orbital position directly from a true anomaly.
/// Unlike `orbit_position_from_mean_anomaly`, this skips the Kepler solver
/// and is used for drawing orbit paths with uniform geometric spacing.
pub fn orbit_position_from_true_anomaly(orbit: &KeplerOrbit, true_anomaly: f64) -> DVec3 {
    let radius = orbital_radius(orbit.semi_major_axis, orbit.eccentricity, true_anomaly);

    let x_orbital = radius * true_anomaly.cos();
    let y_orbital = radius * true_anomaly.sin();

    let cos_w = orbit.argument_of_periapsis.cos();
    let sin_w = orbit.argument_of_periapsis.sin();
    let x_perifocal = x_orbital * cos_w - y_orbital * sin_w;
    let y_perifocal = x_orbital * sin_w + y_orbital * cos_w;

    let cos_i = orbit.inclination.cos();
    let sin_i = orbit.inclination.sin();
    let cos_omega = orbit.longitude_ascending_node.cos();
    let sin_omega = orbit.longitude_ascending_node.sin();

    let x = x_perifocal * cos_omega - y_perifocal * cos_i * sin_omega;
    let y = x_perifocal * sin_omega + y_perifocal * cos_i * cos_omega;
    let z = y_perifocal * sin_i;

    DVec3::new(x, y, z)
}

/// Convert mean anomaly to true anomaly via the Kepler solver
fn mean_anomaly_to_true_anomaly(mean_anomaly: f64, eccentricity: f64) -> f64 {
    let e_anom = solve_kepler(mean_anomaly, eccentricity);
    eccentric_to_true_anomaly(e_anom, eccentricity)
}

fn orbit_path_segments(path: &OrbitPath, orbit: &KeplerOrbit, amplification: f64) -> u32 {
    let eccentricity = orbit.eccentricity.min(MAX_ELLIPTICAL_ECCENTRICITY);
    let eccentricity_segments = if eccentricity > 0.6 {
        (path.segments as f64 * (1.0 + eccentricity * 2.0)).ceil() as u32
    } else {
        path.segments
    };

    let semi_major_render = orbit.semi_major_axis.abs() * SCALING_FACTOR * amplification.abs();
    if semi_major_render <= 0.0 {
        return eccentricity_segments
            .max(path.segments)
            .min(MAX_ORBIT_PATH_SEGMENTS);
    }

    let semi_minor_render = semi_major_render * (1.0 - eccentricity * eccentricity).max(0.0).sqrt();
    let h =
        ((semi_major_render - semi_minor_render) / (semi_major_render + semi_minor_render)).powi(2);
    let circumference = std::f64::consts::PI
        * (semi_major_render + semi_minor_render)
        * (1.0 + (3.0 * h) / (10.0 + (4.0 - 3.0 * h).sqrt()));
    let size_segments = (circumference / ORBIT_PATH_TARGET_CHORD_LENGTH).ceil() as u32;

    eccentricity_segments
        .max(size_segments)
        .max(path.segments)
        .min(MAX_ORBIT_PATH_SEGMENTS)
}

/// System that propagates all orbits based on Keplerian mechanics
/// Updates SpaceCoordinates based on KeplerOrbit elements and elapsed time
/// Uses SimulationTime to allow time scaling via UI controls
///
/// If an entity has an [`OrbitCenter`] component, its orbital position is
/// computed relative to that parent entity's current [`SpaceCoordinates`].
/// Without it, the orbit is relative to the universe origin (0,0,0), which
/// is correct for Sol-system bodies orbiting the Sun.
pub fn propagate_orbits(
    sim_time: Res<SimulationTime>,
    mut param_set: ParamSet<(
        Query<(
            Entity,
            &KeplerOrbit,
            Option<&OrbitCenter>,
            Option<&HyperbolicTrajectory>,
        )>,
        Query<&mut SpaceCoordinates>,
        Query<&SpaceCoordinates, Without<KeplerOrbit>>,
        Query<&SpaceCoordinates>,
    )>,
) {
    // Get elapsed simulation time in seconds
    let elapsed_time = sim_time.elapsed_seconds();

    // First pass: collect all orbiting entities and their (copied) orbital data
    // along with their optional `HyperbolicTrajectory` companion.  GRA-131.
    let mut entries: Vec<(
        Entity,
        KeplerOrbit,
        Option<Entity>,
        Option<HyperbolicTrajectory>,
    )> = Vec::new();
    for (entity, orbit, orbit_center, hyperbolic) in param_set.p0().iter() {
        entries.push((
            entity,
            *orbit,
            orbit_center.map(|oc| oc.0),
            hyperbolic.copied(),
        ));
    }

    // Build a full depth map so entities are processed in ancestry order.
    // Generated multi-star systems can have chains like:
    //   barycenter anchor -> companion star -> planet -> moon
    // The previous capped 0/1/2 classification let planets and moons share
    // the same depth, so a moon could read its planet's previous-frame
    // position and appear to flicker outside its orbit.
    let orbit_center_set: HashMap<Entity, Option<Entity>> =
        entries.iter().map(|(e, _, oc, _)| (*e, *oc)).collect();
    let mut depth_cache: HashMap<Entity, usize> = HashMap::new();

    fn depth_of(
        entity: Entity,
        orbit_center_set: &HashMap<Entity, Option<Entity>>,
        depth_cache: &mut HashMap<Entity, usize>,
    ) -> usize {
        if let Some(depth) = depth_cache.get(&entity) {
            return *depth;
        }

        let depth = match orbit_center_set.get(&entity).copied().flatten() {
            None => 0,
            Some(parent) => {
                if parent == entity {
                    0
                } else {
                    depth_of(parent, orbit_center_set, depth_cache).saturating_add(1)
                }
            }
        };

        depth_cache.insert(entity, depth);
        depth
    }

    entries.sort_by_key(|(entity, _, _, _)| depth_of(*entity, &orbit_center_set, &mut depth_cache));

    // Second pass: perform lookups and mutation without holding the p0 iterator borrow
    for (entity, orbit, orbit_center_entity, hyperbolic) in entries {
        // ── Branch: hyperbolic (`e > 1`) vs elliptic (`e < 1`) ──────────
        // The brief notes that `KeplerOrbit::mean_motion` is meaningless for
        // hyperbolics — we advance the hyperbolic anomaly `H` instead, by
        // solving `e*sinh(H) - H = M₀ + n*t` via Newton-Raphson.  Elliptic
        // entities use the closed-form M = M₀ + n*t → E (Kepler) → ν.  GRA-131.
        let orbit_pos = if orbit.eccentricity > 1.0 {
            let hyp = hyperbolic.unwrap_or_else(|| {
                // Defensive fallback: a `KeplerOrbit` flagged as hyperbolic
                // without a companion is malformed.  Bail out at the origin
                // rather than panic; a missing `HyperbolicTrajectory` would
                // only happen if a modder added a hyperbolic `KeplerOrbit`
                // and forgot the companion.  GRA-131.
                bevy::log::warn!(
                    "propagate_orbits: KeplerOrbit.eccentricity={:.4} on entity {:?} \
                     has no HyperbolicTrajectory companion; leaving position at origin",
                    orbit.eccentricity,
                    entity
                );
                HyperbolicTrajectory {
                    asymptote_velocity_kms: 0.0,
                    periapsis_distance_au: 0.0,
                    b_plane_angle_rad: 0.0,
                    epoch_jd_tdb: 0.0,
                    hyperbolic_anomaly_epoch: 0.0,
                }
            });
            // M at the epoch: M₀ = e*sinh(H₀) - H₀
            let m0 = orbit.eccentricity * hyp.hyperbolic_anomaly_epoch.sinh()
                - hyp.hyperbolic_anomaly_epoch;
            // No mean motion for hyperbolics (v∞ is constant); M(t) = M₀.
            // We keep the field on KeplerOrbit for symmetry with the elliptic
            // path but it is unused.  GRA-131.
            let m = m0 + orbit.mean_motion * elapsed_time;
            let h_now = solve_hyperbolic_kepler(m, orbit.eccentricity);
            let nu = hyperbolic_to_true_anomaly(h_now, orbit.eccentricity);
            orbit_position_from_true_anomaly(&orbit, nu)
        } else {
            // Elliptic path: M = M₀ + n*t, then Kepler solver + rotation.
            let mean_anomaly = orbit.mean_anomaly_epoch + orbit.mean_motion * elapsed_time;
            orbit_position_from_mean_anomaly(&orbit, mean_anomaly)
        };

        // Add parent position if an OrbitCenter is specified
        let parent_pos = if let Some(oc_entity) = orbit_center_entity {
            // Try non-orbiting centers first (static stars), then orbiting ones (binary stars)
            if let Ok(sc) = param_set.p2().get(oc_entity) {
                sc.position
            } else if let Ok(sc) = param_set.p3().get(oc_entity) {
                sc.position
            } else {
                DVec3::ZERO
            }
        } else {
            DVec3::ZERO
        };

        // Update space coordinates (in AU)
        if let Ok(mut coords) = param_set.p1().get_mut(entity) {
            coords.position = parent_pos + orbit_pos;
        }
    }
}

/// Keep the floating origin centered on the currently anchored body while in
/// system view.
///
/// This preserves f32 transform precision for planets and moons orbiting stars
/// that are themselves far from the system barycenter, such as Proxima in Alpha
/// Centauri. Without this recentering, a small local orbit can be added on top
/// of a very large parent translation, producing visible wobble in render space.
pub fn sync_floating_origin_to_anchor(
    view_mode: Res<ViewMode>,
    current_system: Res<CurrentStarSystem>,
    camera_query: Query<&CameraAnchor, With<GameCamera>>,
    anchor_query: Query<(&SpaceCoordinates, Option<&SystemId>)>,
    mut floating_origin: ResMut<FloatingOrigin>,
) {
    if *view_mode != ViewMode::System {
        return;
    }

    let Ok(anchor) = camera_query.single() else {
        return;
    };

    let Some(anchor_entity) = anchor.0 else {
        return;
    };

    let Ok((coords, system_id)) = anchor_query.get(anchor_entity) else {
        return;
    };

    let anchor_system = system_id.map(|id| id.0).unwrap_or(0);
    if anchor_system != current_system.0 {
        return;
    }

    floating_origin.position = coords.position;
}

/// Base visual speed threshold in rad/real-second.
/// When effective orbital speed exceeds this, visual speed is compressed
/// logarithmically so bodies spin faster at higher game speeds but never
/// strobe. 2π ≈ 1 revolution per real second.
pub const VISUAL_SPEED_BASE: f64 = std::f64::consts::TAU;

/// Compress an effective angular speed (rad/real-sec) into a capped visual
/// speed using logarithmic scaling.  Below [`VISUAL_SPEED_BASE`] the speed
/// is returned unchanged.  Above it, `cap = BASE × (1 + ln(speed / BASE))`.
///
/// This gives faster motion at higher game speeds with diminishing returns:
///   2× BASE  → ~1.7× BASE
///   10× BASE → ~3.3× BASE
///   100× BASE → ~5.6× BASE
///
/// Public so fleet parking orbits (`src/fleets/systems.rs`) can apply the
/// same visual cap as orbital bodies, keeping parking fleets and their host
/// bodies visually synchronized at all game speeds.
pub fn capped_visual_speed(effective_speed: f64) -> f64 {
    if effective_speed <= VISUAL_SPEED_BASE {
        effective_speed
    } else {
        VISUAL_SPEED_BASE * (1.0 + (effective_speed / VISUAL_SPEED_BASE).ln())
    }
}

/// System that converts high-precision SpaceCoordinates to rendering Transform.
/// Implements "floating origin" technique by scaling down coordinates and converting to f32.
///
/// For moons with a [`LocalOrbitAmplification`] component the local position is
/// additionally scaled so that the moon renders outside the parent's visual mesh.
///
/// When a body's effective orbital angular speed exceeds [`MAX_VISUAL_ORBITAL_RAD_PER_SEC`],
/// the visual position is capped to orbit at a smooth perceivable rate using
/// real time, while [`SpaceCoordinates`] retains the true analytical position
/// for game logic. This makes fast-orbiting bodies clickable and prevents
/// selection marker flickering.
///
/// Two moon models are handled:
/// - Sol-system moons (no `OrbitCenter`): `SpaceCoordinates` stores only the local
///   orbital offset (relative to the star-system origin). Amplify that directly
///   and add the parent's world position.
/// - Procedural moons (`OrbitCenter` present): `propagate_orbits` has already
///   written `coords.position = parent_pos + local_orbit` (absolute AU).
///   Subtract the parent's position first, amplify the remainder, then add the
///   parent world position without amplification.
pub fn update_render_transform(
    time_scale: Res<TimeScale>,
    real_time: Res<Time<Real>>,
    mut query: Query<(
        &SpaceCoordinates,
        &mut Transform,
        Option<&LocalOrbitAmplification>,
        Option<&LogicalParent>,
        Option<&OrbitCenter>,
        Option<&KeplerOrbit>,
    )>,
    parent_coords: Query<&SpaceCoordinates>,
    floating_origin: Option<Res<crate::astronomy::components::FloatingOrigin>>,
) {
    let origin_offset = floating_origin.map(|fo| fo.position).unwrap_or(DVec3::ZERO);
    let scale = time_scale.scale as f64;
    let real_t = real_time.elapsed_secs() as f64;

    for (coords, mut transform, amplification, logical_parent, orbit_center, kepler_orbit) in
        query.iter_mut()
    {
        // Determine which position to use for rendering.
        // If the body has a KeplerOrbit and orbital speed is capped, compute
        // a visual position from capped mean anomaly × real time.
        let visual_coords = if let Some(orbit) = kepler_orbit {
            let effective_speed = orbit.mean_motion.abs() * scale;
            if effective_speed > VISUAL_SPEED_BASE {
                // Capped visual orbit: logarithmically compressed speed
                let vis_speed = capped_visual_speed(effective_speed) * orbit.mean_motion.signum();
                let visual_mean_anomaly = orbit.mean_anomaly_epoch + vis_speed * real_t;
                let visual_local_pos = orbit_position_from_mean_anomaly(orbit, visual_mean_anomaly);

                // For bodies with OrbitCenter, add parent position to get absolute coords
                let parent_pos = if let Some(oc_entity) = orbit_center.map(|oc| oc.0) {
                    parent_coords
                        .get(oc_entity)
                        .map(|sc| sc.position)
                        .unwrap_or(DVec3::ZERO)
                } else {
                    DVec3::ZERO
                };

                Some((visual_local_pos, parent_pos))
            } else {
                None
            }
        } else {
            None
        };

        let final_translation = if let Some(amp) = amplification {
            let amp_f64 = amp.0 as f64;

            // Resolve parent SpaceCoordinates via LogicalParent
            let parent_sc = logical_parent.and_then(|lp| parent_coords.get(lp.0).ok());

            let (local_pos, parent_world) = if let Some(psc) = parent_sc {
                if let Some((vis_local, _vis_parent)) = &visual_coords {
                    // Speed-capped: use visual local position directly
                    let pw = (psc.position - origin_offset) * SCALING_FACTOR;
                    (*vis_local, pw)
                } else if orbit_center.is_some() {
                    // coords.position is ABSOLUTE (parent + local) because
                    // propagate_orbits added the parent position via OrbitCenter.
                    // Strip parent position to recover the local orbit offset.
                    let local = coords.position - psc.position;
                    let pw = (psc.position - origin_offset) * SCALING_FACTOR;
                    (local, pw)
                } else {
                    // coords.position is already the local orbital offset
                    // (no OrbitCenter → propagate_orbits left it as-is).
                    let pw = (psc.position - origin_offset) * SCALING_FACTOR;
                    (coords.position, pw)
                }
            } else {
                // No parent found — fall back to non-amplified placement
                let scaled = (coords.position - origin_offset) * SCALING_FACTOR;
                transform.translation =
                    Vec3::new(scaled.x as f32, scaled.y as f32, scaled.z as f32);
                continue;
            };

            // Amplify only the local orbit offset, position relative to parent
            let world = parent_world + local_pos * SCALING_FACTOR * amp_f64;
            Vec3::new(world.x as f32, world.y as f32, world.z as f32)
        } else if let Some((vis_local, vis_parent)) = visual_coords {
            // Speed-capped non-moon body: use visual position
            let vis_abs = vis_parent + vis_local;
            let scaled = (vis_abs - origin_offset) * SCALING_FACTOR;
            Vec3::new(scaled.x as f32, scaled.y as f32, scaled.z as f32)
        } else {
            // Non-moon body: straightforward AU → Bevy-unit conversion
            let scaled = (coords.position - origin_offset) * SCALING_FACTOR;
            Vec3::new(scaled.x as f32, scaled.y as f32, scaled.z as f32)
        };

        transform.translation = final_translation;
    }
}

/// System that draws orbit paths as fading trails.
/// The trail is brightest at the body's current position and fades out
/// behind it, creating a comet-tail effect along the orbit.
///
/// At extreme game speeds, when a body completes more than one orbit per
/// frame, the fading trail becomes meaningless. In that case, a solid
/// full-orbit ring is drawn instead. A smooth blend transitions between
/// the two modes.
///
/// Samples uniformly in **true anomaly** so that highly eccentric orbits
/// (comets, long-period objects) get even point density along the geometric
/// ellipse rather than clustering near apoapsis.
pub fn draw_orbit_paths(
    mut gizmos: Gizmos,
    sim_time: Res<SimulationTime>,
    time_scale: Res<TimeScale>,
    real_time: Res<Time<Real>>,
    current_system: Res<CurrentStarSystem>,
    query: Query<(
        &KeplerOrbit,
        &OrbitPath,
        Option<&OrbitCenter>,
        Option<&LogicalParent>,
        Option<&LocalOrbitAmplification>,
        Option<&Visibility>,
        Option<&SystemId>,
        Has<Selected>,
        Has<Hovered>,
        Option<&HyperbolicTrajectory>,
    )>,
    parent_coords: Query<&SpaceCoordinates>,
    floating_origin: Option<Res<crate::astronomy::components::FloatingOrigin>>,
) {
    let elapsed_time = sim_time.elapsed_seconds();
    let scale = time_scale.scale as f64;
    let real_t = real_time.elapsed_secs() as f64;
    let origin_offset = floating_origin.map(|fo| fo.position).unwrap_or(DVec3::ZERO);

    for (
        orbit,
        path,
        orbit_center,
        logical_parent,
        amplification,
        visibility,
        system_id,
        is_selected,
        is_hovered,
        _hyperbolic,
    ) in query.iter()
    {
        if !path.visible {
            continue;
        }

        // Only draw orbits for bodies in the current star system
        let body_system = system_id.map(|s| s.0).unwrap_or(0);
        if body_system != current_system.0 {
            continue;
        }

        // If the entity is hidden (e.g. because it's a moon whose parent is not anchored), don't draw the orbit
        if let Some(vis) = visibility {
            if *vis == Visibility::Hidden {
                continue;
            }
        }

        let amp = amplification.map(|a| a.0 as f64).unwrap_or(1.0);

        let parent_entity = orbit_center
            .map(|center| center.0)
            .or_else(|| logical_parent.map(|parent| parent.0));
        let parent_offset = parent_entity
            .and_then(|parent| parent_coords.get(parent).ok())
            .map(|sc| {
                let pos = (sc.position - origin_offset) * SCALING_FACTOR;
                Vec3::new(pos.x as f32, pos.y as f32, pos.z as f32)
            })
            .unwrap_or(Vec3::ZERO);

        // ── Branch: hyperbolic (`e > 1`) — draw a partial hyperbola ──────
        // The closed-orbit renderer walks the full ellipse backwards in
        // true anomaly, which would cross the asymptote at ν = ±π for a
        // hyperbola and produce non-physical lines.  Hyperbolics get a
        // dedicated partial-arc renderer: sample ν from 0 (periapsis) up
        // to the ν where r reaches 200 AU, with a fading tail.  GRA-131.
        if orbit.eccentricity > 1.0 {
            draw_hyperbolic_orbit_arc(
                &mut gizmos,
                orbit,
                path,
                amp,
                parent_offset,
                is_selected,
                is_hovered,
            );
            continue;
        }

        // Current true anomaly of the body
        let current_mean_anomaly = orbit.mean_anomaly_epoch + orbit.mean_motion * elapsed_time;
        let current_true_anomaly = mean_anomaly_to_true_anomaly(
            current_mean_anomaly.rem_euclid(std::f64::consts::TAU),
            orbit.eccentricity,
        );

        // Effective orbital angular speed in rad/real-second.
        // Uses the same threshold as the rotation cap so that
        // orbit trails and body spin switch to "capped" visuals together.
        let effective_orbital_speed = orbit.mean_motion.abs() * scale;
        // Blend factor: 0.0 = normal fading trail, 1.0 = solid full ring
        // Smooth transition between BASE/2 and BASE rad/real-second
        let ring_blend = ((effective_orbital_speed - VISUAL_SPEED_BASE * 0.5)
            / (VISUAL_SPEED_BASE * 0.5))
            .clamp(0.0, 1.0) as f32;

        // When speed is capped, compute the visual "head" position from the
        // compressed orbit rate so the directional indicator matches the body's
        // visual position (set by update_render_transform).
        let visual_true_anomaly = if effective_orbital_speed > VISUAL_SPEED_BASE {
            let vis_speed =
                capped_visual_speed(effective_orbital_speed) * orbit.mean_motion.signum();
            let vis_ma = orbit.mean_anomaly_epoch + vis_speed * real_t;
            mean_anomaly_to_true_anomaly(
                vis_ma.rem_euclid(std::f64::consts::TAU),
                orbit.eccentricity,
            )
        } else {
            current_true_anomaly
        };

        // The trail/ring "head" uses the visual position so the bright spot
        // coincides with where the body is rendered.
        let head_true_anomaly = if ring_blend > 0.0 {
            // Blend the head position between true and visual
            // For full ring mode, head = visual position entirely
            let diff =
                (visual_true_anomaly - current_true_anomaly).rem_euclid(std::f64::consts::TAU);
            let adjusted_diff = if diff > std::f64::consts::PI {
                diff - std::f64::consts::TAU
            } else {
                diff
            };
            current_true_anomaly + adjusted_diff * ring_blend as f64
        } else {
            current_true_anomaly
        };

        // Increase sampling for both eccentric and very large rendered orbits.
        // This keeps wide stellar binaries from looking polygonal.
        let segments = orbit_path_segments(path, orbit, amp);

        // For highly eccentric orbits (e > 0.9), limit the arc drawn.
        // The full ellipse extends to enormous distances at apoapsis, creating
        // ugly near-parallel lines spanning hundreds of AU. Instead, draw only
        // the portion of the orbit within a maximum distance from the focus.
        // This max distance is perihelion-adaptive: we show the "interesting"
        // part of the orbit near the Sun.
        let max_trail_distance_au = if orbit.eccentricity > 0.9 {
            // Show orbit out to ~60 AU (beyond Neptune) or apoapsis, whichever is smaller
            let apoapsis = orbit.semi_major_axis * (1.0 + orbit.eccentricity);
            apoapsis.min(60.0)
        } else {
            f64::INFINITY
        };

        let true_anomaly_step = std::f64::consts::TAU / (segments as f64);

        // Extract base color channels from path color
        let base = path.color.to_srgba();

        // Orbit highlighting: selected bodies get a bright highlight,
        // hovered bodies get a slightly brighter/more opaque orbit.
        // The boost is multiplicative on the trail alpha (not additive) so
        // the half-open fading trail shape is preserved.
        let highlight_alpha_mult: f32 = if is_selected {
            2.5
        } else if is_hovered {
            1.8
        } else {
            1.0
        };
        let highlight_color_boost: f32 = if is_selected {
            0.3
        } else if is_hovered {
            0.15
        } else {
            0.0
        };
        // Minimum alpha floor for highlighted orbits so the faint tail
        // remains visible even at the very back of the trail.
        let highlight_alpha_floor: f32 = if is_selected {
            0.15
        } else if is_hovered {
            0.08
        } else {
            0.0
        };

        // Trail covers the full orbit but fades from current position backwards.
        // Segment 0 is the body's current position (brightest).
        // Segment N is the point just before the body (dimmest / invisible).
        // In ring mode, a directional gradient centered on the visual head
        // replaces the fading trail, showing orbital direction.
        let mut prev_point: Option<Vec3> = None;

        for i in 0..=segments {
            // Walk backwards from the head position in true anomaly
            let true_anomaly = head_true_anomaly - (i as f64) * true_anomaly_step;
            let position_au = orbit_position_from_true_anomaly(orbit, true_anomaly);

            // For high-eccentricity orbits, skip segments beyond the max distance
            let distance_au = position_au.length();
            if distance_au > max_trail_distance_au {
                prev_point = None; // Break the trail
                continue;
            }

            let scaled_x = (position_au.x * SCALING_FACTOR * amp) as f32;
            let scaled_y = (position_au.y * SCALING_FACTOR * amp) as f32;
            let scaled_z = (position_au.z * SCALING_FACTOR * amp) as f32;
            let point = Vec3::new(scaled_x, scaled_y, scaled_z) + parent_offset;

            if let Some(prev) = prev_point {
                // t goes from 0.0 (at the body/head) to 1.0 (full orbit behind)
                let t = i as f32 / segments as f32;

                // Fading trail alpha: bright near the body, fading to near-zero.
                // Higher fade_exponent = steeper fade = shorter-looking trail.
                let trail_alpha = base.alpha * (1.0 - t).powf(path.fade_exponent);

                // Ring mode: directional gradient centered on the visual head.
                // Bright at the head (t=0), dimming to a base level at the
                // opposite side (t≈0.5), then rising slightly as we approach
                // the head again — creating a comet-like directional glow.
                // Scale ring exponent proportionally so fast-faders stay consistent.
                let ring_exponent = (path.fade_exponent * 0.8 / 1.8).max(0.4);
                let ring_head_alpha = base.alpha * (0.35 + 0.65 * (1.0 - t).powf(ring_exponent));

                // Blend between trail and ring based on speed
                let alpha = trail_alpha * (1.0 - ring_blend) + ring_head_alpha * ring_blend;

                // Apply highlight: multiplicative boost preserves the half-open
                // trail shape; floor ensures the faint tail stays visible.
                let alpha = (alpha * highlight_alpha_mult)
                    .max(highlight_alpha_floor)
                    .min(1.0);

                // Glow boost near the head — visible in both modes but
                // stronger in ring mode to act as a directional indicator.
                let head_region = t < 0.08;
                let glow = if head_region {
                    1.0 + 0.3 * (1.0 - ring_blend) + 0.5 * ring_blend
                } else {
                    1.0
                };

                if alpha > 0.01 {
                    let segment_color = Color::srgba(
                        ((base.red * glow) + highlight_color_boost).min(1.0),
                        ((base.green * glow) + highlight_color_boost).min(1.0),
                        ((base.blue * glow) + highlight_color_boost).min(1.0),
                        alpha,
                    );
                    gizmos.line(prev, point, segment_color);
                }
            }

            prev_point = Some(point);
        }
    }
}

/// Maximum heliocentric distance (AU) to render a hyperbolic orbit-path arc.
///
/// The brief specifies periapsis → 200 AU, fading tail.  Past ~200 AU the
/// starmap camera is zoomed out so much that the asymptote direction matters
/// more than the exact trail; clipping here keeps the renderer fast and
/// avoids drawing near-vertical "asymptote walls" that look like rendering
/// bugs.  GRA-131.
const HYPERBOLIC_ARC_MAX_DISTANCE_AU: f64 = 200.0;

/// Draw a partial-arc orbit path for a hyperbolic `KeplerOrbit`.
///
/// Sweeps `ν` from 0 (periapsis, the closest approach) outward to the
/// true anomaly at which the heliocentric distance reaches
/// `HYPERBOLIC_ARC_MAX_DISTANCE_AU`.  Alpha fades from bright at periapsis
/// to ~0 at the tail so the asymptote direction reads as a fading streak
/// rather than a hard line.  GRA-131.
fn draw_hyperbolic_orbit_arc(
    gizmos: &mut Gizmos,
    orbit: &KeplerOrbit,
    path: &OrbitPath,
    amp: f64,
    parent_offset: Vec3,
    is_selected: bool,
    is_hovered: bool,
) {
    // Solve r(ν) = q (e + cos(ν)) / (1 + e cos(ν)) = max_distance
    // ⇒ cos(ν) = (q - max_distance) / (e * max_distance - q)
    // where q = |a| (e - 1) is the periapsis distance.  We then clamp to the
    // domain (-π, +π) so we don't cross the asymptote.
    let q = (-orbit.semi_major_axis) * (orbit.eccentricity - 1.0);
    let e = orbit.eccentricity;
    let max_d = HYPERBOLIC_ARC_MAX_DISTANCE_AU;
    let cos_nu_max = ((q - max_d) / (e * max_d - q)).clamp(-1.0, 1.0);
    let nu_max = cos_nu_max.acos();
    // Sweep both +ν and -ν (the inbound leg is symmetric to the outbound
    // leg, but we draw both for visual symmetry).
    let nu_max = nu_max.min(std::f64::consts::PI - 1e-3);

    let segments = (path.segments as i32).max(64);
    let half = segments / 2;
    let step = nu_max / half as f64;

    let base = path.color.to_srgba();
    let highlight_alpha_mult: f32 = if is_selected {
        2.5
    } else if is_hovered {
        1.8
    } else {
        1.0
    };
    let highlight_color_boost: f32 = if is_selected {
        0.3
    } else if is_hovered {
        0.15
    } else {
        0.0
    };
    let highlight_alpha_floor: f32 = if is_selected {
        0.10
    } else if is_hovered {
        0.05
    } else {
        0.0
    };

    // Pre-compute trig of argument of periapsis and node so the inner loop
    // stays branch-light.
    let cos_w = orbit.argument_of_periapsis.cos();
    let sin_w = orbit.argument_of_periapsis.sin();
    let cos_i = orbit.inclination.cos();
    let sin_i = orbit.inclination.sin();
    let cos_omega = orbit.longitude_ascending_node.cos();
    let sin_omega = orbit.longitude_ascending_node.sin();
    let a = orbit.semi_major_axis;

    // Sample ν from -nu_max through 0 (periapsis) to +nu_max.  Periapsis
    // (ν=0) is the brightest point; alpha fades toward both ends.
    let mut prev_point: Option<Vec3> = None;
    let total_segments = 2 * half;
    for i in 0..=total_segments {
        // i = 0 → -nu_max, i = half → 0 (periapsis), i = total → +nu_max
        let nu = -nu_max + (i as f64) * step;
        let (sin_nu, cos_nu) = nu.sin_cos();

        // Inline r = a·(1 − e²) / (1 + e·cos ν) so we bypass the
        // `orbital_radius` eccentricity clamp at MAX_ELLIPTICAL_ECCENTRICITY =
        // 0.99999.  The clamp was written for the elliptical Kepler solver;
        // for hyperbolic e>1 it produced a degenerate ellipse near the Sun.
        // Inline the conic radius formula so we bypass the eccentricity clamp.
        let denom = 1.0 + e * cos_nu;
        if denom.abs() < 1e-10 {
            // At/near the asymptote the radius diverges — break the polyline.
            prev_point = None;
            continue;
        }
        let r = a * (1.0 - e * e) / denom;
        if !r.is_finite() {
            prev_point = None;
            continue;
        }

        // Build the position from `(r, ν)` directly in the orbital frame.
        let x_orbital = r * cos_nu;
        let y_orbital = r * sin_nu;
        let x_perifocal = x_orbital * cos_w - y_orbital * sin_w;
        let y_perifocal = x_orbital * sin_w + y_orbital * cos_w;
        let x = x_perifocal * cos_omega - y_perifocal * cos_i * sin_omega;
        let y = x_perifocal * sin_omega + y_perifocal * cos_i * cos_omega;
        let z = y_perifocal * sin_i;
        let position_au = DVec3::new(x, y, z);

        let distance_au = position_au.length();
        if distance_au > max_d {
            prev_point = None;
            continue;
        }

        let scaled_x = (position_au.x * SCALING_FACTOR * amp) as f32;
        let scaled_y = (position_au.y * SCALING_FACTOR * amp) as f32;
        let scaled_z = (position_au.z * SCALING_FACTOR * amp) as f32;
        let point = Vec3::new(scaled_x, scaled_y, scaled_z) + parent_offset;

        if let Some(prev) = prev_point {
            // t goes 0 (at -nu_max) → 0.5 (at periapsis) → 1 (at +nu_max)
            let t = i as f32 / total_segments as f32;
            // Fade: peak at periapsis (t = 0.5), zero at both ends.
            // Use a tent function: alpha = 1 - 2*|t - 0.5| with the path's
            // fade_exponent shaping how fast the ends fade out.
            let t_dist_from_center = (t - 0.5).abs() * 2.0; // 0 at center, 1 at ends
            let trail_alpha = base.alpha * (1.0 - t_dist_from_center).powf(path.fade_exponent);
            let alpha = (trail_alpha * highlight_alpha_mult)
                .max(highlight_alpha_floor)
                .min(1.0);

            if alpha > 0.01 {
                let segment_color = Color::srgba(
                    (base.red + highlight_color_boost).min(1.0),
                    (base.green + highlight_color_boost).min(1.0),
                    (base.blue + highlight_color_boost).min(1.0),
                    alpha,
                );
                gizmos.line(prev, point, segment_color);
            }
        }
        prev_point = Some(point);
    }
}

/// Distance in AU within which a comet tail becomes visible.
/// Real comets start developing tails around 3-5 AU from the Sun.
const COMET_TAIL_ONSET_AU: f64 = 5.0;

/// Minimum distance from sun (in AU) to render tail - avoids rendering inside sun
const COMET_TAIL_MIN_DISTANCE_AU: f64 = 0.02;

/// Maximum visual tail length in Bevy units at perihelion
const COMET_TAIL_MAX_LENGTH: f32 = 300.0;

/// Number of radial segments around the tail cone
const TAIL_RADIAL_SEGMENTS: u32 = 16;

/// Number of length segments for smooth gradients
const TAIL_LENGTH_SEGMENTS: u32 = 32;

/// Number of volumetric strands for ion/dust tail gizmo lines
const TAIL_VOLUME_STRANDS: u32 = 6;

/// Number of line segments per individual comet-tail strand
const COMET_TAIL_SEGMENTS: u32 = 24;

/// Creates a tapered cone mesh with vertex colors for gradient transparency.
/// Used for volumetric comet tails with smooth fade from base to tip.
fn create_tail_cone_mesh(
    length: f32,
    base_radius: f32,
    tip_radius: f32,
    base_color: Color,
    tip_color: Color,
) -> Mesh {
    use bevy::asset::RenderAssetUsages;
    use bevy::mesh::{Indices, PrimitiveTopology};

    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut colors = Vec::new();
    let mut indices = Vec::new();

    // Generate vertices along the cone length
    for length_i in 0..=TAIL_LENGTH_SEGMENTS {
        let t = length_i as f32 / TAIL_LENGTH_SEGMENTS as f32;
        let z = length * t;
        let radius = base_radius + (tip_radius - base_radius) * t;

        // Interpolate color
        let base_rgba = base_color.to_srgba();
        let tip_rgba = tip_color.to_srgba();
        let color = Color::srgba(
            base_rgba.red + (tip_rgba.red - base_rgba.red) * t,
            base_rgba.green + (tip_rgba.green - base_rgba.green) * t,
            base_rgba.blue + (tip_rgba.blue - base_rgba.blue) * t,
            base_rgba.alpha + (tip_rgba.alpha - base_rgba.alpha) * t,
        );

        // Create ring of vertices
        for radial_i in 0..TAIL_RADIAL_SEGMENTS {
            let theta = (radial_i as f32 / TAIL_RADIAL_SEGMENTS as f32) * std::f32::consts::TAU;
            let (sin_theta, cos_theta) = theta.sin_cos();

            let x = radius * cos_theta;
            let y = radius * sin_theta;

            positions.push([x, y, z]);

            // Normal points outward from cone surface
            let normal = Vec3::new(cos_theta, sin_theta, 0.0).normalize();
            normals.push(normal.to_array());

            colors.push(color.to_linear().to_f32_array());
        }
    }

    // Generate indices for triangle strip
    for length_i in 0..TAIL_LENGTH_SEGMENTS {
        for radial_i in 0..TAIL_RADIAL_SEGMENTS {
            let next_radial = (radial_i + 1) % TAIL_RADIAL_SEGMENTS;

            let current_ring = length_i * TAIL_RADIAL_SEGMENTS;
            let next_ring = (length_i + 1) * TAIL_RADIAL_SEGMENTS;

            let i0 = current_ring + radial_i;
            let i1 = current_ring + next_radial;
            let i2 = next_ring + radial_i;
            let i3 = next_ring + next_radial;

            // Two triangles per quad
            indices.push(i0);
            indices.push(i2);
            indices.push(i1);

            indices.push(i1);
            indices.push(i2);
            indices.push(i3);
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));

    mesh
}

/// System that spawns and manages volumetric 3D mesh-based comet tails.
/// Creates true geometry with gradient transparency for realistic appearance.
pub fn manage_comet_tail_meshes(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    current_system: Res<CurrentStarSystem>,
    comet_query: Query<
        (
            Entity,
            &CelestialBody,
            &KeplerOrbit,
            &SpaceCoordinates,
            Option<&SystemId>,
        ),
        (With<Comet>, Without<Destroyed>),
    >,
    tail_query: Query<(Entity, &CometTail)>,
    existing_tails: Query<&CometTail>,
) {
    // Track which comets should have tails
    let mut comets_needing_tails = std::collections::HashSet::new();

    for (entity, body, _orbit, coords, system_id) in comet_query.iter() {
        // Only manage tails for comets in the current star system
        let body_system = system_id.map(|s| s.0).unwrap_or(0);
        if body_system != current_system.0 {
            continue;
        }

        let distance_au = coords.position.length();

        // Check if tail should be visible
        if (COMET_TAIL_MIN_DISTANCE_AU..=COMET_TAIL_ONSET_AU).contains(&distance_au)
            && distance_au > 1e-6
        {
            comets_needing_tails.insert(entity);

            // Check if this comet already has tails
            let has_tails = existing_tails.iter().any(|t| t.comet_entity == entity);

            if !has_tails {
                // Spawn tail meshes for this comet
                spawn_comet_tail_meshes(
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    entity,
                    body,
                    coords,
                    distance_au,
                );
            }
        }
    }

    // Despawn tails for comets that no longer need them
    for (tail_entity, tail) in tail_query.iter() {
        if !comets_needing_tails.contains(&tail.comet_entity) {
            commands.entity(tail_entity).despawn();
        }
    }
}

/// Spawns ion and dust tail meshes for a comet
fn spawn_comet_tail_meshes(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    comet_entity: Entity,
    body: &CelestialBody,
    _coords: &SpaceCoordinates,
    distance_au: f64,
) {
    // Calculate tail parameters
    let intensity = ((1.0 - distance_au / COMET_TAIL_ONSET_AU) as f32).clamp(0.0, 1.0);
    // Adjusted proximity boost - less aggressive than before
    let proximity_boost = (2.0 / distance_au.max(0.5)) as f32;
    let brightness = (intensity * proximity_boost.min(2.0)).clamp(0.0, 1.0);
    // Base tail length for spawning geometry - scaling will be applied dynamically
    let tail_length = COMET_TAIL_MAX_LENGTH;

    // Seed for procedural variation
    let mut seed = 0u32;
    for byte in body.name.bytes() {
        seed = seed.wrapping_mul(31).wrapping_add(byte as u32);
    }

    // === ION TAIL (Type I): narrow, bluish-white ===
    // Scale radii with comet size so tails visually wrap the rear hemisphere.
    // Keep a moderate widening toward the tip for a physically plausible plume.
    let comet_radius = body.visual_radius.max(0.5);
    let ion_base_radius = comet_radius * 0.55;
    let ion_tip_radius = ion_base_radius * 1.45;
    let ion_base_color = Color::srgba(0.7, 0.85, 1.0, brightness * 0.6);
    let ion_tip_color = Color::srgba(0.5, 0.75, 1.0, 0.0);

    let ion_mesh = meshes.add(create_tail_cone_mesh(
        tail_length,
        ion_base_radius,
        ion_tip_radius,
        ion_base_color,
        ion_tip_color,
    ));

    let ion_material = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        emissive: LinearRgba::new(0.5, 0.7, 1.0, 0.0) * brightness * 10.0, // Increased glare (HDR)
        alpha_mode: AlphaMode::Add,
        unlit: true,
        cull_mode: None, // Double-sided
        ..default()
    });

    commands.spawn((
        Mesh3d(ion_mesh),
        MeshMaterial3d(ion_material),
        Transform::default(), // Will be updated by update_tail_transforms
        CometTail {
            comet_entity,
            is_ion_tail: true,
        },
    ));

    // === DUST TAIL (Type II): wider, yellowish ===
    // Dust coma is broader and should almost engulf the comet's dark side.
    let dust_base_radius = comet_radius * 0.95;
    let dust_tip_radius = dust_base_radius * 1.9;
    let dust_base_color = Color::srgba(1.0, 0.85, 0.4, brightness * 0.5);
    let dust_tip_color = Color::srgba(1.0, 0.7, 0.2, 0.0);

    let dust_mesh = meshes.add(create_tail_cone_mesh(
        tail_length * 0.7, // Dust tail is shorter
        dust_base_radius,
        dust_tip_radius,
        dust_base_color,
        dust_tip_color,
    ));

    let dust_material = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        emissive: LinearRgba::new(1.0, 0.75, 0.3, 0.0) * brightness * 8.0, // Increased glare (HDR)
        alpha_mode: AlphaMode::Add,
        unlit: true,
        cull_mode: None,
        ..default()
    });

    commands.spawn((
        Mesh3d(dust_mesh),
        MeshMaterial3d(dust_material),
        Transform::default(),
        CometTail {
            comet_entity,
            is_ion_tail: false,
        },
    ));
}

/// System that updates tail mesh positions and orientations each frame.
/// Tails always point away from the sun and follow their parent comet.
pub fn update_tail_transforms(
    comet_query: Query<(&SpaceCoordinates, &KeplerOrbit, &CelestialBody), With<Comet>>,
    mut tail_query: Query<(&mut Transform, &CometTail)>,
) {
    for (mut transform, tail) in tail_query.iter_mut() {
        if let Ok((coords, orbit, body)) = comet_query.get(tail.comet_entity) {
            // Convert comet position to rendering coordinates
            let comet_pos_scaled = coords.position * SCALING_FACTOR;
            let comet_pos = Vec3::new(
                comet_pos_scaled.x as f32,
                comet_pos_scaled.y as f32,
                comet_pos_scaled.z as f32,
            );

            // Anti-sunward direction (sun at origin)
            let to_sun = -comet_pos;
            let sun_distance = to_sun.length();
            if sun_distance < 1e-6 {
                continue;
            }
            let anti_sun_dir = -to_sun.normalize();

            // Start tail slightly inside the body so the base blends with the nucleus rear side.
            // This avoids a detached seam and gives the "engulfed backside" look up close.
            let surface_offset = (body.visual_radius * 0.35) * anti_sun_dir;

            transform.translation = comet_pos + surface_offset;

            // Compute dynamic scale based on distance from sun
            // Tail should be small/invisible near onset and grow larger near sun
            let distance_au = coords.position.length() as f32;
            let onset_au = COMET_TAIL_ONSET_AU as f32;

            // Normalized intensity (0.0 at onset, 1.0 at sun)
            let intensity = (1.0 - distance_au / onset_au).clamp(0.0, 1.0);

            // Scale factor: start small (0.1) and grow to full size (1.0)
            let dynamic_scale = (0.1 + intensity * 0.9).max(0.01);

            // Apply scale to length (Z) and width (X, Y)
            transform.scale = Vec3::splat(dynamic_scale);

            // Orient tail to point away from sun
            // Cone extends along +Z axis, so look along anti-sunward direction
            if tail.is_ion_tail {
                // Ion tail points straight away from sun
                transform.rotation = Quat::from_rotation_arc(Vec3::Z, anti_sun_dir);
            } else {
                // Dust tail has slight curve based on orbit
                let orbit_normal = Vec3::new(
                    orbit.longitude_ascending_node.sin() as f32 * orbit.inclination.sin() as f32,
                    orbit.inclination.cos() as f32,
                    orbit.longitude_ascending_node.cos() as f32 * orbit.inclination.sin() as f32,
                );
                let velocity_dir = anti_sun_dir.cross(orbit_normal).normalize_or_zero();
                let curved_dir = (anti_sun_dir + velocity_dir * 0.15).normalize();

                transform.rotation = Quat::from_rotation_arc(Vec3::Z, curved_dir);
            }
        }
    }
}
///
/// The tail always points away from the Sun and grows longer + brighter
/// as the comet approaches perihelion. Two visual tails are drawn:
/// - **Ion tail**: straight, narrow, bluish — points directly anti-sunward
/// - **Dust tail**: slightly curved, broader, yellowish — trails behind
///
/// Uses SpaceCoordinates directly for smooth rendering during time acceleration.
pub fn draw_comet_tails(
    view_mode: Res<ViewMode>,
    mut gizmos: Gizmos,
    current_system: Res<CurrentStarSystem>,
    query: Query<
        (
            &CelestialBody,
            &KeplerOrbit,
            &SpaceCoordinates,
            Option<&SystemId>,
        ),
        (With<Comet>, Without<Destroyed>),
    >,
) {
    // Skip comet tails in starmap view
    if *view_mode == ViewMode::Starmap {
        return;
    }

    for (body, orbit, coords, system_id) in query.iter() {
        // Only draw tails for comets in the current star system
        let body_system = system_id.map(|s| s.0).unwrap_or(0);
        if body_system != current_system.0 {
            continue;
        }

        // Current heliocentric distance in AU
        let distance_au = coords.position.length();

        // Only draw tail if within the onset distance and not too close to sun
        if !(COMET_TAIL_MIN_DISTANCE_AU..=COMET_TAIL_ONSET_AU).contains(&distance_au)
            || distance_au < 1e-6
        {
            continue;
        }

        // Convert high-precision position to rendering coordinates
        let body_pos_scaled = coords.position * SCALING_FACTOR;
        let body_pos = Vec3::new(
            body_pos_scaled.x as f32,
            body_pos_scaled.y as f32,
            body_pos_scaled.z as f32,
        );

        // Anti-sunward direction (sun is at origin)
        let to_sun = -body_pos;
        let sun_distance = to_sun.length();
        if sun_distance < 1e-6 {
            continue;
        }
        let anti_sun_dir = -to_sun.normalize();

        // Tail intensity scales inversely with distance squared (solar radiation)
        // Normalized: 1.0 at 0.5 AU, fading to 0 at onset distance
        let intensity = ((1.0 - distance_au / COMET_TAIL_ONSET_AU) as f32).clamp(0.0, 1.0);
        let proximity_boost = (0.5 / distance_au.max(0.1)) as f32; // Brighter when closer
        let brightness = (intensity * proximity_boost.min(3.0)).clamp(0.0, 1.0);

        // Tail length scales with proximity - longer when closer to sun
        let tail_length = COMET_TAIL_MAX_LENGTH * intensity * proximity_boost.min(2.5);

        if tail_length < 1.0 || brightness < 0.01 {
            continue;
        }

        // Use body name hash for consistent slight curl direction on the dust tail
        let mut seed = 0u32;
        for byte in body.name.bytes() {
            seed = seed.wrapping_mul(31).wrapping_add(byte as u32);
        }
        let curl_angle = ((seed % 1000) as f32 / 1000.0 - 0.5) * 0.3; // slight random curl

        // Find a perpendicular vector for the dust tail curve
        let up = if anti_sun_dir.y.abs() > 0.9 {
            Vec3::X
        } else {
            Vec3::Y
        };
        let perp = anti_sun_dir.cross(up).normalize();
        let perp2 = anti_sun_dir.cross(perp).normalize();

        // Compute orbit velocity direction for a more realistic dust tail curve
        // Dust tail curves slightly in the direction opposite to orbital motion
        let orbit_normal = Vec3::new(
            orbit.longitude_ascending_node.sin() as f32 * orbit.inclination.sin() as f32,
            orbit.inclination.cos() as f32,
            orbit.longitude_ascending_node.cos() as f32 * orbit.inclination.sin() as f32,
        );
        let velocity_approx = anti_sun_dir.cross(orbit_normal).normalize_or_zero();

        // === ION TAIL (Type I): straight, narrow, bluish-white ===
        // Draw multiple strands with Fibonacci spiral distribution for natural appearance
        for strand in 0..TAIL_VOLUME_STRANDS {
            // Fibonacci spiral for even distribution (better than uniform circle)
            let golden_angle = std::f32::consts::PI * (3.0 - 5.0_f32.sqrt());
            let angle = golden_angle * (strand as f32);
            let radius_factor = ((strand as f32 + 0.5) / TAIL_VOLUME_STRANDS as f32).sqrt();

            let (sin_a, cos_a) = angle.sin_cos();

            // Offset perpendicular to tail direction - varies by strand
            let base_offset_radius = tail_length * 0.02 * radius_factor; // 0-2% of tail length
            let base_offset = (perp * cos_a + perp2 * sin_a) * base_offset_radius;

            // Procedural variation per strand for natural look
            let strand_seed = seed.wrapping_add(strand * 997);
            let strand_var = ((strand_seed % 1000) as f32) / 1000.0;
            let wiggle_phase = strand_var * std::f32::consts::TAU;

            let mut prev = body_pos;
            for i in 1..=COMET_TAIL_SEGMENTS {
                let t = i as f32 / COMET_TAIL_SEGMENTS as f32;

                // Gentle expansion and wiggle along the tail
                let expanding = 1.0 + t * 0.4;
                let wiggle = (t * 8.0 + wiggle_phase).sin() * 0.15 * t;
                let offset = base_offset * expanding + perp2 * wiggle * base_offset_radius;

                let pos = body_pos + anti_sun_dir * tail_length * t + offset;

                // Fade from bright near body to transparent at tip
                // Use per-strand brightness variation for natural look
                let strand_brightness = 0.8 + strand_var * 0.4;
                let alpha = brightness * 0.5 * strand_brightness * (1.0 - t).powf(1.5)
                    / (TAIL_VOLUME_STRANDS as f32 * 0.7);

                if alpha > 0.005 {
                    // Slight color variation per strand
                    let blue_var = 0.95 + strand_var * 0.05;
                    let color = Color::srgba(
                        0.5 + 0.3 * (1.0 - t), // slight white near head
                        0.65 + 0.2 * (1.0 - t),
                        blue_var,
                        alpha,
                    );
                    gizmos.line(prev, pos, color);
                }
                prev = pos;
            }
        }

        // === DUST TAIL (Type II): curved, broader, yellowish ===
        // Draw multiple strands with more variation and curvature
        for strand in 0..TAIL_VOLUME_STRANDS {
            // Fibonacci spiral distribution
            let golden_angle = std::f32::consts::PI * (3.0 - 5.0_f32.sqrt());
            let angle = golden_angle * (strand as f32) + 0.5; // offset from ion tail
            let radius_factor = ((strand as f32 + 0.5) / TAIL_VOLUME_STRANDS as f32).sqrt();

            let (sin_a, cos_a) = angle.sin_cos();

            // Wider cone for dust tail
            let base_offset_radius = tail_length * 0.045 * radius_factor; // 0-4.5% of tail length
            let base_offset = (perp * cos_a + perp2 * sin_a) * base_offset_radius;

            // More variation for dust particles
            let strand_seed = seed.wrapping_add(strand * 1009);
            let strand_var = ((strand_seed % 1000) as f32) / 1000.0;
            let wiggle_phase = strand_var * std::f32::consts::TAU;

            let mut prev = body_pos;
            for i in 1..=COMET_TAIL_SEGMENTS {
                let t = i as f32 / COMET_TAIL_SEGMENTS as f32;

                // Dust tail is shorter and curves away from orbit direction
                let dust_length = tail_length * 0.7;
                let curve = t * t * 0.3; // quadratic curve

                // More expansion and wiggle for dust
                let expanding = 1.0 + t * 1.5;
                let wiggle = ((t * 6.0 + wiggle_phase).sin() * 0.2 + (t * 3.5).cos() * 0.15) * t;
                let offset = base_offset * expanding + perp2 * wiggle * base_offset_radius;

                let pos = body_pos
                    + anti_sun_dir * dust_length * t
                    + (perp * curl_angle + velocity_approx * 0.15) * dust_length * curve
                    + offset;

                // More variation in dust brightness
                let strand_brightness = 0.7 + strand_var * 0.5;
                let alpha = brightness * 0.4 * strand_brightness * (1.0 - t).powf(1.3)
                    / (TAIL_VOLUME_STRANDS as f32 * 0.7);

                if alpha > 0.005 {
                    // Color varies more along dust tail
                    let yellow_var = 0.92 + strand_var * 0.08;
                    let color = Color::srgba(
                        1.0,
                        yellow_var - 0.15 * t, // yellower at tip
                        0.4 - 0.2 * t,         // orange tint at tip
                        alpha,
                    );
                    gizmos.line(prev, pos, color);
                }
                prev = pos;
            }
        }

        // === COMA (fuzzy glow around the nucleus) ===
        // Draw a small radial starburst around the body
        {
            let coma_radius = body.visual_radius * 2.5 * brightness.max(0.3);
            let coma_alpha = brightness * 0.35;
            if coma_alpha > 0.01 {
                let num_rays = 12;
                for i in 0..num_rays {
                    let angle = (i as f32 / num_rays as f32) * std::f32::consts::TAU;
                    let (sin_a, cos_a) = angle.sin_cos();

                    // Use perpendicular vectors to create rays in a plane
                    let ray_dir = (perp * cos_a + perp2 * sin_a).normalize();
                    let tip = body_pos + ray_dir * coma_radius;

                    let color = Color::srgba(0.9, 0.95, 1.0, coma_alpha * 0.5);
                    gizmos.line(body_pos, tip, color);
                }

                // Sunward jet (brighter toward sun)
                let jet_length = coma_radius * 1.5;
                let jet_tip = body_pos - anti_sun_dir * jet_length; // toward sun
                let jet_color = Color::srgba(0.95, 0.95, 1.0, coma_alpha * 0.7);
                gizmos.line(body_pos, jet_tip, jet_color);
            }
        }
    }
}

/// Perihelion distance (in AU) at which ISON disintegrates
/// Historical: ISON broke apart at perihelion on Nov 28, 2013, at ~0.0124 AU from the Sun center.
/// The nucleus was already fragmenting before perihelion passage.
const ISON_DESTRUCTION_DISTANCE_AU: f64 = 0.0125;

/// Distance (in AU) below which any comet would be destroyed by solar heating.
/// The Roche limit for a low-density body near the Sun is roughly 0.009 AU (about 2
/// solar radii from the center). Bodies reaching this distance are tidally disrupted.
const COMET_GENERAL_DESTRUCTION_AU: f64 = 0.009;

/// System that checks for natural destruction events (e.g., Comet ISON solar disintegration).
/// This system monitors comets approaching the sun and triggers destruction for historically
/// accurate events like ISON's breakup, as well as any comet reaching the solar Roche limit.
pub fn check_natural_destruction(
    mut commands: Commands,
    sim_time: Res<SimulationTime>,
    query: Query<(Entity, &CelestialBody, &SpaceCoordinates), (With<Comet>, Without<Destroyed>)>,
) {
    for (entity, body, coords) in query.iter() {
        let distance_au = coords.position.length();

        // Check for ISON specifically - historically disintegrated near perihelion in Nov 2013
        if body.name == "Comet ISON" && distance_au < ISON_DESTRUCTION_DISTANCE_AU {
            info!(
                "Comet ISON disintegrating due to solar proximity at {:.4} AU",
                distance_au
            );
            commands.entity(entity).insert(Destroyed::new(
                sim_time.elapsed_seconds(),
                2.0, // 2 second fade-out
            ));
            continue;
        }

        // Any comet reaching the solar Roche limit is tidally disrupted
        if distance_au < COMET_GENERAL_DESTRUCTION_AU {
            info!(
                "{} destroyed by tidal forces at {:.4} AU from the Sun",
                body.name, distance_au
            );
            commands
                .entity(entity)
                .insert(Destroyed::new(sim_time.elapsed_seconds(), 1.5));
        }

        // Additional destruction checks can be added here for other scenarios:
        // - Mining operations completing
        // - Weapon impacts
        // - Orbital decay into planets
        // - Collision events
    }
}

/// System that fades out and despawns destroyed celestial bodies.
/// Bodies fade out over their specified duration, then are removed from the simulation along
/// with any child entities (markers, trails, etc.).
pub fn fade_destroyed_bodies(
    mut commands: Commands,
    sim_time: Res<SimulationTime>,
    mut query: Query<(Entity, &Destroyed, Option<&mut Visibility>, &Children), With<CelestialBody>>,
    child_query: Query<Entity>,
) {
    let current_time = sim_time.elapsed_seconds();

    for (entity, destroyed, visibility, children) in query.iter_mut() {
        let elapsed = current_time - destroyed.destruction_time;

        if destroyed.fade_duration <= 0.0 || elapsed >= destroyed.fade_duration {
            // Fade complete or instant destruction - despawn the entity and all children
            info!("Despawning destroyed celestial body (entity {:?})", entity);

            // Despawn all children first (markers, trails, etc.)
            for child in children.iter() {
                if let Ok(child_entity) = child_query.get(child) {
                    commands.entity(child_entity).despawn();
                }
            }

            // Despawn the body itself
            commands.entity(entity).despawn();
        } else if let Some(mut vis) = visibility {
            // During fade-out, gradually hide the body
            // Could also modify alpha/emissive here if we add that capability
            let fade_progress = elapsed / destroyed.fade_duration;
            if fade_progress > 0.8 {
                *vis = Visibility::Hidden;
            }
        }
    }
}

/// System that controls orbit visibility based on body type and camera anchor.
///
/// Moon orbits are only shown when their parent planet is the camera's anchor,
/// the moon itself is the camera's anchor, their parent planet is selected,
/// or the selected fleet orbits their parent planet.
/// Asteroid/DwarfPlanet/Comet orbits are shown when their ledger category group
/// is expanded in the left ledger panel.
pub fn update_orbit_visibility(
    launch_state: Res<LaunchState>,
    view_mode: Res<ViewMode>,
    camera_query: Query<&CameraAnchor, With<GameCamera>>,
    mut orbit_query: Query<(
        Entity,
        &mut OrbitPath,
        Option<&Selected>,
        Option<&Planet>,
        Option<&Star>,
        Option<&Moon>,
        Option<&LogicalParent>,
        Option<&CelestialBody>,
    )>,
    selected_query: Query<(), With<Selected>>,
    fleet_ui_state: Res<crate::ui::FleetUiState>,
    fleet_orbit_query: Query<&crate::fleets::FleetOrbit, With<crate::fleets::Fleet>>,
    fleet_maneuver_query: Query<&crate::fleets::ActiveManeuver, With<crate::fleets::Fleet>>,
    expanded_groups: Res<crate::ui::ExpandedLedgerGroups>,
    // Read-only pre-pass to detect which category groups have a selected body.
    selected_category_query: Query<(&CelestialBody, Option<&LogicalParent>), With<Selected>>,
    // Used to look up body type and parent of any entity (e.g. parent of a selected moon).
    all_body_parents: Query<(&CelestialBody, Option<&LogicalParent>)>,
) {
    // When the menu owns the scene, `hide_in_game_solar_system` sets
    // every body's `Visibility::Hidden` (and `draw_orbit_paths` skips
    // hidden bodies via its `Visibility` check). Running the per-frame
    // "always visible" planet/star flip here would re-set the
    // `OrbitPath::visible = true` flag we use to gate rendering —
    // not a correctness bug, but it costs a query mutation cycle per
    // frame and the data is meaningless while the menu is up.
    if !launch_state.is_in_game() {
        return;
    }

    let Ok(anchor) = camera_query.single() else {
        return;
    };

    // Determine which body the selected fleet is currently orbiting (if any).
    let selected_fleet_orbit_body = fleet_ui_state
        .selected_fleet
        .and_then(|fe| fleet_orbit_query.get(fe).ok())
        .map(|fo| fo.body);

    // Determine the origin/destination bodies of an active transit (if any).
    let selected_fleet_transit_bodies = fleet_ui_state
        .selected_fleet
        .and_then(|fe| fleet_maneuver_query.get(fe).ok())
        .map(|m| (m.origin_body, m.destination_body));

    // Pre-pass: build a set of (parent_entity, group_name) pairs whose category
    // currently has a selected member.  When such a pair is in this set, all
    // OTHER bodies in the same group should hide their orbit so only the
    // selected/highlighted body stands out.
    let mut groups_with_selection: std::collections::HashSet<(Entity, &'static str)> =
        std::collections::HashSet::new();
    // Set of dwarf-planet entities that are the direct parent of a currently-selected moon.
    // These should always show their orbit (like a "parent selected" rule).
    let mut selected_moon_parent_dwarfs: std::collections::HashSet<Entity> =
        std::collections::HashSet::new();
    for (cb, lp) in selected_category_query.iter() {
        let group: Option<&'static str> = match cb.body_type {
            BodyType::DwarfPlanet => Some("Dwarf Planets"),
            BodyType::Asteroid => Some("Asteroids"),
            BodyType::Comet => Some("Comets"),
            _ => None,
        };
        if let (Some(group), Some(parent_e)) = (group, lp.map(|l| l.0)) {
            groups_with_selection.insert((parent_e, group));
        }
        // If a Moon is selected whose parent is a DwarfPlanet, treat the parent
        // dwarf planet like a "parent selected" body: show only its orbit and hide
        // all other dwarf planet orbits in the same group.
        if cb.body_type == BodyType::Moon {
            if let Some(parent_e) = lp.map(|l| l.0) {
                if let Ok((parent_cb, parent_lp)) = all_body_parents.get(parent_e) {
                    if parent_cb.body_type == BodyType::DwarfPlanet {
                        selected_moon_parent_dwarfs.insert(parent_e);
                        // Suppress sibling dwarf planets by adding the grandparent
                        // (the star) + group key to groups_with_selection.
                        if let Some(star_e) = parent_lp.map(|l| l.0) {
                            groups_with_selection.insert((star_e, "Dwarf Planets"));
                        }
                    }
                }
            }
        }
    }

    for (entity, mut orbit_path, selected, planet, star, moon, logical_parent, celestial_body) in
        orbit_query.iter_mut()
    {
        // Hide all orbits in starmap view
        if *view_mode == ViewMode::Starmap {
            orbit_path.visible = false;
            continue;
        }

        if selected.is_some() {
            // Selected bodies always show their orbit
            orbit_path.visible = true;
        } else if planet.is_some() || star.is_some() {
            // Planets and orbiting stars always show their orbit.
            orbit_path.visible = true;
        } else if moon.is_some() {
            let parent_entity = logical_parent.map(|lp| lp.0);
            // Show moon orbits when the parent planet is:
            // 1. the camera anchor
            let parent_anchored =
                anchor.0.is_some() && parent_entity.map(|e| Some(e) == anchor.0).unwrap_or(false);
            // Also show when the moon itself is the camera anchor
            let self_anchored = anchor.0 == Some(entity);
            // 2. selected
            let parent_selected = parent_entity
                .map(|e| selected_query.contains(e))
                .unwrap_or(false);
            // 3. the orbit target of the currently selected fleet (parent or the moon itself)
            let fleet_orbits_parent = parent_entity
                .map(|e| Some(e) == selected_fleet_orbit_body)
                .unwrap_or(false);
            let fleet_orbits_self = selected_fleet_orbit_body == Some(entity);
            // 4. the selected fleet is transiting to/from this moon or its parent
            let fleet_transits_self = selected_fleet_transit_bodies
                .map(|(o, d)| o == entity || d == entity)
                .unwrap_or(false);
            let fleet_transits_parent = parent_entity
                .map(|pe| {
                    selected_fleet_transit_bodies
                        .map(|(o, d)| o == pe || d == pe)
                        .unwrap_or(false)
                })
                .unwrap_or(false);
            orbit_path.visible = parent_anchored
                || self_anchored
                || parent_selected
                || fleet_orbits_parent
                || fleet_orbits_self
                || fleet_transits_self
                || fleet_transits_parent;
        } else {
            // Asteroids, Comets, DwarfPlanets: show orbits when their ledger
            // category group is currently expanded in the left panel — BUT only
            // when no sibling in that group is selected.  Once one is selected
            // it stands out via the `selected.is_some()` branch above; all
            // others are suppressed so the view isn't cluttered.
            let group_name: Option<&'static str> = match celestial_body.map(|cb| &cb.body_type) {
                Some(BodyType::DwarfPlanet) => Some("Dwarf Planets"),
                Some(BodyType::Asteroid) => Some("Asteroids"),
                Some(BodyType::Comet) => Some("Comets"),
                _ => None,
            };
            orbit_path.visible = match (group_name, logical_parent.map(|lp| lp.0)) {
                (Some(group), Some(parent_e)) => {
                    // Always show the dwarf planet whose moon is currently selected
                    // (mirrors the parent_selected rule used for regular moons).
                    if selected_moon_parent_dwarfs.contains(&entity) {
                        true
                    // Hide if a sibling is selected (they are handled by the
                    // `selected.is_some()` branch and already shown).
                    } else if groups_with_selection.contains(&(parent_e, group)) {
                        false
                    } else {
                        expanded_groups
                            .groups
                            .contains(&(parent_e, group.to_string()))
                    }
                }
                _ => false,
            };
        }
    }
}

/// System that toggles moon mesh visibility based on camera anchor.
///
/// Moons are only visible when their parent planet is the camera's anchor,
/// the moon itself is the camera's anchor, the parent is selected, or a
/// selected fleet orbits the parent/moon. This prevents overlapping moon
/// systems from different planets from cluttering the view.
///
/// Also respects the current star system — bodies from other systems are
/// left hidden even if selected or anchored.
pub fn update_body_lod_visibility(
    launch_state: Res<LaunchState>,
    camera_query: Query<&CameraAnchor, With<GameCamera>>,
    current_system: Res<CurrentStarSystem>,
    mut body_query: Query<
        (
            Entity,
            &mut Visibility,
            Option<&LogicalParent>,
            Option<&Moon>,
            Option<&Selected>,
            Option<&SystemId>,
        ),
        With<CelestialBody>,
    >,
    selected_bodies: Query<(), With<Selected>>,
    fleet_ui_state: Res<crate::ui::FleetUiState>,
    fleet_orbit_query: Query<&crate::fleets::FleetOrbit, With<crate::fleets::Fleet>>,
    fleet_maneuver_query: Query<&crate::fleets::ActiveManeuver, With<crate::fleets::Fleet>>,
) {
    let Ok(anchor) = camera_query.single() else {
        return;
    };

    // Determine which body the selected fleet is currently orbiting (if any).
    let selected_fleet_orbit_body = fleet_ui_state
        .selected_fleet
        .and_then(|fe| fleet_orbit_query.get(fe).ok())
        .map(|fo| fo.body);

    // Determine the origin/destination bodies of an active transit (if any).
    let selected_fleet_transit_bodies = fleet_ui_state
        .selected_fleet
        .and_then(|fe| fleet_maneuver_query.get(fe).ok())
        .map(|m| (m.origin_body, m.destination_body));

    // GRA-XYZ: when the menu owns the scene, `hide_in_game_solar_system`
    // (in `MenuBackdropPlugin`) sets every in-game body to
    // `Visibility::Hidden` so the backdrop's Earth is the only visible
    // object. If we ran here, this per-frame "current system →
    // Inherited, other → Hidden" flip would fight that hide every tick
    // — the moons visibly flicker between Hidden and Inherited on the
    // transition frame and planets (which fall through to the
    // "always visible" branch at the bottom of the loop) keep rendering
    // through the menu. Yield to the menu until it exits.
    if !launch_state.is_in_game() {
        return;
    }

    for (entity, mut visibility, logical_parent, moon, selected, system_id) in body_query.iter_mut()
    {
        // Bodies from other star systems must stay hidden, regardless of
        // selection or anchor state.
        let body_system = system_id.map(|s| s.0).unwrap_or(0);
        if body_system != current_system.0 {
            *visibility = Visibility::Hidden;
            continue;
        }

        // Selected bodies in the current system are always visible
        if selected.is_some() {
            *visibility = Visibility::Inherited;
            continue;
        }

        if moon.is_some() {
            // Moon visibility: only when parent planet is the camera anchor,
            // the parent planet is selected, or the selected fleet orbits the
            // parent planet or the moon itself.
            let parent_entity = logical_parent.map(|lp| lp.0);
            let parent_anchored =
                anchor.0.is_some() && parent_entity.map(|e| Some(e) == anchor.0).unwrap_or(false);
            // Also show when the moon itself is the camera anchor
            let self_anchored = anchor.0 == Some(entity);
            let parent_selected = parent_entity
                .map(|e| selected_bodies.contains(e))
                .unwrap_or(false);
            let fleet_orbits_parent = parent_entity
                .map(|e| Some(e) == selected_fleet_orbit_body)
                .unwrap_or(false);
            let fleet_orbits_self = selected_fleet_orbit_body == Some(entity);
            // Also show when the selected fleet is transiting to/from this moon or its parent.
            let fleet_transits_self = selected_fleet_transit_bodies
                .map(|(o, d)| o == entity || d == entity)
                .unwrap_or(false);
            let fleet_transits_parent = parent_entity
                .map(|pe| {
                    selected_fleet_transit_bodies
                        .map(|(o, d)| o == pe || d == pe)
                        .unwrap_or(false)
                })
                .unwrap_or(false);
            *visibility = if parent_anchored
                || self_anchored
                || parent_selected
                || fleet_orbits_parent
                || fleet_orbits_self
                || fleet_transits_self
                || fleet_transits_parent
            {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
        // Planets, Stars, Asteroids, Dwarf Planets: always visible
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_solve_kepler_circular_orbit() {
        // For circular orbit (e=0), eccentric anomaly should equal mean anomaly
        let mean_anomaly = std::f64::consts::PI / 4.0; // 45 degrees
        let eccentricity = 0.0;
        let result = solve_kepler(mean_anomaly, eccentricity);
        assert!((result - mean_anomaly).abs() < 1e-10);
    }

    #[test]
    fn test_solve_kepler_eccentric_orbit() {
        // Test with Earth's eccentricity (e ≈ 0.0167)
        let mean_anomaly = std::f64::consts::PI / 2.0; // 90 degrees
        let eccentricity = 0.0167;
        let eccentric_anomaly = solve_kepler(mean_anomaly, eccentricity);

        // Verify Kepler's equation: M = E - e*sin(E)
        let calculated_mean = eccentric_anomaly - eccentricity * eccentric_anomaly.sin();
        assert!((calculated_mean - mean_anomaly).abs() < KEPLER_TOLERANCE);
    }

    #[test]
    fn test_solve_kepler_high_eccentricity() {
        // Test with higher eccentricity (e = 0.8)
        let mean_anomaly = std::f64::consts::PI;
        let eccentricity = 0.8;
        let eccentric_anomaly = solve_kepler(mean_anomaly, eccentricity);

        // Verify Kepler's equation
        let calculated_mean = eccentric_anomaly - eccentricity * eccentric_anomaly.sin();
        assert!((calculated_mean - mean_anomaly).abs() < KEPLER_TOLERANCE);
    }

    #[test]
    fn test_eccentric_to_true_anomaly_circular() {
        // For circular orbit, true anomaly should equal eccentric anomaly
        let eccentric_anomaly = std::f64::consts::PI / 3.0;
        let eccentricity = 0.0;
        let true_anomaly = eccentric_to_true_anomaly(eccentric_anomaly, eccentricity);
        assert!((true_anomaly - eccentric_anomaly).abs() < 1e-10);
    }

    #[test]
    fn test_orbital_radius_circular() {
        // For circular orbit at any true anomaly, radius should equal semi-major axis
        let semi_major_axis = 1.0;
        let eccentricity = 0.0;
        let true_anomaly = std::f64::consts::PI / 4.0;
        let radius = orbital_radius(semi_major_axis, eccentricity, true_anomaly);
        assert!((radius - semi_major_axis).abs() < 1e-10);
    }

    #[test]
    fn test_orbital_radius_periapsis_apoapsis() {
        // Test periapsis and apoapsis distances
        let semi_major_axis = 1.0;
        let eccentricity = 0.5;

        // At periapsis (true anomaly = 0), r = a(1-e)
        let periapsis_distance = orbital_radius(semi_major_axis, eccentricity, 0.0);
        let expected_periapsis = semi_major_axis * (1.0 - eccentricity);
        assert!((periapsis_distance - expected_periapsis).abs() < 1e-10);

        // At apoapsis (true anomaly = π), r = a(1+e)
        let apoapsis_distance = orbital_radius(semi_major_axis, eccentricity, std::f64::consts::PI);
        let expected_apoapsis = semi_major_axis * (1.0 + eccentricity);
        assert!((apoapsis_distance - expected_apoapsis).abs() < 1e-10);
    }

    #[test]
    fn test_orbit_path_segments_scale_with_render_size() {
        let path = OrbitPath::with_segments(Color::WHITE, 128);
        let planet_scale_orbit = KeplerOrbit::circular(1.0, 0.0);
        let wide_star_orbit = KeplerOrbit::circular(50.0, 0.0);

        let small_segments = orbit_path_segments(&path, &planet_scale_orbit, 1.0);
        let large_segments = orbit_path_segments(&path, &wide_star_orbit, 1.0);

        assert_eq!(small_segments, 128);
        assert!(large_segments > small_segments);
    }

    #[test]
    fn test_orbit_path_segments_respect_upper_bound() {
        let path = OrbitPath::with_segments(Color::WHITE, 128);
        let huge_orbit = KeplerOrbit::circular(5_000.0, 0.0);

        assert_eq!(
            orbit_path_segments(&path, &huge_orbit, 1.0),
            MAX_ORBIT_PATH_SEGMENTS
        );
    }

    #[test]
    fn test_propagate_orbits_system() {
        // Create a test app
        let mut app = App::new();
        app.init_resource::<SimulationTime>();
        app.add_systems(Update, propagate_orbits);

        // Spawn an entity with circular orbit
        let orbit = KeplerOrbit::circular(1.0, std::f64::consts::TAU); // 1 AU, 1 radian/second
        let coords = SpaceCoordinates::default();
        app.world_mut().spawn((orbit, coords));

        // Advance simulation time so orbit has moved
        app.world_mut().resource_mut::<SimulationTime>().elapsed = 0.1;

        // Run one update
        app.update();

        // Verify the entity was processed (coordinates should be updated)
        let mut query = app.world_mut().query::<&SpaceCoordinates>();
        let coords = query.iter(app.world()).next().unwrap();
        // For a circular orbit with elapsed > 0, position should have moved from origin
        assert!(coords.position.x.abs() > 0.0 || coords.position.y.abs() > 0.0);
    }

    #[test]
    fn test_propagate_orbits_updates_deep_hierarchy_in_order() {
        let mut app = App::new();
        app.init_resource::<SimulationTime>();
        app.add_systems(Update, propagate_orbits);

        let root_anchor = app
            .world_mut()
            .spawn(SpaceCoordinates::new(DVec3::new(10.0, 0.0, 0.0)))
            .id();

        let star = app
            .world_mut()
            .spawn((
                KeplerOrbit::new(0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0),
                SpaceCoordinates::default(),
                OrbitCenter(root_anchor),
            ))
            .id();

        let planet = app
            .world_mut()
            .spawn((
                KeplerOrbit::new(0.0, 0.5, 0.0, 0.0, 0.0, std::f64::consts::FRAC_PI_2, 0.0),
                SpaceCoordinates::default(),
                OrbitCenter(star),
            ))
            .id();

        let moon = app
            .world_mut()
            .spawn((
                KeplerOrbit::new(0.0, 0.1, 0.0, 0.0, 0.0, std::f64::consts::PI, 0.0),
                SpaceCoordinates::default(),
                OrbitCenter(planet),
            ))
            .id();

        app.update();

        let star_pos = app.world().get::<SpaceCoordinates>(star).unwrap().position;
        let planet_pos = app
            .world()
            .get::<SpaceCoordinates>(planet)
            .unwrap()
            .position;
        let moon_pos = app.world().get::<SpaceCoordinates>(moon).unwrap().position;

        assert!((star_pos - DVec3::new(12.0, 0.0, 0.0)).length() < 1e-10);
        assert!((planet_pos - DVec3::new(12.0, 0.5, 0.0)).length() < 1e-10);
        assert!((moon_pos - DVec3::new(11.9, 0.5, 0.0)).length() < 1e-10);
    }

    #[test]
    fn test_update_orbit_visibility_keeps_orbiting_star_paths_visible() {
        let mut app = App::new();
        app.insert_resource(crate::ui::launch::LaunchState::InGame);
        app.init_resource::<ViewMode>();
        app.init_resource::<crate::ui::FleetUiState>();
        app.init_resource::<crate::ui::ExpandedLedgerGroups>();
        app.add_systems(Update, update_orbit_visibility);

        app.world_mut().spawn((GameCamera, CameraAnchor(None)));

        let star = app
            .world_mut()
            .spawn((
                Star,
                OrbitPath {
                    color: Color::WHITE,
                    visible: false,
                    segments: 128,
                    fade_exponent: 1.8,
                },
            ))
            .id();

        app.update();

        assert!(
            app.world().get::<OrbitPath>(star).unwrap().visible,
            "orbiting stars should keep their orbit path visible like planets"
        );
    }

    #[test]
    fn test_update_render_transform_scaling() {
        // Test that the transform system correctly scales coordinates
        let mut app = App::new();
        app.add_plugins(bevy::time::TimePlugin);
        app.init_resource::<crate::ui::SimulationTime>();
        app.init_resource::<crate::ui::TimeScale>();
        app.add_systems(Update, update_render_transform);

        // Spawn entity with known space coordinates
        let coords = SpaceCoordinates::new(DVec3::new(1.0, 2.0, 3.0)); // In AU
        let transform = Transform::default();
        app.world_mut().spawn((coords, transform));

        // Run one update
        app.update();

        // Verify transform was updated with scaled values
        let mut query = app.world_mut().query::<&Transform>();
        let transform = query.iter(app.world()).next().unwrap();

        // Should be scaled by SCALING_FACTOR
        let expected = Vec3::new(
            (1.0 * SCALING_FACTOR) as f32,
            (2.0 * SCALING_FACTOR) as f32,
            (3.0 * SCALING_FACTOR) as f32,
        );
        assert!((transform.translation - expected).length() < 1e-5);
    }

    #[test]
    fn test_sync_floating_origin_to_anchor_uses_anchor_position() {
        let mut app = App::new();
        app.init_resource::<CurrentStarSystem>();
        app.init_resource::<FloatingOrigin>();
        app.init_resource::<ViewMode>();
        app.add_systems(Update, sync_floating_origin_to_anchor);

        let anchor_entity = app
            .world_mut()
            .spawn((
                SpaceCoordinates::new(DVec3::new(12345.0, -67.0, 8.5)),
                SystemId(3),
            ))
            .id();

        app.world_mut()
            .spawn((GameCamera, CameraAnchor(Some(anchor_entity))));
        app.world_mut().resource_mut::<CurrentStarSystem>().0 = 3;
        *app.world_mut().resource_mut::<ViewMode>() = ViewMode::System;

        app.update();

        let origin = app.world().resource::<FloatingOrigin>();
        assert_eq!(origin.position, DVec3::new(12345.0, -67.0, 8.5));
    }
}
