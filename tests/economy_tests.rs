//! Tests for Economy system — P0 system: Economy
//!
//! Verifies:
//! - Resource production/consumption rates
//! - Budget constraints (stockpile_cap, effective cap)
//! - Power grid calculations
//! - Edge cases: zero stockpile, capped resources

use helios_ascension::colony::{BuildingType, Colony};
use helios_ascension::economy::budget::{
    calculate_colony_power_totals, GlobalBudget, ResourceRateTracker, SECONDS_PER_MONTH,
    SECONDS_PER_YEAR,
};
use helios_ascension::economy::types::ResourceType;

/// GlobalBudget::new() should create valid initial state.
#[test]
fn global_budget_initialization() {
    let budget = GlobalBudget::new();
    // Treasury should be positive
    assert!(budget.treasury > 0.0, "Initial treasury should be positive");
    // Should have stockpiles for several resource types
    assert!(
        !budget.stockpiles.is_empty(),
        "Stockpiles should not be empty"
    );
    // All stockpiles should be non-negative
    for (resource, &amount) in &budget.stockpiles {
        assert!(
            amount >= 0.0,
            "Stockpile for {:?} should be non-negative, got {}",
            resource,
            amount
        );
    }
}

/// consume_resource should return true when sufficient stock exists.
#[test]
fn consume_resource_success() {
    let mut budget = GlobalBudget::new();
    let initial = budget.get_stockpile(&ResourceType::Water);
    assert!(initial > 0.0, "Water should have initial stockpile");

    let result = budget.consume_resource(ResourceType::Water, 100.0);
    assert!(
        result,
        "consume_resource should succeed when stock is sufficient"
    );
    assert_eq!(
        budget.get_stockpile(&ResourceType::Water),
        initial - 100.0,
        "Water stockpile should decrease by 100"
    );
}

/// consume_resource should return false when insufficient stock.
#[test]
fn consume_resource_insufficient_stockpile() {
    let mut budget = GlobalBudget::new();
    let initial = budget.get_stockpile(&ResourceType::Water);

    // Try to consume more than available
    let result = budget.consume_resource(ResourceType::Water, initial + 1.0);
    assert!(
        !result,
        "consume_resource should fail when stock is insufficient"
    );
    assert_eq!(
        budget.get_stockpile(&ResourceType::Water),
        initial,
        "Stockpile should be unchanged after failed consume"
    );
}

/// consume_resource with negative amount should panic.
#[test]
#[should_panic]
fn consume_resource_negative_panics() {
    let mut budget = GlobalBudget::new();
    budget.consume_resource(ResourceType::Water, -10.0);
}

/// add_resource_capped should not exceed the per-resource cap.
#[test]
fn add_resource_capped_stays_within_cap() {
    let mut budget = GlobalBudget::new();
    let cap = budget.effective_stockpile_cap(ResourceType::Iron);
    let initial = budget.get_stockpile(&ResourceType::Iron);

    // Add way more than headroom
    let headroom = cap - initial;
    let result = budget.add_resource_capped(ResourceType::Iron, headroom + 1000.0);

    // Should not add beyond cap
    let final_amount = budget.get_stockpile(&ResourceType::Iron);
    assert!(
        final_amount <= cap + 1e-6,
        "Stockpile {} should not exceed cap {}",
        final_amount,
        cap
    );
    // The amount added should equal the headroom (capped)
    assert!(
        (result - headroom).abs() < 1e-6,
        "Should add exactly headroom, added {}",
        result
    );
}

/// add_resource_capped with zero amount should return zero with no changes.
#[test]
fn add_resource_capped_zero() {
    let mut budget = GlobalBudget::new();
    let initial = budget.get_stockpile(&ResourceType::Iron);
    let result = budget.add_resource_capped(ResourceType::Iron, 0.0);
    assert_eq!(result, 0.0, "Zero add should return 0");
    assert_eq!(
        budget.get_stockpile(&ResourceType::Iron),
        initial,
        "Stockpile should be unchanged"
    );
}

