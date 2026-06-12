//! Personnel components.
//!
//! PR-A scaffold: type definitions only. Spawning, hiring,
//! promotion, and assignment systems land in PR-C.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::types::{ScientistId, ScientistSpecialty, SeniorityTier};

/// A scientist entity. The analysis queue (PR-C) drives
/// `current_analysis` and updates `lifetime_data_processed` /
/// `lifetime_anomalies_flagged` on job completion. PR-B (GRA-80)
/// adds the survey-mission assignment and injury fields.
#[derive(Debug, Clone, Component, Serialize, Deserialize)]
pub struct Scientist {
    /// Stable id, used by the analysis queue to index this scientist
    /// in [`AnalysisQueueIndex`](crate::survey::data::AnalysisQueueIndex).
    pub id: ScientistId,
    /// Display name (e.g. "Dr. R. Vasquez"). The Personnel menu
    /// lists scientists by name.
    pub name: String,
    /// Primary specialty. Drives the match/mismatch multiplier in
    /// [`ScientistSpecialty::matches_method`].
    pub specialty: ScientistSpecialty,
    /// Current seniority tier. Promoted by
    /// `seniority_promotion` (PR-C) after a configurable number of
    /// successful analysis jobs.
    pub seniority: SeniorityTier,
    /// Body the scientist is currently assigned to. `None` if the
    /// scientist is unassigned (idle in the roster).
    pub assigned_body: Option<Entity>,
    /// Active survey mission the scientist is currently attached to.
    /// Set by the mission dispatch system (PR-B) and cleared on
    /// mission completion (success, failure, or abort). `None` when
    /// the scientist is idle in the roster.
    ///
    /// Distinct from `current_analysis`: a scientist on a survey
    /// mission collects data in the field, then (PR-C) processes it
    /// on the analysis queue. The two states are sequential, not
    /// concurrent — a scientist that just finished a survey mission
    /// becomes available for the analysis queue.
    #[serde(default)]
    pub current_survey_mission: Option<u64>,
    /// Active analysis job. `None` if idle. PR-C drives this.
    pub current_analysis: Option<u64>,
    /// Sim-time the scientist is uninjured. While `current_time <
    /// injured_until_sim_time`, the scientist is blocked from new
    /// survey mission assignments (PR-B sets this on crew-injury
    /// failure; `injured_until = current_time + 90 sim_days`).
    #[serde(default)]
    pub injured_until_sim_time: Option<f64>,
    /// Cumulative megabytes (or gigabytes — units are in
    /// `survey_datasets.ron`) of data processed across the
    /// scientist's career. Used for promotion gating and the
    /// Personnel menu's career stats column.
    pub lifetime_data_processed: f64,
    /// Number of anomalies flagged by this scientist's analyses.
    /// Used for promotion gating and the Personnel menu's
    /// achievements column.
    pub lifetime_anomalies_flagged: u32,
    /// Sim-time the scientist was hired.
    pub hired_sim_time: f64,
}

impl Scientist {
    /// Construct a freshly-hired junior scientist. Used by the
    /// University building's `hire_scientists` system (PR-C).
    pub fn new_junior(
        id: ScientistId,
        name: String,
        specialty: ScientistSpecialty,
        sim_time: f64,
    ) -> Self {
        Self {
            id,
            name,
            specialty,
            seniority: SeniorityTier::Junior,
            assigned_body: None,
            current_survey_mission: None,
            current_analysis: None,
            injured_until_sim_time: None,
            lifetime_data_processed: 0.0,
            lifetime_anomalies_flagged: 0,
            hired_sim_time: sim_time,
        }
    }

    /// Whether the scientist is currently idle (no body assigned,
    /// no analysis in progress, no active survey mission).
    pub fn is_idle(&self) -> bool {
        self.assigned_body.is_none()
            && self.current_analysis.is_none()
            && self.current_survey_mission.is_none()
    }

    /// Whether the scientist is injured at `sim_time`. An injury
    /// blocks new survey-mission assignments until the sim time
    /// advances past `injured_until_sim_time`. Set by the mission
    /// failure system on a `CrewInjury` failure.
    pub fn is_injured(&self, sim_time: f64) -> bool {
        self.injured_until_sim_time
            .map(|until| sim_time < until)
            .unwrap_or(false)
    }

    /// Mark the scientist as injured until `until_sim_time`. The
    /// mission system sets this on a `CrewInjury` failure; the
    /// personnel UI surfaces it as "Injured (NN days)".
    pub fn injure(&mut self, until_sim_time: f64) {
        self.injured_until_sim_time = Some(until_sim_time);
    }

    /// Whether the scientist can be promoted to the next tier.
    /// The thresholds are RON-configurable (PR-C adds
    /// `personnel_promotion.ron`).
    pub fn is_eligible_for_promotion(&self) -> bool {
        match self.seniority {
            SeniorityTier::Junior => self.lifetime_data_processed >= 500.0,
            SeniorityTier::Senior => {
                self.lifetime_data_processed >= 5_000.0 && self.lifetime_anomalies_flagged >= 5
            }
            SeniorityTier::Principal => false,
        }
    }
}
