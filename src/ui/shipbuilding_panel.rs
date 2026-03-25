use bevy::prelude::*;
use bevy_egui::egui;

use super::dashboard::format_mass_compact_tonnes;
use super::shipbuilding_state::{DesignSort, ShipRosterRow, ShipbuildingTab, ShipbuildingUiState};
use super::shipbuilding_tooltip::{
    build_module_tooltip, build_slot_tooltip, prettify_slot_name, render_shipbuilding_tooltip,
};
use super::shipbuilding_workspace::ShipbuildingUiBackend;
use super::*;
use crate::economy::ResourceType;
use crate::economy::components::LocalStockpile;
use crate::fleets::{
    AssignShipsAction, CreateFleetFromShipsAction, FleetRole, ShipClass, ShipInstance,
};
use crate::shipbuilding::{
    ConstructionMode, QueueShipConstructionAction, ShipConstructionProject, ShipConstructionState,
    ShipDesignDraft, ShipDesignLibrary, ShipDesignSummary, ShipDesignTemplate, ShipModuleSelection,
    ShipyardFacility,
};

type ShipyardColonyQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Colony,
        &'static CelestialBody,
        Option<&'static LocalStockpile>,
        Option<&'static ShipyardFacility>,
    ),
>;

type FleetRosterQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Fleet,
        Option<&'static FleetOrbit>,
        Option<&'static ActiveManeuver>,
    ),
>;

type ShipInstanceQuery<'w, 's> = Query<'w, 's, (Entity, &'static ShipInstance)>;

#[derive(Clone)]
struct DesignBrowserRow {
    template_id: uuid::Uuid,
    name: String,
    version: u32,
    hull_name: String,
    hull_class: ShipClass,
    summary: ShipDesignSummary,
    construction_mode: ConstructionMode,
}

pub(super) fn ui_shipbuilding_panel(
    mut contexts: EguiContexts,
    active_menu: Res<ActiveMenu>,
    backend: Res<ShipbuildingUiBackend>,
    colonies: ShipyardColonyQuery,
    fleets: FleetRosterQuery,
    ships: ShipInstanceQuery,
    projects: Query<(Entity, &ShipConstructionProject)>,
    shipbuilding_data: Res<crate::shipbuilding::ShipbuildingData>,
    mut design_library: ResMut<ShipDesignLibrary>,
    research_state: Res<crate::research::ResearchState>,
    mut shipbuilding_actions: ResMut<crate::shipbuilding::PendingShipbuildingActions>,
    mut fleet_actions: ResMut<PendingFleetActions>,
    mut ui_state: ResMut<ShipbuildingUiState>,
    launch_state: Res<crate::shipbuilding::LaunchCapacityState>,
    budget: Res<GlobalBudget>,
) {
    if active_menu.current != GameMenu::Shipbuilding || !backend.uses_legacy_egui() {
        return;
    }

    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let available_hulls = shipbuilding_data.available_hulls(&research_state);
    ensure_defaults(&mut ui_state, &colonies, &available_hulls, &design_library);
    hydrate_selected_design(&mut ui_state, &shipbuilding_data, &research_state);

    egui::CentralPanel::default()
        .frame(theme::central_frame())
        .show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new("SHIP CONSTRUCTION COMMAND")
                            .font(theme::title())
                            .color(theme::ACCENT),
                    );
                    ui.label(
                        egui::RichText::new(
                            "Aurora-style design browsing, colony shipyard oversight, and direct ship-to-fleet assignment on one screen.",
                        )
                        .font(theme::body(11.5))
                        .color(theme::TEXT_DIM),
                    );
                });
                ui.add_space(16.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    ui.horizontal_wrapped(|ui| {
                        stat_chip(
                            ui,
                            "Shipyards",
                            colonies
                                .iter()
                                .filter(|(_, colony, _, _, _)| {
                                    colony.building_count(BuildingType::Shipyard) > 0
                                })
                                .count()
                                .to_string(),
                            theme::ACCENT,
                        );
                        stat_chip(
                            ui,
                            "Designs",
                            design_library.templates.len().to_string(),
                            theme::EP_TEAL,
                        );
                        stat_chip(
                            ui,
                            "Projects",
                            projects.iter().count().to_string(),
                            theme::AMBER,
                        );
                    });
                });
            });

            theme::divider(ui);
            draw_tabs(ui, &mut ui_state.active_tab);
            ui.add_space(8.0);

            match ui_state.active_tab {
                ShipbuildingTab::Design => draw_design_tab(
                    ui,
                    &shipbuilding_data,
                    &mut design_library,
                    &research_state,
                    &mut ui_state,
                    &available_hulls,
                ),
                ShipbuildingTab::Archive => draw_archive_tab(
                    ui,
                    &shipbuilding_data,
                    &mut design_library,
                    &research_state,
                    &mut ui_state,
                ),
                ShipbuildingTab::Construction => draw_construction_tab(
                    ui,
                    &colonies,
                    &projects,
                    &design_library,
                    &shipbuilding_data,
                    &research_state,
                    &mut shipbuilding_actions,
                    &mut ui_state,
                    &launch_state,
                    &budget,
                ),
                ShipbuildingTab::Ships => draw_ships_tab(
                    ui,
                    &colonies,
                    &ships,
                    &fleets,
                    &mut fleet_actions,
                    &mut ui_state,
                ),
            }
        });
}

fn draw_tabs(ui: &mut egui::Ui, active_tab: &mut ShipbuildingTab) {
    ui.horizontal(|ui| {
        tab_button(ui, active_tab, ShipbuildingTab::Design, "Design");
        tab_button(ui, active_tab, ShipbuildingTab::Archive, "Archive");
        tab_button(
            ui,
            active_tab,
            ShipbuildingTab::Construction,
            "Construction Control",
        );
        tab_button(ui, active_tab, ShipbuildingTab::Ships, "Ship Roster");
    });
}

fn tab_button(
    ui: &mut egui::Ui,
    active_tab: &mut ShipbuildingTab,
    tab: ShipbuildingTab,
    label: &str,
) {
    let selected = *active_tab == tab;
    if ui
        .add_sized(
            [180.0, 28.0],
            egui::Button::new(egui::RichText::new(label).font(theme::body(11.5)).color(
                if selected {
                    theme::ACCENT
                } else {
                    theme::TEXT_VALUE
                },
            ))
            .selected(selected),
        )
        .clicked()
    {
        *active_tab = tab;
    }
}

fn draw_design_tab(
    ui: &mut egui::Ui,
    shipbuilding_data: &crate::shipbuilding::ShipbuildingData,
    design_library: &mut ShipDesignLibrary,
    research_state: &crate::research::ResearchState,
    ui_state: &mut ShipbuildingUiState,
    available_hulls: &[&crate::shipbuilding::ShipHullDefinition],
) {
    let selected_hull = ui_state
        .selected_hull_id
        .as_deref()
        .and_then(|hull_id| shipbuilding_data.get_hull(hull_id));
    let current_design = build_current_design(ui_state);
    let current_summary = shipbuilding_data.summarize_design(&current_design, research_state);

    let available_width = ui.available_width();
    if available_width < 1360.0 {
        egui::ScrollArea::vertical()
            .id_salt("design_workspace_stacked_scroll")
            .show(ui, |ui| {
                render_design_workspace_panel(
                    ui,
                    shipbuilding_data,
                    research_state,
                    ui_state,
                    available_hulls,
                    selected_hull,
                );
            });
        ui.add_space(10.0);
        egui::ScrollArea::vertical()
            .id_salt("design_summary_stacked_scroll")
            .show(ui, |ui| {
                render_design_summary_panel(
                    ui,
                    shipbuilding_data,
                    design_library,
                    research_state,
                    ui_state,
                    selected_hull,
                    current_summary.as_ref(),
                );
            });
    } else {
        let summary_width = (available_width * 0.24).clamp(280.0, 380.0);
        let design_width = (available_width - summary_width - 12.0).max(700.0);

        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(design_width, ui.available_height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("design_workspace_wide_scroll")
                        .show(ui, |ui| {
                            render_design_workspace_panel(
                                ui,
                                shipbuilding_data,
                                research_state,
                                ui_state,
                                available_hulls,
                                selected_hull,
                            )
                        })
                },
            );

            ui.add_space(12.0);
            ui.allocate_ui_with_layout(
                egui::vec2(summary_width, ui.available_height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("design_summary_wide_scroll")
                        .show(ui, |ui| {
                            render_design_summary_panel(
                                ui,
                                shipbuilding_data,
                                design_library,
                                research_state,
                                ui_state,
                                selected_hull,
                                current_summary.as_ref(),
                            )
                        })
                },
            );
        });
    }
}

fn render_design_workspace_panel(
    ui: &mut egui::Ui,
    shipbuilding_data: &crate::shipbuilding::ShipbuildingData,
    research_state: &crate::research::ResearchState,
    ui_state: &mut ShipbuildingUiState,
    available_hulls: &[&crate::shipbuilding::ShipHullDefinition],
    selected_hull: Option<&crate::shipbuilding::ShipHullDefinition>,
) {
    theme::elevated_frame().show(ui, |ui| {
        ui.label(
            egui::RichText::new("Design Section")
                .font(theme::heading())
                .color(theme::TEXT_VALUE),
        );
        ui.label(
            egui::RichText::new(
                "Configure a hull, click a section in the schematic, and fit compatible modules from the component panel.",
            )
            .font(theme::body(11.0))
            .color(theme::TEXT_DIM),
        );
        theme::divider(ui);

        design_controls(ui, shipbuilding_data, research_state, ui_state, available_hulls);

        ui.add_space(8.0);
        if let Some(hull) = selected_hull {
            draw_hull_editor(ui, shipbuilding_data, research_state, ui_state, hull);
        } else {
            ui.label(
                egui::RichText::new("No unlocked hull selected.")
                    .font(theme::body(11.0))
                    .color(theme::TEXT_DIM),
            );
        }
    });
}

