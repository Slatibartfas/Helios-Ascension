//! Integration tests for the GRA-111 3-tier resource reveal matrix.
//!
//! Covers the LGD design contract from [GRA-110](https://paperclip.klingspor.one/GRA/issues/GRA-110):
//!
//! 1. **Parameterized gate logic** — for each combination of
//!    `(MineralDeposits.tier, SubsurfaceStructure.tier,
//!    drill_missions_completed)`, `MineralDeposit::tier_breakdown`
//!    returns the expected `[revealed; 3]`.
//! 2. **Atmospheric case** — `is_atmospheric == true` → all three
//!    `revealed: false`.
//! 3. **No-`SurveyState` case** — helper handles `Option::None`
//!    without panic.
//! 4. **Drill-completion integration** — the
//!    `SurveyState::record_drill_mission_completed` counter wires
//!    through the same path the survey mission-completion system
//!    uses; verify the T3 gate flips when the counter crosses 1.
//! 5. **Body-aggregate header** — the dossier's
//!    `body_aggregate_tier_breakdown` sums revealed megatons across
//!    resources.
//! 6. **Reserve zero-default** — `Reserve.<field> <= 0.001` is
//!    treated as "not present" at that tier (matches the existing
//!    threshold in `src/economy/mining.rs:324,335,351`).
//!
//! No regressions to the existing `survey::` test suite (the tests
//! here only add coverage; nothing in the existing tests changes).

use helios_ascension::economy::components::{
    MineralDeposit, PlanetResources, ResourceReserve, SurveyLevel,
};
use helios_ascension::economy::discovery::{
    body_aggregate_tier_breakdown, tier_breakdown_for_reserve, TierLabel, TierReveal,
    RESERVE_PRESENT_THRESHOLD,
};
use helios_ascension::economy::{ResourcePhase, ResourceType};
use helios_ascension::survey::components::{DimensionFidelity, SurveyState};
use helios_ascension::survey::SurveyDimension;
use std::collections::HashMap;

/// Build a `SurveyState` with the given per-dimension tier
/// snapshot. Convenience for the parameterized gate tests.
fn state_with(mineral_tier: u8, subsurface_tier: u8, drill_done: u32) -> SurveyState {
    let mut state = SurveyState::default();
    state.set_fidelity(
        SurveyDimension::MineralDeposits,
        DimensionFidelity::at_tier(mineral_tier, 1.0, Some(0.0)),
    );
    state.set_fidelity(
        SurveyDimension::Subsurface,
        DimensionFidelity::at_tier(subsurface_tier, 1.0, Some(0.0)),
    );
    state.drill_missions_completed = drill_done;
    state
}

/// Build a `ResourceReserve` with all three depth fields populated.
/// Caller can mutate individual fields to drop below the
/// 0.001 Mt threshold.
fn reserve_with(proven: f64, deep: f64, bulk: f64) -> ResourceReserve {
    ResourceReserve::new(proven, deep, bulk, 0.5)
}

fn mineral_deposit(reserve: ResourceReserve) -> MineralDeposit {
    MineralDeposit {
        reserve,
        accessibility: 0.5,
        is_atmospheric: false,
        phase: ResourcePhase::Solid,
    }
}

fn atmospheric_deposit() -> MineralDeposit {
    MineralDeposit {
        reserve: reserve_with(0.0, 0.0, 0.0),
        accessibility: 0.0,
        is_atmospheric: true,
        phase: ResourcePhase::Vapor,
    }
}

// ── 1. Parameterized gate logic ───────────────────────────────────

#[test]
fn gate_logic_unrevealed_for_zero_survey() {
    // MineralDeposits tier 0, Subsurface tier 0, no drill — every
    // tier locked. The matrix renders 3 dimmed placeholder rows.
    let deposit = mineral_deposit(reserve_with(100.0, 200.0, 300.0));
    let state = state_with(0, 0, 0);
    let breakdown = deposit.tier_breakdown(Some(&state));
    assert!(!breakdown[0].revealed);
    assert!(!breakdown[1].revealed);
    assert!(!breakdown[2].revealed);
}

#[test]
fn gate_logic_reveals_t1_at_mineral_tier_2_no_drill_required() {
    // T1 unlocks at MineralDeposits tier 2 with no drill flag.
    let deposit = mineral_deposit(reserve_with(100.0, 200.0, 300.0));
    let state = state_with(2, 0, 0);
    let breakdown = deposit.tier_breakdown(Some(&state));
    assert!(
        breakdown[0].revealed,
        "T1 should open at MineralDeposits tier 2"
    );
    assert!(!breakdown[1].revealed);
    assert!(!breakdown[2].revealed);
}

