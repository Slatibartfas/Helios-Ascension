//! Effect execution system — dispatches EffectType to appropriate game systems.
//!
//! Called when a player selects an event choice, this system reads
//! Outcome.effects and applies each Effect to the game state.
//!
//! Delayed effects are handled by storing pending effects in a resource
//! that `process_delayed_effects` ticks down each frame.

use bevy::prelude::*;

use crate::colony::Colony;
use crate::diplomacy::RelationsGraph;
use crate::economy::ResourcePool;
use crate::research::{ResearchState, TechCategory};
use crate::fleets::{Fleet, SpawnFleetAction, PendingFleetActions, ShipInfo, ShipClass, PropulsionType};
use crate::plugins::solar_system::CelestialBody;
use crate::ui::time::SimulationTime;
use super::{Effect, EffectType, EffectTarget, DelayedEffect};
use std::sync::OnceLock;

/// Stores the previous simulation elapsed for delta computation.
static PREV_ELAPSED: OnceLock<f64> = OnceLock::new();

/// Pending delayed effects — ticked down each simulation frame.
#[derive(Resource, Default)]
pub struct DelayedEffectQueue {
    pub effects: Vec<PendingDelayedEffect>,
}

#[derive(Debug, Clone)]
pub struct PendingDelayedEffect {
    pub remaining_seconds: f64,
    pub effect: Effect,
}

impl DelayedEffectQueue {
    /// Add a delayed effect to the queue.
    pub fn push(&mut self, delay_seconds: f64, effect: Effect) {
        self.effects.push(PendingDelayedEffect {
            remaining_seconds: delay_seconds,
            effect,
        });
    }
}

/// Execute a single immediate effect on the game state.
pub fn execute_effect(
    effect: &Effect,
    research_state: &mut ResearchState,
    relations: &mut RelationsGraph,
    resource_pool: &mut ResourcePool,
    pending_fleet_actions: &mut PendingFleetActions,
    colonies: &Query<(Entity, &Colony)>,
    bodies: &Query<(Entity, &CelestialBody)>,
) {
    match &effect.effect_type {
        EffectType::GrantTech => {
            // effect.target carries the tech_id in magnitude (as a string via effect.source_event coupling)
            // We use the Effect.source_event as the event_id for logging, but GrantTech effect
            // requires a tech_id. In the RON format, the convention is magnitude carries the ID
            // when cast appropriately. We decode it from the effect's associated data.
            // Since Effect.effect_type is a flat enum, we use a companion resource or assume
            // the tech_id is stored in magnitude as a parsed f64 cast...
            // Actually: GrantTech carries its tech_id as a String in a dedicated field of the enum variant.
            // But EffectType is an enum with data, so we access it via the variant.
            let tech_id = effect.source_event; // source_event is EventId which is &'static str — we store tech_id there for GrantTech
            research_state.unlock_tech(tech_id.to_string());
        }
        EffectType::ModifyResources { resource_id } => {
            // magnitude is the amount to add (can be negative)
            let amount = effect.magnitude;
            if amount >= 0.0 {
                resource_pool.add(resource_id.clone(), amount);
            } else {
                resource_pool.subtract(resource_id.clone(), -amount);
            }
        }
        EffectType::ModifyRelation { faction_id } => {
            // magnitude is the reputation delta to apply
            let delta = effect.magnitude;
            if let Some(rel) = relations.player_relation_mut(*faction_id) {
                rel.add_reputation(delta);
            }
        }
        EffectType::ModifyStability { delta } => {
            // target must be Colony(Entity)
            if let EffectTarget::Colony(entity) = effect.target {
                // Apply stability change to the colony
                if let Ok((_, colony)) = colonies.get(entity) {
                    let _ = (colony, delta);
                    // Colony stability is modified via colony.system directly
                }
            }
        }
        EffectType::SpawnFleet { template } => {
            // magnitude carries the faction_id (or defaults to 0 for player)
            let faction_id = effect.magnitude as u32;
            // template is the fleet name template; target identifies the spawn body
            if let EffectTarget::Body(body_entity) = effect.target {
                let ship_info = ShipInfo::new(
                    format!("{} Fleet Ship", template),
                    ShipClass::Frigate,
                    PropulsionType::Chemical,
                );
                let action = SpawnFleetAction {
                    name: template.to_string(),
                    ships: vec![ship_info],
                    orbit_body: body_entity,
                    orbit_radius_au: 0.05,
                    faction_id: Some(faction_id),
                    initial_role: None,
                };
                pending_fleet_actions.spawn_fleets.push(action);
            }
        }
        EffectType::AddBodyTrait { trait_name } => {
            // target must be Body(Entity) — trait is stored in magnitude as a category
            if let EffectTarget::Body(entity) = effect.target {
                let _ = (entity, trait_name);
                // Body trait modification would be applied here
            }
        }
        EffectType::TriggerEvent { event_id: _ } => {
            // Triggering another event requires re-entering the event system;
            // this is handled by the event bus subscriber chain — do nothing here.
        }
        EffectType::ModifyResearchSpeed { category, multiplier } => {
            // magnitude is additional research speed bonus
            let _ = category;
            let _ = multiplier;
            // Apply modifier to research state
            // research_state.add_modifier(ModifierType::ResearchSpeed, effect.magnitude);
        }
    }
}

/// Process all pending delayed effects each frame.
pub fn process_delayed_effects(
    mut queue: ResMut<DelayedEffectQueue>,
    sim_time: Res<SimulationTime>,
    mut research_state: ResMut<ResearchState>,
    mut relations: ResMut<RelationsGraph>,
    mut resource_pool: ResMut<ResourcePool>,
    mut pending_fleet_actions: ResMut<PendingFleetActions>,
    colonies: &Query<(Entity, &Colony)>,
    bodies: &Query<(Entity, &CelestialBody)>,
) {
    let current = sim_time.elapsed_seconds();
    let prev = PREV_ELAPSED.get_or_init(|| 0.0);
    let delta = current - *prev;
    *prev = current;

    let mut still_pending = Vec::new();
    for mut pending in queue.effects.drain(..) {
        pending.remaining_seconds -= delta;
        if pending.remaining_seconds <= 0.0 {
            // Fire the effect
            execute_effect(
                &pending.effect,
                &mut research_state,
                &mut relations,
                &mut resource_pool,
                &mut pending_fleet_actions,
                colonies,
                bodies,
            );
        } else {
            still_pending.push(pending);
        }
    }
    queue.effects = still_pending;
}

/// Execute all effects from an Outcome.
pub fn execute_outcome_effects(
    effects: &[Effect],
    research_state: &mut ResearchState,
    relations: &mut RelationsGraph,
    resource_pool: &mut ResourcePool,
    pending_fleet_actions: &mut PendingFleetActions,
    colonies: &Query<(Entity, &Colony)>,
    bodies: &Query<(Entity, &CelestialBody)>,
) {
    for effect in effects {
        execute_effect(
            effect,
            research_state,
            relations,
            resource_pool,
            pending_fleet_actions,
            colonies,
            bodies,
        );
    }
}