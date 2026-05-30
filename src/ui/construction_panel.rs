use super::dashboard::format_mass_compact;
use super::*;
use crate::game_settings::{ColorBlindMode, GameSettings};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum ConstructionTab {
    Overview,
    Buildings,
    #[default]
    Build,
    Stockpiles,
}

/// UI state for the construction panel (persists across frames)
#[derive(Resource, Debug, Clone)]
pub struct ConstructionUiState {
    /// Build multiplier: how many copies to queue at once
    pub build_multiplier: u32,
    /// Currently selected colony entity (None = auto-select first)
    pub selected_colony: Option<bevy::ecs::entity::Entity>,
    /// Selected top-level tab within the construction menu.
    selected_tab: ConstructionTab,
    /// Selected build-category tab within the Build view.
    selected_build_tab: usize,
}

impl Default for ConstructionUiState {
    fn default() -> Self {
        Self {
            build_multiplier: 1,
            selected_colony: None,
            selected_tab: ConstructionTab::Build,
            selected_build_tab: 0,
        }
    }
}

fn draw_menu_header(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.label(
        egui::RichText::new(title)
            .font(theme::title())
            .color(theme::ACCENT),
    );
    ui.label(
        egui::RichText::new(subtitle)
            .font(theme::body(11.5))
            .color(theme::TEXT_DIM),
    );
    theme::divider(ui);
}

fn draw_status_chip(ui: &mut egui::Ui, label: &str, value: String, color: egui::Color32) {
    ui.vertical(|ui| {
        ui.set_min_width(132.0);
        ui.label(
            egui::RichText::new(label)
                .font(theme::mono(10.0))
                .color(theme::TEXT_DIM),
        );
        ui.label(
            egui::RichText::new(value)
                .font(theme::heading())
                .color(color),
        );
    });
}

fn draw_tab_button(
    ui: &mut egui::Ui,
    label: &str,
    selected: bool,
    enabled: bool,
) -> egui::Response {
    let text = egui::RichText::new(label).size(13.5).color(if selected {
        theme::ACCENT
    } else {
        theme::TEXT
    });
    ui.add_enabled(
        enabled,
        egui::Button::new(text)
            .fill(if selected {
                theme::SURFACE_RAISED
            } else {
                theme::SURFACE
            })
            .stroke(if selected {
                egui::Stroke::new(1.0, theme::ACCENT)
            } else {
                egui::Stroke::new(1.0, theme::BORDER)
            })
            .corner_radius(4.0),
    )
}

pub(super) fn ui_construction_panels(
    mut contexts: EguiContexts,
    active_menu: Res<ActiveMenu>,
    colony_query: Query<(Entity, &Colony, &CelestialBody)>,
    construction_query: Query<(Entity, &ConstructionProject)>,
    mut construction_actions: ResMut<PendingConstructionActions>,
    research_state: Res<crate::research::ResearchState>,
    budget: Res<GlobalBudget>,
    contextual: Res<crate::economy::ContextualStockpile>,
    mut debug_settings: ResMut<ConstructionDebugSettings>,
    mut buildings_data: Option<ResMut<BuildingsData>>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut ui_state: ResMut<ConstructionUiState>,
    mut edit_state: ResMut<crate::colony::BuildingEditState>,
    sim_time: Res<crate::ui::SimulationTime>,
    resource_requests: Res<crate::economy::PendingResourceRequests>,
    mut minimum_stockpiles: Query<&mut crate::economy::MinimumStockpile>,
    settings: Res<GameSettings>,
) {
    if active_menu.current != GameMenu::Construction {
        return;
    }

    // Toggle debug mode with F12
    if keyboard_input.just_pressed(KeyCode::F12) {
        debug_settings.enabled = !debug_settings.enabled;
    }

    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };
    let cb_mode = settings.ui.color_blind_mode;
    let editor_enabled = debug_settings.enabled;

    egui::CentralPanel::default()
        .frame(theme::central_frame())
        .show(ctx, |ui| {
        render_construction_panel(
            ui,
            &colony_query,
            &construction_query,
            &mut construction_actions,
            &research_state,
            &budget,
            &contextual,
            &mut debug_settings,
            buildings_data.as_mut().map(|d| d.as_mut()),
            &mut ui_state,
            &resource_requests,
            &mut minimum_stockpiles,
            cb_mode,
        );
        });

    // Building editor dialog (rendered outside CentralPanel so it floats)
    if editor_enabled {
        render_building_editor(
            ctx,
            buildings_data.as_mut().map(|d| d.as_mut()),
            &mut edit_state,
            sim_time.elapsed_seconds(),
            cb_mode,
        );
    }
}

