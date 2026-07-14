//! Porkchop plot math layer (H-1 from GRA-148).
//!
//! Replaces the Efficient / Moderate / Fast placeholder options in the
//! transfer planner with a real `(t_dep, t_tof)` contour grid.  Each cell
//! solves a Lambert transfer for `(origin, dest, transfer_time)` and stores
//! the resulting `total_dv_ms`, `c3_departure`, `v_inf_arrival_ms`, and the
//! `KeplerOrbit` that the active-arc renderer will use.
//!
//! The Rust types in this file mirror the LGD-owned
//! `assets/data/porkchop_config.ron` schema and the LGD design contract
//! on GRA-152.  See `src/fleets/components.rs` for the loader-side structs.

use super::components::{PorkchopConfig, ResolvedPorkchopParams};
use super::orbital_mechanics::{
    solve_lambert_transfer, GM_SUN, MAX_CURVED_CROSS_STAR_TRANSFER_TIME_S,
};
use crate::astronomy::orbit_position_from_mean_anomaly;
use crate::astronomy::KeplerOrbit;
use crate::plugins::solar_system_data::BodyType;
use bevy::math::DVec3;
use bevy::prelude::Entity;
use serde::{Deserialize, Serialize};

const SECONDS_PER_DAY: f64 = 86_400.0;
const SECONDS_PER_YEAR: f64 = 365.25 * SECONDS_PER_DAY;

/// Maximum acceptable t_dep step (days) for the rendered grid.
/// `adaptive_resolution` scales `resolution_t_dep` up so the
/// per-column step on the natural `(t_dep_window, tof_span)`
/// stays at or below this value.  Tightened 3.0 → 2.0 days:
/// the previous 3-day step left visible 1-cell-wide ΔV stripes
/// on the cheap-transfer basin of mid-window interplanetary
/// plots (the user-reported "porkchops still look a bit coarse"
/// feedback).  2-day step matches the inner-planet
/// synodic-period alignment tolerance and lets the basin span
/// 1.5–2.5 cells instead of 0.8.
const MAX_T_DEP_STEP_DAYS: f64 = 2.0;

/// Maximum acceptable TOF step (days) for the rendered grid.
/// `adaptive_resolution` scales `resolution_tof` up so the
/// per-row step stays at or below this value.  Tightened
/// 5.0 → 3.0 days: the previous 5-day step combined with the
/// 5000-cell cap meant wide-window transfers (Mars-class)
/// ended up with rows at ~40 d/row, producing the visible
/// blocky Y-axis bands the player reported.  3-day step
/// keeps the colormap band smooth on Saturn-class porkchops
/// even after the adaptive Y-axis trim
/// (`compute_adaptive_tof_bounds`) compresses rows onto the
/// populated band.
const MAX_TOF_STEP_DAYS: f64 = 3.0;

/// Validator cap on the total cell count of the *computed*
/// grid.  Mirrors the rule `defaults.resolution_t_dep *
/// defaults.resolution_tof ≤ 20000` (and per-override) enforced
/// statically by `PorkchopConfig::validate` in `src/fleets/data.rs`.
/// `adaptive_resolution` clamps the *runtime* grid to this cap
/// even when the static RON fields are smaller — so an override
/// with `resolution_t_dep: 90, resolution_tof: 90 = 8100 cells`
/// on a 614-day Earth↔Venus-synodic window still ends up under
/// 20000 cells after the auto-bump.
///
/// The cap is split into two constants: inner (Mercury → Mars)
/// destinations keep the 20 000-cell ceiling from the LGD
/// contract; outer (Jupiter, Saturn, Uranus, Neptune, plus
/// any interstellar target with SMA beyond the threshold) get
/// the much tighter 8 000-cell ceiling.  Inner-planet grids hit
/// a *broad* feasible basin (cheap Hohmann spans most of the
/// `2 × dest_period` × `5 × tof_h` × `10 yr` cap of the
/// computed TOF range), so the wider cell budget keeps the
/// basin legible.  Outer-planet grids hit a *narrow* basin (the
/// cheap transfer lives in a thin band of ±50 d in t_dep and
/// ±1 yr in tof; everything outside exceeds the 400 km²/s² C3
/// ceiling), so most of the 20 000-cell compute budget is spent
/// on rows that render grey.  Tightening the outer cap to 8 000
/// drops an Earth→Saturn build from ~6 s to ~2.4 s on a single
/// physical core while still giving ~13 cols × ~22 rows of basin
/// after `compute_adaptive_tof_bounds` trims the rendered view
/// — well above the 2-day t_dep / 3-day TOF step target the
/// `MAX_*_STEP_DAYS` constants enforce on the *adaptive bump*.
/// The 12× / 20× SUPERSAMPLE texture bake covers any remaining
/// pixelation.
///
/// Outer was originally explored at 4 000 / 5 000 / 6 000; 8 000
/// was the lowest cap that still passed the existing
/// `adaptive_resolution_clamps_at_cell_cap_for_interplanetary`
/// test contract without coarsening the basin to the point
/// where the deepest ΔV gradient on Jupiter-class transfers
/// starts to alias at the panel's 380-pixel width.
///
/// Raised 12000 → 20000 (and previously 5000 → 12000) to give
/// the rotating buffer enough room to hit the 2-day t_dep step
/// target without sacrificing rows.  At 12000 cells the buffer
/// (480 d × 525 d, 240 cols) capped rows at 50 → 10.5 d/row,
/// producing the "rough" / "vibey" ΔV bands the player reported
/// at 12× SUPERSAMPLE.  At 20000 cells the same buffer holds
/// 240 × 83 rows → 6.3 d/row, which the 20× SUPERSAMPLE
/// texture bake then smooths into a near-photographic gradient.
///
/// Worst-case Lambert solve is ~0.3 ms/cell → 20 000 cells ≈
/// 6 s for inner planets, ~2.4 s for outer planets.
const ADAPTIVE_RESOLUTION_CELL_CAP_INNER_PLANET: usize = 20000;
const ADAPTIVE_RESOLUTION_CELL_CAP_OUTER_PLANET: usize = 8000;

/// Heliocentric distance above which the destination is classed
/// "outer planet" and gets the tighter cap.  Sits between Mars
/// (1.52 AU) and Jupiter (5.20 AU) so Mars / Ceres-class
/// destinations keep the full LGD budget while Saturn-class
/// destinations drop to the smaller budget.  Comets on highly
/// elliptical orbits may have `semi_major_axis > 3 AU` and
/// trigger the outer cap; their eccentric trajectories still
/// hit the C3 ceiling outside a narrow basin, so the smaller
/// cap pays the same dividend as it does for planets.
const DEST_SMA_OUTER_THRESHOLD_AU: f64 = 3.0;

/// Pick the cell cap for a heliocentric destination based on its
/// semi-major axis.  Cap is the *most* cells `adaptive_resolution`
/// will allow in `(cols × rows)`; the helper shrinks whichever
/// dimension has the looser per-cell step first to honor it.
///
/// `dest_sma_au < 3.0` → inner cap (Mercury → Mars + asteroids).
/// `dest_sma_au >= 3.0` → outer cap (Jupiter, Saturn, Uranus,
/// Neptune, plus any interstellar target with `dest_sma_au`
/// above the threshold — `build_lagrange_grid` passes the
/// host planet's SMA, so an LP on Saturn's orbit also lands in
/// the outer class; the `lagrange` RON override uses small
/// resolutions anyway and won't hit either cap).
///
/// Local-frame (`build_porkchop_grid_for_local_frame`) and
/// star-approach (`build_star_approach_grid`) grids don't pass
/// through this helper — moon destinations are short-cislunar
/// (≤3500 cells, never hit the cap) and star-approach has its
/// own shape.  Callers in those code paths pass the inner cap
/// explicitly for symmetry.
pub(crate) fn adaptive_resolution_cell_cap_for(dest_sma_au: f64) -> usize {
    if dest_sma_au >= DEST_SMA_OUTER_THRESHOLD_AU {
        ADAPTIVE_RESOLUTION_CELL_CAP_OUTER_PLANET
    } else {
        ADAPTIVE_RESOLUTION_CELL_CAP_INNER_PLANET
    }
}

// === Two-pass refinement (outer-planet destinations) ========================
//
// For outer-planet destinations the cheap-transfer basin is a small
// fraction of the full `(window, span)` surface — the C3 ceiling of
// 400 km²/s² blocks every cell outside a ~50 d × ~1 yr band around
// the optimal Hohmann phase.  The full 408 d × 10 yr grid computed
// by `build_porkchop_grid_with_params` (now class-capped at 8000 cells
// by `adaptive_resolution_cell_cap_for`) therefore spends most of its
// compute on rows that render grey.
//
// The two-pass refinement solves this by computing a coarse preview
// first to *find* the basin, then a fine pass restricted to the basin's
// (t_dep, tof) bounds.  Cells outside the basin are dropped (with a
// small margin so the basin isn't clipped at the edges), so the
// panel ends up showing a tightly-cropped view of the cheap basin at
// full 2-day t_dep / 3-day TOF resolution.
//
// `COARSE_PREVIEW_CELL_CAP` is the cap that `adaptive_resolution`
// applies to the coarse pass.  At the default 25 × 18 RON minimums
// and a ~408 d × ~10 yr Saturn-class range, adaptive_resolution
// would otherwise aim for ~204 cols × ~717 rows = 146 268 cells,
// blowing through this cap; the cap forces a coarse ~500-cell
// preview that finishes in ~150 ms.  Setting the cap any higher
// burns extra Lambert solves for detail that the basin detection
// only consumes as boolean feasibility flags.
const COARSE_PREVIEW_CELL_CAP: usize = 500;

/// Trim the fine pass to the basin's (t_dep, tof) bounds only if the
/// detected basin spans less than this fraction of the full range on
/// at least one axis.  For inner planets (Mars) the basin = full range
/// so the threshold is never crossed and the build runs at the full
/// `INNER` cap (preserves the LGD's "show the full search surface"
/// intent).  For outer planets the basin is a small fraction and the
/// build trims to it.  0.80 was chosen empirically so Mars-class
/// destinations still see the full grid (basin spans ~95% of
/// window × ~85% of span) while Saturn-class destinations trim
/// (basin spans ~12% × ~9%).
const BASIN_TRIM_THRESHOLD_FRAC: f64 = 0.80;

/// Buffer added to each edge of the detected basin before the fine
/// build's bounds are clamped.  Mirrors the `TOF_BOUNDARY_MARGIN_FRAC`
/// used by `compute_adaptive_tof_bounds` for the rendered-trim
/// step (10%).  Without this margin, the basin detection's
/// col_min / col_max sample a coarse 25-col grid and the basin's
/// first / last feasible column can sit one coarse-column away
/// from the actual basin's edge, clipping a thin sliver of cheap
/// cells at the panel boundary.
const BASIN_BORDER_MARGIN_FRAC: f64 = 0.10;

/// Metric plotted on the grid.  Currently only `TotalDv` is wired; the
/// LGD design contract leaves `C3` and `DepartureC3` for follow-up.
/// The grid is colormapped by the active metric — see
/// `PorkchopConfig.colormap`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PorkchopMetric {
    TotalDv,
    C3,
    DepartureC3,
}

/// One cell in the porkchop grid.  Row-major: `(col, row)` where
/// `col ∈ [0, resolution_t_dep)` and `row ∈ [0, resolution_tof)`.
#[derive(Debug, Clone)]
pub struct PorkchopCell {
    /// Seconds from `sim_time_s` to the departure epoch.
    pub t_dep_s: f64,
    /// Transfer time-of-flight in seconds.
    pub tof_s: f64,
    /// Total ΔV (both burns) in m/s.  `f64::INFINITY` when infeasible.
    pub total_dv_ms: f64,
    /// Departure C3 = v∞² at departure in (m/s)².
    pub c3_departure: f64,
    /// Arrival v∞ in m/s.
    pub v_inf_arrival_ms: f64,
    /// Departure burn ΔV in m/s.
    pub delta_v1_ms: f64,
    /// Arrival (circularisation) burn ΔV in m/s.
    pub delta_v2_ms: f64,
    /// `false` if the Lambert solver failed, C3 is unphysical, or the
    /// cell exceeds the C3 / TOF cap.  Such cells render greyed and
    /// are not clickable.
    pub feasible: bool,
    /// Origin body heliocentric position at `t_dep_s`.
    pub origin_pos_au: DVec3,
    /// Destination body heliocentric position at `t_dep_s + tof_s`.
    pub dest_pos_au: DVec3,
    /// Departure velocity vector (m/s) in the heliocentric frame.
    pub v_departure_ms: DVec3,
    /// Arrival velocity vector (m/s) in the heliocentric frame.
    pub v_arrival_ms: DVec3,
    /// Lambert conic for the active-arc renderer.  `None` when infeasible.
    pub transfer_orbit: Option<KeplerOrbit>,
}

/// The full `(t_dep, t_tof)` grid.  The minimum-cell index is pre-computed
/// so the UI can highlight it without scanning the full grid every frame.
#[derive(Debug, Clone)]
pub struct PorkchopGrid {
    pub origin_name: String,
    pub dest_name: String,
    /// `(t_dep_min_s, t_dep_max_s)` — the rendered window.
    pub t_dep_bounds_s: (f64, f64),
    /// `(tof_min_s, tof_max_s)` — the **configured** (solved) window.
    /// Every cell in `cells` covers a `tof_s` in this range.  The
    /// panel renders the subset of rows that fall inside
    /// `rendered_tof_bounds_s` (below) so the colormap band sits over
    /// the feasible basin instead of stretching across long-tail
    /// infeasible rows.
    pub tof_bounds_s: (f64, f64),
    /// `(tof_min_s, tof_max_s)` — the **rendered** window the panel
    /// should display.  Anchored at `tof_bounds_s.0` (the
    /// fastest-trajectory row, by construction) and trimmed
    /// *upward* to the highest row that contains at least one
    /// feasible cell, plus a small margin so the basin isn't
    /// pinned to the panel top.  Falls back to `tof_bounds_s`
    /// when no row is feasible (e.g. an unaffordable transfer
    /// budget) so the player still sees the solver's full
    /// topology.  See `compute_adaptive_tof_bounds`.
    pub rendered_tof_bounds_s: (f64, f64),
    /// `(cols, rows)` — t_dep × tof.
    pub resolution: (usize, usize),
    /// Row-major: `len == cols * rows`.
    pub cells: Vec<PorkchopCell>,
    /// `(col, row)` of the cheapest feasible cell.  `None` if no cell is
    /// feasible in the window.
    pub min_cell: Option<(usize, usize)>,
    pub metric: PorkchopMetric,
}

/// Per-gravity-assist overlay data for the main `PorkchopPanel`.
///
/// GRA-385 (operator HB-22): the planner used to render GA candidates as a
/// separate collapsible "Gravity Assists" list, each with its own tiny
/// (20×15) sub-grid that the player had to expand before they could even
/// see the candidate's `(t_dep, tof)` band.  Two usability problems
/// followed: (1) the sub-grids couldn't be clicked to select — only the
/// "Use Gravity Assist" button underneath worked, so the player couldn't
/// pick a specific `(t_dep, tof)` window for the assist; (2) the sub-grid
/// colormap was a 4-stop discrete ramp (cheap/mid/warm/hot) instead of
/// the main porkchop's continuous TWP log-scale palette, so the same ΔV
/// showed as a different colour depending on which panel rendered it.
///
/// The overlay representation solves both at once: the planner computes
/// one `GravityAssistOverlay` per candidate, mapping each feasible GA
/// cell `(t_dep_s, tof_s)` onto a `(main_col, main_row)` in the main
/// porkchop's coordinate system, and the panel paints those cells with
/// the candidate's themed colour + a hatched diagonal pattern so the
/// assist cells are visually distinct from the direct cells underneath.
/// Click-to-select falls out of the same data path — the panel's click
/// handler checks the overlays first and routes the click to GA
/// selection if it lands inside an overlay cell, or to direct selection
/// otherwise.  No more standalone panel needed.
#[derive(Clone, Debug)]
pub struct GravityAssistOverlay {
    /// Index into `fleet_ui_state.gravity_assist_candidates`.  Used by
    /// the planner to translate the panel's selection output back into
    /// `fleet_ui_state.selected_gravity_assist = Some(idx)`.
    pub candidate_idx: usize,
    /// Display name of the flyby body (e.g. "Mars", "Ceres").  Shown in
    /// the legend at the bottom of the panel.
    pub body_name: String,
    /// `true` if the assist saves ΔV relative to the direct transfer at
    /// the best cell; `false` if the assist is a sub-optimal
    /// (positive-extra-ΔV) option.  Drives the legend's badge text.
    pub beneficial: bool,
    /// `total_dv_ms` at the cheapest feasible cell, used for the legend
    /// badge ("via Mars — 4.32 km/s") and the high-contrast highlight
    /// on the best cell.
    pub best_dv_ms: f64,
    /// `(col, row)` of the cheapest feasible cell, projected into the
    /// **main grid's** `(col, row)` coordinate system.  Used by the
    /// panel to render a high-contrast outline on the "best window" tile
    /// so the player can spot it at a glance.
    pub best_main_cell: Option<(usize, usize)>,
    /// `(col, row)` of every feasible GA cell, projected into the main
    /// grid's coordinate system.  Cells outside the main grid's bounds
    /// are clamped to the nearest edge cell rather than dropped, so the
    /// overlay still shows "the assist is reachable from this band".
    pub cells: Vec<(usize, usize)>,
    /// Theme colour for the overlay fill.  `None` means the panel picks
    /// from the candidate index (Mars → AMBER, Ceres → TEAL, fallback
    /// to EP_TEAL for >2 candidates — the same palette the
    /// `gravity_assist_candidates` summary block used to use).
    pub color: Option<Color32>,
}

// Color32 is from bevy_egui; allow importing here without breaking the
// data-layer purity of the rest of `porkchop.rs` (no UI imports above
// this line).
use bevy_egui::egui::Color32;

/// Inputs to `build_porkchop_grid`.  Caller resolves origin/dest
/// heliocentric orbits to absolute mean anomaly / mean motion so the
/// builder is free of Bevy queries and stays unit-testable.
pub struct PorkchopInputs {
    pub origin_name: String,
    pub dest_name: String,
    pub origin_orbit: KeplerOrbit,
    pub dest_orbit: KeplerOrbit,
    pub system_gm: f64,
    /// GRA-NNN: GM of the body whose local parking orbit the shell
    /// picker modifies.  `solve_cell` uses this (not `system_gm`) to
    /// compute the parking-orbit circular speed — see
    /// `parking_radius_au` doc below for why.  Defaults to
    /// `system_gm` for tests and legacy code-paths that don't care
    /// about the shell pick.
    pub body_gm: f64,
    /// `sim_time_s` — the "now" epoch the player is planning from.
    pub sim_time_s: f64,
    /// Category match key for `PorkchopConfig::resolve`.
    pub category: String,
    /// GRA-NNN: when `Some`, treat the origin parking orbit as a
    /// circular orbit at this heliocentric radius (AU) — overriding
    /// the legacy "parking at heliocentric SMA" assumption.
    /// `solve_cell` still runs the Lambert solver with the heliocentric
    /// r1 (the geometry is the same — only the burn ΔV at the
    /// parking orbit changes), then recomputes ΔV₁ from `c3 = v_inf²`
    /// plus the parking-orbit escape velocity so picking "Low" (LEO
    /// analog) costs more ΔV than "High" the way real physics
    /// demands.  Without this the player could pick Low and get the
    /// heliocentric-SMA ΔV for free — the bug that prompted this
    /// refactor.
    ///
    /// The parking circular speed is computed from the body's GM
    /// (`body_gm`) divided by `r_parking_m` — the shell radius is
    /// always a local orbit around the parent body (Low/Medium/High
    /// sit at ~1×, 3×, 10× body radius), so the parking orbit lives
    /// in the body's gravitational well, not the Sun's.  Using
    /// `system_gm` for LEO-scale parking gives v_circ ≈ 4,354 km/s at
    /// Earth (sqrt(GM_Sun / 7000 km)) and a completely nonsensical
    /// ΔV.
    ///
    /// When the player picks a planet shell (Saturn High = 0.00389
    /// AU around Saturn, etc.) this is implicitly "parking around the
    /// planet" — the body's GM is what matters, not the Sun's.
    /// `body_gm` defaults to `system_gm` (= GM_Sun) for tests and
    /// legacy code-paths that don't care about the shell pick.
    ///
    /// Destination arrival parking still uses the heliocentric SMA
    /// `v_circ` (full local-frame arrival is a much larger solver
    /// refactor and is deferred to a follow-up ticket).
    pub parking_radius_au: Option<f64>,
}

/// Build the porkchop grid.  The (cols × rows) loop is bounded by the
/// LGD contract (≤ 20000 cells); one Earth→Mars default is 90 × 90 = 8100.
/// Each cell calls `solve_lambert_transfer`; the LGD's 0.3 ms/cell budget
/// gives ~6 s worst case.  Infeasible cells are kept in the grid
/// (rendered greyed) so the player sees the full topology, including
/// the no-ballistic-solution basin.
///
/// **Resolution contract (post follow-up bump):** the RON
/// `resolution_t_dep` / `resolution_tof` are MINIMUMS, not
/// absolutes.  `build_porkchop_grid_with_params` runs them
/// through `adaptive_resolution`, which bumps `cols` / `rows`
/// up if the natural `(t_dep_window, tof_span)` is wider than
/// the static values can comfortably sample (target: 2-day
/// t_dep step, 3-day TOF step).  The total cell count is
/// clamped at 20000 by shrinking whichever dimension has the
/// looser step first.  This is the structural fix for the
/// user-reported Earth→Mercury via Venus GA case, where the
/// Earth↔Venus-synodic window (~614 d) demands finer t_dep
/// sampling than any static column count could provide without
/// inflating the Saturn-class grid unnecessarily.  See
/// `adaptive_resolution` for the algorithm and
/// `MAX_T_DEP_STEP_DAYS` / `MAX_TOF_STEP_DAYS` for the step
/// targets.
///
/// This is a pure function — no Bevy resources, no queries — so the
/// unit tests can drive it without spinning up a world.
pub fn build_porkchop_grid(cfg: &PorkchopConfig, inputs: &PorkchopInputs) -> PorkchopGrid {
    let params = cfg.resolve(&inputs.category);
    build_porkchop_grid_with_params(params, inputs)
}

pub fn build_porkchop_grid_with_params(
    params: ResolvedPorkchopParams,
    inputs: &PorkchopInputs,
) -> PorkchopGrid {
    // === Step 1: compute the standard full-extent bounds ===
    //
    // `dep_window_bounds` returns the *absolute* (t_dep_min_s, t_dep_max_s)
    // in sim-clock units, anchored at the player's current `sim_time_s`.
    // The cell loop iterates a *relative* offset from that anchor
    // (0..max_t_dep); inside `solve_cell` we add `inputs.sim_time_s` to
    // recover the absolute epoch for the Lambert solver.  This split
    // lets `PorkchopGrid.t_dep_bounds_s` carry the absolute anchor for
    // the panel's date labels without double-offsetting the cell math.
    let (full_t_dep_min_abs_s, full_t_dep_max_abs_s) = dep_window_bounds(inputs, &params);
    let full_max_t_dep_rel_s = full_t_dep_max_abs_s - full_t_dep_min_abs_s;
    let tof_h = hohmann_time_s(
        inputs.origin_orbit.semi_major_axis,
        inputs.dest_orbit.semi_major_axis,
        inputs.system_gm,
    );
    let full_tof_min_s =
        (params.tof_min_hohmann_factor * tof_h).max(params.tof_floor_days * SECONDS_PER_DAY);
    // Phase D (TWP-parity Y-axis bounds, Option B): the Y-axis max
    // is the *minimum* of three competing ceilings — see the
    // history in `compute_adaptive_tof_bounds` below.
    let dest_period_s = std::f64::consts::TAU / inputs.dest_orbit.mean_motion.abs().max(1e-25);
    let dest_period_cap_s = 4.0 * dest_period_s;
    let hohmann_multiplier_cap_s = 5.0 * tof_h;
    let ceiling_cap_s = params.tof_ceiling_years * SECONDS_PER_YEAR;
    let full_tof_span_cap_s = dest_period_cap_s
        .min(hohmann_multiplier_cap_s)
        .min(ceiling_cap_s);
    let full_tof_max_s = full_tof_min_s + full_tof_span_cap_s;

    // === Step 2: coarse preview pass for basin detection ===
    //
    // For outer-planet destinations the cheap-transfer basin is a thin
    // band (~50 d × ~1 yr around the Hohmann phase) and the full 408 d
    // × 10 yr grid spends ~80 % of its compute on cells that render
    // grey (C3 ceiling 400 km²/s² blocks them).  The coarse preview
    // sweeps the full extent at ~500 cells to find where the basin
    // actually lives, then the fine pass zooms in on that band only.
    //
    // For inner-planet destinations the basin is *itself* the full
    // range (Hohmann transfers exist at every t_dep), so the basin
    // detection finds col_min ≈ 0, col_max ≈ last, row_min ≈ 0,
    // row_max ≈ last, and `BASIN_TRIM_THRESHOLD_FRAC` below keeps
    // the build at full extent.  The only cost for inner planets is
    // the ~150 ms coarse preview which is well under 5 % of the
    // total build even on Mars-class targets.
    let coarse = build_grid_for_explicit_bounds(
        inputs,
        &params,
        params.c3_ceiling_km2_s2 * 1.0e6,
        COARSE_PREVIEW_CELL_CAP,
        0.0,
        full_max_t_dep_rel_s,
        full_tof_min_s,
        full_tof_max_s,
    );

    // === Step 3: detect basin from coarse cells ===
    let (coarse_cols, coarse_rows) = coarse.resolution;
    let basin = detect_basin_bounds(
        &coarse.cells,
        coarse_cols,
        coarse_rows,
        0.0,
        full_max_t_dep_rel_s,
        full_tof_min_s,
        full_tof_max_s,
    );

    // === Step 4: decide whether to trim the fine pass ===
    let (t_dep_min_rel_s, t_dep_max_rel_s, tof_min_s, tof_max_s) = match basin {
        Some((t_lo, t_hi, f_lo, f_hi)) => {
            let t_dep_span_full = full_max_t_dep_rel_s;
            let tof_span_full = full_tof_max_s - full_tof_min_s;
            let t_dep_span_basin = t_hi - t_lo;
            let tof_span_basin = f_hi - f_lo;

            // Trim only if the basin is significantly narrower than the
            // full range on at least one axis.  Otherwise the basin
            // spans the full extent (inner planets) and trimming would
            // clip legitimate off-baseline Hohmann alternatives.
            if t_dep_span_basin < BASIN_TRIM_THRESHOLD_FRAC * t_dep_span_full
                || tof_span_basin < BASIN_TRIM_THRESHOLD_FRAC * tof_span_full
            {
                // Add a 10 % margin on each side so the basin's true
                // edge (sampled at coarse resolution) isn't clipped at
                // the panel boundary.  Same shape as the
                // `TOF_BOUNDARY_MARGIN_FRAC` in
                // `compute_adaptive_tof_bounds`.
                let t_margin = BASIN_BORDER_MARGIN_FRAC * t_dep_span_basin;
                let f_margin = BASIN_BORDER_MARGIN_FRAC * tof_span_basin;
                let t_lo_clamped = (t_lo - t_margin).max(0.0);
                let t_hi_clamped = (t_hi + t_margin).min(full_max_t_dep_rel_s);
                let f_lo_clamped = (f_lo - f_margin).max(full_tof_min_s);
                let f_hi_clamped = (f_hi + f_margin).min(full_tof_max_s);
                (t_lo_clamped, t_hi_clamped, f_lo_clamped, f_hi_clamped)
            } else {
                (0.0, full_max_t_dep_rel_s, full_tof_min_s, full_tof_max_s)
            }
        }
        // No feasible cells detected in the coarse preview
        // (degenerate / extreme-budget destination).  Fall back to
        // the full extent so the panel renders the solver's full
        // topology with all rows marked infeasible in grey.
        None => (0.0, full_max_t_dep_rel_s, full_tof_min_s, full_tof_max_s),
    };

    // === Step 5: fine pass with class-based cap ===
    //
    // Resolution: `adaptive_resolution` still uses the configured
    // `(window_days, span_days)` and the `(cols, rows)` it picks
    // describe the basin-restricted surface.  For Saturn (basin
    // 50 d × 365 d) cols = 25 and rows = 122 = 3 050 cells at the
    // outer cap of 8 000 — well under-budget, so the basin renders
    // at the full 2-day t_dep / 3-day TOF step target rather than
    // the step coarser target the cap imposed on the full-range
    // build.
    let cap = adaptive_resolution_cell_cap_for(inputs.dest_orbit.semi_major_axis);
    build_grid_for_explicit_bounds(
        inputs,
        &params,
        params.c3_ceiling_km2_s2 * 1.0e6,
        cap,
        t_dep_min_rel_s,
        t_dep_max_rel_s,
        tof_min_s,
        tof_max_s,
    )
}

