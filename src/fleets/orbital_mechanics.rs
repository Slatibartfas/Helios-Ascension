//! Orbital mechanics calculations for the fleet transfer system.
//!
//! Provides Hohmann transfer computations, multi-option transfer planning,
//! and the Tsiolkovsky rocket equation for fuel estimation.

/// Gravitational parameter of the Sun (m³ s⁻²)
pub const GM_SUN: f64 = 1.327_124_4e20;

/// Newtonian gravitational constant (m³ kg⁻¹ s⁻²)
pub const G_CONST: f64 = 6.674e-11;

/// Metres per Astronomical Unit
pub const AU_IN_METERS: f64 = 1.495_978_707e11;

/// Standard gravity (m s⁻²) — used in the rocket equation
pub const G0: f64 = 9.806_65;

/// A transfer trajectory option for the player to choose between.
#[derive(Debug, Clone)]
pub struct TransferOption {
    /// Human-readable label shown in the UI.
    pub label: &'static str,
    /// Total Δv required (both burns combined) in m/s.
    pub total_delta_v_ms: f64,
    /// Departure burn Δv in m/s.
    pub delta_v1_ms: f64,
    /// Arrival (circularisation) burn Δv in m/s.
    pub delta_v2_ms: f64,
    /// Transfer duration in seconds.
    pub transfer_time_s: f64,
    /// Transfer orbit semi-major axis in AU.
    pub sma_au: f64,
    /// Transfer orbit eccentricity.
    pub eccentricity: f64,
    /// Energy multiplier relative to Hohmann minimum (1.0 = most efficient).
    pub energy_multiplier: f64,
    /// Total powered burn time (seconds) for a fleet with the given thrust profile.
    ///
    /// Set to `0.0` when no fleet propulsion data is available (pure geometry
    /// computations).  The UI fills this in after computing options by calling
    /// [`compute_burn_time_s`] with the fleet's actual min-acceleration and
    /// average specific impulse.
    pub burn_time_s: f64,
}

/// Information about the next optimal Hohmann launch window between two bodies.
///
/// Because the two bodies orbit at different rates, there is only one optimal
/// departure geometry (the Hohmann phase angle) every synodic period.
/// Departing outside this window requires extra ΔV to compensate.
#[derive(Debug, Clone)]
pub struct TransferWindowInfo {
    /// Seconds from **now** until the next optimal Hohmann departure window.
    /// `0.0` means the window is open right now (or orbits are identical).
    pub time_to_window_s: f64,
    /// Time between consecutive optimal windows (synodic period, seconds).
    /// `f64::INFINITY` when origin and destination share the same SMA.
    pub synodic_period_s: f64,
    /// Signed phase-angle error **right now** in radians, range [−π, π].
    /// `0.0` = perfectly at an optimal window; `±π` = worst possible geometry.
    pub phase_error_now_rad: f64,
    /// Rate of change of the (dest − origin) phase angle, radians per second.
    /// Positive means the destination is gaining angular lead over the origin.
    pub phase_rate_rad_s: f64,
}

/// Compute the next optimal Hohmann launch window for a co-planar circular
/// orbit transfer.
///
/// - `r1_au`, `r2_au`: semi-major axes of origin and destination (AU).
/// - `gm`: gravitational parameter of the central body (m³ s⁻²).
/// - `theta1_rad`: **current** heliocentric angle of the origin body (radians).
/// - `theta2_rad`: **current** heliocentric angle of the destination body (radians).
///
/// The returned [`TransferWindowInfo`] is accurate for the instant the angles
/// were sampled; call once per frame to get live values.
pub fn compute_transfer_window(
    r1_au: f64,
    r2_au: f64,
    gm: f64,
    theta1_rad: f64,
    theta2_rad: f64,
) -> TransferWindowInfo {
    use std::f64::consts::{PI, TAU};

    if (r1_au - r2_au).abs() < 1e-9 {
        return TransferWindowInfo {
            time_to_window_s: 0.0,
            synodic_period_s: f64::INFINITY,
            phase_error_now_rad: 0.0,
            phase_rate_rad_s: 0.0,
        };
    }

    let r1 = r1_au * AU_IN_METERS;
    let r2 = r2_au * AU_IN_METERS;

    // Mean motions for circular-orbit approximation
    let n1 = (gm / r1.powi(3)).sqrt(); // rad/s  (origin)
    let n2 = (gm / r2.powi(3)).sqrt(); // rad/s  (destination)

    // Hohmann transfer time (half the period of the transfer ellipse)
    let a = (r1 + r2) / 2.0;
    let t_h = PI * (a.powi(3) / gm).sqrt();

    // Required phase angle at departure: φ_dest − φ_origin = π − n₂·T
    // (Valid for both inward and outward Hohmann transfers.)
    let phi_req = (PI - n2 * t_h).rem_euclid(TAU);

    // Current phase angle (destination − origin), normalised to [0, 2π)
    let phi_curr = (theta2_rad - theta1_rad).rem_euclid(TAU);

    // Rate at which the phase angle changes: d(θ₂−θ₁)/dt = n₂ − n₁
    let d_phi_dt = n2 - n1;

    let synodic_period_s = if d_phi_dt.abs() < 1e-25 {
        f64::INFINITY
    } else {
        TAU / d_phi_dt.abs()
    };

    // Time until next window: solve φ_curr + d_phi_dt·t ≡ φ_req  (mod 2π)
    let delta_phi = phi_req - phi_curr;
    let time_to_window_s = if d_phi_dt.abs() < 1e-25 || synodic_period_s.is_infinite() {
        0.0
    } else {
        (delta_phi / d_phi_dt).rem_euclid(synodic_period_s)
    };

    // Signed phase error right now, normalised to [−π, π]
    let raw_err = phi_curr - phi_req;
    let phase_error_now_rad = ((raw_err + PI).rem_euclid(TAU)) - PI;

    TransferWindowInfo {
        time_to_window_s,
        synodic_period_s,
        phase_error_now_rad,
        phase_rate_rad_s: d_phi_dt,
    }
}

/// ΔV correction factor for a departure with phase-angle error `delta_phi_rad`.
///
/// | Phase error | Factor |
/// |-------------|--------|
/// | 0 (optimal) | 1.0    |
/// | π/2         | 1.35   |
/// | π (worst)   | 2.4    |
/// | 2π (optimal again) | 1.0 |
pub fn phase_dv_factor(delta_phi_rad: f64) -> f64 {
    let s = (delta_phi_rad / 2.0).sin();
    1.0 + 1.4 * s * s
}

// ── Gravity-assist slingshot trajectories ────────────────────────────────────

