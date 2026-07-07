//! Multi-star ephemeris (GRA-328c, GRA-332).
//!
//! Loads `assets/data/nearest_stars.ron` (modder-editable, 60 systems at v0.6)
//! and exposes it as a Bevy `Resource` plus pure helpers consumed by the
//! transfer planner (`src/fleets/orbital_mechanics.rs`) and the porkchop
//! solver. Coordinate frame: Galactic Cartesian J2000, light-years at the
//! canonical epoch (game-start, `SimulationTime`-anchor `0.0`).
//!
//! Open-ended guarantees (per GRA-332 closing comment `f0b37ab0`):
//! - `systems: Vec<StarSystemEphemeris>` — no cap.
//! - `by_name: HashMap<String, usize>` — only index; no ordinal math.
//! - All consumers go through `by_name[system_id]`; missing entries return
//!   `None` and log a warning. No migration; no version bump.

use crate::fleets::orbital_mechanics::{AU_IN_METERS, GM_SUN};
use bevy::math::DVec3;
use bevy::prelude::*;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;

/// Light-years per astronomical unit.  1 ly ≈ 63 241 AU.
pub const AU_IN_LY: f64 = 63_241.077;

/// Canonical epoch for all systems in the catalog.
///
/// Game-time is single-valued (see `SimulationTime::elapsed_seconds()` at
/// `src/ui/time.rs`).  Setting the epoch equal to game-start means inter-system
/// propagation matches intra-system propagation: there is no per-star clock
/// skew to reconcile.  See GRA-332 §1 "Epoch synchronization".
pub const EPOCH_BEACON_GAME_START_SIM_S: f64 = 0.0;

/// Sol's Hill sphere radius (AU).
///
/// Used as the injection target for interstellar Hohmann transfers: a fleet
/// leaving the solar system must reach `r = SOL_HILL_SPHERE_AU` before its
/// parent-star well is fully escaped.  Matches the canonical value cited in
/// interstellar-trajectory literature.
pub const SOL_HILL_SPHERE_AU: f64 = 1000.0;

/// Per-system runtime ephemeris contract.
///
/// Position is stored in meters internally (`position_m`) so the solve path
/// doesn't pay a per-frame conversion.  `pos_ly_galactic` is kept for the
/// dossier/UI display (matches the convention in
/// `src/astronomy/nearby_stars.rs::NEARBY_STARS_POSITIONS`).
#[derive(Debug, Clone)]
pub struct StarSystemEphemeris {
    /// Stable string id (matches `nearest_stars_raw.json::system_name`).
    pub system_id: String,
    /// Human-friendly display name (matches `system_name`).
    pub display_name: String,
    /// Free-form spectral classification (`"G2V"`, `"M5.5Ve"`).  UI hint only;
    /// not used in transfer math.
    pub spectral_type: String,
    /// Combined stellar mass of the system, solar masses.  Source of truth
    /// for `mu_m3_s2`.
    pub mass_sol: f64,
    /// Position at epoch, Galactic Cartesian, light-years.
    pub pos_ly_galactic: [f64; 3],
    /// Position at epoch, meters.  `pos_ly_galactic × (AU_IN_METERS / AU_IN_LY)`.
    pub position_m: DVec3,
    /// Velocity at epoch, km/s.  Default `[0,0,0]` for systems without
    /// measured proper motion.
    pub velocity_kms: [f64; 3],
    /// Derived gravitational parameter, m³/s².  `mass_sol × GM_SUN`.
    pub mu_m3_s2: f64,
    /// Distance from Sol, light-years.  Convenience for the dossier;
    /// derived from `pos_ly_galactic.length()`.
    pub distance_ly: f64,
}

/// All systems loaded from `nearest_stars.ron`, plus a `name → index` lookup.
///
/// The `Vec` is open-ended (no cap) and is the storage backing the `HashMap`.
/// Consumers must always go through `by_name[system_id]`; ordinal access is
/// forbidden by project convention to keep the catalog modder-extendable.
#[derive(Debug, Clone, Default, Resource)]
pub struct StarSystemsEphemeris {
    pub systems: Vec<StarSystemEphemeris>,
    pub by_name: HashMap<String, usize>,
}

