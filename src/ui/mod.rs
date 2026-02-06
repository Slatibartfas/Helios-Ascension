//! UI module for the Helios Ascension interface
//!
//! Provides an egui-based dashboard showing:
//! - Resource stockpiles and critical resources
//! - Power grid status
//! - Selected celestial body information
//! - Time controls for simulation speed

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

pub mod interaction;

pub use interaction::Selection;

use crate::astronomy::{AtmosphereComposition, Hovered, KeplerOrbit, Selected, SpaceCoordinates};
use crate::economy::{format_power, GlobalBudget, PlanetResources, ResourceType};
use crate::plugins::solar_system::{CelestialBody, LogicalParent};
use crate::plugins::solar_system_data::BodyType;
use crate::plugins::camera::{CameraAnchor, GameCamera};

/// Time scale resource for controlling simulation speed
#[derive(Resource, Debug, Clone)]
pub struct TimeScale {
    /// Current time scale multiplier (0.0 = paused, 1.0 = normal, up to 5,000,000)
    pub scale: f32,
}

impl TimeScale {
    /// Create a new time scale with default value
    pub fn new() -> Self {
        Self { scale: 1.0 }
    }

    /// Pause the simulation
    pub fn pause(&mut self) {
        self.scale = 0.0;
    }

    /// Resume at normal speed
    pub fn resume(&mut self) {
        self.scale = 1.0;
    }

    /// Check if paused
    pub fn is_paused(&self) -> bool {
        self.scale == 0.0
    }
}

impl Default for TimeScale {
    fn default() -> Self {
        Self::new()
    }
}

/// Plugin that adds the UI system to the Bevy app
pub struct UIPlugin;

impl Plugin for UIPlugin {
    fn build(&self, app: &mut App) {
        app
            // Note: EguiPlugin is already added by WorldInspectorPlugin in main.rs
            // Resources
            .init_resource::<Selection>()
            .init_resource::<TimeScale>()
            // Systems
            .add_systems(Update, (
                ui_dashboard,
                ui_hover_tooltip,
                sync_selection_with_astronomy,
                apply_time_scale,
            ));
    }
}

/// System that syncs the UI selection with the astronomy Selected component
fn sync_selection_with_astronomy(
    mut selection: ResMut<Selection>,
    selected_query: Query<Entity, (With<Selected>, With<CelestialBody>)>,
) {
    // If something is selected in astronomy, update UI selection
    if let Ok(entity) = selected_query.get_single() {
        if !selection.is_selected(entity) {
            selection.select(entity);
        }
    } else if selection.has_selection() {
        // If nothing is selected in astronomy, clear UI selection
        selection.clear();
    }
}

/// System that applies the time scale to the game time
/// Only updates when TimeScale resource changes for efficiency
fn apply_time_scale(
    time_scale: Res<TimeScale>,
    mut time: ResMut<Time<Virtual>>,
) {
    // Only update when time scale changes or is first added
    if time_scale.is_changed() || time_scale.is_added() {
        time.set_relative_speed(time_scale.scale);
    }
}

/// Helper function to render a selectable label with highlighting for selected items
fn render_selectable_label(
    ui: &mut egui::Ui,
    is_selected: bool,
    name: &str,
) -> egui::Response {
    if is_selected {
        ui.selectable_label(is_selected, name).highlight()
    } else {
        ui.selectable_label(is_selected, name)
    }
}

