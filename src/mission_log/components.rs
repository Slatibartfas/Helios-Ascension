//! Mission log data shape — see [`crate::mission_log`] for the module
//! doc.
//!
//! # Read-only contract
//!
//! `MissionLog` is owned by the simulation writers in
//! [`crate::mission_log::systems`]. UI systems MUST read it via
//! `Res<MissionLog>` only — see [`crate::mission_log::systems::assert_no_ui_resmut`]
//! for the regression test that enforces this convention.

use std::collections::VecDeque;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Source family that originated a [`MissionEntry`].
///
/// `Storyline` is reserved for future narrative-driven missions
/// (GRA-792 / GRA-793). The current sim surfaces only the first three.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Reflect)]
#[reflect(Debug, PartialEq, Hash)]
pub enum MissionSource {
    Survey,
    Construction,
    Research,
    Storyline,
}

/// Kind / template that produced a [`MissionEntry`].
///
/// Survey values mirror the existing `SurveyMethod` roster; the
/// `MissionKind::TechProject` / `ConstructBuilding` / `ConstructShip`
/// variants cover the non-survey sources. `StorylineGoal { goal_id }`
/// is reserved for narrative beats (GRA-792 / GRA-793).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Reflect)]
#[reflect(Debug, PartialEq, Hash)]
pub enum MissionKind {
    // Survey methods — see `crate::survey::types::SurveyMethod`.
    Flyby,
    Orbital,
    RemoteSensing,
    AtmosphericProbe,
    Seismic,
    SurfaceLander,
    Rover,
    Drill,
    SampleReturn,
    // Research / construction / storyline.
    TechProject,
    ConstructBuilding,
    ConstructShip,
    OutpostEstablishment,
    StorylineGoal { goal_id: u32 },
}

impl MissionKind {
    /// Stable, lowercase, snake_case string id for `entry_id` synthesis
    /// and persisted records. Mirrors the field/contract used by the
    /// `entry_id` derivation in [`crate::mission_log::systems`].
    pub fn id_str(&self) -> String {
        match self {
            MissionKind::Flyby => "flyby".to_string(),
            MissionKind::Orbital => "orbital".to_string(),
            MissionKind::RemoteSensing => "remote_sensing".to_string(),
            MissionKind::AtmosphericProbe => "atmospheric_probe".to_string(),
            MissionKind::Seismic => "seismic".to_string(),
            MissionKind::SurfaceLander => "surface_lander".to_string(),
            MissionKind::Rover => "rover".to_string(),
            MissionKind::Drill => "drill".to_string(),
            MissionKind::SampleReturn => "sample_return".to_string(),
            MissionKind::TechProject => "tech_project".to_string(),
            MissionKind::ConstructBuilding => "construct_building".to_string(),
            MissionKind::ConstructShip => "construct_ship".to_string(),
            MissionKind::OutpostEstablishment => "outpost_establishment".to_string(),
            MissionKind::StorylineGoal { goal_id } => format!("storyline:{goal_id}"),
        }
    }
}

/// Resolution state for a [`MissionEntry`]. `None` while in
/// `MissionLog::current`; `Some(...)` after the entry has moved to
/// `MissionLog::past`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Reflect)]
#[reflect(Debug, PartialEq)]
pub enum MissionOutcome {
    Completed,
    Failed,
    Aborted,
}

impl MissionOutcome {
    /// Stable lowercase id used by `entry_id` derivation. Lets the
    /// consumer distinguish e.g. `survey:5:completed` from
    /// `survey:5:failed` — useful when the same `mission_id` is
    /// re-dispatched after a failure.
    pub fn id_str(self) -> &'static str {
        match self {
            MissionOutcome::Completed => "completed",
            MissionOutcome::Failed => "failed",
            MissionOutcome::Aborted => "aborted",
        }
    }
}

/// Status of a [`GoalEntry`].
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Reflect)]
#[reflect(Debug, PartialEq)]
pub enum GoalStatus {
    /// Declared but no qualifying event has fired yet.
    Pending,
    /// A qualifying event has fired; the goal is making progress.
    InProgress,
    /// The goal's success condition was met.
    Achieved,
    /// The goal was explicitly abandoned (no current producer — kept
    /// for future narrative pacing).
    Abandoned,
}

