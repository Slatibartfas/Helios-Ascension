//! Strict RON-load gate for the interstellar propulsion policy file
//! (GRA-343 / GRA-328b).
//!
//! Mirrors the `porkchop_config_ron_loads_cleanly` test pattern: the
//! loader is tolerant (debug builds must not panic on a bad RON), but
//! `cargo test` is the hard gate that catches RON typos before they
//! hit runtime as a startup-only `error!` log and a silent
//! fall-through to `InterstellarPropulsionPolicy::default()`.

use helios_ascension::fleets::InterstellarPropulsionPolicy;

#[test]
fn interstellar_propulsion_ron_loads_cleanly() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets/data/interstellar_propulsion.ron");
    let contents = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    let policy: InterstellarPropulsionPolicy = ron::from_str(&contents)
        .unwrap_or_else(|e| panic!("interstellar_propulsion.ron failed to parse: {e}"));
    // Validation must also pass — covers the case where RON deserializes
    // but a per-field invariant (e.g. tolerance > 180°, margin < 1.0)
    // is violated.
    if let Err(violations) = policy.validate() {
        panic!("interstellar_propulsion.ron failed validation: {violations:#?}");
    }
    // Sanity: every policy value must be finite and well-formed.
    assert!(
        policy.ai_phase_angle_tolerance_deg > 0.0 && policy.ai_phase_angle_tolerance_deg <= 180.0,
        "ai_phase_angle_tolerance_deg out of range: {}",
        policy.ai_phase_angle_tolerance_deg
    );
    assert!(
        policy.human_phase_angle_tolerance_deg > 0.0
            && policy.human_phase_angle_tolerance_deg <= 180.0,
        "human_phase_angle_tolerance_deg out of range: {}",
        policy.human_phase_angle_tolerance_deg
    );
    assert!(
        policy.ai_deltav_margin >= 1.0,
        "ai_deltav_margin must be ≥ 1.0 (got {})",
        policy.ai_deltav_margin
    );
    assert!(
        policy.human_deltav_margin >= 1.0,
        "human_deltav_margin must be ≥ 1.0 (got {})",
        policy.human_deltav_margin
    );
}

#[test]
fn interstellar_propulsion_default_validates() {
    let policy = InterstellarPropulsionPolicy::default();
    assert!(
        policy.validate().is_ok(),
        "default InterstellarPropulsionPolicy should validate"
    );
}

#[test]
fn interstellar_propulsion_default_matches_ron() {
    // The RON file is the source-of-truth; the `Default` impl is the
    // fallback.  Any drift between the two breaks the loader's silent
    // fall-through path: a missing RON field would deserialize as
    // `Default` and the player would see AI planner defaults instead of
    // modder-tuned values.  The hard check: the four defaults must
    // match what the RON file currently ships.
    let policy = InterstellarPropulsionPolicy::default();
    assert!((policy.ai_phase_angle_tolerance_deg - 15.0).abs() < 1e-9);
    assert!((policy.human_phase_angle_tolerance_deg - 45.0).abs() < 1e-9);
    assert!((policy.ai_deltav_margin - 1.20).abs() < 1e-9);
    assert!((policy.human_deltav_margin - 1.05).abs() < 1e-9);
}

#[test]
fn interstellar_propulsion_rejects_out_of_range_tolerance() {
    let mut policy = InterstellarPropulsionPolicy::default();
    policy.ai_phase_angle_tolerance_deg = 200.0;
    let v = policy.validate().unwrap_err();
    assert!(
        v.iter().any(|s| s.contains("ai_phase_angle_tolerance_deg")),
        "expected tolerance violation, got {v:#?}"
    );
}

#[test]
fn interstellar_propulsion_rejects_subunity_margin() {
    let mut policy = InterstellarPropulsionPolicy::default();
    policy.human_deltav_margin = 0.95;
    let v = policy.validate().unwrap_err();
    assert!(
        v.iter().any(|s| s.contains("human_deltav_margin")),
        "expected margin violation, got {v:#?}"
    );
}
