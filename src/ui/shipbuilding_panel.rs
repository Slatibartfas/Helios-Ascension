use std::collections::HashMap;

use super::dashboard::format_mass_compact;
use super::*;
use crate::economy::components::LocalStockpile;

type ShipyardColonyQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Colony,
        &'static CelestialBody,
        Option<&'static LocalStockpile>,
    ),
>;

#[derive(Resource, Debug, Clone)]
pub struct ShipbuildingUiState {
    pub selected_colony: Option<Entity>,
    pub selected_hull_id: Option<String>,
    pub selected_modules: HashMap<String, String>,
    pub design_name: String,
    pub selected_mode: crate::shipbuilding::ConstructionMode,
}

impl Default for ShipbuildingUiState {
    fn default() -> Self {
        Self {
            selected_colony: None,
            selected_hull_id: None,
            selected_modules: HashMap::default(),
            design_name: String::new(),
            selected_mode: crate::shipbuilding::ConstructionMode::SurfaceLaunch,
        }
    }
}

pub(super) fn ui_shipbuilding_panel(
    mut contexts: EguiContexts,
    active_menu: Res<ActiveMenu>,
    colonies: ShipyardColonyQuery,
    projects: Query<(Entity, &crate::shipbuilding::ShipConstructionProject)>,
    shipbuilding_data: Res<crate::shipbuilding::ShipbuildingData>,
    research_state: Res<crate::research::ResearchState>,
    mut actions: ResMut<crate::shipbuilding::PendingShipbuildingActions>,
    mut ui_state: ResMut<ShipbuildingUiState>,
    launch_state: Res<crate::shipbuilding::LaunchCapacityState>,
    budget: Res<GlobalBudget>,
) {
    if active_menu.current != GameMenu::Shipbuilding {
        return;
    }

    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    egui::CentralPanel::default()
        .frame(theme::central_frame())
        .show(ctx, |ui| {
            render_shipbuilding_panel(
                ui,
                &colonies,
                &projects,
                &shipbuilding_data,
                &research_state,
                &mut actions,
                &mut ui_state,
                &launch_state,
                &budget,
            );
        });
}

