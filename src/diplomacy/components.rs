//! Core diplomacy data structures.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Relation stance bands derived from reputation score.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RelationStance {
    Hostile,  // reputation < -50
    Neutral,  // -50 <= reputation <= +25
    Friendly, // +25 < reputation <= +70
    Allied,   // reputation > +70
}

impl RelationStance {
    /// Derive stance from reputation score.
    pub fn from_reputation(rep: f64) -> Self {
        if rep < -50.0 {
            RelationStance::Hostile
        } else if rep <= 25.0 {
            RelationStance::Neutral
        } else if rep <= 70.0 {
            RelationStance::Friendly
        } else {
            RelationStance::Allied
        }
    }
}

/// A diplomatic treaty between two factions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Treaty {
    /// Unique treaty identifier within the relation.
    pub id: u64,
    /// Type of treaty.
    pub treaty_type: TreatyType,
    /// Faction ID that proposed this treaty.
    pub proposed_by: u32,
    /// Remaining duration in ticks. None = permanent until violated.
    pub duration_ticks: Option<u64>,
    /// Concrete effect tags for fast gameplay lookup.
    pub effects: TreatyEffects,
    /// Whether this treaty has been violated.
    pub violated: bool,
    /// Whether a warning has been issued for this violation.
    pub warning_issued: bool,
    /// Tick when treaty was signed.
    pub signed_at_tick: u64,
}

impl Treaty {
    pub fn new(id: u64, treaty_type: TreatyType, proposed_by: u32, duration: Option<u64>, signed_at_tick: u64) -> Self {
        Self {
            id,
            treaty_type,
            proposed_by,
            duration_ticks: duration,
            effects: TreatyEffects::from_type(&treaty_type),
            violated: false,
            warning_issued: false,
            signed_at_tick,
        }
    }

    /// Whether this treaty is permanent (no duration limit).
    pub fn is_permanent(&self) -> bool {
        self.duration_ticks.is_none()
    }

    /// Advance duration by one tick. Returns false if treaty expired.
    pub fn tick(&mut self) -> bool {
        if let Some(remaining) = self.duration_ticks.as_mut() {
            if *remaining == 0 {
                return false;
            }
            *remaining -= 1;
        }
        true
    }
}

/// Treaty type variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TreatyType {
    NonAggressionPact,
    TradeAgreement,
    MilitaryAlliance,
    Vassalization,
}

/// Concrete effects of a treaty for gameplay checks.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TreatyEffects {
    /// If true, attacks against signatory are blocked.
    pub blocks_attacks: bool,
    /// Mining efficiency bonus (multiplicative) when trading with partner.
    pub mining_bonus: f64,
    /// MC per tick bonus from trade.
    pub trade_mc_per_tick: f64,
    /// Fleet defense bonus in allied territory.
    pub defense_bonus: f64,
    /// Tribute MC per tick (positive = paying, negative = receiving).
    pub tribute_mc_per_tick: f64,
    /// Master faction ID for vassalization.
    pub vassal_master: Option<u32>,
}

impl TreatyEffects {
    fn from_type(t: &TreatyType) -> Self {
        match t {
            TreatyType::NonAggressionPact => Self {
                blocks_attacks: true,
                mining_bonus: 0.0,
                trade_mc_per_tick: 0.0,
                defense_bonus: 0.0,
                tribute_mc_per_tick: 0.0,
                vassal_master: None,
            },
            TreatyType::TradeAgreement => Self {
                blocks_attacks: false,
                mining_bonus: 0.05,
                trade_mc_per_tick: 2.0,
                defense_bonus: 0.0,
                tribute_mc_per_tick: 0.0,
                vassal_master: None,
            },
            TreatyType::MilitaryAlliance => Self {
                blocks_attacks: false,
                mining_bonus: 0.0,
                trade_mc_per_tick: 0.0,
                defense_bonus: 0.20,
                tribute_mc_per_tick: 0.0,
                vassal_master: None,
            },
            TreatyType::Vassalization { master_id, tribute_mc_per_tick } => Self {
                blocks_attacks: true,
                mining_bonus: 0.0,
                trade_mc_per_tick: 0.0,
                defense_bonus: 0.0,
                tribute_mc_per_tick: *tribute_mc_per_tick,
                vassal_master: Some(*master_id),
            },
        }
    }
}

/// Relationship state between two factions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactionRelation {
    /// Ordered pair: (from_faction_id, to_faction_id).
    pub pair: (u32, u32),
    /// Reputation score: -100 (hostile) → 0 (neutral) → +100 (allied).
    pub reputation: f64,
    /// Cached stance derived from reputation.
    pub stance: RelationStance,
    /// All treaties currently active between this pair.
    pub treaties: Vec<Treaty>,
    /// Number of treaty violations committed by `from_faction` against `to_faction`.
    pub violations: u8,
    /// Pending proposal sent from `from_faction` to `to_faction` (None = none pending).
    pub pending_proposal: Option<DiplomaticProposal>,
    /// Tick of last diplomatic contact.
    pub last_contact_tick: u64,
}

