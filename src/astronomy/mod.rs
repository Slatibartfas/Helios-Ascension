//! Astronomy module for high-precision orbital mechanics
//! 
//! This module provides Keplerian orbital mechanics with double-precision (f64)
//! for realistic space simulation. It includes:
//! 
//! - SpaceCoordinates: High-precision position tracking using DVec3
//! - KeplerOrbit: Standard orbital elements for elliptical orbits
//! - Kepler solver: Newton-Raphson solver for orbit propagation
//! - Floating origin: Conversion from simulation to rendering coordinates
//! - Multi-system support: Components for galaxy-scale simulation

use bevy::prelude::*;

pub mod components;
pub mod systems;
pub mod multi_system;
pub mod multi_system_systems;

pub use components::{
    AtmosphereComposition, AtmosphericGas, Hovered, KeplerOrbit, OrbitPath, Selected,
    SpaceCoordinates,
};
pub use systems::{
    animate_marker_dots, despawn_hover_markers, despawn_selection_markers, draw_orbit_paths,
    handle_body_selection, handle_body_hover, orbit_position_from_mean_anomaly, propagate_orbits,
    scale_markers_with_zoom, spawn_hover_markers, spawn_selection_markers,
    update_orbit_visibility, update_render_transform,
    zoom_camera_to_anchored_body, SCALING_FACTOR,
};
pub use multi_system::{
    ActiveSystem, GalacticCoordinates, MultiSystemConfig, StarSystem, SystemMember,
    SystemPerformanceMetrics, SystemSimulationState, ViewMode, ViewModeThresholds, ViewModeType,
};
pub use multi_system_systems::{
    MultiSystemPlugin, update_view_mode, apply_view_mode_culling,
    update_performance_metrics, auto_transition_systems,
};

/// Plugin that adds astronomy systems to the Bevy app
pub struct AstronomyPlugin;

impl Plugin for AstronomyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                // Core orbital mechanics
                propagate_orbits,
                update_render_transform.after(propagate_orbits),
                // Selection and hover
                handle_body_selection,
                handle_body_hover,
                // Selection/hover markers
                spawn_selection_markers,
                despawn_selection_markers,
                spawn_hover_markers,
                despawn_hover_markers,
                animate_marker_dots,
                scale_markers_with_zoom,
                // Camera zoom
                zoom_camera_to_anchored_body,
                // Visibility
                update_orbit_visibility,
                // Rendering
                draw_orbit_paths.after(update_orbit_visibility),
            ),
        );
    }
}
