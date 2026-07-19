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
use bevy_egui::EguiPrimaryContextPass;

pub mod asteroids;
pub mod components;
pub mod ephemeris;
pub mod exoplanets;
pub mod lagrange;
pub mod nearby_stars;
pub mod procedural;
pub mod selection;
pub mod star_epoch;
pub mod systems;

pub use components::{
    calculate_general_colony_cost, infer_ocean_properties, AtmosphereComposition, AtmosphericGas,
    CometTail, CurrentStarSystem, Destroyed, FloatingOrigin, HoverMarker, Hovered,
    HyperbolicTrajectory, KeplerOrbit, LagrangePointMarkers, LastLpClick, LocalOrbitAmplification,
    LpMarkerInfo, MarkerDot, MarkerOwner, OceanProperties, OceanType, OrbitCenter, OrbitPath,
    Selected, SelectionMarker, SpaceCoordinates, StellarProperties, SurfaceTemperature, SystemId,
};
pub use ephemeris::{calculate_position_for_body, calculate_positions_at_timestamp};
pub use exoplanets::{ConfirmedPlanet, RealPlanet};
pub use lagrange::{draw_lagrange_point_rings, handle_lp_hover};
pub use nearby_stars::{BinaryOrbitData, NearbyStarsData, PlanetData, StarData, StarSystemData};
pub use procedural::{
    calculate_frost_line, generate_procedural_atmosphere, map_star_to_system_architecture,
    map_star_to_system_architecture_with_orbit_limits, AsteroidBelt, BinaryCompanionContext,
    CometaryCloud, PlanetType, ProceduralPlanet, SystemArchitecture,
};
pub use selection::{
    animate_marker_dots, animate_ring_highlight, apply_ring_highlight,
    cleanup_stale_selection_markers, despawn_hover_markers, despawn_selection_markers,
    handle_body_hover, handle_body_selection, remove_ring_highlight, restore_suppressed_markers,
    scale_markers_with_zoom, spawn_hover_markers, spawn_selection_markers,
    zoom_camera_to_anchored_body, RingHighlight,
};
pub use star_epoch::{
    advance_position, heliocentric_to_galactic, hill_sphere_au, system_barycenter,
    StarSystemEphemeris, StarSystemsEphemeris, AU_IN_LY, EPOCH_BEACON_GAME_START_SIM_S,
    SOL_HILL_SPHERE_AU,
};
pub use systems::{
    capped_visual_speed, check_natural_destruction, draw_orbit_paths, fade_destroyed_bodies,
    manage_comet_tail_meshes, orbit_position_from_mean_anomaly, propagate_orbits,
    sync_floating_origin_to_anchor, update_body_lod_visibility, update_orbit_visibility,
    update_render_transform, update_tail_transforms, SCALING_FACTOR,
};

/// Plugin that adds astronomy systems to the Bevy app
pub struct AstronomyPlugin;

impl Plugin for AstronomyPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(nearby_stars::NearbyStarsPlugin)
            .add_plugins(asteroids::AsteroidRegistryPlugin)
            .init_resource::<LagrangePointMarkers>()
            .init_resource::<LastLpClick>()
            // GRA-319: register every simulation-state type with the
            // Bevy AppTypeRegistry so DynamicScene::from_world can pick
            // it up.  Without these calls, snapshot/restore silently
            // drops the component/resource.
            .register_type::<SpaceCoordinates>()
            .register_type::<FloatingOrigin>()
            .register_type::<CurrentStarSystem>()
            .register_type::<SystemId>()
            .register_type::<OrbitCenter>()
            .register_type::<KeplerOrbit>()
            .register_type::<HyperbolicTrajectory>()
            .register_type::<OrbitPath>()
            .register_type::<Selected>()
            .register_type::<Hovered>()
            .register_type::<Destroyed>()
            .register_type::<CometTail>()
            .register_type::<LocalOrbitAmplification>()
            .register_type::<SelectionMarker>()
            .register_type::<HoverMarker>()
            .register_type::<MarkerOwner>()
            .register_type::<MarkerDot>()
            .register_type::<LpMarkerInfo>()
            .register_type::<OceanType>()
            .register_type::<OceanProperties>()
            .register_type::<SurfaceTemperature>()
            .register_type::<StellarProperties>()
            .register_type::<AtmosphericGas>()
            .register_type::<AtmosphereComposition>()
            .register_type::<RealPlanet>()
            .register_type::<NearbyStarsData>()
            .register_type::<StarSystemData>()
            .register_type::<StarData>()
            .register_type::<PlanetData>()
            .register_type::<BinaryOrbitData>()
            .register_type::<RingHighlight>()
            .add_systems(
                Update,
                (
                    // Core orbital mechanics
                    propagate_orbits,
                    sync_floating_origin_to_anchor.after(propagate_orbits),
                    update_render_transform.after(sync_floating_origin_to_anchor),
                    // Destruction and lifecycle
                    check_natural_destruction.after(propagate_orbits),
                    fade_destroyed_bodies.after(check_natural_destruction),
                    // Selection/hover marker lifecycle. The explicit ordering
                    // flushes deferred despawns before later systems query the
                    // marker set, preventing two systems from queuing cleanup
                    // for the same entity in one frame.
                    despawn_selection_markers,
                    cleanup_stale_selection_markers.after(despawn_selection_markers),
                    despawn_hover_markers,
                    spawn_selection_markers
                        .after(cleanup_stale_selection_markers)
                        .after(despawn_hover_markers),
                    spawn_hover_markers
                        .after(spawn_selection_markers)
                        .after(despawn_hover_markers),
                    restore_suppressed_markers.after(spawn_hover_markers),
                    scale_markers_with_zoom.after(restore_suppressed_markers),
                    animate_marker_dots.after(scale_markers_with_zoom),
                ),
            )
            .add_systems(
                Update,
                (
                    // Ring highlight (emissive glow on actual mesh)
                    apply_ring_highlight,
                    remove_ring_highlight,
                    animate_ring_highlight,
                    // Camera zoom
                    zoom_camera_to_anchored_body,
                    // Visibility / LOD
                    update_orbit_visibility,
                    update_body_lod_visibility,
                    // Rendering
                    draw_orbit_paths
                        .after(update_orbit_visibility)
                        .after(sync_floating_origin_to_anchor)
                        .after(propagate_orbits),
                    draw_lagrange_point_rings
                        .after(update_orbit_visibility)
                        .after(sync_floating_origin_to_anchor)
                        .after(propagate_orbits),
                    // Comet Visuals
                    manage_comet_tail_meshes,
                    update_tail_transforms.after(propagate_orbits),
                ),
            )
            // Selection and hover systems use EguiContexts — must run in EguiPrimaryContextPass
            .add_systems(EguiPrimaryContextPass, handle_body_selection)
            .add_systems(EguiPrimaryContextPass, handle_body_hover)
            .add_systems(
                EguiPrimaryContextPass,
                handle_lp_hover.after(handle_body_hover),
            );
    }
}

#[cfg(test)]
mod atmosphere_integration_tests {
    use super::{AtmosphereComposition, AtmosphericGas};
    use crate::plugins::solar_system_data::SolarSystemData;

