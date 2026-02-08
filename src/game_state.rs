//! Game state and seed management
//! 
//! Provides resources for managing game state, including procedural generation seeds
//! for deterministic and reproducible game worlds. This is essential for save/load
//! functionality.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Resource that stores the seed used for procedural generation
/// This seed determines all procedurally generated content in the game,
/// making generation deterministic and allowing for save/load functionality
#[derive(Resource, Debug, Clone, Copy, Serialize, Deserialize)]
pub struct GameSeed {
    /// The seed value used for procedural generation
    pub value: u64,
}

impl GameSeed {
    /// Create a new game seed from a specific value
    pub fn new(seed: u64) -> Self {
        Self { value: seed }
    }
    
    /// Create a new game seed from the current system time
    /// This generates a unique seed for each game session
    pub fn from_system_time() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs();
        
        info!("Generated game seed from system time: {}", seed);
        Self { value: seed }
    }
    
    /// Create a game seed from a string (for debug/testing)
    /// Uses a simple hash of the string as the seed
    pub fn from_string(s: &str) -> Self {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        s.hash(&mut hasher);
        let seed = hasher.finish();
        
        info!("Generated game seed from string '{}': {}", s, seed);
        Self { value: seed }
    }
}

impl Default for GameSeed {
    fn default() -> Self {
        Self::from_system_time()
    }
}

/// Plugin that manages game state and initialization
pub struct GameStatePlugin;

impl Plugin for GameStatePlugin {
    fn build(&self, app: &mut App) {
        // Initialize the game seed at startup
        app.init_resource::<GameSeed>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_game_seed_creation() {
        let seed1 = GameSeed::new(12345);
        assert_eq!(seed1.value, 12345);
        
        let seed2 = GameSeed::from_system_time();
        assert!(seed2.value > 0);
        
        // Same string should always give same seed
        let seed3 = GameSeed::from_string("test");
        let seed4 = GameSeed::from_string("test");
        assert_eq!(seed3.value, seed4.value);
        
        // Different strings should give different seeds
        let seed5 = GameSeed::from_string("different");
        assert_ne!(seed3.value, seed5.value);
    }
}
