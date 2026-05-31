//! Diplomacy module for faction relations management
//!
//! Handles diplomatic relationships between factions including
//! reputation tracking, treaties, and diplomatic states.

use bevy::prelude::*;

/// Graph of diplomatic relations between all factions.
/// Edges carry a reputation score that affects treaty availability.
#[derive(Debug, Clone, Resource)]
pub struct RelationsGraph {
    /// Player's relation toward each faction (faction_id → relation).
    /// Faction IDs match those in `FactionId::new()`.
    player_relations: std::collections::HashMap<u32, PlayerRelation>,
}

#[derive(Debug, Clone)]
pub struct PlayerRelation {
    pub reputation: f64,
    pub treaty: Option<Treaty>,
    pub war: bool,
}

#[derive(Debug, Clone)]
pub struct Treaty {
    pub name: String,
    pub years_remaining: f64,
}

impl RelationsGraph {
    /// Create a new empty relations graph.
    pub fn new() -> Self {
        Self {
            player_relations: std::collections::HashMap::new(),
        }
    }

    /// Get a mutable reference to the player's relation toward a faction.
    pub fn player_relation_mut(&mut self, faction_id: u32) -> Option<&mut PlayerRelation> {
        self.player_relations.get_mut(&faction_id)
    }

    /// Initialize a relation entry for a faction with zero reputation.
    pub fn init_faction(&mut self, faction_id: u32) {
        self.player_relations.entry(faction_id).or_insert(PlayerRelation {
            reputation: 0.0,
            treaty: None,
            war: false,
        });
    }
}

impl Default for RelationsGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl PlayerRelation {
    /// Add reputation delta (can be negative).
    pub fn add_reputation(&mut self, delta: f64) {
        self.reputation += delta;
    }
}