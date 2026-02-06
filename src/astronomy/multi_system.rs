//! Multi-system components for galaxy-scale simulation
//! 
//! This module provides components for managing multiple star systems,
//! galaxy-scale coordinates, and simulation state management.

use bevy::prelude::*;
use bevy::math::DVec3;
use serde::{Deserialize, Serialize};

/// Marks an entity as a star system container
/// 
/// A star system contains a star and all its orbiting bodies (planets, moons, asteroids, etc.)
/// This component allows for hierarchical organization and selective rendering/simulation.
#[derive(Component, Debug, Clone)]
pub struct StarSystem {
    /// Unique identifier for this system
    pub id: u64,
    
    /// System name (e.g., "Sol", "Alpha Centauri", "Proxima Centauri")
    pub name: String,
    
    /// Position in galactic coordinates (light years from galactic center)
    /// This is the "true" position of the system in the galaxy
    pub galactic_position: DVec3,
    
    /// Current simulation state of this system
    pub simulation_state: SystemSimulationState,
    
    /// Bounding radius in AU for culling calculations
    /// Should be set to the radius of the largest orbit plus some margin
    /// Used to determine when to load/unload system details
    pub bounding_radius_au: f64,
    
    /// Number of celestial bodies in this system (for statistics/UI)
    pub body_count: usize,
    
    /// Star type for this system (e.g., "G2V" for Sol)
    pub star_type: String,
}

impl StarSystem {
    /// Create a new star system
    pub fn new(
        id: u64,
        name: impl Into<String>,
        galactic_position: DVec3,
        star_type: impl Into<String>,
        bounding_radius_au: f64,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            galactic_position,
            simulation_state: SystemSimulationState::Dormant,
            bounding_radius_au,
            body_count: 0,
            star_type: star_type.into(),
        }
    }
}

/// Simulation state for a star system
/// 
/// This determines how frequently the system is updated and whether it's rendered.
/// The state transitions based on player focus and camera distance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SystemSimulationState {
    /// Full simulation + full rendering (active/selected system)
    /// - Orbital propagation every frame
    /// - All bodies rendered with full detail
    /// - Orbit trails, selection markers, etc.
    /// - All game systems (resources, construction, etc.) active
    Active,
    
    /// Lightweight simulation, no rendering (nearby but inactive systems)
    /// - Orbital propagation every N frames (configurable)
    /// - No body rendering (maybe star only)
    /// - Abstract resource production
    /// - Construction continues at reduced update rate
    Background,
    
    /// Minimal/no simulation, no rendering (distant systems)
    /// - No frame-by-frame updates
    /// - State stored but not actively simulated
    /// - Can be "fast-forwarded" when loaded
    /// - Minimal memory footprint
    Dormant,
}

/// Marker component for the currently active/selected star system
/// 
/// Only one system should have this component at a time.
/// Systems with this component receive full simulation and rendering.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct ActiveSystem;

/// Galactic-scale coordinates for star systems
/// 
/// These coordinates are in light years and represent the position of a star system
/// in the galaxy. This is separate from the local system coordinates (AU) and
/// render coordinates (Bevy units).
#[derive(Component, Debug, Clone, Copy)]
pub struct GalacticCoordinates {
    /// Position in light years from galactic center (or arbitrary reference point)
    /// x, y, z in light years
    pub position: DVec3,
}

impl GalacticCoordinates {
    /// Create new galactic coordinates
    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self {
            position: DVec3::new(x, y, z),
        }
    }
    
    /// Calculate distance to another system in light years
    pub fn distance_to(&self, other: &GalacticCoordinates) -> f64 {
        (self.position - other.position).length()
    }
}

/// Marks celestial bodies that belong to a specific star system
/// 
/// This allows queries to filter bodies by system and enables
/// hierarchical organization of the galaxy.
#[derive(Component, Debug, Clone, Copy)]
pub struct SystemMember {
    /// Entity ID of the parent star system
    pub system_entity: Entity,
}

impl SystemMember {
    /// Create a new system member
    pub fn new(system_entity: Entity) -> Self {
        Self { system_entity }
    }
}

/// View mode resource controlling what level of detail to show
/// 
/// This drives the transition between system view (zoomed in, see planets)
/// and galaxy view (zoomed out, see star systems as points).
#[derive(Resource, Debug, Clone)]
pub struct ViewMode {
    /// Current view mode
    pub current: ViewModeType,
    
    /// Transition progress from system to galaxy view (0.0 = system, 1.0 = galaxy)
    pub transition_progress: f32,
    
    /// Camera distance thresholds
    pub thresholds: ViewModeThresholds,
}

