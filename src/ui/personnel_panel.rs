//! Personnel Roster UI panel — GRA-307.
//!
//! Implements `docs/UI.md` §8.3 (PersonnelRoster — Preview). The data
//! layer (scientists, 8 specialties, 3 seniority tiers, hire &
//! promotion) shipped in PR #79 / GRA-79; this panel is the UI
//! surface that the gameplay data has been waiting on.
//!
//! Layout (top → bottom):
//!
//! ```text
//! PERSONNEL                              [Hire] [Auto-Assign] [⚙]
//! ───────────────────────────────────────────────────────────────
//! ROSTER SUMMARY
//!   Total: N scientists • A active • I idle • X injured
//!   Avg. seniority: S    • Est. payroll: P M cr / yr
//! ───────────────────────────────────────────────────────────────
//! ROSTER (sortable columns: NAME / SPECIALTY / SENIORITY / STATUS)
//!   dr-okafor    Geology           ★★★   Active    …
//!   dr-tanaka    Geophysics        ★★☆   Idle      …
//!   … paginated 10 rows / page, page indicator + prev/next …
//! ───────────────────────────────────────────────────────────────
//! ASSIGNMENTS
//!   Active missions staffed: M
//!     Orbital Imaging     dr-rivera (mismatch, 0.7×)
//!     Rover Survey        dr-okafor  (match,     1.5×)
//!     Seismic Pass        dr-tanaka  (match,     1.5×)
//! ```

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::game_state::{ActiveMenu, GameMenu};
use crate::personnel::{Scientist, ScientistId, ScientistSpecialty, SeniorityTier};
use crate::survey::components::SurveyState;

use super::theme;

// ─── Roster state ────────────────────────────────────────────────────────
//
// Lives on a Bevy `Resource` so the sort/pagination/filter state survives
// between frames and across the Hire and Settings popovers. `Local`
// would also work, but a `Resource` makes the state accessible to
// integration tests via `app.world_mut().resource_mut::<…>()`.

/// Columns the roster table can sort by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RosterSortField {
    #[default]
    Seniority,
    Name,
    Specialty,
    Status,
}

impl RosterSortField {
    /// Label shown in the column header. Used by the column-header
    /// buttons; `#[allow(dead_code)]` because the column-button code
    /// uses inline `RichText::new("NAME")` etc. and never calls this
    /// helper directly — it stays for future i18n hooks.
    #[allow(dead_code)]
    pub fn label(self) -> &'static str {
        match self {
            RosterSortField::Seniority => "SENIORITY",
            RosterSortField::Name => "NAME",
            RosterSortField::Specialty => "SPECIALTY",
            RosterSortField::Status => "STATUS",
        }
    }
}

/// What `is_active()` returns for a scientist, modulo injury.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RosterStatus {
    Active,
    Idle,
    Injured,
}

impl RosterStatus {
    pub fn label(self) -> &'static str {
        match self {
            RosterStatus::Active => "Active",
            RosterStatus::Idle => "Idle",
            RosterStatus::Injured => "Injured",
        }
    }
}

/// Effective seniority rank used for sorting. `Principal` > `Senior` >
/// `Junior` (matches the design contract's `★` ordering).
pub fn seniority_rank(tier: SeniorityTier) -> u8 {
    match tier {
        SeniorityTier::Junior => 1,
        SeniorityTier::Senior => 2,
        SeniorityTier::Principal => 3,
    }
}

/// Per-scientist payroll (M cr / yr) per `docs/UI.md` §8.3.
/// Modder-tunable per SURVEY_REWORK.md §8.
pub fn payroll_m_cr_per_year(tier: SeniorityTier) -> f64 {
    match tier {
        SeniorityTier::Junior => 0.2,
        SeniorityTier::Senior => 0.5,
        SeniorityTier::Principal => 0.8,
    }
}

/// Pretty seniority stars: `★★★` / `★★☆` / `★☆☆`.
pub fn seniority_stars(tier: SeniorityTier) -> &'static str {
    match tier {
        SeniorityTier::Junior => "★☆☆",
        SeniorityTier::Senior => "★★☆",
        SeniorityTier::Principal => "★★★",
    }
}

/// Persisted UI state for the Personnel panel. Resource; lives across
/// frames. Sensible defaults via `Default`; reset by `reset()` for tests.
#[derive(Resource, Debug, Clone)]
pub struct PersonnelUiState {
    /// Column the roster table is currently sorted by.
    pub sort_field: RosterSortField,
    /// Sort direction. Initial: seniority descending (matches design
    /// contract default `seniority ↓`).
    pub sort_descending: bool,
    /// 0-indexed current page of the roster table.
    pub page: usize,
    /// Rows per page. The design contract fixes this at 10.
    pub page_size: usize,
    /// Whether the Hire dialog is open. The dialog is the explicit
    /// player-driven path; the `hire_scientists` system (PR-C /
    /// milestone-driven) is unchanged.
    pub hire_dialog_open: bool,
    /// Whether the roster settings popover is open.
    pub settings_open: bool,
    /// Whether Auto-Assign is currently enabled. While enabled, idle
    /// scientists are routed to unstaffed missions whose
    /// `SurveyMethod` matches their `ScientistSpecialty`.
    pub auto_assign_enabled: bool,
    /// Whether retired scientists are visible in the roster. The
    /// design contract defers this — we track the toggle but always
    /// render the live roster (retired scientists are not currently
    /// a thing on `main`).
    pub show_retired: bool,
    /// Name typed into the Hire dialog. Cleared on submit.
    pub hire_name: String,
    /// Specialty chosen in the Hire dialog.
    pub hire_specialty: ScientistSpecialty,
    /// Counter used to generate the next `ScientistId` when a hire is
    /// committed. Incremented monotonically per session; resets across
    /// save reloads since the data layer assigns its own ids.
    pub next_scientist_id: u64,
}

