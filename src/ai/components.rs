//! AI faction components — personalities, difficulty, faction identity.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Difficulty level for AI opponents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AIDifficulty {
    /// 50% production efficiency, passive expansion, conservative combat
    Easy,
    /// 75% production efficiency, balanced strategy
    Normal,
    /// 100% production efficiency, aggressive expansion, smart combat
    Hard,
}

impl AIDifficulty {
    /// Production efficiency multiplier for this difficulty.
    pub fn production_multiplier(&self) -> f64 {
        match self {
            AIDifficulty::Easy => 0.5,
            AIDifficulty::Normal => 0.75,
            AIDifficulty::Hard => 1.0,
        }
    }

    /// Aggression multiplier affects combat behavior and fleet buildup rate.
    pub fn aggression_multiplier(&self) -> f64 {
        match self {
            AIDifficulty::Easy => 0.5,
            AIDifficulty::Normal => 0.75,
            AIDifficulty::Hard => 1.0,
        }
    }

    /// Tech research speed multiplier.
    pub fn research_multiplier(&self) -> f64 {
        match self {
            AIDifficulty::Easy => 0.6,
            AIDifficulty::Normal => 0.8,
            AIDifficulty::Hard => 1.0,
        }
    }
}

impl Default for AIDifficulty {
    fn default() -> Self {
        AIDifficulty::Normal
    }
}

/// AI personality archetypes that define strategic behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AIPersonality {
    /// Prioritises military production and fleet strength.
    /// Aggressive expansion, focuses on weapons and propulsion tech.
    Militarist,
    /// Prioritises economic growth and resource accumulation.
    /// Balanced expansion, focuses on mining and trade tech.
    Economic,
    /// Prioritises technology research and scientific advancement.
    /// Slow expansion but superior tech, focuses on propulsion and engineering.
    Scientific,
    /// Balanced approach across all dimensions.
    /// Flexible strategy adapts to competition.
    Balanced,
}

impl AIPersonality {
    /// Returns (colony_priority, fleet_priority, research_priority, wealth_priority)
    /// as factors 0.0–1.0 that guide resource allocation.
    pub fn priorities(&self) -> (f64, f64, f64, f64) {
        match self {
            AIPersonality::Militarist => (0.3, 0.5, 0.1, 0.1),
            AIPersonality::Economic => (0.35, 0.2, 0.15, 0.3),
            AIPersonality::Scientific => (0.3, 0.15, 0.4, 0.15),
            AIPersonality::Balanced => (0.3, 0.3, 0.2, 0.2),
        }
    }

    /// Preferred tech categories ranked by personality.
    pub fn preferred_tech_categories(&self) -> Vec<String> {
        match self {
            AIPersonality::Militarist => vec![
                "Military".to_string(),
                "Propulsion".to_string(),
                "Weapons".to_string(),
                "Defense".to_string(),
            ],
            AIPersonality::Economic => vec![
                "Mining".to_string(),
                "Trade".to_string(),
                "Construction".to_string(),
                "Colony".to_string(),
            ],
            AIPersonality::Scientific => vec![
                "Propulsion".to_string(),
                "Engineering".to_string(),
                "Weapons".to_string(),
                "Mining".to_string(),
            ],
            AIPersonality::Balanced => vec![
                "Colony".to_string(),
                "Propulsion".to_string(),
                "Military".to_string(),
                "Mining".to_string(),
            ],
        }
    }
}

impl Default for AIPersonality {
    fn default() -> Self {
        AIPersonality::Balanced
    }
}

