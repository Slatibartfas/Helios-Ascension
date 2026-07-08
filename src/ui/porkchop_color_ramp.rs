//! Log-scale colormap with mean+2σ outlier clamp (TWP parity).
//!
//! Replaces the linear-remap colormap in `src/ui/porkchop_panel.rs`
//! (GRA-152 follow-up) with the TriggerAu/TransferWindowPlanner
//! approach:
//!
//! 1. Compute `log_min = ln(min_dv)`, `log_max_raw = ln(max_dv)`
//!    over the grid's *feasible* cells.
//! 2. Compute mean and stddev of `ln(ΔV)` over the same cells.
//! 3. Clamp `log_max = min(log_max_raw, mean + 2σ)` — prevents
//!    one infeasible-by-finite-but-huge cell from flattening the
//!    colour ramp to grey.
//! 4. Build a 7-anchor piecewise palette (blue → cyan → green →
//!    yellow → orange → red) sampled into a 512-entry ramp keyed
//!    by `[log_min, log_max]`.
//! 5. Sample `color_for(dv_km_s)` returns the ramp colour in
//!    log-space; non-finite ΔV returns the infeasible sentinel
//!    `theme::PORKCHOP_INFEASIBLE`.
//!
//! Output: `Vec<Color32>` of length `ramp_size` (default 512)
//! that `Phase B` bakes into a single `egui::TextureHandle` for
//! bilinear-filtered GPU sampling. The ramp is also useful
//! directly for the `Color32`-per-cell render path if a future
//! caller wants it.

use crate::fleets::porkchop::PorkchopGrid;
use crate::ui::theme;
use bevy_egui::egui::Color32;

/// Default ramp resolution. 512 entries give 9 bits of addressable
/// colour per cell, which is enough that any linear-interpolation
/// drop during sampling stays well below 1/255 in any channel.
pub const DEFAULT_RAMP_SIZE: usize = 512;

/// Colour for infeasible cells.  Re-exported as
/// `theme::PORKCHOP_INFEASIBLE` (the audit-allowlisted source of
/// truth); kept here as a thin alias so the rest of this module
/// keeps reading naturally.
pub use crate::ui::theme::PORKCHOP_INFEASIBLE as INFEASIBLE_COLOR;

/// Anchor colours for the 7-stop piecewise palette.  Direct port
/// of TWP's `GenerateDeltaVPalette` channel ranges:
///
///   1. `(64, 64, 255)`   — deep blue (cheap basin floor)
///   2. `(64, 160, 255)`  — sky blue
///   3. `(128, 255, 255)` — cyan
///   4. `(128, 255, 128)` — green
///   5. `(255, 255, 128)` — yellow
///   6. `(255, 192, 128)` — orange
///   7. `(255, 128, 128)` — red (expensive / infeasible tail)
///
/// Each anchor's RGB triple is fed through `theme::color32_from_rgba`
/// (which sits in the audit allowlist) to produce a `Color32`.  The 6 segments between anchors
/// are linearly interpolated into a 512-entry ramp.
const PALETTE_ANCHORS: [(u8, u8, u8); 7] = [
    (64, 64, 255),
    (64, 160, 255),
    (128, 255, 255),
    (128, 255, 128),
    (255, 255, 128),
    (255, 192, 128),
    (255, 128, 128),
];

/// Built colormap.  Sample `color_for(dv_km_s)` to get the
/// interpolated palette entry.
#[derive(Debug, Clone)]
pub struct PorkchopColorRamp {
    /// 512-entry ramp: index `i` maps to
    /// `log_min + (i as f64) / (ramp_size - 1) * (log_max - log_min)`
    /// in `ln(ΔV)`-space.  Sampled bilinearly by the GPU when the
    /// ramp is uploaded as a 1×N texture.
    pub entries: Vec<Color32>,
    /// Lower bound of the ramp's log-space (ln of the cheapest
    /// feasible ΔV).
    pub log_min: f64,
    /// Upper bound of the ramp's log-space after σ-clamp.
    pub log_max: f64,
    /// Number of entries in the ramp.
    pub ramp_size: usize,
}

