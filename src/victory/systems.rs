//! Victory condition detection systems.
//!
//! Runs every frame in `Update`. Victory checks are cheap (simple counters
//! and comparisons) so running every frame has no meaningful performance cost.

use bevy::prelude::*;

use crate::ai::AIFactionList;
use crate::diplomacy::{DiplomaticVictoryTracker, RelationsGraph, RelationStance};
use crate::economy::GlobalBudget;
use crate::research::{ResearchState, TechnologiesData};
use crate::ui::SimulationTime;

use super::types::{VictoryState, VictoryType};

/// Threshold for economic victory: minimum annual income in MC/yr.
const ECONOMIC_GDP_THRESHOLD: f64 = 10_000_000.0;
/// Threshold for economic victory: minimum annual trade volume in Mt.
const ECONOMIC_TRADE_THRESHOLD: f64 = 5_000_000.0;

/// Check all victory conditions each frame.
/// Uses exact equality for flags (set once on first achievement).
/// All other state is read-only — no write contention.
pub fn check_victory_conditions(
    mut victory_state: ResMut<VictoryState>,
    research_state: Res<ResearchState>,
    tech_data: Res<TechnologiesData>,
    ai_factions: Res<AIFactionList>,
    budget: Res<GlobalBudget>,
    sim_time: Res<SimulationTime>,
    // Optional: only present when diplomacy plugin is loaded
    _relations: Option<Res<RelationsGraph>>,
    _tracker: Option<Res<DiplomaticVictoryTracker>>,
) {
    // Skip if a victory has already been claimed
    if victory_state.any_victory_achieved() {
        return;
    }

    let time = sim_time.elapsed_seconds();

    // ── Scientific victory ────────────────────────────────────────────────
    // All technologies in the data file must be unlocked.
    let total_techs = tech_data.technologies.len();
    if total_techs > 0 {
        let unlocked_count = research_state.unlocked_technologies.len();
        if unlocked_count >= total_techs {
            victory_state.claim_victory(0, VictoryType::Scientific, time);
            return;
        }
    }

    // ── Military victory ────────────────────────────────────────────────────
    // All AI factions must be eliminated (entity despawned or colonies empty).
    if all_ai_factions_eliminated(&ai_factions) {
        victory_state.claim_victory(0, VictoryType::Military, time);
        return;
    }

    // ── Economic victory ───────────────────────────────────────────────────
    if budget.income_per_year >= ECONOMIC_GDP_THRESHOLD {
        // Trade volume estimated from income throughput as proxy.
        // Real implementation would track trade fleet cargo (future enhancement).
        let trade_volume_estimate = budget.income_per_year;
        if trade_volume_estimate >= ECONOMIC_TRADE_THRESHOLD {
            victory_state.claim_victory(0, VictoryType::Economic, time);
            return;
        }
    }

    // ── Diplomatic victory ─────────────────────────────────────────────────
    // Delegate to the diplomacy plugin's victory tracker if present.
    // The diplomacy plugin's victory_tracking_system updates the tracker
    // and fires an event; we simply observe the tracker state here.
    if let (Some(tracker), Some(relations)) = (_tracker.as_ref(), _relations.as_ref()) {
        if tracker.allied_count >= tracker.allies_required {
            victory_state.claim_victory(0, VictoryType::Diplomatic, time);
            return;
        }
        // Also check: if all relations with AI factions have Allied stance.
        let all_allied = relations.relations.iter()
            .filter(|r| r.pair.0 == 0) // player relations only
            .all(|r| r.stance == RelationStance::Allied);
        if all_allied && !relations.relations.is_empty() {
            victory_state.claim_victory(0, VictoryType::Diplomatic, time);
        }
    }
}

/// Returns true if every AI faction has been eliminated.
fn all_ai_factions_eliminated(ai_factions: &AIFactionList) -> bool {
    // If AIFactionList is empty, all AI factions are gone.
    ai_factions.factions.is_empty()
}