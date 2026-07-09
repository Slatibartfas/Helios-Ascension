//! GRA-367-B Phase 2: unified selected-option card.
//!
//! Replaces the per-class card fragments that previously lived inline
//! in `transfer_planner::render_transfer_planner` (porkchop stats panel,
//! 3-option row, gravity-assist `CollapsingHeader`, interstellar header,
//! cross-star header).  See `docs/design/TRANSFER_PLANNER_HARMONISATION.md`
//! §Phase 2 for the rationale.
//!
//! **Phase 2 scope (this file):** cosmetic unification of the *display*.
//! The `SelectionSource` enum only has `Empty` + `Porkchop` today; the
//! other variants arrive in Phases 3/4/5/6.  To bridge the gap, the
//! builder accepts `(TransferPlan, Option<CardSupplement>)` where the
//! `CardSupplement` carries the not-yet-migrated UI-owned fields
//! (`gravity_assist_candidates`, `cross_system_grid`, the interstellar
//! distance caption).  Each subsequent phase narrows the supplement as
//! its variant lands on `SelectionSource`.

use crate::fleets::components::{TransferPlan, TransferReferenceFrame};
use crate::fleets::orbital_mechanics::{format_delta_v, format_duration, TransferOption};
use crate::fleets::porkchop::{PorkchopCell, PorkchopGrid};
use crate::ui::theme;
use crate::ui::GravityAssistEntry;

use bevy_egui::egui;

/// Severity tag for a row, driving the `Color32` chosen by the renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Neutral informational row.
    Neutral,
    /// Positive value (ΔV savings, fuel within budget).
    Positive,
    /// Warning (ΔV over fleet budget, thrust-limited).
    Warn,
    /// Hard error (no feasible trajectory).
    Error,
}

/// Single labeled row in the card body.
#[derive(Debug, Clone)]
pub struct CardRow {
    pub label: String,
    pub value: String,
    pub severity: Severity,
}

/// Multi-leg transfer summary (1 leg for direct, 2 for gravity assist).
#[derive(Debug, Clone)]
pub struct CardLeg {
    pub leg_label: String,
    pub summary: String,
}

/// Unified per-class selected-option card.
///
/// Built by `build_selected_card` from `(TransferPlan, Option<CardSupplement>)`.
/// Rendered by `render_card` — both live in this module so each test
/// can assert on the same struct the production renderer consumes.
#[derive(Debug, Clone)]
pub struct CardWidget {
    /// Title row, e.g. `"Porkchop Cell: Earth → Mars"` or `"🌌 Interstellar Mission: α Centauri"`.
    pub title: String,
    /// Optional subtitle below the title (ETA, distance, class badge).
    pub subtitle: Option<String>,
    /// Ordered key/value rows (ΔV total, fuel, v∞ arrival, legs, …).
    pub rows: Vec<CardRow>,
    /// Optional warning row, rendered red if present.
    pub warn: Option<String>,
    /// Multi-leg summary (single entry for direct, two for GA).
    pub legs: Vec<CardLeg>,
    /// Optional override frame caption (Phase 1 read-only display).
    pub frame_caption: Option<String>,
}

/// Supplementary UI-owned fields not yet mirrored onto `TransferPlan`.
///
/// Each Phase-3/4/5/6 child shrinks this struct by moving one field onto
/// `SelectionSource`.  When empty, the supplement is `None` and the
/// builder is purely `TransferPlan`-driven — which is the Phase 6 goal.
///
/// GRA-382 (Phase 5 renderer cleanup) dropped the `is_inter_star_body_transfer`
/// boolean.  The binary cross-star identity now lives on
/// `SelectionSource::Binary { origin_star, dest_star }` and the
/// `cross_system_grid` / `cross_system_selected` pair continues to carry
/// the actual `PorkchopGrid` payload until Phase 6 collapses it onto
/// `SelectionSource::Binary { grid, .. }`.
#[derive(Debug, Clone, Default)]
pub struct CardSupplement {
    pub gravity_assist_candidates: Vec<GravityAssistEntry>,
    pub selected_gravity_assist: Option<usize>,
    pub cross_system_grid: Option<PorkchopGrid>,
    pub cross_system_selected: Option<(usize, usize)>,
    /// `(system_id, display_name, distance_ly)` when the target is an
    /// interstellar star system; populated for the 🌌 header card.
    pub star_system_snap: Option<(usize, String, f32)>,
    /// Reference-frame indicator caption (Phase 1 read-only).
    pub frame_caption: Option<String>,
}

