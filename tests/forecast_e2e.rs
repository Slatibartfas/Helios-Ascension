//! Integration tests for the resource depletion forecast chart.
//!
//! Covers the e2e path: build a `ScopeInputs` from local stockpiles,
//! feed pending construction impacts, and assert the projected curve
//! has the expected shape.  Does not exercise the egui renderer (that
//! lives in the `ui_economy_panels` system which is harder to drive
//! headlessly); instead it pins down the public `economy::forecast`
//! API contract.

use helios_ascension::economy::forecast::{
    apply_construction_impact, build_forecast, project_stockpile, ConstructionImpact,
    ReserveBounds, ScopeInputs, StorageCaps, FORECAST_SAMPLES,
};
use helios_ascension::economy::ResourceType;

#[test]
fn forecast_end_to_end_basic_projection() {
    // Earth in 2026: a small stockpile of iron, a steady monthly draw,
    // no pending construction.  The chart should produce a series for
    // Iron with a finite "runs out" timestamp.
    //
    // ScopeInputs stores (current_mt, **monthly** rate).  Build_forecast
    // converts to annual internally.  -10 Mt/yr means -10/12 ≈ -0.833
    // Mt/mo here.
    let mut scope = ScopeInputs::new();
    scope
        .resources
        .insert(ResourceType::Iron, (100.0, -10.0 / 12.0));

    let series = build_forecast(&scope, &[], 0.0, &StorageCaps::new(), &ReserveBounds::new());
    assert_eq!(series.len(), 1);
    let iron = &series[0];
    assert_eq!(iron.resource, ResourceType::Iron);
    assert_eq!(iron.samples.len(), FORECAST_SAMPLES);
    assert!((iron.current_mt - 100.0).abs() < 1e-9);

    // At -10 Mt/yr on 100 Mt, depletion at year 10.
    let runs_out = iron.runs_out_at_s.expect("iron must deplete");
    let expected = 10.0 * 31_557_600.0;
    assert!((runs_out - expected).abs() < 1.0, "runs_out was {runs_out}");
}

#[test]
fn forecast_construction_step_change() {
    // 50 Mt at -5 Mt/yr (≈ -0.417 Mt/mo); a Farm completes at year 5
    // and adds +10 Mt/yr to the food rate.  Net post-impact: +5 Mt/yr.
    let mut scope = ScopeInputs::new();
    scope
        .resources
        .insert(ResourceType::Food, (50.0, -5.0 / 12.0));

    let impact = ConstructionImpact {
        resource: ResourceType::Food,
        completion_sim_seconds: 5.0 * 31_557_600.0,
        delta_mt_per_year: 15.0, // -5 + 15 = +10 post-completion (annual)
    };
    let series = build_forecast(
        &scope,
        &[impact],
        0.0,
        &StorageCaps::new(),
        &ReserveBounds::new(),
    );
    assert_eq!(series.len(), 1);
    let food = &series[0];
    assert_eq!(food.resource, ResourceType::Food);

    // Pre-impact: linear draw.  At t=5 yr → ~25 Mt remaining.
    // Post-impact: net +10 Mt/yr.  At t=20 yr → ~25 + 15*10 = ~175 Mt.
    let at_5 = food
        .samples
        .iter()
        .find(|p| (p.sim_seconds_offset - 5.0 * 31_557_600.0).abs() < 31_557_600.0 / 24.0)
        .map(|p| p.value_mt)
        .expect("near-5yr sample");
    assert!((at_5 - 25.0).abs() < 5.0, "pre-impact value at 5yr: {at_5}");

    let last = food.samples.last().unwrap().value_mt;
    assert!(
        (last - 175.0).abs() < 10.0,
        "endpoint was {last}, expected ~175"
    );
}

#[test]
fn forecast_skips_zero_resources_with_no_movement() {
    let mut scope = ScopeInputs::new();
    // 0 Mt, 0 rate → no projection (curve would be a flat 0 line).
    scope.resources.insert(ResourceType::Gold, (0.0, 0.0));
    let series = build_forecast(&scope, &[], 0.0, &StorageCaps::new(), &ReserveBounds::new());
    assert!(series.is_empty(), "empty series filtered out");
}

#[test]
fn forecast_skips_zero_stock_with_active_rate() {
    // Active rate but no stock — still produces a series (player needs
    // to know they're bleeding).
    let mut scope = ScopeInputs::new();
    scope.resources.insert(ResourceType::Gold, (0.0, -2.0));
    let series = build_forecast(&scope, &[], 0.0, &StorageCaps::new(), &ReserveBounds::new());
    assert_eq!(series.len(), 1);
    // No runs_out (no stock to hit zero from).
    assert!(series[0].runs_out_at_s.is_none());
}

