//! Tests for the GRA-343 / GRA-328b margin + phase-tolerance helpers in
//! `src/fleets/orbital_mechanics.rs`.
//!
//! These four helpers are the gating predicates the planner UI uses to
//! enable / disable the Execute button on a cross-system Hohmann:
//!   * `meets_ai_margin` — ΔV ≥ dv_required × ai_deltav_margin
//!   * `meets_human_margin` — ΔV ≥ dv_required × human_deltav_margin
//!   * `within_ai_phase_tolerance` — |actual - ideal| ≤ ai_tolerance_deg
//!   * `within_human_phase_tolerance` — same, human
//!
//! The tests construct a small `Fleet` with a known `max_delta_v_ms`
//! (set via `ShipInfo::delta_v_ms`) and a `InterstellarPropulsionPolicy`
//! matching the GRA-331 LGD contract defaults.

use helios_ascension::fleets::orbital_mechanics::{
    meets_ai_margin, meets_human_margin, within_ai_phase_tolerance, within_human_phase_tolerance,
};
use helios_ascension::fleets::{Fleet, InterstellarPropulsionPolicy, ShipInfo};

fn policy() -> InterstellarPropulsionPolicy {
    InterstellarPropulsionPolicy::default()
}

/// Construct a single-ship fleet whose `max_delta_v_ms()` (the
/// fleet-min of every ship's ΔV) is exactly `target_dv_ms`.
///
/// Setting `wet_mass_t` and `dry_mass_t` directly is the easiest way
/// to land on a precise `delta_v_ms()` value: with `isp_s = 450`,
/// `g0 = 9.80665`, the rocket equation yields
/// `delta_v = 450 × 9.80665 × ln(wet/dry)`.
fn fleet_with_max_dv_ms(target_dv_ms: f64) -> Fleet {
    use helios_ascension::fleets::{PropulsionType, ShipClass};
    let isp_s = 450.0_f32;
    // Solve: dv = isp * g0 * ln(wet/dry)  →  wet = dry * exp(dv / (isp*g0))
    let dry = 1_000.0_f64;
    let wet = dry * (target_dv_ms / (isp_s as f64 * 9.806_65)).exp();
    let info = ShipInfo::new(
        "test_ship".to_string(),
        ShipClass::ResearchVessel,
        PropulsionType::Chemical,
    );
    // Overwrite the ShipInfo's mass + Isp to lock the ΔV budget.
    let info = ShipInfo {
        dry_mass_t: dry as f32,
        fuel_mass_t: (wet - dry) as f32,
        max_fuel_t: (wet - dry) as f32,
        isp_s,
        ..info
    };
    Fleet {
        name: "test_fleet".to_string(),
        role: Default::default(),
        ships: vec![info],
    }
}

// ── meets_ai_margin / meets_human_margin ───────────────────────────────────

#[test]
fn ai_margin_pass_at_exact_dv_required() {
    // Policy default: ai_deltav_margin = 1.20 (20% reserve).
    // Fleet with max_dv = 12.0 km/s.  Margin gate = 10.0 km/s × 1.20
    // = 12.0 km/s — fleet exactly meets it.
    let fleet = fleet_with_max_dv_ms(12_000.0);
    assert!(meets_ai_margin(&fleet, 10_000.0, &policy()));
}

#[test]
fn ai_margin_fail_below_reserve() {
    // Same fleet, ΔV required = 10.001 km/s.  Margin gate = 12.0012 km/s
    // — fleet does NOT meet it (12.0 < 12.0012).
    let fleet = fleet_with_max_dv_ms(12_000.0);
    assert!(!meets_ai_margin(&fleet, 10_001.0, &policy()));
}

