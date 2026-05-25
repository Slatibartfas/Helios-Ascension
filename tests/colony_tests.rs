//! Tests for Colony Management system — P0 system: Colony Management
//!
//! Verifies:
//! - Colony creation with initial population
//! - Building placement (add_building)
//! - Construction queue concepts
//! - Population growth (zero pop edge case)
//! - Logistics capacity calculations

use helios_ascension::colony::{BuildingType, Colony};

/// Colony::new should create colony with correct initial state.
#[test]
fn colony_new_with_initial_population() {
    let colony = Colony::new("Mars Base".to_string(), 10_000.0);
    assert_eq!(colony.name, "Mars Base");
    assert_eq!(colony.population, 10_000.0);
    assert_eq!(colony.growth_rate_modifier, 1.0);
    assert!(colony.buildings.is_empty());
}

/// Colony with zero initial population (edge case).
#[test]
fn colony_zero_initial_population() {
    let colony = Colony::new("Empty Base".to_string(), 0.0);
    assert_eq!(colony.population, 0.0, "Zero pop should be exactly 0");
    assert!(colony.buildings.is_empty());
}

/// building_count should return 0 for buildings not yet constructed.
#[test]
fn building_count_nonexistent() {
    let colony = Colony::new("Test".to_string(), 1_000.0);
    assert_eq!(colony.building_count(BuildingType::Factory), 0);
    assert_eq!(colony.building_count(BuildingType::Mine), 0);
}

/// add_building should increment building count.
#[test]
fn add_single_building() {
    let mut colony = Colony::new("Test".to_string(), 1_000.0);
    colony.add_building(BuildingType::Factory);
    assert_eq!(colony.building_count(BuildingType::Factory), 1);
    assert_eq!(colony.building_count(BuildingType::Mine), 0);
}

/// add_building should accumulate multiple buildings of same type.
#[test]
fn add_multiple_buildings() {
    let mut colony = Colony::new("Test".to_string(), 1_000.0);
    colony.add_building(BuildingType::Factory);
    colony.add_building(BuildingType::Factory);
    colony.add_building(BuildingType::Mine);
    assert_eq!(colony.building_count(BuildingType::Factory), 2);
    assert_eq!(colony.building_count(BuildingType::Mine), 1);
}

/// total_buildings should return correct sum.
#[test]
fn total_buildings_sum() {
    let mut colony = Colony::new("Test".to_string(), 1_000.0);
    assert_eq!(colony.total_buildings(), 0);
    colony.add_building(BuildingType::Factory);
    colony.add_building(BuildingType::Factory);
    colony.add_building(BuildingType::Mine);
    assert_eq!(colony.total_buildings(), 3);
}

/// Earth colony should have effectively infinite logistics capacity.
#[test]
fn earth_infinite_logistics() {
    let colony = Colony::new("Earth".to_string(), 8_200_000_000.0);
    assert!(
        colony.logistics_capacity() > 1_000_000_000.0,
        "Earth should have huge logistics capacity"
    );
}

/// Non-Earth colony with no logistics buildings should have zero capacity.
#[test]
fn colony_no_logistics_zero_capacity() {
    let colony = Colony::new("Asteroid Base".to_string(), 100.0);
    assert_eq!(
        colony.logistics_capacity(),
        0.0,
        "No logistics buildings = 0 capacity"
    );
}

/// MassDriver=5000, OrbitalLift=20000, CargoTerminal=2000 logistics contributions.
#[test]
fn logistics_capacity_formula() {
    let mut colony = Colony::new("Test".to_string(), 1_000.0);
    colony.add_building(BuildingType::MassDriver);
    colony.add_building(BuildingType::MassDriver);
    colony.add_building(BuildingType::OrbitalLift);
    colony.add_building(BuildingType::CargoTerminal);

    // 2×5000 + 1×20000 + 1×2000
    let expected = 2.0 * 5_000.0 + 1.0 * 20_000.0 + 1.0 * 2_000.0;
    assert_eq!(
        colony.logistics_capacity(),
        expected,
        "Expected capacity {}, got {}",
        expected,
        colony.logistics_capacity()
    );
}

/// logistics_efficiency for colony with no demand should be 1.0 (no penalty).
#[test]
fn logistics_efficiency_no_demand() {
    let colony = Colony::new("Small Base".to_string(), 100.0);
    // No industrial buildings → zero demand → efficiency 1.0
    assert_eq!(
        colony.logistics_efficiency(),
        1.0,
        "Zero demand should give efficiency 1.0"
    );
}

/// logistics_efficiency with insufficient capacity should be < 1.0.
#[test]
fn logistics_efficiency_insufficient_capacity() {
    let mut colony = Colony::new("Test".to_string(), 1_000.0);
    // 1 mine = 1000 demand, but 0 capacity
    colony.add_building(BuildingType::Mine);
    let eff = colony.logistics_efficiency();
    assert!(
        eff < 1.0,
        "Insufficient capacity should penalize efficiency, got {}",
        eff
    );
    assert!(eff >= 0.0, "Efficiency should be non-negative, got {}", eff);
}

/// logistics_efficiency capped at 1.0 even with excess capacity.
#[test]
fn logistics_efficiency_capped_at_one() {
    let mut colony = Colony::new("Test".to_string(), 1_000.0);
    // Add massive logistics
    for _ in 0..100 {
        colony.add_building(BuildingType::MassDriver);
    }
    // Still have some demand but way more capacity
    colony.add_building(BuildingType::Factory);
    assert!(
        colony.logistics_efficiency() <= 1.0,
        "Efficiency {} should be ≤ 1.0",
        colony.logistics_efficiency()
    );
}

/// All BuildingType variants should have a non-empty display name.
#[test]
fn building_types_have_display_names() {
    let building_types = [
        BuildingType::LifeSupport,
        BuildingType::Housing,
        BuildingType::UndergroundHabitat,
        BuildingType::Mine,
        BuildingType::Refinery,
        BuildingType::Factory,
        BuildingType::SolarPower,
        BuildingType::FissionReactor,
        BuildingType::FusionReactor,
        BuildingType::AgriDome,
        BuildingType::Farm,
        BuildingType::MedicalCenter,
        BuildingType::ResearchLab,
        BuildingType::CommercialHub,
        BuildingType::Shipyard,
        BuildingType::MassDriver,
        BuildingType::OrbitalLift,
        BuildingType::CargoTerminal,
    ];

    for bt in building_types {
        let name = bt.display_name();
        assert!(
            !name.is_empty(),
            "BuildingType {:?} should have non-empty display name",
            bt
        );
    }
}