fn render_body_row(
    ui: &mut egui::Ui,
    entity: Entity,
    body: &CelestialBody,
    selection: &mut Selection,
    commands: &mut Commands,
    selected_query: &Query<Entity, With<Selected>>,
    anchor_query: &mut Query<&mut CameraAnchor, With<GameCamera>>,
) {
    let is_selected = selection.is_selected(entity);
    ui.horizontal(|ui| {
        ui.add_space(20.0);
        if ui.small_button("⚓").on_hover_text("Anchor Camera").clicked() {
            // Select the body when anchoring
            for e in selected_query.iter() { commands.entity(e).remove::<Selected>(); }
            commands.entity(entity).insert(Selected);
            selection.select(entity);
            
            // Anchor the camera
            if let Ok(mut anchor) = anchor_query.get_single_mut() {
                anchor.0 = Some(entity);
            }
        }
        
        // Use a visually distinct style for selected items
        if render_selectable_label(ui, is_selected, &body.name).clicked() {
             for e in selected_query.iter() { commands.entity(e).remove::<Selected>(); }
             commands.entity(entity).insert(Selected);
             selection.select(entity);
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn render_grouped_children(
    ui: &mut egui::Ui,
    children: &[Entity],
    group_name: &str,
    parent_entity: Entity,
    body_map: &std::collections::HashMap<Entity, &CelestialBody>,
    selection: &mut Selection,
    commands: &mut Commands,
    selected_query: &Query<Entity, With<Selected>>,
    anchor_query: &mut Query<&mut CameraAnchor, With<GameCamera>>,
) {
    if children.is_empty() { return; }
    
    // Make ID unique by including parent entity to avoid UI jumping bug
    let id = ui.make_persistent_id((group_name, parent_entity));
    egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false)
        .show_header(ui, |ui| {
             ui.label(format!("{} ({})", group_name, children.len()));
        })
        .body(|ui| {
            for &child_entity in children {
                if let Some(body) = body_map.get(&child_entity) {
                    render_body_row(ui, child_entity, body, selection, commands, selected_query, anchor_query);
                }
            }
        });
}


#[allow(clippy::too_many_arguments)]
fn render_body_tree(
    ui: &mut egui::Ui,
    entity: Entity,
    body_map: &std::collections::HashMap<Entity, &CelestialBody>,
    hierarchy: &std::collections::HashMap<Entity, Vec<Entity>>,
    selection: &mut Selection,
    commands: &mut Commands,
    selected_query: &Query<Entity, With<Selected>>,
    anchor_query: &mut Query<&mut CameraAnchor, With<GameCamera>>,
) {
    if let Some(body) = body_map.get(&entity) {
        let is_selected = selection.is_selected(entity);
        let id = ui.make_persistent_id(entity);
        
        // Group children by type
        let mut child_planets = Vec::new();
        let mut child_moons = Vec::new(); // Usually planets have moons
        let mut child_asteroids = Vec::new();
        let mut child_comets = Vec::new();
        let mut child_dwarf_planets = Vec::new();
        let mut child_others = Vec::new();

        let has_children = if let Some(children) = hierarchy.get(&entity) {
            for &child in children {
                if let Some(child_body) = body_map.get(&child) {
                     match child_body.body_type {
                         BodyType::Planet => child_planets.push(child),
                         BodyType::Moon => child_moons.push(child),
                         BodyType::Asteroid => child_asteroids.push(child),
                         BodyType::Comet => child_comets.push(child),
                         BodyType::DwarfPlanet => child_dwarf_planets.push(child),
                         _ => child_others.push(child),
                     }
                }
            }
            true
        } else {
            false
        };

        if has_children {
             egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, body.name == "Sol")
                .show_header(ui, |ui| {
                    if ui.small_button("⚓").on_hover_text("Anchor Camera").clicked() {
                        // Select the body when anchoring
                        for e in selected_query.iter() { commands.entity(e).remove::<Selected>(); }
                        commands.entity(entity).insert(Selected);
                        selection.select(entity);
                        
                        // Anchor the camera
                        if let Ok(mut anchor) = anchor_query.get_single_mut() {
                            anchor.0 = Some(entity);
                        }
                    }
                    
                    // Use a visually distinct style for selected items
                    if render_selectable_label(ui, is_selected, &body.name).clicked() {
                         for e in selected_query.iter() { commands.entity(e).remove::<Selected>(); }
                         commands.entity(entity).insert(Selected);
                         selection.select(entity);
                    }
                })
                .body(|ui| {
                     // 1. Planets (Recursive)
                    for child in child_planets {
                        render_body_tree(ui, child, body_map, hierarchy, selection, commands, selected_query, anchor_query);
                    }
                    // 2. Dwarf Planets (Grouped or Recursive if important?) Grouped.
                    render_grouped_children(ui, &child_dwarf_planets, "Dwarf Planets", entity, body_map, selection, commands, selected_query, anchor_query);
                    // 3. Moons (Usually under planets, but if under Sol/others?)
                    render_grouped_children(ui, &child_moons, "Moons", entity, body_map, selection, commands, selected_query, anchor_query);
                     // 4. Asteroids
                    render_grouped_children(ui, &child_asteroids, "Asteroids", entity, body_map, selection, commands, selected_query, anchor_query);
                     // 5. Comets
                    render_grouped_children(ui, &child_comets, "Comets", entity, body_map, selection, commands, selected_query, anchor_query);
                     // 6. Others
                    for child in child_others {
                        render_body_tree(ui, child, body_map, hierarchy, selection, commands, selected_query, anchor_query);
                    }
                });
        } else {
             render_body_row(ui, entity, body, selection, commands, selected_query, anchor_query);
        }
    }
}

/// System that displays a tooltip for hovered celestial bodies
fn ui_hover_tooltip(
    mut contexts: EguiContexts,
    hovered_query: Query<&CelestialBody, With<Hovered>>,
) {
    let ctx = match contexts.try_ctx_mut() {
        Some(ctx) => ctx,
        None => return,
    };

    // Display hover tooltip if a body is hovered
    if let Ok(body) = hovered_query.get_single() {
        egui::Area::new("hover_tooltip".into())
            .fixed_pos(egui::pos2(10.0, 60.0))
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(egui::Color32::from_rgba_unmultiplied(30, 30, 30, 240))
                    .stroke(egui::Stroke::new(2.0, egui::Color32::from_rgb(100, 180, 255)))
                    .inner_margin(12.0)
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(&body.name)
                                .size(16.0)
                                .color(egui::Color32::from_rgb(150, 220, 255))
                                .strong()
                        );
                        ui.label(
                            egui::RichText::new(format!("Type: {:?}", body.body_type))
                                .size(12.0)
                                .color(egui::Color32::from_rgb(180, 180, 180))
                        );
                    });
            });
    }
}