/// Minimum relative velocity (m/s) at the flyby body for the encounter to be
/// worth including as a gravity-assist candidate.  Below this threshold the
/// hyperbolic flyby deflection angle is negligible and the assist provides no
/// meaningful benefit.
const MIN_VIABLE_V_INF_MS: f64 = 50.0;
///
/// Produced by [`compute_gravity_assist`] for a single flyby body and by
/// [`find_gravity_assist_options`] for all candidates on a given route.
#[derive(Debug, Clone)]
pub struct GravityAssistOption {
    /// Display name of the flyby body (e.g. "Jupiter").
    pub body_name: String,
    /// Heliocentric orbit radius of the flyby body (AU).
    pub flyby_radius_au: f64,
    /// Hyperbolic excess velocity at closest approach (m/s).
    pub v_inf_ms: f64,
    /// Maximum ΔV the gravity assist can provide (m/s).
    pub max_dv_assist_ms: f64,
    /// Total ΔV for the full two-leg assisted trajectory (m/s).
    pub total_dv_ms: f64,
    /// ΔV savings vs a direct Hohmann (positive = assist saves propellant, m/s).
    pub dv_savings_ms: f64,
    /// Total travel time for the assisted trajectory (seconds).
    pub total_time_s: f64,
    /// Extra travel time vs the direct Hohmann (seconds; negative means shorter).
    pub extra_time_s: f64,
    /// Approximate alignment-window repeat period — the synodic period of the
    /// origin body with respect to the flyby body (seconds).
    pub window_period_s: f64,
    /// Half-period of the Leg 1 transfer ellipse (origin → flyby, seconds).
    /// Used by the render system to predict the flyby body position at intercept.
    pub leg1_time_s: f64,
    /// Half-period of the Leg 2 transfer ellipse (flyby → destination, seconds).
    pub leg2_time_s: f64,
    // ── Individual burn breakdown (used by the execute logic) ─────────────────
    /// Departure burn ΔV at the origin (m/s).
    pub dv_depart_ms: f64,
    /// Mid-course correction at the flyby node (m/s); 0 when the kick is sufficient.
    pub dv_mid_ms: f64,
    /// Arrival circularisation burn at the destination (m/s).
    pub dv_arrive_ms: f64,
}

/// Compute a single-flyby gravity-assist trajectory from `r1_au` to `r2_au`
/// via an intermediate flyby body at `r_fly_au`.
///
/// Uses a **two-leg patched-conic** (Hohmann + hyperbolic flyby) approximation:
///
/// 1. **Leg 1**: spacecraft travels on a Hohmann semi-arc from r1 to r_fly.
/// 2. **Flyby**: hyperbolic encounter at r_fly; maximum ΔV kick limited by
///    `min_periapsis_au` (minimum safe closest-approach distance).
/// 3. **Leg 2**: spacecraft continues from r_fly to r2 using the post-flyby
///    velocity; a mid-course correction burn is added if needed.
///
/// All distance parameters in AU; `gm` / `gm_planet` in m³ s⁻².
pub fn compute_gravity_assist(
    r1_au: f64,
    r2_au: f64,
    r_fly_au: f64,
    gm: f64,
    gm_planet: f64,
    flyby_body_name: String,
    min_periapsis_au: f64,
) -> GravityAssistOption {
    let r1 = r1_au * AU_IN_METERS;
    let r2 = r2_au * AU_IN_METERS;
    let r_fly = r_fly_au * AU_IN_METERS;
    let r_peri = min_periapsis_au * AU_IN_METERS;

    // Direct Hohmann for ΔV comparison
    let (dv_d1, dv_d2, t_direct, _, _) = hohmann_transfer(r1_au, r2_au, gm);
    let total_dv_direct = dv_d1 + dv_d2;

    // ── Leg 1: r1 → r_fly ────────────────────────────────────────────────────
    let a1 = (r1 + r_fly) / 2.0;
    let v_circ1 = (gm / r1).sqrt();
    let v_leg1_at_r1 = (gm * (2.0 / r1 - 1.0 / a1)).sqrt();
    let dv_depart = (v_leg1_at_r1 - v_circ1).abs();

    // Spacecraft velocity on the Leg 1 ellipse at the flyby radius
    let v_sc = (gm * (2.0 / r_fly - 1.0 / a1)).sqrt();
    // Flyby body's circular orbital velocity
    let v_planet = (gm / r_fly).sqrt();
    // Relative speed (v_inf) — always positive
    let v_inf = (v_sc - v_planet).abs();

    // ── Maximum gravity-assist kick ───────────────────────────────────────────
    // Deflection angle limited by minimum flyby periapsis:
    //   sin(δ/2) = 1 / (1 + r_peri × v_inf² / GM_planet)
    let term = if gm_planet > 0.0 { r_peri * v_inf * v_inf / gm_planet } else { 1e9 };
    let sin_half = 1.0 / (1.0 + term);
    let max_dv_assist = 2.0 * v_inf * sin_half;

    // ── Post-flyby spacecraft velocity ────────────────────────────────────────
    // Outward (r2 > r1): craft is slower than planet → trailing flyby adds speed.
    // Inward  (r2 < r1): craft is faster than planet → leading flyby removes speed.
    let outward = r2_au > r1_au;
    let v_after = if outward {
        v_sc + max_dv_assist
    } else {
        (v_sc - max_dv_assist).max(0.0)
    };

    // ── Leg 2: r_fly → r2 ────────────────────────────────────────────────────
    let a2 = (r_fly + r2) / 2.0;
    // Velocity needed at r_fly to enter a Hohmann arc to r2
    let v_need = (gm * (2.0 / r_fly - 1.0 / a2)).sqrt();
    // Mid-course correction burn if the assist over- or under-shoots
    let dv_mid = if outward {
        (v_need - v_after).max(0.0) // need more speed than assist provided
    } else {
        (v_after - v_need).max(0.0) // assist decelerated too much
    };
    // Circularisation at destination
    let v_circ2 = (gm / r2).sqrt();
    let v_dest = (gm * (2.0 / r2 - 1.0 / a2)).sqrt();
    let dv_arrive = (v_circ2 - v_dest).abs();

    // ── Aggregates ────────────────────────────────────────────────────────────
    let total_dv = dv_depart + dv_mid + dv_arrive;
    let dv_savings = total_dv_direct - total_dv;
    // Leg travel times (half-period of each transfer ellipse)
    let t_leg1 = std::f64::consts::PI * (a1.powi(3) / gm).sqrt();
    let t_leg2 = std::f64::consts::PI * (a2.powi(3) / gm).sqrt();
    let total_time = t_leg1 + t_leg2;
    let extra_time = total_time - t_direct;

    // Synodic period between origin and flyby body (alignment-window cadence)
    let n1 = (gm / r1.powi(3)).sqrt();
    let n_fly = (gm / r_fly.powi(3)).sqrt();
    let dn = (n1 - n_fly).abs();
    let window_period_s = if dn > 1e-25 { std::f64::consts::TAU / dn } else { f64::INFINITY };

    GravityAssistOption {
        body_name: flyby_body_name,
        flyby_radius_au: r_fly_au,
        v_inf_ms: v_inf,
        max_dv_assist_ms: max_dv_assist,
        total_dv_ms: total_dv,
        dv_savings_ms: dv_savings,
        total_time_s: total_time,
        extra_time_s: extra_time,
        window_period_s,
        leg1_time_s: t_leg1,
        leg2_time_s: t_leg2,
        dv_depart_ms: dv_depart,
        dv_mid_ms: dv_mid,
        dv_arrive_ms: dv_arrive,
    }
}