#[test]
fn gate_logic_reveals_t2_only_at_subsurface_t3_plus_mineral_t2() {
    // T2 needs both subsurface AND mineral gates. Verify all four
    // quadrants of the (subsurface, mineral) plane.
    let deposit = mineral_deposit(reserve_with(100.0, 200.0, 300.0));

    // subsurface 2, mineral 2 → T2 locked (subsurface below 3)
    let state = state_with(2, 2, 0);
    let breakdown = deposit.tier_breakdown(Some(&state));
    assert!(breakdown[0].revealed);
    assert!(
        !breakdown[1].revealed,
        "T2 should be locked at subsurface 2"
    );

    // subsurface 3, mineral 1 → T2 locked (mineral below 2)
    let state = state_with(1, 3, 0);
    let breakdown = deposit.tier_breakdown(Some(&state));
    assert!(!breakdown[0].revealed);
    assert!(!breakdown[1].revealed, "T2 should be locked at mineral 1");

    // subsurface 3, mineral 2 → T2 opens
    let state = state_with(2, 3, 0);
    let breakdown = deposit.tier_breakdown(Some(&state));
    assert!(breakdown[0].revealed);
    assert!(
        breakdown[1].revealed,
        "T2 should open at subsurface 3 + mineral 2"
    );
    assert!(!breakdown[2].revealed);
}

#[test]
fn gate_logic_reveals_t3_only_at_subsurface_t5_plus_drill_done() {
    // T3 has the strictest gate: subsurface >= 5 AND at least one
    // completed drill mission. Verify the four quadrants of
    // (subsurface, drill_done) — drill is the new flag introduced
    // by GRA-111.
    let deposit = mineral_deposit(reserve_with(100.0, 200.0, 300.0));

    // subsurface 5, no drill → T3 locked
    let state = state_with(2, 5, 0);
    let breakdown = deposit.tier_breakdown(Some(&state));
    assert!(!breakdown[2].revealed, "T3 should require drill completion");

    // subsurface 4, drill done → T3 locked
    let state = state_with(2, 4, 1);
    let breakdown = deposit.tier_breakdown(Some(&state));
    assert!(!breakdown[2].revealed, "T3 should require subsurface >= 5");

    // subsurface 5, drill done → T3 opens
    let state = state_with(2, 5, 1);
    let breakdown = deposit.tier_breakdown(Some(&state));
    assert!(
        breakdown[2].revealed,
        "T3 should open at subsurface 5 + drill"
    );

    // subsurface 5, multiple drill missions → T3 still open
    let state = state_with(2, 5, 3);
    let breakdown = deposit.tier_breakdown(Some(&state));
    assert!(breakdown[2].revealed);
}

#[test]
fn gate_logic_exposes_megatons_only_for_revealed_tiers() {
    // Megatons is `Some(_)` iff the tier is revealed. The dossier
    // renders an em-dash placeholder when `None`.
    let deposit = mineral_deposit(reserve_with(100.0, 200.0, 300.0));
    let state = state_with(2, 5, 1);
    let breakdown = deposit.tier_breakdown(Some(&state));
    assert_eq!(breakdown[0].megatons, Some(100.0));
    assert_eq!(breakdown[1].megatons, Some(200.0));
    assert_eq!(breakdown[2].megatons, Some(300.0));

    let state = state_with(0, 0, 0);
    let breakdown = deposit.tier_breakdown(Some(&state));
    for reveal in &breakdown {
        assert!(reveal.megatons.is_none());
        assert!(reveal.concentration.is_none());
    }
}

// ── 2. Atmospheric case ───────────────────────────────────────────

#[test]
fn atmospheric_deposit_collapses_to_three_dimmed_rows() {
    let deposit = atmospheric_deposit();
    let state = state_with(5, 5, 1);
    let breakdown = deposit.tier_breakdown(Some(&state));
    for (i, reveal) in breakdown.iter().enumerate() {
        assert!(!reveal.revealed, "atmospheric T{i} should be unrevealed");
        assert!(reveal.megatons.is_none());
        assert!(reveal.concentration.is_none());
    }
}

// ── 3. No-`SurveyState` case ──────────────────────────────────────

#[test]
fn no_survey_state_does_not_panic_and_returns_three_dimmed_rows() {
    let deposit = mineral_deposit(reserve_with(100.0, 200.0, 300.0));
    let breakdown = deposit.tier_breakdown(None);
    for (i, reveal) in breakdown.iter().enumerate() {
        assert!(!reveal.revealed, "no-state T{i} should be unrevealed");
        assert!(reveal.megatons.is_none());
        assert!(reveal.concentration.is_none());
    }
}

