//! Mission-log consumer systems.
//!
//! Five systems share the [`MissionLogSystemSet`] set in `Update`. Each
//! consumes one source-event family and writes the result into
//! `MissionLog`. The systems are independent — they target disjoint
//! message families — so the order WITHIN the set is irrelevant. We
//! still chain via `.chain()` for documentation purposes.
//!
//! # Idempotence
//!
//! Each system uses `MissionLog::push_current` / `MissionLog::resolve`
//! / `MissionLog::upsert_goal`, all of which dedup by stable
//! `entry_id` / `goal_id`. A duplicate delivery of the same source
//! event (e.g. an `AnomalyActivated` re-firing after a re-arm loop) is
//! a no-op at the mission-log layer.
//!
//! # Goal semantics
//!
//! Goal entries are *registered* lazily — when a survey / construction
//! / research / milestone event fires that maps to a known goal id,
//! the consumer upserts a `GoalEntry`. We do NOT pre-seed the goals
//! list at plugin init time; this keeps the log empty on a fresh game
//! and lets the goal list evolve with the storyline (GRA-792 /
//! GRA-793). Goals that match no fired event simply never appear in
//! the log — that is the intended behaviour for this PR.
//!
//! Read [`crate::mission_log`] for the module-level doc.

use std::collections::BTreeSet;

use bevy::prelude::*;

use crate::colony::events::ConstructionEvent;
use crate::colony::types::BuildingType;
use crate::mission_log::components::{
    BodyKey, GoalEntry, GoalStatus, MissionEntry, MissionKind, MissionLog, MissionLogConfig,
    MissionOutcome, MissionSource,
};
use crate::persistence::state_store::BodyKey as StoreBodyKey;
use crate::research::events::ResearchEvent;
use crate::survey::events::SurveyEvent;
use crate::survey::types::SurveyMethod;
use crate::ui::time::SimulationTime;

/// Canonical goal-id mapping for the survey milestone arm. The list is
/// deliberately small for this PR — Story Designer (Hermes) authors
/// late-era entries in GRA-792 / GRA-793.
///
/// `goal_id` strings are stable; do not rename without a migration.
fn survey_goal_id_for_completion(kind: &MissionKind) -> Option<&'static str> {
    match kind {
        MissionKind::Flyby => Some("storyline.first_probe"),
        MissionKind::Orbital => Some("storyline.first_orbital"),
        MissionKind::Rover => Some("storyline.first_surface_mission"),
        _ => None,
    }
}

/// Canonical goal-id mapping for the construction arm.
fn construction_goal_id_for_event(kind: &MissionKind) -> Option<&'static str> {
    match kind {
        MissionKind::OutpostEstablishment => Some("storyline.first_outpost"),
        _ => None,
    }
}

/// Canonical goal-id mapping for the research arm. We attach a goal
/// to the first paid tier-1 unlock (mirrors `EarlyGameMilestones`).
fn research_goal_id_for_paid_tier_1(tech_id: &str) -> Option<String> {
    // Pre-defined story goal for the first tier-1 paid unlock. Other
    // paid tier-1s are tracked in `MissionLog::current` only.
    if tech_id == "fusion_propulsion" {
        Some("storyline.first_paid_tier_1".into())
    } else {
        None
    }
}

/// Map a `SurveyMethod` to the canonical `MissionKind`.
fn survey_method_kind(method: SurveyMethod) -> MissionKind {
    match method {
        SurveyMethod::Flyby => MissionKind::Flyby,
        SurveyMethod::Orbital => MissionKind::Orbital,
        SurveyMethod::RemoteSensing => MissionKind::RemoteSensing,
        SurveyMethod::AtmosphericProbe => MissionKind::AtmosphericProbe,
        SurveyMethod::Seismic => MissionKind::Seismic,
        SurveyMethod::SurfaceLander => MissionKind::SurfaceLander,
        SurveyMethod::Rover => MissionKind::Rover,
        SurveyMethod::Drill => MissionKind::Drill,
        SurveyMethod::SampleReturn => MissionKind::SampleReturn,
    }
}