/// Core AI faction component — attached to an entity that represents an AI player.
#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct AIFaction {
    /// Unique faction identifier.
    pub faction_id: u32,
    /// Display name (e.g., "Terran Dominion", "Asteroid Collective").
    pub name: String,
    /// Strategic personality driving decisions.
    pub personality: AIPersonality,
    /// Difficulty settings for this AI.
    pub difficulty: AIDifficulty,
    /// Faction colour for UI display (RGB 0–255).
    pub colour: [u8; 3],
    /// Entities of colonies controlled by this faction.
    pub colonies: Vec<Entity>,
    /// Entities of fleets controlled by this faction.
    pub fleets: Vec<Entity>,
    /// Current faction goals (updated each AI tick).
    pub goals: AIGoals,
    /// Tech categories this faction is currently focusing on.
    pub research_focus: Vec<String>,
    /// AI tick counter — decisions made every N ticks.
    #[serde(default)]
    pub tick_counter: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIGoals {
    /// Target number of colonies to maintain.
    pub target_colonies: u32,
    /// Desired fleet count (total ship count).
    pub target_fleet_size: u32,
    /// Primary system to expand toward.
    pub expansion_target: Option<String>,
    /// Whether this faction is in war mode.
    pub at_war: bool,
    /// Current diplomatic stance toward other factions.
    pub diplomatic_stance: HashMap<u32, DiplomaticStance>,
}

/// Diplomatic stance toward another faction.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum DiplomaticStance {
    Neutral,
    Friendly,
    Hostile,
}

impl AIFaction {
    /// Create a new AI faction with the given parameters.
    pub fn new(
        faction_id: u32,
        name: String,
        personality: AIPersonality,
        difficulty: AIDifficulty,
        colour: [u8; 3],
    ) -> Self {
        Self {
            faction_id,
            name,
            personality,
            difficulty,
            colour,
            colonies: Vec::new(),
            fleets: Vec::new(),
            goals: AIGoals {
                target_colonies: 3,
                target_fleet_size: 10,
                expansion_target: None,
                at_war: false,
                diplomatic_stance: HashMap::new(),
            },
            research_focus: personality.preferred_tech_categories(),
            tick_counter: 0,
        }
    }

    /// Called each AI tick to update tick counter.
    pub fn increment_tick(&mut self) {
        self.tick_counter = self.tick_counter.wrapping_add(1);
    }

    /// Whether this AI should make decisions this tick.
    /// Decisions happen every N ticks based on difficulty.
    pub fn should_decide(&self, tick_interval: u32) -> bool {
        self.tick_counter % tick_interval == 0
    }
}

/// Tag component to mark a colony as AI-controlled.
#[derive(Component, Debug, Clone)]
pub struct AIControlledColony {
    pub faction_id: u32,
}

/// Tag component to mark a fleet as AI-controlled.
#[derive(Component, Debug, Clone)]
pub struct AIControlledFleet {
    pub faction_id: u32,
}

/// AI decision context passed to decision systems each tick.
#[derive(Debug, Clone)]
pub struct AIDecisionContext {
    /// Faction entity this decision is for.
    pub faction_entity: Entity,
    /// Faction data.
    pub faction: AIFaction,
    /// All known celestial bodies with resources.
    pub available_bodies: Vec<Entity>,
    /// All known enemy fleets.
    pub enemy_fleets: Vec<Entity>,
    /// Faction treasury.
    pub treasury_mc: f64,
    /// Faction stockpile summaries.
    pub stockpile: HashMap<String, f64>,
}

/// Per-faction research state — tracks which technologies this faction has unlocked,
/// which research projects are active, and the ordered priority queue for new research.
#[derive(Component, Debug, Clone, Default, Serialize, Deserialize)]
pub struct AIFactionResearchState {
    /// Technologies this faction has unlocked.
    pub unlocked_technologies: Vec<String>,
    /// Tech IDs this faction is currently actively researching (in priority order).
    pub active_research: Vec<String>,
    /// Queue of tech IDs this faction wants to start but hasn't yet (higher = more urgent).
    pub research_queue: Vec<String>,
    /// Tech categories this faction is currently focusing on (from personality).
    pub focus_categories: Vec<String>,
    /// Total RP this faction has accumulated but not yet allocated.
    pub available_rp: f64,
}