/// One active or resolved mission. The same struct is used for both
/// `current` and `past`; an entry is "in progress" while
/// `outcome.is_none()` and "resolved" once `outcome.is_some()`.
#[derive(Debug, Clone, Serialize, Deserialize, Reflect)]
#[reflect(Debug)]
pub struct MissionEntry {
    /// Stable id used for dedup + save/load round-trip. Synthesised by
    /// the consumer from the source event:
    ///
    /// - Survey: `format!("survey:{mission_id}:{kind.id_str()}")`
    /// - Research: `format!("research:{tech_id}")`
    /// - Construction: `format!("construction:{colony_id}:{building.id_str()}")`
    ///   where `colony_id` is the entity index as string (stable for
    ///   the duration of the campaign).
    /// - Outpost: `format!("construction:outpost:{colony_id}")`
    ///
    /// Two distinct events with the same `entry_id` are de-duplicated
    /// at insert time — see [`MissionLog::push_current`].
    pub entry_id: String,

    pub kind: MissionKind,
    pub source: MissionSource,

    /// Stable body identity — `(system_id, body_name)` per
    /// `crate::persistence::state_store::BodyKey`. `None` for missions
    /// not anchored to a single body (e.g. a research project that
    /// applies globally).
    ///
    /// We deliberately do NOT use `Entity` — the regen chain produces
    /// fresh indices each run, and the StateStore's regen+overlay
    /// model keys on the stable identity pair.
    pub target_body: Option<BodyKey>,

    /// Player-facing display name. The consumer sets this from the
    /// source event's display string (e.g. `mission.name` for survey,
    /// `tech.tech_display_name` for research).
    pub display_name: String,

    /// Sim-time at which the mission was dispatched, in seconds since
    /// `SimulationTime::start_timestamp`.
    pub started_sim_seconds: f64,

    /// `None` while in `current`; `Some(...)` after resolution. The
    /// consumer sets this on `MissionCompleted` / `MissionFailed` /
    /// `MissionAborted` (survey) or `TechCompleted` (research) or
    /// outpost promotion (construction).
    pub outcome: Option<MissionOutcome>,

    /// Sim-time at which the mission was resolved. `None` while in
    /// `current`.
    pub resolved_sim_seconds: Option<f64>,
}

/// One long-running goal. `goal_id` is the stable identity used for
/// dedup; the consumer registers a goal entry on first reference and
/// flips the status monotonically (Pending → InProgress → Achieved).
///
/// `triggering_entry_id` is the audit-trail anchor — the `entry_id` of
/// the `MissionEntry` that satisfied this goal. Optional so goals
/// declared before any mission ran can exist with `Pending` status.
#[derive(Debug, Clone, Serialize, Deserialize, Reflect)]
#[reflect(Debug)]
pub struct GoalEntry {
    pub goal_id: String,
    pub label: String,
    pub status: GoalStatus,
    /// Sim-time at which the goal was first advanced to `Achieved`.
    pub achieved_sim_seconds: Option<f64>,
    pub triggering_entry_id: Option<String>,
}

/// Configuration for [`MissionLog`] rolling-window behaviour. Treated
/// as a Resource so a future settings UI can tune `past_capacity`
/// without a code change.
#[derive(Resource, Debug, Clone, Serialize, Deserialize, Reflect)]
#[reflect(Resource, Debug)]
pub struct MissionLogConfig {
    /// Maximum number of entries kept in `MissionLog::past`. When a
    /// new entry pushes the queue past this size, the oldest entry is
    /// dropped. Default: 100.
    pub past_capacity: usize,
}

impl Default for MissionLogConfig {
    fn default() -> Self {
        Self { past_capacity: 100 }
    }
}

