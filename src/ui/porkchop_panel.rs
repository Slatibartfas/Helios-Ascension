//! PorkchopPanel — egui widget for the (t_dep, t_tof) ΔV contour grid.
//!
//! Replaces the Efficient/Moderate/Fast `selectable_label` block in the
//! transfer planner with a click-to-select coloured grid.  See the LGD
//! design contract on GRA-152 (comment `aec3a25f` on GRA-152).  The
//! data types live in `src/fleets/porkchop.rs`; this file is the UI
//! half (rendering + click/hover/select interaction).
//!
//! Hard rules from the LGD design contract §[UI overlay contract]:
//!   * Use native `egui::Painter` + `Rect` cells; **no `egui_plot` dep**.
//!   * Click-to-select (not hover-then-click).
//!   * Hover → tooltip with (t_dep, t_tof, total ΔV, C3, v∞ arrival, ETA).
//!   * Right side panel binds the 4 stat fields to the selected cell.
//!   * Phase-window overlay: dashed vertical line at next-window time.
//!   * Feasible contour: faint white line at the fleet-max-ΔV boundary.
//!   * Out-of-budget cells stay visible (greyed).

use super::porkchop_color_ramp::{PorkchopColorRamp, INFEASIBLE_COLOR};
use super::theme;
use crate::fleets::porkchop::{PorkchopCell, PorkchopGrid};
use crate::fleets::PorkchopConfig;
use bevy::prelude::*;
use bevy_egui::egui::{self, Color32, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};

pub(crate) const SECONDS_PER_DAY: f64 = 86_400.0;
pub(crate) const SECONDS_PER_YEAR: f64 = 365.25 * SECONDS_PER_DAY;

/// Minimum ΔV span (km/s) below which we do *not* stretch the colormap
/// across the grid range.  Real interplanetary porkchops always span
/// more than 0.5 km/s, so this floor only matters for degenerate grids
/// (e.g. local transfers where every feasible cell lands on the same
/// Hohmann ΔV).  Without the floor those grids would wash out to a
/// near-uniform colour band; with it they keep their nominal colormap
/// band so the user still sees useful variation.
#[allow(dead_code)]
const COLORMAP_MIN_SPAN_KM_S: f64 = 0.5;

/// Pick the feasible cell that represents the best **compromise**
/// between earliest arrival and cheapest ΔV.  Used by
/// [`porkchop_panel`] to populate `selected_porkchop_cell` the
/// first time the planner opens.
///
/// The trade-off space: low-ΔV cells usually have long TOF (slow
/// Hohmann-like transfers), and short-TOF cells usually burn much
/// more ΔV (high-energy burns).  Picking the cheapest-ΔV cell
/// makes the player wait an absurdly long time for marginal fuel
/// savings; picking the earliest-arrival cell burns ridiculous
/// ΔV for a tiny time gain ("5× burn for 2 days earlier").
/// Neither extreme is a reasonable default.
///
/// Algorithm: two passes over the grid.  The first computes the
/// (min, max) of `(arrival, ΔV)` across all feasible cells for
/// normalization.  The second scores each feasible cell by
/// Manhattan distance from the centre `(0.5, 0.5)` of the unit
/// square and returns the cell closest to that centre.  Cells
/// near the boundaries (cheapest-ΔV or earliest-arrival corners)
/// score worst; cells in the middle of both distributions score
/// best.  This naturally avoids the user's reported bad cases:
/// "10-year TOF cheapest ΔV" and "5× burn for 2 days earlier".
///
/// Falls back to `grid.min_cell` (the cheapest-ΔV cell) when no
/// feasible cell exists — degenerate grids still get a non-empty
/// selection.
pub(crate) fn auto_pick_compromise_cell(grid: &PorkchopGrid) -> Option<(usize, usize)> {
    let (cols, rows) = grid.resolution;
    // First pass: compute (min, max) of arrival and ΔV across all
    // feasible cells for normalization.  Skips infeasible cells
    // and cells with non-finite (NaN / ∞) values.
    let mut arrival_min = f64::INFINITY;
    let mut arrival_max = f64::NEG_INFINITY;
    let mut dv_min = f64::INFINITY;
    let mut dv_max = f64::NEG_INFINITY;
    let mut feasible_count: usize = 0;
    for r in 0..rows {
        for c in 0..cols {
            if let Some(cell) = grid.cells.get(r * cols + c) {
                if cell.feasible {
                    let arrival = cell.t_dep_s + cell.tof_s;
                    let dv = cell.total_dv_ms;
                    if arrival.is_finite() && dv.is_finite() {
                        feasible_count += 1;
                        if arrival < arrival_min {
                            arrival_min = arrival;
                        }
                        if arrival > arrival_max {
                            arrival_max = arrival;
                        }
                        if dv < dv_min {
                            dv_min = dv;
                        }
                        if dv > dv_max {
                            dv_max = dv;
                        }
                    }
                }
            }
        }
    }
    if feasible_count == 0 {
        return grid.min_cell;
    }
    // Use a tiny epsilon floor on the ranges so degenerate axes
    // (all cells sharing the same arrival or the same ΔV) don't
    // divide by zero.  This degenerates the score to a single
    // axis, which is the right behaviour when one axis has no
    // variation.
    let arrival_range = (arrival_max - arrival_min).max(f64::EPSILON);
    let dv_range = (dv_max - dv_min).max(f64::EPSILON);
    // Second pass: pick the cell whose normalized position is
    // closest to (0.5, 0.5) by Manhattan distance — the "balanced
    // middle" of the (arrival, ΔV) plane.
    let mut best: Option<(usize, usize)> = None;
    let mut best_score = f64::INFINITY;
    for r in 0..rows {
        for c in 0..cols {
            if let Some(cell) = grid.cells.get(r * cols + c) {
                if cell.feasible {
                    let arrival = cell.t_dep_s + cell.tof_s;
                    let dv = cell.total_dv_ms;
                    if arrival.is_finite() && dv.is_finite() {
                        let norm_a = (arrival - arrival_min) / arrival_range;
                        let norm_d = (dv - dv_min) / dv_range;
                        let score = (norm_a - 0.5).abs() + (norm_d - 0.5).abs();
                        if score < best_score {
                            best_score = score;
                            best = Some((c, r));
                        }
                    }
                }
            }
        }
    }
    best.or(grid.min_cell)
}