fn render_design_summary_panel(
    ui: &mut egui::Ui,
    shipbuilding_data: &crate::shipbuilding::ShipbuildingData,
    design_library: &mut ShipDesignLibrary,
    research_state: &crate::research::ResearchState,
    ui_state: &mut ShipbuildingUiState,
    selected_hull: Option<&crate::shipbuilding::ShipHullDefinition>,
    current_summary: Option<&ShipDesignSummary>,
) {
    theme::elevated_frame().show(ui, |ui| {
        ui.label(
            egui::RichText::new("Design Overview")
                .font(theme::heading())
                .color(theme::TEXT_VALUE),
        );
        ui.label(
            egui::RichText::new(
                "A compact command-panel view of the current draft: massing, combat posture, power balance, and save controls.",
            )
            .font(theme::body(11.0))
            .color(theme::TEXT_DIM),
        );
        theme::divider(ui);

        if let Some(template_id) = ui_state.selected_template_id {
            if let Some(template) = design_library.get_template(&template_id) {
                ui.label(
                    egui::RichText::new(format!(
                        "Loaded from archive: {} v{}",
                        template.name, template.version
                    ))
                    .font(theme::body(10.5))
                    .color(theme::TEXT_DIM),
                );
                ui.add_space(6.0);
            }
        }

        draw_current_design_summary(ui, selected_hull, current_summary, ui_state);

        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui.button("Save To Archive").clicked() {
                save_current_design_template(design_library, ui_state);
            }

            if ui.button("Reset Draft").clicked() {
                ui_state.selected_template_id = None;
                ui_state.selected_modules.clear();
                ui_state.selected_slot = None;
                if let Some(hull) = selected_hull {
                    ui_state.design_name = format!("{} Prototype", hull.display_name);
                    ui_state.selected_mode = hull.default_construction_mode;
                }
                hydrate_selected_design(ui_state, shipbuilding_data, research_state);
            }
        });
    });
}

fn draw_archive_tab(
    ui: &mut egui::Ui,
    shipbuilding_data: &crate::shipbuilding::ShipbuildingData,
    design_library: &mut ShipDesignLibrary,
    research_state: &crate::research::ResearchState,
    ui_state: &mut ShipbuildingUiState,
) {
    let browser_rows = build_design_browser_rows(design_library, shipbuilding_data, research_state);
    let mut sorted_rows = browser_rows.clone();
    sorted_rows.sort_by(|left, right| compare_design_rows(left, right, ui_state.design_sort));
    if ui_state.design_sort_descending {
        sorted_rows.reverse();
    }

    let mut open_for_edit = None;
    let mut create_new_from = None;
    let mut copy_from = None;
    let mut delete_selected = None;

    ui.columns(2, |columns| {
        theme::elevated_frame().show(&mut columns[0], |ui| {
            ui.label(
                egui::RichText::new("Design Archive")
                    .font(theme::heading())
                    .color(theme::TEXT_VALUE),
            );
            ui.label(
                egui::RichText::new(
                    "Sort, group, and inspect saved designs by the same hull and combat heuristics players actually use.",
                )
                .font(theme::body(11.0))
                .color(theme::TEXT_DIM),
            );
            theme::divider(ui);

            ui.horizontal(|ui| {
                egui::ComboBox::from_label("Sort By")
                    .selected_text(ui_state.design_sort.label())
                    .show_ui(ui, |ui| {
                        for sort in [
                            DesignSort::HullType,
                            DesignSort::DeltaV,
                            DesignSort::Combat,
                            DesignSort::Weight,
                        ] {
                            ui.selectable_value(&mut ui_state.design_sort, sort, sort.label());
                        }
                    });
                ui.checkbox(&mut ui_state.design_sort_descending, "Descending");
            });

            ui.add_space(8.0);
            egui::ScrollArea::vertical()
                .id_salt("ship_design_archive")
                .show(ui, |ui| {
                    for class in [
                        ShipClass::Courier,
                        ShipClass::Frigate,
                        ShipClass::Destroyer,
                        ShipClass::Cruiser,
                        ShipClass::ResearchVessel,
                        ShipClass::Freighter,
                        ShipClass::Station,
                    ] {
                        let class_rows: Vec<_> = sorted_rows
                            .iter()
                            .filter(|row| row.hull_class == class)
                            .collect();
                        if class_rows.is_empty() {
                            continue;
                        }

                        egui::CollapsingHeader::new(format!(
                            "{} {} ({})",
                            class.icon(),
                            class.display_name(),
                            class_rows.len()
                        ))
                        .default_open(true)
                        .show(ui, |ui| {
                            for row in class_rows {
                                let selected = ui_state.selected_template_id == Some(row.template_id);
                                let response = egui::Frame::NONE
                                    .fill(if selected {
                                        theme::SURFACE_RAISED
                                    } else {
                                        theme::SURFACE
                                    })
                                    .stroke(egui::Stroke::new(1.0, theme::BORDER))
                                    .inner_margin(egui::Margin::same(8))
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.vertical(|ui| {
                                                ui.label(
                                                    egui::RichText::new(format!(
                                                        "{} v{}",
                                                        row.name, row.version
                                                    ))
                                                    .font(theme::body(11.0))
                                                    .color(theme::TEXT_VALUE),
                                                );
                                                ui.label(
                                                    egui::RichText::new(format!(
                                                        "{} | {} | {}",
                                                        row.hull_name,
                                                        row.construction_mode.display_name(),
                                                        format_mass_compact_tonnes(
                                                            row.summary.launch_mass_t
                                                        )
                                                    ))
                                                    .font(theme::body(10.0))
                                                    .color(theme::TEXT_DIM),
                                                );
                                            });
                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| {
                                                    ui.label(
                                                        egui::RichText::new(format!(
                                                            "CV {:.1}",
                                                            combat_score(&row.summary)
                                                        ))
                                                        .font(theme::mono(10.0))
                                                        .color(theme::AMBER),
                                                    );
                                                    ui.label(
                                                        egui::RichText::new(format!(
                                                            "{:.0} m/s",
                                                            row.summary.delta_v_ms
                                                        ))
                                                        .font(theme::mono(10.0))
                                                        .color(theme::RP_BLUE),
                                                    );
                                                },
                                            );
                                        });
                                    })
                                    .response;
                                if response.clicked() {
                                    ui_state.selected_template_id = Some(row.template_id);
                                }
                                ui.add_space(4.0);
                            }
                        });
                    }

                    if sorted_rows.is_empty() {
                        ui.label(
                            egui::RichText::new(
                                "No saved designs yet. Save a draft from the Design tab to populate the archive.",
                            )
                            .font(theme::body(11.0))
                            .color(theme::TEXT_DIM),
                        );
                    }
                });
        });

        theme::elevated_frame().show(&mut columns[1], |ui| {
            ui.label(
                egui::RichText::new("Archive Actions")
                    .font(theme::heading())
                    .color(theme::TEXT_VALUE),
            );
            ui.label(
                egui::RichText::new(
                    "Open a stored design back into the designer, clone it, or remove it from the archive.",
                )
                .font(theme::body(11.0))
                .color(theme::TEXT_DIM),
            );
            theme::divider(ui);

            if let Some(template_id) = ui_state.selected_template_id {
                if let Some(template) = design_library.get_template(&template_id).cloned() {
                    let summary = shipbuilding_data
                        .summarize_design(&design_from_template(&template), research_state);
                    ui.label(
                        egui::RichText::new(format!("{} v{}", template.name, template.version))
                            .font(theme::heading())
                            .color(theme::TEXT_VALUE),
                    );
                    ui.add_space(6.0);
                    if let Some(summary) = summary.as_ref() {
                        metrics_card(
                            ui,
                            "Archive Snapshot",
                            &[
                                ("Hull", &template.hull_id),
                                ("Build", &format!("{:.0} BP", summary.build_points)),
                                ("Mass", &format_mass_compact_tonnes(summary.launch_mass_t)),
                                ("Delta-V", &format!("{:.0} m/s", summary.delta_v_ms)),
                                ("Combat", &format!("{:.1}", combat_score(summary))),
                                ("Sensors", &format!("{:.2} AU", summary.sensor_range_au)),
                            ],
                        );
                    }

                    ui.add_space(10.0);
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Edit").clicked() {
                            open_for_edit = Some(template.id);
                        }
                        if ui.button("Create New From").clicked() {
                            create_new_from = Some(template.id);
                        }
                        if ui.button("Copy").clicked() {
                            copy_from = Some(template.id);
                        }
                        if ui.button("Delete").clicked() {
                            delete_selected = Some(template.id);
                        }
                    });
                } else {
                    ui.label(
                        egui::RichText::new("Selected archive entry no longer exists.")
                            .font(theme::body(10.5))
                            .color(theme::RED),
                    );
                }
            } else {
                ui.label(
                    egui::RichText::new("Select a design in the archive to inspect it.")
                        .font(theme::body(10.5))
                        .color(theme::TEXT_DIM),
                );
            }
        });
    });

    if let Some(template_id) = copy_from {
        if let Some(template) = design_library.get_template(&template_id).cloned() {
            let copied_name = format!("{} Copy", template.name);
            design_library.save_template(ShipDesignTemplate {
                id: uuid::Uuid::new_v4(),
                name: copied_name.clone(),
                hull_id: template.hull_id,
                modules: template.modules,
                version: design_library.latest_version(&copied_name) + 1,
                parent_template_id: Some(template_id),
                created_at_game_time: template.created_at_game_time,
                construction_mode: template.construction_mode,
            });
        }
    }

    if let Some(template_id) = delete_selected {
        design_library.templates.remove(&template_id);
        if ui_state.selected_template_id == Some(template_id) {
            ui_state.selected_template_id = None;
        }
        if ui_state.construction_design_id == Some(template_id) {
            ui_state.construction_design_id = None;
        }
    }

    if let Some(template_id) = open_for_edit {
        if let Some(template) = design_library.get_template(&template_id) {
            load_template_into_ui(ui_state, shipbuilding_data, research_state, template);
            ui_state.active_tab = ShipbuildingTab::Design;
        }
    }

    if let Some(template_id) = create_new_from {
        if let Some(template) = design_library.get_template(&template_id) {
            load_template_into_ui(ui_state, shipbuilding_data, research_state, template);
            ui_state.selected_template_id = None;
            ui_state.design_name = format!("{} Variant", template.name);
            ui_state.active_tab = ShipbuildingTab::Design;
        }
    }
}

