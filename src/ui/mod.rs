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
use bevy::asset::AssetServer;
use bevy::asset::Handle;
use bevy::render::texture::Image;
use std::collections::HashMap;

pub mod interaction;

pub use interaction::Selection;

use crate::astronomy::components::{CurrentStarSystem, SystemId};
use crate::astronomy::nearby_stars::NearbyStarsData;
use crate::astronomy::{AtmosphereComposition, Hovered, KeplerOrbit, Selected, SpaceCoordinates};
use crate::colony::{
    BuildingCategory, BuildingType, BuildingsData, Colony, ConstructionDebugSettings,
    ConstructionProject, PendingConstructionActions,
};
use crate::colony::data::can_afford_resources;
use crate::economy::components::{MineralDeposit, Population, SurveyLevel};
use crate::economy::{
    format_currency, format_power, GlobalBudget, MiningOperation, PlanetResources,
    PowerSourceType, ResourceRateTracker, ResourceType,
};
use crate::game_state::{ActiveMenu, GameMenu};
use crate::plugins::camera::{CameraAnchor, GameCamera, ViewMode};
use crate::plugins::solar_system::{CelestialBody, LogicalParent};
use crate::plugins::solar_system_data::BodyType;
use crate::plugins::starmap::{HoveredStarSystem, SelectedStarSystem, StarSystemIcon};
use crate::research::{
    EngineeringProject, ResearchProject, ResearchState, ResearchTeam, ResearchTeamCapacity,
    TechnologiesData, TechCategory, TechTreeEditState, TechEditData, ContextMenuState,
};

/// Maximum time scale: 1 year per second (365.25 * 86400 ≈ 31,557,600)
const MAX_TIME_SCALE: f32 = 31_557_600.0;

#[derive(Resource, Debug, Clone)]
pub struct ResearchUiPreferences {
    pub show_inactive_warning: bool,
}

impl Default for ResearchUiPreferences {
    fn default() -> Self {
        Self {
            show_inactive_warning: true,
        }
    }
}

/// Loaded textures for the top menu icons
#[derive(Resource)]
pub struct MenuIcons {
    pub handles: HashMap<GameMenu, Handle<Image>>,
    /// Menus that have already been post-processed (white -> transparent)
    pub processed: std::collections::HashSet<GameMenu>,
}

impl Default for MenuIcons {
    fn default() -> Self {
        Self { handles: HashMap::new(), processed: Default::default() }
    }
}

/// Startup system to load menu icon images from assets/textures/ui/menu/
fn load_menu_icons(mut commands: Commands, asset_server: Res<AssetServer>) {
    let mut map = HashMap::new();
    for &menu in GameMenu::all() {
        // File names follow the game's convention, e.g. "main.png", "starmap.png"
        let filename = format!("textures/ui/menu/{}.png", menu.asset_basename());
        let handle: Handle<Image> = asset_server.load(&filename);
        map.insert(menu, handle);
    }
    commands.insert_resource(MenuIcons { handles: map, processed: Default::default() });
}

/// Post-process loaded icon images:
/// 1. Calculate alpha from luminance (inverted) to remove white background
/// 2. Set all RGB pixels to WHITE so they can be tinted at runtime
fn process_menu_icons(mut menu_icons: ResMut<MenuIcons>, mut images: ResMut<Assets<Image>>) {
    // Collect handles to process to avoid mutable/immutable borrow conflicts
    let to_process: Vec<(GameMenu, Handle<Image>)> = menu_icons
        .handles
        .iter()
        .filter(|(menu, _)| !menu_icons.processed.contains(menu))
        .map(|(m, h)| (*m, h.clone()))
        .collect();

    for (menu, handle) in to_process {
        if let Some(image) = images.get_mut(&handle) {
            // Only handle 4-byte-per-pixel formats (assume RGBA8)
            let bytes_per_pixel = 4usize;
            if image.data.len() != (image.texture_descriptor.size.width as usize)
                .saturating_mul(image.texture_descriptor.size.height as usize)
                .saturating_mul(bytes_per_pixel)
            {
                // Unsupported format, mark processed to avoid retrying
                menu_icons.processed.insert(menu);
                continue;
            }

            // Iterate all pixels
            // Assumption: Input is Dark lines on White background
            // Goal: White/Theme lines on Transparent background
            for chunk in image.data.chunks_exact_mut(bytes_per_pixel) {
                let r = chunk[0] as f32 / 255.0;
                let g = chunk[1] as f32 / 255.0;
                let b = chunk[2] as f32 / 255.0;

                // Calculate luminance (perceptual)
                // White (1.0) -> Luminance 1.0 -> Alpha 0.0
                // Black (0.0) -> Luminance 0.0 -> Alpha 1.0
                let luminance = 0.299 * r + 0.587 * g + 0.114 * b;
                
                // Contrast stretch: make light grays fully transparent
                // Input range 0.0 .. 1.0
                // We want > 0.9 to be 0 alpha
                // We want < 0.5 to be 1 alpha (or close)
                let alpha = (1.0 - luminance).powf(3.0); // Power curve to steepen the falloff
                
                // Set pixel colour to pure white so it can be tinted by the UI
                chunk[0] = 255;
                chunk[1] = 255;
                chunk[2] = 255;
                chunk[3] = (alpha.clamp(0.0, 1.0) * 255.0) as u8;
            }

            // Mark as processed so we only do this once per asset
            menu_icons.processed.insert(menu);
        }
    }
}

/// Loaded textures for research category icons
#[derive(Resource)]
pub struct ResearchIcons {
    pub handles: HashMap<TechCategory, Handle<Image>>,
    /// Icons that have already been post-processed
    pub processed: std::collections::HashSet<TechCategory>,
}

impl Default for ResearchIcons {
    fn default() -> Self {
        Self { handles: HashMap::new(), processed: Default::default() }
    }
}

/// Startup system to load research icons from assets/textures/ui/research/
fn load_research_icons(mut commands: Commands, asset_server: Res<AssetServer>) {
    let mut map = HashMap::new();
    for &category in TechCategory::all() {
        let name = match category {
            TechCategory::Electronics => "electronics",
            TechCategory::Military => "military",
            TechCategory::SpaceTechnology => "space_technology",
            TechCategory::Biology => "biology",
            TechCategory::Physics => "physics",
            TechCategory::Energy => "energy",
            TechCategory::Sociology => "sociology",
            TechCategory::Construction => "construction",
            TechCategory::Propulsion => "propulsion",
            TechCategory::Materials => "materials",
            TechCategory::Sensors => "sensors",
            TechCategory::Weapons => "weapons",
            TechCategory::DefensiveSystems => "defensive_systems",
            TechCategory::LifeSupport => "life_support",
            TechCategory::Industry => "industry",
        };
        // Expected path: assets/textures/ui/research/{category}.png
        let filename = format!("textures/ui/research/{}.png", name);
        let handle: Handle<Image> = asset_server.load(&filename);
        map.insert(category, handle);
    }
    commands.insert_resource(ResearchIcons { handles: map, processed: Default::default() });
}