/// Find all single-flyby gravity-assist opportunities for a heliocentric
/// transfer from `r1_au` to `r2_au`.
///
/// Candidates are bodies whose orbit lies strictly between origin and destination.
/// All candidates are returned (including ones with negative ΔV savings) so the
/// player can understand why a particular flyby is not beneficial.
///
/// - `bodies`: `(name, sma_au, gm_planet, min_periapsis_au)` for each body.
///   `min_periapsis_au` should be ≈ 3 × body radius for a safe flyby.
pub fn find_gravity_assist_options(
    r1_au: f64,
    r2_au: f64,
    gm: f64,
    bodies: &[(String, f64, f64, f64)],
) -> Vec<GravityAssistOption> {
    let r_lo = r1_au.min(r2_au);
    let r_hi = r1_au.max(r2_au);
    bodies
        .iter()
        .filter(|(_, sma, _, _)| *sma > r_lo + 1e-4 && *sma < r_hi - 1e-4)
        .map(|(name, sma, gm_p, r_min)| {
            compute_gravity_assist(r1_au, r2_au, *sma, gm, *gm_p, name.clone(), *r_min)
        })
        .filter(|o| o.v_inf_ms > MIN_VIABLE_V_INF_MS) // exclude negligible-deflection encounters
        .collect()
}

/// Compute phase-aware transfer options for a planned departure.
///
/// This is the preferred alternative to [`calculate_transfer_options`] when
/// actual body positions are available.  The three options still represent
/// the Efficient / Moderate / Fast speed trade-off, but their ΔV now
/// includes a phase-correction penalty that **varies with the departure
/// time** and therefore updates live as the simulation clock advances.
///
/// - `departure_offset_s`: seconds from *now* until the fleet departs.
///   `0.0` means depart immediately.
/// - `window`: pre-computed window info for the current frame.
///
/// Phase sensitivity by option:
/// - **Efficient**: full correction (most sensitive — must wait for windows).
/// - **Moderate**: 65 % of correction (balanced).
/// - **Fast**: 30 % of correction (high-thrust can overcome bad geometry).
pub fn calculate_transfer_options_phased(
    r1_au: f64,
    r2_au: f64,
    gm: f64,
    departure_offset_s: f64,
    window: &TransferWindowInfo,
) -> Vec<TransferOption> {
    if (r1_au - r2_au).abs() < 1e-9 {
        return vec![TransferOption {
            label: "Same orbit",
            total_delta_v_ms: 0.0,
            delta_v1_ms: 0.0,
            delta_v2_ms: 0.0,
            transfer_time_s: 0.0,
            sma_au: r1_au,
            eccentricity: 0.0,
            energy_multiplier: 1.0,
            burn_time_s: 0.0,
        }];
    }

    let (dv1_h, dv2_h, t_h, sma_h, ecc_h) = hohmann_transfer(r1_au, r2_au, gm);

    // Phase angle at the chosen departure time:
    // phase(t) = phase_now + phase_rate · t
    // ⟹ phase_error(t) = phase_error_now + phase_rate · t
    let phase_at_dep = window.phase_error_now_rad
        + window.phase_rate_rad_s * departure_offset_s;
    // Normalise to [−π, π]
    let phase_at_dep = ((phase_at_dep + std::f64::consts::PI)
        .rem_euclid(std::f64::consts::TAU))
        - std::f64::consts::PI;

    // Full correction factor (for the Efficient option)
    let corr_full = phase_dv_factor(phase_at_dep.abs());

    // Efficient: most sensitive to phase angle
    let efficient = TransferOption {
        label: "Efficient",
        total_delta_v_ms: (dv1_h + dv2_h) * corr_full,
        delta_v1_ms: dv1_h * corr_full,
        delta_v2_ms: dv2_h * corr_full,
        transfer_time_s: t_h,
        sma_au: sma_h,
        eccentricity: ecc_h,
        energy_multiplier: corr_full,
        burn_time_s: 0.0,
    };

    // Moderate (1.5× Hohmann base): 65 % of phase correction
    let corr_mod = 1.0 + (corr_full - 1.0) * 0.65;
    let hohmann_base = TransferOption {
        label: "", // internal baseline — not returned to the user
        total_delta_v_ms: dv1_h + dv2_h,
        delta_v1_ms: dv1_h,
        delta_v2_ms: dv2_h,
        transfer_time_s: t_h,
        sma_au: sma_h,
        eccentricity: ecc_h,
        energy_multiplier: 1.0,
        burn_time_s: 0.0,
    };
    let moderate = scaled_transfer(&hohmann_base, 1.5 * corr_mod, "Moderate");

    // Fast (2.5× Hohmann base): 30 % of phase correction
    let corr_fast = 1.0 + (corr_full - 1.0) * 0.30;
    let fast = scaled_transfer(&hohmann_base, 2.5 * corr_fast, "Fast");

    vec![efficient, moderate, fast]
}

/// Compute all three standard transfer options (efficient, moderate, fast)
/// between two coplanar circular orbits around the same central body.
///
/// - `r1_au`: origin orbit semi-major axis in AU
/// - `r2_au`: destination orbit semi-major axis in AU
/// - `gm`: gravitational parameter of the central body in m³ s⁻²
///
/// Returns options ordered from least to most Δv.
/// **Prefer [`calculate_transfer_options_phased`] when actual body positions
/// are available**, as this version ignores launch-window geometry.
pub fn calculate_transfer_options(r1_au: f64, r2_au: f64, gm: f64) -> Vec<TransferOption> {
    // Degenerate case — same orbit
    if (r1_au - r2_au).abs() < 1e-9 {
        return vec![TransferOption {
            label: "Same orbit",
            total_delta_v_ms: 0.0,
            delta_v1_ms: 0.0,
            delta_v2_ms: 0.0,
            transfer_time_s: 0.0,
            sma_au: r1_au,
            eccentricity: 0.0,
            energy_multiplier: 1.0,
            burn_time_s: 0.0,
        }];
    }

    let (dv1_h, dv2_h, t_h, sma_h, ecc_h) = hohmann_transfer(r1_au, r2_au, gm);

    let efficient = TransferOption {
        label: "Efficient",
        total_delta_v_ms: dv1_h + dv2_h,
        delta_v1_ms: dv1_h,
        delta_v2_ms: dv2_h,
        transfer_time_s: t_h,
        sma_au: sma_h,
        eccentricity: ecc_h,
        energy_multiplier: 1.0,
        burn_time_s: 0.0,
    };

    // Moderate ≈ 1.5× Δv, ≈ 0.65× time
    let moderate = scaled_transfer(&efficient, 1.5, "Moderate");
    // Fast ≈ 2.5× Δv, ≈ 0.40× time
    let fast = scaled_transfer(&efficient, 2.5, "Fast");

    vec![efficient, moderate, fast]
}

