//! Survey systems.
//!
//! PR-A scaffold: system function stubs only. The actual logic lands
//! in subsequent PRs:
//!
//! - PR-B (instruments + mission templates): `advance_survey_missions`
//!   is wired up.
//! - PR-C (analysis queue): `process_analysis_queue` becomes real.
//! - PR-D (anomalies + landing sites): `surface_anomaly_events` and
//!   `evaluate_landing_sites` become real.
//! - PR-F (mining efficiency): `compute_mining_efficiency` becomes
//!   real.
//!
//! The stubs are present so the plugin registration site is stable
//! across PRs — adding a new system is a one-line change in `mod.rs`
//! rather than a structural refactor.

use bevy::prelude::*;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use super::components::{
    ExtractionSite, LandingSite, SiteScores, SurveyState, LANDING_SITE_EVAL_THRESHOLD,
    MAX_SITES_PER_BODY, MIN_SITES_PER_BODY,
};
use crate::colony::types::BuildingType;
use crate::plugins::solar_system::{Asteroid, CelestialBody, GasGiant};

/// Tick confidence decay for all known dimensions. No-op in PR-A
/// because no `SurveyState` components are attached to bodies yet;
/// wired up in PR-B once the system populator starts inserting
/// `SurveyState`.
pub fn decay_survey_confidence(_time: Res<SimulationTime>, _query: Query<&mut SurveyState>) {
    // PR-A stub. Implementation in PR-B:
    //
    //   use super::types::{CONFIDENCE_DECAY_PER_YEAR, SURVEY_DAYS_PER_YEAR};
    //   let elapsed_years = (time.elapsed_seconds() - state.last_updated_sim_time)
    //       / SURVEY_DAYS_PER_YEAR;
    //   let decay = elapsed_years * CONFIDENCE_DECAY_PER_YEAR as f64;
    //   for fidelity in state.dimensions.values_mut() {
    //       fidelity.confidence = (fidelity.confidence - decay as f32).max(0.0);
    //   }
    //   state.last_updated_sim_time = time.elapsed_seconds();
}

/// Tick active survey missions. No-op in PR-A; wired up in PR-B.
pub fn advance_survey_missions(_time: Res<SimulationTime>, _query: Query<&mut SurveyState>) {
    // PR-A stub.
}

/// Drive the analysis queue. No-op in PR-A; wired up in PR-C.
pub fn process_analysis_queue(_time: Res<SimulationTime>, _query: Query<&mut SurveyState>) {
    // PR-A stub.
}

/// Fire events for newly-detected anomalies. No-op in PR-A; wired up
/// in PR-D.
pub fn surface_anomaly_events(_time: Res<SimulationTime>, _query: Query<&SurveyState>) {
    // PR-A stub.
}

/// Update the system-wide "SURVEY %" stat. No-op in PR-A; wired up
/// in PR-B.
pub fn update_survey_summary(_query: Query<&SurveyState>) {
    // PR-A stub.
}

/// Re-export the simulation time type from the `ui::time` module
/// so systems can declare `Res<SimulationTime>` without taking a
/// hard dependency on the `ui` module.
///
/// Mirrors the pattern used by `economy::systems` and
/// `research::systems`.
pub type SimulationTime = crate::ui::time::SimulationTime;

/// Sim-days between landing-site re-evaluation passes. The system
/// throttles itself so it doesn't re-roll sites every frame as
/// confidence rises — the trigger is the cross from below-threshold
/// to above-threshold (or vice versa), not a continuous recompute.
const LANDING_SITE_EVAL_PERIOD_DAYS: f64 = 30.0;
const SECONDS_PER_DAY_F64: f64 = 86_400.0;