fn draw_construction_tab(
    ui: &mut egui::Ui,
    colonies: &ShipyardColonyQuery,
    projects: &Query<(Entity, &ShipConstructionProject)>,
    design_library: &ShipDesignLibrary,
    shipbuilding_data: &crate::shipbuilding::ShipbuildingData,
    research_state: &crate::research::ResearchState,
    shipbuilding_actions: &mut ResMut<crate::shipbuilding::PendingShipbuildingActions>,
    ui_state: &mut ShipbuildingUiState,
    launch_state: &crate::shipbuilding::LaunchCapacityState,
    budget: &GlobalBudget,
) {
    let browser_rows = build_design_browser_rows(design_library, shipbuilding_data, research_state);
    let selected_colony = ui_state
        .selected_colony
        .and_then(|entity| colonies.get(entity).ok());

    let queue_width = (ui.available_width() * 0.42).clamp(340.0, 520.0);
    let facilities_width = (ui.available_width() - queue_width - 12.0).max(420.0);

    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(queue_width, ui.available_height()),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                theme::elevated_frame().show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("Design Queueing")
                            .font(theme::heading())
                            .color(theme::TEXT_VALUE),
                    );
                    ui.label(
                        egui::RichText::new(
                            "Select a colony, pick a saved class, and push it into the active construction pipeline.",
                        )
                        .font(theme::body(11.0))
                        .color(theme::TEXT_DIM),
                    );
                    theme::divider(ui);

                    draw_build_site_picker(ui, colonies, ui_state);
                    ui.add_space(8.0);
                    draw_design_selector_for_queue(ui, &browser_rows, ui_state);

                    if let Some(template_id) = ui_state.construction_design_id {
                        if let Some(template) = design_library.get_template(&template_id) {
                            let draft = design_from_template(template);
                            let summary = shipbuilding_data.summarize_design(&draft, research_state);
                            let hull = shipbuilding_data.get_hull(&template.hull_id);
                            let queue_errors = crate::shipbuilding::systems::queue_validation_errors(
                                selected_colony.map(|(_, colony, _, _, _)| colony),
                                hull,
                                summary.as_ref(),
                                template.construction_mode,
                            );

                            ui.add_space(10.0);
                            queue_preview_card(ui, template, summary.as_ref(), &queue_errors);

                            ui.add_space(8.0);
                            if ui
                                .add_enabled(
                                    queue_errors.is_empty(),
                                    egui::Button::new("Queue Selected Design"),
                                )
                                .clicked()
                            {
                                if let Some((build_site, _, _, _, _)) = selected_colony {
                                    shipbuilding_actions.queue_projects.push(
                                        QueueShipConstructionAction {
                                            build_site,
                                            design: draft,
                                        },
                                    );
                                }
                            }

                            for error in queue_errors {
                                ui.label(
                                    egui::RichText::new(error)
                                        .font(theme::body(10.5))
                                        .color(theme::RED),
                                );
                            }
                        }
                    }
                });
            },
        );

        ui.add_space(12.0);
        ui.allocate_ui_with_layout(
            egui::vec2(facilities_width, ui.available_height()),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                theme::elevated_frame().show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("Shipyard Facilities")
                            .font(theme::heading())
                            .color(theme::TEXT_VALUE),
                    );
                    ui.label(
                        egui::RichText::new(
                            "Every colony with shipbuilding capacity, current throughput, launch ceiling, and live project status.",
                        )
                        .font(theme::body(11.0))
                        .color(theme::TEXT_DIM),
                    );
                    theme::divider(ui);

                    draw_facilities_overview(ui, colonies, projects, ui_state, launch_state, budget);
                });
            },
        );
    });
}

fn draw_ships_tab(
    ui: &mut egui::Ui,
    colonies: &ShipyardColonyQuery,
    ships: &ShipInstanceQuery,
    fleets: &FleetRosterQuery,
    fleet_actions: &mut ResMut<PendingFleetActions>,
    ui_state: &mut ShipbuildingUiState,
) {
    let roster = build_ship_roster_rows(colonies, ships, fleets);

    let roster_width = (ui.available_width() * 0.52).clamp(420.0, 700.0);
    let assignment_width = (ui.available_width() - roster_width - 12.0).max(320.0);

    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(roster_width, ui.available_height()),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                theme::elevated_frame().show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("Existing Ships")
                            .font(theme::heading())
                            .color(theme::TEXT_VALUE),
                    );
                    ui.label(
                        egui::RichText::new(
                            "Operational hulls, their current fleet, orbital location, and transfer readiness.",
                        )
                        .font(theme::body(11.0))
                        .color(theme::TEXT_DIM),
                    );
                    theme::divider(ui);

                    draw_ship_roster_table(ui, &roster, ui_state);
                });
            },
        );

        ui.add_space(12.0);
        ui.allocate_ui_with_layout(
            egui::vec2(assignment_width, ui.available_height()),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                theme::elevated_frame().show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("Fleet Assignment")
                            .font(theme::heading())
                            .color(theme::TEXT_VALUE),
                    );
                    ui.label(
                        egui::RichText::new(
                            "Reassign ships between fleets parked at the same body, or stand up a new empty fleet at that location.",
                        )
                        .font(theme::body(11.0))
                        .color(theme::TEXT_DIM),
                    );
                    theme::divider(ui);

                    draw_ship_assignment_panel(ui, &roster, fleets, fleet_actions, ui_state);
                });
            },
        );
    });
}

fn design_controls(
    ui: &mut egui::Ui,
    shipbuilding_data: &crate::shipbuilding::ShipbuildingData,
    research_state: &crate::research::ResearchState,
    ui_state: &mut ShipbuildingUiState,
    available_hulls: &[&crate::shipbuilding::ShipHullDefinition],
) {
    let selected_hull_name = ui_state
        .selected_hull_id
        .as_deref()
        .and_then(|hull_id| shipbuilding_data.get_hull(hull_id))
        .map(|hull| hull.display_name.clone())
        .unwrap_or_else(|| "Select hull".to_string());

    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new("Hull")
                    .font(theme::mono(10.0))
                    .color(theme::TEXT_DIM),
            );
            egui::ComboBox::from_id_salt("designer_hull_select")
                .selected_text(selected_hull_name)
                .width(360.0)
                .show_ui(ui, |ui| {
                    for hull in available_hulls {
                        if ui
                            .selectable_label(
                                ui_state.selected_hull_id.as_deref() == Some(hull.id.as_str()),
                                format!(
                                    "{} {} [{}]",
                                    hull.class.icon(),
                                    hull.display_name,
                                    hull.effective_size_tier().display_name()
                                ),
                            )
                            .clicked()
                        {
                            ui_state.selected_hull_id = Some(hull.id.clone());
                            ui_state.design_name = format!("{} Prototype", hull.display_name);
                            ui_state.selected_modules.clear();
                            ui_state.selected_slot = None;
                            ui_state.selected_mode = hull.default_construction_mode;
                            hydrate_selected_design(ui_state, shipbuilding_data, research_state);
                        }
                    }
                });
        });

        ui.add_space(12.0);
        ui.vertical(|ui| {
            ui.label(
                egui::RichText::new("Design Name")
                    .font(theme::mono(10.0))
                    .color(theme::TEXT_DIM),
            );
            ui.add_sized(
                egui::vec2(ui.available_width().max(220.0), 24.0),
                egui::TextEdit::singleline(&mut ui_state.design_name),
            );
        });
    });

    ui.add_space(6.0);
    ui.label(
        egui::RichText::new("Construction mode is now managed from the Construction tab when you queue a design; the designer only edits the hull configuration.")
            .font(theme::mono(10.0))
            .color(theme::TEXT_DIM),
    );
}

fn draw_build_site_picker(
    ui: &mut egui::Ui,
    colonies: &ShipyardColonyQuery,
    ui_state: &mut ShipbuildingUiState,
) {
    let current_name = ui_state
        .selected_colony
        .and_then(|entity| {
            colonies
                .get(entity)
                .ok()
                .map(|(_, colony, _, _, _)| colony.name.clone())
        })
        .unwrap_or_else(|| "No colony selected".to_string());

    egui::ComboBox::from_label("Build Site")
        .selected_text(current_name)
        .show_ui(ui, |ui| {
            let mut rows: Vec<_> = colonies.iter().collect();
            rows.sort_by(|left, right| left.1.name.cmp(&right.1.name));
            for (entity, colony, _, _, _) in rows {
                let selected = ui_state.selected_colony == Some(entity);
                if ui.selectable_label(selected, colony.name.clone()).clicked() {
                    ui_state.selected_colony = Some(entity);
                }
            }
        });
}