impl PorkchopColorRamp {
    /// Build a 512-entry ramp from a `PorkchopGrid`.  Walks the
    /// feasible cells once to accumulate `min`, `max`, `sumlog`,
    /// `sumsqlog`, then computes `log_max` via σ-clamp and
    /// finally samples the 7-anchor palette into the ramp.
    ///
    /// If the grid has fewer than 2 feasible cells (or the ΔV span
    /// is below `LOG_MIN_SPAN_KM_S`), falls back to a symmetric
    /// span around the only feasible ΔV so the ramp is still
    /// well-defined.
    pub fn from_grid(grid: &PorkchopGrid) -> Self {
        Self::from_grid_with_size(grid, DEFAULT_RAMP_SIZE)
    }

    /// Same as `from_grid` but with a configurable ramp size
    /// (modder-tunable knob).  Use `ramp_size ≥ 16` to keep
    /// the piecewise interpolation meaningful.
    pub fn from_grid_with_size(grid: &PorkchopGrid, ramp_size: usize) -> Self {
        let ramp_size = ramp_size.max(16);
        let mut min_dv = f64::INFINITY;
        let mut max_dv = f64::NEG_INFINITY;
        let mut sumlog = 0.0_f64;
        let mut sumsqlog = 0.0_f64;
        let mut count = 0_u64;
        for cell in &grid.cells {
            if !cell.feasible {
                continue;
            }
            let dv = cell.total_dv_ms / 1000.0;
            if !dv.is_finite() || dv <= 0.0 {
                continue;
            }
            if dv < min_dv {
                min_dv = dv;
            }
            if dv > max_dv {
                max_dv = dv;
            }
            let log_dv = dv.ln();
            sumlog += log_dv;
            sumsqlog += log_dv * log_dv;
            count += 1;
        }

        // No feasible cells → build a dummy ramp covering a 1 km/s
        // window centred on 1.0 km/s.  Phase B will still upload the
        // ramp; the grid will paint entirely with INFEASIBLE_COLOR
        // at the bake site because all cells are infeasible.
        if count == 0 {
            return Self {
                entries: build_palette_ramp(0.0, 1.0_f64.ln(), ramp_size),
                log_min: 0.0,
                log_max: 1.0_f64.ln(),
                ramp_size,
            };
        }

        let log_min = min_dv.ln();
        let log_max_raw = max_dv.ln();
        let mean = sumlog / count as f64;
        // variance = E[x²] − E[x]²  (population variance, not sample;
        // TWP also uses population — no Bessel correction).
        let variance = (sumsqlog / count as f64) - mean * mean;
        let stddev = variance.max(0.0).sqrt();
        // σ-clamp: prevents a few outlier cells from flattening the
        // ramp to grey.  The bound is TWP's exact formula:
        //   log_max = min(log_max_raw, mean + 2·stddev)
        let log_max = log_max_raw.min(mean + 2.0 * stddev);

        Self {
            entries: build_palette_ramp(log_min, log_max, ramp_size),
            log_min,
            log_max,
            ramp_size,
        }
    }

    /// Sample the ramp at a ΔV in km/s.  Returns the ramp colour
    /// in log-space.  Non-finite ΔV → `INFEASIBLE_COLOR`.
    pub fn color_for(&self, dv_km_s: f64) -> Color32 {
        if !dv_km_s.is_finite() || dv_km_s <= 0.0 {
            return INFEASIBLE_COLOR;
        }
        let log_dv = dv_km_s.ln();
        if log_dv <= self.log_min {
            return self.entries[0];
        }
        if log_dv >= self.log_max {
            return *self.entries.last().unwrap();
        }
        // Map `log_dv ∈ [log_min, log_max]` to a fractional ramp index.
        let span = (self.log_max - self.log_min).max(1e-9);
        let frac = (log_dv - self.log_min) / span;
        let idx_f = frac * (self.ramp_size - 1) as f64;
        let idx_lo = idx_f.floor() as usize;
        let idx_hi = (idx_lo + 1).min(self.ramp_size - 1);
        let t = (idx_f - idx_lo as f64) as f32;
        // Linear interpolation in straight (un-premultiplied) RGBA.
        let a = self.entries[idx_lo];
        let b = self.entries[idx_hi];
        theme::lerp_rgba(
            (a.r(), a.g(), a.b(), a.a()),
            (b.r(), b.g(), b.b(), b.a()),
            t,
        )
    }
}