impl AIFactionResearchState {
    /// Create a new per-faction research state.
    pub fn new(focus_categories: Vec<String>) -> Self {
        Self {
            unlocked_technologies: Vec::new(),
            active_research: Vec::new(),
            research_queue: Vec::new(),
            focus_categories,
            available_rp: 0.0,
        }
    }

    /// Check if this faction has unlocked a specific technology.
    pub fn is_unlocked(&self, tech_id: &str) -> bool {
        self.unlocked_technologies.contains(&tech_id.to_string())
    }

    /// Add a tech ID to the active research list.
    pub fn start_research(&mut self, tech_id: String) {
        if !self.active_research.contains(&tech_id) {
            self.active_research.push(tech_id);
        }
    }

    /// Remove a tech ID from active research (completed or cancelled).
    pub fn finish_research(&mut self, tech_id: &str) {
        self.active_research.retain(|t| t != tech_id);
    }

    /// Push a tech ID onto the research queue (priority order).
    pub fn enqueue_research(&mut self, tech_id: String) {
        if !self.research_queue.contains(&tech_id)
            && !self.active_research.contains(&tech_id)
        {
            self.research_queue.push(tech_id);
        }
    }

    /// Pop the highest priority research target from the queue.
    pub fn dequeue_research(&mut self) -> Option<String> {
        if self.research_queue.is_empty() {
            return None;
        }
        Some(self.research_queue.remove(0))
    }

    /// Check if a technology is already queued or active.
    pub fn is_queued(&self, tech_id: &str) -> bool {
        self.research_queue.contains(&tech_id.to_string())
            || self.active_research.contains(&tech_id.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_faction_research_state_basics() {
        let mut state = AIFactionResearchState::new(vec!["Military".to_string()]);
        assert!(!state.is_unlocked("FusionReactor"));

        state.unlocked_technologies.push("FusionReactor".to_string());
        assert!(state.is_unlocked("FusionReactor"));

        state.enqueue_research("PlasmaWeapons".to_string());
        assert!(state.is_queued("PlasmaWeapons"));

        let next = state.dequeue_research();
        assert_eq!(next, Some("PlasmaWeapons".to_string()));
        assert!(!state.is_queued("PlasmaWeapons"));
    }

    #[test]
    fn test_no_duplicate_research() {
        let mut state = AIFactionResearchState::default();
        state.enqueue_research("FusionReactor".to_string());
        state.enqueue_research("FusionReactor".to_string());
        assert_eq!(state.research_queue.len(), 1);
    }

    #[test]
    fn test_difficulty_multipliers() {
        assert!((AIDifficulty::Easy.production_multiplier() - 0.5).abs() < 0.001);
        assert!((AIDifficulty::Normal.production_multiplier() - 0.75).abs() < 0.001);
        assert!((AIDifficulty::Hard.production_multiplier() - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_personality_priorities() {
        let (col, fleet, res, wealth) = AIPersonality::Militarist.priorities();
        assert!(fleet > col);
        assert!(fleet > res);
        assert!(fleet > wealth);
    }

    #[test]
    fn test_ai_faction_creation() {
        let faction = AIFaction::new(
            1,
            "Test Faction".to_string(),
            AIPersonality::Scientific,
            AIDifficulty::Hard,
            [255, 0, 0],
        );
        assert_eq!(faction.name, "Test Faction");
        assert_eq!(faction.personality, AIPersonality::Scientific);
        assert_eq!(faction.tick_counter, 0);
    }

    #[test]
    fn test_should_decide() {
        let mut faction = AIFaction::new(
            1,
            "Test".to_string(),
            AIPersonality::Balanced,
            AIDifficulty::Normal,
            [0, 0, 0],
        );
        assert!(faction.should_decide(10));
        faction.increment_tick(); // tick = 1
        assert!(!faction.should_decide(10));
        faction.tick_counter = 10;
        assert!(faction.should_decide(10));
    }
}