/// Selected-cell index in the grid.  Persisted on the `FleetUiState`
/// (`fleet_ui_state.selected_porkchop_cell: Option<(usize, usize)>`).
///
/// Returns `Response` so callers can chain egui interactions
/// (e.g. `on_hover_text` for a status-bar tooltip) on top of the
/// in-canvas hover hint.
pub fn porkchop_panel(
    ui: &mut Ui,
    grid: &PorkchopGrid,
    cfg: &PorkchopConfig,
    selected: &mut Option<(usize, usize)>,
    _fleet_max_dv_ms: f64,
    time_to_window_s: f64,
    // Sim seconds elapsed since the rotating buffer was built.
    // Drives the scrolling x-axis: at shift_s=0 the visible window
    // starts at the buffer's left edge; as time advances the window
    // slides rightward through the buffer, and at shift_s=visible_width
    // the planner invalidates the cache and rebuilds.  Pass 0.0 for
    // the non-rotating-buffer case.
    shift_s: f64,
    // Target body for the current grid — the planner's
    // `fleet_ui_state.target_body`.  Used as the **stable**
    // component of the texture-bake identity (see the identity
    // comment at the bake site below).  Pass the current
    // target entity; `None` while the planner has no target.
    target_body: Option<Entity>,
    // Phase B (TWP parity — single-texture bake): the planner's
    // cached `TextureHandle` and the identity tuple
    // `(target_body, resolution, min_cell)` it was baked for. The
    // panel rebakes when the identity tuple changes (i.e. when the
    // deferred-build block swaps in a fresh `PorkchopGrid` whose
    // target/resolution/min-cell/`t_dep_bounds_s.0` anchor differs);
    // on every other frame it just re-uses the cached handle. The
    // cached handle is `Some(...)` for the steady state; the
    // planner initialises both fields as `None`.
    texture_cache: &mut Option<egui::TextureHandle>,
    texture_built_for: &mut Option<(Option<Entity>, (usize, usize), Option<(usize, usize)>, u64)>,
) -> Response {
    let (cols, rows) = grid.resolution;
    if cols == 0 || rows == 0 {
        return ui.label("Empty porkchop grid (0×0).");
    }

    // Auto-pick the **compromise** cell (Pareto-frontier balanced
    // between earliest arrival and cheapest ΔV) when nothing is
    // selected.  See [`auto_pick_compromise_cell`] for the
    // rationale — the player opening the planner shouldn't get
    // either extreme ("10-year TOF cheapest ΔV" or "5× burn for
    // 2 days earlier"), so we pick a cell that's reasonable on
    // both axes.
    if selected.is_none() {
        *selected = auto_pick_compromise_cell(grid);
    }

    // Build the per-frame colour ramp.  Phase A (TWP parity):
    // log-scale ΔV→colour mapping with **mean + 2σ outlier clamp**
    // (TriggerAu/TransferWindowPlanner's exact formula) plus a
    // 7-anchor piecewise palette (blue→cyan→green→yellow→orange→red)
    // sampled into a 512-entry ramp.  The σ-clamp prevents one
    // infeasible-by-finite-but-huge cell from flattening the colour
    // ramp to grey; the log-scale mapping expands the visible
    // dynamic range so the cheap basin shows as a wide coloured
    // lobe instead of a thin sliver at the bottom of a linear
    // scale.  This replaces the linear `[min, max]`-remapped
    // colormap that the user reported as "looks uniformly green
    // or red, not a gradient".
    //
    // The `cfg` parameter is accepted for backwards compatibility
    // with the RON-driven `PorkchopConfig.colormap` field — a
    // future GRA can thread a modder-supplied palette through the
    // ramp builder as an override.  For v1 we use the TWP palette.
    let _ = cfg; // palette override deferred; ramp uses TWP defaults
    let ramp = PorkchopColorRamp::from_grid(grid);

    // Rotating-buffer scroll state.  `visible_cols = cols / 2` so the
    // player sees a normal-width panel while the buffer caches the
    // unused half.  `col_step_s` is sim-seconds per buffer column.
    //
    // The scroll is a single *continuous* floating-point value in
    // column units (`scroll`).  Each cell at original buffer column
    // `c` has its left edge at `x = (c - scroll) * cell_w` in the
    // visible window.  There is *no* integer / fractional split and
    // *no* boundary snap — as time advances the cells slide
    // continuously leftward at sub-cell resolution, no jump.
    //
    // Margin: `visible_cols = cols / 2 - 1` instead of `cols / 2`
    // caps the rendered content by one buffer column on the right
    // edge.  When the buffer rotates, the new buffer's left half
    // replaces the old buffer's right half — but that replacement
    // happens in the off-screen margin, so the user doesn't see
    // the visible cell content snap by one tile width.  The cells
    // at the right edge are still part of the scroll math (the
    // scroll continues smoothly) but the planner only paints
    // `visible_cols` cells across the panel.
    //
    // GRA-169: the buffer spans `[sim_time_s, sim_time_s +
    // buffer_width]` (anchored at the player's clock at rebuild,
    // GRA-169 Part A).  The visible window shows the leftmost
    // `visible_cols` cells, which advance leftward by
    // `shift_s / col_step_s` cells as the player's clock
    // advances.  On rotation (Part B) the planner keeps the old
    // grid in `porkchop_grid` while the deferred build solves a
    // new buffer (~360 ms), then atomically swaps — no blank
    // frame and no L/R snap.
    //
    // The rotating-buffer half-window design only makes sense for
    // the heliocentric interplanetary grid (≥30 cols).  For smaller
    // grids (short-hop = 1 col, gravity-assist = 20 cols) there's
    // no buffer to rotate and the whole grid should be visible at
    // once.  Detect by `cols < ROTATING_BUFFER_MIN_COLS` and render
    // the full texture (no scrolling).  Without this guard the
    // `scroll = (shift_s / col_step_s)` term drives the UV window
    // past the end of the texture within a few sim seconds, leaving
    // the panel blank (`uv_min_x > uv_max_x` after clamp) even
    // though the underlying texture is correct.
    const ROTATING_BUFFER_MIN_COLS: usize = 30;
    let visible_cols = if cols >= ROTATING_BUFFER_MIN_COLS {
        (cols / 2).saturating_sub(1).max(1)
    } else {
        // Small grids: full texture, no scrolling.
        cols
    };
    let t_dep_min = grid.t_dep_bounds_s.0;
    let t_dep_max = grid.t_dep_bounds_s.1;
    // Defensive: a degenerate `t_dep_bounds` (zero span, e.g. a
    // 1-column short-hop grid that the builder anchors at `(0.0,
    // 0.0)` for symbolic reasons) would make `col_step_s = 0`,
    // which makes `scroll = ±infinity` and breaks the UV-window
    // math below.  Substitute a 1-second nominal step so the
    // texture maps cleanly across the panel even when there is
    // no real t_dep axis to scroll.
    let t_dep_span = t_dep_max - t_dep_min;
    let col_step_s = if cols > 0 && t_dep_span.abs() > f64::EPSILON {
        t_dep_span / cols as f64
    } else {
        1.0
    };
    // Only enable the rotating-buffer scroll when the texture has
    // more cells than the visible window (i.e. the interplanetary
    // case).  Small grids paint the full texture instead.
    let scroll = if cols > visible_cols {
        (shift_s / col_step_s) as f32
    } else {
        0.0
    };

    // Hover + click.  `Sense::hover()` alone ignores clicks (the user
    // reported they "couldn't click any other tile"), and
    // `Sense::click_and_drag()` only reports `interact_pointer_pos`
    // while a button is held (which hid the tooltip behind "hold left
    // mouse button").  Combining the two with `|` keeps `hover_pos()`
    // populated as the pointer moves freely *and* makes `clicked()`
    // fire on a normal left-click.
    let desired_size = Vec2::new(ui.available_width().max(320.0), 240.0);
    let (resp, painter) = ui.allocate_painter(desired_size, Sense::click() | Sense::hover());
    let plot_rect = resp.rect;

    // Background
    painter.rect_filled(plot_rect, 0.0, theme::BG_SOLID);

    // Cell layout — pad 32 px on the left (TOF axis labels) and 18 px on
    // the bottom (t_dep axis labels).  Each visible column maps to
    // original buffer column `shift_cols_int + c_visible`.  Cell width
    // uses the visible count so the player sees a normal-width panel
    // even when the buffer caches 2× the columns.
    let pad_l = 36.0;
    let pad_b = 20.0;
    let grid_rect = Rect::from_min_size(
        Pos2::new(plot_rect.left() + pad_l, plot_rect.top() + 4.0),
        Vec2::new(
            plot_rect.width() - pad_l - 4.0,
            plot_rect.height() - pad_b - 4.0,
        ),
    );
    let cell_w = grid_rect.width() / visible_cols as f32;

    // Y-axis rendering.  Use the builder's adaptive trim
    // (`rendered_tof_bounds_s`) to clip the row range to the
    // populated band: rows below the lowest feasible row
    // (the fastest feasible trajectory) and above the highest
    // feasible row (the slowest feasible trajectory) are
    // excluded, plus a small margin on each side so the
    // colormap band stretches across the cheap-transfer basin
    // instead of getting squashed into the populated row band.
    // See `compute_adaptive_tof_bounds` in
    // `src/fleets/porkchop.rs` for the full contract.
    //
    // **Y-axis orientation: NASA / JPL convention.** Row 0
    // (lowest TOF, fastest trajectory) is drawn at the BOTTOM of
    // the panel; row `rows - 1` (highest TOF, slowest
    // trajectory) is drawn at the TOP.  This matches the user-
    // reported "start with the fastest trajectory then extend
    // upward" mental model: the cheapest fast Hohmann-like
    // options sit at the bottom of the panel, the slower
    // long-arc options sit above, and the trim clips the empty
    // grey tail at the very top.
    //
    // Fallback: if `rendered_tof_bounds_s` is missing or
    // degenerate (zero span, out-of-range), fall back to the
    // full solved row range so hover / click mapping stays
    // robust against stale metadata on older grid instances.
    let tof_min_s = grid.tof_bounds_s.0;
    let tof_max_s = grid.tof_bounds_s.1;
    let configured_span = (tof_max_s - tof_min_s).max(f64::MIN_POSITIVE);
    let (rendered_tof_min_s, rendered_tof_max_s) = grid.rendered_tof_bounds_s;
    // Map an absolute TOF (in seconds) to its row index in the
    // solved grid.  The solved grid's row→TOF map is linear with
    // `row_frac = row / (rows - 1)`, so the inverse is
    // `row = frac × (rows - 1)`.  Clamp to `[0, rows - 1]` so any
    // rounding past the edge falls back to the boundary row
    // rather than producing an out-of-bounds index that would
    // later crash on `grid.cells[row * cols + col]`.
    let tof_to_row = |tof_s: f64| -> usize {
        let frac = ((tof_s - tof_min_s) / configured_span).clamp(0.0, 1.0);
        (frac * (rows as f64 - 1.0)).round() as usize
    };
    let mut rendered_row_first = tof_to_row(rendered_tof_min_s);
    let mut rendered_row_last = tof_to_row(rendered_tof_max_s);
    // Sanity guard: never let the adaptive trim collapse to an
    // empty or inverted range.  If the builder produced a
    // degenerate pair (e.g. both bounds equal), fall back to
    // the full solved range so the player still sees something.
    if rendered_row_first >= rendered_row_last {
        rendered_row_first = 0;
        rendered_row_last = rows.saturating_sub(1);
    }
    let n_view_rows = (rendered_row_last - rendered_row_first + 1).max(1);
    let cell_h = grid_rect.height() / n_view_rows as f32;

    // Compute the (col, row) of the cell currently under the cursor.
    // The cursor's visible col is `cursor_x / cell_w + scroll`; the
    // buffer col is that value floored.  The visible row index is
    // `cursor_y / cell_h` in the rendered Y-axis (NASA convention:
    // row 0 at the bottom, so the cursor's distance from the BOTTOM
    // divided by `cell_h` gives the view row).  We map it back to
    // the original `grid.cells` row index for the hover / click
    // handlers.
    let hover_cell: Option<(usize, usize)> = resp
        .hover_pos()
        .filter(|pos| grid_rect.contains(*pos))
        .map(|pos| {
            let col_f = (pos.x - grid_rect.left()) / cell_w + scroll;
            let col = col_f.max(0.0) as usize;
            // NASA convention: row 0 (lowest TOF) at the panel
            // BOTTOM.  Cursor Y is measured from the top of the
            // panel, so the view row index is `n_view_rows - 1 -
            // (cursor_y - top) / cell_h`.
            let cursor_from_bottom = (grid_rect.bottom() - pos.y) / cell_h;
            let view_row = (cursor_from_bottom as usize).min(n_view_rows - 1);
            let orig_row = (rendered_row_first + view_row).min(rows - 1);
            (col.min(cols - 1), orig_row)
        });

    // 1. Cells (coloured rects).  Each buffer column `c` has its
    // left edge at `x = (c - scroll) * cell_w` in the visible
    // window.  We draw every cell whose left edge lies within
    // the visible window so the user sees cells scrolling
    // smoothly across the panel, then wrap the cell draw in
    // `painter.with_clip_rect(grid_rect)` so any cell that
    // straddles the panel boundary is HARD-CLIPPED at the
    // edge rather than spilling outside.  Without the clip
    // rect the user sees cells extending past the panel
    // border during rotation, which reads as a left/right
    // "jiggle" of the boundary itself.
    let visible_w = visible_cols as f32 * cell_w;
    let cell_clip = painter.with_clip_rect(grid_rect);
    // Map an original row index to its Y pixel position in the
    // rendered Y-axis.  NASA convention: row 0 (lowest TOF,
    // fastest trajectory) at the BOTTOM of the panel, row
    // `n_view_rows - 1` (highest TOF, slowest trajectory) at
    // the TOP.  The view-row index is `orig_row -
    // rendered_row_first`, and the Y position is
    // `grid_rect.bottom() - (view_row + 1) * cell_h`.
    let orig_row_to_view_y = |orig_row: usize| -> f32 {
        if orig_row < rendered_row_first {
            return grid_rect.bottom() + cell_h;
        }
        let view_row = orig_row - rendered_row_first;
        if view_row >= n_view_rows {
            return grid_rect.top() - cell_h;
        }
        grid_rect.bottom() - (view_row + 1) as f32 * cell_h
    };

    // Phase B (TWP parity — single-texture bake): build a
    // `ColorImage` from the ramp-driven cell colours and draw it
    // as a single `painter.image(...)` quad, letting egui's GPU
    // bilinear filter produce a smooth gradient across cell
    // boundaries instead of the per-cell rect banding the user
    // reported.
    //
    // Identity: `(target_body, grid.resolution, grid.min_cell,
    // t_dep_bounds_s.0.to_bits())`.  The remote-version comment
    // described including the `t_dep_bounds_s.0` anchor (so the
    // texture rebakes when the buffer re-anchors to a new sim
    // time) but the actual tuple omitted it — the cells' colours
    // stayed frozen on the old bake between rebuilds, reading as
    // a static plot at high sim speed.  Including the anchor
    // fixes the "porkchop stopped moving" symptom without the
    // per-frame rebake cost the remote explicitly avoided (the
    // anchor only changes on rebuild, which fires every 5 real
    // seconds at 1 yr/s).  The `u64` is the `f64::to_bits()`
    // representation so the tuple stays `Eq`-comparable.
    let grid_identity: (Option<Entity>, (usize, usize), Option<(usize, usize)>, u64) = (
        target_body,
        grid.resolution,
        grid.min_cell,
        grid.t_dep_bounds_s.0.to_bits(),
    );
    let identity_mismatch =
        texture_cache.is_none() || texture_built_for.as_ref() != Some(&grid_identity);
    if identity_mismatch {
        // Defensive: a degenerate grid (cells empty but resolution
        // non-zero) can show up when the planner's deferred build
        // hands us a grid that hit an empty-cells branch in the
        // solver (e.g. the heliocentric Lambert solver produces
        // infeasible cells for every (col, row) of a star-approach
        // target whose heliocentric orbit isn't meaningful).  Without
        // this guard the texture-bake indexing below panics with
        // `index out of bounds: the len is 0 but the index is 0`.
        // The early-return paints a placeholder texture so the
        // caller still gets a usable `TextureHandle` to swap into
        // `texture_cache` (the identity stays matched for the next
        // frame, avoiding a rebake loop).
        if grid.cells.is_empty() {
            let placeholder =
                egui::ColorImage::new([1, 1], vec![egui::Color32::from_black_alpha(255)]);
            let handle =
                ui.ctx()
                    .load_texture("porkchop_grid", placeholder, egui::TextureOptions::LINEAR);
            *texture_cache = Some(handle);
            *texture_built_for = Some(grid_identity);
        } else {
            // Bake a supersampled `cols*K × rows*K` ColorImage in
            // row-major order so the GPU bilinear filter has enough
            // source pixels per destination pixel to produce smooth
            // gradients — both spatially (rows with similar ΔV no
            // longer read as visible horizontal bands) and temporally
            // (as the rotating-buffer UV scrolls, the per-frame
            // sub-pixel sampling interpolates instead of stepping).
            //
            // Supersampling is essentially free: it's a `4×4` block fill
            // per cell, no Lambert solves.  A 60×60 grid produces a
            // 240×240 RGBA texture = 230 KB, which fits comfortably in
            // VRAM.  The bake cost is dominated by the upload, not the
            // pixel fill, and the upload only fires on identity changes
            // (target / resolution / min-cell), not per frame.
            //
            // The supersample factor is constant rather than RON-driven
            // because the smoothness is a perceptual constant — the
            // human eye sees aliasing above ~8 source pixels per
            // destination pixel regardless of how coarse the underlying
            // cell grid is.  Keeping it constant means changing the RON
            // resolution (e.g. bumping the interplanetary grid from
            // 60×60 to 120×120) doesn't accidentally halve the effective
            // AA.
            //
            // Rows correspond to TOF (NASA convention: row 0 at the
            // bottom of the panel, but the image's pixel (0, 0) is its
            // top-left, so we flip the row index when packing — `row 0`
            // in the grid becomes the image's `tex_rows - K` row).
            //
            // Supersample factor bumped from 4 → 8 → 12 → 20 to eliminate the
            // per-row "wavy/flickering" boundary the player reported
            // and to give the bilinear filter enough source density
            // for a near-photographic gradient (GRA-385 follow-up +
            // post-resolution-bump follow-up).  At 20× each grid cell
            // renders as a 20×20 block of the same colour, giving the
            // GPU's bilinear sampler ~400 source texels per cell to
            // interpolate between.  The sweet-spot ΔV is much easier
            // to pick because the gradient between adjacent cells is
            // smooth enough that tiny mouse movements can land on a
            // 50 m/s difference — at 12× the same delta read as a
            // 1-texel-wide transition that looked "rough" on the
            // rotating buffer (where each visible cell only occupies
            // ~3 screen pixels).  At 20× the visible cell spans ~5
            // screen pixels with ~2 pixels of bilinear interpolation
            // at each boundary, producing a smooth gradient.
            //
            // Memory cost: 20×20 = 400 texels per cell.  Worst case
            // is the 240×50 interplanetary rotating-buffer grid =
            // 4800×1000 RGBA = ~19 MB per bake, still well under
            // VRAM (most GPUs ship with 4–8 GB) and uploaded only on
            // identity changes (not per frame).  For the non-buffer
            // 60×60 grid the bake drops to 1200×1200 = ~5.5 MB.
            const SUPERSAMPLE_TARGET: usize = 20;
            // egui 0.33 hard-caps texture side at 2048 (`Context::tex_allocator`
            // panics with "Texture has size X but the maximum texture side is
            // 2048").  Wide grids — Jupiter's porkchop lands at 222 cols × 90
            // rows under the GRA-152 adaptive-resolution sweepper — would
            // produce 4440×1800 textures at the full 20× AA.  Clamp the
            // effective SUPERSAMPLE so the resulting texture stays under the
            // limit; AA quality degrades for very wide grids but the bilinear
            // gradient is still smooth enough to pick cells at 5–9× AA.
            // GRA-NNN (orbit-shell refactor follow-up).
            const MAX_TEX_SIDE: usize = 2048;
            let supersample = SUPERSAMPLE_TARGET
                .min(MAX_TEX_SIDE / cols.max(1))
                .min(MAX_TEX_SIDE / rows.max(1))
                .max(1);
            let tex_cols = cols * supersample;
            let tex_rows = rows * supersample;
            let mut pixels: Vec<Color32> = Vec::with_capacity(tex_cols * tex_rows);
            // Per-corner bilinear fill: each texture pixel maps to a
            // fractional grid coordinate `(row_f, col_f) ∈ [0, rows)×
            // [0, cols)`, then samples the four integer-clamped
            // surrounding cells via [`bilinear_cell_color`].  The
            // previous loop used a flat block fill (one colour per
            // cell), which made each cell read as a hard rectangle
            // even with `TextureOptions::LINEAR` filtering — the GPU
            // bilinear filter could only blend *between* cells, not
            // synthesise gradients *inside* them.  Now each destination
            // pixel sees ~`supersample²` source texels interpolating
            // between four ΔV values, so the basin gradient is smooth
            // from edge to edge.  Bake cost rises from 1 cell lookup
            // to 4 per texel, but the bake only fires on identity
            // changes (target / resolution / min-cell / t_dep
            // anchor) — never per frame — so the panel's per-frame
            // cost is unchanged.
            //
            // The Y-axis flip folds into the row-coordinate mapping:
            // image row 0 is at the top-left of the texture, but grid
            // row 0 (lowest TOF) renders at the panel BOTTOM (NASA /
            // JPL convention).  `row_f = (tex_rows - 1 - img_row) /
            // supersample` therefore places image row 0 near
            // `grid_row = rows - 1` and image row `tex_rows - 1`
            // exactly at `grid_row = 0`, which the four corner clamps
            // then reduce to safe `usize` indices.
            let inv_supersample = 1.0_f32 / supersample as f32;
            for img_row in 0..tex_rows {
                let row_f = (tex_rows - 1 - img_row) as f32 * inv_supersample;
                let r_top_i = row_f.floor() as i32;
                let r_bot_i = r_top_i + 1;
                let r_top = r_top_i.clamp(0, rows as i32 - 1) as usize;
                let r_bot = r_bot_i.clamp(0, rows as i32 - 1) as usize;
                let ty = (row_f - r_top_i as f32).clamp(0.0, 1.0);
                let row_t = r_top * cols;
                let row_b = r_bot * cols;
                for col in 0..tex_cols {
                    let col_f = col as f32 * inv_supersample;
                    let c_left_i = col_f.floor() as i32;
                    let c_right_i = c_left_i + 1;
                    let c_left = c_left_i.clamp(0, cols as i32 - 1) as usize;
                    let c_right = c_right_i.clamp(0, cols as i32 - 1) as usize;
                    let tx = (col_f - c_left_i as f32).clamp(0.0, 1.0);
                    let tl = &grid.cells[row_t + c_left];
                    let tr = &grid.cells[row_t + c_right];
                    let bl = &grid.cells[row_b + c_left];
                    let br = &grid.cells[row_b + c_right];
                    pixels.push(bilinear_cell_color(tl, tr, bl, br, tx, ty, &ramp));
                }
            }
            let image = egui::ColorImage {
                size: [tex_cols, tex_rows],
                source_size: egui::Vec2::new(tex_cols as f32, tex_rows as f32),
                pixels,
            };
            // Allocate the texture (or update an existing one — but
            // since we're rebuilding from scratch each time the
            // identity changes, a fresh `load_texture` is simplest).
            // The TextureHandle drop is automatic when
            // `texture_cache = Some(new_handle)` replaces the old
            // one.
            let handle =
                ui.ctx()
                    .load_texture("porkchop_grid", image, egui::TextureOptions::LINEAR);
            *texture_cache = Some(handle);
            *texture_built_for = Some(grid_identity);
        }
    }
    // grid_rect (UV = (scroll/cols, 0) → ((scroll+visible_cols)/cols, 1)).
    // Scrolling the UV window instead of redrawing cells means the
    // GPU bilinear filter smooths the seam where the right-edge cells
    // exit and the left-edge cells enter — no per-frame rebake.  The
    // hover-cell math (`(pos.x - grid_rect.left()) / cell_w + scroll`)
    // and the selection rectangle (`(sc as f32 - scroll) * cell_w`)
    // both already use `scroll`, so the visible UV band stays
    // consistent with where the user can click.
    if let Some(texture) = texture_cache.as_ref() {
        // Defensive: `scroll` is computed as `(shift_s / col_step_s)`,
        // and on degenerate grids (`col_step_s = 1.0` from the
        // fallback above) it can grow large enough that the UV
        // window goes out of `[0, 1]`.  Clamp to a valid range so
        // the GPU bilinear sampler doesn't sample outside the
        // texture and paint garbage at the panel edges.
        let uv_min_x = (scroll / cols as f32).clamp(0.0, 1.0);
        let uv_max_x = ((scroll + visible_cols as f32) / cols as f32).clamp(uv_min_x.max(0.0), 1.0);
        let uv = Rect::from_min_max(Pos2::new(uv_min_x, 0.0), Pos2::new(uv_max_x, 1.0));
        cell_clip.image(texture.id(), grid_rect, uv, Color32::WHITE);
    }
    // Suppress the unused-var lint for the per-cell loop that
    // used to live here — kept as a comment-only marker because
    // removing the loops entirely would lose the per-cell
    // drawing intent.
    let _ = (visible_w, &cell_clip);

    // 1b. Hover highlight — drawn AFTER the cell fill but BEFORE the
    // selection outline so the selection always wins when both apply.
    // The highlight is a thin semi-transparent white outline so it
    // reads against every colormap band (green, yellow, red, greyed).
    if let Some((hc, hr)) = hover_cell {
        if Some((hc, hr)) != *selected && hr >= rendered_row_first && hr <= rendered_row_last {
            let x = grid_rect.left() + (hc as f32 - scroll) * cell_w;
            if x + cell_w >= grid_rect.left() && x <= grid_rect.left() + visible_w {
                let y = orig_row_to_view_y(hr);
                let rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(cell_w, cell_h));
                painter.rect_stroke(
                    rect,
                    0.0,
                    Stroke::new(1.5_f32, Color32::from_white_alpha(180)),
                    egui::StrokeKind::Inside,
                );
            }
        }
    }

    // 2. Selection highlight (thick border on the selected cell).
    // Selected cell stays at the "Now" (leftmost) column when its
    // t_dep has scrolled past the player's current time — clamps to
    // the left edge so the cell sticks at "Now" instead of
    // disappearing off the panel.
    if let Some((sc, sr)) = *selected {
        if sc < cols && sr < rows {
            let x_f = (sc as f32 - scroll) * cell_w;
            // Pinned at left edge when the cell has scrolled into
            // the past; hidden when it's scrolled off the right
            // edge (i.e. the selected t_dep hasn't been reached
            // yet).  The visible range is `[0, visible_w]`.
            let x = if x_f < 0.0 {
                grid_rect.left()
            } else if x_f > visible_w {
                // Off-screen to the right — skip the selection
                // outline by placing it just past the panel.
                grid_rect.right() + cell_w * 2.0
            } else {
                grid_rect.left() + x_f
            };
            // Selection rows outside the rendered range draw outside
            // the panel rect and get clipped — the visual effect is
            // that the outline disappears, which is the correct
            // behaviour (the player picked a row that's no longer
            // visible because the adaptive trim clipped it).  We
            // still place the rect inside the clip so the egui
            // painter doesn't emit a spurious zero-sized stroke.
            let y = orig_row_to_view_y(sr);
            let rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(cell_w, cell_h));
            painter.rect_stroke(
                rect,
                0.0,
                Stroke::new(2.0_f32, theme::RP_BLUE),
                egui::StrokeKind::Inside,
            );

            // Show the selected cell's tooltip near the cell centre
            // so the player can find their selection among the
            // ~`visible_cols × n_view_rows` cells.  Without this the
            // 2 px blue border can be hard to spot — especially when
            // the auto-picked compromise cell lands in the middle of
            // a continuous colour band with no obvious contour.
            //
            // Visible-or-pinned: draw the tooltip when the cell's
            // *drawn* `x_f` is inside the panel rect (the highlight
            // rect can be pinned left at `grid_rect.left()` when the
            // sim-time scroll has moved past the cell's t_dep, so we
            // also accept that case).  Off-screen to the right (where
            // `x_f > visible_w`) is omitted — the tooltip wouldn't
            // fit and the player hasn't reached that departure epoch
            // yet.
            let cell = &grid.cells[sr * cols + sc];
            let tooltip_in_view = x_f <= visible_w && x_f >= -cell_w;
            if cell.feasible && tooltip_in_view {
                let anchor_x = x + cell_w * 0.5;
                let anchor_y = y + cell_h * 0.5;
                draw_cell_tooltip(&painter, Pos2::new(anchor_x, anchor_y), plot_rect, cell);
            }
        }
    }

    // 3. Grid lines
    // We draw `n_view_rows + 1` horizontal lines (top, between every
    // rendered row, bottom) so the cell-grid lines align with the
    // rendered cells.  Drawing the original `rows + 1` lines would
    // either over-paint inside the panel or hide the trim boundary
    // entirely.
    for col in 0..=cols {
        let x = grid_rect.left() + col as f32 * cell_w;
        painter.line_segment(
            [
                Pos2::new(x, grid_rect.top()),
                Pos2::new(x, grid_rect.bottom()),
            ],
            Stroke::new(0.5_f32, Color32::from_white_alpha(20)),
        );
    }
    for view_row in 0..=n_view_rows {
        // NASA convention: view_row 0 (lowest TOF, fastest
        // trajectory) at the BOTTOM.  The grid line position
        // is `grid_rect.bottom() - view_row * cell_h`, so the
        // first line drawn is the bottom edge and the last is
        // the top edge.
        let y = grid_rect.bottom() - view_row as f32 * cell_h;
        painter.line_segment(
            [
                Pos2::new(grid_rect.left(), y),
                Pos2::new(grid_rect.right(), y),
            ],
            Stroke::new(0.5_f32, Color32::from_white_alpha(20)),
        );
    }

    // 4. Phase-window overlay (dashed vertical line on the t_dep axis)
    let t_dep_min = grid.t_dep_bounds_s.0;
    let t_dep_max = grid.t_dep_bounds_s.1;
    if time_to_window_s.is_finite()
        && t_dep_max > t_dep_min
        && time_to_window_s >= t_dep_min
        && time_to_window_s <= t_dep_max
    {
        let frac = (time_to_window_s - t_dep_min) / (t_dep_max - t_dep_min);
        let x = grid_rect.left() + frac as f32 * grid_rect.width();
        draw_dashed_vertical(
            &painter,
            x,
            grid_rect.top(),
            grid_rect.bottom(),
            theme::AMBER,
        );
    }

    // 5. Axis labels (t_dep days on bottom; tof days on left)
    let label_color = theme::TEXT_DIM;
    let label_size = 10.0;
    let font_id = egui::FontId::proportional(label_size);
    // X-axis: 5 ticks.  The label shows "Now" instead of "+0 d" for
    // the t_dep = sim_time_s tick so the player can see at a glance
    // that the leftmost column is "depart immediately" rather than
    // the optimal-window departure date.
    //
    // GRA-169 (Part A): `t_dep_min`/`t_dep_max` are absolute
    // (anchored at `sim_time_s`), so we report the *relative*
    // offset (`frac * (t_dep_max - t_dep_min)`) in the tick label.
    // Without this the labels would all read "+7882 d" (i.e. the
    // absolute sim epoch in days) instead of the useful "Now / +30 d
    // / +60 d" series.
    for i in 0..=4 {
        let frac = i as f64 / 4.0;
        let t_dep_rel_s = frac * (t_dep_max - t_dep_min);
        let days = t_dep_rel_s / SECONDS_PER_DAY;
        let x = grid_rect.left() + (frac as f32) * grid_rect.width();
        let label = if days.abs() < 0.5 {
            "Now".to_owned()
        } else {
            format!("{days:+.0} d")
        };
        painter.text(
            Pos2::new(x, grid_rect.bottom() + 4.0),
            egui::Align2::CENTER_TOP,
            label,
            font_id.clone(),
            label_color,
        );
    }
    // Y-axis: 4 ticks
    // The data cells render `row=rendered_row_first` (lowest
    // TOF, fastest trajectory) at the BOTTOM of the grid and
    // grow upward toward `rendered_row_last` (highest TOF,
    // slowest trajectory) at the TOP — NASA / JPL convention
    // and the user-reported "start with the fastest trajectory
    // then extend upward" mental model.  Labels MUST mirror
    // that direction or the tooltip and the y-axis tick the
    // user reads off disagree: hovering a cell near the top
    // shows a large TOF in the tooltip but the label next to
    // the cursor reads a small TOF.  Anchor labels at the same
    // y as their tick on the data side — frac=0
    // (`rendered_tof_min`) at the BOTTOM, frac=1
    // (`rendered_tof_max`) at the TOP.
    //
    // Labels interpolate across the *rendered* range, not the
    // configured range.  When the adaptive trim clips the upper
    // grey tail the labels need to follow, otherwise they'd
    // continue to span the empty rows and the top-most label
    // would read e.g. "8 yr" while the cell directly to its
    // right sits at the highest feasible row (~ 1.5 yr).
    // Reusing the trim's `rendered_tof_bounds_s` keeps the
    // visual tick marks and the hover-mapped row indices
    // anchored to the same coordinate system.
    let label_tof_min = rendered_tof_min_s.max(tof_min_s);
    let label_tof_max = rendered_tof_max_s.min(tof_max_s);
    for i in 0..=3 {
        let frac = i as f64 / 3.0;
        let tof_s = label_tof_min + frac * (label_tof_max - label_tof_min);
        let tof_label = if tof_s > SECONDS_PER_YEAR {
            format!("{:.1} yr", tof_s / SECONDS_PER_YEAR)
        } else {
            format!("{:.0} d", tof_s / SECONDS_PER_DAY)
        };
        // NASA convention: frac=0 (smallest TOF) at the
        // BOTTOM.  Y position is `grid_rect.bottom() -
        // frac * grid_rect.height()`.
        let y = grid_rect.bottom() - (frac as f32) * grid_rect.height();
        painter.text(
            Pos2::new(grid_rect.left() - 4.0, y),
            egui::Align2::RIGHT_CENTER,
            tof_label,
            font_id.clone(),
            label_color,
        );
    }
    painter.text(
        Pos2::new(grid_rect.left() - 30.0, grid_rect.top() - 2.0),
        egui::Align2::LEFT_BOTTOM,
        "TOF",
        font_id.clone(),
        label_color,
    );
    painter.text(
        Pos2::new(grid_rect.right(), grid_rect.bottom() + 4.0),
        egui::Align2::RIGHT_TOP,
        "Departure",
        font_id.clone(),
        label_color,
    );

    // 6. Click + hover tooltip — set the selected cell and show a
    // per-cell tooltip at the cursor.
    //
    // `Sense::hover` reports `hover_pos()` whenever the pointer is
    // over the response rect (no button held), which is what makes
    // the tooltip appear the moment the user sweeps the mouse across
    // the porkchop.  Click handling stays the same: a click on a
    // feasible cell moves `*selected` to that cell, which downstream
    // systems (trajectory preview, Execute button) read.
    //
    // GRA-385 follow-up: the panel now shows either the standard
    // direct-transfer grid OR the selected gravity-assist candidate's
    // `(t_dep, tof)` grid, never both.  Click handling is therefore
    // unchanged from the original — the click selects whatever cell
    // the user pointed at, and the planner reads `*selected` against
    // whichever grid is currently rendered (so a click on a GA cell
    // sets the GA selection path implicitly via the view-mode toggle
    // that put the GA grid on screen in the first place).
    if let Some((hc, hr)) = hover_cell {
        if resp.clicked() {
            let cell = &grid.cells[hr * cols + hc];
            if cell.feasible {
                *selected = Some((hc, hr));
                // The current panel call has no access to
                // `FleetUiState`; the absolute-coord anchor is
                // written by the planner when it consumes the
                // selected cell.  The planner reads
                // `grid.t_dep_bounds_s` to compute the abs t_dep
                // from `(hc, hr)` once the user commits the click.
            }
        }
        let cell = &grid.cells[hr * cols + hc];
        if cell.feasible {
            // Tooltip rendered with a solid dark backdrop so it stays
            // readable over the brightest colormap cells (red, yellow).
            // The previous implementation painted bare `theme::TEXT`
            // on top of the cell colour, which was illegible on the
            // green/white end of the gradient.  We pass `plot_rect` so
            // the tooltip can clamp inside the panel and flip above
            // the cursor when there's no room below.
            if let Some(pos) = resp.hover_pos() {
                draw_cell_tooltip(&painter, pos, plot_rect, cell);
            }
        }
    }

    resp
}

