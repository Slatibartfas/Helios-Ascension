//! Tests for Kepler orbital mechanics — P0 system: Orbital Mechanics
//!
//! Verifies:
//! - Keplerian propagation accuracy (solving Kepler's equation)
//! - Transfer calculations (Hohmann, transfer windows)
//! - Edge cases: circular orbits, zero inclination, eccentric orbits

use helios_ascension::astronomy::components::KeplerOrbit;
use helios_ascension::fleets::orbital_mechanics::{
    compute_transfer_window, hohmann_transfer, phase_dv_factor, AU_IN_METERS, GM_SUN,
};

/// Kepler's equation: M = E - e·sin(E)
/// For a circular orbit (e=0), E should equal M exactly.
#[test]
fn solve_kepler_circular_orbit() {
    use helios_ascension::astronomy::systems::solve_kepler;

    // e = 0 → E should equal M for any mean anomaly
    for mean_anomaly in [
        0.0,
        std::f64::consts::FRAC_PI_4,
        std::f64::consts::FRAC_PI_2,
        std::f64::consts::PI,
        2.0,
    ] {
        let E = solve_kepler(mean_anomaly, 0.0);
        assert!(
            (E - mean_anomaly).abs() < 1e-10,
            "Circular orbit: E={} should equal M={}, diff={}",
            E,
            mean_anomaly,
            (E - mean_anomaly).abs()
        );
    }
}

/// For a low-eccentricity orbit the Newton-Raphson solver should converge quickly.
#[test]
fn solve_kepler_low_eccentricity_convergence() {
    use helios_ascension::astronomy::systems::solve_kepler;

    let E = solve_kepler(1.0, 0.1);
    // M = E - e·sin(E) should hold
    let M_check = E - 0.1 * E.sin();
    assert!(
        (M_check - 1.0).abs() < 1e-10,
        "Kepler equation not satisfied for low eccentricity"
    );
}

/// High-eccentricity orbit (e=0.9) should still converge to acceptable precision.
#[test]
fn solve_kepler_high_eccentricity() {
    use helios_ascension::astronomy::systems::solve_kepler;

    let mean_anomaly = std::f64::consts::FRAC_PI_3;
    let E = solve_kepler(mean_anomaly, 0.9);
    let residual = (E - 0.9 * E.sin() - mean_anomaly).abs();
    assert!(
        residual < 1e-8,
        "High-e orbit residual {} exceeds tolerance",
        residual
    );
}

/// Edge case: mean anomaly of exactly zero should return zero eccentric anomaly.
#[test]
fn solve_kepler_zero_mean_anomaly() {
    use helios_ascension::astronomy::systems::solve_kepler;

    let E = solve_kepler(0.0, 0.5);
    assert!(E.abs() < 1e-10, "M=0 should give E≈0, got {}", E);
}

/// Circular orbit: KeplerOrbit::circular should produce e≈0 and nominal semi-major axis.
#[test]
fn kepler_orbit_circular_edge_case() {
    let orbit = KeplerOrbit::circular(2.0, 0.0);
    assert!(
        orbit.eccentricity < 1e-10,
        "circular() should give e≈0, got {}",
        orbit.eccentricity
    );
    assert!(
        (orbit.semi_major_axis - 2.0).abs() < 1e-10,
        "semi_major_axis mismatch"
    );
}

/// Verify orbital mechanics via orbit_position — periapsis and apoapsis radii.
#[test]
fn orbital_position_at_periapsis_and_apoapsis() {
    use helios_ascension::astronomy::systems::orbit_position_from_mean_anomaly;

    let orbit = KeplerOrbit {
        semi_major_axis: 2.0,
        eccentricity: 0.3,
        inclination: 0.0,
        longitude_ascending_node: 0.0,
        argument_of_periapsis: 0.0,
        ..Default::default()
    };

    // At periapsis (M=0, e=0.3): r = a(1-e) = 2×0.7 = 1.4
    let pos_periapsis = orbit_position_from_mean_anomaly(&orbit, 0.0);
    let r_periapsis = pos_periapsis.length();
    let expected_periapsis = 2.0 * (1.0 - 0.3); // a(1-e)
    assert!(
        (r_periapsis - expected_periapsis).abs() < 1e-6,
        "Periapsis radius should be a(1-e)={}, got {}",
        expected_periapsis,
        r_periapsis
    );

    // At apoapsis (M=π, e=0.3): r = a(1+e) = 2×1.3 = 2.6
    let pos_apoapsis = orbit_position_from_mean_anomaly(&orbit, std::f64::consts::PI);
    let r_apoapsis = pos_apoapsis.length();
    let expected_apoapsis = 2.0 * (1.0 + 0.3); // a(1+e)
    assert!(
        (r_apoapsis - expected_apoapsis).abs() < 1e-6,
        "Apoapsis radius should be a(1+e)={}, got {}",
        expected_apoapsis,
        r_apoapsis
    );
}

