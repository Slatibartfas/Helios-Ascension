use super::*;

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

    // Convert loaded handles to egui TextureIds
    if let Some(icons) = &research_icons {
        for (cat, handle) in &icons.handles {
             icon_textures.entry(*cat).or_insert_with(|| contexts.add_image(bevy_egui::EguiTextureHandle::Strong(handle.clone())));
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
                let modifier_names: Vec<String> = available_modifiers.iter().map(|m| m.display_name()).collect();
                
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
    egui::CentralPanel::default().show(ctx, |ui| {
        // Disable text selection cursor everywhere in the research menu
        ui.style_mut().interaction.selectable_labels = false;

        // Debug mode panel (if enabled)
        if debug_settings.enabled {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("🐛 DEBUG MODE").strong().color(egui::Color32::RED));
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
                    .color(egui::Color32::YELLOW));
            });
            ui.add_space(5.0);
        } else {
            // Show subtle hint about debug mode
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Press F12 to toggle debug mode")
                    .small()
                    .italics()
                    .color(egui::Color32::GRAY));
            });
        }
        
        // Tab bar
        ui.horizontal(|ui| {
            ui.selectable_value(&mut *selected_tab, 0, "📊 Overview");
            ui.selectable_value(&mut *selected_tab, 1, "🌳 Tech Tree");
            ui.selectable_value(&mut *selected_tab, 2, "🔬 Research");
            ui.selectable_value(&mut *selected_tab, 3, "⚙ Engineering");
            ui.selectable_value(&mut *selected_tab, 4, "✦ Bonuses");
            ui.selectable_value(&mut *selected_tab, 5, "📚 Archive");
        });
        
        ui.separator();
        
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
            0 => render_overview_tab(ui, &research_state, &tech_data, icon_textures, &research_projects, &engineering_projects, &all_teams, &team_capacity, &mut *ui_prefs),
            1 => render_tech_tree_tab(ui, &research_state, &mut tech_data, icon_textures, debug_settings.enabled, &mut edit_state, &active_research, &mut pending_research, &mut debug_settings),
            2 => render_available_research_tab(ui, &research_state, &tech_data, icon_textures, &active_research, &mut pending_research, &team_capacity),
            3 => render_available_engineering_tab(ui, &research_state, &tech_data, icon_textures),
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
    ui.heading("Research & Engineering Overview");
    ui.checkbox(&mut ui_prefs.show_inactive_warning, "Show Inactive Warning in Top Bar");
    ui.add_space(5.0);
    
    egui::ScrollArea::vertical().show(ui, |ui| {
        // Point Generation
        ui.group(|ui| {
            ui.label(egui::RichText::new("Point Generation").strong().size(16.0));
            ui.separator();
            
            let rp_per_year = research_state.rp_rate_per_second * 31_557_600.0;
            let ep_per_year = research_state.ep_rate_per_second * 31_557_600.0;
            
            ui.horizontal(|ui| {
                ui.label("Research Points:");
                ui.label(egui::RichText::new(format!("{:.0} RP/year", rp_per_year))
                    .color(egui::Color32::from_rgb(100, 200, 255)));
                ui.label(format!("(Pool: {:.0})", research_state.research_points_available));
            });
            
            ui.horizontal(|ui| {
                ui.label("Engineering Points:");
                ui.label(egui::RichText::new(format!("{:.0} EP/year", ep_per_year))
                    .color(egui::Color32::from_rgb(100, 255, 200)));
                ui.label(format!("(Pool: {:.0})", research_state.engineering_points_available));
            });
        });
        
        ui.add_space(10.0);
        
        // Active Research Projects
        ui.group(|ui| {
            let active_count = research_projects.iter().filter(|(_, p, _)| p.active).count();
            let total_count = research_projects.iter().count();
            ui.label(egui::RichText::new(format!(
                "Active Research Projects ({}/{})",
                active_count, team_capacity.max_research_teams
            )).strong().size(16.0));
            ui.separator();
            
            if total_count == 0 {
                ui.label(egui::RichText::new("No active research projects")
                    .italics()
                    .color(egui::Color32::GRAY));
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
                                ui.label(egui::RichText::new(if project.active { "🔬" } else { "⏸" }).size(14.0));
                                ui.label(egui::RichText::new(&tech.name).strong());
                                if !project.active {
                                    ui.label(egui::RichText::new("PAUSED").color(egui::Color32::YELLOW));
                                }
                                ui.label(egui::RichText::new(format!("({})", team.name)).size(11.0).color(egui::Color32::GRAY));
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
                            ui.label(format!("Alloc: {:.0}%", project.rp_allocation_percent * 100.0));
                        });
                    }
                }
            }
        });
        
        ui.add_space(10.0);
        
        // Active Engineering Projects
        ui.group(|ui| {
            ui.label(egui::RichText::new("Active Engineering Projects").strong().size(16.0));
            ui.separator();
            
            let project_count = engineering_projects.iter().count();
            if project_count == 0 {
                ui.label(egui::RichText::new("No active engineering projects")
                    .italics()
                    .color(egui::Color32::GRAY));
            } else {
                for (project, team) in engineering_projects.iter() {
                    if let Some(component) = tech_data.get_component(&project.component_id) {
                        let progress = project.progress_percent();
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("⚙").size(14.0));
                            ui.label(egui::RichText::new(&component.name).strong());
                            ui.label(egui::RichText::new(format!("({})", team.name)).size(11.0).color(egui::Color32::GRAY));
                            ui.add(
                                egui::ProgressBar::new(progress)
                                    .text(format!("{:.0}% ({:.0}/{:.0} EP)", progress * 100.0, project.progress, project.required_points))
                                    .desired_width(200.0),
                            );
                        });
                    }
                }
            }
        });
        
        ui.add_space(10.0);
        
        // Research Teams
        ui.group(|ui| {
            ui.label(egui::RichText::new("Research & Engineering Teams").strong().size(16.0));
            ui.separator();
            
            let team_count = all_teams.iter().count();
            if team_count == 0 {
                ui.label(egui::RichText::new("No teams available - teams will be added in future updates")
                    .italics()
                    .color(egui::Color32::GRAY));
            } else {
                for (_entity, team) in all_teams.iter() {
                    ui.horizontal(|ui| {
                        let icon = if team.is_research { "🔬" } else { "⚙" };
                        ui.label(egui::RichText::new(format!("{} {}", icon, team.name)).strong());
                        ui.label(format!("Lead: {}", team.lead_character));
                    });
                    
                    if let Some(specialty) = team.specialty {
                        ui.label(format!("  Specialty: {} ({})", 
                            specialty.display_name(), 
                            specialty.icon()));
                    }
                    ui.label(format!("  Efficiency: {:.0}%", team.efficiency * 100.0));
                    ui.add_space(5.0);
                }
            }
        });
    });
}

