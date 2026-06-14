//! Survey messages — Bevy 0.18 `Message` types emitted by the survey
//! systems and consumed by the notification surface.
//!
//! PR-C (GRA-81) adds the three anomaly variants on the `Anomaly*`
//! event family: `AnomalyDetected` (a new candidate on a body's
//! dossier), `AnomalyActivated` (confidence crossed the threshold),
//! and `AnomalyRefuted` (a contradicting verification mission
//! dropped confidence below re-arm).
//!
//! PR-B (GRA-80) wires up the mission state machine and fires
//! `Mission*` events for every state transition. The dossier UI
//! and the notification surface subscribe via
//! `MessageReader<SurveyEvent>`.
//!
//! Dispatch and abort are modelled as separate Bevy events so the
//! UI can enqueue them without touching the mission's storage
//! directly — see the [action-queue decoupling] rule in AGENTS.md.

use bevy::prelude::*;

use crate::personnel::types::ScientistId;

use super::types::{AnomalyType, MissionFailureReason, SurveyMethod};

/// All state transitions the survey mission system emits.
///
/// `body` is the celestial body the mission is targeting;
/// `mission_id` matches
/// [`ActiveSurveyMission::id`](crate::survey::components::ActiveSurveyMission::id).
///
/// Events are transient — they live in a Bevy `Messages<SurveyEvent>`
/// buffer and are dropped between game sessions. No `Serialize` /
/// `Deserialize` derive is needed; the entity references are valid
/// only for the lifetime of the in-memory world.
#[derive(Message, Debug, Clone)]
pub enum SurveyEvent {
    // ── PR-C anomaly confidence events ──────────────────────────
    /// A new anomaly was logged on the body. Fires when the
    /// `surface_anomaly_events` system rolls a successful
    /// detection past the false-positive check.
    AnomalyDetected {
        body: Entity,
        anomaly: AnomalyType,
        /// Initial confidence (always 0.10 × axis_match_count per the
        /// r2 model).
        initial_confidence: f32,
    },
    /// Confidence crossed the activation threshold and the anomaly
    /// transitioned to `Verified`. The effect is also applied.
    AnomalyActivated {
        body: Entity,
        anomaly: AnomalyType,
        confidence: f32,
    },
    /// A contradicting verification mission dropped confidence below
    /// the re-arm threshold. The anomaly moves to `Dormant` or
    /// `Suspected`.
    AnomalyRefuted { body: Entity, anomaly: AnomalyType },
    // ── PR-B mission lifecycle events ───────────────────────────
    /// A new mission has been dispatched and entered the `Queued`
    /// state. The dossier UI uses this to flash a "Mission
    /// dispatched" notification.
    MissionStarted {
        body: Entity,
        mission_id: u64,
        name: String,
        method: SurveyMethod,
    },
    /// A mission completed successfully. The dimensional tiers on
    /// the body's [`SurveyState`](crate::survey::components::SurveyState)
    /// have been advanced.
    MissionCompleted {
        body: Entity,
        mission_id: u64,
        name: String,
        method: SurveyMethod,
    },
    /// A mission failed. The reason is in `reason`. Dimensional
    /// progress is partially retained (the partial-progress policy
    /// is the call of a follow-up design pass — for PR-B, all
    /// progress is rolled back to the pre-mission snapshot).
    MissionFailed {
        body: Entity,
        mission_id: u64,
        name: String,
        method: SurveyMethod,
        reason: MissionFailureReason,
    },
    /// A mission was aborted by the player. Distinct from
    /// `MissionFailed` because the player's intent was explicit;
    /// the UI may want to label these "ABORTED" rather than
    /// "FAILED" in the dossier history.
    MissionAborted {
        body: Entity,
        mission_id: u64,
        name: String,
        method: SurveyMethod,
    },
    /// The probe / atmospheric probe was lost (5% on probe-using
    /// methods). Companion to `MissionFailed` with reason
    /// `ProbeLoss`.
    ProbeLost {
        body: Entity,
        mission_id: u64,
        name: String,
    },
    /// The rover became stuck (8% on Rover). Companion to
    /// `MissionFailed` with reason `RoverStuck`.
    RoverStuck {
        body: Entity,
        mission_id: u64,
        name: String,
    },
    /// The drill bit jammed (10% on Drill). Companion to
    /// `MissionFailed` with reason `DrillBitStuck`.
    DrillBitStuck {
        body: Entity,
        mission_id: u64,
        name: String,
    },
    /// A crew member was injured on a ground-team mission (2% on
    /// SurfaceLander, Rover, Drill, SampleReturn). The scientist
    /// transitions to `Injured { remaining_days: 90 }` and is
    /// blocked from new survey-mission assignments.
    CrewInjured {
        body: Entity,
        mission_id: u64,
        name: String,
        scientist: ScientistId,
        scientist_name: String,
        /// Sim-time the injury expires (90 sim-days after the
        /// failure event).
        injured_until_sim_time: f64,
    },
    // ── GRA-120: pre-dispatch gate failures ─────────────────────
    /// The dispatcher rejected the [`DispatchSurveyMission`] event
    /// because the template's gate(s) were not satisfied. The
    /// mission is NOT added to the body's `active_missions` and
    /// the assigned scientists (if any) are not bound. The dossier
    /// UI subscribes to this event to surface a one-line toast
    /// naming the `reason` — the visual toast itself is a
    /// follow-on UI PR; this PR only guarantees the event is
    /// emitted in place of the previous silent `warn!` + drop.
    ///
    /// GRA-120 (mission dispatch gates): the same gate also
    /// applies to legacy `warn!` sites that pre-date this event
    /// — the dispatcher is migrating drop sites one at a time,
    /// starting with the gate failures introduced by the new
    /// `SurveyMissionTemplate` fields. Sites that are not
    /// gate-related (e.g. body-missing `SurveyState`, template
    /// unknown) keep the warn! + drop behaviour for now and
    /// are routed to the matching `MissionLaunchReason` variant
    /// below.
    MissionLaunchBlocked {
        body: Entity,
        /// Matches the request's `name` (the dispatch UI's
        /// auto-generated mission name). `mission_id` is `0` for
        /// launch-blocked events because the mission was never
        /// instantiated — consumers that key off `mission_id`
        /// should fall back to `name` + `body` when the id is
        /// zero.
        mission_id: u64,
        name: String,
        method: SurveyMethod,
        reason: MissionLaunchReason,
    },
}