/// Hohmann transfer Δv values must be positive.
#[test]
fn hohmann_transfer_positive_dv() {
    let (dv1, dv2, _time, _, _) = hohmann_transfer(1.0, 2.0, GM_SUN);
    assert!(dv1 > 0.0, "Departure DV must be positive");
    assert!(dv2 > 0.0, "Arrival DV must be positive");
    // Inward transfer (r2 < r1) should give positive dv1
    let (dv1_in, _dv2_in, _, _, _) = hohmann_transfer(2.0, 1.0, GM_SUN);
    assert!(dv1_in > 0.0, "Inward departure DV must be positive");
}

/// Hohmann transfer time estimate should be reasonable for Earth→Mars.
#[test]
fn hohmann_transfer_earth_mars_time() {
    // 1 AU → 1.524 AU (Mars)
    let (_, _, time_s, _, _) = hohmann_transfer(1.0, 1.524, GM_SUN);
    let days = time_s / 86400.0;
    // Hohmann transfer to Mars is ~259 days (0.71 years)
    assert!(
        days > 200.0 && days < 350.0,
        "Earth→Mars Hohmann should be ~259 days, got {:.0}",
        days
    );
}

/// Transfer window: identical orbital radii should return zero wait time.
#[test]
fn compute_transfer_window_identical_radii() {
    let info = compute_transfer_window(1.0, 1.0, GM_SUN, 0.0, 0.0);
    assert!(
        info.time_to_window_s < 1e-6,
        "Identical radii should give near-zero time_to_window, got {}",
        info.time_to_window_s
    );
    assert!(
        info.synodic_period_s.is_infinite(),
        "Identical radii should give infinite synodic period"
    );
}

/// Phase angle error of 0 should give efficiency factor exactly 1.0.
#[test]
fn phase_dv_factor_optimal() {
    let factor = phase_dv_factor(0.0);
    assert!(
        (factor - 1.0).abs() < 1e-10,
        "Optimal phase angle should give factor=1.0"
    );
}

/// Phase angle error of ±π should give max DV penalty factor.
#[test]
fn phase_dv_factor_worst_case() {
    let factor = phase_dv_factor(std::f64::consts::PI);
    // factor = 1 + 1.4·sin²(π/2) = 1 + 1.4 = 2.4
    assert!(
        (factor - 2.4).abs() < 1e-10,
        "Worst phase should give factor=2.4, got {}",
        factor
    );
}

/// Zero inclination orbit should produce zero out-of-plane Z coordinate.
#[test]
fn zero_inclination_orbit() {
    use helios_ascension::astronomy::systems::orbit_position_from_mean_anomaly;

    let orbit = KeplerOrbit {
        semi_major_axis: 1.0,
        eccentricity: 0.0,
        inclination: 0.0,
        longitude_ascending_node: 0.0,
        argument_of_periapsis: 0.0,
        ..Default::default()
    };
    let pos = orbit_position_from_mean_anomaly(&orbit, 0.0);
    assert!(
        pos.z.abs() < 1e-10,
        "Zero inclination orbit should stay in XY plane, z={}",
        pos.z
    );
}

/// Non-zero argument of periapsis should rotate the orbit in the orbital plane.
#[test]
fn argument_of_periapsis_rotation() {
    use helios_ascension::astronomy::systems::orbit_position_from_mean_anomaly;

    // Periapsis at π/2 → at M=0 the body should be at +y in perifocal frame
    let orbit = KeplerOrbit {
        semi_major_axis: 1.0,
        eccentricity: 0.0,
        inclination: 0.0,
        longitude_ascending_node: 0.0,
        argument_of_periapsis: std::f64::consts::FRAC_PI_2,
        ..Default::default()
    };
    let pos = orbit_position_from_mean_anomaly(&orbit, 0.0);
    // r = a at periapsis; in perifocal frame periapsis is at ω=π/2 → +y
    assert!(
        pos.y > 0.9,
        "At ω=π/2, M=0 should place body near +y direction"
    );
}

/// AU constant should be approximately 1.496e11 meters.
#[test]
fn au_constant_accuracy() {
    assert!(
        (AU_IN_METERS - 1.495_978_707e11).abs() < 1.0,
        "AU_IN_METERS should be ~1.496e11, got {}",
        AU_IN_METERS
    );
}

/// GM_SUN should be approximately 1.327e20 m³/s².
#[test]
fn gm_sun_accuracy() {
    assert!(
        (GM_SUN - 1.327_124_4e20).abs() < 1e15,
        "GM_SUN should be ~1.327e20, got {}",
        GM_SUN
    );
}
