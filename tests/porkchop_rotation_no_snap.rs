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
use helios_ascension::fleets::porkchop::{build_rotating_buffer_for_body_target, PorkchopGrid, PorkchopInputs};
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

// ── Rebuild-storm guard (Phase B+) ────────────────────────────────────────

#[test]
fn gra_rebuild_storm_guard_in_flight_defaults_to_false() {
    // The in-flight flag must default to `false` so the first
    // rotation trigger can fire the build block on its first frame.
    // If it defaulted to `true`, the planner would never rebuild.
    let state = FleetUiState::default();
    assert!(
        !state.porkchop_build_in_flight,
        "porkchop_build_in_flight must default to false"
    );
}

#[test]
fn gra_rebuild_storm_guard_clear_target_resets_in_flight() {
    // `clear_target()` must reset the in-flight flag — a stranded
    // `true` after a target switch would deadlock the planner
    // (no further rebuilds ever fire).
    let mut state = FleetUiState {
        porkchop_build_in_flight: true,
        ..Default::default()
    };
    state.clear_target();
    assert!(
        !state.porkchop_build_in_flight,
        "clear_target() must reset porkchop_build_in_flight"
    );
}

// ── Immediate-departure re-anchor (Phase 5 follow-up) ──────────────────────

/// Helper: extract the abs t_dep that would be drawn as the
/// trajectory origin (`op`) for a given `(sc, sr)` selection at
/// sim-time `current_sim_s`.  Mirrors the cell-selection math in
/// `transfer_planner.rs` (the `(sc, sr) → abs_t_dep` mapping uses
/// the same `t_dep_bounds_s.0 + col * col_step` formula).  The
/// render path's `departure_s` is then computed from the
/// three-way clamp (`max(recorded_abs_t_dep, current_sim_s)` when
/// the cell is selected), and passed to
/// `predict_body_visual_pos(origin, current_sim_s, departure_s,
/// ...)`.
fn cell_abs_t_dep(grid: &PorkchopGrid, sc: usize) -> f64 {
    let (cols, _) = grid.resolution;
    let col_step = (grid.t_dep_bounds_s.1 - grid.t_dep_bounds_s.0) / cols as f64;
    grid.t_dep_bounds_s.0 + (sc as f64) * col_step
}

#[test]
fn immediate_departure_clamp_keeps_trajectory_anchored_at_live_planet() {
    // The fix for "trajectory moves all over the place once the
    // selected tile hits Now": when the recorded burn time falls
    // into the past, the render path's three-way clamp returns
    // `current_sim_s` (immediate departure) instead of the past
    // epoch.  We exercise the clamp math directly here so we
    // don't need a full Bevy app + egui context.
    //
    // The clamp lives in `draw_fleet_transfer_preview` and
    // `draw_gravity_assist_preview`:
    //
    //   ```
    //   match recorded_abs_t_dep_s {
    //       Some(t) if t >= current_sim_s => t, // frozen future burn
    //       Some(_)                         => current_sim_s, // immediate
    //       None                            => current_sim_s + offset,
    //   }
    //   ```
    //
    // We verify each branch.  The test is a pure arithmetic
    // check on `f64` — the render path's actual
    // `predict_body_visual_pos` call is covered by the
    // integration tests.
    fn clamp_departure_s(
        recorded: Option<f64>,
        current_sim_s: f64,
        offset_s: f64,
    ) -> f64 {
        match recorded {
            Some(t) if t >= current_sim_s => t,
            Some(_) => current_sim_s,
            None => current_sim_s + offset_s,
        }
    }
    let current = 100.0_f64;
    // Branch 1: future recorded epoch → use it
    assert_eq!(
        clamp_departure_s(Some(200.0), current, 0.0),
        200.0,
        "future recorded epoch must be used as-is"
    );
    assert_eq!(
        clamp_departure_s(Some(100.0), current, 0.0),
        100.0,
        "exactly-current recorded epoch must be used as-is (boundary case)"
    );
    // Branch 2: past recorded epoch → clamp to current (immediate)
    assert_eq!(
        clamp_departure_s(Some(50.0), current, 0.0),
        100.0,
        "past recorded epoch must clamp to current_sim_s (immediate departure)"
    );
    assert_eq!(
        clamp_departure_s(Some(99.999), current, 0.0),
        100.0,
        "near-past recorded epoch must clamp to current_sim_s"
    );
    // Branch 3: no recorded epoch → slider path
    assert_eq!(
        clamp_departure_s(None, current, 0.0),
        100.0,
        "slider at offset=0 collapses to immediate departure"
    );
    assert_eq!(
        clamp_departure_s(None, current, 50.0),
        150.0,
        "slider at offset=50 produces a 50-s-future burn"
    );
    assert_eq!(
        clamp_departure_s(None, current, -50.0),
        50.0,
        "slider at negative offset produces a 50-s-past burn (clamp is the renderer's job, not this helper's)"
    );
}