    #[test]
    fn test_atmosphere_data_loading() {
        // Load the solar system data
        let data = SolarSystemData::load_from_file("assets/data/solar_system.ron")
            .expect("Failed to load solar system data");

        // Earth should have atmosphere data
        let earth = data.get_body("Earth").expect("Earth not found");
        assert!(
            earth.atmosphere.is_some(),
            "Earth should have atmosphere data"
        );

        let earth_atmo = earth.atmosphere.as_ref().unwrap();
        assert_eq!(earth_atmo.surface_pressure_mbar, 1013.0);
        assert_eq!(earth_atmo.surface_temperature_celsius, 15.0);
        assert!(earth_atmo.gases.iter().any(|g| g.name == "N2"));
        assert!(earth_atmo.gases.iter().any(|g| g.name == "O2"));

        // Mars should have atmosphere data
        let mars = data.get_body("Mars").expect("Mars not found");
        assert!(
            mars.atmosphere.is_some(),
            "Mars should have atmosphere data"
        );

        let mars_atmo = mars.atmosphere.as_ref().unwrap();
        assert_eq!(mars_atmo.surface_pressure_mbar, 6.0);
        assert!(mars_atmo.gases.iter().any(|g| g.name == "CO2"));

        // Venus should have atmosphere data
        let venus = data.get_body("Venus").expect("Venus not found");
        assert!(
            venus.atmosphere.is_some(),
            "Venus should have atmosphere data"
        );

        let venus_atmo = venus.atmosphere.as_ref().unwrap();
        assert_eq!(venus_atmo.surface_pressure_mbar, 92000.0);

        // Jupiter should have atmosphere data
        let jupiter = data.get_body("Jupiter").expect("Jupiter not found");
        assert!(
            jupiter.atmosphere.is_some(),
            "Jupiter should have atmosphere data"
        );

        // Titan should have atmosphere data
        let titan = data.get_body("Titan").expect("Titan not found");
        assert!(
            titan.atmosphere.is_some(),
            "Titan should have atmosphere data"
        );

        // Mercury should NOT have atmosphere data (no significant atmosphere)
        let mercury = data.get_body("Mercury").expect("Mercury not found");
        assert!(
            mercury.atmosphere.is_none(),
            "Mercury should not have atmosphere data"
        );

        // Moon should NOT have atmosphere data
        let moon = data.get_body("Moon").expect("Moon not found");
        assert!(
            moon.atmosphere.is_none(),
            "Moon should not have atmosphere data"
        );
    }

    #[test]
    fn test_atmosphere_breathability_check() {
        // Create atmospheres from loaded data and check breathability calculation
        let data = SolarSystemData::load_from_file("assets/data/solar_system.ron")
            .expect("Failed to load solar system data");

        // Earth should be breathable
        let earth = data.get_body("Earth").unwrap();
        if let Some(earth_atmo_data) = &earth.atmosphere {
            let gases: Vec<AtmosphericGas> = earth_atmo_data
                .gases
                .iter()
                .map(|g| AtmosphericGas::new(&g.name, g.percentage))
                .collect();

            let atmosphere = AtmosphereComposition::new(
                earth_atmo_data.surface_pressure_mbar,
                earth_atmo_data.surface_temperature_celsius,
                gases,
            );

            assert!(atmosphere.breathable, "Earth should be breathable");
            assert!(
                atmosphere.calculate_colony_cost(
                    1.0,
                    atmosphere.surface_temperature_celsius,
                    atmosphere.surface_temperature_celsius
                ) < 0.01,
                "Earth should have colony cost of 0"
            );
        }

        // Mars should not be breathable
        let mars = data.get_body("Mars").unwrap();
        if let Some(mars_atmo_data) = &mars.atmosphere {
            let gases: Vec<AtmosphericGas> = mars_atmo_data
                .gases
                .iter()
                .map(|g| AtmosphericGas::new(&g.name, g.percentage))
                .collect();

            let atmosphere = AtmosphereComposition::new(
                mars_atmo_data.surface_pressure_mbar,
                mars_atmo_data.surface_temperature_celsius,
                gases,
            );

            assert!(!atmosphere.breathable, "Mars should not be breathable");
            assert!(
                atmosphere.calculate_colony_cost(
                    0.38,
                    atmosphere.surface_temperature_celsius,
                    atmosphere.surface_temperature_celsius
                ) > 2.0,
                "Mars should have high colony cost"
            );
        }
    }
}

#[cfg(test)]
mod atmosphere_tests {
    use super::{AtmosphereComposition, AtmosphericGas};
    use crate::plugins::solar_system_data::SolarSystemData;

    #[test]
    fn test_earth_atmosphere_is_breathable() {
        let earth_atmosphere = AtmosphereComposition::new(
            1013.0,
            15.0,
            vec![
                AtmosphericGas::new("N2", 78.0),
                AtmosphericGas::new("O2", 21.0),
                AtmosphericGas::new("Ar", 0.93),
                AtmosphericGas::new("CO2", 0.04),
            ],
        );

        assert!(
            earth_atmosphere.breathable,
            "Earth atmosphere should be breathable"
        );
        assert_eq!(earth_atmosphere.surface_pressure_mbar, 1013.0);
        assert_eq!(earth_atmosphere.surface_temperature_celsius, 15.0);
        assert!(earth_atmosphere.has_gas("O2"));
        assert!(earth_atmosphere.has_gas("N2"));
        assert_eq!(earth_atmosphere.get_gas_percentage("O2"), Some(21.0));
    }

    #[test]
    fn test_mars_atmosphere_not_breathable() {
        let mars_atmosphere = AtmosphereComposition::new(
            6.0,
            -63.0,
            vec![
                AtmosphericGas::new("CO2", 95.0),
                AtmosphericGas::new("N2", 2.7),
                AtmosphericGas::new("Ar", 1.6),
                AtmosphericGas::new("O2", 0.13),
            ],
        );

        assert!(
            !mars_atmosphere.breathable,
            "Mars atmosphere should not be breathable"
        );
        assert_eq!(mars_atmosphere.surface_pressure_mbar, 6.0);
        assert!(mars_atmosphere.has_gas("CO2"));
    }

    #[test]
    fn test_venus_atmosphere_not_breathable() {
        let venus_atmosphere = AtmosphereComposition::new(
            92000.0,
            465.0,
            vec![
                AtmosphericGas::new("CO2", 96.5),
                AtmosphericGas::new("N2", 3.5),
            ],
        );

        assert!(
            !venus_atmosphere.breathable,
            "Venus atmosphere should not be breathable"
        );
        assert_eq!(venus_atmosphere.surface_pressure_mbar, 92000.0);
        assert!(venus_atmosphere.has_gas("CO2"));
        assert!(!venus_atmosphere.has_gas("O2"));
    }

    #[test]
    fn test_jupiter_atmosphere() {
        let jupiter_atmosphere = AtmosphereComposition::new(
            1000.0,
            -145.0,
            vec![
                AtmosphericGas::new("H2", 90.0),
                AtmosphericGas::new("He", 10.0),
            ],
        );

        assert!(
            !jupiter_atmosphere.breathable,
            "Jupiter atmosphere should not be breathable"
        );
        assert!(jupiter_atmosphere.has_gas("H2"));
        assert!(jupiter_atmosphere.has_gas("He"));
    }

