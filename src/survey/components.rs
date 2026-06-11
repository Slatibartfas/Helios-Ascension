//! Survey system components.
//!
//! These components live on celestial body entities. PR-A adds them as
//! **optional** components — bodies keep their existing `SurveyLevel`
//! enum and `discovered_amount()` is the source of truth until Phase 5
//! of the migration plan (SURVEY_REWORK.md §15). The migration shim is
//! [`SurveyState::from_legacy_level`].

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::types::{
    AnomalyType, SurveyDimension, SurveyMethod, INITIAL_CONFIDENCE, MAX_TIER, WARNING_CONFIDENCE,
};
use crate::economy::components::SurveyLevel;

/// Per-dimension survey fidelity on a body.
///
/// Replaces the single-axis `SurveyLevel` enum with a multi-axis
/// structure: a body has eight independent discovery dimensions, each at
/// its own tier (0–5) with its own confidence (0.0–1.0). Confidence rises
/// with more measurements and falls with time (see
/// [`CONFIDENCE_DECAY_PER_YEAR`](super::types::CONFIDENCE_DECAY_PER_YEAR)).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DimensionFidelity {
    /// Resolution tier for this dimension. 0 = unknown, 5 = fully
    /// characterized. Tier semantics are dimension-specific and live in
    /// `assets/data/survey/tiers.ron`.
    pub tier: u8,
    /// Sim-time of the most recent measurement. `None` means the
    /// dimension has never been measured. Used for confidence decay.
    pub last_measured_sim_time: Option<f64>,
    /// Confidence in the current tier reading. Rises with measurements,
    /// falls at `CONFIDENCE_DECAY_PER_YEAR` per sim-year. At confidence
    /// ≤ 0.1 the data is "stale" and a re-survey is recommended.
    pub confidence: f32,
}

impl Default for DimensionFidelity {
    fn default() -> Self {
        Self {
            tier: 0,
            last_measured_sim_time: None,
            confidence: 0.0,
        }
    }
}

impl DimensionFidelity {
    /// A fresh, never-measured dimension (tier 0, confidence 0.0).
    pub const UNKNOWN: Self = Self {
        tier: 0,
        last_measured_sim_time: None,
        confidence: 0.0,
    };

    /// Build a fidelity entry that has just been measured at `tier` with
    /// the default initial confidence. Used by the analysis queue on
    /// completion of a survey job.
    pub fn freshly_measured(tier: u8, sim_time: f64) -> Self {
        Self {
            tier: tier.min(MAX_TIER),
            last_measured_sim_time: Some(sim_time),
            confidence: INITIAL_CONFIDENCE,
        }
    }

    /// Build a fidelity at a given tier and confidence. Used by the
    /// migration shim and by save-load round-trips.
    pub fn at_tier(tier: u8, confidence: f32, sim_time: Option<f64>) -> Self {
        Self {
            tier: tier.min(MAX_TIER),
            last_measured_sim_time: sim_time,
            confidence: confidence.clamp(0.0, 1.0),
        }
    }

    /// Whether this dimension is "known" (tier ≥ 1).
    pub fn is_known(&self) -> bool {
        self.tier > 0
    }

    /// Whether the data is stale (confidence below the warning
    /// threshold).
    pub fn is_stale(&self) -> bool {
        self.confidence < WARNING_CONFIDENCE
    }

    /// Whether the dimension has been fully characterized (tier 5 with
    /// confidence ≥ 0.8).
    pub fn is_fully_characterized(&self) -> bool {
        self.tier >= MAX_TIER && self.confidence >= 0.8
    }
}

