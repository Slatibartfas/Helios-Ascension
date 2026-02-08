//! UI module for the Helios Ascension interface
//!
//! Provides an egui-based dashboard showing:
//! - Resource stockpiles and critical resources
//! - Power grid status
//! - Selected celestial body information
//! - Time controls for simulation speed

use bevy::prelude::*;
use bevy::time::Real;
use bevy_egui::{egui, EguiContexts};

pub mod interaction;

pub use interaction::Selection;

use crate::astronomy::components::{CurrentStarSystem, SystemId};
use crate::astronomy::nearby_stars::NearbyStarsData;
use crate::astronomy::{AtmosphereComposition, Hovered, KeplerOrbit, Selected, SpaceCoordinates};
use crate::economy::components::SurveyLevel;
use crate::economy::{format_power, GlobalBudget, PlanetResources, ResourceType};
use crate::plugins::camera::{CameraAnchor, GameCamera, ViewMode};
use crate::plugins::solar_system::{CelestialBody, LogicalParent};
use crate::plugins::solar_system_data::BodyType;
use crate::plugins::starmap::{HoveredStarSystem, SelectedStarSystem, StarSystemIcon};

/// Maximum time scale: 1 year per second (365.25 * 86400 ≈ 31,557,600)
const MAX_TIME_SCALE: f32 = 31_557_600.0;

/// Time scale resource for controlling simulation speed
#[derive(Resource, Debug, Clone)]
pub struct TimeScale {
    /// Current time scale multiplier (0.0 = paused, 1.0 = normal, up to 604,800.0)
    pub scale: f32,
    /// Last active scale before pausing, restored on resume
    last_active_scale: f32,
}

impl TimeScale {
    /// Create a new time scale with default value
    pub fn new() -> Self {
        Self {
            scale: 1.0,
            last_active_scale: 1.0,
        }
    }

    /// Pause the simulation
    pub fn pause(&mut self) {
        if self.scale > 0.0 {
            self.last_active_scale = self.scale;
        }
        self.scale = 0.0;
    }