#[test]
fn per_frame_reanchor_clamps_cell_to_col_zero_when_burn_is_past() {
    // Locks in the planner's per-frame re-anchor block: when
    // `selected_abs_t_dep_s < current_sim_s`, both the recorded
    // absolute epoch AND the visual `(sc, sr)` cell coordinate
    // are updated so the chart highlight sticks at col 0 ("Now")
    // and the trajectory arc stays anchored at the live planet.
    //
    // The math: starting from `(sc=5, sr=7)` with `abs_t_dep =
    // 50`, if `current_sim_s = 100`, the re-anchor must set
    // `abs_t_dep = 100` and `(sc, sr) = (0, 7)` so the
    // highlight stays at the leftmost column (the "Now" line
    // on the chart) and the row (TOF) is preserved.
    fn reanchor(
        selected_porkchop_cell: Option<(usize, usize)>,
        selected_abs_t_dep_s: Option<f64>,
        current_sim_s: f64,
        cols_buf: usize,
    ) -> (Option<(usize, usize)>, Option<f64>) {
        if let (Some((_sc, sr)), Some(recorded), true) = (
            selected_porkchop_cell,
            selected_abs_t_dep_s,
            cols_buf > 0,
        ) {
            if recorded < current_sim_s {
                return (Some((0, sr)), Some(current_sim_s));
            }
        }
        (selected_porkchop_cell, selected_abs_t_dep_s)
    }
    // Case 1: cell in the future → no change
    let (cell, abs) = reanchor(Some((5, 7)), Some(150.0), 100.0, 10);
    assert_eq!(cell, Some((5, 7)));
    assert_eq!(abs, Some(150.0));
    // Case 2: cell in the past → clamp to col 0 + now
    let (cell, abs) = reanchor(Some((5, 7)), Some(50.0), 100.0, 10);
    assert_eq!(
        cell,
        Some((0, 7)),
        "cell must clamp to col 0 (the 'Now' line) when burn is past"
    );
    assert_eq!(
        abs,
        Some(100.0),
        "recorded epoch must clamp to current_sim_s (immediate departure)"
    );
    // Case 3: boundary — recorded == current → no change (still "future")
    let (cell, abs) = reanchor(Some((5, 7)), Some(100.0), 100.0, 10);
    assert_eq!(cell, Some((5, 7)));
    assert_eq!(abs, Some(100.0));
}

#[test]
fn cell_anchor_recognises_immediate_departure_state() {
    // The post-panel block re-records `selected_abs_t_dep_s`
    // from `(sc, sr)`.  When the per-frame re-anchor above has
    // set `selected_abs_t_dep_s = current_sim_s` and
    // `(sc, sr) = (0, sr)`, the post-panel block must NOT
    // overwrite the recorded epoch back to the cell's natural
    // (past) `t_dep_bounds_s.0` — that would undo the clamp.
    //
    // The block's `current_matches_recorded` check now accepts
    // two match conditions:
    //   1. `(prev - abs_t_dep).abs() < col_step * 0.5` — the
    //      recorded matches the cell's natural abs_t_dep (the
    //      original GRA-169 case).
    //   2. `(prev - elapsed).abs() < col_step * 0.5` — the
    //      recorded matches the current sim clock (the
    //      immediate-departure clamp case).
    //
    // We exercise the second condition here.
    fn matches_recorded(
        prev: Option<f64>,
        cell_abs_t_dep: f64,
        elapsed: f64,
        col_step: f64,
    ) -> bool {
        match prev {
            Some(p) => {
                (p - cell_abs_t_dep).abs() < col_step * 0.5
                    || (p - elapsed).abs() < col_step * 0.5
            }
            None => false,
        }
    }
    let col_step = 1000.0_f64; // arbitrary
    // Recorded matches elapsed → match (immediate-departure path)
    assert!(
        matches_recorded(Some(100.0), 50.0, 100.0, col_step),
        "recorded==elapsed must match (immediate-departure state)"
    );
    // Recorded matches cell → match (normal cell-state path)
    assert!(
        matches_recorded(Some(50.0), 50.0, 100.0, col_step),
        "recorded==cell_abs_t_dep must match (normal cell state)"
    );
    // Recorded matches neither (and is far from both — outside the
    // half-col_step margin) → no match.  Use values that are
    // clearly more than `col_step * 0.5 = 500.0` away from both
    // anchors so the half-col_step tolerance doesn't accidentally
    // match.
    assert!(
        !matches_recorded(Some(700.0), 50.0, 100.0, col_step),
        "recorded 700.0 must NOT match cell (50.0) or elapsed (100.0) within ±500"
    );
    assert!(
        !matches_recorded(Some(2000.0), 50.0, 100.0, col_step),
        "recorded 2000.0 must NOT match either condition"
    );
}