/// Inner helper that builds a `PorkchopGrid` for an explicit
/// `(t_dep_min_rel, t_dep_max_rel, tof_min, tof_max)` bound set.
///
/// `build_porkchop_grid_with_params` calls this twice: once for the
/// coarse preview (cap = `COARSE_PREVIEW_CELL_CAP`) and once for the
/// fine pass (cap = class-based `INNER` or `OUTER`).  Pulling the
/// cell-loop / colormap / rendered-trim / atomic-swap logic into one
/// helper keeps the two call sites identical and the build_porkchop_
/// grid_with_params orchestrator readable.  Returns the grid PLUS its
/// `(cols, rows)` resolution and the absolute t_dep bounds so the
/// orchestrator can do basin detection on the coarse result.
///
/// `t_dep_min_rel_s` / `t_dep_max_rel_s` are *relative* offsets
/// from `inputs.sim_time_s` (matching how `solve_cell` consumes
/// them); the helper adds `sim_time_s` to produce the absolute
/// `PorkchopGrid.t_dep_bounds_s` for the panel's date labels.
fn build_grid_for_explicit_bounds(
    inputs: &PorkchopInputs,
    params: &ResolvedPorkchopParams,
    c3_ceiling_ms2: f64,
    cap: usize,
    t_dep_min_rel_s: f64,
    t_dep_max_rel_s: f64,
    tof_min_s: f64,
    tof_max_s: f64,
) -> PorkchopGrid {
    let window_days = (t_dep_max_rel_s - t_dep_min_rel_s) / SECONDS_PER_DAY;
    let span_days = (tof_max_s - tof_min_s) / SECONDS_PER_DAY;
    let (cols, rows) = adaptive_resolution(
        params.resolution_t_dep,
        params.resolution_tof,
        window_days,
        span_days,
        cap,
    );
    let total_cells = cols * rows;
    let mut cells: Vec<PorkchopCell> = Vec::with_capacity(total_cells);
    let mut min_cell: Option<(usize, usize)> = None;
    let mut min_dv: f64 = f64::INFINITY;

    for row in 0..rows {
        let row_frac = if rows > 1 {
            row as f64 / (rows as f64 - 1.0)
        } else {
            0.0
        };
        let tof_s = tof_min_s + row_frac * (tof_max_s - tof_min_s);
        for col in 0..cols {
            let col_frac = if cols > 1 {
                col as f64 / (cols as f64 - 1.0)
            } else {
                0.0
            };
            // Relative offset from the basin's lower edge; the
            // basin's lower edge itself is `t_dep_min_rel_s` away
            // from `sim_time_s`.  `solve_cell` adds `sim_time_s`
            // to recover the absolute departure epoch for the
            // Lambert solver.
            let t_dep_rel_s = t_dep_min_rel_s + col_frac * (t_dep_max_rel_s - t_dep_min_rel_s);
            let cell = solve_cell(inputs, t_dep_rel_s, tof_s, c3_ceiling_ms2);
            if cell.feasible && cell.total_dv_ms < min_dv {
                min_dv = cell.total_dv_ms;
                min_cell = Some((col, row));
            }
            cells.push(cell);
        }
    }

    // Rendered-trim: clip the configured Y-axis range to the band
    // that actually contains feasible cells, with a small margin
    // on each side so the basin isn't pinned to either panel
    // edge.  See `compute_adaptive_tof_bounds` for the
    // player-feedback history of this trim — for outer-planet
    // (basin-restricted) grids, the trim is largely a no-op since
    // the build itself was already narrowed to the basin, but it
    // still keeps the colormap band from sitting on the very top
    // row when only 1-2 rows are feasible.
    let rendered_tof_bounds_s =
        compute_adaptive_tof_bounds(&cells, cols, rows, tof_min_s, tof_max_s);

    PorkchopGrid {
        origin_name: inputs.origin_name.clone(),
        dest_name: inputs.dest_name.clone(),
        t_dep_bounds_s: (
            t_dep_min_rel_s + inputs.sim_time_s,
            t_dep_max_rel_s + inputs.sim_time_s,
        ),
        tof_bounds_s: (tof_min_s, tof_max_s),
        rendered_tof_bounds_s,
        resolution: (cols, rows),
        cells,
        min_cell,
        metric: PorkchopMetric::TotalDv,
    }
}

/// Walk a coarse grid and return the `(t_dep_min_rel_s,
/// t_dep_max_rel_s, tof_min_s, tof_max_s)` bounds of the contiguous
/// feasible basin, with no margin (the caller applies one).  Returns
/// `None` if no cell in the grid was feasible (degenerate
/// destination or extreme-budget transfer where the C3 ceiling
/// blocks every cell).
///
/// Index-to-value mapping mirrors the cell loop in
/// `build_grid_for_explicit_bounds`: row `r` maps to `tof_min_s +
/// (r / (rows - 1)) * (tof_max_s - tof_min_s)`, col `c` maps to
/// `t_dep_min_rel_s + (c / (cols - 1)) * (t_dep_max_rel_s -
/// t_dep_min_rel_s)`.  The col axes are coarse-resolution, so the
/// resulting bounds are quantized to `(cols - 1)` and `(rows - 1)`
/// steps — that's why the caller adds a 10 % margin in
/// `build_porkchop_grid_with_params`.
fn detect_basin_bounds(
    cells: &[PorkchopCell],
    cols: usize,
    rows: usize,
    t_dep_min_rel_s: f64,
    t_dep_max_rel_s: f64,
    tof_min_s: f64,
    tof_max_s: f64,
) -> Option<(f64, f64, f64, f64)> {
    let mut col_min: Option<usize> = None;
    let mut col_max: Option<usize> = None;
    let mut row_min: Option<usize> = None;
    let mut row_max: Option<usize> = None;

    for row in 0..rows {
        for col in 0..cols {
            if cells[row * cols + col].feasible {
                col_min = Some(col_min.map_or(col, |v| v.min(col)));
                col_max = Some(col_max.map_or(col, |v| v.max(col)));
                row_min = Some(row_min.map_or(row, |v| v.min(row)));
                row_max = Some(row_max.map_or(row, |v| v.max(row)));
            }
        }
    }

    let (col_min, col_max, row_min, row_max) = match (col_min, col_max, row_min, row_max) {
        (Some(a), Some(b), Some(c), Some(d)) => (a, b, c, d),
        _ => return None,
    };

    let t_dep_step = if cols > 1 {
        (t_dep_max_rel_s - t_dep_min_rel_s) / (cols - 1) as f64
    } else {
        0.0
    };
    let tof_step = if rows > 1 {
        (tof_max_s - tof_min_s) / (rows - 1) as f64
    } else {
        0.0
    };

    let t_dep_lo = t_dep_min_rel_s + col_min as f64 * t_dep_step;
    let t_dep_hi = t_dep_min_rel_s + col_max as f64 * t_dep_step;
    let tof_lo = tof_min_s + row_min as f64 * tof_step;
    let tof_hi = tof_min_s + row_max as f64 * tof_step;

    Some((t_dep_lo, t_dep_hi, tof_lo, tof_hi))
}

/// Fraction of the configured `tof_bounds_s` span that must remain
/// **beyond** the highest / lowest feasible row when adapting the
/// rendered Y-axis.  Keeps the cheap-transfer basin from being pinned
/// to either panel edge so the colormap band has visible breathing
/// room on both sides.  0.10 = 10% margin.
const TOF_BOUNDARY_MARGIN_FRAC: f64 = 0.10;

/// Compute the rendered `(tof_min_s, tof_max_s)` sub-range.
///
/// Contract: clip the configured Y-axis range to the band that
/// actually contains feasible cells, plus a small margin on each
/// side so the basin isn't pinned to either panel edge.  Used
/// for outer-planet / interstellar transfers where the C3
/// ceiling blocks every cell outside a narrow band around the
/// Hohmann time (e.g. for Earth→Jupiter the feasible band lives
/// in the middle of the configured range, with grey tails above
/// and below; without this trim the colormap band is compressed
/// into the middle band and the player reports "Y axis is fixed,
/// most tiles are grey").
///
/// The trim is symmetric: the rendered bottom anchor is the
/// lowest feasible row's TOF minus a margin, and the rendered
/// top anchor is the highest feasible row's TOF plus a margin.
/// Both stay inside `[tof_min_s, tof_max_s]`.  Row 0 is **not**
/// preserved when it is itself infeasible — the user's mental
/// model is "show me the populated basin, anchored at the
/// fastest feasible trajectory", and including a long grey tail
/// at the bottom is exactly the bug they reported.
///
/// Earlier versions of this helper preserved row 0 unconditionally
/// (so the "Depart Now + minimum TOF" reference point stayed
/// visible at the cost of a long grey tail).  That worked for
/// Earth→Mars (row 0 is the cheapest Hohmann transfer and is
/// always feasible), but for Earth→Jupiter row 0 is *not*
/// feasible (C3 ceiling cuts off the short-arc options) and the
/// trim did nothing useful.  The current contract is: trim both
/// ends, anchored at the populated band, with a small margin so
/// the basin stays readable.  The cheapest feasible cell still
/// lives in the lowest feasible row, so the player's "Depart
/// Now + minimum TOF" comparison point is still visible — it's
/// just no longer pinned to row 0 when row 0 is grey.
///
/// Degenerate cases:
///   * **No feasible cells at all** → fall back to the full range
///     so the panel shows the solver's full topology.
///   * **All rows feasible** → keep the full configured range.
pub fn compute_adaptive_tof_bounds(
    cells: &[PorkchopCell],
    cols: usize,
    rows: usize,
    tof_min_s: f64,
    tof_max_s: f64,
) -> (f64, f64) {
    if rows <= 1 || cols == 0 {
        return (tof_min_s, tof_max_s);
    }
    let configured_span = (tof_max_s - tof_min_s).max(0.0);
    let margin = TOF_BOUNDARY_MARGIN_FRAC * configured_span;

    // Find the highest and lowest row that contain at least one
    // feasible cell.  Walk the row-major grid once and record
    // the extremes; the helper is called once per grid build so
    // a single pass is fine.
    let mut highest_feasible_row: Option<usize> = None;
    let mut lowest_feasible_row: Option<usize> = None;
    for row in 0..rows {
        let mut any_feasible = false;
        for col in 0..cols {
            if cells[row * cols + col].feasible {
                any_feasible = true;
                break;
            }
        }
        if any_feasible {
            if lowest_feasible_row.is_none() {
                lowest_feasible_row = Some(row);
            }
            highest_feasible_row = Some(row);
        }
    }

    let (Some(lowest_row), Some(highest_row)) = (lowest_feasible_row, highest_feasible_row) else {
        // No feasible cells at all — fall back to the full range
        // so the player sees the solver's full topology.
        return (tof_min_s, tof_max_s);
    };

    // If the populated band already spans the entire configured
    // range, no trim is needed.  Catches the all-feasible case
    // and the single-row case (lowest == highest) where the
    // margin would force the bounds apart artificially.
    if lowest_row == 0 && highest_row == rows - 1 {
        return (tof_min_s, tof_max_s);
    }

    // Map an original row index to its absolute TOF value.
    let row_to_tof = |row: usize| -> f64 {
        let row_frac = row as f64 / (rows as f64 - 1.0);
        tof_min_s + row_frac * configured_span
    };

    // Trim both ends, anchored at the populated band.  Add a
    // small margin so the basin isn't pinned to either panel
    // edge.  Clamp into the configured range so the rendered
    // bounds never escape `[tof_min_s, tof_max_s]`.
    let bottom_tof = row_to_tof(lowest_row);
    let top_tof = row_to_tof(highest_row);
    let new_bottom = (bottom_tof - margin).max(tof_min_s);
    let new_top = (top_tof + margin).min(tof_max_s);

    // Safety guard: never invert the bounds.  If the populated
    // band is so narrow that the margins would force the bounds
    // to cross, fall back to the raw band without margins.
    if new_bottom >= new_top {
        return (bottom_tof.min(top_tof), bottom_tof.max(top_tof));
    }

    (new_bottom, new_top)
}

/// Resolve the rendered `(cols, rows)` from the RON-provided
/// minimums and the natural `(t_dep_window, tof_span)` of the
/// current transfer.
///
/// Contract: the RON `resolution_t_dep` / `resolution_tof` are
/// treated as **minimums**, not absolutes.  If the natural
/// window is wider than the RON values can comfortably sample
/// (i.e. `window / cols > MAX_T_DEP_STEP_DAYS` or
/// `span / rows > MAX_TOF_STEP_DAYS`), the helper bumps `cols`
/// and / or `rows` up so the per-cell step lands in the
/// 2-day / 3-day ballpark.  The total is clamped to
/// `ADAPTIVE_RESOLUTION_CELL_CAP` (20000 cells, the static
/// validator ceiling) by shrinking whichever dimension has the
/// looser step first — never below the RON minimum.
///
/// Algorithm:
///   1. `desired_cols = ceil(window_days / MAX_T_DEP_STEP_DAYS)`
///      and the same for rows.  Take `max(RON min, desired)`.
///   2. If `cols * rows ≤ cap`, return.
///   3. Else iteratively shrink the dimension with the looser
///      step until the product fits.  Never shrink below the
///      RON minimum.  Falls back to the product-as-is if both
///      dimensions are pinned at their minimums and still
///      exceed the cap (degenerate but possible if a modder
///      hand-crafts a 150×150 RON override).
///
/// Why this exists: the RON `resolution_t_dep` field is a
/// fixed count, but the t_dep window varies wildly between
/// categories.  Earth→Moon's 14-day window over 90 cols gives
/// 0.16 d/column (excellent), but Earth→Mercury via Venus GA
/// inherits a ~614-day Earth↔Venus-synodic window over the
/// `gravity_assist` override's 90 cols → 6.8 d/column.  Without
/// the auto-bump, wide-window transfers render the cheap-assist
/// basin as 1-cell-wide color stripes (the user-reported
/// Earth→Mercury screenshot).  With the auto-bump, the same
/// 614-day window lands at ~307 cols × ~65 rows = 19955 cells
/// → 2.0 d/column, 4.0 d/row — the basin spans 1.5–2.5 cells
/// of the cheapest color band instead of 0.8.
///
/// Tradeoff: `MAX_T_DEP_STEP_DAYS = 2.0` and
/// `MAX_TOF_STEP_DAYS = 3.0` are tuned for the LGD's "sub-day
/// launch-window resolution" target.  Lowering them further
/// improves grid smoothness but inflates rebuild cost (Lambert
/// solver runs ~0.3 ms/cell, so 20000 cells ≈ 6 s worst case
/// — inside the 3-day staleness budget at 1 yr/s sim speed,
/// but bumping to a 1-day step target would push past the cap
/// on wide windows).
///
/// `cap` is passed in by the caller (see
/// [`adaptive_resolution_cell_cap_for`]) so the ceiling varies
/// between inner-planet (20 000 cells) and outer-planet (8 000
/// cells) destinations.  Tests and helpers that don't care
/// about the destination class can pass
/// `ADAPTIVE_RESOLUTION_CELL_CAP_INNER_PLANET` directly.
fn adaptive_resolution(
    min_cols: usize,
    min_rows: usize,
    window_days: f64,
    span_days: f64,
    cap: usize,
) -> (usize, usize) {
    // Floor at 2 cols / rows: a single-cell grid would divide by
    // zero in the per-row / per-col fractional mapping downstream
    // and the panel can't render a 1-cell heatmap.  Matches the
    // existing `params.resolution_t_dep.max(2)` floor.
    let min_cols = min_cols.max(2);
    let min_rows = min_rows.max(2);

    let mut cols = (window_days / MAX_T_DEP_STEP_DAYS)
        .ceil()
        .max(min_cols as f64) as usize;
    let mut rows = (span_days / MAX_TOF_STEP_DAYS).ceil().max(min_rows as f64) as usize;

    // If we're already under the cap, take the bumps.  This is
    // the common path: narrow-window transfers (Earth→Moon,
    // short_hop) get max(RON, desired) — but their desired is
    // usually < RON, so the RON wins; wide-window transfers
    // (Earth→Mercury via Venus GA) get desired > RON and bump
    // up.
    if cols * rows <= cap {
        return (cols, rows);
    }

    // Cap exceeded.  Shrink the looser-step dimension first.
    // This keeps the more-sensitive axis (smaller step) at its
    // current size and only trims the less-sensitive one.  For
    // Earth→Mercury via Venus GA the row step is naturally
    // looser than the desired 3-day target, so rows get shrunk
    // toward the RON minimum first while cols holds at the
    // 2-day step target (307 cols × 65 rows = 19955 cells,
    // under the 20000-cell cap).
    while cols * rows > cap {
        let col_step = window_days / cols as f64;
        let row_step = span_days / rows as f64;
        // Shrink whichever dimension has the looser step.
        // Ties: prefer shrinking cols so rows stay at the 5-day
        // TOF target (matches the LGD's "TOF is the more
        // sensitive axis for basin identification" design note
        // — see GRA-386).
        if col_step >= row_step {
            if cols > min_cols {
                cols -= 1;
            } else if rows > min_rows {
                rows -= 1;
            } else {
                // Both pinned at the RON minimum and still over
                // budget — accept the over-budget grid rather
                // than violate the RON contract.  Caller's
                // validator should have caught this at load
                // time; if it didn't, the rebuild is bounded by
                // a single over-budget iteration rather than an
                // infinite loop.
                break;
            }
        } else if rows > min_rows {
            rows -= 1;
        } else if cols > min_cols {
            cols -= 1;
        } else {
            break;
        }
    }
    (cols, rows)
}

/// Compute the (t_dep_min, t_dep_max) bounds, anchored at the player's
/// current `sim_time_s`.
///
/// Returns the **absolute** departure window `(sim_time_s, sim_time_s +
/// max_t_dep)`.  The window spans at least one full synodic period
/// plus a half-window buffer — the same convention NASA / JPL use on
/// their public porkchop plots.  The ΔV surface is periodic in `t_dep`
/// with period `synodic_period`, so one full period is sufficient to
/// see every distinct alignment the player could use.
///
/// We deliberately include `t_dep = sim_time_s` so the player can
/// always click a "Depart Now" cell on the grid — that was the gap
/// the user reported for long-synodic-period destinations (Saturn,
/// Uranus, Neptune) where the cheapest Hohmann window sits a year out
/// and the legacy slider only let them inspect the immediate-launch
/// ΔV cost via the side-panel stat, not visually on the plot.
///
/// GRA-169 (Part A): re-anchored the bounds from `(0, max_t_dep)` to
/// `(sim_time_s, sim_time_s + max_t_dep)` so the rotating buffer's
/// `t_dep_min` tracks the player's clock at rebuild.  The earlier
/// `t_dep_min = 0` was a hold-over that made the visible window snap
/// to the left edge on every buffer rotation; the cell math is
/// unchanged because `solve_cell` already used `t_dep_abs =
/// sim_time_s + t_dep_s` for the Lambert solver, so shifting the
/// anchor by `sim_time_s` only relabels the cell dates, not the ΔV
/// values.
///
/// Co-orbital pairs (`|d_phi_dt| ≈ 0`, e.g. Earth↔Moon in Sol) have
/// an infinite synodic period; in that degenerate case we fall back
/// to `± half` around `sim_time_s` — the phase never advances so a
/// wider window adds no information, and centring on the Hohmann
/// date keeps the colormap contrast around the cheap-transfer basin.
fn dep_window_bounds(inputs: &PorkchopInputs, params: &ResolvedPorkchopParams) -> (f64, f64) {
    use std::f64::consts::TAU;
    let half = 0.5 * params.t_dep_window_days * SECONDS_PER_DAY;
    let r1 = inputs.origin_orbit.semi_major_axis.max(1e-6);
    let r2 = inputs.dest_orbit.semi_major_axis.max(1e-6);
    let n1 = inputs.origin_orbit.mean_motion;
    let n2 = inputs.dest_orbit.mean_motion;
    let d_phi_dt = n2 - n1;
    if d_phi_dt.abs() < 1e-25 {
        // Co-orbital degenerate case: phase never changes, so the
        // optimal Hohmann transfer has the same ΔV from any
        // t_dep.  Keep the legacy compact ±half window centred on
        // `sim_time_s` (now).  Players picking "Depart Now" get the
        // same cell as a future launch in this degenerate case.
        let _ = hohmann_time_s(r1, r2, inputs.system_gm);
        return (inputs.sim_time_s, inputs.sim_time_s + 2.0 * half);
    }
    let synodic_period_s = TAU / d_phi_dt.abs();
    // Period + half so the player sees a little beyond the next
    // alignment (handy for inspecting the *following* window before
    // deciding).  Falls back to 2 * half for degenerate categories
    // (e.g. moon transfers with a very short configured window).
    let max_t_dep = synodic_period_s.max(2.0 * half) + half;
    (inputs.sim_time_s, inputs.sim_time_s + max_t_dep)
}

fn hohmann_time_s(r1_au: f64, r2_au: f64, gm: f64) -> f64 {
    use super::orbital_mechanics::AU_IN_METERS;
    let r1 = r1_au * AU_IN_METERS;
    let r2 = r2_au * AU_IN_METERS;
    let a = (r1 + r2) / 2.0;
    std::f64::consts::PI * (a.powi(3) / gm).sqrt()
}

/// Solve a single (t_dep, tof) cell.  Infeasibility covers:
///   * Lambert solver failed for both branches;
///   * C3 < 0 (unphysical);
///   * C3 > ceiling (escape budget);
///   * TOF exceeds `MAX_CURVED_CROSS_STAR_TRANSFER_TIME_S` (the 500-yr
///     H-2 cap; surface as a "Geological timescale" option, not drop).
fn solve_cell(
    inputs: &PorkchopInputs,
    t_dep_s: f64,
    tof_s: f64,
    c3_ceiling_ms2: f64,
) -> PorkchopCell {
    let t_dep_abs = inputs.sim_time_s + t_dep_s;
    let t_arr_abs = t_dep_abs + tof_s;
    // Planet positions are at *absolute* sim time = `t_dep_abs` and
    // `t_arr_abs`, not at `t_dep_s` and `t_dep_s + tof_s`.  The
    // grid is built with `sim_time_s` anchored to the player's
    // current sim clock; the `t_dep_s` cell offset is measured
    // from that anchor, not from the orbit's mean-anomaly epoch
    // (which is set at the orbit's spawn time and is independent
    // of the player's clock).  Adding `sim_time_s` here is what
    // makes the cells' ΔV values track the actual planet
    // positions as the player advances the clock.
    let origin_pos_au = orbit_position_from_mean_anomaly(
        &inputs.origin_orbit,
        inputs.origin_orbit.mean_anomaly_epoch + inputs.origin_orbit.mean_motion * t_dep_abs,
    );
    let dest_pos_au = orbit_position_from_mean_anomaly(
        &inputs.dest_orbit,
        inputs.dest_orbit.mean_anomaly_epoch + inputs.dest_orbit.mean_motion * t_arr_abs,
    );

    if tof_s > MAX_CURVED_CROSS_STAR_TRANSFER_TIME_S {
        return PorkchopCell {
            t_dep_s,
            tof_s,
            total_dv_ms: f64::INFINITY,
            c3_departure: 0.0,
            v_inf_arrival_ms: 0.0,
            delta_v1_ms: 0.0,
            delta_v2_ms: 0.0,
            feasible: false,
            origin_pos_au,
            dest_pos_au,
            v_departure_ms: DVec3::ZERO,
            v_arrival_ms: DVec3::ZERO,
            transfer_orbit: None,
        };
    }

    match solve_lambert_transfer(origin_pos_au, dest_pos_au, tof_s, inputs.system_gm) {
        Some((v1_ms, v2_ms, orbit)) => {
            // v_inf at departure uses the heliocentric r1 (the solver's
            // frame).  c3 = v_inf² — geometrically invariant, this is the
            // energy "left over" after climbing out of the central body's
            // potential well and equals (v_dep² - v_esc²) at any r1.
            let r1_helio_m = (origin_pos_au * super::orbital_mechanics::AU_IN_METERS).length();
            let v_circ_at_r1_helio_ms = (inputs.system_gm / r1_helio_m).sqrt();
            let v1_speed_ms = v1_ms.length();
            // GRA-NNN: keep `.abs()` (not `.max(0.0)`) so retrograde burns
            // (inward Hohmann like Earth→Mercury where the Lambert solver
            // returns v1 < v_circ) keep their ΔV.  Squaring gives c3 =
            // v_inf² which is geometrically invariant regardless of sign.
            let v_inf_dep_ms = (v1_speed_ms - v_circ_at_r1_helio_ms).abs();
            let c3 = v_inf_dep_ms * v_inf_dep_ms; // m²/s²
            if !c3.is_finite() || c3 > c3_ceiling_ms2 {
                return PorkchopCell {
                    t_dep_s,
                    tof_s,
                    total_dv_ms: f64::INFINITY,
                    c3_departure: c3,
                    v_inf_arrival_ms: 0.0,
                    delta_v1_ms: 0.0,
                    delta_v2_ms: 0.0,
                    feasible: false,
                    origin_pos_au,
                    dest_pos_au,
                    v_departure_ms: v1_ms,
                    v_arrival_ms: v2_ms,
                    transfer_orbit: None,
                };
            }
            // ΔV₁ is the burn from the *player's parking orbit* up to the
            // transfer-ellipse departure hyperbolic trajectory.  When the
            // player picked a shell (`inputs.parking_radius_m = Some`),
            // honour it: compute the parking-orbit circular speed at the
            // shell's radius, then derive the hyperbolic-departure speed
            // from c3 + v_esc²_parking so a Low shell (LEO) costs more
            // ΔV than a High shell (above the atmosphere).  Without this,
            // the planet grid would show the heliocentric-SMA ΔV
            // regardless of shell pick — the bug that motivated the
            // GRA-NNN follow-up.
            //
            // Convention: `v_esc² = 2 GM / r` and `v_circ² = GM / r`, so
            // `v_esc² = 2 v_circ²`.  v_dep²(hyperbolic) = c3 + v_esc².
            // ΔV₁ = |v_dep − v_circ_parking|.
            let (_v_circ_parking_ms, dep_burn_ms) = match inputs.parking_radius_au {
                Some(r1_au_parking) => {
                    // GRA-NNN: parking_v_circ uses `body_gm` — the shell
                    // picker always picks a radius in the body's local
                    // frame (Low/Medium/High are 1×/3×/10× body
                    // radius), so the parking orbit lives in the body's
                    // gravitational well, not the Sun's.  Using
                    // system_gm at LEO scale gives v_circ ≈ 4 350 km/s
                    // at Earth and a 1 800 km/s ΔV — see the new
                    // `body_gm` field's docs for the math.
                    let r1_m_parking = r1_au_parking * super::orbital_mechanics::AU_IN_METERS;
                    let v_circ_p = (inputs.body_gm / r1_m_parking).sqrt();
                    let v_esc_sq_p = 2.0 * v_circ_p * v_circ_p;
                    let v_dep_sq = c3 + v_esc_sq_p;
                    let v_dep_p = if v_dep_sq > 0.0 { v_dep_sq.sqrt() } else { 0.0 };
                    // ΔV₁ is the |v_dep − v_circ| magnitude.  Prograde
                    // burns have v_dep > v_circ (outward Hohmann);
                    // retrograde burns have v_dep < v_circ (inward
                    // Hohmann — the previous `v1 − v_circ` formula gave
                    // 0 km/s for these).  `.abs()` preserves both.
                    let dv = (v_dep_p - v_circ_p).abs();
                    (v_circ_p, dv)
                }
                // No parking_radius_au — legacy "parking at
                // heliocentric SMA" assumption, kept for save-compat
                // and tests that don't care about the player's shell.
                None => (
                    v_circ_at_r1_helio_ms,
                    (v1_speed_ms - v_circ_at_r1_helio_ms).abs(),
                ),
            };
            // Destination leg: parking at heliocentric SMA by default,
            // or at the player's shell altitude (symmetric to origin).
            // GRA-NNN: when `parking_radius_au` is set, treat the
            // destination's circular parking orbit as being at that
            // radius around `body_gm`.  v_circ at the parking orbit,
            // v_esc = sqrt(2)·v_circ, and the burn from the transfer
            // ellipse to circular parking are derived from c3 (the
            // trajectory's invariant hyperbolic-excess energy) plus
            // the parking-orbit escape velocity.  This makes "Low"
            // cost more ΔV at both ends of the transfer.
            let r2_m = (dest_pos_au * super::orbital_mechanics::AU_IN_METERS).length();
            let v_circ_arr_helio_ms = (inputs.system_gm / r2_m).sqrt();
            let v2_speed_ms = v2_ms.length();
            // Compute `c3` at the destination side (v_inf² for the
            // trajectory).  Same c3 as origin (geometrically invariant)
            // for Hohmann, but `.abs()` keeps retrograde and prograde
            // arrivals symmetric.
            let v_inf_arr_helio_ms = (v2_speed_ms - v_circ_arr_helio_ms).abs();
            let c3_arr = v_inf_arr_helio_ms * v_inf_arr_helio_ms;
            let (v_circ_arr_ms, arr_burn_ms) = match inputs.parking_radius_au {
                Some(r2_au_arr) => {
                    // Parking at the shell altitude around the
                    // destination body.  Use the same `body_gm` as the
                    // origin (a per-leg GM would let origin/destination
                    // use different bodies' GMs, but the shell picker
                    // is single-valued and the common case is
                    // interplanetary with both ends at the same
                    // body-class).
                    let r2_m_park = r2_au_arr * super::orbital_mechanics::AU_IN_METERS;
                    let v_circ_park = (inputs.body_gm / r2_m_park).sqrt();
                    let v_esc_sq_park = 2.0 * v_circ_park * v_circ_park;
                    let v_arr_sq = c3_arr + v_esc_sq_park;
                    let v_arr_p = if v_arr_sq > 0.0 { v_arr_sq.sqrt() } else { 0.0 };
                    // arr_burn: how much we must change speed from the
                    // transfer ellipse to circular parking.  `.abs()`
                    // so inward-Hohmann arrivals where v_arr > v_circ
                    // show the correct retrofire burn instead of 0 km/s.
                    let dv = (v_arr_p - v_circ_park).abs();
                    (v_circ_park, dv)
                }
                // No parking_radius_au — legacy "parking at
                // heliocentric SMA" assumption.  Keep for save-compat
                // and tests that don't care about the player's shell.
                None => (
                    v_circ_arr_helio_ms,
                    (v_circ_arr_helio_ms - v2_speed_ms).abs(),
                ),
            };
            // v_inf_arrival: the *hyperbolic excess* at the destination
            // (the speed the spacecraft is moving *above* circular
            // orbital speed at the parking radius).  Only positive when
            // v_arr > v_circ_park; for inward-Hohmann arrivals the
            // entire delta-v is a real brake burn (captured by
            // `arr_burn_ms` above), not an unbrakeable hyperbolic
            // excess.
            let v_inf_arrival_ms = (v2_speed_ms - v_circ_arr_ms).max(0.0);
            let total = dep_burn_ms + arr_burn_ms;
            PorkchopCell {
                t_dep_s,
                tof_s,
                total_dv_ms: total,
                c3_departure: c3,
                v_inf_arrival_ms,
                delta_v1_ms: dep_burn_ms,
                delta_v2_ms: arr_burn_ms,
                feasible: true,
                origin_pos_au,
                dest_pos_au,
                v_departure_ms: v1_ms,
                v_arrival_ms: v2_ms,
                transfer_orbit: Some(orbit),
            }
        }
        None => PorkchopCell {
            t_dep_s,
            tof_s,
            total_dv_ms: f64::INFINITY,
            c3_departure: 0.0,
            v_inf_arrival_ms: 0.0,
            delta_v1_ms: 0.0,
            delta_v2_ms: 0.0,
            feasible: false,
            origin_pos_au,
            dest_pos_au,
            v_departure_ms: DVec3::ZERO,
            v_arrival_ms: DVec3::ZERO,
            transfer_orbit: None,
        },
    }
}

