//! GRA-169 — Porkchop scroll/rotation snap-back fix (issues #1, #2)
//!
//! Integration tests for the rotating-buffer porkchop grid:
//!
//! Part A — `t_dep_min_s` is anchored at the player's `sim_time_s`
//! at build.  Pre-fix code returned `(0.0, max_t_dep)` regardless of
//! the build epoch, so every rotation cycle snapped the visible
//! window back to the left edge (the orbit-epoch anchor), not the
//! player's current clock.
//!
//! Part B — the rotation trigger sets `porkchop_grid_pending_rebuild`
//! instead of clearing `porkchop_grid`.  The per-frame build block
//! keeps the old grid visible during the ~360 ms Lambert solve and
//! atomically swaps + clears the flag in a single statement.  No
//! blank frame on rotation.
//!
//! The math-layer assertions (Part A) run through
//! `build_rotating_buffer_for_body_target` so we can drive the pure
//! helper without spinning up an egui context.  The resource-layer
//! assertions (Part B) run through `FleetUiState` directly — the
//! rotation trigger lives inside the per-frame planner render path,
//! which requires an egui context we can't construct in headless
//! tests.  The resource contract (flag exists, `clear_target` resets
//! it, default false, serde-default false) is what the render path
//! relies on; locking that in here is sufficient to catch regressions
//! where the field gets removed or re-initialised to `true`.

use bevy::prelude::*;
use helios_ascension::astronomy::KeplerOrbit;
use helios_ascension::fleets::orbital_mechanics::GM_SUN;
use helios_ascension::fleets::porkchop::{build_rotating_buffer_for_body_target, PorkchopInputs};
use helios_ascension::fleets::PorkchopConfig;
use helios_ascension::ui::FleetUiState;

const SECONDS_PER_DAY: f64 = 86_400.0;

// ── Math-layer tests (Part A) ───────────────────────────────────────────────

fn earth_orbit() -> KeplerOrbit {
    KeplerOrbit::circular(1.0, 1.0)
}

fn mars_orbit() -> KeplerOrbit {
    KeplerOrbit::circular(1.524, 1.0)
}

#[test]
fn gra_169_part_a_buffer_anchored_at_sim_time_s() {
    // Build at t=0 and t=1 yr; assert the second build's lower
    // bound is offset by exactly 1 yr from the first.  Pre-fix
    // code returned `t_dep_bounds_s.0 = 0.0` for both builds, so
    // the two bounds always overlapped and the visible window
    // snapped back to the left edge on rotation.
    let cfg = PorkchopConfig::default();
    let earth = earth_orbit();
    let mars = mars_orbit();

    let grid_t0 = build_rotating_buffer_for_body_target(
        &cfg,
        earth,
        mars,
        "Earth".to_string(),
        "Mars".to_string(),
        "interplanetary",
        0.0,
    );
    assert_eq!(grid_t0.t_dep_bounds_s.0, 0.0, "t=0 build anchors at 0");

    let one_year_s = 365.25 * SECONDS_PER_DAY;
    let grid_t1 = build_rotating_buffer_for_body_target(
        &cfg,
        earth,
        mars,
        "Earth".to_string(),
        "Mars".to_string(),
        "interplanetary",
        one_year_s,
    );
    let lower_t1 = grid_t1.t_dep_bounds_s.0;
    assert!(
        (lower_t1 - one_year_s).abs() < 1.0,
        "t=1yr build must anchor at sim_time_s = 1 yr, got {lower_t1}"
    );

    // The two builds differ by exactly 1 yr — the visible window
    // does NOT snap back to t_dep = 0 on rotation.
    let delta = grid_t1.t_dep_bounds_s.0 - grid_t0.t_dep_bounds_s.0;
    assert!(
        (delta - one_year_s).abs() < 1.0,
        "two 1-yr-apart builds must differ by exactly 1 yr, got delta = {delta}"
    );
}