/// Render the Tech Tree tab
fn render_tech_tree_tab(
    ui: &mut egui::Ui,
    research_state: &ResearchState,
    tech_data: &mut TechnologiesData,
    icon_textures: &HashMap<TechCategory, egui::TextureId>,
    debug_enabled: bool,
    edit_state: &mut TechTreeEditState,
    active_research: &HashMap<String, ActiveProjectInfo>,
    pending_research: &mut crate::research::PendingResearchActions,
    debug_settings: &mut crate::research::ResearchDebugSettings,
) {
    ui.heading("Technology Tree - Graph View");
    ui.label("Pan: Middle mouse drag | Zoom: Mouse wheel | Click: Select tech & highlight path");
    if debug_enabled {
        ui.label(
            egui::RichText::new("Right-click: Edit/delete node | Right-click empty space: Add new tech")
                .small()
                .color(egui::Color32::from_rgb(255, 200, 100)),
        );
    }
    ui.separator();
    
    // Local state for pan, zoom, and selected tech (using unique ID for persistence)
    let pan_id = ui.id().with("tech_tree_pan");
    let zoom_id = ui.id().with("tech_tree_zoom");
    let sel_persist_id = ui.id().with("tech_tree_selected");
    
    let mut pan_offset: egui::Vec2 = ui.data_mut(|data| {
        data.get_persisted(pan_id)
            .unwrap_or(egui::Vec2::new(50.0, 50.0))
    });
    
    let mut zoom: f32 = ui.data_mut(|data| {
        data.get_persisted(zoom_id).unwrap_or(1.0)
    });
    
    let mut selected_tech: Option<String> = ui.data_mut(|data| {
        data.get_persisted(sel_persist_id)
    });
    
    // ---------- layout constants ----------
    let tier_spacing = 310.0 * zoom;
    let node_gap_y = 14.0 * zoom;
    let category_gap = 24.0 * zoom;
    let pane_pad = (10.0 * zoom).round();
    let pane_rounding = 6.0 * zoom;
    let label_width = (140.0 * zoom).round();
    
    // ---------- status line (fixed height, drawn FIRST so it reserves space at the bottom) ----------
    // We draw it at the end but must reserve its height now.
    let status_height = 26.0;
    
    // ---------- canvas: allocate ALL remaining space minus status ----------
    let avail = ui.available_rect_before_wrap();
    if avail.height() <= status_height + 10.0 {
        ui.label("Window too small to display tech tree");
        return;
    }
    let canvas_rect = egui::Rect::from_min_max(
        avail.min,
        egui::Pos2::new(avail.max.x, avail.max.y - status_height),
    );
    
    // Single response for the whole canvas – handles pan / zoom / click
    let response = ui.allocate_rect(canvas_rect, egui::Sense::click_and_drag());
    
    // Zoom – use pointer position directly so zooming works even when a tooltip is shown
    if ui.input(|i| i.pointer.hover_pos().map_or(false, |pos| canvas_rect.contains(pos))) {
        let scroll_delta = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll_delta != 0.0 {
            zoom = (zoom + scroll_delta * 0.001).clamp(0.3, 3.0);
        }
    }
    // Pan (middle-click drag) – read raw pointer delta so pan works even when a tooltip is shown
    let pointer_in_canvas = ui.input(|i| i.pointer.hover_pos().map_or(false, |pos| canvas_rect.contains(pos)));
    if pointer_in_canvas && ui.input(|i| i.pointer.button_down(egui::PointerButton::Middle)) {
        pan_offset += ui.input(|i| i.pointer.delta());
    }
    
    // Persist pan / zoom immediately
    ui.data_mut(|data| {
        data.insert_persisted(pan_id, pan_offset);
        data.insert_persisted(zoom_id, zoom);
    });
    
    // Clipped painter so nothing bleeds outside the canvas
    let clip = ui.clip_rect().intersect(canvas_rect);
    let painter = ui.painter().with_clip_rect(clip);
    
    // ---------- compute uniform node size ----------
    // Use a fixed node size based on zoom so all boxes are identical.
    // Two rows: row 1 = icon + name, row 2 = research cost
    let font_name = egui::FontId::proportional((12.0 * zoom).round());
    let font_cost = egui::FontId::proportional((10.0 * zoom).round());
    let icon_sz = (16.0 * zoom).round();
    let icon_pad = (4.0 * zoom).round();
    let h_pad = (8.0 * zoom).round();
    let v_pad = (6.0 * zoom).round();
    let row_gap = (3.0 * zoom).round();

    // Measure the widest tech name to determine uniform width
    let mut max_name_w: f32 = 0.0;
    let mut max_cost_w: f32 = 0.0;
    for (_, tech) in &tech_data.technologies {
        let g = painter.layout_no_wrap(tech.name.clone(), font_name.clone(), egui::Color32::WHITE);
        max_name_w = max_name_w.max(g.size().x);
        let cost_text = format!("{:.0} RP", tech.research_cost);
        let g2 = painter.layout_no_wrap(cost_text, font_cost.clone(), egui::Color32::WHITE);
        max_cost_w = max_cost_w.max(g2.size().x);
    }
    // Row heights (approximate from font size)
    let name_row_h = font_name.size * 1.3;
    let cost_row_h = font_cost.size * 1.3;

    let node_w = (icon_sz + icon_pad + max_name_w.max(max_cost_w) + h_pad * 2.0).round();
    let node_h = (v_pad + name_row_h + row_gap + cost_row_h + v_pad).round();

    // ---------- compute node positions: horizontal category bands ----------
    // Layout: each category is a horizontal band (row).  Within each band,
    // tiers run left-to-right as columns.  Multiple techs in the same
    // (category, tier) cell are stacked vertically within that band.
    let mut node_positions: HashMap<String, egui::Pos2> = HashMap::new();
    
    // Collect unique tiers (sorted)
    let mut tier_set: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    for (_, tech) in &tech_data.technologies {
        tier_set.insert(tech.tier);
    }
    let tiers: Vec<u32> = tier_set.into_iter().collect();
    let tier_index_map: HashMap<u32, usize> = tiers.iter().enumerate().map(|(i, &t)| (t, i)).collect();
    
    // Group techs: category -> tier -> Vec<tech>
    let mut techs_by_cat_tier: std::collections::BTreeMap<u8, std::collections::BTreeMap<u32, Vec<&crate::research::types::Technology>>> =
        std::collections::BTreeMap::new();
    for (_, tech) in &tech_data.technologies {
        techs_by_cat_tier
            .entry(tech.category as u8)
            .or_default()
            .entry(tech.tier)
            .or_default()
            .push(tech);
    }
    // Sort techs within each cell alphabetically for deterministic layout
    for cat_tiers in techs_by_cat_tier.values_mut() {
        for cell_techs in cat_tiers.values_mut() {
            cell_techs.sort_by_key(|t| &t.name);
        }
    }
    
    // Compute height of each category band (max stacked techs across all tiers)
    // and record category row Y start positions
    struct CategoryBand {
        category: TechCategory,
        y_start: f32,
        height: f32,
    }
    let mut category_bands: Vec<CategoryBand> = Vec::new();
    let origin_x = canvas_rect.left() + pan_offset.x + label_width;
    let mut current_y = canvas_rect.top() + pan_offset.y;
    
    let categories = TechCategory::all();
    for &cat in categories {
        let cat_key = cat as u8;
        let max_stack = if let Some(cat_tiers) = techs_by_cat_tier.get(&cat_key) {
            cat_tiers.values().map(|v| v.len()).max().unwrap_or(0)
        } else {
            0
        };
        if max_stack == 0 {
            continue; // skip empty categories
        }
        let band_content_h = max_stack as f32 * node_h + (max_stack as f32 - 1.0).max(0.0) * node_gap_y;
        let band_h = band_content_h + pane_pad * 2.0;
        
        category_bands.push(CategoryBand {
            category: cat,
            y_start: current_y,
            height: band_h,
        });
        current_y += band_h + category_gap;
    }
    
    // Place nodes within each category band
    for band in &category_bands {
        let cat_key = band.category as u8;
        if let Some(cat_tiers) = techs_by_cat_tier.get(&cat_key) {
            for (&tier, cell_techs) in cat_tiers {
                let tier_idx = tier_index_map.get(&tier).copied().unwrap_or(0);
                let col_x = origin_x + (tier_idx as f32) * tier_spacing;
                // Center the stack vertically within the band
                let stack_h = cell_techs.len() as f32 * node_h + (cell_techs.len() as f32 - 1.0).max(0.0) * node_gap_y;
                let stack_y_start = band.y_start + pane_pad + (band.height - pane_pad * 2.0 - stack_h) / 2.0;
                
                for (i, tech) in cell_techs.iter().enumerate() {
                    let node_top = stack_y_start + i as f32 * (node_h + node_gap_y);
                    let center_x = col_x + node_w / 2.0;
                    let center_y = node_top + node_h / 2.0;
                    node_positions.insert(tech.id.clone(), egui::Pos2::new(center_x, center_y));
                }
            }
        }
    }
    
    // Compute total width spanned by tier columns for pane drawing
    let total_tier_width = if tiers.is_empty() {
        node_w
    } else {
        (tiers.len() as f32 - 1.0) * tier_spacing + node_w
    };
    
    // ---------- draw category background panes (horizontal bands) ----------
    for band in &category_bands {
        let cat_color = tech_category_color(band.category);
        let bg_color = egui::Color32::from_rgba_unmultiplied(
            cat_color.r(), cat_color.g(), cat_color.b(), 18,
        );
        let border_color = egui::Color32::from_rgba_unmultiplied(
            cat_color.r(), cat_color.g(), cat_color.b(), 40,
        );
        let pane_rect = egui::Rect::from_min_size(
            egui::Pos2::new(origin_x - pane_pad, band.y_start),
            egui::Vec2::new(total_tier_width + pane_pad * 2.0, band.height),
        );
        painter.rect_filled(pane_rect, pane_rounding, bg_color);
        painter.rect_stroke(pane_rect, pane_rounding, egui::Stroke::new(1.0 * zoom, border_color), egui::StrokeKind::Outside);
        
        // Category label on the left: icon + stacked word lines
        let cat_icon = band.category.icon();
        let cat_name = band.category.display_name().to_uppercase();
        
        // Fixed icon size for consistency across variable-height category panes
        let icon_font_size = (22.0 * zoom).round();
        let font_icon_large = egui::FontId::proportional(icon_font_size);
        let font_cat_word = egui::FontId::proportional((11.0 * zoom).round());
        
        // Split name into words, one per line
        let words: Vec<&str> = cat_name.split_whitespace().collect();
        let line_spacing = font_cat_word.size * 1.25;
        let text_block_h = words.len() as f32 * line_spacing;
        let gap_between = (4.0 * zoom).round();
        
        // Total height of the content block
        let total_h = icon_font_size + gap_between + text_block_h;
        
        // Center within the band
        let band_center_y = band.y_start + band.height / 2.0;
        let block_top = band_center_y - total_h / 2.0;
        let label_center_x = origin_x - pane_pad - label_width / 2.0;
        
        // Icon
        painter.text(
            egui::Pos2::new(label_center_x, block_top + icon_font_size / 2.0),
            egui::Align2::CENTER_CENTER,
            cat_icon,
            font_icon_large,
            cat_color,
        );
        
        // Word-per-line text
        let text_top = block_top + icon_font_size + gap_between;
        for (i, word) in words.iter().enumerate() {
            painter.text(
                egui::Pos2::new(label_center_x, text_top + i as f32 * line_spacing + line_spacing / 2.0),
                egui::Align2::CENTER_CENTER,
                *word,
                font_cat_word.clone(),
                egui::Color32::from_rgba_unmultiplied(
                    cat_color.r(), cat_color.g(), cat_color.b(), 200,
                ),
            );
        }
    }
    
    // ---------- draw tier column headers ----------
    let header_y = canvas_rect.top() + pan_offset.y - (22.0 * zoom);
    let font_header = egui::FontId::proportional((15.0 * zoom).round());
    for (i, tier) in tiers.iter().enumerate() {
        let col_x = origin_x + (i as f32) * tier_spacing + node_w / 2.0;
        painter.text(
            egui::Pos2::new(col_x, header_y),
            egui::Align2::CENTER_BOTTOM,
            format!("Tier {}", tier),
            font_header.clone(),
            egui::Color32::from_rgb(180, 180, 190),
        );
    }
    
    // ---------- prerequisite highlight path ----------
    let mut path_techs = std::collections::HashSet::new();
    if let Some(ref sel_id) = selected_tech {
        let mut to_process = vec![sel_id.clone()];
        path_techs.insert(sel_id.clone());
        while let Some(cur) = to_process.pop() {
            if let Some(tech) = tech_data.technologies.get(&cur) {
                for prereq_id in &tech.prerequisites {
                    if path_techs.insert(prereq_id.clone()) {
                        to_process.push(prereq_id.clone());
                    }
                }
            }
        }
    }
    
    // ---------- draw connection lines (cubic bezier) ----------
    // Connect right edge of prerequisite to left edge of dependent
    for (_, tech) in &tech_data.technologies {
        if let Some(tech_center) = node_positions.get(&tech.id) {
            for prereq_id in &tech.prerequisites {
                if let Some(prereq_center) = node_positions.get(prereq_id) {
                    let is_in_path =
                        path_techs.contains(&tech.id) && path_techs.contains(prereq_id);
                    let is_prereq_unlocked = research_state.is_unlocked(prereq_id);
                    let line_color = if is_in_path {
                        egui::Color32::from_rgba_premultiplied(255, 200, 0, 255)
                    } else if is_prereq_unlocked {
                        egui::Color32::from_rgba_premultiplied(100, 255, 100, 80)
                    } else {
                        egui::Color32::from_rgba_premultiplied(120, 120, 120, 60)
                    };
                    let w = if is_in_path { 2.5 * zoom } else { 1.0 * zoom };
                    // From right edge of prereq to left edge of tech
                    let from = egui::Pos2::new(prereq_center.x + node_w / 2.0, prereq_center.y);
                    let to = egui::Pos2::new(tech_center.x - node_w / 2.0, tech_center.y);
                    // Cubic bezier with horizontal tangents for a smooth S-curve
                    let mid_x = (from.x + to.x) * 0.5;
                    let bezier = egui::epaint::CubicBezierShape::from_points_stroke(
                        [
                            from,
                            egui::Pos2::new(mid_x, from.y),
                            egui::Pos2::new(mid_x, to.y),
                            to,
                        ],
                        false,
                        egui::Color32::TRANSPARENT,
                        egui::Stroke::new(w, line_color),
                    );
                    painter.add(bezier);
                }
            }
        }
    }
    
    // ---------- draw nodes & collect hit-test rects ----------
    // We do NOT call ui.allocate_rect for each node (that was the bug).
    // Instead we paint directly and do manual hit-testing against the pointer.
    let pointer_pos = ui.input(|i| i.pointer.interact_pos());
    let pointer_clicked = response.clicked();
    let pointer_right_clicked = response.clicked_by(egui::PointerButton::Secondary);
    let mut hovered_tech_id: Option<String> = None;
    let mut clicked_tech_id: Option<String> = None;
    let mut right_clicked_tech_id: Option<String> = None;
    // We need to collect hovered rect for tooltip
    let mut hovered_rect: Option<egui::Rect> = None;
    
    let unlocked_ids: Vec<_> = research_state.unlocked_technologies.iter().cloned().collect();
    
    for (tech_id, center) in &node_positions {
        if let Some(tech) = tech_data.technologies.get(tech_id) {
            let is_unlocked = research_state.is_unlocked(&tech.id);
            let is_researching = active_research.contains_key(&tech.id);
            let research_progress = active_research.get(&tech.id).map(|info| info.progress_percent);
            let can_research =
                !is_unlocked && !is_researching && tech_data.check_prerequisites(&tech.id, &unlocked_ids);
            let is_in_path = path_techs.contains(&tech.id);
            let is_selected = selected_tech.as_ref() == Some(&tech.id);
            
            // Node fill color — use darker/muted tones so white text is always readable
            let node_color = if is_in_path {
                if is_unlocked {
                    egui::Color32::from_rgb(30, 90, 30)
                } else if is_researching {
                    egui::Color32::from_rgb(20, 60, 110)
                } else if can_research {
                    egui::Color32::from_rgb(90, 75, 15)
                } else {
                    egui::Color32::from_rgb(60, 60, 60)
                }
            } else if is_unlocked {
                egui::Color32::from_rgb(25, 70, 25)
            } else if is_researching {
                egui::Color32::from_rgb(15, 50, 95)
            } else if can_research {
                egui::Color32::from_rgb(70, 60, 15)
            } else {
                egui::Color32::from_rgb(45, 45, 50)
            };
            
            let category_color = tech_category_color(tech.category);
            
            // Build node rect from center
            let node_rect = egui::Rect::from_center_size(
                egui::Pos2::new(center.x.round(), center.y.round()),
                egui::Vec2::new(node_w, node_h),
            );
            
            // --- paint background ---
            let rounding = 4.0 * zoom;
            painter.rect_filled(node_rect, rounding, node_color);
            
            // Border — thicker if selected or in path
            let border_w = if is_selected {
                3.5 * zoom
            } else if is_in_path {
                2.5 * zoom
            } else {
                1.5 * zoom
            };
            painter.rect_stroke(
                node_rect,
                rounding,
                egui::Stroke::new(border_w, category_color),
                egui::StrokeKind::Outside,
            );
            
            // --- row 1: icon + name (left-aligned) ---
            let text_color = if is_in_path {
                egui::Color32::WHITE
            } else if is_unlocked {
                egui::Color32::from_rgb(180, 255, 180)
            } else if can_research {
                egui::Color32::from_rgb(255, 240, 180)
            } else {
                egui::Color32::from_rgb(170, 170, 175)
            };
            
            let row1_y = (node_rect.top() + v_pad + name_row_h / 2.0).round();
            let content_x = (node_rect.left() + h_pad).round();
            
            // Icon
            if let Some(tex) = icon_textures.get(&tech.category) {
                let ir = egui::Rect::from_min_size(
                    egui::Pos2::new(content_x, (row1_y - icon_sz / 2.0).round()),
                    egui::Vec2::splat(icon_sz),
                );
                painter.image(
                    *tex,
                    ir,
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1.0, 1.0)),
                    category_color,
                );
            }
            
            // Name text
            let name_x = (content_x + icon_sz + icon_pad).round();
            painter.text(
                egui::Pos2::new(name_x, row1_y),
                egui::Align2::LEFT_CENTER,
                &tech.name,
                font_name.clone(),
                text_color,
            );
            
            // --- row 2: research cost / progress (left-aligned, dimmer) ---
            let row2_y = (node_rect.top() + v_pad + name_row_h + row_gap + cost_row_h / 2.0).round();
            let (cost_text, cost_color) = if is_unlocked {
                ("✔ Researched".to_string(), egui::Color32::from_rgb(120, 200, 120))
            } else if let Some(pct) = research_progress {
                (
                    format!("⏳ {:.0}%  ({:.0} RP)", pct * 100.0, tech.research_cost),
                    egui::Color32::from_rgb(100, 180, 255),
                )
            } else {
                (format!("{:.0} RP", tech.research_cost), egui::Color32::from_rgb(150, 180, 220))
            };
            painter.text(
                egui::Pos2::new(name_x, row2_y),
                egui::Align2::LEFT_CENTER,
                &cost_text,
                font_cost.clone(),
                cost_color,
            );

            // --- progress bar for actively researching techs ---
            if let Some(pct) = research_progress {
                let bar_h = (3.0 * zoom).max(1.0);
                let bar_rect = egui::Rect::from_min_size(
                    egui::Pos2::new(node_rect.left() + 2.0, node_rect.bottom() - bar_h - 1.0),
                    egui::Vec2::new((node_rect.width() - 4.0) * pct, bar_h),
                );
                painter.rect_filled(bar_rect, 0.0, egui::Color32::from_rgb(80, 160, 255));
                // bg track
                let track_rect = egui::Rect::from_min_size(
                    egui::Pos2::new(node_rect.left() + 2.0 + (node_rect.width() - 4.0) * pct, node_rect.bottom() - bar_h - 1.0),
                    egui::Vec2::new((node_rect.width() - 4.0) * (1.0 - pct), bar_h),
                );
                painter.rect_filled(track_rect, 0.0, egui::Color32::from_rgb(40, 40, 50));
            }
            
            // --- hit-test ---
            if let Some(pp) = pointer_pos {
                if node_rect.contains(pp) && canvas_rect.contains(pp) {
                    hovered_tech_id = Some(tech.id.clone());
                    hovered_rect = Some(node_rect);
                    if pointer_clicked {
                        clicked_tech_id = Some(tech.id.clone());
                    }
                    if pointer_right_clicked {
                        right_clicked_tech_id = Some(tech.id.clone());
                    }
                }
            }
        }
    }
    
    // Handle click – toggle selection
    if let Some(cid) = clicked_tech_id {
        if selected_tech.as_ref() == Some(&cid) {
            selected_tech = None;
        } else {
            selected_tech = Some(cid);
        }
    } else if pointer_clicked {
        // Clicked on empty space (not on any node) – clear selection
        selected_tech = None;
    }

    // Handle right-click – open context menu (debug mode only)
    if debug_enabled && pointer_right_clicked {
        if let Some(pp) = pointer_pos {
            if canvas_rect.contains(pp) {
                edit_state.context_menu = Some(ContextMenuState {
                    pos: (pp.x, pp.y),
                    tech_id: right_clicked_tech_id.clone(),
                });
            }
        }
    }

    // ---------- Debug context menu ----------
    if debug_enabled {
        let mut close_menu = false;
        if let Some(ref ctx_menu) = edit_state.context_menu.clone() {
            let menu_pos = egui::Pos2::new(ctx_menu.pos.0, ctx_menu.pos.1);
            egui::Area::new(ui.id().with("tech_ctx_menu"))
                .fixed_pos(menu_pos)
                .order(egui::Order::Foreground)
                .show(ui.ctx(), |ui| {
                    egui::Frame::menu(ui.style())
                        .inner_margin(4.0)
                        .show(ui, |ui| {
                            ui.set_min_width(160.0);
                            if let Some(ref tid) = ctx_menu.tech_id {
                                // Right-clicked on a node
                                ui.label(egui::RichText::new(format!("Tech: {}", tid)).strong().small());
                                ui.separator();
                                if ui.button("✏ Edit Technology").clicked() {
                                    if let Some(tech) = tech_data.technologies.get(tid) {
                                        edit_state.editing = Some(TechEditData::from_tech(tech));
                                    }
                                    close_menu = true;
                                }
                                if ui.button("🗑 Delete Technology").clicked() {
                                    edit_state.delete_confirm = Some(tid.clone());
                                    close_menu = true;
                                }
                            } else {
                                // Right-clicked on empty space
                                ui.label(egui::RichText::new("Tech Tree").strong().small());
                                ui.separator();
                                if ui.button("➕ Add New Technology").clicked() {
                                    edit_state.adding = Some(TechEditData::new_blank());
                                    close_menu = true;
                                }
                            }
                            if ui.button("✖ Close").clicked() {
                                close_menu = true;
                            }
                        });
                });

            // Close menu if clicked elsewhere
            let any_click = ui.input(|i| {
                i.pointer.any_pressed()
            });
            if any_click && !close_menu {
                // Check if the click was outside the menu area (approximate)
                if let Some(pp) = pointer_pos {
                    let menu_rect = egui::Rect::from_min_size(menu_pos, egui::Vec2::new(170.0, 100.0));
                    if !menu_rect.contains(pp) {
                        close_menu = true;
                    }
                }
            }
        }
        if close_menu {
            edit_state.context_menu = None;
        }

        // ---------- Delete confirmation dialog ----------
        let mut do_delete: Option<String> = None;
        let mut cancel_delete = false;
        if let Some(ref del_id) = edit_state.delete_confirm.clone() {
            let tech_name = tech_data
                .technologies
                .get(del_id)
                .map(|t| t.name.clone())
                .unwrap_or_else(|| del_id.clone());
            egui::Window::new("Confirm Delete")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ui.ctx(), |ui| {
                    ui.label(format!("Delete technology \"{}\" ({})?", tech_name, del_id));
                    ui.label(
                        egui::RichText::new("This will also remove it from all prerequisite lists.")
                            .small()
                            .color(egui::Color32::YELLOW),
                    );
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("🗑 Delete").clicked() {
                            do_delete = Some(del_id.clone());
                        }
                        if ui.button("Cancel").clicked() {
                            cancel_delete = true;
                        }
                    });
                });
        }
        if cancel_delete {
            edit_state.delete_confirm = None;
        }
        if let Some(del_id) = do_delete {
            // Remove the technology
            tech_data.technologies.remove(&del_id);
            // Remove from all prerequisite lists
            for (_, tech) in tech_data.technologies.iter_mut() {
                tech.prerequisites.retain(|p| p != &del_id);
            }
            // Clear selection if it was the deleted tech
            if selected_tech.as_ref() == Some(&del_id) {
                selected_tech = None;
            }
            edit_state.delete_confirm = None;
            save_technologies_to_file(tech_data);
        }

        // ---------- Edit Technology dialog ----------
        render_tech_edit_dialog(ui, tech_data, edit_state, false);

        // ---------- Add Technology dialog ----------
        render_tech_edit_dialog(ui, tech_data, edit_state, true);
    }
    
    // Show tooltip for hovered or selected node
    // Use a tooltip Window instead of show_tooltip_at so the user can interact with it
    let tooltip_hold_id = ui.id().with("tech_tooltip_hold");
    let now = ui.input(|i| i.time);
    let pointer_hover_pos = ui.input(|i| i.pointer.hover_pos());

    if let Some((held_id, _hold_until, held_rect)) =
        ui.data_mut(|data| data.get_temp::<(String, f64, egui::Rect)>(tooltip_hold_id))
    {
        let held_tooltip_pos = egui::pos2(held_rect.right() + 4.0, held_rect.top());
        let held_tooltip_rect = egui::Rect::from_min_max(
            egui::pos2(held_tooltip_pos.x - 2.0, held_tooltip_pos.y - 2.0),
            egui::pos2(held_tooltip_pos.x + 390.0, held_tooltip_pos.y + 430.0),
        );
        let pointer_inside_held_tooltip = pointer_hover_pos
            .map_or(false, |pos| held_tooltip_rect.contains(pos));

        if pointer_inside_held_tooltip {
            hovered_tech_id = None;
            hovered_rect = None;
            let hold_until = now + 0.9;
            ui.data_mut(|data| {
                data.insert_temp(tooltip_hold_id, (held_id, hold_until, held_rect));
            });
        }
    }

    if let (Some(id), Some(rect)) = (&hovered_tech_id, hovered_rect) {
        ui.data_mut(|data| {
            data.insert_temp(tooltip_hold_id, (id.clone(), now + 0.9, rect));
        });
    }

    let mut tooltip_tech_id = hovered_tech_id.clone().or_else(|| selected_tech.clone());
    let mut tooltip_rect = if hovered_tech_id.is_some() {
        hovered_rect
    } else {
        // Use the selected node's rect if we have it
        selected_tech.as_ref().and_then(|sel_id| {
            node_positions.get(sel_id).map(|center| {
                egui::Rect::from_center_size(
                    egui::Pos2::new(center.x, center.y),
                    egui::Vec2::new(node_w, node_h),
                )
            })
        })
    };

    if tooltip_tech_id.is_none() {
        if let Some((held_id, mut hold_until, held_rect)) =
            ui.data_mut(|data| data.get_temp::<(String, f64, egui::Rect)>(tooltip_hold_id))
        {
            let tooltip_pos = egui::pos2(held_rect.right() + 4.0, held_rect.top());
            let hover_bridge = egui::Rect::from_min_max(
                egui::pos2(held_rect.right() - 8.0, held_rect.top() - 20.0),
                egui::pos2(tooltip_pos.x + 390.0, tooltip_pos.y + 430.0),
            );
            let pointer_in_bridge = pointer_hover_pos.map_or(false, |pos| hover_bridge.contains(pos));

            if now <= hold_until || pointer_in_bridge {
                if pointer_in_bridge {
                    hold_until = now + 0.9;
                }
                ui.data_mut(|data| {
                    data.insert_temp(tooltip_hold_id, (held_id.clone(), hold_until, held_rect));
                });
                tooltip_tech_id = Some(held_id);
                tooltip_rect = Some(held_rect);
            } else {
                ui.data_mut(|data| {
                    data.remove::<(String, f64, egui::Rect)>(tooltip_hold_id);
                });
            }
        }
    }
    
    if let (Some(ref tid), Some(tr)) = (&tooltip_tech_id, tooltip_rect) {
        if let Some(tech) = tech_data.technologies.get(tid) {
            let is_researching = active_research.contains_key(&tech.id);
            let can_research =
                !research_state.is_unlocked(&tech.id)
                    && !is_researching
                    && tech_data.check_prerequisites(&tech.id, &unlocked_ids);
            
            let tooltip_pos = egui::pos2(tr.right() + 4.0, tr.top());
            
            egui::Window::new("tech_node_tooltip")
                .id(ui.id().with("tech_tooltip_win"))
                .fixed_pos(tooltip_pos)
                .collapsible(false)
                .resizable(false)
                .title_bar(false)
                .frame(egui::Frame::popup(ui.ctx().style().as_ref())
                    .fill(egui::Color32::from_rgba_unmultiplied(25, 30, 40, 245))
                    .stroke(egui::Stroke::new(2.0, tech_category_color(tech.category))))
                .show(ui.ctx(), |ui| {
                    render_research_tech_tooltip_content(
                        ui,
                        tech,
                        tech_data,
                        research_state,
                        Some(icon_textures),
                        active_research.get(&tech.id),
                    );
                    if !is_researching && can_research {
                        ui.add_space(5.0);
                        ui.separator();
                        if ui.button("🔬 Start Research").clicked() {
                            pending_research.start_research.push(tech.id.clone());
                            pending_research.navigate_to_available_tab = true;
                        }
                    }
                    if debug_enabled {
                        ui.add_space(5.0);
                        ui.separator();
                        ui.label(egui::RichText::new("🐛 Debug").small().color(egui::Color32::RED));
                        if tech.modifiers.is_empty() {
                            ui.label(
                                egui::RichText::new("This tech grants no modifiers.")
                                    .small()
                                    .italics()
                                    .color(egui::Color32::GRAY),
                            );
                        } else {
                            ui.label(
                                egui::RichText::new("Modifiers this tech grants:")
                                    .small()
                                    .color(egui::Color32::from_rgb(200, 200, 200)),
                            );
                            for m in &tech.modifiers {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "  • {}: {:+.1}%",
                                        m.modifier_type.display_name(),
                                        m.value
                                    ))
                                    .small()
                                    .color(if m.value >= 0.0 {
                                        egui::Color32::from_rgb(100, 220, 100)
                                    } else {
                                        egui::Color32::from_rgb(220, 100, 100)
                                    }),
                                );
                            }
                            if ui
                                .button("⚡ Grant Tech Bonuses")
                                .on_hover_text(
                                    "Instantly apply all modifiers from this technology as debug overrides",
                                )
                                .clicked()
                            {
                                for m in &tech.modifiers {
                                    *debug_settings
                                        .debug_modifiers
                                        .entry(m.modifier_type.clone())
                                        .or_insert(0.0) += m.value;
                                }
                            }
                        }
                        ui.add_space(3.0);
                        if ui
                            .button("➕ Custom Modifier…")
                            .on_hover_text("Open the Add Debug Modifier dialog")
                            .clicked()
                        {
                            debug_settings.modifier_dialog_show = true;
                        }
                    }
                });
        }
    }
    
    // Persist selection
    ui.data_mut(|data| {
        if let Some(ref sel) = selected_tech {
            data.insert_persisted(sel_persist_id, sel.clone());
        } else {
            data.remove::<String>(sel_persist_id);
        }
    });
    
    // ---------- status bar ----------
    let status_rect = egui::Rect::from_min_max(
        egui::Pos2::new(avail.min.x, avail.max.y - status_height),
        avail.max,
    );
    ui.scope_builder(egui::UiBuilder::new().max_rect(status_rect), |ui| {
        ui.horizontal(|ui| {
            ui.label("Status:");
            ui.colored_label(egui::Color32::from_rgb(50, 200, 50), "● Unlocked");
            ui.colored_label(egui::Color32::from_rgb(80, 160, 255), "● Researching");
            ui.colored_label(egui::Color32::from_rgb(255, 200, 50), "● Available");
            ui.colored_label(egui::Color32::from_rgb(100, 100, 100), "● Locked");
            ui.label(format!("| Zoom: {:.1}x", zoom));
            if debug_enabled {
                ui.separator();
                ui.colored_label(
                    egui::Color32::from_rgb(255, 100, 100),
                    "Right-click: edit/add techs",
                );
            }
            ui.separator();
            if let Some(ref sel_id) = selected_tech {
                if let Some(sel_tech) = tech_data.technologies.get(sel_id) {
                    ui.label(egui::RichText::new("Selected:").strong());
                    ui.label(&sel_tech.name);
                    ui.label(format!(
                        "({} prerequisites highlighted)",
                        path_techs.len().saturating_sub(1)
                    ));
                }
            } else {
                ui.label(
                    egui::RichText::new("Click a technology to highlight its prerequisite path")
                        .italics(),
                );
            }
        });
    });
}

