use super::*;
use super::dashboard::format_mass;

/// Persisted state for the economy panel's selected tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum EconomyTab {
    #[default]
    Overview,
    Resources,
    Colonies,
    Mining,
    PowerGrid,
}

impl From<u8> for EconomyTab {
    fn from(v: u8) -> Self {
        match v {
            0 => EconomyTab::Overview,
            1 => EconomyTab::Resources,
            2 => EconomyTab::Colonies,
            3 => EconomyTab::Mining,
            4 => EconomyTab::PowerGrid,
            _ => EconomyTab::Overview,
        }
    }
}

impl From<EconomyTab> for u8 {
    fn from(t: EconomyTab) -> u8 {
        match t {
            EconomyTab::Overview => 0,
            EconomyTab::Resources => 1,
            EconomyTab::Colonies => 2,
            EconomyTab::Mining => 3,
            EconomyTab::PowerGrid => 4,
        }
    }
}

/// Source classification for economic entries in the hierarchical view.
/// Prepared for future expansion with stations and mining ships.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EconomySourceKind {
    Colony,
    MiningOp,
    // Future: Station, MiningShip
}

/// Snapshot of a body's economic contribution, aggregated per-frame.
#[allow(dead_code)]
struct BodyEconomyEntry {
    #[allow(dead_code)]
    #[allow(dead_code)]
    body_name: String,
    /// Prepared for future use (stations, mining ships)
    #[allow(dead_code)]
    body_type: BodyType,
    source_kind: EconomySourceKind,
    /// Colony data (if colonised)
    colony: Option<ColonySnapshot>,
    /// Standalone mining operations on this body
    mining_ops: Vec<MiningOpSnapshot>,
    /// Resource deposits on this body
    deposits: Vec<(ResourceType, MineralDeposit)>,
    /// Power generators on this body
    generators: Vec<PowerGenSnapshot>,
}

/// Lightweight copy of colony data for the economy UI.
struct ColonySnapshot {
    name: String,
    population: f64,
    growth_per_year: f64,
    housing_capacity: f64,
    total_buildings: u32,
    workforce_efficiency: f64,
    logistics_efficiency: f64,
    income_per_year: f64,
    operating_cost_per_year: f64,
    buildings: Vec<(BuildingType, u32)>,
}

struct MiningOpSnapshot {
    resource_type: ResourceType,
    rate_mt_per_year: f64,
    active: bool,
}

struct PowerGenSnapshot {
    source_type: PowerSourceType,
    output_watts: f64,
}

/// A star system grouping for the hierarchical economy view.
struct StarSystemGroup {
    system_name: String,
    bodies: Vec<BodyEconomyEntry>,
}

/// Build the hierarchical economy data: star systems → bodies → colonies/mining/power.
fn build_economy_hierarchy(
    body_query: &Query<(
        Entity,
        &CelestialBody,
        Option<&SystemId>,
        Option<&Colony>,
        Option<&PlanetResources>,
        Option<&crate::economy::components::PowerGenerator>,
        Option<&MiningOperation>,
    )>,
    star_query: &Query<(&CelestialBody, &SystemId), With<crate::plugins::solar_system::Star>>,
) -> Vec<StarSystemGroup> {
    use std::collections::BTreeMap;

    // Map system_id → star name
    let mut system_names: BTreeMap<usize, String> = BTreeMap::new();
    for (body, sys_id) in star_query.iter() {
        system_names.entry(sys_id.0).or_insert_with(|| body.name.clone());
    }

    // Group bodies by star system
    let mut system_bodies: BTreeMap<usize, Vec<BodyEconomyEntry>> = BTreeMap::new();

    for (_entity, body, sys_id_opt, colony_opt, resources_opt, gen_opt, mining_opt) in body_query.iter() {
        let sys_id = sys_id_opt.map(|s| s.0).unwrap_or(0);

        // Skip stars themselves in the body list
        if body.body_type == BodyType::Star {
            // Ensure system exists even if star has no economic children
            system_names.entry(sys_id).or_insert_with(|| body.name.clone());
            // But still record power generators on stars
            if let Some(gen) = gen_opt {
                system_bodies.entry(sys_id).or_default().push(BodyEconomyEntry {
                    body_name: body.name.clone(),
                    body_type: body.body_type,
                    source_kind: EconomySourceKind::MiningOp,
                    colony: None,
                    mining_ops: Vec::new(),
                    deposits: Vec::new(),
                    generators: vec![PowerGenSnapshot {
                        source_type: gen.source_type,
                        output_watts: gen.output,
                    }],
                });
            }
            continue;
        }

        // Only include bodies with economic activity
        let has_colony = colony_opt.is_some();
        let has_mining = mining_opt.is_some();
        let has_deposits = resources_opt.map(|r| !r.deposits.is_empty()).unwrap_or(false);
        let has_power = gen_opt.is_some();

        if !has_colony && !has_mining && !has_deposits && !has_power {
            continue;
        }

        let colony_snap = colony_opt.map(|c| ColonySnapshot {
            name: c.name.clone(),
            population: c.population,
            growth_per_year: c.population_growth_per_year(),
            housing_capacity: c.housing_capacity(),
            total_buildings: c.total_buildings(),
            workforce_efficiency: c.workforce_efficiency(),
            logistics_efficiency: c.logistics_efficiency(),
            income_per_year: c.wealth_generation_per_year(),
            operating_cost_per_year: c.operating_cost_per_year(),
            buildings: c.buildings.iter().filter(|(_, &n)| n > 0).map(|(b, &n)| (*b, n)).collect(),
        });

        let mut mining_ops = Vec::new();
        if let Some(op) = mining_opt {
            mining_ops.push(MiningOpSnapshot {
                resource_type: op.resource_type,
                rate_mt_per_year: op.base_rate_mt_per_year,
                active: op.active,
            });
        }

        let deposits: Vec<(ResourceType, MineralDeposit)> = resources_opt
            .map(|r| r.deposits.iter().map(|(rt, d)| (*rt, *d)).collect())
            .unwrap_or_default();

        let mut generators = Vec::new();
        if let Some(gen) = gen_opt {
            generators.push(PowerGenSnapshot {
                source_type: gen.source_type,
                output_watts: gen.output,
            });
        }

        let source_kind = if has_colony {
            EconomySourceKind::Colony
        } else {
            EconomySourceKind::MiningOp
        };

        system_bodies.entry(sys_id).or_default().push(BodyEconomyEntry {
            body_name: body.name.clone(),
            body_type: body.body_type,
            source_kind,
            colony: colony_snap,
            mining_ops,
            deposits,
            generators,
        });
    }

    // Build final groups
    let mut groups: Vec<StarSystemGroup> = Vec::new();
    for (sys_id, bodies) in system_bodies {
        let system_name = system_names
            .get(&sys_id)
            .cloned()
            .unwrap_or_else(|| format!("System #{}", sys_id));
        groups.push(StarSystemGroup {
            system_name: format!("{} System", system_name),
            bodies,
        });
    }

    groups
}