/// Standard Hohmann transfer between two coplanar circular orbits.
///
/// Returns `(delta_v1_ms, delta_v2_ms, transfer_time_s, sma_au, eccentricity)`.
/// Both Δv values are positive magnitudes in m/s.
pub fn hohmann_transfer(r1_au: f64, r2_au: f64, gm: f64) -> (f64, f64, f64, f64, f64) {
    let r1 = r1_au * AU_IN_METERS;
    let r2 = r2_au * AU_IN_METERS;

    // Transfer ellipse semi-major axis (m)
    let a = (r1 + r2) / 2.0;
    let ecc = (r2 - r1).abs() / (r2 + r1);

    // Circular velocities at departure and arrival
    let v1_circ = (gm / r1).sqrt();
    let v2_circ = (gm / r2).sqrt();

    // Velocities on the transfer ellipse at periapsis and apoapsis (vis-viva)
    let v_peri = (gm * (2.0 / r1 - 1.0 / a)).sqrt();
    let v_apo = (gm * (2.0 / r2 - 1.0 / a)).sqrt();

    let dv1 = (v_peri - v1_circ).abs();
    let dv2 = (v2_circ - v_apo).abs();

    // Transfer time = half the period of the transfer ellipse (Kepler's third law)
    let t_transfer = std::f64::consts::PI * (a.powi(3) / gm).sqrt();

    (dv1, dv2, t_transfer, a / AU_IN_METERS, ecc)
}

/// Produce a higher-energy (faster, more expensive) transfer option by scaling
/// the Hohmann Δv budget by `energy_multiplier`.
///
/// Transfer time decreases approximately as `multiplier^(−2/3)`.
fn scaled_transfer(base: &TransferOption, energy_multiplier: f64, label: &'static str) -> TransferOption {
    let dv1 = base.delta_v1_ms * energy_multiplier;
    let dv2 = base.delta_v2_ms * energy_multiplier;

    // Time decreases as energy_multiplier^(-2/3) (Kepler scaling approximation)
    let time_factor = energy_multiplier.powf(-2.0 / 3.0);
    let time = base.transfer_time_s * time_factor;

    // Visual SMA for trajectory rendering — a shrunken/stretched ellipse
    let sma = base.sma_au * time_factor.powf(2.0 / 3.0);
    // Eccentricity — clamp to keep it a valid ellipse
    let eccentricity = (base.eccentricity * energy_multiplier.sqrt()).min(0.95);

    TransferOption {
        label,
        total_delta_v_ms: dv1 + dv2,
        delta_v1_ms: dv1,
        delta_v2_ms: dv2,
        transfer_time_s: time,
        sma_au: sma,
        eccentricity,
        energy_multiplier,
        burn_time_s: 0.0,
    }
}

/// Compute the total powered burn time (seconds) for a fleet to execute a
/// given Δv at its minimum achievable acceleration.
///
/// Derived from the Tsiolkovsky rocket equation time-integral:
/// `t = (1 − e^(−ΔV/vₑ)) × vₑ / a_min`
///
/// where `vₑ = avg_isp_s × g₀` is the effective exhaust velocity and
/// `a_min` is the fleet's minimum acceleration (weakest-ship bottleneck).
///
/// Returns 0.0 when any argument is non-positive.
pub fn compute_burn_time_s(total_dv_ms: f64, fleet_accel_ms2: f64, avg_isp_s: f32) -> f64 {
    if fleet_accel_ms2 <= 0.0 || total_dv_ms <= 0.0 || avg_isp_s <= 0.0 {
        return 0.0;
    }
    let ve = avg_isp_s as f64 * G0;
    let fuel_fraction = 1.0 - (-total_dv_ms / ve).exp();
    fuel_fraction * ve / fleet_accel_ms2
}

/// Build a "Flip & Burn" (brachistochrone) transfer option for a fleet
/// with sufficient thrust *and* propellant.
///
/// A flip-and-burn trajectory applies maximum thrust for the first half
/// of the trip (accelerating toward the target), then rotates 180° and
/// applies maximum thrust for the second half (decelerating to match the
/// destination's orbital velocity).  This minimises transit time for a
/// given thrust level at the cost of very high ΔV expenditure.
///
/// The chord-distance approximation `D ≈ |r₂ − r₁| × AU` is used.
///
/// # Parameters
/// - `r1_au`, `r2_au`: semi-major axes of origin and destination (AU)
/// - `gm`: gravitational parameter of the central body (m³/s²)
/// - `fleet_accel_ms2`: fleet minimum acceleration (m/s²) — use
///   [`Fleet::min_accel_ms2`]
/// - `_fleet_avg_isp_s`: fleet average specific impulse (s); reserved for
///   future partial-thrust models
/// - `fleet_max_dv_ms`: fleet total ΔV capacity (m/s) — use
///   [`Fleet::max_delta_v_ms`].  The option is suppressed when the
///   required brachistochrone ΔV exceeds this value, preventing infeasible
///   options from appearing (e.g. chemical rockets with large thrust but
///   tiny ΔV budgets).
///
/// # Returns
/// `None` when:
/// - the acceleration is below [`MIN_BRACH_ACCEL_MS2`] (0.05 m/s²), or
/// - the two orbits are identical, or
/// - the required ΔV exceeds `fleet_max_dv_ms`.
pub fn brachistochrone_option(
    r1_au: f64,
    r2_au: f64,
    gm: f64,
    fleet_accel_ms2: f64,
    _fleet_avg_isp_s: f32,
    fleet_max_dv_ms: f64,
) -> Option<TransferOption> {
    /// Minimum fleet acceleration (m/s²) to offer the Flip & Burn option.
    /// Set to 0.05 m/s² so that FusionTorch ships (≈ 0.01 g = 0.098 m/s²)
    /// always qualify while ion drives (≈ 0.005 m/s²) are excluded.
    const MIN_BRACH_ACCEL_MS2: f64 = 0.05;
    if fleet_accel_ms2 < MIN_BRACH_ACCEL_MS2 { return None; }

    let d = (r2_au - r1_au).abs() * AU_IN_METERS;
    if d < 1e6 { return None; }

    // Flip-and-burn kinematics (constant acceleration, chord-distance model):
    //   T = 2√(D/a)   ΔV = 2√(a·D)  (symmetric accel + decel half-burns)
    let dv_brach = 2.0 * (fleet_accel_ms2 * d).sqrt();
    let t_brach = 2.0 * (d / fleet_accel_ms2).sqrt();

    // Feasibility check: the fleet must carry enough propellant for the maneuver.
    // Chemical/NTR ships have high acceleration but minuscule ΔV capacity — their
    // required brachistochrone ΔV would be orders of magnitude above available fuel.
    if dv_brach > fleet_max_dv_ms { return None; }

    // Energy multiplier relative to the Hohmann baseline
    let (dv1_h, dv2_h, _, sma_h, ecc_h) = hohmann_transfer(r1_au, r2_au, gm);
    let hohmann_dv = dv1_h + dv2_h;
    let energy_multiplier = if hohmann_dv > 0.0 { dv_brach / hohmann_dv } else { 1.0 };

    Some(TransferOption {
        label: "Flip & Burn",
        total_delta_v_ms: dv_brach,
        delta_v1_ms: dv_brach * 0.5, // acceleration half-burn
        delta_v2_ms: dv_brach * 0.5, // deceleration half-burn
        transfer_time_s: t_brach,
        sma_au: sma_h, // placeholder SMA for arc visualization
        eccentricity: ecc_h,
        energy_multiplier,
        // The entire trip is a powered burn; transfer_time_s IS the burn time
        // (constant-acceleration kinematic model: thrust throughout).
        burn_time_s: t_brach,
    })
}