    #[test]
    fn test_titan_atmosphere() {
        let titan_atmosphere = AtmosphereComposition::new(
            1500.0,
            -179.0,
            vec![
                AtmosphericGas::new("N2", 98.4),
                AtmosphericGas::new("CH4", 1.4),
            ],
        );

        assert!(
            !titan_atmosphere.breathable,
            "Titan atmosphere should not be breathable (no O2)"
        );
        assert!(titan_atmosphere.has_gas("N2"));
        assert!(titan_atmosphere.has_gas("CH4"));
        // Titan has high pressure but no oxygen
        assert!(!titan_atmosphere.has_gas("O2"));
    }

    #[test]
    fn test_colony_cost_calculation_earth() {
        let earth_atmosphere = AtmosphereComposition::new(
            1013.0,
            15.0,
            vec![
                AtmosphericGas::new("N2", 78.0),
                AtmosphericGas::new("O2", 21.0),
                AtmosphericGas::new("Ar", 0.93),
            ],
        );

        let cost = earth_atmosphere.calculate_colony_cost(1.0, 15.0, 15.0);
        assert!(cost < 0.01, "Earth should have colony cost of 0");
    }

    #[test]
    fn test_colony_cost_calculation_mars() {
        let mars_atmosphere = AtmosphereComposition::new(
            6.0,
            -63.0,
            vec![
                AtmosphericGas::new("CO2", 95.0),
                AtmosphericGas::new("N2", 2.7),
            ],
        );

        let cost = mars_atmosphere.calculate_colony_cost(0.379, -63.0, -63.0);
        // Mars: atmosphere cost ~1.2 + temp cost ~1.0 + pressure ~1.5 + gravity ~0.8 ≈ 4.5
        assert!(
            cost > 2.0 && cost < 8.0,
            "Mars should have moderate colony cost (got {})",
            cost
        );
    }

    #[test]
    fn test_colony_cost_calculation_venus() {
        let venus_atmosphere = AtmosphereComposition::new(
            92000.0,
            465.0,
            vec![
                AtmosphericGas::new("CO2", 96.5),
                AtmosphericGas::new("N2", 3.5),
            ],
        );

        let cost = venus_atmosphere.calculate_colony_cost(0.904, 465.0, 465.0);
        // Venus: atmosphere ~1.2 + temp ~2.9 + pressure ~1.3 ≈ 5.4  (bounded 0–10 scale)
        assert!(
            cost > 4.0 && cost <= 10.0,
            "Venus should have high colony cost (got {})",
            cost
        );
    }

    #[test]
    fn test_atmospheric_gas_creation() {
        let gas = AtmosphericGas::new("O2", 21.0);
        assert_eq!(gas.name, "O2");
        assert_eq!(gas.percentage, 21.0);
    }

    #[test]
    fn test_get_gas_percentage_nonexistent() {
        let atmosphere = AtmosphereComposition::new(
            1000.0,
            0.0,
            vec![
                AtmosphericGas::new("N2", 78.0),
                AtmosphericGas::new("O2", 21.0),
            ],
        );

        assert_eq!(atmosphere.get_gas_percentage("He"), None);
        assert!(!atmosphere.has_gas("He"));
    }

    #[test]
    fn test_escape_velocity_calculation() {
        // Test Earth's escape velocity (should be ~11.2 km/s)
        let earth_mass = 5.97237e24; // kg
        let earth_radius = 6371.0; // km
        let earth_escape_velocity =
            AtmosphereComposition::calculate_escape_velocity(earth_mass, earth_radius);

        // Should be close to 11.2 km/s (within 0.1 km/s)
        assert!(
            (earth_escape_velocity - 11.2).abs() < 0.1,
            "Earth escape velocity should be ~11.2 km/s, got {}",
            earth_escape_velocity
        );

        // Test Moon's escape velocity (should be ~2.4 km/s)
        let moon_mass = 7.342e22; // kg
        let moon_radius = 1737.4; // km
        let moon_escape_velocity =
            AtmosphereComposition::calculate_escape_velocity(moon_mass, moon_radius);

        // Should be close to 2.4 km/s
        assert!(
            (moon_escape_velocity - 2.4).abs() < 0.1,
            "Moon escape velocity should be ~2.4 km/s, got {}",
            moon_escape_velocity
        );

        // Test Jupiter's escape velocity (should be ~60 km/s)
        let jupiter_mass = 1.8982e27; // kg
        let jupiter_radius = 69911.0; // km
        let jupiter_escape_velocity =
            AtmosphereComposition::calculate_escape_velocity(jupiter_mass, jupiter_radius);

        // Should be close to 60 km/s (within 1 km/s)
        assert!(
            (jupiter_escape_velocity - 60.0).abs() < 1.0,
            "Jupiter escape velocity should be ~60 km/s, got {}",
            jupiter_escape_velocity
        );
    }

    #[test]
    fn test_atmosphere_retention() {
        // Earth should retain atmosphere (escape velocity ~11.2 km/s)
        let earth_mass = 5.97237e24; // kg
        let earth_radius = 6371.0; // km
        assert!(
            AtmosphereComposition::can_retain_atmosphere(earth_mass, earth_radius),
            "Earth should be able to retain an atmosphere"
        );

        // Mars should retain atmosphere (escape velocity ~5.0 km/s)
        let mars_mass = 6.4171e23; // kg
        let mars_radius = 3389.5; // km
        assert!(
            AtmosphereComposition::can_retain_atmosphere(mars_mass, mars_radius),
            "Mars should be able to retain an atmosphere"
        );

        // Moon is at the retention threshold (escape velocity ~2.4 km/s)
        let moon_mass = 7.342e22; // kg
        let moon_radius = 1737.4; // km
        assert!(
            AtmosphereComposition::can_retain_atmosphere(moon_mass, moon_radius),
            "Moon is at boundary: can retain heavy gases but threshold is ≥ 2.0 km/s"
        );

        // Titan should retain atmosphere (escape velocity ~2.6 km/s, denser than Moon)
        let titan_mass = 1.3452e23; // kg
        let titan_radius = 2574.73; // km
        assert!(
            AtmosphereComposition::can_retain_atmosphere(titan_mass, titan_radius),
            "Titan should be able to retain an atmosphere"
        );

        // Very small asteroid should NOT retain atmosphere
        let small_asteroid_mass = 1.0e15; // kg (very small)
        let small_asteroid_radius = 1.0; // km
        assert!(
            !AtmosphereComposition::can_retain_atmosphere(
                small_asteroid_mass,
                small_asteroid_radius
            ),
            "Small asteroid should not be able to retain an atmosphere"
        );
    }

    #[test]
    fn test_atmosphere_with_body_data() {
        // Test Earth with body data
        let earth_atmosphere = AtmosphereComposition::new_with_body_data(
            1013.0,
            15.0,
            vec![
                AtmosphericGas::new("N2", 78.0),
                AtmosphericGas::new("O2", 21.0),
                AtmosphericGas::new("Ar", 0.93),
            ],
            5.97237e24, // Earth mass
            6371.0,     // Earth radius
            false,      // Surface pressure
        );

        assert!(
            earth_atmosphere.can_support_atmosphere,
            "Earth should support atmosphere"
        );
        assert!(
            earth_atmosphere.breathable,
            "Earth atmosphere should be breathable"
        );
        assert_eq!(earth_atmosphere.surface_pressure_mbar, 1013.0);

        // Test Mars with body data
        let mars_atmosphere = AtmosphereComposition::new_with_body_data(
            6.0,
            -63.0,
            vec![
                AtmosphericGas::new("CO2", 95.0),
                AtmosphericGas::new("N2", 2.7),
            ],
            6.4171e23, // Mars mass
            3389.5,    // Mars radius
            false,     // Surface pressure, not reference
        );

        assert!(
            mars_atmosphere.can_support_atmosphere,
            "Mars should support atmosphere"
        );
        assert!(
            !mars_atmosphere.breathable,
            "Mars atmosphere should not be breathable"
        );
        assert!(
            !mars_atmosphere.is_reference_pressure,
            "Mars has surface pressure"
        );
    }

