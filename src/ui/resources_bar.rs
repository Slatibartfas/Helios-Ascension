use bevy::ecs::system::SystemParam;

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
pub(super) fn get_resource_icon(resource: &ResourceType) -> &'static str {
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
        ResourceType::Tritium => "\u{2622}",   // ☢

        // Fissiles
        ResourceType::Uranium => "\u{2622}", // ☢
        ResourceType::Thorium => "\u{26A1}", // ⚡
        ResourceType::Plutonium => "\u{1F9EA}", // 🧪

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

#[derive(SystemParam)]
pub(super) struct ResourceBarPowerQueries<'w, 's> {
    body_query: Query<
        'w,
        's,
        (
            Entity,
            &'static CelestialBody,
            Option<&'static SystemId>,
            Option<&'static LogicalParent>,
            Option<&'static KeplerOrbit>,
            Option<&'static Colony>,
            Option<&'static PlanetResources>,
            Option<&'static SurveyLevel>,
            Option<&'static crate::economy::components::PowerGenerator>,
            Option<&'static MiningOperation>,
        ),
    >,
    star_query: Query<
        'w,
        's,
        (&'static CelestialBody, &'static SystemId),
        With<crate::plugins::solar_system::Star>,
    >,
    buildings_data: Option<Res<'w, BuildingsData>>,
}

const CONTEXT_TILE_WIDTH: f32 = 88.0;
const CONTEXT_TILE_HEIGHT: f32 = 28.0;
const CONTEXT_NAME_FONT_SIZE: f32 = 11.5;

fn render_context_name_marquee(ui: &mut egui::Ui, text: &str) {
    let font_id = egui::FontId::proportional(CONTEXT_NAME_FONT_SIZE);
    let color = theme::TEXT_VALUE;
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), font_id.clone(), color);
    let text_size = galley.size();
    let row_height = text_size.y.max(12.0);
    let available_width = ui.available_width().max(1.0);
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(available_width, row_height), egui::Sense::hover());
    let clip = rect;
    let painter = ui.painter().with_clip_rect(clip);

    if text_size.x <= clip.width() {
        painter.text(
            clip.left_center(),
            egui::Align2::LEFT_CENTER,
            text,
            font_id,
            color,
        );
    } else {
        let gap = 36.0_f32;
        let cycle = text_size.x + gap;
        let speed = 35.0_f64;
        let t = ui.ctx().input(|i| i.time);
        let offset_x = ((t * speed) % cycle as f64) as f32;
        let y = clip.top() + (clip.height() - text_size.y) * 0.5;
        let x0 = clip.left() - offset_x;
        painter.galley(egui::pos2(x0, y), galley.clone(), color);
        let x1 = x0 + cycle;
        if x1 < clip.right() + text_size.x {
            painter.galley(egui::pos2(x1, y), galley, color);
        }
        ui.ctx().request_repaint();
    }
}