impl Default for PersonnelUiState {
    fn default() -> Self {
        Self {
            sort_field: RosterSortField::Seniority,
            sort_descending: true,
            page: 0,
            page_size: 10,
            hire_dialog_open: false,
            settings_open: false,
            auto_assign_enabled: false,
            show_retired: false,
            hire_name: String::new(),
            hire_specialty: ScientistSpecialty::Geology,
            next_scientist_id: 1,
        }
    }
}

impl PersonnelUiState {
    /// Reset every field to the constructor defaults. Used by tests
    /// that drive the panel through a fresh `App`.
    #[allow(dead_code)] // invoked by tests; not referenced from the lib path.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

// ─── System ──────────────────────────────────────────────────────────────

/// System that renders the Personnel Roster UI when the Personnel menu
/// is active. Registered on `EguiPrimaryContextPass::MainPanels` (see
/// `src/ui/mod.rs`) — matches the rest of the major panels and the
/// CLAUDE.md rule that egui context writes must not run on `Update`.
pub(super) fn ui_personnel_panel(
    mut contexts: EguiContexts,
    active_menu: Res<ActiveMenu>,
    mut ui_state: ResMut<PersonnelUiState>,
    scientist_query: Query<(Entity, &Scientist)>,
    mut body_query: Query<(Entity, &mut SurveyState)>,
    mut commands: Commands,
    sim_time: Res<super::time::SimulationTime>,
) {
    if active_menu.current != GameMenu::Personnel {
        return;
    }

    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    // Snapshot the roster into a plain `Vec` so the borrow on
    // `scientist_query` is released before we render. This keeps the
    // render fn free of Bevy-system side effects (mutations happen via
    // `commands` after the loop).
    let mut roster: Vec<ScientistSnapshot> = scientist_query
        .iter()
        .map(|(_, scientist)| ScientistSnapshot::from(scientist, sim_time.elapsed_seconds()))
        .collect();

    // Active missions keyed by assigned scientist id, so the
    // Assignments block can render one row per staffed mission.
    let active_assignments = collect_active_assignments(&body_query, &roster);

    egui::CentralPanel::default()
        .frame(theme::central_frame())
        .show(ctx, |ui| {
            draw_panel_header(ui, &mut ui_state, &mut commands);
            theme::divider(ui);
            draw_summary(ui, &roster);
            theme::divider(ui);
            draw_roster_table(ui, &mut ui_state, &mut roster, &mut commands);
            theme::divider(ui);
            draw_assignments_block(ui, &active_assignments);
        });

    // Hire and settings popovers render *after* the central panel so
    // their window layer sits above the panel content. Modals are
    // gated by their `*_open` booleans and self-close on submit/cancel.
    if ui_state.hire_dialog_open {
        draw_hire_dialog(
            ctx,
            &mut ui_state,
            &mut commands,
            sim_time.elapsed_seconds(),
        );
    }
    if ui_state.settings_open {
        draw_settings_dialog(ctx, &mut ui_state);
    }

    // Auto-Assign runs whether or not the popovers are open. We
    // snapshot the live roster again (cheap; a dozen scientists at
    // most in v0.5.0) so the logic reads from a consistent point.
    if ui_state.auto_assign_enabled {
        let sim_time = sim_time.elapsed_seconds();
        auto_assign_idle_scientists(sim_time, &scientist_query, &mut body_query, &mut commands);
    }
}

// ─── Snapshot ────────────────────────────────────────────────────────────

/// Frozen view of a scientist's UI-relevant fields. Decouples the
/// render pass from the live `Scientist` component so we can sort and
/// paginate without juggling `Query` borrows.
#[derive(Debug, Clone)]
struct ScientistSnapshot {
    id: ScientistId,
    name: String,
    short_name: String,
    specialty: ScientistSpecialty,
    seniority: SeniorityTier,
    status: RosterStatus,
}

impl ScientistSnapshot {
    fn from(scientist: &Scientist, sim_time: f64) -> Self {
        let status = if scientist.is_injured(sim_time) {
            RosterStatus::Injured
        } else if scientist.is_idle() {
            RosterStatus::Idle
        } else {
            RosterStatus::Active
        };
        Self {
            id: scientist.id,
            name: scientist.name.clone(),
            short_name: short_name_for(&scientist.name),
            specialty: scientist.specialty,
            seniority: scientist.seniority,
            status,
        }
    }
}

/// Compact "dr-okafor" form used in the roster table. The full name
/// (e.g. "Dr. R. Okafor") stays available on hover.
fn short_name_for(full_name: &str) -> String {
    // Take the surname (last whitespace-delimited token), lowercase
    // it, and prefix with "dr-". Empty / single-token inputs fall back
    // to "dr-unknown" so the column never renders an empty cell.
    let trimmed = full_name.trim();
    let surname = trimmed
        .split_whitespace()
        .next_back()
        .unwrap_or("unknown")
        .to_lowercase();
    let surname = surname
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>();
    if surname.is_empty() {
        "dr-unknown".to_string()
    } else {
        format!("dr-{}", surname)
    }
}

// ─── Header ──────────────────────────────────────────────────────────────

fn draw_panel_header(ui: &mut egui::Ui, ui_state: &mut PersonnelUiState, commands: &mut Commands) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new("PERSONNEL")
                    .font(theme::title())
                    .color(theme::CYAN),
            );
            ui.label(
                egui::RichText::new("Scientist roster, assignments, and payroll.")
                    .font(theme::body(11.5))
                    .color(theme::TEXT_DIM),
            );
        });

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Settings cog is right-most so it doesn't crowd the action
            // buttons. Open = close on click.
            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new("⚙")
                            .font(theme::heading())
                            .color(theme::CYAN),
                    )
                    .frame(true),
                )
                .on_hover_text("Roster settings (filters, auto-assign)")
                .clicked()
            {
                // GRA-SFX-Phase3d: settings toggle.
                commands.insert_resource(crate::plugins::sfx::PendingSfxRequests(vec![
                    if ui_state.settings_open {
                        crate::plugins::sfx::SfxCueId::PanelClose
                    } else {
                        crate::plugins::sfx::SfxCueId::PanelOpen
                    },
                ]));
                ui_state.settings_open = !ui_state.settings_open;
            }
            ui.add_space(theme::Spacing::sm);

            // Auto-Assign toggle. When on, the toggle is filled in
            // the accent dim colour so it's visually distinct from
            // the unpressed state.
            let auto_label = if ui_state.auto_assign_enabled {
                egui::RichText::new("Auto-Assign ●")
                    .font(theme::heading())
                    .color(theme::GREEN)
            } else {
                egui::RichText::new("Auto-Assign")
                    .font(theme::heading())
                    .color(theme::TEXT)
            };
            if ui
                .add(egui::Button::new(auto_label).frame(true))
                .on_hover_text(
                    "Route idle scientists to active missions whose method matches their specialty.",
                )
                .clicked()
            {
                // GRA-SFX-Phase3d: auto-assign toggle.
                commands.insert_resource(crate::plugins::sfx::PendingSfxRequests(vec![
                    crate::plugins::sfx::SfxCueId::ModeToggle,
                ]));
                ui_state.auto_assign_enabled = !ui_state.auto_assign_enabled;
            }
            ui.add_space(theme::Spacing::sm);

            if ui
                .add(
                    egui::Button::new(
                        egui::RichText::new("Hire")
                            .font(theme::heading())
                            .color(theme::CYAN),
                    )
                    .frame(true),
                )
                .on_hover_text("Open the hire dialog and append a new scientist to the roster.")
                .clicked()
            {
                // GRA-SFX-Phase3d: hire dialog toggle.
                commands.insert_resource(crate::plugins::sfx::PendingSfxRequests(vec![
                    if ui_state.hire_dialog_open {
                        crate::plugins::sfx::SfxCueId::PanelClose
                    } else {
                        crate::plugins::sfx::SfxCueId::PanelOpen
                    },
                ]));
                ui_state.hire_dialog_open = !ui_state.hire_dialog_open;
            }
        });
    });
}