    #[test]
    fn test_gas_giant_reference_pressure() {
        // Test Jupiter with reference pressure flag
        let jupiter_atmosphere = AtmosphereComposition::new_with_body_data(
            1000.0, // 1 bar reference level
            -108.0,
            vec![
                AtmosphericGas::new("H2", 90.0),
                AtmosphericGas::new("He", 10.0),
            ],
            1.8982e27, // Jupiter mass
            69911.0,   // Jupiter radius
            true,      // Reference pressure, not surface
        );

        assert!(
            jupiter_atmosphere.can_support_atmosphere,
            "Jupiter should support atmosphere"
        );
        assert!(
            !jupiter_atmosphere.breathable,
            "Jupiter atmosphere should not be breathable"
        );
        assert!(
            jupiter_atmosphere.is_reference_pressure,
            "Jupiter uses reference pressure at 1 bar level"
        );
        assert_eq!(jupiter_atmosphere.surface_pressure_mbar, 1000.0);
    }

    #[test]
    fn test_terrestrial_surface_pressure() {
        // Test Earth with surface pressure (not reference)
        let earth_atmosphere = AtmosphereComposition::new_with_body_data(
            1013.0,
            15.0,
            vec![
                AtmosphericGas::new("N2", 78.0),
                AtmosphericGas::new("O2", 21.0),
                AtmosphericGas::new("Ar", 0.93),
            ],
            5.97237e24, // Earth mass
            6371.0,     // Earth radius
            false,      // Surface pressure, not reference
        );

        assert!(
            earth_atmosphere.can_support_atmosphere,
            "Earth should support atmosphere"
        );
        assert!(
            earth_atmosphere.breathable,
            "Earth atmosphere should be breathable"
        );
        assert!(
            !earth_atmosphere.is_reference_pressure,
            "Earth has actual surface pressure"
        );
    }

    #[test]
    fn test_harvest_altitude_gas_giant() {
        // Test Jupiter with harvest altitude
        let jupiter = AtmosphereComposition::new_with_body_data(
            1000.0, // 1 bar reference
            -108.0,
            vec![
                AtmosphericGas::new("H2", 90.0),
                AtmosphericGas::new("He", 10.0),
            ],
            1.8982e27, // Jupiter mass
            69911.0,   // Jupiter radius
            true,      // Gas giant: reference pressure
        );

        // Should have default harvest altitude of 10 bar
        assert_eq!(jupiter.harvest_altitude_bar, 10.0);
        assert_eq!(jupiter.max_harvest_altitude_bar, 50.0);

        // Yield multiplier should be ~10x at 10 bar (vs 1 bar reference)
        let yield_mult = jupiter.harvest_yield_multiplier();
        assert!(
            (yield_mult - 10.0).abs() < 0.1,
            "Yield at 10 bar should be ~10x, got {}",
            yield_mult
        );

        // Should be able to increase harvest altitude
        assert!(jupiter.can_increase_harvest_altitude());
        assert_eq!(jupiter.remaining_harvest_capacity_bar(), 40.0); // 50 - 10 = 40
    }

    #[test]
    fn test_harvest_altitude_terrestrial() {
        // Test Earth - should have no harvest altitude (not a gas giant)
        let earth = AtmosphereComposition::new_with_body_data(
            1013.0,
            15.0,
            vec![
                AtmosphericGas::new("N2", 78.0),
                AtmosphericGas::new("O2", 21.0),
            ],
            5.97237e24, // Earth mass
            6371.0,     // Earth radius
            false,      // Terrestrial: surface pressure
        );

        // Terrestrial planets have no harvest altitude
        assert_eq!(earth.harvest_altitude_bar, 0.0);
        assert_eq!(earth.max_harvest_altitude_bar, 0.0);

        // Harvest yield should be 0 for terrestrial
        assert_eq!(earth.harvest_yield_multiplier(), 0.0);
        assert!(!earth.can_increase_harvest_altitude());
        assert_eq!(earth.remaining_harvest_capacity_bar(), 0.0);
    }

    #[test]
    fn test_harvest_yield_scaling() {
        // Test that harvest yield scales linearly with pressure
        let mut atmosphere = AtmosphereComposition::new_with_body_data(
            1000.0, // 1 bar reference
            -150.0,
            vec![AtmosphericGas::new("H2", 100.0)],
            1.0e27,  // Large mass
            50000.0, // Large radius
            true,    // Gas giant
        );

        // Manually set different harvest altitudes to test scaling
        atmosphere.harvest_altitude_bar = 1.0;
        assert!((atmosphere.harvest_yield_multiplier() - 1.0).abs() < 0.01);

        atmosphere.harvest_altitude_bar = 25.0;
        assert!((atmosphere.harvest_yield_multiplier() - 25.0).abs() < 0.01);

        atmosphere.harvest_altitude_bar = 100.0;
        assert!((atmosphere.harvest_yield_multiplier() - 100.0).abs() < 0.01);
    }

    // ── Atmospheric Scattering Tests ───────────────────────────────────────────

    #[test]
    fn test_mean_molecular_weight_earth() {
        let earth = AtmosphereComposition::new(
            1013.0,
            15.0,
            vec![
                AtmosphericGas::new("N2", 78.0),
                AtmosphericGas::new("O2", 21.0),
                AtmosphericGas::new("Ar", 0.93),
                AtmosphericGas::new("CO2", 0.04),
            ],
        );
        let mmw = earth.mean_molecular_weight();
        // Earth's mean molecular weight is ~28.97 g/mol
        assert!(
            (mmw - 28.97).abs() < 1.0,
            "Earth mmw should be ~28.97, got {mmw}"
        );
    }

    #[test]
    fn test_mean_molecular_weight_hydrogen() {
        let h2_atmo = AtmosphereComposition::new(
            1000.0,
            -145.0,
            vec![
                AtmosphericGas::new("H2", 90.0),
                AtmosphericGas::new("He", 10.0),
            ],
        );
        let mmw = h2_atmo.mean_molecular_weight();
        // 90% H2 (2.016) + 10% He (4.003) = ~2.21
        assert!(
            mmw < 3.0 && mmw > 1.5,
            "H2/He mmw should be ~2.2, got {mmw}"
        );
    }

    #[test]
    fn test_derive_scattering_earth() {
        let mut earth = AtmosphereComposition::new_with_body_data(
            1013.0,
            15.0,
            vec![
                AtmosphericGas::new("N2", 78.0),
                AtmosphericGas::new("O2", 21.0),
                AtmosphericGas::new("Ar", 0.93),
                AtmosphericGas::new("CO2", 0.04),
            ],
            5.97237e24,
            6371.0,
            false,
        );

        earth.derive_scattering_params(1.0, None, None, None, None, None, None, None);

        // Scale height should be close to 8.5 km for Earth
        assert!(
            (earth.scale_height_km - 8.5).abs() < 2.0,
            "Earth scale height should be ~8.5 km, got {}",
            earth.scale_height_km
        );

        // Rayleigh strength should be ~1.0 (Earth reference)
        assert!(
            earth.rayleigh_strength > 0.5 && earth.rayleigh_strength < 2.0,
            "Earth rayleigh_strength should be ~1.0, got {}",
            earth.rayleigh_strength
        );

        // Should have blue-ish Rayleigh tint (blue > red)
        assert!(
            earth.rayleigh_rgb[2] > earth.rayleigh_rgb[0],
            "Earth rayleigh should be blue-dominant"
        );

        // Intensity defaults to 1.0
        assert!(
            (earth.atmosphere_intensity - 1.0).abs() < 0.01,
            "Default intensity should be 1.0"
        );
    }