/// Render the resources bar at the top of the screen (above the menu)
pub(super) fn ui_resources_bar(
    mut contexts: EguiContexts,
    mut pending_research: ResMut<PendingResearchActions>,
    budget: Res<GlobalBudget>,
    contextual: Res<crate::economy::ContextualStockpile>,
    rate_tracker: Res<ResourceRateTracker>,
    power_popup_queries: ResourceBarPowerQueries,
    research_state: Res<ResearchState>,
    population_query: Query<(
        &Population,
        Option<&crate::plugins::solar_system::CelestialBody>,
        Option<&crate::colony::Colony>,
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
    let total_population: f64 = population_query.iter().map(|(p, _, _)| p.count).sum();

    egui::TopBottomPanel::top("resources_bar")
        .exact_height(40.0)
        .show(ctx, |ui| {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add_space(4.0);

                // Context label (e.g. "Sol System" or "All Systems")
                ui.allocate_ui_with_layout(
                    egui::vec2(CONTEXT_TILE_WIDTH, CONTEXT_TILE_HEIGHT),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.set_min_width(CONTEXT_TILE_WIDTH);
                        ui.set_max_width(CONTEXT_TILE_WIDTH);
                        ui.set_min_height(CONTEXT_TILE_HEIGHT);
                        ui.set_max_height(CONTEXT_TILE_HEIGHT);
                        ui.spacing_mut().item_spacing = egui::vec2(2.0, 0.0);
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new("📍")
                                        .size(15.0)
                                        .color(theme::ACCENT),
                                )
                                .selectable(false),
                            );
                            ui.add_space(2.0);
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 0.0;
                                ui.set_width((CONTEXT_TILE_WIDTH - 22.0).max(1.0));
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new("CURRENT SYSTEM")
                                            .size(6.0)
                                            .color(theme::TEXT_HINT),
                                    )
                                    .selectable(false),
                                );
                                render_context_name_marquee(ui, &contextual.context_label);
                            });
                        });
                    },
                );
                ui.add_space(1.0);
                ui.separator();
                ui.add_space(1.0);

                // Show resource categories
                for (category_name, resources) in ResourceType::by_category() {
                    // Calculate total for category from contextual stockpile
                    let category_total: f64 = resources.iter().map(|r| contextual.get(r)).sum();
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
            let hierarchy = super::economy_panel::build_economy_hierarchy(
                &power_popup_queries.body_query,
                &power_popup_queries.star_query,
                power_popup_queries.buildings_data.as_deref(),
            );
            let power_rows = super::economy_panel::collect_power_body_rows(&hierarchy);
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
                    ui.set_min_width(300.0);
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

                    if power_rows.is_empty() {
                        ui.add(egui::Label::new("No active power generation").selectable(false));
                    } else {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new("Top Contributing Bodies")
                                    .size(12.0)
                                    .color(theme::TEXT_DIM),
                            )
                            .selectable(false),
                        );
                        ui.add_space(4.0);

                        let top_count = power_rows.len().min(10);
                        for body in power_rows.iter().take(top_count) {
                            let row_response = ui
                                .horizontal(|ui| {
                                    ui.add(
                                        egui::Label::new(format!(
                                            "{} {}",
                                            super::economy_panel::power_body_icon(body.body_type),
                                            body.body_name
                                        ))
                                        .selectable(false),
                                    );
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(format!(
                                                "({})",
                                                body.system_name
                                            ))
                                            .size(10.5)
                                            .color(theme::TEXT_DIM),
                                        )
                                        .selectable(false),
                                    );
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(format_power(
                                                        body.total_generation_watts,
                                                    ))
                                                    .strong(),
                                                )
                                                .selectable(false),
                                            );
                                        },
                                    );
                                })
                                .response;

                            row_response.on_hover_ui(|ui| {
                                super::economy_panel::render_power_body_detail_tooltip(
                                    ui,
                                    body,
                                    power_popup_queries.buildings_data.as_deref(),
                                );
                            });
                        }

                        let rest_count = power_rows.len().saturating_sub(top_count);
                        let rest_generation: f64 = power_rows
                            .iter()
                            .skip(top_count)
                            .map(|body| body.total_generation_watts)
                            .sum();
                        if rest_count > 0 && rest_generation > 0.0 {
                            ui.separator();
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(format!(
                                            "Rest ({} bodies)",
                                            rest_count
                                        ))
                                        .color(theme::TEXT_DIM),
                                    )
                                    .selectable(false),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(format_power(rest_generation))
                                                    .color(theme::TEXT_DIM),
                                            )
                                            .selectable(false),
                                        );
                                    },
                                );
                            });
                        }
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

                    // Collect colony data: (name, population, housing_cap, growth_per_year)
                    let mut pops: Vec<(String, f64, f64, f64)> = population_query
                        .iter()
                        .filter(|(p, _, _)| p.count > 0.0)
                        .map(|(p, body, colony_opt)| {
                            let name = if let Some(b) = body {
                                b.name.clone()
                            } else {
                                "Unknown".to_string()
                            };
                            let housing = colony_opt
                                .map(|c| c.housing_capacity())
                                .unwrap_or(0.0);
                            let growth_yr = colony_opt
                                .map(|c| c.population_growth_per_year(1.0))
                                .unwrap_or(0.0);
                            (name, p.count, housing, growth_yr)
                        })
                        .collect();

                    // Sort descending by population
                    pops.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

                    // Max population for relative bar scaling
                    let max_pop = pops.first().map(|(_, c, _, _)| *c).unwrap_or(1.0).max(1.0);

                    let top_count = pops.len().min(10);

                    for (name, count, housing, growth_yr) in pops.iter().take(top_count) {
                        ui.add_space(3.0);
                        // Name (left) + population count + growth rate (right)
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(name.as_str()).size(11.0),
                                )
                                .selectable(false),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let growth_month = growth_yr / 12.0;
                                    let (growth_text, growth_color) = if growth_month > 0.5 {
                                        (
                                            format!("+{}/mo", format_population(growth_month)),
                                            theme::GREEN,
                                        )
                                    } else if growth_month < -0.5 {
                                        (
                                            format!("{}/mo", format_population(growth_month)),
                                            theme::RED,
                                        )
                                    } else {
                                        ("\u{2014}".to_string(), theme::TEXT_DIM)
                                    };
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(growth_text)
                                                .size(10.0)
                                                .color(growth_color),
                                        )
                                        .selectable(false),
                                    );
                                    ui.add_space(4.0);
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(format_population(*count))
                                                .size(11.0)
                                                .strong(),
                                        )
                                        .selectable(false),
                                    );
                                },
                            );
                        });

                        // Progress bar: outer = relative to largest, inner = housing utilisation
                        let bar_fill = (*count / max_pop) as f32;
                        let housing_fill = if *housing > 0.0 {
                            (*count / housing).min(1.0) as f32
                        } else {
                            0.0
                        };
                        let bar_color = if housing_fill >= 0.99 {
                            theme::RED
                        } else if housing_fill > 0.85 {
                            theme::AMBER
                        } else {
                            egui::Color32::from_rgb(100, 180, 255)
                        };
                        let (rect, _) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), 4.0),
                            egui::Sense::hover(),
                        );
                        ui.painter().rect_filled(
                            egui::Rect::from_min_size(
                                rect.min,
                                egui::vec2(rect.width() * bar_fill, rect.height()),
                            ),
                            0.0,
                            bar_color.linear_multiply(0.3),
                        );
                        if *housing > 0.0 {
                            ui.painter().rect_filled(
                                egui::Rect::from_min_size(
                                    rect.min,
                                    egui::vec2(
                                        rect.width() * bar_fill * housing_fill,
                                        rect.height(),
                                    ),
                                ),
                                0.0,
                                bar_color,
                            );
                        }
                        if *housing > 0.0 {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(format!(
                                        "  \u{1F3E0} {:.0}% of {}",
                                        housing_fill * 100.0,
                                        format_population(*housing)
                                    ))
                                    .size(9.0)
                                    .color(theme::TEXT_DIM),
                                )
                                .selectable(false),
                            );
                        }
                    }

                    // Summarize colonies beyond top 10
                    if pops.len() > 10 {
                        let other_total: f64 = pops.iter().skip(10).map(|(_, c, _, _)| c).sum();
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
                    // Total + aggregate growth rate
                    let total_growth_yr: f64 = population_query
                        .iter()
                        .filter_map(|(p, _, c)| {
                            if p.count > 0.0 {
                                Some(c.map(|col| col.population_growth_per_year(1.0)).unwrap_or(0.0))
                            } else {
                                None
                            }
                        })
                        .sum();
                    let total_growth_month = total_growth_yr / 12.0;
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Label::new(egui::RichText::new("Total").strong())
                                .selectable(false),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let (g_text, g_color) = if total_growth_month >= 1.0 {
                                (format!("+{}/mo", format_population(total_growth_month)), theme::GREEN)
                            } else if total_growth_month < -1.0 {
                                (format!("{}/mo", format_population(total_growth_month)), theme::RED)
                            } else {
                                ("\u{2014}".to_string(), theme::TEXT_DIM)
                            };
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(g_text).size(10.0).color(g_color),
                                )
                                .selectable(false),
                            );
                            ui.add_space(4.0);
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
                                let amount = contextual.get(resource);
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