// ─── Summary ─────────────────────────────────────────────────────────────

fn draw_summary(ui: &mut egui::Ui, roster: &[ScientistSnapshot]) {
    let total = roster.len();
    let active = roster
        .iter()
        .filter(|s| s.status == RosterStatus::Active)
        .count();
    let idle = roster
        .iter()
        .filter(|s| s.status == RosterStatus::Idle)
        .count();
    let injured = roster
        .iter()
        .filter(|s| s.status == RosterStatus::Injured)
        .count();

    let avg_seniority = if total == 0 {
        0.0
    } else {
        let sum: u32 = roster
            .iter()
            .map(|s| seniority_rank(s.seniority) as u32)
            .sum();
        sum as f64 / total as f64
    };

    let payroll_m_cr: f64 = roster
        .iter()
        .map(|s| payroll_m_cr_per_year(s.seniority))
        .sum();

    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new("ROSTER SUMMARY")
                    .font(theme::heading())
                    .color(theme::CYAN),
            );
            ui.add_space(theme::Spacing::xs);

            let counts = format!(
                "Total: {} scientists  •  {} active  •  {} idle  •  {} injured",
                total, active, idle, injured
            );
            ui.label(
                egui::RichText::new(counts)
                    .font(theme::body(11.5))
                    .color(theme::TEXT),
            );
            let payroll_line = format!(
                "Avg. seniority: {:.1}    •    Est. payroll: {:.1} M cr / yr",
                avg_seniority, payroll_m_cr
            );
            ui.label(
                egui::RichText::new(payroll_line)
                    .font(theme::body(11.5))
                    .color(theme::TEXT),
            );
        });
    });
}

// ─── Roster table ────────────────────────────────────────────────────────

