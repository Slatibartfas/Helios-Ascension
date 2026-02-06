//! Galaxy-scale data structures
//! 
//! Defines the data format for loading multiple star systems from configuration files.

use serde::{Deserialize, Serialize};
use bevy::math::DVec3;

/// Galaxy data containing multiple star systems
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GalaxyData {
    /// Name of the galaxy or region
    pub name: String,
    
    /// Description
    #[serde(default)]
    pub description: String,
    
    /// List of star systems in this galaxy
    pub systems: Vec<StarSystemDefinition>,
    
    /// Optional configuration for procedural generation
    #[serde(default)]
    pub procedural_config: Option<ProceduralConfig>,
}

/// Definition of a single star system for loading
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StarSystemDefinition {
    /// Unique identifier
    pub id: u64,
    
    /// System name (e.g., "Sol", "Alpha Centauri")
    pub name: String,
    
    /// Star spectral type (e.g., "G2V", "M5.5V", "A1V")
    pub star_type: String,
    
    /// Position in galactic coordinates (light years)
    /// Tuple format for RON: (x, y, z)
    pub position: (f64, f64, f64),
    
    /// Path to the system data file (relative to assets/)
    pub data_file: String,
    
    /// Whether this is the starting system for the player
    #[serde(default)]
    pub starting_system: bool,
}

impl StarSystemDefinition {
    /// Convert position tuple to DVec3
    pub fn position_vec(&self) -> DVec3 {
        DVec3::new(self.position.0, self.position.1, self.position.2)
    }
}

/// Configuration for procedural system generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProceduralConfig {
    /// Enable procedural generation
    pub enable: bool,
    
    /// Random seed for generation
    pub seed: u64,
    
    /// Minimum number of systems to generate
    pub min_systems: usize,
    
    /// Maximum number of systems to generate
    pub max_systems: usize,
    
    /// Maximum distance from origin for generated systems (light years)
    pub generation_radius_ly: f64,
}

impl GalaxyData {
    /// Load galaxy data from a RON file
    pub fn load_from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let contents = std::fs::read_to_string(path)?;
        let data: GalaxyData = ron::from_str(&contents)?;
        Ok(data)
    }
    
    /// Get the starting system definition
    pub fn starting_system(&self) -> Option<&StarSystemDefinition> {
        self.systems.iter().find(|s| s.starting_system)
    }
    
    /// Get a system by ID
    pub fn get_system(&self, id: u64) -> Option<&StarSystemDefinition> {
        self.systems.iter().find(|s| s.id == id)
    }
    
    /// Get a system by name
    pub fn get_system_by_name(&self, name: &str) -> Option<&StarSystemDefinition> {
        self.systems.iter().find(|s| s.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_galaxy_data_structure() {
        let galaxy = GalaxyData {
            name: "Test Galaxy".to_string(),
            description: "A test galaxy".to_string(),
            systems: vec![
                StarSystemDefinition {
                    id: 0,
                    name: "Sol".to_string(),
                    star_type: "G2V".to_string(),
                    position: (0.0, 0.0, 0.0),
                    data_file: "sol.ron".to_string(),
                    starting_system: true,
                },
                StarSystemDefinition {
                    id: 1,
                    name: "Alpha Centauri".to_string(),
                    star_type: "G2V".to_string(),
                    position: (4.37, 0.0, 0.0),
                    data_file: "alpha_centauri.ron".to_string(),
                    starting_system: false,
                },
            ],
            procedural_config: None,
        };
        
        assert_eq!(galaxy.systems.len(), 2);
        assert!(galaxy.starting_system().is_some());
        assert_eq!(galaxy.starting_system().unwrap().name, "Sol");
        assert_eq!(galaxy.get_system(1).unwrap().name, "Alpha Centauri");
        assert_eq!(galaxy.get_system_by_name("Sol").unwrap().id, 0);
    }

    #[test]
    fn test_position_conversion() {
        let system = StarSystemDefinition {
            id: 0,
            name: "Test".to_string(),
            star_type: "G2V".to_string(),
            position: (1.0, 2.0, 3.0),
            data_file: "test.ron".to_string(),
            starting_system: false,
        };
        
        let pos = system.position_vec();
        assert_eq!(pos, DVec3::new(1.0, 2.0, 3.0));
    }
}
