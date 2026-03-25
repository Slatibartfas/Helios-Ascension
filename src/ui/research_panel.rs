use super::tech_tree::{render_tech_tree_tab, tech_category_color};
use super::*;
use std::collections::BTreeSet;

/// Info about an active/paused research project, for UI display
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(super) struct ActiveProjectInfo {
    pub(super) entity: Entity,
    pub(super) progress_percent: f32,
    pub(super) progress: f64,
    pub(super) required_points: f64,
    pub(super) allocation_percent: f64,
    pub(super) active: bool,
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

fn draw_tab_button(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(label).size(13.5).color(if selected {
            theme::ACCENT
        } else {
            theme::TEXT
        }))
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

fn draw_section_title(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.label(
        egui::RichText::new(title)
            .font(theme::heading())
            .color(theme::ACCENT),
    );
    if !subtitle.is_empty() {
        ui.label(
            egui::RichText::new(subtitle)
                .size(11.0)
                .color(theme::TEXT_DIM),
        );
    }
    ui.add_space(6.0);
}

pub(super) fn ui_research_panels(
    mut contexts: EguiContexts,
    active_menu: Res<ActiveMenu>,
    research_state: Res<ResearchState>,
    mut tech_data: ResMut<TechnologiesData>,
    mut debug_settings: ResMut<crate::research::ResearchDebugSettings>,
    mut edit_state: ResMut<TechTreeEditState>,
    mut pending_research: ResMut<crate::research::PendingResearchActions>,
    research_icons: Option<Res<ResearchIcons>>,
    mut icon_textures: Local<HashMap<TechCategory, egui::TextureId>>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    research_projects: Query<(Entity, &ResearchProject, &ResearchTeam)>,
    engineering_projects: Query<(&EngineeringProject, &ResearchTeam)>,
    all_teams: Query<(Entity, &ResearchTeam)>,
    team_capacity: Res<ResearchTeamCapacity>,
    mut selected_tab: Local<usize>,
    mut ui_prefs: ResMut<ResearchUiPreferences>,
) {
    if active_menu.current != GameMenu::Research {
        return;
    }

    // Handle navigate-to-available-tab requests (e.g. from tree view Start Research)
    if pending_research.navigate_to_available_tab {
        *selected_tab = 2;
        pending_research.navigate_to_available_tab = false;
    }

    if pending_research.navigate_to_available_engineering_tab {
        *selected_tab = 3;
        pending_research.navigate_to_available_engineering_tab = false;
    }

    if let Some(target) = pending_research.navigate_to_engineering_target.take() {
        ui_prefs.selected_engineering_target = Some(target);
    }

    // Convert loaded handles to egui TextureIds
    if let Some(icons) = &research_icons {
        for (cat, handle) in &icons.handles {
            icon_textures.entry(*cat).or_insert_with(|| {
                contexts.add_image(bevy_egui::EguiTextureHandle::Strong(handle.clone()))
            });
        }
    }
    let icon_textures = &*icon_textures;

    // Toggle debug mode with F12
    if keyboard_input.just_pressed(KeyCode::F12) {
        debug_settings.enabled = !debug_settings.enabled;
    }

    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    // Add Modifier Dialog (separate window)
    if debug_settings.modifier_dialog_show {
        let mut dialog_type_index = debug_settings.modifier_dialog_type_index;
        let mut dialog_value = debug_settings.modifier_dialog_value_input.clone();
        let mut new_modifier: Option<(ModifierType, f64)> = None;
        let mut close_dialog = false;

        egui::Window::new("Add Debug Modifier")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("Select modifier type:");

                let available_modifiers = ModifierType::all_for_debug();
                let modifier_names: Vec<String> = available_modifiers
                    .iter()
                    .map(|m| m.display_name())
                    .collect();

                egui::ComboBox::from_label("Modifier Type")
                    .selected_text(&modifier_names[dialog_type_index])
                    .show_ui(ui, |ui| {
                        for (i, name) in modifier_names.iter().enumerate() {
                            ui.selectable_value(&mut dialog_type_index, i, name);
                        }
                    });

                ui.add_space(5.0);
                ui.label("Value (percentage, e.g. 50 for +50%, -25 for -25%):");
                ui.text_edit_singleline(&mut dialog_value);

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button("Add").clicked() {
                        if let Ok(value) = dialog_value.parse::<f64>() {
                            let modifier_type = available_modifiers[dialog_type_index].clone();
                            new_modifier = Some((modifier_type, value));
                            close_dialog = true;
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        close_dialog = true;
                    }
                });
            });

        debug_settings.modifier_dialog_type_index = dialog_type_index;
        debug_settings.modifier_dialog_value_input = dialog_value;
        if close_dialog {
            debug_settings.modifier_dialog_show = false;
            debug_settings.modifier_dialog_value_input.clear();
        }
        if let Some((mt, val)) = new_modifier {
            debug_settings.debug_modifiers.insert(mt, val);
        }
    }

    // Main panel - Tabbed interface (no left sidebar)
    egui::CentralPanel::default()
        .frame(theme::central_frame())
        .show(ctx, |ui| {
        // Disable text selection cursor everywhere in the research menu
        ui.style_mut().interaction.selectable_labels = false;

        draw_menu_header(
            ui,
            "RESEARCH",
            "Scientific direction, engineering throughput, and technology pipeline control.",
        );

        // Debug mode panel (if enabled)
        if debug_settings.enabled {
            theme::elevated_frame().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("🐛 DEBUG MODE").strong().color(theme::RED));
                    ui.label(egui::RichText::new("(Press F12 to toggle)").italics().small());
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.checkbox(&mut debug_settings.show_all_techs, "Show All Technologies (ignore prerequisites)");
                    ui.checkbox(&mut debug_settings.instant_research, "Instant Research");
                    ui.checkbox(&mut debug_settings.instant_engineering, "Instant Engineering");
                });
                
                // Debug modifiers section
                ui.add_space(5.0);
                ui.label(egui::RichText::new("Debug Modifiers:").strong());
                
                // Display active debug modifiers
                let mut to_remove: Option<ModifierType> = None;
                for (modifier_type, value) in debug_settings.debug_modifiers.iter() {
                    ui.horizontal(|ui| {
                        ui.label(modifier_type.display_name());
                        ui.label(format!("{:+.1}%", value));
                        if ui.button("❌").on_hover_text("Remove modifier").clicked() {
                            to_remove = Some(modifier_type.clone());
                        }
                    });
                }
                if let Some(modifier) = to_remove {
                    debug_settings.debug_modifiers.remove(&modifier);
                }
                
                // Add new modifier button
                if ui.button("➕ Add Debug Modifier").clicked() {
                    debug_settings.modifier_dialog_show = true;
                }
                
                ui.label(egui::RichText::new("⚠ Debug features are for development only and will be removed in release builds")
                    .small()
                    .italics()
                    .color(theme::AMBER));
            });
            ui.add_space(10.0);
        } else {
            // Show subtle hint about debug mode
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Press F12 to toggle debug mode")
                    .small()
                    .italics()
                    .color(theme::TEXT_DIM));
            });
        }
        
        // Tab bar
        ui.horizontal_wrapped(|ui| {
            let tabs = [
                (0usize, "📊 Overview"),
                (1usize, "🌳 Tech Tree"),
                (2usize, "🔬 Research"),
                (3usize, "⚙ Engineering"),
                (4usize, "✦ Bonuses"),
                (5usize, "📚 Archive"),
            ];
            for (tab, label) in tabs {
                if draw_tab_button(ui, label, *selected_tab == tab).clicked() {
                    *selected_tab = tab;
                }
            }
        });
        
        theme::divider(ui);
        
        // Build rich active research info map
        let mut active_research: HashMap<String, ActiveProjectInfo> = HashMap::new();
        for (entity, proj, _team) in research_projects.iter() {
            active_research.insert(proj.tech_id.clone(), ActiveProjectInfo {
                entity,
                progress_percent: proj.progress_percent(),
                progress: proj.progress,
                required_points: proj.required_points,
                allocation_percent: proj.rp_allocation_percent,
                active: proj.active,
            });
        }

        // Tab content
        match *selected_tab {
            0 => render_overview_tab(ui, &research_state, &tech_data, icon_textures, &research_projects, &engineering_projects, &all_teams, &team_capacity, &mut ui_prefs),
            1 => render_tech_tree_tab(ui, &research_state, &mut tech_data, icon_textures, debug_settings.enabled, &mut edit_state, &active_research, &mut pending_research, &mut debug_settings),
            2 => render_available_research_tab(ui, &research_state, &tech_data, icon_textures, &active_research, &mut pending_research, &team_capacity),
            3 => render_available_engineering_tab(
                ui,
                &research_state,
                &tech_data,
                icon_textures,
                &engineering_projects,
                &mut pending_research,
                &team_capacity,
                &mut ui_prefs.selected_engineering_target,
            ),
            4 => render_bonuses_tab(ui, &research_state, &tech_data, icon_textures),
            5 => render_archive_tab(ui, &research_state, &tech_data, icon_textures),
            _ => {},
        }
    });
}
fn render_overview_tab(
    ui: &mut egui::Ui,
    research_state: &ResearchState,
    tech_data: &TechnologiesData,
    icon_textures: &HashMap<TechCategory, egui::TextureId>,
    research_projects: &Query<(Entity, &ResearchProject, &ResearchTeam)>,
    engineering_projects: &Query<(&EngineeringProject, &ResearchTeam)>,
    all_teams: &Query<(Entity, &ResearchTeam)>,
    team_capacity: &ResearchTeamCapacity,
    ui_prefs: &mut ResearchUiPreferences,
) {
    ui.label(
        egui::RichText::new("RESEARCH OVERVIEW")
            .font(theme::heading())
            .color(theme::ACCENT),
    );
    ui.checkbox(
        &mut ui_prefs.show_inactive_warning,
        "Show Inactive Warning in Top Bar",
    );
    ui.add_space(5.0);

    egui::ScrollArea::vertical().show(ui, |ui| {
        // Point Generation
        theme::elevated_frame().show(ui, |ui| {
            ui.label(
                egui::RichText::new("Point Generation")
                    .font(theme::heading())
                    .color(theme::ACCENT),
            );
            ui.separator();

            let rp_per_year = research_state.rp_rate_per_second * 31_557_600.0;
            let ep_per_year = research_state.ep_rate_per_second * 31_557_600.0;

            ui.horizontal(|ui| {
                ui.label("Research Points:");
                ui.label(
                    egui::RichText::new(format!("{:.0} RP/year", rp_per_year))
                        .color(theme::RP_BLUE),
                );
                ui.label(format!(
                    "(Pool: {:.0})",
                    research_state.research_points_available
                ));
            });

            ui.horizontal(|ui| {
                ui.label("Engineering Points:");
                ui.label(
                    egui::RichText::new(format!("{:.0} EP/year", ep_per_year))
                        .color(theme::EP_TEAL),
                );
                ui.label(format!(
                    "(Pool: {:.0})",
                    research_state.engineering_points_available
                ));
            });
        });

        ui.add_space(10.0);

        // Active Research Projects
        theme::elevated_frame().show(ui, |ui| {
            let active_count = research_projects
                .iter()
                .filter(|(_, p, _)| p.active)
                .count();
            let total_count = research_projects.iter().count();
            ui.label(
                egui::RichText::new(format!(
                    "Active Research Projects ({}/{})",
                    active_count, team_capacity.max_research_teams
                ))
                .font(theme::heading())
                .color(theme::ACCENT),
            );
            ui.separator();

            if total_count == 0 {
                ui.label(
                    egui::RichText::new("No active research projects")
                        .italics()
                        .color(theme::TEXT_DIM),
                );
            } else {
                for (entity, project, team) in research_projects.iter() {
                    if let Some(tech) = tech_data.get_tech(&project.tech_id) {
                        let active_info = ActiveProjectInfo {
                            entity,
                            progress_percent: project.progress_percent(),
                            progress: project.progress,
                            required_points: project.required_points,
                            allocation_percent: project.rp_allocation_percent,
                            active: project.active,
                        };
                        ui.horizontal(|ui| {
                            // Info labels in a scope so tooltip hover isn't stolen by progress bar
                            let info_scope = ui.scope(|ui| {
                                ui.label(
                                    egui::RichText::new(if project.active {
                                        "🔬"
                                    } else {
                                        "⏸"
                                    })
                                    .size(14.0),
                                );
                                ui.label(egui::RichText::new(&tech.name).strong());
                                if !project.active {
                                    ui.label(egui::RichText::new("PAUSED").color(theme::AMBER));
                                }
                                ui.label(
                                    egui::RichText::new(format!("({})", team.name))
                                        .size(11.0)
                                        .color(theme::TEXT_DIM),
                                );
                            });
                            info_scope.response.on_hover_ui(|ui| {
                                render_research_tech_tooltip_content(
                                    ui,
                                    tech,
                                    tech_data,
                                    research_state,
                                    Some(icon_textures),
                                    Some(&active_info),
                                );
                            });
                            // Interactive controls outside the tooltip scope
                            ui.add(
                                egui::ProgressBar::new(project.progress_percent())
                                    .text(format!(
                                        "{:.1}% ({:.0}/{:.0} RP)",
                                        project.progress_percent() * 100.0,
                                        project.progress,
                                        project.required_points
                                    ))
                                    .desired_width(180.0),
                            );
                            ui.label(format!(
                                "Alloc: {:.0}%",
                                project.rp_allocation_percent * 100.0
                            ));
                        });
                    }
                }
            }
        });

        ui.add_space(10.0);

        // Active Engineering Projects
        theme::elevated_frame().show(ui, |ui| {
            ui.label(
                egui::RichText::new("Active Engineering Projects")
                    .font(theme::heading())
                    .color(theme::ACCENT),
            );
            ui.separator();

            let project_count = engineering_projects.iter().count();
            if project_count == 0 {
                ui.label(
                    egui::RichText::new("No active engineering projects")
                        .italics()
                        .color(theme::TEXT_DIM),
                );
            } else {
                for (project, team) in engineering_projects.iter() {
                    if let Some(component) = tech_data.get_component(&project.component_id) {
                        let progress = project.progress_percent();
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("⚙").size(14.0));
                            ui.label(egui::RichText::new(&component.name).strong());
                            ui.label(
                                egui::RichText::new(format!("({})", team.name))
                                    .size(11.0)
                                    .color(theme::TEXT_DIM),
                            );
                            ui.add(
                                egui::ProgressBar::new(progress)
                                    .text(format!(
                                        "{:.0}% ({:.0}/{:.0} EP)",
                                        progress * 100.0,
                                        project.progress,
                                        project.required_points
                                    ))
                                    .desired_width(200.0),
                            );
                        });
                    }
                }
            }
        });

        ui.add_space(10.0);

        // Research Teams
        theme::elevated_frame().show(ui, |ui| {
            ui.label(
                egui::RichText::new("Research & Engineering Teams")
                    .font(theme::heading())
                    .color(theme::ACCENT),
            );
            ui.separator();

            let team_count = all_teams.iter().count();
            if team_count == 0 {
                ui.label(
                    egui::RichText::new(
                        "No teams available - teams will be added in future updates",
                    )
                    .italics()
                    .color(theme::TEXT_DIM),
                );
            } else {
                for (_entity, team) in all_teams.iter() {
                    ui.horizontal(|ui| {
                        let icon = if team.is_research { "🔬" } else { "⚙" };
                        ui.label(egui::RichText::new(format!("{} {}", icon, team.name)).strong());
                        ui.label(format!("Lead: {}", team.lead_character));
                    });

                    if let Some(specialty) = team.specialty {
                        ui.label(format!(
                            "  Specialty: {} ({})",
                            specialty.display_name(),
                            specialty.icon()
                        ));
                    }
                    ui.label(format!("  Efficiency: {:.0}%", team.efficiency * 100.0));
                    ui.add_space(5.0);
                }
            }
        });
    });
}
pub(super) fn render_research_tech_tooltip_content(
    ui: &mut egui::Ui,
    tech: &Technology,
    tech_data: &TechnologiesData,
    research_state: &ResearchState,
    icon_textures: Option<&HashMap<TechCategory, egui::TextureId>>,
    active_info: Option<&ActiveProjectInfo>,
) {
    ui.set_max_width(360.0);
    let cat_color = tech_category_color(tech.category);

    ui.scope(|ui| {
        ui.style_mut().interaction.selectable_labels = false;

        ui.label(egui::RichText::new(&tech.name).strong().size(14.0));
        ui.horizontal(|ui| {
            if let Some(icon_map) = icon_textures {
                if let Some(tex) = icon_map.get(&tech.category) {
                    ui.add(
                        egui::Image::new(egui::load::SizedTexture::new(*tex, [16.0, 16.0]))
                            .tint(cat_color),
                    );
                } else {
                    ui.label(tech.category.icon());
                }
            } else {
                ui.label(tech.category.icon());
            }
            ui.label(egui::RichText::new(tech.category.display_name()).color(cat_color));
        });
        ui.separator();
        ui.label(&tech.description);
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!("Tier: {}", tech.tier)).color(theme::TEXT_DIM));
            ui.separator();
            ui.label(
                egui::RichText::new(format!("Cost: {:.0} RP", tech.research_cost))
                    .color(theme::RP_BLUE)
                    .strong(),
            );
        });

        if !tech.prerequisites.is_empty() {
            ui.add_space(4.0);
            ui.label(egui::RichText::new("Prerequisites:").strong());
            for prereq_id in &tech.prerequisites {
                if let Some(prereq) = tech_data.get_tech(prereq_id) {
                    let c = if research_state.is_unlocked(prereq_id) {
                        theme::GREEN
                    } else {
                        theme::RED
                    };
                    ui.label(egui::RichText::new(format!("  • {}", prereq.name)).color(c));
                }
            }
        }

        if !tech.unlocks_components.is_empty() {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Unlocks Components:")
                    .strong()
                    .color(theme::EP_TEAL),
            );
            for comp_id in &tech.unlocks_components {
                if let Some(comp) = tech_data.get_component(comp_id) {
                    ui.label(
                        egui::RichText::new(format!(
                            "  ⚙ {} ({:.0} EP)",
                            comp.name, comp.engineering_cost
                        ))
                        .color(theme::EP_TEAL),
                    );
                }
            }
        }

        if !tech.unlocks_engineering.is_empty() {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Unlocks Engineering Projects:")
                    .strong()
                    .color(theme::EP_TEAL),
            );
            for comp_id in &tech.unlocks_engineering {
                if let Some(comp) = tech_data.get_component(comp_id) {
                    ui.label(
                        egui::RichText::new(format!(
                            "  ⚙ {} ({:.0} EP)",
                            comp.name, comp.engineering_cost
                        ))
                        .color(theme::EP_TEAL),
                    );
                } else {
                    ui.label(
                        egui::RichText::new(format!("  ⚙ {}", comp_id)).color(theme::EP_TEAL),
                    );
                }
            }
        }

        let unlocked_hulls = tech_data.hull_unlocks.get(&tech.id);
        if !unlocked_hulls.is_none_or(|hulls| hulls.is_empty()) {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Unlocks Hull Frames:")
                    .strong()
                    .color(theme::ACCENT),
            );
            for hull in unlocked_hulls.into_iter().flatten() {
                ui.label(
                    egui::RichText::new(format!("  ▣ {}", hull))
                    .color(theme::ACCENT),
                );
            }
        }

        if !tech.modifiers.is_empty() {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Provides Bonuses:")
                    .strong()
                    .color(theme::GREEN),
            );
            for modifier in &tech.modifiers {
                let (value_text, value_color) = match &modifier.modifier_type {
                    crate::research::types::ModifierType::UnlockMechanic(_) => {
                        (modifier.modifier_type.display_name(), theme::ACCENT)
                    }
                    _ => {
                        let is_positive = modifier.value >= 0.0;
                        // For cost-type modifiers, negative is beneficial
                        let is_beneficial = match &modifier.modifier_type {
                            crate::research::types::ModifierType::ConstructionCost
                            | crate::research::types::ModifierType::ShipMaintenance => !is_positive,
                            _ => is_positive,
                        };
                        let value_color = if is_beneficial {
                            theme::GREEN
                        } else {
                            theme::RED
                        };
                        (
                            format!(
                                "{}: {:+.0}%",
                                modifier.modifier_type.display_name(),
                                modifier.value
                            ),
                            value_color,
                        )
                    }
                };
                ui.label(egui::RichText::new(format!("  • {}", value_text)).color(value_color));
            }
        }

        if let Some(info) = active_info {
            ui.add_space(4.0);
            ui.separator();
            let status = if info.active { "Researching" } else { "Paused" };
            ui.label(
                egui::RichText::new(format!(
                    "⏳ {}: {:.1}%",
                    status,
                    info.progress_percent * 100.0
                ))
                .color(theme::RP_BLUE)
                .strong(),
            );
            ui.add(egui::ProgressBar::new(info.progress_percent).text(format!(
                "{:.0}/{:.0} RP",
                info.progress, info.required_points
            )));
            ui.label(format!(
                "Allocation: {:.0}%",
                info.allocation_percent * 100.0
            ));
        }
    });
}