/// Render the construction panel showing colonies, buildings, and construction queues.
fn render_construction_panel(
    ui: &mut egui::Ui,
    colony_query: &Query<(Entity, &Colony, &CelestialBody)>,
    construction_query: &Query<(Entity, &ConstructionProject)>,
    construction_actions: &mut ResMut<PendingConstructionActions>,
    research_state: &crate::research::ResearchState,
    budget: &GlobalBudget,
    contextual: &crate::economy::ContextualStockpile,
    debug_settings: &mut ConstructionDebugSettings,
    buildings_data: Option<&mut BuildingsData>,
    ui_state: &mut ConstructionUiState,
    resource_requests: &crate::economy::PendingResourceRequests,
    minimum_stockpiles: &mut Query<&mut crate::economy::MinimumStockpile>,
    cb_mode: ColorBlindMode,
) {
    draw_menu_header(
        ui,
        "CONSTRUCTION",
        "Industrial planning, queue control, and colony provisioning.",
    );

    // -- Debug panel --
    if debug_settings.enabled {
        theme::elevated_frame().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("DEBUG MODE")
                        .font(theme::heading())
                        .color(theme::red(cb_mode)),
                );
                ui.label(
                    egui::RichText::new("(Press F12 to toggle)")
                        .italics()
                        .small(),
                );
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.checkbox(
                    &mut debug_settings.free_construction,
                    "Free Construction (no resource costs)",
                );
                ui.checkbox(&mut debug_settings.instant_build, "Instant Build");
                ui.checkbox(
                    &mut debug_settings.bypass_tech_requirements,
                    "Bypass Tech Prerequisites",
                );
            });
            ui.label(
                egui::RichText::new("Debug features are for development only")
                    .small()
                    .italics()
                    .color(theme::AMBER),
            );
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Building Editor: right-click a building in the list to open, or use the button below")
                    .small()
                    .color(theme::TEXT_DIM),
            );
        });
        ui.add_space(10.0);
    }

    let balance = budget.balance_per_year();
    let balance_color = if balance >= 0.0 { theme::green(cb_mode) } else { theme::red(cb_mode) };
    let sign = if balance >= 0.0 { "+" } else { "" };
    ui.horizontal_wrapped(|ui| {
        draw_status_chip(
            ui,
            "TREASURY",
            format_currency(budget.treasury),
            theme::GOLD,
        );
        ui.separator();
        draw_status_chip(
            ui,
            "BALANCE",
            format!("{}{}/yr", sign, format_currency(balance)),
            balance_color,
        );
    });

    theme::divider(ui);

    let colonies: Vec<_> = colony_query.iter().collect();

    if colonies.is_empty() {
        theme::elevated_frame().show(ui, |ui| {
            ui.add_space(12.0);
            ui.label(
                egui::RichText::new("NO COLONIES ONLINE")
                    .font(theme::heading())
                    .color(theme::TEXT_DIM),
            );
            ui.add_space(6.0);
            ui.label("Send a colony ship to a celestial body to establish a colony.");
        });
        return;
    }

    // -- Colony selector --
    let selected_valid = ui_state
        .selected_colony
        .map(|e| colonies.iter().any(|(ce, _, _)| *ce == e))
        .unwrap_or(false);
    if !selected_valid {
        ui_state.selected_colony = colonies.first().map(|(e, _, _)| *e);
    }

    let current_name = ui_state
        .selected_colony
        .and_then(|e| colonies.iter().find(|(ce, _, _)| *ce == e))
        .map(|(_, c, _)| c.name.clone())
        .unwrap_or_default();

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("ACTIVE COLONY")
                .font(theme::heading())
                .color(theme::ACCENT),
        );
        ui.add_space(12.0);
        egui::ComboBox::from_id_salt("colony_selector")
            .selected_text(
                egui::RichText::new(&current_name)
                    .color(theme::TEXT_VALUE)
                    .font(theme::body(13.0)),
            )
            .width((ui.available_width() - 12.0).max(240.0))
            .show_ui(ui, |ui| {
                for (entity, colony, _) in &colonies {
                    let label = format!(
                        "{} ({})",
                        colony.name,
                        Colony::format_population(colony.population)
                    );
                    if ui
                        .selectable_label(ui_state.selected_colony == Some(*entity), &label)
                        .clicked()
                    {
                        ui_state.selected_colony = Some(*entity);
                    }
                }
            });
    });

    theme::divider(ui);

    let selected_entity = match ui_state.selected_colony {
        Some(e) => e,
        None => return,
    };
    let (colony_entity, colony, body) =
        match colonies.iter().find(|(e, _, _)| *e == selected_entity) {
            Some(c) => c,
            None => return,
        };

    let bypass_tech = debug_settings.enabled && debug_settings.bypass_tech_requirements;
    let free_build = debug_settings.enabled && debug_settings.free_construction;
    let buildings_total = colony.total_buildings();
    let factories = colony.building_count(BuildingType::Factory) as f64;
    let bp_rate = 1.0 + factories * 10.0;
    let queue: Vec<_> = construction_query
        .iter()
        .filter(|(_, p)| p.colony_entity == *colony_entity)
        .collect();
    let has_stockpile_editor = minimum_stockpiles.get(*colony_entity).is_ok();

    ui.horizontal_wrapped(|ui| {
        let tabs = [
            (
                ConstructionTab::Overview,
                "📊 Overview".to_string(),
            ),
            (
                ConstructionTab::Buildings,
                format!("🏢 Buildings ({buildings_total})"),
            ),
            (
                ConstructionTab::Build,
                format!("🛠 Build ({bp_rate:.1} BP/yr)"),
            ),
            (
                ConstructionTab::Stockpiles,
                "⚖ Stockpiles".to_string(),
            ),
        ];

        for (tab, label) in tabs {
            let enabled = tab != ConstructionTab::Stockpiles || has_stockpile_editor;
            let response = draw_tab_button(ui, &label, ui_state.selected_tab == tab, enabled);
            if response.clicked() {
                ui_state.selected_tab = tab;
            }
            if tab == ConstructionTab::Stockpiles && !has_stockpile_editor {
                response.on_hover_text("Minimum stockpile controls are not available for this colony.");
            }
        }
    });

    theme::divider(ui);

    if ui_state.selected_tab == ConstructionTab::Stockpiles && !has_stockpile_editor {
        ui_state.selected_tab = ConstructionTab::Overview;
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        match ui_state.selected_tab {
            ConstructionTab::Overview => {
                render_construction_overview_tab(
                    ui,
                    colony,
                    body,
                    bp_rate,
                    &queue,
                    construction_actions,
                    resource_requests,
                    cb_mode,
                );
            }
            ConstructionTab::Buildings => {
                render_construction_buildings_tab(ui, colony, buildings_data, cb_mode);
            }
            ConstructionTab::Build => {
                render_construction_build_tab(
                    ui,
                    colony_entity,
                    research_state,
                    contextual,
                    construction_actions,
                    buildings_data,
                    ui_state,
                    bp_rate,
                    bypass_tech,
                    free_build,
                    cb_mode,
                );
            }
            ConstructionTab::Stockpiles => {
                render_minimum_stockpile_editor(ui, *colony_entity, minimum_stockpiles);
            }
        }
    });
}

fn render_construction_overview_tab(
    ui: &mut egui::Ui,
    colony: &Colony,
    body: &CelestialBody,
    bp_rate: f64,
    queue: &[(Entity, &ConstructionProject)],
    construction_actions: &mut PendingConstructionActions,
    resource_requests: &crate::economy::PendingResourceRequests,
    cb_mode: ColorBlindMode,
) {
    ui.label(
        egui::RichText::new("COLONY OVERVIEW")
            .font(theme::heading())
            .color(theme::ACCENT),
    );
    ui.label(
        egui::RichText::new("Operational summary for the selected colony.")
            .size(11.0)
            .color(theme::TEXT_DIM),
    );
    ui.add_space(8.0);

    theme::elevated_frame().show(ui, |ui| {
        ui.label(
            egui::RichText::new(format!("{} -- {}", colony.name, body.name))
                .size(14.0)
                .strong(),
        );
        ui.separator();

        let workforce_eff = colony.workforce_efficiency();
        let wf_color = if workforce_eff >= 1.0 {
            theme::green(cb_mode)
        } else if workforce_eff >= 0.5 {
            theme::AMBER
        } else {
            theme::red(cb_mode)
        };
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!(
                    "Workforce: {} / {} ({:.0}%)",
                    colony.available_workforce(),
                    colony.total_workforce_demand(),
                    workforce_eff * 100.0
                ))
                .color(wf_color),
            );
            if workforce_eff < 1.0 {
                ui.label(
                    egui::RichText::new("understaffed")
                        .size(11.0)
                        .color(theme::red(cb_mode)),
                );
            }
        });

        let efficiency = colony.logistics_efficiency();
        let eff_color = if efficiency >= 1.0 {
            theme::green(cb_mode)
        } else if efficiency >= 0.5 {
            theme::AMBER
        } else {
            theme::red(cb_mode)
        };
        ui.horizontal(|ui| {
            ui.label("Logistics:");
            ui.label(egui::RichText::new(format!("{:.0}%", efficiency * 100.0)).color(eff_color));
            if efficiency < 1.0 {
                ui.label(
                    egui::RichText::new("(build Mass Drivers / Orbital Lifts)")
                        .size(11.0)
                        .color(theme::TEXT_DIM),
                );
            }
        });

        let housing = colony.housing_capacity();
        let housing_util = if housing > 0.0 {
            (colony.population / housing * 100.0).min(100.0)
        } else {
            0.0
        };
        ui.label(format!(
            "Housing: {} / {} ({:.0}%)",
            Colony::format_population(colony.population),
            Colony::format_population(housing),
            housing_util
        ));

        let growth = colony.population_growth_per_year(1.0);
        if growth.abs() > 0.1 {
            ui.label(format!("Growth: +{}/year", Colony::format_population(growth)));
        }

        let income = colony.wealth_generation_per_year();
        let cost = colony.operating_cost_per_year();
        if income > 0.0 || cost > 0.0 {
            let colony_balance = income - cost;
            let cb_color = if colony_balance >= 0.0 { theme::green(cb_mode) } else { theme::red(cb_mode) };
            let sign = if colony_balance >= 0.0 { "+" } else { "" };
            ui.horizontal(|ui| {
                ui.label(format!("Income: {}/yr", format_currency(income)));
                ui.label(format!("| Cost: {}/yr", format_currency(cost)));
                ui.label(
                    egui::RichText::new(format!("| Net: {}{}/yr", sign, format_currency(colony_balance)))
                        .color(cb_color),
                );
            });
        }

        ui.label(format!("Buildings: {}", colony.total_buildings()));
    });

    ui.add_space(8.0);

    theme::elevated_frame().show(ui, |ui| {
        ui.label(egui::RichText::new("Construction Summary").strong());
        ui.separator();
        ui.label(
            egui::RichText::new(format!("Output: {:.1} BP/year", bp_rate))
                .color(theme::green(cb_mode))
                .strong(),
        );
        ui.label(egui::RichText::new("Base: 1 BP/yr + 10 BP/yr per Factory").size(11.0).color(theme::TEXT_DIM));
        ui.add_space(4.0);
        ui.label(format!("Active queue entries: {}", queue.len()));
        if queue.is_empty() {
            ui.label(
                egui::RichText::new("No active projects. Use the Build tab to queue new construction.")
                    .size(11.0)
                    .color(theme::TEXT_DIM),
            );
        }
    });

    ui.add_space(8.0);
    render_construction_queue_section(
        ui,
        queue,
        bp_rate,
        construction_actions,
        resource_requests,
        true,
        cb_mode,
    );
}