fn draw_hull_editor(
    ui: &mut egui::Ui,
    shipbuilding_data: &crate::shipbuilding::ShipbuildingData,
    research_state: &crate::research::ResearchState,
    ui_state: &mut ShipbuildingUiState,
    hull: &crate::shipbuilding::ShipHullDefinition,
) {
    ui.label(
        egui::RichText::new(&hull.description)
            .font(theme::body(10.5))
            .color(theme::TEXT_DIM),
    );

    if ui_state.selected_slot.is_none() {
        ui_state.selected_slot = hull.slot_layout.first().map(|slot| slot.slot_id.clone());
    }

    ui.add_space(8.0);
    let available_width = ui.available_width();
    let stacked_layout = available_width < 980.0;
    let browser_width = (available_width * 0.30).clamp(260.0, 360.0);
    let schematic_width = (available_width - browser_width - 12.0).max(460.0);

    if stacked_layout {
        draw_component_browser(ui, shipbuilding_data, research_state, ui_state, hull);
        ui.add_space(10.0);
        let clicked_slot = draw_ship_schematic(
            ui,
            shipbuilding_data,
            research_state,
            hull,
            &ui_state.selected_modules,
            ui_state.selected_slot.as_deref(),
        );
        if let Some(slot_id) = clicked_slot {
            ui_state.selected_slot = Some(slot_id);
        }
    } else {
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(browser_width, ui.available_height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    draw_component_browser(ui, shipbuilding_data, research_state, ui_state, hull);
                },
            );

            ui.add_space(12.0);
            ui.allocate_ui_with_layout(
                egui::vec2(schematic_width, ui.available_height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    let clicked_slot = draw_ship_schematic(
                        ui,
                        shipbuilding_data,
                        research_state,
                        hull,
                        &ui_state.selected_modules,
                        ui_state.selected_slot.as_deref(),
                    );
                    if let Some(slot_id) = clicked_slot {
                        ui_state.selected_slot = Some(slot_id);
                    }
                },
            );
        });
    }
}

fn draw_ship_schematic(
    ui: &mut egui::Ui,
    shipbuilding_data: &crate::shipbuilding::ShipbuildingData,
    research_state: &crate::research::ResearchState,
    hull: &crate::shipbuilding::ShipHullDefinition,
    selected_modules: &HashMap<String, String>,
    selected_slot: Option<&str>,
) -> Option<String> {
    let mut clicked = None;
    let mut columns: [Vec<&crate::shipbuilding::HullSlotDefinition>; 3] =
        [Vec::new(), Vec::new(), Vec::new()];

    for slot in &hull.slot_layout {
        let column = match slot.category {
            crate::shipbuilding::ShipModuleCategory::Sensors
            | crate::shipbuilding::ShipModuleCategory::SpecialScience
            | crate::shipbuilding::ShipModuleCategory::ArmorDefense
            | crate::shipbuilding::ShipModuleCategory::PowerThermal => 0,
            crate::shipbuilding::ShipModuleCategory::Weapons
            | crate::shipbuilding::ShipModuleCategory::FuelStorage
            | crate::shipbuilding::ShipModuleCategory::UtilitySupport => 2,
            _ => 1,
        };
        columns[column].push(slot);
    }

    theme::elevated_frame().show(ui, |ui| {
        ui.label(
            egui::RichText::new("Hull Layout")
                .font(theme::mono(10.0))
                .color(theme::TEXT_DIM),
        );
        ui.label(
            egui::RichText::new("Click any section card to focus it, then fit a compatible component from the left panel.")
                .font(theme::body(10.0))
                .color(theme::TEXT_DIM),
        );
        ui.add_space(6.0);

        let lane_titles = ["Defensive / Sensor", "Core Hull", "Weapons / Mission"];
        let lane_spacing = 10.0;
        let lane_width = ((ui.available_width() - lane_spacing * 2.0) / 3.0).max(145.0);

        ui.horizontal_top(|ui| {
            for (index, slots) in columns.into_iter().enumerate() {
                ui.allocate_ui_with_layout(
                    egui::vec2(lane_width, ui.available_height()),
                    egui::Layout::top_down(egui::Align::Center),
                    |ui| {
                        ui.label(
                            egui::RichText::new(lane_titles[index])
                                .font(theme::mono(9.5))
                                .color(theme::TEXT_DIM),
                        );
                        ui.add_space(6.0);

                        for slot in slots {
                            let installed_module = selected_modules
                                .get(&slot.slot_id)
                                .and_then(|module_id| shipbuilding_data.get_module(module_id));
                            let module_name = installed_module.map(|module| module.display_name.as_str());
                            let compatible_modules =
                                shipbuilding_data.compatible_modules_for_slot(slot, research_state);
                            let response = draw_slot_card(
                                ui,
                                lane_width,
                                slot,
                                module_name,
                                selected_slot == Some(slot.slot_id.as_str()),
                            );
                            let content = build_slot_tooltip(slot, installed_module, &compatible_modules);
                            let response =
                                response.on_hover_ui(|ui| render_shipbuilding_tooltip(ui, &content));
                            if response.clicked() {
                                clicked = Some(slot.slot_id.clone());
                            }
                            ui.add_space(8.0);
                        }
                    },
                );

                if index < 2 {
                    ui.add_space(lane_spacing);
                }
            }
        });
    });

    clicked
}

fn draw_component_browser(
    ui: &mut egui::Ui,
    shipbuilding_data: &crate::shipbuilding::ShipbuildingData,
    research_state: &crate::research::ResearchState,
    ui_state: &mut ShipbuildingUiState,
    hull: &crate::shipbuilding::ShipHullDefinition,
) {
    theme::elevated_frame().show(ui, |ui| {
        ui.label(
            egui::RichText::new("Components")
                .font(theme::heading())
                .color(theme::TEXT_VALUE),
        );
        ui.label(
            egui::RichText::new("Focused slot details and compatible fittings for the section selected in the hull schematic.")
            .font(theme::body(10.0))
            .color(theme::TEXT_DIM),
        );
        ui.add_space(6.0);

        let Some(selected_slot_id) = ui_state.selected_slot.as_deref() else {
            ui.label(
                egui::RichText::new("Select a hull section in the schematic to inspect compatible components.")
                    .font(theme::body(10.0))
                    .color(theme::TEXT_DIM),
            );
            return;
        };

        let Some(slot) = hull
            .slot_layout
            .iter()
            .find(|slot| slot.slot_id == selected_slot_id)
        else {
            ui.label(
                egui::RichText::new("Selected section is no longer present on this hull.")
                    .font(theme::body(10.0))
                    .color(theme::RED),
            );
            return;
        };

        let compatible = shipbuilding_data.compatible_modules_for_slot(slot, research_state);
        let compatible_count = compatible.len();
        let selected_text = ui_state
            .selected_modules
            .get(&slot.slot_id)
            .and_then(|module_id| shipbuilding_data.get_module(module_id))
            .map(|module| module.display_name.clone())
            .unwrap_or_else(|| {
                if slot.required {
                    "No module fitted".to_string()
                } else {
                    "Optional section".to_string()
                }
            });

        ui.horizontal_wrapped(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(prettify_slot_name(&slot.slot_id))
                        .font(theme::body(11.0))
                        .color(theme::TEXT_VALUE),
                );
                ui.label(
                    egui::RichText::new(format!(
                        "{} | {}{} | {} fits",
                        slot.category.display_name(),
                        slot.size,
                        if slot.required { "" } else { " | optional" },
                        compatible_count
                    ))
                    .font(theme::body(9.2))
                    .color(theme::TEXT_DIM),
                );
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new(format!("Current fit: {selected_text}"))
                        .font(theme::body(9.8))
                        .color(theme::TEXT_VALUE),
                );
            });

            if !slot.required {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    if ui.button("Clear").clicked() {
                        ui_state.selected_modules.remove(&slot.slot_id);
                    }
                });
            }
        });

        theme::divider(ui);

        ui.label(
            egui::RichText::new(format!("Compatible Components ({compatible_count})"))
                .font(theme::mono(10.0))
                .color(theme::TEXT_DIM),
        );

        if compatible.is_empty() {
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("No unlocked components fit this section yet.")
                    .font(theme::body(10.0))
                    .color(theme::AMBER),
            );
            return;
        }

        egui::ScrollArea::vertical()
            .id_salt(format!("component_browser_{}", slot.slot_id))
            .max_height(ui.available_height().max(260.0))
            .show(ui, |ui| {
                for module in compatible {
                    let selected = ui_state
                        .selected_modules
                        .get(&slot.slot_id)
                        .is_some_and(|module_id| module_id == &module.id);
                    let response = egui::Frame::NONE
                        .fill(if selected {
                            theme::SURFACE_RAISED
                        } else {
                            theme::SURFACE
                        })
                        .stroke(egui::Stroke::new(1.0, theme::BORDER))
                        .inner_margin(egui::Margin::same(6))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(&module.display_name)
                                    .font(theme::body(10.0))
                                    .color(theme::TEXT_VALUE),
                            );
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} | {} BP | {}",
                                    module.size,
                                    module.build_points.round(),
                                    format_mass_compact_tonnes(module.dry_mass_t)
                                ))
                                .font(theme::body(9.0))
                                .color(theme::TEXT_DIM),
                            );
                            ui.label(
                                egui::RichText::new(format_power_profile(module))
                                    .font(theme::mono(8.8))
                                    .color(theme::TEXT_DIM),
                            );
                            if !module.resource_costs.is_empty() {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "Cost: {}",
                                        format_resource_costs_inline(&module.resource_costs, 4)
                                    ))
                                    .font(theme::body(8.8))
                                    .color(theme::TEXT_DIM),
                                );
                            }
                            ui.add_space(2.0);
                            ui.label(
                                egui::RichText::new(if selected { "Fitted" } else { "Click to fit" })
                                    .font(theme::mono(8.6))
                                    .color(if selected {
                                        theme::GREEN
                                    } else {
                                        theme::ACCENT
                                    }),
                            );
                            ui.set_min_height(0.0);
                        })
                        .response;
                    let click_response = ui.interact(
                        response.rect,
                        ui.id()
                            .with("component_fit_card")
                            .with(&slot.slot_id)
                            .with(&module.id),
                        egui::Sense::click(),
                    );
                    let content = build_module_tooltip(module, Some(slot));
                    let click_response = click_response
                        .on_hover_ui(|ui| render_shipbuilding_tooltip(ui, &content));
                    if click_response.clicked() {
                        ui_state
                            .selected_modules
                            .insert(slot.slot_id.clone(), module.id.clone());
                    }
                    ui.add_space(4.0);
                }
            });
    });
}

