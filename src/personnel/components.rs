//! Personnel components.
//!
//! PR-A scaffold: type definitions only. Spawning, hiring,
//! promotion, and assignment systems land in PR-C.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::types::{ScientistId, ScientistSpecialty, SeniorityTier};

/// A scientist entity. The analysis queue (PR-C) drives
/// `current_analysis` and updates `lifetime_data_processed` /
/// `lifetime_anomalies_flagged` on job completion.
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
    /// Active analysis job. `None` if idle.
    pub current_analysis: Option<u64>,
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
            current_analysis: None,
            lifetime_data_processed: 0.0,
            lifetime_anomalies_flagged: 0,
            hired_sim_time: sim_time,
        }
    }

    /// Whether the scientist is currently idle (no body assigned,
    /// no analysis in progress).
    pub fn is_idle(&self) -> bool {
        self.assigned_body.is_none() && self.current_analysis.is_none()
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
