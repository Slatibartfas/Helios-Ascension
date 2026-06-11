//! Survey messages — Bevy 0.18 `Message` types emitted by the survey
//! systems and consumed by the notification surface.
//!
//! PR-C adds three new variants covering the r2 anomaly confidence
//! model: `AnomalyDetected` (a new candidate on a body's dossier),
//! `AnomalyActivated` (confidence crossed the threshold), and
//! `AnomalyRefuted` (a contradicting verification mission dropped
//! confidence below re-arm). The notification/UI layer wires these
//! to the player's event log.

use bevy::prelude::*;

use super::types::AnomalyType;

/// All survey-related events. PR-C ships the three anomaly variants;
/// later PRs (notifications roadmap §5.4) will fold mission lifecycle
/// events in here too.
#[derive(Message, Debug, Clone)]
pub struct SurveyEvent {
    /// Sim-time the event fired.
    pub sim_time: f64,
    /// Body the event happened on.
    pub body: Entity,
    /// Which anomaly triggered it.
    pub kind: SurveyEventKind,
}

/// Discriminant for the kind of survey event. Variants are flat
/// (not enum-of-enum) so the message pattern matches the design
/// doc's event table directly.
#[derive(Debug, Clone)]
pub enum SurveyEventKind {
    /// A new anomaly was logged on the body. Fires when the
    /// `surface_anomaly_events` system rolls a successful
    /// detection past the false-positive check.
    AnomalyDetected {
        anomaly: AnomalyType,
        /// Initial confidence (always 0.10 × axis_match_count per the
        /// r2 model).
        initial_confidence: f32,
    },
    /// Confidence crossed the activation threshold and the anomaly
    /// transitioned to `Verified`. The effect is also applied.
    AnomalyActivated {
        anomaly: AnomalyType,
        confidence: f32,
    },
    /// A contradicting verification mission dropped confidence below
    /// the re-arm threshold. The anomaly moves to `Dormant` or
    /// `Suspected`.
    AnomalyRefuted { anomaly: AnomalyType },
}
