//! AI Opponent plugin — Campaign AI and tactical combat AI.
//!
//! This plugin provides autonomous AI faction behavior for single-player sandbox mode.
//! AI factions expand colonies, build fleets, research technologies, and make combat decisions.
//!
//! # Architecture
//!
//! - `components`: AIFaction, AIPersonality, AIDifficulty, and related data structures
//! - `campaign`: High-level strategic decisions (expansion, building, priorities, research)
//! - `tactical`: Combat decisions (engage, retreat, hold, maneuver)
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
        let entity = world.spawn((faction, faction_research, AIControlledColony { faction_id })).id();
        self.factions.push(entity);
        entity
    }
}

/// Spawn default AI opponent factions for sandbox mode.
pub fn spawn_default_ai_opponents(world: &mut World) {
    let base_id = {
        let faction_list = world.resource::<AIFactionList>();
        faction_list.factions.len() as u32
    };

    // Spawn each faction entity directly into the world, then push to the list.
    let spawn = |world: &mut World, name: &str, personality: AIPersonality, difficulty: AIDifficulty, colour: [u8; 3], faction_id: u32| -> Entity {
        let faction = AIFaction::new(faction_id, name.to_string(), personality, difficulty, colour);
        let faction_research = AIFactionResearchState::new(personality.preferred_tech_categories());
        world.spawn((faction, faction_research, AIControlledColony { faction_id })).id()
    };

    let e1 = spawn(world, "Asteroids Collective", AIPersonality::Economic, AIDifficulty::Normal, [180, 140, 90], base_id + 1);
    let e2 = spawn(world, "Martian Directorate", AIPersonality::Militarist, AIDifficulty::Hard, [200, 60, 40], base_id + 2);
    let e3 = spawn(world, "Ceres Scientific Corps", AIPersonality::Scientific, AIDifficulty::Normal, [60, 180, 160], base_id + 3);

    {
        let mut faction_list = world.resource_mut::<AIFactionList>();
        faction_list.factions.push(e1);
        faction_list.factions.push(e2);
        faction_list.factions.push(e3);
    }

    info!(
        "Spawned 3 AI opponent factions: Entity({}), Entity({}), Entity({})",
        e1.index(), e2.index(), e3.index()
    );
}