/// Fleet-side data needed by the builder.  Kept separate from
/// `FleetUiState` so the builder can be unit-tested with hand-rolled
/// `FleetInfo` fixtures (no Bevy ECS required for snapshot tests).
///
/// `wet_mass_t` is only used to compute the fuel-percentage row when
/// the caller didn't pre-compute it via `Fleet::total_fuel_cost_for_dv`.
#[derive(Debug, Clone, Copy)]
pub struct FleetInfo {
    pub max_delta_v_ms: f64,
    pub wet_mass_t: f64,
}

/// Build a `CardWidget` from the active `TransferPlan` (Phase 1 mirror)
/// + optional UI-owned supplement + pre-computed fuel cost callback.
///
/// `fuel_cost_for_dv_ms(dv_ms)` is the fuel in tonnes for a given
/// ΔV — the caller usually passes a closure around `Fleet::total_fuel_cost_for_dv`
/// so the card uses the same physics the rest of the planner uses.
/// Snapshot tests can pass a hand-rolled closure.
pub fn build_selected_card(
    plan: &TransferPlan,
    supplement: Option<&CardSupplement>,
    fleet_info: FleetInfo,
    mut fuel_cost_for_dv_ms: impl FnMut(f64) -> f64,
) -> CardWidget {
    // Porkchop class — selected cell stats.
    if let Some((col, row)) = plan.selected_porkchop_cell {
        if let Some(grid) = plan.porkchop_grid.as_ref() {
            return build_porkchop_card(grid, col, row, fleet_info, &mut fuel_cost_for_dv_ms);
        }
    }

    // Interstellar class — 🌌 header.  Distinguished by the
    // `star_system_snap` supplement (Phase 3+ will move this onto
    // `SelectionSource::Interstellar { … }`).
    if let Some(sup) = supplement {
        if let Some((_, ref name, dist_ly)) = sup.star_system_snap {
            return build_interstellar_card(name, dist_ly, fleet_info);
        }
    }

    // Gravity-assist class — per-candidate card (one card per assist).
    if let Some(sup) = supplement {
        if let Some(idx) = sup.selected_gravity_assist {
            if let Some(entry) = sup.gravity_assist_candidates.get(idx) {
                return build_ga_card(entry);
            }
        } else if !sup.gravity_assist_candidates.is_empty() {
            // No assist selected but candidates exist — render the
            // collapsible summary (single card with the candidate list).
            return build_ga_summary_card(&sup.gravity_assist_candidates);
        }
    }

    // Cross-star single-cell — renders through the same option-row as
    // the 3-option path.  Phase 5 collapses this into a degenerate
    // `PorkchopGrid` (1×1) so it routes through the porkchop arm.
    if let Some(sup) = supplement {
        if let Some(grid) = sup.cross_system_grid.as_ref() {
            // `star_system_snap` carries `(system_id, name, distance_ly)`
            // for the interstellar target — surface the distance as
            // the subtitle (Phase 5 dropped the per-cell `distance_ly`
            // field when refactoring into the shared `PorkchopGrid`).
            let distance_ly = sup.star_system_snap.as_ref().map(|(_, _, ly)| *ly);
            return build_cross_star_card(grid, sup.cross_system_selected, distance_ly);
        }
    }

    // Legacy 3-option row — selected option stats.  This is the most
    // common short-hop / star-approach / moon path.
    if !plan.computed_options.is_empty() {
        let idx = plan
            .selected_option
            .min(plan.computed_options.len().saturating_sub(1));
        if let Some(option) = plan.computed_options.get(idx) {
            return build_3option_card(option, fleet_info, &mut fuel_cost_for_dv_ms);
        }
    }

    // No selection — empty placeholder so the renderer still draws.
    build_empty_card()
}