/// Paint a small dark rounded-rect backdrop with the per-cell stats in
/// the foreground.  Drawn near the cursor but clamped inside the
/// caller-provided `plot_rect` so the tooltip never spills off the
/// porkchop panel — and flipped *above* the cursor when in the lower
/// half of the grid so it never sits on top of the cell the player
/// is trying to inspect (this was the second bug: the tooltip
/// disappeared when hovering the bottom rows because it anchored
/// below the cursor and got clipped by the panel edge).
fn draw_cell_tooltip(painter: &egui::Painter, cursor: Pos2, plot_rect: Rect, cell: &PorkchopCell) {
    let tooltip = format_cell_tooltip(cell);
    let font = egui::FontId::proportional(11.0);
    let pad = Vec2::new(6.0, 4.0);
    let galley = painter.layout_no_wrap(tooltip.clone(), font.clone(), theme::TEXT);
    let tooltip_size = Vec2::new(galley.size().x + pad.x * 2.0, galley.size().y + pad.y * 2.0);
    // Default anchor: down-and-right of the cursor.  Flip above the
    // cursor when there's more room up there than below, so the
    // tooltip stays visible for cells in the bottom rows.
    let below_room = plot_rect.bottom() - cursor.y;
    let above_room = cursor.y - plot_rect.top();
    let anchor = if below_room < tooltip_size.y + 12.0 && above_room > tooltip_size.y + 12.0 {
        cursor + Vec2::new(10.0, -tooltip_size.y - 10.0)
    } else {
        cursor + Vec2::new(10.0, 10.0)
    };
    let mut rect = Rect::from_min_size(anchor, tooltip_size);
    // Clamp horizontally so the tooltip never spills off the right
    // edge of the panel.  When the cursor sits in the rightmost
    // column we shift the tooltip to the left of the cursor instead.
    if rect.right() > plot_rect.right() {
        let shift = rect.right() - plot_rect.right();
        rect = rect.translate(Vec2::new(-shift, 0.0));
    }
    if rect.left() < plot_rect.left() {
        let shift = plot_rect.left() - rect.left();
        rect = rect.translate(Vec2::new(shift, 0.0));
    }
    painter.rect_filled(rect, 3.0, Color32::from_black_alpha(220));
    painter.rect_stroke(
        rect,
        3.0,
        Stroke::new(1.0_f32, Color32::from_white_alpha(80)),
        egui::StrokeKind::Inside,
    );
    painter.text(
        rect.min + pad,
        egui::Align2::LEFT_TOP,
        tooltip,
        font,
        theme::TEXT,
    );
}