fn render_shipbuilding_panel(
    ui: &mut egui::Ui,
    colonies: &ShipyardColonyQuery,
    projects: &Query<(Entity, &crate::shipbuilding::ShipConstructionProject)>,
    shipbuilding_data: &crate::shipbuilding::ShipbuildingData,
    research_state: &crate::research::ResearchState,
    actions: &mut ResMut<crate::shipbuilding::PendingShipbuildingActions>,
    ui_state: &mut ShipbuildingUiState,
    launch_state: &crate::shipbuilding::LaunchCapacityState,
    budget: &GlobalBudget,
) {
    ui.label(
        egui::RichText::new("SHIPBUILDING")
            .font(theme::title())
            .color(theme::ACCENT),
    );
    ui.label(
        egui::RichText::new(
            "Library on the left, design bay in the center, performance and facility stats on the right. Completed projects now feed the real fleet layer.",
        )
        .font(theme::body(11.5))
        .color(theme::TEXT_DIM),
    );
    theme::divider(ui);

    let available_hulls = shipbuilding_data.available_hulls(research_state);
    ensure_shipbuilding_defaults(ui_state, colonies, &available_hulls);

    let selected_hull = ui_state
        .selected_hull_id
        .as_deref()
        .and_then(|hull_id| shipbuilding_data.get_hull(hull_id));
    if let Some(hull) = selected_hull {
        hydrate_default_slot_selection(ui_state, shipbuilding_data, research_state, hull);
    }

    let selected_colony = ui_state.selected_colony.and_then(|entity| colonies.get(entity).ok());
    let design = build_current_design(ui_state);
    let summary = shipbuilding_data.summarize_design(&design, research_state);

    ui.horizontal(|ui| {
        draw_status_chip(ui, "UNLOCKED HULLS", available_hulls.len().to_string(), theme::ACCENT);
        draw_status_chip(
            ui,
            "UNLOCKED MODULES",
            shipbuilding_data
                .modules
                .values()
                .filter(|module| shipbuilding_data.module_is_unlocked(module, research_state))
                .count()
                .to_string(),
            theme::EP_TEAL,
        );
        draw_status_chip(ui, "ACTIVE PROJECTS", projects.iter().count().to_string(), theme::AMBER);
    });

    ui.add_space(8.0);
    ui.columns(3, |columns| {
        theme::elevated_frame().show(&mut columns[0], |ui| {
            draw_library_column(ui, shipbuilding_data, research_state, ui_state, &available_hulls);
        });

        theme::elevated_frame().show(&mut columns[1], |ui| {
            draw_design_column(
                ui,
                colonies,
                shipbuilding_data,
                research_state,
                ui_state,
                &available_hulls,
            );
        });

        theme::elevated_frame().show(&mut columns[2], |ui| {
            draw_summary_column(
                ui,
                shipbuilding_data,
                research_state,
                ui_state,
                selected_hull,
                selected_colony,
                launch_state,
                budget,
                summary.as_ref(),
            );
        });
    });

    ui.add_space(8.0);
    theme::elevated_frame().show(ui, |ui| {
        draw_project_queue(ui, colonies, projects);
    });

    let can_queue = summary
        .as_ref()
        .is_some_and(|summary| summary.missing_required_slots.is_empty());
    if let Some((colony_entity, colony, _, _)) = selected_colony {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let queue_label = match ui_state.selected_mode {
                crate::shipbuilding::ConstructionMode::SurfaceLaunch => "Queue Surface Build",
                crate::shipbuilding::ConstructionMode::OrbitalAssembly => "Queue Orbital Assembly",
                crate::shipbuilding::ConstructionMode::OrbitalShipyard => {
                    "Queue Orbital Shipyard Build"
                }
            };

            if ui
                .add_enabled(can_queue, egui::Button::new(queue_label))
                .clicked()
            {
                actions.queue_projects.push(crate::shipbuilding::QueueShipConstructionAction {
                    build_site: colony_entity,
                    design,
                });
            }

            if colony.building_count(BuildingType::Shipyard) == 0 {
                ui.label(
                    egui::RichText::new(
                        "Selected colony needs a Shipyard before any design can be queued.",
                    )
                    .font(theme::body(11.0))
                    .color(theme::RED),
                );
            }
        });
    }
}

fn draw_status_chip(ui: &mut egui::Ui, label: &str, value: String, color: egui::Color32) {
    ui.vertical(|ui| {
        ui.set_min_width(132.0);
        ui.label(
            egui::RichText::new(label)
                .font(theme::mono(10.0))
                .color(theme::TEXT_DIM),
        );
        ui.label(egui::RichText::new(value).font(theme::heading()).color(color));
    });
}

