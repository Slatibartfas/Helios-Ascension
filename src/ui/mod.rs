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

use crate::astronomy::{KeplerOrbit, Selected, SpaceCoordinates};
use crate::economy::{format_power, GlobalBudget, PlanetResources, ResourceType};
use crate::plugins::solar_system::CelestialBody;

/// Time scale resource for controlling simulation speed
#[derive(Resource, Debug, Clone)]
pub struct TimeScale {
    /// Current time scale multiplier (0.0 = paused, 1.0 = normal, up to 1000.0)
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
    // Only access the mutable resource when time scale has actually changed
    if time_scale.is_changed() && !time_scale.is_added() {
        time.set_relative_speed(time_scale.scale);
    } else if time_scale.is_added() {
        // Always set on first frame to initialize
        time.set_relative_speed(time_scale.scale);
    }
}

/// Main UI dashboard system
fn ui_dashboard(
    mut contexts: EguiContexts,
    budget: Res<GlobalBudget>,
    mut time_scale: ResMut<TimeScale>,
    selection: Res<Selection>,
    // Query for selected body information
    body_query: Query<(&CelestialBody, &SpaceCoordinates, Option<&KeplerOrbit>, Option<&PlanetResources>)>,
) {
    let ctx = contexts.ctx_mut();

    // Top header panel with critical resources and power
    egui::TopBottomPanel::top("header_panel")
        .min_height(60.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Helios: Ascension");
                ui.separator();

                // Show top 5 critical resources
                let critical_resources = [
                    ResourceType::Water,
                    ResourceType::Iron,
                    ResourceType::Helium3,
                    ResourceType::Uranium,
                    ResourceType::NobleMetals,
                ];

                for resource in &critical_resources {
                    let amount = budget.get_stockpile(resource);
                    ui.label(format!("{}: {:.1}", resource.symbol(), amount));
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
                    if let Ok((body, coords, orbit, resources)) = body_query.get(entity) {
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

                        // Resources if available
                        if let Some(resources) = resources {
                            ui.group(|ui| {
                                ui.label(egui::RichText::new("Resources").strong());
                                
                                egui::ScrollArea::vertical()
                                    .max_height(400.0)
                                    .show(ui, |ui| {
                                        // Show all 15 resources
                                        for resource_type in ResourceType::all() {
                                            if let Some(deposit) = resources.get_deposit(resource_type) {
                                                ui.horizontal(|ui| {
                                                    ui.label(format!("{} ({})", 
                                                        resource_type.display_name(),
                                                        resource_type.symbol()
                                                    ));
                                                });
                                                
                                                ui.horizontal(|ui| {
                                                    ui.label("  Abundance:");
                                                    ui.add(egui::ProgressBar::new(deposit.abundance as f32)
                                                        .text(format!("{:.1}%", deposit.abundance * 100.0)));
                                                });
                                                
                                                ui.horizontal(|ui| {
                                                    ui.label("  Access:");
                                                    ui.add(egui::ProgressBar::new(deposit.accessibility)
                                                        .text(format!("{:.1}%", deposit.accessibility * 100.0)));
                                                });
                                                
                                                ui.add_space(5.0);
                                            }
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
                } else {
                    if ui.button("⏸ Pause").clicked() {
                        time_scale.pause();
                    }
                }

                ui.separator();

                // Preset speed buttons
                if ui.button("0.1x").clicked() {
                    time_scale.scale = 0.1;
                }
                if ui.button("1x").clicked() {
                    time_scale.scale = 1.0;
                }
                if ui.button("10x").clicked() {
                    time_scale.scale = 10.0;
                }
                if ui.button("100x").clicked() {
                    time_scale.scale = 100.0;
                }
                if ui.button("1000x").clicked() {
                    time_scale.scale = 1000.0;
                }

                ui.separator();

                // Slider for fine control
                ui.label("Time Scale:");
                ui.add(
                    egui::Slider::new(&mut time_scale.scale, 0.0..=1000.0)
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