fn draw_slot_card(
    ui: &mut egui::Ui,
    width: f32,
    slot: &crate::shipbuilding::HullSlotDefinition,
    module_name: Option<&str>,
    selected: bool,
) -> egui::Response {
    let size = egui::vec2(width, 68.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());

    let filled = module_name.is_some();
    let fill = if selected {
        theme::SURFACE_RAISED
    } else if filled {
        theme::SURFACE_INPUT
    } else {
        theme::SURFACE
    };
    let stroke = egui::Stroke::new(
        if selected { 1.5 } else { 1.0 },
        if selected { theme::ACCENT } else { theme::BORDER },
    );
    ui.painter().rect(
        rect,
        4.0,
        fill,
        stroke,
        egui::StrokeKind::Outside,
    );

    let padding = egui::vec2(10.0, 8.0);
    let top_left = rect.left_top() + padding;
    let status_text = module_name.unwrap_or(if slot.required {
        "Required slot empty"
    } else {
        "Optional slot empty"
    });

    ui.painter().text(
        top_left,
        egui::Align2::LEFT_TOP,
        prettify_slot_name(&slot.slot_id),
        theme::body(10.0),
        theme::TEXT_VALUE,
    );
    ui.painter().text(
        top_left + egui::vec2(0.0, 18.0),
        egui::Align2::LEFT_TOP,
        status_text,
        theme::body(8.8),
        if filled { theme::GREEN } else { theme::TEXT_DIM },
    );
    ui.painter().text(
        top_left + egui::vec2(0.0, 36.0),
        egui::Align2::LEFT_TOP,
        format!(
            "{} | {}{}",
            slot.category.display_name(),
            slot.size,
            if slot.required { "" } else { " | optional" }
        ),
        theme::mono(8.2),
        theme::TEXT_DIM,
    );

    response
}

fn draw_current_design_summary(
    ui: &mut egui::Ui,
    selected_hull: Option<&crate::shipbuilding::ShipHullDefinition>,
    summary: Option<&ShipDesignSummary>,
    ui_state: &ShipbuildingUiState,
) {
    let Some(hull) = selected_hull else {
        ui.label(
            egui::RichText::new("Choose a hull to inspect mass, propulsion, and combat metrics.")
                .font(theme::body(10.5))
                .color(theme::TEXT_DIM),
        );
        return;
    };

    let fitted_slots = ui_state.selected_modules.len();
    let total_slots = hull.slot_layout.len();

    let Some(summary) = summary else {
        summary_banner(
            ui,
            &hull.display_name,
            hull.class.display_name(),
            fitted_slots,
            total_slots,
            None,
        );
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("Current draft is incomplete or locked by research.")
                .font(theme::body(10.5))
                .color(theme::RED),
        );
        return;
    };

    summary_banner(
        ui,
        &hull.display_name,
        hull.class.display_name(),
        fitted_slots,
        total_slots,
        Some(summary),
    );
    ui.add_space(8.0);

    metrics_card(
        ui,
        "Design Overview",
        &[
            ("Build", &format!("{:.0} BP", summary.build_points)),
            ("Mass", &format_mass_compact_tonnes(summary.launch_mass_t)),
            ("Delta-V", &format!("{:.0} m/s", summary.delta_v_ms)),
            ("Accel", &format!("{:.3} m/s²", summary.acceleration_ms2)),
            ("Fuel", &format_mass_compact_tonnes(summary.fuel_capacity_t)),
            ("Gen", &format!("{:.1} MW", summary.power_generation_mw)),
            ("Load", &format!("{:.1} MW", summary.power_draw_mw)),
            ("Net", &format!("{:+.1} MW", summary.power_balance_mw())),
        ],
    );

    if !summary.resource_costs.is_empty() {
        ui.add_space(8.0);
        draw_resource_costs_card(ui, "Material Cost", &summary.resource_costs, 6);
    }

    ui.add_space(8.0);
    draw_rating_block(
        ui,
        "Ratings",
        &[
            (
                "Attack",
                combat_score(summary),
                600.0,
                theme::RED,
                format!("{:.1}", combat_score(summary)),
            ),
            (
                "Defense",
                summary.magazine_capacity_t * 8.0 + summary.power_generation_mw * 0.5,
                220.0,
                theme::ACCENT,
                format!(
                    "{:.1}",
                    summary.magazine_capacity_t * 8.0 + summary.power_generation_mw * 0.5
                ),
            ),
            (
                "Mobility",
                summary.delta_v_ms * 0.02 + summary.acceleration_ms2 * 200.0,
                160.0,
                theme::AMBER,
                format!(
                    "{:.1}",
                    summary.delta_v_ms * 0.02 + summary.acceleration_ms2 * 200.0
                ),
            ),
            (
                "Scanners",
                summary.sensor_range_au * 1200.0,
                900.0,
                theme::RP_BLUE,
                format!("{:.1}", summary.sensor_range_au * 1200.0),
            ),
        ],
    );

    ui.add_space(8.0);
    draw_rating_block(
        ui,
        "Energy",
        &[
            (
                "Reactor Output",
                summary.power_generation_mw,
                40.0,
                theme::GREEN,
                format!("{:.1} MW", summary.power_generation_mw),
            ),
            (
                "System Load",
                summary.power_draw_mw,
                40.0,
                theme::AMBER,
                format!("{:.1} MW", summary.power_draw_mw),
            ),
            (
                "Propulsion",
                summary.thrust_kn / 50.0,
                40.0,
                theme::EP_TEAL,
                format!("{:.0} kN", summary.thrust_kn),
            ),
        ],
    );

    ui.add_space(8.0);
    metrics_card(
        ui,
        "Stores / Utility",
        &[
            ("Sensors", &format!("{:.2} AU", summary.sensor_range_au)),
            (
                "Ordnance",
                &format_mass_compact_tonnes(summary.ordnance_capacity_t),
            ),
            (
                "Magazine",
                &format_mass_compact_tonnes(summary.magazine_capacity_t),
            ),
            ("Docking", &format!("{:.0}", summary.docking_ports)),
            ("Cargo", &format_mass_compact_tonnes(summary.cargo_capacity_t)),
            ("Crew", &format!("{:.0}", summary.crew)),
        ],
    );

    if !summary.missing_required_slots.is_empty() {
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(format!(
                "Missing required slots: {}",
                summary.missing_required_slots.join(", ")
            ))
            .font(theme::body(10.5))
            .color(theme::RED),
        );
    }
}

fn draw_design_selector_for_queue(
    ui: &mut egui::Ui,
    rows: &[DesignBrowserRow],
    ui_state: &mut ShipbuildingUiState,
) {
    ui.label(
        egui::RichText::new("Saved Designs")
            .font(theme::mono(10.0))
            .color(theme::TEXT_DIM),
    );

    egui::ScrollArea::vertical()
        .id_salt("queue_design_selector")
        .max_height(360.0)
        .show(ui, |ui| {
            for row in rows {
                let selected = ui_state.construction_design_id == Some(row.template_id);
                let response = egui::Frame::NONE
                    .fill(if selected {
                        theme::SURFACE_RAISED
                    } else {
                        theme::SURFACE
                    })
                    .stroke(egui::Stroke::new(1.0, theme::BORDER))
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(
                                    egui::RichText::new(format!("{} v{}", row.name, row.version))
                                        .font(theme::body(11.0))
                                        .color(theme::TEXT_VALUE),
                                );
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{} | {} | {}",
                                        row.hull_name,
                                        row.construction_mode.display_name(),
                                        format_mass_compact_tonnes(row.summary.launch_mass_t)
                                    ))
                                    .font(theme::body(10.0))
                                    .color(theme::TEXT_DIM),
                                );
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "CV {:.1}",
                                            combat_score(&row.summary)
                                        ))
                                        .font(theme::mono(10.0))
                                        .color(theme::AMBER),
                                    );
                                },
                            );
                        });
                    })
                    .response;

                if response.clicked() {
                    ui_state.construction_design_id = Some(row.template_id);
                }
                ui.add_space(4.0);
            }
        });
}

fn queue_preview_card(
    ui: &mut egui::Ui,
    template: &ShipDesignTemplate,
    summary: Option<&ShipDesignSummary>,
    queue_errors: &[String],
) {
    theme::elevated_frame().show(ui, |ui| {
        ui.label(
            egui::RichText::new(format!("{} v{}", template.name, template.version))
                .font(theme::heading())
                .color(theme::TEXT_VALUE),
        );
        if let Some(summary) = summary {
            ui.add_space(4.0);
            egui::Grid::new("queue_preview_grid")
                .num_columns(2)
                .show(ui, |ui| {
                    theme::stat_row(ui, "Build Cost", &format!("{:.0} BP", summary.build_points));
                    theme::stat_row(
                        ui,
                        "Launch Mass",
                        &format_mass_compact_tonnes(summary.launch_mass_t),
                    );
                    theme::stat_row(ui, "Delta-V", &format!("{:.0} m/s", summary.delta_v_ms));
                    theme::stat_row(ui, "Combat", &format!("{:.1}", combat_score(summary)));
                    theme::stat_row(
                        ui,
                        "Power Balance",
                        &format!("{:+.1} MW", summary.power_balance_mw()),
                    );
                    theme::stat_row(ui, "Crew", &format!("{:.0}", summary.crew));
                });
        }

        if !queue_errors.is_empty() {
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("Queue blockers")
                    .font(theme::mono(10.0))
                    .color(theme::RED),
            );
        }
    });
}

