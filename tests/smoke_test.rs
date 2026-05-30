//! Headless smoke tests for Helios Ascension core game systems initialization.
//!
//! These tests verify that the core game subsystems can be created and
//! initialized without panics or errors. They are designed to run in CI
//! without requiring a display server.

use bevy::prelude::*;

// =============================================================================
// Test 1: Minimal App Creation
// =============================================================================

#[test]
fn test_minimal_app_creation() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_systems(Startup, || {
        info!("Minimal app started successfully");
    });
    // Should not panic
    app.update();
}

// =============================================================================
// Test 2: Game State Plugin Initialization
// =============================================================================

#[test]
fn test_game_state_plugin_init() {
    use crate::game_state::GameStatePlugin;

    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(GameStatePlugin);

    // Should initialize without panic
    app.update();

    // Verify GameState resource exists
    let game_state = app.world().get_resource::<crate::game_state::GameState>();
    assert!(game_state.is_some(), "GameState resource should exist after init");
}

// =============================================================================
// Test 3: Event Bus Plugin Initialization
// =============================================================================

#[test]
fn test_event_bus_plugin_init() {
    use crate::events::EventBusPlugin;

    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(GameStatePlugin); // Required dependency
    app.add_plugins(EventBusPlugin);

    // Should initialize without panic
    app.update();
}

// =============================================================================
// Test 4: Astronomy Plugin Initialization
// =============================================================================

#[test]
fn test_astronomy_plugin_init() {
    use crate::astronomy::AstronomyPlugin;

    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(GameStatePlugin);
    app.add_plugins(EventBusPlugin);
    app.add_plugins(AstronomyPlugin);

    // Should initialize without panic
    app.update();
}

// =============================================================================
// Test 5: Economy Plugin Initialization
// =============================================================================

#[test]
fn test_economy_plugin_init() {
    use crate::economy::EconomyPlugin;

    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(GameStatePlugin);
    app.add_plugins(EventBusPlugin);
    app.add_plugins(EconomyPlugin);

    // Should initialize without panic
    app.update();

    // Verify economy resources exist
    let budget = app.world().get_resource::<crate::economy::budget::Budget>();
    assert!(budget.is_some(), "Budget resource should exist after init");
}

// =============================================================================
// Test 6: Colony Plugin Initialization
// =============================================================================

#[test]
fn test_colony_plugin_init() {
    use crate::colony::ColonyPlugin;

    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(GameStatePlugin);
    app.add_plugins(EventBusPlugin);
    app.add_plugins(AstronomyPlugin); // Colony depends on astronomy
    app.add_plugins(EconomyPlugin);
    app.add_plugins(ColonyPlugin);

    // Should initialize without panic
    app.update();
}

// =============================================================================
// Test 7: Research Plugin Initialization
// =============================================================================

#[test]
fn test_research_plugin_init() {
    use crate::research::ResearchPlugin;

    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(GameStatePlugin);
    app.add_plugins(EventBusPlugin);
    app.add_plugins(ResearchPlugin);

    // Should initialize without panic
    app.update();
}

// =============================================================================
// Test 8: Fleet Plugin Initialization
// =============================================================================

#[test]
fn test_fleet_plugin_init() {
    use crate::fleets::FleetPlugin;

    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(GameStatePlugin);
    app.add_plugins(EventBusPlugin);
    app.add_plugins(AstronomyPlugin); // Fleet depends on orbital mechanics
    app.add_plugins(FleetPlugin);

    // Should initialize without panic
    app.update();
}

// =============================================================================
// Test 9: Full Game Stack (Core Systems Only)
// =============================================================================

#[test]
fn test_full_game_stack_core_init() {
    use crate::events::{EventBusPlugin, GameEventsPlugin};

    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(GameStatePlugin);
    app.add_plugins(GameEventsPlugin);
    app.add_plugins(EventBusPlugin);
    app.add_plugins(AstronomyPlugin);
    app.add_plugins(EconomyPlugin);
    app.add_plugins(ResearchPlugin);
    app.add_plugins(FleetPlugin);
    app.add_plugins(ColonyPlugin);

    // Should initialize all core systems without panic
    app.update();
}

// =============================================================================
// Test 10: Simulation Time Resource
// =============================================================================

#[test]
fn test_simulation_time_resource() {
    use crate::ui::time::SimulationTime;

    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(GameStatePlugin);

    app.update();

    let sim_time = app.world().get_resource::<SimulationTime>();
    assert!(sim_time.is_some(), "SimulationTime resource should exist");
}

// =============================================================================
// Test 11: Camera Plugin Initialization
// =============================================================================

#[test]
fn test_camera_plugin_init() {
    use crate::plugins::camera::CameraPlugin;

    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(GameStatePlugin);
    app.add_plugins(EventBusPlugin);
    app.add_plugins(CameraPlugin);

    // Should initialize without panic
    app.update();
}