fn build_porkchop_card(
    grid: &PorkchopGrid,
    col: usize,
    row: usize,
    fleet_info: FleetInfo,
    fuel_cost_for_dv_ms: &mut dyn FnMut(f64) -> f64,
) -> CardWidget {
    let mut card = CardWidget {
        title: format!("Porkchop Cell: {} → {}", grid.origin_name, grid.dest_name),
        subtitle: None,
        rows: Vec::new(),
        warn: None,
        legs: vec![CardLeg {
            leg_label: "Leg 1".to_string(),
            summary: "Direct Lambert arc".to_string(),
        }],
        frame_caption: None,
    };
    if let Some(cell) = grid.cells.get(row * grid.resolution.0 + col) {
        if !cell.feasible {
            card.warn = Some("⚠ Selected cell is infeasible (Lambert solver failed).".to_string());
            return card;
        }
        append_porkchop_cell_rows(&mut card, cell, fleet_info, fuel_cost_for_dv_ms);
        let gap_ms = fleet_info.max_delta_v_ms - cell.total_dv_ms;
        if gap_ms < 0.0 {
            card.warn = Some(format!(
                "⚠ Exceeds fleet ΔV budget by {} (ΔV avail {:.2} km/s)",
                format_delta_v(-gap_ms),
                fleet_info.max_delta_v_ms / 1_000.0
            ));
        }
    } else {
        card.warn = Some("⚠ Selected cell out of grid bounds.".to_string());
    }
    card
}

fn append_porkchop_cell_rows(
    card: &mut CardWidget,
    cell: &PorkchopCell,
    fleet_info: FleetInfo,
    fuel_cost_for_dv_ms: &mut dyn FnMut(f64) -> f64,
) {
    let fuel_cost = fuel_cost_for_dv_ms(cell.total_dv_ms);
    let fuel_pct = if fleet_info.wet_mass_t > 0.0 {
        (fuel_cost / fleet_info.wet_mass_t * 100.0) as u32
    } else {
        0
    };
    let severity = if cell.total_dv_ms > fleet_info.max_delta_v_ms {
        Severity::Warn
    } else {
        Severity::Neutral
    };
    card.rows.push(CardRow {
        label: "ΔV total".to_string(),
        value: format!(
            "{:.2} km/s (dep {:.2} + arr {:.2})",
            cell.total_dv_ms / 1_000.0,
            cell.delta_v1_ms / 1_000.0,
            cell.delta_v2_ms / 1_000.0,
        ),
        severity,
    });
    card.rows.push(CardRow {
        label: "t_dep / TOF".to_string(),
        value: format!(
            "{:.0} d / {:.0} d",
            cell.t_dep_s / crate::ui::porkchop_panel::SECONDS_PER_DAY,
            cell.tof_s / crate::ui::porkchop_panel::SECONDS_PER_DAY,
        ),
        severity: Severity::Neutral,
    });
    card.rows.push(CardRow {
        label: "Fuel".to_string(),
        value: format!("{:.1} t ({fuel_pct}%)", fuel_cost),
        severity: Severity::Neutral,
    });
    card.rows.push(CardRow {
        label: "v(arr)".to_string(),
        value: format!("{:.2} km/s", cell.v_arrival_ms.length() / 1_000.0),
        severity: Severity::Neutral,
    });
    card.rows.push(CardRow {
        label: "v∞(arr)".to_string(),
        value: format!("{:.2} km/s", cell.v_inf_arrival_ms / 1_000.0),
        severity: Severity::Neutral,
    });
}

fn build_interstellar_card(name: &str, dist_ly: f32, fleet_info: FleetInfo) -> CardWidget {
    let card = CardWidget {
        title: format!("🌌 Interstellar Mission: {name}"),
        subtitle: Some(format!(
            "Distance: {:.2} ly = {:.0} AU",
            dist_ly,
            dist_ly as f64 * 63_241.077
        )),
        rows: vec![CardRow {
            label: "Mode".to_string(),
            value: "Point-and-burn (kinematic)".to_string(),
            severity: Severity::Neutral,
        }],
        warn: Some(
            "⚠ Interstellar navigation is point-and-burn. \
             Transfer windows do not apply. \
             Ensure adequate ΔV and life-support reserves."
                .to_string(),
        ),
        legs: vec![CardLeg {
            leg_label: "Leg 1".to_string(),
            summary: "Continuous thrust to destination".to_string(),
        }],
        frame_caption: Some(format!(
            "ΔV budget: {:.2} km/s",
            fleet_info.max_delta_v_ms / 1_000.0
        )),
    };
    card
}

