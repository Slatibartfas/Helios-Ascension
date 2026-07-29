//! Cross-system capability validation for ship_hulls.ron (GRA-333).
//!
//! These tests assert structural invariants of the new optional
//! `interstellar_capability` field on every `ShipHullDefinition`. They run
//! against the live RON file loaded through `ShipbuildingPlugin`, so a typo
//! in `assets/data/ship_hulls.ron` fails the test rather than silently
//! shipping.

use bevy::prelude::*;

use helios_ascension::economy::budget::GlobalBudget;
use helios_ascension::economy::logistics::PendingResourceRequests;
use helios_ascension::shipbuilding::{
    HullSlotDefinition, ShipHullDefinition, ShipModuleCategory, ShipbuildingData,
    ShipbuildingPlugin,
};
use helios_ascension::ui::SimulationTime;

fn load_shipbuilding_data_for_test() -> ShipbuildingData {
    let mut app = App::new();
    // ShipbuildingPlugin's Update chain needs `ResearchState`,
    // `PendingResourceRequests`, and `GlobalBudget`. None of the data
    // tests queue any actions, so the bodies are no-ops — but `Res<T>`
    // validation still fires on every tick. Register defaults so the
    // Update schedule can boot on `app.update()`.
    // Pattern lifted from `freighter_templates_data_tests.rs` and
    // `research_shipbuilding_startup_tests.rs`.
    app.add_plugins(MinimalPlugins)
        .insert_resource(GlobalBudget::default())
        .insert_resource(PendingResourceRequests::default())
        .init_resource::<helios_ascension::research::ResearchState>()
        .insert_resource(SimulationTime::new())
        .add_plugins(ShipbuildingPlugin);
    app.update();
    app.world().resource::<ShipbuildingData>().clone()
}

#[test]
fn hull_interstellar_capability_bp_premium_bounded() {
    let data = load_shipbuilding_data_for_test();
    assert!(
        !data.hulls.is_empty(),
        "ship_hulls.ron must load at least one hull"
    );
    for (id, hull) in &data.hulls {
        if let Some(cap) = &hull.interstellar_capability {
            assert!(
                (1.0..=2.0).contains(&cap.bp_premium),
                "hull {id} has bp_premium {} outside [1.0, 2.0]",
                cap.bp_premium
            );
        }
    }
}

#[test]
fn hull_interstellar_capability_implies_tier() {
    let data = load_shipbuilding_data_for_test();
    for (id, hull) in &data.hulls {
        if hull.interstellar_capability.is_some() {
            assert!(
                hull.tier >= 3,
                "hull {id} (tier {}) cannot be interstellar-capable; \
                 minimum tier is 3 (per GRA-333 v2 contract)",
                hull.tier
            );
        }
    }
}

#[test]
fn hull_interstellar_capability_matches_slot_layout() {
    let data = load_shipbuilding_data_for_test();
    for (id, hull) in &data.hulls {
        let Some(cap) = &hull.interstellar_capability else {
            continue;
        };
        if !cap.needs_torch_slot {
            continue;
        }
        let has_large_drive_slot = hull.slot_layout.iter().any(|slot: &HullSlotDefinition| {
            slot.size == "Large" && slot.category == ShipModuleCategory::FlightSystems
        });
        assert!(
            has_large_drive_slot,
            "hull {id} declares needs_torch_slot but slot_layout has no \
             Large FlightSystems slot; cannot accept an interstellar drive"
        );
    }
}

#[test]
fn hull_interstellar_capability_ron_roundtrip() {
    // Roundtrip one hull entry through RON to confirm `#[serde(default)]`
    // keeps save / load compatible when the field is absent.
    let data = load_shipbuilding_data_for_test();
    let hull: &ShipHullDefinition = data
        .hulls
        .values()
        .next()
        .expect("at least one hull loaded");
    let ron_str = ron::to_string(hull).expect("serialize hull to RON");
    let parsed: ShipHullDefinition =
        ron::from_str(&ron_str).expect("re-parse RON roundtrip of hull");
    assert_eq!(parsed.id, hull.id);
    assert_eq!(
        parsed.interstellar_capability.is_some(),
        hull.interstellar_capability.is_some()
    );
}