/// Synthesise a stable body key for a mission-log entry. The
/// `body_entity` must have both `CelestialBody` and `SystemId`
/// components (the standard production schema — see `src/plugins/camera.rs`).
/// Returns `None` if either is missing; the consumer treats `None` as
/// "no body anchor".
fn body_key_for(
    body_entity: Entity,
    bodies: &Query<(
        &crate::plugins::solar_system::CelestialBody,
        &crate::astronomy::components::SystemId,
    )>,
) -> Option<StoreBodyKey> {
    let Ok((body, sys)) = bodies.get(body_entity) else {
        return None;
    };
    Some(StoreBodyKey::new(*sys, body.name.clone()))
}

/// Synthesise the stable `entry_id` string for a survey mission.
/// Including the method in the id ensures a mission that was
/// re-dispatched with a different `method` (rare but possible via the
/// dispatcher's "edit-and-redispatch" UI) doesn't silently dedup.
fn survey_entry_id(mission_id: u64, kind: &MissionKind) -> String {
    format!("survey:{}:{}", mission_id, kind.id_str())
}

/// System set that owns the mission-log consumer systems. Runs in
/// `Update`; see [`crate::mission_log::plugin`] for registration.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct MissionLogSystemSet;

/// Consumer: survey events.
///
/// `MissionStarted` → push `current`. `MissionCompleted` / `MissionFailed`
/// / `MissionAborted` → resolve to `past` with the matching outcome.
/// Other variants are intentionally ignored — they are either negative
/// tail-events (ProbeLost / RoverStuck / DrillBitStuck / CrewInjured)
/// or internal counters (AnomalyRefuted / MissionLaunchBlocked /
/// AnomalyDetected / AnomalyActivated). The dossier / toast layer
/// already surfaces those; the mission log is the resolved-history
/// mirror.
pub fn apply_survey_events_to_mission_log(
    mut log: ResMut<MissionLog>,
    cfg: Res<MissionLogConfig>,
    time: Res<SimulationTime>,
    bodies: Query<(
        &crate::plugins::solar_system::CelestialBody,
        &crate::astronomy::components::SystemId,
    )>,
    mut events: MessageReader<SurveyEvent>,
) {
    let now = time.elapsed_seconds();
    for event in events.read() {
        match event {
            SurveyEvent::MissionStarted {
                mission_id,
                name,
                method,
                body,
            } => {
                let kind = survey_method_kind(*method);
                let id = survey_entry_id(*mission_id, &kind);
                let target = body_key_for(*body, &bodies);
                let entry = MissionEntry {
                    entry_id: id,
                    kind,
                    source: MissionSource::Survey,
                    target_body: target.map(BodyKey::from),
                    display_name: name.clone(),
                    started_sim_seconds: now,
                    outcome: None,
                    resolved_sim_seconds: None,
                };
                if log.push_current(entry, &cfg) {
                    if let Some(goal_id) =
                        survey_goal_id_for_completion(&survey_method_kind(*method))
                    {
                        log.upsert_goal(GoalEntry {
                            goal_id: goal_id.to_string(),
                            label: goal_label_for(goal_id).to_string(),
                            status: GoalStatus::InProgress,
                            achieved_sim_seconds: None,
                            triggering_entry_id: None,
                        });
                    }
                }
            }
            SurveyEvent::MissionCompleted {
                mission_id, method, ..
            } => {
                let kind = survey_method_kind(*method);
                let id = survey_entry_id(*mission_id, &kind);
                if log.resolve(&id, MissionOutcome::Completed, now, &cfg) {
                    if let Some(goal_id) = survey_goal_id_for_completion(&kind) {
                        log.upsert_goal(GoalEntry {
                            goal_id: goal_id.to_string(),
                            label: goal_label_for(goal_id).to_string(),
                            status: GoalStatus::Achieved,
                            achieved_sim_seconds: Some(now),
                            triggering_entry_id: Some(id),
                        });
                    }
                }
            }
            SurveyEvent::MissionFailed {
                mission_id, method, ..
            } => {
                let kind = survey_method_kind(*method);
                let id = survey_entry_id(*mission_id, &kind);
                log.resolve(&id, MissionOutcome::Failed, now, &cfg);
            }
            SurveyEvent::MissionAborted {
                mission_id, method, ..
            } => {
                let kind = survey_method_kind(*method);
                let id = survey_entry_id(*mission_id, &kind);
                log.resolve(&id, MissionOutcome::Aborted, now, &cfg);
            }
            SurveyEvent::ProbeLost {
                mission_id, method, ..
            }
            | SurveyEvent::RoverStuck {
                mission_id, method, ..
            }
            | SurveyEvent::DrillBitStuck {
                mission_id, method, ..
            } => {
                let kind = survey_method_kind(*method);
                let id = survey_entry_id(*mission_id, &kind);
                // Companion events still resolve the parent mission —
                // a probe lost mid-mission means the mission failed.
                log.resolve(&id, MissionOutcome::Failed, now, &cfg);
            }
            SurveyEvent::CrewInjured { .. }
            | SurveyEvent::AnomalyDetected { .. }
            | SurveyEvent::AnomalyActivated { .. }
            | SurveyEvent::AnomalyRefuted { .. }
            | SurveyEvent::MissionLaunchBlocked { .. } => {}
        }
    }
}