/// Post-process loaded research icon images (same as menu icons)
fn process_research_icons(mut icons: ResMut<ResearchIcons>, mut images: ResMut<Assets<Image>>) {
    // Collect handles to process
    let to_process: Vec<(TechCategory, Handle<Image>)> = icons
        .handles
        .iter()
        .filter(|(cat, _)| !icons.processed.contains(cat))
        .map(|(c, h)| (*c, h.clone()))
        .collect();

    for (category, handle) in to_process {
        if let Some(image) = images.get_mut(&handle) {
            let bytes_per_pixel = 4usize;
            if image.data.len() != (image.texture_descriptor.size.width as usize)
                .saturating_mul(image.texture_descriptor.size.height as usize)
                .saturating_mul(bytes_per_pixel)
            {
                icons.processed.insert(category);
                continue;
            }

            for chunk in image.data.chunks_exact_mut(bytes_per_pixel) {
                let r = chunk[0] as f32 / 255.0;
                let g = chunk[1] as f32 / 255.0;
                let b = chunk[2] as f32 / 255.0;
                let luminance = 0.299 * r + 0.587 * g + 0.114 * b;
                let alpha = (1.0 - luminance).powf(3.0);
                
                chunk[0] = 255;
                chunk[1] = 255;
                chunk[2] = 255;
                chunk[3] = (alpha.clamp(0.0, 1.0) * 255.0) as u8;
            }

            icons.processed.insert(category);
        }
    }
}

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
            .init_resource::<ResearchUiPreferences>()
            // ActiveMenu is now initialized in GameStatePlugin
            // to allow access in camera/starmap plugins
            // Load menu icons at startup
            .add_systems(Startup, (load_menu_icons, load_research_icons))
            // UI rendering systems
            // Ordered sequence to ensure correct layout stacking:
            // 1. Top bars (Resources -> Menu)
            // 2. Main content panels (Dashboard / Research)
            // 3. Floating overlays (Tooltips)
            .add_systems(
                Update,
                (
                    ui_resources_bar,
                    ui_top_menu_bar,
                    (ui_dashboard, ui_research_panels, ui_construction_panels, ui_economy_panels),
                    (
                        ui_hover_tooltip,
                        ui_starmap_hover_tooltip,
                        ui_starmap_labels,
                    ),
                )
                    .chain(),
            )
            // UI utility systems
            .add_systems(
                Update,
                (
                    sync_selection_with_astronomy,
                    sync_active_menu_with_view_mode,
                    advance_simulation_time,
                    process_menu_icons,
                    process_research_icons,
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

/// Keeps `ActiveMenu` in sync when `ViewMode` changes via camera zoom
/// (as opposed to clicking a menu button which handles its own sync).
///
/// - `ViewMode::Starmap` → `GameMenu::Starmap`
/// - `ViewMode::System` → `GameMenu::Survey` (unless already on a system-compatible menu)
fn sync_active_menu_with_view_mode(
    view_mode: Res<ViewMode>,
    mut active_menu: ResMut<ActiveMenu>,
) {
    if !view_mode.is_changed() {
        return;
    }

    match *view_mode {
        ViewMode::Starmap => {
            if active_menu.current != GameMenu::Starmap {
                active_menu.current = GameMenu::Starmap;
            }
        }
        ViewMode::System => {
            // When entering System view and the menu is still showing
            // the Starmap ledger, switch to Survey for the body list.
            if active_menu.current == GameMenu::Starmap {
                active_menu.current = GameMenu::Survey;
            }
        }
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

/// Get the icon for a resource category
fn get_resource_category_icon(category: &str) -> &'static str {
    match category {
        "Volatiles" => "💧",
        "Atmospheric Gases" => "☁",
        "Construction" => "🧱",  // Brick instead of crane to differentiate from Construction menu
        "Fusion Fuel" => "🔋",   // Battery/Energy
        "Fissiles" => "☢",
        "Precious Metals" => "💎",
        "Specialty" => "✨",
        _ => "📦",
    }
}

/// Get the icon for a specific resource type
fn get_resource_icon(resource: &ResourceType) -> &'static str {
    match resource {
        // Volatiles
        ResourceType::Water => "💧",
        ResourceType::Hydrogen => "🎈", // Or ⛽
        ResourceType::Ammonia => "🧼",  // Cleaning/Chemical
        ResourceType::Methane => "🔥",
        
        // Atmospheric
        ResourceType::Nitrogen => "🌬", // Wind/Air
        ResourceType::Oxygen => "💨",   // Air
        ResourceType::CarbonDioxide => "🌫", // Gray fog
        ResourceType::Argon => "🟣",    // Noble gas color
        
        // Construction
        ResourceType::Iron => "🔩",     // Metal part
        ResourceType::Aluminum => "✈",  // Lightweight
        ResourceType::Titanium => "🛡", // Shield/Durability
        ResourceType::Silicates => "🪨", // Rock
        
        // Energy
        ResourceType::Helium3 => "☀",   // Fusion/Star
        
        // Fissiles
        ResourceType::Uranium => "☢",
        ResourceType::Thorium => "⚡",

        // Precious
        ResourceType::Gold => "👑",
        ResourceType::Silver => "🥈",
        ResourceType::Platinum => "💍",

        // Specialty
        ResourceType::Copper => "🔌",
        ResourceType::RareEarths => "📱",
    }
}

/// Get color for resource category
fn get_category_color(category: &str) -> egui::Color32 {
    match category {
        "Volatiles" => egui::Color32::from_rgb(100, 200, 255),       // Water Blue
        "Atmospheric Gases" => egui::Color32::from_rgb(200, 230, 255), // Air White/Blue
        "Construction" => egui::Color32::from_rgb(205, 127, 50),     // Bronze/Rust property
        "Fusion Fuel" => egui::Color32::from_rgb(255, 100, 200),     // Plasma/Energy Pink
        "Fissiles" => egui::Color32::from_rgb(100, 255, 100),        // Radioactive Green
        "Precious Metals" => egui::Color32::from_rgb(255, 215, 0),   // Gold
        "Specialty" => egui::Color32::from_rgb(200, 100, 255),       // Exotic Purple
        _ => egui::Color32::LIGHT_GRAY,
    }
}

/// Resource popup that is currently open (if any)
#[derive(Resource, Default)]
struct OpenResourcePopup {
    /// Which category is open, and where to anchor the popup
    open: Option<(String, egui::Rect)>,
}

/// Render the resources bar at the top of the screen (above the menu)
fn ui_resources_bar(
    mut contexts: EguiContexts,
    budget: Res<GlobalBudget>,
    rate_tracker: Res<ResourceRateTracker>,
    research_state: Res<ResearchState>,
    population_query: Query<(&Population, Option<&crate::plugins::solar_system::CelestialBody>)>,
    mut open_popup: Local<OpenResourcePopup>,
    research_projects: Query<&ResearchProject>,
    engineering_projects: Query<&EngineeringProject>,
    technologies: Res<TechnologiesData>,
    time: Res<Time<Real>>,
    ui_prefs: Res<ResearchUiPreferences>,
) {
    let ctx = match contexts.try_ctx_mut() {
        Some(ctx) => ctx,
        None => return,
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
                    let category_rate: f64 =
                        resources.iter().map(|r| rate_tracker.get_resource_rate(r)).sum();

                    let icon = get_resource_category_icon(category_name);
                    let color = get_category_color(category_name);
                    let text_color = egui::Color32::from_rgb(220, 220, 220);

                    let is_this_open = open_popup.open.as_ref().map_or(false, |(n, _)| n == category_name);

                    // Use a Frame for the category display
                    let response = egui::Frame::none()
                        .inner_margin(egui::Margin::symmetric(5.0, 2.0))
                        .show(ui, |ui| {
                            ui.horizontal_centered(|ui| {
                                ui.add(egui::Label::new(egui::RichText::new(icon).size(20.0).color(color)).selectable(false));
                                ui.vertical(|ui| {
                                    ui.add(egui::Label::new(egui::RichText::new(format_mass(category_total)).size(14.0).color(text_color)).selectable(false));
                                    let (rate_text, rate_color) = format_rate_monthly(category_rate);
                                    ui.add(egui::Label::new(egui::RichText::new(rate_text).size(10.0).color(rate_color)).selectable(false));
                                });
                            });
                        }).response;

                    let interact = response.interact(egui::Sense::click());

                    // Hover and open-state border effect
                    if interact.hovered() || is_this_open {
                        ui.painter().rect_stroke(interact.rect, 2.0, egui::Stroke::new(1.0, color));
                        interact.clone().on_hover_cursor(egui::CursorIcon::PointingHand);
                    }

                    // Toggle popup on click
                    if interact.clicked() {
                        if is_this_open {
                            open_popup.open = None;
                        } else {
                            open_popup.open = Some((category_name.to_string(), interact.rect));
                        }
                    }

                    ui.add_space(15.0);
                }

                // Research Points display
                {
                    let rp_color = egui::Color32::from_rgb(100, 200, 255);
                    let text_color = egui::Color32::from_rgb(220, 220, 220);
                    let warning_color = egui::Color32::from_rgb(255, 50, 50);
                    let is_rp_open = open_popup.open.as_ref().map_or(false, |(n, _)| n == "ResearchPoints");

                    // Find active research projects
                    let mut active_rps: Vec<_> = research_projects.iter().filter(|p| p.active).collect();
                    active_rps.sort_by(|a, b| (b.progress / b.required_points).partial_cmp(&(a.progress / a.required_points)).unwrap_or(std::cmp::Ordering::Equal));
                    
                    let furthest_rp = active_rps.first();
                    let has_active_rp = !active_rps.is_empty();
                    
                    // Warning flash
                    let flash = if !has_active_rp && ui_prefs.show_inactive_warning {
                        (time.elapsed_seconds() * 5.0).sin().abs() as f32
                    } else {
                        0.0
                    };
                    
                    let border_color = if flash > 0.5 { warning_color } else { egui::Color32::TRANSPARENT };

                    let response = egui::Frame::none()
                        .inner_margin(egui::Margin::symmetric(5.0, 2.0))
                        .stroke(egui::Stroke::new(if flash > 0.0 { 2.0 } else { 0.0 }, border_color))
                        .show(ui, |ui| {
                            ui.horizontal_centered(|ui| {
                                ui.add(egui::Label::new(egui::RichText::new("🔬").size(20.0).color(rp_color)).selectable(false));
                                ui.vertical(|ui| {
                                    ui.set_min_width(90.0);
                                    // Rate per month (Primary)
                                    let (rp_rate_text, rp_rate_color) = format_points_rate_monthly(rate_tracker.research_rate_per_month);
                                    ui.add(egui::Label::new(egui::RichText::new(rp_rate_text).size(14.0).color(rp_rate_color)).selectable(false));
                                    
                                    // Active Project or Warning
                                    if let Some(project) = furthest_rp {
                                        if let Some(tech) = technologies.technologies.get(&project.tech_id) {
                                            ui.add(egui::Label::new(egui::RichText::new(&tech.name).size(10.0).color(text_color)).selectable(false));
                                            
                                            // Blue Progress Bar
                                            let progress_fraction = (project.progress / project.required_points).clamp(0.0, 1.0) as f32;
                                            ui.add(egui::ProgressBar::new(progress_fraction)
                                                .desired_width(80.0)
                                                .desired_height(4.0)
                                                .fill(egui::Color32::from_rgb(50, 150, 255))
                                                .show_percentage());
                                        } else {
                                            ui.add(egui::Label::new(egui::RichText::new("Unknown Project").size(10.0).color(text_color)).selectable(false));
                                        }
                                    } else {
                                         let warning_text = if !has_active_rp { "No Active Research!" } else { "Idle" };
                                         let warning_text_color = if flash > 0.5 { warning_color } else { text_color };
                                         ui.add(egui::Label::new(egui::RichText::new(warning_text).size(10.0).color(warning_text_color)).selectable(false));
                                    }
                                });
                            });
                        }).response;

                    let interact = response.interact(egui::Sense::click());
                    if interact.hovered() || is_rp_open {
                        ui.painter().rect_stroke(interact.rect, 2.0, egui::Stroke::new(1.0, rp_color));
                        interact.clone().on_hover_cursor(egui::CursorIcon::PointingHand);
                    }

                    if interact.clicked() {
                        if is_rp_open {
                            open_popup.open = None;
                        } else {
                            open_popup.open = Some(("ResearchPoints".to_string(), interact.rect));
                        }
                    }
                    ui.add_space(8.0);
                }

                // Engineering Points display
                {
                    let ep_color = egui::Color32::from_rgb(100, 255, 200);
                    let text_color = egui::Color32::from_rgb(220, 220, 220);
                    let warning_color = egui::Color32::from_rgb(255, 50, 50);
                    let is_ep_open = open_popup.open.as_ref().map_or(false, |(n, _)| n == "EngineeringPoints");

                    // Find active engineering projects
                    let mut active_eps: Vec<_> = engineering_projects.iter().collect();
                    active_eps.sort_by(|a, b| (b.progress / b.required_points).partial_cmp(&(a.progress / a.required_points)).unwrap_or(std::cmp::Ordering::Equal));
                    
                    let furthest_ep = active_eps.first();
                    let has_active_ep = !active_eps.is_empty();

                    // Warning flash
                    let flash = if !has_active_ep && ui_prefs.show_inactive_warning {
                        (time.elapsed_seconds() * 5.0).sin().abs() as f32
                    } else {
                        0.0
                    };
                    
                    let border_color = if flash > 0.5 { warning_color } else { egui::Color32::TRANSPARENT };

                    let response = egui::Frame::none()
                        .inner_margin(egui::Margin::symmetric(5.0, 2.0))
                        .stroke(egui::Stroke::new(if flash > 0.0 { 2.0 } else { 0.0 }, border_color))
                        .show(ui, |ui| {
                            ui.horizontal_centered(|ui| {
                                ui.add(egui::Label::new(egui::RichText::new("⚙").size(20.0).color(ep_color)).selectable(false));
                                ui.vertical(|ui| {
                                    ui.set_min_width(90.0);
                                    // Rate per month (Primary)
                                    // Calculate rate manually or use tracker? Tracker has it.
                                    let (ep_rate_text, ep_rate_color) = format_points_rate_monthly(rate_tracker.engineering_rate_per_month);
                                    ui.add(egui::Label::new(egui::RichText::new(ep_rate_text).size(14.0).color(ep_rate_color)).selectable(false));
                                    
                                     // Active Project or Warning
                                    if let Some(project) = furthest_ep {
                                        let name = technologies.components.get(&project.component_id).map(|c| c.name.as_str()).unwrap_or("Unknown Component");
                                        ui.add(egui::Label::new(egui::RichText::new(name).size(10.0).color(text_color)).selectable(false));
                                        
                                        // Blue Progress Bar
                                        let progress_fraction = (project.progress / project.required_points).clamp(0.0, 1.0) as f32;
                                        ui.add(egui::ProgressBar::new(progress_fraction)
                                            .desired_width(80.0)
                                            .desired_height(4.0)
                                            .fill(egui::Color32::from_rgb(50, 150, 255))
                                            .show_percentage());

                                    } else {
                                         let warning_text = if !has_active_ep { "No Active Eng.!" } else { "Idle" };
                                         let warning_text_color = if flash > 0.5 { warning_color } else { text_color };
                                         ui.add(egui::Label::new(egui::RichText::new(warning_text).size(10.0).color(warning_text_color)).selectable(false));
                                    }
                                });
                            });
                        }).response;

                    let interact = response.interact(egui::Sense::click());
                    if interact.hovered() || is_ep_open {
                        ui.painter().rect_stroke(interact.rect, 2.0, egui::Stroke::new(1.0, ep_color));
                        interact.clone().on_hover_cursor(egui::CursorIcon::PointingHand);
                    }
                    


                    if interact.clicked() {
                        if is_ep_open {
                            open_popup.open = None;
                        } else {
                            open_popup.open = Some(("EngineeringPoints".to_string(), interact.rect));
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
                    
                    ui.add(egui::Label::new(egui::RichText::new(format!(
                        "Type {:.3}",
                        kardashev.max(0.0)
                    )).size(14.0).color(egui::Color32::from_rgb(200, 100, 255))).selectable(false));
                    
                    ui.add(egui::Label::new(egui::RichText::new("Kardashev:").size(14.0).color(egui::Color32::LIGHT_GRAY)).selectable(false));

                    ui.separator();

                    // Power grid status
                    // Color code power: Green if surplus, Red if deficit
                    let net_power = budget.net_power();
                    let power_color = if net_power >= 0.0 {
                        egui::Color32::GREEN
                    } else {
                        egui::Color32::RED
                    };

                    let is_power_open = open_popup
                        .open
                        .as_ref()
                        .map_or(false, |(n, _)| n == "Power");

                    // Power generation display (clickable with tooltip)
                    let response = egui::Frame::none()
                        .inner_margin(egui::Margin::symmetric(5.0, 2.0))
                        .show(ui, |ui| {
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
                        })
                        .response;

                    let interact = response.interact(egui::Sense::click());

                    if interact.hovered() || is_power_open {
                        ui.painter()
                            .rect_stroke(interact.rect, 2.0, egui::Stroke::new(1.0, power_color));
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
                        egui::Color32::from_rgb(255, 215, 0) // Gold
                    } else {
                        egui::Color32::RED
                    };

                    let is_treasury_open = open_popup
                        .open
                        .as_ref()
                        .map_or(false, |(n, _)| n == "Treasury");

                    let treasury_response = egui::Frame::none()
                        .inner_margin(egui::Margin::symmetric(5.0, 2.0))
                        .show(ui, |ui| {
                            ui.horizontal_centered(|ui| {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new("💰")
                                            .size(20.0)
                                            .color(treasury_color),
                                    )
                                    .selectable(false),
                                );
                                ui.scope(|ui| {
                                    // Constrain width to prevent layout issues in right-to-left container
                                    ui.set_max_width(150.0);
                                    ui.vertical(|ui| {
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(format_currency(budget.treasury))
                                                    .size(14.0)
                                                    .strong()
                                                    .color(treasury_color),
                                            )
                                            .selectable(false),
                                        );
                                        let balance_sign = if balance >= 0.0 { "+" } else { "" };
                                        let balance_text = format!("{}{}/yr", balance_sign, format_currency(balance));
                                        let balance_color = if balance >= 0.0 {
                                            egui::Color32::GREEN
                                        } else {
                                            egui::Color32::RED
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
                        .map_or(false, |(n, _)| n == "Population");

                    // Use a Frame for the population display
                    let pop_response = egui::Frame::none()
                        .inner_margin(egui::Margin::symmetric(5.0, 2.0))
                        .show(ui, |ui| {
                            ui.horizontal_centered(|ui| {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(format_population(total_population))
                                            .size(16.0),
                                    )
                                    .selectable(false),
                                );
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new("👥")
                                            .size(20.0)
                                            .color(egui::Color32::WHITE),
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
                            egui::Stroke::new(1.0, egui::Color32::WHITE),
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
                egui::Color32::GREEN
            } else {
                egui::Color32::RED
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
                        ui.add(egui::Label::new(egui::RichText::new("⚡").size(18.0).color(power_color)).selectable(false));
                        ui.add(egui::Label::new(egui::RichText::new("Power Production").size(16.0).strong().color(power_color)).selectable(false));
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
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.add(egui::Label::new(egui::RichText::new(format_power(amount)).strong()).selectable(false));
                                });
                            });
                        }
                    }

                    if !has_sources {
                        ui.add(egui::Label::new("No active power generation").selectable(false));
                    }

                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new(egui::RichText::new("Total").strong()).selectable(false));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add(egui::Label::new(egui::RichText::new(format_power(budget.energy_grid.produced)).strong().color(power_color)).selectable(false));
                        });
                    });
                });

            // Close if clicked outside
            if let Some(inner_response) = window_response {
                if ctx.input(|i| i.pointer.any_pressed()) {
                    if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                        if !inner_response.response.rect.contains(pos) && !anchor_rect.contains(pos) {
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
                egui::Color32::GREEN
            } else {
                egui::Color32::RED
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
                        ui.add(egui::Label::new(egui::RichText::new("💰").size(18.0).color(egui::Color32::from_rgb(255, 215, 0))).selectable(false));
                        ui.add(egui::Label::new(egui::RichText::new("Financial Overview").size(16.0).strong()).selectable(false));
                    });
                    ui.separator();

                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new("Treasury:").selectable(false));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add(egui::Label::new(egui::RichText::new(format_currency(budget.treasury)).strong()).selectable(false));
                        });
                    });

                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new("Income/yr:").selectable(false));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add(egui::Label::new(egui::RichText::new(format_currency(budget.income_per_year)).color(egui::Color32::GREEN)).selectable(false));
                        });
                    });

                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new("Expenses/yr:").selectable(false));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add(egui::Label::new(egui::RichText::new(format_currency(budget.expenses_per_year)).color(egui::Color32::RED)).selectable(false));
                        });
                    });

                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new(egui::RichText::new("Balance/yr:").strong()).selectable(false));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add(egui::Label::new(egui::RichText::new(format_currency(balance)).strong().color(balance_color)).selectable(false));
                        });
                    });
                });

            if let Some(inner_response) = window_response {
                if ctx.input(|i| i.pointer.any_pressed()) {
                    if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                        if !inner_response.response.rect.contains(pos) && !anchor_rect.contains(pos) {
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
                         ui.add(egui::Label::new(egui::RichText::new("🔬").size(18.0).color(egui::Color32::from_rgb(100, 200, 255))).selectable(false));
                         ui.add(egui::Label::new(egui::RichText::new("Active Research Projects").size(16.0).strong()).selectable(false));
                    });
                    ui.separator();
                    
                    let active_rps: Vec<_> = research_projects.iter().filter(|p| p.active).collect();
                    if active_rps.is_empty() {
                         ui.label("No active research projects.");
                    } else {
                        for project in &active_rps {
                            if let Some(tech) = technologies.technologies.get(&project.tech_id) {
                                ui.horizontal(|ui| {
                                    ui.label(&tech.name);
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        let progress = (project.progress / project.required_points * 100.0) as u32;
                                        ui.label(format!("{}%", progress));
                                    });
                                });
                            }
                        }
                    }
                    ui.separator();
                    ui.label(format!("Available: {:.0} RP", research_state.research_points_available));
                });

            if let Some(inner_response) = window_response {
                if ctx.input(|i| i.pointer.any_pressed()) {
                    if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                         if !inner_response.response.rect.contains(pos) && !anchor_rect.contains(pos) {
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
                         ui.add(egui::Label::new(egui::RichText::new("⚙").size(18.0).color(egui::Color32::from_rgb(100, 255, 200))).selectable(false));
                         ui.add(egui::Label::new(egui::RichText::new("Active Engineering Projects").size(16.0).strong()).selectable(false));
                    });
                    ui.separator();
                    
                    let active_eps: Vec<_> = engineering_projects.iter().collect();
                    if active_eps.is_empty() {
                         ui.label("No active engineering projects.");
                    } else {
                        for project in &active_eps {
                            let name = technologies.components.get(&project.component_id).map(|c| c.name.as_str()).unwrap_or("Unknown Component");
                            ui.horizontal(|ui| {
                                ui.label(name);
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let progress = (project.progress / project.required_points * 100.0) as u32;
                                    ui.label(format!("{}%", progress));
                                });
                            });
                        }
                    }
                    ui.separator();
                    ui.label(format!("Available: {:.0} EP", research_state.engineering_points_available));
                });

            if let Some(inner_response) = window_response {
                if ctx.input(|i| i.pointer.any_pressed()) {
                    if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                         if !inner_response.response.rect.contains(pos) && !anchor_rect.contains(pos) {
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
                        ui.add(egui::Label::new(egui::RichText::new("👥").size(18.0).color(egui::Color32::WHITE)).selectable(false));
                        ui.add(egui::Label::new(egui::RichText::new("Population").size(16.0).strong().color(egui::Color32::WHITE)).selectable(false));
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
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.add(egui::Label::new(egui::RichText::new(format_population(*count)).strong()).selectable(false));
                            });
                        });
                    }

                    // Summarize the rest
                    if pops.len() > 10 {
                        let other_total: f64 = pops.iter().skip(10).map(|(_, c)| c).sum();
                        ui.horizontal(|ui| {
                            ui.add(egui::Label::new(egui::RichText::new("Other").italics()).selectable(false));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.add(egui::Label::new(egui::RichText::new(format_population(other_total)).italics()).selectable(false));
                            });
                        });
                    }

                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new(egui::RichText::new("Total").strong()).selectable(false));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add(egui::Label::new(egui::RichText::new(format_population(total_population)).strong().color(egui::Color32::WHITE)).selectable(false));
                        });
                    });
                });

            // Close if clicked outside
            if let Some(inner_response) = window_response {
                if ctx.input(|i| i.pointer.any_pressed()) {
                    if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                        if !inner_response.response.rect.contains(pos) && !anchor_rect.contains(pos) {
                            open_popup.open = None;
                        }
                    }
                }
            }

            if !still_open {
                open_popup.open = None;
            }
        } else if cat_name == "ResearchPoints" {
            let rp_color = egui::Color32::from_rgb(100, 200, 255);
            let mut still_open = true;
            let window_response = egui::Window::new("Research Points")
                .id(egui::Id::new("research_points_window"))
                .fixed_pos(egui::pos2(anchor_rect.left(), anchor_rect.bottom() + 2.0))
                .collapsible(false)
                .resizable(false)
                .title_bar(false)
                .open(&mut still_open)
                .frame(egui::Frame::popup(ctx.style().as_ref()))
                .show(ctx, |ui| {
                    ui.set_min_width(220.0);
                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new(egui::RichText::new("🔬").size(18.0).color(rp_color)).selectable(false));
                        ui.add(egui::Label::new(egui::RichText::new("Research Points").size(16.0).strong().color(rp_color)).selectable(false));
                    });
                    ui.separator();

                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new("Available:").selectable(false));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add(egui::Label::new(egui::RichText::new(format!("{:.0}", research_state.research_points_available)).strong()).selectable(false));
                        });
                    });
                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new("Monthly Income:").selectable(false));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let (rt, rc) = format_points_rate_monthly(rate_tracker.research_rate_per_month);
                            ui.add(egui::Label::new(egui::RichText::new(rt).strong().color(rc)).selectable(false));
                        });
                    });
                });

            if let Some(inner_response) = window_response {
                if ctx.input(|i| i.pointer.any_pressed()) {
                    if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                        if !inner_response.response.rect.contains(pos) && !anchor_rect.contains(pos) {
                            open_popup.open = None;
                        }
                    }
                }
            }
            if !still_open {
                open_popup.open = None;
            }
        } else if cat_name == "EngineeringPoints" {
            let ep_color = egui::Color32::from_rgb(100, 255, 200);
            let mut still_open = true;
            let window_response = egui::Window::new("Engineering Points")
                .id(egui::Id::new("engineering_points_window"))
                .fixed_pos(egui::pos2(anchor_rect.left(), anchor_rect.bottom() + 2.0))
                .collapsible(false)
                .resizable(false)
                .title_bar(false)
                .open(&mut still_open)
                .frame(egui::Frame::popup(ctx.style().as_ref()))
                .show(ctx, |ui| {
                    ui.set_min_width(220.0);
                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new(egui::RichText::new("⚙").size(18.0).color(ep_color)).selectable(false));
                        ui.add(egui::Label::new(egui::RichText::new("Engineering Points").size(16.0).strong().color(ep_color)).selectable(false));
                    });
                    ui.separator();

                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new("Available:").selectable(false));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add(egui::Label::new(egui::RichText::new(format!("{:.0}", research_state.engineering_points_available)).strong()).selectable(false));
                        });
                    });
                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new("Monthly Income:").selectable(false));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let (rt, rc) = format_points_rate_monthly(rate_tracker.engineering_rate_per_month);
                            ui.add(egui::Label::new(egui::RichText::new(rt).strong().color(rc)).selectable(false));
                        });
                    });
                });

            if let Some(inner_response) = window_response {
                if ctx.input(|i| i.pointer.any_pressed()) {
                    if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                        if !inner_response.response.rect.contains(pos) && !anchor_rect.contains(pos) {
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
            let icon = get_resource_category_icon(&cat_name);
            let color = get_category_color(&cat_name);

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
                        ui.add(egui::Label::new(egui::RichText::new(icon).size(18.0).color(color)).selectable(false));
                        ui.add(egui::Label::new(egui::RichText::new(cat_name.as_str()).size(16.0).strong().color(color)).selectable(false));
                    });
                    ui.separator();

                    // Header row
                    ui.horizontal(|ui| {
                        ui.add_space(24.0); // icon space
                        ui.add(egui::Label::new(egui::RichText::new("Resource").strong()).selectable(false));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add(egui::Label::new(egui::RichText::new("  /mo").strong().size(11.0)).selectable(false));
                            ui.add_space(10.0);
                            ui.add(egui::Label::new(egui::RichText::new("Stockpile").strong().size(11.0)).selectable(false));
                        });
                    });

                    for resource in &resources {
                        let amount = budget.get_stockpile(resource);
                        let rate = rate_tracker.get_resource_rate(resource);
                        ui.horizontal(|ui| {
                            ui.add(egui::Label::new(egui::RichText::new(get_resource_icon(resource)).size(16.0)).selectable(false));
                            ui.add(egui::Label::new(resource.display_name()).selectable(false));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                // Monthly rate
                                let (rt, rc) = format_rate_monthly(rate);
                                ui.add(egui::Label::new(egui::RichText::new(rt).size(11.0).color(rc)).selectable(false));
                                ui.add_space(10.0);
                                // Stockpile
                                ui.add(egui::Label::new(egui::RichText::new(format_mass(amount)).strong()).selectable(false));
                            });
                        });
                    }
                });

            // Close if clicked outside
            if let Some(inner_response) = window_response {
                if ctx.input(|i| i.pointer.any_pressed()) {
                    if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                        if !inner_response.response.rect.contains(pos) && !anchor_rect.contains(pos) {
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

fn format_population(count: f64) -> String {
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

/// Render the top menu bar with pictograms
fn ui_top_menu_bar(
    mut contexts: EguiContexts,
    mut active_menu: ResMut<ActiveMenu>,
    mut view_mode: ResMut<ViewMode>,
    menu_icons: Option<Res<MenuIcons>>,
    mut icon_textures: Local<HashMap<GameMenu, egui::TextureId>>,
) {
    // Convert loaded handles to egui TextureIds before creating the UI context.
    // We cache the TextureIds in a Local<HashMap> so that `add_image` is called
    // at most once per GameMenu, and we simply reuse the cached TextureIds on
    // subsequent frames.
    let texture_map: Option<HashMap<GameMenu, egui::TextureId>> =
        if let Some(menu_icons) = menu_icons.as_ref() {
            // Populate the cache lazily: only create a TextureId the first time
            // we see a given GameMenu.
            for (mkey, handle) in menu_icons.handles.iter() {
                icon_textures
                    .entry(*mkey)
                    .or_insert_with(|| contexts.add_image(handle.clone()));
            }
            // Clone the cached map so the rest of the UI code can use an owned
            // HashMap just like before.
            Some(icon_textures.clone())
        } else {
            None
        };

    let ctx = match contexts.try_ctx_mut() {
        Some(ctx) => ctx,
        None => return,
    };

    egui::TopBottomPanel::top("top_menu_bar")
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(10.0);
                
                // Add each menu button
                for &menu in GameMenu::all() {
                    let is_active = active_menu.current == menu;
                    
                    if let Some(map) = texture_map.as_ref() {
                        if let Some(texture_id) = map.get(&menu) {
                            let size = egui::vec2(80.0, 80.0);
                            
                            // Tint the icon:
                            // Blue/Cyan for active, White/Gray for inactive
                            let tint = if is_active {
                                egui::Color32::from_rgb(100, 200, 255)
                            } else {
                                egui::Color32::from_rgb(200, 200, 200)
                            };

                            let mut img = egui::Image::new((*texture_id, size));
                            img = img.tint(tint);
                            
                            let resp = ui.add(egui::ImageButton::new(img));

                            // Highlight active menu by drawing a subtle stroke around the widget
                            if is_active {
                                let rect = resp.rect;
                                ui.painter().rect_stroke(rect, 4.0, egui::Stroke::new(2.0, egui::Color32::from_rgb(100, 200, 255)));
                            }

                            let resp = resp.on_hover_text(menu.name());
                            if resp.clicked() {
                                active_menu.current = menu;
                                match menu {
                                    GameMenu::Starmap => *view_mode = ViewMode::Starmap,
                                    GameMenu::Survey => *view_mode = ViewMode::System,
                                    _ => *view_mode = ViewMode::System,
                                }
                            }
                        } else {
                            // Fallback to text button when the texture is not available
                            let button_text = format!("{} {}", menu.icon(), menu.name());
                            let button = if is_active {
                                egui::Button::new(
                                    egui::RichText::new(button_text)
                                        .size(14.0)
                                        .color(egui::Color32::from_rgb(100, 200, 255))
                                )
                                .fill(egui::Color32::from_rgb(40, 60, 80))
                            } else {
                                egui::Button::new(
                                    egui::RichText::new(button_text)
                                        .size(14.0)
                                )
                                .fill(egui::Color32::from_rgb(30, 30, 35))
                            };

                            if ui.add(button).clicked() {
                                active_menu.current = menu;
                                match menu {
                                    GameMenu::Starmap => *view_mode = ViewMode::Starmap,
                                    GameMenu::Survey => *view_mode = ViewMode::System,
                                    _ => *view_mode = ViewMode::System,
                                }
                            }
                        }
                    } else {
                        // No icons loaded yet - use existing emoji+text button
                        let button_text = format!("{} {}", menu.icon(), menu.name());
                        let button = if is_active {
                            egui::Button::new(
                                egui::RichText::new(button_text)
                                    .size(14.0)
                                    .color(egui::Color32::from_rgb(100, 200, 255))
                            )
                            .fill(egui::Color32::from_rgb(40, 60, 80))
                        } else {
                            egui::Button::new(
                                egui::RichText::new(button_text)
                                    .size(14.0)
                            )
                            .fill(egui::Color32::from_rgb(30, 30, 35))
                        };

                        if ui.add(button).clicked() {
                            active_menu.current = menu;
                            match menu {
                                GameMenu::Starmap => *view_mode = ViewMode::Starmap,
                                GameMenu::Survey => *view_mode = ViewMode::System,
                                _ => *view_mode = ViewMode::System,
                            }
                        }
                    }
                    
                    ui.add_space(5.0);
                }
            });
        });
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
    active_menu: Res<ActiveMenu>,
) {
    // Don't show world tooltips when a full-screen overlay is active
    if active_menu.current.blocks_world_interaction() {
        return;
    }

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
    active_menu: Res<ActiveMenu>,
) {
    // Don't show world tooltips when a full-screen overlay is active
    if active_menu.current.blocks_world_interaction() {
        return;
    }

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

/// Format a monthly rate value with sign and appropriate color.
/// Returns (formatted_string, color).
fn format_rate_monthly(value: f64) -> (String, egui::Color32) {
    if value > 0.0 {
        (format!("+{}/mo", format_mass(value)), egui::Color32::from_rgb(100, 255, 100))
    } else if value < 0.0 {
        (format!("{}/mo", format_mass(value)), egui::Color32::from_rgb(255, 100, 100))
    } else {
        ("+0/mo".to_string(), egui::Color32::GRAY)
    }
}

/// Format a monthly rate for points (integer display).
fn format_points_rate_monthly(value: f64) -> (String, egui::Color32) {
    if value > 0.0 {
        (format!("+{:.0}/mo", value), egui::Color32::from_rgb(100, 255, 100))
    } else if value < 0.0 {
        (format!("{:.0}/mo", value), egui::Color32::from_rgb(255, 100, 100))
    } else {
        ("+0/mo".to_string(), egui::Color32::GRAY)
    }
}

/// Main UI dashboard system
#[allow(clippy::too_many_arguments)]
fn ui_dashboard(
    mut commands: Commands,
    mut contexts: EguiContexts,
    // budget: Res<GlobalBudget>, // Moved to ui_resources_bar
    mut time_scale: ResMut<TimeScale>,
    sim_time: Res<SimulationTime>,
    mut selection: ResMut<Selection>,
    view_mode: Res<ViewMode>,
    current_system: Res<CurrentStarSystem>,
    nearby_stars: Res<NearbyStarsData>,
    active_menu: Res<ActiveMenu>,
    // Query for selected body information
    mut body_query: Query<(
        &CelestialBody,
        &SpaceCoordinates,
        Option<&KeplerOrbit>,
        Option<&PlanetResources>,
        Option<&AtmosphereComposition>,
        Option<&mut SurveyLevel>,
        Option<&Population>,
        Option<&crate::astronomy::SurfaceTemperature>,
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

    if active_menu.current == GameMenu::Research
        || active_menu.current == GameMenu::Construction
        || active_menu.current == GameMenu::Economy
    {
        return;
    }

    // Ledger Panel (Left)
    egui::SidePanel::left("ledger_panel")
        .min_width(200.0)
        .show(ctx, |ui| {
            match active_menu.current {
                GameMenu::Starmap => {
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
                GameMenu::Survey => {
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
                _ => {
                    // Placeholder for other menus
                    ui.heading(active_menu.current.name());
                    ui.separator();
                    
                    ui.label(
                        egui::RichText::new("Coming Soon")
                            .size(16.0)
                            .color(egui::Color32::from_rgb(180, 180, 180))
                    );
                    
                    ui.add_space(10.0);
                    
                    match active_menu.current {
                        GameMenu::Main => {
                            ui.label("Main menu options:");
                            if ui.button("🚪 Quit Game").clicked() {
                                // TODO: Implement quit
                                info!("Quit clicked");
                            }
                            if ui.button("💾 Save Game").clicked() {
                                info!("Save clicked");
                            }
                            if ui.button("📂 Load Game").clicked() {
                                info!("Load clicked");
                            }
                            if ui.button("⚙ Options").clicked() {
                                info!("Options clicked");
                            }
                        }
                        GameMenu::Construction => {
                            // Handled by ui_construction_panels system
                            ui.label("Switch to full Construction view for details.");
                        }
                        GameMenu::Research => {
                            ui.label("Research UI requires loading...");
                            ui.label("Switch to Research view to see tech tree.");
                        }
                        GameMenu::Fleets => {
                            ui.label("Fleet management and deployment will be shown here.");
                        }
                        GameMenu::Shipbuilding => {
                            ui.label("Ship design and construction queue will be shown here.");
                        }
                        GameMenu::Economy => {
                            ui.label("Economic overview and private sector management will be shown here.");
                        }
                        GameMenu::Personnel => {
                            ui.label("Officers, managers, and personnel assignments will be shown here.");
                        }
                        GameMenu::Intel => {
                            ui.label("Intelligence reports on enemy factions will be shown here.");
                        }
                        GameMenu::Diplomacy => {
                            ui.label("Diplomatic relations and treaties will be shown here.");
                        }
                        GameMenu::Starmap | GameMenu::Survey => {
                            // Already handled above
                        }
                    }
                }
            }
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
                    if let Ok((body, coords, orbit, resources, atmosphere, mut survey_level, population, surface_temp)) = body_query.get_mut(entity) {
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
                            ui.label(format!("Gravity: {:.2} g", body.surface_gravity()));
                            if let Some(pop) = population {
                                if pop.count > 0.0 {
                                    ui.label(format!("Population: {}", format_population(pop.count)));
                                }
                            }
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

                        // Show Colony Cost for all bodies
                        ui.group(|ui| {
                            ui.label(egui::RichText::new("Habitability").strong());
                            
                            let mut temp_c = -273.15;
                            let mut min_temp_c = -273.15;
                            let mut max_temp_c = -273.15;

                            // Try to get temperature from SurfaceTemperature component, then Atmosphere
                            if let Some(comp) = surface_temp {
                                temp_c = comp.average_celsius;
                                min_temp_c = comp.min_celsius;
                                max_temp_c = comp.max_celsius;
                            } else if let Some(atm) = atmosphere {
                                temp_c = atm.surface_temperature_celsius;
                                min_temp_c = temp_c;
                                max_temp_c = temp_c;
                            }

                            // Colony Cost
                            ui.horizontal(|ui| {
                                ui.label("Colony Cost:");
                                let gravity = body.surface_gravity();
                                let cost_details = crate::astronomy::components::calculate_colony_cost_details(
                                    gravity, 
                                    min_temp_c, 
                                    max_temp_c,
                                    atmosphere.as_deref()
                                );
                                let cost = cost_details.total_cost;
                                
                                let cost_tooltip = |ui: &mut egui::Ui| {
                                    if cost_details.heavy_gravity_limit_exceeded {
                                        ui.colored_label(egui::Color32::RED, "Uninhabitable: Gravity > 1.7g");
                                    } else {
                                        ui.label(egui::RichText::new("Colony Cost Factors").strong());
                                        ui.separator();
                                        
                                        if cost_details.base_cost > 0.0 {
                                            ui.label(format!("Base Cost (Unbreathable): +{:.2}", cost_details.base_cost));
                                        }
                                        if cost_details.cold_cost > 0.0 {
                                            ui.label(format!("Cold Penalty (Heating): +{:.2}", cost_details.cold_cost));
                                        }
                                        if cost_details.heat_cost > 0.0 {
                                            ui.label(format!("Heat Penalty (Cooling): +{:.2}", cost_details.heat_cost));
                                        }
                                        if cost_details.pressure_cost > 0.0 {
                                            ui.label(format!("High Pressure Penalty: +{:.2}", cost_details.pressure_cost));
                                        }
                                        if cost_details.low_gravity_penalty > 0.0 {
                                            ui.label(format!("Low Gravity Penalty: +{:.2}", cost_details.low_gravity_penalty));
                                        }
                                    }
                                };

                                if cost.is_infinite() {
                                    ui.colored_label(egui::Color32::RED, "Uninhabitable (Gravity)")
                                        .on_hover_ui(cost_tooltip);
                                } else {
                                    let cost_color = if cost <= 0.0 {
                                        egui::Color32::GREEN
                                    } else if cost <= 2.0 {
                                        egui::Color32::YELLOW
                                    } else if cost <= 5.0 {
                                        egui::Color32::from_rgb(255, 165, 0) // Orange
                                    } else {
                                        egui::Color32::RED
                                    };
                                    ui.colored_label(cost_color, format!("{:.2}", cost))
                                        .on_hover_ui(cost_tooltip);
                                }
                            });
                            
                            // Temperature display (moved out of Atmosphere section so it shows for everyone)
                            ui.horizontal(|ui| {
                                ui.label("Temperature:");
                                ui.label(format!("{:.1}°C", temp_c));
                            });
                        });
                        
                        ui.add_space(5.0);

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
                                            ui.label("Breathable:");
                                            if atmosphere.breathable {
                                                ui.colored_label(egui::Color32::GREEN, "✓ Yes");
                                            } else {
                                                ui.colored_label(egui::Color32::RED, "✗ No");
                                            }
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
                                                        
                                                        // Skip resources with negligible amounts
                                                        // Threshold of 0.001 Mt (= 1 kt) prevents showing "0.0 kt" entries
                                                        if discovered_mt < 0.001 && !deposit.is_viable() {
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
                                                        
                                                        // Use the deposit's is_atmospheric flag to decide labels
                                                        // Atmospheric gases show: Atmospheric / Trapped-Dissolved / Chemically Bound
                                                        // Mineral deposits show: Proven Reserves / Deep Deposits / Planetary Bulk
                                                        let is_atm = deposit.is_atmospheric;
                                                        let proven_label = if is_atm { "    Atmospheric:" } else { "    Proven Reserves:" };
                                                        let deep_label = if is_atm { "    Trapped/Dissolved:" } else { "    Deep Deposits:" };
                                                        let bulk_label = if is_atm { "    Chemically Bound:" } else { "    Planetary Bulk:" };
                                                        
                                                        // Proven / Atmospheric (Always visible if Orbital+)
                                                        ui.horizontal(|ui| {
                                                            ui.label(proven_label);
                                                            ui.add(egui::ProgressBar::new(1.0)
                                                                .text(format_mass(deposit.reserve.proven_crustal)));
                                                        });
                                                        
                                                        // Deep / Trapped
                                                        if matches!(current_level, SurveyLevel::SeismicSurvey | SurveyLevel::CoreSample) {
                                                            ui.horizontal(|ui| {
                                                                ui.label(deep_label);
                                                                ui.add(egui::ProgressBar::new(1.0)
                                                                    .text(format_mass(deposit.reserve.deep_deposits)));
                                                            });
                                                        } else {
                                                             ui.label(format!("    {}: ???", if is_atm { "Trapped/Dissolved" } else { "Deep Deposits" }));
                                                        }
                                                        
                                                        // Bulk / Chemically Bound
                                                        if current_level == SurveyLevel::CoreSample {
                                                            ui.horizontal(|ui| {
                                                                ui.label(bulk_label);
                                                                 ui.add(egui::ProgressBar::new(1.0)
                                                                    .text(format_mass(deposit.reserve.planetary_bulk)));
                                                            });
                                                        } else {
                                                            ui.label(format!("    {}: ???", if is_atm { "Chemically Bound" } else { "Planetary Bulk" }));
                                                        }
                                                        
                                                        // Only show concentration for non-atmospheric deposits
                                                        // Concentration is meaningless for gas in an atmosphere
                                                        if !is_atm {
                                                            ui.horizontal(|ui| {
                                                                ui.label("    Concentration:");
                                                                ui.add(egui::ProgressBar::new(deposit.reserve.concentration)
                                                                    .text(format!("{:.1}%", deposit.reserve.concentration * 100.0)));
                                                            });
                                                        }
                                                        
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
                        for (resource_type, deposit) in &resources.deposits {
                            let total = deposit.total_megatons();
                            *total_resources.entry(*resource_type).or_insert(0.0) += total;
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

/// System to render research panels and tech tree
/// Separated from ui_dashboard to avoid parameter count limit
/// Info about an active/paused research project, for UI display
#[derive(Clone, Debug)]
#[allow(dead_code)]
struct ActiveProjectInfo {
    entity: Entity,
    progress_percent: f32,
    progress: f64,
    required_points: f64,
    allocation_percent: f64,
    active: bool,
}

fn ui_research_panels(
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

    // Convert loaded handles to egui TextureIds
    if let Some(icons) = &research_icons {
        for (cat, handle) in &icons.handles {
             icon_textures.entry(*cat).or_insert_with(|| contexts.add_image(handle.clone()));
        }
    }
    let icon_textures = &*icon_textures;
    
    // Toggle debug mode with F12
    if keyboard_input.just_pressed(KeyCode::F12) {
        debug_settings.enabled = !debug_settings.enabled;
    }

    let ctx = match contexts.try_ctx_mut() {
        Some(ctx) => ctx,
        None => return,
    };

    // Main panel - Tabbed interface (no left sidebar)
    egui::CentralPanel::default().show(ctx, |ui| {
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
            ui.selectable_value(&mut *selected_tab, 2, "🔬 Available Research");
            ui.selectable_value(&mut *selected_tab, 3, "⚙ Available Engineering");
            ui.selectable_value(&mut *selected_tab, 4, "📚 Archive");
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
            0 => render_overview_tab(ui, &research_state, &tech_data, &research_projects, &engineering_projects, &all_teams, &team_capacity, &mut *ui_prefs),
            1 => render_tech_tree_tab(ui, &research_state, &mut tech_data, icon_textures, debug_settings.enabled, &mut edit_state, &active_research, &mut pending_research),
            2 => render_available_research_tab(ui, &research_state, &tech_data, icon_textures, &active_research, &mut pending_research, &team_capacity),
            3 => render_available_engineering_tab(ui, &research_state, &tech_data, icon_textures),
            4 => render_archive_tab(ui, &research_state, &tech_data, icon_textures),
            _ => {},
        }
    });
}

/// Render the Overview tab - shows active projects and team assignments
fn render_overview_tab(
    ui: &mut egui::Ui,
    research_state: &ResearchState,
    tech_data: &TechnologiesData,
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
                for (_entity, project, team) in research_projects.iter() {
                    if let Some(tech) = tech_data.get_tech(&project.tech_id) {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(&tech.name).strong());
                            ui.label(format!("(Team: {})", team.name));
                            if !project.active {
                                ui.label(egui::RichText::new("⏸ PAUSED")
                                    .color(egui::Color32::YELLOW));
                            }
                        });
                        
                        let progress = project.progress_percent();
                        ui.add(egui::ProgressBar::new(progress)
                            .text(format!("{:.1}% ({:.0}/{:.0} RP)", 
                                progress * 100.0, project.progress, project.required_points)));
                        
                        ui.horizontal(|ui| {
                            ui.label(format!("Allocation: {:.0}%", project.rp_allocation_percent * 100.0));
                        });
                        
                        ui.add_space(5.0);
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
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(&component.name).strong());
                            ui.label(format!("(Team: {})", team.name));
                        });
                        
                        let progress = project.progress_percent();
                        ui.add(egui::ProgressBar::new(progress)
                            .text(format!("{:.0}%", progress * 100.0)));
                        
                        ui.horizontal(|ui| {
                            ui.label(format!("Progress: {:.0}/{:.0} EP", 
                                project.progress, project.required_points));
                        });
                        
                        ui.add_space(5.0);
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
    let tier_spacing = 350.0 * zoom;
    let node_spacing_y = 80.0 * zoom;
    let category_spacing = 20.0 * zoom;
    
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
    
    // Zoom
    if response.hovered() {
        let scroll_delta = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll_delta != 0.0 {
            zoom = (zoom + scroll_delta * 0.001).clamp(0.3, 3.0);
        }
    }
    // Pan (only middle-click now; right-click is for context menu)
    if response.dragged_by(egui::PointerButton::Middle) {
        pan_offset += response.drag_delta();
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

    // ---------- compute node positions (top-left corner) ----------
    // Uses a barycenter heuristic: tier-0 techs are sorted by category,
    // subsequent tiers sort by the average Y of their prerequisites so that
    // connected nodes stay close together and lines are shorter.
    let mut node_positions: HashMap<String, egui::Pos2> = HashMap::new();
    
    let mut techs_by_tier: std::collections::BTreeMap<u32, Vec<&crate::research::types::Technology>> =
        std::collections::BTreeMap::new();
    for (_, tech) in &tech_data.technologies {
        techs_by_tier.entry(tech.tier).or_default().push(tech);
    }
    
    for (tier_idx, (_tier, techs)) in techs_by_tier.iter().enumerate() {
        let mut sorted_techs = techs.clone();
        if tier_idx == 0 {
            // Root tier: deterministic category + name sort
            sorted_techs.sort_by_key(|t| (t.category as u8, t.name.as_str()));
        } else {
            // Barycenter: sort by the average Y position of prerequisites.
            // Techs with no positioned prerequisites fall back to category sort.
            sorted_techs.sort_by(|a, b| {
                let avg_y = |tech: &&crate::research::types::Technology| -> f64 {
                    let ys: Vec<f32> = tech
                        .prerequisites
                        .iter()
                        .filter_map(|pid| node_positions.get(pid).map(|p| p.y))
                        .collect();
                    if ys.is_empty() {
                        // Fallback: use category ordinal so it groups nicely
                        tech.category as u8 as f64 * 1000.0
                    } else {
                        ys.iter().map(|y| *y as f64).sum::<f64>() / ys.len() as f64
                    }
                };
                avg_y(&a)
                    .partial_cmp(&avg_y(&b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        
        let base_x = (canvas_rect.left() + pan_offset.x + (tier_idx as f32) * tier_spacing).round();
        let mut current_y = (canvas_rect.top() + pan_offset.y).round();
        let mut last_category: Option<TechCategory> = None;
        
        for tech in sorted_techs {
            if let Some(last_cat) = last_category {
                if last_cat != tech.category {
                    current_y += category_spacing;
                }
            }
            last_category = Some(tech.category);
            // Store the CENTER of the node for line connections
            node_positions.insert(
                tech.id.clone(),
                egui::Pos2::new(base_x + node_w / 2.0, current_y + node_h / 2.0),
            );
            current_y += node_h + node_spacing_y;
        }
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
    let tooltip_tech_id = hovered_tech_id.clone().or_else(|| selected_tech.clone());
    let tooltip_rect = if hovered_tech_id.is_some() {
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
    
    if let (Some(ref tid), Some(tr)) = (&tooltip_tech_id, tooltip_rect) {
        if let Some(tech) = tech_data.technologies.get(tid) {
            let is_unlocked = research_state.is_unlocked(&tech.id);
            let is_researching = active_research.contains_key(&tech.id);
            let can_research =
                !is_unlocked && !is_researching && tech_data.check_prerequisites(&tech.id, &unlocked_ids);
            
            let tooltip_pos = egui::pos2(tr.right() + 8.0, tr.top());
            
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
                    ui.set_max_width(350.0);
                    ui.label(egui::RichText::new(&tech.name).strong().size(14.0));
                    ui.horizontal(|ui| {
                        if let Some(tex) = icon_textures.get(&tech.category) {
                            ui.add(egui::Image::new(egui::load::SizedTexture::new(
                                *tex,
                                [16.0, 16.0],
                            )));
                        } else {
                            ui.label(tech.category.icon());
                        }
                        ui.label(
                            egui::RichText::new(tech.category.display_name())
                                .color(tech_category_color(tech.category)),
                        );
                    });
                    ui.separator();
                    ui.label(&tech.description);
                    ui.add_space(5.0);
                    ui.label(format!(
                        "Tier: {} | Cost: {:.0} RP",
                        tech.tier, tech.research_cost
                    ));
                    if !tech.prerequisites.is_empty() {
                        ui.add_space(5.0);
                        ui.label(egui::RichText::new("Prerequisites:").strong());
                        for prereq_id in &tech.prerequisites {
                            if let Some(prereq) = tech_data.get_tech(prereq_id) {
                                let c = if research_state.is_unlocked(prereq_id) {
                                    egui::Color32::from_rgb(100, 255, 100)
                                } else {
                                    egui::Color32::from_rgb(255, 100, 100)
                                };
                                ui.label(
                                    egui::RichText::new(format!("  • {}", prereq.name)).color(c),
                                );
                            }
                        }
                    }
                    if !tech.unlocks_components.is_empty() {
                        ui.add_space(5.0);
                        ui.label(egui::RichText::new("Unlocks Components:").strong());
                        for comp_id in &tech.unlocks_components {
                            if let Some(comp) = tech_data.get_component(comp_id) {
                                ui.label(format!(
                                    "  ⚙ {} ({:.0} EP)",
                                    comp.name, comp.engineering_cost
                                ));
                            }
                        }
                    }
                    if !tech.modifiers.is_empty() {
                        ui.add_space(5.0);
                        ui.label(egui::RichText::new("Provides Bonuses:").strong());
                        for modifier in &tech.modifiers {
                            ui.label(format!(
                                "  • {:?}: {:+.0}%",
                                modifier.modifier_type, modifier.value
                            ));
                        }
                    }
                    if is_researching {
                        if let Some(info) = active_research.get(&tech.id) {
                            ui.add_space(5.0);
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
                        }
                    } else if can_research {
                        ui.add_space(5.0);
                        ui.separator();
                        if ui.button("🔬 Start Research").clicked() {
                            pending_research.start_research.push(tech.id.clone());
                            pending_research.navigate_to_available_tab = true;
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
    ui.allocate_ui_at_rect(status_rect, |ui| {
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
                                egui::ComboBox::from_id_source("tech_edit_cat")
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
                                egui::ComboBox::from_id_source("add_prereq_combo")
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
                    modifiers: Vec::new(),
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
    
    ui.heading("Available Research Projects");
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
            ui.add_space(5.0);
            
            for (tech_id, info) in &active_projects {
                if let Some(tech) = tech_data.get_tech(tech_id) {
                    let cat_color = tech_category_color(tech.category);
                    ui.group(|ui| {
                        ui.set_max_width(600.0);
                        ui.horizontal(|ui| {
                            let status_icon = if info.active { "🔬" } else { "⏸" };
                            ui.label(egui::RichText::new(status_icon).size(16.0));
                            ui.label(egui::RichText::new(&tech.name).strong().size(14.0));
                            if let Some(tex) = icon_textures.get(&tech.category) {
                                ui.add(egui::Image::new(egui::load::SizedTexture::new(*tex, [20.0, 20.0]))
                                    .tint(cat_color));
                            }
                            ui.label(egui::RichText::new(tech.category.display_name()).size(12.0).color(cat_color));
                            if !info.active {
                                ui.label(egui::RichText::new("PAUSED").color(egui::Color32::YELLOW));
                            }
                        });
                        
                        // Progress bar with numeric display
                        ui.add(egui::ProgressBar::new(info.progress_percent)
                            .text(format!("{:.1}% ({:.0}/{:.0} RP)", 
                                info.progress_percent * 100.0, info.progress, info.required_points))
                            .desired_width(500.0));
                        
                        // Allocation slider and control buttons
                        ui.horizontal(|ui| {
                            ui.label("Allocation:");
                            let mut alloc_pct = (info.allocation_percent * 100.0) as f32;
                            let slider_resp = ui.add(
                                egui::Slider::new(&mut alloc_pct, 0.0..=100.0)
                                    .suffix("%")
                                    .fixed_decimals(0)
                            );
                            if slider_resp.changed() {
                                pending_research.update_allocations.push(
                                    (tech_id.to_string(), alloc_pct as f64 / 100.0)
                                );
                            }
                            
                            ui.add_space(10.0);
                            
                            if info.active {
                                if ui.button("⏸ Stop").on_hover_text("Pause research (preserves progress)").clicked() {
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
                        });
                    });
                    ui.add_space(5.0);
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
            ui.add_space(5.0);
            
            available_techs.sort_by(|a, b| {
                a.category.display_name()
                    .cmp(b.category.display_name())
                    .then(a.research_cost.partial_cmp(&b.research_cost).unwrap())
            });
            
            for tech in available_techs {
                ui.group(|ui| {
                    ui.set_max_width(600.0);
                    ui.horizontal(|ui| {
                        let cat_color = tech_category_color(tech.category);
                        ui.label(egui::RichText::new("⏳").color(egui::Color32::from_rgb(255, 255, 100)));
                        ui.label(egui::RichText::new(&tech.name).strong().size(14.0));
                        if let Some(tex) = icon_textures.get(&tech.category) {
                             ui.add(egui::Image::new(egui::load::SizedTexture::new(*tex, [24.0, 24.0]))
                                 .tint(cat_color));
                             ui.label(egui::RichText::new(tech.category.display_name()).size(14.0).color(cat_color));
                        } else {
                            ui.label(egui::RichText::new(format!("{} {}", tech.category.icon(), tech.category.display_name()))
                                .size(14.0)
                                .color(cat_color));
                        }
                    });
                    
                    ui.label(&tech.description);
                    
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!("Cost: {:.0} RP", tech.research_cost))
                            .color(egui::Color32::from_rgb(150, 200, 255)));
                        ui.label(format!("Tier: {}", tech.tier));
                    });
                    
                    if !tech.unlocks_components.is_empty() {
                        ui.label(egui::RichText::new(format!(
                            "Unlocks {} component(s)",
                            tech.unlocks_components.len()
                        )).size(11.0).italics());
                    }
                    
                    if !tech.modifiers.is_empty() {
                        ui.label(egui::RichText::new(format!(
                            "Provides {} bonus(es)",
                            tech.modifiers.len()
                        )).size(11.0).italics().color(egui::Color32::from_rgb(100, 255, 100)));
                    }
                    
                    // Start research button
                    let can_start = teams_available > 0;
                    ui.horizontal(|ui| {
                        let btn = ui.add_enabled(can_start, egui::Button::new("🚀 Start Research"));
                        if can_start && btn.clicked() {
                            pending_research.start_research.push(tech.id.clone());
                        }
                        if !can_start {
                            btn.on_hover_text("No team slots available. Stop another project first.");
                        }
                    });
                });
                
                ui.add_space(5.0);
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
    ui.heading("Available Engineering Projects");
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
                ui.group(|ui| {
                    // Look up parent tech to get category info
                    let parent_tech = tech_data.get_tech(&component.required_tech);
                    let cat_color = parent_tech
                        .map(|t| tech_category_color(t.category))
                        .unwrap_or(egui::Color32::from_rgb(200, 200, 100));

                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("⚙").color(cat_color));
                        ui.label(egui::RichText::new(&component.name).strong().size(14.0));
                        if let Some(tech) = parent_tech {
                            if let Some(tex) = icon_textures.get(&tech.category) {
                                ui.add(egui::Image::new(egui::load::SizedTexture::new(*tex, [20.0, 20.0]))
                                    .tint(cat_color));
                            }
                            ui.label(egui::RichText::new(tech.category.display_name()).size(12.0).color(cat_color));
                        }
                    });
                    
                    ui.label(&component.description);
                    
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!("Cost: {:.0} EP", component.engineering_cost))
                            .color(egui::Color32::from_rgb(150, 255, 200)));
                        
                        if let Some(tech) = tech_data.get_tech(&component.required_tech) {
                            ui.label(egui::RichText::new(format!("From: {}", tech.name))
                                .size(11.0)
                                .italics()
                                .color(egui::Color32::GRAY));
                        }
                    });
                    
                    // Placeholder button for future implementation
                    if ui.button("🔧 Start Engineering (Not Yet Implemented)").clicked() {
                        // Future: Create engineering project entity
                    }
                });
                
                ui.add_space(10.0);
            }
        }
    });
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
                                ui.horizontal(|ui| {
                                    ui.label("✔");
                                    ui.label(&tech.name);
                                    if tech.research_cost > 0.0 {
                                        ui.label(egui::RichText::new(format!("({:.0} RP)", tech.research_cost))
                                            .size(11.0)
                                            .color(egui::Color32::GRAY));
                                    }
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

/// System that renders the construction UI when the Construction menu is active.
///
/// Similar to `ui_research_panels`, this is a standalone system that only activates
/// when `GameMenu::Construction` is selected.
fn ui_construction_panels(
    mut contexts: EguiContexts,
    active_menu: Res<ActiveMenu>,
    colony_query: Query<(Entity, &Colony, &CelestialBody)>,
    construction_query: Query<(Entity, &ConstructionProject)>,
    mut construction_actions: ResMut<PendingConstructionActions>,
    research_state: Res<crate::research::ResearchState>,
    budget: Res<GlobalBudget>,
    mut debug_settings: ResMut<ConstructionDebugSettings>,
    buildings_data: Option<Res<BuildingsData>>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
) {
    if active_menu.current != GameMenu::Construction {
        return;
    }

    // Toggle debug mode with F12
    if keyboard_input.just_pressed(KeyCode::F12) {
        debug_settings.enabled = !debug_settings.enabled;
    }

    let ctx = match contexts.try_ctx_mut() {
        Some(ctx) => ctx,
        None => return,
    };

    egui::CentralPanel::default().show(ctx, |ui| {
        render_construction_panel(
            ui,
            &colony_query,
            &construction_query,
            &mut construction_actions,
            &research_state,
            &budget,
            &mut debug_settings,
            buildings_data.as_deref(),
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
    debug_settings: &mut ConstructionDebugSettings,
    buildings_data: Option<&BuildingsData>,
) {
    ui.heading("Construction");
    ui.separator();

    // Debug mode panel (if enabled)
    if debug_settings.enabled {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("🐛 DEBUG MODE").strong().color(egui::Color32::RED));
                ui.label(egui::RichText::new("(Press F12 to toggle)").italics().small());
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.checkbox(&mut debug_settings.free_construction, "Free Construction (no resource costs)");
                ui.checkbox(&mut debug_settings.instant_build, "Instant Build");
                ui.checkbox(&mut debug_settings.bypass_tech_requirements, "Bypass Tech Prerequisites");
            });
            ui.label(egui::RichText::new("⚠ Debug features are for development only")
                .small()
                .italics()
                .color(egui::Color32::YELLOW));
        });
        ui.separator();
    }

    // Global financial summary
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("💰 Treasury: {}", format_currency(budget.treasury)))
                .size(13.0)
                .color(egui::Color32::from_rgb(255, 215, 0)),
        );
        ui.separator();
        let balance = budget.balance_per_year();
        let balance_color = if balance >= 0.0 {
            egui::Color32::GREEN
        } else {
            egui::Color32::RED
        };
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
                .color(egui::Color32::from_rgb(180, 180, 180)),
        );
        ui.add_space(10.0);
        ui.label("Send a colony ship to a celestial body to establish a colony.");
        return;
    }

    let bypass_tech = debug_settings.enabled && debug_settings.bypass_tech_requirements;
    let free_build = debug_settings.enabled && debug_settings.free_construction;

    egui::ScrollArea::vertical().show(ui, |ui| {
    // Show each colony
    for (colony_entity, colony, body) in &colonies {
        let header = format!(
            "🏠 {} ({})",
            colony.name,
            Colony::format_population(colony.population)
        );

        egui::CollapsingHeader::new(
            egui::RichText::new(&header).size(14.0).strong(),
        )
        .default_open(true)
        .show(ui, |ui| {
            // Colony overview
            ui.horizontal(|ui| {
                ui.label(format!("Body: {}", body.name));
                ui.separator();
                ui.label(format!("Buildings: {}", colony.total_buildings()));
            });

            // Workforce status
            let workforce_eff = colony.workforce_efficiency();
            let wf_color = if workforce_eff >= 1.0 {
                egui::Color32::from_rgb(100, 200, 100)
            } else if workforce_eff >= 0.5 {
                egui::Color32::from_rgb(200, 200, 100)
            } else {
                egui::Color32::from_rgb(200, 100, 100)
            };
            ui.horizontal(|ui| {
                ui.label(format!(
                    "👷 Workforce: {} / {}",
                    colony.available_workforce(),
                    colony.total_workforce_demand()
                ));
                ui.label(
                    egui::RichText::new(format!("({:.0}%)", workforce_eff * 100.0))
                        .color(wf_color),
                );
                if workforce_eff < 1.0 {
                    ui.label(
                        egui::RichText::new("understaffed")
                            .size(11.0)
                            .color(egui::Color32::from_rgb(200, 100, 100)),
                    );
                }
            });

            // Logistics status
            let efficiency = colony.logistics_efficiency();
            let eff_color = if efficiency >= 1.0 {
                egui::Color32::from_rgb(100, 200, 100)
            } else if efficiency >= 0.5 {
                egui::Color32::from_rgb(200, 200, 100)
            } else {
                egui::Color32::from_rgb(200, 100, 100)
            };
            ui.horizontal(|ui| {
                ui.label("Logistics:");
                ui.label(
                    egui::RichText::new(format!("{:.0}%", efficiency * 100.0))
                        .color(eff_color),
                );
                if efficiency < 1.0 {
                    ui.label(
                        egui::RichText::new("(build Mass Drivers / Orbital Lifts)")
                            .size(11.0)
                            .color(egui::Color32::GRAY),
                    );
                }
            });

            // Housing
            let housing = colony.housing_capacity();
            let housing_util = if housing > 0.0 {
                (colony.population / housing * 100.0).min(100.0)
            } else {
                0.0
            };
            ui.horizontal(|ui| {
                ui.label(format!(
                    "Housing: {} / {} ({:.0}%)",
                    Colony::format_population(colony.population),
                    Colony::format_population(housing),
                    housing_util
                ));
            });

            // Growth
            let growth = colony.population_growth_per_year();
            if growth.abs() > 0.1 {
                ui.horizontal(|ui| {
                    ui.label(format!(
                        "Growth: +{}/year",
                        Colony::format_population(growth)
                    ));
                });
            }

            // Colony financials
            let income = colony.wealth_generation_per_year();
            let cost = colony.operating_cost_per_year();
            if income > 0.0 || cost > 0.0 {
                let colony_balance = income - cost;
                let cb_color = if colony_balance >= 0.0 {
                    egui::Color32::GREEN
                } else {
                    egui::Color32::RED
                };
                ui.horizontal(|ui| {
                    ui.label(format!("💰 Income: {}/yr", format_currency(income)));
                    ui.label(format!("| Cost: {}/yr", format_currency(cost)));
                    let sign = if colony_balance >= 0.0 { "+" } else { "" };
                    ui.label(
                        egui::RichText::new(format!("| Net: {}{}/yr", sign, format_currency(colony_balance)))
                            .color(cb_color),
                    );
                });
            }

            ui.add_space(5.0);

            // Existing buildings by category
            let has_buildings = colony.total_buildings() > 0;
            if has_buildings {
                egui::CollapsingHeader::new("📋 Buildings")
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
                                        "  {} {} × {} (👷 {})",
                                        building.icon(),
                                        building.display_name(),
                                        count,
                                        workers
                                    );
                                    // Show maintenance in tooltip
                                    if let Some(data) = buildings_data {
                                        let maint = data.maintenance_resources(&building);
                                        if !maint.is_empty() {
                                            let maint_str: Vec<_> = maint
                                                .iter()
                                                .map(|(r, a)| format!("{:.1} {}/yr", a * count as f64, r))
                                                .collect();
                                            label_text += &format!(" [maint: {}]", maint_str.join(", "));
                                        }
                                    }
                                    ui.horizontal(|ui| {
                                        ui.label(&label_text);
                                    });
                                }
                            }
                        }
                    });
            }

            // Construction queue
            let queue: Vec<_> = construction_query
                .iter()
                .filter(|(_, p)| p.colony_entity == *colony_entity)
                .collect();

            if !queue.is_empty() {
                egui::CollapsingHeader::new(format!("🔨 Queue ({})", queue.len()))
                    .default_open(true)
                    .show(ui, |ui| {
                        for (proj_entity, project) in &queue {
                            ui.horizontal(|ui| {
                                let pct = project.progress_percent() * 100.0;
                                ui.label(format!(
                                    "{} {} - {:.0}%",
                                    project.building_type.icon(),
                                    project.building_type.display_name(),
                                    pct
                                ));
                                if ui
                                    .small_button("✖")
                                    .on_hover_text("Cancel construction")
                                    .clicked()
                                {
                                    construction_actions
                                        .cancel_construction
                                        .push(*proj_entity);
                                }
                            });

                            // Progress bar
                            let bar = egui::ProgressBar::new(project.progress_percent())
                                .show_percentage();
                            ui.add(bar);
                        }
                    });
            }

            // Build new buildings
            egui::CollapsingHeader::new("➕ Build")
                .default_open(queue.is_empty() && !has_buildings)
                .show(ui, |ui| {
                    let factories = colony.building_count(BuildingType::Factory) as f64;
                    let bp_rate = 1.0 + factories * 10.0;
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!("Construction Output: {:.1} BP/year", bp_rate))
                                .color(egui::Color32::from_rgb(100, 200, 100))
                                .strong(),
                        );
                        ui.label(egui::RichText::new("ℹ").small())
                            .on_hover_text("Base: 1 BP/yr + 10 BP/yr per Factory");
                    });
                    ui.separator();

                    for category in BuildingCategory::all() {
                        let available: Vec<_> = category
                            .buildings()
                            .into_iter()
                            .filter(|b| {
                                if bypass_tech {
                                    return true;
                                }
                                // Check tech prerequisite from data file first, fall back to code
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

                        ui.label(
                            egui::RichText::new(category.display_name())
                                .size(12.0)
                                .strong(),
                        );
                        for building in available {
                            let costs = buildings_data
                                .map(|d| d.resource_costs(&building))
                                .unwrap_or(&[]);
                            let can_afford = free_build || costs.is_empty()
                                || can_afford_resources(budget, costs);

                            ui.horizontal(|ui| {
                                // Build resource cost summary
                                let mut cost_parts = Vec::new();
                                cost_parts.push(format!("{:.0} BP", building.build_cost()));
                                cost_parts.push(format!("👷 {}", building.workforce_required()));
                                if !costs.is_empty() {
                                    let res_str: Vec<_> = costs
                                        .iter()
                                        .map(|(r, a)| format!("{:.0} {}", a, r))
                                        .collect();
                                    cost_parts.push(res_str.join(", "));
                                }
                                let label = format!(
                                    "{} {} ({})",
                                    building.icon(),
                                    building.display_name(),
                                    cost_parts.join(" | ")
                                );

                                let button = ui.add_enabled(
                                    can_afford,
                                    egui::Button::new(
                                        egui::RichText::new(&label).size(11.0),
                                    ).small(),
                                );

                                // Build rich tooltip
                                let mut tooltip_text = building.description().to_string();
                                if !costs.is_empty() {
                                    tooltip_text += "\n\n📦 Construction costs:";
                                    for (r, a) in costs {
                                        let available = crate::colony::data::parse_resource_type(r)
                                            .map(|rt| budget.get_stockpile(&rt))
                                            .unwrap_or(0.0);
                                        let status = if available >= *a { "✔" } else { "✘" };
                                        tooltip_text += &format!("\n  {} {:.1} {} (have {:.1})", status, a, r, available);
                                    }
                                }
                                if let Some(data) = buildings_data {
                                    let maint = data.maintenance_resources(&building);
                                    if !maint.is_empty() {
                                        tooltip_text += "\n\n🔧 Maintenance (per year):";
                                        for (r, a) in maint {
                                            tooltip_text += &format!("\n  {:.2} {}", a, r);
                                        }
                                    }
                                    if let Some(def) = data.get(&building) {
                                        if !def.modifiers.is_empty() {
                                            tooltip_text += "\n\n⚡ Effects:";
                                            for m in &def.modifiers {
                                                let sign = if m.value >= 0.0 { "+" } else { "" };
                                                tooltip_text += &format!("\n  {}{:.0}% {}", sign, m.value, m.modifier_type);
                                            }
                                        }
                                    }
                                }

                                let response = button.on_hover_text(&tooltip_text);

                                if !can_afford {
                                    ui.label(
                                        egui::RichText::new("⚠ insufficient resources")
                                            .size(10.0)
                                            .color(egui::Color32::from_rgb(200, 100, 100)),
                                    );
                                }

                                if response.clicked() {
                                    construction_actions
                                        .start_construction
                                        .push((*colony_entity, building));
                                }
                            });
                        }
                        ui.add_space(3.0);
                    }

                    // Show locked buildings (unless bypassing)
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
                            ui.add_space(5.0);
                            ui.label(
                                egui::RichText::new("🔒 Locked (requires research)")
                                    .size(12.0)
                                    .color(egui::Color32::GRAY),
                            );
                            for building in locked {
                                let tech_id = buildings_data
                                    .and_then(|d| d.required_tech(building))
                                    .or_else(|| building.required_tech());
                                if let Some(tech_name) = tech_id {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "  {} {} — requires: {}",
                                            building.icon(),
                                            building.display_name(),
                                            tech_name
                                        ))
                                        .size(11.0)
                                        .color(egui::Color32::from_rgb(120, 120, 120)),
                                    );
                                }
                            }
                        }
                    }
                });

            ui.separator();
        });
    }
    });
}

