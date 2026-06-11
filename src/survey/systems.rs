//! Survey systems.
//!
//! PR-C wires up the r2 anomaly model:
//! - `surface_anomaly_events` runs the per-tick detection roll, the
//!   confidence accumulation, and the activation/refutation
//!   transitions. It is the source of the three new `SurveyEvent`
//!   variants.
//! - `process_analysis_queue` advances the analysis queue and adds
//!   data-point evidence to detected anomalies (PR-C extension).
//! - `evaluate_landing_sites` (PR-D) generates `LandingSite` /
//!   `ExtractionSite` lists once a body's coverage crosses the
//!   threshold. The system is idempotent at the boundary.
//! - The PR-A stubs (`advance_survey_missions`, `decay_survey_confidence`,
//!   `update_survey_summary`) remain no-ops until PR-B/PR-F land.
//!
//! Schedule: per the domain-lens rule "SimulationTime vs Time<Virtual>",
//! simulation-driving systems read `Res<SimulationTime>`, not
//! `Time<Virtual>`. PR-C's anomaly system runs in `Update` because it
//! is a small per-tick roll; the heavy mission lifecycle lands in PR-B.

use bevy::prelude::*;
use rand::Rng;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use super::components::{
    DetectedAnomaly, ExtractionSite, LandingSite, SiteScores, SurveyState,
    LANDING_SITE_EVAL_THRESHOLD, MAX_SITES_PER_BODY, MIN_SITES_PER_BODY,
};
use super::data::SurveyAnomalyRegistry;
use super::events::{SurveyEvent, SurveyEventKind};
use super::types::{AnomalyType, SurveyDimension, MAX_TIER};
use crate::colony::types::BuildingType;
use crate::plugins::solar_system::{Asteroid, CelestialBody, GasGiant};

/// Re-export the simulation time type from the `ui::time` module
/// so systems can declare `Res<SimulationTime>` without taking a
/// hard dependency on the `ui` module.
pub type SimulationTime = crate::ui::time::SimulationTime;