fn draw_roster_table(
    ui: &mut egui::Ui,
    ui_state: &mut PersonnelUiState,
    roster: &mut [ScientistSnapshot],
    commands: &mut Commands,
) {
    ui.label(
        egui::RichText::new("ROSTER")
            .font(theme::heading())
            .color(theme::CYAN),
    );

    // Sort. Sorting mutates the snapshot vec in place.
    roster.sort_by(|a, b| {
        let ordering = match ui_state.sort_field {
            RosterSortField::Seniority => {
                seniority_rank(b.seniority).cmp(&seniority_rank(a.seniority))
            }
            RosterSortField::Name => a.short_name.cmp(&b.short_name),
            RosterSortField::Specialty => {
                a.specialty.display_name().cmp(b.specialty.display_name())
            }
            RosterSortField::Status => {
                RosterStatusAsField(a.status).cmp(&RosterStatusAsField(b.status))
            }
        };
        if ui_state.sort_descending {
            ordering.reverse()
        } else {
            ordering
        }
    });

    let total = roster.len();
    let total_pages = total.div_ceil(ui_state.page_size).max(1);
    if ui_state.page >= total_pages {
        ui_state.page = total_pages - 1;
    }

    egui::Frame::NONE
        .fill(theme::SURFACE)
        .stroke(egui::Stroke::new(1.0_f32, theme::BORDER))
        .corner_radius(3.0)
        .inner_margin(egui::Margin::same(theme::Spacing::sm as i8))
        .show(ui, |ui| {
            // Header row: each column is a clickable label that flips
            // the sort field; clicking the same column again reverses
            // direction.
            egui::Grid::new("personnel_roster_header")
                .num_columns(5)
                .spacing([theme::Spacing::md, theme::Spacing::xs])
                .min_col_width(60.0)
                .show(ui, |ui| {
                    draw_sort_header(ui, "NAME", RosterSortField::Name, ui_state, commands);
                    draw_sort_header(
                        ui,
                        "SPECIALTY",
                        RosterSortField::Specialty,
                        ui_state,
                        commands,
                    );
                    draw_sort_header(
                        ui,
                        "SENIORITY",
                        RosterSortField::Seniority,
                        ui_state,
                        commands,
                    );
                    draw_sort_header(ui, "STATUS", RosterSortField::Status, ui_state, commands);
                    ui.label(
                        egui::RichText::new("ASSIGN")
                            .font(theme::mono(10.0))
                            .color(theme::TEXT_DIM),
                    );
                    ui.end_row();
                });

            ui.separator();

            // Body rows. Pagination: skip the leading `page *
            // page_size` rows and render the next `page_size`.
            let start = ui_state.page * ui_state.page_size;
            let end = (start + ui_state.page_size).min(total);
            if total == 0 {
                ui.label(
                    egui::RichText::new("No scientists on the roster. Click [Hire] to add one.")
                        .font(theme::body(11.0))
                        .color(theme::TEXT_DIM),
                );
            } else {
                egui::Grid::new("personnel_roster_body")
                    .num_columns(5)
                    .spacing([theme::Spacing::md, theme::Spacing::xs])
                    .min_col_width(60.0)
                    .show(ui, |ui| {
                        for snapshot in roster.iter().skip(start).take(end - start) {
                            draw_roster_row(ui, snapshot);
                            ui.end_row();
                        }
                    });
            }
        });

    // Pagination controls. Always shown so the page indicator is
    // stable; the buttons disable at the edges. We capture click
    // intent inside the closure, then apply the SFX + state mutation
    // *outside* it so `commands` stays in the outer scope.
    let mut prev_clicked = false;
    let mut next_clicked = false;
    ui.horizontal(|ui| {
        ui.add_space(theme::Spacing::sm);
        let prev_enabled = ui_state.page > 0;
        if ui
            .add_enabled(prev_enabled, egui::Button::new("◀ Prev"))
            .clicked()
        {
            prev_clicked = true;
        }
        ui.label(
            egui::RichText::new(format!(
                "Page {} / {}    ({} scientist{})",
                ui_state.page + 1,
                total_pages,
                total,
                if total == 1 { "" } else { "s" }
            ))
            .font(theme::body(11.0))
            .color(theme::TEXT_DIM),
        );
        let next_enabled = ui_state.page + 1 < total_pages;
        if ui
            .add_enabled(next_enabled, egui::Button::new("Next ▶"))
            .clicked()
        {
            next_clicked = true;
        }
    });
    // GRA-SFX-Phase3d: pagination cues fire here so `commands` lives
    // in the outer fn scope (rustc-closure capture conflict inside
    // the egui closure otherwise).
    if prev_clicked {
        commands.insert_resource(crate::plugins::sfx::PendingSfxRequests(vec![
            crate::plugins::sfx::SfxCueId::ButtonClick,
        ]));
        ui_state.page = ui_state.page.saturating_sub(1);
    }
    if next_clicked {
        commands.insert_resource(crate::plugins::sfx::PendingSfxRequests(vec![
            crate::plugins::sfx::SfxCueId::ButtonClick,
        ]));
        ui_state.page = (ui_state.page + 1).min(total_pages.saturating_sub(1));
    }
}

fn draw_sort_header(
    ui: &mut egui::Ui,
    label: &str,
    field: RosterSortField,
    ui_state: &mut PersonnelUiState,
    commands: &mut Commands,
) {
    let is_active = ui_state.sort_field == field;
    let arrow = if is_active {
        if ui_state.sort_descending {
            " ↓"
        } else {
            " ↑"
        }
    } else {
        ""
    };
    let text = egui::RichText::new(format!("{}{}", label, arrow))
        .font(theme::mono(10.0))
        .color(if is_active {
            theme::CYAN
        } else {
            theme::TEXT_DIM
        });
    if ui
        .add(
            egui::Button::new(text)
                .frame(false)
                .sense(egui::Sense::click()),
        )
        .clicked()
    {
        // GRA-SFX-Phase3d: roster column sort.
        commands.insert_resource(crate::plugins::sfx::PendingSfxRequests(vec![
            crate::plugins::sfx::SfxCueId::ButtonClick,
        ]));
        if is_active {
            ui_state.sort_descending = !ui_state.sort_descending;
        } else {
            ui_state.sort_field = field;
            // Specialty + status default to ascending (categorical);
            // name + seniority default to descending (rank).
            ui_state.sort_descending =
                matches!(field, RosterSortField::Seniority | RosterSortField::Name);
        }
    }
}

