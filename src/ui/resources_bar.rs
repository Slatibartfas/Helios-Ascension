use super::dashboard::{format_mass, format_rate_monthly};
use super::research_panel::{render_research_tech_tooltip_content, ActiveProjectInfo};
use super::time::{
    estimate_engineering_project_end_timestamp, estimate_research_project_end_timestamp,
    format_timestamp_date_time,
};
use super::*;

fn get_resource_category_icon(category: &str) -> &'static str {
    match category {
        "Biological" => "\u{1F35E}",       // 🍞
        "Volatiles" => "\u{1F4A7}",        // 💧
        "Atmospheric Gases" => "\u{2601}", // ☁
        "Construction" => "\u{1F9F1}",     // 🧱
        "Fusion Fuel" => "\u{1F50B}",      // 🔋
        "Fissiles" => "\u{2622}",          // ☢
        "Precious Metals" => "\u{1F48E}",  // 💎
        "Strategic" => "\u{2699}",         // ⚙
        "Exotic" => "\u{1F52E}",           // 🔮
        _ => "\u{1F4E6}",                  // 📦
    }
}

/// Get the icon for a specific resource type
fn get_resource_icon(resource: &ResourceType) -> &'static str {
    match resource {
        // Biological
        ResourceType::Food => "\u{1F35E}", // 🍞

        // Volatiles
        ResourceType::Water => "\u{1F4A7}",      // 💧
        ResourceType::Hydrogen => "\u{1F388}",   // 🎈
        ResourceType::Ammonia => "\u{1F9FC}",    // 🧼
        ResourceType::Methane => "\u{1F525}",    // 🔥
        ResourceType::Phosphorus => "\u{1F331}", // 🌱

        // Atmospheric
        ResourceType::Nitrogen => "\u{1F32C}",      // 🌬
        ResourceType::Oxygen => "\u{1F4A8}",        // 💨
        ResourceType::CarbonDioxide => "\u{1F32B}", // 🌫
        ResourceType::Argon => "\u{1F7E3}",         // 🟣

        // Construction
        ResourceType::Iron => "\u{1F529}",      // 🔩
        ResourceType::Aluminum => "\u{2708}",   // ✈
        ResourceType::Titanium => "\u{1F6E1}",  // 🛡
        ResourceType::Silicates => "\u{1FAA8}", // 🪨
        ResourceType::Nickel => "\u{1F9F2}",    // 🧲
        ResourceType::Tungsten => "\u{1F3AF}",  // 🎯
        ResourceType::Carbon => "\u{2666}",     // ♦
        ResourceType::Chromium => "\u{1F6E0}",  // 🛠
        ResourceType::Magnesium => "\u{2728}",  // ✨

        // Energy
        ResourceType::Helium3 => "\u{2600}",   // ☀
        ResourceType::Deuterium => "\u{269B}", // ⚛

        // Fissiles
        ResourceType::Uranium => "\u{2622}", // ☢
        ResourceType::Thorium => "\u{26A1}", // ⚡

        // Precious
        ResourceType::Gold => "\u{1F451}",     // 👑
        ResourceType::Silver => "\u{1F948}",   // 🥈
        ResourceType::Platinum => "\u{1F48D}", // 💍

        // Strategic
        ResourceType::Copper => "\u{1F50C}",     // 🔌
        ResourceType::RareEarths => "\u{1F4F1}", // 📱
        ResourceType::Lithium => "\u{1F50B}",    // 🔋
        ResourceType::Sulfur => "\u{1F9EA}",     // 🧪
        ResourceType::Cobalt => "\u{1F535}",     // 🔵
        ResourceType::Fluorine => "\u{1F4A0}",   // 💠
        ResourceType::Polymers => "\u{1F9F4}",   // 🧴

        // Exotic
        ResourceType::Antimatter => "\u{2604}",     // ☄
        ResourceType::ExoticMatter => "\u{1F300}",  // 🌀
        ResourceType::Metamaterials => "\u{1F52C}", // 🔬
        ResourceType::Computronium => "\u{1F9E0}",  // 🧠
    }
}

/// Get color for resource category
fn get_category_color(category: &str) -> egui::Color32 {
    theme::category_color(category)
}

/// Resource popup that is currently open (if any)
#[derive(Resource, Default)]
pub(super) struct OpenResourcePopup {
    /// Which category is open, and where to anchor the popup
    open: Option<(String, egui::Rect)>,
}