/// Render the edit/add technology dialog window
fn render_tech_edit_dialog(
    ui: &mut egui::Ui,
    tech_data: &mut TechnologiesData,
    edit_state: &mut TechTreeEditState,
    is_add: bool,
) {
    let data_opt = if is_add {
        &mut edit_state.adding
    } else {
        &mut edit_state.editing
    };

    let title = if is_add {
        "Add New Technology"
    } else {
        "Edit Technology"
    };

    let mut should_save = false;
    let mut should_close = false;

    if let Some(ref mut edit_data) = data_opt {
        let mut open = true;
        egui::Window::new(title)
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(450.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ui.ctx(), |ui| {
                egui::ScrollArea::vertical()
                    .max_height(500.0)
                    .show(ui, |ui| {
                        egui::Grid::new("tech_edit_grid")
                            .num_columns(2)
                            .spacing([10.0, 6.0])
                            .striped(true)
                            .show(ui, |ui| {
                                // ID
                                ui.label("ID:");
                                if is_add {
                                    ui.text_edit_singleline(&mut edit_data.id);
                                } else {
                                    ui.label(
                                        egui::RichText::new(&edit_data.id)
                                            .monospace()
                                            .color(egui::Color32::GRAY),
                                    );
                                }
                                ui.end_row();

                                // Name
                                ui.label("Name:");
                                ui.text_edit_singleline(&mut edit_data.name);
                                ui.end_row();

                                // Category
                                ui.label("Category:");
                                let categories = TechCategory::all();
                                egui::ComboBox::from_id_salt("tech_edit_cat")
                                    .selected_text(
                                        categories
                                            .get(edit_data.category_index)
                                            .map(|c| c.display_name())
                                            .unwrap_or("Unknown"),
                                    )
                                    .show_ui(ui, |ui| {
                                        for (i, cat) in categories.iter().enumerate() {
                                            ui.selectable_value(
                                                &mut edit_data.category_index,
                                                i,
                                                cat.display_name(),
                                            );
                                        }
                                    });
                                ui.end_row();

                                // Description
                                ui.label("Description:");
                                ui.text_edit_multiline(&mut edit_data.description);
                                ui.end_row();

                                // Research Cost
                                ui.label("Research Cost:");
                                ui.text_edit_singleline(&mut edit_data.research_cost);
                                ui.end_row();

                                // Tier
                                ui.label("Tier:");
                                ui.text_edit_singleline(&mut edit_data.tier);
                                ui.end_row();
                            });

                        ui.add_space(10.0);

                        // Prerequisites section
                        ui.label(egui::RichText::new("Prerequisites:").strong());
                        ui.group(|ui| {
                            let mut remove_idx: Option<usize> = None;
                            for (i, prereq) in edit_data.prerequisites.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    let exists = tech_data.technologies.contains_key(prereq);
                                    let color = if exists {
                                        egui::Color32::from_rgb(100, 255, 100)
                                    } else {
                                        egui::Color32::from_rgb(255, 100, 100)
                                    };
                                    ui.colored_label(color, prereq);
                                    if ui.small_button("✖").clicked() {
                                        remove_idx = Some(i);
                                    }
                                });
                            }
                            if let Some(idx) = remove_idx {
                                edit_data.prerequisites.remove(idx);
                            }

                            // Add prerequisite
                            ui.horizontal(|ui| {
                                egui::ComboBox::from_id_salt("add_prereq_combo")
                                    .selected_text(if edit_data.new_prereq.is_empty() {
                                        "Select prerequisite..."
                                    } else {
                                        &edit_data.new_prereq
                                    })
                                    .show_ui(ui, |ui| {
                                        let mut sorted_ids: Vec<_> = tech_data
                                            .technologies
                                            .keys()
                                            .filter(|id| {
                                                !edit_data.prerequisites.contains(id)
                                                    && **id != edit_data.id
                                            })
                                            .cloned()
                                            .collect();
                                        sorted_ids.sort();
                                        for tid in sorted_ids {
                                            let label = tech_data
                                                .technologies
                                                .get(&tid)
                                                .map(|t| format!("{} ({})", t.name, tid))
                                                .unwrap_or_else(|| tid.clone());
                                            ui.selectable_value(
                                                &mut edit_data.new_prereq,
                                                tid,
                                                label,
                                            );
                                        }
                                    });
                                if ui.button("➕ Add").clicked()
                                    && !edit_data.new_prereq.is_empty()
                                {
                                    edit_data.prerequisites.push(edit_data.new_prereq.clone());
                                    edit_data.new_prereq.clear();
                                }
                            });
                        });

                        ui.add_space(10.0);

                        // Modifiers section
                        ui.label(egui::RichText::new("Modifiers (granted when researched):").strong());
                        ui.group(|ui| {
                            let mut remove_idx: Option<usize> = None;
                            if edit_data.modifiers.is_empty() {
                                ui.label(
                                    egui::RichText::new("No modifiers")
                                        .italics()
                                        .color(egui::Color32::GRAY),
                                );
                            }
                            for (i, m) in edit_data.modifiers.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    ui.colored_label(
                                        if m.value >= 0.0 {
                                            egui::Color32::from_rgb(100, 220, 100)
                                        } else {
                                            egui::Color32::from_rgb(220, 100, 100)
                                        },
                                        format!("{}: {:+.1}%", m.modifier_type.display_name(), m.value),
                                    );
                                    if ui.small_button("✖").clicked() {
                                        remove_idx = Some(i);
                                    }
                                });
                            }
                            if let Some(idx) = remove_idx {
                                edit_data.modifiers.remove(idx);
                            }

                            // Add modifier row
                            ui.horizontal(|ui| {
                                let all_mods = ModifierType::all_for_debug();
                                let selected_name = all_mods
                                    .get(edit_data.new_modifier_type_index)
                                    .map(|m| m.display_name())
                                    .unwrap_or_default();
                                egui::ComboBox::from_id_salt("add_modifier_combo")
                                    .selected_text(selected_name)
                                    .show_ui(ui, |ui| {
                                        for (i, m) in all_mods.iter().enumerate() {
                                            ui.selectable_value(
                                                &mut edit_data.new_modifier_type_index,
                                                i,
                                                m.display_name(),
                                            );
                                        }
                                    });
                                ui.add(
                                    egui::TextEdit::singleline(&mut edit_data.new_modifier_value)
                                        .hint_text("value %")
                                        .desired_width(70.0),
                                );
                                if ui.button("➕ Add").clicked() {
                                    if let Ok(val) = edit_data.new_modifier_value.trim().parse::<f64>() {
                                        let mtype = all_mods[edit_data.new_modifier_type_index].clone();
                                        edit_data.modifiers.push(TechModifierDef {
                                            modifier_type: mtype,
                                            value: val,
                                        });
                                        edit_data.new_modifier_value.clear();
                                    }
                                }
                            });
                        });

                        ui.add_space(10.0);

                        // Validation
                        let mut errors: Vec<String> = Vec::new();
                        if edit_data.id.is_empty() {
                            errors.push("ID is required".to_string());
                        }
                        if edit_data.name.is_empty() {
                            errors.push("Name is required".to_string());
                        }
                        if edit_data.research_cost.parse::<f64>().is_err() {
                            errors.push("Research cost must be a number".to_string());
                        }
                        if edit_data.tier.parse::<u32>().is_err() {
                            errors.push("Tier must be a positive integer".to_string());
                        }
                        if is_add && tech_data.technologies.contains_key(&edit_data.id) {
                            errors.push(format!("ID '{}' already exists", edit_data.id));
                        }

                        if !errors.is_empty() {
                            for err in &errors {
                                ui.colored_label(egui::Color32::from_rgb(255, 100, 100), err);
                            }
                        }

                        ui.add_space(5.0);
                        ui.horizontal(|ui| {
                            let can_save = errors.is_empty();
                            if ui
                                .add_enabled(can_save, egui::Button::new("💾 Save"))
                                .clicked()
                            {
                                should_save = true;
                            }
                            if ui.button("Cancel").clicked() {
                                should_close = true;
                            }
                        });
                    });
            });
        if !open {
            should_close = true;
        }
    }

    // Apply save outside borrow scope
    if should_save {
        let data_opt = if is_add {
            &mut edit_state.adding
        } else {
            &mut edit_state.editing
        };

        if let Some(edit_data) = data_opt.take() {
            let categories = TechCategory::all();
            let category = categories
                .get(edit_data.category_index)
                .copied()
                .unwrap_or(TechCategory::Physics);
            let research_cost = edit_data.research_cost.parse::<f64>().unwrap_or(1000.0);
            let tier = edit_data.tier.parse::<u32>().unwrap_or(1);

            if !is_add {
                // Editing existing tech — update in place, preserving unlocks/modifiers
                if let Some(tech) = tech_data.technologies.get_mut(&edit_data.original_id) {
                    tech.name = edit_data.name;
                    tech.category = category;
                    tech.description = edit_data.description;
                    tech.research_cost = research_cost;
                    tech.tier = tier;
                    tech.prerequisites = edit_data.prerequisites;
                    tech.modifiers = edit_data.modifiers;
                }
            } else {
                // Adding new tech
                let new_tech = crate::research::types::Technology {
                    id: edit_data.id.clone(),
                    name: edit_data.name,
                    category,
                    description: edit_data.description,
                    research_cost,
                    prerequisites: edit_data.prerequisites,
                    unlocks_components: Vec::new(),
                    unlocks_engineering: Vec::new(),
                    modifiers: edit_data.modifiers,
                    tier,
                };
                tech_data.technologies.insert(edit_data.id, new_tech);
            }
            save_technologies_to_file(tech_data);
        }
    } else if should_close {
        if is_add {
            edit_state.adding = None;
        } else {
            edit_state.editing = None;
        }
    }
}