/// Render the Available Research tab
fn render_available_research_tab(
    ui: &mut egui::Ui,
    research_state: &ResearchState,
    tech_data: &TechnologiesData,
    icon_textures: &HashMap<TechCategory, egui::TextureId>,
    active_research: &HashMap<String, ActiveProjectInfo>,
    pending_research: &mut crate::research::PendingResearchActions,
    team_capacity: &ResearchTeamCapacity,
) {
    let active_count = active_research.values().filter(|info| info.active).count();
    let teams_available = team_capacity
        .max_research_teams
        .saturating_sub(active_count);

    draw_section_title(
        ui,
        "RESEARCH PROJECTS",
        "Technologies with all prerequisites met.",
    );
    ui.horizontal_wrapped(|ui| {
        ui.add_space(20.0);
        ui.label(
            egui::RichText::new(format!(
                "Teams: {}/{} in use | {} available",
                active_count, team_capacity.max_research_teams, teams_available
            ))
            .color(if teams_available > 0 {
                theme::GREEN
            } else {
                theme::AMBER
            }),
        );
    });
    theme::divider(ui);

    egui::ScrollArea::vertical().show(ui, |ui| {
        let unlocked_ids: Vec<_> = research_state
            .unlocked_technologies
            .iter()
            .cloned()
            .collect();

        // First: show active/paused research projects with controls
        let mut active_projects: Vec<(&str, &ActiveProjectInfo)> = active_research
            .iter()
            .map(|(id, info)| (id.as_str(), info))
            .collect();
        active_projects.sort_by(|a, b| a.0.cmp(b.0));

        if !active_projects.is_empty() {
            ui.label(
                egui::RichText::new("CURRENT RESEARCH")
                    .font(theme::heading())
                    .color(theme::ACCENT),
            );
            ui.add_space(4.0);

            for (tech_id, info) in &active_projects {
                if let Some(tech) = tech_data.get_tech(tech_id) {
                    let cat_color = tech_category_color(tech.category);
                    ui.horizontal(|ui| {
                        // Info labels in a scope so tooltip hover isn't stolen by interactive widgets
                        let info_scope = ui.scope(|ui| {
                            let status_icon = if info.active { "🔬" } else { "⏸" };
                            ui.label(egui::RichText::new(status_icon).size(14.0));
                            if let Some(tex) = icon_textures.get(&tech.category) {
                                ui.add(
                                    egui::Image::new(egui::load::SizedTexture::new(
                                        *tex,
                                        [16.0, 16.0],
                                    ))
                                    .tint(cat_color),
                                );
                            }
                            ui.label(egui::RichText::new(&tech.name).strong());
                            ui.label(
                                egui::RichText::new(tech.category.display_name())
                                    .size(12.0)
                                    .color(cat_color),
                            );
                            if !info.active {
                                ui.label(egui::RichText::new("PAUSED").color(theme::AMBER));
                            }
                        });
                        info_scope.response.on_hover_ui(|ui| {
                            render_research_tech_tooltip_content(
                                ui,
                                tech,
                                tech_data,
                                research_state,
                                Some(icon_textures),
                                Some(info),
                            );
                        });
                        ui.add_space(8.0);
                        // Interactive controls outside the tooltip scope
                        ui.add(
                            egui::ProgressBar::new(info.progress_percent)
                                .text(format!(
                                    "{:.1}% ({:.0}/{:.0} RP)",
                                    info.progress_percent * 100.0,
                                    info.progress,
                                    info.required_points
                                ))
                                .desired_width(180.0),
                        );
                        ui.label("Alloc:");
                        let mut alloc_pct = (info.allocation_percent * 100.0) as f32;
                        let slider_resp = ui.add(
                            egui::Slider::new(&mut alloc_pct, 0.0..=100.0)
                                .suffix("%")
                                .fixed_decimals(0),
                        );
                        if slider_resp.changed() {
                            pending_research
                                .update_allocations
                                .push((tech_id.to_string(), alloc_pct as f64 / 100.0));
                        }
                        if info.active {
                            if ui
                                .button("⏸ Pause")
                                .on_hover_text(
                                    "Pause research (preserves progress, frees team slot)",
                                )
                                .clicked()
                            {
                                pending_research.stop_research.push(tech_id.to_string());
                            }
                        } else {
                            let can_resume = teams_available > 0;
                            let btn = ui.add_enabled(can_resume, egui::Button::new("▶ Resume"));
                            if !can_resume {
                                btn.on_hover_text("No team slots available");
                            } else if btn.clicked() {
                                pending_research.resume_research.push(tech_id.to_string());
                            }
                        }
                        if ui
                            .button("⏹ Stop")
                            .on_hover_text(
                                "Stop research entirely (removes project, progress is lost)",
                            )
                            .clicked()
                        {
                            // Store pending cancellation in temporary data to show confirmation dialog
                            ui.data_mut(|data| {
                                data.insert_temp(
                                    ui.id().with("pending_cancel"),
                                    tech_id.to_string(),
                                );
                            });
                        }
                    });
                }
            }

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(5.0);
        }

        // Then: show available (not yet started) techs
        let mut available_techs = Vec::new();
        for (tech_id, tech) in &tech_data.technologies {
            if !research_state.is_unlocked(tech_id)
                && !active_research.contains_key(tech_id)
                && tech_data.check_prerequisites(tech_id, &unlocked_ids)
            {
                available_techs.push(tech);
            }
        }

        if available_techs.is_empty() && active_projects.is_empty() {
            ui.label(
                egui::RichText::new("No technologies available for research")
                    .italics()
                    .color(theme::TEXT_DIM),
            );
            ui.label("Complete more research to unlock new technologies.");
        } else if !available_techs.is_empty() {
            ui.label(
                egui::RichText::new("AVAILABLE TO START")
                    .font(theme::heading())
                    .color(theme::ACCENT),
            );
            ui.add_space(4.0);

            available_techs.sort_by(|a, b| {
                a.category
                    .display_name()
                    .cmp(b.category.display_name())
                    .then(a.research_cost.partial_cmp(&b.research_cost).unwrap())
            });

            for tech in available_techs {
                let cat_color = tech_category_color(tech.category);
                let can_start = teams_available > 0;
                ui.horizontal(|ui| {
                    // Info labels in a scope so tooltip hover isn't stolen by the button
                    let info_scope = ui.scope(|ui| {
                        ui.label(egui::RichText::new("⏳").color(theme::AMBER));
                        if let Some(tex) = icon_textures.get(&tech.category) {
                            ui.add(
                                egui::Image::new(egui::load::SizedTexture::new(*tex, [16.0, 16.0]))
                                    .tint(cat_color),
                            );
                        }
                        ui.label(egui::RichText::new(&tech.name).strong());
                        ui.label(
                            egui::RichText::new(tech.category.display_name())
                                .size(12.0)
                                .color(cat_color),
                        );
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(format!("{:.0} RP", tech.research_cost))
                                .color(theme::RP_BLUE),
                        );
                        ui.label(
                            egui::RichText::new(format!("T{}", tech.tier))
                                .size(11.0)
                                .color(theme::TEXT_DIM),
                        );
                        let engineering_unlock_count = tech
                            .unlocks_components
                            .iter()
                            .chain(tech.unlocks_engineering.iter())
                            .cloned()
                            .collect::<BTreeSet<_>>()
                            .len();
                        if engineering_unlock_count > 0 {
                            ui.label(
                                egui::RichText::new(format!("⚙{}", engineering_unlock_count))
                                    .size(11.0)
                                    .color(theme::EP_TEAL),
                            );
                        }
                        if !tech.modifiers.is_empty() {
                            ui.label(
                                egui::RichText::new(format!("✦{}", tech.modifiers.len()))
                                    .size(11.0)
                                    .color(theme::GREEN),
                            );
                        }
                    });
                    info_scope.response.on_hover_ui(|ui| {
                        render_research_tech_tooltip_content(
                            ui,
                            tech,
                            tech_data,
                            research_state,
                            Some(icon_textures),
                            None,
                        );
                    });
                    // Button outside the tooltip scope
                    let btn = ui.add_enabled(can_start, egui::Button::new("🚀 Start"));
                    if can_start && btn.clicked() {
                        pending_research.start_research.push(tech.id.clone());
                    }
                    if !can_start {
                        btn.on_hover_text("No team slots available. Stop another project first.");
                    }
                });
            }
        }
    });
}

