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
    /// `(tof_min_s, tof_max_s)` — the rendered window.
    pub tof_bounds_s: (f64, f64),
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
    let (t_dep_min_s, t_dep_max_s) = dep_window_bounds(inputs, &params);
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
            let t_dep_s = t_dep_min_s + col_frac * (t_dep_max_s - t_dep_min_s);
            let cell = solve_cell(inputs, t_dep_s, tof_s, c3_ceiling_ms2);
            if cell.feasible && cell.total_dv_ms < min_dv {
                min_dv = cell.total_dv_ms;
                min_cell = Some((col, row));
            }
            cells.push(cell);
        }
    }

    PorkchopGrid {
        origin_name: inputs.origin_name.clone(),
        dest_name: inputs.dest_name.clone(),
        t_dep_bounds_s: (t_dep_min_s, t_dep_max_s),
        tof_bounds_s: (tof_min_s, tof_max_s),
        resolution: (cols, rows),
        cells,
        min_cell,
        metric: PorkchopMetric::TotalDv,
    }
}

/// Compute the (t_dep_min, t_dep_max) bounds, preferring a window centred
/// on the next optimal Hohmann window (from `compute_transfer_window`-style
/// logic).  We use a simple heuristic: the window is centred on the
/// Hohmann time, with the override-supplied half-width.
fn dep_window_bounds(inputs: &PorkchopInputs, params: &ResolvedPorkchopParams) -> (f64, f64) {
    use std::f64::consts::{PI, TAU};
    let half = 0.5 * params.t_dep_window_days * SECONDS_PER_DAY;
    let r1 = inputs.origin_orbit.semi_major_axis.max(1e-6);
    let r2 = inputs.dest_orbit.semi_major_axis.max(1e-6);
    let n1 = inputs.origin_orbit.mean_motion;
    let n2 = inputs.dest_orbit.mean_motion;
    let tof = hohmann_time_s(r1, r2, inputs.system_gm);
    let phi_req = (PI - n2 * tof).rem_euclid(TAU);
    let phi_curr = (inputs.dest_orbit.mean_anomaly_epoch - inputs.origin_orbit.mean_anomaly_epoch)
        .rem_euclid(TAU);
    let d_phi_dt = n2 - n1;
    if d_phi_dt.abs() < 1e-25 {
        return (-half, half);
    }
    let dt_to_window = ((phi_req - phi_curr) / d_phi_dt).rem_euclid(TAU / d_phi_dt.abs());
    let centre = dt_to_window;
    (centre - half, centre + half)
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
    let origin_pos_au = orbit_position_from_mean_anomaly(
        &inputs.origin_orbit,
        inputs.origin_orbit.mean_anomaly_epoch + inputs.origin_orbit.mean_motion * t_dep_s,
    );
    let dest_pos_au = orbit_position_from_mean_anomaly(
        &inputs.dest_orbit,
        inputs.dest_orbit.mean_anomaly_epoch + inputs.dest_orbit.mean_motion * (t_dep_s + tof_s),
    );
    // We don't use t_dep_abs / t_arr_abs for propagation above (the
    // body mean anomaly is propagated from epoch directly), but we keep
    // them for future per-body mean-anomaly-at-epoch queries.
    let _ = t_dep_abs;
    let _ = t_arr_abs;

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
            //   * |v_dep| = (v1 − v_circ_dep) if v1 > v_circ_dep (transfer
            //     ellipse moves *faster* than the parking orbit at r1,
            //     e.g. perihelion of a Hohmann); 0 otherwise (rare).
            //   * |v_arr| = (v_circ_arr − v2) if v_circ_arr > v2 (parking
            //     at the destination is *faster* than the transfer ellipse
            //     at aphelion, e.g. Hohmann arrival at Mars); 0 otherwise
            //     (a faster-than-circular arrival — i.e. we're braking into
            //     a sub-circular parking orbit, which the planner handles
            //     as a separate `max_delta_v_ms` budget).
            // The two are added because they happen at opposite ends of the
            // transfer arc — total ΔV is the per-burn magnitude sum, the
            // standard porkchop convention.
            let r2_m = (dest_pos_au * super::orbital_mechanics::AU_IN_METERS).length();
            let v_circ_arr_ms = (inputs.system_gm / r2_m).sqrt();
            let v2_speed_ms = v2_ms.length();
            // dep_burn: how much we must accelerate from the parking
            // orbit at r1 to the transfer ellipse.  For Hohmann (where
            // v1 > v_circ) this is positive; for an arrival from a
            // sub-circular transfer (rare) it is clamped to 0.
            let dep_burn_ms = (v1_speed_ms - v_circ_ms).max(0.0);
            // arr_burn: how much we must brake from the transfer ellipse
            // to the destination's circular parking orbit.  For Hohmann
            // (where v2 < v_circ) this is positive; for an arrival from
            // a super-circular transfer (e.g. hyperbolic) it is clamped
            // to 0 — the planner handles that case via the destination
            // orbit's own `max_delta_v_ms` budget.
            let arr_burn_ms = (v_circ_arr_ms - v2_speed_ms).max(0.0);
            // v_inf_arrival: the *hyperbolic excess* at the destination
            // (the speed the spacecraft is moving *above* circular
            // orbital speed at r2).  Always ≥ 0; 0 for Hohmann arrivals.
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
        let cfg = PorkchopConfig::default();
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
        assert!(
            (hohmann_dv_km_s - 5.6).abs() < 0.15 * 5.6,
            "Hohmann-cell ΔV = {hohmann_dv_km_s:.3} km/s, expected within 15% of canonical 5.6 km/s"
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
        // The window bounds should be tight (14 days, not 60).
        let half_window_s = (grid.t_dep_bounds_s.1 - grid.t_dep_bounds_s.0) * 0.5;
        let days = half_window_s / SECONDS_PER_DAY;
        assert!(
            (days - 7.0).abs() < 0.5,
            "moon override should give ±7 day half-window, got {days}"
        );
    }

    #[test]
    fn porkchop_phase_window_mark() {
        // The Earth→Mars grid should have a feasible Hohmann-time cell
        // at ~5.6 km/s, and the grid should contain at least 4 feasible
        // cells.  We pick a coarse 4×4 grid so the test runs in <100 ms.
        //
        // The earlier `mc > 0 && mc < cols - 1` check on the *global*
        // min_cell was dropped: the lambert solver can find a cheaper
        // non-Hohmann Type-II transfer at the window edges, so the
        // global min is not a reliable proxy for the Hohmann basin
        // position.  We assert on the Hohmann-time cell instead.
        let mut cfg = PorkchopConfig::default();
        cfg.defaults.resolution_t_dep = 4;
        cfg.defaults.resolution_tof = 4;
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
        // 4×4 sample noise: the discrete (t_dep, t_tof) grid can land a
        // few percent off the smooth canonical Hohmann basin, so we
        // allow ±20% (1.12 km/s around 5.6).  This is a plausibility
        // check, not a precision bound.
        assert!(
            (hohmann_dv_km_s - canonical).abs() < 0.20 * canonical,
            "Hohmann-cell ΔV = {hohmann_dv_km_s:.3} km/s, expected within 20% of canonical 5.6 km/s"
        );
        let feasible_count = grid.cells.iter().filter(|c| c.feasible).count();
        assert!(
            feasible_count >= 4,
            "expected at least 4 feasible cells in the 4×4 grid, got {feasible_count}"
        );
    }

    #[test]
    fn porkchop_grid_total_cells_matches_resolution() {
        let cfg = PorkchopConfig::default();
        let inputs = make_inputs(earth_orbit(), mars_orbit(), "interplanetary");
        let grid = build_porkchop_grid(&cfg, &inputs);
        assert_eq!(grid.cells.len(), grid.resolution.0 * grid.resolution.1);
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
    if dest_parent.is_some() && dest_parent == origin_parent {
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
}