/// Format a rate value with sign and color helper.
fn rate_text(rate: f64, suffix: &str) -> (String, egui::Color32) {
    if rate.abs() < 1e-9 {
        return (format!("0{}", suffix), egui::Color32::from_rgb(150, 150, 150));
    }
    let sign = if rate > 0.0 { "+" } else { "" };
    let text = format!("{}{}{}", sign, format_mass(rate), suffix);
    let color = if rate > 0.0 {
        egui::Color32::from_rgb(100, 255, 100)
    } else {
        egui::Color32::from_rgb(255, 100, 100)
    };
    (text, color)
}

/// System that renders the Economy UI when the Economy menu is active.
///
/// This system provides a hierarchical view of the empire's economy broken down
/// by star system → celestial body → buildings/operations. Includes tabs for
/// overview, resources, colonies, mining, and power grid. The architecture is
/// prepared for future expansion with stations and mining ships.
pub(super) fn ui_economy_panels(
    mut contexts: EguiContexts,
    active_menu: Res<ActiveMenu>,
    budget: Res<GlobalBudget>,
    rate_tracker: Res<ResourceRateTracker>,
    body_query: Query<(
        Entity,
        &CelestialBody,
        Option<&SystemId>,
        Option<&Colony>,
        Option<&PlanetResources>,
        Option<&crate::economy::components::PowerGenerator>,
        Option<&MiningOperation>,
    )>,
    star_query: Query<(&CelestialBody, &SystemId), With<crate::plugins::solar_system::Star>>,
    buildings_data: Option<Res<BuildingsData>>,
) {
    if active_menu.current != GameMenu::Economy {
        return;
    }

    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    let hierarchy = build_economy_hierarchy(&body_query, &star_query);

    egui::CentralPanel::default().show(ctx, |ui| {
        // Tab state (persisted across frames)
        let tab_id = ui.id().with("economy_tab");
        let mut current_tab: EconomyTab = ui.data_mut(|data| {
            data.get_persisted(tab_id).unwrap_or(0u8)
        }).into();

        ui.heading("Economy");
        ui.separator();

        // Tab bar
        ui.horizontal(|ui| {
            let tabs = [
                (EconomyTab::Overview, "📊 Overview"),
                (EconomyTab::Resources, "📦 Resources"),
                (EconomyTab::Colonies, "🏠 Colonies"),
                (EconomyTab::Mining, "⛏ Mining"),
                (EconomyTab::PowerGrid, "⚡ Power Grid"),
            ];
            for (tab, label) in &tabs {
                let selected = current_tab == *tab;
                if ui
                    .selectable_label(selected, egui::RichText::new(*label).size(14.0))
                    .clicked()
                {
                    current_tab = *tab;
                }
            }
        });
        ui.separator();

        // Persist tab
        let tab_byte: u8 = current_tab.into();
        ui.data_mut(|data| {
            data.insert_persisted(tab_id, tab_byte);
        });

        match current_tab {
            EconomyTab::Overview => render_econ_overview(ui, &budget, &rate_tracker, &hierarchy),
            EconomyTab::Resources => render_econ_resources(ui, &budget, &rate_tracker, &hierarchy, buildings_data.as_deref()),
            EconomyTab::Colonies => render_econ_colonies(ui, &budget, &hierarchy),
            EconomyTab::Mining => render_econ_mining(ui, &hierarchy),
            EconomyTab::PowerGrid => render_econ_power_grid(ui, &budget, &hierarchy),
        }
    });
}

// ---- Economy Tab: Overview ----