/// Per-body survey state. Replaces the old `SurveyLevel` enum (kept
/// during the Phase 1–4 migration window).
///
/// `SurveyState` carries:
/// - the dimension map ([`SurveyDimension`] → [`DimensionFidelity`])
/// - the list of currently-running missions
/// - a sim-time stamp for confidence decay scheduling
/// - the cumulative science investment (for the dossier "Science
///   Points" readout and the system summary)
#[derive(Debug, Clone, Component, Serialize, Deserialize)]
pub struct SurveyState {
    /// Per-dimension fidelity. Dimensions not present in the map are
    /// treated as [`DimensionFidelity::UNKNOWN`] (tier 0, confidence
    /// 0.0). The map is sparse to keep save size small — most bodies
    /// have 0–3 known dimensions, not 8.
    pub dimensions: HashMap<SurveyDimension, DimensionFidelity>,
    /// Currently-running survey missions on this body. The
    /// `advance_survey_missions` system ticks these.
    pub active_missions: Vec<ActiveSurveyMission>,
    /// Sim-time of the last survey state update. Used for confidence
    /// decay bookkeeping.
    pub last_updated_sim_time: f64,
    /// Total science points invested on this body across all
    /// instruments and missions.
    pub total_science_points_invested: f64,
    /// Anomalies detected on this body. Persists across the migration
    /// window so that reloading a v0.4.x save preserves any anomalies
    /// the player discovered (currently always empty in PR-A — anomaly
    /// events land in PR-D).
    pub detected_anomalies: Vec<DetectedAnomaly>,
}

impl Default for SurveyState {
    fn default() -> Self {
        Self {
            dimensions: HashMap::new(),
            active_missions: Vec::new(),
            last_updated_sim_time: 0.0,
            total_science_points_invested: 0.0,
            detected_anomalies: Vec::new(),
        }
    }
}

impl SurveyState {
    /// A fresh, fully-unsurveyed state. Equivalent to
    /// `SurveyLevel::Unsurveyed`.
    pub fn unsurveyed() -> Self {
        Self::default()
    }

    /// A state where every dimension is at tier 5 with full
    /// confidence. Used by the migration shim for bodies that were at
    /// `SurveyLevel::CoreSample` (e.g. Earth in
    /// `src/plugins/solar_system.rs`).
    pub fn fully_surveyed(sim_time: f64) -> Self {
        let mut dimensions = HashMap::new();
        for dim in SurveyDimension::ALL {
            dimensions.insert(
                dim,
                DimensionFidelity::at_tier(MAX_TIER, 1.0, Some(sim_time)),
            );
        }
        Self {
            dimensions,
            active_missions: Vec::new(),
            last_updated_sim_time: sim_time,
            total_science_points_invested: 0.0,
            detected_anomalies: Vec::new(),
        }
    }

    /// Migration shim: map a legacy `SurveyLevel` to a `SurveyState`.
    /// See SURVEY_REWORK.md §15 (Backward compat) for the mapping
    /// table.
    pub fn from_legacy_level(level: SurveyLevel, sim_time: f64) -> Self {
        let mut dimensions = HashMap::new();
        match level {
            SurveyLevel::Unsurveyed => {}
            SurveyLevel::OrbitalScan => {
                dimensions.insert(
                    SurveyDimension::OrbitalMech,
                    DimensionFidelity::freshly_measured(1, sim_time),
                );
                dimensions.insert(
                    SurveyDimension::Atmosphere,
                    DimensionFidelity::freshly_measured(1, sim_time),
                );
                dimensions.insert(
                    SurveyDimension::MineralClasses,
                    DimensionFidelity::freshly_measured(1, sim_time),
                );
            }
            SurveyLevel::SeismicSurvey => {
                for dim in [
                    SurveyDimension::OrbitalMech,
                    SurveyDimension::Atmosphere,
                    SurveyDimension::MineralClasses,
                ] {
                    dimensions.insert(dim, DimensionFidelity::freshly_measured(1, sim_time));
                }
                dimensions.insert(
                    SurveyDimension::Subsurface,
                    DimensionFidelity::freshly_measured(2, sim_time),
                );
            }
            SurveyLevel::CoreSample => {
                return Self::fully_surveyed(sim_time);
            }
        }
        Self {
            dimensions,
            active_missions: Vec::new(),
            last_updated_sim_time: sim_time,
            total_science_points_invested: 0.0,
            detected_anomalies: Vec::new(),
        }
    }

    /// Look up a dimension's fidelity, returning `UNKNOWN` for
    /// dimensions not yet in the map. The map is sparse on purpose.
    pub fn fidelity(&self, dim: SurveyDimension) -> DimensionFidelity {
        self.dimensions
            .get(&dim)
            .copied()
            .unwrap_or(DimensionFidelity::UNKNOWN)
    }

    /// Mutate a dimension's fidelity. Creates the entry if absent.
    pub fn set_fidelity(&mut self, dim: SurveyDimension, fidelity: DimensionFidelity) {
        self.dimensions.insert(dim, fidelity);
    }