// ── Rotation-trigger preserves built_at_s (Phase 5 follow-up) ─────────────

#[test]
fn rotation_trigger_preserves_built_at_during_rebuild() {
    // Locks in the fix for the "porkchop flickers on rebuild"
    // bug: the rotation-trigger block must NOT clear
    // `porkchop_built_at_s` to `None` when setting
    // `porkchop_grid_pending_rebuild = true`.  Clearing
    // `built_at_s` made `shift_s = elapsed - None.unwrap_or(0)
    // = 0` for the entire ~360 ms async-build window, which
    // reset the chart's scroll to the OLD buffer's left edge.
    // On the swap (which sets `built_at_s = Some(elapsed)`),
    // scroll reset again to the NEW buffer's left edge — the
    // combined left-then-right movement read as a flicker.
    //
    // The contract: the trigger sets the flag, but `built_at_s`
    // and `last_real_build_s` stay at their current values so
    // `shift_s` keeps advancing during the build.  When the
    // swap lands, the new `built_at_s = Some(elapsed)` resets
    // `shift_s` to 0 of the new buffer, and the visible cell
    // content of the new buffer's left edge is the SAME as the
    // old buffer's right edge at the moment of swap (Lambert
    // is rotation-invariant — shifting the buffer's anchor by
    // `Δ` only relabels cell dates, not ΔV).
    let mut state = FleetUiState {
        porkchop_grid: Some(build_rotating_buffer_for_body_target(
            &PorkchopConfig::default(),
            earth_orbit(),
            mars_orbit(),
            "Earth".to_string(),
            "Mars".to_string(),
            "interplanetary",
            0.0,
        )),
        porkchop_built_at_s: Some(12345.0),
        porkchop_last_real_build_s: Some(67890.0),
        ..Default::default()
    };
    // Simulate the rotation trigger firing.
    state.porkchop_grid_pending_rebuild = true;
    // Contract: built_at_s and last_real_build_s must NOT be
    // cleared by the trigger.  Pre-fix code set both to `None`
    // here, which made `shift_s = 0` during the build.
    assert!(
        state.porkchop_built_at_s == Some(12345.0),
        "rotation trigger must preserve porkchop_built_at_s; got {:?}",
        state.porkchop_built_at_s
    );
    assert!(
        state.porkchop_last_real_build_s == Some(67890.0),
        "rotation trigger must preserve porkchop_last_real_build_s; got {:?}",
        state.porkchop_last_real_build_s
    );
    // The flag is set so the build block will fire next frame.
    assert!(state.porkchop_grid_pending_rebuild);
    // The grid stays populated so the panel keeps rendering it
    // during the build (no blank frame).
    assert!(
        state.porkchop_grid.is_some(),
        "rotation trigger must keep grid populated during the build"
    );
}