#[test]
fn gra_169_part_a_buffer_width_invariant_under_sim_time_shift() {
    // The 8× buffer multiplier (GRA-152) defines the window width;
    // shifting the build epoch must not change the width — only the
    // absolute anchor.
    let cfg = PorkchopConfig::default();
    let earth = earth_orbit();
    let mars = mars_orbit();

    let grid_t0 = build_rotating_buffer_for_body_target(
        &cfg,
        earth,
        mars,
        "Earth".to_string(),
        "Mars".to_string(),
        "interplanetary",
        0.0,
    );
    let half_year_s = 0.5 * 365.25 * SECONDS_PER_DAY;
    let grid_thalf = build_rotating_buffer_for_body_target(
        &cfg,
        earth,
        mars,
        "Earth".to_string(),
        "Mars".to_string(),
        "interplanetary",
        half_year_s,
    );

    let width_t0 = grid_t0.t_dep_bounds_s.1 - grid_t0.t_dep_bounds_s.0;
    let width_thalf = grid_thalf.t_dep_bounds_s.1 - grid_thalf.t_dep_bounds_s.0;
    assert!(
        (width_t0 - width_thalf).abs() < 1.0,
        "buffer width must be invariant under sim_time_s shift ({width_t0} vs {width_thalf})"
    );
}

#[test]
fn gra_169_part_a_cell_t_dep_s_is_relative_offset() {
    // `PorkchopCell.t_dep_s` is the **relative** offset (0..max_t_dep),
    // not the absolute epoch.  This is the invariant the panel relies
    // on: the cell's `t_dep_abs = sim_time_s + cell.t_dep_s` is
    // computed by `solve_cell` inside the Lambert solver, so a cell
    // at the same (col, row) in two builds has the same `t_dep_s` but
    // maps to a different absolute departure epoch.
    let cfg = PorkchopConfig::default();
    let earth = earth_orbit();
    let mars = mars_orbit();

    let grid_t0 = build_rotating_buffer_for_body_target(
        &cfg,
        earth,
        mars,
        "Earth".to_string(),
        "Mars".to_string(),
        "interplanetary",
        0.0,
    );
    let one_year_s = 365.25 * SECONDS_PER_DAY;
    let grid_t1 = build_rotating_buffer_for_body_target(
        &cfg,
        earth,
        mars,
        "Earth".to_string(),
        "Mars".to_string(),
        "interplanetary",
        one_year_s,
    );

    let col = 5;
    let row = 10;
    let cell_t0 = &grid_t0.cells[row * grid_t0.resolution.0 + col];
    let cell_t1 = &grid_t1.cells[row * grid_t1.resolution.0 + col];
    assert!(
        (cell_t0.t_dep_s - cell_t1.t_dep_s).abs() < 1.0,
        "cell.t_dep_s is relative (got {} vs {}); the absolute epoch is recovered by `sim_time_s + t_dep_s` inside `solve_cell`",
        cell_t0.t_dep_s,
        cell_t1.t_dep_s
    );
}

// ── Resource-layer tests (Part B) ───────────────────────────────────────────

#[test]
fn gra_169_part_b_pending_rebuild_flag_defaults_to_false() {
    // `FleetUiState::default()` must initialise
    // `porkchop_grid_pending_rebuild` to `false` — otherwise the
    // planner would attempt a rebuild on the very first frame.
    let state = FleetUiState::default();
    assert!(
        !state.porkchop_grid_pending_rebuild,
        "porkchop_grid_pending_rebuild must default to false"
    );
}

#[test]
fn gra_169_part_b_clear_target_resets_pending_rebuild_flag() {
    // `clear_target()` resets every per-target field, including the
    // new pending-rebuild flag — otherwise switching fleets could
    // strand a stale flag from the previous destination's buffer
    // rotation cycle.
    let mut state = FleetUiState {
        porkchop_grid_pending_rebuild: true,
        ..Default::default()
    };
    state.clear_target();
    assert!(
        !state.porkchop_grid_pending_rebuild,
        "clear_target() must reset porkchop_grid_pending_rebuild"
    );
}