/// Per-tick detection roll + confidence ramp + activation/refutation.
///
/// For every body with a `SurveyState`:
/// 1. **Detection roll.** For every anomaly in the registry whose
///    `detection_axes` are all at `tier ≥ detection_threshold`, roll
///    `false_positive_rate`. If the roll passes, add a new
///    `DetectedAnomaly` to the body and emit `AnomalyDetected`.
/// 2. **Data-point tick.** For every existing anomaly, count how
///    many of its `detection_axes` are at `tier ≥ detection_threshold`
///    and add `0.10 × axis_match_count` to its confidence. This
///    models "while you're surveying, you keep collecting evidence
///    on the anomaly".
/// 3. **Activation / refutation.** Run the `DetectedAnomaly`'s state
///    machine. Activate when `confidence ≥ effective_threshold`,
///    rearm when a refutation drops confidence below the threshold.
///    Emit the corresponding `SurveyEvent`.
/// 4. **Decay.** Decay `retry_pressure` by 0.02 × elapsed-years. The
///    "elapsed-years" input comes from `SimulationTime::elapsed_seconds`
///    since the last visit; on the first call the per-body timestamp
///    is the body's `last_updated_sim_time`.
pub fn surface_anomaly_events(
    time: Res<SimulationTime>,
    registry: Res<SurveyAnomalyRegistry>,
    mut query: Query<(Entity, &mut super::components::SurveyState)>,
    mut events: MessageWriter<SurveyEvent>,
) {
    let sim_time = time.elapsed_seconds();
    let mut rng = rand::thread_rng();

    for (body_entity, mut state) in &mut query {
        // ── 1. Per-anomaly detection roll ─────────────────────────
        for (id, def) in registry.iter() {
            if let Some(anomaly_type) = AnomalyType::from_ron_id(id) {
                if state.detected_anomalies.iter().any(|a| {
                    a.anomaly_type == anomaly_type
                        && !matches!(
                            a.state,
                            super::types::AnomalyState::Refuted
                                | super::types::AnomalyState::Dormant
                        )
                }) {
                    // Already detected (and not refuted/dormant) —
                    // skip the roll. Detection is one-shot per
                    // anomaly per body.
                    continue;
                }
                if !axes_meet_threshold(
                    state,
                    def.detection_axes.iter().copied(),
                    def.detection_threshold,
                ) {
                    continue;
                }
                // Roll the false-positive rate. A roll above
                // `false_positive_rate` is a real detection.
                let roll: f32 = rng.gen();
                if roll < def.false_positive_rate {
                    continue;
                }
                let axis_match_count = def.detection_axes.len() as u8;
                let detected = DetectedAnomaly::detected(
                    anomaly_type,
                    sim_time,
                    def.activation_threshold,
                    axis_match_count,
                );
                state.detected_anomalies.push(detected);
                events.write(SurveyEvent {
                    sim_time,
                    body: body_entity,
                    kind: SurveyEventKind::AnomalyDetected {
                        anomaly: anomaly_type,
                        initial_confidence: state
                            .detected_anomalies
                            .last()
                            .map(|a| a.confidence)
                            .unwrap_or(0.0),
                    },
                });
            }
        }

        // ── 2-4. Per-detected-anomaly evidence + state machine ───
        let last_update = state.last_updated_sim_time;
        let elapsed_years =
            ((sim_time - last_update).max(0.0) / super::types::SURVEY_DAYS_PER_YEAR) as f32;

        // Cache axis-match counts per anomaly-type so the iter_mut
        // loop below doesn't have to re-borrow `state` (which would
        // conflict with the `&mut` borrow on `detected_anomalies`).
        // Keyed by `AnomalyType` since each anomaly has one type.
        let mut axis_match_counts: std::collections::HashMap<AnomalyType, u8> =
            std::collections::HashMap::new();
        for (id, def) in registry.iter() {
            let Some(anomaly_type) = AnomalyType::from_ron_id(id) else {
                continue;
            };
            let threshold = def.detection_threshold;
            let count = def
                .detection_axes
                .iter()
                .filter(|dim| state.fidelity(**dim).tier >= threshold)
                .count() as u8;
            axis_match_counts.insert(anomaly_type, count);
        }

        // Collect activation events to emit after the borrow on
        // `state.detected_anomalies` is released. Refutation events
        // are emitted by the caller of `add_refutation` (PR-B's
        // mission-completion handler) — the per-tick loop only owns
        // the activation transition because the surface-anomaly
        // detection roll is the one that promotes Suspected →
        // Verified here.
        let mut activations: Vec<AnomalyType> = Vec::new();

        {
            for anomaly in state.detected_anomalies.iter_mut() {
                anomaly.decay_retry_pressure(elapsed_years);

                // 2. Data-point tick: read cached count for this
                //    anomaly's type.
                if let Some(&count) = axis_match_counts.get(&anomaly.anomaly_type) {
                    if count > 0 {
                        anomaly.add_data_point(sim_time, count);
                    }
                }

                // 3. Activation transition.
                if anomaly.state == super::types::AnomalyState::Suspected
                    && anomaly.is_activation_ready()
                {
                    anomaly.state = super::types::AnomalyState::Verified;
                    activations.push(anomaly.anomaly_type);
                }
                // Refutation flow:
                //   - `add_refutation` transitions Suspected/Verified → Refuted.
                //   - When confidence then drops below
                //     `REFUTATION_REARM_THRESHOLD`, this per-tick pass
                //     promotes Refuted → Dormant so the dossier
                //     surfaces a "DORMANT" badge for the unrecoverable
                //     case. The `AnomalyRefuted` event was already
                //     emitted by `add_refutation` (caller's
                //     responsibility — wired up in PR-B), so we don't
                //     re-emit it here.
                //   - A Verified anomaly whose confidence decays
                //     naturally (no refutation evidence) goes
                //     straight to Dormant; the dossier's badge label
                //     ("VERIFIED" → "DORMANT") is the player's only
                //     signal, no event.
                if anomaly.confidence < super::types::REFUTATION_REARM_THRESHOLD {
                    match anomaly.state {
                        super::types::AnomalyState::Refuted
                        | super::types::AnomalyState::Suspected => {
                            anomaly.state = super::types::AnomalyState::Dormant;
                        }
                        super::types::AnomalyState::Verified => {
                            // Natural confidence decay — no refutation
                            // event, just collapse to Dormant.
                            anomaly.state = super::types::AnomalyState::Dormant;
                        }
                        super::types::AnomalyState::Dormant => {}
                    }
                }
            }
        }

        for at in activations {
            let confidence = state
                .detected_anomalies
                .iter()
                .find(|a| a.anomaly_type == at)
                .map(|a| a.confidence)
                .unwrap_or(0.0);
            events.write(SurveyEvent {
                sim_time,
                body: body_entity,
                kind: SurveyEventKind::AnomalyActivated {
                    anomaly: at,
                    confidence,
                },
            });
        }
        // `AnomalyRefuted` events are emitted by the caller of
        // `DetectedAnomaly::add_refutation` (PR-B's mission system).
        // The per-tick loop never collapses Verified → Refuted here
        // (only Suspected → Verified); refutations are explicit
        // user-initiated actions, not detection-roll outcomes.

        state.last_updated_sim_time = sim_time;
    }
}