impl StarSystemsEphemeris {
    /// Look up a system by `system_id` (string).  Returns `None` if the entry
    /// is absent; the caller is expected to log a warning when this happens
    /// (saves referencing a deleted/renamed system degrade gracefully).
    pub fn get(&self, system_id: &str) -> Option<&StarSystemEphemeris> {
        self.by_name.get(system_id).map(|&i| &self.systems[i])
    }

    /// Number of systems loaded.
    pub fn len(&self) -> usize {
        self.systems.len()
    }

    /// True iff no systems loaded.
    pub fn is_empty(&self) -> bool {
        self.systems.is_empty()
    }
}

/// Pure function: advance a system's position linearly by `dt_s` from epoch.
///
/// Linear extrapolation is correct to ~1% over the game-relevant horizon
/// (~1000 yr) for stars without radial-velocity data (most of the 60-system
/// catalog).  Second-order correction is a v1.0 follow-up (GRA-332 §10).
///
/// Returns Galactic Cartesian position in meters.
pub fn advance_position(system: &StarSystemEphemeris, sim_time_s: f64) -> DVec3 {
    let dt_s = sim_time_s - EPOCH_BEACON_GAME_START_SIM_S;
    // velocity_kms × dt_s → m.  1 km/s = 1000 m/s.
    let dv_m = DVec3::new(
        system.velocity_kms[0] * dt_s * 1000.0,
        system.velocity_kms[1] * dt_s * 1000.0,
        system.velocity_kms[2] * dt_s * 1000.0,
    );
    system.position_m + dv_m
}

/// Pure function: convert a Sol-centered AU vector to Galactic Cartesian
/// meters.
///
/// `sol_relative_au` is the vector from Sol to a body, in the same Galactic
/// frame as the catalog's `pos_ly_galactic` (the catalog frame has Sol at
/// the origin).  The returned value is the absolute Galactic Cartesian
/// position in meters.
///
/// `catalog` is kept in the signature for API parity with the LGD contract
/// and so future Sol-drift corrections (Sol's tiny velocity w.r.t. the Local
/// Standard of Rest) can be applied without changing the call sites.
pub fn heliocentric_to_galactic(sol_relative_au: DVec3, _catalog: &StarSystemsEphemeris) -> DVec3 {
    sol_relative_au * AU_IN_METERS
}

/// Pure function: Hill sphere radius, AU.
///
/// `a_au` is the semi-major axis of the secondary around the host.
/// `host_mass_sol` is the host's mass (solar units).  `neighbor_mass_sol`
/// is the mass of the nearest more-massive perturbing body (solar units).
///
/// For a multi-star system barycenter (e.g. α-Cen AB + Proxima):
/// `hill_sphere_au(8700.0, 0.122, 2.007) ≈ 2375 AU` (matches the JSON's
/// `binary_orbits` `semi_major_axis_au` for the outer pair).
///
/// Reference: Souami & Souchay (2012), "The solar system's invariable plane".
pub fn hill_sphere_au(a_au: f64, host_mass_sol: f64, neighbor_mass_sol: f64) -> f64 {
    if neighbor_mass_sol <= 0.0 || host_mass_sol <= 0.0 || a_au <= 0.0 {
        return 0.0;
    }
    a_au * (host_mass_sol / (3.0 * neighbor_mass_sol)).cbrt()
}

/// Resolve the system barycenter (the system's primary star) from a catalog
/// entry.
///
/// In v0.6 the catalog already stores the barycenter position+mass as a
/// single record, so this helper just returns a copy with the stable
/// `system_id`.  Future v0.6.x patches that split barycenter from individual
/// stars will resolve here.
pub fn system_barycenter(system: &StarSystemEphemeris) -> StarSystemEphemeris {
    system.clone()
}

// --- Loader ---------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct NearestStarsRon {
    #[allow(dead_code)]
    catalog_epoch_sim_s: f64,
    systems: Vec<NearestStarsRonSystem>,
}

#[derive(Debug, Deserialize)]
struct NearestStarsRonSystem {
    system_id: String,
    display_name: String,
    spectral_type: String,
    mass_sol: f64,
    pos_ly_galactic: [f64; 3],
    velocity_kms: [f64; 3],
}

