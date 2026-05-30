//! Event hooks system for Helios Ascension mods.
//!
//! Mods can register callback functions that are invoked when specific game events occur.
//! This allows mods to react to game state changes and extend game behavior.
//!
//! # Available Hooks
//!
//! | Hook | When Called | Payload |
//! |------|-------------|--------|
//! | `on_colony_built` | Colony established on a body | `ColonyBuiltEvent` |
//! | `on_combat_end` | Combat concludes | `CombatEndEvent` |
//! | `on_research_complete` | Tech research finishes | `ResearchCompleteEvent` |
//! | `on_resource_discovered` | New resource deposit found | `ResourceDiscoveryEvent` |
//! | `on_ship_built` | Ship/station constructed | `ShipBuiltEvent` |
//! | `on_year_tick` | Start of each game year | `YearTickEvent` |
//!
//! # Registering Hooks
//!
//! ```rust
//! use crate::modding::hooks::{Hook, HookRegistry};
//!
//! // In your mod's init function:
//! HookRegistry::register(Hook::OnColonyBuilt { callback: my_colony_hook });
//! ```
//!
//! # Implementing a Hook Callback
//!
//! ```rust
//! fn my_colony_hook(event: &ColonyBuiltEvent, world: &mut World) {
//!     info!("Colony built at: {:?}", event.body_name);
//!     // Add your custom logic here
//! }
//! ```

use bevy::prelude::*;
use std::collections::HashMap;
use std::any::TypeId;

/// Event fired when a colony is established
#[derive(Debug, Clone, Event)]
pub struct ColonyBuiltEvent {
    pub colony_entity: Entity,
    pub body_name: String,
    pub population: u32,
}

/// Event fired when combat concludes
#[derive(Debug, Clone, Event)]
pub struct CombatEndEvent {
    pub winner: Option<Entity>,
    pub loser: Option<Entity>,
    pub participants: Vec<Entity>,
}

/// Event fired when research on a technology completes
#[derive(Debug, Clone, Event)]
pub struct ResearchCompleteEvent {
    pub tech_id: String,
    pub researcher_entity: Entity,
}

/// Event fired when a new resource deposit is discovered
#[derive(Debug, Clone, Event)]
pub struct ResourceDiscoveryEvent {
    pub body_name: String,
    pub resource_type: String,
    pub amount: f64,
}

/// Event fired when a ship or station is constructed
#[derive(Debug, Clone, Event)]
pub struct ShipBuiltEvent {
    pub ship_entity: Entity,
    pub ship_name: String,
    pub ship_class: String,
}

/// Event fired at the start of each game year
#[derive(Debug, Clone, Event)]
pub struct YearTickEvent {
    pub year: i32,
}

/// Hook types that can be registered
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum Hook {
    OnColonyBuilt,
    OnCombatEnd,
    OnResearchComplete,
    OnResourceDiscovery,
    OnShipBuilt,
    OnYearTick,
}

/// A hook callback function
pub type HookCallback = Box<dyn Fn(&dyn std::any::Any, &mut World) + Send + Sync>;

/// Registry of all registered hooks
pub struct HookRegistry {
    hooks: HashMap<Hook, Vec<HookCallback>>,
}

impl Default for HookRegistry {
    fn default() -> Self {
        Self {
            hooks: HashMap::new(),
        }
    }
}

impl HookRegistry {
    /// Register a callback for a specific hook
    pub fn register(hook: Hook, callback: HookCallback) {
        debug!("Registering hook: {:?}", hook);
        HookRegistry::instance().hooks.entry(hook).or_default().push(callback);
    }

    /// Trigger all callbacks for a hook
    pub fn trigger<H: Event>(hook: Hook, event: &H, world: &mut World) {
        if let Some(callbacks) = HookRegistry::instance().hooks.get(&hook) {
            let event_any = event as &dyn std::any::Any;
            for callback in callbacks {
                callback(event_any, world);
            }
        }
    }

    fn instance() -> std::sync::MutexGuard<'static, HookRegistry> {
        static INSTANCE: std::sync::Mutex<HookRegistry> = std::sync::Mutex::new(HookRegistry::default());
        INSTANCE.lock().expect("Failed to lock HookRegistry")
    }
}

/// System to process colony built hooks
pub fn process_colony_built_hooks(
    mut events: EventReader<ColonyBuiltEvent>,
    mut world: &mut World,
) {
    for event in events.read() {
        HookRegistry::trigger(Hook::OnColonyBuilt, event, &mut world);
    }
}

/// System to process combat end hooks
pub fn process_combat_end_hooks(
    mut events: EventReader<CombatEndEvent>,
    mut world: &mut World,
) {
    for event in events.read() {
        HookRegistry::trigger(Hook::OnCombatEnd, event, &mut world);
    }
}

/// System to process research complete hooks
pub fn process_research_complete_hooks(
    mut events: EventReader<ResearchCompleteEvent>,
    mut world: &mut World,
) {
    for event in events.read() {
        HookRegistry::trigger(Hook::OnResearchComplete, event, &mut world);
    }
}

/// System to process resource discovery hooks
pub fn process_resource_discovery_hooks(
    mut events: EventReader<ResourceDiscoveryEvent>,
    mut world: &mut World,
) {
    for event in events.read() {
        HookRegistry::trigger(Hook::OnResourceDiscovery, event, &mut world);
    }
}

/// System to process ship built hooks
pub fn process_ship_built_hooks(
    mut events: EventReader<ShipBuiltEvent>,
    mut world: &mut World,
) {
    for event in events.read() {
        HookRegistry::trigger(Hook::OnShipBuilt, event, &mut world);
    }
}

/// System to process year tick hooks
pub fn process_year_tick_hooks(
    mut events: EventReader<YearTickEvent>,
    mut world: &mut World,
) {
    for event in events.read() {
        HookRegistry::trigger(Hook::OnYearTick, event, &mut world);
    }
}

/// Called at startup to register any built-in mod hooks
pub fn register_mod_hooks() {
    debug!("Registering mod hook systems...");
    // Hook systems are registered as Bevy systems in the modding plugin
    // Individual hooks are invoked by game systems via HookRegistry::trigger()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_types() {
        let hook1 = Hook::OnColonyBuilt;
        let hook2 = Hook::OnColonyBuilt;
        let hook3 = Hook::OnCombatEnd;

        assert_eq!(hook1, hook2);
        assert_ne!(hook1, hook3);
    }
}
