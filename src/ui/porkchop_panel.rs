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
    fleet_max_dv_ms: f64,
    time_to_window_s: f64,
    // Sim seconds elapsed since the rotating buffer was built.
    // Drives the scrolling x-axis: at shift_s=0 the visible window
    // starts at the buffer's left edge; as time advances the window
    // slides rightward through the buffer, and at shift_s=visible_width
    // the planner invalidates the cache and rebuilds.  Pass 0.0 for
    // the non-rotating-buffer case.
    shift_s: f64,
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

    // Compute the colormap ΔV range from the grid's *feasible* cells
    // (NASA/JPL-style relative colormap).  Without this, a porkchop
    // whose ΔV band sits entirely in the red half of the absolute
    // colormap would render almost uniformly red — the user reported
    // this as "color always looks quite uniform, not so much green to
    // red".  Stretching the colormap onto the grid's actual min/max
    // makes the gradient readable across every transfer type (Earth↔
    // Mars, deep-space, Jupiter moons).
    let grid_dv_range = compute_grid_dv_range(grid);
    let color_stops = resample_colormap(cfg, grid_dv_range);

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
    let (resp, painter) =
        ui.allocate_painter(desired_size, Sense::click() | Sense::hover());
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
    let cell_h = grid_rect.height() / rows as f32;

    // Compute the (col, row) of the cell currently under the cursor.
    // The cursor's visible col is `cursor_x / cell_w + scroll`; the
    // buffer col is that value floored.
    let hover_cell: Option<(usize, usize)> = resp
        .hover_pos()
        .filter(|pos| grid_rect.contains(*pos))
        .map(|pos| {
            let col_f = (pos.x - grid_rect.left()) / cell_w + scroll;
            let col = col_f.max(0.0) as usize;
            let row = ((pos.y - grid_rect.top()) / cell_h) as usize;
            (col.min(cols - 1), row.min(rows - 1))
        });

    // 1. Cells (coloured rects).  Each buffer column `c` has its
    // left edge at `x = (c - scroll) * cell_w` in the visible
    // window.  We draw any cell whose left edge is at most
    // one cell-width outside the visible window so the player
    // sees cells smoothly scrolling on and off the left edge.
    // The continuous-scroll x position is used directly so the
    // motion is sub-cell resolution with no boundary snap.
    let visible_w = visible_cols as f32 * cell_w;
    for c in 0..cols as i32 {
        let x = grid_rect.left() + (c as f32 - scroll) * cell_w;
        if x + cell_w < grid_rect.left() || x > grid_rect.left() + visible_w {
            continue;
        }
        for row in 0..rows {
            let cell = &grid.cells[row * cols + c as usize];
            let rect = Rect::from_min_size(
                Pos2::new(x, grid_rect.top() + row as f32 * cell_h),
                Vec2::new(cell_w, cell_h),
            );
            let color = cell_color(cell, &color_stops, grid_dv_range, fleet_max_dv_ms);
            painter.rect_filled(rect, 0.0, color);
        }
    }

    // 1b. Hover highlight — drawn AFTER the cell fill but BEFORE the
    // selection outline so the selection always wins when both apply.
    // The highlight is a thin semi-transparent white outline so it
    // reads against every colormap band (green, yellow, red, greyed).
    if let Some((hc, hr)) = hover_cell {
        if Some((hc, hr)) != *selected {
            let x = grid_rect.left() + (hc as f32 - scroll) * cell_w;
            if x + cell_w >= grid_rect.left() && x <= grid_rect.left() + visible_w {
                let rect = Rect::from_min_size(
                    Pos2::new(x, grid_rect.top() + hr as f32 * cell_h),
                    Vec2::new(cell_w, cell_h),
                );
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
            let rect = Rect::from_min_size(
                Pos2::new(x, grid_rect.top() + sr as f32 * cell_h),
                Vec2::new(cell_w, cell_h),
            );
            painter.rect_stroke(
                rect,
                0.0,
                Stroke::new(2.0, theme::RP_BLUE),
                egui::StrokeKind::Inside,
            );
        }
    }

    // 3. Grid lines
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
    for row in 0..=rows {
        let y = grid_rect.top() + row as f32 * cell_h;
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
    // the t_dep = 0 tick so the player can see at a glance that the
    // leftmost column is "depart immediately" rather than the
    // optimal-window departure date.
    for i in 0..=4 {
        let frac = i as f64 / 4.0;
        let t_dep_s = t_dep_min + frac * (t_dep_max - t_dep_min);
        let days = t_dep_s / SECONDS_PER_DAY;
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
    // The data cells render `row=0` (smallest TOF) at the *top* of the
    // grid (see "Cells" loop below: y = grid_rect.top() + row * cell_h)
    // and grow downward toward `tof_max`.  Labels MUST mirror that
    // direction or the tooltip and the y-axis tick the user reads off
    // disagree: hovering a cell near the bottom shows a large TOF in
    // the tooltip but the label next to the cursor reads a small TOF,
    // which the player reads as "tooltip is almost double the y-axis
    // value".  Anchor labels at the same y as their tick on the data
    // side — frac=0 (tof_min) at the top, frac=1 (tof_max) at the
    // bottom — matching the standard NASA / JPL porkchop convention
    // (short trips at the top, long trips at the bottom).
    let tof_min = grid.tof_bounds_s.0;
    let tof_max = grid.tof_bounds_s.1;
    for i in 0..=3 {
        let frac = i as f64 / 3.0;
        let tof_s = tof_min + frac * (tof_max - tof_min);
        let tof_label = if tof_s > SECONDS_PER_YEAR {
            format!("{:.1} yr", tof_s / SECONDS_PER_YEAR)
        } else {
            format!("{:.0} d", tof_s / SECONDS_PER_DAY)
        };
        let y = grid_rect.top() + (frac as f32) * grid_rect.height();
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
fn draw_cell_tooltip(
    painter: &egui::Painter,
    cursor: Pos2,
    plot_rect: Rect,
    cell: &PorkchopCell,
) {
    let tooltip = format_cell_tooltip(cell);
    let font = egui::FontId::proportional(11.0);
    let pad = Vec2::new(6.0, 4.0);
    let galley = painter.layout_no_wrap(tooltip.clone(), font.clone(), theme::TEXT);
    let tooltip_size = Vec2::new(
        galley.size().x + pad.x * 2.0,
        galley.size().y + pad.y * 2.0,
    );
    // Default anchor: down-and-right of the cursor.  Flip above the
    // cursor when there's more room up there than below, so the
    // tooltip stays visible for cells in the bottom rows.
    let below_room = plot_rect.bottom() - cursor.y;
    let above_room = cursor.y - plot_rect.top();
    let anchor = if below_room < tooltip_size.y + 12.0
        && above_room > tooltip_size.y + 12.0
    {
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

fn cell_color(
    cell: &PorkchopCell,
    color_stops: &[crate::fleets::PorkchopColorStop],
    grid_dv_range: Option<(f64, f64)>,
    fleet_max_dv_ms: f64,
) -> Color32 {
    if !cell.feasible {
        // Infeasible cell: muted dim grey, lower-alpha than the colormap
        // stops so the player's eye is drawn to the feasible basin.
        return theme::TEXT_HINT.linear_multiply(0.5);
    }
    let dv_km_s = cell.total_dv_ms / 1000.0;
    let c = sample_relative_colormap(color_stops, dv_km_s, grid_dv_range);
    // Mark out-of-budget cells (fleet ΔV too low) with a red tint.
    // We add a red offset to the colormap colour rather than swapping it
    // outright so the player can still see *which* ΔV band the cell sits
    // in even when it's unaffordable.
    if cell.total_dv_ms > fleet_max_dv_ms {
        return theme::red_tint(c);
    }
    c
}

/// ΔV range (km/s) of the grid's feasible cells, used to remap the
/// colormap stops.  Returns `None` when the grid has no feasible cells
/// (the caller falls back to the absolute colormap).
fn compute_grid_dv_range(grid: &PorkchopGrid) -> Option<(f64, f64)> {
    let mut min_dv = f64::INFINITY;
    let mut max_dv = f64::NEG_INFINITY;
    for cell in &grid.cells {
        if !cell.feasible || !cell.total_dv_ms.is_finite() {
            continue;
        }
        let dv_km_s = cell.total_dv_ms / 1000.0;
        if dv_km_s < min_dv {
            min_dv = dv_km_s;
        }
        if dv_km_s > max_dv {
            max_dv = dv_km_s;
        }
    }
    if !min_dv.is_finite() || !max_dv.is_finite() {
        return None;
    }
    // Floor on the span so degenerate grids (all feasible cells
    // clustered on one Hohmann ΔV) don't wash out to a uniform colour
    // band.  When the real span is below the floor we expand the
    // [min, max] window symmetrically around its midpoint.
    let span = max_dv - min_dv;
    if span < COLORMAP_MIN_SPAN_KM_S {
        let mid = 0.5 * (min_dv + max_dv);
        let half = COLORMAP_MIN_SPAN_KM_S * 0.5;
        min_dv = mid - half;
        max_dv = mid + half;
    }
    Some((min_dv.max(0.0), max_dv))
}

/// Re-sample the configured colormap onto the grid's ΔV range.  We
/// keep the same colour stops (green → yellow → red) but stretch
/// their `delta_v_km_s` anchors onto `[min_dv, max_dv]`.  This makes
/// every feasible cell cover the full gradient, no matter whether the
/// transfer is a low-energy Earth↔Moon hop or a high-energy Mars
/// opposition burn.
fn resample_colormap(
    cfg: &PorkchopConfig,
    range: Option<(f64, f64)>,
) -> Vec<crate::fleets::PorkchopColorStop> {
    let Some((min_dv, max_dv)) = range else {
        return cfg.colormap.clone();
    };
    if cfg.colormap.is_empty() {
        return cfg.colormap.clone();
    }
    // Drop the +∞ sentinel — its only role was to colour infeasible
    // cells, which we now draw separately in `cell_color`.
    let finite_stops: Vec<&crate::fleets::PorkchopColorStop> = cfg
        .colormap
        .iter()
        .filter(|s| s.delta_v_km_s.is_finite())
        .collect();
    if finite_stops.is_empty() {
        return cfg.colormap.clone();
    }
    let span = (max_dv - min_dv).max(COLORMAP_MIN_SPAN_KM_S);
    let first_dv = finite_stops[0].delta_v_km_s;
    let last_dv = finite_stops.last().unwrap().delta_v_km_s;
    let finite_span = (last_dv - first_dv).max(1e-9);
    let remap = |original: f64| -> f64 {
        let t = ((original - first_dv) / finite_span).clamp(0.0, 1.0);
        min_dv + t * span
    };
    let mut out: Vec<crate::fleets::PorkchopColorStop> = finite_stops
        .iter()
        .map(|s| crate::fleets::PorkchopColorStop {
            delta_v_km_s: remap(s.delta_v_km_s),
            rgba: s.rgba,
        })
        .collect();
    // Restore the +∞ sentinel at the end so `sample_relative_colormap`
    // can keep using the same +∞-as-sentinel convention.
    if let Some(last_cfg) = cfg.colormap.iter().find(|s| !s.delta_v_km_s.is_finite()) {
        out.push(*last_cfg);
    }
    out
}

/// Sample the (possibly resampled) colormap at a ΔV value in km/s.
/// Mirrors `sample_colormap` but supports a `None` `grid_dv_range` for
/// the absolute-fallback path.
fn sample_relative_colormap(
    stops: &[crate::fleets::PorkchopColorStop],
    dv_km_s: f64,
    _grid_dv_range: Option<(f64, f64)>,
) -> Color32 {
    if stops.is_empty() {
        return Color32::GRAY;
    }
    let dv = if dv_km_s.is_finite() { dv_km_s } else { 0.0 };
    // Below the first stop: clamp to the first stop's colour.
    if dv <= stops[0].delta_v_km_s {
        return theme::color32_from_rgba(stops[0].rgba);
    }
    // Walk adjacent stop pairs; the last stop may be a +∞ sentinel.
    for window in stops.windows(2) {
        let a = &window[0];
        let b = &window[1];
        if dv <= b.delta_v_km_s {
            if !b.delta_v_km_s.is_finite() {
                return theme::color32_from_rgba(b.rgba);
            }
            let span = b.delta_v_km_s - a.delta_v_km_s;
            let t = if span > 0.0 {
                ((dv - a.delta_v_km_s) / span) as f32
            } else {
                0.0
            };
            return theme::lerp_rgba(a.rgba, b.rgba, t);
        }
    }
    theme::color32_from_rgba(stops.last().unwrap().rgba)
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

    /// Apply the same premultiplied-alpha transform that the egui
    /// unmultiplied RGBA constructor does internally, so tests can
    /// compare a `Color32` against a raw `(r, g, b, a)` tuple.
    fn premul(rgba: (u8, u8, u8, u8)) -> (u8, u8, u8, u8) {
        let (r, g, b, a) = rgba;
        let premul = |v: u8, alpha: u8| -> u8 { (v as f32 * alpha as f32 / 255.0).round() as u8 };
        (premul(r, a), premul(g, a), premul(b, a), a)
    }

    #[test]
    fn sample_colormap_interpolates_between_stops() {
        let cfg = PorkchopConfig::default();
        // No grid range ⇒ absolute colormap.  2.0 km/s should sit
        // between the 0.0 (green) and 4.0 (yellow) stops.
        let c = sample_relative_colormap(&cfg.colormap, 2.0, None);
        let r = c.r();
        // Green is (40, 200, 80), yellow is (220, 200, 60).  At 50% interp
        // the green channel is somewhere between 40 and 220.
        assert!(r > 40 && r < 220, "r={r} should be in (40, 220)");
    }

    #[test]
    fn sample_colormap_clamp_below_first_stop() {
        let cfg = PorkchopConfig::default();
        // -1.0 km/s clamps to the first stop (delta_v_km_s = 0.0).
        // Compare against the raw RGBA tuple on the first stop rather
        // than `c.r()` / `c.g()` because egui stores premultiplied
        // values internally — `c.r()` returns `round(r * a / 255)`,
        // not the raw `r`.  See the `premul` helper above.
        let stop = cfg.colormap.first().expect("default colormap has stops");
        let c = sample_relative_colormap(&cfg.colormap, -1.0, None);
        assert_eq!((c.r(), c.g(), c.b(), c.a()), premul(stop.rgba));
    }

    #[test]
    fn sample_colormap_clamp_above_last_finite_stop() {
        let cfg = PorkchopConfig::default();
        // 1000 km/s clamps to the +∞ stop (60, 60, 60, 180).
        let stop = cfg
            .colormap
            .iter()
            .find(|s| !s.delta_v_km_s.is_finite())
            .expect("default colormap has +∞ sentinel stop");
        let c = sample_relative_colormap(&cfg.colormap, 1000.0, None);
        assert_eq!((c.r(), c.g(), c.b(), c.a()), premul(stop.rgba));
    }

    #[test]
    fn relative_colormap_stretches_onto_grid_range() {
        // Simulate an Earth↔Mars porkchop whose feasible ΔV band sits
        // entirely in [6.0, 9.0] km/s.  Without the relative colormap
        // the absolute 0–15 km/s gradient would render every cell in
        // the orange band; with it the gradient spans the full
        // green→red ramp.
        let cfg = PorkchopConfig::default();
        let range = Some((6.0_f64, 9.0_f64));
        let stops = resample_colormap(&cfg, range);
        // First finite stop should now anchor at 6.0 km/s.
        assert!((stops[0].delta_v_km_s - 6.0).abs() < 1e-9);
        // Last finite stop should anchor at 9.0 km/s.
        let last_finite = stops
            .iter()
            .rev()
            .find(|s| s.delta_v_km_s.is_finite())
            .expect("resampled colormap has finite stops");
        assert!((last_finite.delta_v_km_s - 9.0).abs() < 1e-9);
        // Min cell colour should be the green band.
        let c_min = sample_relative_colormap(&stops, 6.0, range);
        let r = c_min.r();
        assert!(r < 100, "min ΔV should sample the green end: r={r}");
        // Max cell colour should be the red band.
        let c_max = sample_relative_colormap(&stops, 9.0, range);
        let r = c_max.r();
        assert!(r > 150, "max ΔV should sample the red end: r={r}");
    }

    #[test]
    fn relative_colormap_floor_on_degenerate_grid() {
        // A degenerate grid with all cells at ΔV ≈ 7.5 km/s has a span
        // below the colormap floor; the range should be expanded
        // symmetrically so the colormap still produces visible
        // variation rather than a uniform fill.
        let range = compute_grid_dv_range_for_tests(&[7.5, 7.5, 7.5]);
        let (lo, hi) = range.expect("non-empty");
        assert!(hi - lo >= COLORMAP_MIN_SPAN_KM_S - 1e-9);
    }

    /// Test-only helper mirroring `compute_grid_dv_range` for arrays
    /// of ΔV values in km/s.  Used by `relative_colormap_floor_on_
    /// degenerate_grid` to avoid constructing a full `PorkchopGrid`.
    fn compute_grid_dv_range_for_tests(dvs_km_s: &[f64]) -> Option<(f64, f64)> {
        let mut min_dv = f64::INFINITY;
        let mut max_dv = f64::NEG_INFINITY;
        for &dv in dvs_km_s {
            if !dv.is_finite() {
                continue;
            }
            if dv < min_dv {
                min_dv = dv;
            }
            if dv > max_dv {
                max_dv = dv;
            }
        }
        if !min_dv.is_finite() || !max_dv.is_finite() {
            return None;
        }
        let span = max_dv - min_dv;
        if span < COLORMAP_MIN_SPAN_KM_S {
            let mid = 0.5 * (min_dv + max_dv);
            let half = COLORMAP_MIN_SPAN_KM_S * 0.5;
            min_dv = mid - half;
            max_dv = mid + half;
        }
        Some((min_dv.max(0.0), max_dv))
    }
}