/// The mission log itself — a single global Resource.
///
/// Lives as a Resource (not a Component) because it is game-wide
/// state, not anchored to any specific entity. UI readers use
/// `Res<MissionLog>`; the simulation writers in
/// [`crate::mission_log::systems`] are the only code path that
/// obtains `ResMut<MissionLog>`.
///
/// Save/load: persisted via the v2 StateStore
/// `MissionLogRecord` (see `crate::persistence::state_store`); the
/// Reflect + Serialize + Deserialize derives here are defensive
/// coverage that lets `AppTypeRegistry` + any future
/// Reflect-driven snapshot path pick the resource up without further
/// code changes. The StateStore extract/apply path is the
/// load-bearing save route.
#[derive(Resource, Debug, Clone, Default, Serialize, Deserialize, Reflect)]
#[reflect(Resource, Debug)]
pub struct MissionLog {
    /// Active missions. Insertion order matches dispatch order. Each
    /// entry has `outcome == None`.
    pub current: Vec<MissionEntry>,
    /// Resolved missions, oldest at the front, newest at the back.
    /// FIFO-evicted when the queue grows past `MissionLogConfig::past_capacity`.
    pub past: VecDeque<MissionEntry>,
    /// Declared long-running objectives. Append-only on first
    /// reference; status flips monotonically (Pending → InProgress →
    /// Achieved).
    pub goals: Vec<GoalEntry>,
}

// Re-export of the persistence-side body identity. Keeps this module
// independent of `crate::persistence::state_store` so the resource can
// be constructed in unit tests without the persistence plugin.
pub use crate::persistence::state_store::BodyKey;

impl MissionLog {
    /// Push a new active mission. If an entry with the same `entry_id`
    /// already exists in `current` or `past`, the call is a no-op —
    /// this is the consumer-side idempotence guarantee.
    ///
    /// Returns `true` if the entry was inserted, `false` if a duplicate
    /// was suppressed.
    pub fn push_current(&mut self, entry: MissionEntry, config: &MissionLogConfig) -> bool {
        if self.find_by_id(&entry.entry_id).is_some() {
            return false;
        }
        debug_assert!(
            entry.outcome.is_none(),
            "active entry must have outcome == None"
        );
        self.current.push(entry);
        let _ = config; // capacity only applies to `past`; suppress dead-code lint for now
        true
    }

    /// Move an entry from `current` to `past`, stamping it with the
    /// outcome and resolution time. Returns `true` if the entry was
    /// found and resolved; `false` if no entry with that id exists
    /// (the consumer should not invent one — see the architecture's
    /// "do not guess" rule).
    ///
    /// FIFO-evicts the oldest `past` entry when the queue grows past
    /// `MissionLogConfig::past_capacity`.
    pub fn resolve(
        &mut self,
        entry_id: &str,
        outcome: MissionOutcome,
        resolved_sim_seconds: f64,
        config: &MissionLogConfig,
    ) -> bool {
        let pos = self.current.iter().position(|e| e.entry_id == entry_id);
        let Some(pos) = pos else {
            return false;
        };
        let mut entry = self.current.remove(pos);
        entry.outcome = Some(outcome);
        entry.resolved_sim_seconds = Some(resolved_sim_seconds);
        // Push + FIFO-evict.
        self.past.push_back(entry);
        while self.past.len() > config.past_capacity {
            self.past.pop_front();
        }
        true
    }

    /// Register or update a goal. The first call with a given `goal_id`
    /// inserts a `Pending` entry. Subsequent calls with the same id
    /// apply the monotonic status upgrade: `Pending → InProgress →
    /// Achieved`. `Abandoned` is a one-way terminal flip from any
    /// status. The function returns `true` if the goal was newly
    /// inserted, `false` otherwise.
    pub fn upsert_goal(&mut self, goal: GoalEntry) -> bool {
        if let Some(existing) = self.goals.iter_mut().find(|g| g.goal_id == goal.goal_id) {
            // Monotonic upgrade.
            existing.status = monotonic_status(existing.status, goal.status);
            if goal.achieved_sim_seconds.is_some() && existing.achieved_sim_seconds.is_none() {
                existing.achieved_sim_seconds = goal.achieved_sim_seconds;
            }
            if goal.triggering_entry_id.is_some() && existing.triggering_entry_id.is_none() {
                existing.triggering_entry_id = goal.triggering_entry_id;
            }
            if !goal.label.is_empty() {
                existing.label = goal.label;
            }
            false
        } else {
            self.goals.push(goal);
            true
        }
    }