fn draw_library_column(
    ui: &mut egui::Ui,
    shipbuilding_data: &crate::shipbuilding::ShipbuildingData,
    research_state: &crate::research::ResearchState,
    ui_state: &mut ShipbuildingUiState,
    available_hulls: &[&crate::shipbuilding::ShipHullDefinition],
) {
    ui.label(
        egui::RichText::new("Library")
            .font(theme::heading())
            .color(theme::TEXT_VALUE),
    );
    ui.label(
        egui::RichText::new("Available hulls and unlocked modules grouped by size class.")
            .font(theme::body(11.0))
            .color(theme::TEXT_DIM),
    );
    theme::divider(ui);

    // Group hulls by size tier
    use crate::shipbuilding::HullSizeTier;
    let mut hulls_by_tier: std::collections::HashMap<HullSizeTier, Vec<_>> = std::collections::HashMap::new();
    for hull in available_hulls {
        hulls_by_tier.entry(hull.effective_size_tier()).or_default().push(*hull);
    }

    ui.label(
        egui::RichText::new("Hull Catalog")
            .font(theme::mono(10.0))
            .color(theme::TEXT_DIM),
    );
    egui::ScrollArea::vertical()
        .id_salt("shipbuilding_hull_catalog")
        .show(ui, |ui| {
        for tier in [
            HullSizeTier::SmallCraft,
            HullSizeTier::MediumCraft,
            HullSizeTier::LargeCraft,
            HullSizeTier::CapitalShip,
            HullSizeTier::Station,
        ] {
            let Some(tier_hulls) = hulls_by_tier.get(&tier) else {
                continue;
            };

            ui.push_id(format!("hull_tier_{:?}", tier), |ui| {
                egui::CollapsingHeader::new(format!(
                    "{} {} ({})",
                    tier.display_name(),
                    match tier {
                        HullSizeTier::SmallCraft => "🔬",
                        HullSizeTier::MediumCraft => "🔭",
                        HullSizeTier::LargeCraft => "🚀",
                        HullSizeTier::CapitalShip => "🛸",
                        HullSizeTier::Station => "🛰",
                    },
                    tier_hulls.len()
                ))
                .default_open(tier == HullSizeTier::SmallCraft)
                .show(ui, |ui| {
                    for hull in tier_hulls {
                        let selected = ui_state.selected_hull_id.as_deref() == Some(hull.id.as_str());
                        if ui
                            .selectable_label(selected, format!("{} {}", hull.class.icon(), hull.display_name))
                            .clicked()
                        {
                            ui_state.selected_hull_id = Some(hull.id.clone());
                            ui_state.design_name = format!("{} Prototype", hull.display_name);
                            ui_state.selected_modules.clear();
                        }
                    }
                });
            });
        }
    });

    theme::divider(ui);
    ui.label(
        egui::RichText::new("Module Categories")
            .font(theme::mono(10.0))
            .color(theme::TEXT_DIM),
    );
    egui::ScrollArea::vertical()
        .id_salt("shipbuilding_module_library")
        .show(ui, |ui| {
        for category in crate::shipbuilding::ShipModuleCategory::all() {
            let modules: Vec<_> = shipbuilding_data
                .modules
                .values()
                .filter(|module| module.category == *category)
                .filter(|module| shipbuilding_data.module_is_unlocked(module, research_state))
                .collect();

            if modules.is_empty() {
                continue;
            }

            ui.push_id(format!("module_category_{:?}", category), |ui| {
                egui::CollapsingHeader::new(format!(
                    "{} {} ({})",
                    category.icon(),
                    category.display_name(),
                    modules.len()
                ))
                .default_open(matches!(
                    category,
                    crate::shipbuilding::ShipModuleCategory::Command
                        | crate::shipbuilding::ShipModuleCategory::Propulsion
                        | crate::shipbuilding::ShipModuleCategory::Power
                ))
                .show(ui, |ui| {
                    for module in modules {
                        let in_design = ui_state
                            .selected_modules
                            .values()
                            .any(|module_id| module_id == &module.id);
                        ui.label(
                            egui::RichText::new(format!("{} [{}]", module.display_name, module.size))
                                .font(theme::body(10.5))
                                .color(if in_design { theme::ACCENT } else { theme::TEXT }),
                        );
                    }
                });
            });
        }
    });
}