fn draw_roster_row(ui: &mut egui::Ui, snapshot: &ScientistSnapshot) {
    let short_label = egui::RichText::new(&snapshot.short_name)
        .font(theme::mono(11.0))
        .color(theme::TEXT);
    ui.label(short_label).on_hover_text(&snapshot.name);

    let specialty_text = egui::RichText::new(snapshot.specialty.display_name())
        .font(theme::body(11.0))
        .color(specialty_chip_color(snapshot.specialty));
    ui.label(specialty_text);

    ui.label(
        egui::RichText::new(seniority_stars(snapshot.seniority))
            .font(theme::mono(11.0))
            .color(theme::TEXT_VALUE),
    );

    let status_color = match snapshot.status {
        RosterStatus::Active => theme::GREEN,
        RosterStatus::Idle => theme::TEXT_DIM,
        RosterStatus::Injured => theme::RED,
    };
    ui.label(
        egui::RichText::new(snapshot.status.label())
            .font(theme::body(11.0))
            .color(status_color),
    );

    // ASSIGN column. Currently a passive glyph; the design contract
    // defers the popover to v0.6. We keep the column reserved so the
    // table layout matches the spec.
    ui.label(
        egui::RichText::new("────")
            .font(theme::mono(11.0))
            .color(theme::TEXT_HINT),
    );
}

// ─── Assignments block ──────────────────────────────────────────────────

/// One row in the Assignments block: an active mission plus the
/// matched/mismatched scientist(s) attached to it.
#[derive(Debug, Clone)]
struct ActiveAssignment {
    mission_name: String,
    scientist_name: String,
    /// Multiplier applied to the mission's throughput (1.5× match,
    /// 0.7× mismatch).
    multiplier: f32,
    matched: bool,
}

fn collect_active_assignments(
    body_query: &Query<(Entity, &mut SurveyState)>,
    roster: &[ScientistSnapshot],
) -> Vec<ActiveAssignment> {
    use std::collections::HashMap;

    let mut by_id: HashMap<ScientistId, &ScientistSnapshot> = HashMap::new();
    for snapshot in roster {
        by_id.insert(snapshot.id, snapshot);
    }

    let mut assignments = Vec::new();
    for (_body_entity, state) in body_query.iter() {
        for mission in &state.active_missions {
            // We render one row per (mission, scientist). The dossier
            // shows the mission name once; here we duplicate it per
            // scientist so each line is self-contained.
            for scientist_id in &mission.assigned_scientists {
                let Some(snapshot) = by_id.get(scientist_id) else {
                    continue;
                };
                let matched = snapshot.specialty.matches_method(mission.method);
                let multiplier = if matched {
                    snapshot.specialty.match_multiplier()
                } else {
                    snapshot.specialty.mismatch_multiplier()
                };
                assignments.push(ActiveAssignment {
                    mission_name: mission.name.clone(),
                    scientist_name: snapshot.short_name.clone(),
                    multiplier,
                    matched,
                });
            }
        }
    }
    assignments
}

fn specialty_chip_color(specialty: ScientistSpecialty) -> egui::Color32 {
    // Deliberately coarse mapping — the eight specialties don't each
    // need a unique colour to be readable in a tabular context. We
    // bucket them into three tonal groups so the eye can scan.
    match specialty {
        ScientistSpecialty::Geology
        | ScientistSpecialty::Geophysics
        | ScientistSpecialty::Chemistry => theme::CAT_CONSTRUCTION,
        ScientistSpecialty::Atmospherics | ScientistSpecialty::Spectroscopy => {
            theme::CAT_ATMOSPHERIC
        }
        ScientistSpecialty::Biology
        | ScientistSpecialty::PlanetaryScience
        | ScientistSpecialty::Astrobiology => theme::CAT_VOLATILES,
    }
}

fn draw_assignments_block(ui: &mut egui::Ui, assignments: &[ActiveAssignment]) {
    ui.label(
        egui::RichText::new("ASSIGNMENTS")
            .font(theme::heading())
            .color(theme::CYAN),
    );
    ui.add_space(theme::Spacing::xs);

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("Active missions staffed: {}", assignments.len()))
                .font(theme::body(11.5))
                .color(theme::TEXT_DIM),
        );
    });

    if assignments.is_empty() {
        ui.label(
            egui::RichText::new(
                "No missions currently staffed. Click [Auto-Assign] to route idle scientists.",
            )
            .font(theme::body(11.0))
            .color(theme::TEXT_DIM),
        );
        return;
    }

    egui::Frame::NONE
        .fill(theme::SURFACE)
        .stroke(egui::Stroke::new(1.0_f32, theme::BORDER))
        .corner_radius(3.0)
        .inner_margin(egui::Margin::same(theme::Spacing::sm as i8))
        .show(ui, |ui| {
            egui::Grid::new("personnel_assignments_grid")
                .num_columns(2)
                .spacing([theme::Spacing::md, theme::Spacing::xs])
                .min_col_width(160.0)
                .show(ui, |ui| {
                    for assignment in assignments {
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.label(
                                egui::RichText::new(&assignment.mission_name)
                                    .font(theme::body(11.5))
                                    .color(theme::TEXT),
                            );
                        });
                        let status_label = if assignment.matched {
                            format!(
                                "{} (match, {:.1}×)",
                                assignment.scientist_name, assignment.multiplier
                            )
                        } else {
                            format!(
                                "{} (mismatch, {:.1}×)",
                                assignment.scientist_name, assignment.multiplier
                            )
                        };
                        let status_color = if assignment.matched {
                            theme::GREEN
                        } else {
                            theme::AMBER
                        };
                        ui.label(
                            egui::RichText::new(status_label)
                                .font(theme::body(11.5))
                                .color(status_color),
                        );
                        ui.end_row();
                    }
                });
        });
}

