//! Event system data model, ron loading, and event bus pub/sub.
//!
//! - `EventDef`, `EventChoice`, `Effect`, `Condition`: static event definitions from `.ron` files
//! - `EventsData`: resource holding all loaded event pools
//! - `EventBus`: pub/sub dispatcher — subscribe/unsubscribe, publish `GameEvent`
//! - Random event timer: 5-min checks, 30% roll, 15-min cooldown
//!
//! # Architecture
//! ```text
//! condition check → EventBus::publish(GameEvent) → subscriber callbacks
//!                                                              ↓
//!                                              EmitNotification → notification queue
//! ```

pub mod bus;
pub mod load_events;
pub mod systems;

// Re-exports
pub use bus::{EventBus, SubscriptionId};
pub use bus::EventBusPlugin;
pub use super::{EventCategory, EventTag, GameEvent};
pub use load_events::EventsData;
pub use systems::fire_story_event;

// ─── Data model types (shared with .ron loading) ───────────────────────────────

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Unique event identifier.
pub type EventId = &'static str;

/// Category determines how the event is triggered and displayed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventCategory {
    /// Triggered when game state matches conditions (one-shot per campaign).
    Story,
    /// Drawn periodically from a pool.
    Random,
    /// Immediate crisis — bypasses random roll.
    Alert,
}

/// Tags describe the event's nature for filtering and display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventTag {
    Discovery,
    Disaster,
    Opportunity,
    Combat,
    Diplomacy,
    Economy,
    Research,
    StoryMilestone,
    Alien,
    Ancient,
    Refugee,
    Pirate,
}

/// Delayed effect — fires after a game-time offset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelayedEffect {
    pub delay_seconds: f64,
    pub effect: Effect,
}

/// An immediate or delayed mechanical effect.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Effect {
    pub effect_type: EffectType,
    pub target: EffectTarget,
    pub magnitude: f64,
    pub source_event: EventId,
}

/// What an effect operates on.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EffectTarget {
    /// Apply to a specific colony entity.
    Colony(Entity),
    /// Apply to a specific fleet entity.
    Fleet(Entity),
    /// Apply to a specific star/body entity.
    Body(Entity),
    /// Apply globally to the whole empire.
    Global,
    /// Apply to a random entity matching criteria.
    Random(Vec<EventTag>),
}

/// The mechanical type of an effect.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EffectType {
    /// Grant a technology by ID.
    GrantTech,
    /// Add or remove resources (resource_id, amount).
    ModifyResources { resource_id: String },
    /// Change diplomatic relation (faction_id, delta).
    ModifyRelation { faction_id: u32 },
    /// Apply a stability modifier to a colony.
    ModifyStability { delta: i32 },
    /// Spawn a fleet from a template.
    SpawnFleet { template: String },
    /// Add a trait to a body.
    AddBodyTrait { trait_name: String },
    /// Trigger another event by ID.
    TriggerEvent { event_id: EventId },
    /// Modify research speed for a category.
    ModifyResearchSpeed { category: String, multiplier: f64 },
}

/// A condition that must be met for an event to fire.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Condition {
    /// Player has at least N colonies.
    ColonyCountAtLeast(u32),
    /// A specific technology has been researched.
    TechResearched(String),
    /// Player has at least N of a resource.
    ResourceAtLeast { resource_id: String, amount: f64 },
    /// Game is at a specific act (1-5).
    ActAtLeast(u8),
    /// A specific event has already fired this campaign.
    EventFired(EventId),
    /// A specific body has been surveyed.
    BodySurveyed(Entity),
    /// Player fleet count is at least N.
    FleetCountAtLeast(u32),
    /// Faction relation is below a threshold.
    FactionRelationBelow { faction_id: u32, threshold: i32 },
}

/// Outcome when a choice is selected — one is chosen randomly by weight.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Outcome {
    pub weight: f64,
    pub effects: Vec<Effect>,
    /// If set, creates a mission with this ID template.
    pub mission_id: Option<String>,
}

/// A single choice the player can make (or none for forced events).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventChoice {
    pub label: String,
    pub outcomes: Vec<Outcome>,
    /// Optional advisor recommendation key.
    pub ai_recommendation: Option<String>,
}

/// Static event definition loaded from ron files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventDef {
    pub id: EventId,
    pub category: EventCategory,
    pub tags: Vec<EventTag>,
    pub title: String,
    pub description: String,
    pub choices: Vec<EventChoice>,
    pub immediate_effects: Vec<Effect>,
    pub delayed_effects: Vec<DelayedEffect>,
    pub trigger_conditions: Vec<Condition>,
    pub cooldown_minutes: Option<u32>,
    pub repeat: bool,
}

/// Random event pool definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RandomEventPool {
    pub pool_id: String,
    pub events: Vec<EventDef>,
}

/// All event data loaded from ron files.
#[derive(Resource, Default)]
pub struct EventsData {
    pub story_events: HashMap<EventId, EventDef>,
    pub discovery_pool: Vec<EventDef>,
    pub disaster_pool: Vec<EventDef>,
    pub opportunity_pool: Vec<EventDef>,
    pub crisis_pool: Vec<EventDef>,
}

impl EventsData {
    pub fn get_story_event(&self, id: EventId) -> Option<&EventDef> {
        self.story_events.get(id)
    }

    pub fn get_random_pool(&self, pool_id: &str) -> Option<&Vec<EventDef>> {
        match pool_id {
            "discovery" => Some(&self.discovery_pool),
            "disaster" => Some(&self.disaster_pool),
            "opportunity" => Some(&self.opportunity_pool),
            "crisis" => Some(&self.crisis_pool),
            _ => None,
        }
    }
}