#[test]
fn helper_handles_none_state_at_full_reserve_mass() {
    // Sanity: a 100 Gt body with no SurveyState still doesn't
    // crash. The dossier will render a hint pointing at the
    // dispatch mission picker.
    let deposit = mineral_deposit(reserve_with(1.0e9, 1.0e9, 1.0e9));
    let _ = deposit.tier_breakdown(None);
}

// ── 4. Drill-completion integration ──────────────────────────────

#[test]
fn record_drill_mission_completed_opens_t3_gate() {
    // The mission-completion system calls
    // `SurveyState::record_drill_mission_completed` on a successful
    // drill mission. Verify the gate flips at the >= 1 threshold.
    let mut state = state_with(2, 5, 0);
    assert!(!state.planetary_bulk_unlocked(), "T3 should start locked");

    state.record_drill_mission_completed();
    assert_eq!(state.drill_missions_completed, 1);
    assert!(
        state.planetary_bulk_unlocked(),
        "T3 should open at drill count 1"
    );

    // Saturate: a body that has run dozens of drill missions still
    // has T3 open. No need to test the saturating_add overflow path
    // here — the underlying method uses `saturating_add` and the
    // u32 saturation is well-tested in `std`.
    for _ in 0..10 {
        state.record_drill_mission_completed();
    }
    assert_eq!(state.drill_missions_completed, 11);
    assert!(state.planetary_bulk_unlocked());
}

#[test]
fn planetary_bulk_unlocked_requires_both_subsurface_and_drill() {
    // The combined gate: even with many drill missions, a body
    // without enough subsurface tier can't unlock T3. This is the
    // exact recipe the LGD design contract asked for.
    let mut state = state_with(2, 4, 5);
    assert!(!state.planetary_bulk_unlocked());

    state.set_fidelity(
        SurveyDimension::Subsurface,
        DimensionFidelity::at_tier(5, 1.0, Some(0.0)),
    );
    assert!(state.planetary_bulk_unlocked());

    // Reset subsurface to 4 — T3 re-locks.
    state.set_fidelity(
        SurveyDimension::Subsurface,
        DimensionFidelity::at_tier(4, 1.0, Some(0.0)),
    );
    assert!(!state.planetary_bulk_unlocked());
}

// ── 5. Body-aggregate header ─────────────────────────────────────

#[test]
fn body_aggregate_sums_revealed_tier_megatons() {
    // Three iron-like deposits on a body, all revealed at T3.
    // The aggregate header should sum to 6 Gt.
    let mut resources = PlanetResources::default();
    let mut deposits = HashMap::new();
    deposits.insert(
        ResourceType::Iron,
        mineral_deposit(reserve_with(100.0, 200.0, 300.0)),
    );
    deposits.insert(
        ResourceType::Silicates,
        mineral_deposit(reserve_with(50.0, 80.0, 120.0)),
    );
    deposits.insert(
        ResourceType::Aluminum,
        mineral_deposit(reserve_with(0.0, 0.0, 50.0)),
    );
    resources.deposits = deposits;

    let state = state_with(2, 5, 1);
    let aggregate = body_aggregate_tier_breakdown(&resources, Some(&state));

    assert!(aggregate[0].revealed);
    assert!(aggregate[1].revealed);
    assert!(aggregate[2].revealed);
    assert_eq!(aggregate[0].megatons, Some(150.0));
    assert_eq!(aggregate[1].megatons, Some(280.0));
    assert_eq!(aggregate[2].megatons, Some(470.0));
}

#[test]
fn body_aggregate_marks_tier_revealed_when_at_least_one_deposit() {
    // Iron has 100 Mt of proven_crustal; silicates has zero across
    // the board. The aggregate T1 row should still be marked
    // `revealed: true` because at least one deposit has T1 open.
    let mut resources = PlanetResources::default();
    let mut deposits = HashMap::new();
    deposits.insert(
        ResourceType::Iron,
        mineral_deposit(reserve_with(100.0, 0.0, 0.0)),
    );
    deposits.insert(
        ResourceType::Silicates,
        mineral_deposit(reserve_with(0.0, 0.0, 0.0)),
    );
    resources.deposits = deposits;
    let state = state_with(2, 5, 1);
    let aggregate = body_aggregate_tier_breakdown(&resources, Some(&state));
    assert!(aggregate[0].revealed);
    assert!(!aggregate[1].revealed);
    assert!(!aggregate[2].revealed);
    assert_eq!(aggregate[0].megatons, Some(100.0));
}

