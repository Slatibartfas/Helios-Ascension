use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::types::ResourceType;

/// Component that marks a star and defines its system properties
/// Used for multi-star system support with different frost lines
#[derive(Component, Debug, Clone, Copy, Serialize, Deserialize)]
pub struct StarSystem {
    /// Frost line distance in Astronomical Units for this star
    /// Beyond this distance, volatiles become more common
    /// Depends on star's luminosity and temperature
    pub frost_line_au: f64,
    
    /// Stellar classification (for future use: O, B, A, F, G, K, M)
    /// Can affect resource generation parameters
    pub spectral_class: SpectralClass,
}

impl StarSystem {
    /// Create a Sun-like (G-type) star system with standard frost line
    pub fn sun_like() -> Self {
        Self {
            frost_line_au: 2.5,
            spectral_class: SpectralClass::G,
        }
    }
    
    /// Create a custom star system with specified frost line
    pub fn new(frost_line_au: f64, spectral_class: SpectralClass) -> Self {
        Self {
            frost_line_au,
            spectral_class,
        }
    }
    
    /// Calculate frost line based on star luminosity (for procedural generation)
    /// 
    /// Uses the simplified formula: `frost_line = 2.7 * sqrt(L/L_sun)` AU
    /// 
    /// This is a first-order approximation based on stellar equilibrium temperature.
    /// More accurate models would account for:
    /// - Stellar age and protoplanetary disk composition
    /// - Stellar wind effects
    /// - Specific molecular freeze-out temperatures (H2O vs CH4 vs CO2)
    /// 
    /// For game purposes, this provides realistic variety across stellar types.
    pub fn from_luminosity(luminosity_solar: f64, spectral_class: SpectralClass) -> Self {
        let frost_line_au = 2.7 * luminosity_solar.sqrt();
        Self {
            frost_line_au,
            spectral_class,
        }
    }
}

impl Default for StarSystem {
    fn default() -> Self {
        Self::sun_like()
    }
}

/// Stellar spectral classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpectralClass {
    O, // Blue, very hot, luminous
    B, // Blue-white, hot
    A, // White
    F, // Yellow-white
    G, // Yellow (Sun-like)
    K, // Orange
    M, // Red, cool
}

/// Component that tracks which body (usually a star) this body orbits
/// Essential for multi-star system support
#[derive(Component, Debug, Clone, Copy)]
pub struct OrbitsBody {
    /// Entity of the parent body being orbited
    pub parent: Entity,
}

impl OrbitsBody {
    pub fn new(parent: Entity) -> Self {
        Self { parent }
    }
}

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
    fn test_star_system_sun_like() {
        let star = StarSystem::sun_like();
        assert_eq!(star.frost_line_au, 2.5);
        assert_eq!(star.spectral_class, SpectralClass::G);
    }

    #[test]
    fn test_star_system_from_luminosity() {
        // Red dwarf (M-type) with 0.04 solar luminosity
        let m_star = StarSystem::from_luminosity(0.04, SpectralClass::M);
        // frost_line ≈ 2.7 * sqrt(0.04) ≈ 0.54 AU
        assert!(m_star.frost_line_au > 0.5 && m_star.frost_line_au < 0.6);
        
        // Blue giant (A-type) with 40 solar luminosity
        let a_star = StarSystem::from_luminosity(40.0, SpectralClass::A);
        // frost_line ≈ 2.7 * sqrt(40) ≈ 17 AU
        assert!(a_star.frost_line_au > 16.0 && a_star.frost_line_au < 18.0);
    }

    #[test]
    fn test_orbits_body() {
        let parent_entity = Entity::from_raw(42);
        let orbits = OrbitsBody::new(parent_entity);
        assert_eq!(orbits.parent, parent_entity);
    }

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