/// Save the current technologies data back to the RON file
fn save_technologies_to_file(tech_data: &TechnologiesData) {
    use serde::Serialize;

    #[derive(Serialize)]
    struct TechnologiesFile {
        technologies: Vec<crate::research::types::Technology>,
        components: Vec<crate::research::types::ComponentDefinition>,
    }

    let mut techs: Vec<_> = tech_data.technologies.values().cloned().collect();
    techs.sort_by(|a, b| a.tier.cmp(&b.tier).then_with(|| a.category.cmp(&b.category).then_with(|| a.name.cmp(&b.name))));

    let mut comps: Vec<_> = tech_data.components.values().cloned().collect();
    comps.sort_by(|a, b| a.id.cmp(&b.id));

    let file_data = TechnologiesFile {
        technologies: techs,
        components: comps,
    };

    let pretty_config = ron::ser::PrettyConfig::new()
        .depth_limit(4)
        .struct_names(false)
        .enumerate_arrays(false);

    match ron::ser::to_string_pretty(&file_data, pretty_config) {
        Ok(contents) => {
            let path = "assets/data/technologies.ron";
            match std::fs::write(path, &contents) {
                Ok(()) => info!("Saved technologies to {}", path),
                Err(e) => error!("Failed to write technologies file: {}", e),
            }
        }
        Err(e) => error!("Failed to serialize technologies: {}", e),
    }
}