fn render_construction_buildings_tab(
    ui: &mut egui::Ui,
    colony: &Colony,
    buildings_data: Option<&mut BuildingsData>,
    cb_mode: ColorBlindMode,
) {
    ui.label(
        egui::RichText::new("BUILDINGS")
            .font(theme::heading())
            .color(theme::ACCENT),
    );
    if colony.total_buildings() == 0 {
        ui.add_space(12.0);
        ui.label(
            egui::RichText::new("No completed buildings yet.")
                .size(13.0)
                .color(theme::TEXT_DIM),
        );
        return;
    }

    ui.label(
        egui::RichText::new(
            "Grouped by role. Cards show active count, staffing, throughput, and upkeep.",
        )
        .size(11.0)
        .color(theme::TEXT_DIM),
    );
    ui.add_space(6.0);
    render_existing_buildings_section(ui, colony, buildings_data, cb_mode);
}

fn render_construction_queue_section(
    ui: &mut egui::Ui,
    queue: &[(Entity, &ConstructionProject)],
    bp_rate: f64,
    construction_actions: &mut PendingConstructionActions,
    resource_requests: &crate::economy::PendingResourceRequests,
    show_heading: bool,
    cb_mode: ColorBlindMode,
) {
    if show_heading {
        ui.label(
            egui::RichText::new("CONSTRUCTION QUEUE")
                .font(theme::heading())
                .color(theme::ACCENT),
        );
    }
    if queue.is_empty() {
        ui.add_space(12.0);
        ui.label(
            egui::RichText::new("No active construction projects.")
                .size(13.0)
                .color(theme::TEXT_DIM),
        );
        return;
    }

    for (proj_entity, project) in queue {
        let pct = project.progress_percent();
        let remaining_bp = project.building_type.build_cost() * (1.0 - pct as f64);
        let years_remaining = if bp_rate > 0.0 {
            remaining_bp / bp_rate
        } else {
            f64::INFINITY
        };

        ui.horizontal(|ui| {
            ui.label(format!(
                "{} {}",
                project.building_type.icon(),
                project.building_type.display_name()
            ));

            if project.awaiting_resources {
                let req_opt = project
                    .blocking_request_id
                    .and_then(|id| resource_requests.find_by_id(id));

                if let Some(req) = req_opt {
                    use crate::economy::RequestState;
                    let status = match req.state {
                        RequestState::Pending => egui::RichText::new("⏳ Waiting for freighter")
                            .size(11.0)
                            .color(theme::AMBER),
                        RequestState::Assigned | RequestState::InTransit => {
                            let _ = req.eta_seconds;
                            egui::RichText::new("🚀 In transit")
                                .size(11.0)
                                .color(theme::green(cb_mode))
                        }
                        _ => egui::RichText::new("⏳ Awaiting resources")
                            .size(11.0)
                            .color(theme::AMBER),
                    };
                    ui.label(status);
                    ui.label(
                        egui::RichText::new(format!("({:?} {:.1} Mt)", req.resource, req.amount_mt))
                            .size(10.0)
                            .color(theme::TEXT_DIM),
                    );
                } else {
                    ui.label(
                        egui::RichText::new("⏳ Awaiting resources")
                            .size(11.0)
                            .color(theme::AMBER),
                    );
                }
            } else {
                let time_str = if years_remaining < 1.0 {
                    format!("{:.1} mo", years_remaining * 12.0)
                } else {
                    format!("{:.1} yr", years_remaining)
                };
                ui.label(egui::RichText::new(&time_str).size(11.0).color(theme::AMBER));
            }

            if ui
                .small_button("X")
                .on_hover_text("Cancel construction")
                .clicked()
            {
                construction_actions.cancel_construction.push(*proj_entity);
            }
        });

        if project.awaiting_resources {
            let bar = egui::ProgressBar::new(0.0)
                .desired_width(ui.available_width() - 8.0)
                .text("Awaiting delivery");
            ui.add(bar);
        } else {
            let bar = egui::ProgressBar::new(pct)
                .show_percentage()
                .desired_width(ui.available_width() - 8.0);
            ui.add(bar);
        }
        ui.add_space(2.0);
    }
}

fn render_construction_build_tab(
    ui: &mut egui::Ui,
    colony_entity: &Entity,
    research_state: &crate::research::ResearchState,
    contextual: &crate::economy::ContextualStockpile,
    construction_actions: &mut PendingConstructionActions,
    buildings_data: Option<&mut BuildingsData>,
    ui_state: &mut ConstructionUiState,
    bp_rate: f64,
    bypass_tech: bool,
    free_build: bool,
    cb_mode: ColorBlindMode,
) {
    ui.label(
        egui::RichText::new("BUILD")
            .font(theme::heading())
            .color(theme::ACCENT),
    );
    theme::elevated_frame().show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!("Output: {:.1} BP/year", bp_rate))
                    .color(theme::green(cb_mode))
                    .strong(),
            );
            ui.label(egui::RichText::new("i").small())
                .on_hover_text("Base: 1 BP/yr + 10 BP/yr per Factory");
        });
    });

    theme::divider(ui);

    theme::elevated_frame().show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Build qty:").strong());
            for &mult in &[1u32, 5, 10, 25, 50, 100] {
                let selected = ui_state.build_multiplier == mult;
                let btn_text = format!("x{}", mult);
                let btn = egui::Button::new(
                    egui::RichText::new(&btn_text).size(12.0).color(if selected {
                        theme::GOLD
                    } else {
                        egui::Color32::WHITE
                    }),
                )
                .fill(if selected {
                    theme::SURFACE_RAISED
                } else {
                    theme::SURFACE
                })
                .stroke(if selected {
                    egui::Stroke::new(1.0, theme::ACCENT)
                } else {
                    egui::Stroke::new(1.0, theme::BORDER)
                })
                .corner_radius(4.0);
                if ui.add(btn).clicked() {
                    ui_state.build_multiplier = mult;
                }
            }
        });
    });
    ui.add_space(6.0);

    let mut visible_tabs = Vec::new();
    let mut available_by_category = Vec::new();

    for (index, &category) in BuildingCategory::all().iter().enumerate() {
        let available: Vec<_> = category
            .buildings()
            .into_iter()
            .filter(|b| {
                if bypass_tech {
                    return true;
                }
                let tech_req = buildings_data
                    .and_then(|d| d.required_tech(b))
                    .or_else(|| b.required_tech());
                match tech_req {
                    Some(tech_id) => research_state.is_unlocked(tech_id),
                    None => true,
                }
            })
            .collect();

        if !available.is_empty() {
            visible_tabs.push(index);
            available_by_category.push((index, category, available));
        }
    }

    let locked_tab_index = BuildingCategory::all().len();
    let locked: Vec<_> = if bypass_tech {
        Vec::new()
    } else {
        BuildingType::all()
            .iter()
            .filter(|b| {
                let tech_req = buildings_data
                    .and_then(|d| d.required_tech(b))
                    .or_else(|| b.required_tech());
                matches!(tech_req, Some(tech_id) if !research_state.is_unlocked(tech_id))
            })
            .collect()
    };

    if !locked.is_empty() {
        visible_tabs.push(locked_tab_index);
    }

    if !visible_tabs.contains(&ui_state.selected_build_tab) {
        ui_state.selected_build_tab = visible_tabs.first().copied().unwrap_or(0);
    }

    theme::elevated_frame().show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            for (index, category, available) in &available_by_category {
                let label = format!("{} ({})", category.display_name(), available.len());
                if draw_tab_button(ui, &label, ui_state.selected_build_tab == *index, true)
                    .clicked()
                {
                    ui_state.selected_build_tab = *index;
                }
            }

            if !locked.is_empty()
                && draw_tab_button(
                    ui,
                    &format!("Locked ({})", locked.len()),
                    ui_state.selected_build_tab == locked_tab_index,
                    true,
                )
                .clicked()
            {
                ui_state.selected_build_tab = locked_tab_index;
            }
        });
    });

    theme::divider(ui);

    if ui_state.selected_build_tab == locked_tab_index {
        for building in locked {
            let tech_id = buildings_data
                .and_then(|d| d.required_tech(building))
                .or_else(|| building.required_tech());
            if let Some(tech_name) = tech_id {
                ui.label(
                    egui::RichText::new(format!(
                        "{} {} -- requires: {}",
                        building.icon(),
                        building.display_name(),
                        tech_name
                    ))
                    .size(11.0)
                    .color(theme::TEXT_HINT),
                );
            }
        }
        return;
    }

    let Some((_, _, available)) = available_by_category
        .iter()
        .find(|(index, _, _)| *index == ui_state.selected_build_tab)
    else {
        ui.label(
            egui::RichText::new("No unlocked buildings are available in this category.")
                .color(theme::TEXT_DIM),
        );
        return;
    };

    let multiplier = ui_state.build_multiplier;
    let column_count = build_card_columns(ui.available_width());
    let spacing = 16.0;
    let available_width = ui.available_width();
    let card_width = if column_count == 1 {
        available_width.min(BUILDING_CARD_WIDTH)
    } else {
        ((available_width - spacing * (column_count.saturating_sub(1) as f32))
            / column_count as f32)
            .min(BUILDING_CARD_WIDTH)
    };

    let old_spacing = ui.spacing().item_spacing;
    ui.spacing_mut().item_spacing = egui::vec2(spacing, 10.0);

    for chunk in available.chunks(column_count) {
        ui.horizontal_top(|ui| {
            for building in chunk {
                let costs = buildings_data
                    .map(|d| d.resource_costs(building))
                    .unwrap_or(&[]);
                let can_afford = free_build
                    || costs.is_empty()
                    || can_afford_resources_multiplied(contextual, costs, multiplier);

                ui.allocate_ui_with_layout(
                    egui::vec2(card_width, BUILDING_CARD_MIN_HEIGHT),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        render_building_card(
                            ui,
                            *building,
                            multiplier,
                            *colony_entity,
                            costs,
                            contextual,
                            bp_rate,
                            can_afford,
                            construction_actions,
                            card_width,
                            buildings_data.as_mut(),
                            cb_mode,
                        );
                    },
                );
            }

            for _ in chunk.len()..column_count {
                ui.allocate_space(egui::vec2(card_width, BUILDING_CARD_MIN_HEIGHT));
            }
        });
        ui.add_space(10.0);
    }

    ui.spacing_mut().item_spacing = old_spacing;
}