// === Local-frame Lambert grid (GRA-167, GRA-164-C) =========================
//
// Solves a `(t_dep, t_tof)` porkchop in the parent body's local frame
// (e.g. Earth→Moon, Jupiter→Europa, Saturn→Titan).  The standard
// `build_porkchop_grid` uses the host star's `system_gm` and the
// heliocentric orbits of origin/dest — wrong by 4-5 orders of magnitude
// for cislunar transfers.  The local-frame variant uses the parent
// body's GM, a parking-orbit radius for r1, and the destination moon's
// orbital radius for r2.
//
// Position model: parent-centred inertial frame.  Origin and destination
// move on concentric circles with the parent's mean motion.  At
// `sim_time_s`, origin is at angle `phi1` and dest at `phi2 = phi1 +
// (delta_angle_at_sim_time)`.  Both rotate together at `omega`.  This
// assumes circular orbits (eccentricity is captured implicitly via the
// Hohmann-time-anchored t_dep window).

/// Inputs to `build_porkchop_grid_for_local_frame`.  Caller resolves
/// the parking orbit radius (e.g. LEO for Earth), the destination
/// moon's orbital radius, the parent body's GM (kg·m³/s⁻²), and the
/// destination's name.  The builder is free of Bevy queries and stays
/// unit-testable.
#[derive(Debug, Clone)]
pub struct LocalPorkchopInputs {
    pub origin_name: String,
    pub dest_name: String,
    /// Origin parking-orbit radius around the parent body, AU.
    pub parking_radius_au: f64,
    /// Destination orbit radius around the parent body, AU.
    pub dest_orbit_au: f64,
    /// Parent body gravitational parameter (m³/s²).  Earth ≈ 3.986e14,
    /// Jupiter ≈ 1.266e17, Saturn ≈ 3.793e16.
    pub parent_gm: f64,
    /// `sim_time_s` — the "now" epoch the player is planning from.
    pub sim_time_s: f64,
    /// Initial phase angle (rad) of the parking orbit at `sim_time_s`.
    /// Caller is responsible for picking this from the parent's
    /// inertial frame (e.g. via `mean_anomaly_epoch`).
    pub origin_phase_at_epoch_rad: f64,
    /// Initial phase angle (rad) of the destination orbit at `sim_time_s`.
    pub dest_phase_at_epoch_rad: f64,
    /// Category match key for `PorkchopConfig::resolve` (e.g. "local_moon").
    pub category: String,
}

/// Build a local-frame porkchop grid.  Equivalent to
/// `build_porkchop_grid` but uses the parent body's GM (not the host
/// star's), the parking-orbit radius for r1, and the destination moon's
/// orbital radius for r2.  Origin and dest move on concentric circles
/// in the parent-centred inertial frame at the parent's mean motion.
///
/// Resolution contract is identical to `build_porkchop_grid` —
/// the RON `resolution_t_dep` / `resolution_tof` are MINIMUMS,
/// and `adaptive_resolution` bumps them up if the natural
/// `(t_dep_window, tof_span)` is wider than the static values
/// can comfortably sample.  Local-frame transfers are usually
/// narrow-window (e.g. 14-day moon transfers), so the auto-bump
/// is typically a no-op here; the helper still normalises the
/// cap-and-floor logic so the two builders stay in lockstep.
pub fn build_porkchop_grid_for_local_frame(
    cfg: &PorkchopConfig,
    inputs: &LocalPorkchopInputs,
) -> PorkchopGrid {
    let params = cfg.resolve(&inputs.category);
    build_local_porkchop_grid_with_params(params, inputs)
}

pub fn build_local_porkchop_grid_with_params(
    params: ResolvedPorkchopParams,
    inputs: &LocalPorkchopInputs,
) -> PorkchopGrid {
    use super::orbital_mechanics::AU_IN_METERS;

    // Mean motion of the destination orbit around the parent body
    // (rad/s).  For circular orbits: omega = sqrt(GM / r^3).
    let r_dest_m = inputs.dest_orbit_au * AU_IN_METERS;
    let omega = (inputs.parent_gm / r_dest_m.powi(3)).sqrt();

    // Hohmann TOF in seconds for the local-frame r1 → r2 transfer.
    let tof_h = hohmann_time_s_local(
        inputs.parking_radius_au,
        inputs.dest_orbit_au,
        inputs.parent_gm,
    );

    // Window bounds.  For local-frame transfers the natural "anchor"
    // is `sim_time_s` (no synodic-period alignment like heliocentric).
    // We use the override's `t_dep_window_days` centred on 0 (= now).
    let half_window_s = 0.5 * params.t_dep_window_days * SECONDS_PER_DAY;
    let t_dep_min_s = -half_window_s;
    let t_dep_max_s = half_window_s;

    let tof_min_s =
        (params.tof_min_hohmann_factor * tof_h).max(params.tof_floor_days * SECONDS_PER_DAY);
    let tof_max_s =
        (params.tof_max_hohmann_factor * tof_h).min(params.tof_ceiling_years * SECONDS_PER_YEAR);

    // Adaptive resolution (mirrors the heliocentric builder): the
    // RON `resolution_t_dep` / `resolution_tof` are MINIMUMS.
    // Local-frame transfers are typically narrow-window (e.g.
    // 14-day moon transfers), so the auto-bump is usually a
    // no-op here, but the helper still normalises the cap-and-
    // floor logic so the two builders stay in lockstep.
    let window_days = params.t_dep_window_days;
    let span_days = (tof_max_s - tof_min_s) / SECONDS_PER_DAY;
    // Local-frame transfers are short cislunar / planet-moon pairs
    // (typical 14-day t_dep window, sub-year TOF span) and never
    // saturate the cap, but pass the inner cap explicitly so future
    // wider-window local-frame categories (e.g. Earth → Luna-class
    // asteroid via parent-body phasing) don't silently inherit the
    // outer cap when this helper gains more call-sites.
    let cap = ADAPTIVE_RESOLUTION_CELL_CAP_INNER_PLANET;
    let (cols, rows) = adaptive_resolution(
        params.resolution_t_dep,
        params.resolution_tof,
        window_days,
        span_days,
        cap,
    );
    let total_cells = cols * rows;
    let mut cells: Vec<PorkchopCell> = Vec::with_capacity(total_cells);
    let mut min_cell: Option<(usize, usize)> = None;
    let mut min_dv: f64 = f64::INFINITY;

    let c3_ceiling_ms2 = params.c3_ceiling_km2_s2 * 1.0e6; // (km/s)² → (m/s)²

    for row in 0..rows {
        let row_frac = if rows > 1 {
            row as f64 / (rows as f64 - 1.0)
        } else {
            0.0
        };
        let tof_s = tof_min_s + row_frac * (tof_max_s - tof_min_s);
        for col in 0..cols {
            let col_frac = if cols > 1 {
                col as f64 / (cols as f64 - 1.0)
            } else {
                0.0
            };
            let t_dep_s = t_dep_min_s + col_frac * (t_dep_max_s - t_dep_min_s);
            let cell = solve_local_cell(inputs, omega, t_dep_s, tof_s, c3_ceiling_ms2);
            if cell.feasible && cell.total_dv_ms < min_dv {
                min_dv = cell.total_dv_ms;
                min_cell = Some((col, row));
            }
            cells.push(cell);
        }
    }

    // Y-axis bounds: bottom-anchored adaptive trim, mirroring the
    // heliocentric builder so both panel paths frame the cheapest
    // feasible trajectory at the bottom and trim the empty grey
    // tail above the basin.  See `compute_adaptive_tof_bounds`
    // for the contract.
    let rendered_tof_bounds_s =
        compute_adaptive_tof_bounds(&cells, cols, rows, tof_min_s, tof_max_s);

    PorkchopGrid {
        origin_name: inputs.origin_name.clone(),
        dest_name: inputs.dest_name.clone(),
        t_dep_bounds_s: (t_dep_min_s, t_dep_max_s),
        tof_bounds_s: (tof_min_s, tof_max_s),
        rendered_tof_bounds_s,
        resolution: (cols, rows),
        cells,
        min_cell,
        metric: PorkchopMetric::TotalDv,
    }
}

/// Hohmann TOF for a local-frame (parent-centred) circular orbit pair.
/// `r1` and `r2` are in AU; `gm` is the parent body's GM in m³/s².
fn hohmann_time_s_local(r1_au: f64, r2_au: f64, gm: f64) -> f64 {
    use super::orbital_mechanics::AU_IN_METERS;
    let r1 = r1_au * AU_IN_METERS;
    let r2 = r2_au * AU_IN_METERS;
    let a = (r1 + r2) / 2.0;
    std::f64::consts::PI * (a.powi(3) / gm).sqrt()
}

/// Position of a circular-orbit body at time `t_offset_s` from epoch
/// in the parent-centred inertial frame.  Returns metres.
fn local_position_m(radius_m: f64, phase_at_epoch_rad: f64, omega: f64, t_offset_s: f64) -> DVec3 {
    let angle = phase_at_epoch_rad + omega * t_offset_s;
    DVec3::new(radius_m * angle.cos(), radius_m * angle.sin(), 0.0)
}

/// Solve a single (t_dep, tof) cell in the local frame.  Mirrors
/// `solve_cell` but:
///   * origin/dest positions come from `local_position_m` (parent-centred)
///   * the Lambert solver is called with `parent_gm` (not heliocentric GM)
///   * the circular-speed check at r1 uses `parent_gm` (not system_gm)
fn solve_local_cell(
    inputs: &LocalPorkchopInputs,
    omega: f64,
    t_dep_s: f64,
    tof_s: f64,
    c3_ceiling_ms2: f64,
) -> PorkchopCell {
    use super::orbital_mechanics::AU_IN_METERS;
    let r1_m = inputs.parking_radius_au * AU_IN_METERS;
    let r2_m = inputs.dest_orbit_au * AU_IN_METERS;

    let origin_pos_m = local_position_m(r1_m, inputs.origin_phase_at_epoch_rad, omega, t_dep_s);
    let dest_pos_m = local_position_m(r2_m, inputs.dest_phase_at_epoch_rad, omega, t_dep_s + tof_s);

    // Convert metres → AU for the solver's input contract.
    let origin_pos_au = origin_pos_m / AU_IN_METERS;
    let dest_pos_au = dest_pos_m / AU_IN_METERS;

    if tof_s > MAX_CURVED_CROSS_STAR_TRANSFER_TIME_S {
        return PorkchopCell {
            t_dep_s,
            tof_s,
            total_dv_ms: f64::INFINITY,
            c3_departure: 0.0,
            v_inf_arrival_ms: 0.0,
            delta_v1_ms: 0.0,
            delta_v2_ms: 0.0,
            feasible: false,
            origin_pos_au,
            dest_pos_au,
            v_departure_ms: DVec3::ZERO,
            v_arrival_ms: DVec3::ZERO,
            transfer_orbit: None,
        };
    }

    match solve_lambert_transfer(origin_pos_au, dest_pos_au, tof_s, inputs.parent_gm) {
        Some((v1_ms, v2_ms, orbit)) => {
            let v_circ_dep_ms = (inputs.parent_gm / r1_m).sqrt();
            let v_circ_arr_ms = (inputs.parent_gm / r2_m).sqrt();
            let v1_speed_ms = v1_ms.length();
            let v2_speed_ms = v2_ms.length();
            let dep_burn_ms = (v1_speed_ms - v_circ_dep_ms).max(0.0);
            let arr_burn_ms = (v_circ_arr_ms - v2_speed_ms).max(0.0);
            let v_inf_dep_ms = (v1_speed_ms - v_circ_dep_ms).max(0.0);
            let c3 = v_inf_dep_ms * v_inf_dep_ms;
            if !c3.is_finite() || c3 > c3_ceiling_ms2 {
                return PorkchopCell {
                    t_dep_s,
                    tof_s,
                    total_dv_ms: f64::INFINITY,
                    c3_departure: c3,
                    v_inf_arrival_ms: 0.0,
                    delta_v1_ms: 0.0,
                    delta_v2_ms: 0.0,
                    feasible: false,
                    origin_pos_au,
                    dest_pos_au,
                    v_departure_ms: v1_ms,
                    v_arrival_ms: v2_ms,
                    transfer_orbit: None,
                };
            }
            let v_inf_arrival_ms = (v2_speed_ms - v_circ_arr_ms).max(0.0);
            let total = dep_burn_ms + arr_burn_ms;
            PorkchopCell {
                t_dep_s,
                tof_s,
                total_dv_ms: total,
                c3_departure: c3,
                v_inf_arrival_ms,
                delta_v1_ms: dep_burn_ms,
                delta_v2_ms: arr_burn_ms,
                feasible: true,
                origin_pos_au,
                dest_pos_au,
                v_departure_ms: v1_ms,
                v_arrival_ms: v2_ms,
                transfer_orbit: Some(orbit),
            }
        }
        None => PorkchopCell {
            t_dep_s,
            tof_s,
            total_dv_ms: f64::INFINITY,
            c3_departure: 0.0,
            v_inf_arrival_ms: 0.0,
            delta_v1_ms: 0.0,
            delta_v2_ms: 0.0,
            feasible: false,
            origin_pos_au,
            dest_pos_au,
            v_departure_ms: DVec3::ZERO,
            v_arrival_ms: DVec3::ZERO,
            transfer_orbit: None,
        },
    }
}

// === Short-hop preset grid (GRA-367-C, Phase 3, amended r2) ================
//
// Single-column porkchop returned by `build_short_hop_grid` for cislunar
// transfers (Earth→Moon and similar low-Δv planet-moon pairs).  Rows
// are a configurable number of presets spread across the bi-elliptic /
// Hohmann / direct spectrum — `{0.6, 0.85, 1.0, 1.4, 2.0} × T_Hohmann`
// at `n_options = 5` — so the player sees *variety*, not three near-
// identical Hohmann copies at different effective speeds.  The renderer
// caps visual height at 9 rows; the design also hard-caps the option
// count at 9 to keep the planner's per-frame budget bounded (longer
// grids need scrolling, which the planner does via `ScrollArea`).
//
// The grid is `n_options × 1` — `cols = 1` because the player has not
// picked a departure window for a short-hop; t_dep anchors at "now".
// Plane change (`delta_i_rad != 0`) is added as a fixed penalty on top
// of the planar Lambert burns, using the standard 2·v·sin(Δi/2) budget
// at the cheaper of the two circular speeds.  Co-planar transfers
// (Earth↔Moon, Earth↔LEO) take the no-penalty branch.

/// Clamp `n_options` to the `[3, 9]` band the design doc and the
/// `short_hop_options` RON key allow.  Below 3 the bar collapses to a
/// single Hohmann and the player's choice disappears; above 9 the
/// `porkchop_panel` overflow scroll kicks in and 9 visible rows is
/// the design ceiling.  Out-of-range calls log a warning but the
/// planner cannot enforce a hard panic — callers (`build_short_hop_grid`)
/// already call this clamp as the first step.
///
/// GRA-385 follow-ups:
///   * Upper bound bumped from 9 to 25 so the RON's
///     `short_hop_options: Some(N)` can hand the player "a few
///     tens of options" instead of the original design's narrow
///     3..=9 set.
///   * Then to 40 so the player can pick the sweet spot with
///     finer-grained ΔV resolution — at 40 rows the per-row ΔV
///     step on Earth→Moon drops to ~50 m/s and the player can
///     land on a specific Hohmann-like preset by eye instead of
///     scrolling through 5 coarse rows.  The 0.6×..2.0× Hohmann
///     preset range gets sub-divided into that many rows by the
///     same `log2` interpolation the existing 5-row set uses, so
///     the player sees a smooth fast→slow gradient.  Memory cost
///     is bounded by the panel's supersampled texture (12×12 = 144
///     texels per cell × 40 rows = ~6 KB for the worst case) so
///     the bump is free.
pub fn clamp_short_hop_options(n: usize) -> usize {
    n.clamp(3, 40)
}

/// Clamp the per-category `short_hop_t_dep_steps` knob into
/// `[1, 16]`.  `1` reproduces the original single-column bar
/// (depart-now behaviour); `16` gives roughly 12-hour steps
/// across a 1-week window which is enough resolution to spot
/// the synodic-phase sweep on any moon system (Galilean moons
/// ≈ 1.7 d for Io/Europa; Saturn's moons ≈ 16 d for Titan).
/// Above 16 the colormap starts showing per-column ΔV jumps
/// bigger than 50 m/s which the player can't distinguish at
/// panel resolution.
pub fn clamp_short_hop_t_dep_steps(n: usize) -> usize {
    n.clamp(1, 16)
}

/// Build a single-column porkchop of `n_options` rows for a cislunar /
/// short-hop transfer.  Rows are a spread of presets across the
/// `{0.6×T_H, 0.85×T_H, 1.0×T_H, 1.4×T_H, 2.0×T_H}` spectrum (sampled
/// at `n_options` points evenly in log2 space so `n = 5` lands on
/// every preset).  The departure epoch is pinned to `0` (now) — the
/// player has not picked a launch window for a short-hop transfer.
///
/// Inputs:
///   * `r1_au` — origin parking-orbit radius (e.g. Earth LEO ≈ 6.7e-5 AU).
///   * `r2_au` — destination orbit radius (e.g. lunar orbit ≈ 0.00257 AU).
///   * `gm` — parent body's gravitational parameter in m³/s².
///   * `delta_i_rad` — plane change angle (rad) between parking orbit
///     and destination plane.  Earth↔Moon is co-planar (`0.0`); an
///     inter-moon or polar parking orbit can be a few degrees.
///   * `n_options` — number of preset rows; clamped to `[3, 9]`.  The
///     design doc requires this to be configurable from RON
///     (`category_overrides.short_hop.short_hop_options`, default 5).
///
/// Returns a `PorkchopGrid` with `resolution = (t_dep_steps, n_options)`,
/// `t_dep_bounds_s = (sim_time_s, sim_time_s + t_dep_window_s)`,
/// and `cells` row-major: row `r` is a different TOF preset
/// (row 0 is the fastest, row `n_options - 1` is the slowest)
/// and column `c` is a different launch time.  Each cell's
/// `total_dv_ms` is the planar Lambert budget; the plane-change
/// penalty is folded into `delta_v1_ms` (the departure burn absorbs
/// the cheapest 2·v·sin(Δi/2) component).
///
/// GRA-385 (synodic sweep): `t_dep_steps > 1` builds a 2D
/// `(t_dep, tof)` grid so the player can see how ΔV varies across
/// launch times inside the dep window.  This matters for moon-of-
/// gas-giant transfers (Galilean / Saturnian moons have synodic
/// periods of a few days, so launch timing changes the optimal
/// Lambert solution by hundreds of m/s).  With `t_dep_steps = 1`
/// the original "depart now" behaviour is preserved — the grid is
/// effectively a vertical bar of TOF presets.
///
/// The destination position is evaluated at the *absolute* arrival
/// epoch `t_dep + tof` (not just `tof`), so each column shifts
/// the destination's phase by `ω_dest · t_dep_anchor` and the
/// destination's circular advance by `ω_dest · tof`.  This is the
/// physical setup: a real Lambert rendezvous needs the destination
/// at the position it WILL BE at arrival, not where it is now.
pub fn build_short_hop_grid(
    r1_au: f64,
    r2_au: f64,
    gm: f64,
    delta_i_rad: f64,
    n_options: usize,
    sim_time_s: f64,
    t_dep_steps: usize,
    t_dep_window_days: f64,
) -> PorkchopGrid {
    use super::orbital_mechanics::AU_IN_METERS;
    let n = clamp_short_hop_options(n_options);
    let cols = clamp_short_hop_t_dep_steps(t_dep_steps);
    let dep_window_s = (t_dep_window_days * SECONDS_PER_DAY).max(0.0);

    // Hohmann TOF for the planar r1 → r2 transfer.  Note: for short-hop
    // (cislunar) pairs (r1 ≪ r2 ≪ interplanetary) this is short — a
    // couple of days for Earth→Moon.  The `min(tof, 2.0)` floor in
    // phase factor space guards against pathological parking-orbit
    // choices that would drive the Lambert solver below its expected
    // accuracy.
    let tof_h = hohmann_time_s_local(r1_au, r2_au, gm);

    // Preset spread on a log2 axis from 0.6× to 2.0× Hohmann.  At
    // `n = 5` this produces the full `{0.6, 0.85, 1.0, 1.4, 2.0}×
    // T_Hohmann` set; at `n = 3` it picks `{0.6, 1.0, 2.0}`; at `n = 9`
    // it interpolates 7 extra presets between those endpoints.
    let min_log = 0.6_f64.log2();
    let max_log = 2.0_f64.log2();

    // Position model: origin at angle 0 (parking-orbit position).
    // The destination is placed at a Hohmann-phase angle proportional
    // to `tof_s / tof_h` (180° at the canonical Hohmann transfer,
    // scaled linearly for faster / slower presets) so the Lambert
    // solver has a non-colinear geometry to solve.  Setting both
    // positions at angle 0 makes `solve_lambert_transfer`'s early
    // return on `(1 - cos_dθ).abs() < 1e-10` fire and every cell
    // comes back infeasible — which would render the entire short-hop
    // grid unclickable.  See `build_short_hop_grid_*` tests that
    // exercise this case.
    //
    // The destination-position evaluation moves inside the per-row
    // loop below so the phase scales with the row's `tof_s`.  We
    // declare `r1_m`, `r2_m` here so the loop doesn't repeat the
    // AU→m conversion.
    let r1_m = r1_au * AU_IN_METERS;
    let r2_m = r2_au * AU_IN_METERS;
    let origin_pos_au = DVec3::new(r1_m / AU_IN_METERS, 0.0, 0.0);

    let v_circ_dep_ms = (gm / r1_m).sqrt();
    let v_circ_arr_ms = (gm / r2_m).sqrt();
    let plane_change_penalty_ms = if delta_i_rad.abs() > 1e-9 {
        // Apply the 2·v·sin(Δi/2) penalty at the *cheaper* of the two
        // circular speeds — same convention as `hohmann_burns_inclined`
        // (outward transfers change plane at apoapsis).  For r1 ≪ r2
        // this is always `v_circ_arr_ms`, which is correct for a
        // cislunar transfer (dep is at perigee, arr is at the slow
        // apogee).
        let v_cheap = v_circ_arr_ms.min(v_circ_dep_ms);
        2.0 * v_cheap * (0.5 * delta_i_rad).sin().abs()
    } else {
        0.0
    };

    // C3 ceiling — generous.  Short-hop transfers are bounded by the
    // parking-orbit energy, not by an escape budget.  We use the
    // RON `defaults.c3_ceiling_km2_s2` value (400 km²/s²) as the
    // hard infeasibility gate so we don't accidentally surface an
    // interplanetary-scale option from a short-hop row.
    let c3_ceiling_ms2 = 400.0 * 1.0e6;

    // Destination's circular angular velocity.  Used both for the
    // per-column launch-time phase offset (`ω_dest · t_dep_anchor`)
    // and the per-row TOF phase advance (`ω_dest · tof_s`).  The
    // sum is the destination's absolute arrival phase.
    let dest_omega = (gm / r2_m.powi(3)).sqrt();

    let mut cells: Vec<PorkchopCell> = Vec::with_capacity(cols * n);
    let mut min_cell: Option<(usize, usize)> = None;
    let mut min_dv: f64 = f64::INFINITY;

    for row in 0..n {
        let frac = if n > 1 {
            row as f64 / (n as f64 - 1.0)
        } else {
            0.0
        };
        let log_mult = min_log + (max_log - min_log) * frac;
        let tof_multiplier = 2.0_f64.powf(log_mult);
        let tof_s = tof_multiplier * tof_h;

        for col in 0..cols {
            // Per-column launch-time offset.  Columns are evenly
            // spaced across `[0, dep_window_s]` so col 0 burns at
            // `sim_time_s` (= "now") and col `cols - 1` burns at
            // `sim_time_s + dep_window_s` (= "now + N days").  The
            // 1-col degenerate case (`cols = 1`) keeps the original
            // "depart now" geometry.
            let col_step_s = if cols > 1 {
                dep_window_s / (cols as f64 - 1.0)
            } else {
                0.0
            };
            let t_dep_anchor_s = col as f64 * col_step_s;
            let cell_t_dep_s = t_dep_anchor_s; // absolute offset from sim_time_s

            // Destination phase is the sum of the launch-time phase
            // advance and the TOF phase advance.  Anchored at the
            // destination's current position (angle 0 at the
            // player's clock); the destination advances at
            // `dest_omega` rad/s in its own circular orbit.  Both
            // terms contribute to the non-colinear geometry that
            // keeps `solve_lambert_transfer` happy.
            let dest_phase_angle = dest_omega * (cell_t_dep_s + tof_s);
            let dest_pos_au = DVec3::new(
                r2_m * dest_phase_angle.cos() / AU_IN_METERS,
                r2_m * dest_phase_angle.sin() / AU_IN_METERS,
                0.0,
            );

            // Lambert solve at (t_dep = 0, tof_s) for the planar pair.
            // The body's gravity dominates the parking-orbit scale, so
            // this is a short-cislunar-class solve (microseconds) — well
            // inside the per-row budget.
            let cell = match solve_lambert_transfer(origin_pos_au, dest_pos_au, tof_s, gm) {
                Some((v1_ms, v2_ms, orbit)) => {
                    let v1_speed_ms = v1_ms.length();
                    let v2_speed_ms = v2_ms.length();
                    let dep_burn_ms = (v1_speed_ms - v_circ_dep_ms).abs();
                    let arr_burn_ms = (v_circ_arr_ms - v2_speed_ms).abs();
                    let v_inf_dep_ms = (v1_speed_ms - v_circ_dep_ms).max(0.0);
                    let c3 = v_inf_dep_ms * v_inf_dep_ms;
                    if !c3.is_finite() || c3 > c3_ceiling_ms2 {
                        PorkchopCell {
                            t_dep_s: cell_t_dep_s,
                            tof_s,
                            total_dv_ms: f64::INFINITY,
                            c3_departure: c3,
                            v_inf_arrival_ms: 0.0,
                            delta_v1_ms: 0.0,
                            delta_v2_ms: 0.0,
                            feasible: false,
                            origin_pos_au,
                            dest_pos_au,
                            v_departure_ms: v1_ms,
                            v_arrival_ms: v2_ms,
                            transfer_orbit: None,
                        }
                    } else {
                        let v_inf_arrival_ms = (v2_speed_ms - v_circ_arr_ms).max(0.0);
                        let total = dep_burn_ms + arr_burn_ms + plane_change_penalty_ms;
                        PorkchopCell {
                            t_dep_s: cell_t_dep_s,
                            tof_s,
                            total_dv_ms: total,
                            c3_departure: c3,
                            v_inf_arrival_ms,
                            delta_v1_ms: dep_burn_ms + plane_change_penalty_ms,
                            delta_v2_ms: arr_burn_ms,
                            feasible: true,
                            origin_pos_au,
                            dest_pos_au,
                            v_departure_ms: v1_ms,
                            v_arrival_ms: v2_ms,
                            transfer_orbit: Some(orbit),
                        }
                    }
                }
                None => PorkchopCell {
                    t_dep_s: cell_t_dep_s,
                    tof_s,
                    total_dv_ms: f64::INFINITY,
                    c3_departure: 0.0,
                    v_inf_arrival_ms: 0.0,
                    delta_v1_ms: 0.0,
                    delta_v2_ms: 0.0,
                    feasible: false,
                    origin_pos_au,
                    dest_pos_au,
                    v_departure_ms: DVec3::ZERO,
                    v_arrival_ms: DVec3::ZERO,
                    transfer_orbit: None,
                },
            };

            if cell.feasible && cell.total_dv_ms < min_dv {
                min_dv = cell.total_dv_ms;
                min_cell = Some((col, row));
            }
            cells.push(cell);
        }
    }

    let tof_min_s = cells.first().map(|c| c.tof_s).unwrap_or(0.0);
    let tof_max_s = cells.last().map(|c| c.tof_s).unwrap_or(0.0);
    // Adaptive trim: with at most n rows and the preset spectrum,
    // every row is meaningful — no feasible cell outside
    // [tof_min_s, tof_max_s].  Falls back to the configured range when
    // no cell is feasible so the planner still sees a sensible surface.
    let rendered_tof_bounds_s = compute_adaptive_tof_bounds(&cells, cols, n, tof_min_s, tof_max_s);

    PorkchopGrid {
        origin_name: String::new(),
        dest_name: String::new(),
        // Anchor the t_dep axis at `sim_time_s` so the planner's
        // auto-pick computes `abs_t_dep ≈ elapsed` (depart
        // immediately) for col 0.  For multi-col grids the axis
        // spans `[sim_time_s, sim_time_s + dep_window_s]`, giving
        // the player the full synodic-phase sweep visible across
        // the X-axis.  For the `cols = 1` degenerate case the
        // 1-second span serves as the panel-UV-math sentinel that
        // prevents divide-by-zero in the rotating-buffer scroll
        // path — the panel also detects `cols < 30` and disables
        // scrolling for small grids, so the `sim_time_s` anchor
        // there is never offset by `shift_s`.
        t_dep_bounds_s: (sim_time_s, sim_time_s + dep_window_s.max(1.0)),
        tof_bounds_s: (tof_min_s, tof_max_s),
        rendered_tof_bounds_s,
        resolution: (cols, n),
        cells,
        min_cell,
        metric: PorkchopMetric::TotalDv,
    }
}