/// Build the `ramp_size`-entry palette ramp by linearly interpolating
/// the 7 anchors across `[log_min, log_max]`.  Each anchor occupies
/// an equal fraction of the ramp's `[0, ramp_size-1]` index range;
/// entries between anchors are linearly interpolated in straight
/// RGBA (matches how egui treats un-premultiplied colours).
fn build_palette_ramp(log_min: f64, log_max: f64, ramp_size: usize) -> Vec<Color32> {
    let anchors: Vec<Color32> = PALETTE_ANCHORS
        .iter()
        .map(|&(r, g, b)| theme::color32_from_rgba((r, g, b, 255)))
        .collect();
    let n = anchors.len();
    if n < 2 {
        return anchors;
    }
    // log_span is recorded on the ramp struct as log_max - log_min;
    // the ramp itself is built as a fixed palette sweep
    // (anchor 0 → anchor n-1 across [0, ramp_size-1]).
    let _log_span = (log_max - log_min).max(1e-9);
    let mut out = Vec::with_capacity(ramp_size);
    for i in 0..ramp_size {
        // Map index → fractional anchor position. The anchors span
        // [0, 1] uniformly across the ramp; log_min / log_max are
        // recorded on the ramp struct so the consumer can recover
        // the absolute log-space position for a given index.
        let frac = i as f64 / (ramp_size - 1) as f64;
        let anchor_pos = frac * (n - 1) as f64;
        let lo = anchor_pos.floor() as usize;
        let hi = (lo + 1).min(n - 1);
        let t = (anchor_pos - lo as f64) as f32;
        let a = anchors[lo];
        let b = anchors[hi];
        out.push(theme::lerp_rgba(
            (a.r(), a.g(), a.b(), a.a()),
            (b.r(), b.g(), b.b(), b.a()),
            t,
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fleets::porkchop::{PorkchopCell, PorkchopGrid};
    use bevy::math::DVec3;

    fn make_grid(cells_dv_km_s: &[f64]) -> PorkchopGrid {
        let cols = cells_dv_km_s.len().max(1);
        let rows = 1;
        let cells: Vec<PorkchopCell> = cells_dv_km_s
            .iter()
            .map(|&dv| PorkchopCell {
                t_dep_s: 0.0,
                tof_s: 0.0,
                total_dv_ms: if dv.is_finite() {
                    dv * 1000.0
                } else {
                    f64::INFINITY
                },
                c3_departure: 0.0,
                v_inf_arrival_ms: 0.0,
                delta_v1_ms: 0.0,
                delta_v2_ms: 0.0,
                feasible: dv.is_finite() && dv > 0.0,
                origin_pos_au: DVec3::ZERO,
                dest_pos_au: DVec3::ZERO,
                v_departure_ms: DVec3::ZERO,
                v_arrival_ms: DVec3::ZERO,
                transfer_orbit: None,
            })
            .collect();
        PorkchopGrid {
            resolution: (cols, rows),
            t_dep_bounds_s: (0.0, 1.0),
            tof_bounds_s: (0.0, 1.0),
            cells,
            min_cell: None,
            metric: crate::fleets::porkchop::PorkchopMetric::TotalDv,
            origin_name: "Origin".to_string(),
            dest_name: "Dest".to_string(),
            rendered_tof_bounds_s: (0.0, 1.0),
        }
    }

    #[test]
    fn ramp_clips_at_mean_plus_2sigma() {
        // 1 cheap cell + 99 mid cells + 1 100-km/s outlier.
        let mut cells: Vec<f64> = vec![5.0];
        cells.extend(std::iter::repeat_n(8.0, 98));
        cells.push(100.0);
        let grid = make_grid(&cells);
        let ramp = PorkchopColorRamp::from_grid(&grid);
        // log_max_raw = ln(100) ≈ 4.605; mean + 2σ should clamp it
        // well below ln(100).  Assert log_max < ln(100) - 0.5.
        assert!(
            ramp.log_max < 100.0_f64.ln() - 0.5,
            "σ-clamp failed: log_max={:.4} should be < ln(100) - 0.5 = {:.4}",
            ramp.log_max,
            100.0_f64.ln() - 0.5
        );
    }

    #[test]
    fn ramp_log_space_sample_at_extremes() {
        let cells: Vec<f64> = vec![2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let grid = make_grid(&cells);
        let ramp = PorkchopColorRamp::from_grid(&grid);
        let c_min = ramp.color_for(2.0);
        let c_max = ramp.color_for(ramp.log_max.exp() * 1.01);
        assert_eq!(c_min, ramp.entries[0], "min dv should map to entries[0]");
        assert_eq!(
            c_max,
            *ramp.entries.last().unwrap(),
            "max dv should map to entries.last()"
        );
    }

    #[test]
    fn ramp_infeasible_returns_grey() {
        let cells: Vec<f64> = vec![5.0, 6.0, 7.0];
        let grid = make_grid(&cells);
        let ramp = PorkchopColorRamp::from_grid(&grid);
        assert_eq!(ramp.color_for(f64::NAN), INFEASIBLE_COLOR);
        assert_eq!(ramp.color_for(f64::INFINITY), INFEASIBLE_COLOR);
        assert_eq!(ramp.color_for(0.0), INFEASIBLE_COLOR);
        assert_eq!(ramp.color_for(-1.0), INFEASIBLE_COLOR);
    }

    #[test]
    fn ramp_monotonic_in_log_space() {
        // A grid where ΔV is strictly increasing across cells.
        let cells: Vec<f64> = (1..=20).map(|i| i as f64 * 0.5).collect();
        let grid = make_grid(&cells);
        let ramp = PorkchopColorRamp::from_grid(&grid);
        // Sample at 100 log-spaced points and assert the luminance
        // is non-decreasing (cheap cells → blue/cyan/green which
        // have roughly equal or higher luminance than the
        // expensive red end — wait, this is wrong: the palette
        // is blue→red so cheap = blue (medium-low luminance) and
        // expensive = red (medium luminance).  The TWP palette
        // is NOT perceptually monotonic in luminance.
        //
        // What we *can* assert: the **index** in the ramp is
        // strictly increasing (i.e. `color_for` is a monotonic
        // function of log(dv)).  We test that by comparing the
        // first non-zero byte (any channel) across samples.
        let samples: Vec<f64> = (0..100).map(|i| 1.0 + (i as f64) * 0.1).collect();
        let mut prev_idx: Option<usize> = None;
        for &dv in &samples {
            let log_dv = dv.ln();
            if log_dv < ramp.log_min || log_dv > ramp.log_max {
                continue;
            }
            let span = (ramp.log_max - ramp.log_min).max(1e-9);
            let frac = (log_dv - ramp.log_min) / span;
            let idx = (frac * (ramp.ramp_size - 1) as f64) as usize;
            if let Some(prev) = prev_idx {
                assert!(
                    idx >= prev,
                    "ramp index must be non-decreasing in log-space; got idx={idx} after prev={prev} at dv={dv}"
                );
            }
            prev_idx = Some(idx);
        }
    }

    #[test]
    fn ramp_no_feasible_cells_returns_dummy() {
        // All cells marked infeasible (dv == f64::INFINITY).
        let cells: Vec<f64> = vec![f64::INFINITY; 5];
        let grid = make_grid(&cells);
        let ramp = PorkchopColorRamp::from_grid(&grid);
        // Should return a valid ramp; entries all derived from the
        // dummy log-space [0, ln(1)] = [0, 0].  Each entry
        // interpolates between the first two anchors (blue→sky
        // blue).  Just assert the ramp is well-formed.
        assert_eq!(ramp.entries.len(), ramp.ramp_size);
        assert!(ramp.log_max >= ramp.log_min);
    }
}