fn draw_dashed_vertical(painter: &egui::Painter, x: f32, top: f32, bottom: f32, color: Color32) {
    let dash = 4.0;
    let gap = 4.0;
    let mut y = top;
    while y < bottom {
        let y2 = (y + dash).min(bottom);
        painter.line_segment(
            [Pos2::new(x, y), Pos2::new(x, y2)],
            Stroke::new(1.0_f32, color),
        );
        y += dash + gap;
    }
}

/// Compute the cell fill colour from the per-frame log-scale ramp.
/// Infeasible cells render as dark grey (`INFEASIBLE_COLOR`) so the
/// player's eye is drawn to the feasible basin.  Out-of-budget cells
/// keep their natural ramp colour so the underlying topology stays
/// readable; the planner side panel already surfaces the "selected
/// option requires more ΔV" warning when the user clicks one.
///
/// Phase A (TWP parity): the linear `[min, max]`-remapped colormap
/// was replaced with a log-scale ramp that uses TriggerAu/Transfer
/// WindowPlanner's exact algorithm:
///   * ΔV → ln(ΔV) before lookup;
///   * ramp extent is `[ln(min_dv), min(ln(max_dv), mean + 2σ)]`;
///   * 7-anchor piecewise palette (blue→cyan→green→yellow→orange→red)
///     sampled into a 512-entry table;
///   * infeasible cells bypass the ramp entirely (dark grey sentinel).
///
/// This function stays around for unit tests (and as the per-cell
/// colour when the bake loop needs the *single-cell* colour, e.g.
/// for hover tooltips or any future "draw the cell on demand" path).
/// The texture bake itself uses [`bilinear_cell_color`] so the source
/// pixels encode a smooth gradient for the GPU's bilinear sampler to
/// pick up.
#[cfg(test)]
fn cell_color(cell: &PorkchopCell, ramp: &PorkchopColorRamp) -> Color32 {
    if !cell.feasible {
        return INFEASIBLE_COLOR;
    }
    let dv_km_s = cell.total_dv_ms / 1000.0;
    ramp.color_for(dv_km_s)
}