    /// Resume at the speed that was active before pausing
    pub fn resume(&mut self) {
        self.scale = self.last_active_scale;
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

/// Custom simulation clock that tracks game-world elapsed time.
///
/// Unlike Bevy's `Time<Virtual>`, this has **no max-delta cap**, so analytical
/// calculations (Keplerian orbits, body rotation) scale to any speed.
/// Each frame the clock advances by `real_delta × time_scale`.
#[derive(Resource, Debug, Clone)]
pub struct SimulationTime {
    /// Total elapsed simulation time in seconds (f64 for precision)
    pub elapsed: f64,
    /// Starting date as Unix timestamp (January 1, 2026 00:00:00 UTC)
    start_timestamp: i64,
}

impl SimulationTime {
    /// January 1, 2026 00:00:00 UTC as Unix timestamp
    const START_TIMESTAMP: i64 = 1_767_225_600; // Jan 1, 2026 00:00:00 UTC

    pub fn new() -> Self {
        Self {
            elapsed: 0.0,
            start_timestamp: Self::START_TIMESTAMP,
        }
    }

    /// Create a SimulationTime with a custom start date
    ///
    /// For custom game start dates, use this constructor along with
    /// `crate::astronomy::calculate_positions_at_timestamp()` to compute
    /// initial orbital positions for all celestial bodies.
    pub fn with_start_timestamp(start_timestamp: i64) -> Self {
        Self {
            elapsed: 0.0,
            start_timestamp,
        }
    }

    /// Total elapsed simulation seconds
    pub fn elapsed_seconds(&self) -> f64 {
        self.elapsed
    }

    /// Get the current simulation date as Unix timestamp
    pub fn current_timestamp(&self) -> i64 {
        self.start_timestamp + self.elapsed as i64
    }

    /// Format the current date/time as DD.MM.YYYY HH:MM
    pub fn format_date_time(&self) -> String {
        let timestamp = self.current_timestamp();

        // Convert Unix timestamp to date components
        let total_days = timestamp / 86400;
        let time_of_day = timestamp % 86400;

        let hours = (time_of_day / 3600) % 24;
        let minutes = (time_of_day % 3600) / 60;

        // Simplified date calculation starting from Unix epoch (1970-01-01)
        // This is a simplified calculation for display purposes
        let mut days_remaining = total_days;
        let mut year = 1970;

        loop {
            let days_in_year = if is_leap_year(year) { 366 } else { 365 };
            if days_remaining >= days_in_year {
                days_remaining -= days_in_year;
                year += 1;
            } else {
                break;
            }
        }

        let mut month = 1;
        let days_in_months = get_days_in_months(year);

        for &days_in_month in &days_in_months {
            if days_remaining >= days_in_month {
                days_remaining -= days_in_month;
                month += 1;
            } else {
                break;
            }
        }

        let day = days_remaining + 1; // Days are 1-indexed

        format!(
            "{:02}.{:02}.{} {:02}:{:02}",
            day, month, year, hours, minutes
        )
    }
}

/// Check if a year is a leap year
fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

/// Get the number of days in each month for a given year
fn get_days_in_months(year: i64) -> [i64; 12] {
    let feb_days = if is_leap_year(year) { 29 } else { 28 };
    [31, feb_days, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
}

impl Default for SimulationTime {
    fn default() -> Self {
        Self::new()
    }
}

/// Format a time scale multiplier as a human-readable rate string.
/// Examples: "Real time", "2.5 min/s", "1.0 day/s", "1.0 wk/s"
fn format_time_rate(scale: f32) -> String {
    if scale <= 0.0 {
        "Paused".to_string()
    } else if (scale - 1.0).abs() < 0.01 {
        "Real time".to_string()
    } else if scale < 60.0 {
        format!("{:.1}x", scale)
    } else if scale < 3_600.0 {
        format!("{:.1} min/s", scale / 60.0)
    } else if scale < 86_400.0 {
        format!("{:.1} hr/s", scale / 3_600.0)
    } else if scale < 604_800.0 {
        format!("{:.1} day/s", scale / 86_400.0)
    } else if scale < 2_592_000.0 {
        format!("{:.1} wk/s", scale / 604_800.0)
    } else if scale < 31_557_600.0 {
        format!("{:.1} mo/s", scale / 2_592_000.0)
    } else {
        format!("{:.1} yr/s", scale / 31_557_600.0)
    }
}

/// Plugin that adds the UI system to the Bevy app
pub struct UIPlugin;

impl Plugin for UIPlugin {
    fn build(&self, app: &mut App) {
        app
            // Egui plugin is added in `main.rs` (explicit bevy_egui integration)
            // Resources
            .init_resource::<Selection>()
            .init_resource::<TimeScale>()
            .init_resource::<SimulationTime>()
            // Systems
            .add_systems(
                Update,
                (
                    ui_dashboard,
                    ui_hover_tooltip,
                    ui_starmap_hover_tooltip, // New: starmap tooltips
                    ui_starmap_labels,
                    sync_selection_with_astronomy,
                    advance_simulation_time,
                ),
            );
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

/// Advances the custom simulation clock each frame.
///
/// Uses real (wall-clock) delta to avoid Bevy's virtual-time max-delta cap,
/// which previously limited effective speed to ~15×.
fn advance_simulation_time(
    real_time: Res<Time<Real>>,
    time_scale: Res<TimeScale>,
    mut sim_time: ResMut<SimulationTime>,
) {
    let real_delta = real_time.delta_seconds_f64();
    sim_time.elapsed += real_delta * time_scale.scale as f64;
}

/// Render floating labels next to star system icons in starmap view
fn ui_starmap_labels(
    mut contexts: EguiContexts,
    view_mode: Res<ViewMode>,
    camera_query: Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    icon_query: Query<(
        &GlobalTransform,
        &StarSystemIcon,
        Option<&SelectedStarSystem>,
    )>,
) {
    if *view_mode != ViewMode::Starmap {
        return;
    }

    let Ok((camera, camera_transform)) = camera_query.get_single() else {
        return;
    };

    let ctx = contexts.ctx_mut();

    for (icon_transform, icon, is_selected) in icon_query.iter() {
        let icon_pos = icon_transform.translation();

        // Project 3D position to screen space
        if let Some(screen_pos) = camera.world_to_viewport(camera_transform, icon_pos) {
            // Offset label to the right of the icon
            let label_pos = egui::pos2(screen_pos.x + 30.0, screen_pos.y - 10.0);

            egui::Area::new(egui::Id::new(format!("starmap_label_{}", icon.name)))
                .fixed_pos(label_pos)
                .interactable(false)
                .order(egui::Order::Background)
                .show(ctx, |ui| {
                    let color = if is_selected.is_some() {
                        egui::Color32::from_rgb(100, 200, 255) // Bright blue for selected
                    } else {
                        egui::Color32::from_rgb(200, 200, 200) // Light gray for others
                    };

                    ui.colored_label(color, &icon.name);
                });
        }
    }
}

/// Helper function to render a selectable label with highlighting for selected items
fn render_selectable_label(ui: &mut egui::Ui, is_selected: bool, name: &str) -> egui::Response {
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
        if ui
            .small_button("⚓")
            .on_hover_text("Anchor Camera")
            .clicked()
        {
            // Select the body when anchoring
            for e in selected_query.iter() {
                commands.entity(e).remove::<Selected>();
            }
            commands.entity(entity).insert(Selected);
            selection.select(entity);

            // Anchor the camera
            if let Ok(mut anchor) = anchor_query.get_single_mut() {
                anchor.0 = Some(entity);
            }
        }

        // Use a visually distinct style for selected items
        if render_selectable_label(ui, is_selected, &body.name).clicked() {
            for e in selected_query.iter() {
                commands.entity(e).remove::<Selected>();
            }
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
    if children.is_empty() {
        return;
    }

    // Make ID unique by including parent entity to avoid UI jumping bug
    let id = ui.make_persistent_id((group_name, parent_entity));
    egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false)
        .show_header(ui, |ui| {
            ui.label(format!("{} ({})", group_name, children.len()));
        })
        .body(|ui| {
            for &child_entity in children {
                if let Some(body) = body_map.get(&child_entity) {
                    render_body_row(
                        ui,
                        child_entity,
                        body,
                        selection,
                        commands,
                        selected_query,
                        anchor_query,
                    );
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
            egui::collapsing_header::CollapsingState::load_with_default_open(
                ui.ctx(),
                id,
                body.name == "Sol",
            )
            .show_header(ui, |ui| {
                if ui
                    .small_button("⚓")
                    .on_hover_text("Anchor Camera")
                    .clicked()
                {
                    // Select the body when anchoring
                    for e in selected_query.iter() {
                        commands.entity(e).remove::<Selected>();
                    }
                    commands.entity(entity).insert(Selected);
                    selection.select(entity);

                    // Anchor the camera
                    if let Ok(mut anchor) = anchor_query.get_single_mut() {
                        anchor.0 = Some(entity);
                    }
                }

                // Use a visually distinct style for selected items
                if render_selectable_label(ui, is_selected, &body.name).clicked() {
                    for e in selected_query.iter() {
                        commands.entity(e).remove::<Selected>();
                    }
                    commands.entity(entity).insert(Selected);
                    selection.select(entity);
                }
            })
            .body(|ui| {
                // 1. Planets (Recursive)
                for child in child_planets {
                    render_body_tree(
                        ui,
                        child,
                        body_map,
                        hierarchy,
                        selection,
                        commands,
                        selected_query,
                        anchor_query,
                    );
                }
                // 2. Dwarf Planets (Grouped or Recursive if important?) Grouped.
                render_grouped_children(
                    ui,
                    &child_dwarf_planets,
                    "Dwarf Planets",
                    entity,
                    body_map,
                    selection,
                    commands,
                    selected_query,
                    anchor_query,
                );
                // 3. Moons (Usually under planets, but if under Sol/others?)
                render_grouped_children(
                    ui,
                    &child_moons,
                    "Moons",
                    entity,
                    body_map,
                    selection,
                    commands,
                    selected_query,
                    anchor_query,
                );
                // 4. Asteroids
                render_grouped_children(
                    ui,
                    &child_asteroids,
                    "Asteroids",
                    entity,
                    body_map,
                    selection,
                    commands,
                    selected_query,
                    anchor_query,
                );
                // 5. Comets
                render_grouped_children(
                    ui,
                    &child_comets,
                    "Comets",
                    entity,
                    body_map,
                    selection,
                    commands,
                    selected_query,
                    anchor_query,
                );
                // 6. Others
                for child in child_others {
                    render_body_tree(
                        ui,
                        child,
                        body_map,
                        hierarchy,
                        selection,
                        commands,
                        selected_query,
                        anchor_query,
                    );
                }
            });
        } else {
            render_body_row(
                ui,
                entity,
                body,
                selection,
                commands,
                selected_query,
                anchor_query,
            );
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
        // Anchor the tooltip near the mouse pointer so it appears over the 3D view
        let tooltip_pos = ctx
            .input(|i| i.pointer.hover_pos())
            .map(|p| egui::pos2(p.x + 12.0, p.y + 12.0))
            .unwrap_or(egui::pos2(100.0, 100.0));

        egui::Area::new("hover_tooltip".into())
            .fixed_pos(tooltip_pos)
            .interactable(false)
            .order(egui::Order::Tooltip)
            .show(ctx, |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                egui::Frame::none()
                    .fill(egui::Color32::from_rgba_unmultiplied(30, 30, 30, 240))
                    .stroke(egui::Stroke::new(
                        2.0,
                        egui::Color32::from_rgb(100, 180, 255),
                    ))
                    .inner_margin(12.0)
                    .show(ui, |ui| {
                        // Use horizontal layout to prevent narrow wrapping
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(&body.name)
                                    .size(16.0)
                                    .color(egui::Color32::from_rgb(150, 220, 255))
                                    .strong(),
                            );
                        });

                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!("Type: {:?}", body.body_type))
                                    .size(12.0)
                                    .color(egui::Color32::from_rgb(180, 180, 180)),
                            );
                        });
                    });
            });
    }
}

