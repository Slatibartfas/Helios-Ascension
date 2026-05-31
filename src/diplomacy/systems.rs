//! Diplomacy systems — reputation, treaties, proposals, and AI negotiation.

use bevy::prelude::*;
use crate::ai::AIFactionList;
use crate::ai::components::{AIFaction, AIPersonality, AIDifficulty};
use crate::fleets::PendingFleetActions;
use crate::ui::time::SimulationTime;

use super::{
    FactionRelation, TreatyType, DiplomaticProposal, ProposalType, Demand, Offer,
    DiplomaticVictoryTracker, RelationStance,
};
use crate::victory::{VictoryState, VictoryType};

/// Tag component for the player-controlled faction (faction_id = 0).
#[derive(Component)]
pub struct PlayerFaction;

/// Resource holding the player's diplomatic relations with all AI factions.
/// Key: (from_faction_id, to_faction_id) where from=0 is always the player.
/// One FactionRelation per AI faction (player -> each AI).
#[derive(Resource, Default)]
pub struct RelationsGraph {
    /// All faction relations. Player-to-AI relations use (0, ai_faction_id).
    pub relations: Vec<FactionRelation>,
    /// Proposal ID counter.
    pub proposal_counter: u64,
}

impl RelationsGraph {
    /// Get the relation from the player to a given faction.
    pub fn player_relation(&self, faction_id: u32) -> Option<&FactionRelation> {
        self.relations.iter().find(|r| r.pair == (0, faction_id))
    }

    /// Get mutable relation from the player to a given faction.
    pub fn player_relation_mut(&mut self, faction_id: u32) -> Option<&mut FactionRelation> {
        self.relations.iter_mut().find(|r| r.pair == (0, faction_id))
    }

    /// Get relation between two arbitrary factions.
    pub fn relation(&self, from: u32, to: u32) -> Option<&FactionRelation> {
        self.relations.iter().find(|r| r.pair == (from, to))
    }

    /// Get mutable relation between two arbitrary factions.
    pub fn relation_mut(&mut self, from: u32, to: u32) -> Option<&mut FactionRelation> {
        self.relations.iter_mut().find(|r| r.pair == (from, to))
    }

    /// Establish initial neutral relations between the player and all AI factions.
    pub fn spawn_initial_relations(&mut self, ai_factions: &AIFactionList, world: &World) {
        for &entity in &ai_factions.factions {
            if let Some(ai) = world.get::<AIFaction>(entity) {
                let relation = FactionRelation::new(0, ai.faction_id);
                self.relations.push(relation);
            }
        }
    }

    /// Player proposes a treaty to an AI faction.
    pub fn propose_treaty(
        &mut self,
        to_faction: u32,
        treaty_type: TreatyType,
        offer: Option<Offer>,
        demand: Option<Demand>,
        time: u64,
    ) {
        let id = {
            let id = self.proposal_counter.wrapping_add(1);
            self.proposal_counter = id;
            id
        };
        let proposal = DiplomaticProposal {
            id,
            from_faction: 0,
            to_faction,
            proposal_type: ProposalType::ProposeTreaty,
            treaty_type: Some(treaty_type),
            demand,
            offer,
            expires_in_ticks: 180, // ~6 months to respond
            ai_reason: None,
        };

        if let Some(rel) = self.player_relation_mut(to_faction) {
            rel.pending_proposal = Some(proposal);
            rel.last_contact_tick = time;
        }
    }

    /// Player declares war on a faction. Terminates all treaties, sets reputation -100.
    pub fn declare_war(&mut self, target: u32) {
        if let Some(rel) = self.player_relation_mut(target) {
            rel.add_reputation(-100.0);
            rel.clear_treaties();
            rel.pending_proposal = None;
        }
    }

    /// Player sends a gift to improve relations.
    pub fn send_gift(&mut self, target: u32, mc_amount: f64) {
        if let Some(rel) = self.player_relation_mut(target) {
            // Diminishing returns: +1 per 500, then +1 per 1000, etc.
            let rep_gain = if mc_amount >= 1000.0 { mc_amount / 500.0 } else { 1.0 };
            rel.add_reputation(rep_gain);
        }
    }