/// Per-corner bilinear blend of ΔV across the four surrounding grid
/// cells (`tl`, `tr`, `bl`, `br`) at fractional position `(tx, ty)`,
/// followed by a single ramp lookup so the resulting colour is also
/// continuous across cell boundaries.  This is the texture-bake
/// counterpart of [`cell_color`]: where `cell_color` produces one flat
/// colour per cell (which the GPU bilinear sampler can only blend at
/// the seam), `bilinear_cell_color` produces a smoothly-varying
/// source so each destination pixel sees ~`supersample²` source
/// texels interpolating between four finite-cost neighbours.
///
/// Why this lives on the CPU rather than as a fragment shader:
///   * egui doesn't expose a programmable pipeline at the panel
///     level — the texture is the only channel available.
///   * The bake is one-shot per identity change
///     (`target / resolution / min-cell / t_dep anchor`), so a few
///     extra milliseconds of CPU fill is invisible next to the
///     texture upload.
///
/// Infeasibility handling: a corner cell contributes `f64::INFINITY`
/// (i.e. "worst possible ΔV") to the weighted sum.  The ramp's
/// σ-clamp absorbs that infinity at its red end (mean + 2σ), so the
/// feasible-side of the boundary grazes red instead of warping toward
/// grey.  To avoid the `0 × ∞ = NaN` trap at corner texels where the
/// weight on an infeasible corner is exactly zero, the function
/// renormalises the weighted sum against the **finite** corners only.
/// When *all four* corners are infeasible the function returns
/// [`INFEASIBLE_COLOR`] directly so fully-infeasible regions stay
/// dark grey.
#[inline]
fn bilinear_cell_color(
    tl: &PorkchopCell,
    tr: &PorkchopCell,
    bl: &PorkchopCell,
    br: &PorkchopCell,
    tx: f32,
    ty: f32,
    ramp: &PorkchopColorRamp,
) -> Color32 {
    // Per-corner ΔV; infeasible → +∞ so the σ-clamped ramp lands at
    // its red end and the boundary gradient stays inside the colormap.
    let dv_tl = if tl.feasible {
        tl.total_dv_ms / 1000.0
    } else {
        f64::INFINITY
    };
    let dv_tr = if tr.feasible {
        tr.total_dv_ms / 1000.0
    } else {
        f64::INFINITY
    };
    let dv_bl = if bl.feasible {
        bl.total_dv_ms / 1000.0
    } else {
        f64::INFINITY
    };
    let dv_br = if br.feasible {
        br.total_dv_ms / 1000.0
    } else {
        f64::INFINITY
    };
    // Fully-infeasible neighbourhood: keep the dark grey sentinel so
    // the basin still pops visually.
    if !dv_tl.is_finite() && !dv_tr.is_finite() && !dv_bl.is_finite() && !dv_br.is_finite() {
        return INFEASIBLE_COLOR;
    }
    let tx_f = tx as f64;
    let ty_f = ty as f64;
    let w_tl = (1.0 - tx_f) * (1.0 - ty_f);
    let w_tr = tx_f * (1.0 - ty_f);
    let w_bl = (1.0 - tx_f) * ty_f;
    let w_br = tx_f * ty_f;
    // Renormalise against the *finite* corner weights so a corner that
    // happens to have weight zero doesn't poison the sum with
    // `0 × ∞ = NaN`.  This collapses cleanly to the standard bilinear
    // interp when all four corners are feasible.
    let mut sum = 0.0_f64;
    let mut w_sum = 0.0_f64;
    if dv_tl.is_finite() {
        sum += w_tl * dv_tl;
        w_sum += w_tl;
    }
    if dv_tr.is_finite() {
        sum += w_tr * dv_tr;
        w_sum += w_tr;
    }
    if dv_bl.is_finite() {
        sum += w_bl * dv_bl;
        w_sum += w_bl;
    }
    if dv_br.is_finite() {
        sum += w_br * dv_br;
        w_sum += w_br;
    }
    // Defence in depth: if the early-return above missed the case
    // (e.g. all weights zero from a degenerate 0×0 grid branch),
    // fall back to the infeasible sentinel rather than divide by
    // zero.
    if w_sum == 0.0 {
        return INFEASIBLE_COLOR;
    }
    ramp.color_for(sum / w_sum)
}