fn render_econ_overview(
    ui: &mut egui::Ui,
    budget: &GlobalBudget,
    rate_tracker: &ResourceRateTracker,
    hierarchy: &[StarSystemGroup],
) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        // Treasury & Balance
        ui.group(|ui| {
            ui.label(egui::RichText::new("💰 Treasury & Budget").strong().size(16.0));
            ui.separator();

            egui::Grid::new("econ_ov_treasury")
                .num_columns(2)
                .spacing([20.0, 4.0])
                .show(ui, |ui| {
                    ui.label("Treasury:");
                    ui.label(
                        egui::RichText::new(format_currency(budget.treasury))
                            .strong()
                            .color(egui::Color32::from_rgb(255, 215, 0)),
                    );
                    ui.end_row();

                    ui.label("Income:");
                    ui.label(
                        egui::RichText::new(format!("{}/yr", format_currency(budget.income_per_year)))
                            .color(egui::Color32::from_rgb(100, 255, 100)),
                    );
                    ui.end_row();

                    ui.label("Expenses:");
                    ui.label(
                        egui::RichText::new(format!("{}/yr", format_currency(budget.expenses_per_year)))
                            .color(egui::Color32::from_rgb(255, 140, 140)),
                    );
                    ui.end_row();

                    let balance = budget.balance_per_year();
                    let (sign, color) = if balance >= 0.0 {
                        ("+", egui::Color32::GREEN)
                    } else {
                        ("", egui::Color32::RED)
                    };
                    ui.label("Balance:");
                    ui.label(
                        egui::RichText::new(format!("{}{}/yr", sign, format_currency(balance)))
                            .strong()
                            .color(color),
                    );
                    ui.end_row();
                });
        });

        ui.add_space(8.0);

        // Power Grid & Civilization in two columns
        ui.columns(2, |cols| {
            // Power grid summary
            cols[0].group(|ui| {
                ui.label(egui::RichText::new("⚡ Power Grid").strong().size(14.0));
                ui.separator();

                let grid = &budget.energy_grid;
                let surplus = grid.surplus();
                let utilization = grid.load_factor();

                egui::Grid::new("econ_ov_power")
                    .num_columns(2)
                    .spacing([12.0, 3.0])
                    .show(ui, |ui| {
                        ui.label("Production:");
                        ui.label(egui::RichText::new(format_power(grid.produced)).color(egui::Color32::from_rgb(100, 255, 100)));
                        ui.end_row();
                        ui.label("Consumption:");
                        ui.label(egui::RichText::new(format_power(grid.consumed)).color(egui::Color32::from_rgb(255, 180, 100)));
                        ui.end_row();
                        ui.label("Surplus:");
                        let sc = if surplus >= 0.0 { egui::Color32::GREEN } else { egui::Color32::RED };
                        ui.label(egui::RichText::new(format_power(surplus)).strong().color(sc));
                        ui.end_row();
                        ui.label("Load:");
                        let lc = if utilization < 0.8 { egui::Color32::GREEN } else if utilization < 1.0 { egui::Color32::YELLOW } else { egui::Color32::RED };
                        ui.label(egui::RichText::new(format!("{:.1}%", utilization * 100.0)).color(lc));
                        ui.end_row();
                    });
            });

            // Civilization & Population
            cols[1].group(|ui| {
                ui.label(egui::RichText::new("🏆 Civilization").strong().size(14.0));
                ui.separator();

                let total_pop: f64 = hierarchy.iter()
                    .flat_map(|g| g.bodies.iter())
                    .filter_map(|b| b.colony.as_ref())
                    .map(|c| c.population)
                    .sum();
                let total_colonies: usize = hierarchy.iter()
                    .flat_map(|g| g.bodies.iter())
                    .filter(|b| b.colony.is_some())
                    .count();

                egui::Grid::new("econ_ov_civ")
                    .num_columns(2)
                    .spacing([12.0, 3.0])
                    .show(ui, |ui| {
                        ui.label("Score:");
                        ui.label(egui::RichText::new(format!("{:.0}", budget.civilization_score)).strong().color(egui::Color32::from_rgb(255, 215, 0)));
                        ui.end_row();
                        ui.label("Colonies:");
                        ui.label(egui::RichText::new(format!("{}", total_colonies)).strong());
                        ui.end_row();
                        ui.label("Population:");
                        ui.label(egui::RichText::new(Colony::format_population(total_pop)).strong());
                        ui.end_row();
                        ui.label("Systems:");
                        ui.label(egui::RichText::new(format!("{}", hierarchy.len())).strong());
                        ui.end_row();
                    });
            });
        });

        ui.add_space(8.0);

        // Critical resources
        ui.group(|ui| {
            ui.label(egui::RichText::new("⚠ Critical Resources").strong().size(14.0));
            ui.separator();

            let mut has_critical = false;
            for resource in ResourceType::all() {
                let stockpile = budget.get_stockpile(resource);
                let rate = rate_tracker.get_resource_rate(resource);
                let is_critical_rate = rate < -0.01;
                let is_low_stock = stockpile < 100.0 && resource.is_critical();

                if is_critical_rate || is_low_stock {
                    has_critical = true;
                    ui.horizontal(|ui| {
                        let icon = if is_critical_rate { "🔻" } else { "⚠" };
                        ui.label(icon);
                        ui.label(egui::RichText::new(resource.display_name()).strong());
                        ui.label(format!("Stock: {}", format_mass(stockpile)));
                        let (txt, col) = rate_text(rate, "/mo");
                        ui.label(egui::RichText::new(txt).color(col));
                    });
                }
            }
            if !has_critical {
                ui.label(egui::RichText::new("All resources at healthy levels").italics().color(egui::Color32::from_rgb(100, 255, 100)));
            }
        });

        ui.add_space(8.0);

        // Per-star-system summary
        ui.group(|ui| {
            ui.label(egui::RichText::new("🌟 Per-System Summary").strong().size(14.0));
            ui.separator();

            if hierarchy.is_empty() {
                ui.label(egui::RichText::new("No economic activity").italics().color(egui::Color32::GRAY));
            } else {
                for group in hierarchy {
                    let sys_colonies: usize = group.bodies.iter().filter(|b| b.colony.is_some()).count();
                    let sys_pop: f64 = group.bodies.iter().filter_map(|b| b.colony.as_ref()).map(|c| c.population).sum();
                    let sys_income: f64 = group.bodies.iter().filter_map(|b| b.colony.as_ref()).map(|c| c.income_per_year).sum();
                    let sys_cost: f64 = group.bodies.iter().filter_map(|b| b.colony.as_ref()).map(|c| c.operating_cost_per_year).sum();
                    let sys_net = sys_income - sys_cost;
                    let net_color = if sys_net >= 0.0 { egui::Color32::GREEN } else { egui::Color32::RED };

                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(&group.system_name).strong());
                        ui.label(format!("— {} colonies, Pop: {}", sys_colonies, Colony::format_population(sys_pop)));
                        let sign = if sys_net >= 0.0 { "+" } else { "" };
                        ui.label(egui::RichText::new(format!("Net: {}{}/yr", sign, format_currency(sys_net))).color(net_color));
                    });
                }
            }
        });
    });
}

// ---- Economy Tab: Resources ----