/// Check if resource costs x multiplier can all be covered by the contextual (system-scoped) stockpile.
fn can_afford_resources_multiplied(
    contextual: &crate::economy::ContextualStockpile,
    costs: &[crate::colony::data::ResourceCostEntry],
    multiplier: u32,
) -> bool {
    for (name, amount) in costs {
        let total_needed = amount * multiplier as f64;
        if let Some(rt) = crate::colony::data::parse_resource_type(name) {
            if contextual.get(&rt) < total_needed {
                return false;
            }
        }
    }
    true
}

const EXISTING_BUILDING_CARD_WIDTH: f32 = 280.0;
const EXISTING_BUILDING_CARD_HEIGHT: f32 = 108.0;
const BUILDING_CARD_WIDTH: f32 = 300.0;

fn build_card_columns(available_width: f32) -> usize {
    if available_width >= 1280.0 {
        4
    } else if available_width >= 920.0 {
        3
    } else if available_width >= 620.0 {
        2
    } else {
        1
    }
}

fn render_existing_buildings_section(
    ui: &mut egui::Ui,
    colony: &Colony,
    buildings_data: Option<&mut BuildingsData>,
    cb_mode: ColorBlindMode,
) {
    let buildings_data = buildings_data.clone();
    for &category in BuildingCategory::all() {
        let mut buildings_in_category: Vec<_> = category
            .buildings()
            .into_iter()
            .filter_map(|building| {
                let count = colony.building_count(building);
                (count > 0).then_some((building, count))
            })
            .collect();

        buildings_in_category.sort_by(|(left_building, left_count), (right_building, right_count)| {
            right_count
                .cmp(left_count)
                .then_with(|| left_building.display_name().cmp(right_building.display_name()))
        });

        if buildings_in_category.is_empty() {
            continue;
        }

        let total_instances: u64 = buildings_in_category
            .iter()
            .map(|(_, count)| *count as u64)
            .sum();
        let total_workers: u64 = buildings_in_category
            .iter()
            .map(|(building, count)| building.workforce_required() as u64 * *count as u64)
            .sum();

        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(category.display_name())
                        .size(12.5)
                        .strong()
                        .color(building_category_color(category)),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} workers",
                            format_grouped_u64(total_workers)
                        ))
                        .size(10.5)
                        .color(theme::TEXT_HINT),
                    );
                    ui.label(
                        egui::RichText::new(format!(
                            "{} buildings",
                            format_grouped_u64(total_instances)
                        ))
                        .size(10.5)
                        .color(theme::TEXT_DIM),
                    );
                });
            });

            ui.label(
                egui::RichText::new(format!(
                    "{} active types",
                    format_grouped_u64(buildings_in_category.len() as u64)
                ))
                .size(10.0)
                .color(theme::TEXT_HINT),
            );
            ui.add_space(4.0);

            let card_spacing = 12.0;
            let available_width = ui.available_width().max(220.0);
            let card_width = available_width.min(EXISTING_BUILDING_CARD_WIDTH);
            let fit_columns = ((available_width + card_spacing) / (card_width + card_spacing))
                .floor() as usize;
            let column_count = build_card_columns(available_width)
                .min(4)
                .min(fit_columns.max(1));

            for chunk in buildings_in_category.chunks(column_count) {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(card_spacing, card_spacing);

                    for (building, count) in chunk {
                        render_existing_building_card(
                            ui,
                            *building,
                            *count,
                            card_width,
                            buildings_data,
                            cb_mode,
                        );
                    }

                    for _ in chunk.len()..column_count {
                        ui.allocate_space(egui::vec2(card_width, EXISTING_BUILDING_CARD_HEIGHT));
                    }
                });
                ui.add_space(card_spacing);
            }
        });

        ui.add_space(6.0);
    }
}

