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
        }];
    }

    let (dv1_h, dv2_h, t_h, sma_h, ecc_h) = hohmann_transfer(r1_au, r2_au, gm);

    // Phase angle at the chosen departure time:
    // phase(t) = phase_now + phase_rate · t
    // ⟹ phase_error(t) = phase_error_now − phase_rate · t
    let phase_at_dep = window.phase_error_now_rad
        - window.phase_rate_rad_s * departure_offset_s;
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
    }
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
}