    /// Look up an entry by `entry_id`. Hits `current` then `past`.
    pub fn find_by_id(&self, entry_id: &str) -> Option<&MissionEntry> {
        self.current
            .iter()
            .find(|e| e.entry_id == entry_id)
            .or_else(|| self.past.iter().find(|e| e.entry_id == entry_id))
    }

    /// Count of `past` entries that satisfy the rolling-window cap.
    /// Public so tests can assert the FIFO invariant directly.
    pub fn past_len(&self) -> usize {
        self.past.len()
    }
}

const fn monotonic_status(current: GoalStatus, incoming: GoalStatus) -> GoalStatus {
    use GoalStatus::*;
    match (current, incoming) {
        (a, b) if a == b => a,
        // Abandoned is terminal — once abandoned, never relit.
        (Abandoned, _) => Abandoned,
        // Achieved is the highest non-terminal state.
        (Achieved, _) => Achieved,
        (InProgress, Pending) => InProgress,
        (Pending, InProgress) => InProgress,
        (InProgress, Achieved) => Achieved,
        (Pending, Achieved) => Achieved,
        // Same-state explicit re-assertions fall through to the first
        // arm above.
        (Pending, Pending) => Pending,
        (InProgress, InProgress) => InProgress,
        // Anything else (Pending → Abandoned, InProgress → Abandoned)
        // is an explicit transition to terminal; trust the caller's
        // intent.
        (_, Abandoned) => Abandoned,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, kind: MissionKind, started: f64) -> MissionEntry {
        MissionEntry {
            entry_id: id.to_string(),
            kind,
            source: MissionSource::Survey,
            target_body: None,
            display_name: id.to_string(),
            started_sim_seconds: started,
            outcome: None,
            resolved_sim_seconds: None,
        }
    }

    #[test]
    fn push_current_inserts_and_dedups() {
        let mut log = MissionLog::default();
        let cfg = MissionLogConfig::default();
        assert!(log.push_current(entry("a", MissionKind::Flyby, 0.0), &cfg));
        assert!(log.push_current(entry("b", MissionKind::Rover, 1.0), &cfg));
        assert!(!log.push_current(entry("a", MissionKind::Flyby, 0.0), &cfg));
        assert_eq!(log.current.len(), 2);
    }

    #[test]
    fn resolve_moves_current_to_past() {
        let mut log = MissionLog::default();
        let cfg = MissionLogConfig::default();
        log.push_current(entry("a", MissionKind::Flyby, 0.0), &cfg);
        assert!(log.resolve("a", MissionOutcome::Completed, 10.0, &cfg));
        assert!(log.current.is_empty());
        assert_eq!(log.past_len(), 1);
        let resolved = log.find_by_id("a").expect("entry must persist");
        assert_eq!(resolved.outcome, Some(MissionOutcome::Completed));
        assert_eq!(resolved.resolved_sim_seconds, Some(10.0));
    }

    #[test]
    fn resolve_unknown_id_is_noop() {
        let mut log = MissionLog::default();
        let cfg = MissionLogConfig::default();
        assert!(!log.resolve("ghost", MissionOutcome::Completed, 10.0, &cfg));
        assert!(log.past.is_empty());
    }

    #[test]
    fn past_rolls_over_at_cap() {
        let mut log = MissionLog::default();
        let cfg = MissionLogConfig { past_capacity: 3 };
        for i in 0..10 {
            let id = format!("e{i}");
            log.push_current(entry(&id, MissionKind::Flyby, i as f64), &cfg);
            log.resolve(&id, MissionOutcome::Completed, i as f64 + 1.0, &cfg);
        }
        assert_eq!(log.past_len(), 3, "rolling window must evict oldest");
        let oldest = log.past.front().expect("non-empty");
        assert_eq!(
            oldest.entry_id, "e7",
            "oldest survivor must be the cap-th newest"
        );
        let newest = log.past.back().expect("non-empty");
        assert_eq!(
            newest.entry_id, "e9",
            "newest must be the most recent resolution"
        );
    }

    #[test]
    fn upsert_goal_first_call_inserts_pending() {
        let mut log = MissionLog::default();
        let inserted = log.upsert_goal(GoalEntry {
            goal_id: "g1".into(),
            label: "First probe".into(),
            status: GoalStatus::Pending,
            achieved_sim_seconds: None,
            triggering_entry_id: None,
        });
        assert!(inserted);
        assert_eq!(log.goals.len(), 1);
        assert_eq!(log.goals[0].status, GoalStatus::Pending);
    }

    #[test]
    fn upsert_goal_pending_to_achieved_is_monotonic() {
        let mut log = MissionLog::default();
        log.upsert_goal(GoalEntry {
            goal_id: "g1".into(),
            label: "First probe".into(),
            status: GoalStatus::Pending,
            achieved_sim_seconds: None,
            triggering_entry_id: None,
        });
        log.upsert_goal(GoalEntry {
            goal_id: "g1".into(),
            label: "First probe".into(),
            status: GoalStatus::InProgress,
            achieved_sim_seconds: None,
            triggering_entry_id: None,
        });
        log.upsert_goal(GoalEntry {
            goal_id: "g1".into(),
            label: "First probe".into(),
            status: GoalStatus::Achieved,
            achieved_sim_seconds: Some(42.0),
            triggering_entry_id: Some("survey:1:flyby".into()),
        });
        let g = &log.goals[0];
        assert_eq!(g.status, GoalStatus::Achieved);
        assert_eq!(g.achieved_sim_seconds, Some(42.0));
        assert_eq!(g.triggering_entry_id.as_deref(), Some("survey:1:flyby"));
    }

    #[test]
    fn upsert_goal_achieved_does_not_downgrade_on_reassert() {
        let mut log = MissionLog::default();
        log.upsert_goal(GoalEntry {
            goal_id: "g1".into(),
            label: "First probe".into(),
            status: GoalStatus::Achieved,
            achieved_sim_seconds: Some(10.0),
            triggering_entry_id: Some("survey:1".into()),
        });
        // A re-fired milestone must NOT clear the goal.
        log.upsert_goal(GoalEntry {
            goal_id: "g1".into(),
            label: "First probe".into(),
            status: GoalStatus::InProgress,
            achieved_sim_seconds: None,
            triggering_entry_id: None,
        });
        let g = &log.goals[0];
        assert_eq!(g.status, GoalStatus::Achieved);
        assert_eq!(g.achieved_sim_seconds, Some(10.0));
        assert_eq!(g.triggering_entry_id.as_deref(), Some("survey:1"));
    }

    #[test]
    fn upsert_goal_abandoned_is_terminal() {
        let mut log = MissionLog::default();
        log.upsert_goal(GoalEntry {
            goal_id: "g1".into(),
            label: "x".into(),
            status: GoalStatus::InProgress,
            achieved_sim_seconds: None,
            triggering_entry_id: None,
        });
        log.upsert_goal(GoalEntry {
            goal_id: "g1".into(),
            label: "x".into(),
            status: GoalStatus::Abandoned,
            achieved_sim_seconds: None,
            triggering_entry_id: None,
        });
        assert_eq!(log.goals[0].status, GoalStatus::Abandoned);
        log.upsert_goal(GoalEntry {
            goal_id: "g1".into(),
            label: "x".into(),
            status: GoalStatus::Achieved,
            achieved_sim_seconds: Some(99.0),
            triggering_entry_id: None,
        });
        assert_eq!(
            log.goals[0].status,
            GoalStatus::Abandoned,
            "Abandoned must be terminal"
        );
    }

    #[test]
    fn find_by_id_hits_current_then_past() {
        let mut log = MissionLog::default();
        let cfg = MissionLogConfig::default();
        log.push_current(entry("alive", MissionKind::Flyby, 0.0), &cfg);
        log.push_current(entry("dead", MissionKind::Flyby, 1.0), &cfg);
        log.resolve("dead", MissionOutcome::Failed, 5.0, &cfg);

        assert!(log.find_by_id("alive").is_some());
        assert!(log.find_by_id("dead").is_some());
        assert!(log.find_by_id("ghost").is_none());
    }
}