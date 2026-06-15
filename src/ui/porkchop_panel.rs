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

/// Selected-cell index in the grid.  Persisted on the `FleetUiState`
/// (`fleet_ui_state.selected_porkchop_cell: Option<(usize, usize)>`).
pub fn porkchop_panel(
    ui: &mut Ui,
    grid: &PorkchopGrid,
    cfg: &PorkchopConfig,
    selected: &mut Option<(usize, usize)>,
    fleet_max_dv_ms: f64,
    time_to_window_s: f64,
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

    let desired_size = Vec2::new(ui.available_width().max(320.0), 240.0);
    let (resp, painter) = ui.allocate_painter(desired_size, Sense::click_and_drag());
    let plot_rect = resp.rect;

    // Background
    painter.rect_filled(plot_rect, 0.0, theme::BG_SOLID);

    // Cell layout — pad 32 px on the left (TOF axis labels) and 18 px on
    // the bottom (t_dep axis labels).
    let pad_l = 36.0;
    let pad_b = 20.0;
    let grid_rect = Rect::from_min_size(
        Pos2::new(plot_rect.left() + pad_l, plot_rect.top() + 4.0),
        Vec2::new(
            plot_rect.width() - pad_l - 4.0,
            plot_rect.height() - pad_b - 4.0,
        ),
    );
    let cell_w = grid_rect.width() / cols as f32;
    let cell_h = grid_rect.height() / rows as f32;

    // 1. Cells (coloured rects)
    for row in 0..rows {
        for col in 0..cols {
            let cell = &grid.cells[row * cols + col];
            let rect = Rect::from_min_size(
                Pos2::new(
                    grid_rect.left() + col as f32 * cell_w,
                    grid_rect.top() + row as f32 * cell_h,
                ),
                Vec2::new(cell_w, cell_h),
            );
            let color = cell_color(cell, cfg, fleet_max_dv_ms);
            painter.rect_filled(rect, 0.0, color);
        }
    }

    // 2. Selection highlight (thick border on the selected cell)
    if let Some((sc, sr)) = *selected {
        if sc < cols && sr < rows {
            let rect = Rect::from_min_size(
                Pos2::new(
                    grid_rect.left() + sc as f32 * cell_w,
                    grid_rect.top() + sr as f32 * cell_h,
                ),
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
    // X-axis: 5 ticks
    for i in 0..=4 {
        let frac = i as f64 / 4.0;
        let t_dep_s = t_dep_min + frac * (t_dep_max - t_dep_min);
        let days = t_dep_s / SECONDS_PER_DAY;
        let x = grid_rect.left() + (frac as f32) * grid_rect.width();
        let label = format!("{days:+.0} d");
        painter.text(
            Pos2::new(x, grid_rect.bottom() + 4.0),
            egui::Align2::CENTER_TOP,
            label,
            font_id.clone(),
            label_color,
        );
    }
    // Y-axis: 4 ticks
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

    // 6. Click handler — set the selected cell.
    if let Some(pos) = resp.interact_pointer_pos() {
        if grid_rect.contains(pos) {
            let col = ((pos.x - grid_rect.left()) / cell_w) as usize;
            let row = ((pos.y - grid_rect.top()) / cell_h) as usize;
            let col = col.min(cols - 1);
            let row = row.min(rows - 1);
            if resp.clicked() || resp.drag_started() {
                let cell = &grid.cells[row * cols + col];
                if cell.feasible {
                    *selected = Some((col, row));
                }
            }
            // Hover tooltip (single frame, latched by interact_pos).
            if let Some((sc, sr)) = *selected {
                if sc == col && sr == row {
                    let tooltip = format_cell_tooltip(&grid.cells[row * cols + col]);
                    painter.text(
                        pos + Vec2::new(8.0, -8.0),
                        egui::Align2::LEFT_BOTTOM,
                        tooltip,
                        egui::FontId::proportional(11.0),
                        theme::TEXT,
                    );
                }
            }
        }
    }

    resp
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

fn cell_color(cell: &PorkchopCell, cfg: &PorkchopConfig, fleet_max_dv_ms: f64) -> Color32 {
    if !cell.feasible {
        // Infeasible cell: muted dim grey, lower-alpha than the colormap
        // stops so the player's eye is drawn to the feasible basin.
        return theme::TEXT_HINT.linear_multiply(0.5);
    }
    let dv_km_s = (cell.total_dv_ms / 1000.0).clamp(0.0, cfg.display_max_dv_km_s);
    let c = sample_colormap(&cfg.colormap, dv_km_s);
    // Mark out-of-budget cells (fleet ΔV too low) with a red tint.
    // We add a red offset to the colormap colour rather than swapping it
    // outright so the player can still see *which* ΔV band the cell sits
    // in even when it's unaffordable.
    if cell.total_dv_ms > fleet_max_dv_ms {
        return theme::red_tint(c);
    }
    c
}

fn sample_colormap(stops: &[crate::fleets::PorkchopColorStop], dv_km_s: f64) -> Color32 {
    if stops.is_empty() {
        return Color32::GRAY;
    }
    // Below the first stop: clamp to the first stop's colour.
    if dv_km_s <= stops[0].delta_v_km_s {
        return theme::color32_from_rgba(stops[0].rgba);
    }
    // Walk adjacent stop pairs; the last stop is the +∞ sentinel and
    // colours everything above the last finite stop.
    for window in stops.windows(2) {
        let a = &window[0];
        let b = &window[1];
        if dv_km_s <= b.delta_v_km_s {
            // +∞ sentinel: the b stop *is* the colour above the last
            // finite ΔV — no interpolation, no division by +INF.
            if !b.delta_v_km_s.is_finite() {
                return theme::color32_from_rgba(b.rgba);
            }
            let span = b.delta_v_km_s - a.delta_v_km_s;
            let t = if span > 0.0 {
                ((dv_km_s - a.delta_v_km_s) / span) as f32
            } else {
                0.0
            };
            return theme::lerp_rgba(a.rgba, b.rgba, t);
        }
    }
    // Above the last stop (defensive — the +∞ branch should have caught it).
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
    let vinf_arr_km_s = cell.v_inf_arrival_ms / 1000.0;
    format!(
        "TOF: {tof_d:.1} d\nΔV: {:.2} km/s\nC3: {:.2} km²/s²\nv∞ arr: {:.2} km/s",
        dv_km_s, c3_km2_s2, vinf_arr_km_s
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Apply the same premultiplied-alpha transform that
    /// `egui::Color32::from_rgba_unmultiplied` does internally, so tests
    /// can compare a `Color32` against a raw `(r, g, b, a)` tuple.
    fn premul(rgba: (u8, u8, u8, u8)) -> (u8, u8, u8, u8) {
        let (r, g, b, a) = rgba;
        let premul = |v: u8, alpha: u8| -> u8 { (v as f32 * alpha as f32 / 255.0).round() as u8 };
        (premul(r, a), premul(g, a), premul(b, a), a)
    }

    #[test]
    fn sample_colormap_interpolates_between_stops() {
        let cfg = PorkchopConfig::default();
        // 2.0 km/s should be between the 0.0 (green) and 4.0 (yellow) stops.
        let c = sample_colormap(&cfg.colormap, 2.0);
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
        // than `c.r()` / `c.g()` because `Color32::from_rgba_unmultiplied`
        // stores premultiplied values internally — `c.r()` returns
        // `round(r * a / 255)`, not the raw `r`.
        let stop = cfg.colormap.first().expect("default colormap has stops");
        let c = sample_colormap(&cfg.colormap, -1.0);
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
        let c = sample_colormap(&cfg.colormap, 1000.0);
        assert_eq!((c.r(), c.g(), c.b(), c.a()), premul(stop.rgba));
    }
}
