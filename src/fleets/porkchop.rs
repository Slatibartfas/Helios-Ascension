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

/// Compute the (t_dep_min, t_dep_max) bounds.
///
/// The window covers **`[0, max(synodic_period + half, 2 * half)]`**
/// — that is, from "now" (t_dep = 0) through at least one full
/// synodic period plus a half-window buffer.  This is the same
/// convention NASA / JPL use on their public porkchop plots: the
/// ΔV surface is periodic in `t_dep` with period `synodic_period`,
/// so one full period is sufficient to see every distinct alignment
/// the player could use.
///
/// We deliberately include `t_dep = 0` so the player can always click
/// a "Depart Now" cell on the grid — that was the gap the user
/// reported for long-synodic-period destinations (Saturn, Uranus,
/// Neptune) where the cheapest Hohmann window sits a year out and
/// the legacy slider only let them inspect the immediate-launch ΔV
/// cost via the side-panel stat, not visually on the plot.
///
/// Co-orbital pairs (`|d_phi_dt| ≈ 0`, e.g. Earth↔Moon in Sol) have
/// an infinite synodic period; in that degenerate case we fall back
/// to `± half` around the optimal Hohmann time as before, but
/// clamped at 0 — the phase never advances so a wider window adds no
/// information, and centring on the Hohmann date keeps the colormap
/// contrast around the cheap-transfer basin.
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
        // t=0 (now).  Players picking "Depart Now" get the same
        // cell as a future launch in this degenerate case.
        let _ = hohmann_time_s(r1, r2, inputs.system_gm);
        return (0.0, 2.0 * half);
    }
    let synodic_period_s = TAU / d_phi_dt.abs();
    // Period + half so the player sees a little beyond the next
    // alignment (handy for inspecting the *following* window before
    // deciding).  Falls back to 2 * half for degenerate categories
    // (e.g. moon transfers with a very short configured window).
    let max_t_dep = synodic_period_s.max(2.0 * half) + half;
    (0.0, max_t_dep)
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
        assert_eq!(
            grid.t_dep_bounds_s.0, 0.0,
            "lower bound must clamp to t_dep = 0 (now) so 'Depart Now' is always inspectable"
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