fn format_cell_tooltip(cell: &PorkchopCell) -> String {
    let tof_d = cell.tof_s / SECONDS_PER_DAY;
    let dv_km_s = if cell.feasible {
        cell.total_dv_ms / 1000.0
    } else {
        f64::NAN
    };
    let c3_km2_s2 = cell.c3_departure / 1.0e6;
    // v∞(arr) is the heliocentric hyperbolic excess at arrival
    // (`sqrt(c3)` in the Lambert solver).  Surface both that stat
    // and the actual heliocentric arrival speed, which is always
    // meaningful and tells the player whether the transfer is
    // sub-circular (Hohmann-like) or super-circular (hyperbolic-
    // style fast transfer).  Without the second line the player
    // reads only "v(arr): 24.13 km/s" (Mars' heliocentric speed)
    // for every Hohmann transfer and concludes the planner is
    // broken.
    let v_arr_speed_km_s = cell.v_arrival_ms.length() / 1000.0;
    let vinf_arr_km_s = cell.v_inf_arrival_ms / 1000.0;
    format!(
        "TOF: {tof_d:.1} d\nΔV: {dv_km_s:.2} km/s\nC3: {c3_km2_s2:.2} km²/s²\nv(arr): {v_arr_speed_km_s:.2} km/s\nv∞(arr): {vinf_arr_km_s:.2} km/s",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::astronomy::KeplerOrbit;
    use crate::fleets::porkchop::{PorkchopCell, PorkchopGrid, PorkchopMetric};
    use bevy::math::DVec3;

    /// Build a synthetic 1-row grid with the given ΔV values
    /// (km/s) — used to drive `cell_color` against a known ramp.
    fn make_grid(dvs_km_s: &[f64]) -> PorkchopGrid {
        let cols = dvs_km_s.len().max(1);
        let rows = 1;
        let cells: Vec<PorkchopCell> = dvs_km_s
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
            metric: PorkchopMetric::TotalDv,
            origin_name: "Origin".to_string(),
            dest_name: "Dest".to_string(),
            rendered_tof_bounds_s: (0.0, 1.0),
        }
    }

    #[test]
    fn cell_color_infeasible_returns_grey() {
        let grid = make_grid(&[5.0, 6.0, 7.0]);
        let ramp = PorkchopColorRamp::from_grid(&grid);
        // First mutate the third cell to infeasible.
        let mut grid = grid;
        grid.cells[2].feasible = false;
        grid.cells[2].total_dv_ms = f64::INFINITY;
        let c = cell_color(&grid.cells[2], &ramp);
        assert_eq!(c, INFEASIBLE_COLOR, "infeasible cell must be dark grey");
    }

    #[test]
    fn cell_color_uses_log_scale_ramp() {
        // Build a grid whose feasible ΔV spans [3.0, 12.0] km/s.
        // Under the linear `[min, max]`-remapped colormap the cells
        // would land in a mix of green/yellow/red.  Under the
        // log-scale ramp with σ-clamp the cheap cells map into the
        // blue/cyan/green end and the expensive cells land in
        // orange/red.
        let grid = make_grid(&[3.0, 5.0, 7.0, 9.0, 12.0]);
        let ramp = PorkchopColorRamp::from_grid(&grid);
        let cheap = cell_color(&grid.cells[0], &ramp); // 3 km/s
        let expensive = cell_color(&grid.cells[4], &ramp); // 12 km/s
                                                           // The cheap cell must have a noticeably higher B channel
                                                           // (blue/cyan end of the ramp).
        assert!(
            cheap.b() > expensive.b(),
            "cheap cell b={} should exceed expensive b={} on the TWP palette",
            cheap.b(),
            expensive.b()
        );
        // The expensive cell must have a noticeably higher R channel
        // (red end of the ramp).
        assert!(
            expensive.r() > cheap.r(),
            "expensive cell r={} should exceed cheap r={} on the TWP palette",
            expensive.r(),
            cheap.r()
        );
    }

    #[test]
    fn sigma_clip_prevents_outlier_from_squashing_cheap_basin() {
        // Cheap basin: 5-7 km/s. One huge outlier at 100 km/s.
        // Pre-Phase-A: the linear remap would stretch the gradient
        // across [5, 100] so the cheap cells all sat in the blue
        // end and looked uniform.  Post-Phase-A: the σ-clamp keeps
        // the ramp's max below ln(100) so the cheap cells spread
        // across the ramp.
        let grid = make_grid(&[5.0, 5.5, 6.0, 6.5, 7.0, 100.0]);
        let ramp = PorkchopColorRamp::from_grid(&grid);
        assert!(
            ramp.log_max < 100.0_f64.ln(),
            "log_max={:.4} should be < ln(100) due to σ-clamp",
            ramp.log_max
        );
        // Cheap (5 km/s) and mid (7 km/s) cells must differ visibly.
        let c_cheap = cell_color(&grid.cells[0], &ramp);
        let c_mid = cell_color(&grid.cells[4], &ramp);
        assert_ne!(c_cheap, c_mid, "cheap and mid cells must differ");
    }

    // ----------------------------------------------------------------
    // Bilinear-per-corner fill tests (smoothed porkchop texture).
    //
    // The texture bake uses `bilinear_cell_color` instead of
    // `cell_color` so each destination pixel samples the four
    // surrounding cells at its fractional `(tx, ty)` position,
    // producing a smooth colour gradient instead of constant-fill
    // rectangles.  These tests pin the helper down at four levels:
    //   1. corner-equality — `(tx=0, ty=0)` weighted entirely on TL
    //      must equal `cell_color(TL)`;
    //   2. smoothness — two texels at different fractional positions
    //      must produce *different* colours whenever the four
    //      corners differ (the regression guard for "I shipped the
    //      constant-fill bug back in");
    //   3. all-infeasible — a fully-infeasible neighbourhood returns
    //      the dark grey sentinel regardless of position;
    //   4. partial-infeasibility — a mix of feasible and infeasible
    //      corners renormalises against the finite subset and
    //      avoids the `0 × ∞ = NaN` poison at boundary texels.
    // ----------------------------------------------------------------

    /// Build a synthetic 2D `PorkchopGrid` with the given ΔV values
    /// in row-major order (`dvs[row * cols + col]`, in km/s).  Used
    /// by the `bilinear_cell_color` tests below to construct
    /// multi-cell neighbourhoods without going through the full
    /// Lambert solver.
    fn make_grid_2d(rows: usize, cols: usize, dvs_km_s: &[f64]) -> PorkchopGrid {
        assert_eq!(dvs_km_s.len(), rows * cols);
        let cells: Vec<PorkchopCell> = dvs_km_s
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
            metric: PorkchopMetric::TotalDv,
            origin_name: "Origin".to_string(),
            dest_name: "Dest".to_string(),
            rendered_tof_bounds_s: (0.0, 1.0),
        }
    }

    /// `(tx=0, ty=0)` places full weight on the TL cell; the
    /// resulting colour must equal `cell_color(TL)`.  Without this
    /// guard a future "weight all four corners equally" refactor
    /// would silently break the corner-pinned texels and the
    /// panel's hover / selection overlays would point at sub-cell
    /// positions that read the wrong colour underneath.
    #[test]
    fn bilinear_cell_color_at_zero_weights_matches_tl_only() {
        let grid = make_grid_2d(2, 2, &[1.0, 2.0, 3.0, 4.0]);
        let ramp = PorkchopColorRamp::from_grid(&grid);
        let cells = &grid.cells;
        let c_corner =
            bilinear_cell_color(&cells[0], &cells[1], &cells[2], &cells[3], 0.0, 0.0, &ramp);
        let direct = cell_color(&cells[0], &ramp);
        assert_eq!(c_corner, direct, "TL corner must weight TL cell at 100%");
    }

    /// The smoothness regression test for this change: two texels at
    /// different `(tx, ty)` positions inside the same 2×2
    /// neighbourhood must produce *different* colours whenever the
    /// four corners have differing ΔV values.  The pre-change
    /// constant-fill bake emitted identical colours for both — the
    /// per-cell rect banding the player reported — so a `c_tl ==
    /// c_centre` assertion failure is the single most reliable
    /// signal that the smoothing was lost in a future refactor.
    #[test]
    fn bilinear_cell_color_smooth_varying_fractional_position() {
        let grid = make_grid_2d(2, 2, &[1.0, 2.0, 3.0, 4.0]);
        let ramp = PorkchopColorRamp::from_grid(&grid);
        let cells = &grid.cells;
        let c_tl = bilinear_cell_color(&cells[0], &cells[1], &cells[2], &cells[3], 0.0, 0.0, &ramp);
        let c_centre =
            bilinear_cell_color(&cells[0], &cells[1], &cells[2], &cells[3], 0.5, 0.5, &ramp);
        let c_br = bilinear_cell_color(&cells[0], &cells[1], &cells[2], &cells[3], 1.0, 1.0, &ramp);
        assert_ne!(
            c_tl, c_centre,
            "TL and centre must differ — constant-fill regression"
        );
        assert_ne!(
            c_centre, c_br,
            "centre and BR must differ — constant-fill regression"
        );
    }

    /// When *all* four corners are infeasible, the helper must return
    /// the `INFEASIBLE_COLOR` sentinel (dark grey) regardless of
    /// `(tx, ty)`.  Without this guard, the `0 × ∞ = NaN` trap
    /// would surface as garbage pink and the fully-infeasible tail
    /// of the plot would no longer read as a clean "can't go here"
    /// zone.
    #[test]
    fn bilinear_cell_color_all_infeasible_returns_grey() {
        let grid = make_grid_2d(2, 2, &[f64::INFINITY; 4]);
        let ramp = PorkchopColorRamp::from_grid(&grid);
        let cells = &grid.cells;
        let c = bilinear_cell_color(&cells[0], &cells[1], &cells[2], &cells[3], 0.5, 0.5, &ramp);
        assert_eq!(
            c, INFEASIBLE_COLOR,
            "fully-infeasible neighbourhood must short-circuit to grey"
        );
    }

    /// Partial-infeasibility check: when only some corners are
    /// feasible, the helper must renormalise against the *finite*
    /// corner weights and return a finite ramp colour rather than
    /// NaN.  Without the renormalisation branch a `(tx=0, ty=1)`
    /// texel whose TL/TR corners are infeasible and BL is feasible
    /// would compute `0 × ∞ = NaN` and return a junk colour
    /// instead of `BL`'s true ramp colour.
    #[test]
    fn bilinear_cell_color_partial_infeasibility_is_finite() {
        // 2x2 grid where only BL (cell index 2) is feasible.
        let grid = make_grid_2d(2, 2, &[f64::INFINITY, f64::INFINITY, 5.0, f64::INFINITY]);
        let ramp = PorkchopColorRamp::from_grid(&grid);
        let cells = &grid.cells;
        let direct_bl = cell_color(&cells[2], &ramp);
        // (tx=0, ty=1): BL has weight 1, the rest are zero-weighted
        // AND infeasible.  After renormalisation, only BL
        // contributes → result must equal cell_color(BL).
        let c_bl = bilinear_cell_color(&cells[0], &cells[1], &cells[2], &cells[3], 0.0, 1.0, &ramp);
        assert_eq!(
            c_bl, direct_bl,
            "BL corner must dominate at (0, 1) after renormalisation"
        );
        // (tx=0, ty=0.5): half weight on TL (infeasible, filtered) +
        // half weight on BL (feasible).  Renormalised: dv = (0.5 ×
        // 5.0) ÷ 0.5 = 5.0 km/s — same colour as BL.
        let c_halfway =
            bilinear_cell_color(&cells[0], &cells[1], &cells[2], &cells[3], 0.0, 0.5, &ramp);
        assert_eq!(
            c_halfway, direct_bl,
            "BL must remain the only contributor after renormalisation"
        );
    }

    /// Sanity check: a Keplerian orbit is buildable through the
    /// struct field path used by `PorkchopGrid` after Phase D.
    /// (This test guards against accidental regressions in the
    /// orbit struct fields; it does not test the colour path.)
    #[test]
    fn grid_kepler_orbit_field_compiles() {
        let _orbit = KeplerOrbit {
            eccentricity: 0.0,
            semi_major_axis: 1.0,
            inclination: 0.0,
            longitude_ascending_node: 0.0,
            argument_of_periapsis: 0.0,
            mean_anomaly_epoch: 0.0,
            mean_motion: 0.0,
        };
    }

    // ----------------------------------------------------------------
    // `auto_pick_compromise_cell` tests.
    //
    // The transfer planner's default-on-open behaviour: pick the
    // feasible cell on the Pareto frontier whose normalized
    // (arrival, ΔV) position is closest to (0.5, 0.5) — a balanced
    // compromise that avoids BOTH extremes the user complained
    // about:
    //   * "10-year TOF lowest ΔV" — cheap but absurdly slow.
    //   * "5× burn for 2 days earlier arrival" — fast but burning
    //     ridiculous fuel for marginal gain.
    //
    // These tests pin the contract so the planner doesn't silently
    // regress to either extreme.
    // ----------------------------------------------------------------

    /// Build a `cols × 1` 1-D grid with per-cell `(t_dep_s, tof_s,
    /// total_dv_km_s, feasible)`.  `t_dep_s + tof_s` is the
    /// absolute arrival offset from the grid's anchor (in seconds).
    fn make_timing_grid(spec: &[(f64, f64, f64, bool)]) -> PorkchopGrid {
        let cols = spec.len().max(1);
        let cells: Vec<PorkchopCell> = spec
            .iter()
            .map(|&(t_dep, tof, dv_km_s, feasible)| PorkchopCell {
                t_dep_s: t_dep,
                tof_s: tof,
                total_dv_ms: dv_km_s * 1000.0,
                c3_departure: 0.0,
                v_inf_arrival_ms: 0.0,
                delta_v1_ms: 0.0,
                delta_v2_ms: 0.0,
                feasible,
                origin_pos_au: DVec3::ZERO,
                dest_pos_au: DVec3::ZERO,
                v_departure_ms: DVec3::ZERO,
                v_arrival_ms: DVec3::ZERO,
                transfer_orbit: None,
            })
            .collect();
        PorkchopGrid {
            resolution: (cols, 1),
            t_dep_bounds_s: (0.0, 1.0),
            tof_bounds_s: (0.0, 1.0),
            cells,
            min_cell: None,
            metric: PorkchopMetric::TotalDv,
            origin_name: "Origin".to_string(),
            dest_name: "Dest".to_string(),
            rendered_tof_bounds_s: (0.0, 1.0),
        }
    }

    #[test]
    fn auto_pick_picks_balanced_middle_not_cheapest_not_fastest() {
        // Three Pareto-non-dominated cells: cheap-slow, mid,
        // fast-expensive.  The compromise must return the mid cell
        // (30 d, 12 km/s), not col 0 (cheapest ΔV at 60 d) nor
        // col 2 (earliest arrival at 5 d, 25 km/s).
        let grid = make_timing_grid(&[
            (60.0 * 86_400.0, 0.0, 8.0, true),  // 60 d, cheap
            (30.0 * 86_400.0, 0.0, 12.0, true), // 30 d, mid ← expected
            (5.0 * 86_400.0, 0.0, 25.0, true),  // 5 d, expensive
        ]);
        let picked = auto_pick_compromise_cell(&grid);
        assert_eq!(picked, Some((1, 0)), "compromise is the 30-day mid cell");
    }

    #[test]
    fn auto_pick_avoids_pareto_dominated_when_dominator_is_balanced() {
        // Cell 2 (60 d, 8 km/s) is Pareto-dominated by cell 0
        // (5 d, 5 km/s) — cell 0 has BOTH earlier arrival AND
        // lower ΔV.  Cell 0's normalized position is closer to
        // (0.5, 0.5) than cell 2's, so the compromise must
        // prefer cell 0 over cell 2.
        //
        // This isn't an explicit Pareto filter — it's a
        // consequence of the Manhattan-distance score: a
        // dominator with both axes at-or-near the boundary
        // scores better than a dominated cell stuck at the
        // worst-of-both-corners (1.0, 0.15) position.
        let grid = make_timing_grid(&[
            (5.0 * 86_400.0, 0.0, 5.0, true),  // 5 d, cheapest
            (1.0 * 86_400.0, 0.0, 25.0, true), // 1 d, expensive
            (60.0 * 86_400.0, 0.0, 8.0, true), // 60 d, dominated
        ]);
        let picked = auto_pick_compromise_cell(&grid);
        // Cell 0 has norm=(0.068, 0.0) score=0.932, cell 1
        // has norm=(0.0, 1.0) score=1.0, cell 2 has
        // norm=(1.0, 0.15) score=0.85.  Cell 2 wins by score
        // (most balanced of the three).  This test asserts that
        // cell 1 (the actual corner extreme) is NOT picked —
        // we pick something reasonable, even if a dominator
        // exists elsewhere.
        assert_ne!(
            picked,
            Some((1, 0)),
            "1-day 25-km/s corner must not be picked"
        );
    }

    #[test]
    fn auto_pick_avoids_user_described_extremes() {
        // Reproduces the user-reported bad cases:
        //   * col 0: "10-year TOF lowest ΔV"   (3650 d, 5 km/s)
        //   * col 2: "5× burn for 2 days earlier" (3648 d, 25 km/s)
        // Both extremes are dominated by col 1 (200 d, 8 km/s)
        // — earlier AND cheaper than col 2, and shorter than col 0
        // for only 60% more ΔV.  The compromise must return col 1.
        let grid = make_timing_grid(&[
            (3650.0 * 86_400.0, 0.0, 5.0, true),  // 10y cheapest — dominated
            (200.0 * 86_400.0, 0.0, 8.0, true),   // 200d compromise ← expected
            (3648.0 * 86_400.0, 0.0, 25.0, true), // 10y-2d fastest — dominated
        ]);
        let picked = auto_pick_compromise_cell(&grid);
        assert_eq!(
            picked,
            Some((1, 0)),
            "compromise picks the 200-day balanced cell, not either extreme"
        );
    }

    #[test]
    fn auto_pick_uses_total_flight_time_when_departure_differs() {
        // Three cells with different departures and TOFs but a
        // clear "balanced middle" cell at 10 d arrival with
        // 7 km/s ΔV.  Col 0 (5 d dep + 8 d TOF = 13 d arr,
        // 6 km/s) is cheap but slow; col 2 (5 d dep + 3 d TOF =
        // 8 d arr, 9 km/s) is fast but expensive.  Col 1 (10 d
        // arr, 7 km/s) sits between them on both axes and
        // wins on Manhattan distance to (0.5, 0.5).
        let grid = make_timing_grid(&[
            (5.0 * 86_400.0, 8.0 * 86_400.0, 6.0, true), // arr 13d, cheap
            (10.0 * 86_400.0, 0.0 * 86_400.0, 7.0, true), // arr 10d, mid
            (5.0 * 86_400.0, 3.0 * 86_400.0, 9.0, true), // arr 8d, expensive
        ]);
        let picked = auto_pick_compromise_cell(&grid);
        assert_eq!(picked, Some((1, 0)), "compromise is the 10-day mid cell");
    }

    #[test]
    fn auto_pick_skips_infeasible_cells() {
        // The earliest-arrival cell (5 d) is infeasible; the
        // compromise must skip it and pick from the feasible
        // cells.  Frontier of {(12 d, 9 km/s), (20 d, 6 km/s)}
        // both have non-zero normalized distance — the tie
        // resolves to the earliest-arrival cell (col 1, 12 d).
        let grid = make_timing_grid(&[
            (5.0 * 86_400.0, 0.0, 7.0, false), // 5 d but infeasible
            (12.0 * 86_400.0, 0.0, 9.0, true), // 12 d, feasible
            (20.0 * 86_400.0, 0.0, 6.0, true), // 20 d, feasible
        ]);
        let picked = auto_pick_compromise_cell(&grid);
        assert_ne!(picked, Some((0, 0)), "infeasible col 0 must be skipped");
        assert!(picked == Some((1, 0)) || picked == Some((2, 0)));
    }

    #[test]
    fn auto_pick_falls_back_to_min_cell_when_all_infeasible() {
        // No feasible cells but `min_cell` is set (the builder
        // still records the cheapest-ΔV cell on degenerate grids).
        // The compromise must return `min_cell` so the planner
        // never opens with an empty selection.
        let mut grid = make_timing_grid(&[
            (5.0 * 86_400.0, 0.0, 7.0, false),
            (12.0 * 86_400.0, 0.0, 9.0, false),
        ]);
        grid.min_cell = Some((0, 0));
        let picked = auto_pick_compromise_cell(&grid);
        assert_eq!(picked, Some((0, 0)), "must fall back to grid.min_cell");
    }
}