/// Why a [`SurveyEvent::MissionLaunchBlocked`] event was emitted.
///
/// One variant per dispatch-time gate (or per legacy warn! site
/// that has been migrated to the event). New gates should add a
/// new variant here; the dossier UI's toast layer maps each
/// variant to a one-line human-readable string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MissionLaunchReason {
    /// The template's `min_assigned_scientists` was not met.
    /// Ground-team templates (Rover, Drill, SampleReturn) reach
    /// this path when the dossier dispatched with an empty
    /// scientist team. The legacy `is_ground_team && empty` warn
    /// in `dispatch_survey_mission` now routes here.
    NoScientists,
    /// The template's `requires_ship_class` / `requires_min_ship_count`
    /// gate was not met. Currently a world-wide count of matching
    /// `ShipTemplateRef` entities (per-body ship inventory at a
    /// starmap location is a follow-on to GRA-120 — see the
    /// `count_ships_with_hull_class` TODO in `src/survey/systems.rs`).
    /// In practice this means a fresh game-state with no freighters
    /// yet reports the gate as unsatisfied rather than silently
    /// passing; once the per-body inventory lands the reason text
    /// should be re-tightened to "at the target body's starmap
    /// location" wording.
    NoShipAvailable,
    /// The template id does not exist in the
    /// [`SurveyMissionTemplates`](crate::survey::data::SurveyMissionTemplates)
    /// resource. Almost always a UI bug (stale RON id, typo) —
    /// surfaced as an event so the dossier can flag the
    /// "RECOMMENDED" CTA as broken rather than silently
    /// dispatching nothing.
    TemplateUnknown,
    /// One of the assigned scientists is in the `Injured` state
    /// and cannot accept new missions until
    /// `INJURY_DURATION_DAYS` have elapsed. The legacy
    /// `s.is_injured(sim_time)` warn site now routes here.
    ScientistInjured,
    /// One of the assigned scientists already has a
    /// `current_survey_mission` set. The legacy
    /// `s.current_survey_mission.is_some()` warn site now
    /// routes here.
    ScientistOnOtherMission,
    /// The body the dispatch targeted does not have a
    /// `SurveyState` component AND did not have a legacy
    /// `SurveyLevel` either. The system populator fills in
    /// `SurveyLevel` for celestial bodies, so this should not
    /// fire in normal play — kept as an explicit event variant
    /// so the dossier's missing-state guard is observable
    /// from the event stream.
    BodyMissingSurveyState,
    /// The referenced scientist id was not found in the
    /// `Scientist` component query. Distinct from
    /// `ScientistInjured` (the scientist exists but is
    /// unavailable) — this is a desync between the UI's
    /// roster snapshot and the live `Scientist` set (e.g. a
    /// scientist was deleted between menu-open and dispatch).
    ScientistMissing,
}