// ============================================================================
// Economy Panel
// ============================================================================

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
    if rate > 0.01 {
        (format!("+{:.2}{}", rate, suffix), egui::Color32::from_rgb(100, 255, 100))
    } else if rate < -0.01 {
        (format!("{:.2}{}", rate, suffix), egui::Color32::from_rgb(255, 100, 100))
    } else {
        (format!("{:.2}{}", rate, suffix), egui::Color32::from_rgb(150, 150, 150))
    }
}

/// System that renders the Economy UI when the Economy menu is active.
///
/// This system provides a hierarchical view of the empire's economy broken down
/// by star system → celestial body → buildings/operations. Includes tabs for
/// overview, resources, colonies, mining, and power grid. The architecture is
/// prepared for future expansion with stations and mining ships.
fn ui_economy_panels(
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

    let ctx = match contexts.try_ctx_mut() {
        Some(ctx) => ctx,
        None => return,
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
                        ui.label(format!("Stock: {:.1} Mt", stockpile));
                        let (txt, col) = rate_text(rate, " Mt/mo");
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
    ui.label(egui::RichText::new("All quantities in Megatons (Mt). Rates are net monthly.").size(11.0).color(egui::Color32::GRAY));
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
                        ui.label(egui::RichText::new("Stockpile (Mt)").strong());
                        ui.label(egui::RichText::new("Net Rate (Mt/mo)").strong());
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
                            ui.label(egui::RichText::new(format!("{:.1}", stockpile)).monospace().color(stock_color));

                            let (txt, col) = rate_text(rate, "");
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
                                    let mut total_mining_rate = 0.0_f64;
                                    let mut total_atmo_rate = 0.0_f64;
                                    for (bt, count) in &colony.buildings {
                                        if *count == 0 { continue; }
                                        if let Some(def) = data.get(bt) {
                                            for modifier in &def.modifiers {
                                                match modifier.modifier_type.as_str() {
                                                    "MiningEfficiency" => total_mining_rate += modifier.value * *count as f64,
                                                    "AtmosphericHarvesting" => total_atmo_rate += modifier.value * *count as f64,
                                                    _ => {}
                                                }
                                            }
                                        }
                                    }

                                    // Solid mining production breakdown
                                    if total_mining_rate > 0.0 {
                                        let minable: Vec<(ResourceType, f64)> = body_entry.deposits.iter()
                                            .filter(|(_, d)| !d.is_atmospheric && (d.reserve.proven_crustal > 0.001 || d.reserve.deep_deposits > 0.001))
                                            .map(|(rt, d)| (*rt, (d.reserve.concentration as f64).max(0.001)))
                                            .collect();
                                        let total_weight: f64 = minable.iter().map(|(_, w)| w).sum();
                                        if total_weight > 0.0 {
                                            let monthly_total = total_mining_rate / 12.0;
                                            for (rt, weight) in &minable {
                                                let share = weight / total_weight;
                                                production_rows.push(("Mining".to_string(), *rt, monthly_total * share));
                                            }
                                        }
                                    }

                                    // Atmospheric harvesting production breakdown
                                    if total_atmo_rate > 0.0 {
                                        let harvestable: Vec<(ResourceType, f64)> = body_entry.deposits.iter()
                                            .filter(|(_, d)| d.is_atmospheric && (d.reserve.proven_crustal > 0.001 || d.reserve.deep_deposits > 0.001))
                                            .map(|(rt, d)| (*rt, (d.reserve.concentration as f64).max(0.001)))
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
                                        ui.label(egui::RichText::new("Production (Mt/mo):").strong().size(11.0).color(egui::Color32::from_rgb(100, 255, 100)));
                                        egui::Grid::new(format!("econ_prod_{}", body_entry.body_name))
                                            .num_columns(3)
                                            .spacing([10.0, 2.0])
                                            .striped(true)
                                            .show(ui, |ui| {
                                                for (source, rt, monthly) in &production_rows {
                                                    ui.label(egui::RichText::new(source).size(11.0));
                                                    ui.label(egui::RichText::new(rt.display_name()).size(11.0));
                                                    ui.label(egui::RichText::new(format!("+{:.3}", monthly)).monospace().size(11.0).color(egui::Color32::from_rgb(100, 255, 100)));
                                                    ui.end_row();
                                                }
                                            });
                                    }

                                    if !consumption_rows.is_empty() {
                                        ui.label(egui::RichText::new("Consumption (Mt/mo):").strong().size(11.0).color(egui::Color32::from_rgb(255, 140, 140)));
                                        egui::Grid::new(format!("econ_cons_{}", body_entry.body_name))
                                            .num_columns(3)
                                            .spacing([10.0, 2.0])
                                            .striped(true)
                                            .show(ui, |ui| {
                                                for (source, rt, monthly) in &consumption_rows {
                                                    ui.label(egui::RichText::new(source).size(11.0));
                                                    ui.label(egui::RichText::new(rt.display_name()).size(11.0));
                                                    ui.label(egui::RichText::new(format!("-{:.3}", monthly)).monospace().size(11.0).color(egui::Color32::from_rgb(255, 140, 140)));
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
                                    ui.label(egui::RichText::new(format!("⛏ {} — {:.3} Mt/mo [{}]", op.resource_type.display_name(), monthly, status)).size(11.0));
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
                                let est_load = colony.total_buildings as f64 * 10_000_000.0;
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