fn draw_design_column(
    ui: &mut egui::Ui,
    colonies: &ShipyardColonyQuery,
    shipbuilding_data: &crate::shipbuilding::ShipbuildingData,
    research_state: &crate::research::ResearchState,
    ui_state: &mut ShipbuildingUiState,
    available_hulls: &[&crate::shipbuilding::ShipHullDefinition],
) {
    ui.label(
        egui::RichText::new("Design Bay")
            .font(theme::heading())
            .color(theme::TEXT_VALUE),
    );
    ui.label(
        egui::RichText::new("Select a build site, choose a hull, and fill each module slot.")
            .font(theme::body(11.0))
            .color(theme::TEXT_DIM),
    );
    theme::divider(ui);

    let current_colony_name = ui_state
        .selected_colony
        .and_then(|entity| colonies.get(entity).ok().map(|(_, colony, _, _)| colony.name.clone()))
        .unwrap_or_else(|| "No colony".to_string());
    egui::ComboBox::from_label("Build Site")
        .selected_text(current_colony_name)
        .show_ui(ui, |ui| {
            let mut colony_rows: Vec<_> = colonies.iter().collect();
            colony_rows.sort_by(|left, right| left.1.name.cmp(&right.1.name));
            for (entity, colony, _, _) in colony_rows {
                ui.selectable_value(&mut ui_state.selected_colony, Some(entity), colony.name.clone());
            }
        });

    let current_hull_name = ui_state
        .selected_hull_id
        .as_deref()
        .and_then(|hull_id| shipbuilding_data.get_hull(hull_id))
        .map(|hull| hull.display_name.clone())
        .unwrap_or_else(|| "Select hull".to_string());
    let mut selected_hull_id = ui_state.selected_hull_id.clone();
    egui::ComboBox::from_label("Hull")
        .selected_text(current_hull_name)
        .show_ui(ui, |ui| {
            for hull in available_hulls {
                ui.selectable_value(
                    &mut selected_hull_id,
                    Some(hull.id.clone()),
                    format!("{} {} [{}]", hull.class.icon(), hull.display_name, hull.effective_size_tier().display_name()),
                );
            }
        });

    if selected_hull_id != ui_state.selected_hull_id {
        ui_state.selected_hull_id = selected_hull_id;
        ui_state.selected_modules.clear();
        if let Some(hull_id) = &ui_state.selected_hull_id {
            if let Some(hull) = shipbuilding_data.get_hull(hull_id) {
                ui_state.design_name = format!("{} Prototype", hull.display_name);
            }
        }
    }

    ui.label(egui::RichText::new("Design Name").font(theme::mono(10.0)).color(theme::TEXT_DIM));
    ui.text_edit_singleline(&mut ui_state.design_name);

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Construction Path")
                .font(theme::mono(10.0))
                .color(theme::TEXT_DIM),
        );
        for mode in [
            crate::shipbuilding::ConstructionMode::SurfaceLaunch,
            crate::shipbuilding::ConstructionMode::OrbitalAssembly,
            crate::shipbuilding::ConstructionMode::OrbitalShipyard,
        ] {
            ui.selectable_value(&mut ui_state.selected_mode, mode, mode.short_name());
        }
    });

    theme::divider(ui);
    if let Some(hull_id) = &ui_state.selected_hull_id {
        if let Some(hull) = shipbuilding_data.get_hull(hull_id) {
            ui.label(
                egui::RichText::new(&hull.description)
                    .font(theme::body(11.0))
                    .color(theme::TEXT_DIM),
            );
            ui.add_space(6.0);

            egui::ScrollArea::vertical()
                .id_salt(format!("shipbuilding_slots_{}", hull.id))
                .show(ui, |ui| {
                for category in crate::shipbuilding::ShipModuleCategory::all() {
                    let category_slots: Vec<_> = hull
                        .slot_layout
                        .iter()
                        .filter(|slot| slot.category == *category)
                        .collect();
                    if category_slots.is_empty() {
                        continue;
                    }

                    ui.push_id(format!("slot_category_{:?}", category), |ui| {
                        ui.label(
                            egui::RichText::new(format!(
                                "{} {}",
                                category.icon(),
                                category.display_name()
                            ))
                            .font(theme::mono(10.0))
                            .color(theme::TEXT_DIM),
                        );

                        for slot in category_slots {
                            let compatible = shipbuilding_data.compatible_modules_for_slot(slot, research_state);
                            let selected_text = ui_state
                                .selected_modules
                                .get(&slot.slot_id)
                                .and_then(|module_id| shipbuilding_data.get_module(module_id))
                                .map(|module| module.display_name.clone())
                                .unwrap_or_else(|| {
                                    if slot.required {
                                        "Select module".to_string()
                                    } else {
                                        "Optional".to_string()
                                    }
                                });

                            egui::Frame::NONE
                                .fill(theme::SURFACE)
                                .stroke(egui::Stroke::new(1.0, theme::BORDER))
                                .inner_margin(egui::Margin::same(6))
                                .show(ui, |ui| {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "{} [{}]{}",
                                            slot.slot_id,
                                            slot.size,
                                            if slot.required { "" } else { " optional" }
                                        ))
                                        .font(theme::body(10.8))
                                        .color(theme::TEXT_VALUE),
                                    );

                                    egui::ComboBox::from_id_salt(format!("ship_slot_{}", slot.slot_id))
                                        .selected_text(selected_text)
                                        .width(ui.available_width())
                                        .show_ui(ui, |ui| {
                                            if !slot.required {
                                                let cleared = ui
                                                    .selectable_label(
                                                        !ui_state.selected_modules.contains_key(&slot.slot_id),
                                                        "None",
                                                    )
                                                    .clicked();
                                                if cleared {
                                                    ui_state.selected_modules.remove(&slot.slot_id);
                                                }
                                            }

                                            for module in compatible {
                                                let selected = ui_state
                                                    .selected_modules
                                                    .get(&slot.slot_id)
                                                    .is_some_and(|module_id| module_id == &module.id);
                                                if ui
                                                    .selectable_label(selected, module.display_name.clone())
                                                    .clicked()
                                                {
                                                    ui_state
                                                        .selected_modules
                                                        .insert(slot.slot_id.clone(), module.id.clone());
                                                }
                                            }
                                        });
                                });
                        }
                    });
                }
            });
        }
    }
}