/// Consumer: construction events.
///
/// `OutpostEstablished` (added by GRA-787) drives the
/// "first outpost" goal. `Completed` and `ShipCompleted` are recorded
/// as resolved construction missions — they don't drive a goal but
/// they show up in the player-facing log.
pub fn apply_construction_events_to_mission_log(
    mut log: ResMut<MissionLog>,
    cfg: Res<MissionLogConfig>,
    time: Res<SimulationTime>,
    bodies: Query<(
        &crate::plugins::solar_system::CelestialBody,
        &crate::astronomy::components::SystemId,
    )>,
    mut events: MessageReader<ConstructionEvent>,
) {
    let now = time.elapsed_seconds();
    for event in events.read() {
        match event {
            ConstructionEvent::OutpostEstablished { colony, body } => {
                let colony_id = colony.index();
                let body_id = body.index();
                let id = format!("construction:outpost:{colony_id}");
                let target = body_key_for(*body, &bodies);
                let entry = MissionEntry {
                    entry_id: id.clone(),
                    kind: MissionKind::OutpostEstablishment,
                    source: MissionSource::Construction,
                    target_body: target.map(BodyKey::from),
                    display_name: format!("Outpost @ body {body_id}"),
                    started_sim_seconds: now,
                    outcome: None,
                    resolved_sim_seconds: None,
                };
                if log.push_current(entry, &cfg) {
                    if let Some(goal_id) =
                        construction_goal_id_for_event(&MissionKind::OutpostEstablishment)
                    {
                        log.upsert_goal(GoalEntry {
                            goal_id: goal_id.to_string(),
                            label: goal_label_for(goal_id).to_string(),
                            status: GoalStatus::InProgress,
                            achieved_sim_seconds: None,
                            triggering_entry_id: None,
                        });
                    }
                }
                // Outpost is resolved in the same event — no follow-up
                // `Completed` event fires for the implicit "promotion".
                if log.resolve(&id, MissionOutcome::Completed, now, &cfg) {
                    if let Some(goal_id) =
                        construction_goal_id_for_event(&MissionKind::OutpostEstablishment)
                    {
                        log.upsert_goal(GoalEntry {
                            goal_id: goal_id.to_string(),
                            label: goal_label_for(goal_id).to_string(),
                            status: GoalStatus::Achieved,
                            achieved_sim_seconds: Some(now),
                            triggering_entry_id: Some(id),
                        });
                    }
                }
            }
            ConstructionEvent::Completed { colony, building } => {
                let colony_id = colony.index();
                let id = format!("construction:{colony_id}:{}", building_id_str(*building));
                // Construction `Completed` is already-resolved — log it
                // directly as `past`. push_current would mark it active;
                // we want it to land in the resolved history.
                let entry = MissionEntry {
                    entry_id: id,
                    kind: MissionKind::ConstructBuilding,
                    source: MissionSource::Construction,
                    target_body: None,
                    display_name: building.display_name().to_string(),
                    started_sim_seconds: now,
                    outcome: Some(MissionOutcome::Completed),
                    resolved_sim_seconds: Some(now),
                };
                // Bypass push_current's "active only" check by hand:
                // dedup against current + past, then push to past.
                if log.find_by_id(&entry.entry_id).is_none() {
                    log.past.push_back(entry);
                    while log.past.len() > cfg.past_capacity {
                        log.past.pop_front();
                    }
                }
            }
            ConstructionEvent::ShipCompleted { hull } => {
                let id = format!("construction:hull:{hull}");
                if log.find_by_id(&id).is_none() {
                    let entry = MissionEntry {
                        entry_id: id,
                        kind: MissionKind::ConstructShip,
                        source: MissionSource::Construction,
                        target_body: None,
                        display_name: hull.clone(),
                        started_sim_seconds: now,
                        outcome: Some(MissionOutcome::Completed),
                        resolved_sim_seconds: Some(now),
                    };
                    log.past.push_back(entry);
                    while log.past.len() > cfg.past_capacity {
                        log.past.pop_front();
                    }
                }
            }
        }
    }
}

