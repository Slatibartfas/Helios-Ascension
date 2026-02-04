use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::types::ResourceType;

/// Represents a mineral deposit on a celestial body
/// Contains information about the quantity and ease of extraction
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MineralDeposit {
    /// Abundance of the resource (0.0 to 1.0, where 1.0 is extremely abundant)
    /// This represents the concentration and total amount available
    pub abundance: f64,
    
    /// Accessibility of the resource (0.0 to 1.0, where 1.0 is easily accessible)
    /// This represents how difficult it is to extract (depth, location, processing difficulty)
    pub accessibility: f32,
}

impl MineralDeposit {
    /// Create a new mineral deposit
    pub fn new(abundance: f64, accessibility: f32) -> Self {
        Self {
            abundance: abundance.clamp(0.0, 1.0),
            accessibility: accessibility.clamp(0.0, 1.0),
        }
    }

    /// Create a deposit with zero resources
    pub fn none() -> Self {
        Self {
            abundance: 0.0,
            accessibility: 0.0,
        }
    }

    /// Calculate the effective resource value (abundance * accessibility)
    /// This represents the practical value of the deposit
    pub fn effective_value(&self) -> f64 {
        self.abundance * self.accessibility as f64
    }

    /// Returns true if this deposit is economically viable (has meaningful resources)
    pub fn is_viable(&self) -> bool {
        self.effective_value() > 0.01
    }
}

impl Default for MineralDeposit {
    fn default() -> Self {
        Self::none()
    }
}

/// Component that stores all resource deposits on a planet or moon
#[derive(Component, Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlanetResources {
    /// Map of resource type to its deposit information
    pub deposits: HashMap<ResourceType, MineralDeposit>,
}

impl PlanetResources {
    /// Create a new empty resource collection
    pub fn new() -> Self {
        Self {
            deposits: HashMap::new(),
        }
    }

    /// Add or update a resource deposit
    pub fn add_deposit(&mut self, resource: ResourceType, deposit: MineralDeposit) {
        self.deposits.insert(resource, deposit);
    }

    /// Get a deposit for a specific resource
    pub fn get_deposit(&self, resource: &ResourceType) -> Option<&MineralDeposit> {
        self.deposits.get(resource)
    }

    /// Get the abundance of a specific resource (returns 0.0 if not present)
    pub fn get_abundance(&self, resource: &ResourceType) -> f64 {
        self.deposits
            .get(resource)
            .map(|d| d.abundance)
            .unwrap_or(0.0)
    }

    /// Get the accessibility of a specific resource (returns 0.0 if not present)
    pub fn get_accessibility(&self, resource: &ResourceType) -> f32 {
        self.deposits
            .get(resource)
            .map(|d| d.accessibility)
            .unwrap_or(0.0)
    }

    /// Get all viable (economically useful) deposits
    pub fn viable_deposits(&self) -> impl Iterator<Item = (&ResourceType, &MineralDeposit)> {
        self.deposits
            .iter()
            .filter(|(_, deposit)| deposit.is_viable())
    }

    /// Count the number of viable deposits
    pub fn viable_count(&self) -> usize {
        self.viable_deposits().count()
    }

    /// Calculate total resource value of this body
    pub fn total_value(&self) -> f64 {
        self.deposits
            .values()
            .map(|d| d.effective_value())
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mineral_deposit_creation() {
        let deposit = MineralDeposit::new(0.8, 0.6);
        assert_eq!(deposit.abundance, 0.8);
        assert_eq!(deposit.accessibility, 0.6);
    }

    #[test]
    fn test_mineral_deposit_clamping() {
        let deposit = MineralDeposit::new(1.5, -0.5);
        assert_eq!(deposit.abundance, 1.0);
        assert_eq!(deposit.accessibility, 0.0);
    }

    #[test]
    fn test_mineral_deposit_effective_value() {
        let deposit = MineralDeposit::new(0.8, 0.5);
        assert!((deposit.effective_value() - 0.4).abs() < 0.001);
    }

    #[test]
    fn test_mineral_deposit_viability() {
        let viable = MineralDeposit::new(0.5, 0.5);
        assert!(viable.is_viable());

        let not_viable = MineralDeposit::new(0.01, 0.01);
        assert!(!not_viable.is_viable());
    }

    #[test]
    fn test_planet_resources_add_and_get() {
        let mut resources = PlanetResources::new();
        let deposit = MineralDeposit::new(0.7, 0.8);
        
        resources.add_deposit(ResourceType::Iron, deposit);
        
        let retrieved = resources.get_deposit(&ResourceType::Iron).unwrap();
        assert_eq!(retrieved.abundance, 0.7);
        assert_eq!(retrieved.accessibility, 0.8);
    }

    #[test]
    fn test_planet_resources_viable_deposits() {
        let mut resources = PlanetResources::new();
        
        // Add viable deposit
        resources.add_deposit(ResourceType::Iron, MineralDeposit::new(0.8, 0.7));
        // Add non-viable deposit
        resources.add_deposit(ResourceType::Water, MineralDeposit::new(0.001, 0.001));
        
        assert_eq!(resources.viable_count(), 1);
    }

    #[test]
    fn test_planet_resources_total_value() {
        let mut resources = PlanetResources::new();
        
        resources.add_deposit(ResourceType::Iron, MineralDeposit::new(0.8, 0.5)); // 0.4
        resources.add_deposit(ResourceType::Water, MineralDeposit::new(0.6, 0.5)); // 0.3
        
        let total = resources.total_value();
        assert!((total - 0.7).abs() < 0.001);
    }
}
