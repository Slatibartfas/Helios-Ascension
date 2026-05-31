//! Resource pool for direct resource manipulation by game systems.
//!
//! Unlike [`GlobalBudget`] which handles economic accounting and logistics,
//! `ResourcePool` provides a direct interface for systems like events/effects
//! to modify resource stockpiles without going through the logistics layer.

use bevy::prelude::*;
use crate::economy::types::ResourceType;

/// Direct access to the game's resource stockpiles.
/// Used by event/effect systems that need to immediately add or subtract
/// resources (e.g. triggered by events, cheats, or debug commands).
///
/// For normal economic flow (mining, consumption, logistics), use
/// [`GlobalBudget`] and the logistics system instead.
#[derive(Debug, Clone, Resource)]
pub struct ResourcePool {
    /// Current stockpile quantities keyed by resource type.
    stockpiles: std::collections::HashMap<ResourceType, f64>,
}

impl ResourcePool {
    /// Create a new empty resource pool.
    pub fn new() -> Self {
        Self {
            stockpiles: std::collections::HashMap::new(),
        }
    }

    /// Add `amount` of `resource_id` to the stockpile.
    pub fn add(&mut self, resource_id: ResourceType, amount: f64) {
        *self.stockpiles.entry(resource_id).or_insert(0.0) += amount;
    }

    /// Subtract `amount` of `resource_id` from the stockpile.
    /// Does not go below zero.
    pub fn subtract(&mut self, resource_id: ResourceType, amount: f64) {
        let entry = self.stockpiles.entry(resource_id).or_insert(0.0);
        *entry = (*entry - amount).max(0.0);
    }

    /// Get the current stockpile for a resource.
    pub fn get(&self, resource_id: ResourceType) -> f64 {
        self.stockpiles.get(&resource_id).copied().unwrap_or(0.0)
    }
}

impl Default for ResourcePool {
    fn default() -> Self {
        Self::new()
    }
}