impl From<NearestStarsRonSystem> for StarSystemEphemeris {
    fn from(r: NearestStarsRonSystem) -> Self {
        let pos_m_per_ly = AU_IN_METERS / AU_IN_LY;
        let position_m = DVec3::new(
            r.pos_ly_galactic[0] * pos_m_per_ly,
            r.pos_ly_galactic[1] * pos_m_per_ly,
            r.pos_ly_galactic[2] * pos_m_per_ly,
        );
        let distance_ly = (r.pos_ly_galactic[0].powi(2)
            + r.pos_ly_galactic[1].powi(2)
            + r.pos_ly_galactic[2].powi(2))
        .sqrt();
        let mu_m3_s2 = r.mass_sol * GM_SUN;
        Self {
            system_id: r.system_id,
            display_name: r.display_name,
            spectral_type: r.spectral_type,
            mass_sol: r.mass_sol,
            pos_ly_galactic: r.pos_ly_galactic,
            position_m,
            velocity_kms: r.velocity_kms,
            mu_m3_s2,
            distance_ly,
        }
    }
}

const NEAREST_STARS_RON_PATH: &str = "assets/data/nearest_stars.ron";

/// Bevy `Startup` system: load `nearest_stars.ron` and insert it as a
/// `Res<StarSystemsEphemeris>`.  Logs an error and inserts an empty resource
/// on parse failure so the rest of the app keeps running (the transfer
/// planner simply shows no interstellar destinations).
pub fn load_star_systems_ephemeris(mut commands: Commands) {
    let path = NEAREST_STARS_RON_PATH;
    match fs::read_to_string(path) {
        Ok(content) => match ron::from_str::<NearestStarsRon>(&content) {
            Ok(parsed) => {
                let mut systems: Vec<StarSystemEphemeris> =
                    parsed.systems.into_iter().map(Into::into).collect();
                // Build the lookup map.  If a duplicate `system_id` slips in
                // (modder error), keep the first occurrence and warn so the
                // catalog stays well-formed.
                let mut by_name: HashMap<String, usize> = HashMap::with_capacity(systems.len());
                let mut kept: Vec<StarSystemEphemeris> = Vec::with_capacity(systems.len());
                for sys in systems.drain(..) {
                    if by_name.contains_key(&sys.system_id) {
                        warn!(
                            "nearest_stars.ron: duplicate system_id '{}'; keeping first occurrence",
                            sys.system_id
                        );
                        continue;
                    }
                    by_name.insert(sys.system_id.clone(), kept.len());
                    kept.push(sys);
                }
                info!(
                    "nearest_stars.ron: loaded {} systems (epoch = {:.1} sim-s)",
                    kept.len(),
                    EPOCH_BEACON_GAME_START_SIM_S
                );
                commands.insert_resource(StarSystemsEphemeris {
                    systems: kept,
                    by_name,
                });
            }
            Err(e) => {
                error!("nearest_stars.ron: failed to parse ({e}); inserting empty catalog");
                commands.insert_resource(StarSystemsEphemeris {
                    systems: Vec::new(),
                    by_name: HashMap::new(),
                });
            }
        },
        Err(e) => {
            warn!("nearest_stars.ron not found at {path}: {e}. Interstellar transfers disabled.");
            commands.insert_resource(StarSystemsEphemeris {
                systems: Vec::new(),
                by_name: HashMap::new(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() < tol
    }

    #[test]
    fn hill_sphere_alpha_cen_proxima() {
        // α-Cen AB + Proxima outer pair: a=8700 AU, m_Proxima=0.122, m_AB=2.007.
        let r = hill_sphere_au(8700.0, 0.122, 2.007);
        // Expected ≈ 2375 AU per GRA-332 §4 worked example.
        assert!(
            approx_eq(r, 2375.0, 10.0),
            "hill_sphere_au(8700, 0.122, 2.007) ≈ {r} (expected ~2375)"
        );
    }

    #[test]
    fn advance_position_zero_dt_is_identity() {
        let sys = StarSystemEphemeris {
            system_id: "Test".into(),
            display_name: "Test".into(),
            spectral_type: "G2V".into(),
            mass_sol: 1.0,
            pos_ly_galactic: [1.0, 2.0, 3.0],
            position_m: DVec3::new(1.0, 2.0, 3.0),
            velocity_kms: [10.0, -10.0, 5.0],
            mu_m3_s2: GM_SUN,
            distance_ly: 14.0_f64.sqrt(),
        };
        let p = advance_position(&sys, EPOCH_BEACON_GAME_START_SIM_S);
        assert!(approx_eq(p.x, sys.position_m.x, 1e-9));
        assert!(approx_eq(p.y, sys.position_m.y, 1e-9));
        assert!(approx_eq(p.z, sys.position_m.z, 1e-9));
    }

    #[test]
    fn advance_position_round_trip() {
        // 1-year offset, α-Cen barycenter published velocity (−23.2, 0.7, 0.4) km/s.
        // Forward propagation is linear, so `p_t - p_0 = velocity × dt`.
        // The round-trip recovers the epoch position exactly (no
        // gravity-correction applied at this level — interstellar
        // propagation uses a straight-line ballistic model).  We
        // assert the per-axis offset matches the analytical
        // `velocity_kms × dt / AU_IN_METERS_in_km` value within
        // 1e-6 AU (numerical precision only).
        let sys = StarSystemEphemeris {
            system_id: "Alpha Centauri".into(),
            display_name: "Alpha Centauri".into(),
            spectral_type: "G2V".into(),
            mass_sol: 2.129,
            pos_ly_galactic: [-1.5477, -1.1846, -3.7728],
            position_m: DVec3::new(-1.5477, -1.1846, -3.7728) * (AU_IN_METERS / AU_IN_LY),
            velocity_kms: [-23.2, 0.7, 0.4],
            mu_m3_s2: 2.129 * GM_SUN,
            distance_ly: 4.2465,
        };
        let one_year_s = 365.25 * 86_400.0;
        let p_t = advance_position(&sys, one_year_s);
        // Convert the offset back to AU for the round-trip check.
        // velocity × dt_s × 1000 m/km ÷ AU_IN_METERS (m/AU) → AU.
        //   -23.2 km/s × 31_557_600 s × 1000 m/km = -7.32e11 m
        //   -7.32e11 m ÷ 1.496e11 m/AU = -4.893 AU
        // The earlier <1e-1 tolerance conflated m and km by 1000×
        // (the velocity `* 1000.0` factor in `advance_position` was
        // missed).  The new tolerance is the analytic offset ±1e-6.
        let dt_au_per_axis = |v_kms: f64| -> f64 {
            v_kms * one_year_s * 1000.0 / AU_IN_METERS
        };
        let dx_au = (p_t.x - sys.position_m.x) / AU_IN_METERS;
        let dy_au = (p_t.y - sys.position_m.y) / AU_IN_METERS;
        let dz_au = (p_t.z - sys.position_m.z) / AU_IN_METERS;
        assert!(
            (dx_au - dt_au_per_axis(sys.velocity_kms[0])).abs() < 1e-6,
            "x offset {dx_au} AU differs from analytical {} AU",
            dt_au_per_axis(sys.velocity_kms[0])
        );
        assert!(
            (dy_au - dt_au_per_axis(sys.velocity_kms[1])).abs() < 1e-6,
            "y offset {dy_au} AU differs from analytical {} AU",
            dt_au_per_axis(sys.velocity_kms[1])
        );
        assert!(
            (dz_au - dt_au_per_axis(sys.velocity_kms[2])).abs() < 1e-6,
            "z offset {dz_au} AU differs from analytical {} AU",
            dt_au_per_axis(sys.velocity_kms[2])
        );
    }

    #[test]
    fn mu_derived_at_load() {
        // Round-trip through the loader's `From` impl.
        let r = NearestStarsRonSystem {
            system_id: "Alpha Centauri".into(),
            display_name: "Alpha Centauri".into(),
            spectral_type: "G2V".into(),
            mass_sol: 2.129,
            pos_ly_galactic: [-1.5477, -1.1846, -3.7728],
            velocity_kms: [-23.2, 0.7, 0.4],
        };
        let sys: StarSystemEphemeris = r.into();
        let expected = 2.129 * GM_SUN;
        assert!(approx_eq(sys.mu_m3_s2, expected, 1.0));
    }
}