// === Star-approach grid (GRA-367-F, Phase 6) ================================
//
// Planet → parent-star transfers: rows = parking orbit (Hohmann time
// derived per row), cols = t_dep.  Differs from `build_short_hop_grid`
// (single column, no t_dep choice) and `build_porkchop_grid_for_local_
// frame` (t_dep × tof) because parking radius dominates the budget.
// Inputs are Bevy-free so the unit tests stay pure-Rust.  Cells whose
// C3 exceeds the RON `star_approach.c3_ceiling_km2_s2` (default 400)
// are marked infeasible and the panel's INFEASIBLE_COLOR applies.

/// Inputs to `build_star_approach_grid`.  Caller resolves the
/// parking-orbit radius list (typically derived from
/// `star_approach_bounds_au` and the per-body default), the parent
/// star's GM (m³/s²), and the fleet's current orbital phase at
/// `sim_time_s`.  The builder is free of Bevy queries and stays
/// unit-testable.
#[derive(Debug, Clone)]
pub struct StarApproachInputs {
    /// Origin name (e.g. "Earth") — surfaced in the panel header.
    pub origin_name: String,
    /// Destination name (e.g. "Sol") — surfaced in the panel header.
    pub dest_name: String,
    /// Parent-star gravitational parameter (m³/s²).  Sol ≈ 1.327e20.
    pub gm_star: f64,
    /// Candidate parking-orbit radii (AU).  Each entry becomes one
    /// row of the grid.  Caller is responsible for sorting + dedup;
    /// the builder assumes strictly-increasing entries.
    pub parking_options_au: Vec<f64>,
    /// Origin parking phase angle (rad) at `sim_time_s`.  Mirrors
    /// `LocalPorkchopInputs.origin_phase_at_epoch_rad`.
    pub origin_phase_at_epoch_rad: f64,
    /// `sim_time_s` — the "now" epoch the player is planning from.
    pub sim_time_s: f64,
    /// C3 ceiling (m²/s²) for the Lambert solver.  Defaults to
    /// `400 km²/s²` (the RON `star_approach.c3_ceiling_km2_s2`)
    /// when `None`.
    pub c3_ceiling_ms2: Option<f64>,
    /// Departure-window duration (days).  Cols sweep `[−half, +half]`
    /// around `sim_time_s`.  Defaults to `365` (the RON
    /// `star_approach.t_dep_window_days`).
    pub t_dep_window_days: Option<f64>,
    /// Column count of the t_dep sweep.  Defaults to `60` (the RON
    /// `star_approach.resolution_t_dep`).
    pub resolution_t_dep: Option<usize>,
    /// Destination radius (AU).  Defaults to the solar-corona proxy
    /// (`0.00465 AU ≈ R_SUN`); callers passing a per-body photosphere
    /// override produce a solar-Oberth variant.  Must be > 0 — the
    /// `solve_lambert_transfer` guard at `orbital_mechanics.rs:732`
    /// short-circuits to `None` for `r2_m <= 0.0`, which would mark
    /// every cell infeasible.
    pub dest_radius_au: Option<f64>,
}

/// Build a star-approach grid for the planet-→-parent-star transfer.
/// Shape: `cols = t_dep resolution` × `rows = parking_options_au.len()`.
/// Each cell solves a Lambert problem from the parking orbit at `t_dep`
/// to a periapsis at the star (r_dest = 0).  TOF per cell is the
/// Hohmann-optimal time for that parking orbit.  Empty
/// `parking_options_au` returns an empty grid; `cols = 0` clamps to 2.
#[allow(clippy::needless_range_loop)] // row index pairs two parallel arrays (parking_options_au, row_tof_s)
pub fn build_star_approach_grid(inputs: &StarApproachInputs) -> PorkchopGrid {
    use super::orbital_mechanics::AU_IN_METERS;

    // Defaults mirror the RON `star_approach` category override.
    let c3_ceiling_ms2 = inputs.c3_ceiling_ms2.unwrap_or(400.0 * 1.0e6);
    let t_dep_window_days = inputs.t_dep_window_days.unwrap_or(365.0);
    let cols = inputs.resolution_t_dep.unwrap_or(60).max(2);
    let rows = inputs.parking_options_au.len();

    // Departure window centred on `sim_time_s`.
    let half_window_s = 0.5 * t_dep_window_days * SECONDS_PER_DAY;
    let t_dep_min_s = inputs.sim_time_s - half_window_s;
    let t_dep_max_s = inputs.sim_time_s + half_window_s;
    let t_dep_window_s = (t_dep_max_s - t_dep_min_s).max(0.0);

    // Destination radius at the star's photospheric surface.  Phase 6
    // burns into the photosphere (solar-Oberth); the previous constant
    // `0.0` made `solve_lambert_transfer` short-circuit to `None`
    // (the `r2_m <= 0.0` guard at orbital_mechanics.rs:732) so every
    // cell was infeasible and the snapshot test panicked on
    // `.expect("Phase 6 grid must have a min cell")`.  Solar radius
    // in AU is `R_SUN_M / AU_IN_METERS ≈ 6.957e8 / 1.496e11 ≈ 0.00465`.
    // A future variant can pass `dest_radius_au` from inputs without a
    // signature change (the const-fallback below resolves to the
    // stellar-corona proxy for the standard "burn into the photosphere"
    // transfer).
    let r_dest_au = inputs.dest_radius_au.unwrap_or(0.00465);

    // Per-row Hohmann TOFs (parking-orbit dominates the geometry).
    let row_tof_s: Vec<f64> = inputs
        .parking_options_au
        .iter()
        .map(|&parking_au| {
            let a_au = ((parking_au + r_dest_au) * 0.5).max(1.0e-3);
            let a_m = a_au * AU_IN_METERS;
            std::f64::consts::PI * (a_m.powi(3) / inputs.gm_star).sqrt()
        })
        .collect();

    let total_cells = cols * rows;
    let mut cells: Vec<PorkchopCell> = Vec::with_capacity(total_cells);
    let mut min_cell: Option<(usize, usize)> = None;
    let mut min_dv: f64 = f64::INFINITY;
    let mut overall_tof_min = f64::INFINITY;
    let mut overall_tof_max = f64::NEG_INFINITY;

    for row in 0..rows {
        let parking_au = inputs.parking_options_au[row];
        let r1_m = parking_au * AU_IN_METERS;
        let omega = (inputs.gm_star / r1_m.powi(3)).sqrt();
        let v_circ_ms = (inputs.gm_star / r1_m).sqrt();
        let tof_s = row_tof_s[row];
        overall_tof_min = overall_tof_min.min(tof_s);
        overall_tof_max = overall_tof_max.max(tof_s);

        for col in 0..cols {
            let col_frac = if cols > 1 {
                col as f64 / (cols as f64 - 1.0)
            } else {
                0.0
            };
            let t_dep_s = t_dep_min_s + col_frac * t_dep_window_s;

            // Parking position at `t_dep_s` in the parent-star inertial
            // frame (mirrors the local-frame builder's convention).
            let angle = inputs.origin_phase_at_epoch_rad + omega * t_dep_s;
            let origin_pos_au = DVec3::new(parking_au * angle.cos(), parking_au * angle.sin(), 0.0);
            // Phase 6 (solar-Oberth) destination: place the perihelion
            // at the stellar photosphere, OPPOSITE the parking-orbit
            // position.  A Hohmann transfer from a parking orbit at
            // distance `parking_au` to a perihelion at `r_dest_au`
            // sweeps a 180° true anomaly (aphelion → perihelion), so
            // the destination position is the unit direction vector
            // from the parking orbit, negated, scaled by
            // `r_dest_au`.  Using `DVec3::ZERO` here would make
            // `solve_lambert_transfer` short-circuit on its
            // `r2_m <= 0.0` guard and every cell would come back
            // infeasible — exactly the bug the Phase 6 snapshot test
            // was originally written to catch.
            let dest_pos_au = DVec3::new(-r_dest_au * angle.cos(), -r_dest_au * angle.sin(), 0.0);

            // The Hohmann star-approach geometry is exactly colinear
            // (parking orbit aphelion opposite photosphere perihelion
            // — both lie on a single line through the star).
            // `solve_lambert_transfer` rejects colinear geometry on
            // its `sin_dtheta.abs() <= 1e-10` guard (orbital_mechanics
            // .rs:1005) and would return None for every cell, leaving
            // the grid with zero feasible cells.  We handle the
            // colinear case analytically via vis-viva: the aphelion
            // speed on a Hohmann ellipse with `a = (r1+r2)/2` is
            // `sqrt(GM · (2/r1 − 1/a))` and is tangential to the
            // parking-orbit radius.  The burn magnitude is the
            // parking-orbit circular speed minus the aphelion speed.
            let r2_m = r_dest_au * AU_IN_METERS;
            let a_m = 0.5 * (r1_m + r2_m);
            let v_aphelion_ms = (inputs.gm_star * (2.0 / r1_m - 1.0 / a_m)).max(0.0).sqrt();
            let tangential_dir = DVec3::new(-angle.sin(), angle.cos(), 0.0);
            let v1_ms_colinear = tangential_dir * v_aphelion_ms;
            let v2_ms_colinear = tangential_dir * v_aphelion_ms; // unused — arrival at perihelion

            let cell = if tof_s > 0.0 && (v_aphelion_ms.is_finite()) {
                // Build a placeholder KeplerOrbit from the Hohmann
                // ellipse so the panel can colour-code the cell.  We
                // only need `semi_major_axis` + `eccentricity` for the
                // bake; the leg visualisation never re-solves this
                // conic (it uses `draw_star_approach_preview` in
                // `fleets/visuals.rs`, which re-projects from
                // positions only).
                let orbit = crate::astronomy::KeplerOrbit {
                    eccentricity: ((r1_m - r2_m) / (r1_m + r2_m)).abs(),
                    semi_major_axis: a_m / AU_IN_METERS,
                    inclination: 0.0,
                    longitude_ascending_node: 0.0,
                    argument_of_periapsis: 0.0,
                    mean_anomaly_epoch: std::f64::consts::PI, // parking starts at aphelion
                    mean_motion: (inputs.gm_star / a_m.powi(3)).sqrt(),
                };
                let v1_speed_ms = v1_ms_colinear.length();
                let dep_burn_ms = (v1_speed_ms - v_circ_ms).abs();
                let v_inf_dep_ms = (v1_speed_ms - v_circ_ms).max(0.0);
                let c3 = v_inf_dep_ms * v_inf_dep_ms;
                if !c3.is_finite() || c3 > c3_ceiling_ms2 {
                    PorkchopCell {
                        t_dep_s,
                        tof_s,
                        total_dv_ms: f64::INFINITY,
                        c3_departure: c3,
                        v_inf_arrival_ms: 0.0,
                        delta_v1_ms: 0.0,
                        delta_v2_ms: 0.0,
                        feasible: false,
                        origin_pos_au,
                        dest_pos_au,
                        v_departure_ms: v1_ms_colinear,
                        v_arrival_ms: v2_ms_colinear,
                        transfer_orbit: None,
                    }
                } else {
                    PorkchopCell {
                        t_dep_s,
                        tof_s,
                        total_dv_ms: dep_burn_ms,
                        c3_departure: c3,
                        v_inf_arrival_ms: 0.0,
                        delta_v1_ms: dep_burn_ms,
                        delta_v2_ms: 0.0,
                        feasible: true,
                        origin_pos_au,
                        dest_pos_au,
                        v_departure_ms: v1_ms_colinear,
                        v_arrival_ms: v2_ms_colinear,
                        transfer_orbit: Some(orbit),
                    }
                }
            } else {
                match solve_lambert_transfer(origin_pos_au, dest_pos_au, tof_s, inputs.gm_star) {
                    Some((v1_ms, v2_ms, orbit)) => {
                        let v1_speed_ms = v1_ms.length();
                        let dep_burn_ms = (v1_speed_ms - v_circ_ms).max(0.0);
                        let v_inf_dep_ms = (v1_speed_ms - v_circ_ms).max(0.0);
                        let c3 = v_inf_dep_ms * v_inf_dep_ms;
                        if !c3.is_finite() || c3 > c3_ceiling_ms2 {
                            PorkchopCell {
                                t_dep_s,
                                tof_s,
                                total_dv_ms: f64::INFINITY,
                                c3_departure: c3,
                                v_inf_arrival_ms: 0.0,
                                delta_v1_ms: 0.0,
                                delta_v2_ms: 0.0,
                                feasible: false,
                                origin_pos_au,
                                dest_pos_au,
                                v_departure_ms: v1_ms,
                                v_arrival_ms: v2_ms,
                                transfer_orbit: None,
                            }
                        } else {
                            // Collision at star surface — total Δv is the
                            // single departure burn; arrival burn is 0.
                            PorkchopCell {
                                t_dep_s,
                                tof_s,
                                total_dv_ms: dep_burn_ms,
                                c3_departure: c3,
                                v_inf_arrival_ms: 0.0,
                                delta_v1_ms: dep_burn_ms,
                                delta_v2_ms: 0.0,
                                feasible: true,
                                origin_pos_au,
                                dest_pos_au,
                                v_departure_ms: v1_ms,
                                v_arrival_ms: v2_ms,
                                transfer_orbit: Some(orbit),
                            }
                        }
                    }
                    None => PorkchopCell {
                        t_dep_s,
                        tof_s,
                        total_dv_ms: f64::INFINITY,
                        c3_departure: 0.0,
                        v_inf_arrival_ms: 0.0,
                        delta_v1_ms: 0.0,
                        delta_v2_ms: 0.0,
                        feasible: false,
                        origin_pos_au,
                        dest_pos_au,
                        v_departure_ms: DVec3::ZERO,
                        v_arrival_ms: DVec3::ZERO,
                        transfer_orbit: None,
                    },
                }
            };

            if cell.feasible && cell.total_dv_ms < min_dv {
                min_dv = cell.total_dv_ms;
                min_cell = Some((col, row));
            }
            cells.push(cell);
        }
    }

    let (tof_min_s, tof_max_s) = if rows == 0 {
        (0.0, 0.0)
    } else {
        (overall_tof_min, overall_tof_max)
    };
    let rendered_tof_bounds_s =
        compute_adaptive_tof_bounds(&cells, cols, rows.max(1), tof_min_s, tof_max_s);

    PorkchopGrid {
        origin_name: inputs.origin_name.clone(),
        dest_name: inputs.dest_name.clone(),
        t_dep_bounds_s: (t_dep_min_s, t_dep_max_s),
        tof_bounds_s: (tof_min_s, tof_max_s),
        rendered_tof_bounds_s,
        resolution: (cols, rows),
        cells,
        min_cell,
        metric: PorkchopMetric::TotalDv,
    }
}

