//! Astronomy module for high-precision orbital mechanics
//! 
//! This module provides Keplerian orbital mechanics with double-precision (f64)
//! for realistic space simulation. It includes:
//! 
//! - SpaceCoordinates: High-precision position tracking using DVec3
//! - KeplerOrbit: Standard orbital elements for elliptical orbits
//! - Kepler solver: Newton-Raphson solver for orbit propagation
//! - Floating origin: Conversion from simulation to rendering coordinates

use bevy::prelude::*;

pub mod components;
pub mod systems;

pub use components::{AtmosphereComposition, AtmosphericGas, Hovered, KeplerOrbit, OrbitPath, Selected, SpaceCoordinates};
pub use systems::{
    draw_orbit_paths, handle_body_selection, handle_body_hover, draw_hover_effects, 
    zoom_camera_to_anchored_body, propagate_orbits, 
    update_orbit_visibility_by_zoom, update_render_transform, update_selected_orbit_visibility, 
    SCALING_FACTOR,
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
                // Camera zoom
                zoom_camera_to_anchored_body,
                // Visibility
                update_orbit_visibility_by_zoom,
                update_selected_orbit_visibility.after(update_orbit_visibility_by_zoom),
                // Rendering
                draw_orbit_paths.after(update_selected_orbit_visibility),
                draw_hover_effects,
            ),
        );
    }
}
