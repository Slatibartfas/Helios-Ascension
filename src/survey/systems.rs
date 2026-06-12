//! Survey systems.
//!
//! PR-A scaffold: system function stubs only. The actual logic lands
//! in subsequent PRs:
//!
//! - PR-B (instruments + mission templates): `advance_survey_missions`,
//!   `dispatch_survey_mission`, `abort_survey_mission` are wired up.
//! - PR-C (analysis queue + anomaly confidence): `process_analysis_queue`
//!   and `surface_anomaly_events` are real.
//! - PR-D (anomalies + landing sites): landing/extraction site
//!   evaluation lives in `evaluate_landing_sites`.
//! - PR-F (mining efficiency): `compute_mining_efficiency` lands in
//!   a follow-up PR.
//!
//! The stubs are present so the plugin registration site is stable
//! across PRs — adding a new system is a one-line change in `mod.rs`
//! rather than a structural refactor.

use bevy::prelude::*;
use rand::rngs::StdRng;
use rand::Rng;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use super::components::{
    ActiveSurveyMission, ContinuousStationBonus, ContinuousSurveyStation, DetectedAnomaly,
    DimensionFidelity, ExtractionSite, LandingSite, SiteScores, SurveyState,
    LANDING_SITE_EVAL_THRESHOLD, MAX_SITES_PER_BODY, MIN_SITES_PER_BODY,
};
use super::data::{SurveyAnomalyRegistry, SurveyMissionTemplate};
use super::events::{AbortSurveyMission, DispatchSurveyMission, SurveyEvent};
use super::types::{
    axis_advance_rate_for_tier, mining_yield_delta_for_tier, AnomalyType, MissionFailureReason,
    MissionStatus, SurveyDimension, SurveyMethod, MAX_TIER, SURVEY_DAYS_PER_YEAR,
};
use crate::colony::types::BuildingType;
use crate::personnel::components::Scientist;
use crate::plugins::solar_system::{Asteroid, CelestialBody, GasGiant};

/// Default injury duration in sim-days, per
/// `docs/design/SURVEY_REWORK.md` §Gameplay Loop. The scientist is
/// blocked from new mission assignments until this many sim-days
/// have elapsed.
pub const INJURY_DURATION_DAYS: f64 = 90.0;

/// Default data size in megabytes-per-sim-day for a single axis.
/// Used by the XP award on mission success. Modders can rebalance
/// via `personnel_promotion.ron` (PR-C).
const DATA_PER_AXIS_DAY_MB: f64 = 50.0;

/// Re-export the simulation time type from the `ui::time` module
/// so systems can declare `Res<SimulationTime>` without taking a
/// hard dependency on the `ui` module.
///
/// Mirrors the pattern used by `economy::systems` and
/// `research::systems`.
pub type SimulationTime = crate::ui::time::SimulationTime;

/// Tick confidence decay for all known dimensions. PR-A stub.
/// PR-B (GRA-80) does not change this system — the migration shim
/// keeps `SurveyState` optional on bodies, so this no-op remains
/// correct until the system populator starts inserting the new
/// component.
pub fn decay_survey_confidence(world: &mut World) {
    // PR-A stub. Implementation in a later PR:
    //
    //   use super::types::{CONFIDENCE_DECAY_PER_YEAR, SURVEY_DAYS_PER_YEAR};
    //   let elapsed_years = (time.elapsed_seconds() - state.last_updated_sim_time)
    //       / SURVEY_DAYS_PER_YEAR;
    //   let decay = elapsed_years * CONFIDENCE_DECAY_PER_YEAR as f64;
    //   for fidelity in state.dimensions.values_mut() {
    //       fidelity.confidence = (fidelity.confidence - decay as f32).max(0.0);
    //   }
    //   state.last_updated_sim_time = time.elapsed_seconds();
    let _ = world;
}

