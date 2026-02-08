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
pub mod ephemeris;
pub mod exoplanets;
pub mod nearby_stars;
pub mod procedural;
pub mod systems;

pub use components::{
    AtmosphereComposition, AtmosphericGas, CometTail, Destroyed, FloatingOrigin, Hovered, KeplerOrbit,
    LocalOrbitAmplification, OrbitCenter, OrbitPath, Selected, SpaceCoordinates,
};
pub use ephemeris::{calculate_position_for_body, calculate_positions_at_timestamp};
pub use exoplanets::{ConfirmedPlanet, RealPlanet};
pub use procedural::{
    calculate_frost_line, map_star_to_system_architecture, AsteroidBelt, CometaryCloud,
    PlanetType, ProceduralPlanet, SystemArchitecture,
};
pub use systems::{
    animate_marker_dots, check_natural_destruction, despawn_hover_markers,
    despawn_selection_markers, draw_orbit_paths, fade_destroyed_bodies, handle_body_selection,
    handle_body_hover, manage_comet_tail_meshes, orbit_position_from_mean_anomaly,
    propagate_orbits, scale_markers_with_zoom, spawn_hover_markers, spawn_selection_markers,
    update_body_lod_visibility, update_orbit_visibility, update_render_transform,
    update_tail_transforms, zoom_camera_to_anchored_body, SCALING_FACTOR,
};

/// Plugin that adds astronomy systems to the Bevy app
pub struct AstronomyPlugin;

impl Plugin for AstronomyPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(nearby_stars::NearbyStarsPlugin)
            .add_systems(
            Update,
            (
                // Core orbital mechanics
                propagate_orbits,
                update_render_transform.after(propagate_orbits),
                // Destruction and lifecycle
                check_natural_destruction.after(propagate_orbits),
                fade_destroyed_bodies.after(check_natural_destruction),
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
                // Visibility / LOD
                update_orbit_visibility,
                update_body_lod_visibility,
                // Rendering
                draw_orbit_paths.after(update_orbit_visibility),
            ),
        );
    }
}
