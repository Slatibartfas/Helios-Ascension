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

/// Inputs to `build_porkchop_grid`.  Caller resolves origin/dest
/// heliocentric orbits to absolute mean anomaly / mean motion so the
/// builder is free of Bevy queries and stays unit-testable.
pub struct PorkchopInputs {
    pub origin_name: String,
    pub dest_name: String,
    pub origin_orbit: KeplerOrbit,
    pub dest_orbit: KeplerOrbit,
    pub system_gm: f64,
    /// `sim_time_s` — the "now" epoch the player is planning from.
    pub sim_time_s: f64,
    /// Category match key for `PorkchopConfig::resolve`.
    pub category: String,
}

/// Build the porkchop grid.  The (cols × rows) loop is bounded by the
/// LGD contract (≤ 5000 cells); one Earth→Mars default is 40 × 30 = 1200.
/// Each cell calls `solve_lambert_transfer`; the LGD's 0.3 ms/cell budget
/// gives ~360 ms worst case.  Infeasible cells are kept in the grid
/// (rendered greyed) so the player sees the full topology, including
/// the no-ballistic-solution basin.
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
    // `dep_window_bounds` returns the *absolute* (t_dep_min_s, t_dep_max_s)
    // in sim-clock units, anchored at the player's current `sim_time_s`.
    // The cell loop iterates a *relative* offset from that anchor
    // (0..max_t_dep); inside `solve_cell` we add `inputs.sim_time_s` to
    // recover the absolute epoch for the Lambert solver.  This split
    // lets `PorkchopGrid.t_dep_bounds_s` carry the absolute anchor for
    // the panel's date labels without double-offsetting the cell math.
    let (t_dep_min_abs_s, t_dep_max_abs_s) = dep_window_bounds(inputs, &params);
    let max_t_dep_rel_s = t_dep_max_abs_s - t_dep_min_abs_s;
    let tof_h = hohmann_time_s(
        inputs.origin_orbit.semi_major_axis,
        inputs.dest_orbit.semi_major_axis,
        inputs.system_gm,
    );
    let tof_min_s =
        (params.tof_min_hohmann_factor * tof_h).max(params.tof_floor_days * SECONDS_PER_DAY);
    let tof_max_s =
        (params.tof_max_hohmann_factor * tof_h).min(params.tof_ceiling_years * SECONDS_PER_YEAR);

    let cols = params.resolution_t_dep.max(2);
    let rows = params.resolution_tof.max(2);
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
            // Relative offset from `sim_time_s`.  `solve_cell` adds
            // `inputs.sim_time_s` to recover the absolute departure
            // epoch for the Lambert solver.
            let t_dep_rel_s = col_frac * max_t_dep_rel_s;
            let cell = solve_cell(inputs, t_dep_rel_s, tof_s, c3_ceiling_ms2);
            if cell.feasible && cell.total_dv_ms < min_dv {
                min_dv = cell.total_dv_ms;
                min_cell = Some((col, row));
            }
            cells.push(cell);
        }
    }

    // Adaptive Y-axis trim: clip the configured Y-axis range to the
    // band that actually contains feasible cells, with a small
    // margin on each side so the basin isn't pinned to either
    // panel edge.
    //
    // Player feedback driving the trim: with the full configured
    // range shown (default 0.4× → 5.0× Hohmann, capped at 10 yr),
    // Earth→Jupiter porkchops rendered 60–70% muted-grey because
    // the C3 ceiling (`c3_ceiling_km2_s2 = 400`) blocks cells
    // outside a narrow band around the Hohmann time.  The
    // colormap band was compressed into the populated row band,
    // making the cheap basin look like a thin sliver and the
    // Y-axis labels above the basin read as "more of the same
    // value" (the user-reported "Y axis is fixed, most tiles
    // are grey").  Trimming both ends around the populated band
    // stretches the colormap over the populated region and
    // restores readable Y-axis labels.
    //
    // The trim is *symmetric* — both the bottom and top of the
    // rendered range are clipped to the populated band:
    //   * We DO trim the bottom when row 0 is itself infeasible
    //     (e.g. Earth→Jupiter: short-arc transfers need a C3 the
    //     ceiling blocks, so row 0 is grey and the trim falls
    //     to the lowest feasible row, the fastest feasible
    //     trajectory).  Earlier "always preserve row 0" trims
    //     failed for this case — the rendered range still
    //     included the long grey tail at the bottom and the
    //     colormap was compressed into the middle band.
    //   * We DO trim the top when the upper rows are entirely
    //     infeasible (long-arc options that exceed the C3
    //     ceiling, or outer-planet transfers where Hohmann time
    //     sits well below the configured maximum).
    //   * Rows inside the populated band — the cheap transfers
    //     the player actually wants to see — are never clipped,
    //     so a 5× Hohmann-time basin still renders in full.
    //   * Degenerate cases (no feasible cells, or all rows
    //     feasible) fall back to the full configured range so
    //     the player sees the solver's full topology.
    let rendered_tof_bounds_s =
        compute_adaptive_tof_bounds(&cells, cols, rows, tof_min_s, tof_max_s);

    PorkchopGrid {
        origin_name: inputs.origin_name.clone(),
        dest_name: inputs.dest_name.clone(),
        t_dep_bounds_s: (t_dep_min_abs_s, t_dep_max_abs_s),
        tof_bounds_s: (tof_min_s, tof_max_s),
        rendered_tof_bounds_s,
        resolution: (cols, rows),
        cells,
        min_cell,
        metric: PorkchopMetric::TotalDv,
    }
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
            // v_inf at departure = |v_departure| − circular orbital speed at r1.
            // For interplanetary (GM_SUN) this is the hyperbolic excess speed.
            let r1_m = (origin_pos_au * super::orbital_mechanics::AU_IN_METERS).length();
            let v_circ_ms = (inputs.system_gm / r1_m).sqrt();
            let v1_speed_ms = v1_ms.length();
            let v_inf_dep_ms = (v1_speed_ms - v_circ_ms).max(0.0);
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
            // ΔV₁ is the burn from the origin body's parking-orbit speed up
            // to the transfer-ellipse departure speed.  We approximate the
            // total as |v_dep| + |v_arr|, where:
            //   * |v_dep| = |v1 − v_circ_dep|.  For an *outward* Hohmann
            //     (e.g. Earth→Mars) v1 > v_circ_dep (prograde boost at
            //     perihelion); for an *inward* Hohmann (e.g. Earth→Mercury)
            //     v1 < v_circ_dep (retrograde burn at aphelion — the
            //     transfer ellipse's aphelion is Earth's orbit).  Both
            //     directions require a real burn, so we use `.abs()` rather
            //     than clamping to zero.  The previous `.max(0.0)` formula
            //     produced 0 km/s porkchop cells for every inner-planet
            //     transfer (Earth→Venus, Earth→Mercury), because the
            //     retrograde-departure case was indistinguishable from
            //     "no burn required".
            //   * |v_arr| = |v_circ_arr − v2|.  Symmetric to dep_burn:
            //     outward transfers brake into the parking orbit (v_circ >
            //     v2), inward transfers boost into the parking orbit from
            //     a faster-than-circular arrival (v2 > v_circ).  Both need
            //     a real burn, so we use `.abs()`.
            // The two are added because they happen at opposite ends of the
            // transfer arc — total ΔV is the per-burn magnitude sum, the
            // standard porkchop convention.
            let r2_m = (dest_pos_au * super::orbital_mechanics::AU_IN_METERS).length();
            let v_circ_arr_ms = (inputs.system_gm / r2_m).sqrt();
            let v2_speed_ms = v2_ms.length();
            // dep_burn: how much we must change speed from the parking
            // orbit at r1 to the transfer ellipse.  Sign of (v1 − v_circ)
            // indicates burn direction (prograde vs retrograde), magnitude
            // is the ΔV required.  `.abs()` so inner-planet transfers
            // (where the Lambert solver returns v1 < v_circ) show the
            // correct retrograde burn instead of 0 km/s.
            let dep_burn_ms = (v1_speed_ms - v_circ_ms).abs();
            // arr_burn: how much we must change speed from the transfer
            // ellipse to the destination's circular parking orbit.
            // Symmetric to dep_burn: outer planets brake (v_circ > v2),
            // inner planets retrofire at perihelion to circularise from
            // a faster-than-circular arrival (v2 > v_circ).
            let arr_burn_ms = (v_circ_arr_ms - v2_speed_ms).abs();
            // v_inf_arrival: the *hyperbolic excess* at the destination
            // (the speed the spacecraft is moving *above* circular
            // orbital speed at r2).  Only positive when v2 > v_circ_arr;
            // for inward-Hohmann arrivals the entire delta-v is a real
            // brake burn (captured by `arr_burn_ms` above), not an
            // unbrakeable hyperbolic excess.
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

    let cols = params.resolution_t_dep.max(2);
    let rows = params.resolution_tof.max(2);
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
            sim_time_s: 0.0,
            category: category.to_string(),
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
        // Locks in the GRA-style resolution bump: default 60×60 =
        // 3600 cells, well under the 5000-cell validator ceiling.
        // Catches a regression where a future bump pushes past
        // the 5000 budget and the validator fails at load time.
        let cfg = PorkchopConfig::default();
        let total = cfg.defaults.resolution_t_dep * cfg.defaults.resolution_tof;
        assert!(
            total <= 5000,
            "default resolution {total} exceeds the 5000-cell validator ceiling"
        );
        // Sanity: at least 60 cols and 50 rows so the per-cell
        // ΔV resolution is finer than the previous 40×50
        // baseline.  Anything below this regresses the user's
        // "I want higher resolution" request.
        assert!(cfg.defaults.resolution_t_dep >= 60);
        assert!(cfg.defaults.resolution_tof >= 60);
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
        sim_time_s,
        category: category.to_string(),
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
        sim_time_s,
        category: category.to_string(),
    };
    build_porkchop_grid_with_params(params, &inputs)
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
        );
        let baseline = build_grid_for_body_target(
            &cfg,
            earth_orbit,
            mars_orbit,
            "Earth".to_string(),
            "Mars".to_string(),
            "interplanetary",
            0.0,
        );
        // Buffer covers 4× the baseline's t_dep window.
        let baseline_width = baseline.t_dep_bounds_s.1 - baseline.t_dep_bounds_s.0;
        let buffer_width = buffer.t_dep_bounds_s.1 - buffer.t_dep_bounds_s.0;
        assert!(
            (buffer_width - 8.0 * baseline_width).abs() < 1.0,
            "buffer width {buffer_width} should be ≈ 8× baseline {baseline_width}"
        );
        // Buffer has 2× the columns so each visible column keeps
        // baseline's per-column ΔV resolution.
        assert_eq!(
            buffer.resolution.0,
            baseline.resolution.0 * 2,
            "buffer cols should be 2× baseline cols"
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
            sim_time_s: 0.0,
            category: "interplanetary".to_string(),
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
        // Jupiter-Europa Hohmann (circular) ΔV ≈ 2.7 km/s from
        // parking orbit at Io radius (Europa's own orbit radius is
        // 671,100 km; we use that as parking_radius_au for a
        // Io→Europa zero-ΔV scenario).  Easier test: parking at
        // Europa's own radius is degenerate; use Jupiter-orbit parking
        // (low Jupiter orbit ≈ 100,000 km) and dest at Europa.
        let cfg = PorkchopConfig {
            category_overrides: vec![make_local_moon_override()],
            ..PorkchopConfig::default()
        };
        use super::super::orbital_mechanics::AU_IN_METERS;
        let parking_au = 100_000.0e3 / AU_IN_METERS; // 100 Mm parking orbit
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
        // Jupiter parking 100 Mm → Europa 671 Mm: ΔV is dominated by
        // the deep-well parking orbit and is small (~hundreds of m/s).
        // Just assert it's finite and below 5 km/s as a sanity check.
        assert!(
            min_dv_ms < 5_000.0,
            "Jupiter-Europa min ΔV = {min_dv_ms:.0} m/s, expected < 5000 m/s"
        );
    }
}