    #[test]
    fn test_derive_scattering_mars() {
        let mut mars = AtmosphereComposition::new_with_body_data(
            6.0,
            -63.0,
            vec![
                AtmosphericGas::new("CO2", 95.0),
                AtmosphericGas::new("N2", 2.7),
                AtmosphericGas::new("Ar", 1.6),
                AtmosphericGas::new("O2", 0.13),
            ],
            6.4171e23,
            3389.5,
            false,
        );

        // Mars gravity ~0.38 g
        mars.derive_scattering_params(0.38, None, None, None, None, None, None, None);

        // Mars has very thin atmosphere, so rayleigh_strength should be << 1.0
        assert!(
            mars.rayleigh_strength < 0.2,
            "Mars rayleigh_strength should be very low, got {}",
            mars.rayleigh_strength
        );

        // CO2 dominant → warm red-orange tint
        assert!(
            mars.rayleigh_rgb[0] > mars.rayleigh_rgb[2],
            "Mars rayleigh should be red-dominant for CO2 atmosphere"
        );
    }

    #[test]
    fn test_derive_scattering_overrides() {
        let mut earth = AtmosphereComposition::new_with_body_data(
            1013.0,
            15.0,
            vec![
                AtmosphericGas::new("N2", 78.0),
                AtmosphericGas::new("O2", 21.0),
            ],
            5.97237e24,
            6371.0,
            false,
        );

        // Override all parameters
        earth.derive_scattering_params(
            1.0,
            Some(10.0),            // scale height override
            Some((1.0, 0.0, 0.0)), // pure red rayleigh
            Some(5.0),             // strong rayleigh
            Some(0.1),             // mie override
            Some(0.9),             // mie_g override
            Some((0.5, 0.5, 0.5)), // grey haze
            Some(2.0),             // double intensity
        );

        assert!((earth.scale_height_km - 10.0).abs() < 0.01);
        assert!((earth.rayleigh_rgb[0] - 1.0).abs() < 0.01);
        assert!((earth.rayleigh_rgb[1]).abs() < 0.01);
        assert!((earth.rayleigh_strength - 5.0).abs() < 0.01);
        assert!((earth.mie_strength - 0.1).abs() < 0.01);
        assert!((earth.mie_g - 0.9).abs() < 0.01);
        assert!((earth.haze_color[0] - 0.5).abs() < 0.01);
        assert!((earth.atmosphere_intensity - 2.0).abs() < 0.01);
    }

    #[test]
    fn test_derive_scattering_titan_haze() {
        let mut titan = AtmosphereComposition::new_with_body_data(
            1500.0,
            -179.0,
            vec![
                AtmosphericGas::new("N2", 98.4),
                AtmosphericGas::new("CH4", 1.4),
            ],
            1.3452e23,
            2574.73,
            false,
        );

        // Titan gravity ~0.14 g
        titan.derive_scattering_params(0.14, None, None, None, None, None, None, None);

        // Titan has CH4 → higher mie_strength (haze factor = 0.08)
        assert!(
            titan.mie_strength > 0.01,
            "Titan mie_strength should be elevated due to CH4, got {}",
            titan.mie_strength
        );

        // Haze colour should be orange/amber
        assert!(
            titan.haze_color[0] > titan.haze_color[2],
            "Titan haze should be warm-coloured"
        );
    }

    #[test]
    fn test_ron_optional_scattering_fields_deserialize() {
        // Verify that the RON file still loads with the new optional fields
        let data = SolarSystemData::load_from_file("assets/data/solar_system.ron")
            .expect("solar_system.ron should load with new optional scattering fields");

        // Earth should have scattering overrides
        let earth = data.get_body("Earth").expect("Earth should exist");
        let atmo = earth
            .atmosphere
            .as_ref()
            .expect("Earth should have atmosphere");
        assert!(
            atmo.scale_height_km.is_some(),
            "Earth should have scale_height_km override"
        );
        assert!(
            atmo.rayleigh_rgb.is_some(),
            "Earth should have rayleigh_rgb override"
        );

        // Mars should have scattering overrides
        let mars = data.get_body("Mars").expect("Mars should exist");
        let mars_atmo = mars
            .atmosphere
            .as_ref()
            .expect("Mars should have atmosphere");
        assert!(
            mars_atmo.atmosphere_intensity.is_some(),
            "Mars should have atmosphere_intensity override"
        );

        // Moon should have no atmosphere
        let moon = data.get_body("Moon").expect("Moon should exist");
        assert!(moon.atmosphere.is_none(), "Moon should not have atmosphere");

        // Jupiter — atmosphere without overrides should still work
        let jupiter = data.get_body("Jupiter").expect("Jupiter should exist");
        let jup_atmo = jupiter
            .atmosphere
            .as_ref()
            .expect("Jupiter should have atmosphere");
        assert!(
            jup_atmo.scale_height_km.is_none(),
            "Jupiter should have no scale_height override (uses defaults)"
        );
    }
}

#[cfg(test)]
mod atmosphere_ui_tests {
    use super::{AtmosphereComposition, AtmosphericGas};

    #[test]
    fn test_atmosphere_ui_data_available() {
        // Test that atmosphere data can be properly queried for UI display

        // Create a test atmosphere similar to Earth
        let earth_atmosphere = AtmosphereComposition::new(
            1013.0,
            15.0,
            vec![
                AtmosphericGas::new("N2", 78.0),
                AtmosphericGas::new("O2", 21.0),
                AtmosphericGas::new("Ar", 0.93),
                AtmosphericGas::new("CO2", 0.04),
            ],
        );

        // Verify atmosphere properties are accessible for UI
        assert_eq!(earth_atmosphere.surface_pressure_mbar, 1013.0);
        assert_eq!(earth_atmosphere.surface_temperature_celsius, 15.0);
        assert!(earth_atmosphere.breathable);
        assert!(
            earth_atmosphere
                .calculate_colony_cost(1.0, 15.0, 15.0)
                .abs()
                < 0.01
        );

        // Verify gas composition can be iterated
        assert_eq!(earth_atmosphere.gases.len(), 4);

        // Verify pressure conversion for UI display
        let pressure_bar = earth_atmosphere.surface_pressure_mbar / 1000.0;
        assert!((pressure_bar - 1.013).abs() < 0.01);
    }