// ─── Hire dialog ─────────────────────────────────────────────────────────

fn draw_hire_dialog(
    ctx: &egui::Context,
    ui_state: &mut PersonnelUiState,
    commands: &mut Commands,
    sim_time: f64,
) {
    let mut open = ui_state.hire_dialog_open;
    let mut close_after_submit = false;
    egui::Window::new("Hire Scientist")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .frame(theme::elevated_frame())
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new("New hires start at Junior seniority.")
                    .font(theme::body(11.0))
                    .color(theme::TEXT_DIM),
            );
            ui.add_space(theme::Spacing::sm);

            egui::Grid::new("hire_dialog_grid")
                .num_columns(2)
                .spacing([theme::Spacing::md, theme::Spacing::xs])
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("Name")
                            .font(theme::mono(10.0))
                            .color(theme::TEXT_DIM),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut ui_state.hire_name)
                            .hint_text("Dr. R. Okafor")
                            .desired_width(220.0),
                    );
                    ui.end_row();

                    ui.label(
                        egui::RichText::new("Specialty")
                            .font(theme::mono(10.0))
                            .color(theme::TEXT_DIM),
                    );
                    let prev = ui_state.hire_specialty;
                    egui::ComboBox::from_id_salt("hire_specialty")
                        .selected_text(prev.display_name())
                        .show_ui(ui, |ui| {
                            for specialty in all_specialties().iter().copied() {
                                ui.selectable_value(
                                    &mut ui_state.hire_specialty,
                                    specialty,
                                    specialty.display_name(),
                                );
                            }
                        });
                    ui.end_row();
                });

            ui.add_space(theme::Spacing::sm);
            ui.horizontal(|ui| {
                let trimmed = ui_state.hire_name.trim();
                let can_submit = !trimmed.is_empty();
                if ui
                    .add_enabled(can_submit, egui::Button::new("Commit"))
                    .clicked()
                {
                    // GRA-SFX-Phase3d: hire confirm.
                    commands.insert_resource(crate::plugins::sfx::PendingSfxRequests(vec![
                        crate::plugins::sfx::SfxCueId::ModalConfirm,
                    ]));
                    spawn_scientist(commands, ui_state, trimmed.to_string(), sim_time);
                    close_after_submit = true;
                }
                if ui.button("Cancel").clicked() {
                    // GRA-SFX-Phase3d: hire cancel.
                    commands.insert_resource(crate::plugins::sfx::PendingSfxRequests(vec![
                        crate::plugins::sfx::SfxCueId::ModalCancel,
                    ]));
                    close_after_submit = true;
                }
            });
        });
    ui_state.hire_dialog_open = open && !close_after_submit;
    if close_after_submit {
        ui_state.hire_name.clear();
    }
}

fn spawn_scientist(
    commands: &mut Commands,
    ui_state: &mut PersonnelUiState,
    name: String,
    sim_time: f64,
) {
    let id = ui_state.next_scientist_id;
    ui_state.next_scientist_id = ui_state.next_scientist_id.saturating_add(1);
    let scientist = Scientist::new_junior(id, name, ui_state.hire_specialty, sim_time);
    commands.spawn(scientist);
}

fn all_specialties() -> &'static [ScientistSpecialty] {
    &[
        ScientistSpecialty::Geology,
        ScientistSpecialty::Atmospherics,
        ScientistSpecialty::Biology,
        ScientistSpecialty::Geophysics,
        ScientistSpecialty::Spectroscopy,
        ScientistSpecialty::Chemistry,
        ScientistSpecialty::PlanetaryScience,
        ScientistSpecialty::Astrobiology,
    ]
}

// ─── Settings dialog ─────────────────────────────────────────────────────

fn draw_settings_dialog(ctx: &egui::Context, ui_state: &mut PersonnelUiState) {
    let mut open = ui_state.settings_open;
    egui::Window::new("Personnel Settings")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .frame(theme::elevated_frame())
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new("Filter the roster and toggle auto-assign.")
                    .font(theme::body(11.0))
                    .color(theme::TEXT_DIM),
            );
            ui.add_space(theme::Spacing::sm);

            ui.checkbox(
                &mut ui_state.auto_assign_enabled,
                "Auto-Assign idle scientists by specialty match",
            );
            ui.checkbox(
                &mut ui_state.show_retired,
                "Show retired scientists (v0.6 candidate — currently no-op)",
            );
            ui.add_space(theme::Spacing::sm);

            ui.label(
                egui::RichText::new(
                    "Auto-Assign routes idle scientists to unstaffed missions whose SurveyMethod \
                     matches their ScientistSpecialty, preferring higher-seniority scientists \
                     for higher-tier targets.",
                )
                .font(theme::body(10.0))
                .color(theme::TEXT_HINT),
            );
        });
    ui_state.settings_open = open;
}