/// Render the Available Engineering tab
fn render_available_engineering_tab(
    ui: &mut egui::Ui,
    research_state: &ResearchState,
    tech_data: &TechnologiesData,
    icon_textures: &HashMap<TechCategory, egui::TextureId>,
    engineering_projects: &Query<(&EngineeringProject, &ResearchTeam)>,
    pending_research: &mut crate::research::PendingResearchActions,
    team_capacity: &ResearchTeamCapacity,
    selected_engineering_target: &mut Option<String>,
) {
    draw_section_title(
        ui,
        "ENGINEERING PROJECTS",
        "Component designs unlocked by research and ready for implementation.",
    );
    theme::divider(ui);

    egui::ScrollArea::vertical().show(ui, |ui| {
        let mut available_components = Vec::new();

        for (comp_id, component) in &tech_data.components {
            if research_state.is_unlocked(&component.required_tech)
                && !research_state.is_component_completed(comp_id)
            {
                available_components.push(component);
            }
        }

        if available_components.is_empty() {
            ui.label(
                egui::RichText::new("No components available for engineering")
                    .italics()
                    .color(theme::TEXT_DIM),
            );
            ui.label("Research new technologies to unlock component designs.");
        } else {
            // Sort by cost
            available_components
                .sort_by(|a, b| a.engineering_cost.partial_cmp(&b.engineering_cost).unwrap());

            for component in available_components {
                let is_preselected = selected_engineering_target
                    .as_deref()
                    .is_some_and(|target| target == component.id);
                let in_progress = engineering_projects
                    .iter()
                    .any(|(project, _)| project.component_id == component.id);
                let used_team_slots = engineering_projects.iter().count();
                let can_start = !in_progress && used_team_slots < team_capacity.max_engineering_teams;
                let parent_tech = tech_data.get_tech(&component.required_tech);
                let cat_color = parent_tech
                    .map(|t| tech_category_color(t.category))
                    .unwrap_or(theme::AMBER);

                let row = egui::Frame::new()
                    .fill(if is_preselected {
                        theme::SURFACE_RAISED
                    } else {
                        theme::SURFACE
                    })
                    .stroke(if is_preselected {
                        egui::Stroke::new(1.0, theme::ACCENT)
                    } else {
                        egui::Stroke::new(1.0, theme::BORDER)
                    })
                    .corner_radius(4.0)
                    .inner_margin(egui::Margin::symmetric(8, 6))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("⚙").color(cat_color));
                            if let Some(tech) = parent_tech {
                                if let Some(tex) = icon_textures.get(&tech.category) {
                                    ui.add(
                                        egui::Image::new(egui::load::SizedTexture::new(*tex, [16.0, 16.0]))
                                            .tint(cat_color),
                                    );
                                }
                            }
                            ui.label(egui::RichText::new(&component.name).strong());
                            if let Some(tech) = parent_tech {
                                ui.label(
                                    egui::RichText::new(tech.category.display_name())
                                        .size(12.0)
                                        .color(cat_color),
                                );
                            }
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new(format!("{:.0} EP", component.engineering_cost))
                                    .color(theme::EP_TEAL),
                            );
                            if let Some(tech) = parent_tech {
                                ui.label(
                                    egui::RichText::new(format!("(from: {})", tech.name))
                                        .size(11.0)
                                        .italics()
                                        .color(theme::TEXT_DIM),
                                );
                            }
                            let button_label = if in_progress {
                                "⏳ Engineering"
                            } else {
                                "🔧 Start Engineering"
                            };
                            let start_btn = ui.add_enabled(can_start, egui::Button::new(button_label));
                            if can_start && start_btn.clicked() {
                                pending_research.start_engineering.push(component.id.clone());
                            }
                            if !can_start && !in_progress {
                                start_btn.on_hover_text("No engineering team slots available.");
                            }
                        });
                    });
                if is_preselected {
                    row.response.scroll_to_me(Some(egui::Align::Center));
                }
                // Tooltip with component details
                row.response.on_hover_ui(|ui| {
                    ui.set_max_width(320.0);
                    ui.label(egui::RichText::new(&component.name).strong().size(14.0));
                    if let Some(tech) = parent_tech {
                        ui.horizontal(|ui| {
                            if let Some(tex) = icon_textures.get(&tech.category) {
                                ui.add(
                                    egui::Image::new(egui::load::SizedTexture::new(
                                        *tex,
                                        [16.0, 16.0],
                                    ))
                                    .tint(cat_color),
                                );
                            }
                            ui.label(
                                egui::RichText::new(tech.category.display_name()).color(cat_color),
                            );
                        });
                    }
                    ui.separator();
                    ui.label(&component.description);
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(format!(
                            "Engineering Cost: {:.0} EP",
                            component.engineering_cost
                        ))
                        .color(theme::EP_TEAL)
                        .strong(),
                    );
                    if let Some(tech) = parent_tech {
                        ui.label(
                            egui::RichText::new(format!("Required Tech: {}", tech.name))
                                .size(12.0)
                                .color(theme::TEXT_DIM),
                        );
                    }
                });
            }
        }
    });
}