/// Display hover tooltip for star systems in starmap view
fn ui_starmap_hover_tooltip(
    mut contexts: EguiContexts,
    hovered_query: Query<&StarSystemIcon, With<HoveredStarSystem>>,
    bodies_query: Query<(&CelestialBody, &SystemId)>,
    view_mode: Res<ViewMode>,
) {
    // Only show tooltips in starmap view
    if *view_mode != ViewMode::Starmap {
        return;
    }

    let ctx = match contexts.try_ctx_mut() {
        Some(ctx) => ctx,
        None => return,
    };

    // Display hover tooltip if a star system is hovered
    if let Ok(icon) = hovered_query.get_single() {
        // Anchor the tooltip near the mouse pointer
        let tooltip_pos = ctx
            .input(|i| i.pointer.hover_pos())
            .map(|p| egui::pos2(p.x + 12.0, p.y + 12.0))
            .unwrap_or(egui::pos2(100.0, 100.0));

        // Count bodies in this system
        let body_count = bodies_query
            .iter()
            .filter(|(_, sys_id)| sys_id.0 == icon.id)
            .count();

        // Calculate distance from Sol
        let distance_ly = icon.position.length() / 63241.077; // AU to light years

        egui::Area::new(format!("starmap_hover_{}", icon.id).into())
            .fixed_pos(tooltip_pos)
            .interactable(false)
            .order(egui::Order::Tooltip)
            .show(ctx, |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                egui::Frame::none()
                    .fill(egui::Color32::from_rgba_unmultiplied(30, 30, 30, 240))
                    .stroke(egui::Stroke::new(
                        2.0,
                        egui::Color32::from_rgb(255, 180, 100),
                    ))
                    .inner_margin(12.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(&icon.name)
                                    .size(16.0)
                                    .color(egui::Color32::from_rgb(255, 220, 150))
                                    .strong(),
                            );
                        });

                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!("Distance: {:.2} ly", distance_ly))
                                    .size(12.0)
                                    .color(egui::Color32::from_rgb(180, 180, 180)),
                            );
                        });

                        if body_count > 0 {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(format!("Bodies: {}", body_count))
                                        .size(12.0)
                                        .color(egui::Color32::from_rgb(180, 180, 180)),
                                );
                            });
                        }
                    });
            });
    }
}