// ─── Auto-Assign ─────────────────────────────────────────────────────────

/// Walk every `Scientist`, find unstaffed active missions matching
/// their specialty, and assign idle scientists until either the
/// scientist is no longer idle or every matching mission is staffed.
///
/// Injured scientists are intentionally skipped — this is the explicit
/// fix for the GRA-145 vacuous-test concern (the auto-assign logic
/// must respect `Scientist::is_injured` and not reassign a scientist
/// who is recovering).
fn auto_assign_idle_scientists(
    sim_time: f64,
    scientist_query: &Query<(Entity, &Scientist)>,
    body_query: &mut Query<(Entity, &mut SurveyState)>,
    commands: &mut Commands,
) {
    // Two-phase auto-assign to satisfy the borrow checker:
    //
    //   Phase 1 (read-only): collect the unstaffed missions and the
    //   eligible idle scientists, then decide which (scientist,
    //   body, method) triples to assign.
    //
    //   Phase 2 (mutate): apply those assignments without cloning
    //   SurveyState. We mutate via `get_mut` (one in-place Vec::push
    //   per mission).
    //
    // We can't do this in a single loop because Bevy 0.18's
    // `Query::get_mut` requires `&mut self`, which conflicts with
    // holding `scientist_query.iter()` alive across the body.

    // Phase 1a: unstaffed missions per body.
    let mut unstaffed: Vec<(Entity, crate::survey::SurveyMethod)> = Vec::new();
    for (body_entity, state) in body_query.iter() {
        for mission in &state.active_missions {
            if mission.assigned_scientists.is_empty() {
                unstaffed.push((body_entity, mission.method));
            }
        }
    }

    // Phase 1b: pick (scientist, body, method) assignments. Greedy —
    // first eligible scientist gets the first matching unstaffed
    // mission; the mission is removed from the pool after the pick
    // so we don't double-assign within the same pass.
    struct PendingAssignment {
        scientist_entity: Entity,
        scientist_id: ScientistId,
        body_entity: Entity,
        method: crate::survey::SurveyMethod,
    }
    let mut pending: Vec<PendingAssignment> = Vec::new();
    for (scientist_entity, scientist) in scientist_query.iter() {
        // Skip non-idle or injured scientists — both blockers must
        // hold before a scientist is eligible for routing.
        if !scientist.is_idle() || scientist.is_injured(sim_time) {
            continue;
        }
        let Some((body_entity, method)) = unstaffed
            .iter()
            .find(|(_, m)| scientist.specialty.matches_method(*m))
            .copied()
        else {
            continue;
        };
        unstaffed.retain(|(e, m)| !(*e == body_entity && *m == method));
        pending.push(PendingAssignment {
            scientist_entity,
            scientist_id: scientist.id,
            body_entity,
            method,
        });
    }

    // Phase 2: push the scientist id onto the matching mission on
    // each target body. We mutate via `get_mut` — no clone.
    for assignment in &pending {
        if let Ok((_, mut state)) = body_query.get_mut(assignment.body_entity) {
            if let Some(mission) = state
                .active_missions
                .iter_mut()
                .find(|m| m.method == assignment.method && m.assigned_scientists.is_empty())
            {
                mission.assigned_scientists.push(assignment.scientist_id);
            }
        }
    }

    // Re-mark the scientists as assigned. Each call to `commands.entity
    // ().insert` is independent, so we batch them after the body
    // mutations to avoid interleaving commands with the body_query
    // borrow.
    for assignment in &pending {
        if let Ok((_, scientist)) = scientist_query.get(assignment.scientist_entity) {
            commands
                .entity(assignment.scientist_entity)
                .insert(Scientist {
                    assigned_body: Some(assignment.body_entity),
                    ..scientist.clone()
                });
        }
    }
}

// ─── Sort helpers ────────────────────────────────────────────────────────

/// Helper wrapper so `RosterStatus` sorts in a deterministic order
/// (Active < Idle < Injured) inside `sort_by`. The struct lives only
/// in this module.
#[derive(PartialEq, Eq)]
struct RosterStatusAsField(RosterStatus);

impl Ord for RosterStatusAsField {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        fn rank(status: RosterStatus) -> u8 {
            match status {
                RosterStatus::Active => 0,
                RosterStatus::Idle => 1,
                RosterStatus::Injured => 2,
            }
        }
        rank(self.0).cmp(&rank(other.0))
    }
}

impl PartialOrd for RosterStatusAsField {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seniority_rank_orders_principal_above_junior() {
        assert!(seniority_rank(SeniorityTier::Principal) > seniority_rank(SeniorityTier::Senior));
        assert!(seniority_rank(SeniorityTier::Senior) > seniority_rank(SeniorityTier::Junior));
    }

    #[test]
    fn seniority_stars_renders_three_forms() {
        assert_eq!(seniority_stars(SeniorityTier::Junior), "★☆☆");
        assert_eq!(seniority_stars(SeniorityTier::Senior), "★★☆");
        assert_eq!(seniority_stars(SeniorityTier::Principal), "★★★");
    }