/// Main UI dashboard system
#[allow(clippy::too_many_arguments)]
fn ui_dashboard(
    mut commands: Commands,
    mut contexts: EguiContexts,
    budget: Res<GlobalBudget>,
    mut time_scale: ResMut<TimeScale>,
    mut selection: ResMut<Selection>,
    // Query for selected body information
    body_query: Query<(&CelestialBody, &SpaceCoordinates, Option<&KeplerOrbit>, Option<&PlanetResources>, Option<&AtmosphereComposition>)>,
    // Ledger queries
    all_bodies_query: Query<(Entity, &CelestialBody, Option<&LogicalParent>)>,
    selected_query: Query<Entity, With<Selected>>,
    mut anchor_query: Query<&mut CameraAnchor, With<GameCamera>>,
) {
    let ctx = match contexts.try_ctx_mut() {
        Some(ctx) => ctx,
        None => return,
    };

    // Ledger Panel (Left)
    egui::SidePanel::left("ledger_panel")
        .min_width(200.0)
        .show(ctx, |ui| {
            ui.heading("Celestial Objects");
            ui.separator();
            
            egui::ScrollArea::vertical().show(ui, |ui| {
                let mut hierarchy: std::collections::HashMap<Entity, Vec<Entity>> = std::collections::HashMap::new();
                let mut roots: Vec<Entity> = Vec::new();
                let mut body_map: std::collections::HashMap<Entity, &CelestialBody> = std::collections::HashMap::new();

                for (entity, body, logical_parent) in all_bodies_query.iter() {
                    body_map.insert(entity, body);
                    if let Some(logical_parent) = logical_parent {
                        hierarchy.entry(logical_parent.0).or_default().push(entity);
                    } else {
                        roots.push(entity);
                    }
                }
                
                 roots.sort_by(|a, b| {
                     let name_a = &body_map.get(a).unwrap().name;
                     let name_b = &body_map.get(b).unwrap().name;
                     if name_a == "Sol" { return std::cmp::Ordering::Less; }
                     if name_b == "Sol" { return std::cmp::Ordering::Greater; }
                     name_a.cmp(name_b)
                 });

                for root in roots {
                    render_body_tree(ui, root, &body_map, &hierarchy, &mut selection, &mut commands, &selected_query, &mut anchor_query);
                }
            });
        });

    // Top header panel with resource categories and power
    egui::TopBottomPanel::top("header_panel")
        .min_height(80.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Helios: Ascension");
                ui.separator();

                // Show resource categories with hover expansion
                for (category_name, resources) in ResourceType::by_category() {
                    // Calculate total for category
                    let category_total: f64 = resources.iter()
                        .map(|r| budget.get_stockpile(r))
                        .sum();
                    
                    let category_label = ui.label(format!("{}: {:.1} Mt", category_name, category_total));
                    
                    // Show detailed breakdown on hover
                    category_label.on_hover_ui(|ui| {
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new(category_name).strong());
                            ui.separator();
                            for resource in &resources {
                                let amount = budget.get_stockpile(resource);
                                ui.label(format!("  {} ({}): {:.1} Mt", 
                                    resource.display_name(),
                                    resource.symbol(),
                                    amount
                                ));
                            }
                        });
                    });
                    
                    ui.separator();
                }

                // Power grid status
                let net_power = budget.net_power();
                let power_color = if net_power >= 0.0 {
                    egui::Color32::GREEN
                } else {
                    egui::Color32::RED
                };
                
                ui.colored_label(
                    power_color,
                    format!("⚡ {}", format_power(budget.energy_grid.produced)),
                );
                ui.label(format!("(Net: {})", format_power(net_power)));
            });

            // Second row with civilization score and efficiency
            ui.horizontal(|ui| {
                ui.label(format!("Civilization Score: {:.1}", budget.civilization_score));
                ui.separator();
                ui.label(format!(
                    "Grid Efficiency: {:.1}%",
                    budget.power_efficiency() * 100.0
                ));
            });
        });

    // Right side panel for selected body information
    if selection.has_selection() {
        egui::SidePanel::right("selection_panel")
            .min_width(300.0)
            .max_width(400.0)
            .show(ctx, |ui| {
                ui.heading("Selected Body");
                ui.separator();

                if let Some(entity) = selection.get() {
                    if let Ok((body, coords, orbit, resources, atmosphere)) = body_query.get(entity) {
                        // Body name and basic info
                        ui.label(egui::RichText::new(&body.name).size(18.0).strong());
                        ui.add_space(10.0);

                        // Position information
                        ui.group(|ui| {
                            ui.label(egui::RichText::new("Position").strong());
                            let distance = coords.position.length();
                            ui.label(format!("Distance from Sun: {:.3} AU", distance));
                            ui.label(format!("Radius: {:.1} km", body.radius));
                            ui.label(format!("Mass: {:.2e} kg", body.mass));
                        });

                        ui.add_space(10.0);

                        // Orbital data if available
                        if let Some(orbit) = orbit {
                            ui.group(|ui| {
                                ui.label(egui::RichText::new("Orbital Elements").strong());
                                ui.label(format!("Semi-major axis: {:.3} AU", orbit.semi_major_axis));
                                ui.label(format!("Eccentricity: {:.4}", orbit.eccentricity));
                                ui.label(format!("Inclination: {:.2}°", orbit.inclination.to_degrees()));
                                
                                // Calculate and show orbital period
                                let period_seconds = crate::astronomy::KeplerOrbit::period_from_mean_motion(orbit.mean_motion);
                                let period_days = period_seconds / 86400.0;
                                if period_days < 365.0 {
                                    ui.label(format!("Period: {:.1} days", period_days));
                                } else {
                                    ui.label(format!("Period: {:.2} years", period_days / 365.25));
                                }
                            });

                            ui.add_space(10.0);
                        }

                        // Atmosphere data if available
                        if let Some(atmosphere) = atmosphere {
                            ui.group(|ui| {
                                let id = ui.make_persistent_id(("atmosphere_header", entity));
                                egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true)
                                    .show_header(ui, |ui| {
                                        ui.label(egui::RichText::new("🌍 Atmosphere").strong());
                                    })
                                    .body(|ui| {
                                        // Basic atmosphere properties
                                        ui.horizontal(|ui| {
                                            // Display appropriate label based on whether this is reference or surface pressure
                                            if atmosphere.is_reference_pressure {
                                                ui.label("Pressure (at 1 bar ref):");
                                            } else {
                                                ui.label("Surface Pressure:");
                                            }
                                            let pressure_bar = atmosphere.surface_pressure_mbar / 1000.0;
                                            if pressure_bar >= 1.0 {
                                                ui.label(format!("{:.2} bar", pressure_bar));
                                            } else {
                                                ui.label(format!("{:.0} mbar", atmosphere.surface_pressure_mbar));
                                            }
                                        });
                                        
                                        // Show harvest altitude for gas giants
                                        if atmosphere.is_reference_pressure && atmosphere.harvest_altitude_bar > 0.0 {
                                            ui.horizontal(|ui| {
                                                ui.label("Harvest Altitude:");
                                                let yield_mult = atmosphere.harvest_yield_multiplier();
                                                ui.label(format!("{:.1} bar ({:.1}× yield)", 
                                                    atmosphere.harvest_altitude_bar, yield_mult));
                                            });
                                            
                                            ui.horizontal(|ui| {
                                                ui.label("Max Harvest Depth:");
                                                ui.label(format!("{:.1} bar (tech-limited)", 
                                                    atmosphere.max_harvest_altitude_bar));
                                            });
                                        }
                                        
                                        ui.horizontal(|ui| {
                                            ui.label("Temperature:");
                                            ui.label(format!("{:.1}°C", atmosphere.surface_temperature_celsius));
                                        });
                                        
                                        ui.horizontal(|ui| {
                                            ui.label("Breathable:");
                                            if atmosphere.breathable {
                                                ui.colored_label(egui::Color32::GREEN, "✓ Yes");
                                            } else {
                                                ui.colored_label(egui::Color32::RED, "✗ No");
                                            }
                                        });
                                        
                                        ui.horizontal(|ui| {
                                            ui.label("Colony Cost:");
                                            let cost = atmosphere.calculate_colony_cost();
                                            let cost_color = match cost {
                                                0 => egui::Color32::GREEN,
                                                1..=3 => egui::Color32::YELLOW,
                                                4..=6 => egui::Color32::from_rgb(255, 165, 0), // Orange
                                                _ => egui::Color32::RED,
                                            };
                                            ui.colored_label(cost_color, format!("{}/8", cost));
                                        });
                                        
                                        ui.add_space(5.0);
                                        
                                        // Gas composition in collapsible section
                                        let gas_id = ui.make_persistent_id(("gas_composition", entity));
                                        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), gas_id, false)
                                            .show_header(ui, |ui| {
                                                ui.label(egui::RichText::new("Gas Composition").size(12.0));
                                            })
                                            .body(|ui| {
                                                for gas in &atmosphere.gases {
                                                    ui.horizontal(|ui| {
                                                        ui.label(format!("  {}:", gas.name));
                                                        ui.label(format!("{:.2}%", gas.percentage));
                                                    });
                                                }
                                            });
                                    });
                            });

                            ui.add_space(10.0);
                        }

                        // Resources if available
                        if let Some(resources) = resources {
                            ui.group(|ui| {
                                ui.label(egui::RichText::new("Resources").strong());
                                ui.label(format!("Body mass: {:.2e} kg", body.mass));
                                ui.add_space(5.0);
                                
                                egui::ScrollArea::vertical()
                                    .max_height(400.0)
                                    .show(ui, |ui| {
                                        // Group resources by category
                                        for (category_name, category_resources) in ResourceType::by_category() {
                                            ui.label(egui::RichText::new(category_name).strong().color(egui::Color32::LIGHT_BLUE));
                                            
                                            for resource_type in &category_resources {
                                                if let Some(deposit) = resources.get_deposit(resource_type) {
                                                    // Calculate absolute amount in megatons
                                                    let amount_mt = deposit.calculate_megatons(body.mass);
                                                    
                                                    ui.horizontal(|ui| {
                                                        ui.label(format!("  {} ({})", 
                                                            resource_type.display_name(),
                                                            resource_type.symbol()
                                                        ));
                                                    });
                                                    
                                                    ui.horizontal(|ui| {
                                                        ui.label("    Amount:");
                                                        ui.label(egui::RichText::new(format!("{:.2e} Mt", amount_mt)).strong());
                                                    });
                                                    
                                                    ui.horizontal(|ui| {
                                                        ui.label("    Concentration:");
                                                        ui.add(egui::ProgressBar::new(deposit.abundance as f32)
                                                            .text(format!("{:.1}%", deposit.abundance * 100.0)));
                                                    });
                                                    
                                                    ui.horizontal(|ui| {
                                                        ui.label("    Accessibility:");
                                                        ui.add(egui::ProgressBar::new(deposit.accessibility)
                                                            .text(format!("{:.1}%", deposit.accessibility * 100.0)));
                                                    });
                                                    
                                                    ui.add_space(3.0);
                                                }
                                            }
                                            
                                            ui.add_space(8.0);
                                        }

                                        // Summary
                                        ui.separator();
                                        ui.label(format!("Total viable deposits: {}", resources.viable_count()));
                                        ui.label(format!("Total resource value: {:.2}", resources.total_value()));
                                    });
                            });
                        } else {
                            ui.label("No resource data available");
                        }
                    } else {
                        ui.label("Selected entity not found");
                    }
                } else {
                    ui.label("No selection");
                }
            });
    }

    // Bottom panel for time controls
    egui::TopBottomPanel::bottom("time_controls")
        .min_height(80.0)
        .show(ctx, |ui| {
            ui.heading("Time Controls");
            ui.separator();

            ui.horizontal(|ui| {
                // Pause/Resume button
                if time_scale.is_paused() {
                    if ui.button("▶ Resume").clicked() {
                        time_scale.resume();
                    }
                } else if ui.button("⏸ Pause").clicked() {
                    time_scale.pause();
                }

                ui.separator();

                // Preset speed buttons with more moderate, performance-friendly time scales
                // Speed 1: 1 Earth rotation per 10 seconds (more moderate starting speed)
                if ui.button("⟳ 1d/10s").on_hover_text("1 Earth rotation per 10 seconds").clicked() {
                    time_scale.scale = 8640.0;
                }
                // Speed 2: 1 Earth rotation per 2 seconds
                if ui.button("⟳ 1d/2s").on_hover_text("1 Earth rotation per 2 seconds").clicked() {
                    time_scale.scale = 43200.0;
                }
                // Speed 3: 1 week in 10 seconds
                if ui.button("🗓 1w/10s").on_hover_text("1 week per 10 seconds").clicked() {
                    time_scale.scale = 60480.0;
                }
                // Speed 4: 1 month in 10 seconds
                if ui.button("📅 1mo/10s").on_hover_text("1 month per 10 seconds").clicked() {
                    time_scale.scale = 259200.0;
                }
                // Speed 5: 1 year in 30 seconds
                if ui.button("🌍 1y/30s").on_hover_text("1 year per 30 seconds").clicked() {
                    time_scale.scale = 1051938.0;
                }

                ui.separator();

                // Slider for fine control with extended range
                ui.label("Time Scale:");
                ui.add(
                    egui::Slider::new(&mut time_scale.scale, 0.0..=5000000.0)
                        .logarithmic(true)
                        .text("x")
                );
            });

            ui.horizontal(|ui| {
                ui.label(format!("Current speed: {:.1}x", time_scale.scale));
                if time_scale.is_paused() {
                    ui.colored_label(egui::Color32::RED, "⏸ PAUSED");
                }
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_scale_creation() {
        let time_scale = TimeScale::new();
        assert_eq!(time_scale.scale, 1.0);
        assert!(!time_scale.is_paused());
    }

    #[test]
    fn test_time_scale_pause() {
        let mut time_scale = TimeScale::new();
        time_scale.pause();
        
        assert!(time_scale.is_paused());
        assert_eq!(time_scale.scale, 0.0);
    }

    #[test]
    fn test_time_scale_resume() {
        let mut time_scale = TimeScale::new();
        time_scale.pause();
        time_scale.resume();
        
        assert!(!time_scale.is_paused());
        assert_eq!(time_scale.scale, 1.0);
    }

    #[test]
    fn test_selection_basics() {
        let selection = Selection::new();
        assert!(!selection.has_selection());
        
        let mut selection = Selection::new();
        let entity = Entity::from_raw(1);
        selection.select(entity);
        
        assert!(selection.has_selection());
        assert_eq!(selection.get(), Some(entity));
    }
}