fn draw_summary_column(
    ui: &mut egui::Ui,
    shipbuilding_data: &crate::shipbuilding::ShipbuildingData,
    research_state: &crate::research::ResearchState,
    ui_state: &ShipbuildingUiState,
    selected_hull: Option<&crate::shipbuilding::ShipHullDefinition>,
    selected_colony: Option<(
        Entity,
        &Colony,
        &CelestialBody,
        Option<&LocalStockpile>,
    )>,
    launch_state: &crate::shipbuilding::LaunchCapacityState,
    budget: &GlobalBudget,
    summary: Option<&crate::shipbuilding::ShipDesignSummary>,
) {
    ui.label(
        egui::RichText::new("Performance")
            .font(theme::heading())
            .color(theme::TEXT_VALUE),
    );
    ui.label(
        egui::RichText::new("Design performance, construction requirements, and site capacity.")
            .font(theme::body(11.0))
            .color(theme::TEXT_DIM),
    );
    theme::divider(ui);

    if let Some(hull) = selected_hull {
        egui::Grid::new("shipbuilding_summary_grid").show(ui, |ui| {
            theme::stat_row(ui, "Hull", &hull.display_name);
            theme::stat_row(ui, "Size Class", hull.effective_size_tier().display_name());
            theme::stat_row(ui, "Role", hull.class.display_name());
            theme::stat_row(ui, "Construction", ui_state.selected_mode.display_name());

            if let Some(summary) = summary {
                theme::stat_row(ui, "Build Points", &format!("{:.0}", summary.build_points));
                theme::stat_row(ui, "Dry Mass", &format_mass_compact(summary.dry_mass_t));
                theme::stat_row(ui, "Launch Mass", &format_mass_compact(summary.launch_mass_t));
                theme::stat_row(ui, "Fuel Capacity", &format_mass_compact(summary.fuel_capacity_t));
                theme::stat_row(ui, "Cargo Capacity", &format_mass_compact(summary.cargo_capacity_t));
                if summary.ordnance_capacity_t > 0.0 {
                    theme::stat_row(ui, "Ordnance Cap", &format_mass_compact(summary.ordnance_capacity_t));
                }
                if summary.magazine_capacity_t > 0.0 {
                    theme::stat_row(ui, "Magazine Cap", &format_mass_compact(summary.magazine_capacity_t));
                }
                theme::stat_row(ui, "Crew", &format!("{:.0}", summary.crew));
                theme::stat_row(ui, "Power Gen", &format!("{:.1} MW", summary.power_generation_mw));
                theme::stat_row(ui, "Power Draw", &format!("{:.1} MW", summary.power_draw_mw));
                theme::stat_row(ui, "Power Balance", &format!("{:+.1} MW", summary.power_balance_mw()));
                theme::stat_row(ui, "Thrust", &format!("{:.1} kN", summary.thrust_kn));
                theme::stat_row(ui, "Acceleration", &format!("{:.3} m/s²", summary.acceleration_ms2));
                theme::stat_row(ui, "Delta-V", &format!("{:.0} m/s", summary.delta_v_ms));
                theme::stat_row(ui, "Isp", &format!("{:.0} s", summary.isp_s));
                theme::stat_row(ui, "Sensor Range", &format!("{:.2} AU", summary.sensor_range_au));
                theme::stat_row(ui, "Docking Ports", &format!("{:.0}", summary.docking_ports));
                theme::stat_row(
                    ui,
                    "Orbital Construction",
                    &format!("{:.0} BP/yr", summary.construction_capacity_bp_per_year),
                );
            }
        });

        if let Some(summary) = summary {
            if !summary.missing_required_slots.is_empty() {
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(format!(
                        "Missing required slots: {}",
                        summary.missing_required_slots.join(", ")
                    ))
                    .font(theme::body(11.0))
                    .color(theme::RED),
                );
            }

            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("Construction Inputs")
                    .font(theme::mono(10.0))
                    .color(theme::TEXT_DIM),
            );
            for (resource, amount) in &summary.resource_costs {
                ui.label(
                    egui::RichText::new(format!("{:?}: {}", resource, format_material_amount_mt(*amount)))
                        .font(theme::body(10.5))
                        .color(theme::TEXT_VALUE),
                );
            }

            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("Launch")
                    .font(theme::mono(10.0))
                    .color(theme::TEXT_DIM),
            );
            ui.label(
                egui::RichText::new(format!(
                    "Launch Credit Cost: {:.0} MC",
                    summary.launch_mass_t * 0.45
                ))
                .font(theme::body(10.5))
                .color(theme::TEXT_VALUE),
            );
            for (resource, amount) in crate::shipbuilding::systems::launch_resource_costs(summary.launch_mass_t) {
                ui.label(
                    egui::RichText::new(format!("{:?}: {}", resource, format_material_amount_mt(amount)))
                        .font(theme::body(10.5))
                        .color(theme::TEXT_VALUE),
                );
            }

            if let Some((site_entity, colony, _, stockpile)) = selected_colony {
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new("Site Status")
                        .font(theme::mono(10.0))
                        .color(theme::TEXT_DIM),
                );
                let annual_capacity = crate::shipbuilding::systems::annual_launch_capacity_t(colony);
                let available_capacity = launch_state
                    .available_mass_t
                    .get(&site_entity)
                    .copied()
                    .unwrap_or(annual_capacity);
                ui.label(
                    egui::RichText::new(format!(
                        "Available Launch Capacity: {:.0} / {:.0} t",
                        available_capacity, annual_capacity
                    ))
                    .font(theme::body(10.5))
                    .color(theme::TEXT_VALUE),
                );
                ui.label(
                    egui::RichText::new(format!("Treasury: {}", format_currency(budget.treasury)))
                        .font(theme::body(10.5))
                        .color(theme::TEXT_VALUE),
                );
                if let Some(stockpile) = stockpile {
                    for (resource, amount) in &summary.resource_costs {
                        let local = stockpile.get(resource);
                        ui.label(
                            egui::RichText::new(format!(
                                "Local {:?}: {} / {}",
                                resource,
                                format_material_amount_mt(local),
                                format_material_amount_mt(*amount)
                            ))
                            .font(theme::body(10.0))
                            .color(if local >= *amount { theme::GREEN } else { theme::AMBER }),
                        );
                    }
                }
            }
        }
    } else {
        let _ = (shipbuilding_data, research_state);
        ui.label(
            egui::RichText::new("Select an unlocked hull to begin assembling a design.")
                .font(theme::body(11.0))
                .color(theme::TEXT_DIM),
        );
    }
}