/// Formats large mass values (in megatons) to user-readable strings with metric prefixes.
/// Supports kt, Mt, Gt, Tt, Pt, Et...
fn format_mass(megatons: f64) -> String {
    let abs_val = megatons.abs();

    // Handle 0
    if abs_val == 0.0 {
        return "0.0 kt".to_string();
    }

    // Smallest unit: kilotons (kt)
    // 1 Mt = 1000 kt
    if abs_val < 1.0 {
        // For very small amounts (e.g. < 0.1 kt), maybe use tons?
        // But user requested "kilotons, megatons and Gigatons".
        return format!("{:.1} kt", megatons * 1000.0);
    }

    // Megatons (Mt)
    if abs_val < 1000.0 {
        return format!("{:.1} Mt", megatons);
    }

    // Gigatons (Gt) - 1 Gt = 1000 Mt
    if abs_val < 1_000_000.0 {
        return format!("{:.1} Gt", megatons / 1000.0);
    }

    // Teratons (Tt) - 1 Tt = 1000 Gt = 1,000,000 Mt
    if abs_val < 1_000_000_000.0 {
        return format!("{:.1} Tt", megatons / 1_000_000.0);
    }

    // Petatons (Pt) - 1 Pt = 1000 Tt = 1,000,000,000 Mt
    if abs_val < 1_000_000_000_000.0 {
        return format!("{:.1} Pt", megatons / 1_000_000_000.0);
    }

    // Exatons (Et) and beyond
    // 1 Et = 1000 Pt = 1,000,000,000,000 Mt
    format!("{:.1} Et", megatons / 1_000_000_000_000.0)
}