#[test]
fn human_margin_tighter_than_ai() {
    // Same fleet.  ΔV required = 11.5 km/s.
    //   AI gate    = 11.5 × 1.20 = 13.8 km/s — fleet FAILS (12.0 < 13.8).
    //   Human gate = 11.5 × 1.05 = 12.075 km/s — fleet FAILS (12.0 < 12.075).
    let fleet = fleet_with_max_dv_ms(12_000.0);
    assert!(!meets_ai_margin(&fleet, 11_500.0, &policy()));
    assert!(!meets_human_margin(&fleet, 11_500.0, &policy()));
}

#[test]
fn human_margin_passes_at_smaller_reserve() {
    // ΔV required = 11.0 km/s.
    //   AI gate    = 11.0 × 1.20 = 13.2 km/s — fleet FAILS.
    //   Human gate = 11.0 × 1.05 = 11.55 km/s — fleet PASSES (12.0 ≥ 11.55).
    let fleet = fleet_with_max_dv_ms(12_000.0);
    assert!(!meets_ai_margin(&fleet, 11_000.0, &policy()));
    assert!(meets_human_margin(&fleet, 11_000.0, &policy()));
}

#[test]
fn margin_helpers_reject_zero_or_negative_dv() {
    let fleet = fleet_with_max_dv_ms(12_000.0);
    assert!(!meets_ai_margin(&fleet, 0.0, &policy()));
    assert!(!meets_human_margin(&fleet, -100.0, &policy()));
}

#[test]
fn margin_helpers_reject_subunity_margin() {
    let fleet = fleet_with_max_dv_ms(12_000.0);
    let mut bad_policy = policy();
    bad_policy.ai_deltav_margin = 0.5;
    assert!(!meets_ai_margin(&fleet, 1_000.0, &bad_policy));
    bad_policy.ai_deltav_margin = 1.20;
    bad_policy.human_deltav_margin = 0.5;
    assert!(!meets_human_margin(&fleet, 1_000.0, &bad_policy));
}

#[test]
fn empty_fleet_never_meets_margin() {
    let empty = Fleet::new("empty".to_string());
    assert!(!meets_ai_margin(&empty, 1.0, &policy()));
    assert!(!meets_human_margin(&empty, 1.0, &policy()));
}

// ── within_ai_phase_tolerance / within_human_phase_tolerance ───────────────

#[test]
fn phase_tolerance_within_ai_cone() {
    // Policy default: ai_phase_angle_tolerance_deg = 15.0.
    let p = policy();
    // Exact match
    assert!(within_ai_phase_tolerance(0.0, 0.0, &p));
    // Edge of cone
    assert!(within_ai_phase_tolerance(15.0, 0.0, &p));
    // Just outside cone
    assert!(!within_ai_phase_tolerance(15.001, 0.0, &p));
    // Negative direction is symmetric
    assert!(within_ai_phase_tolerance(-14.999, 0.0, &p));
    assert!(!within_ai_phase_tolerance(-15.001, 0.0, &p));
}

#[test]
fn phase_tolerance_human_wider_than_ai() {
    let p = policy();
    // 30° from ideal — fails AI (±15°), passes human (±45°).
    assert!(!within_ai_phase_tolerance(30.0, 0.0, &p));
    assert!(within_human_phase_tolerance(30.0, 0.0, &p));
    // 50° from ideal — fails both.
    assert!(!within_ai_phase_tolerance(50.0, 0.0, &p));
    assert!(!within_human_phase_tolerance(50.0, 0.0, &p));
}

#[test]
fn phase_tolerance_with_nonzero_ideal_phase() {
    // actual = 30°, ideal = 25°  →  diff = 5°  →  within AI (±15°).
    let p = policy();
    assert!(within_ai_phase_tolerance(30.0, 25.0, &p));
    assert!(within_human_phase_tolerance(30.0, 25.0, &p));
    // actual = 100°, ideal = 25°  →  diff = 75°  →  fails both.
    assert!(!within_ai_phase_tolerance(100.0, 25.0, &p));
    assert!(!within_human_phase_tolerance(100.0, 25.0, &p));
}