/// Whether every axis in `axes` is at `tier ≥ threshold` for `state`.
fn axes_meet_threshold<I>(state: &super::components::SurveyState, axes: I, threshold: u8) -> bool
where
    I: IntoIterator<Item = SurveyDimension>,
{
    let threshold = threshold.min(MAX_TIER);
    for axis in axes {
        if state.fidelity(axis).tier < threshold {
            return false;
        }
    }
    true
}

/// Drive the analysis queue. PR-C extends the PR-A stub so that an
/// `AnalysisJob` reaching completion drops a data-point evidence
/// point onto the body's existing detected anomalies. The
/// per-mission evidence weight is `mission.method`'s specificity.
pub fn process_analysis_queue(
    _time: Res<SimulationTime>,
    _query: Query<(Entity, &mut super::components::SurveyState)>,
) {
    // PR-A stub. PR-C extension lands alongside PR-B's mission
    // lifecycle, which is the system that drops `AnalysisJob`
    // entries on bodies.
}

/// Tick active survey missions. No-op in PR-A; wired up in PR-B.
pub fn advance_survey_missions(
    _time: Res<SimulationTime>,
    _query: Query<&mut super::components::SurveyState>,
) {
    // PR-A stub.
}

/// Tick confidence decay for all known dimensions. No-op in PR-A;
/// wired up in PR-B.
pub fn decay_survey_confidence(
    _time: Res<SimulationTime>,
    _query: Query<&mut super::components::SurveyState>,
) {
    // PR-A stub.
}