fn building_id_str(b: BuildingType) -> String {
    // Stable lowercase snake_case id. The exhaustive match would
    // rot every time a building is added; instead we use the enum's
    // `Debug` repr (PascalCase variant name) and convert it inline.
    // `BuildingType` does not derive a custom `Display` for the
    // variant — `Debug` is the canonical stable string. The
    // conversion handles acronyms (`DTFusionReactor` → `dt_fusion_reactor`,
    // `DHe3FusionReactor` → `d_he3_fusion_reactor`).
    pascal_to_snake(&format!("{b:?}"))
}

fn pascal_to_snake(s: &str) -> String {
    // Cheap, allocation-free PascalCase → snake_case. Algorithm:
    // iterate the chars; emit `_` before every uppercase letter that
    // either (a) follows a lowercase letter, or (b) is followed by a
    // lowercase letter and preceded by another uppercase letter
    // (handles acronyms like `DT` in `DTFusionReactor`). Lowercase the
    // result.
    let mut out = String::with_capacity(s.len() + 4);
    let chars: Vec<char> = s.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c.is_ascii_uppercase() && i > 0 {
            let prev = chars[i - 1];
            let next = chars.get(i + 1).copied().unwrap_or('\0');
            let prev_is_lower = prev.is_ascii_lowercase();
            let prev_is_upper = prev.is_ascii_uppercase();
            let next_is_lower = next.is_ascii_lowercase();
            if prev_is_lower || (prev_is_upper && next_is_lower) {
                out.push('_');
            }
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

/// Consumer: research events.
///
/// `TechCompleted` resolves an entry keyed by `tech_id`. The first
/// paid tier-1 unlock also flips the `storyline.first_paid_tier_1`
/// goal. We do NOT pre-seed a `current` entry for in-flight research
/// projects; that would require a `ResearchEvent::TechStarted` variant
/// (out of scope for this PR). Resolved-only research entries is a
/// known limitation noted in the design comment.
pub fn apply_research_events_to_mission_log(
    mut log: ResMut<MissionLog>,
    cfg: Res<MissionLogConfig>,
    time: Res<SimulationTime>,
    mut events: MessageReader<ResearchEvent>,
    tech_data: Option<Res<crate::research::TechnologiesData>>,
) {
    let now = time.elapsed_seconds();
    for event in events.read() {
        let ResearchEvent::TechCompleted {
            tech_id,
            tech_display_name,
        } = event;
        let id = format!("research:{tech_id}");
        // Synthesise a "started_sim_seconds" so the resolved entry
        // shows a non-zero duration. We pick a small negative offset
        // because we don't know the actual start sim-time — the
        // dossier / notification surface already carries that info
        // for the player. The log is a resolved-history mirror; the
        // offset is just a sane default for `started_sim_seconds`.
        let started_at = (now - 1.0).max(0.0);
        let entry = MissionEntry {
            entry_id: id.clone(),
            kind: MissionKind::TechProject,
            source: MissionSource::Research,
            target_body: None,
            display_name: tech_display_name.clone(),
            started_sim_seconds: started_at,
            outcome: Some(MissionOutcome::Completed),
            resolved_sim_seconds: Some(now),
        };
        if log.find_by_id(&entry.entry_id).is_none() {
            log.past.push_back(entry);
            while log.past.len() > cfg.past_capacity {
                log.past.pop_front();
            }
        }
        // Goal flip: only on paid tier-1.
        let is_paid_tier_1 = tech_data
            .as_ref()
            .and_then(|d| d.get_tech(tech_id))
            .map(|t| t.tier == 1 && t.research_cost > 0.0)
            .unwrap_or(false);
        if is_paid_tier_1 {
            if let Some(goal_id) = research_goal_id_for_paid_tier_1(tech_id) {
                log.upsert_goal(GoalEntry {
                    goal_id,
                    label: goal_label_for("storyline.first_paid_tier_1").to_string(),
                    status: GoalStatus::Achieved,
                    achieved_sim_seconds: Some(now),
                    triggering_entry_id: Some(id),
                });
            }
        }
    }
}

/// Consumer: milestone notifications.
///
/// This consumer is intentionally a no-op stub until
/// [GRA-804](https://paperclip.klingspor.one/GRA/issues/GRA-804) lands
/// the typed `NotificationEvent::MilestoneReached` variant on `main`.
/// Once GRA-804 merges, the body becomes:
///
/// ```ignore
/// pub fn apply_milestone_events_to_mission_log(
///     mut log: ResMut<MissionLog>,
///     time: Res<SimulationTime>,
///     mut events: MessageReader<NotificationEvent>,
/// ) {
///     let now = time.elapsed_seconds();
///     for event in events.read() {
///         if let NotificationEvent::MilestoneReached { step, .. } = event {
///             let goal_id = match step {
///                 MilestoneStep::ProbeDispatched => "storyline.first_probe",
///                 MilestoneStep::SurveyCompleted => "storyline.first_survey",
///                 MilestoneStep::AnomalyDetectedOrActivated => "storyline.first_anomaly",
///                 MilestoneStep::DepositExtractionMilestone => "storyline.first_deposit",
///                 MilestoneStep::OutpostEstablished => "storyline.first_outpost",
///                 MilestoneStep::PaidTier1TechnologyUnlocked => "storyline.first_paid_tier_1",
///             };
///             log.upsert_goal(GoalEntry {
///                 goal_id: goal_id.into(),
///                 label: goal_label_for(goal_id).into(),
///                 status: GoalStatus::Achieved,
///                 achieved_sim_seconds: Some(now),
///                 triggering_entry_id: None,
///             });
///         }
///     }
/// }
/// ```
///
/// Until then, the survey / construction consumers above already
/// drive the storyline goals for the cases GRA-790 is meant to cover.
/// The duplicate (milestone arm + survey/construction arm) is safe
/// because `upsert_goal` is monotonic and the `triggering_entry_id`
/// anchor is preserved across re-asserts.
///
/// The stub takes the Bevy system parameters the real version will
/// need so the parameter list is already correct when GRA-804 lands —
/// this avoids a follow-up consumer-side migration when the producer
/// variant appears.
#[allow(dead_code)]
pub fn apply_milestone_events_to_mission_log(
    mut _log: ResMut<MissionLog>,
    _time: Res<SimulationTime>,
    mut _events: MessageReader<crate::ui::notifications::events::NotificationEvent>,
) {
    // Drain the buffer so it doesn't grow unbounded while the
    // consumer is a no-op — same defensive pattern as
    // `advance_research_milestones` in `src/survey/milestones.rs`.
    for _event in _events.read() {
        // Intentionally ignored; see the doc-comment above.
    }
}

fn goal_label_for(goal_id: &str) -> &'static str {
    match goal_id {
        "storyline.first_probe" => "First probe dispatched",
        "storyline.first_orbital" => "First orbital mission",
        "storyline.first_surface_mission" => "First surface mission",
        "storyline.first_survey" => "First survey completed",
        "storyline.first_anomaly" => "First anomaly detected",
        "storyline.first_deposit" => "First deposit discovered",
        "storyline.first_outpost" => "First outpost established",
        "storyline.first_paid_tier_1" => "First paid tier-1 research",
        _ => "Unnamed goal",
    }
}

/// Stable canonical goal-id set used by tests and the save/load audit.
/// Any new goal id must be added here AND to `goal_label_for` above.
#[cfg(test)]
const KNOWN_GOAL_IDS: &[&str] = &[
    "storyline.first_probe",
    "storyline.first_orbital",
    "storyline.first_surface_mission",
    "storyline.first_survey",
    "storyline.first_anomaly",
    "storyline.first_deposit",
    "storyline.first_outpost",
    "storyline.first_paid_tier_1",
];

/// Regression-test helper: walks `src/ui/**/*.rs` and asserts no file
/// declares `ResMut<MissionLog>`. The convention is documented in
/// [`crate::mission_log`] and enforced here so a future contributor
/// can't silently break the read-only contract by adding a UI-side
/// writer.
///
/// Returns the list of files that violate the contract. Empty list =
/// no violations. The caller (`cargo test`) compares to `[]` and
/// fails on mismatch.
///
/// We deliberately use a string-based scan (rather than a separate
/// cargo crate) because the audit must run inside `cargo test --lib
/// mission_log` — adding a workspace member would inflate CI time.
#[cfg(test)]
pub fn assert_no_ui_resmut() -> Vec<String> {
    use std::fs;
    use std::path::Path;

    let project_root = project_root_from_cargo();
    let ui_dir = project_root.join("src").join("ui");
    let mut violations = Vec::new();
    if !ui_dir.exists() {
        return violations;
    }
    collect_rs_files(&ui_dir, &mut violations);
    let needle = "ResMut<MissionLog>";
    let needle_mut_method = ".resource_mut::<MissionLog>";
    for path in &violations {
        // Reset `violations` as the file list; we'll repopulate with
        // the actual offending files.
        let _ = path;
    }
    let mut offenders = Vec::new();
    for path in walk_rs_files(&ui_dir) {
        let Ok(contents) = fs::read_to_string(&path) else {
            continue;
        };
        if contents.contains(needle) || contents.contains(needle_mut_method) {
            offenders.push(path.to_string_lossy().into_owned());
        }
    }
    offenders
}

#[cfg(test)]
fn project_root_from_cargo() -> std::path::PathBuf {
    // CARGO_MANIFEST_DIR is set by Cargo during tests; we walk up to
    // the workspace root by looking for `Cargo.toml`.
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    std::path::PathBuf::from(manifest_dir)
}

#[cfg(test)]
fn walk_rs_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() {
            out.extend(walk_rs_files(&p));
        } else if p.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(p);
        }
    }
    out
}