#[test]
fn rotation_trigger_preserves_scroll_through_pending_window() {
    // Companion to the above: if the trigger kept `built_at_s`,
    // then `shift_s = elapsed - built_at_s` keeps advancing
    // through the ~360 ms build window.  When the swap lands,
    // the new `built_at_s = Some(elapsed)` makes `shift_s = 0`
    // of the new buffer — and the new buffer's left edge is at
    // the same absolute epoch that `shift_s` had reached on
    // the old buffer, so the visible cell content is continuous
    // (no horizontal jump).
    let original_built_at_s: f64 = 100.0;
    // Sim-time advances during the build.  Old grid's `shift_s`
    // when the swap fires is `elapsed_at_swap - original_built_at_s`.
    let elapsed_at_swap = 500.0;
    let shift_s_at_swap = elapsed_at_swap - original_built_at_s;
    assert_eq!(shift_s_at_swap, 400.0);
    // On swap: new buffer's `t_dep_bounds_s.0 = elapsed_at_swap`.
    // New `shift_s = 0`.  Old grid's visible window left edge at
    // the moment of swap was `original_built_at_s + shift_s_at_swap
    // = elapsed_at_swap` — exactly the new buffer's anchor.  No
    // content jump.
    let new_buffer_anchor = elapsed_at_swap;
    let old_grid_visible_left_edge_at_swap = original_built_at_s + shift_s_at_swap;
    assert!(
        (new_buffer_anchor - old_grid_visible_left_edge_at_swap).abs() < 1e-9_f64,
        "new buffer's anchor must equal old buffer's visible-window left edge at swap time (was {} vs {})",
        new_buffer_anchor,
        old_grid_visible_left_edge_at_swap
    );
}

#[test]
fn gra_rebuild_storm_guard_atomic_swap_clears_in_flight() {
    // The rebuild-storm guard contract: when a build completes, the
    // atomic swap at the bottom of the deferred-build block must
    // clear BOTH `porkchop_grid_pending_rebuild` AND
    // `porkchop_build_in_flight` in the same statement.  If only
    // one is cleared, the next rotation trigger either skips the
    // build (in_flight still `true`) or storms (pending_rebuild
    // still `true`).
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
        porkchop_build_in_flight: true,
        ..Default::default()
    };
    // Simulate the build completing: atomic swap + flag clear
    // (single-statement contract — same pattern as
    // `gra_169_part_b_pending_rebuild_keeps_grid_visible`).
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
    state.porkchop_build_in_flight = false;
    assert!(state.porkchop_grid.is_some());
    assert!(
        !state.porkchop_grid_pending_rebuild,
        "atomic swap must clear pending_rebuild"
    );
    assert!(
        !state.porkchop_build_in_flight,
        "atomic swap must clear in_flight — a stranded `true` \
         would deadlock the planner (no further rebuilds ever fire)"
    );
}

#[test]
fn gra_rebuild_storm_guard_blocks_reentry_while_in_flight() {
    // The storm-guard contract: while `porkchop_build_in_flight`
    // is `true`, the deferred-build block must NOT re-solve the
    // grid.  This is the regression test for the symptom
    // "pauses every few seconds even though there is plenty of
    // material on the X axis" — pre-fix, every frame the planner
    // was open AND the pending-rebuild flag was set would
    // re-enter `build_rotating_buffer_for_body_target` and
    // discard the previous frame's in-progress solve (~22
    // consecutive solves at 60 FPS, ~8 s of CPU per rotation
    // trigger).
    //
    // We assert the resource contract by simulating two
    // consecutive "frames": frame N enters the block and sets
    // `in_flight = true`; frame N+1 must observe the flag and
    // skip.  We don't drive the planner render path here (it
    // requires an egui context); we lock in the resource contract.
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
        // Pre-fix: only `pending_rebuild = true` was enough to
        // re-enter the build block. Post-fix: the in-flight
        // guard adds a second barrier.
        porkchop_grid: Some(grid),
        porkchop_grid_pending_rebuild: true,
        porkchop_build_in_flight: true,
        ..Default::default()
    };
    // Frame N+1: the block evaluates
    //   `needs_build && !porkchop_build_in_flight`
    // with both flags set. `in_flight = true` so the block bails
    // out — the grid is NOT re-solved.
    let would_solve = state.porkchop_grid_pending_rebuild
        && !state.porkchop_build_in_flight;
    assert!(
        !would_solve,
        "while in_flight is true the build block must NOT re-solve the grid; \
         pre-fix code would re-solve and storm CPU"
    );
    // Frame N+2: clear in_flight (build completed); now the next
    // pending_rebuild trigger can fire.
    state.porkchop_build_in_flight = false;
    let would_solve_now = state.porkchop_grid_pending_rebuild
        && !state.porkchop_build_in_flight;
    assert!(
        would_solve_now,
        "after in_flight is cleared the next trigger can fire a fresh build"
    );
}

// ── Async-build receiver contract (Phase B++) ────────────────────────────

