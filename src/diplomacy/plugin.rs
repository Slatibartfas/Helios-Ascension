//! Bevy plugin for the Diplomacy system.

use bevy::prelude::*;

use super::{
    RelationsGraph, DiplomaticVictoryTracker,
    systems::{
        reputation_drift_system,
        treaty_compliance_bonus_system,
        violation_penalty_system,
        treaty_duration_system,
        nap_compliance_system,
        ai_proposal_generation_system,
        ai_proposal_response_system,
        victory_tracking_system,
        stance_update_system,
    },
};

/// Diplomacy plugin — register all diplomacy systems and resources.
pub struct DiplomacyPlugin;

impl Plugin for DiplomacyPlugin {
    fn build(&self, app: &mut App) {
        // Initialise resources.
        app.init_resource::<RelationsGraph>();
        app.init_resource::<DiplomaticVictoryTracker>();

        // Add systems — run in Update (not EguiPrimaryContextPass).
        app.add_systems(Update, reputation_drift_system);
        app.add_systems(Update, treaty_compliance_bonus_system);
        app.add_systems(Update, violation_penalty_system);
        app.add_systems(Update, treaty_duration_system);
        app.add_systems(Update, nap_compliance_system);
        app.add_systems(Update, ai_proposal_generation_system);
        app.add_systems(Update, ai_proposal_response_system);
        app.add_systems(Update, victory_tracking_system);
        app.add_systems(Update, stance_update_system);
    }
}