/// Get the unique category color for a TechCategory
fn tech_category_color(cat: TechCategory) -> egui::Color32 {
    match cat {
        TechCategory::Electronics => egui::Color32::from_rgb(100, 150, 255),
        TechCategory::Propulsion => egui::Color32::from_rgb(255, 150, 50),
        TechCategory::Energy => egui::Color32::from_rgb(255, 255, 50),
        TechCategory::Physics => egui::Color32::from_rgb(150, 100, 255),
        TechCategory::Military => egui::Color32::from_rgb(255, 50, 50),
        TechCategory::Weapons => egui::Color32::from_rgb(200, 50, 50),
        TechCategory::DefensiveSystems => egui::Color32::from_rgb(50, 150, 255),
        TechCategory::Materials => egui::Color32::from_rgb(150, 150, 50),
        TechCategory::Construction => egui::Color32::from_rgb(200, 150, 100),
        TechCategory::Biology => egui::Color32::from_rgb(50, 255, 150),
        TechCategory::Sensors => egui::Color32::from_rgb(100, 255, 255),
        TechCategory::SpaceTechnology => egui::Color32::from_rgb(150, 200, 255),
        TechCategory::Sociology => egui::Color32::from_rgb(255, 150, 200),
        TechCategory::LifeSupport => egui::Color32::from_rgb(100, 255, 100),
        TechCategory::Industry => egui::Color32::from_rgb(180, 180, 50),
    }
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
                    ui.add(egui::Image::new(egui::load::SizedTexture::new(*tex, [16.0, 16.0])).tint(cat_color));
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
            ui.label(egui::RichText::new(format!("Tier: {}", tech.tier)).color(egui::Color32::GRAY));
            ui.separator();
            ui.label(
                egui::RichText::new(format!("Cost: {:.0} RP", tech.research_cost))
                    .color(egui::Color32::from_rgb(120, 200, 255))
                    .strong(),
            );
        });

        if !tech.prerequisites.is_empty() {
            ui.add_space(4.0);
            ui.label(egui::RichText::new("Prerequisites:").strong());
            for prereq_id in &tech.prerequisites {
                if let Some(prereq) = tech_data.get_tech(prereq_id) {
                    let c = if research_state.is_unlocked(prereq_id) {
                        egui::Color32::from_rgb(100, 255, 100)
                    } else {
                        egui::Color32::from_rgb(255, 100, 100)
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
                    .color(egui::Color32::from_rgb(140, 230, 200)),
            );
            for comp_id in &tech.unlocks_components {
                if let Some(comp) = tech_data.get_component(comp_id) {
                    ui.label(
                        egui::RichText::new(format!("  ⚙ {} ({:.0} EP)", comp.name, comp.engineering_cost))
                            .color(egui::Color32::from_rgb(140, 230, 200)),
                    );
                }
            }
        }

        if !tech.modifiers.is_empty() {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Provides Bonuses:")
                    .strong()
                    .color(egui::Color32::from_rgb(120, 255, 140)),
            );
            for modifier in &tech.modifiers {
                let (value_text, value_color) = match &modifier.modifier_type {
                    crate::research::types::ModifierType::UnlockMechanic(_) => (
                        modifier.modifier_type.display_name(),
                        egui::Color32::from_rgb(120, 220, 255),
                    ),
                    _ => {
                        let is_positive = modifier.value >= 0.0;
                        // For cost-type modifiers, negative is beneficial
                        let is_beneficial = match &modifier.modifier_type {
                            crate::research::types::ModifierType::ConstructionCost
                            | crate::research::types::ModifierType::ShipMaintenance => !is_positive,
                            _ => is_positive,
                        };
                        let value_color = if is_beneficial {
                            egui::Color32::from_rgb(120, 255, 140)
                        } else {
                            egui::Color32::from_rgb(255, 120, 120)
                        };
                        (
                            format!("{}: {:+.0}%", modifier.modifier_type.display_name(), modifier.value),
                            value_color,
                        )
                    },
                };
                ui.label(egui::RichText::new(format!("  • {}", value_text)).color(value_color));
            }
        }

        if let Some(info) = active_info {
            ui.add_space(4.0);
            ui.separator();
            let status = if info.active { "Researching" } else { "Paused" };
            ui.label(
                egui::RichText::new(format!("⏳ {}: {:.1}%", status, info.progress_percent * 100.0))
                    .color(egui::Color32::from_rgb(100, 180, 255))
                    .strong(),
            );
            ui.add(
                egui::ProgressBar::new(info.progress_percent)
                    .text(format!("{:.0}/{:.0} RP", info.progress, info.required_points)),
            );
            ui.label(format!("Allocation: {:.0}%", info.allocation_percent * 100.0));
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
    let teams_available = team_capacity.max_research_teams.saturating_sub(active_count);
    
    ui.heading("Research Projects");
    ui.horizontal(|ui| {
        ui.label("Technologies with all prerequisites met.");
        ui.add_space(20.0);
        ui.label(egui::RichText::new(format!(
            "Teams: {}/{} in use | {} available",
            active_count, team_capacity.max_research_teams, teams_available
        )).color(if teams_available > 0 {
            egui::Color32::from_rgb(100, 255, 100)
        } else {
            egui::Color32::from_rgb(255, 200, 100)
        }));
    });
    ui.separator();
    
    egui::ScrollArea::vertical().show(ui, |ui| {
        let unlocked_ids: Vec<_> = research_state.unlocked_technologies.iter().cloned().collect();
        
        // First: show active/paused research projects with controls
        let mut active_projects: Vec<(&str, &ActiveProjectInfo)> = active_research
            .iter()
            .map(|(id, info)| (id.as_str(), info))
            .collect();
        active_projects.sort_by(|a, b| a.0.cmp(b.0));
        
        if !active_projects.is_empty() {
            ui.label(egui::RichText::new("Current Research").strong().size(16.0));
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
                                ui.add(egui::Image::new(egui::load::SizedTexture::new(*tex, [16.0, 16.0]))
                                    .tint(cat_color));
                            }
                            ui.label(egui::RichText::new(&tech.name).strong());
                            ui.label(egui::RichText::new(tech.category.display_name()).size(12.0).color(cat_color));
                            if !info.active {
                                ui.label(egui::RichText::new("PAUSED").color(egui::Color32::YELLOW));
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
                            pending_research.update_allocations.push(
                                (tech_id.to_string(), alloc_pct as f64 / 100.0),
                            );
                        }
                        if info.active {
                            if ui.button("⏸ Pause").on_hover_text("Pause research (preserves progress, frees team slot)").clicked() {
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
                        if ui.button("⏹ Stop").on_hover_text("Stop research entirely (removes project, progress is lost)").clicked() {
                            // Store pending cancellation in temporary data to show confirmation dialog
                            ui.data_mut(|data| {
                                data.insert_temp(ui.id().with("pending_cancel"), tech_id.to_string());
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
                && tech_data.check_prerequisites(tech_id, &unlocked_ids) {
                available_techs.push(tech);
            }
        }
        
        if available_techs.is_empty() && active_projects.is_empty() {
            ui.label(egui::RichText::new("No technologies available for research")
                .italics()
                .color(egui::Color32::GRAY));
            ui.label("Complete more research to unlock new technologies.");
        } else if !available_techs.is_empty() {
            ui.label(egui::RichText::new("Available to Start").strong().size(16.0));
            ui.add_space(4.0);
            
            available_techs.sort_by(|a, b| {
                a.category.display_name()
                    .cmp(b.category.display_name())
                    .then(a.research_cost.partial_cmp(&b.research_cost).unwrap())
            });
            
            for tech in available_techs {
                let cat_color = tech_category_color(tech.category);
                let can_start = teams_available > 0;
                ui.horizontal(|ui| {
                    // Info labels in a scope so tooltip hover isn't stolen by the button
                    let info_scope = ui.scope(|ui| {
                        ui.label(egui::RichText::new("⏳").color(egui::Color32::from_rgb(255, 255, 100)));
                        if let Some(tex) = icon_textures.get(&tech.category) {
                            ui.add(egui::Image::new(egui::load::SizedTexture::new(*tex, [16.0, 16.0]))
                                .tint(cat_color));
                        }
                        ui.label(egui::RichText::new(&tech.name).strong());
                        ui.label(egui::RichText::new(tech.category.display_name()).size(12.0).color(cat_color));
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new(format!("{:.0} RP", tech.research_cost))
                            .color(egui::Color32::from_rgb(150, 200, 255)));
                        ui.label(egui::RichText::new(format!("T{}", tech.tier))
                            .size(11.0).color(egui::Color32::GRAY));
                        if !tech.unlocks_components.is_empty() {
                            ui.label(egui::RichText::new(format!("⚙{}", tech.unlocks_components.len()))
                                .size(11.0).color(egui::Color32::from_rgb(140, 230, 200)));
                        }
                        if !tech.modifiers.is_empty() {
                            ui.label(egui::RichText::new(format!("✦{}", tech.modifiers.len()))
                                .size(11.0).color(egui::Color32::from_rgb(100, 255, 100)));
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
) {
    ui.heading("Engineering Projects");
    ui.label("Component designs ready for engineering");
    ui.separator();
    
    egui::ScrollArea::vertical().show(ui, |ui| {
        let mut available_components = Vec::new();
        
        for (comp_id, component) in &tech_data.components {
            if research_state.is_unlocked(&component.required_tech) 
                && !research_state.is_component_completed(comp_id) {
                available_components.push(component);
            }
        }
        
        if available_components.is_empty() {
            ui.label(egui::RichText::new("No components available for engineering")
                .italics()
                .color(egui::Color32::GRAY));
            ui.label("Research new technologies to unlock component designs.");
        } else {
            // Sort by cost
            available_components.sort_by(|a, b| {
                a.engineering_cost.partial_cmp(&b.engineering_cost).unwrap()
            });

            for component in available_components {
                let parent_tech = tech_data.get_tech(&component.required_tech);
                let cat_color = parent_tech
                    .map(|t| tech_category_color(t.category))
                    .unwrap_or(egui::Color32::from_rgb(200, 200, 100));

                let row = ui.horizontal(|ui| {
                    // Component icon
                    ui.label(egui::RichText::new("⚙").color(cat_color));
                    // Category icon
                    if let Some(tech) = parent_tech {
                        if let Some(tex) = icon_textures.get(&tech.category) {
                            ui.add(egui::Image::new(egui::load::SizedTexture::new(*tex, [16.0, 16.0]))
                                .tint(cat_color));
                        }
                    }
                    // Component name
                    ui.label(egui::RichText::new(&component.name).strong());
                    // Category
                    if let Some(tech) = parent_tech {
                        ui.label(egui::RichText::new(tech.category.display_name()).size(12.0).color(cat_color));
                    }
                    ui.add_space(8.0);
                    // Cost
                    ui.label(egui::RichText::new(format!("{:.0} EP", component.engineering_cost))
                        .color(egui::Color32::from_rgb(150, 255, 200)));
                    // From tech
                    if let Some(tech) = parent_tech {
                        ui.label(egui::RichText::new(format!("(from: {})", tech.name))
                            .size(11.0)
                            .italics()
                            .color(egui::Color32::GRAY));
                    }
                    // Start button
                    let _ = ui.button("🔧 Start Engineering (NYI)");
                });
                // Tooltip with component details
                row.response.on_hover_ui(|ui| {
                    ui.set_max_width(320.0);
                    ui.label(egui::RichText::new(&component.name).strong().size(14.0));
                    if let Some(tech) = parent_tech {
                        ui.horizontal(|ui| {
                            if let Some(tex) = icon_textures.get(&tech.category) {
                                ui.add(egui::Image::new(egui::load::SizedTexture::new(*tex, [16.0, 16.0]))
                                    .tint(cat_color));
                            }
                            ui.label(egui::RichText::new(tech.category.display_name()).color(cat_color));
                        });
                    }
                    ui.separator();
                    ui.label(&component.description);
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new(format!("Engineering Cost: {:.0} EP", component.engineering_cost))
                        .color(egui::Color32::from_rgb(150, 255, 200)).strong());
                    if let Some(tech) = parent_tech {
                        ui.label(egui::RichText::new(format!("Required Tech: {}", tech.name))
                            .size(12.0).color(egui::Color32::GRAY));
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
    ui.heading("Current Bonuses");
    ui.label("Active modifiers from researched technologies");
    ui.separator();

    // Build a lookup: for each modifier type, which techs contribute and how much.
    // Done outside the scroll area so the detail Area can reference it unconditionally.
    let mut modifier_sources: HashMap<&ModifierType, Vec<(&crate::research::types::Technology, f64)>> = HashMap::new();
    for (_tech_id, tech) in &tech_data.technologies {
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
    let pinned_id   = ui.id().with("bonuses_pinned");   // (name, row_rect): pinned on click
    let hover_id    = ui.id().with("bonuses_hover");    // (name, hold_until, row_rect): hover with hold time

    let now = ui.input(|i| i.time);
    let pinned_data:   Option<(String, egui::Rect)> = ui.data(|d| d.get_temp(pinned_id));
    let hover_data:    Option<(String, f64, egui::Rect)> = ui.data(|d| d.get_temp(hover_id));

    // Sort and partition modifiers
    let mut sorted_modifiers: Vec<_> = research_state.active_modifiers.iter().collect();
    sorted_modifiers.sort_by(|(a, _), (b, _)| {
        let a_is_unlock = matches!(a, ModifierType::UnlockMechanic(_));
        let b_is_unlock = matches!(b, ModifierType::UnlockMechanic(_));
        b_is_unlock.cmp(&a_is_unlock).then_with(|| a.display_name().cmp(&b.display_name()))
    });
    let (unlocks, bonuses): (Vec<_>, Vec<_>) = sorted_modifiers
        .into_iter()
        .partition(|(m, _)| matches!(m, ModifierType::UnlockMechanic(_)));

    egui::ScrollArea::vertical().show(ui, |ui| {
        if research_state.active_modifiers.is_empty() {
            ui.label(egui::RichText::new("No bonuses active yet")
                .italics()
                .color(egui::Color32::GRAY));
            ui.label("Research technologies to unlock bonuses.");
            return;
        }

        // Helper: render a single bonus row, returning the row response.
        // No detail box is rendered here — the caller handles that after all rows.
        let pinned_name = pinned_data.as_ref().map(|(n, _)| n.as_str());

        if !bonuses.is_empty() {
            ui.label(egui::RichText::new("Numeric Bonuses").strong().size(16.0));
            ui.add_space(4.0);

            for (modifier_type, total_value) in &bonuses {
                let is_positive = **total_value >= 0.0;
                let is_beneficial = match modifier_type {
                    ModifierType::ConstructionCost | ModifierType::ShipMaintenance => !is_positive,
                    _ => is_positive,
                };
                let value_color = if is_beneficial {
                    egui::Color32::from_rgb(100, 255, 100)
                } else {
                    egui::Color32::from_rgb(255, 100, 100)
                };

                let modifier_name = modifier_type.display_name();
                let is_pinned = pinned_name.map_or(false, |p| p == modifier_name);

                let row_rect = {
                    let row = ui.horizontal(|ui| {
                        // Highlight pinned row
                        if is_pinned {
                            let row_rect = ui.max_rect();
                            ui.painter().rect_filled(
                                row_rect,
                                2.0,
                                egui::Color32::from_rgba_unmultiplied(40, 40, 40, 120),
                            );
                        }
                        ui.label(egui::RichText::new(if is_beneficial { "▲" } else { "▼" })
                            .color(value_color));
                        ui.label(egui::RichText::new(&modifier_name).strong());
                        ui.label(egui::RichText::new(format!("{:+.0}%", total_value))
                            .color(value_color)
                            .strong());
                        let source_count = modifier_sources.get(modifier_type).map_or(0, |v| v.len());
                        if source_count > 0 {
                            ui.label(egui::RichText::new(format!(
                                "({} source{})", source_count, if source_count > 1 { "s" } else { "" }
                            )).size(11.0).color(egui::Color32::GRAY));
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
                    interact.clone().on_hover_cursor(egui::CursorIcon::PointingHand);
                    ui.data_mut(|d| d.insert_temp(hover_id, (modifier_name.clone(), now + 0.25, row_rect)));
                }
                if interact.clicked() {
                    if is_pinned {
                        ui.data_mut(|d| d.remove::<(String, egui::Rect)>(pinned_id));
                    } else {
                        ui.data_mut(|d| d.insert_temp(pinned_id, (modifier_name.clone(), row_rect)));
                    }
                }

                ui.add_space(2.0);
            }
            ui.add_space(10.0);
        }

        if !unlocks.is_empty() {
            ui.label(egui::RichText::new("Unlocked Mechanics").strong().size(16.0));
            ui.add_space(4.0);

            for (modifier_type, _value) in &unlocks {
                let modifier_name = modifier_type.display_name();
                let is_pinned = pinned_name.map_or(false, |p| p == modifier_name);

                let row_rect = {
                    let row = ui.horizontal(|ui| {
                        if is_pinned {
                            let row_rect = ui.max_rect();
                            ui.painter().rect_filled(
                                row_rect,
                                2.0,
                                egui::Color32::from_rgba_unmultiplied(40, 40, 40, 120),
                            );
                        }
                        ui.label(egui::RichText::new("✔").color(egui::Color32::from_rgb(100, 255, 200)));
                        ui.label(egui::RichText::new(&modifier_name)
                            .strong()
                            .color(egui::Color32::from_rgb(120, 220, 255)));
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
                    interact.clone().on_hover_cursor(egui::CursorIcon::PointingHand);
                    ui.data_mut(|d| d.insert_temp(hover_id, (modifier_name.clone(), now + 0.25, row_rect)));
                }
                if interact.clicked() {
                    if is_pinned {
                        ui.data_mut(|d| d.remove::<(String, egui::Rect)>(pinned_id));
                    } else {
                        ui.data_mut(|d| d.insert_temp(pinned_id, (modifier_name.clone(), row_rect)));
                    }
                }

                ui.add_space(2.0);
            }
        }

        ui.add_space(4.0);
        ui.label(egui::RichText::new("Click a row to pin its detail box. Click again to unpin.")
            .size(10.0)
            .italics()
            .color(egui::Color32::DARK_GRAY));
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
        if let Some((modifier_type, total_value)) = all_modifiers.find(|(m, _)| m.display_name() == detail_name) {
            let is_positive = **total_value >= 0.0;
            let is_beneficial = match modifier_type {
                ModifierType::ConstructionCost | ModifierType::ShipMaintenance => !is_positive,
                _ => is_positive,
            };
            let value_color = if is_beneficial {
                egui::Color32::from_rgb(100, 255, 100)
            } else {
                egui::Color32::from_rgb(255, 100, 100)
            };
            let border_color = if is_pinned {
                value_color
            } else {
                egui::Color32::from_rgb(100, 100, 100)
            };
            let border_width = if is_pinned { 2.0 } else { 1.0 };

            let pos = egui::pos2(row_rect.right() + 24.0, row_rect.top());

            let area_resp = egui::Area::new(ui.id().with("bonus_detail_float"))
                .fixed_pos(pos)
                .order(egui::Order::Tooltip)
                .interactable(true)
                .show(ui.ctx(), |ui| {
                    egui::Frame::NONE
                        .fill(egui::Color32::from_rgba_unmultiplied(30, 30, 35, 245))
                        .stroke(egui::Stroke::new(border_width, border_color))
                        .inner_margin(10.0)
                        .corner_radius(4.0)
                        .show(ui, |ui| {
                            ui.set_max_width(280.0);
                            render_bonus_detail_content(
                                ui, modifier_type, **total_value, &modifier_sources, icon_textures,
                            );
                        });
                });

            // If the pointer is over the floating Area, refresh the hover hold time
            // so the box stays open while the user reads it.
            if area_resp.response.hovered() || area_resp.response.contains_pointer() {
                ui.data_mut(|d| d.insert_temp(hover_id, (detail_name.clone(), now + 0.25, row_rect)));
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
        ui.label(egui::RichText::new(format!("Total: {:+.0}%", total_value))
            .color(if total_value >= 0.0 {
                egui::Color32::from_rgb(100, 255, 100)
            } else {
                egui::Color32::from_rgb(255, 100, 100)
            })
            .strong()
            .size(13.0));
        ui.add_space(3.0);
    }

    ui.label(egui::RichText::new("Contributing Technologies:").strong().size(12.0));
    ui.add_space(2.0);
    
    if let Some(sources) = modifier_sources.get(modifier_type) {
        for (tech, value) in sources {
            let cat_color = tech_category_color(tech.category);
            ui.horizontal(|ui| {
                if let Some(tex) = icon_textures.get(&tech.category) {
                    ui.add(egui::Image::new(egui::load::SizedTexture::new(*tex, [12.0, 12.0]))
                        .tint(cat_color));
                }
                ui.label(egui::RichText::new(&tech.name).color(cat_color).size(11.0));
                if !is_unlock {
                    ui.label(egui::RichText::new(format!("{:+.0}%", value))
                        .size(11.0)
                        .color(if *value >= 0.0 {
                            egui::Color32::from_rgb(100, 255, 100)
                        } else {
                            egui::Color32::from_rgb(255, 100, 100)
                        }));
                }
            });
        }
    } else {
        ui.label(egui::RichText::new("No tech sources found")
            .italics()
            .size(10.0)
            .color(egui::Color32::GRAY));
    }
}

/// Render the Archive tab
fn render_archive_tab(
    ui: &mut egui::Ui,
    research_state: &ResearchState,
    tech_data: &TechnologiesData,
    icon_textures: &HashMap<TechCategory, egui::TextureId>,
) {
    ui.heading("Research Archive");
    ui.label("Completed technologies and components");
    ui.separator();
    
    egui::ScrollArea::vertical().show(ui, |ui| {
        // Completed Technologies
        ui.group(|ui| {
            ui.label(egui::RichText::new("Completed Technologies").strong().size(16.0));
            ui.separator();
            
            let unlocked_count = research_state.unlocked_technologies.len();
            ui.label(format!("Total: {} technologies", unlocked_count));
            ui.add_space(5.0);
            
            if unlocked_count == 0 {
                ui.label(egui::RichText::new("No technologies researched yet")
                    .italics()
                    .color(egui::Color32::GRAY));
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
                                ui.add(egui::Image::new(egui::load::SizedTexture::new(*tex, [16.0, 16.0])));
                            } else {
                                ui.label(category.icon());
                            }
                            ui.label(egui::RichText::new(format!(
                                "{} ({} completed)",
                                category.display_name(),
                                category_completed.len()
                            )).strong());
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
                                        ui.label(egui::RichText::new(format!("({:.0} RP)", tech.research_cost))
                                            .size(11.0)
                                            .color(egui::Color32::from_rgb(120, 200, 255)));
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
            ui.label(egui::RichText::new("Completed Components").strong().size(16.0));
            ui.separator();
            
            let completed_count = research_state.completed_components.len();
            ui.label(format!("Total: {} components", completed_count));
            ui.add_space(5.0);
            
            if completed_count == 0 {
                ui.label(egui::RichText::new("No components engineered yet")
                    .italics()
                    .color(egui::Color32::GRAY));
            } else {
                for comp_id in &research_state.completed_components {
                    if let Some(component) = tech_data.get_component(comp_id) {
                        ui.horizontal(|ui| {
                            ui.label("⚙");
                            ui.label(&component.name);
                            ui.label(egui::RichText::new(format!("({:.0} EP)", component.engineering_cost))
                                .size(11.0)
                                .color(egui::Color32::GRAY));
                        });
                    }
                }
            }
        });
    });
}

