//! Building-economy cost audit (GRA-22c Phase 2).
//!
//! Asserts two structural rules on every loaded `BuildingDefinition`:
//!
//! 1. **Resource diversity**: every `resource_costs` vec must have
//!    at least 4 distinct material entries. Buildings that draw on
//!    a single bulk material don't reflect the realistic bill-of-
//!    materials a player would actually purchase (steel + bearings +
//!    electronics + cabling + coolant + fasteners).
//! 2. **No single material dominant**: the largest per-material Mt
//!    in `resource_costs` must not exceed 60% of the building's
//!    total Mt. Builds that spend 667 Mt of Titanium and 50 Mt of
//!    everything else (Shipyard pre-Phase-2) are un-affordable for
//!    any colony without a Titanium stockpile cap.
//!
//! The hard-gate pipeline is `BuildingsData::load_for_tests()` in
//! the `colony::data` module — every assertion below loads the
//! real `assets/data/buildings.ron` so a regressing RON edit trips
//! the test before the build reaches a player.
//!
//! This file is the **executable enforcement** of the BoM
//! reference in `docs/BUILDING_BOM.md`. If the BoM doc changes,
//! these thresholds must change in lockstep.

use helios_ascension::colony::data::BuildingsData;
use helios_ascension::colony::types::BuildingType;

/// Minimum number of distinct materials in a building's
/// `resource_costs` (GRA-22c §2.1, Stage 2.0). Matches the
/// existing `audit_buildings` maintenance threshold
/// (`MAINTENANCE_AUDIT_MIN` = 4) so a building can't sneak past
/// the cost or maintenance gates with a single-resource tail.
pub const COST_AUDIT_MIN_MATERIALS: usize = 4;

/// Maximum fraction of total `resource_costs` Mt that any single
/// material may consume (GRA-22c §2.1). 60% leaves meaningful
/// diversity while still allowing "this resource is the
/// dominant input" — e.g. a dam's concrete-aggregate (Si) at
/// 70% would be repelled, but a dam where Si is 55% and steel
/// is 35% and copper is 10% passes.
pub const COST_AUDIT_MAX_SINGLE_SHARE: f64 = 0.60;

/// Floating-point tolerance for the single-share check. Iron / Cu
/// mix-ins where some lines round-trip through `f64` should be
/// evaluated with a 1e-9 tolerance.
pub const COST_AUDIT_EPSILON: f64 = 1e-9;

/// BuildingType variants that are intentionally excluded from
/// the cost-audit thresholds. The list tracks the
/// "orphan variant" set — variants kept in the enum so existing
/// saves deserialize cleanly, but with no RON definition (the
/// RON was renamed or removed in a recent phase). Add to this
/// list when an enum variant is intentionally orphaned; never
/// remove entries without verifying all variants in the list
/// are still orphans.
pub const ORPHAN_BUILDING_VARIANTS: &[BuildingType] = &[
    // v3.10 (GRA-22c Phase 4C-2): SpacePort. The RON entry was
    // renamed to `ControlCenter`. The enum variant stays for
    // save-game backward compatibility, but new builds look up
    // `BuildingType::SpacePort` → no RON match → 0 Mt cost.
    //
    // TradePort: removed from this list — the enum variant was
    // also removed in v3.10 (GRA-22c Phase 4C-2). Saves with
    // `TradePort` counts deserialize as orphan entries
    // (unknown enum variant → serde_json silently drops the
    // count). The orphan-list no longer needs to exclude it
    // because `BuildingType::all()` no longer iterates over it.
    BuildingType::SpacePort,
];

#[test]
fn every_building_has_at_least_four_construction_materials() {
    let data = BuildingsData::load_for_tests();
    let mut failures: Vec<String> = Vec::new();
    for &bt in BuildingType::all() {
        if ORPHAN_BUILDING_VARIANTS.contains(&bt) {
            continue;
        }
        let costs = data.resource_costs(&bt);
        let distinct: std::collections::BTreeSet<&str> =
            costs.iter().map(|(name, _)| name.as_str()).collect();
        if distinct.len() < COST_AUDIT_MIN_MATERIALS {
            failures.push(format!(
                "{bt:?}: {} distinct materials, expected >= {} ({:?})",
                distinct.len(),
                COST_AUDIT_MIN_MATERIALS,
                costs
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} building(s) have fewer than {COST_AUDIT_MIN_MATERIALS} distinct \
         construction materials:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn no_single_material_dominates_any_building_construction_cost() {
    let data = BuildingsData::load_for_tests();
    let mut failures: Vec<String> = Vec::new();
    for &bt in BuildingType::all() {
        let costs = data.resource_costs(&bt);
        if costs.is_empty() {
            continue;
        }
        let total: f64 = costs.iter().map(|(_, mt)| *mt).sum();
        if total <= 0.0 {
            continue;
        }
        let max_mt = costs.iter().map(|(_, mt)| *mt).fold(0.0_f64, f64::max);
        let share = max_mt / total;
        if share - COST_AUDIT_MAX_SINGLE_SHARE > COST_AUDIT_EPSILON {
            failures.push(format!(
                "{bt:?}: dominant material at {:.1}% (max {max_mt:.2} Mt / total \
                 {total:.2} Mt), expected <= {:.0}%",
                share * 100.0,
                COST_AUDIT_MAX_SINGLE_SHARE * 100.0,
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} building(s) have a single construction material exceeding \
         {:.0}% of the total cost:\n{}",
        failures.len(),
        COST_AUDIT_MAX_SINGLE_SHARE * 100.0,
        failures.join("\n")
    );
}

#[test]
fn cost_audit_thresholds_match_reference() {
    // If anyone tunes `COST_AUDIT_MIN_MATERIALS` or
    // `COST_AUDIT_MAX_SINGLE_SHARE`, this test fails to force the
    // doc update (`docs/BUILDING_BOM.md`).
    //
    // v3.10 (GRA-22c Phase 4C-2): catalogue size 97 → 96
    // (TradePort removed; maintenance draw reassigned to
    // CommercialHub).
    assert_eq!(COST_AUDIT_MIN_MATERIALS, 4);
    assert!((COST_AUDIT_MAX_SINGLE_SHARE - 0.60).abs() < COST_AUDIT_EPSILON);
    assert_eq!(96, BuildingType::all().len(), "catalogue drift");
}