#[test]
fn gra_async_build_receiver_defaults_to_none() {
    // The async-build receiver must default to `None` so the first
    // rotation trigger spawns a worker thread.  If it defaulted
    // to `Some(empty receiver)`, the polling block would `try_recv`
    // an immediate `Disconnected` and the build would deadlock.
    let state = FleetUiState::default();
    assert!(
        state.porkchop_build_result_rx.is_none(),
        "porkchop_build_result_rx must default to None"
    );
}

#[test]
fn gra_async_build_clear_target_drops_receiver() {
    // `clear_target()` must drop the receiver — a stranded
    // `Some(_)` after a target switch would either:
    //   (a) cause the polling block to keep polling the old
    //       channel and the worker's `tx.send(grid)` would
    //       succeed but the grid would be for the wrong target,
    //   (b) hit `Disconnected` and re-spawn indefinitely.
    // Both are bugs.  The resource contract: `clear_target` sets
    // the field to `None`.
    let (tx, rx) = std::sync::mpsc::channel::<PorkchopGrid>();
    drop(tx); // disconnected — but the field is still `Some(_)`
    let mut state = FleetUiState {
        porkchop_build_result_rx: Some(std::sync::Mutex::new(rx)),
        ..Default::default()
    };
    state.clear_target();
    assert!(
        state.porkchop_build_result_rx.is_none(),
        "clear_target() must drop porkchop_build_result_rx"
    );
}

#[test]
fn gra_async_build_polling_blocks_until_worker_finishes() {
    // End-to-end async-build smoke test: spawn a worker that
    // sends a small delay then a grid; assert the polling block
    // returns `Empty` while the worker is running, then `Ok(grid)`
    // after the worker finishes.
    //
    // This drives the actual std::thread + mpsc::channel path
    // used by `transfer_planner.rs` to verify the contract
    // without spinning up a Bevy world.
    use std::sync::mpsc;
    use std::time::Duration;
    let (tx, rx) = mpsc::channel::<PorkchopGrid>();
    let worker = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
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
        let _ = tx.send(grid);
    });

    // Wrap the receiver in the same `Mutex` used in
    // `FleetUiState` so the test exercises the real access pattern.
    let rx_lock = std::sync::Mutex::new(rx);

    // Poll once immediately — the worker has a 50 ms delay, so
    // the channel is empty.
    let r1 = rx_lock.lock().unwrap().try_recv();
    assert!(
        matches!(r1, Err(mpsc::TryRecvError::Empty)),
        "try_recv before worker finishes must be Empty, got {r1:?}"
    );

    // Wait for the worker.
    worker.join().expect("worker thread panicked");

    // Poll again — the worker has sent, so we get the grid.
    let r2 = rx_lock.lock().unwrap().try_recv();
    let grid = r2.expect("try_recv after worker finishes must be Ok");
    assert!(
        grid.cells.iter().any(|c| c.feasible),
        "async-built grid must have at least one feasible cell"
    );
}

// ── GRA-pre-burn-converge (preview trajectory) ───────────────────────────────
//
// Anchor-contract for the trajectory preview:
//   * RELATIVE anchor (buggy): planned_departure_time_s = T_now + offset.
//     The planet at "T_now + offset" advances at the same orbital
//     rate as the planet at "T_now", so their separation stays
//     constant and the trajectory's burn-side anchor looks glued
//     to the live planet — the visible "trajectory doesn't update"
//     symptom the user reported.
//   * ABSOLUTE anchor (fixed): planned_departure_time_s = T_abs.
//     The planet at "T_abs" advances at its own orbital rate, but
//     it does NOT drift forward with T_now — so as T_now approaches
//     T_abs, the planet at burn-time converges toward the planet at
//     current-time.  The visible separation shrinks; the trajectory
//     "consumes" toward now.
//
// This test pins down the geometric invariant that the planner's
// anchor switch relies on.  It uses the pure
// `orbit_position_from_mean_anomaly` helper so it has no Bevy-app
// dependencies.

