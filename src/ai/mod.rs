//! AI Opponent plugin — Campaign AI and tactical combat AI.
//!
//! This plugin provides autonomous AI faction behavior for single-player sandbox mode.
//! AI factions expand colonies, build fleets, research technologies, and make combat decisions.
//!
//! # Architecture
//!
//! - `components`: AIFaction, AIPersonality, AIDifficulty, and related data structures
//! - `campaign`: High-level strategic decisions (expansion, building priorities, research)
//! - `tactical`: Combat decisions (engage/retreat/hold, maneuver, targeting)
//! - `plugin`: Bevy plugin that registers all AI systems
//!
//! # Usage
//!
//! AI factions are spawned as entities with `AIFaction` component during game setup.
//! The `AIFactionPlugin` systems run each simulation tick to update AI decisions.
//!
//! # Difficulty Scaling
//!
//! - **Easy**: 50% production, 60% research, passive expansion, conservative combat
//! - **Normal**: 75% production, 80% research, balanced strategy
//! - **Hard**: 100% production, 100% research, aggressive expansion, smart combat
//!
//! # AI Personalities
//!
//! - **Militarist**: Prioritises military production and fleet strength
//! - **Economic**: Focuses on resource accumulation and trade
//! - **Scientific**: Pursues technology leadership with slow expansion
//! - **Balanced**: Flexible approach adapts to game state

pub mod campaign;
pub mod components;
pub mod tactical;

use bevy::prelude::*;

use campaign::run_campaign_ai;
use tactical::run_tactical_ai;
use components::{AIFaction, AIPersonality, AIDifficulty, AIControlledColony, AIControlledFleet, AIFactionResearchState};

/// AI Faction plugin — provides autonomous opponent behavior.
pub struct AIFactionPlugin;

impl Plugin for AIFactionPlugin {
    fn build(&self, app: &mut App) {
        // Register AI components.
        app.register_type::<AIFaction>();
        app.register_type::<AIPersonality>();
        app.register_type::<AIDifficulty>();
        app.register_type::<AIControlledColony>();
        app.register_type::<AIControlledFleet>();
        app.register_type::<AIFactionResearchState>();

        // Register AI faction resource.
        app.init_resource::<AIFactionList>();

        // Add AI systems — run in Update (not EguiPrimaryContextPass).
        // Campaign AI: strategic decisions (expansion, building, research).
        app.add_systems(Update, run_campaign_ai);
        // Tactical AI: combat decisions (engage, hold, retreat).
        app.add_systems(Update, run_tactical_ai);
    }
}

/// Resource holding all AI factions in the game world.
#[derive(Resource, Debug, Default)]
pub struct AIFactionList {
    pub factions: Vec<Entity>,
}

impl AIFactionList {
    /// Spawn a new AI faction entity with default configuration.
    pub fn spawn_faction(
        &mut self,
        world: &mut World,
        name: String,
        personality: AIPersonality,
        difficulty: AIDifficulty,
        colour: [u8; 3],
    ) -> Entity {
        let faction_id = self.factions.len() as u32 + 1;
        let faction = AIFaction::new(faction_id, name, personality, difficulty, colour);
        let faction_research = AIFactionResearchState::new(personality.preferred_tech_categories());
        let entity = world.spawn((faction, faction_research, AIControlledColony { faction_id }));
        self.factions.push(entity);
        entity
    }
}

/// Spawn default AI opponent factions for sandbox mode.
pub fn spawn_default_ai_opponents(world: &mut World) {
    let mut faction_list = world.resource_mut::<AIFactionList>();

    // Asteroids Collective — Economic AI (medium difficulty).
    faction_list.spawn_faction(
        world,
        "Asteroids Collective".to_string(),
        AIPersonality::Economic,
        AIDifficulty::Normal,
        [180, 140, 90], // Rust-orange colour.
    );

    // Martian Directorate — Militarist AI (hard difficulty).
    faction_list.spawn_faction(
        world,
        "Martian Directorate".to_string(),
        AIPersonality::Militarist,
        AIDifficulty::Hard,
        [200, 60, 40], // Crimson colour.
    );

    // Ceres Scientific Corps — Scientific AI (normal difficulty).
    faction_list.spawn_faction(
        world,
        "Ceres Scientific Corps".to_string(),
        AIPersonality::Scientific,
        AIDifficulty::Normal,
        [60, 180, 160], // Teal colour.
    );

    info!(
        "Spawned {} AI opponent factions: {:?}",
        faction_list.factions.len(),
        faction_list
            .factions
            .iter()
            .map(|e| format!("Entity({})", e.index()))
            .collect::<Vec<_>>()
    );
}