/// Dispatch a survey mission. Fired by the dossier UI's
/// "DISPATCH MISSION" button (or, in PR-B's test suite, by
/// `World::write_message`). Consumed by
/// [`dispatch_survey_mission`](crate::survey::systems::dispatch_survey_mission).
#[derive(Message, Debug, Clone)]
pub struct DispatchSurveyMission {
    /// Celestial body the mission targets. Must have a
    /// [`SurveyState`](crate::survey::components::SurveyState)
    /// component.
    pub body: Entity,
    /// RON id of the [`SurveyMissionTemplate`](
    /// crate::survey::data::SurveyMissionTemplate) to instantiate.
    /// Must exist in the
    /// [`SurveyMissionTemplates`](crate::survey::data::SurveyMissionTemplates)
    /// resource.
    pub template_id: String,
    /// Display name to assign to the new mission (e.g.
    /// "Mare Imbrium 1"). The UI generates this from a counter
    /// before firing the event.
    pub name: String,
    /// Scientists assigned to the mission. Empty for solo probe
    /// missions (Flyby, Orbital). For ground-team missions the
    /// event is dropped at dispatch time if this is empty.
    pub scientist_ids: Vec<ScientistId>,
}

/// Abort a survey mission. Fired by the dossier UI's "ABORT"
/// button. The mission is removed from
/// [`SurveyState::active_missions`](crate::survey::components::SurveyState::active_missions),
/// any assigned scientists are freed, and a `MissionFailed` event
/// with reason `Aborted` is fired.
#[derive(Message, Debug, Clone)]
pub struct AbortSurveyMission {
    pub body: Entity,
    pub mission_id: u64,
}

/// Dismiss a failed-mission notification from the dossier
/// "FAILED MISSIONS" section. Fired by the dossier UI's
/// "ACCEPT LOSS" button (PR-G, GRA-85). The corresponding
/// [`FailedMissionRecord`](crate::survey::components::FailedMissionRecord)
/// is removed from the body's
/// [`SurveyState::failed_mission_notifications`](crate::survey::components::SurveyState::failed_mission_notifications)
/// vec; if the record's `recovery_mission_active_id` is set,
/// the linked recovery mission is left running in the body's
/// `active_missions` (the player is signalling "I accept the
/// failed mission's data loss; the recovery is still in
/// flight").
#[derive(Message, Debug, Clone)]
pub struct DismissFailedMission {
    pub body: Entity,
    /// Matches
    /// [`FailedMissionRecord::mission_id`](crate::survey::components::FailedMissionRecord::mission_id).
    pub mission_id: u64,
}

/// Dismiss a completed mission from the dossier ACTIVE MISSIONS
/// list. PR-F (GRA-117) — terminal missions (Succeeded / Failed
/// recovery paths) linger in `active_missions` for a few sim-days
/// so the player can read the result and decide what to do next.
/// Hitting "DISMISS" hides the mission immediately; the next
/// auto-archive sweep also removes any mission that has been
/// terminal for more than [`ARCHIVE_LINGER_DAYS`](crate::survey::systems::ARCHIVE_LINGER_DAYS).
///
/// The handler sets `mission.dismissed = true` and the dossier
/// filters dismissed missions out of the default render. The
/// mission is physically removed on the next archive sweep (a
/// "soft delete" — keeps the data on disk through the rest of the
/// current Update tick in case another system reads it).
#[derive(Message, Debug, Clone)]
pub struct DismissSurveyMission {
    pub body: Entity,
    /// Matches
    /// [`ActiveSurveyMission::id`](crate::survey::components::ActiveSurveyMission::id).
    pub mission_id: u64,
}