/// Compute the propellant mass fraction consumed to perform a given Δv.
///
/// Tsiolkovsky rocket equation:
/// Δv = Isp × g₀ × ln(m₀ / m_f)  →  fuel_fraction = 1 − e^(−Δv / (Isp × g₀))
///
/// Returns a fraction in \[0, 1\].
pub fn rocket_equation_fuel_fraction(delta_v_ms: f64, isp_s: f32) -> f64 {
    if isp_s <= 0.0 || delta_v_ms <= 0.0 {
        return 0.0;
    }
    let exhaust_velocity = isp_s as f64 * G0;
    1.0 - (-delta_v_ms / exhaust_velocity).exp()
}

/// Estimate the propellant mass in tonnes needed for a fleet to perform `delta_v_ms`.
///
/// - `wet_mass_t`: current total mass including propellant (tonnes)
/// - `isp_s`: fleet effective specific impulse (seconds)
/// - `delta_v_ms`: required Δv (m/s)
pub fn estimate_fuel_cost_tonnes(wet_mass_t: f32, isp_s: f32, delta_v_ms: f64) -> f32 {
    let frac = rocket_equation_fuel_fraction(delta_v_ms, isp_s) as f32;
    wet_mass_t * frac
}

/// Format a Δv value for human display.
pub fn format_delta_v(dv_ms: f64) -> String {
    if dv_ms >= 1_000.0 {
        format!("{:.2} km/s", dv_ms / 1_000.0)
    } else {
        format!("{:.0} m/s", dv_ms)
    }
}