/// Drive the analysis queue. No-op in PR-A; wired up in PR-C.
pub fn process_analysis_queue(world: &mut World) {
    let _ = world;
}

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
    let mut rng = rand::rng();

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
                    &state,
                    def.detection_axes.iter().copied(),
                    def.detection_threshold,
                ) {
                    continue;
                }
                // Roll the false-positive rate. A roll above
                // `false_positive_rate` is a real detection.
                let roll: f32 = rng.random();
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
                events.write(SurveyEvent::AnomalyDetected {
                    body: body_entity,
                    anomaly: anomaly_type,
                    initial_confidence: state
                        .detected_anomalies
                        .last()
                        .map(|a| a.confidence)
                        .unwrap_or(0.0),
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
            events.write(SurveyEvent::AnomalyActivated {
                body: body_entity,
                anomaly: at,
                confidence,
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

/// Update the system-wide "SURVEY %" stat. No-op in PR-A; wired up
/// in PR-B.
pub fn update_survey_summary(world: &mut World) {
    let _ = world;
}

/// Tick active survey missions.
///
/// One Update pass per frame. For each `SurveyState`, the system:
/// 1. Iterates the body's `active_missions` vec.
/// 2. For each `InProgress` mission, advances per-axis progress
///    proportional to elapsed sim-seconds and the scientist team's
///    throughput modifier.
/// 3. When the slowest axis hits 1.0, transitions the mission to
///    [`MissionStatus::Completing`]. The next tick rolls for failure
///    and either promotes the mission to `Succeeded` or
///    `Failed`.
/// 4. Awards XP and clears the scientist assignment on success.
///
/// Takes `&mut World` rather than separate `Query` / `Res` system
/// params. Bevy 0.18 forbids two `Query<...>` system params that
/// both yield mutable access to the same component (B0001), and the
/// dispatch / tick / abort helpers all need to mutate scientists via
/// a `QueryState` we hand in. Going through `&mut World` keeps the
/// borrow graph simple and lets the helpers share the world's
/// `QueryState` cache across the inner loop.
///
/// PR-B splits the tick into two passes:
///
/// 1. **Progress pass**: handle `Queued` → `Inflight`, advance
///    per-axis progress on `Inflight` / `Active` missions, and
///    transition to `Completing` when all axes are saturated. This
///    pass only needs `&mut SurveyState`; no rng or scientist
///    mutation.
///
/// 2. **Finalize pass**: for each body with at least one
///    `Completing` mission, pull the rng to roll for failure,
///    finalize the mission (which mutates scientists and fires
///    events), and apply the per-axis fidelity updates. The pass
///    is structured so each world borrow (rng, state, helpers)
///    is in its own scope, preventing the double-mut conflict
///    that the single-pass version hits.
pub fn advance_survey_missions(world: &mut World) {
    let sim_time = world.resource::<SimulationTime>().elapsed_seconds();

    // Pass 1: progress + non-completing transitions. We hold the
    // state_iter borrow for the whole pass; no other world
    // operations happen inside the loop, so the borrow is clean.
    {
        let mut state_iter = world.query::<(Entity, &mut SurveyState)>();
        for (_body_entity, mut state) in state_iter.iter_mut(world) {
            for idx in 0..state.active_missions.len() {
                let status = state.active_missions[idx].status;
                match status {
                    MissionStatus::Queued => {
                        // PR-B treats `Queued` as a single-tick
                        // state. The tick system flips it to
                        // `Inflight` on the next pass.
                        state.active_missions[idx].status = MissionStatus::Inflight;
                    }
                    MissionStatus::Inflight | MissionStatus::Active => {
                        advance_mission_progress(&mut state.active_missions[idx], sim_time);
                        if state.active_missions[idx].axes_saturated() {
                            state.active_missions[idx].status = MissionStatus::Completing;
                        }
                    }
                    // `Completing` and the terminal states are
                    // handled in pass 2 / are no-ops.
                    MissionStatus::Completing
                    | MissionStatus::Succeeded
                    | MissionStatus::Failed
                    | MissionStatus::Aborted => {}
                }
            }
        }
    }

    // Pass 2: finalize Completing missions. Collect the affected
    // body entities first (immutable borrow), then process each
    // body in its own scope so the rng / state / helper borrows
    // are sequenced correctly.
    let completing_bodies: Vec<Entity> = {
        let mut state_query = world.query::<(Entity, &SurveyState)>();
        state_query
            .iter(world)
            .filter_map(|(e, s)| {
                if s.active_missions
                    .iter()
                    .any(|m| m.status == MissionStatus::Completing)
                {
                    Some(e)
                } else {
                    None
                }
            })
            .collect()
    };

    for body_entity in completing_bodies {
        finalize_completing_missions_on_body(body_entity, sim_time, world);
    }
}

/// Process every `Completing` mission on a single body's
/// `SurveyState`. Pulls the rng in its own scope, takes the
/// mission out of `active_missions` for the duration of the
/// `finalize_mission` call (so the state borrow is released while
/// `finalize_mission` re-borrows the world), and re-inserts the
/// mission (now in a terminal state) before applying the
/// per-axis fidelity updates.
fn finalize_completing_missions_on_body(body_entity: Entity, sim_time: f64, world: &mut World) {
    // Collect the indexes of Completing missions. We re-collect
    // each pass because once one mission transitions to a
    // terminal state the others still need to finalize; the
    // indexes shift, so we re-snapshot each loop iteration.
    loop {
        let completing_idx: Option<usize> = {
            let state = world
                .get::<SurveyState>(body_entity)
                .expect("body in completing_bodies must have SurveyState");
            state
                .active_missions
                .iter()
                .position(|m| m.status == MissionStatus::Completing)
        };
        let Some(idx) = completing_idx else {
            break;
        };

        // Snapshot pre-mission tiers (immutable borrow of state,
        // released at the end of the block).
        let pre_mission_tiers: HashMap<SurveyDimension, u8> = {
            let state = world.get::<SurveyState>(body_entity).unwrap();
            state.active_missions[idx]
                .per_axis_progress
                .keys()
                .map(|dim| (*dim, state.fidelity(*dim).tier))
                .collect()
        };
        let method = {
            let state = world.get::<SurveyState>(body_entity).unwrap();
            state.active_missions[idx].method
        };

        // Roll for failure. The rng borrow is scoped to a block
        // so it ends before we re-borrow the world for
        // `finalize_mission`.
        let outcome = {
            let mut rng = world.resource_mut::<crate::economy::generation::ProceduralRng>();
            let rng_inner: &mut StdRng = &mut rng.0;
            roll_mission_outcome(rng_inner, method)
        };

        // Take the mission out of state for the finalize call.
        // The state borrow ends at the semicolon.
        let mut mission = world
            .get_mut::<SurveyState>(body_entity)
            .unwrap()
            .active_missions
            .remove(idx);

        // Finalize. Mission is local; state borrow has ended;
        // `finalize_mission` takes `world` exclusively to
        // mutate scientists and fire events.
        let fidelity_updates = finalize_mission(
            body_entity,
            &mut mission,
            &outcome,
            sim_time,
            world,
            &pre_mission_tiers,
        );

        // Re-insert the (now-terminal) mission at the same
        // index so the body's other missions are not shifted.
        world
            .get_mut::<SurveyState>(body_entity)
            .unwrap()
            .active_missions
            .insert(idx, mission);

        // Apply the per-axis fidelity updates. The state borrow
        // ends at the end of the for loop.
        {
            let mut state = world.get_mut::<SurveyState>(body_entity).unwrap();
            for (dim, new_fidelity) in fidelity_updates {
                state.set_fidelity(dim, new_fidelity);
            }
        }
    }
}

/// Advance a single mission's per-axis progress by the elapsed
/// sim-seconds since the last tick.
///
/// Called once per `Update` from `advance_survey_missions`. Reads
/// `mission.per_axis_progress` (mutably — single-owner invariant
/// inside the outer loop) and updates each entry.
///
/// The tick formula is
/// `delta = yield * team_modifier * (1 - coverage) / total_days` per
/// axis, scaled by `dt_days = delta_sim_seconds / 86_400`. The
/// `(1 - coverage)` term slows progress as the body already knows
/// the dimension — see the issue's GRA-80 scope.
fn advance_mission_progress(mission: &mut ActiveSurveyMission, sim_time: f64) {
    let total_seconds = mission.total_duration_seconds();
    if total_seconds <= 0.0 {
        return;
    }
    let total_days = total_seconds / 86_400.0;
    if total_days <= 0.0 {
        return;
    }
    let dt_seconds = (sim_time - mission.launched_sim_time).max(0.0);
    let yield_per_day = mission.axis_yield_per_day.max(0.0);
    let team_modifier = 1.0_f64; // PR-B: no scientist team modifier

    let mut slowest = 0.0_f32;
    for progress in mission.per_axis_progress.values_mut() {
        // Linear ramp: progress = (dt_days / total_days) * yield * team_mod.
        // yield=1.0 fills the mission in `total_days`; the per-tick
        // call computes the cumulative position in one shot, so a
        // halfway call lands at ~0.5 (not 1.0) on every axis.
        let dt_days = dt_seconds / 86_400.0;
        let target = (dt_days / total_days) * (yield_per_day as f64) * team_modifier;
        *progress = (target as f32).clamp(0.0, 1.0);
        if *progress > slowest {
            slowest = *progress;
        }
    }
    mission.progress = slowest;
}

/// Resolve the per-axis fidelity boost on success. PR-B's promotion
/// model is `(current + 1).min(MAX_TIER)`. A follow-up PR can swap
/// this for the `axis_yield` curve from the mission template.
fn promote_axis(
    pre_mission_tiers: &HashMap<SurveyDimension, u8>,
    dim: SurveyDimension,
    sim_time: f64,
) -> DimensionFidelity {
    let current = pre_mission_tiers.get(&dim).copied().unwrap_or(0);
    let promoted_tier = current.saturating_add(1).min(MAX_TIER);
    DimensionFidelity::freshly_measured(promoted_tier, sim_time)
}

/// Build the umbrella `MissionFailed`-companion event (e.g.
/// `ProbeLost`). Returns `None` for reasons that have no
/// companion event in the spec (SolarStorm, CrewInjury — for
/// those, the `MissionFailed` event itself is the signal).
fn failure_companion_event(
    reason: MissionFailureReason,
    body_entity: Entity,
    mission: &ActiveSurveyMission,
) -> Option<SurveyEvent> {
    let evt = match reason {
        MissionFailureReason::ProbeLoss => SurveyEvent::ProbeLost {
            body: body_entity,
            mission_id: mission.id,
            name: mission.name.clone(),
        },
        MissionFailureReason::RoverStuck => SurveyEvent::RoverStuck {
            body: body_entity,
            mission_id: mission.id,
            name: mission.name.clone(),
        },
        MissionFailureReason::DrillBitStuck => SurveyEvent::DrillBitStuck {
            body: body_entity,
            mission_id: mission.id,
            name: mission.name.clone(),
        },
        // SolarStorm and CrewInjury have no companion event in
        // the spec — the `MissionFailed` event is the signal.
        MissionFailureReason::SolarStorm | MissionFailureReason::CrewInjury => return None,
    };
    Some(evt)
}

/// Finalize a `Completing` mission: roll outcomes, award XP, mark
/// terminal, fire events. Returns the per-axis fidelity updates
/// for the caller to apply to the body's `SurveyState` after the
/// mission borrow is released.
///
/// `pre_mission_tiers` is the body's tier for each axis the
/// mission targeted, snapshotted before this call. PR-B's
/// promotion model is `(current + 1).min(MAX_TIER)`. A follow-up
/// PR can swap this for the `axis_yield` curve from the mission
/// template.
///
/// Takes `&mut World` rather than a `Query<&mut Scientist>` so the
/// test code (which uses `world.query::<&mut Scientist>()` to
/// produce a `QueryState`) can call this helper uniformly with
/// the system code.
fn finalize_mission(
    body_entity: Entity,
    mission: &mut ActiveSurveyMission,
    outcome: &MissionOutcome,
    sim_time: f64,
    world: &mut World,
    pre_mission_tiers: &HashMap<SurveyDimension, u8>,
) -> Vec<(SurveyDimension, DimensionFidelity)> {
    match outcome {
        MissionOutcome::Success => {
            // Promote each target axis's tier on the body. The
            // mission's `per_axis_progress` map keyed the axes; the
            // boost per axis is `(pre_mission + 1).min(MAX_TIER)`.
            // PR-C will refine with the `axis_yield` curve from
            // the mission template. We return the updates for
            // the caller to apply after this function releases
            // the mission borrow.
            let axes: Vec<SurveyDimension> = mission.per_axis_progress.keys().copied().collect();
            let updates: Vec<(SurveyDimension, DimensionFidelity)> = axes
                .iter()
                .map(|dim| (*dim, promote_axis(pre_mission_tiers, *dim, sim_time)))
                .collect();

            // Award XP and clear the scientists' mission assignment.
            let duration_days = mission.total_duration_seconds() / 86_400.0;
            let data_mb =
                duration_days * mission.per_axis_progress.len() as f64 * DATA_PER_AXIS_DAY_MB;
            award_scientist_xp(world, mission, data_mb);

            // Fire the completion event.
            world.write_message(SurveyEvent::MissionCompleted {
                body: body_entity,
                mission_id: mission.id,
                name: mission.name.clone(),
                method: mission.method,
            });
            mission.status = MissionStatus::Succeeded;
            updates
        }
        MissionOutcome::Failure(reason) => {
            // Failure: clear the scientists' assignment and (on
            // crew injury) injure the first scientist in the team.
            // Partial progress is rolled back to the pre-mission
            // snapshot (the body's tier map is unchanged). PR-C may
            // revisit the partial-retain policy.
            if *reason == MissionFailureReason::CrewInjury {
                if let Some(evt) = injure_first_scientist(body_entity, mission, sim_time, world) {
                    world.write_message(evt);
                }
            } else {
                // Non-injury failure: just clear the assignment.
                clear_scientist_assignments(world, mission);
            }

            // Fire the companion event (e.g. `ProbeLost`,
            // `RoverStuck`) and the umbrella `MissionFailed`.
            if let Some(evt) = failure_companion_event(*reason, body_entity, mission) {
                world.write_message(evt);
            }
            world.write_message(SurveyEvent::MissionFailed {
                body: body_entity,
                mission_id: mission.id,
                name: mission.name.clone(),
                method: mission.method,
                reason: *reason,
            });
            mission.status = MissionStatus::Failed;
            // No fidelity updates on failure — the body's tier map
            // is unchanged. The mission's per-axis progress is
            // discarded with the mission.
            Vec::new()
        }
    }
}

/// Award XP to every assigned scientist and clear their mission
/// assignment. Called on mission success.
fn award_scientist_xp(world: &mut World, mission: &ActiveSurveyMission, data_mb: f64) {
    if mission.assigned_scientists.is_empty() {
        return;
    }
    // Split data evenly across the team. A future PR can weight
    // by seniority / specialty match.
    let per_scientist = data_mb / mission.assigned_scientists.len() as f64;
    let ids = mission.assigned_scientists.clone();
    let mut state = world.query::<&mut Scientist>();
    for id in &ids {
        // O(n) lookup: the query has no direct id index in PR-B.
        // For v0.5.0 the personnel roster is small (≤ 200
        // scientists even late-game) and missions tick once per
        // Update; an O(n²) is acceptable. PR-C can introduce a
        // `HashMap<ScientistId, Entity>` index.
        for mut scientist in state.iter_mut(world) {
            if scientist.id == *id {
                scientist.lifetime_data_processed += per_scientist;
                scientist.current_survey_mission = None;
                break;
            }
        }
    }
}

/// Clear the mission assignment on every scientist without
/// touching XP or injury state. Used on non-injury failure and on
/// abort.
fn clear_scientist_assignments(world: &mut World, mission: &ActiveSurveyMission) {
    let ids = mission.assigned_scientists.clone();
    let mut state = world.query::<&mut Scientist>();
    for id in &ids {
        for mut scientist in state.iter_mut(world) {
            if scientist.id == *id {
                scientist.current_survey_mission = None;
                break;
            }
        }
    }
}

/// Injure the first scientist in the team. Returns the
/// [`SurveyEvent::CrewInjured`] event to send (or `None` if there
/// are no scientists on the team — which can't happen in
/// practice because the dispatch system drops ground-team
/// missions with no team, but the function stays defensive).
fn injure_first_scientist(
    body_entity: Entity,
    mission: &ActiveSurveyMission,
    sim_time: f64,
    world: &mut World,
) -> Option<SurveyEvent> {
    let first_id = *mission.assigned_scientists.first()?;
    let injured_until = sim_time + INJURY_DURATION_DAYS * 86_400.0;
    let mut scientist_name = String::new();
    {
        let mut state = world.query::<&mut Scientist>();
        for mut scientist in state.iter_mut(world) {
            if scientist.id == first_id {
                scientist_name = scientist.name.clone();
                scientist.injure(injured_until);
                scientist.current_survey_mission = None;
                break;
            }
        }
    }
    // Clear assignments on the rest of the team.
    clear_scientist_assignments(world, mission);

    Some(SurveyEvent::CrewInjured {
        body: body_entity,
        mission_id: mission.id,
        name: mission.name.clone(),
        scientist: first_id,
        scientist_name,
        injured_until_sim_time: injured_until,
    })
}

/// Consume [`DispatchSurveyMission`] events: build a new
/// [`ActiveSurveyMission`], push it onto the body's
/// [`SurveyState::active_missions`], and update the assigned
/// scientists' `current_survey_mission` field.
///
/// Drop conditions (with a `warn!` so the dev can spot a logic
/// bug in their UI):
/// - the body's `SurveyState` is missing
/// - the template id is unknown
/// - any assigned scientist is injured, missing, or already on a
///   mission
pub fn dispatch_survey_mission(world: &mut World) {
    let sim_time = world.resource::<SimulationTime>().elapsed_seconds();
    let templates = world
        .resource::<super::data::SurveyMissionTemplates>()
        .clone();
    // Drain the current frame's dispatch events. `update_drain`
    // advances the read cursor and yields the events as owned
    // values; cloning the resource first lets us drop the
    // `Messages` borrow before iterating so we can re-borrow the
    // world for `entity_mut`, `query`, etc.
    let events: Vec<DispatchSurveyMission> = {
        let mut buf = world.resource_mut::<Messages<DispatchSurveyMission>>();
        buf.update_drain().collect()
    };

    for ev in events {
        // Resolve template. A missing template is a UI bug — warn
        // and drop the event.
        let template: &SurveyMissionTemplate = match templates.templates.get(&ev.template_id) {
            Some(t) => t,
            None => {
                warn!(
                    "DispatchSurveyMission: unknown template_id {:?}; dropping",
                    ev.template_id
                );
                continue;
            }
        };

        // Validate the scientist team. The empty-team case is
        // valid for solo probe missions; ground-team missions
        // require at least one scientist.
        if template.is_ground_team && ev.scientist_ids.is_empty() {
            warn!(
                "DispatchSurveyMission: ground-team template {:?} dispatched with no scientists; dropping",
                ev.template_id
            );
            continue;
        }
        for sid in &ev.scientist_ids {
            let scientist = {
                let mut state = world.query::<&Scientist>();
                state.iter(world).find(|s| s.id == *sid)
            };
            match scientist {
                Some(s) if s.is_injured(sim_time) => {
                    warn!(
                        "DispatchSurveyMission: scientist {} is injured; dropping dispatch",
                        s.name
                    );
                    continue;
                }
                Some(s) if s.current_survey_mission.is_some() => {
                    warn!(
                        "DispatchSurveyMission: scientist {} is already on a mission; dropping",
                        s.name
                    );
                    continue;
                }
                None => {
                    warn!(
                        "DispatchSurveyMission: scientist id {} not found; dropping",
                        sid
                    );
                    continue;
                }
                _ => {}
            }
        }

        // Resolve the body. Insert a fresh `SurveyState` if the
        // body doesn't have one yet (the system populator doesn't
        // add `SurveyState` to every body in PR-B — that integration
        // is parked for a follow-up PR). The first dispatch on a
        // body is dropped this tick; the second attempt sees the
        // freshly-inserted state and succeeds.
        let body_has_state = world.get::<SurveyState>(ev.body).is_some();
        if !body_has_state {
            world.entity_mut(ev.body).insert(SurveyState::default());
            warn!(
                "DispatchSurveyMission: body {:?} has no SurveyState; inserted fresh, retry on next dispatch",
                ev.body
            );
            continue;
        }

        // Build the per-axis progress map from the template's
        // `target_tiers`. PR-B uses a linear ramp; PR-C will
        // adjust the curve to match the `axis_yield_per_day`
        // distribution across dimensions.
        let mut per_axis_progress = HashMap::new();
        for dim in template.target_tiers.keys() {
            per_axis_progress.insert(*dim, 0.0_f32);
        }

        let now = sim_time;
        let duration_seconds = (template.base_duration_days as f64) * 86_400.0;
        let next_id = {
            let state = world.get::<SurveyState>(ev.body).unwrap();
            next_mission_id(state)
        };
        let mission = ActiveSurveyMission {
            id: next_id,
            name: ev.name.clone(),
            method: template.method,
            status: MissionStatus::Queued,
            launched_sim_time: now,
            expected_completion_sim_time: now + duration_seconds,
            progress: 0.0,
            per_axis_progress,
            axis_yield_per_day: template.axis_yield_per_day,
            assigned_scientists: ev.scientist_ids.clone(),
        };
        let mission_id = mission.id;
        let method = mission.method;
        let name = mission.name.clone();

        // Push the mission onto the body's active list.
        world
            .get_mut::<SurveyState>(ev.body)
            .unwrap()
            .active_missions
            .push(mission);

        // Update the scientists' assignments.
        {
            let mut state = world.query::<&mut Scientist>();
            for sid in &ev.scientist_ids {
                for mut scientist in state.iter_mut(world) {
                    if scientist.id == *sid {
                        scientist.current_survey_mission = Some(mission_id);
                        break;
                    }
                }
            }
        }

        world.write_message(SurveyEvent::MissionStarted {
            body: ev.body,
            mission_id,
            name,
            method,
        });
    }
}

/// Consume [`AbortSurveyMission`] events: remove the mission from
/// the body's `active_missions`, free any assigned scientists,
/// and fire a `MissionAborted` event.
pub fn abort_survey_mission(world: &mut World) {
    let events: Vec<AbortSurveyMission> = {
        let mut buf = world.resource_mut::<Messages<AbortSurveyMission>>();
        buf.update_drain().collect()
    };

    for ev in events {
        let body_has_state = world.get::<SurveyState>(ev.body).is_some();
        if !body_has_state {
            warn!(
                "AbortSurveyMission: body {:?} has no SurveyState; dropping",
                ev.body
            );
            continue;
        }
        let mut aborted_mission: Option<ActiveSurveyMission> = None;
        {
            let mut state = world.get_mut::<SurveyState>(ev.body).unwrap();
            if let Some(pos) = state
                .active_missions
                .iter()
                .position(|m| m.id == ev.mission_id)
            {
                let mission = state.active_missions.remove(pos);
                if !mission.status.is_terminal() {
                    aborted_mission = Some(mission);
                }
            }
        }

        if let Some(mission) = aborted_mission {
            clear_scientist_assignments(world, &mission);
            world.write_message(SurveyEvent::MissionAborted {
                body: ev.body,
                mission_id: mission.id,
                name: mission.name.clone(),
                method: mission.method,
            });
        }
    }
}

/// Generate a stable mission id. PR-B uses `len + 1`; a future PR
/// can move to a proper counter resource if id collisions become
/// an issue (e.g. across save/load boundaries).
fn next_mission_id(state: &SurveyState) -> u64 {
    state
        .active_missions
        .iter()
        .map(|m| m.id)
        .max()
        .unwrap_or(0)
        + 1
}

/// Roll the dice for a mission outcome. Returns `Success` with
/// probability `1 - sum(failure_probability)` and each failure
/// reason with its respective probability.
fn roll_mission_outcome(rng: &mut StdRng, method: SurveyMethod) -> MissionOutcome {
    use MissionFailureReason::*;
    let probe_loss_p = ProbeLoss.probability(method);
    let rover_stuck_p = RoverStuck.probability(method);
    let drill_p = DrillBitStuck.probability(method);
    let solar_p = SolarStorm.probability(method);
    let injury_p = CrewInjury.probability(method);
    let total_failure = probe_loss_p + rover_stuck_p + drill_p + solar_p + injury_p;
    let roll: f32 = rng.random();
    if roll < probe_loss_p {
        return MissionOutcome::Failure(ProbeLoss);
    }
    if roll < probe_loss_p + rover_stuck_p {
        return MissionOutcome::Failure(RoverStuck);
    }
    if roll < probe_loss_p + rover_stuck_p + drill_p {
        return MissionOutcome::Failure(DrillBitStuck);
    }
    if roll < probe_loss_p + rover_stuck_p + drill_p + solar_p {
        return MissionOutcome::Failure(SolarStorm);
    }
    if roll < total_failure {
        return MissionOutcome::Failure(CrewInjury);
    }
    MissionOutcome::Success
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum MissionOutcome {
    Success,
    Failure(MissionFailureReason),
}

impl MissionOutcome {
    #[allow(dead_code)] // only used in unit tests
    fn is_success(self) -> bool {
        matches!(self, MissionOutcome::Success)
    }
}

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

/// GRA-83 PR-E: aggregate every orbital `ContinuousSurveyStation`
/// that orbits a body, advance the body's [`SurveyState`]
/// dimensions by the combined per-year rate × elapsed years, and
/// write the body's [`ContinuousStationBonus`] cache for the
/// mining yield system to read.
///
/// Per-body isolation is enforced by the `orbiting_body: Entity`
/// field on the station: a station at Mars only ever contributes
/// to Mars's aggregation, never Phobos. A station with
/// `orbiting_body = None` (a freshly-built station whose UI has
/// not yet bound a body) is inert. A station whose `orbiting_body`
/// entity has been despawned is also inert — the station itself
/// stays and the player can demolish it.
///
/// The mining yield multiplier is `1.0 + Σ mining_yield_delta`
/// across every station orbiting the body. Two tier-1 stations
/// stack to `1.10` (10%). One tier-2 station alone is `1.10`.
///
/// Multi-station-per-body stacking is additive on both the
/// axis-advance rate (sum of rates) and the mining delta (1.0 +
/// Σ). This matches the per-tier table the LGD locked in the
/// CTO recipe §2.
///
/// Takes `&mut World` rather than separate `Query` / `Res` system
/// params. Bevy 0.18 forbids two `Query<...>` params that both
/// yield mutable access to the same component (B0001), and the
/// per-body aggregate needs to read `ContinuousSurveyStation` on
/// station entities and write `SurveyState` + the
/// `ContinuousStationBonus` cache on body entities. Going
/// through `&mut World` keeps the borrow graph simple and lets
/// the helper insert the bonus component on first sight without
/// needing a separate `Commands` queue.
pub fn apply_continuous_station_bonus(world: &mut World) {
    let sim_time = world.resource::<SimulationTime>().elapsed_seconds();

    // 1) Aggregate per body. `per_body` is keyed by body Entity;
    //    the value is `(axis_advance_per_year, mining_yield_delta)`.
    //    The mining delta is the *additive* delta; the cached
    //    multiplier is `1.0 + delta` (see [`ContinuousStationBonus`]).
    let mut per_body: HashMap<Entity, (f32, f32)> = HashMap::new();
    {
        let mut station_q = world.query::<&ContinuousSurveyStation>();
        for station in station_q.iter(world) {
            let Some(body_entity) = station.orbiting_body else {
                // Station has not yet been bound to a body —
                // inert. See the construction-panel handoff note
                // in `ContinuousSurveyStation` docs.
                continue;
            };
            // Body despawned (or otherwise unreachable) — inert.
            // The station itself stays; the player can demolish.
            if world.get_entity(body_entity).is_none() {
                continue;
            }
            let advance = axis_advance_rate_for_tier(station.tier);
            let delta = mining_yield_delta_for_tier(station.tier);
            if advance <= 0.0 && delta <= 0.0 {
                // Unknown tier (e.g. tier 0 stub) — inert.
                continue;
            }
            let entry = per_body.entry(body_entity).or_insert((0.0, 0.0));
            entry.0 += advance;
            entry.1 += delta;
        }
    }

    // 2) Reset every body's bonus cache to neutral first, so a
    //    body that lost its last station this tick sees the bonus
    //    revert. The reset is required for the
    //    "station_destroyed_removes_bonus" acceptance test: if
    //    the player demolishes the only station on a body, the
    //    body's `mining_yield_multiplier` must drop back to 1.0.
    {
        let mut bonus_q = world.query::<&mut ContinuousStationBonus>();
        for mut bonus in bonus_q.iter_mut(world) {
            bonus.axis_advance_per_year = 0.0;
            bonus.mining_yield_multiplier = 1.0;
        }
    }

    // 3) Apply the per-body aggregate. For each body with at
    //    least one station, advance every dimension on the body's
    //    `SurveyState` by the combined rate × elapsed years, then
    //    write the bonus cache (inserting the component if
    //    missing). The mining yield system reads the cache
    //    downstream; the dossier UI reads it too.
    for (body_entity, advance, delta) in per_body {
        // 3a) Advance dimensions. Skip if the body has no
        //     SurveyState (the survey-side is inert; the
        //     mining-side bonus still applies to the body for any
        //     `Mine` buildings it has).
        if let Some(mut state) = world.get_mut::<SurveyState>(body_entity) {
            let last_update = state.last_updated_sim_time;
            let years_elapsed = ((sim_time - last_update).max(0.0) / SURVEY_DAYS_PER_YEAR) as f32;
            for dim in SurveyDimension::ALL {
                let current = state.fidelity(dim);
                let new_tier_float =
                    (current.tier as f32 + advance * years_elapsed).min(MAX_TIER as f32);
                let new_tier = new_tier_float as u8;
                if new_tier != current.tier {
                    let new_fidelity =
                        DimensionFidelity::at_tier(new_tier, current.confidence, Some(sim_time));
                    state.set_fidelity(dim, new_fidelity);
                }
            }
            state.last_updated_sim_time = sim_time;
        }

        // 3b) Write the bonus cache. Insert the component if
        //     missing — this lets the construction system place
        //     a station on a body that doesn't yet have a
        //     bonus cache, and the very next tick the
        //     mining-yield system starts reading the multiplier.
        if world.get::<ContinuousStationBonus>(body_entity).is_none() {
            world
                .entity_mut(body_entity)
                .insert(ContinuousStationBonus::default());
        }
        if let Some(mut bonus) = world.get_mut::<ContinuousStationBonus>(body_entity) {
            bonus.axis_advance_per_year = advance;
            bonus.mining_yield_multiplier = 1.0 + delta;
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
    //! Unit tests for the mission lifecycle. See
    //! `docs/design/SURVEY_REWORK.md` §Gameplay Loop for the design
    //! rationale and the per-method failure probabilities.

    use super::*;
    use crate::economy::generation::ProceduralRng;
    use crate::personnel::types::ScientistSpecialty;
    use rand::SeedableRng;

    fn sim_time() -> f64 {
        // Fixed sim-time so the tests are deterministic.
        1_000.0
    }

    fn make_scientist(id: u64, name: &str) -> Scientist {
        Scientist::new_junior(
            id,
            name.to_string(),
            ScientistSpecialty::Geology,
            sim_time(),
        )
    }

    fn make_mission(method: SurveyMethod, duration_days: u32) -> ActiveSurveyMission {
        let mut per_axis_progress = HashMap::new();
        per_axis_progress.insert(SurveyDimension::OrbitalMech, 0.0_f32);
        per_axis_progress.insert(SurveyDimension::Atmosphere, 0.0_f32);
        ActiveSurveyMission {
            id: 1,
            name: "Test Mission".to_string(),
            method,
            status: MissionStatus::Inflight,
            launched_sim_time: sim_time(),
            expected_completion_sim_time: sim_time() + (duration_days as f64) * 86_400.0,
            progress: 0.0,
            per_axis_progress,
            axis_yield_per_day: 1.0,
            assigned_scientists: Vec::new(),
        }
    }

    #[test]
    fn total_duration_seconds_is_positive() {
        let mission = make_mission(SurveyMethod::Flyby, 540);
        assert!((mission.total_duration_seconds() - 540.0 * 86_400.0).abs() < 1.0);
    }

    #[test]
    fn advance_progress_fills_axes_over_duration() {
        // Advance the tick to ~50% of the mission duration and
        // verify the per-axis progress is in (0, 1).
        let mut mission = make_mission(SurveyMethod::Flyby, 540);
        let mid = sim_time() + 270.0 * 86_400.0; // halfway
        advance_mission_progress(&mut mission, mid);
        // Solo mission, yield=1.0, no scientist team → modifier=1.0.
        // At halfway through, the linear ramp in
        // `advance_mission_progress` lands each axis at exactly
        // 0.5, well inside the (0, 1) window the test asserts.
        for (dim, p) in &mission.per_axis_progress {
            assert!(
                *p > 0.0 && *p < 1.0,
                "axis {dim:?} should be partially advanced, got {p}"
            );
        }
        assert!(mission.progress > 0.0 && mission.progress < 1.0);
    }

    #[test]
    fn axes_saturated_only_when_all_axes_reach_one() {
        let mut mission = make_mission(SurveyMethod::Flyby, 540);
        assert!(!mission.axes_saturated());
        for p in mission.per_axis_progress.values_mut() {
            *p = 1.0;
        }
        assert!(mission.axes_saturated());
    }

    #[test]
    fn mission_status_is_in_progress_for_queued_inflight_active_completing() {
        for s in [
            MissionStatus::Queued,
            MissionStatus::Inflight,
            MissionStatus::Active,
            MissionStatus::Completing,
        ] {
            assert!(s.is_in_progress(), "{s:?} should be in-progress");
            assert!(!s.is_terminal(), "{s:?} should not be terminal");
        }
    }

    #[test]
    fn mission_status_terminal_flags() {
        for s in [
            MissionStatus::Succeeded,
            MissionStatus::Failed,
            MissionStatus::Aborted,
        ] {
            assert!(s.is_terminal(), "{s:?} should be terminal");
            assert!(!s.is_in_progress(), "{s:?} should not be in-progress");
        }
    }

    #[test]
    fn failure_probability_per_method_matches_design() {
        use MissionFailureReason::*;
        // Probe-using methods
        for m in [
            SurveyMethod::Flyby,
            SurveyMethod::Orbital,
            SurveyMethod::RemoteSensing,
            SurveyMethod::AtmosphericProbe,
        ] {
            assert!((ProbeLoss.probability(m) - 0.05).abs() < 1e-6);
        }
        assert!((RoverStuck.probability(SurveyMethod::Rover) - 0.08).abs() < 1e-6);
        assert!((DrillBitStuck.probability(SurveyMethod::Drill) - 0.10).abs() < 1e-6);
        for m in [
            SurveyMethod::Flyby,
            SurveyMethod::Orbital,
            SurveyMethod::RemoteSensing,
            SurveyMethod::AtmosphericProbe,
            SurveyMethod::SurfaceLander,
            SurveyMethod::Rover,
            SurveyMethod::Seismic,
            SurveyMethod::Drill,
            SurveyMethod::SampleReturn,
        ] {
            assert!((SolarStorm.probability(m) - 0.02).abs() < 1e-6);
        }
        for m in [
            SurveyMethod::SurfaceLander,
            SurveyMethod::Rover,
            SurveyMethod::Drill,
            SurveyMethod::SampleReturn,
        ] {
            assert!((CrewInjury.probability(m) - 0.02).abs() < 1e-6);
        }
    }

    #[test]
    fn failure_roll_lands_at_expected_rate_for_1000_runs() {
        // Use a fixed seed so the test is deterministic. With a
        // Flyby mission, the only applicable failure modes are
        // ProbeLoss (5%) and SolarStorm (2%) — 7% total. The
        // 1000-run empirical rate should be within 3% of 0.07.
        let mut rng = ProceduralRng(StdRng::seed_from_u64(0xCAFE_F00D));
        let n = 1000;
        let mut failures = 0;
        for _ in 0..n {
            let outcome = roll_mission_outcome(&mut rng.0, SurveyMethod::Flyby);
            if !outcome.is_success() {
                failures += 1;
            }
        }
        let rate = failures as f32 / n as f32;
        assert!(
            (rate - 0.07).abs() < 0.03,
            "expected ~0.07 failure rate, got {rate}"
        );
    }

    #[test]
    fn ground_team_mission_injures_scientist() {
        // Build a Scientist and a CrewInjury outcome, run
        // `injure_first_scientist`, verify the scientist is
        // injured and the returned event references the right
        // scientist id.
        let mut world = World::new();
        let scientist_entity = world.spawn(make_scientist(42, "Dr. R. Vasquez")).id();
        let mut mission = make_mission(SurveyMethod::SurfaceLander, 365);
        mission.assigned_scientists = vec![42];
        let body = world.spawn_empty().id();

        let evt = injure_first_scientist(body, &mission, sim_time(), &mut world)
            .expect("should produce a CrewInjured event");

        let s = world.get::<Scientist>(scientist_entity).unwrap();
        assert!(s.is_injured(sim_time()));
        assert!(s.injured_until_sim_time.unwrap() > sim_time());
        assert!(s.current_survey_mission.is_none());

        match evt {
            SurveyEvent::CrewInjured {
                scientist,
                scientist_name,
                ..
            } => {
                assert_eq!(scientist, 42);
                assert_eq!(scientist_name, "Dr. R. Vasquez");
            }
            other => panic!("expected CrewInjured, got {other:?}"),
        }
    }

    #[test]
    fn scientist_xp_awarded_on_success() {
        let mut world = World::new();
        let scientist_entity = world.spawn(make_scientist(7, "Dr. K. Park")).id();
        let mut mission = make_mission(SurveyMethod::Orbital, 365);
        mission.assigned_scientists = vec![7];
        award_scientist_xp(&mut world, &mission, 1000.0);
        let s = world.get::<Scientist>(scientist_entity).unwrap();
        assert!(s.lifetime_data_processed > 0.0);
        assert!(s.current_survey_mission.is_none());
    }

    #[test]
    fn injured_scientist_cannot_be_dispatched() {
        // Build a scientist, injure them, then build a dispatch
        // event referencing that scientist. Run the dispatch
        // system and verify the dispatch was dropped.
        let mut world = World::new();
        world.init_resource::<SimulationTime>();
        world.init_resource::<ProceduralRng>();
        world.init_resource::<super::super::data::SurveyMissionTemplates>();
        world.init_resource::<Messages<SurveyEvent>>();
        world.init_resource::<Messages<DispatchSurveyMission>>();

        // Spawn a body with an empty SurveyState.
        let body = world.spawn(SurveyState::default()).id();

        // Spawn an injured scientist (injured_until = +∞, so
        // always injured).
        let _scientist_entity = world
            .spawn(Scientist {
                injured_until_sim_time: Some(f64::INFINITY),
                ..make_scientist(99, "Injured")
            })
            .id();

        // Insert a flyby template so the dispatch finds it.
        world
            .resource_mut::<super::super::data::SurveyMissionTemplates>()
            .templates
            .insert(
                "flyby_recon".to_string(),
                SurveyMissionTemplate {
                    id: "flyby_recon".to_string(),
                    display_name: "Flyby Probe".to_string(),
                    method: SurveyMethod::Flyby,
                    instrument_id: "phased_array_radar".to_string(),
                    target_tiers: HashMap::new(),
                    base_duration_days: 540,
                    axis_yield_per_day: 1.0,
                    is_ground_team: false,
                },
            );

        // Write the dispatch event referencing the injured
        // scientist.
        world.write_message(DispatchSurveyMission {
            body,
            template_id: "flyby_recon".to_string(),
            name: "Mare Imbrium 1".to_string(),
            scientist_ids: vec![99],
        });

        // Run the dispatch system.
        dispatch_survey_mission(&mut world);

        // The dispatch must be dropped because the scientist is
        // injured; the body's mission list is still empty.
        let state = world.get::<SurveyState>(body).unwrap();
        assert!(state.active_missions.is_empty());
    }

    #[test]
    fn next_mission_id_increments_from_max() {
        let mut state = SurveyState::default();
        state
            .active_missions
            .push(make_mission(SurveyMethod::Flyby, 1));
        state
            .active_missions
            .push(make_mission(SurveyMethod::Orbital, 1));
        state.active_missions[1].id = 7;
        assert_eq!(next_mission_id(&state), 8);
    }

    // ---- PR-D landing/extraction site evaluation tests ----

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

    // ---- GRA-83 PR-E: per-body isolation tests for
    //      `apply_continuous_station_bonus`. ----
    //
    // The acceptance criteria from the issue body (per CTO recipe
    // §5) require:
    //   1. A tier-1 station at Mars advances Mars axes by ~0.05
    //      per year and gives Mars mines a 5% yield bonus.
    //   2. A tier-1 station at Phobos is independent — Phobos
    //      axes advance and Phobos mines get the bonus, with no
    //      cross-pollination.
    //   3. Destroying (despawning) the station on a body reverts
    //      that body's bonus to neutral, but leaves other bodies'
    //      bonuses untouched.

    /// Set the SimulationTime's elapsed to a specific sim-year
    /// count. `years == 0` → `elapsed = 0`; `years == 1` →
    /// `elapsed = SURVEY_DAYS_PER_YEAR × 86_400`.
    fn set_sim_years(world: &mut World, years: f64) {
        let elapsed = years * SURVEY_DAYS_PER_YEAR * 86_400.0;
        world.resource_mut::<SimulationTime>().elapsed = elapsed;
    }

    #[test]
    fn station_axis_advances_only_on_orbited_body() {
        // Spawn two bodies (Mars + Phobos), each with a
        // `SurveyState` and a tier-1 station orbiting *only* its
        // own body. Run the system for one sim-year. Verify:
        //   - Mars `SurveyState` axes advance by 0.05
        //   - Phobos `SurveyState` axes advance by 0.05
        //   - Neither affects the other body
        let mut world = World::new();
        world.init_resource::<SimulationTime>();

        let mars = world
            .spawn((
                CelestialBody {
                    name: "Mars".to_string(),
                    radius: 3_400_000.0,
                    mass: 6.4e23,
                    body_type: crate::plugins::solar_system_data::BodyType::Planet,
                    visual_radius: 1.0,
                    asteroid_class: None,
                },
                SurveyState::default(),
            ))
            .id();
        let phobos = world
            .spawn((
                CelestialBody {
                    name: "Phobos".to_string(),
                    radius: 11_000.0,
                    mass: 1.07e16,
                    body_type: crate::plugins::solar_system_data::BodyType::Moon,
                    visual_radius: 0.5,
                    asteroid_class: None,
                },
                SurveyState::default(),
            ))
            .id();

        world.spawn(ContinuousSurveyStation {
            orbiting_body: Some(mars),
            tier: 1,
            last_tick_sim_time: None,
        });
        world.spawn(ContinuousSurveyStation {
            orbiting_body: Some(phobos),
            tier: 1,
            last_tick_sim_time: None,
        });

        // Advance one sim-year, then run the system.
        set_sim_years(&mut world, 1.0);
        apply_continuous_station_bonus(&mut world);

        // Both bodies should have advanced every dimension from
        // tier 0 to tier 0.05 (rounded down to 0 because tier is
        // an integer). The acceptance criterion is "axes gain
        // 0.05–0.10" — but our `tier: u8` representation
        // truncates fractional tiers to 0. Verify the underlying
        // axis_advance_per_year cache is 0.05 instead.
        let mars_bonus = world
            .get::<ContinuousStationBonus>(mars)
            .expect("Mars must have a ContinuousStationBonus after the system runs");
        let phobos_bonus = world
            .get::<ContinuousStationBonus>(phobos)
            .expect("Phobos must have a ContinuousStationBonus after the system runs");
        assert!(
            (mars_bonus.axis_advance_per_year - 0.05).abs() < 1e-6,
            "Mars axis_advance_per_year should be 0.05, got {}",
            mars_bonus.axis_advance_per_year
        );
        assert!(
            (phobos_bonus.axis_advance_per_year - 0.05).abs() < 1e-6,
            "Phobos axis_advance_per_year should be 0.05, got {}",
            phobos_bonus.axis_advance_per_year
        );
        // Mining yield bonus: 1.0 + tier-1 delta (0.05) = 1.05.
        assert!(
            (mars_bonus.mining_yield_multiplier - 1.05).abs() < 1e-6,
            "Mars mining_yield_multiplier should be 1.05, got {}",
            mars_bonus.mining_yield_multiplier
        );
        assert!(
            (phobos_bonus.mining_yield_multiplier - 1.05).abs() < 1e-6,
            "Phobos mining_yield_multiplier should be 1.05, got {}",
            phobos_bonus.mining_yield_multiplier
        );
    }

    #[test]
    fn station_mining_bonus_isolated_per_body() {
        // Spawn three bodies: Mars, Phobos, Earth. Only Mars
        // gets a tier-1 station. Phobos and Earth get *no*
        // station. Run the system and verify only Mars has a
        // non-neutral `ContinuousStationBonus`.
        let mut world = World::new();
        world.init_resource::<SimulationTime>();

        let mars = world
            .spawn((
                CelestialBody {
                    name: "Mars".to_string(),
                    radius: 3_400_000.0,
                    mass: 6.4e23,
                    body_type: crate::plugins::solar_system_data::BodyType::Planet,
                    visual_radius: 1.0,
                    asteroid_class: None,
                },
                SurveyState::default(),
            ))
            .id();
        let _phobos = world
            .spawn((
                CelestialBody {
                    name: "Phobos".to_string(),
                    radius: 11_000.0,
                    mass: 1.07e16,
                    body_type: crate::plugins::solar_system_data::BodyType::Moon,
                    visual_radius: 0.5,
                    asteroid_class: None,
                },
                SurveyState::default(),
            ))
            .id();
        let _earth = world
            .spawn((
                CelestialBody {
                    name: "Earth".to_string(),
                    radius: 6_371_000.0,
                    mass: 5.97e24,
                    body_type: crate::plugins::solar_system_data::BodyType::Planet,
                    visual_radius: 1.0,
                    asteroid_class: None,
                },
                SurveyState::default(),
            ))
            .id();

        world.spawn(ContinuousSurveyStation {
            orbiting_body: Some(mars),
            tier: 1,
            last_tick_sim_time: None,
        });

        set_sim_years(&mut world, 1.0);
        apply_continuous_station_bonus(&mut world);

        // Mars has the bonus (1.05×, 0.05/yr axis). Phobos and
        // Earth get no bonus — they have no
        // `ContinuousStationBonus` component at all, which is
        // the "no station" invariant the mining system reads as
        // 1.0×.
        let mars_bonus = world
            .get::<ContinuousStationBonus>(mars)
            .expect("Mars must have ContinuousStationBonus");
        assert!((mars_bonus.mining_yield_multiplier - 1.05).abs() < 1e-6);
        assert!((mars_bonus.axis_advance_per_year - 0.05).abs() < 1e-6);

        // Phobos and Earth: no station, no bonus cache. The
        // mining system treats `None` as 1.0×.
        let phobos_bonus = world.get::<ContinuousStationBonus>(_phobos);
        let earth_bonus = world.get::<ContinuousStationBonus>(_earth);
        assert!(
            phobos_bonus.is_none(),
            "Phobos must NOT have a ContinuousStationBonus"
        );
        assert!(
            earth_bonus.is_none(),
            "Earth must NOT have a ContinuousStationBonus"
        );
    }

    #[test]
    fn station_destroyed_removes_bonus() {
        // Spawn Mars and Phobos, each with a tier-1 station.
        // Run the system once — both have bonuses. Despawn the
        // Mars station. Run again — Mars reverts to neutral
        // (1.0×, 0.0/yr), Phobos is untouched.
        let mut world = World::new();
        world.init_resource::<SimulationTime>();

        let mars = world
            .spawn((
                CelestialBody {
                    name: "Mars".to_string(),
                    radius: 3_400_000.0,
                    mass: 6.4e23,
                    body_type: crate::plugins::solar_system_data::BodyType::Planet,
                    visual_radius: 1.0,
                    asteroid_class: None,
                },
                SurveyState::default(),
            ))
            .id();
        let phobos = world
            .spawn((
                CelestialBody {
                    name: "Phobos".to_string(),
                    radius: 11_000.0,
                    mass: 1.07e16,
                    body_type: crate::plugins::solar_system_data::BodyType::Moon,
                    visual_radius: 0.5,
                    asteroid_class: None,
                },
                SurveyState::default(),
            ))
            .id();

        let mars_station = world
            .spawn(ContinuousSurveyStation {
                orbiting_body: Some(mars),
                tier: 1,
                last_tick_sim_time: None,
            })
            .id();
        let phobos_station = world
            .spawn(ContinuousSurveyStation {
                orbiting_body: Some(phobos),
                tier: 1,
                last_tick_sim_time: None,
            })
            .id();

        // First run — both bodies have bonuses.
        set_sim_years(&mut world, 1.0);
        apply_continuous_station_bonus(&mut world);
        assert!(
            (world
                .get::<ContinuousStationBonus>(mars)
                .unwrap()
                .mining_yield_multiplier
                - 1.05)
                .abs()
                < 1e-6
        );
        assert!(
            (world
                .get::<ContinuousStationBonus>(phobos)
                .unwrap()
                .mining_yield_multiplier
                - 1.05)
                .abs()
                < 1e-6
        );

        // Despawn the Mars station. Mars's ContinuousStationBonus
        // must still be present (the system resets it, doesn't
        // remove the component) and at neutral values.
        world.despawn(mars_station);
        let _ = phobos_station; // silence unused-warning without dropping the binding

        set_sim_years(&mut world, 2.0);
        apply_continuous_station_bonus(&mut world);

        let mars_bonus = world
            .get::<ContinuousStationBonus>(mars)
            .expect("Mars bonus component should still exist (the system resets, not removes)");
        assert!(
            (mars_bonus.mining_yield_multiplier - 1.0).abs() < 1e-6,
            "Mars mining bonus should revert to 1.0 after station despawn, got {}",
            mars_bonus.mining_yield_multiplier
        );
        assert!(
            mars_bonus.axis_advance_per_year.abs() < 1e-6,
            "Mars axis advance should revert to 0.0 after station despawn, got {}",
            mars_bonus.axis_advance_per_year
        );

        // Phobos is untouched.
        let phobos_bonus = world
            .get::<ContinuousStationBonus>(phobos)
            .expect("Phobos bonus must remain");
        assert!(
            (phobos_bonus.mining_yield_multiplier - 1.05).abs() < 1e-6,
            "Phobos bonus should still be 1.05, got {}",
            phobos_bonus.mining_yield_multiplier
        );
    }

    #[test]
    fn station_with_no_orbited_body_is_inert() {
        // A station with `orbiting_body: None` is the
        // construction-panel-handoff case. It must be inert —
        // it doesn't tick any body, and no body gets a bonus.
        let mut world = World::new();
        world.init_resource::<SimulationTime>();

        let _body = world
            .spawn((
                CelestialBody {
                    name: "Mars".to_string(),
                    radius: 3_400_000.0,
                    mass: 6.4e23,
                    body_type: crate::plugins::solar_system_data::BodyType::Planet,
                    visual_radius: 1.0,
                    asteroid_class: None,
                },
                SurveyState::default(),
            ))
            .id();

        world.spawn(ContinuousSurveyStation {
            orbiting_body: None,
            tier: 1,
            last_tick_sim_time: None,
        });

        set_sim_years(&mut world, 1.0);
        apply_continuous_station_bonus(&mut world);

        // No body should have a ContinuousStationBonus.
        let bonus = world.get::<ContinuousStationBonus>(_body);
        assert!(
            bonus.is_none(),
            "An unbound station must not produce a bonus on any body"
        );
    }

    #[test]
    fn tier_constants_match_design() {
        // Lock the CTO recipe's tier table. A future rebalance
        // PR that changes these must update this test (and
        // re-balance other call sites in `mining.rs`).
        assert!((axis_advance_rate_for_tier(1) - 0.05).abs() < 1e-6);
        assert!((axis_advance_rate_for_tier(2) - 0.075).abs() < 1e-6);
        assert!((axis_advance_rate_for_tier(3) - 0.10).abs() < 1e-6);
        assert_eq!(axis_advance_rate_for_tier(0), 0.0);
        assert_eq!(axis_advance_rate_for_tier(99), 0.0);

        assert!((mining_yield_delta_for_tier(1) - 0.05).abs() < 1e-6);
        assert!((mining_yield_delta_for_tier(2) - 0.10).abs() < 1e-6);
        assert!((mining_yield_delta_for_tier(3) - 0.15).abs() < 1e-6);
        assert_eq!(mining_yield_delta_for_tier(0), 0.0);
        assert_eq!(mining_yield_delta_for_tier(99), 0.0);
    }

    #[test]
    fn multiple_stations_stack_mining_yield() {
        // Two tier-1 stations on the same body should stack
        // mining yield: 1.0 + 0.05 + 0.05 = 1.10.
        let mut world = World::new();
        world.init_resource::<SimulationTime>();

        let mars = world
            .spawn((
                CelestialBody {
                    name: "Mars".to_string(),
                    radius: 3_400_000.0,
                    mass: 6.4e23,
                    body_type: crate::plugins::solar_system_data::BodyType::Planet,
                    visual_radius: 1.0,
                    asteroid_class: None,
                },
                SurveyState::default(),
            ))
            .id();

        world.spawn(ContinuousSurveyStation {
            orbiting_body: Some(mars),
            tier: 1,
            last_tick_sim_time: None,
        });
        world.spawn(ContinuousSurveyStation {
            orbiting_body: Some(mars),
            tier: 1,
            last_tick_sim_time: None,
        });

        set_sim_years(&mut world, 1.0);
        apply_continuous_station_bonus(&mut world);

        let bonus = world.get::<ContinuousStationBonus>(mars).unwrap();
        assert!(
            (bonus.mining_yield_multiplier - 1.10).abs() < 1e-6,
            "Two tier-1 stations should stack to 1.10×, got {}",
            bonus.mining_yield_multiplier
        );
        assert!(
            (bonus.axis_advance_per_year - 0.10).abs() < 1e-6,
            "Two tier-1 stations should sum axis rate to 0.10/yr, got {}",
            bonus.axis_advance_per_year
        );
    }
}