fn build_ga_card(entry: &GravityAssistEntry) -> CardWidget {
    let opt = &entry.option;
    let savings_str = if opt.dv_savings_ms > 100.0 {
        format_delta_v(opt.dv_savings_ms)
    } else {
        format!("+{} (sub-optimal)", format_delta_v(-opt.dv_savings_ms))
    };
    let sign = if opt.extra_time_s >= 0.0 { "+" } else { "" };
    let win_str = if opt.window_period_s.is_finite() {
        format_duration(opt.window_period_s).to_string()
    } else {
        "∞".to_owned()
    };
    CardWidget {
        title: format!("⚡ via {}", opt.body_name),
        subtitle: None,
        rows: vec![
            CardRow {
                label: "ΔV saved".to_string(),
                value: savings_str,
                severity: if opt.dv_savings_ms > 100.0 {
                    Severity::Positive
                } else {
                    Severity::Warn
                },
            },
            CardRow {
                label: "Extra time".to_string(),
                value: format!("{sign}{}", format_duration(opt.extra_time_s.abs())),
                severity: Severity::Neutral,
            },
            CardRow {
                label: "Window every".to_string(),
                value: win_str,
                severity: Severity::Neutral,
            },
            CardRow {
                label: "v∞".to_string(),
                value: format_delta_v(opt.v_inf_ms),
                severity: Severity::Neutral,
            },
        ],
        warn: None,
        legs: vec![
            CardLeg {
                leg_label: "Leg 1".to_string(),
                summary: format!("Origin → {} flyby", opt.body_name),
            },
            CardLeg {
                leg_label: "Leg 2".to_string(),
                summary: format!("{} flyby → destination", opt.body_name),
            },
        ],
        frame_caption: None,
    }
}

fn build_ga_summary_card(candidates: &[GravityAssistEntry]) -> CardWidget {
    let mut rows = Vec::new();
    for entry in candidates {
        let savings_str = if entry.option.dv_savings_ms > 100.0 {
            format_delta_v(entry.option.dv_savings_ms)
        } else {
            format!(
                "+{} (sub-optimal)",
                format_delta_v(-entry.option.dv_savings_ms)
            )
        };
        rows.push(CardRow {
            label: format!("via {}", entry.option.body_name),
            value: format!(
                "ΔV {} · v∞ {}",
                savings_str,
                format_delta_v(entry.option.v_inf_ms)
            ),
            severity: if entry.option.dv_savings_ms > 100.0 {
                Severity::Positive
            } else {
                Severity::Warn
            },
        });
    }
    CardWidget {
        title: format!("⚡ Gravity Assists ({} available)", candidates.len()),
        subtitle: None,
        rows,
        warn: None,
        legs: vec![CardLeg {
            leg_label: "Assist".to_string(),
            summary: "Click 'Use Assist' to apply".to_string(),
        }],
        frame_caption: None,
    }
}

fn build_cross_star_card(
    grid: &PorkchopGrid,
    selected: Option<(usize, usize)>,
    distance_ly: Option<f32>,
) -> CardWidget {
    let (col, row) = selected.unwrap_or((0, 0));
    let (grid_cols, _grid_rows) = grid.resolution;
    let cell: Option<&PorkchopCell> = grid.cells.get(row * grid_cols + col);
    let subtitle = distance_ly.map(|ly| format!("Distance: {:.2} ly", ly));
    let mut card = CardWidget {
        title: format!("Cross-star Transfer: → {}", grid.dest_name),
        subtitle,
        rows: Vec::new(),
        warn: None,
        legs: vec![CardLeg {
            leg_label: "Leg 1".to_string(),
            summary: "System barycentric".to_string(),
        }],
        frame_caption: None,
    };
    match cell {
        Some(c) if c.feasible => {
            let dv_kms = c.total_dv_ms / 1_000.0;
            card.rows.push(CardRow {
                label: "ΔV".to_string(),
                value: format!("{dv_kms:.2} km/s"),
                severity: Severity::Neutral,
            });
            card.rows.push(CardRow {
                label: "Travel time".to_string(),
                value: format_duration(c.tof_s).to_string(),
                severity: Severity::Neutral,
            });
        }
        Some(_) => {
            card.warn =
                Some("⚠ No feasible cross-system trajectory for this (t_dep, tof).".to_string());
        }
        None => {
            card.warn = Some("⚠ Selected cell out of grid bounds.".to_string());
        }
    }
    card
}

