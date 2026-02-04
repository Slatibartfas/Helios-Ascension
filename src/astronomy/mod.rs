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

pub use components::{KeplerOrbit, SpaceCoordinates};
pub use systems::{propagate_orbits, update_render_transform, SCALING_FACTOR};

/// Plugin that adds astronomy systems to the Bevy app
pub struct AstronomyPlugin;

impl Plugin for AstronomyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (
            propagate_orbits,
            update_render_transform.after(propagate_orbits),
        ));
    }
}
