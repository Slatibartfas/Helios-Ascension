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
    mut buildings_data: Option<ResMut<BuildingsData>>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut ui_state: ResMut<ConstructionUiState>,
    mut edit_state: ResMut<crate::colony::BuildingEditState>,
    sim_time: Res<crate::ui::SimulationTime>,
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

    // Building editor dialog (rendered outside CentralPanel so it floats)
    if debug_settings.enabled {
        render_building_editor(
            ctx,
            buildings_data.as_deref_mut(),
            &mut edit_state,
            sim_time.elapsed_seconds(),
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
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Building Editor: right-click a building in the list to open, or use the button below")
                    .small()
                    .color(theme::TEXT_DIM),
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
                                    buildings_data,
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
/// Tall enough for: icon+name (1 row), description (1 line), separator,
/// stats (1 row), build-time (1 row), effects (up to 2 lines), separator,
/// cost rows in 2-column pairs (up to 3 pairs), Queue button.
const BUILDING_CARD_MIN_HEIGHT: f32 = 230.0;

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
    buildings_data: Option<&crate::colony::BuildingsData>,
) {
    let total_bp = building.build_cost() * multiplier as f64;
    let years_to_build = if bp_rate > 0.0 { total_bp / bp_rate } else { f64::INFINITY };

    // Power demand from data file
    let power_demand_mw = buildings_data
        .and_then(|d| d.get(&building))
        .map(|def| def.power_demand_mw)
        .unwrap_or(0.0)
        * multiplier as f64;

    ui.group(|ui| {
        ui.set_min_width(170.0);
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
                            .small()
                            .color(theme::TEXT_DIM),
                    )
                    .wrap()
                    .truncate(),
                );
            });
        });

        ui.separator();

        // ── Build stats ───────────────────────────────────────────────────
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!("{:.0} BP", total_bp)).size(10.0));
            ui.separator();
            ui.label(
                egui::RichText::new(format!("👷 {}", building.workforce_required() * multiplier))
                    .size(10.0),
            );
            if power_demand_mw > 0.0 {
                ui.separator();
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
        let effects = building.effects_summary();
        if !effects.is_empty() {
            ui.separator();
            for line in effects {
                ui.label(
                    egui::RichText::new(format!("▸ {}", line))
                        .size(10.0)
                        .color(theme::GREEN),
                );
            }
        }

        // ── Resource costs in compact 2-per-row pairs ────────────────────
        ui.separator();
        if costs.is_empty() {
            ui.label(
                egui::RichText::new("No materials required")
                    .size(10.0)
                    .color(theme::TEXT_DIM),
            );
        } else {
            // Collect formatted cost entries
            let cost_entries: Vec<(String, egui::Color32)> = costs
                .iter()
                .map(|(r, a)| {
                    let total_needed = a * multiplier as f64;
                    let rt_opt = crate::colony::data::parse_resource_type(r);
                    let available = rt_opt.map(|rt| contextual.get(&rt)).unwrap_or(0.0);
                    let ok = available >= total_needed;
                    let color = if ok { theme::GREEN } else { theme::RED };
                    let icon = rt_opt
                        .as_ref()
                        .map(|rt| super::resources_bar::get_resource_icon(rt))
                        .unwrap_or("?");
                    let need_str = format_compact(total_needed);
                    let avail_str = format_compact(available);
                    (format!("{} {} {}/{}", icon, r, need_str, avail_str), color)
                })
                .collect();

            // Render 2 costs per row to keep cards uniform height
            for pair in cost_entries.chunks(2) {
                ui.horizontal(|ui| {
                    for (text, color) in pair {
                        ui.label(egui::RichText::new(text).size(9.5).color(*color));
                        ui.add_space(4.0);
                    }
                });
            }
        }

        // ── Queue button (pinned to bottom) ───────────────────────────────
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
                        theme::RED
                    } else {
                        theme::GREEN
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

