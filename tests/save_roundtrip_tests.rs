//! Save/Load round-trip integration tests
//!
//! Verifies end-to-end save/load functionality including:
//! - Game state serialization/deserialization
//! - Save slot operations (quicksave, load, delete)
//! - Autosave system
//! - No deserialization errors or data loss
//!
//! Run with: `cargo test --test save_roundtrip_tests`

use helios_ascension::save::{
    autosave::{AutosaveRotation, AutosaveTimer, AUTOSAVE_INTERVAL_SECS},
    slots::{
        delete_slot, get_slot_metadata, list_slots, load_from_slot, quicksave, quickload,
        save_to_slot, has_quicksave, QUICKSAVE_SLOT, AUTOSAVE_SLOTS,
    },
    GameSavedState, ColonySaved, FleetSaved, ShipSaved, ResearchSaved,
    EconomySaved, MiningOperationSaved,
};
use std::path::PathBuf;

/// Create a test game state with populated fields.
fn create_test_game_state() -> GameSavedState {
    GameSavedState {
        version: 1,
        timestamp: 1_767_225_600, // Jan 1, 2026 00:00:00 UTC
        description: "Test save".to_string(),
        seed: 42,
        elapsed_seconds: 7200.0, // 2 hours
        time_scale: 2.0,
        colonies: vec![
            ColonySaved {
                entity_id: 1,
                name: "Earth".to_string(),
                population: 8_000_000.0,
                growth_rate_modifier: 1.0,
                buildings: vec![
                    ("MiningFacility".to_string(), 3),
                    ("PowerPlant".to_string(), 2),
                ],
                construction_queue: vec![],
                position: [1.0, 0.0, 0.0],
                orbiting_entity: None,
            },
            ColonySaved {
                entity_id: 2,
                name: "Mars".to_string(),
                population: 1_000_000.0,
                growth_rate_modifier: 0.8,
                buildings: vec![("MiningFacility".to_string(), 1)],
                construction_queue: vec![],
                position: [1.52, 0.0, 0.0],
                orbiting_entity: None,
            },
        ],
        fleets: vec![
            FleetSaved {
                entity_id: 10,
                name: "Fleet Alpha".to_string(),
                ships: vec![
                    ShipSaved {
                        class: "Cruiser".to_string(),
                        name: "HMS Victory".to_string(),
                        health_percent: 0.95,
                    },
                    ShipSaved {
                        class: "Frigate".to_string(),
                        name: "HMS Courageous".to_string(),
                        health_percent: 1.0,
                    },
                ],
                orbit_body: Some(1),
                orbit_angle: 0.5,
                maneuver: None,
                standing_orders: None,
            },
        ],
        research: ResearchSaved {
            active_projects: vec![],
            completed_technologies: vec![
                "FusionPower".to_string(),
                "AdvancedPropulsion".to_string(),
            ],
            engineering_projects: vec![],
        },
        economy: EconomySaved {
            treasury: 15_000.0,
            resource_stockpiles: vec![
                ("Minerals".to_string(), 5000.0),
                ("Energy".to_string(), 3000.0),
            ],
            active_mining_operations: vec![MiningOperationSaved {
                body_entity: 1,
                resource_type: "Minerals".to_string(),
                extraction_rate: 100.0,
            }],
        },
        current_system: 0,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Game state serialization round-trip tests
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_game_saved_state_roundtrip() {
    let original = create_test_game_state();

    // Serialize to JSON
    let json = serde_json::to_string(&original).expect("Serialization should succeed");
    assert!(!json.is_empty(), "JSON should not be empty");

    // Deserialize back
    let restored: GameSavedState =
        serde_json::from_str(&json).expect("Deserialization should succeed");

    // Verify key fields match
    assert_eq!(restored.version, original.version);
    assert_eq!(restored.seed, original.seed);
    assert_eq!(restored.elapsed_seconds, original.elapsed_seconds);
    assert_eq!(restored.time_scale, original.time_scale);
    assert_eq!(restored.current_system, original.current_system);
    assert_eq!(restored.colonies.len(), original.colonies.len());
    assert_eq!(restored.fleets.len(), original.fleets.len());
}

#[test]
fn test_game_saved_state_colonies_match() {
    let original = create_test_game_state();

    let json = serde_json::to_string(&original).expect("Serialization should succeed");
    let restored: GameSavedState = serde_json::from_str(&json).expect("Deserialization should succeed");

    // Verify colony details
    assert_eq!(restored.colonies[0].name, "Earth");
    assert_eq!(restored.colonies[0].population, 8_000_000.0);
    assert_eq!(restored.colonies[1].name, "Mars");
    assert_eq!(restored.colonies[1].population, 1_000_000.0);

    // Verify building counts
    assert_eq!(restored.colonies[0].buildings.len(), 2);
    assert_eq!(restored.colonies[0].buildings[0].0, "MiningFacility");
    assert_eq!(restored.colonies[0].buildings[0].1, 3);
}

#[test]
fn test_game_saved_state_fleets_match() {
    let original = create_test_game_state();

    let json = serde_json::to_string(&original).expect("Serialization should succeed");
    let restored: GameSavedState = serde_json::from_str(&json).expect("Deserialization should succeed");

    // Verify fleet details
    assert_eq!(restored.fleets[0].name, "Fleet Alpha");
    assert_eq!(restored.fleets[0].ships.len(), 2);
    assert_eq!(restored.fleets[0].ships[0].name, "HMS Victory");
    assert_eq!(restored.fleets[0].ships[0].health_percent, 0.95);
}

#[test]
fn test_game_saved_state_research_matches() {
    let original = create_test_game_state();

    let json = serde_json::to_string(&original).expect("Serialization should succeed");
    let restored: GameSavedState = serde_json::from_str(&json).expect("Deserialization should succeed");

    // Verify research completed technologies
    assert_eq!(restored.research.completed_technologies.len(), 2);
    assert!(restored
        .research
        .completed_technologies
        .contains(&"FusionPower".to_string()));
}

#[test]
fn test_game_saved_state_economy_matches() {
    let original = create_test_game_state();

    let json = serde_json::to_string(&original).expect("Serialization should succeed");
    let restored: GameSavedState = serde_json::from_str(&json).expect("Deserialization should succeed");

    // Verify economy
    assert_eq!(restored.economy.treasury, 15_000.0);
    assert_eq!(restored.economy.resource_stockpiles.len(), 2);
    assert_eq!(restored.economy.active_mining_operations.len(), 1);
}

// ─────────────────────────────────────────────────────────────────────────────
// Save file compression round-trip tests
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_save_file_compression_roundtrip() {
    use helios_ascension::save::{write_save, read_save};

    let original = create_test_game_state();
    let test_path = get_test_save_dir().join("compression_test.ron.zst");

    // Write with compression
    write_save(&test_path, &original).expect("Write save should succeed");

    // Read back
    let restored: GameSavedState = read_save(&test_path).expect("Read save should succeed");

    // Verify all fields
    assert_eq!(restored.seed, original.seed);
    assert_eq!(restored.elapsed_seconds, original.elapsed_seconds);
    assert_eq!(restored.colonies.len(), original.colonies.len());
    assert_eq!(restored.fleets.len(), original.fleets.len());
    assert_eq!(restored.economy.treasury, original.economy.treasury);
}

// ─────────────────────────────────────────────────────────────────────────────
// Save slot operations tests
// ─────────────────────────────────────────────────────────────────────────────

fn get_test_save_dir() -> PathBuf {
    // Use a temp directory for test saves
    std::env::temp_dir().join("helios_ascension_test_saves")
}

fn setup_test_save_dir() {
    let dir = get_test_save_dir();
    if dir.exists() {
        std::fs::remove_dir_all(&dir).ok();
    }
    std::fs::create_dir_all(&dir).ok();
}

fn cleanup_test_save_dir() {
    let dir = get_test_save_dir();
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_save_to_slot_and_load() {
    setup_test_save_dir();

    let state = create_test_game_state();
    let slot = 1;

    // Save to slot
    let metadata =
        save_to_slot(slot, &state, "Test Save").expect("Save to slot should succeed");

    assert_eq!(metadata.slot, slot);
    assert_eq!(metadata.name, "Test Save");
    assert_eq!(metadata.elapsed_seconds, 7200.0);

    // Load from slot
    let loaded: GameSavedState = load_from_slot(slot).expect("Load from slot should succeed");

    assert_eq!(loaded.seed, state.seed);
    assert_eq!(loaded.elapsed_seconds, state.elapsed_seconds);
    assert_eq!(loaded.colonies.len(), state.colonies.len());
    assert_eq!(loaded.fleets.len(), state.fleets.len());

    // Verify colony data
    assert_eq!(loaded.colonies[0].name, "Earth");
    assert_eq!(loaded.colonies[0].population, 8_000_000.0);

    cleanup_test_save_dir();
}

#[test]
fn test_quicksave_and_quickload() {
    setup_test_save_dir();

    let state = create_test_game_state();

    // Perform quicksave
    let metadata = quicksave(&state).expect("Quicksave should succeed");
    assert_eq!(metadata.slot, QUICKSAVE_SLOT);

    // Check quicksave exists
    assert!(has_quicksave(), "Quicksave should exist after quicksave");

    // Quickload
    let loaded: GameSavedState = quickload().expect("Quickload should succeed");

    assert_eq!(loaded.seed, state.seed);
    assert_eq!(loaded.economy.treasury, state.economy.treasury);

    cleanup_test_save_dir();
}

#[test]
fn test_save_slot_metadata() {
    setup_test_save_dir();

    let state = create_test_game_state();
    let slot = 2;

    // Save to slot
    save_to_slot(slot, &state, "Metadata Test").expect("Save should succeed");

    // Get metadata without loading full save
    let metadata = get_slot_metadata(slot).expect("Get metadata should succeed");
    assert!(metadata.is_some());

    let meta = metadata.unwrap();
    assert_eq!(meta.slot, slot);
    assert_eq!(meta.name, "Metadata Test");
    assert!(meta.elapsed_seconds > 0.0);
    assert!(meta.file_size > 0);

    cleanup_test_save_dir();
}

#[test]
fn test_delete_slot() {
    setup_test_save_dir();

    let state = create_test_game_state();
    let slot = 3;

    // Save to slot
    save_to_slot(slot, &state, "Delete Test").expect("Save should succeed");

    // Verify it exists
    let metadata = get_slot_metadata(slot).expect("Get metadata should succeed");
    assert!(metadata.is_some());

    // Delete the slot
    delete_slot(slot).expect("Delete slot should succeed");

    // Verify it's gone
    let metadata = get_slot_metadata(slot).expect("Get metadata should succeed");
    assert!(metadata.is_none());

    cleanup_test_save_dir();
}

#[test]
fn test_list_slots() {
    setup_test_save_dir();

    let state = create_test_game_state();

    // Save to two slots
    save_to_slot(1, &state, "Slot 1").expect("Save should succeed");
    save_to_slot(2, &state, "Slot 2").expect("Save should succeed");

    // List all slots
    let slots = list_slots().expect("List slots should succeed");

    assert_eq!(slots.len(), 10); // MAX_SLOTS = 10

    // Slots 1 and 2 should have metadata
    assert!(slots[1].is_some());
    assert!(slots[2].is_some());
    assert!(slots[3].is_none()); // Empty slot

    cleanup_test_save_dir();
}

// ─────────────────────────────────────────────────────────────────────────────
// Autosave rotation tests
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_autosave_rotation_cycles_through_slots() {
    let mut rotation = AutosaveRotation::new();

    // Should cycle through 7, 8, 9, 7, 8, 9...
    let slots: Vec<usize> = (0..6).map(|_| rotation.next_slot()).collect();
    assert_eq!(slots, vec![7, 8, 9, 7, 8, 9]);
}

#[test]
fn test_autosave_timer_initialization() {
    let timer = AutosaveTimer::new();

    assert!(!timer.should_autosave());
    assert_eq!(timer.elapsed_secs(), 0.0);
}

#[test]
fn test_autosave_timer_triggers_after_interval() {
    let mut timer = AutosaveTimer::new();

    // Initially should not trigger
    assert!(!timer.should_autosave());

    // Add time equal to interval
    timer.add_time(AUTOSAVE_INTERVAL_SECS as f64);

    // Now should trigger
    assert!(timer.should_autosave());
}

#[test]
fn test_autosave_timer_reset() {
    let mut timer = AutosaveTimer::new();

    timer.add_time(AUTOSAVE_INTERVAL_SECS as f64);
    assert!(timer.should_autosave());

    timer.reset();
    assert!(!timer.should_autosave());
    assert_eq!(timer.elapsed_secs(), 0.0);
}

#[test]
fn test_autosave_timer_prevents_concurrent_saves() {
    let timer = AutosaveTimer::new();

    timer.start_save();
    assert!(!timer.should_autosave(), "Should not autosave while saving in progress");

    timer.end_save();
    assert!(timer.should_autosave(), "Should be able to autosave after save completes");
}

// ─────────────────────────────────────────────────────────────────────────────
// Edge cases and data integrity tests
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_empty_game_state_roundtrip() {
    let empty_state = GameSavedState {
        version: 1,
        timestamp: 0,
        description: "Empty".to_string(),
        seed: 0,
        elapsed_seconds: 0.0,
        time_scale: 1.0,
        colonies: vec![],
        fleets: vec![],
        research: ResearchSaved {
            active_projects: vec![],
            completed_technologies: vec![],
            engineering_projects: vec![],
        },
        economy: EconomySaved {
            treasury: 0.0,
            resource_stockpiles: vec![],
            active_mining_operations: vec![],
        },
        current_system: 0,
    };

    let json = serde_json::to_string(&empty_state).expect("Serialization should succeed");
    let restored: GameSavedState = serde_json::from_str(&json).expect("Deserialization should succeed");

    assert_eq!(restored.colonies.len(), 0);
    assert_eq!(restored.fleets.len(), 0);
    assert_eq!(restored.economy.treasury, 0.0);
}

#[test]
fn test_large_population_values() {
    let mut state = create_test_game_state();
    state.colonies[0].population = 1e12; // Very large population

    let json = serde_json::to_string(&state).expect("Serialization should succeed");
    let restored: GameSavedState = serde_json::from_str(&json).expect("Deserialization should succeed");

    assert_eq!(restored.colonies[0].population, 1e12);
}

#[test]
fn test_invalid_slot_returns_error() {
    let state = create_test_game_state();

    // Slot 99 is invalid (MAX_SLOTS = 10)
    let result = save_to_slot(99, &state, "Invalid");
    assert!(result.is_err());

    let result = load_from_slot(99);
    assert!(result.is_err());

    let result = get_slot_metadata(99);
    assert!(result.is_err());

    let result = delete_slot(99);
    assert!(result.is_err());
}