    /// Weighted-average tier across all dimensions, used by the system
    /// summary "SURVEY %" stat. Result is in `0.0..=1.0` (divide by
    /// `MAX_TIER` to get a percentage).
    pub fn average_tier(&self) -> f32 {
        if self.dimensions.is_empty() {
            return 0.0;
        }
        let sum: u32 = self.dimensions.values().map(|f| f.tier as u32).sum();
        // Average across all 8 dimensions, not just the ones the map
        // happens to know about — a body with 3 known dimensions is
        // more surveyed than a body with 0.
        let denom = SurveyDimension::ALL.len() as u32;
        (sum as f32) / (denom as f32 * MAX_TIER as f32)
    }

    /// Number of dimensions at tier ≥ 1.
    pub fn known_dimension_count(&self) -> usize {
        self.dimensions.values().filter(|f| f.is_known()).count()
    }

    /// Number of dimensions fully characterized (tier 5, confidence
    /// ≥ 0.8).
    pub fn fully_characterized_count(&self) -> usize {
        self.dimensions
            .values()
            .filter(|f| f.is_fully_characterized())
            .count()
    }

    /// Whether at least one mission is currently running.
    pub fn has_active_missions(&self) -> bool {
        !self.active_missions.is_empty()
    }
}

/// A running survey mission on a body. Lives on [`SurveyState`].
///
/// PR-A: the struct is defined for shape stability, but no system
/// creates or ticks missions yet — that lands in PR-B (instruments)
/// and PR-C (analysis queue).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveSurveyMission {
    /// Stable id so the UI can correlate a row in the "Active Missions"
    /// list with the underlying mission.
    pub id: u64,
    /// Display name (e.g. "Mare Imbrium 1", "Ariel-2"). Free-form.
    pub name: String,
    /// Method used by this mission.
    pub method: SurveyMethod,
    /// Sim-time the mission was launched.
    pub launched_sim_time: f64,
    /// Sim-time the mission is expected to complete. The
    /// `advance_survey_missions` system ticks the mission toward this
    /// timestamp; on completion, the mission is removed and a
    /// [`AnalysisJob`] is enqueued.
    pub expected_completion_sim_time: f64,
    /// Mission progress in `[0.0, 1.0]`. The dossier UI shows this as
    /// a bar.
    pub progress: f32,
}

/// A pending data analysis job. The analysis queue (PR-C) drives these.
///
/// While a job is unassigned, the underlying data sits unprocessed.
/// When a scientist is assigned, the job's `progress` advances at a
/// rate determined by the scientist's seniority × specialty match.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisJob {
    /// Stable id (also used by [`Scientist::current_analysis`](
    /// crate::personnel::components::Scientist::current_analysis)).
    pub id: u64,
    /// Body the data was collected from.
    pub body: Entity,
    /// Mission that produced the data. Optional because a job can be
    /// created from sources other than an active mission (e.g. a
    /// one-off lab analysis).
    pub source_mission: Option<u64>,
    /// Display label for the dossier / analysis queue.
    pub label: String,
    /// Method that produced the data. Drives the specialty-match
    /// multiplier.
    pub method: SurveyMethod,
    /// Sim-time the job was enqueued.
    pub enqueued_sim_time: f64,
    /// Sim-time the job completed. `None` while in flight.
    pub completed_sim_time: Option<f64>,
    /// Progress in `[0.0, 1.0]`. Reaches 1.0 on completion.
    pub progress: f32,
    /// Whether an anomaly was flagged by this analysis. Set by the
    /// analysis queue on completion if a discovery method affinity
    /// matches an anomaly present on the body.
    pub anomaly_flagged: Option<AnomalyType>,
}

/// An anomaly that has been detected and logged on a body's dossier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedAnomaly {
    /// Anomaly type. Modders add new types via `anomalies.ron`.
    pub anomaly_type: AnomalyType,
    /// Sim-time of detection.
    pub detected_sim_time: f64,
    /// Confidence in the detection (rises with re-observation).
    pub confidence: f32,
    /// Whether the player has acknowledged the anomaly in the dossier
    /// (used to suppress repeat notifications).
    pub acknowledged: bool,
}