    #[test]
    fn test_atmosphere_ui_formatting() {
        // Test that different atmospheres format correctly for UI

        // Venus - high pressure
        let venus = AtmosphereComposition::new(
            92000.0,
            465.0,
            vec![
                AtmosphericGas::new("CO2", 96.5),
                AtmosphericGas::new("N2", 3.5),
            ],
        );

        let pressure_bar = venus.surface_pressure_mbar / 1000.0;
        assert!(pressure_bar >= 1.0); // Should display as "bar"
        assert!(venus.calculate_colony_cost(0.904, 465.0, 465.0) > 4.0); // Venus: high cost on 0-10 scale

        // Mars - low pressure
        let mars = AtmosphereComposition::new(
            6.0,
            -63.0,
            vec![
                AtmosphericGas::new("CO2", 95.0),
                AtmosphericGas::new("N2", 2.7),
            ],
        );

        let pressure_bar = mars.surface_pressure_mbar / 1000.0;
        assert!(pressure_bar < 1.0); // Should display as "mbar"
        assert!(mars.calculate_colony_cost(0.379, -63.0, -63.0) > 2.0); // Mars cost: 2.0 Base + Temp cost
    }

    #[test]
    fn test_colony_cost_colors() {
        // Test that colony costs map to correct color categories

        let test_atmospheres = [
            // Good (0-3)
            AtmosphereComposition::new(
                1013.0,
                15.0,
                vec![
                    AtmosphericGas::new("N2", 78.0),
                    AtmosphericGas::new("O2", 21.0),
                ],
            ),
            // Moderate (4-6)
            AtmosphereComposition::new(500.0, -30.0, vec![AtmosphericGas::new("N2", 95.0)]),
            // Bad (7-8)
            AtmosphereComposition::new(92000.0, 465.0, vec![AtmosphericGas::new("CO2", 96.5)]),
        ];

        let costs: Vec<f32> = test_atmospheres
            .iter()
            .map(|a| {
                a.calculate_colony_cost(
                    1.0,
                    a.surface_temperature_celsius,
                    a.surface_temperature_celsius,
                )
            })
            .collect();

        // Verify we have a range of costs
        assert!(costs[0] <= 0.01); // Earth-like
        assert!(costs[1] > 1.0); // Moderate
        assert!(costs[2] > 4.0); // Extreme (bounded 0-10 scale)
    }

    #[test]
    fn test_gas_composition_display() {
        // Test that gas composition is properly formatted for display

        let atmosphere = AtmosphereComposition::new(
            1000.0,
            0.0,
            vec![
                AtmosphericGas::new("H2", 90.0),
                AtmosphericGas::new("He", 10.0),
            ],
        );

        // Verify gases can be accessed for display
        assert_eq!(atmosphere.gases.len(), 2);

        // Verify gas percentages sum to 100
        let total: f32 = atmosphere.gases.iter().map(|g| g.percentage).sum();
        assert!((total - 100.0).abs() < 0.1);

        // Verify individual gas properties are accessible
        for gas in &atmosphere.gases {
            assert!(!gas.name.is_empty());
            assert!(gas.percentage >= 0.0 && gas.percentage <= 100.0);
        }
    }
}

#[cfg(test)]
mod procedural_generation_tests {
    //! Integration tests for procedural star system generation
    //!
    //! Tests the complete workflow: frost line calculation, system architecture generation,
    //! planet spawning, asteroid belts, cometary clouds, and metallicity bonuses.

    use super::{
        calculate_frost_line, map_star_to_system_architecture, PlanetType, SpaceCoordinates,
    };
    use crate::economy::components::{OrbitsBody, PlanetResources, SpectralClass, StarSystem};
    use crate::economy::generation::{generate_solar_system_resources, ProceduralRng};
    use crate::economy::types::ResourceType;
    use crate::plugins::solar_system::{CelestialBody, Moon, Planet, Star};
    use crate::plugins::solar_system_data::BodyType;
    use bevy::math::DVec3;
    use bevy::prelude::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    #[test]
    fn test_frost_line_calculations_for_different_stars() {
        // Test frost line for various stellar types

        // Sun (G2V): L = 1.0 L☉
        let sun_frost = calculate_frost_line(1.0);
        assert!(
            (sun_frost - 4.85).abs() < 0.01,
            "Sun frost line should be ~4.85 AU, got {:.2}",
            sun_frost
        );

        // Alpha Centauri A (G2V): L = 1.519 L☉
        let alpha_cen_a_frost = calculate_frost_line(1.519);
        assert!(
            alpha_cen_a_frost > 5.9 && alpha_cen_a_frost < 6.1,
            "Alpha Cen A frost line should be ~5.98 AU, got {:.2}",
            alpha_cen_a_frost
        );

        // Proxima Centauri (M5.5Ve): L = 0.0017 L☉
        let proxima_frost = calculate_frost_line(0.0017);
        assert!(
            proxima_frost < 0.21,
            "Proxima frost line should be ~0.20 AU, got {:.2}",
            proxima_frost
        );

        // Sirius A (A1V): L = 25.4 L☉
        let sirius_frost = calculate_frost_line(25.4);
        assert!(
            sirius_frost > 24.0 && sirius_frost < 25.0,
            "Sirius frost line should be ~24.4 AU, got {:.2}",
            sirius_frost
        );
    }

    #[test]
    fn test_system_generation_for_empty_sun_like_system() {
        let mut rng = StdRng::seed_from_u64(12345);

        let architecture = map_star_to_system_architecture(
            "Test Star",
            1.0, // 1.0 solar mass
            1.0, // Solar luminosity
            0,   // No existing planets
            &[], // No existing orbits
            0,
            &mut rng,
        );

        // Should generate enough planets to reach target of 5
        let total_planets = architecture.rocky_planets.len() + architecture.gas_giants.len();
        assert!(
            (4..=7).contains(&total_planets),
            "Expected 4-7 planets for empty system, got {}",
            total_planets
        );

        // Frost line should be solar-like
        assert!(
            (architecture.frost_line_au - 4.85).abs() < 0.5,
            "Frost line should be ~4.85 AU, got {:.2}",
            architecture.frost_line_au
        );

        // Asteroid belt has 80% chance - don't assert it must exist
        // But if it exists, verify it's in a reasonable location
        if let Some(ref belt) = architecture.asteroid_belt {
            assert!(
                belt.inner_au > 0.1 && belt.outer_au > belt.inner_au,
                "Asteroid belt should have valid bounds"
            );
        }
    }

    #[test]
    fn test_system_generation_respects_existing_planets() {
        let mut rng = StdRng::seed_from_u64(67890);

        // System with 3 existing planets at specific orbits
        let existing_orbits = vec![0.72, 1.0, 1.52]; // Venus, Earth, Mars-like

        let architecture = map_star_to_system_architecture(
            "Test Star",
            1.0, // 1.0 solar mass
            1.0, // Solar luminosity
            3,   // 3 existing planets
            &existing_orbits,
            3,
            &mut rng,
        );

        // Should generate 2-3 more planets to reach target of 5-6
        let total_planets = architecture.rocky_planets.len() + architecture.gas_giants.len();
        assert!(
            (1..=4).contains(&total_planets),
            "Expected 1-4 new planets to fill gaps, got {}",
            total_planets
        );

        // New planets should not overlap with existing ones
        for planet in architecture.rocky_planets.iter() {
            for &existing in &existing_orbits {
                let separation = (planet.semi_major_axis_au - existing).abs();
                assert!(
                    separation > 0.1,
                    "Rocky planet at {:.2} AU too close to existing planet at {:.2} AU (sep: {:.3})",
                    planet.semi_major_axis_au,
                    existing,
                    separation
                );
            }
        }

        for planet in architecture.gas_giants.iter() {
            for &existing in &existing_orbits {
                let separation = (planet.semi_major_axis_au - existing).abs();
                assert!(
                    separation > 0.5,
                    "Gas giant at {:.2} AU too close to existing planet at {:.2} AU (sep: {:.3})",
                    planet.semi_major_axis_au,
                    existing,
                    separation
                );
            }
        }
    }