impl Default for ViewMode {
    fn default() -> Self {
        Self {
            current: ViewModeType::SystemView,
            transition_progress: 0.0,
            thresholds: ViewModeThresholds::default(),
        }
    }
}

/// Type of view mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewModeType {
    /// System view: Full 3D rendering of celestial bodies, orbits, etc.
    SystemView,
    
    /// Galaxy view: Strategic view showing star systems as points/icons
    GalaxyView,
}

/// Camera distance thresholds for view mode transitions
#[derive(Debug, Clone)]
pub struct ViewModeThresholds {
    /// Maximum distance for full system view (Bevy units)
    pub system_view_max: f32,
    
    /// Start of transition zone (Bevy units)
    pub transition_start: f32,
    
    /// End of transition zone / start of galaxy view (Bevy units)
    pub transition_end: f32,
}

impl Default for ViewModeThresholds {
    fn default() -> Self {
        Self {
            system_view_max: 100_000.0,
            transition_start: 100_000.0,
            transition_end: 500_000.0,
        }
    }
}

/// Configuration for multi-system simulation
/// 
/// Controls update frequencies and resource limits for different system states.
#[derive(Resource, Debug, Clone)]
pub struct MultiSystemConfig {
    /// Maximum number of systems in Active state (usually 1)
    pub max_active_systems: usize,
    
    /// Maximum number of systems in Background state
    pub max_background_systems: usize,
    
    /// Update frequency for background systems (frames between updates)
    pub background_update_interval: u32,
    
    /// Update frequency for dormant systems (frames between updates)
    pub dormant_update_interval: u32,
    
    /// Whether to automatically transition systems based on camera distance
    pub auto_transition_systems: bool,
    
    /// Distance threshold for activating a system (light years)
    pub activation_distance_ly: f64,
    
    /// Distance threshold for moving system to background (light years)
    pub background_distance_ly: f64,
    
    /// Distance threshold for moving system to dormant (light years)
    pub dormant_distance_ly: f64,
}

impl Default for MultiSystemConfig {
    fn default() -> Self {
        Self {
            max_active_systems: 1,
            max_background_systems: 10,
            background_update_interval: 10, // Update every 10 frames
            dormant_update_interval: 600,   // Update every 600 frames (~10 seconds at 60 FPS)
            auto_transition_systems: true,
            activation_distance_ly: 0.0,     // Only active if selected
            background_distance_ly: 50.0,    // Within 50 light years
            dormant_distance_ly: 100.0,      // Beyond 100 light years
        }
    }
}

/// Performance metrics for multi-system simulation
/// 
/// Tracks resource usage and performance for debugging and optimization.
#[derive(Resource, Debug, Default)]
pub struct SystemPerformanceMetrics {
    // System counts
    pub active_systems: usize,
    pub background_systems: usize,
    pub dormant_systems: usize,
    pub total_systems: usize,
    
    // Body counts
    pub active_bodies: usize,
    pub background_bodies: usize,
    
    // Timing (in milliseconds)
    pub active_simulation_time_ms: f32,
    pub background_simulation_time_ms: f32,
    pub render_time_ms: f32,
    pub total_frame_time_ms: f32,
    
    // Memory (approximate, in MB)
    pub active_memory_mb: f32,
    pub background_memory_mb: f32,
    pub total_memory_mb: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_star_system_creation() {
        let system = StarSystem::new(
            0,
            "Test System",
            DVec3::new(10.0, 0.0, 0.0),
            "G2V",
            50.0,
        );
        
        assert_eq!(system.id, 0);
        assert_eq!(system.name, "Test System");
        assert_eq!(system.galactic_position, DVec3::new(10.0, 0.0, 0.0));
        assert_eq!(system.simulation_state, SystemSimulationState::Dormant);
        assert_eq!(system.bounding_radius_au, 50.0);
    }

    #[test]
    fn test_galactic_coordinates_distance() {
        let coord1 = GalacticCoordinates::new(0.0, 0.0, 0.0);
        let coord2 = GalacticCoordinates::new(3.0, 4.0, 0.0);
        
        let distance = coord1.distance_to(&coord2);
        assert!((distance - 5.0).abs() < 1e-10);
    }

    #[test]
    fn test_view_mode_default() {
        let view_mode = ViewMode::default();
        assert_eq!(view_mode.current, ViewModeType::SystemView);
        assert_eq!(view_mode.transition_progress, 0.0);
    }

    #[test]
    fn test_multi_system_config_default() {
        let config = MultiSystemConfig::default();
        assert_eq!(config.max_active_systems, 1);
        assert_eq!(config.max_background_systems, 10);
        assert!(config.auto_transition_systems);
    }
}