#[cfg(test)]
mod tests {
    //! Migration-shim tests for PR-A.
    //!
    //! These tests lock in the SURVEY_REWORK.md §15 (Backward compat)
    //! mapping table. Future PRs that touch the shim will fail these
    //! tests if they break save compatibility.

    use super::*;
    use crate::economy::components::SurveyLevel;

    fn sim_time() -> f64 {
        // Fixed sim-time so the tests are deterministic.
        1_000.0
    }

    #[test]
    fn unsurveyed_maps_to_empty_state() {
        let state = SurveyState::from_legacy_level(SurveyLevel::Unsurveyed, sim_time());
        assert!(state.dimensions.is_empty());
        assert_eq!(state.last_updated_sim_time, sim_time());
        assert!(state.active_missions.is_empty());
        assert!(state.detected_anomalies.is_empty());
        assert_eq!(state.average_tier(), 0.0);
        assert_eq!(state.known_dimension_count(), 0);
    }

    #[test]
    fn orbital_scan_maps_to_three_dimensions_at_tier_one() {
        let state = SurveyState::from_legacy_level(SurveyLevel::OrbitalScan, sim_time());
        assert_eq!(state.known_dimension_count(), 3);
        for dim in [
            SurveyDimension::OrbitalMech,
            SurveyDimension::Atmosphere,
            SurveyDimension::MineralClasses,
        ] {
            let f = state.fidelity(dim);
            assert_eq!(f.tier, 1, "{dim} should be tier 1");
            assert_eq!(f.last_measured_sim_time, Some(sim_time()));
            assert!(f.confidence > 0.0, "{dim} should have nonzero confidence");
        }
        // Subsurface and MineralDeposits stay unknown.
        assert_eq!(state.fidelity(SurveyDimension::Subsurface).tier, 0);
        assert_eq!(state.fidelity(SurveyDimension::MineralDeposits).tier, 0);
    }

    #[test]
    fn seismic_survey_adds_subsurface_at_tier_two() {
        let state = SurveyState::from_legacy_level(SurveyLevel::SeismicSurvey, sim_time());
        assert_eq!(state.known_dimension_count(), 4);
        assert_eq!(state.fidelity(SurveyDimension::Subsurface).tier, 2);
        // The three tier-1 dims from OrbitalScan are preserved.
        for dim in [
            SurveyDimension::OrbitalMech,
            SurveyDimension::Atmosphere,
            SurveyDimension::MineralClasses,
        ] {
            assert_eq!(state.fidelity(dim).tier, 1);
        }
    }

    #[test]
    fn core_sample_maps_to_fully_surveyed() {
        let state = SurveyState::from_legacy_level(SurveyLevel::CoreSample, sim_time());
        // All 8 dimensions at tier 5 with confidence 1.0.
        for dim in SurveyDimension::ALL {
            let f = state.fidelity(dim);
            assert_eq!(f.tier, 5, "{dim} should be tier 5");
            assert!(f.is_fully_characterized(), "{dim} should be fully characterized");
        }
        assert_eq!(state.fully_characterized_count(), SurveyDimension::ALL.len());
        assert_eq!(state.average_tier(), 1.0);
    }

    #[test]
    fn average_tier_normalizes_over_all_dimensions() {
        // A body with 1 dimension at tier 4 (and 7 unknown) has
        // average_tier = 4 / (8 * 5) = 0.1.
        let mut state = SurveyState::default();
        state.set_fidelity(
            SurveyDimension::OrbitalMech,
            DimensionFidelity::at_tier(4, 0.9, Some(sim_time())),
        );
        let avg = state.average_tier();
        assert!(
            (avg - 0.1).abs() < 1e-6,
            "expected 0.1, got {avg}"
        );
        assert_eq!(state.known_dimension_count(), 1);
    }

    #[test]
    fn fidelity_clamp_keeps_tier_in_range() {
        // Calling freshly_measured with tier > MAX_TIER clamps to MAX_TIER.
        let f = DimensionFidelity::freshly_measured(99, sim_time());
        assert_eq!(f.tier, MAX_TIER);
        // at_tier clamps confidence into [0, 1].
        let over = DimensionFidelity::at_tier(1, 5.0, Some(sim_time()));
        assert_eq!(over.confidence, 1.0);
        let under = DimensionFidelity::at_tier(1, -1.0, Some(sim_time()));
        assert_eq!(under.confidence, 0.0);
    }
}