/// Render the Bonuses tab — shows all active modifiers and their contributing technologies
fn render_bonuses_tab(
    ui: &mut egui::Ui,
    research_state: &ResearchState,
    tech_data: &TechnologiesData,
    icon_textures: &HashMap<TechCategory, egui::TextureId>,
) {
    draw_section_title(
        ui,
        "CURRENT BONUSES",
        "Active modifiers and unlocked mechanics derived from completed technologies.",
    );
    theme::divider(ui);

    // Build a lookup: for each modifier type, which techs contribute and how much.
    // Done outside the scroll area so the detail Area can reference it unconditionally.
    let mut modifier_sources: HashMap<
        &ModifierType,
        Vec<(&crate::research::types::Technology, f64)>,
    > = HashMap::new();
    for tech in tech_data.technologies.values() {
        if research_state.is_unlocked(&tech.id) {
            for modifier_def in &tech.modifiers {
                modifier_sources
                    .entry(&modifier_def.modifier_type)
                    .or_default()
                    .push((tech, modifier_def.value));
            }
        }
    }

    // Persistent state keys
    let pinned_id = ui.id().with("bonuses_pinned"); // (name, row_rect): pinned on click
    let hover_id = ui.id().with("bonuses_hover"); // (name, hold_until, row_rect): hover with hold time

    let now = ui.input(|i| i.time);
    let pinned_data: Option<(String, egui::Rect)> = ui.data(|d| d.get_temp(pinned_id));
    let hover_data: Option<(String, f64, egui::Rect)> = ui.data(|d| d.get_temp(hover_id));

    // Sort and partition modifiers
    let mut sorted_modifiers: Vec<_> = research_state.active_modifiers.iter().collect();
    sorted_modifiers.sort_by(|(a, _), (b, _)| {
        let a_is_unlock = matches!(a, ModifierType::UnlockMechanic(_));
        let b_is_unlock = matches!(b, ModifierType::UnlockMechanic(_));
        b_is_unlock
            .cmp(&a_is_unlock)
            .then_with(|| a.display_name().cmp(&b.display_name()))
    });
    let (unlocks, bonuses): (Vec<_>, Vec<_>) = sorted_modifiers
        .into_iter()
        .partition(|(m, _)| matches!(m, ModifierType::UnlockMechanic(_)));

    egui::ScrollArea::vertical().show(ui, |ui| {
        if research_state.active_modifiers.is_empty() {
            ui.label(
                egui::RichText::new("No bonuses active yet")
                    .italics()
                    .color(theme::TEXT_DIM),
            );
            ui.label("Research technologies to unlock bonuses.");
            return;
        }

        // Helper: render a single bonus row, returning the row response.
        // No detail box is rendered here — the caller handles that after all rows.
        let pinned_name = pinned_data.as_ref().map(|(n, _)| n.as_str());

        if !bonuses.is_empty() {
            ui.label(
                egui::RichText::new("NUMERIC BONUSES")
                    .font(theme::heading())
                    .color(theme::ACCENT),
            );
            ui.add_space(4.0);

            for (modifier_type, total_value) in &bonuses {
                let is_positive = **total_value >= 0.0;
                let is_beneficial = match modifier_type {
                    ModifierType::ConstructionCost | ModifierType::ShipMaintenance => !is_positive,
                    _ => is_positive,
                };
                let value_color = if is_beneficial {
                    theme::GREEN
                } else {
                    theme::RED
                };

                let modifier_name = modifier_type.display_name();
                let is_pinned = pinned_name.is_some_and(|p| p == modifier_name);

                let row_rect = {
                    let row = ui.horizontal(|ui| {
                        // Highlight pinned row
                        if is_pinned {
                            let row_rect = ui.max_rect();
                            ui.painter().rect_filled(
                                row_rect,
                                2.0,
                                egui::Color32::from_rgba_unmultiplied(13, 17, 23, 120),
                            );
                        }
                        ui.label(
                            egui::RichText::new(if is_beneficial { "▲" } else { "▼" })
                                .color(value_color),
                        );
                        ui.label(egui::RichText::new(&modifier_name).strong());
                        ui.label(
                            egui::RichText::new(format!("{:+.0}%", total_value))
                                .color(value_color)
                                .strong(),
                        );
                        let source_count =
                            modifier_sources.get(modifier_type).map_or(0, |v| v.len());
                        if source_count > 0 {
                            ui.label(
                                egui::RichText::new(format!(
                                    "({} source{})",
                                    source_count,
                                    if source_count > 1 { "s" } else { "" }
                                ))
                                .size(11.0)
                                .color(theme::TEXT_DIM),
                            );
                        }
                        if is_pinned {
                            ui.label(egui::RichText::new("📌").size(10.0));
                        }
                    });
                    row.response.rect
                };

                // Use explicit interact so both hover and click work for any row
                let interact = ui.interact(
                    row_rect,
                    ui.id().with("bonus_row").with(&modifier_name),
                    egui::Sense::click(),
                );
                if interact.hovered() {
                    interact
                        .clone()
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    ui.data_mut(|d| {
                        d.insert_temp(hover_id, (modifier_name.clone(), now + 0.25, row_rect))
                    });
                }
                if interact.clicked() {
                    if is_pinned {
                        ui.data_mut(|d| d.remove::<(String, egui::Rect)>(pinned_id));
                    } else {
                        ui.data_mut(|d| {
                            d.insert_temp(pinned_id, (modifier_name.clone(), row_rect))
                        });
                    }
                }

                ui.add_space(2.0);
            }
            ui.add_space(10.0);
        }

        if !unlocks.is_empty() {
            ui.label(
                egui::RichText::new("UNLOCKED MECHANICS")
                    .font(theme::heading())
                    .color(theme::ACCENT),
            );
            ui.add_space(4.0);

            for (modifier_type, _value) in &unlocks {
                let modifier_name = modifier_type.display_name();
                let is_pinned = pinned_name.is_some_and(|p| p == modifier_name);

                let row_rect = {
                    let row = ui.horizontal(|ui| {
                        if is_pinned {
                            let row_rect = ui.max_rect();
                            ui.painter().rect_filled(
                                row_rect,
                                2.0,
                                egui::Color32::from_rgba_unmultiplied(13, 17, 23, 120),
                            );
                        }
                        ui.label(egui::RichText::new("✔").color(theme::EP_TEAL));
                        ui.label(
                            egui::RichText::new(&modifier_name)
                                .strong()
                                .color(theme::ACCENT),
                        );
                        if is_pinned {
                            ui.label(egui::RichText::new("📌").size(10.0));
                        }
                    });
                    row.response.rect
                };

                let interact = ui.interact(
                    row_rect,
                    ui.id().with("unlock_row").with(&modifier_name),
                    egui::Sense::click(),
                );
                if interact.hovered() {
                    interact
                        .clone()
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    ui.data_mut(|d| {
                        d.insert_temp(hover_id, (modifier_name.clone(), now + 0.25, row_rect))
                    });
                }
                if interact.clicked() {
                    if is_pinned {
                        ui.data_mut(|d| d.remove::<(String, egui::Rect)>(pinned_id));
                    } else {
                        ui.data_mut(|d| {
                            d.insert_temp(pinned_id, (modifier_name.clone(), row_rect))
                        });
                    }
                }

                ui.add_space(2.0);
            }
        }

        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("Click a row to pin its detail box. Click again to unpin.")
                .size(10.0)
                .italics()
                .color(theme::TEXT_HINT),
        );
    });

    // Determine which detail box to show and at what position.
    // Pinned takes priority over hovered. Both are rendered as a floating Area outside the
    // scroll area so they never cause layout reflow (which was the cause of flickering).
    let detail_show: Option<(String, egui::Rect, bool)> = {
        if let Some((name, rect)) = &pinned_data {
            Some((name.clone(), *rect, true))
        } else if let Some((name, hold_until, rect)) = &hover_data {
            if now <= *hold_until {
                Some((name.clone(), *rect, false))
            } else {
                ui.data_mut(|d| d.remove::<(String, f64, egui::Rect)>(hover_id));
                None
            }
        } else {
            None
        }
    };

    if let Some((detail_name, row_rect, is_pinned)) = detail_show {
        let mut all_modifiers = bonuses.iter().chain(unlocks.iter());
        if let Some((modifier_type, total_value)) =
            all_modifiers.find(|(m, _)| m.display_name() == detail_name)
        {
            let is_positive = **total_value >= 0.0;
            let is_beneficial = match modifier_type {
                ModifierType::ConstructionCost | ModifierType::ShipMaintenance => !is_positive,
                _ => is_positive,
            };
            let value_color = if is_beneficial {
                theme::GREEN
            } else {
                theme::RED
            };
            let border_color = if is_pinned {
                value_color
            } else {
                theme::TEXT_HINT
            };
            let border_width = if is_pinned { 2.0 } else { 1.0 };

            let pos = egui::pos2(row_rect.right() + 24.0, row_rect.top());

            let area_resp = egui::Area::new(ui.id().with("bonus_detail_float"))
                .fixed_pos(pos)
                .order(egui::Order::Tooltip)
                .interactable(true)
                .show(ui.ctx(), |ui| {
                    egui::Frame::NONE
                        .fill(egui::Color32::from_rgba_unmultiplied(13, 17, 23, 245))
                        .stroke(egui::Stroke::new(border_width, border_color))
                        .inner_margin(10.0)
                        .corner_radius(4.0)
                        .show(ui, |ui| {
                            ui.set_max_width(280.0);
                            render_bonus_detail_content(
                                ui,
                                modifier_type,
                                **total_value,
                                &modifier_sources,
                                icon_textures,
                            );
                        });
                });

            // If the pointer is over the floating Area, refresh the hover hold time
            // so the box stays open while the user reads it.
            if area_resp.response.hovered() || area_resp.response.contains_pointer() {
                ui.data_mut(|d| {
                    d.insert_temp(hover_id, (detail_name.clone(), now + 0.25, row_rect))
                });
            }
        }
    }
}