// ── 6. Reserve zero-default ──────────────────────────────────────

#[test]
fn zero_reserve_collapses_to_dimmed_rows() {
    // The existing 0.001 Mt threshold from mining.rs:324,335,351
    // must be mirrored in tier_breakdown. A deposit with megatons
    // at or below the threshold is treated as "not present" at
    // that depth — gate or no gate, the row stays dimmed.
    let deposit = mineral_deposit(reserve_with(
        RESERVE_PRESENT_THRESHOLD,
        RESERVE_PRESENT_THRESHOLD,
        RESERVE_PRESENT_THRESHOLD,
    ));
    let state = state_with(5, 5, 1);
    let breakdown = deposit.tier_breakdown(Some(&state));
    for (i, reveal) in breakdown.iter().enumerate() {
        assert!(
            !reveal.revealed,
            "below-threshold T{i} should be unrevealed"
        );
    }
}

#[test]
fn reserve_at_threshold_plus_epsilon_reveals() {
    // Boundary check: 0.001 + 0.0001 should be above the
    // threshold and reveal the tier.
    let deposit = mineral_deposit(reserve_with(
        RESERVE_PRESENT_THRESHOLD + 0.0001,
        RESERVE_PRESENT_THRESHOLD + 0.0001,
        RESERVE_PRESENT_THRESHOLD + 0.0001,
    ));
    let state = state_with(2, 5, 1);
    let breakdown = deposit.tier_breakdown(Some(&state));
    for reveal in &breakdown {
        assert!(reveal.revealed, "above-threshold tier should reveal");
    }
}

// ── TierLabel & TierReveal helpers ───────────────────────────────

#[test]
fn tier_label_display_names_match_design_contract() {
    // T1 / T2 / T3 names from the LGD design contract:
    //   T1 = Proven Crustal / Atmospheric
    //   T2 = Deep Deposits / Trapped / Dissolved
    //   T3 = Planetary Bulk / Chemically Bound
    assert_eq!(TierLabel::ProvenCrustal.display(false), "Proven Crustal");
    assert_eq!(TierLabel::ProvenCrustal.display(true), "Atmospheric");
    assert_eq!(TierLabel::DeepDeposits.display(false), "Deep Deposits");
    assert_eq!(TierLabel::DeepDeposits.display(true), "Trapped / Dissolved");
    assert_eq!(TierLabel::PlanetaryBulk.display(false), "Planetary Bulk");
    assert_eq!(TierLabel::PlanetaryBulk.display(true), "Chemically Bound");
}

#[test]
fn tier_label_threshold_text_is_player_readable() {
    // The dimmed row surfaces the gate text. Lock these strings —
    // any rebalance in the underlying gate logic must update both
    // the helper and this test in the same PR.
    assert_eq!(
        TierLabel::ProvenCrustal.threshold_text(),
        "MineralDeposits \u{2265} 2"
    );
    assert_eq!(
        TierLabel::DeepDeposits.threshold_text(),
        "Subsurface \u{2265} 3 + Mineral \u{2265} 2"
    );
    assert_eq!(
        TierLabel::PlanetaryBulk.threshold_text(),
        "Subsurface \u{2265} 5 + drill"
    );
}

#[test]
fn tier_reveal_default_is_unrevealed() {
    // The dashboard call site uses `[Default::default(); 3]`. The
    // default must collapse to the dimmed placeholder rendering
    // (revealed: false, megatons: None, concentration: None,
    // gate: Unsurveyed).
    let reveal: TierReveal = TierReveal::default();
    assert!(!reveal.revealed);
    assert!(reveal.megatons.is_none());
    assert!(reveal.concentration.is_none());
    assert_eq!(reveal.gate, SurveyLevel::Unsurveyed);
}

#[test]
fn tier_breakdown_for_reserve_is_pure() {
    // `tier_breakdown_for_reserve` is a free function so the
    // dossier can call it without a deposit (e.g. on an empty
    // PlanetResources entry). Verify the pure-helper shape:
    // same args produce the same `[TierReveal; 3]` regardless of
    // ECS state.
    let reserve = reserve_with(100.0, 200.0, 300.0);
    let state = state_with(2, 5, 1);
    let a = tier_breakdown_for_reserve(reserve, false, Some(&state));
    let b = tier_breakdown_for_reserve(reserve, false, Some(&state));
    assert_eq!(a, b);
}