fn build_3option_card(
    option: &TransferOption,
    fleet_info: FleetInfo,
    fuel_cost_for_dv_ms: &mut dyn FnMut(f64) -> f64,
) -> CardWidget {
    let fuel_cost = fuel_cost_for_dv_ms(option.total_delta_v_ms);
    let fuel_pct = if fleet_info.wet_mass_t > 0.0 {
        (fuel_cost / fleet_info.wet_mass_t * 100.0) as u32
    } else {
        0
    };
    let affordable = option.total_delta_v_ms <= fleet_info.max_delta_v_ms;
    let mut card = CardWidget {
        title: format!("Selected: {}", option.label),
        subtitle: None,
        rows: vec![
            CardRow {
                label: "Total ΔV".to_string(),
                value: format_delta_v(option.total_delta_v_ms),
                severity: if affordable {
                    Severity::Neutral
                } else {
                    Severity::Warn
                },
            },
            CardRow {
                label: "Travel time".to_string(),
                value: format_duration(option.transfer_time_s).to_string(),
                severity: Severity::Neutral,
            },
            CardRow {
                label: "Fuel".to_string(),
                value: format!("{:.1} t ({fuel_pct}%)", fuel_cost),
                severity: Severity::Neutral,
            },
            CardRow {
                label: "Departure burn".to_string(),
                value: format_delta_v(option.delta_v1_ms),
                severity: Severity::Neutral,
            },
            CardRow {
                label: "Arrival burn".to_string(),
                value: format_delta_v(option.delta_v2_ms),
                severity: Severity::Neutral,
            },
            CardRow {
                label: "Plane change ΔV".to_string(),
                value: format_delta_v(option.plane_change_dv_ms),
                severity: Severity::Neutral,
            },
        ],
        warn: if option.is_thrust_limited {
            Some(
                "⚠ Thrust-limited: ΔV requires continuous burn longer than transfer time."
                    .to_string(),
            )
        } else if !affordable {
            Some(format!(
                "⚠ Exceeds fleet ΔV budget by {} (ΔV avail {:.2} km/s)",
                format_delta_v(option.total_delta_v_ms - fleet_info.max_delta_v_ms),
                fleet_info.max_delta_v_ms / 1_000.0,
            ))
        } else {
            None
        },
        legs: vec![CardLeg {
            leg_label: "Leg 1".to_string(),
            summary: format!("Hohmann-class (energy ×{:.2})", option.energy_multiplier),
        }],
        frame_caption: None,
    };
    if option.burn_time_s > 0.0 {
        card.rows.push(CardRow {
            label: "Burn time".to_string(),
            value: format_duration(option.burn_time_s).to_string(),
            severity: Severity::Neutral,
        });
    }
    card
}

fn build_empty_card() -> CardWidget {
    CardWidget {
        title: "(no transfer selected)".to_string(),
        subtitle: None,
        rows: Vec::new(),
        warn: None,
        legs: Vec::new(),
        frame_caption: None,
    }
}

/// Render a `CardWidget` into the egui UI.
///
/// Layout: `ui.group → title (size 13 strong) → subtitle (size 11 dim) →
/// egui::Grid (2 col) → legs (small dim) → warn (red if Some)`.  The
/// commit-flow buttons (Execute / Cancel) stay in the caller — they
/// belong to the planner, not the card.
pub fn render_card(ui: &mut egui::Ui, card: &CardWidget) {
    ui.group(|ui| {
        ui.label(
            egui::RichText::new(&card.title)
                .strong()
                .size(13.0)
                .color(theme::ACCENT),
        );
        if let Some(sub) = &card.subtitle {
            ui.label(egui::RichText::new(sub).size(11.0).color(theme::TEXT_DIM));
        }
        if !card.rows.is_empty() {
            ui.add_space(4.0);
            egui::Grid::new("card_body")
                .num_columns(2)
                .spacing([8.0, 2.0])
                .show(ui, |ui| {
                    for row in &card.rows {
                        ui.label(egui::RichText::new(&row.label).size(11.0));
                        let color = match row.severity {
                            Severity::Neutral => theme::TEXT,
                            Severity::Positive => theme::GREEN,
                            Severity::Warn => theme::AMBER,
                            Severity::Error => theme::RED,
                        };
                        ui.label(
                            egui::RichText::new(&row.value)
                                .size(11.0)
                                .strong()
                                .color(color),
                        );
                        ui.end_row();
                    }
                });
        }
        if !card.legs.is_empty() {
            ui.add_space(2.0);
            for leg in &card.legs {
                ui.label(
                    egui::RichText::new(format!("{}: {}", leg.leg_label, leg.summary))
                        .size(10.5)
                        .color(theme::TEXT_DIM),
                );
            }
        }
        if let Some(frame) = &card.frame_caption {
            ui.label(
                egui::RichText::new(frame)
                    .size(10.5)
                    .italics()
                    .color(theme::TEXT_DIM),
            );
        }
        if let Some(warn) = &card.warn {
            ui.label(egui::RichText::new(warn).size(11.0).color(theme::RED));
        }
    });
}