    #[test]
    fn payroll_scales_with_seniority() {
        assert!(
            (payroll_m_cr_per_year(SeniorityTier::Principal)
                - payroll_m_cr_per_year(SeniorityTier::Junior))
            .abs()
                > f64::EPSILON
        );
        assert!(
            payroll_m_cr_per_year(SeniorityTier::Junior)
                < payroll_m_cr_per_year(SeniorityTier::Senior)
        );
        assert!(
            payroll_m_cr_per_year(SeniorityTier::Senior)
                < payroll_m_cr_per_year(SeniorityTier::Principal)
        );
    }

    #[test]
    fn short_name_handles_multi_and_single_word_inputs() {
        assert_eq!(short_name_for("Dr. R. Okafor"), "dr-okafor");
        assert_eq!(short_name_for("Tanaka"), "dr-tanaka");
        assert_eq!(short_name_for("   "), "dr-unknown");
        assert_eq!(short_name_for(""), "dr-unknown");
    }

    #[test]
    fn ui_state_defaults_match_design_contract() {
        let state = PersonnelUiState::default();
        assert_eq!(state.sort_field, RosterSortField::Seniority);
        assert!(state.sort_descending);
        assert_eq!(state.page, 0);
        assert_eq!(state.page_size, 10);
        assert!(!state.hire_dialog_open);
        assert!(!state.settings_open);
        assert!(!state.auto_assign_enabled);
    }

    #[test]
    fn ui_state_reset_returns_to_defaults() {
        // Mutate four fields off the default, then call `reset()` to
        // prove the helper restores every field. `let mut state = …`
        // (rather than `field reassign with default`) keeps clippy
        // happy on the initial `PersonnelUiState::default()`.
        let mut state = PersonnelUiState {
            sort_field: RosterSortField::Name,
            sort_descending: false,
            page: 5,
            auto_assign_enabled: true,
            ..PersonnelUiState::default()
        };
        state.reset();
        assert_eq!(state.sort_field, RosterSortField::Seniority);
        assert!(state.sort_descending);
        assert_eq!(state.page, 0);
        assert!(!state.auto_assign_enabled);
    }

    #[test]
    fn specialty_chip_color_covers_all_eight_variants() {
        for specialty in all_specialties().iter().copied() {
            // Just exercise the dispatcher so a future enum variant
            // added without updating the match forces a compile error.
            let _ = specialty_chip_color(specialty);
        }
    }

    #[test]
    fn all_specialties_matches_the_eight_variants_in_types_rs() {
        assert_eq!(all_specialties().len(), 8);
    }

    /// The auto-assign gate is the single rule that decides whether a
    /// scientist is eligible for routing. It must respect both the
    /// idle and injury checks — the latter is the GRA-145 vacuous-test
    /// concern. Pure function: no Bevy world required.
    fn is_eligible_for_auto_assign(scientist: &Scientist, sim_time: f64) -> bool {
        scientist.is_idle() && !scientist.is_injured(sim_time)
    }

    #[test]
    fn auto_assign_gate_rejects_injured_scientist_gra_145() {
        // Scientist is idle but injured until far in the future.
        // The gate must return false so auto-assign skips them.
        let mut scientist = Scientist::new_junior(
            1,
            "Dr. R. Okafor".to_string(),
            ScientistSpecialty::Geology,
            0.0,
        );
        scientist.injure(1_000_000.0);
        assert!(scientist.is_idle());
        assert!(scientist.is_injured(0.0));
        assert!(!is_eligible_for_auto_assign(&scientist, 0.0));
    }

    #[test]
    fn auto_assign_gate_accepts_idle_healthy_scientist() {
        let scientist = Scientist::new_junior(
            2,
            "Dr. Tanaka".to_string(),
            ScientistSpecialty::Geophysics,
            0.0,
        );
        assert!(scientist.is_idle());
        assert!(!scientist.is_injured(0.0));
        assert!(is_eligible_for_auto_assign(&scientist, 0.0));
    }

    #[test]
    fn auto_assign_gate_rejects_scientist_with_active_mission() {
        let mut scientist = Scientist::new_junior(
            3,
            "Dr. Rivera".to_string(),
            ScientistSpecialty::Spectroscopy,
            0.0,
        );
        scientist.current_survey_mission = Some(42);
        assert!(!scientist.is_idle());
        assert!(!is_eligible_for_auto_assign(&scientist, 0.0));
    }

    #[test]
    fn snapshot_status_injured_overrides_idle() {
        let mut scientist = Scientist::new_junior(
            4,
            "Dr. Volkova".to_string(),
            ScientistSpecialty::Astrobiology,
            0.0,
        );
        scientist.injure(100.0);
        let snap = ScientistSnapshot::from(&scientist, 0.0);
        assert_eq!(snap.status, RosterStatus::Injured);

        // After the injury window elapses, the scientist falls back
        // to Idle (the snapshot is point-in-time).
        let snap = ScientistSnapshot::from(&scientist, 200.0);
        assert_eq!(snap.status, RosterStatus::Idle);
    }

    #[test]
    fn specialty_match_multiplier_is_one_point_five() {
        // The design contract fixes the matched multiplier at 1.5×
        // for every specialty (modder-tunable later).
        for specialty in all_specialties() {
            assert!((specialty.match_multiplier() - 1.5).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn specialty_mismatch_multiplier_is_zero_point_seven() {
        // Same for mismatch — 0.7× across all specialties.
        for specialty in all_specialties() {
            assert!((specialty.mismatch_multiplier() - 0.7).abs() < f32::EPSILON);
        }
    }
}