/// Evaluate landing / extraction sites for every body with a
/// `SurveyState`. Lands in PR-D.
///
/// Behaviour:
/// - Gas giants are skipped entirely (no sites; the dossier section
///   is hidden — see GRA-82 acceptance criteria).
/// - Asteroids get an `ExtractionSite` list with a mining-focused
///   `feasible_for` set.
/// - Other solid bodies (planet, dwarf planet, moon) get a
///   `LandingSite` list with the surface-base / habitat / launch
///   `feasible_for` set.
/// - The system is idempotent at the boundary: once sites are
///   generated, the list is held stable across re-evaluations unless
///   the body drops back below the threshold AND the existing sites
///   are inconsistent with the current coverage (we just leave them
///   in place — re-rolling sites every frame would surprise the
///   player who's about to click one).
pub fn evaluate_landing_sites(
    time: Res<SimulationTime>,
    mut query: Query<
        (
            Entity,
            &CelestialBody,
            &mut SurveyState,
            Option<&Asteroid>,
            Option<&GasGiant>,
        ),
        bevy::ecs::query::Changed<SurveyState>,
    >,
) {
    let now = time.elapsed_seconds();
    for (_entity, body, mut state, asteroid_marker, gas_giant_marker) in &mut query {
        // Throttle: only re-evaluate once per period.
        let period_seconds = LANDING_SITE_EVAL_PERIOD_DAYS * SECONDS_PER_DAY_F64;
        if (now - state.last_landing_site_eval_sim_time) < period_seconds {
            continue;
        }
        state.last_landing_site_eval_sim_time = now;

        // Gas giants: hide the section entirely. No sites.
        if gas_giant_marker.is_some() {
            state.landing_sites.clear();
            state.extraction_sites.clear();
            continue;
        }

        // Coverage gate.
        let coverage = state.landing_site_eval_coverage();
        let below_threshold = coverage < LANDING_SITE_EVAL_THRESHOLD;

        if below_threshold {
            // Below threshold: don't regenerate. The first PR-D pass
            // gives us sites the moment coverage crosses 0.6; if the
            // player later downgrades (unlikely) the existing list
            // stays. A follow-up PR can re-add a "clear on
            // down-grade" hook if gameplay wants it.
            continue;
        }

        // Above threshold and not yet generated: roll the sites.
        if asteroid_marker.is_some() {
            if state.extraction_sites.is_empty() {
                state.extraction_sites = generate_extraction_sites(body);
            }
        } else if state.landing_sites.is_empty() {
            state.landing_sites = generate_landing_sites(body);
        }
    }
}

/// Number of sites to generate per body. Sits in
/// `MIN_SITES_PER_BODY..=MAX_SITES_PER_BODY` and is stable per body
/// (the body-name seed picks a count once).
fn site_count_for_body(body: &CelestialBody, is_asteroid: bool) -> usize {
    let range = (MAX_SITES_PER_BODY - MIN_SITES_PER_BODY + 1) as u32;
    let offset = stable_hash_u32(&body.name, b"site_count", is_asteroid) % range;
    (MIN_SITES_PER_BODY as u32 + offset) as usize
}

/// Deterministic, body-stable hash. The same `body.name` + tag always
/// produces the same `u32`, so a saved body's site list is stable
/// across save/load (and across runs). Modders can rebalance by
/// editing the `body.name` to seed differently; the surface biases
/// themselves are RON-extensible in a follow-up.
fn stable_hash_u32(name: &str, tag: &[u8], is_asteroid: bool) -> u32 {
    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    tag.hash(&mut hasher);
    is_asteroid.hash(&mut hasher);
    hasher.finish() as u32
}

/// Map a `u32` hash slice into `[0.0, 1.0)`.
fn hash_to_unit(h: u32) -> f32 {
    (h as f32) / (u32::MAX as f32)
}

/// Stable per-site name table. Site N on a body uses the Nth entry
/// (modulo length) of this list. Pure-string, no allocation, no RNG.
const SITE_NAME_FRAGMENTS: &[&str] = &[
    "Alpha", "Beta", "Gamma", "Delta", "Epsilon", "Zeta", "Eta", "Theta", "Iota", "Kappa",
];