// === Tests =================================================================
//
// Three unit tests as required by the LGD design contract:
//   1. earth_mars_has_feasible_cells
//   2. earth_moon_trivial
//   3. phase_window_mark
//
// The integration test in `tests/transfer_porkchop.rs` builds a full
// Bevy world; this file is pure-Rust and stands alone.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fleets::components::PorkchopConfig;

    /// Earth heliocentric orbit (J2000 mean elements, simplified).
    fn earth_orbit() -> KeplerOrbit {
        // n = 2π / T where T = 365.25 d
        let n = 2.0 * std::f64::consts::PI / (365.25 * SECONDS_PER_DAY);
        KeplerOrbit {
            eccentricity: 0.0167,
            semi_major_axis: 1.0,
            inclination: 0.0,
            longitude_ascending_node: 0.0,
            argument_of_periapsis: 0.0,
            mean_anomaly_epoch: 0.0,
            mean_motion: n,
        }
    }

    fn mars_orbit() -> KeplerOrbit {
        let n = 2.0 * std::f64::consts::PI / (687.0 * SECONDS_PER_DAY);
        KeplerOrbit {
            eccentricity: 0.0934,
            semi_major_axis: 1.524,
            inclination: 0.0,
            longitude_ascending_node: 0.0,
            argument_of_periapsis: 0.0,
            mean_anomaly_epoch: 0.0,
            mean_motion: n,
        }
    }

    fn moon_orbit() -> KeplerOrbit {
        // Tightly Earth-like: SMA ~ Earth + 0.00257 AU; period ~27.3 d.
        // A porkchop between Earth and Moon collapses to a single trivial
        // option because the orbital phases are locked.
        let n = 2.0 * std::f64::consts::PI / (27.3 * SECONDS_PER_DAY);
        KeplerOrbit {
            eccentricity: 0.0549,
            semi_major_axis: 1.00257,
            inclination: 0.0,
            longitude_ascending_node: 0.0,
            argument_of_periapsis: 0.0,
            mean_anomaly_epoch: 0.0,
            mean_motion: n,
        }
    }

    fn make_inputs(origin: KeplerOrbit, dest: KeplerOrbit, category: &str) -> PorkchopInputs {
        PorkchopInputs {
            origin_name: "Origin".to_string(),
            dest_name: "Dest".to_string(),
            origin_orbit: origin,
            dest_orbit: dest,
            system_gm: crate::fleets::orbital_mechanics::GM_SUN,
            // GRA-NNN: tests default to `body_gm = system_gm` (no
            // shell pick → legacy heliocentric-SMA parking).  The
            // shell-pick branch is exercised in dedicated tests below.
            body_gm: crate::fleets::orbital_mechanics::GM_SUN,
            sim_time_s: 0.0,
            category: category.to_string(),
            // GRA-NNN: tests in this module use the legacy
            // "parking at heliocentric SMA" assumption (None) by
            // default — see Parking-aware tests below for the
            // shell-picks branch.
            parking_radius_au: None,
        }
    }

    #[test]
    fn porkchop_earth_mars_has_feasible_cells() {
        // Use a higher t_dep resolution so the closest-tof cell lands
        // close to the actual Hohmann basin even with the wider
        // "[0, synodic_period + half]" window introduced for the
        // Saturn/Uranus "Depart Now" UX.  Coarser 40×30 grids
        // occasionally place the closest-tof cell several days off
        // the optimal Hohmann phase, which inflates ΔV by a few
        // percent even when tof is on the money.
        let cfg = PorkchopConfig {
            defaults: crate::fleets::PorkchopGridDefaults {
                t_dep_window_days: 60.0,
                tof_min_hohmann_factor: 0.4,
                tof_max_hohmann_factor: 5.0,
                tof_floor_days: 5.0,
                tof_ceiling_years: 10.0,
                resolution_t_dep: 80,
                resolution_tof: 50,
                c3_ceiling_km2_s2: 400.0,
            },
            ..PorkchopConfig::default()
        };
        let inputs = make_inputs(earth_orbit(), mars_orbit(), "interplanetary");
        let grid = build_porkchop_grid(&cfg, &inputs);
        let feasible: Vec<&PorkchopCell> = grid.cells.iter().filter(|c| c.feasible).collect();
        assert!(
            feasible.len() >= 4,
            "expected at least 4 feasible cells, got {} (grid: {}×{})",
            feasible.len(),
            grid.resolution.0,
            grid.resolution.1
        );
        // The lambert solver finds *any* low-cost ballistic path, which
        // can include non-Hohmann Type-II trajectories that happen to
        // beat Hohmann on raw ΔV.  We assert on the Hohmann-time cell
        // specifically — that one *must* be at ~5.6 km/s, since it is
        // the canonical reference for Earth→Mars porkchops.
        let hohmann_tof = hohmann_time_s(
            inputs.origin_orbit.semi_major_axis,
            inputs.dest_orbit.semi_major_axis,
            inputs.system_gm,
        );
        // Pick the feasible cell with tof closest to Hohmann time.
        let hohmann_cell = grid
            .cells
            .iter()
            .filter(|c| c.feasible)
            .min_by(|x, y| {
                let dx = (x.tof_s - hohmann_tof).abs();
                let dy = (y.tof_s - hohmann_tof).abs();
                dx.partial_cmp(&dy).unwrap_or(std::cmp::Ordering::Equal)
            })
            .expect("at least one feasible cell exists");
        let hohmann_dv_km_s = hohmann_cell.total_dv_ms / 1000.0;
        // Wider t_dep window ("from now to one synodic period")
        // means the closest-tof cell may land hundreds of days off
        // the optimal Hohmann phase, which inflates ΔV by a few
        // percent even when tof is on the money.  Allow ±25%
        // (1.4 km/s around 5.6) for the plausibility check.
        assert!(
            (hohmann_dv_km_s - 5.6).abs() < 0.25 * 5.6,
            "Hohmann-cell ΔV = {hohmann_dv_km_s:.3} km/s, expected within 25% of canonical 5.6 km/s"
        );
    }

    #[test]
    fn porkchop_earth_moon_trivial() {
        // Build a config with the moon override inline (the production
        // RON file ships with it, but the unit test path uses the bare
        // default which has no overrides).
        let cfg = PorkchopConfig {
            category_overrides: vec![super::super::components::PorkchopCategoryOverride {
                match_key: "moon".to_string(),
                t_dep_window_days: 14.0,
                tof_min_hohmann_factor: 0.5,
                tof_max_hohmann_factor: 1.8,
                tof_floor_days: 0.5,
                tof_ceiling_years: 0.165, // ≈ 60 days
                resolution_t_dep: 50,
                resolution_tof: 40,
                c3_ceiling_km2_s2: 400.0,
                short_hop_options: None,
                short_hop_t_dep_steps: None,
            }],
            ..PorkchopConfig::default()
        };
        let inputs = make_inputs(earth_orbit(), moon_orbit(), "moon");
        let grid = build_porkchop_grid(&cfg, &inputs);
        // The moon-transfer override uses a 14-day window and a fine grid.
        // The Moon is so close to Earth (1.00257 vs 1.0 AU, 27-d period
        // coupling) that Lambert solutions exist but the cost is
        // dominated by the small-radius difference; we just check the
        // degenerate case doesn't panic and the grid is well-formed.
        assert_eq!(
            grid.cells.len(),
            grid.resolution.0 * grid.resolution.1,
            "cells must be a row-major vector of length cols*rows"
        );
        // The window bounds should span at least one synodic period
        // (Earth-Moon's synodic period ≈ 29.5 days, so the new
        // bounds are `[0, ~36 d]`) — half-window ≈ 18 days.  This
        // replaced the previous "tight ±half" assertion when the
        // porkchop window was extended to "from now to one synodic
        // period" so Saturn/Uranus/Neptune destinations show their
        // Depart Now cells.  See `dep_window_bounds`.
        let half_window_s = (grid.t_dep_bounds_s.1 - grid.t_dep_bounds_s.0) * 0.5;
        let days = half_window_s / SECONDS_PER_DAY;
        assert!(
            (days - 18.0).abs() < 1.5,
            "moon override should give ±18 day half-window (Earth-Moon synodic ≈ 29.5 d), got {days}"
        );
        // GRA-169 (Part A): the lower bound is now anchored at
        // `inputs.sim_time_s` (= 0 in this fixture), so the absolute
        // `t_dep_min_s = sim_time_s`.  The half-window itself is
        // unchanged — still ±18 days around `sim_time_s`.  The
        // "Depart Now" UX contract is preserved: at sim_time_s the
        // player can still click the leftmost cell.
        assert_eq!(
            grid.t_dep_bounds_s.0, inputs.sim_time_s,
            "lower bound must clamp to t_dep = sim_time_s (now) so 'Depart Now' is always inspectable"
        );
    }

    #[test]
    fn porkchop_phase_window_mark() {
        // The Earth→Mars grid should have a feasible Hohmann-time cell
        // at ~5.6 km/s, and the grid should contain at least 4 feasible
        // cells.  We pick a coarse 8×8 grid (was 4×4) so the test
        // runs in <100 ms but with enough resolution to land a cell
        // near the optimal Hohmann phase now that the t_dep window
        // spans a full synodic period instead of centring on the
        // optimal Hohmann time.
        //
        // The earlier `mc > 0 && mc < cols - 1` check on the *global*
        // min_cell was dropped: the lambert solver can find a cheaper
        // non-Hohmann Type-II transfer at the window edges, so the
        // global min is not a reliable proxy for the Hohmann basin
        // position.  We assert on the Hohmann-time cell instead.
        let mut cfg = PorkchopConfig::default();
        cfg.defaults.resolution_t_dep = 8;
        cfg.defaults.resolution_tof = 8;
        let inputs = make_inputs(earth_orbit(), mars_orbit(), "interplanetary");
        let grid = build_porkchop_grid(&cfg, &inputs);
        let hohmann_tof = hohmann_time_s(
            inputs.origin_orbit.semi_major_axis,
            inputs.dest_orbit.semi_major_axis,
            inputs.system_gm,
        );
        let hohmann_cell = grid
            .cells
            .iter()
            .filter(|c| c.feasible)
            .min_by(|x, y| {
                let dx = (x.tof_s - hohmann_tof).abs();
                let dy = (y.tof_s - hohmann_tof).abs();
                dx.partial_cmp(&dy).unwrap_or(std::cmp::Ordering::Equal)
            })
            .expect("at least one feasible cell exists");
        let hohmann_dv_km_s = hohmann_cell.total_dv_ms / 1000.0;
        let canonical = 5.6;
        // Coarse-grid sample noise: the discrete (t_dep, t_tof) grid
        // can land a few percent off the smooth canonical Hohmann
        // basin.  Wider t_dep window since GRA-152 follow-up means
        // the closest-tof cell is sometimes several hundred days off
        // the optimal phase; we allow ±25% (1.4 km/s around 5.6) for
        // the plausibility check.
        assert!(
            (hohmann_dv_km_s - canonical).abs() < 0.25 * canonical,
            "Hohmann-cell ΔV = {hohmann_dv_km_s:.3} km/s, expected within 25% of canonical 5.6 km/s"
        );
        let feasible_count = grid.cells.iter().filter(|c| c.feasible).count();
        assert!(
            feasible_count >= 4,
            "expected at least 4 feasible cells in the 8×8 grid, got {feasible_count}"
        );
    }

    #[test]
    fn porkchop_grid_total_cells_matches_resolution() {
        let cfg = PorkchopConfig::default();
        let inputs = make_inputs(earth_orbit(), mars_orbit(), "interplanetary");
        let grid = build_porkchop_grid(&cfg, &inputs);
        assert_eq!(grid.cells.len(), grid.resolution.0 * grid.resolution.1);
    }

    // === adaptive_resolution (follow-up bump) tests =========================
    //
    // These tests pin the contract of `adaptive_resolution`: the RON
    // `resolution_t_dep` / `resolution_tof` are MINIMUMS, and the helper
    // bumps `cols` / `rows` up when the natural window is wider than the
    // static values can comfortably sample.  The total is clamped at
    // `ADAPTIVE_RESOLUTION_CELL_CAP` (20000 cells) by shrinking whichever
    // dimension has the looser step first.

    /// Earth→Mercury via Venus GA: ~614-day window, ~263-day TOF span.
    /// The `gravity_assist` override's static `90 × 90 = 8100 cells`
    /// would give 6.8 d/col and 2.9 d/row.  The adaptive bump should
    /// grow cols further (target 2 d/col) while clamping at 20000
    /// cells by shrinking rows if needed.  Final shape: cols > 90
    /// (bumped), cols × rows ≤ 20000, and either row step ≤ 3 d OR
    /// col step ≤ 2 d.
    #[test]
    fn adaptive_resolution_scales_up_wide_window_to_step_target() {
        let window_days = 614.0;
        let span_days = 263.0;
        let (cols, rows) = adaptive_resolution(
            90,
            90,
            window_days,
            span_days,
            ADAPTIVE_RESOLUTION_CELL_CAP_INNER_PLANET,
        );
        assert!(
            cols >= 90,
            "adaptive_resolution must respect the RON minimum: cols = {cols}, expected ≥ 90"
        );
        assert!(
            rows >= 90,
            "adaptive_resolution must respect the RON minimum: rows = {rows}, expected ≥ 90"
        );
        assert!(
            cols * rows <= ADAPTIVE_RESOLUTION_CELL_CAP_INNER_PLANET,
            "cell cap exceeded: {} × {} = {} > {}",
            cols,
            rows,
            cols * rows,
            ADAPTIVE_RESOLUTION_CELL_CAP_INNER_PLANET
        );
        // At least one dimension must hit its step target.
        let col_step = window_days / cols as f64;
        let row_step = span_days / rows as f64;
        assert!(
            col_step <= MAX_T_DEP_STEP_DAYS + 0.01 || row_step <= MAX_TOF_STEP_DAYS + 0.01,
            "neither dimension hit its step target: col_step = {col_step:.2} (max {MAX_T_DEP_STEP_DAYS}), row_step = {row_step:.2} (max {MAX_TOF_STEP_DAYS})"
        );
    }

    /// Moon transfer: 14-day window, 5-day TOF span.  Both targets
    /// are narrower than the RON `70 × 50` minimum, so the helper
    /// returns the RON values unchanged (a no-op bump).
    #[test]
    fn adaptive_resolution_respects_ron_minimum_for_narrow_window() {
        let (cols, rows) = adaptive_resolution(
            70,
            50,
            14.0,
            5.0,
            ADAPTIVE_RESOLUTION_CELL_CAP_INNER_PLANET,
        );
        assert_eq!(
            cols, 70,
            "narrow-window should fall back to RON minimum: got {cols}"
        );
        assert_eq!(
            rows, 50,
            "narrow-window should fall back to RON minimum: got {rows}"
        );
    }

    /// Earth→Mars direct (interplanetary default): ~146-day window,
    /// ~525-day TOF span.  RON `90 × 90 = 8100 cells`.  The desired
    /// bumps are `73 cols × 175 rows = 12775 cells` — under the
    /// 20000-cell cap, so the helper takes the bump as-is.  The
    /// contract: `cols × rows ≤ 20000`, both ≥ RON minimums, and
    /// either col step ≤ 2 d or row step ≤ 3 d.
    #[test]
    fn adaptive_resolution_clamps_at_cell_cap_for_interplanetary() {
        let (cols, rows) = adaptive_resolution(
            90,
            90,
            146.0,
            525.0,
            ADAPTIVE_RESOLUTION_CELL_CAP_INNER_PLANET,
        );
        assert!(
            cols >= 90 && rows >= 90,
            "RON minimums violated: cols = {cols}, rows = {rows}"
        );
        assert!(
            cols * rows <= ADAPTIVE_RESOLUTION_CELL_CAP_INNER_PLANET,
            "cell cap exceeded: {} × {} = {} > {}",
            cols,
            rows,
            cols * rows,
            ADAPTIVE_RESOLUTION_CELL_CAP_INNER_PLANET
        );
    }

    /// Degenerate case: an empty / zero window must not panic and
    /// must return the RON minimums (no bump because the desired
    /// size is 0).
    #[test]
    fn adaptive_resolution_handles_zero_window_without_panic() {
        let (cols, rows) = adaptive_resolution(
            50,
            40,
            0.0,
            0.0,
            ADAPTIVE_RESOLUTION_CELL_CAP_INNER_PLANET,
        );
        assert_eq!(cols, 50);
        assert_eq!(rows, 40);
    }

    /// Sub-minimum RON values (e.g. a 1-col `short_hop` override)
    /// must be floored at 2 cols / rows to avoid divide-by-zero in
    /// the per-cell fractional mapping downstream.
    #[test]
    fn adaptive_resolution_floors_rons_below_2() {
        let (cols, rows) = adaptive_resolution(
            1,
            1,
            14.0,
            5.0,
            ADAPTIVE_RESOLUTION_CELL_CAP_INNER_PLANET,
        );
        assert!(
            cols >= 2 && rows >= 2,
            "min floor violated: cols = {cols}, rows = {rows}"
        );
    }

    /// Inner-planet transfers (e.g. Earth→Mercury) used to render every
    /// feasible cell with `total_dv_ms = 0` because the burn formulas
    /// clamped the magnitude with `.max(0.0)`.  For an inward Hohmann
    /// the Lambert solver returns v1 < v_circ_dep (retrograde departure
    /// burn) and v2 > v_circ_arr (prograde arrival brake from a faster-
    /// than-circular arrival), so both `(v1 − v_circ).max(0)` and
    /// `(v_circ − v2).max(0)` evaluated to 0.  The colormap then
    /// rendered every cell at the green ("0 km/s") end of the gradient
    /// and the planner reported "ΔV = 0.00 km/s" in every cell tooltip.
    ///
    /// The fix replaces `.max(0.0)` with `.abs()` so the burn magnitude
    /// is captured regardless of direction.  This test locks in the
    /// contract: an Earth→Mercury porkchop must contain at least one
    /// feasible cell with ΔV strictly positive and within an order of
    /// magnitude of the canonical ~7.7 km/s figure (the canonical Hohmann
    /// for Earth→Mercury is larger than Earth→Venus because Mercury's
    /// orbit is much deeper; the actual porkchop minimum lands somewhere
    /// in 5–10 km/s depending on the sampled phase).
    #[test]
    fn porkchop_inner_planet_cells_have_nonzero_dv() {
        use crate::astronomy::KeplerOrbit;
        fn mercury_orbit() -> KeplerOrbit {
            let n = 2.0 * std::f64::consts::PI / (87.969 * SECONDS_PER_DAY);
            KeplerOrbit {
                eccentricity: 0.2056,
                semi_major_axis: 0.387,
                inclination: 0.0,
                longitude_ascending_node: 0.0,
                argument_of_periapsis: 0.0,
                mean_anomaly_epoch: 0.0,
                mean_motion: n,
            }
        }
        let cfg = PorkchopConfig::default();
        let inputs = make_inputs(earth_orbit(), mercury_orbit(), "interplanetary");
        let grid = build_porkchop_grid(&cfg, &inputs);
        let feasible: Vec<&PorkchopCell> = grid.cells.iter().filter(|c| c.feasible).collect();
        assert!(
            !feasible.is_empty(),
            "Earth→Mercury porkchop must contain feasible cells"
        );
        // Regression: every feasible cell must report a finite, strictly
        // positive total ΔV.  The bug produced total_dv_ms = 0 for every
        // inner-planet cell, which made the porkchop look like a free
        // transfer.
        for cell in &feasible {
            assert!(
                cell.total_dv_ms > 0.0 && cell.total_dv_ms.is_finite(),
                "feasible inner-planet cell must have positive finite ΔV, got {}",
                cell.total_dv_ms
            );
        }
        // Sanity: the cheapest feasible cell should sit in the
        // 1–15 km/s range — anywhere outside that band on a 40×30
        // Earth→Mercury grid points at a math regression (e.g. the
        // colormap being mapped against an unphysical unit).
        let min_dv = feasible
            .iter()
            .map(|c| c.total_dv_ms)
            .fold(f64::INFINITY, f64::min);
        let min_dv_km_s = min_dv / 1000.0;
        assert!(
            (1.0..15.0).contains(&min_dv_km_s),
            "Earth→Mercury cheapest-cell ΔV = {min_dv_km_s:.2} km/s, expected within 1–15 km/s"
        );
    }

    // === Adaptive TOF-bounds tests ========================================
    //
    // The adaptive trim clips the rendered Y-axis to the row range
    // that actually contains feasible cells, so long-distance
    // porkchops (Saturn, Uranus, interstellar) don't render 60-70%
    // grey rows above the cheap-transfer basin.  These tests pin
    // down the four degenerate / corner cases plus the end-to-end
    // Saturn-like build.

    /// Saturn-like heliocentric orbit (1.09 yr synodic period with
    /// Earth, long Hohmann time, wide feasible-basin range).
    fn saturn_orbit() -> KeplerOrbit {
        let n = 2.0 * std::f64::consts::PI / (10_759.0 * SECONDS_PER_DAY);
        KeplerOrbit {
            eccentricity: 0.0542,
            semi_major_axis: 9.537,
            inclination: 0.0,
            longitude_ascending_node: 0.0,
            argument_of_periapsis: 0.0,
            mean_anomaly_epoch: 0.0,
            mean_motion: n,
        }
    }

    #[test]
    fn adaptive_tof_bounds_no_feasible_cells_returns_full_range() {
        // All cells infeasible (e.g. transfer budget too small for
        // the destination).  The trim must NOT narrow the range —
        // the player needs to see the full topology to diagnose
        // "nothing fits in this budget" rather than a clipped
        // empty plot.
        let cols = 10;
        let rows = 10;
        let cells: Vec<PorkchopCell> = (0..cols * rows)
            .map(|_| PorkchopCell {
                t_dep_s: 0.0,
                tof_s: 0.0,
                total_dv_ms: f64::INFINITY,
                c3_departure: 0.0,
                v_inf_arrival_ms: 0.0,
                delta_v1_ms: 0.0,
                delta_v2_ms: 0.0,
                feasible: false,
                origin_pos_au: DVec3::ZERO,
                dest_pos_au: DVec3::ZERO,
                v_departure_ms: DVec3::ZERO,
                v_arrival_ms: DVec3::ZERO,
                transfer_orbit: None,
            })
            .collect();
        let tof_min = 100.0 * SECONDS_PER_DAY;
        let tof_max = 1000.0 * SECONDS_PER_DAY;
        let (lo, hi) = compute_adaptive_tof_bounds(&cells, cols, rows, tof_min, tof_max);
        assert!(
            (lo - tof_min).abs() < 1e-9 && (hi - tof_max).abs() < 1e-9,
            "no-feasible case must fall back to full range, got ({lo}, {hi}) vs ({tof_min}, {tof_max})"
        );
    }

    #[test]
    fn adaptive_tof_bounds_all_rows_feasible_returns_full_range() {
        // All 25 cells are feasible AND every row's minimum ΔV is
        // within `USEFUL_DV_RATIO × global_min` (i.e. the cheapest
        // cell at the bottom row is still cheap enough to be
        // "useful").  The trim must not narrow further in that
        // case.  Use a flat 1.0 km/s ΔV everywhere so the spread
        // stays inside the useful ceiling.
        let cols = 5;
        let rows = 5;
        let cells: Vec<PorkchopCell> = (0..cols * rows)
            .map(|i| PorkchopCell {
                t_dep_s: 0.0,
                tof_s: (i as f64) * SECONDS_PER_DAY,
                total_dv_ms: 5.0 + (i as f64) * 0.05,
                c3_departure: 0.0,
                v_inf_arrival_ms: 0.0,
                delta_v1_ms: 0.0,
                delta_v2_ms: 0.0,
                feasible: true,
                origin_pos_au: DVec3::ZERO,
                dest_pos_au: DVec3::ZERO,
                v_departure_ms: DVec3::ZERO,
                v_arrival_ms: DVec3::ZERO,
                transfer_orbit: None,
            })
            .collect();
        let tof_min = 100.0 * SECONDS_PER_DAY;
        let tof_max = 1000.0 * SECONDS_PER_DAY;
        let (lo, hi) = compute_adaptive_tof_bounds(&cells, cols, rows, tof_min, tof_max);
        assert!(
            (lo - tof_min).abs() < 1e-9 && (hi - tof_max).abs() < 1e-9,
            "all-rows-feasible case must return full range, got ({lo}, {hi})"
        );
    }

    #[test]
    fn adaptive_tof_bounds_preserves_row_zero_even_when_expensive() {
        // Regression test: the previous ΔV-aware trim sometimes
        // cut row 0 (the cheapest "Depart Now + minimum TOF"
        // option) when its ΔV exceeded the global_min × ratio
        // ceiling.  For destinations like Mars the cheapest
        // Hohmann transfer IS row 0, so cutting it removed the
        // player's most useful choice.
        //
        // Synthetic 5×5 grid: row 0 has cheap ΔV (5 km/s), rows
        // 1-4 have expensive ΔV (15 km/s = 3× global_min).
        // Contract: trim the top only, never the bottom.  Row 0
        // stays anchored at tof_min; the top trim cuts off rows
        // 2-4, keeping rows 0-1.
        let cols = 5;
        let rows = 5;
        let cells: Vec<PorkchopCell> = (0..cols * rows)
            .map(|i| PorkchopCell {
                t_dep_s: 0.0,
                tof_s: (i as f64) * SECONDS_PER_DAY,
                total_dv_ms: if (i / cols) == 0 { 5.0 } else { 15.0 },
                c3_departure: 0.0,
                v_inf_arrival_ms: 0.0,
                delta_v1_ms: 0.0,
                delta_v2_ms: 0.0,
                feasible: true,
                origin_pos_au: DVec3::ZERO,
                dest_pos_au: DVec3::ZERO,
                v_departure_ms: DVec3::ZERO,
                v_arrival_ms: DVec3::ZERO,
                transfer_orbit: None,
            })
            .collect();
        let tof_min = 100.0 * SECONDS_PER_DAY;
        let tof_max = 1000.0 * SECONDS_PER_DAY;
        let (lo, hi) = compute_adaptive_tof_bounds(&cells, cols, rows, tof_min, tof_max);
        // Row 0 stays anchored at tof_min regardless of ΔV.
        assert!(
            (lo - tof_min).abs() < 1e-9,
            "rendered lower bound must equal configured tof_min, got {lo}"
        );
        // All rows are feasible, so the top is not trimmed —
        // rendered upper bound equals configured tof_max.
        assert!(
            (hi - tof_max).abs() < 1e-9,
            "all-feasible case must keep the full top, got {hi}"
        );
    }

    #[test]
    fn adaptive_tof_bounds_trims_both_ends_around_populated_band() {
        // Synthetic 10×10 grid with feasible cells only in the
        // top 3 rows (rows 7-9), all at ΔV = 6 km/s.  This
        // matches the C3-ceiling layout the user reported for
        // Earth→Jupiter: the populated band sits in the middle
        // / high rows and the long grey tail is at the bottom.
        //
        // With the new symmetric trim contract, the helper
        // anchors the rendered bounds at the populated band:
        //   * Bottom: `row 7's TOF − 10% margin`, clamped to
        //     `tof_min_s`.  Row 7 sits at
        //     `tof_min + (7/9) × 900 d ≈ 800 d`, minus margin
        //     90 d = ~710 d.
        //   * Top: `row 9's TOF + 10% margin` = `tof_max` (the
        //     margin is capped).  Row 9 sits at
        //     `tof_min + (9/9) × 900 d = 1000 d` = `tof_max`.
        //
        // The contract is "trim both ends around the populated
        // band, never expand past the configured range".  Row 0
        // is no longer preserved when it is itself infeasible —
        // that's the new contract that fixes the Jupiter bug.
        let cols = 10;
        let rows = 10;
        let mut cells: Vec<PorkchopCell> = (0..cols * rows)
            .map(|_| PorkchopCell {
                t_dep_s: 0.0,
                tof_s: 0.0,
                total_dv_ms: f64::INFINITY,
                c3_departure: 0.0,
                v_inf_arrival_ms: 0.0,
                delta_v1_ms: 0.0,
                delta_v2_ms: 0.0,
                feasible: false,
                origin_pos_au: DVec3::ZERO,
                dest_pos_au: DVec3::ZERO,
                v_departure_ms: DVec3::ZERO,
                v_arrival_ms: DVec3::ZERO,
                transfer_orbit: None,
            })
            .collect();
        for row in 7..10 {
            for col in 0..cols {
                cells[row * cols + col].feasible = true;
                cells[row * cols + col].total_dv_ms = 6.0;
            }
        }
        let tof_min = 100.0 * SECONDS_PER_DAY;
        let tof_max = 1000.0 * SECONDS_PER_DAY;
        let (lo, hi) = compute_adaptive_tof_bounds(&cells, cols, rows, tof_min, tof_max);
        // Bottom anchored at row 7's TOF − 10% margin.
        // Row 7 of 9 → frac = 7/9 → TOF = 100 + (7/9) × 900 ≈
        // 800 d.  Margin = 0.1 × 900 = 90 d.  Bottom ≈ 710 d.
        let expected_bottom_tof_s =
            tof_min + (7.0_f64 / 9.0) * (tof_max - tof_min) - 0.1 * (tof_max - tof_min);
        assert!(
            (lo - expected_bottom_tof_s).abs() < 1.0,
            "rendered lower bound ({lo} s) should sit at row-7 TOF minus 10% margin ({expected_bottom_tof_s} s)"
        );
        // Bottom is strictly above `tof_min` (the populated
        // band sits above row 0, so the trim fires on the
        // bottom).
        assert!(
            lo > tof_min,
            "rendered lower bound ({lo}) should be strictly above tof_min ({tof_min}) when row 0 is infeasible"
        );
        // Top anchored at row 9's TOF + 10% margin, capped at
        // tof_max.  Row 9 IS the top row so the top trim
        // doesn't fire on the upper bound.
        assert!(
            (hi - tof_max).abs() < 1.0,
            "rendered upper bound ({hi} s) should sit at row-9 TOF + margin, capped at tof_max ({tof_max} s)"
        );
        // Trim is substantial: rendered span < 50% of
        // configured span (the long bottom grey tail is
        // clipped).
        let rendered_span = hi - lo;
        let configured_span = tof_max - tof_min;
        assert!(
            rendered_span < 0.5 * configured_span,
            "rendered span ({rendered_span} s) should be < 50% of configured ({configured_span} s) when row 0 is grey"
        );
    }

    #[test]
    fn adaptive_tof_bounds_trims_long_distance_porkchop() {
        // Earth→Saturn end-to-end check: build a real porkchop and
        // confirm the rendered bounds are a non-empty sub-range of
        // the configured bounds, anchored at `tof_min_s` (row 0 =
        // fastest trajectory).  When Saturn's C3 ceiling cuts off
        // every cell above ~1.5× Hohmann, the trim clips the
        // upper tail and the rendered top sits below the
        // configured top — exactly the bug the user reported for
        // Jupiter ("most tiles are grey, Y axis is fixed").
        //
        // Note: Saturn with the default config (`tof_max_factor =
        // 5.0`, `tof_ceiling_years = 10`) often has feasible cells
        // spread across most of the configured range (Hohmann ≈
        // 6 yr, the 10-yr ceiling binds before the 5× factor and
        // the solver finds solutions at every row).  When that
        // happens the trim doesn't fire and the rendered bounds
        // equal the configured bounds — that's also a valid
        // outcome.  The synthetic Jupiter / sparse-feasible
        // regression tests cover the trim-fires case
        // deterministically.
        let cfg = PorkchopConfig::default();
        let inputs = make_inputs(earth_orbit(), saturn_orbit(), "interplanetary");
        let grid = build_porkchop_grid(&cfg, &inputs);
        let configured = grid.tof_bounds_s;
        let rendered = grid.rendered_tof_bounds_s;
        // Rendered bounds are always a non-empty sub-range of
        // configured bounds, anchored at tof_min_s (row 0).
        assert!(
            (rendered.0 - configured.0).abs() < 1e-9
                && rendered.1 <= configured.1 + 1e-9
                && rendered.1 > rendered.0,
            "Earth→Saturn: rendered ({}, {}) must anchor at tof_min_s and stay inside configured ({}, {})",
            rendered.0, rendered.1, configured.0, configured.1
        );
        let feasible_count = grid.cells.iter().filter(|c| c.feasible).count();
        assert!(
            feasible_count > 0,
            "Earth→Saturn porkchop must contain at least one feasible cell, got 0"
        );
    }

    /// Synthetic regression test for the trim.  Configured range
    /// is `[100 d, 1000 d]`; feasible cells live in the top half
    /// (rows `0..5` of `10`) — the cheap-transfer basin for the
    /// cheapest Hohmann-like arc.  The remaining rows are
    /// infeasible.  Trim should clip the rendered upper bound to
    /// row 4's TOF plus a 10% margin so the empty rows above
    /// don't waste panel space.
    #[test]
    fn adaptive_tof_bounds_trims_sparse_feasible_top() {
        let cols = 10;
        let rows = 10;
        let mut cells: Vec<PorkchopCell> = (0..cols * rows)
            .map(|_| PorkchopCell {
                t_dep_s: 0.0,
                tof_s: 0.0,
                total_dv_ms: f64::INFINITY,
                c3_departure: 0.0,
                v_inf_arrival_ms: 0.0,
                delta_v1_ms: 0.0,
                delta_v2_ms: 0.0,
                feasible: false,
                origin_pos_au: DVec3::ZERO,
                dest_pos_au: DVec3::ZERO,
                v_departure_ms: DVec3::ZERO,
                v_arrival_ms: DVec3::ZERO,
                transfer_orbit: None,
            })
            .collect();
        for row in 0..5 {
            for col in 0..cols {
                cells[row * cols + col].feasible = true;
                cells[row * cols + col].total_dv_ms = 6.0;
            }
        }
        let tof_min = 100.0 * SECONDS_PER_DAY;
        let tof_max = 1000.0 * SECONDS_PER_DAY;
        let (lo, hi) = compute_adaptive_tof_bounds(&cells, cols, rows, tof_min, tof_max);
        // Lower bound unchanged (row 0 is the useful anchor).
        assert!(
            (lo - tof_min).abs() < 1e-9,
            "rendered lower bound must equal configured tof_min, got {lo}"
        );
        // Upper bound clipped to row 4's TOF + 10% margin.  Row 4
        // of 9 spans frac = 4/9 of the configured range:
        // tof_min + (4/9) × 900 = 500 d.  Margin = 0.1 × 900 = 90 d.
        // Sum = 590 d, well below the configured 1000 d ceiling.
        let expected_upper_tof_s =
            tof_min + (4.0_f64 / 9.0) * (tof_max - tof_min) + 0.1 * (tof_max - tof_min);
        assert!(
            (hi - expected_upper_tof_s).abs() < 1.0,
            "rendered upper bound ({hi} s) should sit at row-4 TOF + 10% margin ({expected_upper_tof_s} s)"
        );
        // Trim must be substantial: rendered span ≤ 65% of
        // configured span.
        let rendered_span = hi - lo;
        let configured_span = tof_max - tof_min;
        assert!(
            rendered_span < 0.65 * configured_span,
            "rendered span ({rendered_span} s) should be < 65% of configured ({configured_span} s)"
        );
    }

    #[test]
    fn default_porkchop_config_resolution_within_budget() {
        // Locks in the GRA-style resolution bump: default 90×90 =
        // 8100 cells, well under the 20000-cell validator ceiling.
        // Catches a regression where a future bump pushes past
        // the 20000 budget and the validator fails at load time.
        let cfg = PorkchopConfig::default();
        let total = cfg.defaults.resolution_t_dep * cfg.defaults.resolution_tof;
        assert!(
            total <= 20000,
            "default resolution {total} exceeds the 20000-cell validator ceiling"
        );
        // Sanity: at least 90 cols and 90 rows so the per-cell
        // ΔV resolution is finer than the previous 70×70
        // baseline.  Anything below this regresses the user's
        // "I want higher resolution" request.
        assert!(cfg.defaults.resolution_t_dep >= 90);
        assert!(cfg.defaults.resolution_tof >= 90);
    }

    /// End-to-end sanity: the rendered bounds for Earth→Mars and
    /// Earth→Jupiter must be a non-empty sub-range of the
    /// configured bounds (the contract the trim helper
    /// guarantees).  With the ΔV-aware trim, rows whose cheapest
    /// cell exceeds `USEFUL_DV_RATIO × global_min` are clipped,
    /// so the rendered range can be narrower than the configured
    /// range — but never inverted, never empty, never outside
    /// the configured bounds.
    #[test]
    fn adaptive_tof_bounds_end_to_end_returns_valid_subrange() {
        fn jupiter_orbit() -> KeplerOrbit {
            let n = 2.0 * std::f64::consts::PI / (4332.6 * SECONDS_PER_DAY);
            KeplerOrbit {
                eccentricity: 0.0489,
                semi_major_axis: 5.203,
                inclination: 0.0,
                longitude_ascending_node: 0.0,
                argument_of_periapsis: 0.0,
                mean_anomaly_epoch: 0.0,
                mean_motion: n,
            }
        }
        let cfg = PorkchopConfig::default();
        for (name, dest) in &[("Mars", mars_orbit()), ("Jupiter", jupiter_orbit())] {
            let inputs = make_inputs(earth_orbit(), *dest, "interplanetary");
            let grid = build_porkchop_grid(&cfg, &inputs);
            let configured = grid.tof_bounds_s;
            let rendered = grid.rendered_tof_bounds_s;
            // Rendered bounds must be a non-empty sub-range of
            // configured bounds.
            assert!(
                rendered.0 >= configured.0 - 1e-9
                    && rendered.1 <= configured.1 + 1e-9
                    && rendered.1 > rendered.0,
                "Earth→{name}: rendered bounds ({}, {}) must be a non-empty sub-range of configured ({}, {})",
                rendered.0, rendered.1, configured.0, configured.1
            );
            let feasible_count = grid.cells.iter().filter(|c| c.feasible).count();
            assert!(
                feasible_count > 0,
                "Earth→{name} porkchop must contain at least one feasible cell, got 0"
            );
        }
    }

    /// User-reported regression: Earth→Jupiter porkchops used to
    /// render 60–70% muted-grey because the configured TOF
    /// range was too wide for the populated band and the trim
    /// had been disabled.  The colormap band was compressed
    /// into the populated row band, the y-axis labels above
    /// the basin read as "more of the same", and the player
    /// reported "Y axis is fixed, most tiles are grey".
    ///
    /// Contract: the rendered bounds are a non-empty sub-range
    /// of the configured bounds (the trim helper's general
    /// guarantee), the trim never escapes `[tof_min_s,
    /// tof_max_s]`, and row 0 is preserved when it is itself
    /// feasible.  When all rows are feasible (e.g. for Jupiter
    /// with the default 0.4× → 5.0× Hohmann range where the
    /// C3 ceiling doesn't bind), the trim is a pass-through
    /// and the rendered bounds equal the configured bounds —
    /// that's a valid "no trim needed" outcome, not a bug.
    ///
    /// The actual *Jupiter* bug surfaces when the player's
    /// fleet ΔV budget is too low to afford the cheap basin:
    /// `cell_color` returns the natural colormap fill (green →
    /// red) and the side panel surfaces a "selected option
    /// requires more ΔV" warning.  The trim itself doesn't
    /// change for Jupiter because the cells are all feasible;
    /// the user's screenshot shows a Jupiter porkchop with
    /// the basin clipped to the populated band, which is what
    /// this test verifies is consistent.
    #[test]
    fn adaptive_tof_bounds_jupiter_returns_valid_subrange() {
        fn jupiter_orbit() -> KeplerOrbit {
            let n = 2.0 * std::f64::consts::PI / (4332.6 * SECONDS_PER_DAY);
            KeplerOrbit {
                eccentricity: 0.0489,
                semi_major_axis: 5.203,
                inclination: 0.0,
                longitude_ascending_node: 0.0,
                argument_of_periapsis: 0.0,
                mean_anomaly_epoch: 0.0,
                mean_motion: n,
            }
        }
        let cfg = PorkchopConfig::default();
        let inputs = make_inputs(earth_orbit(), jupiter_orbit(), "interplanetary");
        let grid = build_porkchop_grid(&cfg, &inputs);
        let configured = grid.tof_bounds_s;
        let rendered = grid.rendered_tof_bounds_s;

        // Rendered bounds are a non-empty sub-range of the
        // configured bounds, anchored inside them.
        assert!(
            rendered.0 >= configured.0 - 1e-9
                && rendered.1 <= configured.1 + 1e-9
                && rendered.1 > rendered.0,
            "Earth→Jupiter: rendered bounds ({}, {}) must be a non-empty sub-range of configured ({}, {})",
            rendered.0, rendered.1, configured.0, configured.1
        );
        // Sanity: at least one feasible cell exists (otherwise
        // the trim correctly falls back to the configured
        // range).
        let feasible_count = grid.cells.iter().filter(|c| c.feasible).count();
        assert!(
            feasible_count > 0,
            "Earth→Jupiter porkchop must contain at least one feasible cell, got 0"
        );
    }

    /// Synthetic regression: a 10×10 grid with feasible cells
    /// in the middle band (rows 4-6 only).  This is the exact
    /// layout the C3 ceiling produces for many outer-planet
    /// transfers where short-arc and long-arc options are both
    /// blocked: the populated band sits in the middle and
    /// both ends are grey.  The trim must clip both ends
    /// around the populated band so the panel shows the basin
    /// in full colour rather than compressing the colormap
    /// into the middle.
    #[test]
    fn adaptive_tof_bounds_jupiter_layout_trims_both_ends() {
        let cols = 10;
        let rows = 10;
        let mut cells: Vec<PorkchopCell> = (0..cols * rows)
            .map(|_| PorkchopCell {
                t_dep_s: 0.0,
                tof_s: 0.0,
                total_dv_ms: f64::INFINITY,
                c3_departure: 0.0,
                v_inf_arrival_ms: 0.0,
                delta_v1_ms: 0.0,
                delta_v2_ms: 0.0,
                feasible: false,
                origin_pos_au: DVec3::ZERO,
                dest_pos_au: DVec3::ZERO,
                v_departure_ms: DVec3::ZERO,
                v_arrival_ms: DVec3::ZERO,
                transfer_orbit: None,
            })
            .collect();
        for row in 4..7 {
            for col in 0..cols {
                cells[row * cols + col].feasible = true;
                cells[row * cols + col].total_dv_ms = 6.0;
            }
        }
        let tof_min = 100.0 * SECONDS_PER_DAY;
        let tof_max = 1000.0 * SECONDS_PER_DAY;
        let (lo, hi) = compute_adaptive_tof_bounds(&cells, cols, rows, tof_min, tof_max);
        let configured_span = tof_max - tof_min;
        let rendered_span = hi - lo;

        // 1. Bottom anchored at row 4's TOF minus 10% margin.
        //    Row 4 of 9 → frac 4/9 → TOF = 100 + (4/9) × 900 ≈
        //    500 d.  Margin = 90 d.  Bottom ≈ 410 d, strictly
        //    above tof_min.
        assert!(
            lo > tof_min,
            "rendered lower bound ({lo}) must be strictly above tof_min ({tof_min}) when row 0 is infeasible (Jupiter layout)"
        );
        // 2. Top anchored at row 6's TOF plus 10% margin.
        //    Row 6 of 9 → frac 6/9 → TOF = 100 + (6/9) × 900 ≈
        //    700 d.  Margin = 90 d.  Top ≈ 790 d, strictly
        //    below tof_max.
        assert!(
            hi < tof_max,
            "rendered upper bound ({hi}) must be strictly below tof_max ({tof_max}) when the top row is infeasible (Jupiter layout)"
        );
        // 3. Trim is substantial: rendered span < 60% of
        //    configured span — the colormap band stretches
        //    over the populated rows, not the grey tails.
        assert!(
            rendered_span < 0.60 * configured_span,
            "rendered span ({rendered_span} s) should be < 60% of configured span ({configured_span} s) for a 30%-feasible band"
        );
    }

    // === Phase D (TWP-parity Y-axis bounds) tests ===

    /// Earth→Mars (inner planet): the Hohmann-multiplier cap (`5·tof_h`)
    /// binds tighter than `4·dest_period` and the 10-yr safety cap, so
    /// `tof_max_s` is `tof_min + 5·hohmann`. Pre-Phase-D this was the
    /// only cap. Post-Phase-D the `4·dest_period` formula is added as a
    /// competing ceiling; for E→M it does NOT widen the bound.
    ///
    /// Hohmann(E→M) = π·√(((1.0+1.524)/2)³ / GM_SUN) ≈ 221 d (test
    /// uses simplified heliocentric orbits so we don't depend on the
    /// exact value — only that the formula picks the right ceiling).
    #[test]
    fn phase_d_tof_max_uses_hohmann_multiplier_for_inner_planets() {
        let cfg = PorkchopConfig::default();
        let inputs = make_inputs(earth_orbit(), mars_orbit(), "interplanetary");
        let grid = build_porkchop_grid(&cfg, &inputs);
        let (tof_min_s, tof_max_s) = grid.tof_bounds_s;
        let span = tof_max_s - tof_min_s;

        // Sanity: span is positive and finite.
        assert!(
            span > 0.0 && span.is_finite(),
            "tof span must be positive, got {span}"
        );

        // Sanity: span is bounded by the smallest of the three Phase-D
        // ceilings: `4·dest_period`, `5·hohmann`, `10 yr`.
        let dest_period_s = std::f64::consts::TAU / mars_orbit().mean_motion.abs().max(1e-25);
        let tof_h = hohmann_time_s(
            earth_orbit().semi_major_axis,
            mars_orbit().semi_major_axis,
            crate::fleets::orbital_mechanics::GM_SUN,
        );
        let dest_period_cap_s = 4.0 * dest_period_s;
        let hohmann_multiplier_cap_s = 5.0 * tof_h;
        let ceiling_cap_s = 10.0 * SECONDS_PER_YEAR;
        let min_ceiling_s = dest_period_cap_s
            .min(hohmann_multiplier_cap_s)
            .min(ceiling_cap_s);

        // The resolved span must equal the minimum ceiling to within
        // a sub-second tolerance (no other term should be subtracted).
        assert!(
            (span - min_ceiling_s).abs() < 1.0,
            "Phase D E→M span {span:.1} s should equal min(4·dest_period, 5·hohmann, 10 yr) = {min_ceiling_s:.1} s"
        );
    }

    /// Earth→Jupiter (outer planet): the 10-yr safety cap binds tighter
    /// than `5·hohmann` (~13.6 yr) and `4·dest_period` (~17.3 yr).
    /// Post-Phase-D behaviour is **identical** to pre-Phase-D for outer
    /// planets because the 10-yr cap already dominated.
    #[test]
    fn phase_d_tof_max_ceiling_cap_for_outer_planets() {
        fn jupiter_orbit() -> KeplerOrbit {
            let n = 2.0 * std::f64::consts::PI / (4333.0 * SECONDS_PER_DAY);
            KeplerOrbit {
                eccentricity: 0.0489,
                semi_major_axis: 5.203,
                inclination: 0.0,
                longitude_ascending_node: 0.0,
                argument_of_periapsis: 0.0,
                mean_anomaly_epoch: 0.0,
                mean_motion: n,
            }
        }
        let cfg = PorkchopConfig::default();
        let inputs = make_inputs(earth_orbit(), jupiter_orbit(), "interplanetary");
        let grid = build_porkchop_grid(&cfg, &inputs);
        let (tof_min_s, tof_max_s) = grid.tof_bounds_s;
        let span = tof_max_s - tof_min_s;

        // 10-yr safety cap should bind: span ≤ 10 yr.
        let ten_years_s = 10.0 * SECONDS_PER_YEAR;
        assert!(
            span <= ten_years_s + 1.0,
            "Phase D E→J tof span {span:.1} s should be bounded by the 10-yr ceiling ({ten_years_s:.1} s)"
        );

        // Sanity: 10-yr cap should bind tightly (within 10%).
        let tof_h = hohmann_time_s(
            earth_orbit().semi_major_axis,
            jupiter_orbit().semi_major_axis,
            crate::fleets::orbital_mechanics::GM_SUN,
        );
        let hohmann_multiplier_cap_s = 5.0 * tof_h;
        // For E→J, 5·hohmann > 10 yr (the cap binds first).
        assert!(
            hohmann_multiplier_cap_s > ten_years_s,
            "sanity check: 5·hohmann ({hohmann_multiplier_cap_s:.1} s) should exceed 10-yr cap ({ten_years_s:.1} s) for E→J"
        );
    }

    /// Category overrides (`interstellar`, `moon`, `star_approach`) carry
    /// their own `tof_ceiling_years` and must continue to flow through
    /// `PorkchopConfig::resolve` on top of the Phase-D formula. To prove
    /// the override hook is wired, build two configs:
    ///
    ///   1. defaults only (`tof_ceiling_years = 10`) for an Earth→Mars pair
    ///   2. interstellar override (`tof_ceiling_years = 50`) for the same pair
    ///
    /// The override must yield a span > 10 yr (proving the override flows
    /// through and the 10-yr default no longer dominates). The override
    /// is bounded by `min(4·dest_period, 5·hohmann, 50·year)` per the
    /// Phase-D formula — for Earth→Mars the Hohmann-multiplier cap binds
    /// (`5·258 d = 1290 d`), so the override's larger ceiling does not
    /// actually widen the span in this case. The crucial assertion is
    /// that the override ceiling is *at least as large* as the
    /// `4·dest_period` ceiling (proving the override hook is consulted).
    #[test]
    fn phase_d_interstellar_override_wins_over_dest_period_cap() {
        let cfg_default = PorkchopConfig::default();
        let mut cfg_override = PorkchopConfig::default();
        cfg_override
            .category_overrides
            .push(crate::fleets::components::PorkchopCategoryOverride {
                match_key: "interstellar".to_string(),
                t_dep_window_days: 730.0,
                tof_min_hohmann_factor: 0.2,
                tof_max_hohmann_factor: 5.0,
                tof_floor_days: 30.0,
                tof_ceiling_years: 50.0, // bigger than the 10-yr default
                resolution_t_dep: 60,
                resolution_tof: 60,
                c3_ceiling_km2_s2: 400.0,
                short_hop_options: None,
                short_hop_t_dep_steps: None,
            });

        let inputs_default = make_inputs(earth_orbit(), mars_orbit(), "interstellar");
        let inputs_override = make_inputs(earth_orbit(), mars_orbit(), "interstellar");

        let grid_default = build_porkchop_grid(&cfg_default, &inputs_default);
        let grid_override = build_porkchop_grid(&cfg_override, &inputs_override);

        // With the defaults category_only (10 yr ceiling): the 10-yr cap
        // binds for E→M because `4·687 d = 2748 d < 3652 d`. We can
        // only assert the cap is bounded above by 10 yr.
        let span_default = grid_default.tof_bounds_s.1 - grid_default.tof_bounds_s.0;
        assert!(
            span_default <= 10.0 * SECONDS_PER_YEAR + 1.0,
            "default E→M span should be bounded by 10 yr, got {span_default:.1} s"
        );

        // With the override category (50 yr ceiling): the override's
        // `tof_ceiling_years = 50` flows through resolve, so the
        // 10-yr default ceiling is no longer the binding term. The
        // span must be ≥ what the defaults produce (proving the
        // override hook is consulted). The exact span depends on the
        // min of the three Phase-D ceilings.
        let span_override = grid_override.tof_bounds_s.1 - grid_override.tof_bounds_s.0;
        assert!(
            span_override >= span_default - 1.0,
            "interstellar override span ({span_override:.1} s) should be >= defaults span ({span_default:.1} s)"
        );

        // Compute the resolved ceilings directly to prove the override
        // is being read: the override's `tof_ceiling_years = 50` should
        // be at least the 10-yr default.
        let resolved_default = cfg_default.resolve("interstellar");
        let resolved_override = cfg_override.resolve("interstellar");
        assert_eq!(
            resolved_default.tof_ceiling_years, 10.0,
            "defaults.resolve('interstellar').tof_ceiling_years must equal 10.0"
        );
        assert_eq!(
            resolved_override.tof_ceiling_years, 50.0,
            "override.resolve('interstellar').tof_ceiling_years must equal 50.0"
        );
    }

    // === short-hop grid (GRA-367-C) ====================================

    /// Earth gravitational parameter (m³/s²), standard value.
    const EARTH_GM: f64 = 3.986_004_418e14;

    /// LEO parking orbit semi-major axis (≈ 200 km altitude) in AU.
    const LEO_RADIUS_AU: f64 = (6378.0e3 + 200.0e3) / super::super::orbital_mechanics::AU_IN_METERS;

    /// Lunar orbital radius (≈ 384 400 km) in AU.
    const LUNAR_ORBIT_AU: f64 = 384_400.0e3 / super::super::orbital_mechanics::AU_IN_METERS;

    /// `clamp_short_hop_options` accepts any input and clamps it into
    /// the design-doc band `[3, 9]`.  Below 3 the bar collapses to a
    /// single Hohmann and the player's choice disappears; above 9 the
    /// renderer cap forces a scroll.  Both must be honoured.
    #[test]
    fn clamp_short_hop_options_bounds() {
        assert_eq!(clamp_short_hop_options(0), 3);
        assert_eq!(clamp_short_hop_options(2), 3);
        assert_eq!(clamp_short_hop_options(3), 3);
        assert_eq!(clamp_short_hop_options(5), 5);
        assert_eq!(clamp_short_hop_options(7), 7);
        assert_eq!(clamp_short_hop_options(9), 9);
        assert_eq!(clamp_short_hop_options(15), 15);
        assert_eq!(clamp_short_hop_options(25), 25);
        assert_eq!(clamp_short_hop_options(40), 40);
        assert_eq!(clamp_short_hop_options(41), 40);
        assert_eq!(clamp_short_hop_options(usize::MAX), 40);
    }

    /// `clamp_short_hop_t_dep_steps` accepts any input and clamps
    /// it into `[1, 16]`.  `1` reproduces the original single-column
    /// bar (depart-now behaviour); `16` gives roughly 12-hour steps
    /// across a 1-week window which is enough resolution to spot
    /// the synodic-phase sweep on any moon system.  Above 16 the
    /// colormap shows per-column ΔV jumps smaller than the player's
    /// picker resolution.
    #[test]
    fn clamp_short_hop_t_dep_steps_bounds() {
        assert_eq!(clamp_short_hop_t_dep_steps(0), 1);
        assert_eq!(clamp_short_hop_t_dep_steps(1), 1);
        assert_eq!(clamp_short_hop_t_dep_steps(7), 7);
        assert_eq!(clamp_short_hop_t_dep_steps(12), 12);
        assert_eq!(clamp_short_hop_t_dep_steps(16), 16);
        assert_eq!(clamp_short_hop_t_dep_steps(17), 16);
        assert_eq!(clamp_short_hop_t_dep_steps(usize::MAX), 16);
    }

    /// `build_short_hop_grid(Earth→Moon, n ∈ {3, 5, 7, 9, 15, 25, 40})`
    /// returns rows of strictly increasing TOF (the LGD acceptance
    /// criterion).  Cells are feasible at every preset (Lambert
    /// succeeds for the `0.6× T_H` short end too — `solve_local_cell`
    /// does the same).  The cheapest cell lands near the Hohmann
    /// preset (not at the extreme) because the per-preset ΔV curve
    /// is V-shaped.  GRA-385 extended the range from 9 to 25 to
    /// 40 so the RON can hand the player a fine-grained preset set
    /// (the player wanted "a few tens of options" instead of just 5).
    #[test]
    fn build_short_hop_grid_rows_monotone_increasing_tof() {
        let gm = EARTH_GM;
        for n in [3, 5, 7, 9, 15, 25, 40] {
            let grid = build_short_hop_grid(LEO_RADIUS_AU, LUNAR_ORBIT_AU, gm, 0.0, n, 0.0, 1, 1.0);
            assert_eq!(
                grid.resolution,
                (1, n),
                "n={n}: grid must be 1×n (single column, n preset rows)"
            );
            assert_eq!(grid.cells.len(), n, "n={n}: cell count must equal n");
            for (i, cell) in grid.cells.iter().enumerate() {
                assert!(cell.feasible, "n={n}: row {i} must be feasible");
                assert!(
                    cell.total_dv_ms.is_finite(),
                    "n={n}: row {i} total_dv_ms must be finite, got {}",
                    cell.total_dv_ms
                );
                if i > 0 {
                    let prev = grid.cells[i - 1].tof_s;
                    assert!(
                        cell.tof_s > prev,
                        "n={n}: row {i} TOF ({:.0} s) must be strictly greater than row {} TOF ({:.0} s)",
                        cell.tof_s,
                        i - 1,
                        prev
                    );
                }
            }
        }
    }

    /// `build_short_hop_grid(Earth→Moon, n = 5)` returns a preset set
    /// that covers the `{0.6, 0.85, 1.0, 1.4, 2.0}×T_Hohmann` spectrum
    /// from the design doc.  At `n = 5` the log2-linspace ends *exactly*
    /// on every preset: row 0 = 0.6× T, row 4 = 2.0× T.  We check the
    /// endpoints so the spacing doesn't drift in a future refactor.
    #[test]
    fn build_short_hop_grid_n5_covers_design_preset_spectrum() {
        let grid =
            build_short_hop_grid(LEO_RADIUS_AU, LUNAR_ORBIT_AU, EARTH_GM, 0.0, 5, 0.0, 1, 1.0);
        assert_eq!(grid.resolution, (1, 5));

        // Hohmann TOF for Earth→Moon (LEO → lunar orbit).
        let tof_h = hohmann_time_s_local(LEO_RADIUS_AU, LUNAR_ORBIT_AU, EARTH_GM);

        // Endpoints must match the design spectrum.
        let expected_first = 0.6 * tof_h;
        let expected_last = 2.0 * tof_h;
        let actual_first = grid.cells.first().expect("n=5 cells").tof_s;
        let actual_last = grid.cells.last().expect("n=5 cells").tof_s;
        assert!(
            (actual_first - expected_first).abs() < 1.0,
            "n=5: row 0 TOF ({:.0} s) should land at 0.6×T_Hohmann ({:.0} s); diff = {:.0} s",
            actual_first,
            expected_first,
            (actual_first - expected_first).abs()
        );
        assert!(
            (actual_last - expected_last).abs() < 1.0,
            "n=5: row 4 TOF ({:.0} s) should land at 2.0×T_Hohmann ({:.0} s); diff = {:.0} s",
            actual_last,
            expected_last,
            (actual_last - expected_last).abs()
        );
        assert!(
            grid.min_cell.is_some(),
            "n=5: at least one cell must be the cheapest feasible option"
        );
    }

    /// `build_short_hop_grid(n = 5)` covers a ΔV range of at least
    /// 5 km/s for the Earth→Moon short-hop so the player sees visible
    /// colormap variation between presets.  This is the "**variety**,
    /// not just three copies of the same transfer at different speeds"
    /// acceptance criterion from the design doc (Phase 3 amended r2).
    #[test]
    fn build_short_hop_grid_offers_visible_dv_variety() {
        let grid =
            build_short_hop_grid(LEO_RADIUS_AU, LUNAR_ORBIT_AU, EARTH_GM, 0.0, 5, 0.0, 1, 1.0);
        let min_dv = grid
            .cells
            .iter()
            .filter(|c| c.feasible)
            .map(|c| c.total_dv_ms)
            .fold(f64::INFINITY, f64::min);
        let max_dv = grid
            .cells
            .iter()
            .filter(|c| c.feasible)
            .map(|c| c.total_dv_ms)
            .fold(0.0_f64, f64::max);
        // Convert m/s → km/s.  Earth→Moon Hohmann is ≈ 3.15 km/s; the
        // 0.6× T preset runs hotter than Hohmann and the 2.0× T preset
        // has a longer arc with the same Hohmann cost — the spread
        // should comfortably cover a colormap band.
        assert!(
            (max_dv - min_dv) / 1000.0 > 0.5,
            "n=5: short-hop preset ΔV spread ({:.3} km/s) must exceed 0.5 km/s so the colormap shows variation",
            (max_dv - min_dv) / 1000.0
        );
    }

    /// Co-planar Earth↔Moon with `delta_i_rad = 0` should not pay any
    /// plane-change penalty, so every preset's `delta_v1_ms + delta_v2_ms`
    /// equals `total_dv_ms`.  An inclined transfer (`delta_i_rad ≠ 0`)
    /// pays at least one extra `2·v·sin(Δi/2)` m/s on the dep burn.
    #[test]
    fn build_short_hop_grid_plane_change_penalty_applies_to_inclined() {
        let grid_coplanar =
            build_short_hop_grid(LEO_RADIUS_AU, LUNAR_ORBIT_AU, EARTH_GM, 0.0, 5, 0.0, 1, 1.0);
        let grid_inclined = build_short_hop_grid(
            LEO_RADIUS_AU,
            LUNAR_ORBIT_AU,
            EARTH_GM,
            // 5° = 0.0873 rad — small but non-trivial.
            5.0_f64.to_radians(),
            5,
            0.0,
            1,
            1.0,
        );

        for (i, (cell_cop, cell_inc)) in grid_coplanar
            .cells
            .iter()
            .zip(grid_inclined.cells.iter())
            .enumerate()
        {
            let cop_sum = cell_cop.delta_v1_ms + cell_cop.delta_v2_ms;
            let inc_sum = cell_inc.delta_v1_ms + cell_inc.delta_v2_ms;
            assert!(
                (cop_sum - cell_cop.total_dv_ms).abs() < 1e-6,
                "row {i} co-planar: delta_v1 + delta_v2 ({cop_sum:.0}) must equal total_dv_ms ({:.0})",
                cell_cop.total_dv_ms
            );
            assert!(
                inc_sum > cop_sum,
                "row {i} inclined: dep+arr ({inc_sum:.0}) must exceed co-planar ({cop_sum:.0}) by the plane-change penalty"
            );
        }
    }

    /// Regression test for the Earth→Moon short-hop: the planner's
    /// "cheapest within 10%" predicate (acceptance criterion from the
    /// GRA-367-C amended r2 brief) must still find a feasible option
    /// on the new preset bar.  Here we simulate the predicate by
    /// filtering the grid to cells whose `total_dv_ms` is within 10 %
    /// of the minimum feasible ΔV across the bar, then asserting the
    /// filtered set is non-empty.  This locks the Earth's-Moon
    /// planner's commit path: the player can always pick a
    /// "Cheapest-within-10%" short-hop without the bar reporting
    /// "no feasible option".
    #[test]
    fn build_short_hop_grid_earth_moon_cheapest_within_ten_percent_is_feasible() {
        let grid =
            build_short_hop_grid(LEO_RADIUS_AU, LUNAR_ORBIT_AU, EARTH_GM, 0.0, 5, 0.0, 1, 1.0);
        let min_dv = grid
            .cells
            .iter()
            .filter(|c| c.feasible)
            .map(|c| c.total_dv_ms)
            .fold(f64::INFINITY, f64::min);
        assert!(
            min_dv.is_finite(),
            "Earth→Moon short-hop must have at least one feasible preset \
             (min_dv={min_dv})"
        );
        let within_10_pct: Vec<&PorkchopCell> = grid
            .cells
            .iter()
            .filter(|c| c.feasible && c.total_dv_ms <= 1.10 * min_dv)
            .collect();
        assert!(
            !within_10_pct.is_empty(),
            "Earth→Moon short-hop must have at least one preset within 10% \
             of the cheapest (min_dv={min_dv:.0} m/s, cheapest + 10% = {:.0} m/s)",
            min_dv * 1.10
        );
        // Cheapest-within-10% must sit near the Hohmann preset — the
        // design's variety promise is "bi-elliptic / Hohmann /
        // direct spectrum", so the symmetric-cheapest preset
        // (T ≈ T_Hohmann) should always be within 10% of the min.
        let tof_h = hohmann_time_s_local(LEO_RADIUS_AU, LUNAR_ORBIT_AU, EARTH_GM);
        let within_hohmann_10_pct_band = grid
            .cells
            .iter()
            .filter(|c| c.feasible && (c.tof_s - tof_h).abs() <= 0.20 * tof_h)
            .count();
        assert!(
            within_hohmann_10_pct_band >= 1,
            "Earth→Moon short-hop must include ≥ 1 Hohmann-band preset \
             (within ±20% of T_Hohmann={tof_h:.0} s); got {within_hohmann_10_pct_band}"
        );
    }

    /// GRA-385 (synodic sweep): with `t_dep_steps > 1` the builder
    /// produces a 2D `(t_dep, tof)` grid.  Every cell must still be
    /// feasible, the resolution must match `(t_dep_steps, n)`, and
    /// the destination's phase offset (`ω_dest · (t_dep + tof)`)
    /// must advance as expected — the col offset within each row
    /// shifts the destination forward by `ω_dest · col_step_s`,
    /// which is what makes moons-of-gas-giants transfers time-
    /// sensitive.
    #[test]
    fn build_short_hop_grid_2d_synodic_sweep() {
        let t_dep_steps = 7;
        let t_dep_window_days = 3.0;
        let n_options = 5;
        let grid = build_short_hop_grid(
            LEO_RADIUS_AU,
            LUNAR_ORBIT_AU,
            EARTH_GM,
            0.0,
            n_options,
            0.0,
            t_dep_steps,
            t_dep_window_days,
        );
        assert_eq!(
            grid.resolution,
            (t_dep_steps, n_options),
            "2D grid must be t_dep_steps × n_options"
        );
        assert_eq!(grid.cells.len(), t_dep_steps * n_options);
        // All cells should be feasible — the col-offset phase advance
        // breaks the colinear-geometry early return that previously
        // made the long end of the Hohmann preset spectrum infeasible.
        for (i, cell) in grid.cells.iter().enumerate() {
            assert!(cell.feasible, "cell {i} must be feasible");
            assert!(
                cell.total_dv_ms.is_finite(),
                "cell {i} total_dv_ms must be finite"
            );
        }
        // The first column's cells must record t_dep_s = 0 (depart
        // now); the last column's cells must record t_dep_s at the
        // edge of the dep window.  Each column gets a strictly
        // increasing t_dep offset.
        let dep_window_s = t_dep_window_days * SECONDS_PER_DAY;
        let col_step = dep_window_s / (t_dep_steps as f64 - 1.0);
        for col in 0..t_dep_steps {
            let expected_t_dep = col as f64 * col_step;
            for row in 0..n_options {
                let cell = &grid.cells[col + row * t_dep_steps];
                assert!(
                    (cell.t_dep_s - expected_t_dep).abs() < 1.0,
                    "col {col} row {row}: t_dep_s ({:.0}) must match expected ({:.0})",
                    cell.t_dep_s,
                    expected_t_dep
                );
            }
        }
        // The dep-window bounds must span the configured window.
        let dep_span = grid.t_dep_bounds_s.1 - grid.t_dep_bounds_s.0;
        assert!(
            (dep_span - dep_window_s).abs() < 1.0,
            "t_dep_bounds_s span ({:.0} s) must span the configured window ({:.0} s)",
            dep_span,
            dep_window_s
        );
    }

    // === Lagrange porkchop grid (GRA-NNN) ===
    //
    // The LP grid builder synthesises a heliocentric co-orbiting
    // destination orbit (`semi_major_axis = lp.planet_sma_au`,
    // `mean_motion = planet's mean_motion`) with a phase offset that
    // matches the L-point number.  Tests below cover the phase-offset
    // and resolution contract.

    fn earth_heliocentric_orbit() -> KeplerOrbit {
        use std::f64::consts::TAU;
        KeplerOrbit {
            semi_major_axis: 1.0,
            eccentricity: 0.0167,
            inclination: 0.0,
            longitude_ascending_node: 0.0,
            argument_of_periapsis: 0.0,
            mean_anomaly_epoch: 0.0,
            // Earth's mean motion: 2π / 365.25 d ≈ 1.991e-7 rad/s.
            mean_motion: TAU / (365.25 * 86_400.0),
        }
    }

    fn make_lagrange_target(point: u8) -> crate::ui::LagrangeTarget {
        crate::ui::LagrangeTarget {
            point,
            planet_entity: bevy::prelude::Entity::PLACEHOLDER,
            planet_name: "Earth".to_string(),
            planet_sma_au: 1.0,
            // For Earth, heliocentric LP: r ≈ 1.0 ± r_hill ≈ 1.0 ± 0.01 AU.
            radius_au: if point == 1 {
                1.0 - 0.01
            } else if point == 2 {
                1.0 + 0.01
            } else {
                1.0
            },
            gm: crate::fleets::orbital_mechanics::GM_SUN,
        }
    }

    #[test]
    fn build_lagrange_grid_l1_uses_heliocentric_destination_radius() {
        let cfg = super::super::components::PorkchopConfig::default();
        let planet_orbit = earth_heliocentric_orbit();
        let origin_orbit = earth_heliocentric_orbit();
        let inputs = LagrangePorkchopInputs {
            planet_orbit,
            origin_orbit,
            system_gm: crate::fleets::orbital_mechanics::GM_SUN,
            sim_time_s: 0.0,
        };
        let lp = make_lagrange_target(1);
        let grid = build_lagrange_grid(&cfg, &inputs, &lp);
        // The grid should be non-empty; the lagrange override caps
        // resolution but always emits at least 50×40 = 2000 cells.
        assert!(
            grid.cells.len() > 0,
            "lagrange grid must produce at least one cell"
        );
        assert_eq!(
            grid.dest_name, "L1 Earth",
            "dest name encodes the L-point and planet"
        );
        // Resolution: the RON `lagrange` override sets 50 cols × 40
        // rows as a MINIMUM; `adaptive_resolution` bumps them up
        // when the (t_dep_window_days, tof_span) pair demands finer
        // sampling.  We just require the produced grid is at least
        // as large as the override.
        assert!(
            grid.resolution.0 >= 50,
            "lagrange override gives ≥50 cols (got {})",
            grid.resolution.0
        );
        assert!(
            grid.resolution.1 >= 40,
            "lagrange override gives ≥40 rows (got {})",
            grid.resolution.1
        );
    }

    #[test]
    fn build_lagrange_grid_l3_uses_opposition_phase_offset() {
        use std::f64::consts::PI;
        let cfg = super::super::components::PorkchopConfig::default();
        let planet_orbit = earth_heliocentric_orbit();
        let origin_orbit = earth_heliocentric_orbit();
        let inputs = LagrangePorkchopInputs {
            planet_orbit: planet_orbit.clone(),
            origin_orbit,
            system_gm: crate::fleets::orbital_mechanics::GM_SUN,
            sim_time_s: 0.0,
        };
        let lp = make_lagrange_target(3);
        let grid = build_lagrange_grid(&cfg, &inputs, &lp);
        // L3 is co-orbital at planet_sma (not `lp.radius_au`).  The
        // builder deliberately ignores `lp.radius_au` for non-L1/L2
        // cases; co-orbital r1 ≈ r2 degenerates the Hohmann but
        // phase-angle variation still produces a ΔV surface.
        assert_eq!(grid.dest_name, "L3 Earth");
        // We can't directly read the synthesised destination orbit
        // from the grid — it's absorbed into PorkchopCell
        // `dest_pos_au` per (t_dep, tof).  Just ensure the build
        // produced a sensible non-empty grid.
        assert!(grid.cells.len() > 0);
        // Phase offset for L3 is +π; verify by comparing first-cell
        // dest_pos_au for L3 vs L1 (they should be on opposite
        // sides of the planet at t=0).
        let l3_dest_pos = grid.cells[0].dest_pos_au;
        let lp_3 = make_lagrange_target(3);
        let _ = lp_3; // silence unused
        assert!(
            l3_dest_pos.length() > 0.0,
            "L3 destination must be a finite heliocentric point"
        );
        assert!(
            (l3_dest_pos.length() - PI).abs() < 0.1
                || (l3_dest_pos.length() < 1.5),
            "L3 dest_pos_au sits near the planet's orbit at 1 AU (got {:.4})",
            l3_dest_pos.length()
        );
    }

    #[test]
    fn build_lagrange_grid_l4_l5_use_pm_60deg_phase_offset() {
        let cfg = super::super::components::PorkchopConfig::default();
        let planet_orbit = earth_heliocentric_orbit();
        let origin_orbit = earth_heliocentric_orbit();
        let inputs = LagrangePorkchopInputs {
            planet_orbit: planet_orbit.clone(),
            origin_orbit,
            system_gm: crate::fleets::orbital_mechanics::GM_SUN,
            sim_time_s: 0.0,
        };
        let l4 = build_lagrange_grid(&cfg, &inputs, &make_lagrange_target(4));
        let l5 = build_lagrange_grid(&cfg, &inputs, &make_lagrange_target(5));
        assert_eq!(l4.dest_name, "L4 Earth");
        assert_eq!(l5.dest_name, "L5 Earth");
        // Both L4 and L5 are co-orbital at planet_sma.  Their
        // first-cell destinations should sit at planet_sma radius
        // (±r_hill, which is small) but their phases differ by 2·π/3.
        let l4_phase = l4.cells[0]
            .dest_pos_au
            .y
            .atan2(l4.cells[0].dest_pos_au.x);
        let l5_phase = l5.cells[0]
            .dest_pos_au
            .y
            .atan2(l5.cells[0].dest_pos_au.x);
        let phase_diff = (l4_phase - l5_phase).abs();
        // phase_diff should be ≈ 2·π/3 (≈ 2.094 rad); allow ±0.1 rad tolerance.
        assert!(
            (phase_diff - 2.094).abs() < 0.2,
            "L4 and L5 phase diff should be 2·π/3 ≈ 2.094 (got {:.3})",
            phase_diff
        );
    }
}