fn draw_facilities_overview(
    ui: &mut egui::Ui,
    colonies: &ShipyardColonyQuery,
    projects: &Query<(Entity, &ShipConstructionProject)>,
    ui_state: &mut ShipbuildingUiState,
    launch_state: &crate::shipbuilding::LaunchCapacityState,
    budget: &GlobalBudget,
) {
    let mut colony_rows: Vec<_> = colonies
        .iter()
        .filter(|(_, colony, _, _, _)| colony.building_count(BuildingType::Shipyard) > 0)
        .collect();
    colony_rows.sort_by(|left, right| left.1.name.cmp(&right.1.name));

    egui::ScrollArea::vertical()
        .id_salt("shipyard_facilities")
        .show(ui, |ui| {
            for (entity, colony, _, stockpile, facility) in colony_rows {
                let selected = ui_state.selected_colony == Some(entity);
                let shipyard_count = colony.building_count(BuildingType::Shipyard) as f64;
                let factory_count = colony.building_count(BuildingType::Factory) as f64;
                let engineering_count = colony.building_count(BuildingType::EngineeringBay) as f64;

                let (total_capacity, slipway_capacity, slipway_count, idle_count, retooling_count) =
                    if let Some(facility) = facility {
                        (
                            facility.total_capacity_bp_per_year(),
                            facility.slipway_capacity_bp_per_year(),
                            facility.slipways.len() as f64,
                            facility.idle_count() as f64,
                            facility.retooling_count() as f64,
                        )
                    } else {
                        let bonus = 1.0 + engineering_count * 0.05;
                        let total =
                            (shipyard_count * 2_500.0 + factory_count * 125.0) * bonus;
                        (total, total / shipyard_count.max(1.0), shipyard_count, shipyard_count, 0.0)
                    };

                let available_launch = launch_state
                    .available_mass_t
                    .get(&entity)
                    .copied()
                    .unwrap_or_else(|| crate::shipbuilding::systems::annual_launch_capacity_t(colony));
                let max_launch = crate::shipbuilding::systems::annual_launch_capacity_t(colony);

                let mut local_projects: Vec<_> = projects
                    .iter()
                    .filter(|(_, project)| project.build_site == entity)
                    .collect();
                local_projects.sort_by(|left, right| left.1.design_name.cmp(&right.1.design_name));

                egui::Frame::NONE
                    .fill(if selected { theme::SURFACE_RAISED } else { theme::SURFACE })
                    .stroke(egui::Stroke::new(1.0, theme::BORDER))
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(
                                    egui::RichText::new(&colony.name)
                                        .font(theme::heading())
                                        .color(theme::TEXT_VALUE),
                                );
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{} shipyards | {:.0} BP/yr total | {:.0} t launch ceiling",
                                        shipyard_count,
                                        total_capacity,
                                        max_launch
                                    ))
                                    .font(theme::body(10.5))
                                    .color(theme::TEXT_DIM),
                                );
                            });

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button("Select Site").clicked() {
                                    ui_state.selected_colony = Some(entity);
                                }
                            });
                        });

                        ui.add_space(6.0);
                        egui::Grid::new(format!("facility_grid_{:?}", entity))
                            .num_columns(2)
                            .show(ui, |ui| {
                                theme::stat_row(ui, "Slipways", &format!("{:.0}", slipway_count));
                                theme::stat_row(ui, "Idle", &format!("{:.0}", idle_count));
                                theme::stat_row(ui, "Retooling", &format!("{:.0}", retooling_count));
                                theme::stat_row(ui, "Per Slipway", &format!("{:.0} BP/yr", slipway_capacity));
                                theme::stat_row(ui, "Available Launch", &format!("{:.0} / {:.0} t", available_launch, max_launch));
                                theme::stat_row(ui, "Treasury", &format_currency(budget.treasury));
                            });

                        if let Some(stockpile) = stockpile {
                            ui.add_space(6.0);
                            ui.label(
                                egui::RichText::new(format!(
                                    "Local stockpile: Fe {:.2} Mt | Al {:.2} Mt | Polymers {:.2} Mt",
                                    stockpile.get(&ResourceType::Iron),
                                    stockpile.get(&ResourceType::Aluminum),
                                    stockpile.get(&ResourceType::Polymers)
                                ))
                                .font(theme::body(10.0))
                                .color(theme::TEXT_DIM),
                            );
                        }

                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new("Current Construction")
                                .font(theme::mono(10.0))
                                .color(theme::TEXT_DIM),
                        );
                        if local_projects.is_empty() {
                            ui.label(
                                egui::RichText::new("No queued ship projects.")
                                    .font(theme::body(10.5))
                                    .color(theme::TEXT_DIM),
                            );
                        } else {
                            for (_, project) in local_projects {
                                project_row(ui, project);
                            }
                        }
                    });
                ui.add_space(6.0);
            }

            if colonies
                .iter()
                .all(|(_, colony, _, _, _)| colony.building_count(BuildingType::Shipyard) == 0)
            {
                ui.label(
                    egui::RichText::new("No operational shipyards found.")
                        .font(theme::body(11.0))
                        .color(theme::TEXT_DIM),
                );
            }
        });
}

fn project_row(ui: &mut egui::Ui, project: &ShipConstructionProject) {
    let status_color = if project.awaiting_resources {
        theme::RED
    } else {
        match project.state {
            ShipConstructionState::Building => theme::AMBER,
            ShipConstructionState::ReadyForLaunch => theme::ACCENT,
            ShipConstructionState::CompletedInOrbit => theme::GREEN,
        }
    };

    egui::Frame::NONE
        .fill(theme::SURFACE_INPUT)
        .stroke(egui::Stroke::new(1.0, theme::BORDER))
        .inner_margin(egui::Margin::same(6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(&project.design_name)
                            .font(theme::body(11.0))
                            .color(theme::TEXT_VALUE),
                    );
                    ui.label(
                        egui::RichText::new(format!(
                            "{} | {} | {:.0} BP",
                            project.construction_mode.display_name(),
                            format_mass_compact_tonnes(project.launch_mass_t),
                            project.required_build_points.round()
                        ))
                        .font(theme::body(10.0))
                        .color(theme::TEXT_DIM),
                    );
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(if project.awaiting_resources {
                            "Awaiting Resources"
                        } else {
                            project.state.label()
                        })
                        .font(theme::mono(10.0))
                        .color(status_color),
                    );
                    ui.label(
                        egui::RichText::new(format!("{:.0}%", project.progress_percent() * 100.0))
                            .font(theme::mono(10.0))
                            .color(theme::TEXT_VALUE),
                    );
                });
            });
        });
}

fn draw_ship_roster_table(
    ui: &mut egui::Ui,
    roster: &[ShipRosterRow],
    ui_state: &mut ShipbuildingUiState,
) {
    egui::ScrollArea::vertical()
        .id_salt("ship_roster_table")
        .show(ui, |ui| {
            for row in roster {
                let selected = ui_state.selected_ship == Some(row.ship_entity);
                egui::Frame::NONE
                    .fill(if selected {
                        theme::SURFACE_RAISED
                    } else {
                        theme::SURFACE
                    })
                    .stroke(egui::Stroke::new(1.0, theme::BORDER))
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if ui
                                .selectable_label(
                                    selected,
                                    format!("{} {}", row.ship_class.icon(), row.ship_name),
                                )
                                .clicked()
                            {
                                ui_state.selected_ship = Some(row.ship_entity);
                                ui_state.assignment_target_fleet = None;
                                if ui_state.new_fleet_name.trim().is_empty() {
                                    ui_state.new_fleet_name = format!("{} Detached", row.ship_name);
                                }
                            }

                            ui.vertical(|ui| {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{} | {} | {}",
                                        row.fleet_name,
                                        row.role
                                            .map(FleetRole::display_name)
                                            .unwrap_or("Independent"),
                                        row.location
                                    ))
                                    .font(theme::body(10.0))
                                    .color(theme::TEXT_DIM),
                                );
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "Fuel {:>3.0}%",
                                            row.fuel_fraction * 100.0
                                        ))
                                        .font(theme::mono(10.0))
                                        .color(
                                            if row.fuel_fraction > 0.5 {
                                                theme::GREEN
                                            } else {
                                                theme::AMBER
                                            },
                                        ),
                                    );
                                    ui.label(
                                        egui::RichText::new(format!("{:.0} m/s", row.delta_v_ms))
                                            .font(theme::mono(10.0))
                                            .color(theme::RP_BLUE),
                                    );
                                    ui.label(
                                        egui::RichText::new(format_mass_compact_tonnes(
                                            row.dry_mass_t,
                                        ))
                                        .font(theme::mono(10.0))
                                        .color(theme::TEXT_VALUE),
                                    );
                                },
                            );
                        });
                    });
                ui.add_space(4.0);
            }

            if roster.is_empty() {
                ui.label(
                    egui::RichText::new("No operational ships found.")
                        .font(theme::body(11.0))
                        .color(theme::TEXT_DIM),
                );
            }
        });
}