/// Render resource stockpiles and net rates with per-system breakdown.
fn render_econ_resources(
    ui: &mut egui::Ui,
    budget: &GlobalBudget,
    rate_tracker: &ResourceRateTracker,
    hierarchy: &[StarSystemGroup],
    buildings_data: Option<&BuildingsData>,
) {
    ui.label(egui::RichText::new("Rates are net monthly. Units scale automatically (t, kt, Mt, Gt).").size(11.0).color(egui::Color32::GRAY));
    ui.separator();

    egui::ScrollArea::vertical().show(ui, |ui| {
        // Global resource stockpiles by category
        let categories = ResourceType::by_category();
        for (category_name, resources) in &categories {
            egui::CollapsingHeader::new(
                egui::RichText::new(format!("📦 {}", category_name)).strong().size(14.0),
            )
            .default_open(true)
            .show(ui, |ui| {
                egui::Grid::new(format!("econ_res_{}", category_name))
                    .num_columns(4)
                    .spacing([15.0, 3.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new("Resource").strong());
                        ui.label(egui::RichText::new("Symbol").strong());
                        ui.label(egui::RichText::new("Stockpile").strong());
                        ui.label(egui::RichText::new("Net Rate (/mo)").strong());
                        ui.end_row();

                        for resource in resources {
                            let stockpile = budget.get_stockpile(resource);
                            let rate = rate_tracker.get_resource_rate(resource);

                            ui.label(resource.display_name());
                            ui.label(egui::RichText::new(resource.symbol()).monospace().color(egui::Color32::from_rgb(180, 180, 200)));

                            let stock_color = if stockpile <= 0.0 {
                                egui::Color32::from_rgb(255, 80, 80)
                            } else if stockpile < 100.0 && resource.is_critical() {
                                egui::Color32::from_rgb(255, 200, 80)
                            } else {
                                egui::Color32::from_rgb(200, 200, 200)
                            };
                            ui.label(egui::RichText::new(format_mass(stockpile)).monospace().color(stock_color));

                            let (txt, col) = rate_text(rate, "/mo");
                            ui.label(egui::RichText::new(txt).monospace().color(col));
                            ui.end_row();
                        }
                    });
            });
        }

        ui.add_space(8.0);

        // Research & Engineering rates
        ui.group(|ui| {
            ui.label(egui::RichText::new("🔬 Research & Engineering Output").strong().size(14.0));
            ui.separator();
            egui::Grid::new("econ_res_rp_ep")
                .num_columns(2)
                .spacing([20.0, 4.0])
                .show(ui, |ui| {
                    ui.label("Research Points:");
                    ui.label(egui::RichText::new(format!("{:.1} RP/mo", rate_tracker.research_rate_per_month)).color(egui::Color32::from_rgb(100, 180, 255)));
                    ui.end_row();
                    ui.label("Engineering Points:");
                    ui.label(egui::RichText::new(format!("{:.1} EP/mo", rate_tracker.engineering_rate_per_month)).color(egui::Color32::from_rgb(100, 255, 180)));
                    ui.end_row();
                });
        });

        ui.add_space(8.0);

        // Per-system resource production breakdown
        ui.group(|ui| {
            ui.label(egui::RichText::new("🌟 Production & Consumption by Location").strong().size(14.0));
            ui.separator();

            if hierarchy.is_empty() {
                ui.label(egui::RichText::new("No economic activity detected").italics().color(egui::Color32::GRAY));
                return;
            }

            for group in hierarchy {
                egui::CollapsingHeader::new(
                    egui::RichText::new(format!("⭐ {}", group.system_name)).strong().size(13.0),
                )
                .default_open(true)
                .show(ui, |ui| {
                    for body_entry in &group.bodies {
                        if body_entry.colony.is_none() && body_entry.mining_ops.is_empty() {
                            continue; // Skip bodies with no active production
                        }

                        let body_icon = match body_entry.body_type {
                            BodyType::Planet | BodyType::GasGiant => "🪐",
                            BodyType::Moon => "🌙",
                            BodyType::Asteroid => "🪨",
                            BodyType::DwarfPlanet => "⚫",
                            BodyType::Comet => "☄",
                            _ => "🔵",
                        };

                        egui::CollapsingHeader::new(
                            egui::RichText::new(format!("{} {}", body_icon, body_entry.body_name)).size(12.0),
                        )
                        .default_open(false)
                        .show(ui, |ui| {
                            // Colony building production/consumption
                            if let Some(colony) = &body_entry.colony {
                                if let Some(data) = buildings_data {
                                    let mut production_rows: Vec<(String, ResourceType, f64)> = Vec::new();
                                    let mut consumption_rows: Vec<(String, ResourceType, f64)> = Vec::new();

                                    for (building_type, count) in &colony.buildings {
                                        if *count == 0 { continue; }
                                        // Maintenance consumption
                                        let maint = data.maintenance_resources(building_type);
                                        for (res_name, annual_amt) in maint {
                                            if let Some(rt) = crate::colony::data::parse_resource_type(res_name) {
                                                consumption_rows.push((
                                                    format!("{} ×{}", building_type.display_name(), count),
                                                    rt,
                                                    annual_amt * (*count as f64) / 12.0,
                                                ));
                                            }
                                        }
                                    }

                                    // Mining production (estimate from colony's deposits)
                                    // Show which resources the colony's mines/atmo processors extract
                                    let mut ui_surface_rate = 0.0_f64;
                                    let mut ui_deep_rate    = 0.0_f64;
                                    let mut ui_bulk_rate    = 0.0_f64;
                                    let mut total_atmo_rate = 0.0_f64;
                                    for (bt, count) in &colony.buildings {
                                        if *count == 0 { continue; }
                                        if let Some(def) = data.get(bt) {
                                            for modifier in &def.modifiers {
                                                match modifier.modifier_type.as_str() {
                                                    "MiningEfficiency"      => ui_surface_rate += modifier.value * *count as f64,
                                                    "DeepMiningEfficiency"  => ui_deep_rate    += modifier.value * *count as f64,
                                                    "BulkMiningEfficiency"  => ui_bulk_rate    += modifier.value * *count as f64,
                                                    "AtmosphericHarvesting" => total_atmo_rate += modifier.value * *count as f64,
                                                    _ => {}
                                                }
                                            }
                                        }
                                    }

                                    // Solid mining production breakdown — three tiers, no overflow
                                    if ui_surface_rate > 0.0 {
                                        let eligible: Vec<(ResourceType, f64)> = body_entry.deposits.iter()
                                            .filter(|(_, d)| !d.is_atmospheric && d.reserve.proven_crustal > 0.001)
                                            .map(|(rt, d)| (*rt, (d.reserve.concentration as f64).max(1e-10)))
                                            .collect();
                                        let total_weight: f64 = eligible.iter().map(|(_, w)| w).sum();
                                        if total_weight > 0.0 {
                                            let monthly = ui_surface_rate / 12.0;
                                            for (rt, weight) in &eligible {
                                                production_rows.push(("Mining".to_string(), *rt, monthly * weight / total_weight));
                                            }
                                        }
                                    }
                                    if ui_deep_rate > 0.0 {
                                        let eligible: Vec<(ResourceType, f64)> = body_entry.deposits.iter()
                                            .filter(|(_, d)| !d.is_atmospheric && d.reserve.deep_deposits > 0.001)
                                            .map(|(rt, d)| (*rt, (d.reserve.concentration as f64).max(1e-10)))
                                            .collect();
                                        let total_weight: f64 = eligible.iter().map(|(_, w)| w).sum();
                                        if total_weight > 0.0 {
                                            let monthly = ui_deep_rate / 12.0;
                                            for (rt, weight) in &eligible {
                                                production_rows.push(("Deep Mining".to_string(), *rt, monthly * weight / total_weight));
                                            }
                                        }
                                    }
                                    if ui_bulk_rate > 0.0 {
                                        let eligible: Vec<(ResourceType, f64)> = body_entry.deposits.iter()
                                            .filter(|(_, d)| !d.is_atmospheric && d.reserve.planetary_bulk > 0.001)
                                            .map(|(rt, d)| (*rt, (d.reserve.concentration as f64).max(1e-10)))
                                            .collect();
                                        let total_weight: f64 = eligible.iter().map(|(_, w)| w).sum();
                                        if total_weight > 0.0 {
                                            let monthly = ui_bulk_rate / 12.0;
                                            for (rt, weight) in &eligible {
                                                production_rows.push(("Bulk Mining".to_string(), *rt, monthly * weight / total_weight));
                                            }
                                        }
                                    }

                                    // Atmospheric harvesting production breakdown
                                    if total_atmo_rate > 0.0 {
                                        let harvestable: Vec<(ResourceType, f64)> = body_entry.deposits.iter()
                                            .filter(|(_, d)| d.is_atmospheric && (d.reserve.proven_crustal > 0.001 || d.reserve.deep_deposits > 0.001))
                                            .map(|(rt, d)| (*rt, (d.reserve.concentration as f64).max(1e-10)))
                                            .collect();
                                        let total_weight: f64 = harvestable.iter().map(|(_, w)| w).sum();
                                        if total_weight > 0.0 {
                                            let monthly_total = total_atmo_rate / 12.0;
                                            for (rt, weight) in &harvestable {
                                                let share = weight / total_weight;
                                                production_rows.push(("Atmo Harvesting".to_string(), *rt, monthly_total * share));
                                            }
                                        }
                                    }

                                    if !production_rows.is_empty() {
                                        ui.label(egui::RichText::new("Production (/mo):").strong().size(11.0).color(egui::Color32::from_rgb(100, 255, 100)));
                                        egui::Grid::new(format!("econ_prod_{}", body_entry.body_name))
                                            .num_columns(3)
                                            .spacing([10.0, 2.0])
                                            .striped(true)
                                            .show(ui, |ui| {
                                                for (source, rt, monthly) in &production_rows {
                                                    ui.label(egui::RichText::new(source).size(11.0));
                                                    ui.label(egui::RichText::new(rt.display_name()).size(11.0));
                                                    ui.label(egui::RichText::new(format!("+{}", format_mass(*monthly))).monospace().size(11.0).color(egui::Color32::from_rgb(100, 255, 100)));
                                                    ui.end_row();
                                                }
                                            });
                                    }

                                    if !consumption_rows.is_empty() {
                                        ui.label(egui::RichText::new("Consumption (/mo):").strong().size(11.0).color(egui::Color32::from_rgb(255, 140, 140)));
                                        egui::Grid::new(format!("econ_cons_{}", body_entry.body_name))
                                            .num_columns(3)
                                            .spacing([10.0, 2.0])
                                            .striped(true)
                                            .show(ui, |ui| {
                                                for (source, rt, monthly) in &consumption_rows {
                                                    ui.label(egui::RichText::new(source).size(11.0));
                                                    ui.label(egui::RichText::new(rt.display_name()).size(11.0));
                                                    ui.label(egui::RichText::new(format!("-{}", format_mass(*monthly))).monospace().size(11.0).color(egui::Color32::from_rgb(255, 140, 140)));
                                                    ui.end_row();
                                                }
                                            });
                                    }

                                    if production_rows.is_empty() && consumption_rows.is_empty() {
                                        ui.label(egui::RichText::new("No resource flows").italics().size(11.0).color(egui::Color32::GRAY));
                                    }
                                } else {
                                    ui.label(egui::RichText::new("Building data not loaded").italics().size(11.0).color(egui::Color32::GRAY));
                                }
                            }

                            // Standalone mining operations
                            for op in &body_entry.mining_ops {
                                let status = if op.active { "Active" } else { "Idle" };
                                let monthly = op.rate_mt_per_year / 12.0;
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new(format!("⛏ {} — {}/mo [{}]", op.resource_type.display_name(), format_mass(monthly), status)).size(11.0));
                                });
                            }
                        });
                    }
                });
            }
        });

        // Placeholder for future sources
        ui.add_space(8.0);
        ui.group(|ui| {
            ui.label(egui::RichText::new("🚧 Future Sources").size(12.0).color(egui::Color32::from_rgb(150, 150, 150)));
            ui.label(egui::RichText::new("Stations and mining ships will appear here when implemented.").italics().size(11.0).color(egui::Color32::from_rgb(120, 120, 120)));
        });
    });
}

