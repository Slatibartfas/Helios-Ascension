use super::*;

/// UI state for the construction panel (persists across frames)
#[derive(Resource, Debug, Clone)]
pub struct ConstructionUiState {
    /// Build multiplier: how many copies to queue at once
    pub build_multiplier: u32,
    /// Currently selected colony entity (None = auto-select first)
    pub selected_colony: Option<bevy::ecs::entity::Entity>,
}

impl Default for ConstructionUiState {
    fn default() -> Self {
        Self {
            build_multiplier: 1,
            selected_colony: None,
        }
    }
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
    buildings_data: Option<Res<BuildingsData>>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut ui_state: ResMut<ConstructionUiState>,
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
            &contextual,
            &mut debug_settings,
            buildings_data.as_deref(),
            &mut ui_state,
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
    contextual: &crate::economy::ContextualStockpile,
    debug_settings: &mut ConstructionDebugSettings,
    buildings_data: Option<&BuildingsData>,
    ui_state: &mut ConstructionUiState,
) {
    ui.heading("Construction");
    ui.separator();

    // -- Debug panel --
    if debug_settings.enabled {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("DEBUG MODE")
                        .strong()
                        .color(theme::RED),
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
        });
        ui.separator();
    }

    // -- Treasury / balance row --
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("Treasury: {}", format_currency(budget.treasury)))
                .size(13.0)
                .color(theme::GOLD),
        );
        ui.separator();
        let balance = budget.balance_per_year();
        let balance_color = if balance >= 0.0 { theme::GREEN } else { theme::RED };
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
        ui.label(egui::RichText::new("Colony:").strong());
        egui::ComboBox::from_id_salt("colony_selector")
            .selected_text(&current_name)
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
    ui.separator();

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

    egui::ScrollArea::vertical().show(ui, |ui| {
        // -- Colony stats --
        ui.group(|ui| {
            ui.label(
                egui::RichText::new(format!("{} -- {}", colony.name, body.name))
                    .size(14.0)
                    .strong(),
            );
            ui.separator();

            let workforce_eff = colony.workforce_efficiency();
            let wf_color = if workforce_eff >= 1.0 {
                theme::GREEN
            } else if workforce_eff >= 0.5 {
                theme::AMBER
            } else {
                theme::RED
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
                            .color(theme::RED),
                    );
                }
            });

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
                    egui::RichText::new(format!("{:.0}%", efficiency * 100.0)).color(eff_color),
                );
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
                ui.label(format!(
                    "Growth: +{}/year",
                    Colony::format_population(growth)
                ));
            }

            let income = colony.wealth_generation_per_year();
            let cost = colony.operating_cost_per_year();
            if income > 0.0 || cost > 0.0 {
                let colony_balance = income - cost;
                let cb_color = if colony_balance >= 0.0 { theme::GREEN } else { theme::RED };
                let sign = if colony_balance >= 0.0 { "+" } else { "" };
                ui.horizontal(|ui| {
                    ui.label(format!("Income: {}/yr", format_currency(income)));
                    ui.label(format!("| Cost: {}/yr", format_currency(cost)));
                    ui.label(
                        egui::RichText::new(format!(
                            "| Net: {}{}/yr",
                            sign,
                            format_currency(colony_balance)
                        ))
                        .color(cb_color),
                    );
                });
            }

            ui.label(format!("Buildings: {}", colony.total_buildings()));
        });

        ui.add_space(4.0);

        // -- Build multiplier selector --
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
                );
                if ui.add(btn).clicked() {
                    ui_state.build_multiplier = mult;
                }
            }
        });

        ui.separator();

        // -- Existing buildings (collapsible) --
        if colony.total_buildings() > 0 {
            egui::CollapsingHeader::new("Buildings")
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
                                    "  {} {} x {} (workers: {})",
                                    building.icon(),
                                    building.display_name(),
                                    count,
                                    workers
                                );
                                if let Some(data) = buildings_data {
                                    let maint = data.maintenance_resources(&building);
                                    if !maint.is_empty() {
                                        let maint_str: Vec<_> = maint
                                            .iter()
                                            .map(|(r, a)| {
                                                format!("{:.1} {}/yr", a * count as f64, r)
                                            })
                                            .collect();
                                        label_text +=
                                            &format!(" [maint: {}]", maint_str.join(", "));
                                    }
                                }
                                ui.label(&label_text);
                            }
                        }
                    }
                });
        }

        // -- Construction queue --
        let factories = colony.building_count(BuildingType::Factory) as f64;
        let bp_rate = 1.0 + factories * 10.0;

        let queue: Vec<_> = construction_query
            .iter()
            .filter(|(_, p)| p.colony_entity == *colony_entity)
            .collect();

        if !queue.is_empty() {
            egui::CollapsingHeader::new(
                egui::RichText::new(format!("Queue ({})", queue.len())).strong(),
            )
            .default_open(true)
            .show(ui, |ui| {
                for (proj_entity, project) in &queue {
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
                        let time_str = if years_remaining < 1.0 {
                            format!("{:.1} mo", years_remaining * 12.0)
                        } else {
                            format!("{:.1} yr", years_remaining)
                        };
                        ui.label(egui::RichText::new(&time_str).size(11.0).color(theme::AMBER));
                        if ui
                            .small_button("X")
                            .on_hover_text("Cancel construction")
                            .clicked()
                        {
                            construction_actions.cancel_construction.push(*proj_entity);
                        }
                    });

                    let bar = egui::ProgressBar::new(pct)
                        .show_percentage()
                        .desired_width(ui.available_width() - 8.0);
                    ui.add(bar);
                    ui.add_space(2.0);
                }
            });
        }

        ui.add_space(4.0);

        // -- Build section --
        egui::CollapsingHeader::new(
            egui::RichText::new(format!("Build  ({:.1} BP/yr)", bp_rate)).strong(),
        )
        .default_open(true)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("Output: {:.1} BP/year", bp_rate))
                        .color(theme::GREEN)
                        .strong(),
                );
                ui.label(egui::RichText::new("i").small())
                    .on_hover_text("Base: 1 BP/yr + 10 BP/yr per Factory");
            });
            ui.separator();

            let multiplier = ui_state.build_multiplier;

            for category in BuildingCategory::all() {
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

                if available.is_empty() {
                    continue;
                }

                egui::CollapsingHeader::new(
                    egui::RichText::new(format!(
                        "{} ({})",
                        category.display_name(),
                        available.len()
                    ))
                    .size(13.0)
                    .strong(),
                )
                .default_open(true)
                .show(ui, |ui| {
                    let chunks: Vec<&[BuildingType]> = available.chunks(3).collect();
                    for chunk in chunks {
                        let card_width = (ui.available_width() - 24.0) / 3.0;
                        ui.columns(3, |cols| {
                            for (i, building) in chunk.iter().enumerate() {
                                let costs = buildings_data
                                    .map(|d| d.resource_costs(building))
                                    .unwrap_or(&[]);
                                let can_afford = free_build
                                    || costs.is_empty()
                                    || can_afford_resources_multiplied(contextual, costs, multiplier);

                                render_building_card(
                                    &mut cols[i],
                                    *building,
                                    multiplier,
                                    *colony_entity,
                                    costs,
                                    contextual,
                                    bp_rate,
                                    can_afford,
                                    construction_actions,
                                    card_width,
                                );
                            }
                        });
                        ui.add_space(4.0);
                    }
                });

                ui.add_space(2.0);
            }

            // -- Locked buildings --
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
                    egui::CollapsingHeader::new(
                        egui::RichText::new(format!("Locked ({})", locked.len()))
                            .size(13.0)
                            .color(theme::TEXT_DIM),
                    )
                    .default_open(false)
                    .show(ui, |ui| {
                        for building in locked {
                            let tech_id = buildings_data
                                .and_then(|d| d.required_tech(building))
                                .or_else(|| building.required_tech());
                            if let Some(tech_name) = tech_id {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "  {} {} -- requires: {}",
                                        building.icon(),
                                        building.display_name(),
                                        tech_name
                                    ))
                                    .size(11.0)
                                    .color(theme::TEXT_HINT),
                                );
                            }
                        }
                    });
                }
            }
        });
    });
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