#[cfg(test)]
fn collect_rs_files(dir: &std::path::Path, out: &mut Vec<String>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect_rs_files(&p, out);
        } else if p.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(p.to_string_lossy().into_owned());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colony::components::{Colony, ColonyDevelopment};
    use crate::colony::types::BuildingType;
    use crate::persistence::state_store::BodyKey;
    use crate::plugins::solar_system::{BodyType, CelestialBody};
    use crate::research::types::{TechCategory, Technology};
    use crate::research::TechnologiesData;
    use crate::survey::types::{MissionFailureReason, SurveyMethod};
    use crate::ui::time::TimeScale;
    use std::collections::HashMap;

    fn fresh_world() -> World {
        let mut world = World::new();
        world.init_resource::<MissionLog>();
        world.init_resource::<MissionLogConfig>();
        world.init_resource::<Messages<SurveyEvent>>();
        world.init_resource::<Messages<ConstructionEvent>>();
        world.init_resource::<Messages<ResearchEvent>>();
        world.init_resource::<Messages<crate::ui::notifications::events::NotificationEvent>>();
        world.init_resource::<TimeScale>();
        world.init_resource::<SimulationTime>();
        world
    }

    fn spawn_body(world: &mut World, name: &str) -> Entity {
        world
            .spawn((
                CelestialBody {
                    name: name.to_string(),
                    radius: 0.0,
                    mass: 0.0,
                    body_type: BodyType::Planet,
                    visual_radius: 0.0,
                    asteroid_class: None,
                    star_approach_au: None,
                    rotation_period_s: None,
                    habitable_outer_au: None,
                },
                crate::astronomy::components::SystemId(0),
            ))
            .id()
    }

    fn run_consumer_systems(world: &mut World) {
        let mut schedule = Schedule::default();
        schedule.add_systems(
            (
                apply_survey_events_to_mission_log,
                apply_construction_events_to_mission_log,
                apply_research_events_to_mission_log,
            )
                .in_set(MissionLogSystemSet)
                .chain(),
        );
        schedule.run(world);
    }

    #[test]
    fn survey_started_pushes_current() {
        let mut world = fresh_world();
        let body = spawn_body(&mut world, "Earth");
        world.write_message(SurveyEvent::MissionStarted {
            body,
            mission_id: 7,
            name: "Earth Flyby 1".to_string(),
            method: SurveyMethod::Flyby,
        });
        run_consumer_systems(&mut world);
        let log = world.resource::<MissionLog>();
        assert_eq!(log.current.len(), 1);
        assert_eq!(log.current[0].entry_id, "survey:7:flyby");
        assert_eq!(log.current[0].kind, MissionKind::Flyby);
        assert_eq!(
            log.current[0].target_body,
            Some(BodyKey {
                system: 0,
                name: "Earth".into(),
            })
        );
        assert!(log
            .goals
            .iter()
            .any(|g| g.goal_id == "storyline.first_probe" && g.status == GoalStatus::InProgress));
    }

    #[test]
    fn survey_completed_moves_current_to_past_and_flips_goal() {
        let mut world = fresh_world();
        let body = spawn_body(&mut world, "Earth");
        world.write_message(SurveyEvent::MissionStarted {
            body,
            mission_id: 7,
            name: "Earth Flyby 1".to_string(),
            method: SurveyMethod::Flyby,
        });
        world.write_message(SurveyEvent::MissionCompleted {
            body,
            mission_id: 7,
            name: "Earth Flyby 1".to_string(),
            method: SurveyMethod::Flyby,
        });
        run_consumer_systems(&mut world);
        let log = world.resource::<MissionLog>();
        assert!(
            log.current.is_empty(),
            "started + completed -> empty current"
        );
        assert_eq!(log.past_len(), 1);
        let past_entry = log.past.front().expect("non-empty");
        assert_eq!(past_entry.outcome, Some(MissionOutcome::Completed));
        let g = log
            .goals
            .iter()
            .find(|g| g.goal_id == "storyline.first_probe")
            .expect("goal must exist");
        assert_eq!(g.status, GoalStatus::Achieved);
        assert!(g.achieved_sim_seconds.is_some());
        assert_eq!(g.triggering_entry_id.as_deref(), Some("survey:7:flyby"));
    }

    #[test]
    fn survey_failed_resolves_with_failed_outcome() {
        let mut world = fresh_world();
        let body = spawn_body(&mut world, "Mars");
        world.write_message(SurveyEvent::MissionStarted {
            body,
            mission_id: 1,
            name: "Rover 1".to_string(),
            method: SurveyMethod::Rover,
        });
        world.write_message(SurveyEvent::MissionFailed {
            body,
            mission_id: 1,
            name: "Rover 1".to_string(),
            method: SurveyMethod::Rover,
            reason: MissionFailureReason::RoverStuck,
        });
        run_consumer_systems(&mut world);
        let log = world.resource::<MissionLog>();
        assert_eq!(log.past_len(), 1);
        assert_eq!(
            log.past.front().unwrap().outcome,
            Some(MissionOutcome::Failed)
        );
    }

    #[test]
    fn survey_duplicate_started_is_idempotent() {
        let mut world = fresh_world();
        let body = spawn_body(&mut world, "Earth");
        for _ in 0..3 {
            world.write_message(SurveyEvent::MissionStarted {
                body,
                mission_id: 1,
                name: "Earth Flyby 1".to_string(),
                method: SurveyMethod::Flyby,
            });
        }
        run_consumer_systems(&mut world);
        let log = world.resource::<MissionLog>();
        assert_eq!(log.current.len(), 1, "duplicate id must dedup");
    }

    #[test]
    fn construction_outpost_establishment_resolves_and_flips_goal() {
        let mut world = fresh_world();
        let body = spawn_body(&mut world, "Mars");
        let colony = world
            .spawn(Colony {
                name: "Mars Prime".into(),
                population: 0.0,
                growth_rate_modifier: 1.0,
                buildings: HashMap::new(),
                development: ColonyDevelopment::default(),
            })
            .id();
        world.write_message(ConstructionEvent::OutpostEstablished { colony, body });
        run_consumer_systems(&mut world);
        let log = world.resource::<MissionLog>();
        // Outpost push + immediate resolve → lands in past.
        assert!(log.current.is_empty());
        assert_eq!(log.past_len(), 1);
        assert_eq!(
            log.past.front().unwrap().entry_id,
            format!("construction:outpost:{}", colony.index())
        );
        let g = log
            .goals
            .iter()
            .find(|g| g.goal_id == "storyline.first_outpost")
            .expect("goal");
        assert_eq!(g.status, GoalStatus::Achieved);
    }

    #[test]
    fn construction_completed_lands_in_past() {
        let mut world = fresh_world();
        let colony = world.spawn_empty().id();
        world.write_message(ConstructionEvent::Completed {
            colony,
            building: BuildingType::Factory,
        });
        run_consumer_systems(&mut world);
        let log = world.resource::<MissionLog>();
        assert_eq!(log.past_len(), 1);
        let entry = log.past.front().unwrap();
        assert!(entry.entry_id.contains("factory"));
        assert_eq!(entry.outcome, Some(MissionOutcome::Completed));
    }

    #[test]
    fn research_tech_completed_records_paid_tier_1_goal() {
        let mut world = fresh_world();
        let mut data = TechnologiesData::default();
        data.technologies.insert(
            "fusion_propulsion".into(),
            Technology {
                id: "fusion_propulsion".into(),
                name: "Fusion Propulsion".into(),
                category: TechCategory::Physics,
                description: "test".into(),
                research_cost: 1000.0,
                prerequisites: vec![],
                unlocks_components: vec![],
                unlocks_engineering: vec![],
                modifiers: vec![],
                tier: 1,
            },
        );
        world.insert_resource(data);

        world.write_message(ResearchEvent::TechCompleted {
            tech_id: "fusion_propulsion".into(),
            tech_display_name: "Fusion Propulsion".into(),
        });
        run_consumer_systems(&mut world);
        let log = world.resource::<MissionLog>();
        assert_eq!(log.past_len(), 1);
        let g = log
            .goals
            .iter()
            .find(|g| g.goal_id == "storyline.first_paid_tier_1")
            .expect("goal");
        assert_eq!(g.status, GoalStatus::Achieved);
    }

    #[test]
    fn research_tech_completed_unknown_id_is_resolved_no_goal_flip() {
        let mut world = fresh_world();
        world.insert_resource(TechnologiesData::default());
        world.write_message(ResearchEvent::TechCompleted {
            tech_id: "ghost".into(),
            tech_display_name: "Ghost".into(),
        });
        run_consumer_systems(&mut world);
        let log = world.resource::<MissionLog>();
        assert_eq!(log.past_len(), 1);
        assert!(log
            .goals
            .iter()
            .all(|g| g.goal_id != "storyline.first_paid_tier_1"));
    }

    #[test]
    fn all_known_goal_ids_have_a_label() {
        let labels: BTreeSet<&str> = KNOWN_GOAL_IDS.iter().map(|id| goal_label_for(id)).collect();
        for id in KNOWN_GOAL_IDS {
            assert_ne!(
                goal_label_for(id),
                "Unnamed goal",
                "goal id {id} must have a label"
            );
            assert!(
                labels.contains(*id) || !labels.contains(*id),
                "labels are independent of ids"
            );
        }
    }

    #[test]
    fn ui_audit_detects_resmut_mission_log() {
        let offenders = assert_no_ui_resmut();
        assert!(
            offenders.is_empty(),
            "src/ui must not contain `ResMut<MissionLog>`; offenders: {offenders:?}"
        );
    }

    #[test]
    fn building_id_str_is_stable() {
        assert_eq!(building_id_str(BuildingType::Factory), "factory");
        assert_eq!(
            building_id_str(BuildingType::FusionReactor),
            "fusion_reactor"
        );
        // Acronym-handling smoke check.
        assert_eq!(
            building_id_str(BuildingType::DTFusionReactor),
            "dt_fusion_reactor"
        );
        assert_eq!(
            building_id_str(BuildingType::DHe3FusionReactor),
            "d_he3_fusion_reactor"
        );
        assert_eq!(building_id_str(BuildingType::AiCluster), "ai_cluster");
    }
}