fn render_existing_building_card(
    ui: &mut egui::Ui,
    building: BuildingType,
    count: u32,
    card_width: f32,
    buildings_data: Option<&mut BuildingsData>,
    cb_mode: ColorBlindMode,
) {
    let definition = buildings_data.and_then(|data| data.get(&building));
    let display_name = definition
        .map(|def| def.display_name.as_str())
        .unwrap_or(building.display_name());
    let description = definition
        .map(|def| def.description.as_str())
        .unwrap_or(building.description());
    let icon = definition
        .map(|def| def.icon.as_str())
        .unwrap_or(building.icon());
    let workers = building.workforce_required() as u64 * count as u64;

    let operational_entries = operational_stat_entries(building, count, definition, cb_mode);
    let upkeep_entries = maintenance_entries(count, definition);
    let operations_summary = summarize_card_entries(&operational_entries, 1);
    let upkeep_summary = summarize_card_entries(&upkeep_entries, 2);
    let target_width = card_width.clamp(220.0, EXISTING_BUILDING_CARD_WIDTH);
    let response = ui
        .allocate_ui_with_layout(
            egui::vec2(target_width, EXISTING_BUILDING_CARD_HEIGHT),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.group(|ui| {
                    ui.set_min_width(target_width);
                    ui.set_max_width(target_width);
                    ui.set_min_height(EXISTING_BUILDING_CARD_HEIGHT);

                    ui.horizontal_top(|ui| {
                        ui.label(egui::RichText::new(icon).size(18.0));
                        let count_width = 56.0;
                        let text_width = (target_width - 18.0 - count_width - 34.0).max(96.0);
                        ui.allocate_ui_with_layout(
                            egui::vec2(text_width, 26.0),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                super::fleets_panel::render_marquee_line(
                                    ui,
                                    display_name,
                                    theme::ACCENT,
                                    egui::FontId::proportional(11.0),
                                );
                                super::fleets_panel::render_marquee_line(
                                    ui,
                                    description,
                                    theme::TEXT_DIM,
                                    egui::FontId::proportional(8.75),
                                );
                            },
                        );
                        ui.add_space(2.0);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                            ui.add_sized(
                                [count_width, 18.0],
                                egui::Label::new(
                                    egui::RichText::new(format!("×{}", format_grouped_u64(count as u64)))
                                        .size(11.0)
                                        .strong()
                                        .color(theme::GOLD),
                                ),
                            );
                        });
                    });

                    ui.add_space(2.0);
                    render_existing_building_summary_row(
                        ui,
                        "Staff",
                        &format_compact_u64(workers),
                        theme::TEXT_VALUE,
                    );

                    if let Some((summary, color)) = operations_summary.as_ref() {
                        render_existing_building_summary_row(ui, "Ops", summary, *color);
                    }

                    if let Some((summary, color)) = upkeep_summary.as_ref() {
                        render_existing_building_summary_row(ui, "Upkeep", summary, *color);
                    } else {
                        render_existing_building_summary_row(ui, "Upkeep", "None", theme::TEXT_HINT);
                    }
                })
                .response
            },
        )
        .inner;

    response.on_hover_ui(|ui| {
        ui.label(egui::RichText::new(display_name).strong());
        ui.label(
            egui::RichText::new(format!(
                "Count: {}   Workforce: {}",
                format_grouped_u64(count as u64),
                format_grouped_u64(workers)
            ))
            .size(10.5)
            .color(theme::TEXT_DIM),
        );

        if !operational_entries.is_empty() {
            ui.separator();
            ui.label(egui::RichText::new("Operations").strong().size(10.5));
            for (text, color) in &operational_entries {
                ui.label(egui::RichText::new(text).size(10.0).color(*color));
            }
        }

        if !upkeep_entries.is_empty() {
            ui.separator();
            ui.label(egui::RichText::new("Upkeep / yr").strong().size(10.5));
            for (text, color) in &upkeep_entries {
                ui.label(egui::RichText::new(text).size(10.0).color(*color));
            }
        }
    });
}

fn render_existing_building_summary_row(
    ui: &mut egui::Ui,
    label: &str,
    value: &str,
    value_color: egui::Color32,
) {
    ui.horizontal(|ui| {
        ui.add_sized(
            [44.0, 14.0],
            egui::Label::new(
                egui::RichText::new(label)
                    .size(9.25)
                    .color(theme::TEXT_HINT),
            ),
        );
        ui.add_space(4.0);
        let value_width = ui.available_width().max(0.0);
        ui.allocate_ui_with_layout(
            egui::vec2(value_width, 14.0),
            egui::Layout::left_to_right(egui::Align::Min),
            |ui| {
                super::fleets_panel::render_marquee_line(
                    ui,
                    value,
                    value_color,
                    egui::FontId::proportional(9.25),
                );
            },
        );
    });
}

fn summarize_card_entries(
    entries: &[(String, egui::Color32)],
    max_entries: usize,
) -> Option<(String, egui::Color32)> {
    if entries.is_empty() {
        return None;
    }

    let max_entries = max_entries.max(1);
    let mut summary = entries
        .iter()
        .take(max_entries)
        .map(|(text, _)| abbreviate_card_summary(text))
        .collect::<Vec<_>>()
        .join(" | ");
    if entries.len() > max_entries {
        summary.push_str(&format!(" +{}", entries.len() - max_entries));
    }

    Some((summary, entries[0].1))
}

fn abbreviate_card_summary(text: &str) -> String {
    text.replace(" mining output", " mining")
        .replace(" deep mining output", " deep mining")
        .replace(" bulk mining output", " bulk mining")
        .replace(" atmospheric harvest", " atmo")
        .replace(" chemical processing/yr", " chem/yr")
        .replace(" power output", " power")
        .replace(" research speed", " research")
        .replace(" engineering speed", " engineering")
        .replace(" population growth", " pop")
        .replace(" construction costs", " build cost")
        .replace(" stockpile capacity", " capacity")
}

fn building_category_color(category: BuildingCategory) -> egui::Color32 {
    match category {
        BuildingCategory::Infrastructure => theme::ACCENT,
        BuildingCategory::Industry => theme::CAT_CONSTRUCTION,
        BuildingCategory::Logistics => theme::AMBER,
        BuildingCategory::Power => theme::GOLD,
        BuildingCategory::Population => theme::GREEN,
        BuildingCategory::Research => theme::RP_BLUE,
        BuildingCategory::Financial => theme::EP_TEAL,
        BuildingCategory::Military => theme::RED,
    }
}

fn format_grouped_u64(value: u64) -> String {
    let digits = value.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);

    for (index, ch) in digits.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }

    grouped.chars().rev().collect()
}

fn format_compact_u64(value: u64) -> String {
    if value >= 1_000_000_000 {
        format!("{:.1}B", value as f64 / 1_000_000_000.0)
    } else if value >= 1_000_000 {
        format!("{:.1}M", value as f64 / 1_000_000.0)
    } else if value >= 1_000 {
        format!("{:.1}k", value as f64 / 1_000.0)
    } else {
        value.to_string()
    }
}

/// Minimum card height for construction building cards.
/// Tall enough for: icon+name (1 row), description (1 line), separator,
/// stats (1 row), build-time (1 row), effects (up to 2 lines), separator,
/// cost rows in 2-column pairs (up to 3 pairs), Queue button.
const BUILDING_CARD_MIN_HEIGHT: f32 = 168.0;