    #[test]
    fn test_rocky_planets_inside_frost_line() {
        let mut rng = StdRng::seed_from_u64(11111);

        let architecture =
            map_star_to_system_architecture("Test Star", 1.0, 1.0, 0, &[], 0, &mut rng);

        let frost_line = architecture.frost_line_au;

        // All rocky planets should be inside the frost line
        for planet in &architecture.rocky_planets {
            assert!(
                planet.semi_major_axis_au < frost_line,
                "Rocky planet at {:.2} AU should be inside frost line ({:.2} AU)",
                planet.semi_major_axis_au,
                frost_line
            );

            // With new architecture, can be Rocky, SuperEarth, DesertWorld, LavaWorld, or WaterWorld
            assert!(
                matches!(
                    planet.planet_type,
                    PlanetType::Rocky
                        | PlanetType::SuperEarth
                        | PlanetType::DesertWorld
                        | PlanetType::LavaWorld
                        | PlanetType::WaterWorld
                ),
                "Planet should be terrestrial type, got {:?}",
                planet.planet_type
            );

            // Terrestrial planets should have reasonable masses (0.05 - 12 M⊕)
            assert!(
                planet.mass_earth > 0.05 && planet.mass_earth < 12.0,
                "Terrestrial planet mass {:.1} M⊕ out of reasonable range",
                planet.mass_earth
            );

            // Low eccentricity for inner system
            assert!(
                planet.eccentricity < 0.35,
                "Terrestrial planet eccentricity {:.2} too high",
                planet.eccentricity
            );
        }
    }

    #[test]
    fn test_gas_giants_outside_frost_line() {
        let mut rng = StdRng::seed_from_u64(22222);

        let architecture =
            map_star_to_system_architecture("Test Star", 1.0, 1.0, 0, &[], 0, &mut rng);

        let frost_line = architecture.frost_line_au;

        // All gas/ice giants should be outside the frost line
        for planet in &architecture.gas_giants {
            assert!(
                planet.semi_major_axis_au > frost_line,
                "Gas giant at {:.2} AU should be outside frost line ({:.2} AU)",
                planet.semi_major_axis_au,
                frost_line
            );

            assert!(
                planet.planet_type == PlanetType::GasGiant
                    || planet.planet_type == PlanetType::IceGiant,
                "Planet should be gas or ice giant type"
            );

            // Giants should have significant mass (> 10 M⊕)
            assert!(
                planet.mass_earth > 10.0,
                "Gas giant mass {:.1} M⊕ too low",
                planet.mass_earth
            );
        }
    }

    #[test]
    fn test_asteroid_belt_generation() {
        let mut rng = StdRng::seed_from_u64(33333);

        let architecture =
            map_star_to_system_architecture("Test Star", 1.0, 1.0, 0, &[], 0, &mut rng);

        if let Some(belt) = &architecture.asteroid_belt {
            // Belt should be in reasonable location
            assert!(
                belt.inner_au < belt.outer_au,
                "Belt inner edge {:.2} should be less than outer edge {:.2}",
                belt.inner_au,
                belt.outer_au
            );

            // Belt should have reasonable width (0.5 - 3 AU typically)
            let width = belt.outer_au - belt.inner_au;
            assert!(
                width > 0.3 && width < 5.0,
                "Belt width {:.2} AU seems unreasonable",
                width
            );

            // Should spawn a reasonable number of asteroids
            assert!(
                belt.count >= 50 && belt.count <= 200,
                "Belt asteroid count {} out of expected range",
                belt.count
            );
        }
    }

    #[test]
    fn test_cometary_cloud_generation() {
        let mut rng = StdRng::seed_from_u64(44444);

        let architecture =
            map_star_to_system_architecture("Test Star", 1.0, 1.0, 0, &[], 0, &mut rng);

        if let Some(cloud) = &architecture.cometary_cloud {
            // Cloud should be beyond the immediate planetary zone.
            // For compact systems the cloud scales down proportionally with the
            // effective frost line, so the absolute minimum is much lower than
            // for a Sol-scale system.
            assert!(
                cloud.inner_au > 1.0,
                "Cometary cloud inner edge {:.2} AU too close to star",
                cloud.inner_au
            );

            assert!(
                cloud.outer_au > cloud.inner_au,
                "Cloud inner edge {:.2} should be less than outer edge {:.2}",
                cloud.inner_au,
                cloud.outer_au
            );

            // Should spawn a reasonable number of comets
            assert!(
                cloud.count >= 20 && cloud.count <= 80,
                "Cloud comet count {} out of expected range",
                cloud.count
            );
        }
    }

    #[test]
    fn test_procedural_planet_kepler_orbit_conversion() {
        let mut rng = StdRng::seed_from_u64(55555);

        let architecture =
            map_star_to_system_architecture("Test Star", 1.0, 1.0, 0, &[], 0, &mut rng);

        // Test conversion to KeplerOrbit for all generated planets
        for planet in architecture.rocky_planets.iter() {
            let kepler = planet.to_kepler_orbit();

            assert_eq!(
                kepler.semi_major_axis, planet.semi_major_axis_au,
                "Semi-major axis should match"
            );
            assert_eq!(
                kepler.eccentricity, planet.eccentricity,
                "Eccentricity should match"
            );
            assert!(kepler.mean_motion > 0.0, "Mean motion should be positive");

            // Verify Kepler's third law: T² ∝ a³
            let period_from_orbit = std::f64::consts::TAU / kepler.mean_motion;
            let period_from_planet = planet.period_days * 86400.0;
            assert!(
                (period_from_orbit - period_from_planet).abs() < 1.0,
                "Orbital period mismatch: {:.1} vs {:.1} seconds",
                period_from_orbit,
                period_from_planet
            );
        }
    }

    #[test]
    fn test_metallicity_multiplier() {
        // Test the metallicity multiplier calculation

        // Solar metallicity ([Fe/H] = 0.0) should give 1.0x
        let solar = StarSystem::with_metallicity(4.85, SpectralClass::G, 0.0);
        assert!(
            (solar.metallicity_multiplier() - 1.0).abs() < 0.01,
            "Solar metallicity should give 1.0x multiplier, got {:.3}",
            solar.metallicity_multiplier()
        );

        // Metal-rich star ([Fe/H] = +0.3) should give higher multiplier
        let metal_rich = StarSystem::with_metallicity(4.85, SpectralClass::G, 0.3);
        assert!(
            metal_rich.metallicity_multiplier() > 1.15,
            "Metal-rich star should give >1.15x multiplier, got {:.3}",
            metal_rich.metallicity_multiplier()
        );

        // Metal-poor star ([Fe/H] = -0.3) should give lower multiplier
        let metal_poor = StarSystem::with_metallicity(4.85, SpectralClass::G, -0.3);
        assert!(
            metal_poor.metallicity_multiplier() < 0.85,
            "Metal-poor star should give <0.85x multiplier, got {:.3}",
            metal_poor.metallicity_multiplier()
        );

        // Test clamping: very high metallicity
        let very_high = StarSystem::with_metallicity(4.85, SpectralClass::G, 2.0);
        assert!(
            very_high.metallicity_multiplier() <= 1.5,
            "Multiplier should be clamped at 1.5x, got {:.3}",
            very_high.metallicity_multiplier()
        );

        // Test clamping: very low metallicity
        let very_low = StarSystem::with_metallicity(4.85, SpectralClass::G, -2.0);
        assert!(
            very_low.metallicity_multiplier() >= 0.5,
            "Multiplier should be clamped at 0.5x, got {:.3}",
            very_low.metallicity_multiplier()
        );
    }

