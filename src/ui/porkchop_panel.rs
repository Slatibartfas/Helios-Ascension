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
    // target/resolution/min-cell differs); on every other frame it
    // just re-uses the cached handle. The cached handle is `Some(...)`
    // for the steady state; the planner initialises both fields as
    // `None`.
    texture_cache: &mut Option<egui::TextureHandle>,
    texture_built_for: &mut Option<(Option<Entity>, (usize, usize), Option<(usize, usize)>)>,
) -> Response {
    let (cols, rows) = grid.resolution;
    if cols == 0 || rows == 0 {
        return ui.label("Empty porkchop grid (0×0).");
    }

    // Auto-pick the cheapest feasible cell when nothing is selected.
    if selected.is_none() {
        if let Some((c, r)) = grid.min_cell {
            *selected = Some((c, r));
        }
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
    let visible_cols = (cols / 2).saturating_sub(1).max(1);
    let t_dep_min = grid.t_dep_bounds_s.0;
    let t_dep_max = grid.t_dep_bounds_s.1;
    let col_step_s = if cols > 0 {
        (t_dep_max - t_dep_min) / cols as f64
    } else {
        1.0
    };
    let scroll = (shift_s / col_step_s) as f32;

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
    // Identity: `(target_body, grid.resolution, grid.min_cell)`.
    // Crucially we do NOT use `t_dep_bounds_s.0` (which is the
    // absolute sim-time anchor at build time — it shifts every
    // rotation trigger, ~1 real-second at 1 hr/s, causing a
    // rebake every second). The Lambert-solve cell content is
    // determined by `(col_frac, row_frac)` in the buffer, NOT by
    // the absolute anchor; rotation produces visually-equivalent
    // colours. So the texture only needs to rebake when:
    //   * target body changes (`target_body`),
    //   * grid resolution changes (config-driven, rare), or
    //   * the cheapest feasible cell shifts by more than a pixel
    //     (`min_cell`).
    // Each of these is a stable signal across rotations. The
    // texture upload (which runs synchronously in
    // `EguiPrimaryContextPass` and is the source of the visible
    // "jump every second") only fires on these rare events.
    let grid_identity: (Option<Entity>, (usize, usize), Option<(usize, usize)>) =
        (target_body, grid.resolution, grid.min_cell);
    let identity_mismatch = texture_cache.is_none()
        || texture_built_for.as_ref() != Some(&grid_identity);
    if identity_mismatch {
        // Bake a `cols × rows` ColorImage in row-major order so the
        // GPU can bilinear-filter it.  Rows correspond to TOF
        // (NASA convention: row 0 at the bottom of the panel,
        // but the image's pixel (0, 0) is its top-left, so we
        // flip the row index when packing — `row 0` in the grid
        // becomes the image's `rows - 1` row).
        let mut pixels: Vec<Color32> = Vec::with_capacity(cols * rows);
        for img_row in 0..rows {
            // The grid's `orig_row = rows - 1 - img_row`
            // because NASA convention flips the Y axis.
            let orig_row = rows - 1 - img_row;
            for col in 0..cols {
                let cell = &grid.cells[orig_row * cols + col];
                pixels.push(cell_color(cell, &ramp));
            }
        }
        let image = egui::ColorImage {
            size: [cols, rows],
            source_size: egui::Vec2::new(cols as f32, rows as f32),
            pixels,
        };
        // Allocate the texture (or update an existing one — but
        // since we're rebuilding from scratch each time the
        // identity changes, a fresh `load_texture` is simplest).
        // The TextureHandle drop is automatic when
        // `texture_cache = Some(new_handle)` replaces the old
        // one.
        let handle = ui.ctx().load_texture(
            "porkchop_grid",
            image,
            egui::TextureOptions::LINEAR,
        );
        *texture_cache = Some(handle);
        *texture_built_for = Some(grid_identity);
    }
    // Draw the full grid texture as a single quad mapped to the
    // grid_rect (UV = (0,0) → (1,1)).  Bilinear filtering
    // produces a continuous gradient across cell boundaries.
    if let Some(texture) = texture_cache.as_ref() {
        let uv = Rect::from_min_max(
            Pos2::new(0.0, 0.0),
            Pos2::new(1.0, 1.0),
        );
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
                    Stroke::new(1.5, Color32::from_white_alpha(180)),
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
                Stroke::new(2.0, theme::RP_BLUE),
                egui::StrokeKind::Inside,
            );
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
            Stroke::new(0.5, Color32::from_white_alpha(20)),
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
            Stroke::new(0.5, Color32::from_white_alpha(20)),
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
        Stroke::new(1.0, Color32::from_white_alpha(80)),
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
        painter.line_segment([Pos2::new(x, y), Pos2::new(x, y2)], Stroke::new(1.0, color));
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
fn cell_color(cell: &PorkchopCell, ramp: &PorkchopColorRamp) -> Color32 {
    if !cell.feasible {
        return INFEASIBLE_COLOR;
    }
    let dv_km_s = cell.total_dv_ms / 1000.0;
    ramp.color_for(dv_km_s)
}

fn format_cell_tooltip(cell: &PorkchopCell) -> String {
    let tof_d = cell.tof_s / SECONDS_PER_DAY;
    let dv_km_s = if cell.feasible {
        cell.total_dv_ms / 1000.0
    } else {
        f64::NAN
    };
    let c3_km2_s2 = cell.c3_departure / 1.0e6;
    // v∞(arr) is "speed above circular at destination" — 0 for any
    // Hohmann-shaped arrival (the spacecraft arrives *slower* than
    // circular and must boost to circularise).  Surface both that
    // stat and the actual arrival speed, which is always meaningful
    // and tells the player whether the transfer is sub-circular
    // (Hohmann-like) or super-circular (hyperbolic-style fast
    // transfer).  Without the second line the player reads "v∞
    // arr: 0.00" on every Hohmann and concludes the planner is
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
                total_dv_ms: if dv.is_finite() { dv * 1000.0 } else { f64::INFINITY },
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
}