fn format_card_scalar(value: f64) -> String {
    if (value.fract()).abs() < 0.01 {
        format!("{value:.0}")
    } else {
        format!("{value:.2}").trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

fn operational_stat_entries(
    building: BuildingType,
    multiplier: u32,
    definition: Option<&crate::colony::BuildingDefinition>,
    cb_mode: ColorBlindMode,
) -> Vec<(String, egui::Color32)> {
    let multiplier = multiplier as f64;
    let mut entries = Vec::new();

    if matches!(building, BuildingType::Factory) {
        entries.push((
            format!("+{} BP/yr construction speed", format_card_scalar(10.0 * multiplier)),
            theme::green(cb_mode),
        ));
    }

    if let Some(definition) = definition {
        for modifier in &definition.modifiers {
            let total = modifier.value * multiplier;
            let entry = match modifier.modifier_type.as_str() {
                "MiningEfficiency" => Some((
                    format!("+{}% mining output", format_card_scalar(total)),
                    theme::green(cb_mode),
                )),
                "DeepMiningEfficiency" => Some((
                    format!("+{}% deep mining output", format_card_scalar(total)),
                    theme::green(cb_mode),
                )),
                "BulkMiningEfficiency" => Some((
                    format!("+{}% bulk mining output", format_card_scalar(total)),
                    theme::green(cb_mode),
                )),
                "AtmosphericHarvesting" => Some((
                    format!("+{}/yr atmospheric harvest", format_mass_compact(total)),
                    theme::green(cb_mode),
                )),
                "ChemicalProcessing" => Some((
                    format!("+{} chemical processing/yr", format_card_scalar(total)),
                    theme::green(cb_mode),
                )),
                "PowerGeneration" => Some((
                    format!("+{} GW power output", format_card_scalar(total)),
                    theme::green(cb_mode),
                )),
                "ResearchSpeed" => Some((
                    format!("+{}% research speed", format_card_scalar(total)),
                    theme::green(cb_mode),
                )),
                "EngineeringSpeed" => Some((
                    format!("+{}% engineering speed", format_card_scalar(total)),
                    theme::green(cb_mode),
                )),
                "PopulationGrowth" => Some((
                    format!("+{}% population growth", format_card_scalar(total)),
                    theme::green(cb_mode),
                )),
                "ConstructionCost" => {
                    let color = if total <= 0.0 { theme::green(cb_mode) } else { theme::red(cb_mode) };
                    Some((
                        format!("{:+}% construction costs", total.round()),
                        color,
                    ))
                }
                "StorageCapacity" => Some((
                    format!("+{}% stockpile capacity", format_card_scalar(total * 100.0)),
                    theme::green(cb_mode),
                )),
                _ => None,
            };

            if let Some(entry) = entry {
                entries.push(entry);
            }
        }
    }

    if entries.is_empty() {
        entries.extend(
            building
                .effects_summary()
                .iter()
                .map(|line| ((*line).to_string(), theme::green(cb_mode))),
        );
    }

    entries
}

fn maintenance_entries(
    multiplier: u32,
    definition: Option<&crate::colony::BuildingDefinition>,
) -> Vec<(String, egui::Color32)> {
    definition
        .map(|definition| {
            definition
                .maintenance_resources
                .iter()
                .map(|(resource, amount)| {
                    let total = amount * multiplier as f64;
                    let icon = crate::colony::data::parse_resource_type(resource)
                        .as_ref()
                        .map(super::resources_bar::get_resource_icon)
                        .unwrap_or("?");
                    (
                        format!("{} {} {}/yr", icon, resource, format_mass_compact(total)),
                        theme::AMBER,
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

fn summarize_construction_card_entries(
    entries: &[(String, egui::Color32)],
    max_entries: usize,
) -> Option<(String, egui::Color32)> {
    if entries.is_empty() {
        return None;
    }

    let mut summary = entries
        .iter()
        .take(max_entries.max(1))
        .map(|(text, _)| abbreviate_card_summary(text))
        .collect::<Vec<_>>()
        .join(" | ");
    if entries.len() > max_entries.max(1) {
        summary.push_str(&format!(" +{}", entries.len() - max_entries.max(1)));
    }

    Some((summary, entries[0].1))
}

fn render_building_card(
    ui: &mut egui::Ui,
    building: BuildingType,
    multiplier: u32,
    colony_entity: bevy::ecs::entity::Entity,
    costs: &[crate::colony::data::ResourceCostEntry],
    contextual: &crate::economy::ContextualStockpile,
    bp_rate: f64,
    can_afford: bool,
    construction_actions: &mut PendingConstructionActions,
    card_width: f32,
    buildings_data: Option<&mut BuildingsData>,
    cb_mode: ColorBlindMode,
) {
    let total_bp = building.build_cost() * multiplier as f64;
    let years_to_build = if bp_rate > 0.0 { total_bp / bp_rate } else { f64::INFINITY };
    let definition = buildings_data.and_then(|data| data.get(&building));
    let operational_entries = operational_stat_entries(building, multiplier, definition, cb_mode);
    let maintenance_entries = maintenance_entries(multiplier, definition);
    let operations_summary = summarize_construction_card_entries(&operational_entries, 1);
    let upkeep_summary = summarize_construction_card_entries(&maintenance_entries, 2);

    // Power demand from data file
    let power_demand_mw = definition.map(|def| def.power_demand_mw).unwrap_or(0.0) * multiplier as f64;

    ui.group(|ui| {
        let target_width = card_width;
        ui.set_min_width(target_width);
        ui.set_max_width(target_width);
        ui.set_min_height(BUILDING_CARD_MIN_HEIGHT);

        // ── Header: icon + name + description (max 2 lines) ──────────────
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(building.icon()).size(22.0));
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(building.display_name()).strong().size(12.0));
                // Cap description to 2 lines so all cards stay uniform height.
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(building.description())
                            .size(9.0)
                            .color(theme::TEXT_DIM),
                    )
                    .truncate(),
                );
            });
        });

        ui.separator();

        // ── Build stats ───────────────────────────────────────────────────
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!("{:.0} BP", total_bp)).size(10.0));
            ui.label(egui::RichText::new("|").size(9.0).color(theme::TEXT_DIM));
            ui.label(
                egui::RichText::new(format!("👷 {}", building.workforce_required() * multiplier))
                    .size(10.0),
            );
            if power_demand_mw > 0.0 {
                ui.label(egui::RichText::new("|").size(9.0).color(theme::TEXT_DIM));
                let power_str = if power_demand_mw >= 1000.0 {
                    format!("⚡ {:.1} GW", power_demand_mw / 1000.0)
                } else {
                    format!("⚡ {:.0} MW", power_demand_mw)
                };
                ui.label(egui::RichText::new(power_str).size(10.0).color(theme::TEXT_DIM));
            }
        });

        // Build time
        let time_str = if years_to_build.is_infinite() {
            "∞ (no factory)".to_string()
        } else if years_to_build < 1.0 {
            format!("⏱ {:.1} mo", years_to_build * 12.0)
        } else {
            format!("⏱ {:.1} yr", years_to_build)
        };
        ui.label(egui::RichText::new(&time_str).size(10.0).color(theme::AMBER));

        // ── Effect lines ─────────────────────────────────────────────────
        if let Some((summary, color)) = operations_summary {
            ui.separator();
            ui.add(
                egui::Label::new(egui::RichText::new(summary).size(9.5).color(color)).truncate(),
            );
        }

        if let Some((summary, color)) = upkeep_summary {
            ui.add(
                egui::Label::new(egui::RichText::new(summary).size(9.25).color(color)).truncate(),
            );
        }

        // ── Resource costs ────────────────────────────────────────────────
        ui.separator();
        if costs.is_empty() {
            ui.label(
                egui::RichText::new("No materials required")
                    .size(9.25)
                    .color(theme::TEXT_DIM),
            );
        } else {
            let cost_entries: Vec<(String, egui::Color32)> = costs
                .iter()
                .map(|(r, a)| {
                    let total_needed = a * multiplier as f64;
                    let rt_opt = crate::colony::data::parse_resource_type(r);
                    let available = rt_opt.map(|rt| contextual.get(&rt)).unwrap_or(0.0);
                    let ok = available >= total_needed;
                    let color = if ok { theme::green(cb_mode) } else { theme::red(cb_mode) };
                    let icon = rt_opt
                        .as_ref()
                        .map(|rt| super::resources_bar::get_resource_icon(rt))
                        .unwrap_or("?");
                    let need_str = format_mass_compact(total_needed);
                    let avail_str = format_mass_compact(available);
                    (format!("{} {} {}/{}", icon, r, need_str, avail_str), color)
                })
                .collect();

            let summary = cost_entries
                .iter()
                .take(2)
                .map(|(text, _)| text.as_str())
                .collect::<Vec<_>>()
                .join(" | ");
            let summary_color = if cost_entries.iter().take(2).all(|(_, color)| *color == theme::green(cb_mode)) {
                theme::green(cb_mode)
            } else {
                theme::red(cb_mode)
            };
            ui.add(
                egui::Label::new(
                    egui::RichText::new(summary).size(9.25).color(summary_color),
                )
                .truncate(),
            );
        }

        ui.add_space(4.0);

        // ── Queue button ───────────────────────────────────────────────────
        ui.horizontal_centered(|ui| {
            let btn_text = if multiplier == 1 {
                "Queue".to_string()
            } else {
                format!("Queue ×{}", multiplier)
            };
            let response = ui.add_enabled(
                can_afford,
                egui::Button::new(egui::RichText::new(&btn_text).size(11.0))
                    .min_size(egui::vec2(140.0, 20.0)),
            );
            if !can_afford {
                response.on_hover_text("Insufficient resources in this system");
            } else if response.clicked() {
                for _ in 0..multiplier {
                    construction_actions
                        .start_construction
                        .push((colony_entity, building));
                }
            }
        });
    });
}