// ---- Economy Tab: Colonies ----

fn render_econ_colonies(
    ui: &mut egui::Ui,
    budget: &GlobalBudget,
    hierarchy: &[StarSystemGroup],
) {
    // Summary bar
    let all_colonies: Vec<&ColonySnapshot> = hierarchy.iter()
        .flat_map(|g| g.bodies.iter())
        .filter_map(|b| b.colony.as_ref())
        .collect();

    let total_pop: f64 = all_colonies.iter().map(|c| c.population).sum();
    let total_income: f64 = all_colonies.iter().map(|c| c.income_per_year).sum();
    let total_cost: f64 = all_colonies.iter().map(|c| c.operating_cost_per_year).sum();
    let net = total_income - total_cost;
    let net_color = if net >= 0.0 { egui::Color32::GREEN } else { egui::Color32::RED };

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(format!("{} colonies", all_colonies.len())).strong());
        ui.separator();
        ui.label(egui::RichText::new(format!("Pop: {}", Colony::format_population(total_pop))));
        ui.separator();
        ui.label(egui::RichText::new(format!("Income: {}/yr", format_currency(total_income))).color(egui::Color32::from_rgb(100, 255, 100)));
        ui.separator();
        ui.label(egui::RichText::new(format!("Costs: {}/yr", format_currency(total_cost))).color(egui::Color32::from_rgb(255, 140, 140)));
        ui.separator();
        let sign = if net >= 0.0 { "+" } else { "" };
        ui.label(egui::RichText::new(format!("Net: {}{}/yr", sign, format_currency(net))).strong().color(net_color));
        ui.separator();
        ui.label(egui::RichText::new(format!("💰 {}", format_currency(budget.treasury))).color(egui::Color32::from_rgb(255, 215, 0)));
    });
    ui.separator();

    if all_colonies.is_empty() {
        ui.add_space(20.0);
        ui.label(egui::RichText::new("No colonies established yet").size(14.0).italics().color(egui::Color32::GRAY));
        ui.label("Establish a colony to see economic breakdowns here.");
        return;
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        for group in hierarchy {
            let sys_colonies: Vec<&BodyEconomyEntry> = group.bodies.iter().filter(|b| b.colony.is_some()).collect();
            if sys_colonies.is_empty() {
                continue;
            }

            let sys_income: f64 = sys_colonies.iter().filter_map(|b| b.colony.as_ref()).map(|c| c.income_per_year).sum();
            let sys_cost: f64 = sys_colonies.iter().filter_map(|b| b.colony.as_ref()).map(|c| c.operating_cost_per_year).sum();
            let sys_net = sys_income - sys_cost;
            let sys_net_color = if sys_net >= 0.0 { egui::Color32::GREEN } else { egui::Color32::RED };
            let sys_sign = if sys_net >= 0.0 { "+" } else { "" };

            egui::CollapsingHeader::new(
                egui::RichText::new(format!(
                    "⭐ {} — {} colonies, Net: {}{}/yr",
                    group.system_name, sys_colonies.len(), sys_sign, format_currency(sys_net),
                )).strong().size(14.0).color(sys_net_color),
            )
            .default_open(true)
            .show(ui, |ui| {
                for body_entry in &sys_colonies {
                    let colony = body_entry.colony.as_ref().unwrap();
                    let income = colony.income_per_year;
                    let cost = colony.operating_cost_per_year;
                    let colony_net = income - cost;
                    let cn_color = if colony_net >= 0.0 { egui::Color32::GREEN } else { egui::Color32::RED };
                    let cn_sign = if colony_net >= 0.0 { "+" } else { "" };

                    let body_icon = match body_entry.body_type {
                        BodyType::Planet | BodyType::GasGiant => "🪐",
                        BodyType::Moon => "🌙",
                        BodyType::Asteroid => "🪨",
                        BodyType::DwarfPlanet => "⚫",
                        _ => "🔵",
                    };

                    egui::CollapsingHeader::new(
                        egui::RichText::new(format!(
                            "{} {} ({}) — Net: {}{}/yr",
                            body_icon, colony.name, body_entry.body_name,
                            cn_sign, format_currency(colony_net),
                        )).strong().color(cn_color),
                    )
                    .default_open(true)
                    .show(ui, |ui| {
                        egui::Grid::new(format!("econ_col_{}", colony.name))
                            .num_columns(2)
                            .spacing([20.0, 3.0])
                            .striped(true)
                            .show(ui, |ui| {
                                ui.label("Population:");
                                ui.label(Colony::format_population(colony.population));
                                ui.end_row();

                                ui.label("Growth:");
                                ui.label(format!("+{}/yr", Colony::format_population(colony.growth_per_year)));
                                ui.end_row();

                                ui.label("Housing:");
                                let util = if colony.housing_capacity > 0.0 { colony.population / colony.housing_capacity * 100.0 } else { 0.0 };
                                ui.label(format!("{} / {} ({:.0}%)", Colony::format_population(colony.population), Colony::format_population(colony.housing_capacity), util));
                                ui.end_row();

                                ui.label("Buildings:");
                                ui.label(format!("{}", colony.total_buildings));
                                ui.end_row();

                                ui.label("Workforce:");
                                let wf_color = if colony.workforce_efficiency >= 1.0 { egui::Color32::GREEN } else if colony.workforce_efficiency >= 0.5 { egui::Color32::YELLOW } else { egui::Color32::RED };
                                ui.label(egui::RichText::new(format!("{:.0}%", colony.workforce_efficiency * 100.0)).color(wf_color));
                                ui.end_row();

                                ui.label("Logistics:");
                                let log_color = if colony.logistics_efficiency >= 1.0 { egui::Color32::GREEN } else if colony.logistics_efficiency >= 0.5 { egui::Color32::YELLOW } else { egui::Color32::RED };
                                ui.label(egui::RichText::new(format!("{:.0}%", colony.logistics_efficiency * 100.0)).color(log_color));
                                ui.end_row();

                                ui.label("Income:");
                                ui.label(egui::RichText::new(format!("{}/yr", format_currency(income))).color(egui::Color32::from_rgb(100, 255, 100)));
                                ui.end_row();

                                ui.label("Operating Cost:");
                                ui.label(egui::RichText::new(format!("{}/yr", format_currency(cost))).color(egui::Color32::from_rgb(255, 140, 140)));
                                ui.end_row();

                                ui.label("Net:");
                                ui.label(egui::RichText::new(format!("{}{}/yr", cn_sign, format_currency(colony_net))).strong().color(cn_color));
                                ui.end_row();
                            });

                        // Buildings breakdown by category
                        if colony.total_buildings > 0 {
                            ui.add_space(4.0);
                            egui::CollapsingHeader::new("📋 Buildings")
                                .default_open(false)
                                .show(ui, |ui| {
                                    for category in BuildingCategory::all() {
                                        let in_cat: Vec<(BuildingType, u32)> = colony.buildings.iter()
                                            .filter(|(bt, _)| category.buildings().contains(bt))
                                            .map(|(bt, n)| (*bt, *n))
                                            .collect();

                                        if !in_cat.is_empty() {
                                            ui.label(egui::RichText::new(category.display_name()).size(12.0).strong());
                                            for (building, count) in in_cat {
                                                ui.label(format!("  {} {} × {}", building.icon(), building.display_name(), count));
                                            }
                                        }
                                    }
                                });
                        }
                    });
                    ui.add_space(3.0);
                }
            });
        }

        // Future: Stations section placeholder
        ui.add_space(8.0);
        ui.group(|ui| {
            ui.label(egui::RichText::new("🛸 Stations").size(12.0).color(egui::Color32::from_rgb(150, 150, 150)));
            ui.label(egui::RichText::new("Space stations will appear here when implemented.").italics().size(11.0).color(egui::Color32::from_rgb(120, 120, 120)));
        });
    });
}