/// Format a duration in seconds as a human-readable string.
pub fn format_duration(seconds: f64) -> String {
    let days = seconds / 86_400.0;
    if days < 1.0 {
        format!("{:.1} h", seconds / 3_600.0)
    } else if days < 30.0 {
        format!("{:.1} d", days)
    } else if days < 365.25 {
        format!("{:.1} mo", days / 30.44)
    } else {
        format!("{:.2} yr", days / 365.25)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that Earth→Mars Hohmann matches published values.
    /// Earth SMA ≈ 1.000 AU, Mars SMA ≈ 1.524 AU.
    /// Expected Δv ≈ 5.6 km/s total, transfer ≈ 259 days.
    #[test]
    fn test_earth_to_mars_hohmann() {
        let (dv1, dv2, time_s, sma_au, ecc) = hohmann_transfer(1.000, 1.524, GM_SUN);

        let total_dv = dv1 + dv2;
        let days = time_s / 86_400.0;

        // Total Δv should be ≈ 5.6 km/s (allow ±5%)
        assert!(
            (total_dv - 5596.0).abs() < 300.0,
            "Earth→Mars Δv: expected ≈5.6 km/s, got {:.0} m/s",
            total_dv
        );

        // Transfer time should be ≈ 259 days (allow ±5 days)
        assert!(
            (days - 258.9).abs() < 10.0,
            "Earth→Mars transfer time: expected ≈259 days, got {:.1} days",
            days
        );

        // SMA should be ≈ 1.262 AU
        assert!(
            (sma_au - 1.262).abs() < 0.01,
            "Transfer SMA: expected ≈1.262 AU, got {:.3} AU",
            sma_au
        );

        // Eccentricity should be ≈ 0.208
        assert!(
            (ecc - 0.208).abs() < 0.01,
            "Transfer eccentricity: expected ≈0.208, got {:.3}",
            ecc
        );
    }

    /// Inward transfer (Mars → Earth) should produce the same Δv *totals* as outward.
    /// The individual burns swap: dv1(outward) == dv2(inward) and vice-versa.
    #[test]
    fn test_mars_to_earth_hohmann() {
        let (dv1_out, dv2_out, t_out, _, _) = hohmann_transfer(1.000, 1.524, GM_SUN);
        let (dv1_in, dv2_in, t_in, _, _) = hohmann_transfer(1.524, 1.000, GM_SUN);

        // Total Δv is the same both ways (reversed burn magnitudes)
        let total_out = dv1_out + dv2_out;
        let total_in = dv1_in + dv2_in;
        assert!(
            (total_out - total_in).abs() < 1.0,
            "Outward/inward Δv totals should match: {:.0} vs {:.0} m/s",
            total_out,
            total_in
        );

        // Individual burns swap: departure burn outward ≈ arrival burn inward
        assert!(
            (dv1_out - dv2_in).abs() < 5.0,
            "Outward dv1 should ≈ inward dv2: {:.0} vs {:.0} m/s",
            dv1_out,
            dv2_in
        );
        assert!(
            (dv2_out - dv1_in).abs() < 5.0,
            "Outward dv2 should ≈ inward dv1: {:.0} vs {:.0} m/s",
            dv2_out,
            dv1_in
        );

        // Transfer times are the same
        assert!((t_out - t_in).abs() < 1.0, "time symmetry");
    }

    #[test]
    fn test_calculate_transfer_options_returns_three() {
        let options = calculate_transfer_options(1.0, 1.524, GM_SUN);
        assert_eq!(options.len(), 3, "should produce 3 options");
        assert_eq!(options[0].label, "Efficient");
        assert_eq!(options[1].label, "Moderate");
        assert_eq!(options[2].label, "Fast");

        // Δv increases from efficient to fast
        assert!(options[1].total_delta_v_ms > options[0].total_delta_v_ms);
        assert!(options[2].total_delta_v_ms > options[1].total_delta_v_ms);
        // Transfer time decreases from efficient to fast
        assert!(options[1].transfer_time_s < options[0].transfer_time_s);
        assert!(options[2].transfer_time_s < options[1].transfer_time_s);
    }

    #[test]
    fn test_same_orbit_returns_zero_delta_v() {
        let options = calculate_transfer_options(1.0, 1.0, GM_SUN);
        assert_eq!(options.len(), 1);
        assert_eq!(options[0].total_delta_v_ms, 0.0);
    }

    #[test]
    fn test_rocket_equation_zero_delta_v() {
        let frac = rocket_equation_fuel_fraction(0.0, 450.0);
        assert_eq!(frac, 0.0);
    }

    #[test]
    fn test_rocket_equation_earth_mars_chemical() {
        // ~5.6 km/s with chemical engine (Isp=450 s)
        // expected fuel fraction ≈ 1 - e^(-5600 / (450*9.81)) ≈ 1 - e^(-1.268) ≈ 0.719
        let frac = rocket_equation_fuel_fraction(5600.0, 450.0);
        assert!(
            (frac - 0.719).abs() < 0.01,
            "Chemical fuel fraction: expected ≈0.72, got {:.3}",
            frac
        );
    }

    #[test]
    fn test_rocket_equation_nuclear_thermal() {
        // Same Δv with nuclear thermal (Isp=900 s): fuel fraction ≈ 0.467
        let frac = rocket_equation_fuel_fraction(5600.0, 900.0);
        assert!(
            (frac - 0.467).abs() < 0.01,
            "NTR fuel fraction: expected ≈0.47, got {:.3}",
            frac
        );
    }

    #[test]
    fn test_format_delta_v() {
        assert_eq!(format_delta_v(500.0), "500 m/s");
        assert_eq!(format_delta_v(3_560.0), "3.56 km/s");
    }

    #[test]
    fn test_format_duration() {
        assert!(format_duration(3600.0).contains("h"));
        assert!(format_duration(86_400.0 * 15.0).contains("d"));
        assert!(format_duration(86_400.0 * 50.0).contains("mo"));
        assert!(format_duration(86_400.0 * 400.0).contains("yr"));
    }

    // ── Transfer window tests ────────────────────────────────────────────────

    /// Earth–Venus synodic period should be ≈ 584 days.
    #[test]
    fn test_earth_venus_synodic_period() {
        let w = compute_transfer_window(1.000, 0.723, GM_SUN, 0.0, 0.0);
        let synodic_days = w.synodic_period_s / 86_400.0;
        assert!(
            (synodic_days - 583.9).abs() < 10.0,
            "Venus synodic period: expected ≈ 584 d, got {:.1} d",
            synodic_days
        );
    }

    /// Earth–Mars synodic period should be ≈ 780 days.
    #[test]
    fn test_earth_mars_synodic_period() {
        let w = compute_transfer_window(1.000, 1.524, GM_SUN, 0.0, 0.0);
        let synodic_days = w.synodic_period_s / 86_400.0;
        assert!(
            (synodic_days - 779.9).abs() < 15.0,
            "Mars synodic period: expected ≈ 780 d, got {:.1} d",
            synodic_days
        );
    }

    /// When positioned at the exact Hohmann phase angle, time_to_window ≈ 0
    /// and phase_error ≈ 0.
    #[test]
    fn test_window_at_optimal_phase() {
        let r1 = 1.000_f64;
        let r2 = 1.524_f64;

        // Compute the required phase angle
        let r1_m = r1 * AU_IN_METERS;
        let r2_m = r2 * AU_IN_METERS;
        let n2 = (GM_SUN / r2_m.powi(3)).sqrt();
        let a = (r1_m + r2_m) / 2.0;
        let t_h = std::f64::consts::PI * (a.powi(3) / GM_SUN).sqrt();
        let phi_req = std::f64::consts::PI - n2 * t_h; // ≈ 0.773 rad

        let w = compute_transfer_window(r1, r2, GM_SUN, 0.0, phi_req);

        assert!(
            w.phase_error_now_rad.abs() < 1e-6,
            "Phase error at optimal window should be ~0, got {:.2e}",
            w.phase_error_now_rad
        );
        assert!(
            w.time_to_window_s < 1.0,
            "Time to window at optimal window should be ~0, got {:.1} s",
            w.time_to_window_s
        );
    }

    /// time_to_window is always in [0, synodic_period).
    #[test]
    fn test_window_time_in_range() {
        let w = compute_transfer_window(1.0, 1.524, GM_SUN, 1.23, 2.89);
        assert!(w.time_to_window_s >= 0.0);
        assert!(w.time_to_window_s < w.synodic_period_s);
    }

    /// phase_dv_factor is 1.0 at 0 and 2π, and monotonically increasing in [0, π].
    #[test]
    fn test_phase_dv_factor_shape() {
        let f0 = phase_dv_factor(0.0);
        let f_half = phase_dv_factor(std::f64::consts::FRAC_PI_2);
        let f_pi = phase_dv_factor(std::f64::consts::PI);
        let f_tau = phase_dv_factor(std::f64::consts::TAU);

        assert!((f0 - 1.0).abs() < 1e-10, "factor at 0 should be 1.0, got {}", f0);
        assert!(f_half > f0, "factor increases from 0 to π/2");
        assert!(f_pi > f_half, "factor increases from π/2 to π");
        assert!((f_tau - 1.0).abs() < 1e-6, "factor at 2π should be ~1.0, got {}", f_tau);
    }

    /// At optimal window (departure at time_to_window), phased Efficient option
    /// should match the base Hohmann ΔV within 0.1 m/s.
    #[test]
    fn test_phased_options_at_optimal_window_match_hohmann() {
        let r1 = 1.000_f64;
        let r2 = 1.524_f64;

        // Place destination at the exact required phase angle
        let r1_m = r1 * AU_IN_METERS;
        let r2_m = r2 * AU_IN_METERS;
        let n2 = (GM_SUN / r2_m.powi(3)).sqrt();
        let a = (r1_m + r2_m) / 2.0;
        let t_h = std::f64::consts::PI * (a.powi(3) / GM_SUN).sqrt();
        let phi_req = std::f64::consts::PI - n2 * t_h;

        let window = compute_transfer_window(r1, r2, GM_SUN, 0.0, phi_req);
        // Depart at time_to_window (should be ≈ 0 since we set the exact angle)
        let phased = calculate_transfer_options_phased(r1, r2, GM_SUN, window.time_to_window_s, &window);
        let base = calculate_transfer_options(r1, r2, GM_SUN);

        assert!(
            (phased[0].total_delta_v_ms - base[0].total_delta_v_ms).abs() < 0.5,
            "At optimal window, phased Efficient ΔV ({:.1}) should ≈ Hohmann ({:.1})",
            phased[0].total_delta_v_ms,
            base[0].total_delta_v_ms
        );
    }

    /// Off-window phased ΔV must be ≥ Hohmann for Efficient and ≥ base for others.
    #[test]
    fn test_phased_options_off_window_cost_more() {
        let r1 = 1.000_f64;
        let r2 = 1.524_f64;
        // Use a deliberately bad phase angle (π rad off from optimal)
        let window = compute_transfer_window(r1, r2, GM_SUN, 0.0, 0.0); // arbitrary non-optimal
        let base = calculate_transfer_options(r1, r2, GM_SUN);
        let phased = calculate_transfer_options_phased(r1, r2, GM_SUN, 0.0, &window);

        // All options should cost at least as much as base Hohmann
        for (p, b) in phased.iter().zip(base.iter()) {
            assert!(
                p.total_delta_v_ms >= b.total_delta_v_ms - 1.0,
                "Phased {} should cost ≥ base ({:.0} vs {:.0})",
                p.label, p.total_delta_v_ms, b.total_delta_v_ms
            );
        }
    }

    // ── Gravity assist tests ─────────────────────────────────────────────────

    /// Earth → Saturn with a Jupiter flyby should save significant ΔV.
    #[test]
    fn test_earth_saturn_via_jupiter_saves_dv() {
        let r_earth   = 1.000_f64;
        let r_saturn  = 9.537_f64;
        let r_jupiter = 5.204_f64;
        let gm_jup = 1.267e17_f64;
        // Minimum flyby periapsis ≈ 3 × Jupiter radius (71,492 km)
        let r_peri = 3.0 * 71_492.0e3 / AU_IN_METERS;

        let opt = compute_gravity_assist(
            r_earth, r_saturn, r_jupiter, GM_SUN, gm_jup,
            "Jupiter".to_string(), r_peri,
        );

        // Jupiter slingshot should save at least 1 km/s vs direct Hohmann
        assert!(
            opt.dv_savings_ms > 1_000.0,
            "Jupiter flyby for Earth→Saturn should save >1 km/s, saved {:.0} m/s",
            opt.dv_savings_ms
        );
        // v_inf at Jupiter should be significant
        assert!(
            opt.v_inf_ms > 1_000.0,
            "v_inf at Jupiter should be >1 km/s, got {:.0} m/s",
            opt.v_inf_ms
        );
        // leg1_time + leg2_time should equal total_time
        assert!(
            (opt.leg1_time_s + opt.leg2_time_s - opt.total_time_s).abs() < 1.0,
            "leg1 + leg2 should equal total time: {:.0} + {:.0} ≠ {:.0}",
            opt.leg1_time_s, opt.leg2_time_s, opt.total_time_s
        );
    }

    /// find_gravity_assist_options returns Jupiter and Mars for Earth→Saturn,
    /// but not Venus or Earth (outside the range).
    #[test]
    fn test_find_gravity_assist_earth_to_saturn() {
        let r_peri_jup = 3.0 * 71_492.0e3 / AU_IN_METERS;
        let bodies = vec![
            ("Venus".to_string(),   0.723, 3.248e14, 3.0 * 6_051.0e3 / AU_IN_METERS),
            ("Earth".to_string(),   1.000, 3.986e14, 3.0 * 6_371.0e3 / AU_IN_METERS),
            ("Mars".to_string(),    1.524, 4.282e13, 3.0 * 3_390.0e3 / AU_IN_METERS),
            ("Jupiter".to_string(), 5.204, 1.267e17, r_peri_jup),
        ];

        let opts = find_gravity_assist_options(1.0, 9.537, GM_SUN, &bodies);

        assert!(opts.iter().any(|o| o.body_name == "Jupiter"),
            "Jupiter should be a candidate for Earth→Saturn");
        assert!(opts.iter().any(|o| o.body_name == "Mars"),
            "Mars should be a candidate for Earth→Saturn");
        assert!(!opts.iter().any(|o| o.body_name == "Venus"),
            "Venus should NOT be a candidate (outside range 1.0–9.537 AU)");
        assert!(!opts.iter().any(|o| o.body_name == "Earth"),
            "Earth should NOT be a candidate (origin body)");
    }

    /// Earth → Mars: no gravity-assist candidates (no planets between 1 and 1.524 AU).
    #[test]
    fn test_no_gravity_assist_earth_to_mars() {
        let bodies = vec![
            ("Venus".to_string(),   0.723, 3.248e14, 3.0 * 6_051.0e3 / AU_IN_METERS),
            ("Earth".to_string(),   1.000, 3.986e14, 3.0 * 6_371.0e3 / AU_IN_METERS),
            ("Jupiter".to_string(), 5.204, 1.267e17, 3.0 * 71_492.0e3 / AU_IN_METERS),
        ];

        let opts = find_gravity_assist_options(1.0, 1.524, GM_SUN, &bodies);
        assert!(
            opts.is_empty(),
            "No candidates between Earth and Mars, but got: {:?}",
            opts.iter().map(|o| o.body_name.as_str()).collect::<Vec<_>>()
        );
    }

    // ── Thrust and burn-time tests ───────────────────────────────────────────

    /// Chemical engine burn is short (impulsive).
    /// Frigate: dry 2000 t, fuel fraction 0.45 → wet ≈ 3636 t.
    ///   thrust_kn  = TWR(10) × dry_mass(2000 t) × g0(9.81) = 196 200 kN
    ///   accel      = thrust_kn / wet_mass_t        ≈ 54 m/s²   (kN/t = m/s²)
    /// Earth-Mars Hohmann ΔV ≈ 5.6 km/s; expected burn < 5 minutes.
    #[test]
    fn test_burn_time_chemical_is_impulsive() {
        let isp = 450.0_f32;
        // Frigate Chemical: TWR=10 vs 2000 t dry, wet=2000/(1-0.45)≈3636 t
        let thrust_kn = 10.0_f64 * 2_000.0 * 9.81; // = 196 200 kN
        let wet_mass_t = 2_000.0_f64 / (1.0 - 0.45); // ≈ 3636 t
        let accel = thrust_kn / wet_mass_t; // kN/t = m/s²
        let t = compute_burn_time_s(5_600.0, accel, isp);
        assert!(
            t < 300.0,
            "Chemical burn should be impulsive (<5 min), got {:.0} s",
            t
        );
    }

    /// Ion-drive burn is extended (many hours).
    /// Frigate: dry 2000 t, fuel fraction 0.45 → wet ≈ 3636 t.
    ///   thrust_kn = TWR(0.001) × dry_mass(2000 t) × g0(9.81) ≈ 19.62 kN
    ///   accel     = 19.62 / 3636 ≈ 0.0054 m/s²
    /// Same 5.6 km/s ΔV should take multiple hours.
    #[test]
    fn test_burn_time_ion_drive_is_extended() {
        let isp = 5_000.0_f32;
        // Frigate IonDrive: TWR=0.001 vs 2000 t dry, wet≈3636 t
        let thrust_kn = 0.001_f64 * 2_000.0 * 9.81; // ≈ 19.62 kN
        let wet_mass_t = 2_000.0_f64 / (1.0 - 0.45); // ≈ 3636 t
        let accel = thrust_kn / wet_mass_t; // kN/t = m/s²
        let t = compute_burn_time_s(5_600.0, accel, isp);
        let hours = t / 3_600.0;
        assert!(
            hours > 1.0,
            "Ion drive burn should take >1 hour, got {:.2} h",
            hours
        );
    }

    /// compute_burn_time_s returns 0 for non-positive inputs.
    #[test]
    fn test_burn_time_zero_inputs() {
        assert_eq!(compute_burn_time_s(0.0, 10.0, 450.0), 0.0);
        assert_eq!(compute_burn_time_s(1000.0, 0.0, 450.0), 0.0);
        assert_eq!(compute_burn_time_s(1000.0, 10.0, 0.0), 0.0);
    }

    /// brachistochrone_option returns None for sub-threshold acceleration.
    #[test]
    fn test_brachistochrone_below_threshold() {
        // Ion drive accel ≈ 0.0054 m/s² — below the 0.05 m/s² threshold.
        // Provide a large max_dv so only the accel check fires.
        let result = brachistochrone_option(1.0, 1.524, GM_SUN, 0.0054, 5_000.0, 1_000_000_000.0);
        assert!(result.is_none(),
            "Ion drive should not produce a Flip & Burn option");
    }

    /// brachistochrone_option produces a valid option for high-thrust, high-Isp ships.
    /// Earth-Mars with accel = 10 m/s²: t < Hohmann, ΔV >> Hohmann.
    #[test]
    fn test_brachistochrone_high_thrust() {
        let accel = 10.0_f64; // m/s²
        // Antimatter Frigate: Isp=1 000 000 s, fuel_frac=0.45, wet=3636t
        // ΔV_max = 1_000_000 × 9.81 × ln(1.818) ≈ 5 866 km/s  (plenty)
        let max_dv = 1_000_000.0_f64 * 9.806_65 * (3636.0_f64 / 2000.0_f64).ln();
        let opt = brachistochrone_option(1.0, 1.524, GM_SUN, accel, 50_000.0, max_dv)
            .expect("High-thrust, high-Isp fleet should get Flip & Burn option");
        assert_eq!(opt.label, "Flip & Burn");
        // ΔV should be far above Hohmann
        let (dv1, dv2, t_h, _, _) = hohmann_transfer(1.0, 1.524, GM_SUN);
        let hohmann_dv = dv1 + dv2;
        assert!(
            opt.total_delta_v_ms > hohmann_dv * 5.0,
            "Flip & Burn ΔV ({:.0} m/s) should >> Hohmann ({:.0} m/s)",
            opt.total_delta_v_ms, hohmann_dv
        );
        // Transfer time should be less than Hohmann
        assert!(
            opt.transfer_time_s < t_h,
            "Flip & Burn time ({:.1} d) should be < Hohmann ({:.1} d)",
            opt.transfer_time_s / 86_400.0, t_h / 86_400.0
        );
        // burn_time_s should equal transfer_time_s (always thrusting)
        assert!(
            (opt.burn_time_s - opt.transfer_time_s).abs() < 1.0,
            "Flip & Burn: burn_time should ≈ transfer_time"
        );
    }

    /// Chemical ships have high acceleration but tiny ΔV capacity.
    /// brachistochrone_option must return None even though accel > threshold.
    #[test]
    fn test_brachistochrone_insufficient_dv() {
        // Chemical Frigate: dry=2000t, fuel_frac=0.45, Isp=450s
        //   ΔV_max = 450 × 9.81 × ln(1.818) ≈ 2 640 m/s (only ~2.6 km/s)
        //   accel   = 10 × 2000 × 9.81 / 3636 ≈ 54 m/s² (high, above threshold)
        let isp = 450.0_f32;
        let dry = 2_000.0_f64;
        let fuel_frac = 0.45_f64;
        let wet = dry / (1.0 - fuel_frac);
        let max_dv = isp as f64 * G0 * (wet / dry).ln(); // ≈ 2 640 m/s
        let accel = 10.0 * dry * 9.81 / wet; // ≈ 54 m/s²

        let result = brachistochrone_option(1.0, 1.524, GM_SUN, accel, isp, max_dv);
        assert!(
            result.is_none(),
            "Chemical ship with only {:.0} m/s ΔV should not get Flip & Burn (needs >> that)",
            max_dv
        );
    }

    /// At 0.01 g (0.098 m/s²), Mars minimum approach (~0.38 AU) brachistochrone
    /// should take ≈ 17.6 days — matching the user-provided reference table.
    #[test]
    fn test_brachistochrone_fusion_mars_17days() {
        let accel = 0.098_1_f64; // 0.01 g
        // Fusion torch Frigate: Isp=50 000 s, fuel_frac=0.45
        //   ΔV_max ≈ 50 000 × 9.81 × 0.598 ≈ 293 km/s
        let dry = 2_000.0_f64;
        let wet = dry / (1.0 - 0.45_f64);
        let max_dv = 50_000.0_f64 * G0 * (wet / dry).ln(); // ≈ 293 km/s

        // Use r2 = 1.38 AU so |r2-r1| × AU ≈ 0.38 AU = minimum Earth–Mars distance
        let opt = brachistochrone_option(1.0, 1.38, GM_SUN, accel, 50_000.0, max_dv)
            .expect("0.01 g fusion should produce a Flip & Burn option for Mars min approach");
        let days = opt.transfer_time_s / 86_400.0;
        assert!(
            (days - 17.6).abs() < 1.5,
            "0.01 g brachistochrone to Mars min (0.38 AU): expected ≈17.6 days, got {:.1} days",
            days
        );
        // ΔV should be feasible (< fleet max ΔV)
        assert!(
            opt.total_delta_v_ms <= max_dv,
            "Required ΔV ({:.0} m/s) must not exceed fleet capacity ({:.0} m/s)",
            opt.total_delta_v_ms, max_dv
        );
    }

    /// At ~1 g (9.81 m/s²), Mars minimum approach brachistochrone should take
    /// ≈ 1.7 days — matching the antimatter reference values.
    #[test]
    fn test_brachistochrone_antimatter_mars_17days() {
        let accel = 9.81_f64; // 1 g
        // Antimatter Frigate: Isp=1 000 000 s, fuel_frac=0.45
        //   ΔV_max ≈ 1 000 000 × 9.81 × 0.598 ≈ 5 866 km/s
        let dry = 2_000.0_f64;
        let wet = dry / (1.0 - 0.45_f64);
        let max_dv = 1_000_000.0_f64 * G0 * (wet / dry).ln(); // ≈ 5 866 km/s

        let opt = brachistochrone_option(1.0, 1.38, GM_SUN, accel, 1_000_000.0, max_dv)
            .expect("1 g antimatter should produce a Flip & Burn option for Mars min approach");
        let days = opt.transfer_time_s / 86_400.0;
        assert!(
            (days - 1.76).abs() < 0.2,
            "1 g brachistochrone to Mars min: expected ≈1.76 days, got {:.2} days",
            days
        );
    }
}