fn draw_project_queue(
    ui: &mut egui::Ui,
    colonies: &ShipyardColonyQuery,
    projects: &Query<(Entity, &crate::shipbuilding::ShipConstructionProject)>,
) {
    ui.label(
        egui::RichText::new("Construction Queue")
            .font(theme::heading())
            .color(theme::TEXT_VALUE),
    );
    ui.add_space(4.0);

    let mut project_rows: Vec<_> = projects.iter().collect();
    project_rows.sort_by(|left, right| left.1.design_name.cmp(&right.1.design_name));

    if project_rows.is_empty() {
        ui.label(
            egui::RichText::new("No ship projects queued yet.")
                .font(theme::body(11.0))
                .color(theme::TEXT_DIM),
        );
        return;
    }

    egui::Grid::new("shipbuilding_queue_grid")
        .num_columns(7)
        .spacing([18.0, 8.0])
        .striped(true)
        .show(ui, |ui| {
            for (_, project) in project_rows {
                let colony_name = colonies
                    .get(project.build_site)
                    .map(|(_, colony, _, _)| colony.name.clone())
                    .unwrap_or_else(|_| "Unknown Site".to_string());

                ui.label(egui::RichText::new(&project.design_name).font(theme::body(11.5)).color(theme::TEXT_VALUE));
                ui.label(egui::RichText::new(colony_name).font(theme::body(11.0)).color(theme::TEXT_DIM));
                ui.label(egui::RichText::new(project.construction_mode.short_name()).font(theme::mono(10.0)).color(theme::ACCENT));
                ui.label(
                    egui::RichText::new(if project.awaiting_resources {
                        "Awaiting Resources"
                    } else {
                        project.state.label()
                    })
                    .font(theme::body(11.0))
                    .color(if project.awaiting_resources {
                        theme::AMBER
                    } else {
                        match project.state {
                            crate::shipbuilding::ShipConstructionState::Building => theme::AMBER,
                            crate::shipbuilding::ShipConstructionState::ReadyForLaunch => theme::GREEN,
                            crate::shipbuilding::ShipConstructionState::CompletedInOrbit => theme::ACCENT,
                        }
                    }),
                );
                ui.label(egui::RichText::new(format!("{:.0}%", project.progress_percent() as f64 * 100.0)).font(theme::mono(10.0)).color(theme::TEXT_VALUE));
                ui.label(egui::RichText::new(format!("{:.0} BP", project.required_build_points)).font(theme::mono(10.0)).color(theme::TEXT_DIM));
                ui.label(egui::RichText::new(format_mass_compact(project.launch_mass_t)).font(theme::mono(10.0)).color(theme::TEXT_DIM));
                ui.end_row();
            }
        });
}