    /// Player requests ceasefire with a faction they're at war with.
    pub fn request_ceasefire(&mut self, target: u32, time: u64) {
        let new_id = self.proposal_counter.wrapping_add(1);
        self.proposal_counter = new_id;

        let rel = match self.player_relation_mut(target) {
            Some(r) if r.is_at_war() => r,
            _ => return,
        };
        let proposal = DiplomaticProposal {
            id: new_id,
            from_faction: 0,
            to_faction: target,
            proposal_type: ProposalType::RequestCeasefire,
            treaty_type: Some(TreatyType::NonAggressionPact),
            demand: None,
            offer: None,
            expires_in_ticks: 90,
            ai_reason: None,
        };
        rel.pending_proposal = Some(proposal);
        rel.last_contact_tick = time;
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Reputation system
// ─────────────────────────────────────────────────────────────────────────────

/// System: natural reputation drift toward 0 when no treaties exist.
pub fn reputation_drift_system(
    time: Res<SimulationTime>,
    mut relations: ResMut<RelationsGraph>,
) {
    let ticks_per_month = 30.0_f64;
    let tick = time.tick();
    let drift = if (tick % ticks_per_month as u64) == 0 { 1.0 } else { 0.0 };

    if drift > 0.0 {
        for relation in &mut relations.relations {
            if relation.treaties.is_empty() && relation.reputation != 0.0 {
                let signum = relation.reputation.signum();
                relation.add_reputation(-signum * drift);
            }
        }
    }
}

/// System: treaty compliance bonus — +2 reputation per treaty year when all treaties upheld.
pub fn treaty_compliance_bonus_system(
    time: Res<SimulationTime>,
    mut relations: ResMut<RelationsGraph>,
) {
    let ticks_per_year = 360.0_f64;
    if (time.tick() % ticks_per_year as u64) != 0 {
        return;
    }

    for relation in &mut relations.relations {
        if relation.treaties.iter().all(|t| !t.violated) && !relation.treaties.is_empty() {
            relation.add_reputation(2.0);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Treaty compliance & violation detection
// ─────────────────────────────────────────────────────────────────────────────

/// Event emitted when a treaty violation is detected.
#[derive(Event, Debug, Clone)]
pub struct TreatyViolationEvent {
    pub relation_pair: (u32, u32),
    pub treaty_type: TreatyType,
    pub violator: u32,
    pub victim: u32,
    pub severity: ViolationSeverity,
}

#[derive(Debug, Clone, Copy)]
pub enum ViolationSeverity {
    Warning,  // First offense — warning issued
    Serious,  // Second offense — treaty suspended
    Critical, // Third offense — automatic war
}

/// System: check NAP compliance — currently a stub.
/// In a full implementation, this would filter pending fleet attack orders
/// against NAP signatories by resolving target_entity -> faction_id.
pub fn nap_compliance_system(
    _relations: Res<RelationsGraph>,
    _pending_actions: ResMut<PendingFleetActions>,
) {
    // TODO: resolve target_entity -> faction_id for each pending attack,
    // then remove any that target a NAP signatory.
}

/// System: apply violation penalties when treaties are violated.
pub fn violation_penalty_system(mut relations: ResMut<RelationsGraph>) {
    // First pass: detect and mark first-time violations
    for relation in &mut relations.relations {
        for treaty in &mut relation.treaties {
            if treaty.violated && !treaty.warning_issued {
                treaty.warning_issued = true;
                relation.violations += 1;
                info!(
                    "Treaty violation: {:?} by {:?} vs {:?}",
                    treaty.treaty_type,
                    relation.pair.0,
                    relation.pair.1
                );
            }
        }
    }
    // Second pass: apply penalties for already-warned violations
    for relation in &mut relations.relations {
        let has_warned_violation = relation.treaties.iter().any(|t| t.violated && t.warning_issued);
        if has_warned_violation {
            if relation.violations == 1 {
                relation.add_reputation(-30.0);
                info!("Treaty suspended: second violation");
            } else if relation.violations >= 2 {
                relation.add_reputation(-100.0);
                relation.clear_treaties();
                info!("Automatic war: third violation");
            }
        }
    }
}

/// System: tick treaty durations and remove expired treaties.
pub fn treaty_duration_system(
    time: Res<SimulationTime>,
    mut relations: ResMut<RelationsGraph>,
) {
    let tick = time.tick();
    for relation in &mut relations.relations {
        relation.treaties.retain(|t| {
            if let Some(remaining) = t.duration_ticks {
                if remaining == 0 {
                    return false;
                }
            }
            true
        });
        for treaty in &mut relation.treaties {
            treaty.tick();
        }
        // Decrement duration counters
        for treaty in &mut relation.treaties {
            if let Some(remaining) = treaty.duration_ticks.as_mut() {
                if tick > 0 {
                    *remaining = remaining.saturating_sub(1);
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// AI negotiation
// ─────────────────────────────────────────────────────────────────────────────

/// Evaluate a proposal from the AI's perspective and return the AI's decision.
pub fn evaluate_proposal(
    proposal: &DiplomaticProposal,
    relation: &FactionRelation,
    ai: &AIFaction,
    ai_has_strong_fleet: bool,
    ai_losing_territory: bool,
    ai_economic_distress: bool,
) -> ProposalDecision {
    let mut score = 50.0; // base acceptance midpoint

    // Personality modifier
    match (ai.personality, &proposal.proposal_type) {
        (AIPersonality::Militarist, ProposalType::ProposeTreaty) => score += 10.0,
        (AIPersonality::Militarist, ProposalType::OfferTrade) => score -= 15.0,
        (AIPersonality::Economic, ProposalType::OfferTrade) => score += 15.0,
        (AIPersonality::Economic, ProposalType::RequestAlliance) => score -= 10.0,
        (AIPersonality::Scientific, ProposalType::OfferTrade) => score += 10.0,
        (AIPersonality::Balanced, _) => score += 5.0,
        _ => {}
    }

    // Reputation modifier
    score += (relation.reputation - 25.0) / 2.0;

    // Strategic context modifiers
    if ai_losing_territory {
        score += 10.0; // AI values NAP when losing territory
    }
    if ai_economic_distress {
        score += 20.0; // AI values trade when economically distressed
    }
    if ai_has_strong_fleet {
        score += 10.0; // Strong AI more willing to alliance
    }

    // Difficulty modifier (effective acceptance threshold)
    let threshold = match ai.difficulty {
        AIDifficulty::Easy => 50.0,
        AIDifficulty::Normal => 60.0,
        AIDifficulty::Hard => 70.0,
    };

    if score >= threshold + 10.0 {
        ProposalDecision::Accept
    } else if score >= threshold - 20.0 {
        ProposalDecision::CounterProposal(counter_proposal(proposal, ai))
    } else {
        ProposalDecision::Reject
    }
}

pub enum ProposalDecision {
    Accept,
    CounterProposal(DiplomaticProposal),
    Reject,
}

/// Generate a counter-proposal based on AI personality.
fn counter_proposal(proposal: &DiplomaticProposal, ai: &AIFaction) -> DiplomaticProposal {
    let mut counter = proposal.clone();
    counter.id = proposal.id.wrapping_add(1000); // New ID
    counter.from_faction = proposal.to_faction;
    counter.to_faction = proposal.from_faction;

    // Adjust demands based on personality
    let extra = match ai.personality {
        AIPersonality::Militarist => 20.0,
        AIPersonality::Economic => 15.0,
        AIPersonality::Scientific => 10.0,
        AIPersonality::Balanced => 10.0,
    };

    if let Some(ref mut demand) = counter.demand {
        demand.value += extra;
    }

    counter.ai_reason =
        Some("We find those terms unfavorable. Perhaps you could offer more.".to_string());

    counter
}

/// System: AI proposes treaties on its own schedule.
pub fn ai_proposal_generation_system(
    time: Res<SimulationTime>,
    mut relations: ResMut<RelationsGraph>,
    ai_factions: Query<(Entity, &AIFaction)>,
) {
    // AI proposes every 180-360 ticks
    let tick = time.tick();
    let proposal_interval = 240_u64;
    if tick % proposal_interval != 0 {
        return;
    }

    for (entity, ai) in &ai_factions {
        if !ai.should_decide(4) {
            continue;
        }

        // Read the counter before taking the mutable borrow
        let new_counter = relations.proposal_counter.wrapping_add(1);
        relations.proposal_counter = new_counter;

        // For each player relation, consider proposing something
        let rel = match relations.relation_mut(ai.faction_id, 0) {
            Some(rel) => rel,
            None => continue,
        };
        // Skip if already at war or has pending proposal
        if rel.is_at_war() || rel.pending_proposal.is_some() {
            continue;
        }

        // Only propose if reputation >= -20 (somewhat friendly)
        if rel.reputation < -20.0 {
            continue;
        }

        let proposal_type = match ai.personality {
            AIPersonality::Militarist if rel.reputation > 30.0 => ProposalType::RequestAlliance,
            AIPersonality::Militarist => ProposalType::ProposeTreaty,
            AIPersonality::Economic => ProposalType::OfferTrade,
            AIPersonality::Scientific if rel.reputation > 10.0 => ProposalType::OfferTrade,
            AIPersonality::Balanced => {
                if rel.reputation > 50.0 {
                    ProposalType::RequestAlliance
                } else {
                    ProposalType::ProposeTreaty
                }
            }
            _ => ProposalType::ProposeTreaty,
        };

        let treaty_type = match proposal_type {
            ProposalType::RequestAlliance => Some(TreatyType::MilitaryAlliance),
            ProposalType::OfferTrade => Some(TreatyType::TradeAgreement),
            _ => Some(TreatyType::NonAggressionPact),
        };

        let ai_proposal = DiplomaticProposal {
            id: new_counter,
            from_faction: ai.faction_id,
            to_faction: 0,
            proposal_type,
            treaty_type,
            demand: None,
            offer: None,
            expires_in_ticks: 180,
            ai_reason: Some(format!("The {} extends an offer of friendship.", ai.name)),
        };

        rel.pending_proposal = Some(ai_proposal);
        rel.last_contact_tick = tick;
    }
}

/// System: process pending AI responses to player proposals.
pub fn ai_proposal_response_system(
    mut relations: ResMut<RelationsGraph>,
    _time: Res<SimulationTime>,
) {
    // First pass: tick down expiry timers.
    for relation in &mut relations.relations {
        if let Some(ref mut proposal) = relation.pending_proposal {
            if proposal.expires_in_ticks > 0 {
                proposal.expires_in_ticks -= 1;
            }
        }
    }
    // Second pass: expire stale proposals.
    for relation in &mut relations.relations {
        if let Some(ref proposal) = relation.pending_proposal {
            if proposal.expires_in_ticks == 0 {
                relation.pending_proposal = None;
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Victory tracking
// ─────────────────────────────────────────────────────────────────────────────

/// System: update diplomatic victory tracker each tick.
pub fn victory_tracking_system(
    relations: Res<RelationsGraph>,
    mut tracker: ResMut<DiplomaticVictoryTracker>,
    mut victory_state: ResMut<VictoryState>,
    time: Res<SimulationTime>,
) {
    // Count allied pairs (player to AI where stance = Allied)
    let new_allied_count = relations
        .relations
        .iter()
        .filter(|r| r.stance == RelationStance::Allied)
        .count() as u8;

    if new_allied_count != tracker.allied_count {
        tracker.allied_count = new_allied_count;

        // Fire diplomatic victory event when threshold reached
        if tracker.allied_count >= tracker.allies_required
            && !victory_state.diplomatic_victory_achieved
        {
            info!(
                "Diplomatic victory achieved! Allied with {} factions.",
                tracker.allied_count
            );
            victory_state.claim_victory(0, VictoryType::Diplomatic, time.elapsed_seconds());
        }
    }
}

/// System: update cached stance on all relations.
pub fn stance_update_system(mut relations: ResMut<RelationsGraph>) {
    for relation in &mut relations.relations {
        relation.stance = RelationStance::from_reputation(relation.reputation);
    }
}