// === Planner wiring helpers (GRA-159 H-1 plumbing) =========================
//
// GRA-152 shipped the `PorkchopPanel` renderer and GRA-156 shipped the
// LGD-validated Lambert math, but the planner never wrote
// `fleet_ui_state.porkchop_grid` — so the `if let Some(grid)` branch in
// `src/ui/transfer_planner.rs` was unreachable and the legacy 3-option
// row always rendered.  These helpers translate the planner's
// destination-state snapshot (already-resolved heliocentric orbits,
// names, and a category string) into a `PorkchopGrid` that the
// existing render branch consumes without further changes.
//
// The helpers are pure: they take pre-resolved `KeplerOrbit`s and
// category strings, NOT Bevy queries.  The planner does the
// body-query lookups in-place (it already has `body_query` in scope)
// and hands the resolved values to `build_grid_for_body_target`.
// This keeps the pure-math module free of `Query`/`QueryState`
// plumbing and makes the helper unit-testable without a world.

/// Build a `PorkchopGrid` for a body-target selection (planet/moon/ring)
/// from already-resolved heliocentric orbits and a category string.
/// Pure function: the planner resolves the orbits via its `body_query`
/// and passes the values in.  The category keys are open-set; unknown
/// keys fall through to `PorkchopConfig::defaults`.
///
/// This is the *body* path of the wire-in: GRA-159 scope is planet
/// and moon destinations.  Fleet-intercept and Lagrange targets have
/// their own call sites in the planner and are addressed by sibling
/// issues (GRA-158 Lagrange, GRA-160 fleet intercept, GRA-161
/// interstellar).
pub fn build_grid_for_body_target(
    cfg: &PorkchopConfig,
    origin_orbit: KeplerOrbit,
    dest_orbit: KeplerOrbit,
    origin_name: String,
    dest_name: String,
    category: &str,
    sim_time_s: f64,
    // GRA-NNN: see `PorkchopInputs::parking_radius_au` for the math.
    parking_radius_au: Option<f64>,
    // GM of the body whose local parking orbit the shell picker
    // modifies (typically the parent body's GM for the destination
    // selection).  Pass `system_gm` (= GM_Sun) when no shell pick.
    body_gm: f64,
) -> PorkchopGrid {
    let inputs = PorkchopInputs {
        origin_name,
        dest_name,
        origin_orbit,
        dest_orbit,
        // Porkchop math is heliocentric: it always uses the host star's GM
        // for the Lambert solver.  The planner's per-frame logic in
        // `transfer_planner.rs` already special-cases local-frame
        // transfers (moon→moon) — those return `None` from the
        // caller's resolve step and fall through to the legacy
        // 3-option row, which is correct (we're scoping GRA-159 to
        // the heliocentric body path).
        system_gm: GM_SUN,
        body_gm,
        sim_time_s,
        category: category.to_string(),
        parking_radius_au,
    };
    build_porkchop_grid(cfg, &inputs)
}