/// Frame caption resolver — exposes the auto-detected frame so the
/// 1-line indicator above the picker can show it.  Mirrors the
/// pre-Phase-2 `resolve_planner_transfer_frame` logic without taking
/// a dependency on `FleetUiState`.
pub fn frame_caption(frame: Option<TransferReferenceFrame>) -> Option<String> {
    match frame? {
        TransferReferenceFrame::SystemBarycentric => Some("Frame: System Barycentric".to_string()),
        TransferReferenceFrame::Body(_) => Some("Frame: Body Local".to_string()),
    }
}

#[cfg(test)]
mod gra_384_snapshot_tests {
    //! GRA-384 — snapshot tests for the 4 transfer planner classes
    //! wired into the unified `build_selected_card` dispatcher
    //! (short-hop, star-approach, cross-star, interstellar).
    //!
    //! Each test feeds a hand-rolled `CardSupplement` to
    //! `build_selected_card` and asserts on the resulting
    //! `CardWidget` fields (title, rows, warn) so the dispatcher's
    //! per-class branch is locked.  Tests are Bevy-free: the
    //! planner's `build_selected_card` helper only consumes
    //! `TransferPlan` (Phase 1 mirror) + `CardSupplement`, so a
    //! default `TransferPlan` plus a hand-built supplement is
    //! enough to drive each branch.
    use super::*;
    use crate::fleets::components::{SelectionSource, TransferPlan};
    use crate::fleets::porkchop::{PorkchopCell, PorkchopGrid, PorkchopMetric};
    use bevy::math::DVec3;

    /// Stub fleet info — a 25 km/s ΔV budget with 1 000 t wet mass.
    /// Mirrors a small LEO-bound fleet that can comfortably afford a
    /// cislunar transfer but is well below the interstellar ΔV
    /// requirement, so the cross-star / interstellar cards light up
    /// the `Exceeds fleet ΔV budget` warning row.
    fn test_fleet_info() -> FleetInfo {
        FleetInfo {
            max_delta_v_ms: 25_000.0,
            wet_mass_t: 1_000.0,
        }
    }

    /// Build a degenerate 1×1 `PorkchopGrid` for cross-star /
    /// interstellar tests (GRA-367-E).  When `feasible` is `false`,
    /// the card surfaces the "No feasible cross-system trajectory"
    /// warning so the test can assert on the warn string.
    fn degenerate_cross_star_grid(feasible: bool, dv_ms: f64, dest_name: &str) -> PorkchopGrid {
        let cell = PorkchopCell {
            t_dep_s: 0.0,
            tof_s: 86_400.0 * 365.0 * 4.37, // 4.37 yr (α Cen TOF proxy)
            total_dv_ms: dv_ms,
            c3_departure: 0.0,
            v_inf_arrival_ms: 0.0,
            delta_v1_ms: dv_ms * 0.5,
            delta_v2_ms: dv_ms * 0.5,
            feasible,
            origin_pos_au: DVec3::ZERO,
            dest_pos_au: DVec3::new(4.37 * 63_241.077, 0.0, 0.0),
            v_departure_ms: DVec3::ZERO,
            v_arrival_ms: DVec3::new(dv_ms * 0.5, 0.0, 0.0),
            transfer_orbit: None,
        };
        PorkchopGrid {
            origin_name: "Sol".to_string(),
            dest_name: dest_name.to_string(),
            t_dep_bounds_s: (0.0, 0.0),
            tof_bounds_s: (0.0, 86_400.0 * 365.0 * 4.37),
            rendered_tof_bounds_s: (0.0, 86_400.0 * 365.0 * 4.37),
            resolution: (1, 1),
            cells: vec![cell],
            min_cell: if feasible { Some((0, 0)) } else { None },
            metric: PorkchopMetric::TotalDv,
        }
    }

