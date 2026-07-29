//! Multi-star ephemeris catalog tests (GRA-328c, GRA-332).
//!
//! These tests assert the structural and numeric invariants of
//! `assets/data/nearest_stars.ron` and the runtime contract in
//! `src/astronomy/star_epoch.rs`.  They run against the live RON file
//! loaded through `NearbyStarsPlugin`, so a typo in the catalog fails the
//! test rather than silently shipping a broken transfer-math source of
//! truth.

use bevy::prelude::*;

use helios_ascension::astronomy::{
    advance_position, hill_sphere_au, nearby_stars::NearbyStarsPlugin, StarSystemsEphemeris,
    EPOCH_BEACON_GAME_START_SIM_S,
};
use helios_ascension::fleets::GM_SUN;

fn load_catalog_for_test() -> StarSystemsEphemeris {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(NearbyStarsPlugin);
    app.update();
    app.world().resource::<StarSystemsEphemeris>().clone()
}

fn approx_eq(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() < tol
}

#[test]
fn nearest_stars_ron_loads_60_systems() {
    let catalog = load_catalog_for_test();
    assert_eq!(
        catalog.len(),
        60,
        "nearest_stars.ron must contain all 60 systems (GRA-332 §6 — \
         '60 systems, the full content of nearest_stars_raw.json')"
    );
}

#[test]
fn star_system_ephemeris_mu_derived_correctly() {
    let catalog = load_catalog_for_test();
    // Alpha Centauri is mass_sol = sum of A+B+Proxima ≈ 2.129.
    let alpha_cen = catalog
        .get("Alpha Centauri")
        .expect("Alpha Centauri must be present");
    let expected_mu = 2.129 * GM_SUN;
    assert!(
        approx_eq(alpha_cen.mu_m3_s2, expected_mu, 1.0),
        "α-Cen mu_m3_s2 = {} (expected ~{})",
        alpha_cen.mu_m3_s2,
        expected_mu
    );
}

#[test]
fn advance_position_at_epoch_unchanged() {
    let catalog = load_catalog_for_test();
    for sys in &catalog.systems {
        let p = advance_position(sys, EPOCH_BEACON_GAME_START_SIM_S);
        assert!(
            approx_eq(p.x, sys.position_m.x, 1e-9)
                && approx_eq(p.y, sys.position_m.y, 1e-9)
                && approx_eq(p.z, sys.position_m.z, 1e-9),
            "advance_position(&{}, 0.0) must return position_m exactly (got {:?} vs {:?})",
            sys.system_id,
            (p.x, p.y, p.z),
            (sys.position_m.x, sys.position_m.y, sys.position_m.z)
        );
    }
}

#[test]
fn advance_position_at_1yr_is_velocity_offset() {
    let catalog = load_catalog_for_test();
    let alpha_cen = catalog
        .get("Alpha Centauri")
        .expect("Alpha Centauri must be present");
    // α-Cen published velocity (Gaia DR3): (-23.2, 0.7, 0.4) km/s.
    let one_year_s = 365.25 * 86_400.0;
    let p_t = advance_position(alpha_cen, one_year_s);
    let expected_dx_m = alpha_cen.velocity_kms[0] * one_year_s * 1000.0;
    let expected_dy_m = alpha_cen.velocity_kms[1] * one_year_s * 1000.0;
    let expected_dz_m = alpha_cen.velocity_kms[2] * one_year_s * 1000.0;
    // Tolerance 0.01% per GRA-332 §8.
    let tol = 1e-4 * (expected_dx_m.abs() + expected_dy_m.abs() + expected_dz_m.abs() + 1.0);
    let actual_dx = p_t.x - alpha_cen.position_m.x;
    let actual_dy = p_t.y - alpha_cen.position_m.y;
    let actual_dz = p_t.z - alpha_cen.position_m.z;
    assert!(
        (actual_dx - expected_dx_m).abs() < tol
            && (actual_dy - expected_dy_m).abs() < tol
            && (actual_dz - expected_dz_m).abs() < tol,
        "α-Cen 1-yr advance: expected Δm = ({:.3e}, {:.3e}, {:.3e}), got ({:.3e}, {:.3e}, {:.3e})",
        expected_dx_m,
        expected_dy_m,
        expected_dz_m,
        actual_dx,
        actual_dy,
        actual_dz,
    );
}