/// Render the minimum stockpile configuration section for a colony.
///
/// Shows a table of all resource types. The player
/// can set (or clear) a per-resource minimum threshold; when the local stockpile
/// falls below it a Maintenance-priority freighter request is generated.
fn render_minimum_stockpile_editor(
    ui: &mut egui::Ui,
    colony_entity: bevy::ecs::entity::Entity,
    minimum_stockpiles: &mut Query<&mut crate::economy::MinimumStockpile>,
) {
    use crate::economy::types::ResourceType;

    let Ok(mut minimum) = minimum_stockpiles.get_mut(colony_entity) else {
        return;
    };

    ui.label(
        egui::RichText::new("MINIMUM STOCKPILES")
            .font(theme::heading())
            .color(theme::ACCENT),
    );
    ui.label(
        egui::RichText::new(
            "Set a minimum amount for each resource. Freighters will automatically resupply when stocks fall below the threshold.",
        )
        .size(11.0)
        .color(theme::TEXT_DIM),
    );
    ui.add_space(4.0);

    let categories = ResourceType::by_category();
    let column_count = if ui.available_width() >= 1080.0 {
        3
    } else if ui.available_width() >= 720.0 {
        2
    } else {
        1
    };

    let mut columns: Vec<Vec<(&'static str, Vec<ResourceType>)>> =
        (0..column_count).map(|_| Vec::new()).collect();
    let mut column_loads = vec![0usize; column_count];

    for category in categories {
        let next_column = column_loads
            .iter()
            .enumerate()
            .min_by_key(|(_, load)| **load)
            .map(|(index, _)| index)
            .unwrap_or(0);
        column_loads[next_column] += category.1.len() + 1;
        columns[next_column].push(category);
    }

    ui.columns(column_count, |ui_columns| {
        for (column_ui, groups) in ui_columns.iter_mut().zip(columns) {
            for (category_name, resources) in groups {
                render_minimum_stockpile_group(
                    column_ui,
                    category_name,
                    &resources,
                    &mut minimum,
                );
                column_ui.add_space(8.0);
            }
        }
    });
}

fn render_minimum_stockpile_group(
    ui: &mut egui::Ui,
    category_name: &str,
    resources: &[crate::economy::types::ResourceType],
    minimum: &mut crate::economy::MinimumStockpile,
) {
    theme::elevated_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(
            egui::RichText::new(category_name)
                .strong()
                .size(11.5)
                .color(theme::category_color(category_name)),
        );
        ui.add_space(4.0);

        egui::Grid::new(format!("min_stockpile_grid_{category_name}"))
            .num_columns(3)
            .striped(true)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                ui.label(egui::RichText::new("Resource").strong().size(10.5));
                ui.label(egui::RichText::new("Min (Mt)").strong().size(10.5));
                ui.label(egui::RichText::new("").size(10.5));
                ui.end_row();

                for &resource in resources {
                    let current = minimum.get(&resource);
                    let has_threshold = current > 0.0;

                    ui.label(
                        egui::RichText::new(format!(
                            "{} {}",
                            super::resources_bar::get_resource_icon(&resource),
                            resource.display_name()
                        ))
                        .size(11.0),
                    );

                    let mut value_text = if has_threshold {
                        format!("{current:.1}")
                    } else {
                        String::new()
                    };

                    let text_edit = egui::TextEdit::singleline(&mut value_text)
                        .desired_width(72.0)
                        .hint_text("—");
                    if ui.add(text_edit).changed() {
                        let trimmed = value_text.trim();
                        if trimmed.is_empty() {
                            minimum.clear(&resource);
                        } else if let Ok(value) = trimmed.parse::<f64>() {
                            if value > 0.0 {
                                minimum.set(resource, value);
                            } else {
                                minimum.clear(&resource);
                            }
                        }
                    }

                    if has_threshold {
                        if ui
                            .small_button("✕")
                            .on_hover_text("Clear minimum threshold")
                            .clicked()
                        {
                            minimum.clear(&resource);
                        }
                    } else {
                        ui.label("");
                    }
                    ui.end_row();
                }
            });
    });
}

// ══════════════════════════════════════════════════════════════════════════════
// Building Editor (F12 debug mode)
// ══════════════════════════════════════════════════════════════════════════════

