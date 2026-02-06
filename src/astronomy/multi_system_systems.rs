//! Systems for multi-system management
//! 
//! Provides systems for:
//! - View mode transitions based on camera distance
//! - System state management (active/background/dormant)
//! - Performance metrics tracking

use bevy::prelude::*;
use crate::plugins::camera::{GameCamera, OrbitCamera};
use super::multi_system::*;

/// System that updates view mode based on camera distance
/// 
/// This system checks the camera's distance from the active system's center
/// and transitions between SystemView and GalaxyView accordingly.
pub fn update_view_mode(
    camera_query: Query<&OrbitCamera, With<GameCamera>>,
    mut view_mode: ResMut<ViewMode>,
) {
    let Ok(camera) = camera_query.get_single() else {
        return;
    };
    
    let distance = camera.radius;
    let thresholds = &view_mode.thresholds;
    
    // Determine current view mode and transition progress
    if distance < thresholds.system_view_max {
        // Full system view
        view_mode.current = ViewModeType::SystemView;
        view_mode.transition_progress = 0.0;
    } else if distance < thresholds.transition_end {
        // In transition zone - determine if we're in system or galaxy view
        let transition_range = thresholds.transition_end - thresholds.transition_start;
        if transition_range > 0.0 {
            let t = (distance - thresholds.transition_start) / transition_range;
            view_mode.transition_progress = t.clamp(0.0, 1.0);
            
            // Switch to galaxy view at 50% through transition
            if t >= 0.5 {
                view_mode.current = ViewModeType::GalaxyView;
            } else {
                view_mode.current = ViewModeType::SystemView;
            }
        }
    } else {
        // Full galaxy view
        view_mode.current = ViewModeType::GalaxyView;
        view_mode.transition_progress = 1.0;
    }
}

/// System that applies view-mode-based rendering culling
/// 
/// In galaxy view, hide all celestial body meshes and show only system-level icons.
/// In system view, show full detail for the active system.
pub fn apply_view_mode_culling(
    view_mode: Res<ViewMode>,
    mut body_query: Query<&mut Visibility, (With<super::components::SpaceCoordinates>, Without<StarSystem>)>,
) {
    // Only update when view mode changes
    if !view_mode.is_changed() {
        return;
    }
    
    match view_mode.current {
        ViewModeType::SystemView => {
            // Show all bodies in system view
            for mut visibility in body_query.iter_mut() {
                *visibility = Visibility::Inherited;
            }
        }
        ViewModeType::GalaxyView => {
            // Hide all bodies in galaxy view
            for mut visibility in body_query.iter_mut() {
                *visibility = Visibility::Hidden;
            }
        }
    }
}

/// System that updates performance metrics
/// 
/// Tracks system counts, body counts, and timing for debugging and optimization.
pub fn update_performance_metrics(
    mut metrics: ResMut<SystemPerformanceMetrics>,
    system_query: Query<&StarSystem>,
    active_query: Query<&StarSystem, With<ActiveSystem>>,
    body_query: Query<(), With<super::components::SpaceCoordinates>>,
    time: Res<Time>,
) {
    // Count systems by state
    let mut active = 0;
    let mut background = 0;
    let mut dormant = 0;
    
    for system in system_query.iter() {
        match system.simulation_state {
            SystemSimulationState::Active => active += 1,
            SystemSimulationState::Background => background += 1,
            SystemSimulationState::Dormant => dormant += 1,
        }
    }
    
    metrics.active_systems = active;
    metrics.background_systems = background;
    metrics.dormant_systems = dormant;
    metrics.total_systems = system_query.iter().count();
    
    // Count total bodies
    // In a full implementation, we'd count active vs background bodies separately
    metrics.active_bodies = body_query.iter().count();
    
    // Update frame time (in milliseconds)
    metrics.total_frame_time_ms = time.delta_seconds() * 1000.0;
    
    // TODO: Add more detailed timing once we have separate systems for active/background
    // For now, use placeholder values
    metrics.active_simulation_time_ms = 0.0;
    metrics.background_simulation_time_ms = 0.0;
    metrics.render_time_ms = 0.0;
}

/// System that manages automatic state transitions based on distance
/// 
/// This system can automatically move systems between Active/Background/Dormant
/// states based on their distance from the player's current location.
/// 
/// Note: Currently a placeholder - full implementation would require:
/// - Player position tracking
/// - Distance-based state transitions
/// - Coordination with system loading/unloading
pub fn auto_transition_systems(
    config: Res<MultiSystemConfig>,
    active_query: Query<&GalacticCoordinates, With<ActiveSystem>>,
    mut system_query: Query<(&mut StarSystem, &GalacticCoordinates), Without<ActiveSystem>>,
) {
    if !config.auto_transition_systems {
        return;
    }
    
    // Get active system position (player's current location)
    let Ok(active_coords) = active_query.get_single() else {
        return;
    };
    
    // Update states based on distance
    for (mut system, coords) in system_query.iter_mut() {
        let distance = active_coords.distance_to(coords);
        
        let new_state = if distance <= config.activation_distance_ly {
            SystemSimulationState::Active
        } else if distance <= config.background_distance_ly {
            SystemSimulationState::Background
        } else {
            SystemSimulationState::Dormant
        };
        
        if system.simulation_state != new_state {
            info!(
                "System '{}' transitioning from {:?} to {:?} (distance: {:.2} ly)",
                system.name, system.simulation_state, new_state, distance
            );
            system.simulation_state = new_state;
        }
    }
}

/// Plugin that adds multi-system management systems
pub struct MultiSystemPlugin;

impl Plugin for MultiSystemPlugin {
    fn build(&self, app: &mut App) {
        app
            // Initialize resources
            .init_resource::<ViewMode>()
            .init_resource::<MultiSystemConfig>()
            .init_resource::<SystemPerformanceMetrics>()
            // Add systems
            .add_systems(Update, (
                update_view_mode,
                apply_view_mode_culling.after(update_view_mode),
                update_performance_metrics,
                auto_transition_systems,
            ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multi_system_plugin() {
        let mut app = App::new();
        app.add_plugins(MultiSystemPlugin);
        
        // Verify resources were initialized
        assert!(app.world().contains_resource::<ViewMode>());
        assert!(app.world().contains_resource::<MultiSystemConfig>());
        assert!(app.world().contains_resource::<SystemPerformanceMetrics>());
    }
}