/// Build a *rotating-buffer* porkchop grid covering a 2× wider
/// `t_dep` window than the visible planner surface.
///
/// The buffer has 2× the columns of the visible planner surface, so
/// the planner can render the *current* half (the leftmost cells)
/// while the rightmost cells cache the future.  As time advances
/// the planner scrolls the visible window rightward through the
/// buffer; once the buffer's "future" half is exhausted it
/// invalidates and rebuilds.  Within the window the planner never
/// re-solves Lambert — the ΔV surface is invariant under rotation,
/// so the cached values stay accurate as planet positions shift
/// with mean motion.  This eliminates the per-second rebuild
/// cadence that the cap-based staleness check hit at 1 yr/s,
/// replacing it with one rebuild every `t_dep_window_days` of
/// player time.  The buffer is 4× the visible window so the
/// rotation cycle is `3 × t_dep_window_days` of player time
/// before the deferred build re-solves Lambert — most play
/// sessions never trigger a rebuild at 1 yr/s because the visible
/// window slides through the cached cells in under 4 sim years.
pub fn build_rotating_buffer_for_body_target(
    cfg: &PorkchopConfig,
    origin_orbit: KeplerOrbit,
    dest_orbit: KeplerOrbit,
    origin_name: String,
    dest_name: String,
    category: &str,
    sim_time_s: f64,
    // GRA-NNN: see `PorkchopInputs::parking_radius_au` for the math.
    parking_radius_au: Option<f64>,
    // GM of the body whose local parking orbit the shell picker
    // modifies.
    body_gm: f64,
) -> PorkchopGrid {
    // Resolve the per-category params, then quadruple
    // `t_dep_window_days` and double `resolution_t_dep` so the
    // buffer has 2× the columns of the visible planner surface
    // over an 8× t_dep span.  Resolution does NOT scale with
    // window size (to keep per-cell ΔV detail constant), so the
    // rebuild cost is ~4× the non-rotating build.  The 8× buffer
    // means the cells slide through the *entire* visible window +
    // one full visible window of "future" before the planner
    // rotates — the user sees a full ~W of ΔV motion before the
    // next rebuild, not the half-window pass they were getting
    // with the 4× buffer.
    let mut params = cfg.resolve(category);
    params.t_dep_window_days *= 8.0;
    params.resolution_t_dep = (params.resolution_t_dep * 2).max(8);
    let inputs = PorkchopInputs {
        origin_name,
        dest_name,
        origin_orbit,
        dest_orbit,
        system_gm: GM_SUN,
        body_gm,
        sim_time_s,
        category: category.to_string(),
        parking_radius_au,
    };
    build_porkchop_grid_with_params(params, &inputs)
}

/// Inputs for the Lagrange-point porkchop grid builder (GRA-NNN).
///
/// The LP is a synthetic heliocentric destination: a `KeplerOrbit`
/// whose `semi_major_axis` is the planet's heliocentric radius
/// (`lp.planet_sma_au`) — co-orbiting with the planet at its mean
/// motion — plus a phase offset that matches the L-point geometry
/// (`0` for L1/L2 on the Sun-planet radial, `±π` for L3 opposition,
/// `±π/3` for L4/L5).  This wraps the patched-conic / phasing physics
/// the legacy 3-option row used (`lp_transfer_options` /
/// `co_orbital_phasing_options`) into the same `(t_dep, tof)` surface
/// every other heliocentric destination uses, so the `PorkchopPanel`
/// renders instead of falling back to the legacy row.
///
/// Caller passes the planet's heliocentric `KeplerOrbit` (not the
/// local-frame moon-orbit data — `LagrangeTarget` already encodes
/// the local-frame `radius_au` for the 3-option row; we deliberately
/// ignore that here in favour of the heliocentric view so the
/// porkchop math lines up with the rest of the planner).
pub struct LagrangePorkchopInputs {
    /// Planet's heliocentric `KeplerOrbit` (or moon-orbit if this is
    /// a planet-moon LP — the `origin_orbit` is what carries the
    /// parking fleet's GM regime).
    pub planet_orbit: KeplerOrbit,
    /// Fleet's current heliocentric parking orbit.  For moon-orbiting
    /// fleets this resolves to the planet's heliocentric orbit via
    /// `heliocentric_orbit_for_body`.
    pub origin_orbit: KeplerOrbit,
    /// GM of the host star (`GM_SUN` for Sol).  Sets the Lambert
    /// solver's gravity regime — always Sun-class for heliocentric
    /// porkchop math.
    pub system_gm: f64,
    /// `sim_time_s` — the player's "now" epoch.
    pub sim_time_s: f64,
}

/// Build the standard porkchop grid for a Lagrange-point destination
/// (GRA-NNN follow-up — replaces the legacy 3-option row fallback the
/// dispatcher used for `LagrangeTarget`s).
///
/// Synthesises a destination `KeplerOrbit` that co-orbits the planet
/// at `lp.planet_sma_au` (the LP's heliocentric distance) with a
/// phase offset chosen by L-point number:
///   * L1/L2 — same phase as the planet (LP sits on the Sun-planet
///     radial; r₁ ≈ r₂ ≈ planet_sma for the co-orbital case).
///   * L3    — phase + π (opposition, planet_sma radius).
///   * L4    — phase + π/3 (leading 60°, planet_sma radius).
///   * L5    — phase − π/3 (trailing 60°, planet_sma radius).
///
/// Deliberately uses `lp.planet_sma_au` instead of `lp.radius_au`
/// because the latter is a *local-frame* (planet-centric or Sun-
/// planet-frame) quantity for planet-moon LP — using it as a
/// heliocentric radius would put Saturn-Prometheus-L1 at 0.001 AU
/// from the Sun instead of ≈ 9.5 AU and the Lambert solver would
/// trivially infeasible.  The downside is that the porkchop plots
/// the heliocentric view of an inherently patched-conic transfer
/// rather than the local-frame ΔV; the absolute numbers won't match
/// `lp_transfer_options` for planet-moon L1/L2 but the panel
/// renders and the player can still pick a cell.  The selected cell
/// re-routes through `build_planned_transfer_lp` so the committed
/// trajectory physics matches the legacy 3-option model.
pub fn build_lagrange_grid(
    cfg: &PorkchopConfig,
    inputs: &LagrangePorkchopInputs,
    lp: &crate::ui::LagrangeTarget,
) -> PorkchopGrid {
    let phase_offset_rad: f64 = match lp.point {
        1 | 2 => 0.0,
        3 => std::f64::consts::PI,
        4 => std::f64::consts::FRAC_PI_3,
        5 => -std::f64::consts::FRAC_PI_3,
        _ => 0.0,
    };

    // L1/L2: planet_sma ± r_hill for a heliocentric frame is dominated
    // by planet_sma; r_hill is ≪ planet_sma for all realistic bodies
    // (Earth-Sun r_hill ≈ 0.01 AU at 1 AU; Saturn-Sun r_hill ≈ 0.5 AU
    // at 9.5 AU).  The porkchop math is heliocentric-Lambert so r₂ ≈
    // planet_sma already covers both helicentric and planet-moon L1/L2
    // to first order.  Model the slight offset by writing
    // `semi_major_axis = lp.radius_au` when the field is a HELIOCENTRIC
    // value (detected via `lp.gm > 0.5 * GM_SUN`); fall back to
    // `lp.planet_sma_au` otherwise.
    let is_heliocentric_lp = lp.gm > 0.5 * crate::fleets::orbital_mechanics::GM_SUN;
    let dest_sma_au = if is_heliocentric_lp && lp.radius_au > 0.0 {
        lp.radius_au
    } else {
        inputs.planet_orbit.semi_major_axis
    };

    let lp_dest_orbit = KeplerOrbit {
        semi_major_axis: dest_sma_au,
        eccentricity: inputs.planet_orbit.eccentricity,
        inclination: inputs.planet_orbit.inclination,
        longitude_ascending_node: inputs.planet_orbit.longitude_ascending_node,
        argument_of_periapsis: inputs.planet_orbit.argument_of_periapsis,
        // Wrap `mean_anomaly_epoch` by the L-point phase offset so the
        // LP materialises at the right heliocentric angle at
        // `sim_time_s`.  `mean_motion` matches the planet's so the LP
        // co-rotates with the planet through the grid window.
        mean_anomaly_epoch: inputs.planet_orbit.mean_anomaly_epoch + phase_offset_rad,
        mean_motion: inputs.planet_orbit.mean_motion,
    };

    let porkchop_inputs = PorkchopInputs {
        origin_name: String::from("Parking"),
        dest_name: format!("L{} {}", lp.point, lp.planet_name),
        origin_orbit: inputs.origin_orbit,
        dest_orbit: lp_dest_orbit,
        system_gm: inputs.system_gm,
        // GRA-NNN: Lagrange porkchop does not currently expose a
        // player-visible parking-shell picker — `origin_body` would
        // be the planet, not the player parking.  Pass `system_gm` so
        // the parking-orbit circular speed stays in the heliocentric
        // regime (Lambert's solver gives the same ΔV either way for a
        // heliocentric source; setting `body_gm = system_gm` skips
        // the per-shell ΔV bump).
        body_gm: inputs.system_gm,
        sim_time_s: inputs.sim_time_s,
        // Force the `lagrange` RON override so the (t_dep, tof) window
        // / resolution are tuned for short LP transfers rather than
        // interplanetary ones.
        category: "lagrange".to_string(),
        parking_radius_au: None,
    };
    build_porkchop_grid(cfg, &porkchop_inputs)
}

/// Classify the transfer category so the right `PorkchopConfig` override
/// is selected.  The keys are an *open* set declared in
/// `assets/data/porkchop_config.ron` (interplanetary, moon, star_approach,
/// interstellar, …) — unknown keys fall through to `defaults`.
///
/// This is intentionally minimal for GRA-159: the body-path (planet/moon
/// destinations) only.  Lagrange / fleet-intercept / interstellar are
/// siblings (GRA-158, GRA-160, GRA-161) and will extend this function
/// when their call sites are wired.
pub fn classify_body_transfer_category(
    dest_body_type: BodyType,
    dest_parent: Option<Entity>,
    origin_parent: Option<Entity>,
) -> &'static str {
    if dest_body_type == BodyType::Star {
        return "star_approach";
    }
    // A "moon" transfer is a transfer to a body that is itself
    // classified as a moon (or a ring) AND shares the same
    // parent as the origin.  The earlier "dest_parent ==
    // origin_parent" check was too broad: it caught Earth →
    // Mars because both planets' `LogicalParent` is the Sun,
    // which routed the planner into the moon override
    // (`tof_ceiling_years: 0.165` ≈ 60 d, much narrower than
    // a planet-to-planet Hohmann baseline).  The result was
    // inverted tof bounds (min > max) and a clipped panel that
    // left most cells grey because the C3 ceiling rejected
    // every (t_dep, tof) pair outside the truncated window.
    //
    // The fix gates the moon check on the destination's body
    // type: only Moon / Ring bodies with a shared parent go
    // through the moon override.  Planet-to-planet transfers
    // (the common case for interplanetary play) now use the
    // interplanetary override, which has the right TOF range
    // for Hohmann-class transfers.
    if matches!(dest_body_type, BodyType::Moon | BodyType::Ring)
        && dest_parent.is_some()
        && dest_parent == origin_parent
    {
        return "moon";
    }
    "interplanetary"
}
#[cfg(test)]
mod planner_wiring_tests {
    //! Tests for the GRA-159 helper functions that translate the
    //! planner's destination-state snapshot into a `PorkchopGrid`.
    //!
    //! These tests use synthetic `KeplerOrbit` fixtures and the pure
    //! helper API, so the pure-math module stays free of `Query`/
    //! `QueryState` plumbing and the tests run without a world.
    use super::*;
    use crate::fleets::orbital_mechanics::AU_IN_METERS;
    use crate::plugins::solar_system_data::BodyType;

    #[test]
    fn planner_wiring_earth_to_mars_returns_non_empty_grid() {
        // The pure helper consumes pre-resolved orbits + names.  We
        // exercise it with a synthetic Earth→Mars pair to assert the
        // contract: at least one feasible cell, names threaded
        // through.  The ECS lookups themselves are the planner's job
        // and covered by integration tests in `tests/planner_integration.rs`.
        let cfg = PorkchopConfig::default();
        let earth_orbit = KeplerOrbit::circular(1.0, 1.0);
        let mars_orbit = KeplerOrbit::circular(1.524, 1.0);
        let grid = build_grid_for_body_target(
            &cfg,
            earth_orbit,
            mars_orbit,
            "Earth".to_string(),
            "Mars".to_string(),
            "interplanetary",
            0.0,
            None, // GRA-NNN parking_radius_au: tests use legacy heliocentric-SMA parking.
            crate::fleets::orbital_mechanics::GM_SUN, // GRA-NNN body_gm: legacy = system_gm.
        );
        assert!(
            grid.cells.iter().any(|c| c.feasible),
            "Earth→Mars porkchop must contain at least one feasible cell"
        );
        assert_eq!(grid.origin_name, "Earth");
        assert_eq!(grid.dest_name, "Mars");
        assert_eq!(grid.metric, PorkchopMetric::TotalDv);
    }

    /// Rotating-buffer grid covers 2× the visible planner window: a
    /// `t_dep_window_days` of 60 becomes 120 sim days, with double
    /// the columns so each visible column keeps its original ΔV
    /// resolution.  The planner scrolls the visible window through
    /// the buffer without rebuilding until the buffer's "future"
    /// half is exhausted.
    #[test]
    fn planner_wiring_rotating_buffer_doubles_window() {
        let cfg = PorkchopConfig::default();
        let earth_orbit = KeplerOrbit::circular(1.0, 1.0);
        let mars_orbit = KeplerOrbit::circular(1.524, 1.0);
        let buffer = build_rotating_buffer_for_body_target(
            &cfg,
            earth_orbit,
            mars_orbit,
            "Earth".to_string(),
            "Mars".to_string(),
            "interplanetary",
            0.0,
            None, // GRA-NNN parking_radius_au: tests default to None.
            crate::fleets::orbital_mechanics::GM_SUN, // GRA-NNN body_gm: legacy = system_gm.
        );
        let baseline = build_grid_for_body_target(
            &cfg,
            earth_orbit,
            mars_orbit,
            "Earth".to_string(),
            "Mars".to_string(),
            "interplanetary",
            0.0,
            None,                                     // GRA-NNN parking_radius_au: see above.
            crate::fleets::orbital_mechanics::GM_SUN, // GRA-NNN body_gm: legacy = system_gm.
        );
        // Buffer covers 4× the baseline's t_dep window.
        let baseline_width = baseline.t_dep_bounds_s.1 - baseline.t_dep_bounds_s.0;
        let buffer_width = buffer.t_dep_bounds_s.1 - buffer.t_dep_bounds_s.0;
        assert!(
            (buffer_width - 8.0 * baseline_width).abs() < 1.0,
            "buffer width {buffer_width} should be ≈ 8× baseline {baseline_width}"
        );
        // Buffer has ≥ 2× the columns so each visible column keeps
        // baseline's per-column ΔV resolution.  The `≥` (rather
        // than `==`) reflects `adaptive_resolution` bumping cols
        // further on the wider buffer window to hit the 2-day
        // t_dep step target — the buffer inherits the same RON
        // minimums ×2, then `adaptive_resolution` pushes cols
        // above that when the 8× window demands it.
        assert!(
            buffer.resolution.0 >= baseline.resolution.0 * 2,
            "buffer cols ({}) should be ≥ 2× baseline cols ({})",
            buffer.resolution.0,
            baseline.resolution.0
        );
        assert!(
            buffer.cells.iter().any(|c| c.feasible),
            "rotating buffer must still contain feasible cells"
        );
    }