/// Render the floating building editor window.
///
/// Shows a left-hand list of all building types.  Clicking one opens an edit
/// form on the right that lets the developer tweak every field of the
/// `BuildingDefinition`.  Saving writes back to in-memory `BuildingsData` and
/// serialises the result to `assets/data/buildings.ron`.
pub(super) fn render_building_editor(
    ctx: &egui::Context,
    buildings_data: Option<&mut BuildingsData>,
    edit_state: &mut crate::colony::BuildingEditState,
    elapsed: f64,
    cb_mode: ColorBlindMode,
) {
    let Some(data) = buildings_data else {
        return;
    };

    let mut open = true;
    egui::Window::new("🏗 Building Editor")
        .id(egui::Id::new("building_editor_window"))
        .open(&mut open)
        .collapsible(true)
        .resizable(true)
        .default_size([800.0, 600.0])
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            // Status message
            if let Some((ref msg, ts)) = edit_state.status_message.clone() {
                let age = elapsed - ts;
                if age < 4.0 {
                    let alpha = ((4.0 - age) / 2.0).min(1.0) as f32;
                    let color = if msg.starts_with("Error") {
                        theme::red(cb_mode)
                    } else {
                        theme::green(cb_mode)
                    };
                    ui.colored_label(color.linear_multiply(alpha), msg.as_str());
                } else {
                    edit_state.status_message = None;
                }
                ui.separator();
            }

            ui.columns(2, |cols| {
                // ── Left column: building type list ──
                let left = &mut cols[0];
                left.label(egui::RichText::new("Buildings").strong());
                left.separator();
                egui::ScrollArea::vertical()
                    .id_salt("bld_editor_list")
                    .max_height(550.0)
                    .show(left, |ui| {
                        for cat in BuildingCategory::all() {
                            ui.label(
                                egui::RichText::new(cat.display_name())
                                    .size(11.0)
                                    .strong()
                                    .color(theme::TEXT_DIM),
                            );
                            for btype in cat.buildings() {
                                let is_selected =
                                    edit_state.selected_type == Some(btype);
                                let def_name = data
                                    .get(&btype)
                                    .map(|d| d.display_name.as_str())
                                    .unwrap_or(btype.display_name());
                                let row_text = format!(
                                    "{} {}",
                                    data.get(&btype)
                                        .map(|d| d.icon.as_str())
                                        .unwrap_or(""),
                                    def_name
                                );
                                let response = ui.selectable_label(is_selected, &row_text);
                                if response.clicked() {
                                    edit_state.selected_type = Some(btype);
                                    // Open edit form
                                    if let Some(def) = data.get(&btype) {
                                        edit_state.editing = Some(
                                            crate::colony::BuildingEditData::from_def(btype, def),
                                        );
                                    }
                                }
                            }
                            ui.add_space(4.0);
                        }
                    });

                // ── Right column: edit form ──
                let right = &mut cols[1];
                if let Some(ref mut ed) = edit_state.editing {
                    right.label(
                        egui::RichText::new(format!("Edit: {}", ed.display_name))
                            .strong(),
                    );
                    right.separator();

                    let mut save_clicked = false;

                    egui::ScrollArea::vertical()
                        .id_salt("bld_editor_form")
                        .max_height(510.0)
                        .show(right, |ui| {
                            egui::Grid::new("bld_edit_grid")
                                .num_columns(2)
                                .spacing([8.0, 5.0])
                                .striped(true)
                                .show(ui, |ui| {
                                    // ID (read-only)
                                    ui.label("ID:");
                                    ui.label(
                                        egui::RichText::new(
                                            format!("{:?}", ed.building_type),
                                        )
                                        .monospace()
                                        .color(theme::TEXT_DIM),
                                    );
                                    ui.end_row();

                                    // Display name
                                    ui.label("Display Name:");
                                    ui.text_edit_singleline(&mut ed.display_name);
                                    ui.end_row();

                                    // Icon
                                    ui.label("Icon:");
                                    ui.text_edit_singleline(&mut ed.icon);
                                    ui.end_row();

                                    // Category
                                    ui.label("Category:");
                                    egui::ComboBox::from_id_salt("bld_cat_combo")
                                        .selected_text(
                                            BuildingCategory::all()
                                                .get(ed.category_index)
                                                .map(|c| c.display_name())
                                                .unwrap_or("?"),
                                        )
                                        .show_ui(ui, |ui| {
                                            for (i, cat) in
                                                BuildingCategory::all().iter().enumerate()
                                            {
                                                ui.selectable_value(
                                                    &mut ed.category_index,
                                                    i,
                                                    cat.display_name(),
                                                );
                                            }
                                        });
                                    ui.end_row();

                                    // Build Points
                                    ui.label("Build Points:");
                                    ui.text_edit_singleline(&mut ed.build_points);
                                    ui.end_row();

                                    // Workforce
                                    ui.label("Workforce:");
                                    ui.text_edit_singleline(&mut ed.workforce);
                                    ui.end_row();

                                    // Power Demand
                                    ui.label("Power Demand (MW):");
                                    ui.text_edit_singleline(&mut ed.power_demand_mw);
                                    ui.end_row();

                                    // Required Tech
                                    ui.label("Required Tech:");
                                    ui.text_edit_singleline(&mut ed.required_tech);
                                    ui.end_row();

                                    // Description
                                    ui.label("Description:");
                                    ui.text_edit_multiline(&mut ed.description);
                                    ui.end_row();
                                });

                            ui.add_space(8.0);

                            // ── Resource Costs ──
                            ui.label(egui::RichText::new("Resource Costs (construction)").strong());
                            ui.group(|ui| {
                                let mut remove_idx: Option<usize> = None;
                                for (i, (name, amt)) in ed.resource_costs.iter_mut().enumerate() {
                                    ui.horizontal(|ui| {
                                        ui.add(egui::TextEdit::singleline(name).desired_width(100.0));
                                        ui.label("×");
                                        ui.add(egui::TextEdit::singleline(amt).desired_width(60.0));
                                        if ui.small_button("✖").clicked() {
                                            remove_idx = Some(i);
                                        }
                                    });
                                }
                                if let Some(idx) = remove_idx {
                                    ed.resource_costs.remove(idx);
                                }
                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::TextEdit::singleline(&mut ed.new_cost_name)
                                            .hint_text("Resource")
                                            .desired_width(100.0),
                                    );
                                    ui.label("×");
                                    ui.add(
                                        egui::TextEdit::singleline(&mut ed.new_cost_amount)
                                            .hint_text("Amount")
                                            .desired_width(60.0),
                                    );
                                    if ui.button("➕").clicked()
                                        && !ed.new_cost_name.is_empty()
                                        && !ed.new_cost_amount.is_empty()
                                    {
                                        ed.resource_costs.push((
                                            std::mem::take(&mut ed.new_cost_name),
                                            std::mem::take(&mut ed.new_cost_amount),
                                        ));
                                    }
                                });
                            });

                            ui.add_space(6.0);

                            // ── Maintenance Resources ──
                            ui.label(egui::RichText::new("Maintenance Resources (per year)").strong());
                            ui.group(|ui| {
                                let mut remove_idx: Option<usize> = None;
                                for (i, (name, amt)) in
                                    ed.maintenance_resources.iter_mut().enumerate()
                                {
                                    ui.horizontal(|ui| {
                                        ui.add(egui::TextEdit::singleline(name).desired_width(100.0));
                                        ui.label("×");
                                        ui.add(egui::TextEdit::singleline(amt).desired_width(60.0));
                                        if ui.small_button("✖").clicked() {
                                            remove_idx = Some(i);
                                        }
                                    });
                                }
                                if let Some(idx) = remove_idx {
                                    ed.maintenance_resources.remove(idx);
                                }
                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::TextEdit::singleline(&mut ed.new_maint_name)
                                            .hint_text("Resource")
                                            .desired_width(100.0),
                                    );
                                    ui.label("×");
                                    ui.add(
                                        egui::TextEdit::singleline(&mut ed.new_maint_amount)
                                            .hint_text("Amount")
                                            .desired_width(60.0),
                                    );
                                    if ui.button("➕").clicked()
                                        && !ed.new_maint_name.is_empty()
                                        && !ed.new_maint_amount.is_empty()
                                    {
                                        ed.maintenance_resources.push((
                                            std::mem::take(&mut ed.new_maint_name),
                                            std::mem::take(&mut ed.new_maint_amount),
                                        ));
                                    }
                                });
                            });

                            ui.add_space(6.0);

                            // ── Modifiers ──
                            ui.label(egui::RichText::new("Modifiers (operational effects)").strong());
                            ui.group(|ui| {
                                let mut remove_idx: Option<usize> = None;
                                for (i, m) in ed.modifiers.iter().enumerate() {
                                    ui.horizontal(|ui| {
                                        ui.colored_label(theme::AMBER, format!("{}: {}", m.modifier_type, m.value));
                                        if ui.small_button("✖").clicked() {
                                            remove_idx = Some(i);
                                        }
                                    });
                                }
                                if let Some(idx) = remove_idx {
                                    ed.modifiers.remove(idx);
                                }
                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::TextEdit::singleline(&mut ed.new_modifier_type)
                                            .hint_text("ModifierType")
                                            .desired_width(140.0),
                                    );
                                    ui.add(
                                        egui::TextEdit::singleline(&mut ed.new_modifier_value)
                                            .hint_text("Value")
                                            .desired_width(60.0),
                                    );
                                    if ui.button("➕").clicked()
                                        && !ed.new_modifier_type.is_empty()
                                        && !ed.new_modifier_value.is_empty()
                                    {
                                        if let Ok(val) = ed.new_modifier_value.parse::<f64>() {
                                            ed.modifiers.push(
                                                crate::colony::data::BuildingModifierDef {
                                                    modifier_type: std::mem::take(&mut ed.new_modifier_type),
                                                    value: val,
                                                },
                                            );
                                            ed.new_modifier_value.clear();
                                        }
                                    }
                                });
                            });

                            ui.add_space(10.0);
                            if ui
                                .button(egui::RichText::new("💾 Save & Write to File").strong())
                                .clicked()
                            {
                                save_clicked = true;
                            }
                        });

                    // Apply save outside the scroll area borrow
                    if save_clicked {
                        let btype = ed.building_type;
                        if let Some(def) = data.definitions.get_mut(&btype) {
                            ed.apply_to(def);
                            let status = save_buildings_to_file(data);
                            edit_state.status_message = Some((status, elapsed));
                        }
                    }
                } else {
                    right.label(
                        egui::RichText::new("Select a building from the list to edit it.")
                            .color(theme::TEXT_DIM),
                    );
                }
            });
        });

    if !open {
        // If the user closed the window, keep edit_state open but unselected
        // (the window will re-open on next F12 toggle)
        edit_state.editing = None;
    }
}

/// Serialise all building definitions back to `assets/data/buildings.ron`.
/// Returns a human-readable status string.
fn save_buildings_to_file(data: &BuildingsData) -> String {
    use crate::colony::data::BuildingDefinition;
    use serde::Serialize;

    #[derive(Serialize)]
    struct BuildingsFile<'a> {
        buildings: Vec<&'a BuildingDefinition>,
    }

    // Sort by category then display_name for stable output
    let mut defs: Vec<&BuildingDefinition> = data.definitions.values().collect();
    defs.sort_by(|a, b| {
        a.category
            .cmp(&b.category)
            .then_with(|| a.display_name.cmp(&b.display_name))
    });

    let file_data = BuildingsFile { buildings: defs };

    let pretty = ron::ser::PrettyConfig::new()
        .depth_limit(4)
        .struct_names(false)
        .enumerate_arrays(false);

    match ron::ser::to_string_pretty(&file_data, pretty) {
        Ok(contents) => {
            let path = "assets/data/buildings.ron";
            match std::fs::write(path, &contents) {
                Ok(()) => format!("Saved {} buildings to {}", data.definitions.len(), path),
                Err(e) => format!("Error writing file: {}", e),
            }
        }
        Err(e) => format!("Error serialising: {}", e),
    }
}