/// Render detail content for a bonus showing all contributing technologies
fn render_bonus_detail_content(
    ui: &mut egui::Ui,
    modifier_type: &ModifierType,
    total_value: f64,
    modifier_sources: &HashMap<&ModifierType, Vec<(&crate::research::types::Technology, f64)>>,
    icon_textures: &HashMap<TechCategory, egui::TextureId>,
) {
    let is_unlock = matches!(modifier_type, ModifierType::UnlockMechanic(_));

    if !is_unlock {
        ui.label(
            egui::RichText::new(format!("Total: {:+.0}%", total_value))
                .color(if total_value >= 0.0 {
                    theme::GREEN
                } else {
                    theme::RED
                })
                .strong()
                .size(13.0),
        );
        ui.add_space(3.0);
    }

    ui.label(
        egui::RichText::new("Contributing Technologies:")
            .strong()
            .size(12.0),
    );
    ui.add_space(2.0);

    if let Some(sources) = modifier_sources.get(modifier_type) {
        for (tech, value) in sources {
            let cat_color = tech_category_color(tech.category);
            ui.horizontal(|ui| {
                if let Some(tex) = icon_textures.get(&tech.category) {
                    ui.add(
                        egui::Image::new(egui::load::SizedTexture::new(*tex, [12.0, 12.0]))
                            .tint(cat_color),
                    );
                }
                ui.label(egui::RichText::new(&tech.name).color(cat_color).size(11.0));
                if !is_unlock {
                    ui.label(
                        egui::RichText::new(format!("{:+.0}%", value))
                            .size(11.0)
                            .color(if *value >= 0.0 {
                                theme::GREEN
                            } else {
                                theme::RED
                            }),
                    );
                }
            });
        }
    } else {
        ui.label(
            egui::RichText::new("No tech sources found")
                .italics()
                .size(10.0)
                .color(theme::TEXT_DIM),
        );
    }
}