    /// ── Cross-star (Sol → α Centauri) ────────────────────────────────
    /// Verifies the dispatcher routes the cross-system degenerate
    /// grid through `build_cross_star_card` and that the surface
    /// shows the destination name, distance subtitle, and ΔV / TOF
    /// rows.  Mirrors the GRA-367-E data-layer shape.
    #[test]
    fn cross_star_card_snapshot_alpha_centauri() {
        let grid = degenerate_cross_star_grid(true, 53_000.0, "α Centauri");
        // NOTE: do NOT set `star_system_snap` here — the dispatcher's
        // interstellar branch runs before the cross-star branch, and
        // populating it would route this test through the interstellar
        // card (which always surfaces a warn + has no "ΔV" row).
        let sup = CardSupplement {
            cross_system_grid: Some(grid.clone()),
            cross_system_selected: Some((0, 0)),
            ..CardSupplement::default()
        };
        let card = build_selected_card(
            &TransferPlan::default(),
            Some(&sup),
            test_fleet_info(),
            |_dv| 0.0,
        );
        assert!(
            card.title.contains("α Centauri"),
            "title must include destination name, got {:?}",
            card.title
        );
        assert!(
            card.subtitle.as_deref().unwrap_or("").contains("4.37"),
            "subtitle must include distance, got {:?}",
            card.subtitle
        );
        assert_eq!(card.legs.len(), 1, "single leg for direct cross-star");
        assert!(
            card.warn.is_none(),
            "feasible cell must not surface a warning, got {:?}",
            card.warn
        );
        let dv_row = card
            .rows
            .iter()
            .find(|r| r.label == "ΔV")
            .expect("cross-star card must include ΔV row");
        assert!(
            dv_row.value.contains("53.00"),
            "ΔV row must show 53.00 km/s, got {:?}",
            dv_row.value
        );
    }

    /// ── Interstellar (Sol → Sirius) ──────────────────────────────────
    /// Verifies the dispatcher routes the star-system supplement
    /// through `build_interstellar_card` (the 🌌 header) and that the
    /// 8.6 ly distance surfaces in the subtitle.  The ΔV budget
    /// warning fires because 8.6 ly × 12 km/s/ly ≈ 104 km/s ≫ 25 km/s
    /// fleet budget.
    #[test]
    fn interstellar_card_snapshot_sirius() {
        let sup = CardSupplement {
            star_system_snap: Some((7, "Sirius".to_string(), 8.6)),
            ..CardSupplement::default()
        };
        let card = build_selected_card(
            &TransferPlan::default(),
            Some(&sup),
            test_fleet_info(),
            |_dv| 0.0,
        );
        assert!(
            card.title.contains("Sirius"),
            "title must include Sirius, got {:?}",
            card.title
        );
        assert!(
            card.subtitle.as_deref().unwrap_or("").contains("8.60"),
            "subtitle must include distance, got {:?}",
            card.subtitle
        );
        assert!(
            card.warn
                .as_deref()
                .unwrap_or("")
                .contains("Interstellar navigation"),
            "interstellar warn must explain point-and-burn, got {:?}",
            card.warn
        );
        assert_eq!(card.legs.len(), 1);
    }