fn draw_ship_assignment_panel(
    ui: &mut egui::Ui,
    roster: &[ShipRosterRow],
    fleets: &FleetRosterQuery,
    fleet_actions: &mut ResMut<PendingFleetActions>,
    ui_state: &mut ShipbuildingUiState,
) {
    let Some(ship_entity) = ui_state.selected_ship else {
        ui.label(
            egui::RichText::new(
                "Select a ship from the roster to enable assignment and fleet creation controls.",
            )
            .font(theme::body(10.5))
            .color(theme::TEXT_DIM),
        );
        return;
    };

    let Some(ship_row) = roster.iter().find(|row| row.ship_entity == ship_entity) else {
        ui.label(
            egui::RichText::new("Selected ship is no longer available.")
                .font(theme::body(10.5))
                .color(theme::RED),
        );
        return;
    };

    metrics_card(
        ui,
        "Selected Ship",
        &[
            ("Ship", ship_row.ship_name.as_str()),
            ("Class", ship_row.ship_class.display_name()),
            ("Fleet", ship_row.fleet_name.as_str()),
            (
                "Role",
                ship_row
                    .role
                    .map(FleetRole::display_name)
                    .unwrap_or("Independent"),
            ),
            ("Location", ship_row.location.as_str()),
            ("Mass", &format_mass_compact_tonnes(ship_row.dry_mass_t)),
            ("Delta-V", &format!("{:.0} m/s", ship_row.delta_v_ms)),
        ],
    );

    ui.add_space(8.0);
    if ship_row.in_transit {
        ui.label(
            egui::RichText::new("Ships in transit cannot be reassigned until the fleet is parked.")
                .font(theme::body(10.5))
                .color(theme::AMBER),
        );
        return;
    }

    let candidate_fleets: Vec<_> = fleets
        .iter()
        .filter_map(|(fleet_entity, _fleet, orbit, maneuver)| {
            if Some(fleet_entity) == ship_row.fleet_entity || maneuver.is_some() {
                return None;
            }

            orbit
                .filter(|orbit| orbit.body == ship_row.parked_body)
                .map(|_| fleet_entity)
        })
        .collect();

    let mut unique_candidates = Vec::new();
    for entity in candidate_fleets {
        if !unique_candidates.contains(&entity) {
            unique_candidates.push(entity);
        }
    }

    let selected_target_name = ui_state
        .assignment_target_fleet
        .and_then(|entity| {
            fleets
                .get(entity)
                .ok()
                .map(|(_, fleet, _, _)| fleet.name.clone())
        })
        .unwrap_or_else(|| "Select destination fleet".to_string());

    egui::ComboBox::from_label("Assign To Fleet")
        .selected_text(selected_target_name)
        .show_ui(ui, |ui| {
            for entity in unique_candidates {
                if let Ok((_, fleet, _, _)) = fleets.get(entity) {
                    ui.selectable_value(
                        &mut ui_state.assignment_target_fleet,
                        Some(entity),
                        fleet.name.clone(),
                    );
                }
            }
        });

    if ui
        .add_enabled(
            ui_state.assignment_target_fleet.is_some(),
            egui::Button::new("Assign To Fleet"),
        )
        .clicked()
    {
        if let Some(destination_fleet) = ui_state.assignment_target_fleet {
            fleet_actions.assign_ships.push(AssignShipsAction {
                ship_entities: vec![ship_entity],
                destination_fleet: Some(destination_fleet),
            });
        }
    }

    if ui
        .add_enabled(
            ship_row.fleet_entity.is_some(),
            egui::Button::new("Detach To Independent Orbit"),
        )
        .clicked()
    {
        fleet_actions.assign_ships.push(AssignShipsAction {
            ship_entities: vec![ship_entity],
            destination_fleet: None,
        });
    }

    ui.add_space(10.0);
    ui.label(
        egui::RichText::new("Create New Fleet")
            .font(theme::mono(10.0))
            .color(theme::TEXT_DIM),
    );
    ui.text_edit_singleline(&mut ui_state.new_fleet_name);

    let can_create_fleet = !ui_state.new_fleet_name.trim().is_empty();
    if ui
        .add_enabled(
            can_create_fleet,
            egui::Button::new("Create Fleet From Ship"),
        )
        .clicked()
    {
        fleet_actions
            .create_fleets_from_ships
            .push(CreateFleetFromShipsAction {
                name: ui_state.new_fleet_name.trim().to_string(),
                orbit_body: ship_row.parked_body,
                orbit_radius_au: ship_row.parked_orbit_radius_au,
                stationary: ship_row.stationary,
                ship_entities: vec![ship_entity],
            });
    }

    ui.label(
        egui::RichText::new(
            "Ships can now exist independently of fleets. Assign them directly, detach them back to independent orbit, or spin up a new fleet from the selected hull.",
        )
        .font(theme::body(10.0))
        .color(theme::TEXT_DIM),
    );
}

fn ensure_defaults(
    ui_state: &mut ShipbuildingUiState,
    colonies: &ShipyardColonyQuery,
    available_hulls: &[&crate::shipbuilding::ShipHullDefinition],
    design_library: &ShipDesignLibrary,
) {
    if ui_state.selected_colony.is_none() {
        let mut rows: Vec<_> = colonies.iter().collect();
        rows.sort_by(|left, right| left.1.name.cmp(&right.1.name));
        ui_state.selected_colony = rows.first().map(|(entity, _, _, _, _)| *entity);
    }

    if ui_state.selected_hull_id.is_none() {
        if let Some(hull) = available_hulls.first() {
            ui_state.selected_hull_id = Some(hull.id.clone());
            ui_state.design_name = format!("{} Prototype", hull.display_name);
            ui_state.selected_mode = hull.default_construction_mode;
        }
    }

    if ui_state.construction_design_id.is_none() {
        ui_state.construction_design_id = design_library
            .all_templates()
            .first()
            .map(|template| template.id);
    }
}

fn hydrate_selected_design(
    ui_state: &mut ShipbuildingUiState,
    shipbuilding_data: &crate::shipbuilding::ShipbuildingData,
    research_state: &crate::research::ResearchState,
) {
    let Some(hull_id) = ui_state.selected_hull_id.as_deref() else {
        return;
    };
    let Some(hull) = shipbuilding_data.get_hull(hull_id) else {
        return;
    };

    for slot in &hull.slot_layout {
        if ui_state.selected_modules.contains_key(&slot.slot_id) {
            continue;
        }

        let compatible = shipbuilding_data.compatible_modules_for_slot(slot, research_state);
        if let Some(module) = compatible.first() {
            ui_state
                .selected_modules
                .insert(slot.slot_id.clone(), module.id.clone());
        }
    }
}