impl FactionRelation {
    /// Create a new neutral relation between two factions.
    pub fn new(from: u32, to: u32) -> Self {
        Self {
            pair: (from, to),
            reputation: 0.0,
            stance: RelationStance::Neutral,
            treaties: Vec::new(),
            violations: 0,
            pending_proposal: None,
            last_contact_tick: 0,
        }
    }

    /// Update reputation and re-derive stance.
    pub fn set_reputation(&mut self, rep: f64) {
        self.reputation = rep.clamp(-100.0, 100.0);
        self.stance = RelationStance::from_reputation(self.reputation);
    }

    /// Add a reputation delta, clamped to [-100, 100].
    pub fn add_reputation(&mut self, delta: f64) {
        self.set_reputation(self.reputation + delta);
    }

    /// Add a treaty to this relation.
    pub fn add_treaty(&mut self, treaty: Treaty) {
        // Prevent duplicate treaty types (except vassalization which is unique)
        self.treaties.retain(|t| !t.treaty_type.can_coexist_with(&treaty.treaty_type));
        self.treaties.push(treaty);
    }

    /// Get the active treaty of a given type, if any.
    pub fn get_treaty(&self, t: &TreatyType) -> Option<&Treaty> {
        self.treaties.iter().find(|tr| tr.treaty_type == *t && !tr.violated)
    }

    /// Get mutable treaty of a given type, if any.
    pub fn get_treaty_mut(&mut self, t: &TreatyType) -> Option<&mut Treaty> {
        self.treaties.iter_mut().find(|tr| tr.treaty_type == *t && !tr.violated)
    }

    /// Remove all treaties.
    pub fn clear_treaties(&mut self) {
        self.treaties.clear();
    }

    /// Whether from_faction is at war with to_faction (violations >= 3 or reputation = -100).
    pub fn is_at_war(&self) -> bool {
        self.reputation <= -100.0 || self.violations >= 3
    }

    /// Whether there's an active NAP between these factions.
    pub fn has_nap(&self) -> bool {
        self.get_treaty(&TreatyType::NonAggressionPact).is_some()
    }

    /// Whether there's an active trade agreement.
    pub fn has_trade_agreement(&self) -> bool {
        self.get_treaty(&TreatyType::TradeAgreement).is_some()
    }

    /// Whether there's an active military alliance.
    pub fn has_alliance(&self) -> bool {
        self.get_treaty(&TreatyType::MilitaryAlliance).is_some()
    }
}

impl TreatyType {
    /// Whether two treaty types can coexist.
    pub fn can_coexist_with(&self, other: &TreatyType) -> bool {
        // NAP and Alliance can coexist; vassalization is unique and exclusive
        !matches!(self, TreatyType::Vassalization { .. }) && !matches!(other, TreatyType::Vassalization { .. })
    }
}

/// A diplomatic proposal sent from one faction to another.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiplomaticProposal {
    /// Unique proposal ID.
    pub id: u64,
    pub from_faction: u32,
    pub to_faction: u32,
    pub proposal_type: ProposalType,
    pub treaty_type: Option<TreatyType>,
    /// What the proposer is demanding (if any).
    pub demand: Option<Demand>,
    /// What the proposer is offering in exchange.
    pub offer: Option<Offer>,
    /// Ticks remaining before this proposal expires.
    pub expires_in_ticks: u64,
    /// Human-readable reason the AI generated this proposal / response.
    pub ai_reason: Option<String>,
}

impl DiplomaticProposal {
    /// Generate a unique proposal ID.
    pub fn new_id(counter: &mut u64) -> u64 {
        *counter += 1;
        *counter
    }
}

/// Type of diplomatic proposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProposalType {
    ProposeTreaty,
    DemandConcessions,
    OfferTrade,
    RequestCeasefire,
    RequestAlliance,
    VassalizationOffer,
}

/// What a proposer demands in exchange.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Demand {
    pub kind: DemandKind,
    pub value: f64,
}

/// Kind of demand.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DemandKind {
    /// resource_type_id and amount per tick.
    Resource(u32),
    /// One-time MC reparation.
    Reparation(f64),
    /// Technology key to transfer.
    TechTransfer(String),
}

/// What a proposer offers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Offer {
    pub kind: OfferKind,
    pub value: f64,
}

/// Kind of offer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OfferKind {
    /// resource_type_id and amount per tick.
    Resource(u32, f64),
    /// Technology key to share.
    TechTransfer(String),
    /// Military aid strength equivalent.
    MilitaryAid(f64),
}

/// Tracks diplomatic victory progress.
#[derive(Resource, Default)]
pub struct DiplomaticVictoryTracker {
    /// Cumulative diplomatic score per faction pair.
    pub faction_scores: HashMap<(u32, u32), f64>,
    /// Number of faction pairs currently at Allied stance.
    pub allied_count: u8,
    /// Required allied factions for diplomatic victory.
    pub allies_required: u8,
}

impl DiplomaticVictoryTracker {
    pub fn new(allies_required: u8) -> Self {
        Self {
            faction_scores: HashMap::new(),
            allied_count: 0,
            allies_required,
        }
    }

    /// Score earned when a treaty is signed.
    pub const TREATY_SIGN_BONUS: f64 = 10.0;
    /// Score earned per treaty year of sustained compliance.
    pub const COMPLIANCE_SCORE_PER_YEAR: f64 = 1.0;
}