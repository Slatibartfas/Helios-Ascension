use super::*;

pub(super) fn ui_construction_panels(
    mut contexts: EguiContexts,
    active_menu: Res<ActiveMenu>,
    colony_query: Query<(Entity, &Colony, &CelestialBody)>,
    construction_query: Query<(Entity, &ConstructionProject)>,
    mut construction_actions: ResMut<PendingConstructionActions>,
    research_state: Res<crate::research::ResearchState>,
    budget: Res<GlobalBudget>,
    mut debug_settings: ResMut<ConstructionDebugSettings>,
    buildings_data: Option<Res<BuildingsData>>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
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

    egui::CentralPanel::default().show(ctx, |ui| {
        render_construction_panel(
            ui,
            &colony_query,
            &construction_query,
            &mut construction_actions,
            &research_state,
            &budget,
            &mut debug_settings,
            buildings_data.as_deref(),
        );
    });
}

/// Render the construction panel showing colonies, buildings, and construction queues.
fn render_construction_panel(
    ui: &mut egui::Ui,
    colony_query: &Query<(Entity, &Colony, &CelestialBody)>,
    construction_query: &Query<(Entity, &ConstructionProject)>,
    construction_actions: &mut ResMut<PendingConstructionActions>,
    research_state: &crate::research::ResearchState,
    budget: &GlobalBudget,
    debug_settings: &mut ConstructionDebugSettings,
    buildings_data: Option<&BuildingsData>,
) {
    ui.heading("Construction");
    ui.separator();

    // Debug mode panel (if enabled)
    if debug_settings.enabled {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("🐛 DEBUG MODE").strong().color(theme::RED));
                ui.label(egui::RichText::new("(Press F12 to toggle)").italics().small());
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.checkbox(&mut debug_settings.free_construction, "Free Construction (no resource costs)");
                ui.checkbox(&mut debug_settings.instant_build, "Instant Build");
                ui.checkbox(&mut debug_settings.bypass_tech_requirements, "Bypass Tech Prerequisites");
            });
            ui.label(egui::RichText::new("⚠ Debug features are for development only")
                .small()
                .italics()
                .color(theme::AMBER));
        });
        ui.separator();
    }

    // Global financial summary
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("💰 Treasury: {}", format_currency(budget.treasury)))
                .size(13.0)
                .color(theme::GOLD),
        );
        ui.separator();
        let balance = budget.balance_per_year();
        let balance_color = if balance >= 0.0 {
            theme::GREEN
        } else {
            theme::RED
        };
        let sign = if balance >= 0.0 { "+" } else { "" };
        ui.label(
            egui::RichText::new(format!("Balance: {}{}/yr", sign, format_currency(balance)))
                .size(13.0)
                .color(balance_color),
        );
    });
    ui.separator();

    let colonies: Vec<_> = colony_query.iter().collect();

    if colonies.is_empty() {
        ui.add_space(20.0);
        ui.label(
            egui::RichText::new("No colonies established yet.")
                .size(14.0)
                .color(theme::TEXT_DIM),
        );
        ui.add_space(10.0);
        ui.label("Send a colony ship to a celestial body to establish a colony.");
        return;
    }

    let bypass_tech = debug_settings.enabled && debug_settings.bypass_tech_requirements;
    let free_build = debug_settings.enabled && debug_settings.free_construction;

    egui::ScrollArea::vertical().show(ui, |ui| {
    // Show each colony
    for (colony_entity, colony, body) in &colonies {
        let header = format!(
            "🏠 {} ({})",
            colony.name,
            Colony::format_population(colony.population)
        );

        egui::CollapsingHeader::new(
            egui::RichText::new(&header).size(14.0).strong(),
        )
        .default_open(true)
        .show(ui, |ui| {
            // Colony overview
            ui.horizontal(|ui| {
                ui.label(format!("Body: {}", body.name));
                ui.separator();
                ui.label(format!("Buildings: {}", colony.total_buildings()));
            });

            // Workforce status
            let workforce_eff = colony.workforce_efficiency();
            let wf_color = if workforce_eff >= 1.0 {
                theme::GREEN
            } else if workforce_eff >= 0.5 {
                theme::AMBER
            } else {
                theme::RED
            };
            ui.horizontal(|ui| {
                ui.label(format!(
                    "👷 Workforce: {} / {}",
                    colony.available_workforce(),
                    colony.total_workforce_demand()
                ));
                ui.label(
                    egui::RichText::new(format!("({:.0}%)", workforce_eff * 100.0))
                        .color(wf_color),
                );
                if workforce_eff < 1.0 {
                    ui.label(
                        egui::RichText::new("understaffed")
                            .size(11.0)
                            .color(theme::RED),
                    );
                }
            });

            // Logistics status
            let efficiency = colony.logistics_efficiency();
            let eff_color = if efficiency >= 1.0 {
                theme::GREEN
            } else if efficiency >= 0.5 {
                theme::AMBER
            } else {
                theme::RED
            };
            ui.horizontal(|ui| {
                ui.label("Logistics:");
                ui.label(
                    egui::RichText::new(format!("{:.0}%", efficiency * 100.0))
                        .color(eff_color),
                );
                if efficiency < 1.0 {
                    ui.label(
                        egui::RichText::new("(build Mass Drivers / Orbital Lifts)")
                            .size(11.0)
                            .color(theme::TEXT_DIM),
                    );
                }
            });

            // Housing
            let housing = colony.housing_capacity();
            let housing_util = if housing > 0.0 {
                (colony.population / housing * 100.0).min(100.0)
            } else {
                0.0
            };
            ui.horizontal(|ui| {
                ui.label(format!(
                    "Housing: {} / {} ({:.0}%)",
                    Colony::format_population(colony.population),
                    Colony::format_population(housing),
                    housing_util
                ));
            });

            // Growth
            let growth = colony.population_growth_per_year(1.0);
            if growth.abs() > 0.1 {
                ui.horizontal(|ui| {
                    ui.label(format!(
                        "Growth: +{}/year",
                        Colony::format_population(growth)
                    ));
                });
            }

            // Colony financials
            let income = colony.wealth_generation_per_year();
            let cost = colony.operating_cost_per_year();
            if income > 0.0 || cost > 0.0 {
                let colony_balance = income - cost;
                let cb_color = if colony_balance >= 0.0 {
                    theme::GREEN
                } else {
                    theme::RED
                };
                ui.horizontal(|ui| {
                    ui.label(format!("💰 Income: {}/yr", format_currency(income)));
                    ui.label(format!("| Cost: {}/yr", format_currency(cost)));
                    let sign = if colony_balance >= 0.0 { "+" } else { "" };
                    ui.label(
                        egui::RichText::new(format!("| Net: {}{}/yr", sign, format_currency(colony_balance)))
                            .color(cb_color),
                    );
                });
            }

            ui.add_space(5.0);

            // Existing buildings by category
            let has_buildings = colony.total_buildings() > 0;
            if has_buildings {
                egui::CollapsingHeader::new("📋 Buildings")
                    .default_open(false)
                    .show(ui, |ui| {
                        for category in BuildingCategory::all() {
                            let buildings_in_cat: Vec<_> = category
                                .buildings()
                                .iter()
                                .filter(|b| colony.building_count(**b) > 0)
                                .map(|b| (*b, colony.building_count(*b)))
                                .collect();

                            if !buildings_in_cat.is_empty() {
                                ui.label(
                                    egui::RichText::new(category.display_name())
                                        .size(12.0)
                                        .strong(),
                                );
                                for (building, count) in buildings_in_cat {
                                    let workers = building.workforce_required() * count;
                                    let mut label_text = format!(
                                        "  {} {} × {} (👷 {})",
                                        building.icon(),
                                        building.display_name(),
                                        count,
                                        workers
                                    );
                                    // Show maintenance in tooltip
                                    if let Some(data) = buildings_data {
                                        let maint = data.maintenance_resources(&building);
                                        if !maint.is_empty() {
                                            let maint_str: Vec<_> = maint
                                                .iter()
                                                .map(|(r, a)| format!("{:.1} {}/yr", a * count as f64, r))
                                                .collect();
                                            label_text += &format!(" [maint: {}]", maint_str.join(", "));
                                        }
                                    }
                                    ui.horizontal(|ui| {
                                        ui.label(&label_text);
                                    });
                                }
                            }
                        }
                    });
            }

            // Construction queue
            let queue: Vec<_> = construction_query
                .iter()
                .filter(|(_, p)| p.colony_entity == *colony_entity)
                .collect();

            if !queue.is_empty() {
                egui::CollapsingHeader::new(format!("🔨 Queue ({})", queue.len()))
                    .default_open(true)
                    .show(ui, |ui| {
                        for (proj_entity, project) in &queue {
                            ui.horizontal(|ui| {
                                let pct = project.progress_percent() * 100.0;
                                ui.label(format!(
                                    "{} {} - {:.0}%",
                                    project.building_type.icon(),
                                    project.building_type.display_name(),
                                    pct
                                ));
                                if ui
                                    .small_button("✖")
                                    .on_hover_text("Cancel construction")
                                    .clicked()
                                {
                                    construction_actions
                                        .cancel_construction
                                        .push(*proj_entity);
                                }
                            });

                            // Progress bar
                            let bar = egui::ProgressBar::new(project.progress_percent())
                                .show_percentage();
                            ui.add(bar);
                        }
                    });
            }

            // Build new buildings
            egui::CollapsingHeader::new("➕ Build")
                .default_open(queue.is_empty() && !has_buildings)
                .show(ui, |ui| {
                    let factories = colony.building_count(BuildingType::Factory) as f64;
                    let bp_rate = 1.0 + factories * 10.0;
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!("Construction Output: {:.1} BP/year", bp_rate))
                                .color(theme::GREEN)
                                .strong(),
                        );
                        ui.label(egui::RichText::new("ℹ").small())
                            .on_hover_text("Base: 1 BP/yr + 10 BP/yr per Factory");
                    });
                    ui.separator();

                    for category in BuildingCategory::all() {
                        let available: Vec<_> = category
                            .buildings()
                            .into_iter()
                            .filter(|b| {
                                if bypass_tech {
                                    return true;
                                }
                                // Check tech prerequisite from data file first, fall back to code
                                let tech_req = buildings_data
                                    .and_then(|d| d.required_tech(b))
                                    .or_else(|| b.required_tech());
                                match tech_req {
                                    Some(tech_id) => research_state.is_unlocked(tech_id),
                                    None => true,
                                }
                            })
                            .collect();

                        if available.is_empty() {
                            continue;
                        }

                        ui.label(
                            egui::RichText::new(category.display_name())
                                .size(12.0)
                                .strong(),
                        );
                        for building in available {
                            let costs = buildings_data
                                .map(|d| d.resource_costs(&building))
                                .unwrap_or(&[]);
                            let can_afford = free_build || costs.is_empty()
                                || can_afford_resources(budget, costs);

                            ui.horizontal(|ui| {
                                // Build resource cost summary
                                let mut cost_parts = Vec::new();
                                cost_parts.push(format!("{:.0} BP", building.build_cost()));
                                cost_parts.push(format!("👷 {}", building.workforce_required()));
                                if !costs.is_empty() {
                                    let res_str: Vec<_> = costs
                                        .iter()
                                        .map(|(r, a)| format!("{:.0} {}", a, r))
                                        .collect();
                                    cost_parts.push(res_str.join(", "));
                                }
                                let label = format!(
                                    "{} {} ({})",
                                    building.icon(),
                                    building.display_name(),
                                    cost_parts.join(" | ")
                                );

                                let button = ui.add_enabled(
                                    can_afford,
                                    egui::Button::new(
                                        egui::RichText::new(&label).size(11.0),
                                    ).small(),
                                );

                                // Build rich tooltip
                                let mut tooltip_text = building.description().to_string();
                                if !costs.is_empty() {
                                    tooltip_text += "\n\n📦 Construction costs:";
                                    for (r, a) in costs {
                                        let available = crate::colony::data::parse_resource_type(r)
                                            .map(|rt| budget.get_stockpile(&rt))
                                            .unwrap_or(0.0);
                                        let status = if available >= *a { "✔" } else { "✘" };
                                        tooltip_text += &format!("\n  {} {:.1} {} (have {:.1})", status, a, r, available);
                                    }
                                }
                                if let Some(data) = buildings_data {
                                    let maint = data.maintenance_resources(&building);
                                    if !maint.is_empty() {
                                        tooltip_text += "\n\n🔧 Maintenance (per year):";
                                        for (r, a) in maint {
                                            tooltip_text += &format!("\n  {:.2} {}", a, r);
                                        }
                                    }
                                    if let Some(def) = data.get(&building) {
                                        if !def.modifiers.is_empty() {
                                            tooltip_text += "\n\n⚡ Effects:";
                                            for m in &def.modifiers {
                                                let sign = if m.value >= 0.0 { "+" } else { "" };
                                                tooltip_text += &format!("\n  {}{:.0}% {}", sign, m.value, m.modifier_type);
                                            }
                                        }
                                    }
                                }

                                let response = button.on_hover_text(&tooltip_text);

                                if !can_afford {
                                    ui.label(
                                        egui::RichText::new("⚠ insufficient resources")
                                            .size(10.0)
                                            .color(theme::RED),
                                    );
                                }

                                if response.clicked() {
                                    construction_actions
                                        .start_construction
                                        .push((*colony_entity, building));
                                }
                            });
                        }
                        ui.add_space(3.0);
                    }

                    // Show locked buildings (unless bypassing)
                    if !bypass_tech {
                        let locked: Vec<_> = BuildingType::all()
                            .iter()
                            .filter(|b| {
                                let tech_req = buildings_data
                                    .and_then(|d| d.required_tech(b))
                                    .or_else(|| b.required_tech());
                                if let Some(tech_id) = tech_req {
                                    !research_state.is_unlocked(tech_id)
                                } else {
                                    false
                                }
                            })
                            .collect();

                        if !locked.is_empty() {
                            ui.add_space(5.0);
                            ui.label(
                                egui::RichText::new("🔒 Locked (requires research)")
                                    .size(12.0)
                                    .color(theme::TEXT_DIM),
                            );
                            for building in locked {
                                let tech_id = buildings_data
                                    .and_then(|d| d.required_tech(building))
                                    .or_else(|| building.required_tech());
                                if let Some(tech_name) = tech_id {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "  {} {} — requires: {}",
                                            building.icon(),
                                            building.display_name(),
                                            tech_name
                                        ))
                                        .size(11.0)
                                        .color(theme::TEXT_HINT),
                                    );
                                }
                            }
                        }
                    }
                });

            ui.separator();
        });
    }
    });
}