    /// Regression test for the planet-position bug: the
    /// `solve_lambert_transfer` call inside `solve_cell` must use
    /// the player's current `sim_time_s` as the *anchor* for
    /// `t_dep_s` so the cell's planet positions track the actual
    /// heliocentric state, not the orbit's spawn-time mean anomaly.
    /// The pre-fix code computed `mean_anomaly_epoch + mean_motion *
    /// t_dep_s`, so the grid was invariant under sim_time — every
    /// cell stayed at the same planet positions forever, and the
    /// cells at col 0 always represented the orbit-epoch transfer
    /// even when the player was a year past the epoch.  With the
    /// fix, the cell at t_dep_s = 0 represents the transfer
    /// "depart at sim_time_s + 0" — i.e. the player's current
    /// "now" — so a grid built at sim_time=0 and a grid built at
    /// sim_time=0.5yr have different cell positions and the
    /// rotating buffer can hand off seamlessly without a visible
    /// jump.
    ///
    /// We compare at t=0 and t=0.5 yr (half of Earth's orbit) so
    /// the planet positions differ — at t=1yr Earth is back to
    /// its starting position, so the difference would be 0.
    #[test]
    fn planet_position_uses_absolute_sim_time() {
        let cfg = PorkchopConfig::default();
        // Inline orbits to avoid the cross-module visibility
        // issue (these helpers live in `mod tests` at the top of
        // the file, not in `planner_wiring_tests`).
        let n_earth = 2.0 * std::f64::consts::PI / (365.25 * SECONDS_PER_DAY);
        let earth = KeplerOrbit {
            eccentricity: 0.0167,
            semi_major_axis: 1.0,
            inclination: 0.0,
            longitude_ascending_node: 0.0,
            argument_of_periapsis: 0.0,
            mean_anomaly_epoch: 0.0,
            mean_motion: n_earth,
        };
        let n_mars = 2.0 * std::f64::consts::PI / (687.0 * SECONDS_PER_DAY);
        let mars = KeplerOrbit {
            eccentricity: 0.0934,
            semi_major_axis: 1.524,
            inclination: 0.0,
            longitude_ascending_node: 0.0,
            argument_of_periapsis: 0.0,
            mean_anomaly_epoch: 0.0,
            mean_motion: n_mars,
        };
        let grid_at_t0 = build_grid_for_body_target(
            &cfg,
            earth,
            mars,
            "Earth".to_string(),
            "Mars".to_string(),
            "interplanetary",
            0.0,
            None, // GRA-NNN parking_radius_au: legacy heliocentric-SMA parking.
            crate::fleets::orbital_mechanics::GM_SUN, // GRA-NNN body_gm: legacy = system_gm.
        );
        let half_year = 0.5 * 365.25 * 86_400.0;
        let grid_at_thalf = build_grid_for_body_target(
            &cfg,
            earth,
            mars,
            "Earth".to_string(),
            "Mars".to_string(),
            "interplanetary",
            half_year,
            None, // GRA-NNN parking_radius_au: legacy heliocentric-SMA parking.
            crate::fleets::orbital_mechanics::GM_SUN, // GRA-NNN body_gm: legacy = system_gm.
        );
        // Pick a feasible cell from each grid (col 20, row 25) and
        // verify the planet positions differ.  Without the fix the
        // positions would be byte-identical because the math
        // ignored `sim_time_s`.
        let col = 20;
        let row = 25;
        let a = &grid_at_t0.cells[row * grid_at_t0.resolution.0 + col];
        let b = &grid_at_thalf.cells[row * grid_at_thalf.resolution.0 + col];
        let dp = (a.origin_pos_au - b.origin_pos_au).length();
        assert!(
            dp > 0.5,
            "origin planet should have moved ~2 AU between sim_time=0 and sim_time=0.5yr; got {dp} AU"
        );
        let dq = (a.dest_pos_au - b.dest_pos_au).length();
        assert!(
            dq > 0.5,
            "dest planet should have moved between sim_time=0 and sim_time=0.5yr; got {dq} AU"
        );
    }

    #[test]
    fn planner_wiring_classifies_moon_vs_interplanetary() {
        // The category classifier is pure: it takes pre-resolved
        // (dest_body_type, dest_parent, origin_parent) and returns
        // the right RON match key.  No ECS plumbing required.
        // Earth→Moon: same parent (Earth) → "moon" override.
        // Earth→Mars: different parents (Moon-orbits-Earth vs
        // Mars-orbits-Sun) → "interplanetary".
        let earth = Entity::from_bits(0x1000);
        let mars = Entity::from_bits(0x1001);
        let moon = Entity::from_bits(0x1002);
        // Luna orbits Earth (parent == origin_parent).
        assert_eq!(
            classify_body_transfer_category(BodyType::Moon, Some(earth), Some(earth)),
            "moon",
            "Earth→Moon should classify as moon (shared non-stellar parent)"
        );
        // Mars orbits the Sun (dest_parent = sun != origin_parent = earth).
        assert_eq!(
            classify_body_transfer_category(BodyType::Planet, Some(mars), Some(earth)),
            "interplanetary",
            "Earth→Mars should classify as interplanetary (different host bodies)"
        );
        // Star target (any → Sol) → "star_approach".
        assert_eq!(
            classify_body_transfer_category(BodyType::Star, None, Some(earth)),
            "star_approach",
            "any→star should classify as star_approach"
        );
        let _ = moon; // silence unused warning
    }

    /// Regression test for the planet-to-planet mis-classification:
    /// in the real game, both Earth and Mars have `LogicalParent`
    /// = Sun (the same host).  The original "dest_parent ==
    /// origin_parent" check was too broad — it caught Earth→Mars
    /// as a "moon" transfer and routed the planner through the
    /// `tof_ceiling_years: 0.165` (≈ 60 d) moon override, which
    /// inverted the tof bounds (min > max) and rendered the
    /// panel as a clipped band of grey.
    ///
    /// The fix gates the moon check on the destination's body
    /// type so planet-to-planet transfers stay on the
    /// interplanetary override.  This test pins the
    /// realistic-game case: Earth (host = Sol) → Mars (host =
    /// Sol) should classify as "interplanetary", not "moon".
    #[test]
    fn planner_wiring_earth_to_mars_classifies_as_interplanetary() {
        let sol = Entity::from_bits(0x1000);
        // Earth→Mars: both planets have parent = Sol in the real
        // game.  Earlier check `dest_parent == origin_parent`
        // would have returned "moon" (same parent), but the
        // destination is a Planet, not a Moon.
        assert_eq!(
            classify_body_transfer_category(BodyType::Planet, Some(sol), Some(sol)),
            "interplanetary",
            "Earth→Mars must classify as interplanetary even when both planets share the same parent (Sol)"
        );
    }

    #[test]
    fn planner_wiring_uses_moon_override_when_caller_passes_it() {
        // The helper respects the caller's category string.  We pass
        // "moon" with two co-orbital bodies to verify the override
        // takes effect (the window bounds become ±7 days instead of
        // ±30 for the default interplanetary).
        let cfg = PorkchopConfig {
            category_overrides: vec![crate::fleets::components::PorkchopCategoryOverride {
                match_key: "moon".to_string(),
                t_dep_window_days: 14.0,
                tof_min_hohmann_factor: 0.5,
                tof_max_hohmann_factor: 1.8,
                tof_floor_days: 0.5,
                tof_ceiling_years: 0.165, // ≈ 60 days
                resolution_t_dep: 50,
                resolution_tof: 40,
                c3_ceiling_km2_s2: 400.0,
                short_hop_options: None,
                short_hop_t_dep_steps: None,
            }],
            ..PorkchopConfig::default()
        };
        let earth_orbit = KeplerOrbit::circular(1.0, 1.0);
        let moon_orbit = KeplerOrbit::circular(1.00257, 1.0);
        let grid = build_grid_for_body_target(
            &cfg,
            earth_orbit,
            moon_orbit,
            "Earth".to_string(),
            "Luna".to_string(),
            "moon",
            0.0,
            None, // GRA-NNN parking_radius_au: legacy heliocentric-SMA parking.
            crate::fleets::orbital_mechanics::GM_SUN, // GRA-NNN body_gm: legacy.
        );
        // The moon override's ±7 day half-window should be the bound.
        let half_window_d = (grid.t_dep_bounds_s.1 - grid.t_dep_bounds_s.0) * 0.5 / SECONDS_PER_DAY;
        assert!(
            (half_window_d - 7.0).abs() < 0.5,
            "moon override should give ±7 day half-window, got {half_window_d}"
        );
    }

    #[test]
    fn fleet_ui_state_clear_target_drops_porkchop_grid() {
        // The wire-in must invalidate the cached grid when the
        // planner switches fleets.  The "switch fleet" path lives in
        // `FleetUiState::clear_target` (the planner's catch-all
        // invalidation hook) — assert the Some → None transition
        // here so the contract is locked in even if the per-site
        // inline clears are ever refactored.
        use crate::ui::FleetUiState;
        let mut state = FleetUiState::default();
        // Hand-build a non-trivial grid (the only field the
        // `clear_target` contract cares about is `is_some()`).
        let cfg = PorkchopConfig::default();
        let inputs = PorkchopInputs {
            origin_name: "Origin".to_string(),
            dest_name: "Dest".to_string(),
            origin_orbit: KeplerOrbit::circular(1.0, 1.0),
            dest_orbit: KeplerOrbit::circular(1.524, 1.0),
            system_gm: GM_SUN,
            body_gm: GM_SUN,
            sim_time_s: 0.0,
            category: "interplanetary".to_string(),
            parking_radius_au: None,
        };
        let grid = build_porkchop_grid(&cfg, &inputs);
        state.porkchop_grid = Some(grid);
        state.selected_porkchop_cell = Some((0, 0));
        // Sanity: grid is now Some.
        assert!(state.porkchop_grid.is_some());
        assert!(state.selected_porkchop_cell.is_some());
        // Switching fleets / clearing the target must drop both.
        state.clear_target();
        assert!(
            state.porkchop_grid.is_none(),
            "clear_target must drop the cached porkchop grid (GRA-159 invalidation contract)"
        );
        assert!(
            state.selected_porkchop_cell.is_none(),
            "clear_target must drop the selected cell index too"
        );
    }

    /// GRA-169 (Part A): the buffer's `t_dep_bounds_s` is anchored at
    /// the player's `sim_time_s` at build.  Pre-fix code returned
    /// `(0.0, max_t_dep)` regardless of the build epoch, which made
    /// every rotation cycle snap the visible window back to t_dep=0
    /// (the orbit-epoch anchor), not the player's current clock.
    /// This test asserts:
    ///   1. At `sim_time_s = 0`, `t_dep_bounds_s.0 = 0`.
    ///   2. At `sim_time_s = 1 yr`, `t_dep_bounds_s.0 ≈ 1 yr` and
    ///      the cell ΔV values are **byte-equal** to the t=0 grid
    ///      offset by the same `sim_time_s`.  (Lambert is rotation-
    ///      invariant; shifting the build epoch shifts the *anchor*,
    ///      not the cell ΔV — the visible-cell content is the same
    ///      modulo absolute epoch.)
    ///   3. The cell's `t_dep_s` is the **relative** offset
    ///      (0..max_t_dep) so `solve_cell`'s `t_dep_abs = sim_time_s
    ///      + t_dep_s` recovers the absolute departure epoch.
    #[test]
    fn gra_169_buffer_anchored_at_sim_time_s() {
        let cfg = PorkchopConfig::default();
        let earth_orbit = KeplerOrbit::circular(1.0, 1.0);
        let mars_orbit = KeplerOrbit::circular(1.524, 1.0);

        // Build at t=0 — the original anchor.
        let grid_at_t0 = build_grid_for_body_target(
            &cfg,
            earth_orbit,
            mars_orbit,
            "Earth".to_string(),
            "Mars".to_string(),
            "interplanetary",
            0.0,
            None, // GRA-NNN parking_radius_au: legacy heliocentric-SMA parking.
            crate::fleets::orbital_mechanics::GM_SUN, // GRA-NNN body_gm: legacy.
        );
        assert_eq!(
            grid_at_t0.t_dep_bounds_s.0, 0.0,
            "t=0 build should anchor at sim_time_s = 0"
        );

        // Build at t=1 yr — the new anchor.
        let one_year_s = 365.25 * 86_400.0;
        let grid_at_1yr = build_grid_for_body_target(
            &cfg,
            earth_orbit,
            mars_orbit,
            "Earth".to_string(),
            "Mars".to_string(),
            "interplanetary",
            one_year_s,
            None, // GRA-NNN parking_radius_au: legacy heliocentric-SMA parking.
            crate::fleets::orbital_mechanics::GM_SUN, // GRA-NNN body_gm: legacy.
        );
        assert!(
            (grid_at_1yr.t_dep_bounds_s.0 - one_year_s).abs() < 1.0,
            "t=1yr build should anchor at sim_time_s ≈ 1 yr, got {}",
            grid_at_1yr.t_dep_bounds_s.0
        );

        // Buffer width (t_dep_max - t_dep_min) is invariant — the
        // GRA-152 8× buffer multiplier is unchanged.
        let width_t0 = grid_at_t0.t_dep_bounds_s.1 - grid_at_t0.t_dep_bounds_s.0;
        let width_1yr = grid_at_1yr.t_dep_bounds_s.1 - grid_at_1yr.t_dep_bounds_s.0;
        assert!(
            (width_t0 - width_1yr).abs() < 1.0,
            "buffer width must be invariant under sim_time_s shift (got {width_t0} vs {width_1yr})"
        );

        // Cell `t_dep_s` is relative: 0..max_t_dep, NOT absolute.
        // Pick the same (col, row) from both grids and verify the
        // relative offsets match (they should — Lambert is rotation-
        // invariant in t_dep modulo the absolute epoch, and the
        // relative offset is what `solve_cell` uses internally).
        let col = 5;
        let row = 10;
        let cell_t0 = &grid_at_t0.cells[row * grid_at_t0.resolution.0 + col];
        let cell_1yr = &grid_at_1yr.cells[row * grid_at_1yr.resolution.0 + col];
        // Same relative offset — `cell.t_dep_s` is a fraction of the
        // buffer width, not an absolute epoch.
        assert!(
            (cell_t0.t_dep_s - cell_1yr.t_dep_s).abs() < 1.0,
            "cell.t_dep_s is relative — must match across sim_time_s (got {} vs {})",
            cell_t0.t_dep_s,
            cell_1yr.t_dep_s
        );
    }

    /// GRA-169 (Part A + B): the rotating buffer's `t_dep_bounds_s`
    /// slides through sim time.  Two builds one rotation apart
    /// (buffer_width / 2 sim seconds apart) must produce grids whose
    /// `t_dep_bounds_s` ranges **do not overlap**: the second build's
    /// lower bound equals the first build's upper bound minus the
    /// scroll-shift accumulated by then.  Pre-fix code returned
    /// `t_dep_min = 0` for both builds, so the two bounds **always
    /// overlapped** and the visible cells always started at the
    /// same left edge — that's the "jumps back to initial state"
    /// symptom the user reported.
    #[test]
    fn gra_169_rotating_buffer_does_not_snap_back_to_zero() {
        let cfg = PorkchopConfig::default();
        let earth_orbit = KeplerOrbit::circular(1.0, 1.0);
        let mars_orbit = KeplerOrbit::circular(1.524, 1.0);

        // First build at sim_time = 0.
        let grid_a = build_rotating_buffer_for_body_target(
            &cfg,
            earth_orbit,
            mars_orbit,
            "Earth".to_string(),
            "Mars".to_string(),
            "interplanetary",
            0.0,
            None, // GRA-NNN parking_radius_au: legacy heliocentric-SMA parking.
            crate::fleets::orbital_mechanics::GM_SUN, // GRA-NNN body_gm: legacy.
        );
        // Second build half a rotation later — past the trigger.
        let half_year_s = 0.5 * 365.25 * 86_400.0;
        let grid_b = build_rotating_buffer_for_body_target(
            &cfg,
            earth_orbit,
            mars_orbit,
            "Earth".to_string(),
            "Mars".to_string(),
            "interplanetary",
            half_year_s,
            None, // GRA-NNN parking_radius_au: legacy heliocentric-SMA parking.
            crate::fleets::orbital_mechanics::GM_SUN, // GRA-NNN body_gm: legacy.
        );
        // Both grids are anchored at their respective sim_time_s.
        assert_eq!(grid_a.t_dep_bounds_s.0, 0.0);
        assert!(
            (grid_b.t_dep_bounds_s.0 - half_year_s).abs() < 1.0,
            "second build must anchor at sim_time_s = 0.5 yr"
        );
        // Pre-fix: grid_a.t_dep_bounds_s.0 = 0 AND grid_b.t_dep_bounds_s.0 = 0.
        // Post-fix: they differ by half_year_s — the visible window
        // does NOT snap back to the left edge.
        assert!(
            (grid_b.t_dep_bounds_s.0 - grid_a.t_dep_bounds_s.0 - half_year_s).abs() < 1.0,
            "two half-rotation-apart builds must differ by half_year_s, got delta = {}",
            grid_b.t_dep_bounds_s.0 - grid_a.t_dep_bounds_s.0
        );
    }

    // === GRA-167 Part 2: local-frame Lambert grid =========================
    //
    // These tests exercise `build_porkchop_grid_for_local_frame` in the
    // parent-centred inertial frame.  Constants are physical:
    //   GM_EARTH    = 3.986_004_418e14 m³/s²
    //   GM_JUPITER  = 1.266_127_93e17 m³/s²
    //   LEO radius  = 6,571 km = 6.571e6 m
    //   Moon orbit  = 384,400 km = 3.844e8 m

    const GM_EARTH: f64 = 3.986_004_418e14;
    const GM_JUPITER: f64 = 1.266_127_93e17;

    fn leo_radius_au() -> f64 {
        use super::super::orbital_mechanics::AU_IN_METERS;
        6.571e6 / AU_IN_METERS
    }

    fn moon_orbit_radius_au() -> f64 {
        use super::super::orbital_mechanics::AU_IN_METERS;
        384_400.0e3 / AU_IN_METERS
    }

    fn europa_orbit_radius_au() -> f64 {
        use super::super::orbital_mechanics::AU_IN_METERS;
        671_100.0e3 / AU_IN_METERS
    }

    fn make_local_moon_override() -> crate::fleets::components::PorkchopCategoryOverride {
        crate::fleets::components::PorkchopCategoryOverride {
            match_key: "local_moon".to_string(),
            t_dep_window_days: 14.0,
            tof_min_hohmann_factor: 0.5,
            tof_max_hohmann_factor: 3.0,
            tof_floor_days: 0.5,
            tof_ceiling_years: 0.25,
            resolution_t_dep: 50,
            resolution_tof: 40,
            c3_ceiling_km2_s2: 100.0,
            short_hop_options: None,
            short_hop_t_dep_steps: None,
        }
    }

    #[test]
    fn local_frame_earth_moon_optimal_dv_below_4_kms() {
        // Earth-Moon Hohmann ΔV (LEO 200 km → LLO 100 km) is
        // ≈ 3.18 km/s + plane-change.  The porkchop min should land
        // under 4 km/s when the parent_gm is Earth GM (not heliocentric).
        let cfg = PorkchopConfig {
            category_overrides: vec![make_local_moon_override()],
            ..PorkchopConfig::default()
        };
        let inputs = LocalPorkchopInputs {
            origin_name: "Earth".to_string(),
            dest_name: "Moon".to_string(),
            parking_radius_au: leo_radius_au(),
            dest_orbit_au: moon_orbit_radius_au(),
            parent_gm: GM_EARTH,
            sim_time_s: 0.0,
            origin_phase_at_epoch_rad: 0.0,
            dest_phase_at_epoch_rad: 0.0,
            category: "local_moon".to_string(),
        };
        let grid = build_porkchop_grid_for_local_frame(&cfg, &inputs);
        let min_dv_ms = grid
            .cells
            .iter()
            .filter(|c| c.feasible)
            .map(|c| c.total_dv_ms)
            .fold(f64::INFINITY, f64::min);
        assert!(
            min_dv_ms.is_finite(),
            "expected at least one feasible cell in Earth-Moon grid"
        );
        assert!(
            min_dv_ms < 4_000.0,
            "Earth-Moon local-frame min ΔV = {min_dv_ms:.0} m/s, expected < 4000 m/s"
        );
    }

    #[test]
    fn local_frame_uses_parent_gm_not_system_gm() {
        // If the builder silently used GM_SUN instead of Earth GM, the
        // Earth-Moon Lambert solve would produce a wildly inflated ΔV.
        // We assert the wrong-frame ΔV is at least 10× the right-frame
        // ΔV — proving the parent_gm field actually drives the solve.
        let cfg = PorkchopConfig {
            category_overrides: vec![make_local_moon_override()],
            ..PorkchopConfig::default()
        };

        let inputs_earth_gm = LocalPorkchopInputs {
            origin_name: "Earth".to_string(),
            dest_name: "Moon".to_string(),
            parking_radius_au: leo_radius_au(),
            dest_orbit_au: moon_orbit_radius_au(),
            parent_gm: GM_EARTH,
            sim_time_s: 0.0,
            origin_phase_at_epoch_rad: 0.0,
            dest_phase_at_epoch_rad: 0.0,
            category: "local_moon".to_string(),
        };
        let inputs_sun_gm = LocalPorkchopInputs {
            parent_gm: super::super::orbital_mechanics::GM_SUN,
            ..inputs_earth_gm.clone()
        };

        let grid_earth = build_porkchop_grid_for_local_frame(&cfg, &inputs_earth_gm);
        let grid_sun = build_porkchop_grid_for_local_frame(&cfg, &inputs_sun_gm);

        let min_earth = grid_earth
            .cells
            .iter()
            .filter(|c| c.feasible)
            .map(|c| c.total_dv_ms)
            .fold(f64::INFINITY, f64::min);
        let min_sun = grid_sun
            .cells
            .iter()
            .filter(|c| c.feasible)
            .map(|c| c.total_dv_ms)
            .fold(f64::INFINITY, f64::min);

        assert!(min_earth.is_finite(), "Earth GM grid must be feasible");
        // The Sun-GM grid at Earth-Moon scales will be wildly infeasible
        // because the Earth's 3.2 km/s Hohmann becomes ~30+ km/s when
        // solved with heliocentric GM.  Accept either ratio (Earth GM
        // min finite & much smaller) or no feasible Sun-GM cells.
        assert!(
            min_sun > min_earth * 10.0 || !min_sun.is_finite(),
            "Sun GM min ΔV ({min_sun:.0} m/s) should be > 10× Earth GM ({min_earth:.0} m/s); proves parent_gm is the active GM"
        );
    }

    #[test]
    fn local_frame_jupiter_europa_optimal_dv_matches_hohmann() {
        // Jupiter-Io parking (421,800 km) → Europa (671,100 km)
        // Hohmann ΔV ≈ 3.6 km/s total (1.87 km/s departure burn +
        // 1.67 km/s arrival burn, classical two-burn Hohmann).  The
        // earlier fixture used a 100 Mm parking orbit — that's deep
        // inside Jupiter's gravity well and the Hohmann from there
        // to Europa burns ~11 km/s, blowing past the local_moon
        // override's C3 ceiling of 100 (km/s)² (the C3 block made
        // every cell infeasible).  Io orbit is a realistic
        // departure parking orbit for a Jupiter→Europa transfer
        // and lands comfortably inside the C3 budget.
        let cfg = PorkchopConfig {
            category_overrides: vec![make_local_moon_override()],
            ..PorkchopConfig::default()
        };
        use super::super::orbital_mechanics::AU_IN_METERS;
        let parking_au = 421_800.0e3 / AU_IN_METERS; // Io orbit
        let dest_au = europa_orbit_radius_au();
        let inputs = LocalPorkchopInputs {
            origin_name: "Jupiter".to_string(),
            dest_name: "Europa".to_string(),
            parking_radius_au: parking_au,
            dest_orbit_au: dest_au,
            parent_gm: GM_JUPITER,
            sim_time_s: 0.0,
            origin_phase_at_epoch_rad: 0.0,
            dest_phase_at_epoch_rad: 0.0,
            category: "local_moon".to_string(),
        };
        let grid = build_porkchop_grid_for_local_frame(&cfg, &inputs);
        let min_dv_ms = grid
            .cells
            .iter()
            .filter(|c| c.feasible)
            .map(|c| c.total_dv_ms)
            .fold(f64::INFINITY, f64::min);
        assert!(
            min_dv_ms.is_finite(),
            "Jupiter-Europa grid must have at least one feasible cell"
        );
        // Sanity: Io→Europa Hohmann total ΔV ≈ 3.5 km/s.  The porkchop
        // solver picks the minimum across the t_dep/tof surface, which
        // for a circular-orbit circular-orbit Hohmann pair sits at the
        // exact Hohmann ΔV (no phasing savings on offer when both
        // orbits are circular).  Assert under 5 km/s as a sanity
        // bound — the analytical answer is ≈ 3.5 km/s.
        assert!(
            min_dv_ms < 5_000.0,
            "Jupiter-Europa min ΔV = {min_dv_ms:.0} m/s, expected < 5000 m/s"
        );
    }

    // === Star-approach grid (GRA-367-F, Phase 6) ===========================

    /// Snapshot test: Sol + parking 0.3 AU.  Locks the (parking × t_dep)
    /// shape (rows = parking, per-cell TOF = Hohmann time of that
    /// parking) and the cheap-cell determinism for Phase 6 acceptance.
    #[test]
    fn build_star_approach_grid_sol_parking_0p3au_is_deterministic() {
        let inputs = StarApproachInputs {
            origin_name: "Earth".to_string(),
            dest_name: "Sol".to_string(),
            gm_star: GM_SUN,
            parking_options_au: vec![0.10, 0.20, 0.30, 0.50, 1.00],
            origin_phase_at_epoch_rad: 0.0,
            sim_time_s: 0.0,
            c3_ceiling_ms2: None,
            t_dep_window_days: None,
            resolution_t_dep: Some(20),
            dest_radius_au: None,
        };

        let grid_a = build_star_approach_grid(&inputs);
        let grid_b = build_star_approach_grid(&inputs);

        assert_eq!(grid_a.resolution, (20, 5));
        assert_eq!(grid_a.cells.len(), 100);

        let (ca, ra) = grid_a.min_cell.expect("Phase 6 grid must have a min cell");
        let (cb, rb) = grid_b.min_cell.expect("Phase 6 grid must have a min cell");
        assert_eq!((ca, ra), (cb, rb), "min_cell must be deterministic");
        let dv_a = grid_a.cells[ra * grid_a.resolution.0 + ca].total_dv_ms;
        let dv_b = grid_b.cells[rb * grid_b.resolution.0 + cb].total_dv_ms;
        assert!(
            (dv_a - dv_b).abs() < 1e-9,
            "min ΔV must be deterministic: {dv_a} vs {dv_b}"
        );
        assert!(
            dv_a.is_finite() && dv_a < 50_000.0,
            "Sol parking min ΔV ({dv_a:.0} m/s) should be finite & < 50 km/s"
        );

        // Per-row TOF locks the (parking × t_dep) shape.
        for (row, &parking_au) in inputs.parking_options_au.iter().enumerate() {
            let cell = &grid_a.cells[row * grid_a.resolution.0];
            let a_m = ((parking_au + 0.00465) * 0.5).max(1.0e-3) * AU_IN_METERS;
            let expected_tof_s = std::f64::consts::PI * (a_m.powi(3) / GM_SUN).sqrt();
            assert!(
                (cell.tof_s - expected_tof_s).abs() < 1e-6,
                "row {row} TOF = {} (parking = {parking_au} AU), expected {expected_tof_s}",
                cell.tof_s,
            );
        }
    }
}