fn make_site_name(body_name: &str, index: u32) -> String {
    let fragment = SITE_NAME_FRAGMENTS[(index as usize) % SITE_NAME_FRAGMENTS.len()];
    format!("{body_name} {fragment}")
}

/// Generate the per-site scores for an index, deterministically.
///
/// All six sub-scores are derived from the same `DefaultHasher` so a
/// site's score vector is stable across save/load. The values
/// represent `0.0..=1.0` qualities; the composite is computed by
/// [`SiteScores::composite`].
fn make_scores(body_name: &str, index: u32, is_asteroid: bool) -> SiteScores {
    let pick = |tag: &[u8]| -> f32 {
        let h = stable_hash_u32(body_name, tag, is_asteroid)
            .wrapping_add(index.wrapping_mul(0x9E3779B9));
        hash_to_unit(h).clamp(0.05, 0.99)
    };
    SiteScores {
        slope: pick(b"slope"),
        roughness: pick(b"roughness"),
        radiation: pick(b"radiation"),
        temperature: pick(b"temperature"),
        regolith: pick(b"regolith"),
        comm: pick(b"comm"),
    }
}

/// Generate the lat/lon pair for a site. Distributed across the body
/// so the dossier shows distinct candidates. Deterministic per
/// (body, index).
fn make_lat_lon(body_name: &str, index: u32, is_asteroid: bool) -> (f32, f32) {
    let lat_h = stable_hash_u32(body_name, b"lat", is_asteroid)
        .wrapping_add(index.wrapping_mul(0x6C50B47C));
    let lon_h = stable_hash_u32(body_name, b"lon", is_asteroid)
        .wrapping_add(index.wrapping_mul(0x6C50B47C));
    // Map to [-90, 90] and [-180, 180].
    let lat = (hash_to_unit(lat_h) * 180.0) - 90.0;
    let lon = (hash_to_unit(lon_h) * 360.0) - 180.0;
    (lat, lon)
}

/// Default `feasible_for` for a landing site on a solid body.
///
/// Maps the GRA-82 brief ("SurfaceBase, UndergroundHabitat,
/// LaunchSite, LandingPad") to existing `BuildingType` variants —
/// "SurfaceBase" → `HabitatDome` (closest existing building for
/// surface habitation), "LandingPad" → `SpacePort` (closest existing
/// launch infrastructure). The list is intentionally a superset so
/// the UI shows the player every building that's not blocked.
fn landing_site_feasible_default() -> Vec<BuildingType> {
    vec![
        BuildingType::HabitatDome,
        BuildingType::UndergroundHabitat,
        BuildingType::LaunchSite,
        BuildingType::SpacePort,
    ]
}

/// Default `feasible_for` for an extraction site on an asteroid.
fn extraction_site_feasible_default() -> Vec<BuildingType> {
    vec![
        BuildingType::Mine,
        BuildingType::Refinery,
        BuildingType::MassDriver,
    ]
}

/// Compute the per-site blockers from the score vector. Mirrors the
/// GRA-82 example: heavy industry is blocked on steep slopes, launch
/// sites on rough ground, and habitats where radiation or
/// temperature are dangerous.
fn landing_site_blockers(scores: &SiteScores) -> Vec<BuildingType> {
    let mut blockers = Vec::new();
    if scores.slope < 0.4 {
        // Slope too steep for heavy industry.
        blockers.push(BuildingType::Factory);
        blockers.push(BuildingType::Refinery);
    }
    if scores.roughness < 0.35 {
        blockers.push(BuildingType::LaunchSite);
        blockers.push(BuildingType::SpacePort);
    }
    if scores.radiation < 0.4 || scores.temperature < 0.4 {
        blockers.push(BuildingType::HabitatDome);
        blockers.push(BuildingType::Housing);
    }
    if scores.regolith < 0.35 {
        blockers.push(BuildingType::UndergroundHabitat);
    }
    blockers
}