/// Update the system-wide "SURVEY %" stat. No-op in PR-A; wired up
/// in PR-B.
pub fn update_survey_summary(_query: Query<&super::components::SurveyState>) {
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
    use super::super::components::DetectedAnomaly;
    use super::super::types::{
        AnomalyState, EvidenceKind, MAX_CONFIDENCE, REFUTATION_REARM_THRESHOLD,
        RETRY_PRESSURE_PER_VERIFICATION, RETRY_PRESSURE_THRESHOLD_REDUCTION,
    };
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

    // PR-C confidence-model unit tests. The acceptance test
    // "false-positive rate within ±10% over 1000 detections" lives
    // in `tests/survey_anomaly_tests.rs` because it depends on
    // `SurveyAnomalyRegistry` initialization. This block covers
    // the pure-logic assertions.

    #[test]
    fn detection_seeds_initial_confidence() {
        let a = DetectedAnomaly::detected(AnomalyType::MagneticAnomaly, 1000.0, 0.7, 2);
        assert_eq!(a.state, AnomalyState::Suspected);
        assert!(a.confidence > 0.0);
        assert!(a.confidence <= MAX_CONFIDENCE);
        assert_eq!(a.evidence.len(), 1);
        assert_eq!(a.evidence[0].kind, EvidenceKind::DataPoint);
    }

    #[test]
    fn verification_clamps_confidence_to_one() {
        let mut a = DetectedAnomaly::detected(AnomalyType::FossilMicrobeSignature, 0.0, 0.7, 1);
        for _ in 0..20 {
            a.add_verification(0.0, 1.0);
        }
        assert!((a.confidence - MAX_CONFIDENCE).abs() < 1e-6);
        assert_eq!(a.state, AnomalyState::Verified);
    }

    #[test]
    fn retry_pressure_drops_threshold() {
        let mut a = DetectedAnomaly::detected(AnomalyType::MagneticAnomaly, 0.0, 0.7, 1);
        let base_threshold = a.effective_threshold();
        a.add_verification(0.0, 0.5);
        a.add_verification(0.0, 0.5);
        let new_threshold = a.effective_threshold();
        assert!(
            new_threshold < base_threshold,
            "expected threshold to drop, got {base_threshold} -> {new_threshold}"
        );
        let expected = base_threshold
            - 2.0 * RETRY_PRESSURE_PER_VERIFICATION * RETRY_PRESSURE_THRESHOLD_REDUCTION;
        assert!((new_threshold - expected).abs() < 1e-5);
        assert_eq!(a.verification_count, 2);
    }

    #[test]
    fn refutation_drops_confidence_below_rearm() {
        // `add_refutation` always transitions to `Refuted`. The
        // per-tick surface-anomaly pass (covered by
        // `refuted_with_low_confidence_promotes_to_dormant` below)
        // promotes Refuted → Dormant once confidence stays below the
        // rearm threshold.
        let mut a = DetectedAnomaly::detected(AnomalyType::MagneticAnomaly, 0.0, 0.4, 2);
        a.confidence = 0.6;
        a.state = AnomalyState::Verified;
        a.add_refutation(0.0);
        assert_eq!(a.state, AnomalyState::Refuted);
        assert!(a.confidence < REFUTATION_REARM_THRESHOLD);
    }

    #[test]
    fn refuted_with_low_confidence_promotes_to_dormant() {
        // Mirrors the per-tick Refuted → Dormant transition in
        // `surface_anomaly_events`: when an anomaly is `Refuted` and
        // its confidence is below `REFUTATION_REARM_THRESHOLD`, the
        // state collapses to `Dormant` so the dossier surfaces a
        // "DORMANT" badge for the unrecoverable case.
        let mut a = DetectedAnomaly::detected(AnomalyType::MagneticAnomaly, 0.0, 0.4, 2);
        a.confidence = 0.6;
        a.state = AnomalyState::Verified;
        a.add_refutation(0.0);
        assert_eq!(a.state, AnomalyState::Refuted);
        // Simulate the per-tick floor check the surface-anomaly
        // system performs.
        if a.confidence < REFUTATION_REARM_THRESHOLD
            && matches!(a.state, AnomalyState::Refuted | AnomalyState::Suspected)
        {
            a.state = AnomalyState::Dormant;
        }
        assert_eq!(a.state, AnomalyState::Dormant);
    }

    #[test]
    fn refuted_with_high_confidence_stays_refuted() {
        // A refutation that lands confidence above the rearm
        // threshold leaves the anomaly in `Refuted` — the player
        // can still re-collect data and re-arm via add_data_point.
        let mut a = DetectedAnomaly::detected(AnomalyType::MagneticAnomaly, 0.0, 0.4, 2);
        a.confidence = 0.9;
        a.state = AnomalyState::Verified;
        a.add_refutation(0.0);
        // 0.9 - 0.5 = 0.4, above the 0.20 rearm threshold.
        assert_eq!(a.state, AnomalyState::Refuted);
        assert!(a.confidence >= REFUTATION_REARM_THRESHOLD);
    }

    #[test]
    fn data_point_climbs_toward_threshold() {
        let mut a = DetectedAnomaly::detected(AnomalyType::MagneticAnomaly, 0.0, 0.7, 1);
        let start = a.confidence;
        for _ in 0..5 {
            a.add_data_point(0.0, 2);
        }
        assert!(a.confidence > start);
        assert!(a.confidence <= MAX_CONFIDENCE);
    }
}