// ---- Economy Tab: Mining ----

fn render_econ_mining(
    ui: &mut egui::Ui,
    hierarchy: &[StarSystemGroup],
) {
    ui.label(egui::RichText::new("Mining operations and resource deposits by location").size(11.0).color(egui::Color32::GRAY));
    ui.separator();

    egui::ScrollArea::vertical().show(ui, |ui| {
        if hierarchy.is_empty() {
            ui.add_space(20.0);
            ui.label(egui::RichText::new("No mining activity or surveyed deposits").size(14.0).italics().color(egui::Color32::GRAY));
            return;
        }

        for group in hierarchy {
            let has_mining = group.bodies.iter().any(|b| !b.mining_ops.is_empty() || b.colony.is_some());
            let has_deposits = group.bodies.iter().any(|b| !b.deposits.is_empty());

            if !has_mining && !has_deposits {
                continue;
            }

            egui::CollapsingHeader::new(
                egui::RichText::new(format!("⭐ {}", group.system_name)).strong().size(14.0),
            )
            .default_open(true)
            .show(ui, |ui| {
                for body_entry in &group.bodies {
                    if body_entry.mining_ops.is_empty() && body_entry.deposits.is_empty() && body_entry.colony.is_none() {
                        continue;
                    }

                    let body_icon = match body_entry.body_type {
                        BodyType::Planet | BodyType::GasGiant => "🪐",
                        BodyType::Moon => "🌙",
                        BodyType::Asteroid => "🪨",
                        BodyType::DwarfPlanet => "⚫",
                        BodyType::Comet => "☄",
                        _ => "🔵",
                    };

                    let deposit_count = body_entry.deposits.len();
                    let op_count = body_entry.mining_ops.len();
                    let has_colony_mining = body_entry.colony.as_ref().map(|c| c.total_buildings > 0).unwrap_or(false);

                    let mut header_parts = Vec::new();
                    if deposit_count > 0 { header_parts.push(format!("{} deposits", deposit_count)); }
                    if op_count > 0 { header_parts.push(format!("{} ops", op_count)); }
                    if has_colony_mining { header_parts.push("colony mining".to_string()); }

                    egui::CollapsingHeader::new(
                        egui::RichText::new(format!("{} {} ({})", body_icon, body_entry.body_name, header_parts.join(", "))).size(13.0),
                    )
                    .default_open(false)
                    .show(ui, |ui| {
                        // Active mining operations
                        if !body_entry.mining_ops.is_empty() {
                            ui.label(egui::RichText::new("⛏ Active Operations").strong().size(12.0));
                            egui::Grid::new(format!("econ_mine_ops_{}", body_entry.body_name))
                                .num_columns(3)
                                .spacing([12.0, 2.0])
                                .striped(true)
                                .show(ui, |ui| {
                                    ui.label(egui::RichText::new("Resource").strong().size(11.0));
                                    ui.label(egui::RichText::new("Rate (Mt/yr)").strong().size(11.0));
                                    ui.label(egui::RichText::new("Status").strong().size(11.0));
                                    ui.end_row();

                                    for op in &body_entry.mining_ops {
                                        ui.label(egui::RichText::new(op.resource_type.display_name()).size(11.0));
                                        ui.label(egui::RichText::new(format!("{:.2}", op.rate_mt_per_year)).monospace().size(11.0));
                                        let (st, sc) = if op.active { ("Active", egui::Color32::GREEN) } else { ("Idle", egui::Color32::GRAY) };
                                        ui.label(egui::RichText::new(st).size(11.0).color(sc));
                                        ui.end_row();
                                    }
                                });
                            ui.add_space(4.0);
                        }

                        // Colony mining indicator
                        if let Some(colony) = &body_entry.colony {
                            if colony.total_buildings > 0 {
                                ui.label(egui::RichText::new(format!("🏠 Colony: {} ({} buildings)", colony.name, colony.total_buildings)).size(11.0));
                                ui.add_space(2.0);
                            }
                        }

                        // Resource deposits
                        if !body_entry.deposits.is_empty() {
                            ui.label(egui::RichText::new("🌍 Resource Deposits").strong().size(12.0));
                            egui::Grid::new(format!("econ_deposits_{}", body_entry.body_name))
                                .num_columns(5)
                                .spacing([10.0, 2.0])
                                .striped(true)
                                .show(ui, |ui| {
                                    ui.label(egui::RichText::new("Resource").strong().size(11.0));
                                    ui.label(egui::RichText::new("Proven (Mt)").strong().size(11.0));
                                    ui.label(egui::RichText::new("Deep (Mt)").strong().size(11.0));
                                    ui.label(egui::RichText::new("Access").strong().size(11.0));
                                    ui.label(egui::RichText::new("Type").strong().size(11.0));
                                    ui.end_row();

                                    let mut sorted: Vec<_> = body_entry.deposits.iter().collect();
                                    sorted.sort_by_key(|(rt, _)| rt.display_name());

                                    for (rt, deposit) in &sorted {
                                        ui.label(egui::RichText::new(rt.display_name()).size(11.0));
                                        ui.label(egui::RichText::new(format!("{:.1}", deposit.reserve.proven_crustal)).monospace().size(11.0));
                                        ui.label(egui::RichText::new(format!("{:.1}", deposit.reserve.deep_deposits)).monospace().size(11.0));
                                        let acc_color = if deposit.accessibility > 0.7 { egui::Color32::GREEN } else if deposit.accessibility > 0.3 { egui::Color32::YELLOW } else { egui::Color32::RED };
                                        ui.label(egui::RichText::new(format!("{:.0}%", deposit.accessibility * 100.0)).size(11.0).color(acc_color));
                                        let type_label = if deposit.is_atmospheric { "Atmo" } else { "Surface" };
                                        ui.label(egui::RichText::new(type_label).size(10.0).color(egui::Color32::from_rgb(180, 180, 200)));
                                        ui.end_row();
                                    }
                                });
                        }
                    });
                }
            });
        }

        // Future mining ships section
        ui.add_space(10.0);
        ui.group(|ui| {
            ui.label(egui::RichText::new("🚀 Mining Ships").size(12.0).color(egui::Color32::from_rgb(150, 150, 150)));
            ui.label(egui::RichText::new("Automated mining ships will appear here when implemented.").italics().size(11.0).color(egui::Color32::from_rgb(120, 120, 120)));
        });

        ui.add_space(5.0);
        ui.group(|ui| {
            ui.label(egui::RichText::new("🛸 Mining Stations").size(12.0).color(egui::Color32::from_rgb(150, 150, 150)));
            ui.label(egui::RichText::new("Orbital mining stations will appear here when implemented.").italics().size(11.0).color(egui::Color32::from_rgb(120, 120, 120)));
        });
    });
}