#[test]
fn hill_sphere_alpha_cen_proxima_correct() {
    // α-Cen AB + Proxima outer pair: a = 8700 AU, m_Proxima = 0.122,
    // m_α-Cen_AB ≈ 2.007.  Expected ≈ 2375 AU per GRA-332 §4 worked
    // example.
    let r = hill_sphere_au(8700.0, 0.122, 2.007);
    assert!(
        approx_eq(r, 2375.0, 10.0),
        "hill_sphere_au(8700, 0.122, 2.007) = {r} AU (expected ~2375 AU)"
    );
}

#[test]
fn name_to_idx_roundtrip() {
    let catalog = load_catalog_for_test();
    for sys in &catalog.systems {
        let resolved = catalog
            .get(&sys.system_id)
            .expect("by_name lookup must succeed for every loaded system");
        assert_eq!(resolved.system_id, sys.system_id);
        assert_eq!(resolved.display_name, sys.display_name);
    }
}

#[test]
fn open_ended_loader_accepts_extra_entries() {
    // Modder scenario: the catalog schema is an open-ended Vec, so adding
    // a 61st system must parse without Rust changes.  We assert the
    // deserializer side of the contract: any number of `systems`
    // entries round-trips.  The runtime picks them up via the production
    // loader's `Vec::with_capacity` + drain loop, which has no length cap.
    #[derive(serde::Deserialize)]
    struct Probe {
        #[allow(dead_code)]
        catalog_epoch_sim_s: f64,
        systems: Vec<ProbeSystem>,
    }
    // Mirror the production schema: every field the runtime cares about
    // is optional on the probe struct so this test stays focused on
    // entry-count round-trip, not field coverage (covered by the other
    // data tests in this file).
    #[derive(serde::Deserialize)]
    #[allow(dead_code)]
    struct ProbeSystem {
        #[serde(default)]
        system_id: String,
        #[serde(default)]
        display_name: String,
        #[serde(default)]
        spectral_type: String,
        #[serde(default)]
        mass_sol: f32,
        #[serde(default)]
        pos_ly_galactic: (f32, f32, f32),
        #[serde(default)]
        velocity_kms: (f32, f32, f32),
    }
    let big_ron = std::fs::read_to_string("assets/data/nearest_stars.ron")
        .expect("catalog file is required for the modder-scenario test");
    let probe: Probe = ron::from_str(&big_ron).expect("production RON deserializes");
    assert_eq!(
        probe.systems.len(),
        60,
        "production catalog must have 60 systems (parity check)"
    );
    // Add a 61st entry to the RON string and re-parse.  A schema that's
    // "open-ended" will accept this without modification.
    let mut augmented = big_ron.clone();
    // Detect the line ending the catalog uses so the appended entry
    // matches (the production file ships with CRLF on Windows).
    let crlf = augmented.contains("\r\n");
    let nl = if crlf { "\r\n" } else { "\n" };
    // Locate the last entry's closing `)` so we can splice a new entry
    // right after it (followed by `,` and the array close `]`).  We
    // keep up to and including `        )` and drop the trailing `,`
    // + array/struct close; the appended text starts with a fresh `,`
    // separator before the new entry, then re-closes the array + struct.
    let last_entry_close_marker = format!("        ),{nl}    ],{nl})");
    if let Some(idx) = augmented.rfind(&last_entry_close_marker) {
        // Keep everything up to `        )` (drop the trailing `,` and
        // the `],)` close); the appended text provides the rest.
        let keep_len = idx + "        )".len();
        augmented.truncate(keep_len);
        let entry = format!(
            ",{nl}        ({nl}            system_id: \"Test Modder System\",{nl}            \
             display_name: \"Test Modder System\",{nl}            spectral_type: \"G2V\",{nl}            \
             mass_sol: 1.0,{nl}            pos_ly_galactic: (0.0, 0.0, 0.0),{nl}            \
             velocity_kms: (0.0, 0.0, 0.0),{nl}        ),{nl}    ],{nl})",
        );
        augmented.push_str(&entry);
    }
    let probe2: Probe = ron::from_str(&augmented).expect("augmented RON must deserialize");
    assert_eq!(
        probe2.systems.len(),
        61,
        "RON schema must accept arbitrary numbers of system entries (modder scenario)"
    );
    assert!(
        probe2
            .systems
            .iter()
            .any(|s| s.system_id == "Test Modder System"),
        "modder-added 61st system must appear in the parsed catalog"
    );
}