/// Render the resources bar at the top of the screen (above the menu)
pub(super) fn ui_resources_bar(
    mut contexts: EguiContexts,
    mut pending_research: ResMut<PendingResearchActions>,
    budget: Res<GlobalBudget>,
    rate_tracker: Res<ResourceRateTracker>,
    research_state: Res<ResearchState>,
    population_query: Query<(
        &Population,
        Option<&crate::plugins::solar_system::CelestialBody>,
    )>,
    mut open_popup: Local<OpenResourcePopup>,
    research_projects: Query<&ResearchProject>,
    engineering_projects: Query<&EngineeringProject>,
    research_teams: Query<&ResearchTeam>,
    technologies: Res<TechnologiesData>,
    sim_time: Res<SimulationTime>,
    time: Res<Time<Real>>,
    ui_prefs: Res<ResearchUiPreferences>,
) {
    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    // Calculate total population
    let total_population: f64 = population_query.iter().map(|(p, _)| p.count).sum();

    egui::TopBottomPanel::top("resources_bar")
        .min_height(40.0)
        .show(ctx, |ui| {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add_space(10.0);

                // Show resource categories
                for (category_name, resources) in ResourceType::by_category() {
                    // Calculate total for category
                    let category_total: f64 =
                        resources.iter().map(|r| budget.get_stockpile(r)).sum();
                    let category_rate: f64 = resources
                        .iter()
                        .map(|r| rate_tracker.get_resource_rate(r))
                        .sum();

                    let icon = get_resource_category_icon(category_name);
                    let color = get_category_color(category_name);
                    let text_color = theme::TEXT;

                    let is_this_open = open_popup
                        .open
                        .as_ref()
                        .is_some_and(|(n, _)| n == category_name);

                    // Use a Frame for the category display
                    let response = egui::Frame::NONE
                        .inner_margin(egui::Margin::symmetric(1, 2))
                        .show(ui, |ui| {
                            ui.horizontal_centered(|ui| {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(icon).size(16.0).color(color),
                                    )
                                    .selectable(false),
                                );
                                ui.add_space(1.0);
                                ui.vertical(|ui| {
                                    ui.set_min_width(68.0); // Fixed width to prevent wiggling
                                    ui.set_max_width(68.0);
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(format_mass(category_total))
                                                .size(13.0)
                                                .color(text_color),
                                        )
                                        .selectable(false),
                                    );
                                    let (rate_text, rate_color) =
                                        format_rate_monthly(category_rate);
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(rate_text)
                                                .size(10.0)
                                                .color(rate_color),
                                        )
                                        .selectable(false),
                                    );
                                });
                            });
                        })
                        .response;

                    let interact = response.interact(egui::Sense::click());

                    // Hover and open-state border effect
                    if interact.hovered() || is_this_open {
                        ui.painter().rect_stroke(
                            interact.rect,
                            2.0,
                            egui::Stroke::new(1.0, color),
                            egui::StrokeKind::Outside,
                        );
                    }

                    // Toggle popup on click
                    if interact.clicked() {
                        if is_this_open {
                            open_popup.open = None;
                        } else {
                            open_popup.open = Some((category_name.to_string(), interact.rect));
                        }
                    }

                    ui.add_space(4.0);
                }

                // Research Points display
                {
                    let rp_color = theme::RP_BLUE;
                    let text_color = theme::TEXT;
                    let warning_color = theme::RED;
                    let is_rp_open = open_popup
                        .open
                        .as_ref()
                        .is_some_and(|(n, _)| n == "ResearchPoints");

                    // Find active research projects
                    let mut active_rps: Vec<_> =
                        research_projects.iter().filter(|p| p.active).collect();
                    active_rps.sort_by(|a, b| {
                        b.progress_percent()
                            .partial_cmp(&a.progress_percent())
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });

                    let furthest_rp = active_rps.first();
                    let has_active_rp = !active_rps.is_empty();

                    // Warning flash
                    let flash = if !has_active_rp && ui_prefs.show_inactive_warning {
                        (time.elapsed_secs() * 5.0).sin().abs()
                    } else {
                        0.0
                    };

                    let border_color = if flash > 0.5 {
                        warning_color
                    } else {
                        egui::Color32::TRANSPARENT
                    };

                    let response = egui::Frame::NONE
                        .inner_margin(egui::Margin::symmetric(1, 2))
                        .stroke(egui::Stroke::new(
                            if flash > 0.0 { 2.0 } else { 0.0 },
                            border_color,
                        ))
                        .show(ui, |ui| {
                            ui.horizontal_centered(|ui| {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new("🔬").size(20.0).color(rp_color),
                                    )
                                    .selectable(false),
                                );
                                ui.add_space(1.0);
                                ui.vertical(|ui| {
                                    ui.set_min_width(115.0); // Fixed width to prevent wiggling
                                    ui.set_max_width(115.0);

                                    if let Some(project) = furthest_rp {
                                        if let Some(tech) =
                                            technologies.technologies.get(&project.tech_id)
                                        {
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(&tech.name)
                                                        .size(12.0)
                                                        .color(text_color),
                                                )
                                                .selectable(false),
                                            );

                                            let progress_fraction = project.progress_percent();
                                            ui.add(
                                                egui::ProgressBar::new(progress_fraction)
                                                    .desired_width(100.0)
                                                    .desired_height(4.0)
                                                    .fill(theme::RP_BLUE),
                                            );
                                        } else {
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new("Unknown Project")
                                                        .size(10.0)
                                                        .color(text_color),
                                                )
                                                .selectable(false),
                                            );
                                        }
                                    } else {
                                        let warning_text = if !has_active_rp {
                                            "No Active Research!"
                                        } else {
                                            "Idle"
                                        };
                                        let warning_text_color = if flash > 0.5 {
                                            warning_color
                                        } else {
                                            text_color
                                        };
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(warning_text)
                                                    .size(10.0)
                                                    .color(warning_text_color),
                                            )
                                            .selectable(false),
                                        );
                                    }
                                });
                            });
                        })
                        .response;

                    let interact = response.interact(egui::Sense::click());
                    if interact.hovered() || is_rp_open {
                        ui.painter().rect_stroke(
                            interact.rect,
                            2.0,
                            egui::Stroke::new(1.0, rp_color),
                            egui::StrokeKind::Outside,
                        );
                    }

                    if interact.double_clicked() {
                        pending_research.navigate_to_available_tab = true;
                        open_popup.open = None;
                    } else if interact.clicked() {
                        if is_rp_open {
                            open_popup.open = None;
                        } else {
                            open_popup.open = Some(("ResearchPoints".to_string(), interact.rect));
                        }
                    }
                    ui.add_space(4.0);
                }

                // Engineering Points display
                {
                    let ep_color = theme::EP_TEAL;
                    let text_color = theme::TEXT;
                    let warning_color = theme::RED;
                    let is_ep_open = open_popup
                        .open
                        .as_ref()
                        .is_some_and(|(n, _)| n == "EngineeringPoints");

                    // Find active engineering projects
                    let mut active_eps: Vec<_> = engineering_projects.iter().collect();
                    active_eps.sort_by(|a, b| {
                        b.progress_percent()
                            .partial_cmp(&a.progress_percent())
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });

                    let furthest_ep = active_eps.first();
                    let has_active_ep = !active_eps.is_empty();

                    // Warning flash
                    let flash = if !has_active_ep && ui_prefs.show_inactive_warning {
                        (time.elapsed_secs() * 5.0).sin().abs()
                    } else {
                        0.0
                    };

                    let border_color = if flash > 0.5 {
                        warning_color
                    } else {
                        egui::Color32::TRANSPARENT
                    };

                    let response = egui::Frame::NONE
                        .inner_margin(egui::Margin::symmetric(1, 2))
                        .stroke(egui::Stroke::new(
                            if flash > 0.0 { 2.0 } else { 0.0 },
                            border_color,
                        ))
                        .show(ui, |ui| {
                            ui.horizontal_centered(|ui| {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new("⚙").size(20.0).color(ep_color),
                                    )
                                    .selectable(false),
                                );
                                ui.add_space(1.0);
                                ui.vertical(|ui| {
                                    ui.set_min_width(115.0); // Fixed width to prevent wiggling
                                    ui.set_max_width(115.0);

                                    if let Some(project) = furthest_ep {
                                        let name = technologies
                                            .components
                                            .get(&project.component_id)
                                            .map(|c| c.name.as_str())
                                            .unwrap_or("Unknown Component");
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(name)
                                                    .size(12.0)
                                                    .color(text_color),
                                            )
                                            .selectable(false),
                                        );

                                        let progress_fraction = project.progress_percent();
                                        ui.add(
                                            egui::ProgressBar::new(progress_fraction)
                                                .desired_width(100.0)
                                                .desired_height(4.0)
                                                .fill(theme::RP_BLUE),
                                        );
                                    } else {
                                        let warning_text = if !has_active_ep {
                                            "No Active Eng.!"
                                        } else {
                                            "Idle"
                                        };
                                        let warning_text_color = if flash > 0.5 {
                                            warning_color
                                        } else {
                                            text_color
                                        };
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(warning_text)
                                                    .size(10.0)
                                                    .color(warning_text_color),
                                            )
                                            .selectable(false),
                                        );
                                    }
                                });
                            });
                        })
                        .response;

                    let interact = response.interact(egui::Sense::click());
                    if interact.hovered() || is_ep_open {
                        ui.painter().rect_stroke(
                            interact.rect,
                            2.0,
                            egui::Stroke::new(1.0, ep_color),
                            egui::StrokeKind::Outside,
                        );
                    }

                    if interact.double_clicked() {
                        pending_research.navigate_to_available_engineering_tab = true;
                        open_popup.open = None;
                    } else if interact.clicked() {
                        if is_ep_open {
                            open_popup.open = None;
                        } else {
                            open_popup.open =
                                Some(("EngineeringPoints".to_string(), interact.rect));
                        }
                    }
                }

                // Push to the right side
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(10.0);

                    // Kardashev scale calculation (based on total power)
                    // type I: 10^16 W, Type II: 10^26 W. Scale is logarithmic.
                    // K = (log10(Power_in_Watts) - 6) / 10 is the Carl Sagan formula.
                    let produced_watts = budget.energy_grid.produced.max(1.0); // avoid log(0) or negative
                    let kardashev = (produced_watts.log10() - 6.0) / 10.0;

                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(format!("Type {:.3}", kardashev.max(0.0)))
                                .size(14.0)
                                .color(theme::CAT_STRATEGIC),
                        )
                        .selectable(false),
                    );

                    ui.separator();

                    // Power grid status
                    // Color code power: Green if surplus, Red if deficit
                    let net_power = budget.net_power();
                    let power_color = if net_power >= 0.0 {
                        theme::GREEN
                    } else {
                        theme::RED
                    };

                    let is_power_open = open_popup
                        .open
                        .as_ref()
                        .is_some_and(|(n, _)| n == "Power");

                    // Power generation display (clickable with tooltip)
                    let response = egui::Frame::NONE
                        .inner_margin(egui::Margin::symmetric(1, 2))
                        .show(ui, |ui| {
                            ui.horizontal_centered(|ui| {
                                ui.set_min_width(82.0); // Fixed width to prevent wiggling
                                ui.set_max_width(82.0);
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(format!(
                                            "⚡ {}",
                                            format_power(budget.energy_grid.produced)
                                        ))
                                        .size(14.0)
                                        .strong()
                                        .color(power_color),
                                    )
                                    .selectable(false),
                                );
                            });
                        })
                        .response;

                    let interact = response.interact(egui::Sense::click());

                    if interact.hovered() || is_power_open {
                        ui.painter().rect_stroke(
                            interact.rect,
                            2.0,
                            egui::Stroke::new(1.0, power_color),
                            egui::StrokeKind::Outside,
                        );
                        interact
                            .clone()
                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                    }

                    if interact.clicked() {
                        if is_power_open {
                            open_popup.open = None;
                        } else {
                            open_popup.open = Some(("Power".to_string(), interact.rect));
                        }
                    }

                    ui.separator();

                    // Treasury / Financial status
                    let balance = budget.balance_per_year();
                    let treasury_color = if balance >= 0.0 {
                        theme::GOLD
                    } else {
                        theme::RED
                    };

                    let is_treasury_open = open_popup
                        .open
                        .as_ref()
                        .is_some_and(|(n, _)| n == "Treasury");

                    let treasury_response = egui::Frame::NONE
                        .inner_margin(egui::Margin::symmetric(1, 2))
                        .show(ui, |ui| {
                            ui.horizontal_centered(|ui| {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new("💰").size(20.0).color(treasury_color),
                                    )
                                    .selectable(false),
                                );
                                ui.add_space(1.0);
                                ui.scope(|ui| {
                                    // Fixed width to prevent layout issues in right-to-left container
                                    ui.set_min_width(90.0);
                                    ui.set_max_width(90.0);
                                    ui.vertical(|ui| {
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(format_currency(
                                                    budget.treasury,
                                                ))
                                                .size(14.0)
                                                .strong()
                                                .color(treasury_color),
                                            )
                                            .selectable(false),
                                        );
                                        let balance_sign = if balance >= 0.0 { "+" } else { "" };
                                        let balance_text = format!(
                                            "{}{}/yr",
                                            balance_sign,
                                            format_currency(balance)
                                        );
                                        let balance_color = if balance >= 0.0 {
                                            theme::GREEN
                                        } else {
                                            theme::RED
                                        };
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(balance_text)
                                                    .size(10.0)
                                                    .color(balance_color),
                                            )
                                            .selectable(false),
                                        );
                                    });
                                });
                            });
                        })
                        .response;

                    let treasury_interact = treasury_response.interact(egui::Sense::click());

                    if treasury_interact.hovered() || is_treasury_open {
                        ui.painter().rect_stroke(
                            treasury_interact.rect,
                            2.0,
                            egui::Stroke::new(1.0, treasury_color),
                            egui::StrokeKind::Outside,
                        );
                        treasury_interact
                            .clone()
                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                    }

                    if treasury_interact.clicked() {
                        if is_treasury_open {
                            open_popup.open = None;
                        } else {
                            open_popup.open =
                                Some(("Treasury".to_string(), treasury_interact.rect));
                        }
                    }

                    ui.separator();

                    // Population
                    let is_pop_open = open_popup
                        .open
                        .as_ref()
                        .is_some_and(|(n, _)| n == "Population");

                    // Use a Frame for the population display
                    let pop_response = egui::Frame::NONE
                        .inner_margin(egui::Margin::symmetric(1, 2))
                        .show(ui, |ui| {
                            ui.horizontal_centered(|ui| {
                                ui.set_min_width(68.0); // Fixed width to prevent wiggling
                                ui.set_max_width(68.0);
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(format_population(total_population))
                                            .size(16.0),
                                    )
                                    .selectable(false),
                                );
                                ui.add_space(1.0);
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new("👥").size(20.0).color(theme::TEXT),
                                    )
                                    .selectable(false),
                                );
                            });
                        })
                        .response;

                    let pop_interact = pop_response.interact(egui::Sense::click());

                    if pop_interact.hovered() || is_pop_open {
                        ui.painter().rect_stroke(
                            pop_interact.rect,
                            2.0,
                            egui::Stroke::new(1.0, theme::TEXT),
                            egui::StrokeKind::Outside,
                        );
                        pop_interact
                            .clone()
                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                    }

                    if pop_interact.clicked() {
                        if is_pop_open {
                            open_popup.open = None;
                        } else {
                            open_popup.open = Some(("Population".to_string(), pop_interact.rect));
                        }
                    }
                });
            });
        });

    // Render the resource popup as a floating egui::Window OUTSIDE the panel
    // so it is not clipped by the TopBottomPanel's bounds.
    if let Some((ref cat_name, anchor_rect)) = open_popup.open.clone() {
        if cat_name == "Power" {
            let mut still_open = true;
            // Determine color from budget - recalculate here
            let net_power = budget.net_power();
            let power_color = if net_power >= 0.0 {
                theme::GREEN
            } else {
                theme::RED
            };

            let window_response = egui::Window::new("Power Breakdown")
                .id(egui::Id::new("power_breakdown_window"))
                .fixed_pos(egui::pos2(anchor_rect.left(), anchor_rect.bottom() + 2.0))
                .collapsible(false)
                .resizable(false)
                .title_bar(false)
                .open(&mut still_open)
                .frame(egui::Frame::popup(ctx.style().as_ref()))
                .show(ctx, |ui| {
                    ui.set_min_width(220.0);
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new("⚡").size(18.0).color(power_color),
                            )
                            .selectable(false),
                        );
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new("Power Production")
                                    .size(16.0)
                                    .strong()
                                    .color(power_color),
                            )
                            .selectable(false),
                        );
                    });
                    ui.separator();

                    let sources = [
                        PowerSourceType::Planet,
                        PowerSourceType::Station,
                        PowerSourceType::Ship,
                        PowerSourceType::Asteroid,
                    ];

                    let mut has_sources = false;
                    for source in sources {
                        let amount = budget.power_breakdown.get(&source).copied().unwrap_or(0.0);
                        if amount > 0.0 {
                            has_sources = true;
                            ui.horizontal(|ui| {
                                ui.add(egui::Label::new(format!("{}", source)).selectable(false));
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(format_power(amount)).strong(),
                                            )
                                            .selectable(false),
                                        );
                                    },
                                );
                            });
                        }
                    }

                    if !has_sources {
                        ui.add(egui::Label::new("No active power generation").selectable(false));
                    }

                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Label::new(egui::RichText::new("Total").strong())
                                .selectable(false),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(format_power(budget.energy_grid.produced))
                                        .strong()
                                        .color(power_color),
                                )
                                .selectable(false),
                            );
                        });
                    });
                });

            // Close if clicked outside
            if let Some(inner_response) = window_response {
                if ctx.input(|i| i.pointer.any_pressed()) {
                    if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                        if !inner_response.response.rect.contains(pos) && !anchor_rect.contains(pos)
                        {
                            open_popup.open = None;
                        }
                    }
                }
            }

            if !still_open {
                open_popup.open = None;
            }
        } else if cat_name == "Treasury" {
            let mut still_open = true;
            let balance = budget.balance_per_year();
            let balance_color = if balance >= 0.0 {
                theme::GREEN
            } else {
                theme::RED
            };

            let window_response = egui::Window::new("Treasury Breakdown")
                .id(egui::Id::new("treasury_breakdown_window"))
                .fixed_pos(egui::pos2(anchor_rect.left(), anchor_rect.bottom() + 2.0))
                .collapsible(false)
                .resizable(false)
                .title_bar(false)
                .open(&mut still_open)
                .frame(egui::Frame::popup(ctx.style().as_ref()))
                .show(ctx, |ui| {
                    ui.set_min_width(220.0);
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new("💰").size(18.0).color(theme::GOLD),
                            )
                            .selectable(false),
                        );
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new("Financial Overview")
                                    .size(16.0)
                                    .strong(),
                            )
                            .selectable(false),
                        );
                    });
                    ui.separator();

                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new("Treasury:").selectable(false));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(format_currency(budget.treasury)).strong(),
                                )
                                .selectable(false),
                            );
                        });
                    });

                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new("Income/yr:").selectable(false));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(format_currency(budget.income_per_year))
                                        .color(theme::GREEN),
                                )
                                .selectable(false),
                            );
                        });
                    });

                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new("Expenses/yr:").selectable(false));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(format_currency(budget.expenses_per_year))
                                        .color(theme::RED),
                                )
                                .selectable(false),
                            );
                        });
                    });

                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Label::new(egui::RichText::new("Balance/yr:").strong())
                                .selectable(false),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(format_currency(balance))
                                        .strong()
                                        .color(balance_color),
                                )
                                .selectable(false),
                            );
                        });
                    });
                });

            if let Some(inner_response) = window_response {
                if ctx.input(|i| i.pointer.any_pressed()) {
                    if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                        if !inner_response.response.rect.contains(pos) && !anchor_rect.contains(pos)
                        {
                            open_popup.open = None;
                        }
                    }
                }
            }

            if !still_open {
                open_popup.open = None;
            }
        } else if cat_name == "ResearchPoints" {
            let mut still_open = true;
            let window_response = egui::Window::new("Research Breakdown")
                .id(egui::Id::new("research_breakdown_window"))
                .fixed_pos(egui::pos2(anchor_rect.left(), anchor_rect.bottom() + 2.0))
                .collapsible(false)
                .resizable(false)
                .title_bar(false)
                .open(&mut still_open)
                .frame(egui::Frame::popup(ctx.style().as_ref()))
                .show(ctx, |ui| {
                    ui.set_min_width(250.0);
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new("🔬").size(18.0).color(theme::RP_BLUE),
                            )
                            .selectable(false),
                        );
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new("Active Research Projects")
                                    .size(16.0)
                                    .strong(),
                            )
                            .selectable(false),
                        );
                    });
                    ui.separator();

                    let mut active_rps: Vec<_> =
                        research_projects.iter().filter(|p| p.active).collect();
                    active_rps.sort_by(|a, b| {
                        let a_progress = if a.required_points > 0.0 {
                            a.progress / a.required_points
                        } else {
                            1.0
                        };
                        let b_progress = if b.required_points > 0.0 {
                            b.progress / b.required_points
                        } else {
                            1.0
                        };
                        b_progress
                            .partial_cmp(&a_progress)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });

                    let total_allocation: f64 = active_rps
                        .iter()
                        .filter(|project| {
                            project.required_points > project.progress
                                && project.rp_allocation_percent > 0.0
                        })
                        .map(|project| project.rp_allocation_percent)
                        .sum();

                    if active_rps.is_empty() {
                        ui.add(egui::Label::new("No active research projects.").selectable(false));
                    } else {
                        for project in &active_rps {
                            if let Some(tech) = technologies.technologies.get(&project.tech_id) {
                                let progress = if project.required_points > 0.0 {
                                    (project.progress / project.required_points * 100.0)
                                        .clamp(0.0, 100.0)
                                } else {
                                    100.0
                                };
                                let end_date_text = estimate_research_project_end_timestamp(
                                    project,
                                    research_teams.get(project.team_id).ok(),
                                    &technologies,
                                    &research_state,
                                    total_allocation,
                                    sim_time.current_timestamp(),
                                )
                                .map(format_timestamp_date_time)
                                .unwrap_or_else(|| "ETA: Paused".to_string());

                                let row = ui.horizontal(|ui| {
                                    ui.add(egui::Label::new(tech.name.as_str()).selectable(false));
                                });
                                let active_info = ActiveProjectInfo {
                                    entity: Entity::PLACEHOLDER,
                                    progress_percent: (progress / 100.0) as f32,
                                    progress: project.progress,
                                    required_points: project.required_points,
                                    allocation_percent: project.rp_allocation_percent,
                                    active: project.active,
                                };
                                row.response.on_hover_ui(|ui| {
                                    render_research_tech_tooltip_content(
                                        ui,
                                        tech,
                                        &technologies,
                                        &research_state,
                                        None,
                                        Some(&active_info),
                                    );
                                });
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(format!("  {}", end_date_text))
                                            .size(10.0)
                                            .color(egui::Color32::GRAY),
                                    )
                                    .selectable(false),
                                );
                                ui.add(
                                    egui::ProgressBar::new((progress / 100.0) as f32)
                                        .desired_width(220.0),
                                );
                                ui.add_space(4.0);
                            }
                        }
                    }
                });

            if let Some(inner_response) = window_response {
                if ctx.input(|i| i.pointer.any_pressed()) {
                    if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                        if !inner_response.response.rect.contains(pos) && !anchor_rect.contains(pos)
                        {
                            open_popup.open = None;
                        }
                    }
                }
            }

            if !still_open {
                open_popup.open = None;
            }
        } else if cat_name == "EngineeringPoints" {
            let mut still_open = true;
            let window_response = egui::Window::new("Engineering Breakdown")
                .id(egui::Id::new("engineering_breakdown_window"))
                .fixed_pos(egui::pos2(anchor_rect.left(), anchor_rect.bottom() + 2.0))
                .collapsible(false)
                .resizable(false)
                .title_bar(false)
                .open(&mut still_open)
                .frame(egui::Frame::popup(ctx.style().as_ref()))
                .show(ctx, |ui| {
                    ui.set_min_width(250.0);
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new("⚙").size(18.0).color(theme::EP_TEAL),
                            )
                            .selectable(false),
                        );
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new("Active Engineering Projects")
                                    .size(16.0)
                                    .strong(),
                            )
                            .selectable(false),
                        );
                    });
                    ui.separator();

                    let mut active_eps: Vec<_> = engineering_projects.iter().collect();
                    active_eps.sort_by(|a, b| {
                        let a_progress = if a.required_points > 0.0 {
                            a.progress / a.required_points
                        } else {
                            1.0
                        };
                        let b_progress = if b.required_points > 0.0 {
                            b.progress / b.required_points
                        } else {
                            1.0
                        };
                        b_progress
                            .partial_cmp(&a_progress)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });

                    if active_eps.is_empty() {
                        ui.add(
                            egui::Label::new("No active engineering projects.").selectable(false),
                        );
                    } else {
                        for project in &active_eps {
                            let name = technologies
                                .components
                                .get(&project.component_id)
                                .map(|c| c.name.as_str())
                                .unwrap_or("Unknown Component");
                            let progress = if project.required_points > 0.0 {
                                (project.progress / project.required_points * 100.0)
                                    .clamp(0.0, 100.0)
                            } else {
                                100.0
                            };
                            let end_date_text = estimate_engineering_project_end_timestamp(
                                project,
                                research_teams.get(project.team_id).ok(),
                                &research_state,
                                sim_time.current_timestamp(),
                            )
                            .map(format_timestamp_date_time)
                            .unwrap_or_else(|| "ETA: Unassigned".to_string());

                            ui.horizontal(|ui| {
                                ui.add(egui::Label::new(name).selectable(false));
                            });
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(format!("  {}", end_date_text))
                                        .size(10.0)
                                        .color(egui::Color32::GRAY),
                                )
                                .selectable(false),
                            );
                            ui.add(
                                egui::ProgressBar::new((progress / 100.0) as f32)
                                    .desired_width(220.0),
                            );
                            ui.add_space(4.0);
                        }
                    }
                });

            if let Some(inner_response) = window_response {
                if ctx.input(|i| i.pointer.any_pressed()) {
                    if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                        if !inner_response.response.rect.contains(pos) && !anchor_rect.contains(pos)
                        {
                            open_popup.open = None;
                        }
                    }
                }
            }

            if !still_open {
                open_popup.open = None;
            }
        } else if cat_name == "Population" {
            let mut still_open = true;
            let window_response = egui::Window::new("Population Breakdown")
                .id(egui::Id::new("population_breakdown_window"))
                .fixed_pos(egui::pos2(anchor_rect.left(), anchor_rect.bottom() + 2.0))
                .collapsible(false)
                .resizable(false)
                .title_bar(false)
                .open(&mut still_open)
                .frame(egui::Frame::popup(ctx.style().as_ref()))
                .show(ctx, |ui| {
                    ui.set_min_width(220.0);
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new("👥").size(18.0).color(theme::TEXT),
                            )
                            .selectable(false),
                        );
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new("Population")
                                    .size(16.0)
                                    .strong()
                                    .color(theme::TEXT),
                            )
                            .selectable(false),
                        );
                    });
                    ui.separator();

                    // Collect and sort populations
                    let mut pops: Vec<(String, f64)> = population_query
                        .iter()
                        .filter(|(p, _)| p.count > 0.0)
                        .map(|(p, body)| {
                            let name = if let Some(b) = body {
                                b.name.clone()
                            } else {
                                "Unknown".to_string()
                            };
                            (name, p.count)
                        })
                        .collect();

                    // Sort descending
                    pops.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

                    let top_10_count = pops.len().min(10);

                    for (name, count) in pops.iter().take(top_10_count) {
                        ui.horizontal(|ui| {
                            ui.add(egui::Label::new(name.as_str()).selectable(false));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(format_population(*count)).strong(),
                                        )
                                        .selectable(false),
                                    );
                                },
                            );
                        });
                    }

                    // Summarize the rest
                    if pops.len() > 10 {
                        let other_total: f64 = pops.iter().skip(10).map(|(_, c)| c).sum();
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::Label::new(egui::RichText::new("Other").italics())
                                    .selectable(false),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(format_population(other_total))
                                                .italics(),
                                        )
                                        .selectable(false),
                                    );
                                },
                            );
                        });
                    }

                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Label::new(egui::RichText::new("Total").strong())
                                .selectable(false),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(format_population(total_population))
                                        .strong()
                                        .color(theme::TEXT),
                                )
                                .selectable(false),
                            );
                        });
                    });
                });

            // Close if clicked outside
            if let Some(inner_response) = window_response {
                if ctx.input(|i| i.pointer.any_pressed()) {
                    if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                        if !inner_response.response.rect.contains(pos) && !anchor_rect.contains(pos)
                        {
                            open_popup.open = None;
                        }
                    }
                }
            }

            if !still_open {
                open_popup.open = None;
            }
        } else if let Some((_, resources)) = ResourceType::by_category()
            .into_iter()
            .find(|(name, _)| *name == cat_name.as_str())
        {
            let icon = get_resource_category_icon(cat_name);
            let color = get_category_color(cat_name);

            let mut still_open = true;
            let window_response = egui::Window::new(cat_name.as_str())
                .id(egui::Id::new(format!("res_window_{}", cat_name)))
                .fixed_pos(egui::pos2(anchor_rect.left(), anchor_rect.bottom() + 2.0))
                .collapsible(false)
                .resizable(false)
                .title_bar(false)
                .open(&mut still_open)
                .frame(egui::Frame::popup(ctx.style().as_ref()))
                .show(ctx, |ui| {
                    ui.set_min_width(280.0);
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Label::new(egui::RichText::new(icon).size(18.0).color(color))
                                .selectable(false),
                        );
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(cat_name.as_str())
                                    .size(16.0)
                                    .strong()
                                    .color(color),
                            )
                            .selectable(false),
                        );
                    });
                    ui.separator();

                    // Header + data rows in a single grid so columns stay aligned
                    egui::Grid::new(format!("res_popup_{}", cat_name))
                        .num_columns(3)
                        .spacing([20.0, 4.0])
                        .striped(true)
                        .show(ui, |ui| {
                            // Header
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new("Resource").strong().size(11.0),
                                )
                                .selectable(false),
                            );
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new("Stockpile").strong().size(11.0),
                                )
                                .selectable(false),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new("/mo").strong().size(11.0),
                                        )
                                        .selectable(false),
                                    );
                                },
                            );
                            ui.end_row();

                            for resource in &resources {
                                let amount = budget.get_stockpile(resource);
                                let rate = rate_tracker.get_resource_rate(resource);

                                // Icon + Name in one cell
                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(get_resource_icon(resource))
                                                .size(14.0),
                                        )
                                        .selectable(false),
                                    );
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(resource.display_name()).size(12.0),
                                        )
                                        .selectable(false),
                                    );
                                });
                                // Stockpile — left-aligned
                                {
                                    let stock_color = if amount <= 0.0 {
                                        theme::RED
                                    } else if amount < 100.0 && resource.is_critical() {
                                        theme::AMBER
                                    } else {
                                        theme::TEXT
                                    };
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(format_mass(amount))
                                                .monospace()
                                                .size(12.0)
                                                .color(stock_color),
                                        )
                                        .selectable(false),
                                    );
                                }
                                // Rate — right-aligned
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        let (rt, rc) = format_rate_monthly(rate);
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(rt)
                                                    .monospace()
                                                    .size(11.0)
                                                    .color(rc),
                                            )
                                            .selectable(false),
                                        );
                                    },
                                );
                                ui.end_row();
                            }
                        });
                });

            // Close if clicked outside
            if let Some(inner_response) = window_response {
                if ctx.input(|i| i.pointer.any_pressed()) {
                    if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                        if !inner_response.response.rect.contains(pos) && !anchor_rect.contains(pos)
                        {
                            open_popup.open = None;
                        }
                    }
                }
            }

            if !still_open {
                open_popup.open = None;
            }
        } else {
            // Category not found (shouldn't happen), close
            open_popup.open = None;
        }
    }
}

pub(super) fn format_population(count: f64) -> String {
    if count < 1_000.0 {
        return format!("{:.0}", count);
    }
    if count < 1_000_000.0 {
        return format!("{:.1} k", count / 1_000.0);
    }
    if count < 1_000_000_000.0 {
        return format!("{:.1} M", count / 1_000_000.0);
    }
    format!("{:.2} B", count / 1_000_000_000.0)
}