    #[test]
    fn test_dim_star_system_generation() {
        // Test generation for a dim M-dwarf star (like Proxima Centauri)
        let mut rng = StdRng::seed_from_u64(66666);

        let architecture = map_star_to_system_architecture(
            "Proxima",
            0.12,   // ~0.12 solar masses (Proxima Centauri)
            0.0017, // Very low luminosity
            0,
            &[],
            0,
            &mut rng,
        );

        // Frost line should be very close to star
        assert!(
            architecture.frost_line_au < 0.25,
            "M-dwarf frost line should be < 0.25 AU, got {:.2}",
            architecture.frost_line_au
        );

        // Rocky planets should be very close in
        for planet in &architecture.rocky_planets {
            assert!(
                planet.semi_major_axis_au < 0.35,
                "Rocky planet should be < 0.35 AU for M-dwarf, got {:.2}",
                planet.semi_major_axis_au
            );
        }
    }

    #[test]
    fn test_moon_uses_parent_star_frost_line() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);

        // Spawn star with a non-default frost line (10 AU)
        let world = app.world_mut();
        let star_entity = world
            .spawn((
                Star,
                CelestialBody {
                    name: "TS".into(),
                    mass: 1.989e30,
                    radius: 695700.0,
                    body_type: BodyType::Star,
                    visual_radius: 10.0,
                    asteroid_class: None,
                    star_approach_au: None,
                    // GRA-NNN: shell-cache fields for the orbit-shell resolver.
                    rotation_period_s: None,
                    habitable_outer_au: None,
                },
                SpaceCoordinates::new(DVec3::ZERO),
                StarSystem::with_metallicity(10.0, SpectralClass::G, 0.0),
            ))
            .id();

        // Planet at 5.0 AU (inside star frost line)
        let planet_entity = world
            .spawn((
                Planet,
                CelestialBody {
                    name: "TS Planet at 5.00AU".into(),
                    mass: 5.972e24,
                    radius: 6371.0,
                    body_type: BodyType::Planet,
                    visual_radius: 2.0,
                    asteroid_class: None,
                    star_approach_au: None,
                    // GRA-NNN: shell-cache fields for the orbit-shell resolver.
                    rotation_period_s: None,
                    habitable_outer_au: None,
                },
                SpaceCoordinates::new(DVec3::new(5.0, 0.0, 0.0)),
                OrbitsBody::new(star_entity),
            ))
            .id();

        // Moon at 5.1 AU (should be inside same star frost line)
        let moon_entity = world
            .spawn((
                Moon,
                CelestialBody {
                    name: "TS Planet at 5.00AU Moon 1".into(),
                    mass: 1.0e20,
                    radius: 100.0,
                    body_type: BodyType::Moon,
                    visual_radius: 1.0,
                    asteroid_class: None,
                    star_approach_au: None,
                    // GRA-NNN: shell-cache fields for the orbit-shell resolver.
                    rotation_period_s: None,
                    habitable_outer_au: None,
                },
                SpaceCoordinates::new(DVec3::new(5.1, 0.0, 0.0)),
                OrbitsBody::new(planet_entity),
            ))
            .id();

        app.add_systems(Update, generate_solar_system_resources);

        // Seed the procedural RNG so this test is deterministic (the system
        // pulls from ResMut<ProceduralRng> rather than the thread-local RNG
        // — see GRA-91).  Picking a seed that places the moon firmly in the
        // inner-system profile is not required: with a fixed seed, the
        // generated water fraction is the same on every run, so the assertion
        // below either passes consistently or fails consistently.  We choose
        // 0x1A2B_3C4D_5E6F_7081 so the seed is easy to spot in debug logs.
        app.insert_resource(ProceduralRng::from_seed(0x1A2B_3C4D_5E6F_7081));

        // Run one update to generate resources
        app.update();

        let res = app
            .world()
            .get::<PlanetResources>(moon_entity)
            .expect("moon should have resources after generation");

        let water = res.get_abundance(&ResourceType::Water);
        let total: f64 = res.deposits.values().map(|d| d.reserve.total_mass()).sum();
        let water_fraction = if total > 0.0 { water / total } else { 0.0 };

        // Since parent star frost_line is 10 AU and moon distance (5.1 AU) < frost_line,
        // moon must be treated as 'inner' and therefore have very low volatiles (water < 2%)
        assert!(
            water_fraction < 0.02,
            "Moon incorrectly treated as outer system; water fraction = {:.3}",
            water_fraction
        );
    }

    #[test]
    fn test_bright_star_system_generation() {
        // Test generation for a bright A-type star (like Sirius A)
        let mut rng = StdRng::seed_from_u64(77777);

        let architecture = map_star_to_system_architecture(
            "Sirius",
            2.02, // ~2.02 solar masses (Sirius A)
            25.4, // High luminosity
            0,
            &[],
            0,
            &mut rng,
        );

        // Frost line should be far from star
        assert!(
            architecture.frost_line_au > 20.0,
            "A-type star frost line should be > 20 AU, got {:.2}",
            architecture.frost_line_au
        );

        // For bright stars, rocky planets can extend quite far since frost line is far out
        // With new architecture, they should span from inner system to near frost line
        if !architecture.rocky_planets.is_empty() {
            let min_rocky_orbit = architecture
                .rocky_planets
                .iter()
                .map(|p| p.semi_major_axis_au)
                .fold(f64::MAX, f64::min);
            // Rocky planets should span a reasonable range within the inner system
            assert!(
                min_rocky_orbit < 5.0,
                "Bright star should have inner rocky planets, got {:.2}",
                min_rocky_orbit
            );
        }
    }

    #[test]
    fn test_deterministic_generation_with_seed() {
        // Test that generation is deterministic with the same seed

        let mut rng1 = StdRng::seed_from_u64(99999);
        let arch1 = map_star_to_system_architecture("Star", 1.0, 1.0, 0, &[], 0, &mut rng1);

        let mut rng2 = StdRng::seed_from_u64(99999);
        let arch2 = map_star_to_system_architecture("Star", 1.0, 1.0, 0, &[], 0, &mut rng2);

        // Should generate same number of planets
        assert_eq!(
            arch1.rocky_planets.len(),
            arch2.rocky_planets.len(),
            "Should generate same number of rocky planets with same seed"
        );
        assert_eq!(
            arch1.gas_giants.len(),
            arch2.gas_giants.len(),
            "Should generate same number of gas giants with same seed"
        );

        // First rocky planet should have same properties
        if !arch1.rocky_planets.is_empty() {
            let p1 = &arch1.rocky_planets[0];
            let p2 = &arch2.rocky_planets[0];

            assert!(
                (p1.semi_major_axis_au - p2.semi_major_axis_au).abs() < 0.001,
                "Rocky planet orbits should match with same seed"
            );
            assert!(
                (p1.mass_earth - p2.mass_earth).abs() < 0.001,
                "Rocky planet masses should match with same seed"
            );
        }
    }
}