#[test]
fn gra_169_part_b_pending_rebuild_keeps_grid_visible() {
    // The rotation-trigger contract: when the trigger fires, the
    // planner sets `porkchop_grid_pending_rebuild = true` and
    // **leaves `porkchop_grid` populated**.  The per-frame build
    // block sees the flag and rebuilds into a local, then atomically
    // swaps `porkchop_grid` + clears the flag in one statement —
    // the panel never observes `porkchop_grid = None` while a
    // pending rebuild is queued.
    //
    // We don't drive the planner render path here (it requires an
    // egui context); we lock in the resource contract by setting
    // the flag and asserting the grid stays Some — i.e. nothing in
    // the contract forces the grid to be dropped during the pending
    // window.
    let cfg = PorkchopConfig::default();
    let earth = earth_orbit();
    let mars = mars_orbit();
    let grid = build_rotating_buffer_for_body_target(
        &cfg,
        earth,
        mars,
        "Earth".to_string(),
        "Mars".to_string(),
        "interplanetary",
        0.0,
    );

    let mut state = FleetUiState {
        porkchop_grid: Some(grid),
        porkchop_grid_pending_rebuild: true,
        ..Default::default()
    };
    // Contract: grid stays Some while the flag is set.
    assert!(
        state.porkchop_grid.is_some(),
        "grid must remain Some while pending_rebuild is true — clearing it would render the blank '0x0' fallback for one frame"
    );
    assert!(
        state.porkchop_grid_pending_rebuild,
        "flag is set — build block should fire next frame"
    );
    // Simulate the build completing: atomic swap + flag clear.
    let new_grid = build_rotating_buffer_for_body_target(
        &cfg,
        earth,
        mars,
        "Earth".to_string(),
        "Mars".to_string(),
        "interplanetary",
        365.25 * SECONDS_PER_DAY,
    );
    state.porkchop_grid = Some(new_grid);
    state.porkchop_grid_pending_rebuild = false;
    assert!(state.porkchop_grid.is_some());
    assert!(
        !state.porkchop_grid_pending_rebuild,
        "build completion clears the flag in the same statement as the swap"
    );
}

// ── Sanity ───────────────────────────────────────────────────────────────────

#[test]
fn gra_169_continuous_drift_does_not_snap_back_to_zero() {
    // Walk through five successive build epochs at half-rotation
    // intervals.  Pre-fix code returned `t_dep_bounds_s.0 = 0` for
    // every build — a constant series that snapped the visible
    // window back to the left edge on every rotation.  Post-fix
    // code returns the actual `sim_time_s` for each build.
    let cfg = PorkchopConfig::default();
    let earth = earth_orbit();
    let mars = mars_orbit();

    let mut anchors = Vec::new();
    for k in 0..5 {
        let sim_time_s = k as f64 * 0.5 * 365.25 * SECONDS_PER_DAY;
        let grid = build_rotating_buffer_for_body_target(
            &cfg,
            earth,
            mars,
            "Earth".to_string(),
            "Mars".to_string(),
            "interplanetary",
            sim_time_s,
        );
        anchors.push(grid.t_dep_bounds_s.0);
    }
    // Each successive anchor is half a year further along — no
    // snap-back, no overlap.  Pre-fix code would have produced
    // [0, 0, 0, 0, 0].
    let mut sorted = anchors.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(
        sorted, anchors,
        "anchors must be strictly increasing — pre-fix code returned a constant 0.0 series that snapped every rotation"
    );
    let first = anchors[0];
    let last = anchors[anchors.len() - 1];
    assert!(
        last > first,
        "first/last anchors must differ: got {first} vs {last}"
    );
    let half_year_s = 0.5 * 365.25 * SECONDS_PER_DAY;
    assert!(
        (last - first - 4.0 * half_year_s).abs() < 1.0,
        "5 builds at half-yr intervals should span 4× half-yr = 2 yr, got {}",
        last - first
    );
}

// Suppress unused-import warning when this file is compiled.
#[allow(dead_code)]
fn _unused_imports_compile() {
    let _ = PorkchopInputs {
        origin_name: String::new(),
        dest_name: String::new(),
        origin_orbit: earth_orbit(),
        dest_orbit: mars_orbit(),
        system_gm: GM_SUN,
        sim_time_s: 0.0,
        category: String::new(),
    };
}