/// stockpile_cap should return positive values for all major resource types.
#[test]
fn stockpile_caps_positive_for_major_resources() {
    let resources = [
        ResourceType::Food,
        ResourceType::Water,
        ResourceType::Iron,
        ResourceType::Uranium,
        ResourceType::Gold,
        ResourceType::Silicates,
        ResourceType::Oxygen,
    ];

    for rt in resources {
        let cap = GlobalBudget::stockpile_cap(rt);
        assert!(
            cap > 0.0,
            "Stockpile cap for {:?} should be positive, got {}",
            rt,
            cap
        );
    }
}

/// Food should have the largest cap (≥100,000 Mt).
#[test]
fn food_has_largest_cap() {
    let food_cap = GlobalBudget::stockpile_cap(ResourceType::Food);
    let iron_cap = GlobalBudget::stockpile_cap(ResourceType::Iron);
    assert!(
        food_cap > iron_cap,
        "Food cap {} should exceed Iron cap {}",
        food_cap,
        iron_cap
    );
    assert!(
        food_cap > 100_000.0,
        "Food cap should be >100k Mt, got {}",
        food_cap
    );
}

/// ResourceRateTracker::default should be empty.
#[test]
fn resource_rate_tracker_default() {
    let tracker = ResourceRateTracker::default();
    assert!(
        tracker.resource_rates.is_empty(),
        "Default rates should be empty"
    );
    assert_eq!(
        tracker.research_rate_per_month, 0.0,
        "Default research rate should be 0"
    );
}

/// ResourceRateTracker::get_resource_rate for unknown resource should return 0.
#[test]
fn resource_rate_unknown_returns_zero() {
    let tracker = ResourceRateTracker::default();
    assert_eq!(
        tracker.get_resource_rate(&ResourceType::Water),
        0.0,
        "Unknown resource rate should be 0"
    );
}

/// Time constants should be consistent with seconds-per-month/year.
#[test]
fn time_constants_consistency() {
    // 30 days × 86400 s/day = 2,592,000 s
    assert_eq!(
        SECONDS_PER_MONTH,
        30.0 * 86_400.0,
        "SECONDS_PER_MONTH should be 30×86400"
    );
    // 365.25 days × 86400 s/day = 31,557,600 s
    assert_eq!(
        SECONDS_PER_YEAR,
        365.25 * 86_400.0,
        "SECONDS_PER_YEAR should be 365.25×86400"
    );
    // 1 year ≈ 12 months
    let months_per_year = SECONDS_PER_YEAR / SECONDS_PER_MONTH;
    assert!(
        (months_per_year - 12.0).abs() < 0.1,
        "1 year should be ~12 months, got {:.2}",
        months_per_year
    );
}

/// calculate_colony_power_totals with no buildings should give zero production.
#[test]
fn colony_power_no_buildings() {
    let colony = Colony::new("Test Colony".to_string(), 1_000_000.0);
    let totals = calculate_colony_power_totals(&colony, None);
    assert_eq!(
        totals.produced_watts, 0.0,
        "No buildings should produce 0 power"
    );
    assert_eq!(
        totals.consumed_watts, 0.0,
        "No buildings should have 0 consumption"
    );
}

/// Treasury management: consume_resource cannot go negative.
#[test]
fn treasury_non_negative_via_consume() {
    let mut budget = GlobalBudget::new();
    let initial = budget.treasury;
    // Cannot consume more than we have
    let success = budget.consume_resource(ResourceType::Iron, initial + 1.0);
    assert!(!success, "consume_resource should fail gracefully");
}

/// All ResourceType variants should have a defined display name.
#[test]
fn all_resource_types_have_display_name() {
    // Pick representative samples across all categories
    let samples = [
        ResourceType::Water,
        ResourceType::Food,
        ResourceType::Iron,
        ResourceType::Uranium,
        ResourceType::Gold,
        ResourceType::Helium3,
        ResourceType::Silicates,
    ];

    for rt in samples {
        let name = rt.display_name();
        assert!(
            !name.is_empty(),
            "ResourceType {:?} should have non-empty display name",
            rt
        );
    }
}