/// Blockers for an extraction site. Fewer constraints than a landing
/// site — most mining is robotic.
fn extraction_site_blockers(scores: &SiteScores) -> Vec<BuildingType> {
    let mut blockers = Vec::new();
    if scores.slope < 0.25 {
        blockers.push(BuildingType::Refinery);
    }
    if scores.regolith < 0.2 {
        blockers.push(BuildingType::Mine);
    }
    blockers
}

fn generate_landing_sites(body: &CelestialBody) -> Vec<LandingSite> {
    let count = site_count_for_body(body, false);
    (0..count as u32)
        .map(|i| {
            let scores = make_scores(&body.name, i, false);
            let (lat, lon) = make_lat_lon(&body.name, i, false);
            LandingSite {
                id: i,
                name: make_site_name(&body.name, i),
                latitude: lat,
                longitude: lon,
                blockers: landing_site_blockers(&scores),
                feasible_for: landing_site_feasible_default(),
                scores,
            }
        })
        .collect()
}

fn generate_extraction_sites(body: &CelestialBody) -> Vec<ExtractionSite> {
    let count = site_count_for_body(body, true);
    (0..count as u32)
        .map(|i| {
            let scores = make_scores(&body.name, i, true);
            let (lat, lon) = make_lat_lon(&body.name, i, true);
            ExtractionSite {
                id: i,
                name: make_site_name(&body.name, i),
                latitude: lat,
                longitude: lon,
                blockers: extraction_site_blockers(&scores),
                feasible_for: extraction_site_feasible_default(),
                scores,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    //! PR-D tests for landing-site evaluation. Exercises the score /
    //! blocker / site generation helpers without spinning up Bevy —
    //! the system-level coverage / throttling tests live in
    //! `tests/survey_systems_tests.rs` so they can use the full
    //! `App` harness.

    use super::*;
    use crate::colony::types::BuildingType;
    use crate::plugins::solar_system::CelestialBody;
    use crate::plugins::solar_system_data::{AsteroidClass, BodyType};

    fn body(name: &str, body_type: BodyType) -> CelestialBody {
        CelestialBody {
            name: name.to_string(),
            radius: 1_000.0,
            mass: 1.0e20,
            body_type,
            visual_radius: 1.0,
            asteroid_class: if body_type == BodyType::Asteroid {
                Some(AsteroidClass::SType)
            } else {
                None
            },
        }
    }

    #[test]
    fn site_count_is_in_range_for_various_bodies() {
        for (name, bt) in [
            ("Mars", BodyType::Planet),
            ("Europa", BodyType::Moon),
            ("Ceres", BodyType::DwarfPlanet),
            ("Vesta", BodyType::Asteroid),
        ] {
            let b = body(name, bt);
            let n = site_count_for_body(&b, bt == BodyType::Asteroid);
            assert!(
                (MIN_SITES_PER_BODY..=MAX_SITES_PER_BODY).contains(&n),
                "{name}: site count {n} out of range"
            );
        }
    }

    #[test]
    fn site_count_is_stable_per_body() {
        // Same name → same count. The hash-based selection must be
        // deterministic so save/load is stable.
        let b = body("Mars", BodyType::Planet);
        let a = site_count_for_body(&b, false);
        let b2 = site_count_for_body(&b, false);
        assert_eq!(a, b2);
    }

    #[test]
    fn landing_site_blockers_fire_on_poor_scores() {
        // Below-threshold slope/roughness/radiation/regolith should
        // all add the expected buildings to the blocker list.
        let s = SiteScores {
            slope: 0.1,
            roughness: 0.1,
            radiation: 0.1,
            temperature: 0.5,
            regolith: 0.1,
            comm: 0.5,
        };
        let b = landing_site_blockers(&s);
        assert!(b.contains(&BuildingType::Factory));
        assert!(b.contains(&BuildingType::Refinery));
        assert!(b.contains(&BuildingType::LaunchSite));
        assert!(b.contains(&BuildingType::SpacePort));
        assert!(b.contains(&BuildingType::HabitatDome));
        assert!(b.contains(&BuildingType::Housing));
        assert!(b.contains(&BuildingType::UndergroundHabitat));
    }

    #[test]
    fn landing_site_blockers_empty_for_ideal_scores() {
        // All sub-scores at 1.0 → no constraints fire. The dossier
        // should show "all buildings feasible" for the site.
        let s = SiteScores {
            slope: 1.0,
            roughness: 1.0,
            radiation: 1.0,
            temperature: 1.0,
            regolith: 1.0,
            comm: 1.0,
        };
        assert!(landing_site_blockers(&s).is_empty());
    }

    #[test]
    fn extraction_site_blockers_are_stricter_on_mining() {
        // An asteroid site with the same low scores as the landing
        // site should block Refinery + Mine (no habitat concerns).
        let s = SiteScores {
            slope: 0.1,
            roughness: 0.5,
            radiation: 0.5,
            temperature: 0.5,
            regolith: 0.1,
            comm: 0.5,
        };
        let b = extraction_site_blockers(&s);
        assert!(b.contains(&BuildingType::Refinery));
        assert!(b.contains(&BuildingType::Mine));
        // Habitats aren't on the feasible list, so they're never
        // blocked either.
        assert!(!b.contains(&BuildingType::HabitatDome));
    }

    #[test]
    fn generated_landing_sites_have_unique_ids_and_names() {
        let b = body("Mars", BodyType::Planet);
        let sites = generate_landing_sites(&b);
        assert!(sites.len() >= MIN_SITES_PER_BODY);
        assert!(sites.len() <= MAX_SITES_PER_BODY);
        let mut ids: Vec<u32> = sites.iter().map(|s| s.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), sites.len(), "duplicate site ids");
        for s in &sites {
            assert!(s.name.starts_with("Mars "), "got {}", s.name);
            assert!(s.latitude >= -90.0 && s.latitude <= 90.0);
            assert!(s.longitude >= -180.0 && s.longitude <= 180.0);
        }
    }

    #[test]
    fn generated_extraction_sites_use_mining_feasible_set() {
        let b = body("Vesta", BodyType::Asteroid);
        let sites = generate_extraction_sites(&b);
        assert!(!sites.is_empty());
        for s in &sites {
            assert!(s.feasible_for.contains(&BuildingType::Mine));
            assert!(s.feasible_for.contains(&BuildingType::MassDriver));
            // No habitat buildings on an asteroid — they're not in
            // the feasible set, so they can't be in the blocker
            // set either.
            assert!(!s.feasible_for.contains(&BuildingType::HabitatDome));
        }
    }

    #[test]
    fn make_lat_lon_is_stable_for_same_inputs() {
        let (a_lat, a_lon) = make_lat_lon("Mars", 0, false);
        let (b_lat, b_lon) = make_lat_lon("Mars", 0, false);
        assert_eq!(a_lat, b_lat);
        assert_eq!(a_lon, b_lon);
        // Different index → different position.
        let (c_lat, _) = make_lat_lon("Mars", 1, false);
        assert_ne!(a_lat, c_lat);
    }

    #[test]
    fn make_scores_is_stable_for_same_inputs() {
        let a = make_scores("Mars", 0, false);
        let b = make_scores("Mars", 0, false);
        assert_eq!(a.slope, b.slope);
        assert_eq!(a.roughness, b.roughness);
        // Asteroid vs planet tag should produce a different score
        // vector (otherwise the per-site deterministic bias is
        // leaking across body kinds).
        let c = make_scores("Vesta", 0, true);
        let d = make_scores("Vesta", 0, true);
        assert_eq!(c.slope, d.slope);
        // Different tag at the same body+index should differ.
        let e = make_scores("Vesta", 0, false);
        assert_ne!(c.slope, e.slope);
    }
}