// ---- Economy Tab: Power Grid ----

fn render_econ_power_grid(
    ui: &mut egui::Ui,
    budget: &GlobalBudget,
    hierarchy: &[StarSystemGroup],
) {
    let grid = &budget.energy_grid;
    let surplus = grid.surplus();
    let utilization = grid.load_factor();

    // Grid status header
    ui.group(|ui| {
        let (status_text, status_color) = if utilization < 0.5 {
            ("Abundant Power", egui::Color32::from_rgb(100, 255, 100))
        } else if utilization < 0.8 {
            ("Healthy", egui::Color32::from_rgb(200, 255, 100))
        } else if utilization < 1.0 {
            ("Strained", egui::Color32::YELLOW)
        } else {
            ("DEFICIT — Build more power!", egui::Color32::RED)
        };

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("⚡ Grid Status:").strong().size(14.0));
            ui.label(egui::RichText::new(status_text).strong().size(14.0).color(status_color));
        });

        let bar_pct = utilization.min(1.0) as f32;
        ui.add(
            egui::ProgressBar::new(bar_pct)
                .text(format!(
                    "{} / {} ({:.1}%)",
                    format_power(grid.consumed),
                    format_power(grid.produced),
                    utilization * 100.0,
                ))
                .desired_width(ui.available_width().min(600.0)),
        );

        let surplus_color = if surplus >= 0.0 { egui::Color32::GREEN } else { egui::Color32::RED };
        ui.label(egui::RichText::new(format!("Surplus: {}", format_power(surplus))).color(surplus_color));
    });

    ui.separator();

    egui::ScrollArea::vertical().show(ui, |ui| {
        // Power breakdown by source type
        if !budget.power_breakdown.is_empty() {
            ui.group(|ui| {
                ui.label(egui::RichText::new("🔋 Production by Source Type").strong().size(14.0));
                ui.separator();

                egui::Grid::new("econ_pwr_breakdown")
                    .num_columns(2)
                    .spacing([20.0, 3.0])
                    .striped(true)
                    .show(ui, |ui| {
                        for (source_type, wattage) in &budget.power_breakdown {
                            ui.label(format!("{}", source_type));
                            ui.label(egui::RichText::new(format_power(*wattage)).monospace().color(egui::Color32::from_rgb(100, 255, 100)));
                            ui.end_row();
                        }
                    });
            });
            ui.add_space(8.0);
        }

        // Per-system power breakdown
        ui.group(|ui| {
            ui.label(egui::RichText::new("🌟 Power by Location").strong().size(14.0));
            ui.separator();

            if hierarchy.is_empty() {
                ui.label(egui::RichText::new("No power sources detected").italics().color(egui::Color32::GRAY));
                return;
            }

            for group in hierarchy {
                let has_power_data = group.bodies.iter().any(|b| !b.generators.is_empty() || b.colony.is_some());
                if !has_power_data {
                    continue;
                }

                egui::CollapsingHeader::new(
                    egui::RichText::new(format!("⭐ {}", group.system_name)).strong().size(13.0),
                )
                .default_open(true)
                .show(ui, |ui| {
                    for body_entry in &group.bodies {
                        if body_entry.generators.is_empty() && body_entry.colony.is_none() {
                            continue;
                        }

                        let body_icon = match body_entry.body_type {
                            BodyType::Planet | BodyType::GasGiant => "🪐",
                            BodyType::Moon => "🌙",
                            BodyType::Asteroid => "🪨",
                            BodyType::DwarfPlanet => "⚫",
                            _ => "🔵",
                        };

                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(format!("{} {}", body_icon, body_entry.body_name)).strong().size(12.0));

                            // Generators on this body
                            for gen in &body_entry.generators {
                                ui.label(egui::RichText::new(format!("| {} {}", format!("{}", gen.source_type), format_power(gen.output_watts))).size(11.0).color(egui::Color32::from_rgb(100, 255, 100)));
                            }

                            // Colony estimated consumption
                            if let Some(colony) = &body_entry.colony {
                                // Assume ~400MW per mega-structure building to match ~18TW total consumption
                                let est_load = colony.total_buildings as f64 * 400_000_000.0;
                                ui.label(egui::RichText::new(format!("| Load ~{}", format_power(est_load))).size(11.0).color(egui::Color32::from_rgb(255, 180, 100)));
                            }
                        });
                    }
                });
            }
        });

        // Future sources
        ui.add_space(8.0);
        ui.group(|ui| {
            ui.label(egui::RichText::new("🚧 Future Power Sources").size(12.0).color(egui::Color32::from_rgb(150, 150, 150)));
            ui.label(egui::RichText::new("Station and ship power grids will appear here when implemented.").italics().size(11.0).color(egui::Color32::from_rgb(120, 120, 120)));
        });
    });
}