#[test]
fn forecast_piecewise_linear_no_discontinuity() {
    // Sample 4 is before the impact, sample 5 is at or after the impact.
    // The curve should be C0-continuous at the kink.
    let mut series = project_stockpile(100.0, -10.0, None, None);
    let impact = ConstructionImpact {
        resource: ResourceType::Iron,
        completion_sim_seconds: 10.0 * 31_557_600.0,
        delta_mt_per_year: 25.0, // -10 + 25 = +15 Mt/yr post-impact
    };
    apply_construction_impact(&mut series, 0.0, impact);

    // Find the sample closest to the impact timestamp.
    let target = 10.0 * 31_557_600.0;
    let mut best_idx = 0;
    let mut best_d = f64::INFINITY;
    for (i, p) in series.samples.iter().enumerate() {
        let d = (p.sim_seconds_offset - target).abs();
        if d < best_d {
            best_d = d;
            best_idx = i;
        }
    }
    let idx_at = best_idx;
    let idx_pre = idx_at.saturating_sub(1);
    let idx_post = (idx_at + 1).min(series.samples.len() - 1);

    let pre = series.samples[idx_pre].value_mt;
    let at = series.samples[idx_at].value_mt;
    let post = series.samples[idx_post].value_mt;
    // The pre segment is monotonically decreasing; at <= pre.
    assert!(
        pre >= at - 1.0,
        "pre segment not monotonic (pre={pre}, at={at})"
    );
    // The post-impact rate is +15 Mt/yr but the impact itself is at the
    // very boundary of the sample — post may equal at or rise slightly.
    assert!(
        post >= at - 1.0,
        "post segment should not fall below the kink (at={at}, post={post})"
    );
}

#[test]
fn forecast_sorted_alphabetically() {
    // Mixed-order input should come out sorted by display name.
    let mut scope = ScopeInputs::new();
    scope.resources.insert(ResourceType::Iron, (50.0, -1.0));
    scope.resources.insert(ResourceType::Food, (100.0, 1.0));
    scope.resources.insert(ResourceType::Water, (200.0, -5.0));
    let series = build_forecast(&scope, &[], 0.0, &StorageCaps::new(), &ReserveBounds::new());
    assert_eq!(series.len(), 3);
    let names: Vec<&str> = series.iter().map(|s| s.resource.display_name()).collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted, "series should be alphabetically sorted");
}

#[test]
fn forecast_handles_zero_horizon_depletion() {
    // Instant depletion: current stock + rate that would have already
    // run out before t=0.  Monthly rate = -1e9 means annual = -1.2e10,
    // which empties 10 Mt in ~1 microsecond.
    let mut scope = ScopeInputs::new();
    scope.resources.insert(ResourceType::Iron, (10.0, -1e9));
    let series = build_forecast(&scope, &[], 0.0, &StorageCaps::new(), &ReserveBounds::new());
    assert_eq!(series.len(), 1);
    let runs_out = series[0].runs_out_at_s.expect("must deplete");
    assert!(
        runs_out < 100.0,
        "depletion must be near-instant (was {runs_out})"
    );
}

#[test]
fn forecast_impact_far_future_is_ignored() {
    // Construction impact after the 20-yr horizon should not affect the
    // curve (we don't have samples beyond the horizon).
    let mut scope = ScopeInputs::new();
    // Monthly rate = -5 Mt/yr ÷ 12 ≈ -0.417
    scope
        .resources
        .insert(ResourceType::Iron, (100.0, -5.0 / 12.0));
    let impact = ConstructionImpact {
        resource: ResourceType::Iron,
        completion_sim_seconds: 100.0 * 31_557_600.0, // 100 yr out
        delta_mt_per_year: 50.0,
    };
    let series = build_forecast(
        &scope,
        &[impact],
        0.0,
        &StorageCaps::new(),
        &ReserveBounds::new(),
    );
    assert_eq!(series.len(), 1);
    // Curve unchanged at the horizon: 100 + (-5/12)*12*20 = 100 - 100 = 0
    let last = series[0].samples.last().unwrap().value_mt;
    let expected: f64 = (100.0_f64 - 5.0 * 20.0).max(0.0_f64);
    assert!((last - expected).abs() < 1.0, "horizon endpoint was {last}");
}