/// Render the Archive tab
fn render_archive_tab(
    ui: &mut egui::Ui,
    research_state: &ResearchState,
    tech_data: &TechnologiesData,
    icon_textures: &HashMap<TechCategory, egui::TextureId>,
) {
    draw_section_title(
        ui,
        "RESEARCH ARCHIVE",
        "Completed technologies and engineered components, organized for review.",
    );
    theme::divider(ui);

    egui::ScrollArea::vertical().show(ui, |ui| {
        // Completed Technologies
        theme::elevated_frame().show(ui, |ui| {
            ui.label(
                egui::RichText::new("Completed Technologies")
                    .font(theme::heading())
                    .color(theme::ACCENT),
            );
            ui.separator();

            let unlocked_count = research_state.unlocked_technologies.len();
            ui.label(format!("Total: {} technologies", unlocked_count));
            ui.add_space(5.0);

            if unlocked_count == 0 {
                ui.label(
                    egui::RichText::new("No technologies researched yet")
                        .italics()
                        .color(theme::TEXT_DIM),
                );
            } else {
                // Organize by category
                for category in TechCategory::all() {
                    let category_techs = tech_data.get_by_category(*category);
                    let category_completed: Vec<_> = category_techs
                        .iter()
                        .filter(|t| research_state.is_unlocked(&t.id))
                        .copied()
                        .collect();

                    if !category_completed.is_empty() {
                        ui.horizontal(|ui| {
                            if let Some(tex) = icon_textures.get(category) {
                                ui.add(egui::Image::new(egui::load::SizedTexture::new(
                                    *tex,
                                    [16.0, 16.0],
                                )));
                            } else {
                                ui.label(category.icon());
                            }
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} ({} completed)",
                                    category.display_name(),
                                    category_completed.len()
                                ))
                                .strong(),
                            );
                        });

                        ui.indent(format!("archive_cat_{}", category.display_name()), |ui| {
                            for tech in category_completed {
                                let row = ui.horizontal(|ui| {
                                    ui.label("✔");
                                    ui.label(
                                        egui::RichText::new(&tech.name)
                                            .color(tech_category_color(*category))
                                            .strong(),
                                    );
                                    if tech.research_cost > 0.0 {
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "({:.0} RP)",
                                                tech.research_cost
                                            ))
                                            .size(11.0)
                                            .color(theme::RP_BLUE),
                                        );
                                    }
                                });
                                row.response.on_hover_ui(|ui| {
                                    render_research_tech_tooltip_content(
                                        ui,
                                        tech,
                                        tech_data,
                                        research_state,
                                        Some(icon_textures),
                                        None,
                                    );
                                });
                            }
                        });

                        ui.add_space(5.0);
                    }
                }
            }
        });

        ui.add_space(15.0);

        // Completed Components
        ui.group(|ui| {
            ui.label(
                egui::RichText::new("Completed Components")
                    .strong()
                    .size(16.0),
            );
            ui.separator();

            let completed_count = research_state.completed_components.len();
            ui.label(format!("Total: {} components", completed_count));
            ui.add_space(5.0);

            if completed_count == 0 {
                ui.label(
                    egui::RichText::new("No components engineered yet")
                        .italics()
                        .color(theme::TEXT_DIM),
                );
            } else {
                for comp_id in &research_state.completed_components {
                    if let Some(component) = tech_data.get_component(comp_id) {
                        ui.horizontal(|ui| {
                            ui.label("⚙");
                            ui.label(&component.name);
                            ui.label(
                                egui::RichText::new(format!(
                                    "({:.0} EP)",
                                    component.engineering_cost
                                ))
                                .size(11.0)
                                .color(theme::TEXT_DIM),
                            );
                        });
                    }
                }
            }
        });
    });
}