/// Minimum card height for construction building cards.
/// This is tall enough to fit: header (2 lines) + stats (2 lines) + up to 4
/// resource cost rows + queue button without resizing when fewer rows are shown.
const BUILDING_CARD_MIN_HEIGHT: f32 = 195.0;
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
    _card_width: f32,
) {
    let total_bp = building.build_cost() * multiplier as f64;
    let years_to_build = if bp_rate > 0.0 { total_bp / bp_rate } else { f64::INFINITY };

    ui.group(|ui| {
        ui.set_min_width(170.0);
        ui.set_min_height(BUILDING_CARD_MIN_HEIGHT);

        // Icon + name header (fixed 2-line header area)
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(building.icon()).size(22.0));
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(building.display_name()).strong().size(12.0));
                // Wrap description to 2 lines
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(building.description())
                            .small()
                            .color(theme::TEXT_DIM),
                    )
                    .wrap(),
                );
            });
        });

        ui.separator();

        // Build stats: BP + workers on one line, build-time on next
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!("{:.0} BP", total_bp)).size(10.0));
            ui.separator();
            ui.label(
                egui::RichText::new(format!(
                    "👷 {}",
                    building.workforce_required() * multiplier
                ))
                .size(10.0),
            );
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

        // Resource costs — icon + name + need/available
        ui.separator();
        if costs.is_empty() {
            ui.label(egui::RichText::new("No materials required").size(10.0).color(theme::TEXT_DIM));
        } else {
            for (r, a) in costs {
                let total_needed = a * multiplier as f64;
                let rt_opt = crate::colony::data::parse_resource_type(r);
                let available = rt_opt.map(|rt| contextual.get(&rt)).unwrap_or(0.0);
                let ok = available >= total_needed;
                let color = if ok { theme::GREEN } else { theme::RED };
                let icon = rt_opt
                    .as_ref()
                    .map(|rt| super::resources_bar::get_resource_icon(rt))
                    .unwrap_or("?");
                // Format amounts: compact thousands
                let need_str = format_compact(total_needed);
                let avail_str = format_compact(available);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(icon).size(11.0));
                    ui.label(
                        egui::RichText::new(format!("{} {}/{}", r, need_str, avail_str))
                            .size(10.0)
                            .color(color),
                    );
                });
            }
        }

        // Push Queue button to bottom
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
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

/// Compact number formatter: 1500 → "1.5k", 1_000_000 → "1.0M"
fn format_compact(v: f64) -> String {
    if v >= 1_000_000.0 {
        format!("{:.1}M", v / 1_000_000.0)
    } else if v >= 1_000.0 {
        format!("{:.1}k", v / 1_000.0)
    } else {
        format!("{:.0}", v)
    }
}