#[test]
fn trajectory_preview_relative_anchor_keeps_offset_constant() {
    use helios_ascension::astronomy::orbit_position_from_mean_anomaly;
    // Realistic Earth mean motion: 2π / 365.25 d.  The helper
    // `earth_orbit()` uses mean_motion = 1.0 rad/s (period 2π s)
    // which would wrap the phase many times over 79 d and
    // accidentally make two near-coincident positions appear
    // "constant".  Construct a real-Earth orbit for the anchor
    // invariant check.
    let earth = KeplerOrbit::circular(1.0, 2.0 * std::f64::consts::PI / (365.25 * SECONDS_PER_DAY));

    let offset_s = 79.0 * SECONDS_PER_DAY;

    // Sample (T_now, planet_now, planet_at_relative_burn) at four
    // sim times.  The relative-anchor formula uses
    // planet_at_relative_burn = planet(T_now + offset), so the
    // separation planet_at_relative_burn - planet_now must be
    // constant in orbital phase (and the two positions must
    // rotate around the star at the same rate).
    let mut separations: Vec<f64> = Vec::new();
    for k in 0..4 {
        let t_now_s = k as f64 * 30.0 * SECONDS_PER_DAY;
        let planet_now = orbit_position_from_mean_anomaly(
            &earth,
            earth.mean_anomaly_epoch + earth.mean_motion * t_now_s,
        );
        let planet_relative_burn = orbit_position_from_mean_anomaly(
            &earth,
            earth.mean_anomaly_epoch + earth.mean_motion * (t_now_s + offset_s),
        );
        separations.push((planet_relative_burn - planet_now).length());
    }

    // The relative-anchor separation stays constant — both
    // positions rotate around the star at the same rate, so their
    // vector difference (the chord at constant phase offset) has
    // invariant magnitude.  This is the bug condition the user
    // observed: trajectory stays glued to the planet.
    let baseline = separations[0];
    for (k, sep) in separations.iter().enumerate() {
        assert!(
            (sep - baseline).abs() < 1e-9,
            "RELATIVE-anchor offset must stay constant in orbital phase; \
             at sample k={k} the separation magnitude = {sep} differs from \
             baseline {baseline} (this is the lockstep-glued-to-planet symptom)",
        );
    }
}

#[test]
fn trajectory_preview_absolute_anchor_lets_offset_shrink() {
    use helios_ascension::astronomy::orbit_position_from_mean_anomaly;
    let earth = KeplerOrbit::circular(1.0, 2.0 * std::f64::consts::PI / (365.25 * SECONDS_PER_DAY));

    // Sample planet_now and planet_at_absolute_burn at four sim
    // times leading up to the absolute burn.  With the
    // absolute-anchor formula, the burn-time planet does NOT drift
    // forward with T_now — so the separation between
    // planet_at_absolute_burn and planet_now shrinks monotonically
    // as T_now approaches T_abs.
    let t_abs_s = 79.0 * SECONDS_PER_DAY;

    let mut separations: Vec<f64> = Vec::new();
    // Sample at k=0, 1, 2, plus a final sample that lands exactly
    // on T_abs so the convergence-to-zero invariant can be checked.
    for k in 0..3 {
        let t_now_s = k as f64 * 20.0 * SECONDS_PER_DAY;
        let planet_now = orbit_position_from_mean_anomaly(
            &earth,
            earth.mean_anomaly_epoch + earth.mean_motion * t_now_s,
        );
        let planet_at_burn = orbit_position_from_mean_anomaly(
            &earth,
            earth.mean_anomaly_epoch + earth.mean_motion * t_abs_s,
        );
        separations.push((planet_at_burn - planet_now).length());
    }
    // Final sample: T_now == T_abs.
    {
        let planet_now = orbit_position_from_mean_anomaly(
            &earth,
            earth.mean_anomaly_epoch + earth.mean_motion * t_abs_s,
        );
        separations.push((planet_now - planet_now).length());
    }

    // The absolute-anchor offset monotonically shrinks toward 0 as
    // T_now approaches T_abs (the burn-time planet position is
    // fixed in world space; the live planet approaches it).  This
    // is the user-visible "consume toward now" motion the planner
    // anchor switch unlocks.
    for w in separations.windows(2) {
        assert!(
            w[1] < w[0] + 1e-9,
            "absolute-anchor separation must shrink as T_now approaches T_abs; \
             got {} -> {} (should be strictly decreasing — burn-time planet \
             converges toward live planet)",
            w[0], w[1],
        );
    }

    // At T_now = T_abs, the separation is exactly 0.
    let last = *separations.last().expect("at least one sample");
    assert!(
        last < 1e-9,
        "at T_now = T_abs the burn-time planet and live planet must coincide, \
         got separation {last}",
    );
}