fn build_current_design(ui_state: &ShipbuildingUiState) -> ShipDesignDraft {
    let mut modules: Vec<_> = ui_state
        .selected_modules
        .iter()
        .map(|(slot_id, module_id)| ShipModuleSelection {
            slot_id: slot_id.clone(),
            module_id: module_id.clone(),
        })
        .collect();
    modules.sort_by(|left, right| left.slot_id.cmp(&right.slot_id));

    ShipDesignDraft {
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

fn save_current_design_template(
    design_library: &mut ShipDesignLibrary,
    ui_state: &ShipbuildingUiState,
) {
    let design = build_current_design(ui_state);
    let template = ShipDesignTemplate {
        id: uuid::Uuid::new_v4(),
        name: design.name,
        hull_id: design.hull_id,
        modules: design.modules,
        version: design_library.latest_version(ui_state.design_name.trim()) + 1,
        parent_template_id: None,
        created_at_game_time: 0.0,
        construction_mode: design.construction_mode,
    };
    design_library.save_template(template);
}

fn load_template_into_ui(
    ui_state: &mut ShipbuildingUiState,
    shipbuilding_data: &crate::shipbuilding::ShipbuildingData,
    research_state: &crate::research::ResearchState,
    template: &ShipDesignTemplate,
) {
    ui_state.selected_template_id = Some(template.id);
    ui_state.selected_hull_id = Some(template.hull_id.clone());
    ui_state.design_name = template.name.clone();
    ui_state.selected_mode = template.construction_mode;
    ui_state.selected_modules = template
        .modules
        .iter()
        .map(|selection| (selection.slot_id.clone(), selection.module_id.clone()))
        .collect();
    hydrate_selected_design(ui_state, shipbuilding_data, research_state);
}

fn build_design_browser_rows(
    design_library: &ShipDesignLibrary,
    shipbuilding_data: &crate::shipbuilding::ShipbuildingData,
    research_state: &crate::research::ResearchState,
) -> Vec<DesignBrowserRow> {
    let mut rows = Vec::new();
    for template in design_library.all_templates() {
        let draft = design_from_template(template);
        if let Some(summary) = shipbuilding_data.summarize_design(&draft, research_state) {
            let hull_name = shipbuilding_data
                .get_hull(&template.hull_id)
                .map(|hull| hull.display_name.clone())
                .unwrap_or_else(|| template.hull_id.clone());
            rows.push(DesignBrowserRow {
                template_id: template.id,
                name: template.name.clone(),
                version: template.version,
                hull_name,
                hull_class: summary.ship_class,
                summary,
                construction_mode: template.construction_mode,
            });
        }
    }
    rows
}

fn design_from_template(template: &ShipDesignTemplate) -> ShipDesignDraft {
    ShipDesignDraft {
        name: template.name.clone(),
        hull_id: template.hull_id.clone(),
        modules: template.modules.clone(),
        construction_mode: template.construction_mode,
    }
}

fn compare_design_rows(
    left: &DesignBrowserRow,
    right: &DesignBrowserRow,
    sort: DesignSort,
) -> std::cmp::Ordering {
    match sort {
        DesignSort::HullType => left
            .hull_class
            .display_name()
            .cmp(right.hull_class.display_name())
            .then_with(|| left.name.cmp(&right.name)),
        DesignSort::DeltaV => left
            .summary
            .delta_v_ms
            .partial_cmp(&right.summary.delta_v_ms)
            .unwrap_or(std::cmp::Ordering::Equal),
        DesignSort::Combat => combat_score(&left.summary)
            .partial_cmp(&combat_score(&right.summary))
            .unwrap_or(std::cmp::Ordering::Equal),
        DesignSort::Weight => left
            .summary
            .launch_mass_t
            .partial_cmp(&right.summary.launch_mass_t)
            .unwrap_or(std::cmp::Ordering::Equal),
    }
}

fn combat_score(summary: &ShipDesignSummary) -> f64 {
    summary.ordnance_capacity_t * 12.0
        + summary.magazine_capacity_t * 6.0
        + summary.sensor_range_au * 20.0
        + summary.thrust_kn * 0.01
        + summary.power_generation_mw.max(0.0) * 0.05
}

fn build_ship_roster_rows(
    colonies: &ShipyardColonyQuery,
    ships: &ShipInstanceQuery,
    fleets: &FleetRosterQuery,
) -> Vec<ShipRosterRow> {
    let mut rows = Vec::new();
    for (ship_entity, ship) in ships.iter() {
        let (fleet_entity, fleet_name, role, location, in_transit) =
            if let Some(fleet_entity) = ship.assigned_fleet {
                if let Ok((_, fleet, orbit, maneuver)) = fleets.get(fleet_entity) {
                    let location = if let Some(orbit) = orbit {
                        colonies
                            .get(orbit.body)
                            .map(|(_, colony, _, _, _)| format!("Orbiting {}", colony.name))
                            .unwrap_or_else(|_| "Parked in orbit".to_string())
                    } else if let Some(maneuver) = maneuver {
                        colonies
                            .get(maneuver.destination_body)
                            .map(|(_, colony, _, _, _)| format!("In transit to {}", colony.name))
                            .unwrap_or_else(|_| "In transit".to_string())
                    } else {
                        "Unknown".to_string()
                    };

                    (
                        Some(fleet_entity),
                        fleet.name.clone(),
                        Some(fleet.role),
                        location,
                        maneuver.is_some(),
                    )
                } else {
                    (
                        None,
                        "Independent".to_string(),
                        None,
                        "Independent orbit".to_string(),
                        false,
                    )
                }
            } else {
                let location = colonies
                    .get(ship.parked_body)
                    .map(|(_, colony, _, _, _)| format!("Holding orbit at {}", colony.name))
                    .unwrap_or_else(|_| "Independent orbit".to_string());

                (None, "Independent".to_string(), None, location, false)
            };

        rows.push(ShipRosterRow {
            ship_entity,
            fleet_entity,
            fleet_name,
            ship_name: ship.info.name.clone(),
            ship_class: ship.info.class,
            dry_mass_t: ship.info.dry_mass_t as f64,
            delta_v_ms: ship.info.delta_v_ms(),
            fuel_fraction: ship.info.fuel_fraction(),
            role,
            location,
            parked_body: ship.parked_body,
            parked_orbit_radius_au: ship.parked_orbit_radius_au,
            stationary: ship.stationary,
            in_transit,
        });
    }

    rows.sort_by(|left, right| {
        left.location
            .cmp(&right.location)
            .then_with(|| left.fleet_name.cmp(&right.fleet_name))
            .then_with(|| left.ship_name.cmp(&right.ship_name))
    });
    rows
}

fn stat_chip(ui: &mut egui::Ui, label: &str, value: String, color: egui::Color32) {
    ui.allocate_ui_with_layout(
        egui::vec2(112.0, 44.0),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            egui::Frame::NONE
                .fill(theme::SURFACE)
                .stroke(egui::Stroke::new(1.0, theme::BORDER))
                .inner_margin(egui::Margin::same(6))
                .show(ui, |ui| {
                    ui.set_min_size(egui::vec2(100.0, 32.0));
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new(label)
                                .font(theme::mono(9.0))
                                .color(theme::TEXT_DIM),
                        );
                        ui.label(
                            egui::RichText::new(value)
                                .font(theme::heading())
                                .color(color),
                        );
                    });
                });
        },
    );
}

fn metrics_card(ui: &mut egui::Ui, title: &str, rows: &[(&str, &str)]) {
    theme::elevated_frame().show(ui, |ui| {
        ui.label(
            egui::RichText::new(title)
                .font(theme::mono(10.0))
                .color(theme::TEXT_DIM),
        );
        ui.add_space(4.0);
        egui::Grid::new(title).num_columns(2).show(ui, |ui| {
            for (label, value) in rows {
                theme::stat_row(ui, label, value);
            }
        });
    });
}

fn summary_banner(
    ui: &mut egui::Ui,
    hull_name: &str,
    hull_type: &str,
    fitted_slots: usize,
    total_slots: usize,
    summary: Option<&ShipDesignSummary>,
) {
    theme::elevated_frame().show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(hull_name)
                        .font(theme::heading())
                        .color(theme::TEXT_VALUE),
                );
                ui.label(
                    egui::RichText::new(hull_type)
                        .font(theme::body(10.0))
                        .color(theme::TEXT_DIM),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let completeness = format!("{}/{} fitted", fitted_slots, total_slots);
                ui.label(
                    egui::RichText::new(completeness)
                        .font(theme::mono(9.0))
                        .color(theme::ACCENT),
                );
            });
        });

        ui.add_space(6.0);
        let ratio = if total_slots == 0 {
            0.0
        } else {
            fitted_slots as f32 / total_slots as f32
        };
        ui.add(
            egui::ProgressBar::new(ratio)
                .desired_width(ui.available_width())
                .fill(theme::ACCENT)
                .show_percentage(),
        );

        if let Some(summary) = summary {
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                mini_metric(ui, "Combat", format!("{:.1}", combat_score(summary)), theme::AMBER);
                mini_metric(ui, "Delta-V", format!("{:.0} m/s", summary.delta_v_ms), theme::EP_TEAL);
                mini_metric(ui, "Power", format!("{:+.1} MW", summary.power_balance_mw()), theme::GREEN);
                mini_metric(ui, "Sensors", format!("{:.2} AU", summary.sensor_range_au), theme::RP_BLUE);
            });
        }
    });
}

fn mini_metric(ui: &mut egui::Ui, label: &str, value: String, color: egui::Color32) {
    egui::Frame::NONE
        .fill(theme::SURFACE_INPUT)
        .stroke(egui::Stroke::new(1.0, theme::BORDER))
        .inner_margin(egui::Margin::same(6))
        .show(ui, |ui| {
            ui.set_min_width(68.0);
            ui.label(
                egui::RichText::new(label)
                    .font(theme::mono(8.2))
                    .color(theme::TEXT_DIM),
            );
            ui.label(
                egui::RichText::new(value)
                    .font(theme::body(10.5))
                    .color(color),
            );
        });
}

fn draw_rating_block(
    ui: &mut egui::Ui,
    title: &str,
    rows: &[(&str, f64, f64, egui::Color32, String)],
) {
    theme::elevated_frame().show(ui, |ui| {
        ui.label(
            egui::RichText::new(title)
                .font(theme::mono(10.0))
                .color(theme::TEXT_DIM),
        );
        ui.add_space(4.0);
        for (label, value, scale_max, color, display) in rows {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(*label)
                        .font(theme::body(9.4))
                        .color(theme::TEXT_VALUE),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(display)
                            .font(theme::mono(8.8))
                            .color(*color),
                    );
                });
            });
            let normalized = if *scale_max <= 0.0 {
                0.0
            } else {
                (*value / *scale_max).clamp(0.0, 1.0) as f32
            };
            ui.add(
                egui::ProgressBar::new(normalized)
                    .desired_width(ui.available_width())
                    .fill(*color),
            );
            ui.add_space(4.0);
        }
    });
}

fn format_power_profile(module: &crate::shipbuilding::ShipModuleDefinition) -> String {
    match (module.power_generation_mw > 0.0, module.power_draw_mw > 0.0) {
        (true, true) => format!(
            "Power +{:.1} MW / -{:.1} MW",
            module.power_generation_mw, module.power_draw_mw
        ),
        (true, false) => format!("Power +{:.1} MW", module.power_generation_mw),
        (false, true) => format!("Power -{:.1} MW", module.power_draw_mw),
        (false, false) => "Power 0.0 MW".to_string(),
    }
}

fn format_resource_costs_inline(costs: &[(ResourceType, f64)], max_items: usize) -> String {
    let mut parts = Vec::new();
    for (index, (resource, amount)) in costs.iter().enumerate() {
        if index >= max_items {
            parts.push(format!("+{} more", costs.len() - max_items));
            break;
        }
        parts.push(format!("{} {:.1}", resource.display_name(), amount));
    }
    parts.join(" | ")
}

fn draw_resource_costs_card(
    ui: &mut egui::Ui,
    title: &str,
    costs: &[(ResourceType, f64)],
    max_rows: usize,
) {
    theme::elevated_frame().show(ui, |ui| {
        ui.label(
            egui::RichText::new(title)
                .font(theme::mono(10.0))
                .color(theme::TEXT_DIM),
        );
        ui.add_space(4.0);

        egui::Grid::new(title).num_columns(2).show(ui, |ui| {
            for (resource, amount) in costs.iter().take(max_rows) {
                theme::stat_row(ui, resource.display_name(), &format!("{amount:.1}"));
            }

            if costs.len() > max_rows {
                theme::stat_row(
                    ui,
                    "Additional",
                    &format!("{} more entries", costs.len() - max_rows),
                );
            }
        });
    });
}