    /// ── Short-hop (Earth → Moon) ─────────────────────────────────────
    /// Verifies the dispatcher routes a `ShortHop` `SelectionSource`
    /// (set on `TransferPlan.source` by GRA-381's per-class mirror)
    /// through the same per-class rendering as a directly-populated
    /// grid.  We synthesise a 1×5 single-column grid (the RON
    /// `short_hop` category override's resolution) and assert the
    /// card title + structure.
    #[test]
    fn short_hop_card_snapshot_earth_moon() {
        let n_rows = 5;
        let cells: Vec<PorkchopCell> = (0..n_rows)
            .map(|row| PorkchopCell {
                t_dep_s: 0.0,
                tof_s: 86_400.0 * 3.0 + (row as f64) * 86_400.0, // 3-7 day crescent
                total_dv_ms: 3_200.0 + (row as f64) * 50.0,
                c3_departure: 0.0,
                v_inf_arrival_ms: 0.0,
                delta_v1_ms: 3_100.0,
                delta_v2_ms: 100.0,
                feasible: true,
                origin_pos_au: DVec3::new(6.571e-4, 0.0, 0.0), // LEO proxy
                dest_pos_au: DVec3::new(0.00257, 0.0, 0.0),    // Lunar SMA
                v_departure_ms: DVec3::ZERO,
                v_arrival_ms: DVec3::ZERO,
                transfer_orbit: None,
            })
            .collect();
        let grid = PorkchopGrid {
            origin_name: "Earth".to_string(),
            dest_name: "Moon".to_string(),
            t_dep_bounds_s: (0.0, 0.0),
            tof_bounds_s: (86_400.0 * 3.0, 86_400.0 * 7.0),
            rendered_tof_bounds_s: (86_400.0 * 3.0, 86_400.0 * 7.0),
            resolution: (1, n_rows),
            cells,
            min_cell: Some((0, 0)),
            metric: PorkchopMetric::TotalDv,
        };
        // The dispatcher's first arm reads `plan.selected_porkchop_cell` +
        // `plan.porkchop_grid`; populating only `SelectionSource::ShortHop`
        // on `plan.source` is not enough.  Mirror the production wiring
        // (which anchors `selected_porkchop_cell` after building the
        // grid) so the dispatcher routes through `build_porkchop_card`
        // with the Earth→Moon grid.
        let plan = TransferPlan {
            source: SelectionSource::ShortHop { grid: grid.clone() },
            porkchop_grid: Some(grid.clone()),
            selected_porkchop_cell: Some((0, 0)),
            ..TransferPlan::default()
        };
        // Empty supplement: short-hop class doesn't need cross_system_grid
        // (that field routes to `build_cross_star_card` instead).
        let sup = CardSupplement::default();
        let card = build_selected_card(&plan, Some(&sup), test_fleet_info(), |_dv| 0.0);
        assert!(
            card.title.contains("Earth") && card.title.contains("Moon"),
            "title must include origin + dest, got {:?}",
            card.title
        );
        assert!(
            card.title.contains("Porkchop Cell") || card.title.contains("Cross-star"),
            "title must dispatch to the per-class surface, got {:?}",
            card.title
        );
    }

    /// ── Star-approach (Earth → Sol) ──────────────────────────────────
    /// Verifies the dispatcher accepts a 20×5 parking-radius grid
    /// and routes through the per-class surface.  Mirrors the
    /// existing `build_star_approach_grid_sol_parking_0p3au_is_deterministic`
    /// snapshot in `porkchop.rs` at the card surface.
    #[test]
    fn star_approach_card_snapshot_earth_sol() {
        let cols = 20;
        let rows = 5;
        let cells: Vec<PorkchopCell> = (0..rows)
            .flat_map(|row| {
                (0..cols).map(move |col| PorkchopCell {
                    t_dep_s: (col as f64) * 86_400.0 * 18.25, // 18.25-day col step
                    tof_s: 86_400.0 * 30.0 * ((row + 1) as f64),
                    total_dv_ms: 6_000.0 + (row as f64) * 4_000.0 + (col as f64) * 100.0,
                    c3_departure: 0.0,
                    v_inf_arrival_ms: 0.0,
                    delta_v1_ms: 5_500.0,
                    delta_v2_ms: 500.0,
                    feasible: row == 2 && col == 10, // one feasible cell
                    origin_pos_au: DVec3::new(1.0, 0.0, 0.0),
                    dest_pos_au: DVec3::new(0.3, 0.0, 0.0),
                    v_departure_ms: DVec3::ZERO,
                    v_arrival_ms: DVec3::ZERO,
                    transfer_orbit: None,
                })
            })
            .collect();
        let grid = PorkchopGrid {
            origin_name: "Earth".to_string(),
            dest_name: "Sol".to_string(),
            t_dep_bounds_s: (0.0, 86_400.0 * 365.0),
            tof_bounds_s: (86_400.0 * 30.0, 86_400.0 * 30.0 * 5.0),
            rendered_tof_bounds_s: (86_400.0 * 30.0, 86_400.0 * 30.0 * 5.0),
            resolution: (cols, rows),
            cells,
            min_cell: Some((10, 2)),
            metric: PorkchopMetric::TotalDv,
        };
        // Dispatcher first arm reads `plan.porkchop_grid` +
        // `plan.selected_porkchop_cell`; `SelectionSource::StarApproach`
        // alone is not enough (same caveat as the short-hop test above).
        let plan = TransferPlan {
            source: SelectionSource::StarApproach { grid: grid.clone() },
            porkchop_grid: Some(grid.clone()),
            selected_porkchop_cell: Some((10, 2)),
            ..TransferPlan::default()
        };
        let sup = CardSupplement::default();
        let card = build_selected_card(&plan, Some(&sup), test_fleet_info(), |_dv| 0.0);
        assert!(
            card.title.contains("Earth") && card.title.contains("Sol"),
            "title must include origin + dest, got {:?}",
            card.title
        );
    }
}