fn ensure_shipbuilding_defaults(
    ui_state: &mut ShipbuildingUiState,
    colonies: &ShipyardColonyQuery,
    available_hulls: &[&crate::shipbuilding::ShipHullDefinition],
) {
    if ui_state.selected_colony.is_none() {
        let mut colony_rows: Vec<_> = colonies.iter().collect();
        colony_rows.sort_by(|left, right| left.1.name.cmp(&right.1.name));
        ui_state.selected_colony = colony_rows.first().map(|(entity, _, _, _)| *entity);
    }

    if ui_state.selected_hull_id.is_none() {
        ui_state.selected_hull_id = available_hulls.first().map(|hull| hull.id.clone());
        if ui_state.design_name.is_empty() {
            if let Some(hull) = available_hulls.first() {
                ui_state.design_name = format!("{} Prototype", hull.display_name);
            }
        }
    }
}

fn hydrate_default_slot_selection(
    ui_state: &mut ShipbuildingUiState,
    shipbuilding_data: &crate::shipbuilding::ShipbuildingData,
    research_state: &crate::research::ResearchState,
    hull: &crate::shipbuilding::ShipHullDefinition,
) {
    for slot in &hull.slot_layout {
        if ui_state.selected_modules.contains_key(&slot.slot_id) {
            continue;
        }

        let compatible = shipbuilding_data.compatible_modules_for_slot(slot, research_state);
        if let Some(module) = compatible.first() {
            ui_state.selected_modules.insert(slot.slot_id.clone(), module.id.clone());
        }
    }
}

fn build_current_design(ui_state: &ShipbuildingUiState) -> crate::shipbuilding::ShipDesignDraft {
    let mut modules: Vec<_> = ui_state
        .selected_modules
        .iter()
        .map(|(slot_id, module_id)| crate::shipbuilding::ShipModuleSelection {
            slot_id: slot_id.clone(),
            module_id: module_id.clone(),
        })
        .collect();
    modules.sort_by(|left, right| left.slot_id.cmp(&right.slot_id));

    crate::shipbuilding::ShipDesignDraft {
        name: if ui_state.design_name.trim().is_empty() {
            "Unnamed Design".to_string()
        } else {
            ui_state.design_name.trim().to_string()
        },
        hull_id: ui_state.selected_hull_id.clone().unwrap_or_default(),
        modules,
        construction_mode: ui_state.selected_mode,
    }
}

fn format_material_amount_mt(amount_mt: f64) -> String {
    if amount_mt >= 1.0 {
        format!("{amount_mt:.2} Mt")
    } else if amount_mt >= 0.001 {
        format!("{:.2} kt", amount_mt * 1_000.0)
    } else if amount_mt >= 0.000_001 {
        format!("{:.2} t", amount_mt * 1_000_000.0)
    } else {
        format!("{:.1} kg", amount_mt * 1_000_000_000.0)
    }
}