/// Main UI dashboard system
#[allow(clippy::too_many_arguments)]
fn ui_dashboard(
    mut commands: Commands,
    mut contexts: EguiContexts,
    budget: Res<GlobalBudget>,
    mut time_scale: ResMut<TimeScale>,
    sim_time: Res<SimulationTime>,
    mut selection: ResMut<Selection>,
    view_mode: Res<ViewMode>,
    current_system: Res<CurrentStarSystem>,
    nearby_stars: Res<NearbyStarsData>,
    // Query for selected body information
    mut body_query: Query<(
        &CelestialBody,
        &SpaceCoordinates,
        Option<&KeplerOrbit>,
        Option<&PlanetResources>,
        Option<&AtmosphereComposition>,
        Option<&mut SurveyLevel>,
    )>,
    // Resource query for system totals
    resource_query: Query<(&SystemId, &PlanetResources)>,
    // Ledger queries
    all_bodies_query: Query<(
        Entity,
        &CelestialBody,
        Option<&LogicalParent>,
        Option<&KeplerOrbit>,
        Option<&SystemId>,
    )>,
    selected_query: Query<Entity, With<Selected>>,
    // Starmap queries
    star_system_query: Query<(Entity, &StarSystemIcon, Option<&SelectedStarSystem>)>,
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
            match *view_mode {
                ViewMode::Starmap => {
                    // Starmap view: show list of star systems
                    ui.heading("Star Systems");
                    ui.separator();

                    egui::ScrollArea::vertical()
                        .id_source("starmap_ledger_scroll")
                        .show(ui, |ui| {
                            for (entity, icon, is_selected) in star_system_query.iter() {
                                let response =
                                    render_selectable_label(ui, is_selected.is_some(), &icon.name);

                                if response.double_clicked() {
                                    // Anchor camera to this system
                                    if let Ok(mut anchor) = anchor_query.get_single_mut() {
                                        anchor.0 = Some(entity);
                                        info!("Anchored to {}", icon.name);
                                    }
                                }
                            }
                        });
                }
                ViewMode::System => {
                    // System view: show celestial body hierarchy
                    ui.heading("Celestial Objects");
                    ui.separator();

                    egui::ScrollArea::vertical()
                        .id_source("ledger_scroll")
                        .show(ui, |ui| {
                            let mut hierarchy: std::collections::HashMap<Entity, Vec<Entity>> =
                                std::collections::HashMap::new();
                            let mut roots: Vec<Entity> = Vec::new();
                            let mut body_map: std::collections::HashMap<Entity, &CelestialBody> =
                                std::collections::HashMap::new();
                            let mut orbit_map: std::collections::HashMap<Entity, f64> =
                                std::collections::HashMap::new();

                            for (entity, body, logical_parent, orbit, system_id) in
                                all_bodies_query.iter()
                            {
                                // Filter by current star system
                                let sys_id = system_id.map(|s| s.0).unwrap_or(0);
                                if sys_id != current_system.0 {
                                    continue;
                                }

                                body_map.insert(entity, body);
                                if let Some(orbit) = orbit {
                                    orbit_map.insert(entity, orbit.semi_major_axis);
                                }

                                if let Some(logical_parent) = logical_parent {
                                    hierarchy.entry(logical_parent.0).or_default().push(entity);
                                } else {
                                    roots.push(entity);
                                }
                            }

                            // Helper closure to sort entities
                            let sort_entities = |entities: &mut Vec<Entity>| {
                                entities.sort_by(|a, b| {
                                    let name_a = &body_map.get(a).unwrap().name;
                                    let name_b = &body_map.get(b).unwrap().name;

                                    // Always keep Sol at the top
                                    if name_a == "Sol" {
                                        return std::cmp::Ordering::Less;
                                    }
                                    if name_b == "Sol" {
                                        return std::cmp::Ordering::Greater;
                                    }

                                    // Sort by orbit distance (semi-major axis)
                                    let dist_a = orbit_map.get(a).unwrap_or(&0.0);
                                    let dist_b = orbit_map.get(b).unwrap_or(&0.0);

                                    match dist_a.partial_cmp(dist_b) {
                                        Some(std::cmp::Ordering::Equal) | None => {
                                            name_a.cmp(name_b)
                                        } // Fallback to name
                                        Some(ord) => ord,
                                    }
                                });
                            };

                            // Sort roots
                            sort_entities(&mut roots);

                            // Sort all children lists in the hierarchy
                            for children in hierarchy.values_mut() {
                                sort_entities(children);
                            }

                            for root in roots {
                                render_body_tree(
                                    ui,
                                    root,
                                    &body_map,
                                    &hierarchy,
                                    &mut selection,
                                    &mut commands,
                                    &selected_query,
                                    &mut anchor_query,
                                );
                            }
                        });
                }
            }
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
                    let category_total: f64 =
                        resources.iter().map(|r| budget.get_stockpile(r)).sum();

                    let category_label = ui.label(format!(
                        "{}: {}",
                        category_name,
                        format_mass(category_total)
                    ));

                    // Show detailed breakdown on hover
                    category_label.on_hover_ui(|ui| {
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new(category_name).strong());
                            ui.separator();
                            for resource in &resources {
                                let amount = budget.get_stockpile(resource);
                                ui.label(format!(
                                    "  {} ({}): {}",
                                    resource.display_name(),
                                    resource.symbol(),
                                    format_mass(amount)
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
                ui.label(format!(
                    "Civilization Score: {:.1}",
                    budget.civilization_score
                ));
                ui.separator();
                ui.label(format!(
                    "Grid Efficiency: {:.1}%",
                    budget.power_efficiency() * 100.0
                ));
            });
        });

    // Right side panel - show either selected star system or selected body
    let selected_star_system = star_system_query
        .iter()
        .find(|(_, _, selected)| selected.is_some());

    if let Some((_star_entity, star_icon, _)) = selected_star_system {
        // Show star system details
        render_star_system_panel(
            ctx,
            star_icon,
            &all_bodies_query,
            &resource_query,
            &nearby_stars,
        );
    } else if selection.has_selection() {
        // Show selected celestial body details
        egui::SidePanel::right("selection_panel")
            .min_width(300.0)
            .max_width(400.0)
            .show(ctx, |ui| {
                ui.heading("Selected Body");
                ui.separator();

                if let Some(entity) = selection.get() {
                    if let Ok((body, coords, orbit, resources, atmosphere, mut survey_level)) = body_query.get_mut(entity) {
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
                                
                                // Survey Controls
                                let current_level = survey_level.as_deref().copied().unwrap_or(SurveyLevel::Unsurveyed);
                                
                                ui.group(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.label("Survey Status:");
                                        let status_color = match current_level {
                                            SurveyLevel::Unsurveyed => egui::Color32::GRAY,
                                            SurveyLevel::OrbitalScan => egui::Color32::LIGHT_BLUE,
                                            SurveyLevel::SeismicSurvey => egui::Color32::YELLOW,
                                            SurveyLevel::CoreSample => egui::Color32::GREEN,
                                        };
                                        ui.label(egui::RichText::new(format!("{:?}", current_level)).strong().color(status_color));
                                    });
                                    
                                    if let Some(survey) = survey_level.as_deref_mut() {
                                        if *survey != SurveyLevel::CoreSample {
                                            if ui.button("Upgrade Survey").clicked() {
                                                *survey = match *survey {
                                                    SurveyLevel::Unsurveyed => SurveyLevel::OrbitalScan,
                                                    SurveyLevel::OrbitalScan => SurveyLevel::SeismicSurvey,
                                                    SurveyLevel::SeismicSurvey => SurveyLevel::CoreSample,
                                                    _ => SurveyLevel::CoreSample,
                                                };
                                            }
                                        }
                                    } else {
                                        if ui.button("Initialize Survey System").clicked() {
                                            commands.entity(entity).insert(SurveyLevel::OrbitalScan);
                                        }
                                    }
                                });
                                
                                ui.add_space(5.0);

                                if current_level != SurveyLevel::Unsurveyed {
                                    egui::ScrollArea::vertical()
                                        .max_height(400.0)
                                        .show(ui, |ui| {
                                            // Group resources by category
                                            for (category_name, category_resources) in ResourceType::by_category() {
                                                ui.label(egui::RichText::new(category_name).strong().color(egui::Color32::LIGHT_BLUE));
                                                
                                                for resource_type in &category_resources {
                                                    if let Some(deposit) = resources.get_deposit(resource_type) {
                                                        // Calculate discovered amount
                                                        let discovered_mt = current_level.discovered_amount(&deposit.reserve);
                                                        
                                                        // Skip if nothing discovered yet (or if very trace)
                                                        if discovered_mt <= 0.0 && !deposit.is_viable() {
                                                             continue;
                                                        }

                                                        ui.horizontal(|ui| {
                                                            ui.label(format!("  {} ({})", 
                                                                resource_type.display_name(),
                                                                resource_type.symbol()
                                                            ));
                                                        });
                                                        
                                                        // Tiered Display
                                                        ui.horizontal(|ui| {
                                                            ui.label("    Total Discovered:");
                                                            ui.label(egui::RichText::new(format_mass(discovered_mt)).strong());
                                                        });
                                                        
                                                        // Proven (Always visible if Orbital+)
                                                        ui.horizontal(|ui| {
                                                            ui.label("    Proven Reserves:");
                                                            ui.add(egui::ProgressBar::new(1.0) // Just a full bar or use ratio?
                                                                .text(format_mass(deposit.reserve.proven_crustal)));
                                                        });
                                                        
                                                        // Deep
                                                        if matches!(current_level, SurveyLevel::SeismicSurvey | SurveyLevel::CoreSample) {
                                                            ui.horizontal(|ui| {
                                                                ui.label("    Deep Deposits:");
                                                                ui.add(egui::ProgressBar::new(1.0)
                                                                    .text(format_mass(deposit.reserve.deep_deposits)));
                                                            });
                                                        } else {
                                                             ui.label("    Deep Deposits: ???");
                                                        }
                                                        
                                                        // Bulk
                                                        if current_level == SurveyLevel::CoreSample {
                                                            ui.horizontal(|ui| {
                                                                ui.label("    Planetary Bulk:");
                                                                 ui.add(egui::ProgressBar::new(1.0)
                                                                    .text(format_mass(deposit.reserve.planetary_bulk)));
                                                            });
                                                        } else {
                                                            ui.label("    Planetary Bulk: ???");
                                                        }
                                                        
                                                        ui.horizontal(|ui| {
                                                            ui.label("    Concentration:");
                                                            ui.add(egui::ProgressBar::new(deposit.reserve.concentration)
                                                                .text(format!("{:.1}%", deposit.reserve.concentration * 100.0)));
                                                        });
                                                        
                                                        ui.add_space(3.0);
                                                    }
                                                }
                                                
                                                ui.add_space(8.0);
                                            }

                                            // Summary
                                            ui.separator();
                                            ui.label(format!("Total viable deposits: {}", resources.viable_count()));
                                            ui.label(format!("Total resource value estimates: {:.2}", resources.total_value()));
                                        });
                                } else {
                                    ui.label("Perform orbital scan to detect resources.");
                                }
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

                // Preset speed buttons with meaningful labels
                if ui.button("1 hr/s").clicked() {
                    time_scale.scale = 3_600.0;
                }
                if ui.button("1 day/s").clicked() {
                    time_scale.scale = 86_400.0;
                }
                if ui.button("1 wk/s").clicked() {
                    time_scale.scale = 604_800.0;
                }
                if ui.button("1 mo/s").clicked() {
                    time_scale.scale = 2_592_000.0;
                }
                if ui.button("1 yr/s").clicked() {
                    time_scale.scale = 31_557_600.0;
                }

                ui.separator();

                // Logarithmic slider for fine control
                ui.label("Speed:");
                ui.add(
                    egui::Slider::new(&mut time_scale.scale, 1.0..=MAX_TIME_SCALE)
                        .logarithmic(true)
                        .text("")
                        .custom_formatter(|v, _| format_time_rate(v as f32)),
                );
            });

            ui.horizontal(|ui| {
                ui.label(format!("Speed: {}", format_time_rate(time_scale.scale)));
                if time_scale.is_paused() {
                    ui.colored_label(egui::Color32::RED, "⏸ PAUSED");
                }
                ui.separator();
                ui.label(format!("Date: {}", sim_time.format_date_time()));
                ui.separator();
                // View mode indicator
                let (view_label, view_color) = match *view_mode {
                    ViewMode::System => ("🔭 System View", egui::Color32::from_rgb(120, 180, 255)),
                    ViewMode::Starmap => {
                        ("🌌 Starmap View", egui::Color32::from_rgb(255, 200, 100))
                    }
                };
                ui.colored_label(view_color, view_label);
            });
        });
}

