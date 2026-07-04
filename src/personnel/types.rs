//! Personnel system core enums.
//!
//! Scientist specialties and seniority tiers defined by
//! SURVEY_REWORK.md §8 (Personnel: Field Scientists). Modders can
//! add a new specialty by adding a variant here + a row to
//! `personnel_specialties.ron` (PR-C).

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::fmt;

/// The eight scientist specialties defined by SURVEY_REWORK.md §8.
/// Modders can add a new specialty by adding a variant here + a row
/// in `assets/data/personnel_specialties.ron`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub enum ScientistSpecialty {
    Geology,
    Atmospherics,
    Biology,
    Geophysics,
    Spectroscopy,
    Chemistry,
    PlanetaryScience,
    Astrobiology,
}

impl ScientistSpecialty {
    /// Throughput multiplier when this specialty matches the job's
    /// method. Default: 1.5× (per SURVEY_REWORK.md §8).
    pub fn match_multiplier(self) -> f32 {
        1.5
    }

    /// Throughput multiplier when this specialty does NOT match the
    /// job's method. Default: 0.7×.
    pub fn mismatch_multiplier(self) -> f32 {
        0.7
    }

    /// Whether this specialty matches a given
    /// [`SurveyMethod`](crate::survey::SurveyMethod). Drives the
    /// analysis queue's throughput calculation.
    #[allow(clippy::match_like_matches_macro)]
    pub fn matches_method(self, method: crate::survey::SurveyMethod) -> bool {
        use crate::survey::SurveyMethod;
        match (self, method) {
            (ScientistSpecialty::Geology, SurveyMethod::SurfaceLander)
            | (ScientistSpecialty::Geology, SurveyMethod::Rover)
            | (ScientistSpecialty::Geology, SurveyMethod::Drill)
            | (ScientistSpecialty::Geology, SurveyMethod::SampleReturn) => true,
            (ScientistSpecialty::Atmospherics, SurveyMethod::AtmosphericProbe)
            | (ScientistSpecialty::Atmospherics, SurveyMethod::Flyby) => true,
            (ScientistSpecialty::Biology, SurveyMethod::SurfaceLander)
            | (ScientistSpecialty::Biology, SurveyMethod::Rover) => true,
            (ScientistSpecialty::Geophysics, SurveyMethod::Seismic) => true,
            (ScientistSpecialty::Spectroscopy, SurveyMethod::RemoteSensing)
            | (ScientistSpecialty::Spectroscopy, SurveyMethod::Orbital) => true,
            (ScientistSpecialty::Chemistry, SurveyMethod::Drill)
            | (ScientistSpecialty::Chemistry, SurveyMethod::SampleReturn) => true,
            (ScientistSpecialty::PlanetaryScience, SurveyMethod::Orbital)
            | (ScientistSpecialty::PlanetaryScience, SurveyMethod::Flyby) => true,
            (ScientistSpecialty::Astrobiology, SurveyMethod::SurfaceLander)
            | (ScientistSpecialty::Astrobiology, SurveyMethod::Rover)
            | (ScientistSpecialty::Astrobiology, SurveyMethod::SampleReturn) => true,
            _ => false,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            ScientistSpecialty::Geology => "Geology",
            ScientistSpecialty::Atmospherics => "Atmospherics",
            ScientistSpecialty::Biology => "Biology",
            ScientistSpecialty::Geophysics => "Geophysics",
            ScientistSpecialty::Spectroscopy => "Spectroscopy",
            ScientistSpecialty::Chemistry => "Chemistry",
            ScientistSpecialty::PlanetaryScience => "Planetary Science",
            ScientistSpecialty::Astrobiology => "Astrobiology",
        }
    }

    pub fn ron_id(self) -> &'static str {
        match self {
            ScientistSpecialty::Geology => "geology",
            ScientistSpecialty::Atmospherics => "atmospherics",
            ScientistSpecialty::Biology => "biology",
            ScientistSpecialty::Geophysics => "geophysics",
            ScientistSpecialty::Spectroscopy => "spectroscopy",
            ScientistSpecialty::Chemistry => "chemistry",
            ScientistSpecialty::PlanetaryScience => "planetary_science",
            ScientistSpecialty::Astrobiology => "astrobiology",
        }
    }

    pub fn from_ron_id(id: &str) -> Option<Self> {
        match id {
            "geology" => Some(ScientistSpecialty::Geology),
            "atmospherics" => Some(ScientistSpecialty::Atmospherics),
            "biology" => Some(ScientistSpecialty::Biology),
            "geophysics" => Some(ScientistSpecialty::Geophysics),
            "spectroscopy" => Some(ScientistSpecialty::Spectroscopy),
            "chemistry" => Some(ScientistSpecialty::Chemistry),
            "planetary_science" => Some(ScientistSpecialty::PlanetaryScience),
            "astrobiology" => Some(ScientistSpecialty::Astrobiology),
            _ => None,
        }
    }
}

impl fmt::Display for ScientistSpecialty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.display_name())
    }
}

/// The three seniority tiers defined by SURVEY_REWORK.md §8.
///
/// Throughput and confidence multipliers per tier:
///
/// - **Junior** — 1.0× throughput, 0.8× confidence multiplier
/// - **Senior** — 1.5× throughput, 1.0× confidence multiplier
/// - **Principal** — 2.0× throughput, 1.2× confidence multiplier,
///   +10% chance of finding anomalies
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub enum SeniorityTier {
    Junior,
    Senior,
    Principal,
}

impl SeniorityTier {
    /// Throughput multiplier for this seniority tier.
    pub fn throughput_multiplier(self) -> f32 {
        match self {
            SeniorityTier::Junior => 1.0,
            SeniorityTier::Senior => 1.5,
            SeniorityTier::Principal => 2.0,
        }
    }

    /// Confidence multiplier — how much a scientist's analysis
    /// raises the dimension's confidence.
    pub fn confidence_multiplier(self) -> f32 {
        match self {
            SeniorityTier::Junior => 0.8,
            SeniorityTier::Senior => 1.0,
            SeniorityTier::Principal => 1.2,
        }
    }

    /// Bonus chance of finding an anomaly on completion (additive
    /// to the instrument's `produces_anomalies` flag).
    pub fn anomaly_bonus(self) -> f32 {
        match self {
            SeniorityTier::Junior => 0.0,
            SeniorityTier::Senior => 0.0,
            SeniorityTier::Principal => 0.10,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            SeniorityTier::Junior => "Junior",
            SeniorityTier::Senior => "Senior",
            SeniorityTier::Principal => "Principal",
        }
    }

    pub fn ron_id(self) -> &'static str {
        match self {
            SeniorityTier::Junior => "junior",
            SeniorityTier::Senior => "senior",
            SeniorityTier::Principal => "principal",
        }
    }
}

impl fmt::Display for SeniorityTier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.display_name())
    }
}

/// Stable id for a scientist. Used by
/// [`Scientist::current_analysis`](crate::personnel::components::Scientist::current_analysis)
/// and [`AnalysisQueueIndex`](crate::survey::data::AnalysisQueueIndex).
pub type ScientistId = u64;