/// Render detailed information panel for a selected star system
fn render_star_system_panel(
    ctx: &egui::Context,
    star_icon: &StarSystemIcon,
    bodies_query: &Query<(
        Entity,
        &CelestialBody,
        Option<&LogicalParent>,
        Option<&KeplerOrbit>,
        Option<&SystemId>,
    )>,
    resource_query: &Query<(&SystemId, &PlanetResources)>,
    nearby_stars: &Res<NearbyStarsData>,
) {
    egui::SidePanel::right("star_system_panel")
        .min_width(300.0)
        .max_width(400.0)
        .show(ctx, |ui| {
            ui.heading("Selected Star System");
            ui.separator();

            // System name
            ui.label(egui::RichText::new(&star_icon.name).size(18.0).strong());
            ui.add_space(10.0);

            // Distance from Sol
            let distance_ly = star_icon.position.length() / 63241.077;
            ui.group(|ui| {
                ui.label(egui::RichText::new("System Info").strong());
                ui.label(format!("Distance: {:.2} ly", distance_ly));
                ui.label(format!("System ID: {}", star_icon.id));
            });

            ui.add_space(10.0);

            // Try to find detailed system data
            if let Some(system_data) = nearby_stars.get_by_id(star_icon.id) {
                // Star properties
                ui.group(|ui| {
                    ui.label(egui::RichText::new("Star Properties").strong());

                    for (star_idx, star_data) in system_data.stars.iter().enumerate() {
                        if system_data.stars.len() > 1 {
                            ui.label(
                                egui::RichText::new(format!(
                                    "Star {}: {}",
                                    star_idx + 1,
                                    &star_data.name
                                ))
                                .color(egui::Color32::from_rgb(200, 200, 255)),
                            );
                        } else {
                            ui.label(
                                egui::RichText::new(&star_data.name)
                                    .color(egui::Color32::from_rgb(200, 200, 255)),
                            );
                        }

                        ui.label(format!("  Type: {}", star_data.spectral_type));
                        ui.label(format!("  Mass: {:.2} M☉", star_data.mass_sol));
                        ui.label(format!("  Radius: {:.2} R☉", star_data.radius_sol));
                        ui.label(format!("  Luminosity: {:.3} L☉", star_data.luminosity_sol));
                        ui.label(format!("  Temperature: {} K", star_data.temp_k));

                        if let Some(metallicity) = star_data.metallicity {
                            let metallicity_color = if metallicity > 0.0 {
                                egui::Color32::from_rgb(255, 220, 100)
                            } else if metallicity < 0.0 {
                                egui::Color32::from_rgb(150, 150, 200)
                            } else {
                                egui::Color32::from_rgb(200, 200, 200)
                            };

                            ui.label(
                                egui::RichText::new(format!(
                                    "  Metallicity: [Fe/H] = {:.2}",
                                    metallicity
                                ))
                                .color(metallicity_color),
                            );
                        }

                        ui.add_space(5.0);
                    }
                });

                ui.add_space(10.0);
            }

            // Count bodies in this system
            let bodies: Vec<_> = bodies_query
                .iter()
                .filter(|(_, _, _, _, sys_id)| sys_id.map(|s| s.0 == star_icon.id).unwrap_or(false))
                .collect();

            ui.group(|ui| {
                ui.label(egui::RichText::new("System Bodies").strong());
                ui.label(format!("Total bodies: {}", bodies.len()));

                // Count by type
                let stars = bodies
                    .iter()
                    .filter(|(_, b, _, _, _)| matches!(b.body_type, BodyType::Star))
                    .count();
                let planets = bodies
                    .iter()
                    .filter(|(_, b, _, _, _)| matches!(b.body_type, BodyType::Planet))
                    .count();
                let dwarf_planets = bodies
                    .iter()
                    .filter(|(_, b, _, _, _)| matches!(b.body_type, BodyType::DwarfPlanet))
                    .count();
                let moons = bodies
                    .iter()
                    .filter(|(_, b, _, _, _)| matches!(b.body_type, BodyType::Moon))
                    .count();
                let asteroids = bodies
                    .iter()
                    .filter(|(_, b, _, _, _)| matches!(b.body_type, BodyType::Asteroid))
                    .count();
                let comets = bodies
                    .iter()
                    .filter(|(_, b, _, _, _)| matches!(b.body_type, BodyType::Comet))
                    .count();

                if stars > 0 {
                    ui.label(format!("  Stars: {}", stars));
                }
                if planets > 0 {
                    ui.label(format!("  Planets: {}", planets));
                }
                if dwarf_planets > 0 {
                    ui.label(format!("  Dwarf Planets: {}", dwarf_planets));
                }
                if moons > 0 {
                    ui.label(format!("  Moons: {}", moons));
                }
                if asteroids > 0 {
                    ui.label(format!("  Asteroids: {}", asteroids));
                }
                if comets > 0 {
                    ui.label(format!("  Comets: {}", comets));
                }
            });

            ui.add_space(10.0);

            // Calculate total resources
            ui.group(|ui| {
                ui.label(egui::RichText::new("System Resources").strong());

                // Sum up all resources in this system
                let mut total_resources: std::collections::HashMap<ResourceType, f64> =
                    std::collections::HashMap::new();

                for (sys_id, resources) in resource_query.iter() {
                    if sys_id.0 == star_icon.id {
                        for deposit in &resources.deposits {
                            let total = deposit.reserve.total();
                            *total_resources.entry(deposit.resource_type).or_insert(0.0) += total;
                        }
                    }
                }

                if total_resources.is_empty() {
                    ui.label("No surveyed resources yet");
                } else {
                    ui.label(format!(
                        "Surveyed resource types: {}",
                        total_resources.len()
                    ));

                    // Show top 5 resources by abundance
                    let mut sorted_resources: Vec<_> = total_resources.iter().collect();
                    sorted_resources
                        .sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(std::cmp::Ordering::Equal));

                    ui.label(egui::RichText::new("Top resources:").italics());
                    for (resource_type, amount) in sorted_resources.iter().take(5) {
                        ui.label(format!(
                            "  {}: {}",
                            resource_type.display_name(),
                            format_mass(**amount)
                        ));
                    }
                }
            });

            ui.add_space(10.0);

            // Population (placeholder for future)
            ui.group(|ui| {
                ui.label(egui::RichText::new("Population").strong());
                ui.label("Coming soon: Population management");
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
        time_scale.scale = 100.0;
        time_scale.pause();
        time_scale.resume();

        assert!(!time_scale.is_paused());
        assert_eq!(time_scale.scale, 100.0);
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
