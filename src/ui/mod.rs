//! UI module for the Helios Ascension interface
//!
//! Provides an egui-based dashboard showing:
//! - Resource stockpiles and critical resources
//! - Power grid status
//! - Selected celestial body information
//! - Time controls for simulation speed

use bevy::prelude::*;
use bevy::time::Real;
use bevy::window::PrimaryWindow;
use bevy_egui::{egui, EguiContexts, EguiPrimaryContextPass};
use bevy::asset::AssetServer;
use bevy::asset::Handle;
use bevy::image::Image;
use std::collections::HashMap;

pub mod interaction;

pub use interaction::Selection;

use crate::astronomy::components::{CurrentStarSystem, SystemId};
use crate::astronomy::nearby_stars::NearbyStarsData;
use crate::astronomy::{AtmosphereComposition, Hovered, KeplerOrbit, LagrangePointMarkers, LastLpClick, Selected, SpaceCoordinates};
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
use crate::plugins::camera::{capture_egui_panel_bounds, CameraAnchor, GameCamera, OrbitCamera, ViewMode, STARMAP_THRESHOLD_MULTIPLIER, MIN_STARMAP_THRESHOLD};
use crate::plugins::solar_system::{CelestialBody, LogicalParent};
use crate::plugins::solar_system_data::BodyType;
use crate::plugins::starmap::{HoveredStarSystem, SelectedStarSystem, StarSystemIcon, SystemMetadata};
use crate::research::{
    EngineeringProject, PendingResearchActions, ResearchProject, ResearchState, ResearchTeam, ResearchTeamCapacity,
    TechnologiesData, TechCategory, TechTreeEditState, TechEditData, ContextMenuState,
    Technology, ModifierType, TechModifierDef,
};
use crate::fleets::{
    ActiveManeuver, Fleet, FleetOrbit, MergeFleetAction, PendingFleetActions, PlannedTransfer,
    StartTransferAction, TransferOption, TransferWindowInfo,
    AU_IN_METERS, G_CONST, GM_SUN,
};
use crate::fleets::orbital_mechanics::{
    apply_thrust_limits, kinematic_transfer_options, calculate_transfer_options,
    calculate_transfer_options_phased, co_orbital_phasing_options, direct_lp_transfer_options,
    compute_burn_time_s, compute_transfer_window,
    find_gravity_assist_options, format_delta_v, format_duration,
    hohmann_transfer, GravityAssistOption,
};

/// Maximum time scale: 1 year per second (365.25 * 86400 ≈ 31,557,600)
const MAX_TIME_SCALE: f32 = 31_557_600.0;

/// Semi-major axis threshold (AU) below which a body's orbit is considered
/// non-heliocentric (e.g. a moon orbiting a planet rather than the star).
/// Used when walking up the hierarchy to find the heliocentric SMA.
const MIN_HELIOCENTRIC_SMA_AU: f64 = 0.05;

/// Minimum supported window dimensions to prevent UI overlap
/// Full HD (1920×1080) is required for the complex strategy game UI
const MIN_WINDOW_WIDTH: f32 = 1920.0;
const MIN_WINDOW_HEIGHT: f32 = 1080.0;

/// Resource to track if we should display the low resolution warning
#[derive(Resource, Default)]
pub struct ResolutionWarning {
    pub should_show: bool,
    pub dismissed: bool,
}

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

/// One of the five Lagrange equilibrium points of a planet–star system.
/// Used as a synthetic transfer destination (no ECS entity).
#[derive(Debug, Clone)]
pub struct LagrangeTarget {
    /// L-point index (1–5).
    pub point: u8,
    /// Parent planet entity whose L-points these are.
    pub planet_entity: Entity,
    /// Human-readable planet name.
    pub planet_name: String,
    /// Planet's heliocentric SMA in AU.
    pub planet_sma_au: f64,
    /// Effective heliocentric orbital radius of this L-point (AU).
    /// L1/L2: planet_sma ± r_hill; L3/L4/L5: approximately planet_sma.
    pub radius_au: f64,
    /// Gravitational parameter used for this transfer (GM of central star, m³ s⁻²).
    pub gm: f64,
}

impl LagrangeTarget {
    /// Short qualifier shown after the L-number in the UI.
    pub fn qualifier(&self) -> &'static str {
        match self.point {
            1 => "Inner",
            2 => "Outer",
            3 => "Opposition",
            4 => "Leading (+60°)",
            5 => "Trailing (-60°)",
            _ => "",
        }
    }
}

/// Pairs a [`GravityAssistOption`] (pure physics) with the ECS entity of the flyby
/// body, so the 3-D slingshot preview renderer can resolve screen coordinates.
#[derive(Debug, Clone)]
pub struct GravityAssistEntry {
    /// The computed gravity-assist trajectory data.
    pub option: GravityAssistOption,
    /// ECS entity for the flyby body (used by `draw_gravity_assist_preview`).
    pub flyby_entity: Entity,
}

/// Per-frame UI state for the Fleets panel.
///
/// Persists selected fleet and planned transfer between frames.
#[derive(Resource, Default)]
pub struct FleetUiState {
    /// Currently selected fleet entity in the list.
    pub selected_fleet: Option<Entity>,
    /// Target body chosen for transfer planning.
    pub target_body: Option<Entity>,
    /// Selected Lagrange-point target (mutually exclusive with `target_body`).
    pub target_lagrange: Option<LagrangeTarget>,
    /// Selected top-level category in the two-level destination selector.
    /// Holds the category label string (e.g. "Earth", "Mars", "Fleets").
    pub selected_dest_category: Option<String>,
    /// Fleet entity targeted for an intercept course.
    /// Mutually exclusive with `target_body` and `target_lagrange`.
    pub target_fleet: Option<Entity>,
    /// Desired passing distance for fleet intercepts (km). 0 = rendezvous.
    pub intercept_passing_km: f64,
    /// Desired encounter speed for fleet intercepts (m/s). 0 = match velocity.
    pub intercept_speed_ms: f64,
    /// Days from *now* until the fleet's planned departure (0 = depart immediately).
    /// Adjusted by the departure-time slider in the transfer planner.
    pub departure_offset_days: f64,
    /// Index into `computed_options` the player has highlighted.
    pub selected_option: usize,
    /// Transfer options computed for the current (fleet, target) pair.
    pub computed_options: Vec<TransferOption>,
    /// Fully assembled transfer plan ready for execution (if any).
    pub planned_transfer: Option<PlannedTransfer>,
    /// Whether the floating Transfer Planner popup window is open.
    pub show_transfer_popup: bool,
    /// Gravity-assist flyby candidates for the current heliocentric transfer.
    /// Recomputed every frame when a body target is selected.
    pub gravity_assist_candidates: Vec<GravityAssistEntry>,
    /// Index of the currently chosen gravity-assist candidate (`None` = direct transfer).
    pub selected_gravity_assist: Option<usize>,
    /// Interstellar target: (system_id, display_name, distance_ly).
    /// Mutually exclusive with `target_body`, `target_lagrange`, and `target_fleet`.
    pub target_star_system: Option<(usize, String, f32)>,
    /// Currently editing fleet name: (fleet_entity, new_name).
    pub editing_fleet_name: Option<(Entity, String)>,
    /// Multi-selected fleet entities for bulk operations (merge).
    pub selected_fleets: Vec<Entity>,
    /// Fleet pending disband confirmation popup.
    pub disband_confirm_fleet: Option<Entity>,
    /// Anchor for shift-range selection (the last plain-click entity).
    pub last_single_selected: Option<Entity>,
    /// Selected spawn location body for the "Create Fleet" picker.
    pub spawn_location_body: Option<Entity>,
}

impl FleetUiState {
    /// Clear all per-target state (transfer planning, rename, etc.).
    pub fn clear_target(&mut self) {
        self.target_body = None;
        self.target_lagrange = None;
        self.target_fleet = None;
        self.target_star_system = None;
        self.selected_dest_category = None;
        self.departure_offset_days = 0.0;
        self.computed_options.clear();
        self.planned_transfer = None;
        self.selected_option = 0;
        self.gravity_assist_candidates.clear();
        self.selected_gravity_assist = None;
        self.editing_fleet_name = None;
    }

    /// Clear multi-selection state.
    pub fn clear_multi_selection(&mut self) {
        self.selected_fleets.clear();
        self.last_single_selected = None;
    }
}

/// System sets for UI ordering. Avoids Bevy's tuple-complexity limit
/// by grouping systems into named sets instead of using `.chain()` on
/// large heterogeneous tuples.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
enum UiSystemSet {
    /// Resource bar & top menu (rendered first)
    TopBar,
    /// Dashboard, research, construction, economy panels
    MainPanels,
    /// Tooltips and floating overlays (rendered last)
    Overlays,
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
            if image.data.as_ref().unwrap().len() != (image.texture_descriptor.size.width as usize)
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
            for chunk in image.data.as_mut().unwrap().chunks_exact_mut(bytes_per_pixel) {
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
                let alpha = (1.0_f32 - luminance).powf(3.0); // Power curve to steepen the falloff
                
                // Premultiply alpha: bevy_egui 0.39.1+ no longer premultiplies
                // in the shader, so textures must store premultiplied values.
                // Since base colour is pure white (1.0), premultiplied RGB = alpha.
                let a = alpha.clamp(0.0, 1.0);
                let pa = (a * 255.0) as u8;
                chunk[0] = pa;
                chunk[1] = pa;
                chunk[2] = pa;
                chunk[3] = pa;
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
            if image.data.as_ref().unwrap().len() != (image.texture_descriptor.size.width as usize)
                .saturating_mul(image.texture_descriptor.size.height as usize)
                .saturating_mul(bytes_per_pixel)
            {
                icons.processed.insert(category);
                continue;
            }

            for chunk in image.data.as_mut().unwrap().chunks_exact_mut(bytes_per_pixel) {
                let r = chunk[0] as f32 / 255.0;
                let g = chunk[1] as f32 / 255.0;
                let b = chunk[2] as f32 / 255.0;
                let luminance = 0.299 * r + 0.587 * g + 0.114 * b;
                let alpha = (1.0_f32 - luminance).powf(3.0);

                // Premultiply alpha: bevy_egui 0.39.1+ no longer premultiplies
                // in the shader, so textures must store premultiplied values.
                let a = alpha.clamp(0.0, 1.0);
                let pa = (a * 255.0) as u8;
                chunk[0] = pa;
                chunk[1] = pa;
                chunk[2] = pa;
                chunk[3] = pa;
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

fn format_timestamp_date_time(timestamp: i64) -> String {
    let total_days = timestamp / 86400;
    let time_of_day = timestamp % 86400;

    let hours = (time_of_day / 3600) % 24;
    let minutes = (time_of_day % 3600) / 60;

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

    let day = days_remaining + 1;

    format!("{:02}.{:02}.{} {:02}:{:02}", day, month, year, hours, minutes)
}

fn estimate_research_project_end_timestamp(
    project: &ResearchProject,
    team: Option<&ResearchTeam>,
    technologies: &TechnologiesData,
    research_state: &ResearchState,
    total_allocation: f64,
    current_timestamp: i64,
) -> Option<i64> {
    if project.progress >= project.required_points {
        return Some(current_timestamp);
    }

    if !project.active || project.rp_allocation_percent <= 0.0 || total_allocation <= 0.0 {
        return None;
    }

    let base_rate = research_state.rp_rate_per_second * (project.rp_allocation_percent / total_allocation);
    if base_rate <= 0.0 {
        return None;
    }

    let technology = technologies.technologies.get(&project.tech_id);
    let category_bonus = technology
        .map(|tech| 1.0 + (research_state.category_research_bonus(tech.category) / 100.0))
        .unwrap_or(1.0);

    let team_efficiency = technology
        .map(|tech| team.map(|entry| entry.category_efficiency(tech.category) as f64).unwrap_or(1.0))
        .unwrap_or(1.0);

    let effective_rate = base_rate * category_bonus * team_efficiency;
    if effective_rate <= 0.0 {
        return None;
    }

    let remaining_points = (project.required_points - project.progress).max(0.0);
    let eta_seconds = remaining_points / effective_rate;
    if !eta_seconds.is_finite() {
        return None;
    }

    Some(current_timestamp + eta_seconds.ceil() as i64)
}

fn estimate_engineering_project_end_timestamp(
    project: &EngineeringProject,
    team: Option<&ResearchTeam>,
    research_state: &ResearchState,
    current_timestamp: i64,
) -> Option<i64> {
    if project.progress >= project.required_points {
        return Some(current_timestamp);
    }

    let team_efficiency = team.map(|entry| entry.efficiency as f64).unwrap_or(1.0);
    let effective_rate = team_efficiency * research_state.engineering_speed_multiplier();
    if effective_rate <= 0.0 {
        return None;
    }

    let remaining_points = (project.required_points - project.progress).max(0.0);
    let eta_seconds = remaining_points / effective_rate;
    if !eta_seconds.is_finite() {
        return None;
    }

    Some(current_timestamp + eta_seconds.ceil() as i64)
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

/// Setup custom fonts for better Unicode and emoji/icon support
/// 
/// Font Stack:
/// - **Inter** (Regular/SemiBold/Bold): Primary UI font with excellent Unicode coverage
/// - **GeistMono** (Medium): Monospace font for numbers, code, and resource rates
/// - **Hack Nerd Font**: Fallback for developer icons and special symbols
/// - **Noto Emoji**: Broad monochrome emoji coverage (Unicode 15+)
/// - **Noto Sans Symbols 2**: Astronomical, geometric, and miscellaneous symbols
fn setup_egui_fonts(mut contexts: EguiContexts) {
    let mut fonts = egui::FontDefinitions::default();
    
    // Load primary fonts
    fonts.font_data.insert(
        "Inter-Regular".to_owned(),
        egui::FontData::from_static(include_bytes!("../../assets/fonts/Inter-Regular.otf")).into(),
    );
    fonts.font_data.insert(
        "Inter-SemiBold".to_owned(),
        egui::FontData::from_static(include_bytes!("../../assets/fonts/Inter-SemiBold.otf")).into(),
    );
    fonts.font_data.insert(
        "Inter-Bold".to_owned(),
        egui::FontData::from_static(include_bytes!("../../assets/fonts/Inter-Bold.otf")).into(),
    );
    fonts.font_data.insert(
        "GeistMono-Medium".to_owned(),
        egui::FontData::from_static(include_bytes!("../../assets/fonts/GeistMono-Medium.ttf")).into(),
    );
    fonts.font_data.insert(
        "HackNerdFont".to_owned(),
        egui::FontData::from_static(include_bytes!("../../assets/fonts/HackNerdFont-Regular.ttf")).into(),
    );
    // Hubot Sans for Headers
    fonts.font_data.insert(
        "Hubot-Sans-ExtraBoldExpanded".to_owned(),
        egui::FontData::from_static(include_bytes!("../../assets/fonts/Hubot-Sans-ExtraBoldExpanded.ttf")).into(),
    );
    fonts.font_data.insert(
        "Hubot-Sans-SemiBoldCondensed".to_owned(),
        egui::FontData::from_static(include_bytes!("../../assets/fonts/Hubot-Sans-SemiBoldCondensed.ttf")).into(),
    );
    // Noto Emoji for broad monochrome emoji coverage (Unicode 15+)
    fonts.font_data.insert(
        "NotoEmoji".to_owned(),
        egui::FontData::from_static(include_bytes!("../../assets/fonts/NotoEmoji-Regular.ttf")).into(),
    );
    // Noto Sans Symbols 2 for astronomical (☉), geometric, and misc symbols
    fonts.font_data.insert(
        "NotoSansSymbols2".to_owned(),
        egui::FontData::from_static(include_bytes!("../../assets/fonts/NotoSansSymbols2-Regular.ttf")).into(),
    );

    // Setup font families with Inter as primary, HackNerdFont as fallback for icons
    // Added "emoji-icon-font" (default egui emoji font) to fix broken emojis
    fonts.families.insert(
        egui::FontFamily::Proportional,
        vec![
            "Inter-Regular".to_owned(),
            "HackNerdFont".to_owned(), // Fallback for developer icons
            "NotoEmoji".to_owned(),    // Broad emoji coverage
            "NotoSansSymbols2".to_owned(), // Astronomical & geometric symbols
            "emoji-icon-font".to_owned(), // egui built-in (last resort)
        ],
    );

    fonts.families.insert(
        egui::FontFamily::Monospace,
        vec![
            "GeistMono-Medium".to_owned(),
            "HackNerdFont".to_owned(), // Fallback for developer icons
            "NotoEmoji".to_owned(),
            "NotoSansSymbols2".to_owned(),
            "emoji-icon-font".to_owned(),
        ],
    );

    // Define custom font families for headers
    // "heading" -> Game Title (Hubot Sans Extra Bold Expanded)
    fonts.families.insert(
        egui::FontFamily::Name("heading".into()),
        vec![
            "Hubot-Sans-ExtraBoldExpanded".to_owned(),
            "HackNerdFont".to_owned(),
            "NotoEmoji".to_owned(),
            "NotoSansSymbols2".to_owned(),
            "emoji-icon-font".to_owned(),
        ],
    );

    // "semibold" -> Window/Menu Headers (Hubot Sans SemiBold Condensed)
    fonts.families.insert(
        egui::FontFamily::Name("semibold".into()),
        vec![
            "Hubot-Sans-SemiBoldCondensed".to_owned(),
            "HackNerdFont".to_owned(),
            "NotoEmoji".to_owned(),
            "NotoSansSymbols2".to_owned(),
            "emoji-icon-font".to_owned(),
        ],
    );
        
    if let Ok(ctx) = contexts.ctx_mut() {
        ctx.set_fonts(fonts);
    }
}

impl Plugin for UIPlugin {
    fn build(&self, app: &mut App) {
        app
            // Egui plugin is added in `main.rs` (explicit bevy_egui integration)
            // Resources
            .init_resource::<Selection>()
            .init_resource::<TimeScale>()
            .init_resource::<SimulationTime>()
            .init_resource::<ResearchUiPreferences>()
            .init_resource::<FleetUiState>()
            .init_resource::<ResolutionWarning>()
            // ActiveMenu is now initialized in GameStatePlugin
            // to allow access in camera/starmap plugins
            // Load menu icons at startup
            .add_systems(Startup, (load_menu_icons, load_research_icons, setup_egui_fonts, check_window_resolution))
            // UI rendering systems
            // Ordered sequence to ensure correct layout stacking:
            // 1. Top bars (Resources -> Menu)
            // 2. Main content panels (Dashboard / Research)
            // 3. Floating overlays (Tooltips)
            //
            // Uses UiSystemSet to avoid Bevy's tuple type-complexity limit.
            .configure_sets(
                EguiPrimaryContextPass,
                (
                    UiSystemSet::TopBar,
                    UiSystemSet::MainPanels,
                    UiSystemSet::Overlays,
                )
                    .chain(),
            )
            .add_systems(
                EguiPrimaryContextPass,
                (ui_resources_bar, ui_top_menu_bar, ui_time_controls)
                    .chain()
                    .in_set(UiSystemSet::TopBar),
            )
            .add_systems(EguiPrimaryContextPass, ui_dashboard.in_set(UiSystemSet::MainPanels))
            .add_systems(EguiPrimaryContextPass, ui_research_panels.in_set(UiSystemSet::MainPanels))
            .add_systems(EguiPrimaryContextPass, ui_construction_panels.in_set(UiSystemSet::MainPanels))
            .add_systems(EguiPrimaryContextPass, ui_economy_panels.in_set(UiSystemSet::MainPanels))
            .add_systems(EguiPrimaryContextPass, ui_fleets_panel.in_set(UiSystemSet::MainPanels))
            .add_systems(EguiPrimaryContextPass, ui_fleet_action_bar.in_set(UiSystemSet::MainPanels))
            .add_systems(
                EguiPrimaryContextPass,
                (
                    ui_hover_tooltip,
                    ui_starmap_hover_tooltip,
                    ui_starmap_labels,
                    ui_resolution_warning,
                    ui_transfer_planner_popup,
                    ui_lp_click_handler,
                )
                    .in_set(UiSystemSet::Overlays),
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
            )
            // Capture egui's available_rect AFTER all panels have registered themselves
            // this frame. The camera system reads this next frame to detect panel bounds.
            // Must run in Update (inside egui's frame), not PostUpdate (context is closed).
            .add_systems(
                EguiPrimaryContextPass,
                capture_egui_panel_bounds.after(UiSystemSet::Overlays),
            );
    }
} 

/// System that syncs the UI selection with the astronomy Selected component
fn sync_selection_with_astronomy(
    mut selection: ResMut<Selection>,
    selected_query: Query<Entity, (With<Selected>, With<CelestialBody>)>,
) {
    // If something is selected in astronomy, update UI selection
    if let Ok(entity) = selected_query.single() {
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
    let real_delta = real_time.delta_secs_f64();
    sim_time.elapsed += real_delta * time_scale.scale as f64;
}

/// Get the icon for a resource category
fn get_resource_category_icon(category: &str) -> &'static str {
    match category {
        "Volatiles" => "\u{1F4A7}",          // 💧
        "Atmospheric Gases" => "\u{2601}",   // ☁
        "Construction" => "\u{1F9F1}",       // 🧱
        "Fusion Fuel" => "\u{1F50B}",        // 🔋
        "Fissiles" => "\u{2622}",            // ☢
        "Precious Metals" => "\u{1F48E}",    // 💎
        "Specialty" => "\u{2728}",           // ✨
        _ => "\u{1F4E6}",                    // 📦
    }
}

/// Get the icon for a specific resource type
fn get_resource_icon(resource: &ResourceType) -> &'static str {
    match resource {
        // Volatiles
        ResourceType::Water => "\u{1F4A7}",          // 💧
        ResourceType::Hydrogen => "\u{1F388}",       // 🎈
        ResourceType::Ammonia => "\u{1F9FC}",        // 🧼
        ResourceType::Methane => "\u{1F525}",        // 🔥
        
        // Atmospheric
        ResourceType::Nitrogen => "\u{1F32C}",       // 🌬
        ResourceType::Oxygen => "\u{1F4A8}",         // 💨
        ResourceType::CarbonDioxide => "\u{1F32B}",  // 🌫
        ResourceType::Argon => "\u{1F7E3}",          // 🟣
        
        // Construction
        ResourceType::Iron => "\u{1F529}",           // 🔩
        ResourceType::Aluminum => "\u{2708}",        // ✈
        ResourceType::Titanium => "\u{1F6E1}",       // 🛡
        ResourceType::Silicates => "\u{1FAA8}",      // 🪨
        
        // Energy
        ResourceType::Helium3 => "\u{2600}",         // ☀
        
        // Fissiles
        ResourceType::Uranium => "\u{2622}",         // ☢
        ResourceType::Thorium => "\u{26A1}",         // ⚡

        // Precious
        ResourceType::Gold => "\u{1F451}",           // 👑
        ResourceType::Silver => "\u{1F948}",         // 🥈
        ResourceType::Platinum => "\u{1F48D}",       // 💍

        // Specialty
        ResourceType::Copper => "\u{1F50C}",         // 🔌
        ResourceType::RareEarths => "\u{1F4F1}",     // 📱
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
    mut pending_research: ResMut<PendingResearchActions>,
    budget: Res<GlobalBudget>,
    rate_tracker: Res<ResourceRateTracker>,
    research_state: Res<ResearchState>,
    population_query: Query<(&Population, Option<&crate::plugins::solar_system::CelestialBody>)>,
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
                    let category_rate: f64 =
                        resources.iter().map(|r| rate_tracker.get_resource_rate(r)).sum();

                    let icon = get_resource_category_icon(category_name);
                    let color = get_category_color(category_name);
                    let text_color = egui::Color32::from_rgb(220, 220, 220);

                    let is_this_open = open_popup.open.as_ref().map_or(false, |(n, _)| n == category_name);

                    // Use a Frame for the category display
                    let response = egui::Frame::NONE
                        .inner_margin(egui::Margin::symmetric(3, 2))
                        .show(ui, |ui| {
                            ui.horizontal_centered(|ui| {
                                ui.add(egui::Label::new(egui::RichText::new(icon).size(20.0).color(color)).selectable(false));
                                ui.add_space(1.0);
                                ui.vertical(|ui| {
                                    ui.set_min_width(72.0);  // Fixed width to prevent wiggling
                                    ui.set_max_width(72.0);
                                    ui.add(egui::Label::new(egui::RichText::new(format_mass(category_total)).size(14.0).color(text_color)).selectable(false));
                                    let (rate_text, rate_color) = format_rate_monthly(category_rate);
                                    ui.add(egui::Label::new(egui::RichText::new(rate_text).size(10.0).color(rate_color)).selectable(false));
                                });
                            });
                        }).response;

                    let interact = response.interact(egui::Sense::click());

                    // Hover and open-state border effect
                    if interact.hovered() || is_this_open {
                        ui.painter().rect_stroke(interact.rect, 2.0, egui::Stroke::new(1.0, color), egui::StrokeKind::Outside);
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

                    ui.add_space(8.0);
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
                        (time.elapsed_secs() * 5.0).sin().abs() as f32
                    } else {
                        0.0
                    };
                    
                    let border_color = if flash > 0.5 { warning_color } else { egui::Color32::TRANSPARENT };

                    let response = egui::Frame::NONE
                        .inner_margin(egui::Margin::symmetric(3, 2))
                        .stroke(egui::Stroke::new(if flash > 0.0 { 2.0 } else { 0.0 }, border_color))
                        .show(ui, |ui| {
                            ui.horizontal_centered(|ui| {
                                ui.add(egui::Label::new(egui::RichText::new("🔬").size(20.0).color(rp_color)).selectable(false));
                                ui.add_space(1.0);
                                ui.vertical(|ui| {
                                    ui.set_min_width(115.0);  // Fixed width to prevent wiggling
                                    ui.set_max_width(115.0);
                                    
                                    if let Some(project) = furthest_rp {
                                        if let Some(tech) = technologies.technologies.get(&project.tech_id) {
                                            ui.add(egui::Label::new(egui::RichText::new(&tech.name).size(12.0).color(text_color)).selectable(false));
                                            
                                            let progress_fraction = (project.progress / project.required_points).clamp(0.0, 1.0) as f32;
                                            ui.add(egui::ProgressBar::new(progress_fraction)
                                                .desired_width(100.0)
                                                .desired_height(4.0)
                                                .fill(egui::Color32::from_rgb(50, 150, 255)));
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
                        ui.painter().rect_stroke(interact.rect, 2.0, egui::Stroke::new(1.0, rp_color), egui::StrokeKind::Outside);
                        interact.clone().on_hover_cursor(egui::CursorIcon::PointingHand);
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
                        (time.elapsed_secs() * 5.0).sin().abs() as f32
                    } else {
                        0.0
                    };
                    
                    let border_color = if flash > 0.5 { warning_color } else { egui::Color32::TRANSPARENT };

                    let response = egui::Frame::NONE
                        .inner_margin(egui::Margin::symmetric(3, 2))
                        .stroke(egui::Stroke::new(if flash > 0.0 { 2.0 } else { 0.0 }, border_color))
                        .show(ui, |ui| {
                            ui.horizontal_centered(|ui| {
                                ui.add(egui::Label::new(egui::RichText::new("⚙").size(20.0).color(ep_color)).selectable(false));
                                ui.add_space(1.0);
                                ui.vertical(|ui| {
                                    ui.set_min_width(115.0);  // Fixed width to prevent wiggling
                                    ui.set_max_width(115.0);
                                    
                                    if let Some(project) = furthest_ep {
                                        let name = technologies.components.get(&project.component_id).map(|c| c.name.as_str()).unwrap_or("Unknown Component");
                                        ui.add(egui::Label::new(egui::RichText::new(name).size(12.0).color(text_color)).selectable(false));
                                        
                                        let progress_fraction = (project.progress / project.required_points).clamp(0.0, 1.0) as f32;
                                        ui.add(egui::ProgressBar::new(progress_fraction)
                                            .desired_width(100.0)
                                            .desired_height(4.0)
                                            .fill(egui::Color32::from_rgb(50, 150, 255)));

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
                        ui.painter().rect_stroke(interact.rect, 2.0, egui::Stroke::new(1.0, ep_color), egui::StrokeKind::Outside);
                        interact.clone().on_hover_cursor(egui::CursorIcon::PointingHand);
                    }
                    
                    if interact.double_clicked() {
                        pending_research.navigate_to_available_engineering_tab = true;
                        open_popup.open = None;
                    } else if interact.clicked() {
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
                    let response = egui::Frame::NONE
                        .inner_margin(egui::Margin::symmetric(3, 2))
                        .show(ui, |ui| {
                            ui.horizontal_centered(|ui| {
                                ui.set_min_width(82.0);  // Fixed width to prevent wiggling
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
                        ui.painter()
                            .rect_stroke(interact.rect, 2.0, egui::Stroke::new(1.0, power_color), egui::StrokeKind::Outside);
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

                    let treasury_response = egui::Frame::NONE
                        .inner_margin(egui::Margin::symmetric(3, 2))
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
                                ui.add_space(1.0);
                                ui.scope(|ui| {
                                    // Fixed width to prevent layout issues in right-to-left container
                                    ui.set_min_width(90.0);
                                    ui.set_max_width(90.0);
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
                        .map_or(false, |(n, _)| n == "Population");

                    // Use a Frame for the population display
                    let pop_response = egui::Frame::NONE
                        .inner_margin(egui::Margin::symmetric(3, 2))
                        .show(ui, |ui| {
                            ui.horizontal_centered(|ui| {
                                ui.set_min_width(68.0);  // Fixed width to prevent wiggling
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
                    
                    let mut active_rps: Vec<_> = research_projects.iter().filter(|p| p.active).collect();
                    active_rps.sort_by(|a, b| {
                        let a_progress = if a.required_points > 0.0 { a.progress / a.required_points } else { 1.0 };
                        let b_progress = if b.required_points > 0.0 { b.progress / b.required_points } else { 1.0 };
                        b_progress.partial_cmp(&a_progress).unwrap_or(std::cmp::Ordering::Equal)
                    });

                    let total_allocation: f64 = active_rps
                        .iter()
                        .filter(|project| project.required_points > project.progress && project.rp_allocation_percent > 0.0)
                        .map(|project| project.rp_allocation_percent)
                        .sum();

                    if active_rps.is_empty() {
                        ui.add(egui::Label::new("No active research projects.").selectable(false));
                    } else {
                        for project in &active_rps {
                            if let Some(tech) = technologies.technologies.get(&project.tech_id) {
                                let progress = if project.required_points > 0.0 {
                                    (project.progress / project.required_points * 100.0).clamp(0.0, 100.0)
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
                                ui.add(egui::Label::new(egui::RichText::new(format!("  {}", end_date_text)).size(10.0).color(egui::Color32::GRAY)).selectable(false));
                                ui.add(egui::ProgressBar::new((progress / 100.0) as f32).desired_width(220.0));
                                ui.add_space(4.0);
                            }
                        }
                    }
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
                    
                    let mut active_eps: Vec<_> = engineering_projects.iter().collect();
                    active_eps.sort_by(|a, b| {
                        let a_progress = if a.required_points > 0.0 { a.progress / a.required_points } else { 1.0 };
                        let b_progress = if b.required_points > 0.0 { b.progress / b.required_points } else { 1.0 };
                        b_progress.partial_cmp(&a_progress).unwrap_or(std::cmp::Ordering::Equal)
                    });

                    if active_eps.is_empty() {
                        ui.add(egui::Label::new("No active engineering projects.").selectable(false));
                    } else {
                        for project in &active_eps {
                            let name = technologies.components.get(&project.component_id).map(|c| c.name.as_str()).unwrap_or("Unknown Component");
                            let progress = if project.required_points > 0.0 {
                                (project.progress / project.required_points * 100.0).clamp(0.0, 100.0)
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
                            ui.add(egui::Label::new(egui::RichText::new(format!("  {}", end_date_text)).size(10.0).color(egui::Color32::GRAY)).selectable(false));
                            ui.add(egui::ProgressBar::new((progress / 100.0) as f32).desired_width(220.0));
                            ui.add_space(4.0);
                        }
                    }
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
    pending_research: Res<PendingResearchActions>,
    menu_icons: Option<Res<MenuIcons>>,
    mut icon_textures: Local<HashMap<GameMenu, egui::TextureId>>,
    current_system: Res<CurrentStarSystem>,
    system_metadata: Res<SystemMetadata>,
    mut camera_query: Query<(&mut OrbitCamera, &mut CameraAnchor), With<GameCamera>>,
    star_icon_query: Query<(Entity, Option<&SelectedStarSystem>), With<StarSystemIcon>>,
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
                    .or_insert_with(|| contexts.add_image(bevy_egui::EguiTextureHandle::Strong(handle.clone())));
            }
            // Clone the cached map so the rest of the UI code can use an owned
            // HashMap just like before.
            Some(icon_textures.clone())
        } else {
            None
        };

    // Pre-compute the camera radius needed to be comfortably in starmap view.
    // This is 1.5× the entry threshold so the camera is clearly above it and
    // `update_view_mode` won't immediately revert back to System.
    let starmap_radius = {
        let bounding_radius_au = system_metadata.get_bounding_radius(current_system.0);
        let base_threshold = (bounding_radius_au
            * crate::astronomy::SCALING_FACTOR as f64
            * STARMAP_THRESHOLD_MULTIPLIER as f64) as f32;
        base_threshold.max(MIN_STARMAP_THRESHOLD) * 1.5
    };

    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    if pending_research.navigate_to_available_tab
        || pending_research.navigate_to_available_engineering_tab
    {
        active_menu.current = GameMenu::Research;
        *view_mode = ViewMode::System;
    }

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
                            
                            let resp = ui.add(egui::Button::image(img));

                            // Highlight active menu by drawing a subtle stroke around the widget
                            if is_active {
                                let rect = resp.rect;
                                ui.painter().rect_stroke(rect, 4.0, egui::Stroke::new(2.0, egui::Color32::from_rgb(100, 200, 255)), egui::StrokeKind::Outside);
                            }

                            let resp = resp.on_hover_text(menu.name());
                            if resp.clicked() {
                                active_menu.current = menu;
                                match menu {
                                    GameMenu::Starmap => {
                                        *view_mode = ViewMode::Starmap;
                                        if let Ok((mut orbit, mut anchor)) = camera_query.single_mut() {
                                            orbit.radius = starmap_radius;
                                            orbit.target_center = Vec3::ZERO;
                                            anchor.0 = None;
                                        }
                                    }
                                    GameMenu::Survey => {
                                        *view_mode = ViewMode::System;
                                        if let Ok((mut orbit, mut anchor)) = camera_query.single_mut() {
                                            // If not anchored, try anchoring to the selected star system
                                            if anchor.0.is_none() {
                                                if let Some((sel_entity, _)) = star_icon_query.iter().find(|(_, sel)| sel.is_some()) {
                                                    anchor.0 = Some(sel_entity);
                                                }
                                            }
                                            // Zoom into system-view range (mirrors double-click behaviour)
                                            orbit.radius = 150_000.0;
                                        }
                                    }
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
                                    GameMenu::Starmap => {
                                        *view_mode = ViewMode::Starmap;
                                        if let Ok((mut orbit, mut anchor)) = camera_query.single_mut() {
                                            orbit.radius = starmap_radius;
                                            orbit.target_center = Vec3::ZERO;
                                            anchor.0 = None;
                                        }
                                    }
                                    GameMenu::Survey => {
                                        *view_mode = ViewMode::System;
                                        if let Ok((mut orbit, mut anchor)) = camera_query.single_mut() {
                                            if anchor.0.is_none() {
                                                if let Some((sel_entity, _)) = star_icon_query.iter().find(|(_, sel)| sel.is_some()) {
                                                    anchor.0 = Some(sel_entity);
                                                }
                                            }
                                            orbit.radius = 150_000.0;
                                        }
                                    }
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
                                GameMenu::Starmap => {
                                    *view_mode = ViewMode::Starmap;
                                    if let Ok((mut orbit, mut anchor)) = camera_query.single_mut() {
                                        orbit.radius = starmap_radius;
                                        orbit.target_center = Vec3::ZERO;
                                        anchor.0 = None;
                                    }
                                }
                                GameMenu::Survey => {
                                    *view_mode = ViewMode::System;
                                    if let Ok((mut orbit, mut anchor)) = camera_query.single_mut() {
                                        if anchor.0.is_none() {
                                            if let Some((sel_entity, _)) = star_icon_query.iter().find(|(_, sel)| sel.is_some()) {
                                                anchor.0 = Some(sel_entity);
                                            }
                                        }
                                        orbit.radius = 150_000.0;
                                    }
                                }
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

    let Ok((camera, camera_transform)) = camera_query.single() else {
        return;
    };

    // Use ctx_mut to safely handle context access
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    // Get the available screen rect (excludes all anchored side/top/bottom panels).
    // Using a Painter with this as the clip rect guarantees labels never bleed
    // through panels, regardless of text width or floating area render order.
    let available_rect = ctx.available_rect();

    // Create a painter clipped strictly to the panel-free area.
    // Order::Background keeps labels beneath floating windows/tooltips.
    let mut painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Background,
        egui::Id::new("starmap_labels"),
    ));
    painter.set_clip_rect(available_rect);

    let font_id = egui::FontId::proportional(14.0);

    for (icon_transform, icon, is_selected) in icon_query.iter() {
        let icon_pos = icon_transform.translation();

        // Project 3D position to screen space
        if let Ok(screen_pos) = camera.world_to_viewport(camera_transform, icon_pos) {
            let label_pos = egui::pos2(screen_pos.x + 20.0, screen_pos.y - 10.0);

            // Skip if the anchor is clearly off-screen (painter clip handles edge overflow)
            if !available_rect.expand(200.0).contains(label_pos) {
                continue;
            }

            let color = if is_selected.is_some() {
                egui::Color32::from_rgb(100, 200, 255) // Bright blue for selected
            } else {
                egui::Color32::from_rgb(200, 200, 200) // Light gray for others
            };

            painter.text(
                label_pos,
                egui::Align2::LEFT_TOP,
                &icon.name,
                font_id.clone(),
                color,
            );
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

/// Returns a Unicode icon for each body type to distinguish entries in the ledger
fn body_type_icon(body_type: &BodyType) -> &'static str {
    match body_type {
        BodyType::Star => "\u{2605}",       // ★
        BodyType::Planet => "\u{25CF}",     // ●
        BodyType::Moon => "\u{25D1}",       // ◑
        BodyType::DwarfPlanet => "\u{25CC}", // ◌
        BodyType::Asteroid => "\u{25C6}",   // ◆
        BodyType::Comet => "\u{2604}",      // ☄
        BodyType::GasGiant => "\u{25C9}",   // ◉
        BodyType::Ring => "\u{25CB}",       // ○
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
    let type_icon = body_type_icon(&body.body_type);
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
            if let Ok(mut anchor) = anchor_query.single_mut() {
                anchor.0 = Some(entity);
            }
        }

        // Use a visually distinct style for selected items
        let display_name = format!("{} {}", type_icon, body.name);
        if render_selectable_label(ui, is_selected, &display_name).clicked() {
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
    hierarchy: &std::collections::HashMap<Entity, Vec<Entity>>,
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
                // Use render_body_tree so bodies with children (e.g. Pluto → Charon)
                // are expanded recursively rather than shown as a flat row.
                render_body_tree(
                    ui,
                    child_entity,
                    body_map,
                    hierarchy,
                    selection,
                    commands,
                    selected_query,
                    anchor_query,
                );
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
        let mut child_rings = Vec::new();
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
                        BodyType::Ring => child_rings.push(child),
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
                body.body_type == BodyType::Star,
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
                    if let Ok(mut anchor) = anchor_query.single_mut() {
                        anchor.0 = Some(entity);
                    }
                }

                // Use a visually distinct style for selected items
                let type_icon = body_type_icon(&body.body_type);
                let display_name = format!("{} {}", type_icon, body.name);
                if render_selectable_label(ui, is_selected, &display_name).clicked() {
                    for e in selected_query.iter() {
                        commands.entity(e).remove::<Selected>();
                    }
                    commands.entity(entity).insert(Selected);
                    selection.select(entity);
                }
            })
            .body(|ui| {
                // 0. Rings — shown first so they are never buried under 46+ moons
                for child in child_rings {
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
                // 2. Dwarf Planets (Grouped, recursive so moons like Charon are shown)
                render_grouped_children(
                    ui,
                    &child_dwarf_planets,
                    "Dwarf Planets",
                    entity,
                    body_map,
                    hierarchy,
                    selection,
                    commands,
                    selected_query,
                    anchor_query,
                );
                // 3. Moons — listed directly under the parent (no collapsible group)
                for child in child_moons {
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
                // 4. Asteroids
                render_grouped_children(
                    ui,
                    &child_asteroids,
                    "Asteroids",
                    entity,
                    body_map,
                    hierarchy,
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
                    hierarchy,
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

/// Render fleet rows inside the left ledger "Fleets" collapsible section.
///
/// Each row shows status icon, fleet name, and current location.
/// Clicking a row selects/deselects the fleet and clears any body selection.
#[allow(clippy::too_many_arguments)]
fn render_fleet_ledger_tree(
    ui: &mut egui::Ui,
    fleet_query: &Query<(Entity, &Fleet, Option<&FleetOrbit>, Option<&ActiveManeuver>)>,
    body_map: &std::collections::HashMap<Entity, &CelestialBody>,
    fleet_ui_state: &mut FleetUiState,
    selected_query: &Query<Entity, With<Selected>>,
    commands: &mut Commands,
    selection: &mut Selection,
    elapsed: f64,
    anchor_query: &mut Query<&mut CameraAnchor, With<GameCamera>>,
) {
    let mut fleets: Vec<(Entity, &Fleet, Option<&FleetOrbit>, Option<&ActiveManeuver>)> =
        fleet_query.iter().map(|(e, f, o, m)| (e, f, o, m)).collect();
    fleets.sort_by(|a, b| a.1.name.cmp(&b.1.name));

    if fleets.is_empty() {
        ui.label(
            egui::RichText::new("  No fleets deployed")
                .size(12.0)
                .color(egui::Color32::GRAY)
                .italics(),
        );
        return;
    }

    for (entity, fleet, maybe_orbit, maybe_maneuver) in fleets {
        let is_selected = fleet_ui_state.selected_fleet == Some(entity);

        let status_icon = if maybe_maneuver.is_some() { "✈" } else { "🛰" };
        let display_name = format!("{} {}", status_icon, fleet.name);

        let row_color = if is_selected {
            egui::Color32::from_rgb(100, 220, 100)
        } else {
            egui::Color32::from_rgb(170, 200, 170)
        };

        let sub_status = if let Some(maneuver) = maybe_maneuver {
            if elapsed < maneuver.departure_time {
                "⏳ Waiting to depart".to_string()
            } else {
                "↗ In transit".to_string()
            }
        } else if let Some(orbit) = maybe_orbit {
            let body = body_map.get(&orbit.body);
            let body_name = body.map(|b| b.name.as_str()).unwrap_or("?");
            // Show a distinct label for heliocentric Lagrange-point orbits.
            if body.map(|b| b.body_type) == Some(BodyType::Star) {
                format!("✦ Lagrange Orbit ({body_name})")
            } else {
                format!("⊙ Orbiting {body_name}")
            }
        } else {
            "Location unknown".to_string()
        };

        let ships_txt = format!(
            "{} {}",
            fleet.ships.len(),
            if fleet.ships.len() == 1 { "ship" } else { "ships" }
        );

        let row_response = ui.selectable_label(
            is_selected,
            egui::RichText::new(&display_name).color(row_color).size(13.0),
        );

        if row_response.clicked() {
            if is_selected {
                fleet_ui_state.selected_fleet = None;
            } else {
                // Clear body selection
                for e in selected_query.iter() {
                    commands.entity(e).remove::<Selected>();
                }
                selection.clear();
                fleet_ui_state.selected_fleet = Some(entity);
                fleet_ui_state.clear_target();
            }
        }

        if row_response.double_clicked() {
            // Select the fleet
            for e in selected_query.iter() {
                commands.entity(e).remove::<Selected>();
            }
            selection.clear();
            fleet_ui_state.selected_fleet = Some(entity);
            fleet_ui_state.clear_target();

            // Anchor camera to the fleet's current or departure body
            let anchor_body = if let Some(maneuver) = maybe_maneuver {
                // In transit: anchor to departure (origin) body
                Some(maneuver.origin_body)
            } else if let Some(orbit) = maybe_orbit {
                // Orbiting: anchor to that body
                Some(orbit.body)
            } else {
                None
            };

            if let Some(body_entity) = anchor_body {
                if let Ok(mut anchor) = anchor_query.single_mut() {
                    anchor.0 = Some(body_entity);
                }
            }
        }

        // Sub-status line
        ui.horizontal(|ui| {
            ui.add_space(18.0);
            ui.label(
                egui::RichText::new(format!("{sub_status}  {ships_txt}"))
                    .size(10.0)
                    .color(egui::Color32::GRAY),
            );
        });
    }
}

/// System that displays a tooltip for hovered celestial bodies or Lagrange points
fn ui_hover_tooltip(
    mut contexts: EguiContexts,
    hovered_query: Query<&CelestialBody, With<Hovered>>,
    lp_markers: Res<LagrangePointMarkers>,
    active_menu: Res<ActiveMenu>,
) {
    // Don't show world tooltips when a full-screen overlay is active
    if active_menu.current.blocks_world_interaction() {
        return;
    }

    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    // Helper: LP qualifier label
    let lp_qualifier = |point: u8| -> &'static str {
        match point {
            1 => "Inner",
            2 => "Outer",
            3 => "Opposition",
            4 => "Leading (+60\u{00b0})",
            5 => "Trailing (-60\u{00b0})",
            _ => "",
        }
    };

    // LP hover takes priority: show LP tooltip when a Lagrange point is hovered.
    if let Some(idx) = lp_markers.hovered_index {
        if let Some(m) = lp_markers.markers.get(idx) {
            let available_rect = ctx.available_rect();
            let tooltip_pos = ctx
                .input(|i| i.pointer.hover_pos())
                .map(|p| egui::pos2(p.x + 12.0, p.y + 12.0))
                .unwrap_or(egui::pos2(100.0, 100.0));

            egui::Area::new("lp_hover_tooltip".into())
                .fixed_pos(tooltip_pos)
                .interactable(false)
                .order(egui::Order::Tooltip)
                .constrain_to(available_rect)
                .show(ctx, |ui| {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                    egui::Frame::NONE
                        .fill(egui::Color32::from_rgba_unmultiplied(20, 25, 35, 240))
                        .stroke(egui::Stroke::new(2.0, egui::Color32::from_rgb(80, 180, 255)))
                        .inner_margin(12.0)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(format!("L{}", m.point))
                                        .size(16.0)
                                        .color(egui::Color32::from_rgb(100, 200, 255))
                                        .strong(),
                                );
                                ui.label(
                                    egui::RichText::new(format!(" \u{2013} {}", m.planet_name))
                                        .size(16.0)
                                        .color(egui::Color32::from_rgb(200, 220, 255))
                                        .strong(),
                                );
                            });
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(lp_qualifier(m.point))
                                        .size(12.0)
                                        .color(egui::Color32::from_rgb(150, 180, 210)),
                                );
                            });
                            // Distance from parent planet (more intuitive than heliocentric radius).
                            // L1/L2: Hill-sphere offset; L3: diameter of orbit; L4/L5: equilateral-triangle side.
                            let dist_from_planet_au = match m.point {
                                1 | 2 => (m.planet_sma_au - m.lp_radius_au).abs(),
                                3 => 2.0 * m.planet_sma_au,
                                _ => m.planet_sma_au, // L4/L5: equilateral triangle
                            };
                            const AU_KM: f64 = 149_597_870.7;
                            let dist_str = if dist_from_planet_au < 0.01 {
                                format!("{:.0} km from {}", dist_from_planet_au * AU_KM, m.planet_name)
                            } else {
                                format!("{:.3} AU from {}", dist_from_planet_au, m.planet_name)
                            };
                            let stability = match m.point {
                                4 | 5 => ("Stable", egui::Color32::from_rgb(100, 210, 130)),
                                _ => ("Unstable", egui::Color32::from_rgb(220, 160, 80)),
                            };
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(dist_str)
                                        .size(11.0)
                                        .color(egui::Color32::from_rgb(130, 160, 190)),
                                );
                            });
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(stability.0)
                                        .size(11.0)
                                        .color(stability.1),
                                );
                            });
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("Click to select as fleet target")
                                        .size(10.0)
                                        .italics()
                                        .color(egui::Color32::from_rgb(120, 140, 160)),
                                );
                            });
                        });
                });
            return;
        }
    }

    // Display hover tooltip if a body is hovered
    if let Ok(body) = hovered_query.single() {
        // Anchor the tooltip near the mouse pointer so it appears over the 3D view
        let available_rect = ctx.available_rect();
        let tooltip_pos = ctx
            .input(|i| i.pointer.hover_pos())
            .map(|p| egui::pos2(p.x + 12.0, p.y + 12.0))
            .unwrap_or(egui::pos2(100.0, 100.0));

        egui::Area::new("hover_tooltip".into())
            .fixed_pos(tooltip_pos)
            .interactable(false)
            .order(egui::Order::Tooltip)
            .constrain_to(available_rect)
            .show(ctx, |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                egui::Frame::NONE
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

/// Read [`LastLpClick`] resource and update the fleet transfer planner
/// so that the clicked LP becomes the active transfer target.
fn ui_lp_click_handler(
    mut last_click: ResMut<LastLpClick>,
    mut fleet_ui_state: ResMut<FleetUiState>,
) {
    let Some(m_owned) = last_click.info.take() else { return; };
    let m = &m_owned;
    fleet_ui_state.target_lagrange = Some(LagrangeTarget {
            point: m.point,
            planet_entity: m.planet_entity,
            planet_name: m.planet_name.clone(),
            planet_sma_au: m.planet_sma_au,
            radius_au: m.lp_radius_au,
            gm: m.gm,
        });
    fleet_ui_state.target_body  = None;
    fleet_ui_state.target_fleet = None;
    fleet_ui_state.selected_option = 0;
    fleet_ui_state.selected_gravity_assist = None;
    fleet_ui_state.computed_options.clear();
    fleet_ui_state.planned_transfer = None;
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

    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    // Display hover tooltip if a star system is hovered
    if let Ok(icon) = hovered_query.single() {
        // Anchor the tooltip near the mouse pointer
        let available_rect = ctx.available_rect();
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
            .constrain_to(available_rect)
            .show(ctx, |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                egui::Frame::NONE
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

    // Near-zero
    if abs_val < 1e-9 {
        return "0 t".to_string();
    }

    // Tons: 1 Mt = 1,000,000 t  (for values < 1 kt = 0.001 Mt)
    if abs_val < 0.001 {
        return format!("{:.1} t", megatons * 1_000_000.0);
    }

    // Kilotons: 1 Mt = 1000 kt  (for values < 1 Mt)
    if abs_val < 1.0 {
        return format!("{:.1} kt", megatons * 1000.0);
    }

    // Megatons (for values < 1 Gt = 1000 Mt)
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
    format!("{:.1} Et", megatons / 1_000_000_000_000.0)
}

/// Format a monthly rate value with sign and appropriate color.
/// Returns (formatted_string, color).
fn format_rate_monthly(value: f64) -> (String, egui::Color32) {
    if value.abs() < 1e-9 {
        return ("+0/mo".to_string(), egui::Color32::GRAY);
    }
    if value > 0.0 {
        (format!("+{}/mo", format_mass(value)), egui::Color32::from_rgb(100, 255, 100))
    } else {
        (format!("{}/mo", format_mass(value)), egui::Color32::from_rgb(255, 100, 100))
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
    mut selection: ResMut<Selection>,
    current_system: Res<CurrentStarSystem>,
    nearby_stars: Res<NearbyStarsData>,
    active_menu: Res<ActiveMenu>,
    mut fleet_ui_state: ResMut<FleetUiState>,
    fleet_query: Query<(Entity, &Fleet, Option<&FleetOrbit>, Option<&ActiveManeuver>)>,
    // Query for selected body information
    mut body_query: Query<(
        &CelestialBody,
        Option<&SpaceCoordinates>,
        Option<&KeplerOrbit>,
        Option<&PlanetResources>,
        Option<&AtmosphereComposition>,
        Option<&mut SurveyLevel>,
        Option<&Population>,
        Option<&crate::astronomy::SurfaceTemperature>,
        Option<&LogicalParent>,
    )>,
    // Read-only lookup for parent body coordinates
    parent_coords_query: Query<&SpaceCoordinates>,
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
    sim_time: Res<SimulationTime>,
) {
    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    if active_menu.current == GameMenu::Research
        || active_menu.current == GameMenu::Construction
        || active_menu.current == GameMenu::Economy
        || active_menu.current == GameMenu::Fleets
    {
        return;
    }

    // Ledger Panel (Left)
    egui::SidePanel::left("ledger_panel")
        .min_width(200.0)
        .default_width(230.0)
        .show(ctx, |ui| {
            match active_menu.current {
                GameMenu::Starmap => {
                    // Starmap view: show list of star systems
                    ui.heading("Star Systems");
                    ui.separator();

                    egui::ScrollArea::vertical()
                        .id_salt("starmap_ledger_scroll")
                        .show(ui, |ui| {
                            for (entity, icon, is_selected) in star_system_query.iter() {
                                let response =
                                    render_selectable_label(ui, is_selected.is_some(), &icon.name);

                                if response.clicked() {
                                    // Single click: select the star system and anchor camera
                                    // Clear previous selections first
                                    for (e, _, sel) in star_system_query.iter() {
                                        if sel.is_some() {
                                            commands.entity(e).remove::<SelectedStarSystem>();
                                        }
                                    }
                                    commands.entity(entity).insert(SelectedStarSystem);

                                    // Anchor camera to this system
                                    if let Ok(mut anchor) = anchor_query.single_mut() {
                                        anchor.0 = Some(entity);
                                        info!("Selected and anchored to {}", icon.name);
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
                        .id_salt("ledger_scroll")
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

                            // ── Fleet section ─────────────────────────────
                            ui.add_space(4.0);
                            ui.separator();

                            let fleet_id = ui.make_persistent_id("survey_fleet_tree");
                            egui::collapsing_header::CollapsingState::load_with_default_open(
                                ui.ctx(),
                                fleet_id,
                                true,
                            )
                            .show_header(ui, |ui| {
                                let n = fleet_query.iter().count();
                                ui.label(
                                    egui::RichText::new(format!("🚀 Fleets ({n})"))
                                        .strong()
                                        .color(egui::Color32::from_rgb(120, 210, 140)),
                                );
                            })
                            .body(|ui| {
                                render_fleet_ledger_tree(
                                    ui,
                                    &fleet_query,
                                    &body_map,
                                    &mut fleet_ui_state,
                                    &selected_query,
                                    &mut commands,
                                    &mut selection,
                                    sim_time.elapsed_seconds(),
                                    &mut anchor_query,
                                );
                            });
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
                            ui.label("Fleet panel is open in the main view.");
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
                    if let Ok((body, opt_coords, orbit, resources, atmosphere, mut survey_level, population, surface_temp, logical_parent)) = body_query.get_mut(entity) {
                        // Body name and basic info
                        ui.label(egui::RichText::new(&body.name).size(18.0).strong());
                        ui.add_space(10.0);

                        // Position information
                        ui.group(|ui| {
                            ui.label(egui::RichText::new("Position").strong());
                            // For non-star bodies, compute distance relative to
                            // the system primary (star) using the absolute position.
                            if !matches!(body.body_type, crate::plugins::solar_system_data::BodyType::Star) {
                                if let Some(coords) = opt_coords {
                                    // Walk up the LogicalParent chain to find the system star,
                                    // then subtract its absolute universe position so we get the
                                    // true orbital distance regardless of the star's distance from Sol.
                                    let star_pos = {
                                        let mut current = logical_parent.map(|lp| lp.0);
                                        let mut found = bevy::math::DVec3::ZERO;
                                        while let Some(parent_entity) = current {
                                            if let Ok((_, parent_body, grandparent, _, _)) = all_bodies_query.get(parent_entity) {
                                                if matches!(parent_body.body_type, crate::plugins::solar_system_data::BodyType::Star) {
                                                    if let Ok(star_coords) = parent_coords_query.get(parent_entity) {
                                                        found = star_coords.position;
                                                    }
                                                    break;
                                                }
                                                current = grandparent.map(|gp| gp.0);
                                            } else {
                                                break;
                                            }
                                        }
                                        found
                                    };
                                    let distance = (coords.position - star_pos).length();
                                    ui.label(format!("Distance from Star: {:.3} AU", distance));
                                } else {
                                    ui.label("Position: co-orbiting parent body");
                                }
                            }
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

                        // Rings cannot be colonised or have buildings — show a concise note
                        // instead of the full habitability / colony-cost section.
                        if body.body_type == crate::plugins::solar_system_data::BodyType::Ring {
                            ui.group(|ui| {
                                ui.label(
                                    egui::RichText::new("⚠ Orbital Mining Only")
                                        .strong()
                                        .color(egui::Color32::from_rgb(255, 165, 0)),
                                );
                                ui.label("Ring systems consist of free-floating ice and dust.");
                                ui.label("They cannot be colonised or have buildings constructed.");
                                ui.label("Resources must be harvested by mining ships in orbit.");
                            });
                        } else {
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
                        }); // end Habitability group
                        } // end else (non-ring body)
                        
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
                                                            if deposit.reserve.proven_crustal < 0.001 {
                                                                ui.label(egui::RichText::new("Depleted").color(egui::Color32::RED).strong());
                                                            } else {
                                                                ui.add(egui::ProgressBar::new(1.0)
                                                                    .text(format_mass(deposit.reserve.proven_crustal)));
                                                            }
                                                        });
                                                        
                                                        // Deep / Trapped
                                                        if matches!(current_level, SurveyLevel::SeismicSurvey | SurveyLevel::CoreSample) {
                                                            ui.horizontal(|ui| {
                                                                ui.label(deep_label);
                                                                if deposit.reserve.deep_deposits < 0.001 {
                                                                    ui.label(egui::RichText::new("Depleted").color(egui::Color32::RED).strong());
                                                                } else {
                                                                    ui.add(egui::ProgressBar::new(1.0)
                                                                        .text(format_mass(deposit.reserve.deep_deposits)));
                                                                }
                                                            });
                                                        } else {
                                                             ui.label(format!("    {}: ???", if is_atm { "Trapped/Dissolved" } else { "Deep Deposits" }));
                                                        }
                                                        
                                                        // Bulk / Chemically Bound
                                                        if current_level == SurveyLevel::CoreSample {
                                                            ui.horizontal(|ui| {
                                                                ui.label(bulk_label);
                                                                if deposit.reserve.planetary_bulk < 0.001 {
                                                                    ui.label(egui::RichText::new("Depleted").color(egui::Color32::RED).strong());
                                                                } else {
                                                                    ui.add(egui::ProgressBar::new(1.0)
                                                                        .text(format_mass(deposit.reserve.planetary_bulk)));
                                                                }
                                                            });
                                                        } else {
                                                            ui.label(format!("    {}: ???", if is_atm { "Chemically Bound" } else { "Planetary Bulk" }));
                                                        }
                                                        
                                                        // Only show concentration for non-atmospheric deposits
                                                        // Concentration is meaningless for gas in an atmosphere
                                                        if !is_atm {
                                                            ui.horizontal(|ui| {
                                                                ui.label("    Concentration:");
                                                                let conc = deposit.reserve.concentration;
                                                                let conc_text = if conc >= 0.01 {
                                                                    format!("{:.1}%", conc * 100.0)
                                                                } else if conc >= 0.000_01 {
                                                                    format!("{:.1} ppm", conc * 1_000_000.0)
                                                                } else if conc >= 0.000_000_01 {
                                                                    format!("{:.2} ppb", conc * 1_000_000_000.0)
                                                                } else {
                                                                    format!("{:.2e}", conc)
                                                                };
                                                                ui.add(egui::ProgressBar::new(conc.min(1.0))
                                                                    .text(conc_text));
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
}

/// Always-visible bottom panel for time controls.
///
/// Registered in `UiSystemSet::TopBar` so egui reserves the bottom strip
/// **before** any side panel (Research, Construction, Economy, etc.) is
/// rendered. This ensures the panel is never occluded regardless of the
/// active menu.
fn ui_time_controls(
    mut contexts: EguiContexts,
    mut time_scale: ResMut<TimeScale>,
    sim_time: Res<SimulationTime>,
    view_mode: Res<ViewMode>,
) {
    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

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

fn render_research_tech_tooltip_content(
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

/// Check window resolution at startup and flag if below minimum
fn check_window_resolution(
    windows: Query<&Window, With<PrimaryWindow>>,
    mut warning: ResMut<ResolutionWarning>,
) {
    if let Ok(window) = windows.single() {
        if window.width() < MIN_WINDOW_WIDTH || window.height() < MIN_WINDOW_HEIGHT {
            warning.should_show = true;
        }
    }
}

/// Display a warning dialog if the window resolution is below minimum
fn ui_resolution_warning(
    mut contexts: EguiContexts,
    mut warning: ResMut<ResolutionWarning>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    // Only show if flagged and not dismissed
    if !warning.should_show || warning.dismissed {
        return;
    }

    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };
    
    // Get current window size for display
    let (current_width, current_height) = if let Ok(window) = windows.single() {
        (window.width(), window.height())
    } else {
        return;
    };

    let window_response = egui::Window::new("⚠ Display Resolution Notice")
        .id(egui::Id::new("resolution_warning_dialog"))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.set_min_width(520.0);
            ui.set_max_width(520.0);
            
            ui.vertical_centered(|ui| {
                ui.add_space(10.0);
                ui.label(
                    egui::RichText::new("⚠")
                        .size(56.0)
                        .color(egui::Color32::from_rgb(255, 200, 0))
                );
                ui.add_space(10.0);
                ui.label(
                    egui::RichText::new("Low Resolution Detected")
                        .size(18.0)
                        .strong()
                        .color(egui::Color32::from_rgb(255, 220, 100))
                );
            });

            ui.separator();
            ui.add_space(12.0);

            // Current vs Required
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label("Your resolution:");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(format!("{}×{}", current_width as u32, current_height as u32))
                                .strong()
                                .size(15.0)
                                .color(egui::Color32::from_rgb(255, 100, 100))
                        );
                    });
                });
                ui.horizontal(|ui| {
                    ui.label("Required minimum:");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(format!("{}×{} (Full HD)", MIN_WINDOW_WIDTH as u32, MIN_WINDOW_HEIGHT as u32))
                                .strong()
                                .size(15.0)
                                .color(egui::Color32::from_rgb(100, 255, 100))
                        );
                    });
                });
            });

            ui.add_space(12.0);

            // Explanation
            ui.label(
                egui::RichText::new("Why Full HD is Required:")
                    .strong()
                    .size(13.0)
            );
            ui.add_space(4.0);
            ui.label(
                "Helios Ascension is a complex 4X grand strategy game with extensive UI elements including:"
            );
            ui.add_space(4.0);
            ui.indent("ui_elements", |ui| {
                ui.label("• Resource & economy tracking panels");
                ui.label("• Research & engineering progress displays");
                ui.label("• Colony management interfaces");
                ui.label("• Star system navigation controls");
                ui.label("• Detailed celestial body information");
                ui.label("• Technology tree visualization");
            });
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("At lower resolutions, these elements will overlap and become difficult or impossible to use.")
                    .size(12.0)
                    .color(egui::Color32::from_rgb(220, 220, 220))
            );

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);

            // Solutions
            ui.label(
                egui::RichText::new("Recommended Solutions:")
                    .strong()
                    .size(13.0)
            );
            ui.add_space(4.0);
            ui.indent("solutions", |ui| {
                ui.label("1. Switch to Full HD (1920×1080) or higher resolution");
                ui.label("2. Maximize the game window");
                ui.label("3. Reduce display scaling in Windows settings");
                ui.label("4. Use an external monitor if on a laptop");
            });

            ui.add_space(16.0);
            ui.separator();
            ui.add_space(8.0);

            let mut dismiss = false;
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("You may continue, but expect UI issues.")
                        .size(11.0)
                        .italics()
                        .color(egui::Color32::from_rgb(180, 180, 180))
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(egui::RichText::new("I Understand").size(14.0)).clicked() {
                        dismiss = true;
                    }
                });
            });

            ui.add_space(4.0);
            
            dismiss
        });

    // Check if the user clicked the dismiss button
    if let Some(inner_response) = window_response {
        if inner_response.inner == Some(true) {
            warning.dismissed = true;
        }
    }
}

// ── Fleet UI panel ────────────────────────────────────────────────────────────

/// Full-screen fleet management and orbital transfer planning panel.
fn ui_fleets_panel(
    mut contexts: EguiContexts,
    active_menu: Res<ActiveMenu>,
    fleet_query: Query<(
        Entity,
        &Fleet,
        Option<&FleetOrbit>,
        Option<&ActiveManeuver>,
        &SpaceCoordinates,
    )>,
    body_query: Query<(Entity, &CelestialBody, &SpaceCoordinates, Option<&KeplerOrbit>, Option<&LogicalParent>)>,
    colony_query: Query<(Entity, &Colony)>,
    mut pending_actions: ResMut<PendingFleetActions>,
    mut fleet_ui_state: ResMut<FleetUiState>,
    sim_time: Res<SimulationTime>,
) {
    if active_menu.current != GameMenu::Fleets {
        return;
    }

    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    let elapsed = sim_time.elapsed_seconds();

    egui::CentralPanel::default().show(ctx, |ui| {
        ui.heading("Fleets");
        ui.separator();

        // ── Top summary bar ──────────────────────────────────────────────────
        let fleet_count = fleet_query.iter().count();
        let in_transit = fleet_query.iter().filter(|(_, _, _, m, _)| m.is_some()).count();
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!("🚀 Total Fleets: {fleet_count}"))
                    .size(13.0)
                    .color(egui::Color32::from_rgb(200, 220, 255)),
            );
            ui.separator();
            ui.label(
                egui::RichText::new(format!("✈ In Transit: {in_transit}"))
                    .size(13.0)
                    .color(egui::Color32::from_rgb(100, 200, 255)),
            );
        });
        ui.separator();

        // ── Main two-column layout ───────────────────────────────────────────
        let available = ui.available_size();
        let left_width = (available.x * 0.42).max(380.0);

        ui.horizontal_top(|ui| {
            // ── Left column: fleet list ──────────────────────────────────────
            ui.allocate_ui_with_layout(
                egui::Vec2::new(left_width, available.y - 80.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                egui::Frame::default()
                    .inner_margin(egui::Margin::same(6i8))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(60, 80, 120)))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new("Fleet List")
                                .strong()
                                .size(14.0)
                                .color(egui::Color32::from_rgb(180, 210, 255)),
                        );
                        ui.separator();

                        egui::ScrollArea::vertical()
                            .id_salt("fleet_list_scroll")
                            .max_height(available.y - 140.0)
                            .show(ui, |ui| {
                                render_fleet_list(
                                    ui,
                                    &fleet_query,
                                    &body_query,
                                    &mut fleet_ui_state,
                                    &mut pending_actions,
                                    elapsed,
                                );
                            });

                        ui.separator();
                        // ── Create Fleet section ─────────────────────────────
                        {
                            // Build sorted colony list grouped by star system
                            let mut colony_entries: Vec<(Entity, String, String)> = colony_query
                                .iter()
                                .filter_map(|(e, _colony)| {
                                    body_query.get(e).ok().map(|(_, body, _, _, _)| {
                                        let star = find_body_star_name(e, &body_query);
                                        (e, body.name.clone(), star)
                                    })
                                })
                                .collect();
                            colony_entries.sort_by(|a, b| a.2.cmp(&b.2).then(a.1.cmp(&b.1)));

                            // Keep selection valid; default to selected fleet location → Earth → first colony
                            let selection_valid = fleet_ui_state.spawn_location_body
                                .map(|e| colony_entries.iter().any(|(ce, _, _)| *ce == e))
                                .unwrap_or(false);
                            if !selection_valid {
                                let fallback = fleet_ui_state.selected_fleet
                                    .and_then(|sel| fleet_query.get(sel).ok().and_then(|(_, _, mo, _, _)| mo.map(|o| o.body)))
                                    .and_then(|e| colony_entries.iter().any(|(ce, _, _)| *ce == e).then_some(e))
                                    .or_else(|| body_query.iter().find(|(_, b, _, _, _)| b.name == "Earth").map(|(e, _, _, _, _)| e)
                                        .and_then(|e| colony_entries.iter().any(|(ce, _, _)| *ce == e).then_some(e)))
                                    .or_else(|| colony_entries.first().map(|(e, _, _)| *e));
                                fleet_ui_state.spawn_location_body = fallback;
                            }

                            let selected_label = fleet_ui_state.spawn_location_body
                                .and_then(|e| colony_entries.iter().find(|(ce, _, _)| *ce == e))
                                .map(|(_, name, star)| format!("{name} ({star})"))
                                .unwrap_or_else(|| "— No colony —".to_string());

                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("Location:")
                                        .size(12.0)
                                        .color(egui::Color32::GRAY),
                                );
                                if colony_entries.is_empty() {
                                    ui.label(
                                        egui::RichText::new("No colonies yet")
                                            .size(12.0)
                                            .italics()
                                            .color(egui::Color32::DARK_GRAY),
                                    );
                                } else {
                                    egui::ComboBox::from_id_salt("create_fleet_location")
                                        .selected_text(egui::RichText::new(&selected_label).size(12.0))
                                        .width(210.0)
                                        .show_ui(ui, |ui| {
                                            let mut current_star = String::new();
                                            for (e, body_name, star_name) in &colony_entries {
                                                if *star_name != current_star {
                                                    current_star = star_name.clone();
                                                    ui.add_space(2.0);
                                                    ui.label(
                                                        egui::RichText::new(format!("★ {star_name}"))
                                                            .size(11.0)
                                                            .strong()
                                                            .color(egui::Color32::from_rgb(255, 220, 100)),
                                                    );
                                                }
                                                let is_sel = fleet_ui_state.spawn_location_body == Some(*e);
                                                if ui.selectable_label(
                                                    is_sel,
                                                    egui::RichText::new(format!("  {body_name}")).size(12.0),
                                                ).clicked() {
                                                    fleet_ui_state.spawn_location_body = Some(*e);
                                                }
                                            }
                                        });
                                }
                            });

                            if ui
                                .button(egui::RichText::new("＋ Create Fleet").size(13.0))
                                .clicked()
                            {
                                let spawn_body = fleet_ui_state.spawn_location_body.or_else(|| {
                                    body_query
                                        .iter()
                                        .find(|(_, b, _, _, _)| b.name == "Earth")
                                        .map(|(e, _, _, _, _)| e)
                                });
                                if let Some(body_entity) = spawn_body {
                                    let orbit_radius_au = 6_771.0_f64 * 1_000.0 / AU_IN_METERS;
                                    pending_actions.spawn_fleets.push(
                                        crate::fleets::components::SpawnFleetAction {
                                            name: format!("New Fleet {}", fleet_count + 1),
                                            ships: Vec::new(),
                                            orbit_body: body_entity,
                                            orbit_radius_au,
                                        },
                                    );
                                }
                            }
                        }
                    });
            });

            ui.add_space(8.0);

            // ── Right column: selected fleet details + transfer planner ──────
            let remaining = ui.available_width().min(480.0);
            ui.allocate_ui_with_layout(
                egui::Vec2::new(remaining, available.y - 80.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                egui::Frame::default()
                    .inner_margin(egui::Margin::same(6i8))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(60, 80, 120)))
                    .show(ui, |ui| {
                if let Some(selected) = fleet_ui_state.selected_fleet {
                    if let Ok((_, fleet, maybe_orbit, maybe_maneuver, _)) =
                        fleet_query.get(selected)
                    {
                        egui::ScrollArea::vertical()
                            .id_salt("fleet_detail_scroll")
                            .show(ui, |ui| {
                                render_fleet_detail(
                                    ui,
                                    selected,
                                    fleet,
                                    maybe_orbit,
                                    maybe_maneuver,
                                    &body_query,
                                    &mut fleet_ui_state,
                                    &mut pending_actions,
                                    elapsed,
                                );
                            });
                    } else {
                        // Selected entity no longer exists
                        fleet_ui_state.selected_fleet = None;
                    }
                } else {
                    ui.vertical_centered(|ui| {
                        ui.add_space(60.0);
                        ui.label(
                            egui::RichText::new("Select a fleet from the list to view details.")
                                .size(14.0)
                                .italics()
                                .color(egui::Color32::GRAY),
                        );
                    });
                }
                    });
            });
        });
    });

    // ── Disband confirmation popup ────────────────────────────────────────────
    if let Some(fleet_to_disband) = fleet_ui_state.disband_confirm_fleet {
        let fleet_info = fleet_query.get(fleet_to_disband).ok().map(|(_, f, _, _, _)| (f.name.clone(), f.ships.len()));
        if let Some((fleet_name, ship_count)) = fleet_info {
            let mut do_disband = false;
            let mut cancel = false;
            egui::Window::new("⚠ Confirm Disband")
                .id(egui::Id::new("fleet_disband_confirm"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.set_min_width(360.0);
                    ui.vertical_centered(|ui| {
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new("⚠").size(36.0).color(egui::Color32::from_rgb(255, 180, 40)));
                    });
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(format!("Disband \"{}\"?", fleet_name))
                            .strong()
                            .size(15.0)
                            .color(egui::Color32::from_rgb(255, 220, 120)),
                    );
                    if ship_count > 0 {
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new(format!(
                                "This will permanently destroy {} ship(s).\nThis action cannot be undone.",
                                ship_count
                            ))
                            .size(13.0)
                            .color(egui::Color32::from_rgb(220, 120, 100)),
                        );
                    }
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.button(egui::RichText::new("Cancel").size(13.0)).clicked() {
                            cancel = true;
                        }
                        ui.add_space(12.0);
                        if ui
                            .button(
                                egui::RichText::new("🗑 Disband")
                                    .size(13.0)
                                    .color(egui::Color32::from_rgb(230, 80, 60)),
                            )
                            .clicked()
                        {
                            do_disband = true;
                        }
                    });
                    ui.add_space(4.0);
                });
            if do_disband {
                pending_actions.disband_fleets.push(fleet_to_disband);
                if fleet_ui_state.selected_fleet == Some(fleet_to_disband) {
                    fleet_ui_state.selected_fleet = None;
                }
                fleet_ui_state.selected_fleets.retain(|&e| e != fleet_to_disband);
                fleet_ui_state.disband_confirm_fleet = None;
            }
            if cancel {
                fleet_ui_state.disband_confirm_fleet = None;
            }
        } else {
            // Fleet no longer exists
            fleet_ui_state.disband_confirm_fleet = None;
        }
    }
}

/// Walk up the `LogicalParent` chain to find the star name for a body.
fn find_body_star_name(
    mut body_entity: Entity,
    body_query: &Query<(Entity, &CelestialBody, &SpaceCoordinates, Option<&KeplerOrbit>, Option<&LogicalParent>)>,
) -> String {
    for _ in 0..10 {
        match body_query.get(body_entity) {
            Ok((_, body, _, _, maybe_parent)) => {
                if body.body_type == BodyType::Star {
                    return body.name.clone();
                }
                if let Some(LogicalParent(parent)) = maybe_parent {
                    body_entity = *parent;
                } else {
                    return body.name.clone();
                }
            }
            Err(_) => break,
        }
    }
    "Unknown System".to_string()
}

/// Render the scrollable list of fleets on the left side, grouped by star system.
fn render_fleet_list(
    ui: &mut egui::Ui,
    fleet_query: &Query<(
        Entity,
        &Fleet,
        Option<&FleetOrbit>,
        Option<&ActiveManeuver>,
        &SpaceCoordinates,
    )>,
    body_query: &Query<(Entity, &CelestialBody, &SpaceCoordinates, Option<&KeplerOrbit>, Option<&LogicalParent>)>,
    fleet_ui_state: &mut FleetUiState,
    pending_actions: &mut PendingFleetActions,
    elapsed: f64,
) {
    struct FEntry {
        entity: Entity,
        ship_count: usize,
        role_icon: &'static str,
        name: String,
        in_transit: bool,
        star_name: String,
        location_text: String,
        fuel_pct: u32,
        transit_progress: Option<(u32, String)>,
        waiting_depart: Option<String>,
    }

    let mut entries: Vec<FEntry> = fleet_query
        .iter()
        .map(|(entity, fleet, maybe_orbit, maybe_maneuver, _)| {
            let in_transit = maybe_maneuver.is_some();
            let (location_text, star_name) = if let Some(orbit) = maybe_orbit {
                let body_name = body_query
                    .get(orbit.body)
                    .map(|(_, b, _, _, _)| b.name.clone())
                    .unwrap_or_default();
                let star = find_body_star_name(orbit.body, body_query);
                (format!("📍 {body_name}"), star)
            } else if let Some(man) = maybe_maneuver {
                let src = body_query
                    .get(man.origin_body)
                    .map(|(_, b, _, _, _)| b.name.clone())
                    .unwrap_or_default();
                let dst = body_query
                    .get(man.destination_body)
                    .map(|(_, b, _, _, _)| b.name.clone())
                    .unwrap_or_default();
                let star = find_body_star_name(man.destination_body, body_query);
                (format!("{src} → {dst}"), star)
            } else {
                ("Unknown".to_string(), "Unknown System".to_string())
            };

            let transit_progress = maybe_maneuver.and_then(|man| {
                if elapsed >= man.departure_time {
                    let prog = (man.progress(elapsed) * 100.0) as u32;
                    let rem = format_duration(man.time_remaining_s(elapsed));
                    Some((prog, rem))
                } else {
                    None
                }
            });
            let waiting_depart = maybe_maneuver.and_then(|man| {
                if elapsed < man.departure_time {
                    Some(format_duration(man.departure_time - elapsed))
                } else {
                    None
                }
            });

            FEntry {
                entity,
                ship_count: fleet.ships.len(),
                role_icon: if in_transit { "✈" } else { fleet.role.icon() },
                name: fleet.name.clone(),
                in_transit,
                star_name,
                location_text,
                fuel_pct: (fleet.fuel_fraction() * 100.0) as u32,
                transit_progress,
                waiting_depart,
            }
        })
        .collect();

    entries.sort_by(|a, b| a.star_name.cmp(&b.star_name).then(a.name.cmp(&b.name)));

    // Ordered list of entities (same order as display) for shift-range select.
    let sorted_entities: Vec<Entity> = entries.iter().map(|e| e.entity).collect();

    let mut current_system = String::new();
    for entry in &entries {
        // ── System header ─────────────────────────────────────────────────────
        if entry.star_name != current_system {
            current_system = entry.star_name.clone();
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(format!("★  {current_system}"))
                    .size(12.0)
                    .strong()
                    .color(egui::Color32::from_rgb(255, 220, 100)),
            );
        }

        let is_primary = fleet_ui_state.selected_fleet == Some(entry.entity);
        let is_checked = fleet_ui_state.selected_fleets.contains(&entry.entity);
        let row_text = format!("{} {} — {} ship(s)", entry.role_icon, entry.name, entry.ship_count);
        let row_color = if entry.in_transit {
            egui::Color32::from_rgb(100, 180, 255)
        } else {
            egui::Color32::from_rgb(100, 220, 100)
        };

        // ── Row: [checkbox] [drop-zone selectable] ────────────────────────────
        ui.horizontal(|ui| {
            let mut checked = is_checked;
            if ui.checkbox(&mut checked, "").changed() {
                if checked {
                    if !fleet_ui_state.selected_fleets.contains(&entry.entity) {
                        fleet_ui_state.selected_fleets.push(entry.entity);
                    }
                } else {
                    fleet_ui_state.selected_fleets.retain(|&e| e != entry.entity);
                }
            }

            let drop_result = ui.dnd_drop_zone::<(Entity, usize), _>(egui::Frame::NONE, |ui| {
                let resp = ui.selectable_label(
                    is_primary,
                    egui::RichText::new(&row_text).size(13.0).color(row_color),
                );
                if resp.clicked() {
                    let ctrl = ui.input(|i| i.modifiers.ctrl || i.modifiers.mac_cmd);
                    let shift = ui.input(|i| i.modifiers.shift);
                    if ctrl {
                        if fleet_ui_state.selected_fleets.contains(&entry.entity) {
                            fleet_ui_state.selected_fleets.retain(|&e| e != entry.entity);
                        } else {
                            fleet_ui_state.selected_fleets.push(entry.entity);
                        }
                    } else if shift {
                        let anchor = fleet_ui_state.last_single_selected.unwrap_or(entry.entity);
                        let ai = sorted_entities.iter().position(|&e| e == anchor).unwrap_or(0);
                        let ci = sorted_entities.iter().position(|&e| e == entry.entity).unwrap_or(0);
                        let (lo, hi) = if ai <= ci { (ai, ci) } else { (ci, ai) };
                        for &e in &sorted_entities[lo..=hi] {
                            if !fleet_ui_state.selected_fleets.contains(&e) {
                                fleet_ui_state.selected_fleets.push(e);
                            }
                        }
                    } else {
                        if fleet_ui_state.selected_fleet == Some(entry.entity) {
                            fleet_ui_state.selected_fleet = None;
                        } else {
                            fleet_ui_state.selected_fleet = Some(entry.entity);
                            fleet_ui_state.clear_target();
                            fleet_ui_state.last_single_selected = Some(entry.entity);
                        }
                        fleet_ui_state.selected_fleets.clear();
                    }
                }
                resp
            });

            if let Some(payload) = drop_result.1 {
                let (source_fleet, ship_idx) = *payload;
                if source_fleet != entry.entity {
                    pending_actions.transfer_ships.push(
                        crate::fleets::components::TransferShipsAction {
                            source_fleet,
                            destination_fleet: entry.entity,
                            ship_indices: vec![ship_idx],
                        },
                    );
                }
            }
        });

        // ── Sub-status line ───────────────────────────────────────────────────
        let sub = if let Some(wait_str) = &entry.waiting_depart {
            egui::RichText::new(format!("    Waiting — T-minus {wait_str}"))
                .size(11.0)
                .color(egui::Color32::from_rgb(255, 200, 100))
        } else if let Some((prog, rem)) = &entry.transit_progress {
            egui::RichText::new(format!(
                "    ✈ {} — {}% done, {} left",
                entry.location_text, prog, rem
            ))
            .size(11.0)
            .color(egui::Color32::from_rgb(160, 190, 230))
        } else {
            egui::RichText::new(format!("    {} — fuel {}%", entry.location_text, entry.fuel_pct))
                .size(11.0)
                .color(egui::Color32::GRAY)
        };
        ui.label(sub);
    }

    // ── Multi-select action bar ───────────────────────────────────────────────
    let n = fleet_ui_state.selected_fleets.len();
    if n >= 2 {
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!("{n} selected"))
                    .size(12.0)
                    .color(egui::Color32::from_rgb(200, 220, 255)),
            );
            // All selected fleets must be in orbit at the same body (not in transit).
            let merge_bodies: Vec<Option<Entity>> = fleet_ui_state
                .selected_fleets
                .iter()
                .map(|&e| fleet_query.get(e).ok().and_then(|(_, _, mo, ma, _)| {
                    if ma.is_some() { None } else { mo.map(|o| o.body) }
                }))
                .collect();
            let all_same_location = !merge_bodies.is_empty()
                && merge_bodies[0].is_some()
                && merge_bodies.iter().all(|&b| b == merge_bodies[0]);
            let merge_tooltip = if all_same_location {
                "Merge into one fleet — the largest fleet keeps its name".to_string()
            } else {
                "Cannot merge: all fleets must be in orbit at the same location".to_string()
            };
            if ui
                .add_enabled(all_same_location, egui::Button::new(egui::RichText::new("⊕ Merge").size(13.0)))
                .on_hover_text(merge_tooltip)
                .clicked()
            {
                let target_fleet = fleet_ui_state
                    .selected_fleets
                    .iter()
                    .copied()
                    .max_by_key(|&e| fleet_query.get(e).map(|(_, f, _, _, _)| f.ships.len()).unwrap_or(0));
                if let Some(target_fleet) = target_fleet {
                    let source_fleets = fleet_ui_state
                        .selected_fleets
                        .iter()
                        .copied()
                        .filter(|&e| e != target_fleet)
                        .collect();
                    pending_actions.merge_fleets.push(MergeFleetAction { source_fleets, target_fleet });
                    fleet_ui_state.selected_fleet = Some(target_fleet);
                    fleet_ui_state.clear_multi_selection();
                    fleet_ui_state.clear_target();
                }
            }
            if ui
                .button(egui::RichText::new("✕ Clear").size(12.0))
                .clicked()
            {
                fleet_ui_state.clear_multi_selection();
            }
        });
    }

    ui.add_space(6.0);
    ui.label(
        egui::RichText::new("💡 Drag ship name → fleet to transfer  ·  Ctrl/⌘+click or Shift+click to multi-select")
            .size(10.0)
            .italics()
            .color(egui::Color32::from_rgb(120, 140, 170)),
    );
}

/// Render the fleet name with a marquee (ticker) scroll effect when the name is
/// too wide to fit in `max_width` pixels.  Uses `ui.input(|i| i.time)` for
/// real-time animation so it runs regardless of simulation speed.
/// Appends the ✏ rename button and writes into `editing_fleet_name` on click.
fn render_fleet_name_marquee(
    ui: &mut egui::Ui,
    fleet: &Fleet,
    fleet_entity: Entity,
    max_width: f32,
    editing_fleet_name: &mut Option<(Entity, String)>,
) {
    let name_text = format!("{} {}", fleet.role.icon(), fleet.name);
    let font_id = egui::FontId::proportional(18.0);
    let name_color = egui::Color32::from_rgb(200, 230, 255);

    // Measure the full text width at the desired font size.
    let full_width = ui
        .painter()
        .layout_no_wrap(name_text.clone(), font_id.clone(), name_color)
        .size()
        .x;

    if full_width <= max_width {
        // Fits entirely — plain label.
        ui.label(
            egui::RichText::new(&name_text)
                .strong()
                .size(18.0)
                .color(name_color),
        );
    } else {
        // Continuous marquee: scroll left at constant speed, loop seamlessly.
        // Two copies of the text are painted side-by-side separated by a gap;
        // the offset cycles over (full_width + gap) so the join is invisible.
        let gap = 72.0_f32;
        let cycle = (full_width + gap) as f64;
        let speed = 50.0_f64; // px / real-second
        let t = ui.input(|i| i.time);
        let offset_x = ((t * speed) % cycle) as f32;

        let text_height = ui
            .painter()
            .layout_no_wrap(name_text.clone(), font_id.clone(), name_color)
            .size()
            .y;
        let widget_height = text_height.max(24.0);
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(max_width, widget_height),
            egui::Sense::hover(),
        );
        let painter = ui.painter_at(rect); // clips to rect automatically
        let y = rect.top() + (rect.height() - text_height) * 0.5;

        // Re-layout for painting (layout_no_wrap needs Arc<Galley>)
        let galley2 = painter.layout_no_wrap(name_text.clone(), font_id.clone(), name_color);
        let x0 = rect.left() - offset_x;
        painter.galley(egui::pos2(x0, y), galley2.clone(), name_color);
        let x1 = x0 + (full_width + gap);
        if x1 < rect.right() + full_width {
            painter.galley(egui::pos2(x1, y), galley2, name_color);
        }

        ui.ctx().request_repaint();
    }

    if ui.button("✏").on_hover_text("Rename Fleet").clicked() {
        *editing_fleet_name = Some((fleet_entity, fleet.name.clone()));
    }
}

/// Render the right panel: fleet details (ship manifest, stats, status) and transfer planner.
#[allow(clippy::too_many_arguments)]
fn render_fleet_detail(
    ui: &mut egui::Ui,
    fleet_entity: Entity,
    fleet: &Fleet,
    maybe_orbit: Option<&FleetOrbit>,
    maybe_maneuver: Option<&ActiveManeuver>,
    body_query: &Query<(Entity, &CelestialBody, &SpaceCoordinates, Option<&KeplerOrbit>, Option<&LogicalParent>)>,
    fleet_ui_state: &mut FleetUiState,
    pending_actions: &mut PendingFleetActions,
    elapsed: f64,
) {
    // ── Fleet header ─────────────────────────────────────────────────────────
    // Row 1: fleet name + ✏ button (full width, no competing right-side controls)
    ui.horizontal(|ui| {
        let name_area_width = (ui.available_width() - 32.0).max(60.0);

        let is_editing_this = fleet_ui_state.editing_fleet_name
            .as_ref()
            .map(|(e, _)| *e == fleet_entity)
            .unwrap_or(false);

        if is_editing_this {
            let (committed_name, should_cancel) = {
                if let Some((_, ref mut current_name)) = fleet_ui_state.editing_fleet_name {
                    let response = ui.add_sized(
                        [name_area_width, 24.0],
                        egui::TextEdit::singleline(current_name),
                    );
                    let cancelled = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape));
                    let committed = response.lost_focus() && !cancelled;
                    if !committed && !cancelled {
                        response.request_focus();
                    }
                    (if committed { Some(current_name.clone()) } else { None }, cancelled)
                } else {
                    (None, false)
                }
            };
            if let Some(name) = committed_name {
                pending_actions.rename_fleets.push((fleet_entity, name));
                fleet_ui_state.editing_fleet_name = None;
            } else if should_cancel {
                fleet_ui_state.editing_fleet_name = None;
            }
        } else {
            render_fleet_name_marquee(ui, fleet, fleet_entity, name_area_width, &mut fleet_ui_state.editing_fleet_name);
        }
    });

    // Row 2: Role selector + Disband (right-aligned)
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(egui::RichText::new("🗑 Disband").color(egui::Color32::from_rgb(220, 80, 60)))
                .on_hover_text(if fleet.ships.is_empty() { "Disband this fleet" } else { "Disband fleet (destroys all ships)" })
                .clicked()
            {
                fleet_ui_state.disband_confirm_fleet = Some(fleet_entity);
            }
            egui::ComboBox::from_id_salt("fleet_role_combo")
                .selected_text(fleet.role.display_name())
                .show_ui(ui, |ui| {
                    use crate::fleets::types::FleetRole;
                    let roles = [
                        FleetRole::Unassigned,
                        FleetRole::Attack,
                        FleetRole::Defend,
                        FleetRole::Survey,
                        FleetRole::Transport,
                        FleetRole::Explore,
                    ];
                    for role in roles {
                        if ui.selectable_label(
                            fleet.role == role,
                            format!("{} {}", role.icon(), role.display_name()),
                        ).clicked() {
                            pending_actions.change_fleet_roles.push((fleet_entity, role));
                        }
                    }
                });
            ui.label("Role:");
        });
    });
    ui.separator();

    // ── Current status ────────────────────────────────────────────────────────
    if let Some(maneuver) = maybe_maneuver {
        render_active_maneuver_status(ui, fleet_entity, maneuver, fleet, body_query, pending_actions, elapsed);
    } else if let Some(orbit) = maybe_orbit {
        render_orbit_status(ui, orbit, fleet, body_query);
    }

    ui.separator();

    // ── Ship manifest ─────────────────────────────────────────────────────────
    ui.label(
        egui::RichText::new("Ship Manifest")
            .strong()
            .size(14.0),
    );
    let in_orbit_for_manifest = maybe_orbit.is_some();
    egui::Grid::new("ship_manifest")
        .num_columns(8)
        .spacing([12.0, 4.0])
        .striped(true)
        .show(ui, |ui| {
            // Header row
            ui.label(egui::RichText::new("Name").strong().size(12.0));
            ui.label(egui::RichText::new("Class").strong().size(12.0));
            ui.label(egui::RichText::new("Dry (t)").strong().size(12.0));
            ui.label(egui::RichText::new("Fuel").strong().size(12.0));
            ui.label(egui::RichText::new("Drive").strong().size(12.0));
            ui.label(egui::RichText::new("Thrust").strong().size(12.0));
            ui.label(egui::RichText::new("Max ΔV").strong().size(12.0));
            ui.label(egui::RichText::new("Actions").strong().size(12.0));
            ui.end_row();

            for (idx, ship) in fleet.ships.iter().enumerate() {
                let drag_id = egui::Id::new("drag_ship").with(fleet_entity).with(idx);
                ui.dnd_drag_source(drag_id, (fleet_entity, idx), |ui| {
                    ui.label(egui::RichText::new(&ship.name).size(12.0));
                });
                ui.label(
                    egui::RichText::new(format!(
                        "{} {}",
                        ship.class.icon(),
                        ship.class.display_name()
                    ))
                    .size(12.0),
                );
                ui.label(
                    egui::RichText::new(format!("{:.0}", ship.dry_mass_t)).size(12.0),
                );
                let fuel_pct = (ship.fuel_fraction() * 100.0) as u32;
                let fuel_color = if fuel_pct > 50 {
                    egui::Color32::from_rgb(100, 220, 100)
                } else if fuel_pct > 20 {
                    egui::Color32::from_rgb(220, 180, 60)
                } else {
                    egui::Color32::from_rgb(220, 80, 60)
                };
                ui.label(
                    egui::RichText::new(format!("{fuel_pct}%"))
                        .size(12.0)
                        .color(fuel_color),
                );
                ui.label(
                    egui::RichText::new(ship.propulsion.display_name()).size(12.0),
                );
                ui.label(
                    egui::RichText::new(format!("{:.0} kN", ship.thrust_kn)).size(12.0),
                );
                ui.label(
                    egui::RichText::new(format_delta_v(ship.delta_v_ms())).size(12.0),
                );
                // Refuel button — enabled only while in a stable orbit.
                // Currently fills to max for free (debug). In future will
                // draw propellant from the orbited body's stockpile.
                let refuel_resp = ui.add_enabled(
                    in_orbit_for_manifest,
                    egui::Button::new(egui::RichText::new("⛽ Refuel").size(11.0))
                        .min_size(egui::Vec2::new(60.0, 18.0)),
                );
                if refuel_resp
                    .on_hover_text(if in_orbit_for_manifest {
                        "Refuel this ship to full capacity (free — debug)"
                    } else {
                        "Cannot refuel while in transit"
                    })
                    .clicked()
                {
                    pending_actions.refuel_fleets.push(fleet_entity);
                }
                ui.end_row();
            }
        });

    // ── Fleet aggregate stats ─────────────────────────────────────────────────
    ui.add_space(6.0);
    egui::Grid::new("fleet_stats")
        .num_columns(4)
        .spacing([20.0, 4.0])
        .show(ui, |ui| {
            ui.label(egui::RichText::new("Dry mass:").size(12.0));
            ui.label(
                egui::RichText::new(format!("{:.0} t", fleet.total_dry_mass_t()))
                    .size(12.0)
                    .strong(),
            );
            ui.label(egui::RichText::new("Fuel:").size(12.0));
            let fuel_pct = (fleet.fuel_fraction() * 100.0) as u32;
            ui.label(
                egui::RichText::new(format!(
                    "{:.0} t ({fuel_pct}%)",
                    fleet.total_fuel_t()
                ))
                .size(12.0)
                .strong(),
            );
            ui.end_row();

            ui.label(egui::RichText::new("Min thrust:").size(12.0));
            ui.label(
                egui::RichText::new(format!("{:.0} kN", fleet.min_thrust_kn()))
                    .size(12.0)
                    .strong(),
            );
            ui.label(egui::RichText::new("Max ΔV:").size(12.0));
            ui.label(
                egui::RichText::new(format_delta_v(fleet.max_delta_v_ms()))
                    .size(12.0)
                    .strong()
                    .color(egui::Color32::from_rgb(100, 220, 255)),
            );
            ui.end_row();
        });

    // ── Transfer Planner shortcut ─────────────────────────────────────────
    // The planner now lives in a floating popup; show a button to open it.
    let can_plan = maybe_orbit.is_some()
        || maybe_maneuver.is_some();
    if can_plan {
        ui.separator();
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new("📡 Open Transfer Planner ↗").size(13.0),
                )
                .min_size(egui::Vec2::new(200.0, 32.0)),
            )
            .on_hover_text("Open the orbital transfer planner in a floating window")
            .clicked()
        {
            fleet_ui_state.show_transfer_popup = true;
        }
    }
}

// (Transfer Planner is now a floating popup — see ui_transfer_planner_popup.)

/// Show current maneuver status with a progress bar.
fn render_active_maneuver_status(
    ui: &mut egui::Ui,
    fleet_entity: Entity,
    maneuver: &ActiveManeuver,
    fleet: &Fleet,
    body_query: &Query<(Entity, &CelestialBody, &SpaceCoordinates, Option<&KeplerOrbit>, Option<&LogicalParent>)>,
    pending_actions: &mut PendingFleetActions,
    elapsed: f64,
) {
    let dest_name = body_query
        .get(maneuver.destination_body)
        .map(|(_, b, _, _, _)| b.name.as_str())
        .unwrap_or("Unknown");

    if elapsed < maneuver.departure_time {
        let wait_time = maneuver.departure_time - elapsed;
        let wait_str = format_duration(wait_time);
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("⏳ Waiting to depart for {dest_name}"))
                        .strong()
                        .size(14.0)
                        .color(egui::Color32::from_rgb(255, 200, 100)),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!("T-minus {}", wait_str))
                            .size(12.0)
                            .color(egui::Color32::GRAY),
                    );
                });
            });
            
            ui.add_space(4.0);
            if ui.button(egui::RichText::new("🛑 Abort Mission").size(12.0)).clicked() {
                pending_actions.cancel_maneuvers.push(fleet_entity);
            }
        });
        return;
    }

    let progress = maneuver.progress(elapsed) as f32;
    let remaining = format_duration(maneuver.time_remaining_s(elapsed));

    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!("✈ En Route → {dest_name}"))
                    .strong()
                    .size(14.0)
                    .color(egui::Color32::from_rgb(100, 200, 255)),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format!("{} remaining", remaining))
                        .size(12.0)
                        .color(egui::Color32::GRAY),
                );
            });
        });

        ui.add(
            egui::ProgressBar::new(progress)
                .text(format!("{:.1}%", progress * 100.0))
                .desired_width(ui.available_width()),
        );

        egui::Grid::new("maneuver_info")
            .num_columns(4)
            .spacing([16.0, 3.0])
            .show(ui, |ui| {
                ui.label(egui::RichText::new("Option:").size(12.0));
                ui.label(
                    egui::RichText::new(maneuver.option_label).size(12.0).strong(),
                );
                ui.label(egui::RichText::new("Arrival ΔV:").size(12.0));
                ui.label(
                    egui::RichText::new(format_delta_v(maneuver.arrival_delta_v_ms))
                        .size(12.0)
                        .strong(),
                );
                ui.end_row();

                ui.label(egui::RichText::new("Fuel used:").size(12.0));
                ui.label(
                    egui::RichText::new(format!("{:.0} t", maneuver.fuel_used_t))
                        .size(12.0)
                        .strong(),
                );
                let fuel_pct = (fleet.fuel_fraction() * 100.0) as u32;
                ui.label(egui::RichText::new("Remaining fuel:").size(12.0));
                ui.label(
                    egui::RichText::new(format!("{fuel_pct}%"))
                        .size(12.0)
                        .strong(),
                );
                ui.end_row();
            });
    });
}

/// Show stable orbit information.
fn render_orbit_status(
    ui: &mut egui::Ui,
    orbit: &FleetOrbit,
    fleet: &Fleet,
    body_query: &Query<(Entity, &CelestialBody, &SpaceCoordinates, Option<&KeplerOrbit>, Option<&LogicalParent>)>,
) {
    let (body_name, body_type) = body_query
        .get(orbit.body)
        .map(|(_, b, _, _, _)| (b.name.as_str(), b.body_type))
        .unwrap_or(("Unknown", BodyType::Planet));

    let fuel_pct = (fleet.fuel_fraction() * 100.0) as u32;

    // For star-orbiting fleets (Lagrange points), display heliocentric orbital
    // radius in AU rather than an altitude in km (which would be nonsensical
    // at interplanetary scales).
    let altitude_label;
    let altitude_value;
    if body_type == BodyType::Star {
        altitude_label = "Orbital radius:";
        altitude_value = format!("{:.4} AU", orbit.radius_au);
    } else {
        let radius_km = orbit.radius_au * AU_IN_METERS / 1_000.0;
        altitude_label = "Altitude:";
        altitude_value = format!("{:.0} km", radius_km);
    };

    // Label: for star-orbiting fleets say "at Lagrange point" to make it clear.
    let status_label = if body_type == BodyType::Star {
        format!("🛰 Lagrange Orbit ({body_name})")
    } else {
        format!("🛰 Orbiting {body_name}")
    };

    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(status_label)
                    .strong()
                    .size(14.0)
                    .color(egui::Color32::from_rgb(100, 220, 100)),
            );
        });
        egui::Grid::new("orbit_info")
            .num_columns(4)
            .spacing([16.0, 3.0])
            .show(ui, |ui| {
                ui.label(egui::RichText::new(altitude_label).size(12.0));
                ui.label(
                    egui::RichText::new(altitude_value)
                        .size(12.0)
                        .strong(),
                );
                ui.label(egui::RichText::new("Fuel:").size(12.0));
                ui.label(
                    egui::RichText::new(format!("{fuel_pct}%"))
                        .size(12.0)
                        .strong(),
                );
                ui.end_row();
            });
    });
}

/// Transfer planning sub-panel: choose a destination and transfer option.
#[allow(clippy::too_many_arguments)]
fn render_transfer_planner(
    ui: &mut egui::Ui,
    fleet_entity: Entity,
    fleet: &Fleet,
    orbit: &FleetOrbit,
    current_maneuver: Option<&ActiveManeuver>,
    body_query: &Query<(Entity, &CelestialBody, &SpaceCoordinates, Option<&KeplerOrbit>, Option<&LogicalParent>)>,
    all_fleets_query: &Query<(Entity, &Fleet, &SpaceCoordinates, Option<&FleetOrbit>, Option<&ActiveManeuver>), Without<CelestialBody>>,
    fleet_ui_state: &mut FleetUiState,
    pending_actions: &mut PendingFleetActions,
    current_system_id: usize,
    body_system_ids: &Query<&SystemId>,
    elapsed: f64,
    nearby_stars: &NearbyStarsData,
) {
    let is_course_correction = current_maneuver.is_some();

    if is_course_correction {
        ui.label(
            egui::RichText::new("🔄 Course Correction")
                .strong()
                .size(15.0)
                .color(egui::Color32::from_rgb(255, 200, 80)),
        );
        ui.label(
            egui::RichText::new("Redirecting mid-transit burns additional fuel for the abort maneuver.")
                .size(11.0)
                .italics()
                .color(egui::Color32::GRAY),
        );
    } else {
        ui.label(
            egui::RichText::new("📡 Orbital Transfer Planner")
                .strong()
                .size(15.0)
                .color(egui::Color32::from_rgb(200, 220, 255)),
        );
    }
    ui.separator();

    // ── Hierarchical destination selector ────────────────────────────────────
    // DestEntry variants:
    //   Header — non-clickable category label; separator drawn BEFORE it (but not the very first)
    //   Body   — selectable destination
    //   Ring   — selectable ring destination (no KeplerOrbit; radius from body.radius field)
    //   Lagrange — one of the 5 L-points of a planet-star system
    //   FleetTarget — another fleet (for intercept course)
    //   StarSystem — interstellar target (another star system)
    #[derive(Clone)]
    enum DestEntry {
        Header(String),
        Body { entity: Entity, name: String },
        // Rings are treated like regular bodies for selection; the extra
        // parent/radius information used to be stored here but never read.
        Ring { entity: Entity, name: String },
        Lagrange { lp: LagrangeTarget },
        FleetTarget { entity: Entity, name: String, in_transit: bool },
        StarSystem { system_id: usize, name: String, distance_ly: f32 },
    }

    let mut dest_entries: Vec<DestEntry> = Vec::new();

    // Collect all valid candidate bodies (exclude Star, include Ring)
    // For Rings: sma = None (no KeplerOrbit); radius stored via body.radius field separately.
    let candidates: Vec<(Entity, String, BodyType, Option<f64>, Option<Entity>)> = body_query
        .iter()
        .filter_map(|(e, body, _, maybe_ko, maybe_lp)| {
            if e == orbit.body { return None; }
            if body.body_type == BodyType::Star { return None; }
            if !body_system_ids.get(e).ok().map(|s| s.0 == current_system_id).unwrap_or(false) {
                return None;
            }
            let sma = maybe_ko.map(|ko| ko.semi_major_axis);
            let parent = maybe_lp.map(|lp| lp.0);
            Some((e, body.name.clone(), body.body_type, sma, parent))
        })
        .collect();

    // Separate ring bodies out; they lack KeplerOrbits so need special handling
    let ring_candidates: Vec<(Entity, String, Option<Entity>, f64)> = body_query
        .iter()
        .filter_map(|(e, body, _, _, maybe_lp)| {
            if body.body_type != BodyType::Ring { return None; }
            if !body_system_ids.get(e).ok().map(|s| s.0 == current_system_id).unwrap_or(false) {
                return None;
            }
            let parent = maybe_lp.map(|lp| lp.0)?;
            // Use body.radius (km) as the representative ring orbit distance from planet centre
            let radius_au = (body.radius as f64 * 1_000.0) / AU_IN_METERS;
            Some((e, body.name.clone(), Some(parent), radius_au))
        })
        .collect();

    // ── Group 1: bodies that directly orbit the fleet's current body ──────────
    {
        let orbit_body_name = body_query.get(orbit.body)
            .map(|(_, b, _, _, _)| b.name.clone()).unwrap_or_default();
        let mut local: Vec<(Entity, String, f64)> = candidates.iter()
            .filter(|(_, _, btype, _, parent)| {
                *parent == Some(orbit.body) && *btype != BodyType::Ring
            })
            .filter_map(|(e, name, _, sma, _)| sma.map(|s| (*e, name.clone(), s)))
            .collect();
        // Rings around the current orbit body
        let mut local_rings: Vec<(Entity, String, Option<Entity>, f64)> = ring_candidates.iter()
            .filter(|(_, _, parent, _)| *parent == Some(orbit.body))
            .cloned().collect();
        if !local.is_empty() || !local_rings.is_empty() {
            local.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
            local_rings.sort_by(|a, b| a.3.partial_cmp(&b.3).unwrap_or(std::cmp::Ordering::Equal));
            dest_entries.push(DestEntry::Header(format!("{orbit_body_name} System")));
            for (e, name, _) in &local {
                dest_entries.push(DestEntry::Body { entity: *e, name: name.clone() });
            }
            for (e, name, parent, _radius_au) in local_rings {
                if parent.is_some() {
                    dest_entries.push(DestEntry::Ring { entity: e, name });
                }
            }
        }

        // ── Lagrange points for the fleet's own orbit body (Sun-Planet) ──────
        if let Ok((_, orbit_body_data, _, Some(orbit_ko), _)) = body_query.get(orbit.body) {
            if matches!(orbit_body_data.body_type, BodyType::Planet | BodyType::GasGiant | BodyType::DwarfPlanet) {
                let a = orbit_ko.semi_major_axis;
                let m_star = 1.989e30_f64;
                let m_planet = orbit_body_data.mass;
                let r_hill = a * (m_planet / (3.0 * m_star)).powf(1.0 / 3.0);
                let l45_offset = r_hill * 0.05;
                let lp_radii: [(u8, f64); 5] = [
                    (1, (a - r_hill).max(1e-4)),
                    (2, a + r_hill),
                    (3, a),
                    (4, a + l45_offset),
                    (5, (a - l45_offset).max(1e-4)),
                ];
                dest_entries.push(DestEntry::Header(format!("{orbit_body_name} Lagrange Points")));
                for (point, radius_au) in lp_radii {
                    dest_entries.push(DestEntry::Lagrange {
                        lp: LagrangeTarget {
                            point,
                            planet_entity: orbit.body,
                            planet_name: orbit_body_name.clone(),
                            planet_sma_au: a,
                            radius_au,
                            gm: GM_SUN,
                        },
                    });
                }
            }
        }

        // ── Moon Lagrange points (Planet-Moon LPs for significant moons) ─────
        for (moon_e, moon_name, moon_sma) in &local {
            // Use the orbit body (planet) mass as central mass
            if let Ok((_, orbit_body_data, _, _, _)) = body_query.get(orbit.body) {
                if let Ok((_, moon_body, _, _, _)) = body_query.get(*moon_e) {
                    let a_moon = *moon_sma;
                    let m_planet = orbit_body_data.mass;
                    let m_moon = moon_body.mass;
                    if m_moon > 0.0 && m_planet > 0.0 {
                        let r_hill = a_moon * (m_moon / (3.0 * m_planet)).powf(1.0 / 3.0);
                        let l45_offset = r_hill * 0.05;
                        let lp_radii: [(u8, f64); 5] = [
                            (1, (a_moon - r_hill).max(1e-6)),
                            (2, a_moon + r_hill),
                            (3, a_moon),
                            (4, a_moon + l45_offset),
                            (5, (a_moon - l45_offset).max(1e-6)),
                        ];
                        dest_entries.push(DestEntry::Header(format!("{moon_name} Lagrange Points")));
                        for (point, radius_au) in lp_radii {
                            dest_entries.push(DestEntry::Lagrange {
                                lp: LagrangeTarget {
                                    point,
                                    planet_entity: *moon_e,
                                    planet_name: moon_name.clone(),
                                    planet_sma_au: a_moon,
                                    radius_au,
                                    gm: G_CONST * m_planet,
                                },
                            });
                        }
                    }
                }
            }
        }
    }

    // ── Groups 2+: planet systems (moons/rings orbiting a planet that isn't fleet's body) ──
    let mut planet_map: std::collections::BTreeMap<String, (Entity, f64, Vec<(Entity, String, f64, bool)>)> =
        std::collections::BTreeMap::new();

    // Regular moons / small bodies orbiting a planet
    for (e, name, btype, sma, parent) in &candidates {
        if *btype == BodyType::Ring { continue; }
        let parent_e = match parent { Some(p) => *p, None => continue };
        if parent_e == orbit.body { continue; }
        if let Ok((_, pb, _, parent_ko, _)) = body_query.get(parent_e) {
            if pb.body_type == BodyType::Star { continue; }
            let parent_sma = parent_ko.map(|ko| ko.semi_major_axis).unwrap_or(0.0);
            if let Some(s) = sma {
                planet_map.entry(pb.name.clone())
                    .or_insert_with(|| (parent_e, parent_sma, vec![]))
                    .2.push((*e, name.clone(), *s, false)); // false = not a ring
            }
        }
    }
    // Rings orbiting a planet that isn't the fleet's body
    for (e, name, parent_opt, radius_au) in &ring_candidates {
        let parent_e = match parent_opt { Some(p) => *p, None => continue };
        if parent_e == orbit.body { continue; }
        if let Ok((_, pb, _, parent_ko, _)) = body_query.get(parent_e) {
            if pb.body_type == BodyType::Star { continue; }
            let parent_sma = parent_ko.map(|ko| ko.semi_major_axis).unwrap_or(0.0);
            planet_map.entry(pb.name.clone())
                .or_insert_with(|| (parent_e, parent_sma, vec![]))
                .2.push((*e, name.clone(), *radius_au, true)); // true = ring
        }
    }

    let mut sorted_planet_systems: Vec<_> = planet_map.into_iter().collect();
    sorted_planet_systems.sort_by(|a, b| a.1.1.partial_cmp(&b.1.1).unwrap_or(std::cmp::Ordering::Equal));

    let mut planets_shown = std::collections::HashSet::<Entity>::new();
    for (planet_name, (parent_e, _parent_sma, mut children)) in sorted_planet_systems {
        planets_shown.insert(parent_e);
        children.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
        dest_entries.push(DestEntry::Header(format!("{planet_name} System")));
        if orbit.body != parent_e {
            dest_entries.push(DestEntry::Body { entity: parent_e, name: planet_name.clone() });
        }
        for (e, name, _sma, is_ring) in &children {
            if *is_ring {
                dest_entries.push(DestEntry::Ring {
                    entity: *e,
                    name: name.clone(),
                });
            } else {
                dest_entries.push(DestEntry::Body { entity: *e, name: name.clone() });
            }
        }
        // ── Lagrange points sub-group ──────────────────────────────────────
        // Compute Hill-sphere radius from planet's heliocentric SMA and mass.
        if let Ok((_, planet_body, _, Some(planet_ko), _)) = body_query.get(parent_e) {
            let a = planet_ko.semi_major_axis; // AU
            let m_star = 1.989e30_f64; // Solar mass (kg)
            let m_planet = planet_body.mass;
            // r_hill ≈ a * (m_planet / (3 * m_star))^(1/3) [AU]
            let r_hill = a * (m_planet / (3.0 * m_star)).powf(1.0 / 3.0);
            // L4/L5 use a tiny offset so the degenerate same-orbit case gives a
            // small but non-zero phasing ΔV rather than exactly 0 km/s.
            let l45_offset = r_hill * 0.05;
            let lp_radii: [(u8, f64); 5] = [
                (1, (a - r_hill).max(1e-4)),
                (2, a + r_hill),
                (3, a),       // opposite side — same SMA, handled as special note in UI
                (4, a + l45_offset),
                (5, (a - l45_offset).max(1e-4)),
            ];
            dest_entries.push(DestEntry::Header(format!("{planet_name} Lagrange Points")));
            for (point, radius_au) in lp_radii {
                dest_entries.push(DestEntry::Lagrange {
                    lp: LagrangeTarget {
                        point,
                        planet_entity: parent_e,
                        planet_name: planet_name.clone(),
                        planet_sma_au: a,
                        radius_au,
                        gm: GM_SUN,
                    },
                });
            }

            // ── Moon Lagrange points (Planet-Moon LPs) ─────────────────────
            for (child_e, child_name, _child_sma, is_ring) in &children {
                if *is_ring { continue; }
                if let Ok((_, moon_body, _, Some(moon_ko), _)) = body_query.get(*child_e) {
                    let a_moon = moon_ko.semi_major_axis;
                    let m_moon = moon_body.mass;
                    if m_moon > 0.0 && m_planet > 0.0 {
                        let r_hill_m = a_moon * (m_moon / (3.0 * m_planet)).powf(1.0 / 3.0);
                        let l45_m = r_hill_m * 0.05;
                        let lp_moon: [(u8, f64); 5] = [
                            (1, (a_moon - r_hill_m).max(1e-6)),
                            (2, a_moon + r_hill_m),
                            (3, a_moon),
                            (4, a_moon + l45_m),
                            (5, (a_moon - l45_m).max(1e-6)),
                        ];
                        dest_entries.push(DestEntry::Header(format!("{child_name} Lagrange Points")));
                        for (pt, r_au) in lp_moon {
                            dest_entries.push(DestEntry::Lagrange {
                                lp: LagrangeTarget {
                                    point: pt,
                                    planet_entity: *child_e,
                                    planet_name: child_name.clone(),
                                    planet_sma_au: a_moon,
                                    radius_au: r_au,
                                    gm: G_CONST * m_planet,
                                },
                            });
                        }
                    }
                }
            }
        }
    }

    // ── Group: Planets/GasGiants not yet shown (no children found in data) ───
    let already_listed: std::collections::HashSet<Entity> = dest_entries.iter()
        .filter_map(|de| match de {
            DestEntry::Body { entity, .. } | DestEntry::Ring { entity, .. } => Some(*entity),
            _ => None,
        })
        .collect();

    let mut standalone: Vec<(Entity, String, f64)> = candidates.iter()
        .filter(|(e, _, btype, sma, _)| {
            matches!(btype, BodyType::Planet | BodyType::GasGiant | BodyType::DwarfPlanet)
                && sma.is_some()
                && !planets_shown.contains(e)
                && !already_listed.contains(e)
                && orbit.body != *e
        })
        .map(|(e, name, _, sma, _)| (*e, name.clone(), sma.unwrap()))
        .collect();
    if !standalone.is_empty() {
        standalone.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
        dest_entries.push(DestEntry::Header("Planets".to_string()));
        for (e, name, _) in standalone {
            dest_entries.push(DestEntry::Body { entity: e, name });
        }
    }

    // ── Group: Small bodies ─────────────────────────────────────────────────
    let already_listed2: std::collections::HashSet<Entity> = dest_entries.iter()
        .filter_map(|de| match de {
            DestEntry::Body { entity, .. } | DestEntry::Ring { entity, .. } => Some(*entity),
            _ => None,
        })
        .collect();
    let mut small_bodies: Vec<(Entity, String, f64)> = candidates.iter()
        .filter(|(e, _, btype, sma, _)| {
            matches!(btype, BodyType::Asteroid | BodyType::Comet)
                && sma.is_some()
                && !already_listed2.contains(e)
                && orbit.body != *e
        })
        .map(|(e, name, _, sma, _)| (*e, name.clone(), sma.unwrap()))
        .collect();
    if !small_bodies.is_empty() {
        small_bodies.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
        let sb_label = if small_bodies.len() > 5 {
            format!("Small Bodies ({} total)", small_bodies.len())
        } else {
            "Small Bodies".to_string()
        };
        dest_entries.push(DestEntry::Header(sb_label));
        for (e, name, _) in small_bodies {
            dest_entries.push(DestEntry::Body { entity: e, name });
        }
    }

    // ── Group: Solar Approach ────────────────────────────────────────────────
    // Always offer a direct solar-approach destination so the player can plot
    // an inward heliocentric transfer toward the star.  Filter by current_system_id
    // to find Sol, not Alpha Centauri or another star from a different system.
    let star_entity = body_query.iter()
        .find(|(e, b, _, _, _)| {
            b.body_type == BodyType::Star
                && body_system_ids.get(*e).ok().map(|s| s.0 == current_system_id).unwrap_or(false)
        })
        .map(|(e, _, _, _, _)| e);
    if let Some(star_e) = star_entity {
        dest_entries.push(DestEntry::Header("Solar".to_string()));
        dest_entries.push(DestEntry::Body {
            entity: star_e,
            name: "☀ Solar Approach (0.3 AU)".to_string(),
        });
    }

    // ── Group: Interstellar ──────────────────────────────────────────────────
    // List every other star system from NearbyStarsData as an interstellar target.
    // The current system is identified by its numeric id; Sol = id 0 by convention.
    {
        let mut interstellar_entries: Vec<DestEntry> = nearby_stars.systems
            .iter()
            .filter(|sys| {
                // Exclude the current system (id comparison via name match is a fallback)
                // NearbyStarsData systems use 0-based index ordering; system_id 0 = Sol.
                // We exclude any system whose name matches current system's star name.
                let this_star_name = body_query.iter()
                    .find(|(e, b, _, _, _)| {
                        b.body_type == BodyType::Star
                            && body_system_ids.get(*e).ok()
                                .map(|s| s.0 == current_system_id)
                                .unwrap_or(false)
                    })
                    .map(|(_, b, _, _, _)| b.name.as_str())
                    .unwrap_or("Sol");
                // Each StarSystemData has stars[0].name; compare to current star
                !sys.stars.iter().any(|s| s.name == this_star_name)
                    && sys.distance_ly > 0.0
            })
            .enumerate()
            .map(|(idx, sys)| {
                let display = format!("✨ {} ({:.2} ly)", sys.system_name, sys.distance_ly);
                // Use index+1 as system_id (0 reserved for Sol in current system)
                DestEntry::StarSystem {
                    system_id: idx + 1,
                    name: display,
                    distance_ly: sys.distance_ly,
                }
            })
            .collect();

        if !interstellar_entries.is_empty() {
            interstellar_entries.sort_by(|a, b| {
                let da = if let DestEntry::StarSystem { distance_ly, .. } = a { *distance_ly } else { 0.0 };
                let db = if let DestEntry::StarSystem { distance_ly, .. } = b { *distance_ly } else { 0.0 };
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            });
            dest_entries.push(DestEntry::Header("Interstellar".to_string()));
            dest_entries.extend(interstellar_entries);
        }
    }

    // ── Build hierarchical categories from dest_entries ─────────────────────
    // Top-level headers ("…System", "Small Bodies", "Heliocentric") become
    // category names in the first-level picker. Lagrange sub-headers are kept
    // as visual separators inside each category group.
    #[derive(Clone)]
    struct DestGroup {
        name: String,
        entries: Vec<DestEntry>,
    }

    let mut groups: Vec<DestGroup> = Vec::new();
    for entry in dest_entries {
        let is_top_header = match &entry {
            DestEntry::Header(label) => {
                label.ends_with(" System")
                    || label == "Planets"
                    || label == "Solar"
                    || label == "Interstellar"
                    || label.starts_with("Small Bodies")
            }
            _ => false,
        };
        if is_top_header {
            let name = match &entry {
                DestEntry::Header(label) => {
                    label.strip_suffix(" System").unwrap_or(label).to_string()
                }
                _ => unreachable!(),
            };
            groups.push(DestGroup { name, entries: Vec::new() });
        } else if let Some(g) = groups.last_mut() {
            g.entries.push(entry);
        }
    }

    // ── Fleet intercept category ─────────────────────────────────────────────
    {
        let other_fleets: Vec<(Entity, String, bool)> = all_fleets_query
            .iter()
            .filter(|(e, _, _, _, _)| *e != fleet_entity)
            .map(|(e, f, _, _, maybe_ma)| (e, f.name.clone(), maybe_ma.is_some()))
            .collect();
        if !other_fleets.is_empty() {
            let mut fleet_group = DestGroup { name: "Fleets".to_string(), entries: Vec::new() };
            // In-orbit fleets first
            for (e, name, in_transit) in &other_fleets {
                fleet_group.entries.push(DestEntry::FleetTarget {
                    entity: *e,
                    name: name.clone(),
                    in_transit: *in_transit,
                });
            }
            groups.push(fleet_group);
        }
    }

    // ── Auto-select category if a target is selected ─────────────────────────
    let mut correct_category = None;
    if let Some(target) = fleet_ui_state.target_body {
        for group in &groups {
            if group.entries.iter().any(|e| match e {
                DestEntry::Body { entity, .. } | DestEntry::Ring { entity, .. } => *entity == target,
                _ => false,
            }) {
                correct_category = Some(group.name.clone());
                break;
            }
        }
    } else if let Some(ref lp) = fleet_ui_state.target_lagrange {
        for group in &groups {
            if group.entries.iter().any(|e| match e {
                DestEntry::Lagrange { lp: entry_lp } => entry_lp.point == lp.point && entry_lp.planet_entity == lp.planet_entity,
                _ => false,
            }) {
                correct_category = Some(group.name.clone());
                break;
            }
        }
    } else if let Some(tf) = fleet_ui_state.target_fleet {
        for group in &groups {
            if group.entries.iter().any(|e| match e {
                DestEntry::FleetTarget { entity, .. } => *entity == tf,
                _ => false,
            }) {
                correct_category = Some(group.name.clone());
                break;
            }
        }
    } else if let Some((tss_id, _, _)) = fleet_ui_state.target_star_system {
        for group in &groups {
            if group.entries.iter().any(|e| match e {
                DestEntry::StarSystem { system_id, .. } => *system_id == tss_id,
                _ => false,
            }) {
                correct_category = Some(group.name.clone());
                break;
            }
        }
    }

    if let Some(cat) = correct_category {
        let sel = fleet_ui_state.selected_dest_category.as_deref();
        if sel != Some(&cat) && !(sel == Some("Small Bodies") && cat.starts_with("Small Bodies")) {
            fleet_ui_state.selected_dest_category = Some(cat);
        }
    }

    // ── Render the two-level selector ────────────────────────────────────────
    // Step 1: category (planet system / small bodies / fleets)
    let cat_label = groups.iter().find(|g| {
        let sel = fleet_ui_state.selected_dest_category.as_deref();
        sel == Some(&g.name) || (sel == Some("Small Bodies") && g.name.starts_with("Small Bodies"))
    }).map(|g| g.name.clone()).unwrap_or_else(|| fleet_ui_state.selected_dest_category.clone().unwrap_or_else(|| "— System —".to_owned()));

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("System:").size(13.0));
        egui::ComboBox::from_id_salt("fleet_dest_category")
            .selected_text(&cat_label)
            .width(200.0)
            .show_ui(ui, |ui| {
                for group in &groups {
                    let sel = fleet_ui_state.selected_dest_category.as_deref();
                    let cat_is_sel = sel == Some(&group.name) || (sel == Some("Small Bodies") && group.name.starts_with("Small Bodies"));
                    if ui.selectable_label(
                        cat_is_sel,
                        egui::RichText::new(&group.name).size(13.0),
                    ).clicked() && !cat_is_sel {
                        fleet_ui_state.selected_dest_category = Some(group.name.clone());
                        // Clear the specific target so the second step is re-selected
                        fleet_ui_state.target_body = None;
                        fleet_ui_state.target_lagrange = None;
                        fleet_ui_state.target_fleet = None;
                        fleet_ui_state.target_star_system = None;
                        fleet_ui_state.computed_options.clear();
                        fleet_ui_state.planned_transfer = None;
                        fleet_ui_state.selected_option = 0;
                        fleet_ui_state.selected_gravity_assist = None;
                    }
                }
            });
    });

    // Step 2: specific target within selected category
    let active_group = groups.iter().find(|g| {
        let sel = fleet_ui_state.selected_dest_category.as_deref();
        sel == Some(&g.name) || (sel == Some("Small Bodies") && g.name.starts_with("Small Bodies"))
    });

    let target_label = if let Some(ref lp) = fleet_ui_state.target_lagrange {
        format!("L{} {} — {}", lp.point, lp.planet_name, lp.qualifier())
    } else if let Some(tf) = fleet_ui_state.target_fleet {
        all_fleets_query.get(tf)
            .map(|(_, f, _, _, ma)| {
                let status = if ma.is_some() { "✈" } else { "🛰" };
                format!("{status} {}", f.name)
            })
            .unwrap_or_else(|_| "— Target —".to_owned())
    } else if let Some((_, ref name, _)) = fleet_ui_state.target_star_system {
        name.clone()
    } else {
        fleet_ui_state.target_body
            .and_then(|e| body_query.get(e).ok())
            .map(|(_, b, _, _, _)| {
                if b.body_type == BodyType::Ring {
                    format!("{} 💍", b.name)
                } else {
                    b.name.clone()
                }
            })
            .unwrap_or_else(|| "— Target —".to_owned())
    };

    if active_group.is_some() {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Target:").size(13.0));
            egui::ComboBox::from_id_salt("fleet_target_body")
                .selected_text(&target_label)
                .width(280.0)
                .show_ui(ui, |ui| {
                    if let Some(group) = active_group {
                        let mut first_sub = true;
                        for entry in &group.entries {
                            match entry {
                                DestEntry::Header(label) => {
                                    if !first_sub { ui.add_space(4.0); }
                                    first_sub = false;
                                    ui.label(
                                        egui::RichText::new(label.as_str())
                                            .strong()
                                            .size(11.0)
                                            .color(egui::Color32::from_rgb(180, 180, 100)),
                                    );
                                }
                                DestEntry::Body { entity, name } => {
                                    first_sub = false;
                                    let selected = fleet_ui_state.target_body == Some(*entity)
                                        && fleet_ui_state.target_lagrange.is_none()
                                        && fleet_ui_state.target_fleet.is_none();
                                    if ui.selectable_label(
                                        selected,
                                        egui::RichText::new(format!("  {name}")).size(12.0),
                                    ).clicked() && !selected {
                                        fleet_ui_state.target_body = Some(*entity);
                                        fleet_ui_state.target_lagrange = None;
                                        fleet_ui_state.target_fleet = None;
                                        fleet_ui_state.computed_options.clear();
                                        fleet_ui_state.planned_transfer = None;
                                        fleet_ui_state.selected_option = 0;
                                        fleet_ui_state.selected_gravity_assist = None;
                                    }
                                }
                                DestEntry::Ring { entity, name } => {
                                    first_sub = false;
                                    let selected = fleet_ui_state.target_body == Some(*entity)
                                        && fleet_ui_state.target_lagrange.is_none()
                                        && fleet_ui_state.target_fleet.is_none();
                                    if ui.selectable_label(
                                        selected,
                                        egui::RichText::new(format!("  {name} 💍")).size(12.0),
                                    ).clicked() && !selected {
                                        fleet_ui_state.target_body = Some(*entity);
                                        fleet_ui_state.target_lagrange = None;
                                        fleet_ui_state.target_fleet = None;
                                        fleet_ui_state.computed_options.clear();
                                        fleet_ui_state.planned_transfer = None;
                                        fleet_ui_state.selected_option = 0;
                                        fleet_ui_state.selected_gravity_assist = None;
                                    }
                                }
                                DestEntry::Lagrange { lp } => {
                                    first_sub = false;
                                    let is_sel = fleet_ui_state.target_lagrange.as_ref()
                                        .map(|cur| cur.point == lp.point && cur.planet_entity == lp.planet_entity)
                                        .unwrap_or(false);
                                    let lp_label = format!("  L{}  —  {}", lp.point, lp.qualifier());
                                    if ui.selectable_label(
                                        is_sel,
                                        egui::RichText::new(lp_label)
                                            .size(12.0)
                                            .color(egui::Color32::from_rgb(140, 210, 160)),
                                    ).clicked() && !is_sel {
                                        fleet_ui_state.target_body = None;
                                        fleet_ui_state.target_lagrange = Some(lp.clone());
                                        fleet_ui_state.target_fleet = None;
                                        fleet_ui_state.computed_options.clear();
                                        fleet_ui_state.planned_transfer = None;
                                        fleet_ui_state.selected_option = 0;
                                        fleet_ui_state.selected_gravity_assist = None;
                                    }
                                }
                                DestEntry::FleetTarget { entity, name, in_transit } => {
                                    first_sub = false;
                                    let is_sel = fleet_ui_state.target_fleet == Some(*entity);
                                    let icon = if *in_transit { "✈" } else { "🛰" };
                                    let status = if *in_transit { "In transit" } else { "In orbit" };
                                    let label = format!("  {icon} {name}  ({status})");
                                    if ui.selectable_label(
                                        is_sel,
                                        egui::RichText::new(label)
                                            .size(12.0)
                                            .color(egui::Color32::from_rgb(100, 210, 240)),
                                    ).clicked() && !is_sel {
                                        fleet_ui_state.target_fleet = Some(*entity);
                                        fleet_ui_state.target_body = None;
                                        fleet_ui_state.target_lagrange = None;
                                        fleet_ui_state.target_star_system = None;
                                        fleet_ui_state.computed_options.clear();
                                        fleet_ui_state.planned_transfer = None;
                                        fleet_ui_state.selected_option = 0;
                                        fleet_ui_state.selected_gravity_assist = None;
                                    }
                                }
                                DestEntry::StarSystem { system_id, name, distance_ly } => {
                                    first_sub = false;
                                    let is_sel = fleet_ui_state.target_star_system
                                        .as_ref().map(|(id, _, _)| *id == *system_id)
                                        .unwrap_or(false);
                                    if ui.selectable_label(
                                        is_sel,
                                        egui::RichText::new(format!("  {name}"))
                                            .size(12.0)
                                            .color(egui::Color32::from_rgb(200, 180, 255)),
                                    ).clicked() && !is_sel {
                                        fleet_ui_state.target_star_system = Some((*system_id, name.clone(), *distance_ly));
                                        fleet_ui_state.target_body = None;
                                        fleet_ui_state.target_lagrange = None;
                                        fleet_ui_state.target_fleet = None;
                                        fleet_ui_state.computed_options.clear();
                                        fleet_ui_state.planned_transfer = None;
                                        fleet_ui_state.selected_option = 0;
                                        fleet_ui_state.selected_gravity_assist = None;
                                    }
                                }
                            }
                        }
                    }
                });
        });
    }

    // ── Intercept parameters (shown only when a fleet is targeted) ────────────
    if fleet_ui_state.target_fleet.is_some() {
        ui.add_space(6.0);
        ui.group(|ui| {
            ui.label(
                egui::RichText::new("⚔ Intercept Parameters")
                    .strong()
                    .size(13.0)
                    .color(egui::Color32::from_rgb(220, 160, 80)),
            );
            ui.add_space(4.0);

            // Passing distance slider: 0 = rendezvous / dock, up to 1 000 km = fast flyby
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Passing distance:").size(12.0));
                let mut pd = fleet_ui_state.intercept_passing_km as f32;
                if ui.add(
                    egui::Slider::new(&mut pd, 0.0_f32..=1_000.0_f32)
                        .suffix(" km")
                        .text("0 = rendezvous")
                        .step_by(10.0),
                ).changed() {
                    fleet_ui_state.intercept_passing_km = pd as f64;
                    fleet_ui_state.computed_options.clear();
                }
            });

            // Encounter speed: 0 = match velocity (boarding), up to 30 km/s = high-speed pass
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Encounter speed:").size(12.0));
                let mut spd_kms = (fleet_ui_state.intercept_speed_ms / 1_000.0) as f32;
                if ui.add(
                    egui::Slider::new(&mut spd_kms, 0.0_f32..=30.0_f32)
                        .suffix(" km/s")
                        .text("0 = match velocity")
                        .step_by(0.5),
                ).changed() {
                    fleet_ui_state.intercept_speed_ms = spd_kms as f64 * 1_000.0;
                    fleet_ui_state.computed_options.clear();
                }
            });

            ui.label(
                egui::RichText::new(
                    if fleet_ui_state.intercept_passing_km < 1.0 && fleet_ui_state.intercept_speed_ms < 100.0 {
                        "Mode: Rendezvous / docking approach"
                    } else if fleet_ui_state.intercept_passing_km > 100.0 || fleet_ui_state.intercept_speed_ms > 5_000.0 {
                        "Mode: High-speed flyby (combat pass)"
                    } else {
                        "Mode: Close approach (boarding range)"
                    }
                )
                .size(11.0)
                .italics()
                .color(egui::Color32::from_rgb(160, 200, 160)),
            );
        });
    }

    // ── Compute transfer options when a target is selected ───────────────────
    let fleet_target_snap = fleet_ui_state.target_fleet;
    let star_system_snap = fleet_ui_state.target_star_system.clone();
    let any_target = fleet_ui_state.target_body.is_some()
        || fleet_ui_state.target_lagrange.is_some()
        || fleet_target_snap.is_some()
        || star_system_snap.is_some();
    // Snapshot lagrange so we can use it immutably while also mut-borrowing fleet_ui_state below
    let lp_target_snap = fleet_ui_state.target_lagrange.clone();
    let body_target_snap = fleet_ui_state.target_body;

    // Transfer window info computed this frame (Some only for body-target transfers).
    // Kept as a local so the window UI section can read it without re-computing.
    let mut window_this_frame: Option<TransferWindowInfo> = None;
    let mut window_max_slider_days: f64 = 730.0;

    if any_target {
        // Recompute every frame — body angles (SpaceCoordinates) update with the simulation clock,
        // so the phase error and launch-window countdown change live.

        // ── Fleet intercept computation ──────────────────────────────────────
        if let Some(target_fleet_entity) = fleet_target_snap {
            // Use the target fleet's current heliocentric position as the intercept radius.
            // r2 = distance from origin (0,0,0) to target fleet position in AU.
            let target_sc = all_fleets_query.get(target_fleet_entity)
                .map(|(_, _, sc, _, _)| sc.position)
                .unwrap_or(bevy::math::DVec3::ZERO);
            let r2_au = target_sc.length().max(0.001);

            // r1: heliocentric distance of the departing fleet
            let r1_au = {
                let own_ko = body_query.get(orbit.body).ok()
                    .and_then(|(_, _, _, ko, _)| ko)
                    .map(|ko| ko.semi_major_axis);
                let origin_parent = body_query.get(orbit.body).ok()
                    .and_then(|(_, _, _, _, lp)| lp).map(|lp| lp.0);
                if own_ko.map(|s| s < MIN_HELIOCENTRIC_SMA_AU).unwrap_or(true) {
                    origin_parent
                        .and_then(|pe| body_query.get(pe).ok())
                        .and_then(|(_, _, _, ko, _)| ko)
                        .map(|ko| ko.semi_major_axis)
                        .or(own_ko)
                        .unwrap_or(1.0)
                } else {
                    own_ko.unwrap_or(1.0)
                }
            };
            fleet_ui_state.computed_options = calculate_transfer_options(r1_au, r2_au, GM_SUN);
            // Post-process: fill burn_time_s and flag thrust-limited options.
            apply_thrust_limits(
                &mut fleet_ui_state.computed_options,
                fleet.min_accel_ms2(),
                fleet.average_isp_s(),
            );
            // Add kinematic options for high-thrust fleets intercepting other fleets.
            let hohmann_dv = fleet_ui_state.computed_options.first().map(|o| o.total_delta_v_ms).unwrap_or(0.0);
            let sma_h = fleet_ui_state.computed_options.first().map(|o| o.sma_au).unwrap_or(0.0);
            let ecc_h = fleet_ui_state.computed_options.first().map(|o| o.eccentricity).unwrap_or(0.0);
            let d = (r2_au - r1_au).abs() * crate::fleets::orbital_mechanics::AU_IN_METERS;
            let mut kinematics = kinematic_transfer_options(
                d, fleet.min_accel_ms2(), fleet.max_delta_v_ms(),
                hohmann_dv, sma_h, ecc_h, false
            );
            fleet_ui_state.computed_options.append(&mut kinematics);
        } else if let Some(target_entity) = body_target_snap {
            //   - Ring transfer (dest has no KeplerOrbit; use body.radius as r2):
            //       r1 = fleet orbit radius or origin SMA, r2 = ring.radius_au, GM = parent mass * G
            //   - Local transfer (dest orbits fleet's body, e.g. Earth→Moon):
            //       r1 = fleet's parking orbit radius, r2 = dest SMA, GM = parent mass * G
            //   - Moon-to-moon (both orbit the same planet):
            //       r1 = origin moon SMA, r2 = dest moon SMA, GM = shared planet mass * G
            //   - Solar approach (dest is a star):
            //       r1 = fleet's heliocentric SMA, r2 = 0.3 AU, GM = GM_SUN
            //   - Heliocentric transfer (both in heliocentric orbits):
            //       r1 = origin body heliocentric SMA, r2 = dest heliocentric SMA, GM_SUN
            let dest_body_type = body_query.get(target_entity).ok()
                .map(|(_, b, _, _, _)| b.body_type);
            let dest_has_orbit = body_query.get(target_entity).ok()
                .and_then(|(_, _, _, ko, _)| ko).is_some();
            let dest_parent = body_query.get(target_entity).ok()
                .and_then(|(_, _, _, _, lp)| lp).map(|lp| lp.0);
            let origin_parent = body_query.get(orbit.body).ok()
                .and_then(|(_, _, _, _, lp)| lp).map(|lp| lp.0);

            // Target solar approach orbit (AU from star).  Inside Mercury's orbit so the
            // transfer is always clearly "inward".  Requires advanced propulsion (~10–20 km/s).
            const SOLAR_APPROACH_AU: f64 = 0.3;

            let (r1, r2, gm) = if dest_body_type == Some(BodyType::Star) {
                // Heliocentric inward transfer: plot a Hohmann from the fleet's heliocentric
                // distance to SOLAR_APPROACH_AU using GM_SUN as the central-body parameter.
                // Walk up the parent chain to find the fleet's heliocentric SMA.
                let own_sma = body_query.get(orbit.body).ok()
                    .and_then(|(_, _, _, ko, _)| ko)
                    .map(|ko| ko.semi_major_axis);
                let r1_au = if own_sma.map(|s| s < MIN_HELIOCENTRIC_SMA_AU).unwrap_or(true) {
                    // Fleet is parked at a moon/sub-body; use its planet's heliocentric SMA.
                    origin_parent
                        .and_then(|pe| body_query.get(pe).ok())
                        .and_then(|(_, _, _, ko, _)| ko)
                        .map(|ko| ko.semi_major_axis)
                        .or(own_sma)
                        .unwrap_or(1.0)
                } else {
                    own_sma.unwrap_or(1.0)
                };
                // Ensure r2 is strictly less than r1 (always an inward transfer).
                let r2_au = SOLAR_APPROACH_AU.min(r1_au * 0.5);
                (r1_au, r2_au, GM_SUN)
            } else if !dest_has_orbit && dest_parent == Some(orbit.body) {
                // Ring around current orbit body
                let parent_mass = body_query.get(orbit.body).ok()
                    .map(|(_, b, _, _, _)| b.mass).unwrap_or(5.972e24);
                let r2 = body_query.get(target_entity).ok()
                    .map(|(_, b, _, _, _)| (b.radius as f64 * 1_000.0) / AU_IN_METERS)
                    .unwrap_or(0.001);
                (orbit.radius_au, r2, G_CONST * parent_mass)
            } else if !dest_has_orbit && dest_parent.is_some() && dest_parent == origin_parent {
                // Ring around another planet (dest_parent is a planet, not fleet's body)
                let shared = dest_parent.unwrap();
                let parent_mass = body_query.get(shared).ok()
                    .map(|(_, b, _, _, _)| b.mass).unwrap_or(5.972e24);
                let r1 = body_query.get(orbit.body).ok()
                    .and_then(|(_, _, _, ko, _)| ko)
                    .map(|ko| ko.semi_major_axis).unwrap_or(0.00257);
                let r2 = body_query.get(target_entity).ok()
                    .map(|(_, b, _, _, _)| (b.radius as f64 * 1_000.0) / AU_IN_METERS)
                    .unwrap_or(0.001);
                (r1, r2, G_CONST * parent_mass)
            } else if dest_parent == Some(orbit.body) {
                // Local: destination orbits the fleet's current body
                let parent_mass = body_query.get(orbit.body).ok()
                    .map(|(_, b, _, _, _)| b.mass).unwrap_or(5.972e24);
                let r2 = body_query.get(target_entity).ok()
                    .and_then(|(_, _, _, ko, _)| ko)
                    .map(|ko| ko.semi_major_axis).unwrap_or(0.00257);
                (orbit.radius_au, r2, G_CONST * parent_mass)
            } else if dest_parent.is_some() && dest_parent == origin_parent {
                // Both orbit the same central body (moon-to-moon, OR interplanetary e.g. Earth→Mars)
                let shared = dest_parent.unwrap();
                // NOTE: The Sun lacks SpaceCoordinates, so body_query.get(Sun) fails.
                // Fall back to GM_SUN so interplanetary transfers compute correctly.
                let gm = body_query.get(shared).ok()
                    .map(|(_, b, _, _, _)| {
                        if b.body_type == BodyType::Star { GM_SUN } else { G_CONST * b.mass }
                    })
                    .unwrap_or(GM_SUN);
                let r1 = body_query.get(orbit.body).ok()
                    .and_then(|(_, _, _, ko, _)| ko)
                    .map(|ko| ko.semi_major_axis).unwrap_or(0.00257);
                let r2 = body_query.get(target_entity).ok()
                    .and_then(|(_, _, _, ko, _)| ko)
                    .map(|ko| ko.semi_major_axis).unwrap_or(0.00257);
                (r1, r2, gm)
            } else if Some(target_entity) == origin_parent {
                // Downward transfer: fleet is at a moon, destination is the parent planet.
                // e.g. Moon → Earth: r1 = Moon SMA around Earth, r2 = low parking orbit, gm = planet GM.
                let parent_mass = body_query.get(target_entity).ok()
                    .map(|(_, b, _, _, _)| b.mass).unwrap_or(5.972e24);
                let r1 = body_query.get(orbit.body).ok()
                    .and_then(|(_, _, _, ko, _)| ko)
                    .map(|ko| ko.semi_major_axis).unwrap_or(0.00257);
                // Park at ~3× destination body surface radius (low orbit).
                let r2 = body_query.get(target_entity).ok()
                    .map(|(_, b, _, _, _)| (b.radius as f64 * 3_000.0) / AU_IN_METERS)
                    .unwrap_or(4.26e-5);
                (r1, r2.min(r1 * 0.5), G_CONST * parent_mass)
            } else {
                // Heliocentric: fleet is at a body that is not in the same parent chain as dest.
                // If fleet is parked at a moon, its KeplerOrbit SMA is Earth-relative, NOT
                // heliocentric. Walk up one level to get the heliocentric SMA.
                let own_sma = body_query.get(orbit.body).ok()
                    .and_then(|(_, _, _, ko, _)| ko)
                    .map(|ko| ko.semi_major_axis);
                let r1 = if own_sma.map(|s| s < MIN_HELIOCENTRIC_SMA_AU).unwrap_or(true) {
                    // Small SMA → likely a moon; use its parent's heliocentric SMA
                    origin_parent
                        .and_then(|pe| body_query.get(pe).ok())
                        .and_then(|(_, _, _, ko, _)| ko)
                        .map(|ko| ko.semi_major_axis)
                        .or(own_sma)
                        .unwrap_or(1.0)
                } else {
                    own_sma.unwrap_or(1.0)
                };
                let dest_sma = body_query.get(target_entity).ok()
                    .and_then(|(_, _, _, ko, _)| ko)
                    .map(|ko| ko.semi_major_axis);
                let r2 = if dest_sma.map(|s| s < MIN_HELIOCENTRIC_SMA_AU).unwrap_or(true) {
                    // Small SMA → likely a moon; use its parent's heliocentric SMA
                    dest_parent
                        .and_then(|pe| body_query.get(pe).ok())
                        .and_then(|(_, _, _, ko, _)| ko)
                        .map(|ko| ko.semi_major_axis)
                        .or(dest_sma)
                        .unwrap_or(1.5)
                } else {
                    dest_sma.unwrap_or(1.5)
                };
                (r1, r2, GM_SUN)
            };
            fleet_ui_state.computed_options = {
                // Extract angles of origin and destination bodies in the correct coordinate system.
                let is_heliocentric = (gm - GM_SUN).abs() < 1e10;
                // Moon → parent-planet case: target IS the body that origin orbits around.
                let is_moon_to_parent = Some(target_entity) == origin_parent;

                let get_heliocentric_pos = |entity: Entity| -> Option<bevy::math::DVec3> {
                    let entry = body_query.get(entity).ok()?;
                    let is_moon = entry.1.body_type == BodyType::Moon;
                    if is_moon {
                        let parent_entity = entry.4?.0;
                        let parent_entry = body_query.get(parent_entity).ok()?;
                        Some(parent_entry.2.position)
                    } else {
                        Some(entry.2.position)
                    }
                };

                let get_local_pos = |entity: Entity, central_body: Entity| -> Option<bevy::math::DVec3> {
                    if entity == central_body {
                        Some(bevy::math::DVec3::ZERO)
                    } else {
                        let entry = body_query.get(entity).ok()?;
                        Some(entry.2.position)
                    }
                };

                let (pos1, pos2) = if is_moon_to_parent {
                    // Moon→parent: use Moon's position relative to the parent planet.
                    // The parent planet is at the centre of the local frame.
                    let moon_helio = body_query.get(orbit.body).ok()
                        .map(|(_, _, sc, _, _)| sc.position)
                        .unwrap_or(bevy::math::DVec3::ZERO);
                    let planet_helio = body_query.get(target_entity).ok()
                        .map(|(_, _, sc, _, _)| sc.position)
                        .unwrap_or(bevy::math::DVec3::ZERO);
                    (Some(moon_helio - planet_helio), Some(bevy::math::DVec3::ZERO))
                } else if is_heliocentric {
                    (get_heliocentric_pos(orbit.body), get_heliocentric_pos(target_entity))
                } else {
                    let central_body = dest_parent.unwrap_or(orbit.body);
                    (get_local_pos(orbit.body, central_body), get_local_pos(target_entity, central_body))
                };

                let theta1 = pos1.map(|p| p.y.atan2(p.x)).unwrap_or(0.0);
                let theta2 = pos2.map(|p| p.y.atan2(p.x)).unwrap_or(0.0);

                // Compute transfer window from live positions
                let window = compute_transfer_window(r1, r2, gm, theta1, theta2);
                window_max_slider_days = if window.synodic_period_s.is_finite() {
                    (window.synodic_period_s / 86_400.0 * 1.5).max(1.0)
                } else {
                    730.0
                };
                // Consume the "auto-set to next window" signal (departure_offset_days == -1.0)
                // that is set when the player first right-clicks a target body.  We resolve it
                // here — after the window is computed but before departure_s is used — so the
                // slider, quality indicator, and phased options all start at the optimal position.
                if fleet_ui_state.departure_offset_days < 0.0 {
                    fleet_ui_state.departure_offset_days =
                        (window.time_to_window_s / 86_400.0).max(0.0);
                }
                let departure_s = fleet_ui_state.departure_offset_days * 86_400.0;
                let opts = calculate_transfer_options_phased(r1, r2, gm, departure_s, &window);
                window_this_frame = Some(window);
                opts
            };
            // Post-process: fill burn_time_s, flag thrust-limited options,
            // and add kinematic options for high-thrust fleets.
            {
                let accel = fleet.min_accel_ms2();
                let isp = fleet.average_isp_s();
                apply_thrust_limits(&mut fleet_ui_state.computed_options, accel, isp);
                
                let hohmann_dv = fleet_ui_state.computed_options.first().map(|o| o.total_delta_v_ms).unwrap_or(0.0);
                let sma_h = fleet_ui_state.computed_options.first().map(|o| o.sma_au).unwrap_or(0.0);
                let ecc_h = fleet_ui_state.computed_options.first().map(|o| o.eccentricity).unwrap_or(0.0);
                let d = (r2 - r1).abs() * crate::fleets::orbital_mechanics::AU_IN_METERS;
                let mut kinematics = kinematic_transfer_options(
                    d, accel, fleet.max_delta_v_ms(),
                    hohmann_dv, sma_h, ecc_h, false
                );
                fleet_ui_state.computed_options.append(&mut kinematics);
            }
            // ── Gravity assist candidates (heliocentric transfers only) ─────────
            // Collect planets between r1 and r2, compute two-leg patched-conic options.
            // Only meaningful when GM ≈ GM_SUN (genuinely heliocentric transfer).
            if (gm - GM_SUN).abs() < 1e10 && !is_course_correction {
                let ga_bodies: Vec<(String, f64, f64, f64)> = body_query
                    .iter()
                    .filter_map(|(e, body, _, maybe_ko, _)| {
                        if !matches!(body.body_type,
                            BodyType::Planet | BodyType::GasGiant | BodyType::DwarfPlanet)
                        { return None; }
                        // Exclude the fleet's current body and the chosen destination
                        if e == orbit.body || Some(e) == body_target_snap { return None; }
                        // Only consider planets/bodies in the current star system
                        if body_system_ids.get(e).map(|s| s.0).unwrap_or(0) != current_system_id {
                            return None;
                        }
                        let sma = maybe_ko?.semi_major_axis;
                        if sma < MIN_HELIOCENTRIC_SMA_AU { return None; }
                        let gm_p = G_CONST * body.mass;
                        // Safe flyby periapsis: 3 × body radius (km → m → AU)
                        let min_peri = (body.radius as f64 * 3_000.0) / AU_IN_METERS;
                        Some((body.name.clone(), sma, gm_p, min_peri.max(1e-6)))
                    })
                    .collect();

                let new_candidates: Vec<GravityAssistEntry> =
                    find_gravity_assist_options(r1, r2, gm, &ga_bodies)
                    .into_iter()
                    .filter_map(|opt| {
                        // Resolve each candidate to its ECS entity by name
                        let entity = body_query
                            .iter()
                            .find(|(_, b, _, _, _)| b.name == opt.body_name)
                            .map(|(e, _, _, _, _)| e)?;
                        Some(GravityAssistEntry { option: opt, flyby_entity: entity })
                    })
                    .collect();

                fleet_ui_state.gravity_assist_candidates = new_candidates;

                // Validate selected index is still in-range (target may have changed)
                if fleet_ui_state.selected_gravity_assist
                    .map(|i| i >= fleet_ui_state.gravity_assist_candidates.len())
                    .unwrap_or(false)
                {
                    fleet_ui_state.selected_gravity_assist = None;
                }
            } else {
                fleet_ui_state.gravity_assist_candidates.clear();
                fleet_ui_state.selected_gravity_assist = None;
            }

            // If a gravity assist is selected, prepend it as option 0 so the
            // regular execute/select logic treats it uniformly.
            if let Some(sel_ga) = fleet_ui_state.selected_gravity_assist {
                let ga_data = fleet_ui_state.gravity_assist_candidates.get(sel_ga)
                    .map(|e| (
                        e.option.total_dv_ms,
                        e.option.total_time_s,
                        e.option.flyby_radius_au,
                        e.option.dv_depart_ms + e.option.dv_mid_ms, // departure + mid-course
                        e.option.dv_arrive_ms,
                    ));
                if let Some((total_dv, total_time, fly_r, dv1, dv2)) = ga_data {
                    // Use Leg-2 Hohmann parameters for the transfer-orbit visualization
                    // (the arc the fleet actually flies after the flyby).
                    let (_, _, _, ga_sma, ga_ecc) = hohmann_transfer(fly_r, r2, gm);
                    let burn_t = compute_burn_time_s(total_dv, fleet.min_accel_ms2(), fleet.average_isp_s());
                    // Gravity-assist options use multi-leg patched-conic timing; the burn
                    // is spread across two legs so we apply the thrust-limit check here.
                    let (ga_transfer_time, ga_thrust_limited) = if burn_t > 0.0 && burn_t > total_time {
                        (burn_t, true)
                    } else {
                        (total_time, false)
                    };
                    let ga_option = TransferOption {
                        label: "Gravity Assist",
                        total_delta_v_ms: total_dv,
                        delta_v1_ms: dv1,   // actual departure + any mid-course burn
                        delta_v2_ms: dv2,   // actual arrival circularisation
                        transfer_time_s: ga_transfer_time,
                        sma_au: ga_sma,     // Leg-2 ellipse SMA for arc rendering
                        eccentricity: ga_ecc,
                        energy_multiplier: 1.0,
                        burn_time_s: burn_t,
                        is_thrust_limited: ga_thrust_limited,
                    };
                    fleet_ui_state.computed_options.insert(0, ga_option);
                }
            }
            } else if let Some(ref lp) = lp_target_snap {
                // Lagrange-point transfer.
                // Determine the fleet's current heliocentric SMA, walking up to
                // the planet's SMA when the fleet is parked at a moon/sub-body.
                // When orbiting the star directly (e.g. after a previous LP transfer),
                // use the fleet's parking radius if available, otherwise the LP planet's SMA.
                let r1_lp = body_query.get(orbit.body).ok()
                    .and_then(|(_, body, _, ko, _)| {
                        if body.body_type == BodyType::Star {
                            // Fleet parked around the star — use its parking orbit radius
                            // or fall back to the target LP's planet SMA.
                            if orbit.radius_au > 0.01 {
                                Some(orbit.radius_au)
                            } else {
                                Some(lp.planet_sma_au)
                            }
                        } else {
                            ko.map(|ko| ko.semi_major_axis)
                        }
                    })
                    .or_else(|| {
                        body_query.get(orbit.body).ok()
                            .and_then(|(_, _, _, _, parent)| parent)
                            .and_then(|lpp| body_query.get(lpp.0).ok()
                                .and_then(|(_, _, _, ko, _)| ko)
                                .map(|ko| ko.semi_major_axis))
                    })
                    .unwrap_or(lp.planet_sma_au);

                // L3/L4/L5 are co-orbital with the planet (same heliocentric radius,
                // different phase angle).  A Hohmann gives 0 Delta-V in this case.
                // Use a phasing-orbit maneuver instead: lower into a shorter-period
                // orbit and drift the 60 deg (L4/L5) or 180 deg (L3) phase gap in N laps.
                let co_orbital = matches!(lp.point, 3 | 4 | 5)
                    && (r1_lp - lp.planet_sma_au).abs() < 0.02;

                if co_orbital {
                    let delta_phi = if lp.point == 3 {
                        std::f64::consts::PI           // L3: 180 deg opposition
                    } else {
                        std::f64::consts::FRAC_PI_3    // L4/L5: 60 deg
                    };
                    fleet_ui_state.computed_options =
                        co_orbital_phasing_options(lp.planet_sma_au, lp.gm, delta_phi);
                    apply_thrust_limits(
                        &mut fleet_ui_state.computed_options,
                        fleet.min_accel_ms2(),
                        fleet.average_isp_s(),
                    );
                    // Kinematic options: arc-length of the phase drift as proxy distance.
                    let hohmann_dv = fleet_ui_state.computed_options.first().map(|o| o.total_delta_v_ms).unwrap_or(0.0);
                    let sma_h = fleet_ui_state.computed_options.first().map(|o| o.sma_au).unwrap_or(r1_lp);
                    let d = lp.planet_sma_au * delta_phi * crate::fleets::orbital_mechanics::AU_IN_METERS;
                    let mut kinematics = kinematic_transfer_options(
                        d, fleet.min_accel_ms2(), fleet.max_delta_v_ms(),
                        hohmann_dv, sma_h, 0.0, false
                    );
                    fleet_ui_state.computed_options.append(&mut kinematics);
                } else if matches!(lp.point, 1 | 2) {
                    // L1/L2: small radial offset from planet (~r_hill ≈ 0.01 AU).
                    // Use a direct manifold-like trajectory (realistic ~1–3 month travel
                    // time) instead of a Hohmann half-orbit that takes 6 months and arrives
                    // 180° away from the LP.
                    fleet_ui_state.computed_options =
                        direct_lp_transfer_options(r1_lp, lp.radius_au, lp.gm);
                    apply_thrust_limits(
                        &mut fleet_ui_state.computed_options,
                        fleet.min_accel_ms2(),
                        fleet.average_isp_s(),
                    );
                    let hohmann_dv = fleet_ui_state.computed_options.first().map(|o| o.total_delta_v_ms).unwrap_or(0.0);
                    let sma_h = fleet_ui_state.computed_options.first().map(|o| o.sma_au).unwrap_or(0.0);
                    let ecc_h = fleet_ui_state.computed_options.first().map(|o| o.eccentricity).unwrap_or(0.0);
                    let d = (lp.radius_au - r1_lp).abs() * crate::fleets::orbital_mechanics::AU_IN_METERS;
                    let mut kinematics = kinematic_transfer_options(
                        d, fleet.min_accel_ms2(), fleet.max_delta_v_ms(),
                        hohmann_dv, sma_h, ecc_h, false
                    );
                    fleet_ui_state.computed_options.append(&mut kinematics);
                } else {
                    // L3/L4/L5 cross-orbit (fleet NOT co-orbital with the planet):
                    // standard Hohmann Keplerian transfer to the planet's SMA.
                    fleet_ui_state.computed_options =
                        calculate_transfer_options(r1_lp, lp.radius_au, lp.gm);
                    apply_thrust_limits(
                        &mut fleet_ui_state.computed_options,
                        fleet.min_accel_ms2(),
                        fleet.average_isp_s(),
                    );
                    let hohmann_dv = fleet_ui_state.computed_options.first().map(|o| o.total_delta_v_ms).unwrap_or(0.0);
                    let sma_h = fleet_ui_state.computed_options.first().map(|o| o.sma_au).unwrap_or(0.0);
                    let ecc_h = fleet_ui_state.computed_options.first().map(|o| o.eccentricity).unwrap_or(0.0);
                    let d = (lp.radius_au - r1_lp).abs() * crate::fleets::orbital_mechanics::AU_IN_METERS;
                    let mut kinematics = kinematic_transfer_options(
                        d, fleet.min_accel_ms2(), fleet.max_delta_v_ms(),
                        hohmann_dv, sma_h, ecc_h, false
                    );
                    fleet_ui_state.computed_options.append(&mut kinematics);
                }
            }

        // ── Interstellar transfer computation ───────────────────────────────
        if let Some((_, _, distance_ly)) = star_system_snap {
            use crate::fleets::orbital_mechanics::{AU_IN_METERS, TransferOption};
            // 1 ly = 63 241.077 AU
            const AU_PER_LY: f64 = 63_241.077;
            let distance_m  = distance_ly as f64 * AU_PER_LY * AU_IN_METERS;
            let accel       = fleet.min_accel_ms2();
            let max_dv      = fleet.max_delta_v_ms();

            fleet_ui_state.computed_options.clear();

            let mut kinematics = kinematic_transfer_options(
                distance_m, accel, max_dv,
                0.0, 0.0, 0.0, true
            );
            fleet_ui_state.computed_options.append(&mut kinematics);

            if fleet_ui_state.computed_options.is_empty() {
                // Fleet lacks the minimum thrust for interstellar travel
                fleet_ui_state.computed_options.push(TransferOption {
                    label: "Insufficient thrust",
                    total_delta_v_ms: 0.0,
                    delta_v1_ms: 0.0,
                    delta_v2_ms: 0.0,
                    transfer_time_s: f64::INFINITY,
                    sma_au: 0.0,
                    eccentricity: 0.0,
                    energy_multiplier: 0.0,
                    burn_time_s: 0.0,
                    is_thrust_limited: true,
                });
            }
        }

        // ── Transfer Window info + departure slider ─────────────────────────
        // Show a co-orbital / L-point info section for Lagrange targets.
        if window_this_frame.is_none() && lp_target_snap.is_some() {
            ui.add_space(6.0);
            ui.horizontal_top(|ui| {
                // Left: Lagrange transfer info
                ui.group(|ui| {
                    ui.vertical(|ui| {
                        ui.set_min_height(82.0);
                        let lp = lp_target_snap.as_ref().unwrap();
                        // Determine actual transfer type — same logic as the computation
                        // section above.  L3/L4/L5 are co-orbital only when the fleet is
                        // already near the planet's SMA (within 0.02 AU).
                        let r1_info = body_query.get(orbit.body).ok()
                            .and_then(|(_, body, _, ko, _)| {
                                if body.body_type == BodyType::Star {
                                    if orbit.radius_au > 0.01 { Some(orbit.radius_au) }
                                    else { Some(lp.planet_sma_au) }
                                } else { ko.map(|k| k.semi_major_axis) }
                            })
                            .or_else(|| {
                                body_query.get(orbit.body).ok()
                                    .and_then(|(_, _, _, _, parent)| parent)
                                    .and_then(|lpp| body_query.get(lpp.0).ok()
                                        .and_then(|(_, _, _, ko, _)| ko)
                                        .map(|ko| ko.semi_major_axis))
                            })
                            .unwrap_or(lp.planet_sma_au);
                        let is_co_orbital = matches!(lp.point, 3 | 4 | 5)
                            && (r1_info - lp.planet_sma_au).abs() < 0.02;
                        let is_l12_direct = matches!(lp.point, 1 | 2);
                        if is_co_orbital {
                            ui.label(
                                egui::RichText::new("⟳ Co-orbital Phasing")
                                    .strong().size(12.0)
                                    .color(egui::Color32::from_rgb(160, 210, 255)),
                            );
                            ui.add_space(3.0);
                            ui.label(
                                egui::RichText::new("Depart any time")
                                    .size(12.0).strong()
                                    .color(egui::Color32::from_rgb(80, 220, 80)),
                            );
                            ui.label(
                                egui::RichText::new("Fleet drifts in a slightly\nlower orbit to cover the\nphase gap over N laps.")
                                    .size(10.0).color(egui::Color32::GRAY),
                            );
                        } else if is_l12_direct {
                            ui.label(
                                egui::RichText::new("🎯 Direct LP Transfer")
                                    .strong().size(12.0)
                                    .color(egui::Color32::from_rgb(160, 210, 255)),
                            );
                            ui.add_space(3.0);
                            ui.label(
                                egui::RichText::new(format!("L{}: {}", lp.point, lp.qualifier()))
                                    .size(12.0).strong()
                                    .color(egui::Color32::from_rgb(200, 200, 200)),
                            );
                            ui.label(
                                egui::RichText::new(format!("r = {:.4} AU", lp.radius_au))
                                    .size(11.0).color(egui::Color32::GRAY),
                            );
                            ui.label(
                                egui::RichText::new("Low-energy manifold trajectory\nto the Lagrange equilibrium.")
                                    .size(10.0).color(egui::Color32::GRAY),
                            );
                        } else {
                            // L3/L4/L5 cross-orbit (fleet not co-orbital): Hohmann
                            ui.label(
                                egui::RichText::new("⬆ Hohmann Transfer")
                                    .strong().size(12.0)
                                    .color(egui::Color32::from_rgb(160, 210, 255)),
                            );
                            ui.add_space(3.0);
                            ui.label(
                                egui::RichText::new(format!("L{}: {}", lp.point, lp.qualifier()))
                                    .size(12.0).strong()
                                    .color(egui::Color32::from_rgb(200, 200, 200)),
                            );
                            ui.label(
                                egui::RichText::new(format!("r = {:.4} AU", lp.radius_au))
                                    .size(11.0).color(egui::Color32::GRAY),
                            );
                            ui.label(
                                egui::RichText::new("Keplerian transfer arc,\nthen phase into the LP.")
                                    .size(10.0).color(egui::Color32::GRAY),
                            );
                        }
                    });
                });
                // Fleet stats infobox (same as body-target section)
                ui.group(|ui| {
                    ui.set_min_width(90.0);
                    ui.set_max_width(96.0);
                    ui.vertical(|ui| {
                        ui.set_min_height(82.0);
                        ui.label(
                            egui::RichText::new("\u{1f680} Fleet")
                                .strong().size(12.0)
                                .color(egui::Color32::from_rgb(160, 210, 255)),
                        );
                        ui.add_space(3.0);
                        let dv_kms = fleet.max_delta_v_ms() / 1_000.0;
                        let thrust_kn = fleet.min_thrust_kn();
                        let thrust_str = if thrust_kn >= 1_000.0 {
                            format!("{:.1} MN", thrust_kn / 1_000.0)
                        } else {
                            format!("{:.0} kN", thrust_kn)
                        };
                        let accel_g = fleet.min_accel_ms2() / 9.80665;
                        ui.label(egui::RichText::new("ΔV avail.").size(10.0).color(egui::Color32::GRAY));
                        ui.label(
                            egui::RichText::new(format!("{:.2} km/s", dv_kms))
                                .size(11.0).strong()
                                .color(egui::Color32::from_rgb(200, 230, 255)),
                        );
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new("Thrust").size(10.0).color(egui::Color32::GRAY));
                        ui.label(
                            egui::RichText::new(thrust_str)
                                .size(11.0).strong()
                                .color(egui::Color32::from_rgb(200, 230, 255)),
                        );
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new("Accel.").size(10.0).color(egui::Color32::GRAY));
                        ui.label(
                            egui::RichText::new(format!("{:.3} g", accel_g))
                                .size(11.0).strong()
                                .color(egui::Color32::from_rgb(200, 230, 255)),
                        );
                    });
                });
            });
        }
        if let Some(ref window) = window_this_frame {
            let syn_days = if window.synodic_period_s.is_finite() {
                window.synodic_period_s / 86_400.0
            } else {
                f64::INFINITY
            };
            let window_days = window.time_to_window_s / 86_400.0;

            ui.add_space(6.0);

            let max_days = window_max_slider_days.min(1_825.0); // cap at 5 years
            let step_size = if max_days <= 1.0 {
                0.01 // ~14 mins
            } else if max_days <= 10.0 {
                0.05 // ~1.2 hours
            } else if max_days <= 50.0 {
                0.1 // ~2.4 hours
            } else if max_days <= 200.0 {
                0.5 // 12 hours
            } else {
                1.0 // 1 day
            };

            // ── Transfer Window (left) + Planned Departure (right) side by side ──
            ui.horizontal_top(|ui| {
                // Left: Transfer Window box
                ui.group(|ui| {
                    ui.vertical(|ui| {
                        ui.set_min_height(82.0);
                        ui.label(
                            egui::RichText::new("⏱ Transfer Window")
                                .strong()
                                .size(12.0)
                                .color(egui::Color32::from_rgb(160, 210, 255)),
                        );
                        ui.add_space(3.0);

                        egui::Grid::new("window_info_grid")
                            .num_columns(2)
                            .spacing([8.0, 3.0])
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new("Next window:").size(12.0));
                                if window_days < 1.0 {
                                    ui.label(
                                        egui::RichText::new("NOW  ✓")
                                            .size(12.0)
                                            .strong()
                                            .color(egui::Color32::from_rgb(80, 220, 80)),
                                    );
                                } else {
                                    ui.label(
                                        egui::RichText::new(format!("{}", format_duration(window.time_to_window_s)))
                                            .size(12.0)
                                            .color(egui::Color32::from_rgb(200, 200, 200)),
                                    );
                                }
                                ui.end_row();

                                ui.label(egui::RichText::new("Synodic period:").size(12.0));
                                let syn_str = if syn_days.is_finite() {
                                    format_duration(window.synodic_period_s)
                                } else {
                                    "∞ (same orbit)".to_owned()
                                };
                                ui.label(egui::RichText::new(syn_str).size(12.0).color(egui::Color32::GRAY));
                                ui.end_row();
                            });
                    });
                });

                // Right: Planned Departure box
                ui.group(|ui| {
                    ui.vertical(|ui| {
                        ui.set_min_height(82.0);
                        // Row 1: label
                        ui.label(
                            egui::RichText::new("🕐 Planned Departure")
                                .strong()
                                .size(12.0)
                                .color(egui::Color32::from_rgb(160, 210, 255)),
                        );

                        // Row 2: slider
                        let mut offset_days = fleet_ui_state.departure_offset_days as f32;
                        let slider = egui::Slider::new(&mut offset_days, 0.0_f32..=max_days as f32)
                            .step_by(step_size as f64)
                            .custom_formatter(|v, _| {
                                if v < 0.01 {
                                    "Now".to_owned()
                                } else {
                                    format_duration(v as f64 * 86_400.0)
                                }
                            });
                        if ui.add(slider).changed() {
                            fleet_ui_state.departure_offset_days = offset_days as f64;
                        }

                        // Row 3: alignment indicator (below the slider)
                        let dep_s = fleet_ui_state.departure_offset_days * 86_400.0;
                        let phase_at = {
                            let raw = window.phase_error_now_rad + window.phase_rate_rad_s * dep_s;
                            ((raw + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU)) - std::f64::consts::PI
                        };
                        let factor = crate::fleets::orbital_mechanics::phase_dv_factor(phase_at.abs());
                        let (quality_str, quality_color) = if factor < 1.05 {
                            ("● Optimal", egui::Color32::from_rgb(80, 220, 80))
                        } else if factor < 1.40 {
                            ("◑ Good", egui::Color32::from_rgb(180, 220, 80))
                        } else if factor < 1.80 {
                            ("◔ Fair", egui::Color32::from_rgb(220, 180, 60))
                        } else {
                            ("○ Poor", egui::Color32::from_rgb(220, 80, 60))
                        };
                        ui.label(egui::RichText::new(quality_str).size(11.0).color(quality_color))
                            .on_hover_text("Indicates how well the planets are aligned for a transfer at the planned departure time. Poor alignment requires significantly more ΔV.");

                        // Next Window button on its own row
                        if window_days > 0.5 {
                            ui.add_space(2.0);
                            if ui.small_button(format!("🎯 Next Window (+{:.0} d)", window_days)).clicked() {
                                fleet_ui_state.departure_offset_days = window_days;
                            }
                        }
                    });
                });

                // Fleet stats infobox (narrow, right-most)
                ui.group(|ui| {
                    ui.set_min_width(90.0);
                    ui.set_max_width(96.0);
                    ui.vertical(|ui| {
                        ui.set_min_height(82.0);
                        ui.label(
                            egui::RichText::new("🚀 Fleet")
                                .strong()
                                .size(12.0)
                                .color(egui::Color32::from_rgb(160, 210, 255)),
                        );
                        ui.add_space(3.0);

                        let dv_kms = fleet.max_delta_v_ms() / 1_000.0;
                        let thrust_kn = fleet.min_thrust_kn();
                        let thrust_str = if thrust_kn >= 1_000.0 {
                            format!("{:.1} MN", thrust_kn / 1_000.0)
                        } else {
                            format!("{:.0} kN", thrust_kn)
                        };
                        let accel_ms2 = fleet.min_accel_ms2();
                        let accel_g = accel_ms2 / 9.80665;
                        let accel_str = format!("{:.3} g", accel_g);

                        ui.label(egui::RichText::new("ΔV avail.").size(10.0).color(egui::Color32::GRAY));
                        ui.label(
                            egui::RichText::new(format!("{:.2} km/s", dv_kms))
                                .size(11.0)
                                .strong()
                                .color(egui::Color32::from_rgb(200, 230, 255)),
                        );
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new("Thrust").size(10.0).color(egui::Color32::GRAY));
                        ui.label(
                            egui::RichText::new(thrust_str)
                                .size(11.0)
                                .strong()
                                .color(egui::Color32::from_rgb(200, 230, 255)),
                        );
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new("Accel.").size(10.0).color(egui::Color32::GRAY));
                        ui.label(
                            egui::RichText::new(accel_str)
                                .size(11.0)
                                .strong()
                                .color(egui::Color32::from_rgb(200, 230, 255)),
                        );
                    });
                });
            });
        }

        if !fleet_ui_state.computed_options.is_empty() {
            ui.add_space(6.0);

            let fleet_max_dv = fleet.max_delta_v_ms();

            // Ensure selected_option is within bounds
            if fleet_ui_state.selected_option >= fleet_ui_state.computed_options.len() {
                fleet_ui_state.selected_option = fleet_ui_state.computed_options.len() - 1;
            }

            // Pre-compute execute button state
            let sel_option = fleet_ui_state.computed_options[fleet_ui_state.selected_option].clone();
            let abort_cost_t: f32 = if let Some(maneuver) = current_maneuver {
                let progress = maneuver.progress(elapsed) as f32;
                let abort_factor = 4.0 * progress * (1.0 - progress);
                maneuver.fuel_used_t * abort_factor * 0.6
            } else {
                0.0
            };
            let dv_after_abort = if abort_cost_t > 0.0 {
                fleet.min_delta_v_after_abort(abort_cost_t)
            } else {
                fleet_max_dv
            };
            let sel_affordable_with_abort = sel_option.total_delta_v_ms <= dv_after_abort;

            // Interstellar note
            let is_interstellar = star_system_snap.is_some();
            if is_interstellar {
                if let Some((_, ref sys_name, dist_ly)) = star_system_snap {
                    ui.group(|ui| {
                        ui.label(
                            egui::RichText::new(format!("\u{1F30C} Interstellar Mission: {}", sys_name))
                                .strong().size(13.0).color(egui::Color32::from_rgb(200, 180, 255)),
                        );
                        ui.label(
                            egui::RichText::new(format!(
                                "Distance: {:.2} ly = {:.0} AU",
                                dist_ly,
                                dist_ly as f64 * 63_241.077
                            )).size(11.0).color(egui::Color32::GRAY),
                        );
                        ui.label(
                            egui::RichText::new(
                                "\u{26A0} Interstellar navigation is point-and-burn. \
                                 Transfer windows do not apply. \
                                 Ensure adequate \u{394}V and life-support reserves."
                            ).size(11.0).italics().color(egui::Color32::from_rgb(220, 180, 80)),
                        );
                    });
                    ui.add_space(4.0);
                }
            }

            let btn_label = if is_interstellar {
                "\u{1F680} Commit Interstellar Course".to_string()
            } else if is_course_correction {
                if abort_cost_t > 0.01 {
                    let abort_dv_kms = (fleet_max_dv - dv_after_abort) / 1_000.0;
                    format!("\u{1F504} Execute Course Correction (+{:.2} km/s abort burn)", abort_dv_kms)
                } else {
                    "\u{1F504} Execute Course Correction".to_string()
                }
            } else {
                "\u{1F680} Execute Transfer".to_string()
            };

            // For fleet intercepts note the encounter speed penalty
            if fleet_target_snap.is_some() && fleet_ui_state.intercept_speed_ms > 100.0 {
                let extra_dv_kms = fleet_ui_state.intercept_speed_ms / 1_000.0;
                ui.label(
                    egui::RichText::new(format!(
                        "\u{26A0} +{:.1} km/s added for encounter speed (not included in \u{394}V below)",
                        extra_dv_kms
                    ))
                    .size(11.0)
                    .italics()
                    .color(egui::Color32::from_rgb(220, 160, 60)),
                );
            }

            // Execute Transfer button with ETA on the same row
            ui.horizontal(|ui| {
                let insufficient = sel_option.is_thrust_limited && is_interstellar && sel_option.total_delta_v_ms == 0.0;
                let btn = egui::Button::new(
                    egui::RichText::new(&btn_label).size(13.0).strong(),
                );
                let resp = ui.add_enabled(!insufficient && (sel_affordable_with_abort || is_interstellar), btn);
                if resp.clicked() {
                    if is_interstellar {
                        // Interstellar travel: no ECS destination body; log mission intent.
                        // Full multi-system navigation will be implemented in a future session.
                        if let Some((sys_id, ref sys_name, dist_ly)) = star_system_snap {
                            info!(
                                "Fleet '{}' committed to interstellar course: {} ({:.2} ly, system_id {}). \
                                 \u{394}V required: {:.1} km/s, travel time: {:.1} years. \
                                 Multi-system navigation NYI.",
                                fleet.name, sys_name, dist_ly, sys_id,
                                sel_option.total_delta_v_ms / 1_000.0,
                                sel_option.transfer_time_s / (365.25 * 86_400.0),
                            );
                        }
                    } else {
                        let maybe_transfer = if let Some(ref lp) = lp_target_snap {
                            build_planned_transfer_lp(fleet_entity, fleet, orbit, lp, body_query, &sel_option)
                        } else if let Some(tfe) = fleet_target_snap {
                            all_fleets_query.get(tfe).ok()
                                .and_then(|(_, _, _, maybe_fo, _)| maybe_fo)
                                .and_then(|fo| {
                                    build_planned_transfer(fleet_entity, fleet, orbit, fo.body, body_query, &sel_option)
                                })
                        } else if let Some(te) = body_target_snap {
                            build_planned_transfer(fleet_entity, fleet, orbit, te, body_query, &sel_option)
                        } else {
                            None
                        };
                        if let Some(transfer) = maybe_transfer {
                            pending_actions.start_transfers.push(StartTransferAction {
                                fleet: fleet_entity,
                                transfer,
                                abort_cost_t,
                                departure_offset_s: fleet_ui_state.departure_offset_days * 86_400.0,
                            });
                        }
                    }
                }
                if !is_interstellar {
                    ui.add_space(12.0);
                    ui.label(
                        egui::RichText::new(format!("ETA  {}", format_duration(sel_option.transfer_time_s)))
                            .size(12.0)
                            .color(egui::Color32::from_rgb(160, 220, 160)),
                    );
                }
            });
            if !is_interstellar && !sel_affordable_with_abort {
                ui.label(
                    egui::RichText::new(
                        if abort_cost_t > 0.0 {
                            "Insufficient \u{394}V remaining after abort burn."
                        } else {
                            "Selected option requires more \u{394}V than this fleet can provide."
                        },
                    )
                    .size(11.0)
                    .italics()
                    .color(egui::Color32::from_rgb(200, 100, 60)),
                );
            }
        }

        // ── Gravity Assists panel ─────────────────────────────────────────────
        // Shown whenever there are heliocentric flyby candidates for this route.
        if !fleet_ui_state.gravity_assist_candidates.is_empty() {
            ui.add_space(6.0);
            let num_ga = fleet_ui_state.gravity_assist_candidates.len();
            let header_text = format!("⚡ Gravity Assists ({num_ga} available)");
            egui::CollapsingHeader::new(
                egui::RichText::new(header_text)
                    .size(12.0)
                    .strong()
                    .color(egui::Color32::from_rgb(120, 220, 255)),
            )
            .default_open(true)
            .show(ui, |ui| {
                // Snapshot data before mut-borrowing fleet_ui_state below
                let snapped: Vec<(usize, String, f64, f64, f64, f64)> =
                    fleet_ui_state.gravity_assist_candidates
                        .iter()
                        .enumerate()
                        .map(|(i, e)| (
                            i,
                            e.option.body_name.clone(),
                            e.option.dv_savings_ms,
                            e.option.extra_time_s,
                            e.option.window_period_s,
                            e.option.v_inf_ms,
                        ))
                        .collect();

                for (idx, body_name, savings, extra_t, win_period, v_inf) in snapped {
                    let is_sel = fleet_ui_state.selected_gravity_assist == Some(idx);
                    let beneficial = savings > 100.0;
                    let header_color = if is_sel {
                        egui::Color32::from_rgb(80, 255, 180)
                    } else if beneficial {
                        egui::Color32::from_rgb(160, 255, 100)
                    } else {
                        egui::Color32::GRAY
                    };

                    ui.group(|ui| {
                        ui.set_min_width(ui.available_width());
                        ui.label(
                            egui::RichText::new(format!("⚡ via {body_name}"))
                                .size(12.0)
                                .strong()
                                .color(header_color),
                        );
                        egui::Grid::new(format!("ga_grid_{idx}"))
                            .num_columns(2)
                            .spacing([8.0, 2.0])
                            .show(ui, |ui| {
                                if beneficial {
                                    ui.label(egui::RichText::new("ΔV saved:").size(11.0));
                                    ui.label(
                                        egui::RichText::new(format_delta_v(savings))
                                            .size(11.0)
                                            .strong()
                                            .color(egui::Color32::from_rgb(80, 220, 80)),
                                    );
                                } else {
                                    ui.label(egui::RichText::new("Extra ΔV:").size(11.0));
                                    ui.label(
                                        egui::RichText::new(format_delta_v(-savings))
                                            .size(11.0)
                                            .color(egui::Color32::GRAY),
                                    );
                                }
                                ui.end_row();

                                ui.label(egui::RichText::new("Extra time:").size(11.0));
                                let sign = if extra_t >= 0.0 { "+" } else { "" };
                                ui.label(
                                    egui::RichText::new(
                                        format!("{sign}{}", format_duration(extra_t.abs()))
                                    )
                                    .size(11.0)
                                    .color(egui::Color32::LIGHT_GRAY),
                                );
                                ui.end_row();

                                ui.label(egui::RichText::new("Window every:").size(11.0));
                                let win_str = if win_period.is_finite() {
                                    format_duration(win_period)
                                } else {
                                    "∞".to_owned()
                                };
                                ui.label(
                                    egui::RichText::new(win_str)
                                        .size(11.0)
                                        .color(egui::Color32::GRAY),
                                );
                                ui.end_row();

                                ui.label(egui::RichText::new("v∞:").size(11.0));
                                ui.label(
                                    egui::RichText::new(format_delta_v(v_inf))
                                        .size(11.0)
                                        .color(egui::Color32::GRAY),
                                );
                                ui.end_row();
                            });

                        ui.horizontal(|ui| {
                            if is_sel {
                                if ui.small_button("✕ Clear Assist").clicked() {
                                    fleet_ui_state.selected_gravity_assist = None;
                                    // Shift selection back to direct Efficient option
                                    fleet_ui_state.selected_option = 0;
                                    fleet_ui_state.planned_transfer = None;
                                }
                            } else {
                                let label = if beneficial { "⚡ Use Gravity Assist" } else { "Use Suboptimal Assist" };
                                if ui.small_button(label).clicked() {
                                    fleet_ui_state.selected_gravity_assist = Some(idx);
                                    fleet_ui_state.selected_option = 0; // GA is option 0
                                    fleet_ui_state.planned_transfer = None;
                                }
                            }
                        });
                    });
                    ui.add_space(2.0);
                }
            });
        }

        if !fleet_ui_state.computed_options.is_empty() {
            let fleet_wet_mass = fleet.total_wet_mass_t();
            let fleet_max_dv = fleet.max_delta_v_ms();

            ui.add_space(4.0);
            ui.label(egui::RichText::new("Transfer Options:").strong().size(13.0));
            ui.add_space(2.0);

            let options: Vec<_> = fleet_ui_state.computed_options.clone();
            for (idx, option) in options.iter().enumerate() {
                let fuel_cost = fleet.total_fuel_cost_for_dv(option.total_delta_v_ms);
                let fuel_pct = if fleet_wet_mass > 0.0 {
                    (fuel_cost / fleet_wet_mass * 100.0) as u32
                } else {
                    0
                };
                let affordable = option.total_delta_v_ms <= fleet_max_dv;

                let is_selected = fleet_ui_state.selected_option == idx;
                let row_color = if !affordable {
                    egui::Color32::from_rgb(180, 80, 80)
                } else if is_selected {
                    egui::Color32::from_rgb(100, 180, 255)
                } else {
                    egui::Color32::from_rgb(200, 200, 200)
                };

                ui.group(|ui| {
                    ui.set_min_width(ui.available_width());
                    let resp = ui.selectable_label(
                        is_selected,
                        egui::RichText::new(format!(
                            "{} {}",
                            if is_selected { "●" } else { "○" },
                            option.label
                        ))
                        .size(13.0)
                        .strong()
                        .color(row_color),
                    );
                    if resp.clicked() {
                        fleet_ui_state.selected_option = idx;
                        fleet_ui_state.planned_transfer = None;
                    }

                    egui::Grid::new(format!("option_{idx}"))
                        .num_columns(4)
                        .spacing([16.0, 2.0])
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new("Total ΔV:").size(12.0));
                            ui.label(
                                egui::RichText::new(format_delta_v(option.total_delta_v_ms))
                                    .size(12.0)
                                    .strong()
                                    .color(row_color),
                            );
                            ui.label(egui::RichText::new("Travel time:").size(12.0));
                            ui.label(
                                egui::RichText::new(format_duration(option.transfer_time_s))
                                    .size(12.0)
                                    .strong(),
                            );
                            ui.end_row();

                            ui.label(egui::RichText::new("Est. fuel:").size(12.0));
                            let fuel_color = if affordable {
                                egui::Color32::from_rgb(220, 180, 60)
                            } else {
                                egui::Color32::from_rgb(220, 80, 60)
                            };
                            ui.label(
                                egui::RichText::new(format!("{:.0} t ({fuel_pct}%)", fuel_cost))
                                    .size(12.0)
                                    .color(fuel_color),
                            );
                            ui.label(egui::RichText::new("Departure burn:").size(12.0));
                            ui.label(
                                egui::RichText::new(format_delta_v(option.delta_v1_ms))
                                    .size(12.0),
                            );
                            ui.end_row();

                            // Burn time row — shows how long the fleet's engines fire.
                            if option.burn_time_s > 0.0 {
                                // Classify burn profile based on burn/transfer time ratio.
                                let (profile_label, profile_color) =
                                    if option.is_thrust_limited {
                                        // Burn time >= Hohmann time: impulsive assumption invalid.
                                        ("⚠ Thrust-limited", egui::Color32::from_rgb(220, 100, 40))
                                    } else if option.label == "Flip & Burn" {
                                        // Entire trip is a burn
                                        ("⚡ Full thrust", egui::Color32::from_rgb(255, 180, 60))
                                    } else {
                                        let ratio = option.burn_time_s / option.transfer_time_s.max(1.0);
                                        if option.burn_time_s < 3_600.0 {
                                            ("Impulsive", egui::Color32::from_rgb(120, 200, 120))
                                        } else if ratio < 0.05 {
                                            ("Short burn", egui::Color32::from_rgb(140, 210, 140))
                                        } else if ratio < 0.25 {
                                            ("Extended burn", egui::Color32::from_rgb(220, 200, 80))
                                        } else {
                                            ("Continuous thrust", egui::Color32::from_rgb(220, 140, 60))
                                        }
                                    };
                                ui.label(egui::RichText::new("Burn time:").size(12.0));
                                ui.label(
                                    egui::RichText::new(format_duration(option.burn_time_s))
                                        .size(12.0)
                                        .strong(),
                                );
                                ui.label(egui::RichText::new("Profile:").size(12.0));
                                ui.label(
                                    egui::RichText::new(profile_label)
                                        .size(12.0)
                                        .color(profile_color),
                                );
                                ui.end_row();

                                let accel_ms2 = fleet.min_accel_ms2();
                                let accel_g = accel_ms2 / 9.80665;
                                ui.label(egui::RichText::new("Acceleration:").size(12.0));
                                ui.label(
                                    egui::RichText::new(format!("{:.2} g", accel_g))
                                        .size(12.0)
                                        .strong(),
                                );
                                ui.end_row();

                                // Extra warning row for thrust-limited options.
                                if option.is_thrust_limited {
                                    ui.label(
                                        egui::RichText::new("  Low-thrust spiral — travel time ≥ burn time")
                                            .size(11.0)
                                            .italics()
                                            .color(egui::Color32::from_rgb(180, 130, 80)),
                                    );
                                    ui.end_row();
                                }
                            }

                            if !affordable {
                                ui.label(
                                    egui::RichText::new("⚠ Insufficient ΔV capacity")
                                        .size(11.0)
                                        .color(egui::Color32::from_rgb(220, 80, 60)),
                                );
                            }
                        });
                });
                ui.add_space(2.0);
            }

        }
    }
}

/// Floating popup window showing the Transfer Planner over the 3D view.
///
/// Opened by the "Transfer Planner" button in the fleet action bar or the Fleet Management
/// panel shortcut button. Closed with the window's × button or by deselecting the fleet.
fn ui_transfer_planner_popup(
    mut contexts: EguiContexts,
    fleet_query: Query<(Entity, &Fleet, Option<&FleetOrbit>, Option<&ActiveManeuver>)>,
    all_fleets_query: Query<(Entity, &Fleet, &SpaceCoordinates, Option<&FleetOrbit>, Option<&ActiveManeuver>), Without<CelestialBody>>,
    body_query: Query<(Entity, &CelestialBody, &SpaceCoordinates, Option<&KeplerOrbit>, Option<&LogicalParent>)>,
    body_system_ids: Query<&SystemId>,
    mut pending_actions: ResMut<PendingFleetActions>,
    mut fleet_ui_state: ResMut<FleetUiState>,
    sim_time: Res<SimulationTime>,
    current_system: Res<CurrentStarSystem>,
    nearby_stars: Res<NearbyStarsData>,
) {
    if !fleet_ui_state.show_transfer_popup {
        return;
    }

    let Some(fleet_entity) = fleet_ui_state.selected_fleet else {
        fleet_ui_state.show_transfer_popup = false;
        return;
    };

    let Ok((_, fleet, maybe_orbit, maybe_maneuver)) = fleet_query.get(fleet_entity) else {
        fleet_ui_state.show_transfer_popup = false;
        return;
    };

    let planner_orbit: Option<FleetOrbit> = if let Some(orbit) = maybe_orbit {
        Some(*orbit)
    } else if let Some(maneuver) = maybe_maneuver {
        Some(FleetOrbit::new(maneuver.destination_body, maneuver.arrival_orbit_radius_au))
    } else {
        None
    };

    let Some(orbit) = planner_orbit else {
        fleet_ui_state.show_transfer_popup = false;
        return;
    };

    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    let elapsed = sim_time.elapsed_seconds();
    let current_system_id = current_system.0;

    // `open` is a separate local bool — `Window::open()` sets it to false when
    // the user clicks the × close button.
    let mut open = true;
    egui::Window::new(format!("📡 Transfer Planner — {}", fleet.name))
        .open(&mut open)
        .resizable(true)
        .collapsible(false)
        .default_width(460.0)
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-10.0, 135.0))
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .max_height(600.0)
                .show(ui, |ui| {
                    render_transfer_planner(
                        ui,
                        fleet_entity,
                        fleet,
                        &orbit,
                        maybe_maneuver,
                        &body_query,
                        &all_fleets_query,
                        &mut fleet_ui_state,
                        &mut pending_actions,
                        current_system_id,
                        &body_system_ids,
                        elapsed,
                        &nearby_stars,
                    );
                });
        });

    if !open {
        fleet_ui_state.show_transfer_popup = false;
    }
}

/// Build a `PlannedTransfer` from the selected transfer option and fleet/body state.
fn build_planned_transfer(
    _fleet_entity: Entity,
    fleet: &Fleet,
    orbit: &FleetOrbit,
    target_entity: Entity,
    body_query: &Query<(Entity, &CelestialBody, &SpaceCoordinates, Option<&KeplerOrbit>, Option<&LogicalParent>)>,
    option: &TransferOption,
) -> Option<PlannedTransfer> {
    use crate::astronomy::KeplerOrbit;
    use crate::fleets::orbital_mechanics::{AU_IN_METERS, G_CONST, GM_SUN};

    let (_, origin_body, origin_sc, origin_ko, origin_lp) = body_query.get(orbit.body).ok()?;
    let (_, dest_body, _dest_sc, dest_ko, dest_lp) = body_query.get(target_entity).ok()?;

    let dest_parent = dest_lp.map(|lp| lp.0);
    let origin_parent = origin_lp.map(|lp| lp.0);
    let dest_is_star = dest_body.body_type == BodyType::Star;
    let dest_is_ring = dest_body.body_type == BodyType::Ring;

    // Determine: (origin_sma, dest_sma, gm, orbit_center, actual destination body for FleetOrbit)
    // For Rings: redirect the FleetOrbit destination to the ring's parent planet.
    // For Stars: Fleet will orbit the star at the planet SOI boundary; orbit_center = star entity.
    let (origin_sma_au, dest_sma_au, gm, orbit_center, actual_dest_body) = if dest_is_star {
        // Heliocentric escape: orbit body = current body's parent star
        let parent_mass = origin_body.mass;
        let planet_sma_au = origin_ko.map(|ko| ko.semi_major_axis).unwrap_or(1.0);
        let soi_au = planet_sma_au * (parent_mass / 1.989e30_f64).powf(0.4);
        (orbit.radius_au, soi_au.max(orbit.radius_au * 50.0), G_CONST * parent_mass, target_entity, target_entity)
    } else if dest_is_ring {
        // Ring: resolve to orbiting the ring's parent planet at ring.radius altitude
        let ring_parent = dest_parent.unwrap_or(orbit.body);
        let parent_mass = body_query.get(ring_parent).ok()
            .map(|(_, b, _, _, _)| b.mass).unwrap_or(5.972e24);
        let ring_radius_au = (dest_body.radius as f64 * 1_000.0) / AU_IN_METERS;
        let r1 = if ring_parent == orbit.body {
            orbit.radius_au
        } else {
            origin_ko.map(|ko| ko.semi_major_axis).unwrap_or(0.01)
        };
        (r1, ring_radius_au, G_CONST * parent_mass, ring_parent, ring_parent)
    } else if dest_parent == Some(orbit.body) {
        // Local (e.g., Earth → Moon)
        let r2 = dest_ko.map(|ko| ko.semi_major_axis).unwrap_or(0.00257);
        (orbit.radius_au, r2, G_CONST * origin_body.mass, orbit.body, target_entity)
    } else if dest_parent.is_some() && dest_parent == origin_parent {
        // Both orbit the same central body (moon-to-moon OR interplanetary, e.g. Earth→Mars).
        // NOTE: The Sun lacks SpaceCoordinates so body_query.get(Sun) fails — fall back to GM_SUN.
        let shared = dest_parent.unwrap();
        let gm = body_query.get(shared).ok()
            .map(|(_, b, _, _, _)| if b.body_type == BodyType::Star { GM_SUN } else { G_CONST * b.mass })
            .unwrap_or(GM_SUN);
        let r1 = origin_ko.map(|ko| ko.semi_major_axis).unwrap_or(0.00257);
        let r2 = dest_ko.map(|ko| ko.semi_major_axis).unwrap_or(0.00257);
        (r1, r2, gm, shared, target_entity)
    } else if Some(target_entity) == origin_parent {
        // Downward transfer: fleet is at a moon, destination is the parent planet.
        // e.g. Moon → Earth: r1 = Moon SMA around Earth, r2 = low parking orbit, gm = planet GM.
        let parent_mass = dest_body.mass;
        let r1 = origin_ko.map(|ko| ko.semi_major_axis).unwrap_or(0.00257);
        let r2 = (dest_body.radius as f64 * 3_000.0) / AU_IN_METERS;
        (r1, r2.min(r1 * 0.5), G_CONST * parent_mass, target_entity, target_entity)
    } else {
        // Heliocentric: if fleet is at a moon, its own SMA is Earth-relative — use parent's SMA.
        let r1 = if origin_ko.map(|ko| ko.semi_major_axis < 0.05).unwrap_or(true) {
            origin_parent
                .and_then(|pe| body_query.get(pe).ok())
                .and_then(|(_, _, _, ko, _)| ko)
                .map(|ko| ko.semi_major_axis)
                .or_else(|| origin_ko.map(|ko| ko.semi_major_axis))
                .unwrap_or(1.0)
        } else {
            origin_ko.map(|ko| ko.semi_major_axis).unwrap_or(1.0)
        };
        let r2 = dest_ko.map(|ko| ko.semi_major_axis).unwrap_or(1.5);
        let star = body_query.iter()
            .find(|(_, b, _, ko, _)| ko.is_none() && b.body_type == BodyType::Star)
            .map(|(e, _, _, _, _)| e)
            .unwrap_or(orbit.body);
        (r1, r2, GM_SUN, star, target_entity)
    };

    let outward = dest_sma_au >= origin_sma_au;
    let center_pos = body_query.get(orbit_center).map(|(_, _, sc, _, _)| sc.position).unwrap_or(bevy::math::DVec3::ZERO);
    let rel_pos = origin_sc.position - center_pos;
    let departure_angle = rel_pos.y.atan2(rel_pos.x);
    let argument_of_periapsis = if outward { departure_angle } else { departure_angle - std::f64::consts::PI };
    let mean_anomaly_epoch = if outward { 0.0 } else { std::f64::consts::PI };
    let sma_m = option.sma_au * AU_IN_METERS;
    let mean_motion = (gm / sma_m.powi(3)).sqrt();

    let transfer_orbit = KeplerOrbit {
        semi_major_axis: option.sma_au,
        eccentricity: option.eccentricity,
        inclination: 0.0,
        longitude_ascending_node: 0.0,
        argument_of_periapsis,
        mean_anomaly_epoch,
        mean_motion,
    };

    // Arrival orbit radius: for rings use the ring radius, otherwise reuse fleet parking radius
    let arrival_orbit_radius_au = if dest_is_ring {
        dest_sma_au
    } else if dest_is_star {
        dest_sma_au // park at SOI boundary initially
    } else {
        orbit.radius_au
    };

    let fuel_cost = fleet.total_fuel_cost_for_dv(option.total_delta_v_ms);

    Some(PlannedTransfer {
        origin_body: orbit.body,
        destination_body: actual_dest_body,
        orbit_center,
        transfer_orbit,
        duration_s: option.transfer_time_s,
        arrival_delta_v_ms: option.delta_v2_ms,
        arrival_orbit_radius_au,
        fuel_cost_t: fuel_cost,
        option_label: option.label,
        start_position_au: None,
        end_position_au: None,
    })
}

/// Build a `PlannedTransfer` targeting a Lagrange point (no dedicated ECS entity).
fn build_planned_transfer_lp(
    _fleet_entity: Entity,
    fleet: &Fleet,
    orbit: &FleetOrbit,
    lp: &LagrangeTarget,
    body_query: &Query<(Entity, &CelestialBody, &SpaceCoordinates, Option<&KeplerOrbit>, Option<&LogicalParent>)>,
    option: &TransferOption,
) -> Option<PlannedTransfer> {
    use crate::astronomy::KeplerOrbit;
    use crate::fleets::orbital_mechanics::AU_IN_METERS;

    // LP transfers are heliocentric – find the star as orbit center
    let star_entity = body_query.iter()
        .find(|(_, b, _, ko, _)| ko.is_none() && b.body_type == BodyType::Star)
        .map(|(e, _, _, _, _)| e)
        .unwrap_or(orbit.body);

    // Determine departure position.  For fleets orbiting the star directly
    // (e.g. after a previous LP transfer), `orbit.body` is the star whose
    // SpaceCoordinates are at the heliocentric origin → rel_pos would be
    // (0,0,0) and departure_angle 0.  In that case use the L-point's parent
    // planet position instead so the orbit geometry is meaningful.
    let center_pos = body_query.get(star_entity)
        .map(|(_, _, sc, _, _)| sc.position)
        .unwrap_or(bevy::math::DVec3::ZERO);

    let origin_pos = {
        let (_, body_data, origin_sc, _, _) = body_query.get(orbit.body).ok()?;
        if body_data.body_type == BodyType::Star {
            // Fleet is parked around the star — use the planet's current position
            // as the departure reference instead.
            body_query.get(lp.planet_entity)
                .map(|(_, _, sc, _, _)| sc.position)
                .unwrap_or(origin_sc.position)
        } else {
            origin_sc.position
        }
    };

    let rel_pos = origin_pos - center_pos;
    let departure_angle = rel_pos.y.atan2(rel_pos.x);

    // L1/L2 direct transfers use kinematic (straight-line) rendering.
    // Override the option label to trigger kinematic mode in the rendering pipeline.
    let is_direct_lp = matches!(lp.point, 1 | 2);
    let option_label: &'static str = if is_direct_lp {
        match option.label {
            "Efficient" => "Direct Efficient",
            "Moderate"  => "Direct Moderate",
            "Fast"      => "Direct Fast",
            other       => other,  // kinematic labels pass through unchanged
        }
    } else {
        option.label
    };

    let gm = lp.gm;
    let sma_m = option.sma_au * AU_IN_METERS;
    let mean_motion = (gm / sma_m.powi(3)).sqrt();

    let outward = lp.radius_au >= lp.planet_sma_au;
    let argument_of_periapsis = if outward {
        departure_angle
    } else {
        departure_angle - std::f64::consts::PI
    };
    let mean_anomaly_epoch = if outward { 0.0 } else { std::f64::consts::PI };

    let transfer_orbit = KeplerOrbit {
        semi_major_axis: option.sma_au,
        eccentricity: option.eccentricity,
        inclination: 0.0,
        longitude_ascending_node: 0.0,
        argument_of_periapsis,
        mean_anomaly_epoch,
        mean_motion,
    };

    let fuel_cost = fleet.total_fuel_cost_for_dv(option.total_delta_v_ms);

    // For direct L1/L2 transfers, pre-compute start and end positions so the
    // kinematic rendering draws a straight line to the LP, not to the Sun.
    let (start_pos, end_pos) = if is_direct_lp {
        // Start: fleet's origin body (planet) position
        let start = origin_pos;
        // End: LP position at the same angle as the planet (L1/L2 are along the
        // Sun-planet radial line).
        let planet_dir = if rel_pos.length() > 1e-12 {
            rel_pos.normalize()
        } else {
            bevy::math::DVec3::X
        };
        let end = center_pos + planet_dir * lp.radius_au;
        (Some(start), Some(end))
    } else {
        (None, None)
    };

    Some(PlannedTransfer {
        origin_body: orbit.body,
        // Lagrange point orbits are heliocentric: the fleet parks around the star at the
        // LP's heliocentric radius, NOT around the planet.  Parking around the planet at
        // `lp.radius_au` (≈1 AU) would put the fleet ~2 AU from the Sun and off-screen.
        destination_body: star_entity,
        orbit_center: star_entity,
        transfer_orbit,
        duration_s: option.transfer_time_s,
        arrival_delta_v_ms: option.delta_v2_ms,
        arrival_orbit_radius_au: lp.radius_au,
        fuel_cost_t: fuel_cost,
        option_label,
        start_position_au: start_pos,
        end_position_au: end_pos,
    })
}

// ── Fleet action bar (bottom overlay) ────────────────────────────────────────

/// Renders a thin action bar at the bottom of the 3D view whenever a fleet is
/// selected.  The bar is hidden in the full-screen Fleets panel, Research,
/// Construction, and Economy menus because those already fill the screen.
fn ui_fleet_action_bar(
    mut contexts: EguiContexts,
    active_menu: Res<ActiveMenu>,
    mut fleet_ui_state: ResMut<FleetUiState>,
    fleet_query: Query<(Entity, &Fleet, Option<&FleetOrbit>, Option<&ActiveManeuver>)>,
    mut pending_fleet_actions: ResMut<PendingFleetActions>,
    sim_time: Res<SimulationTime>,
) {
    // Only show when a fleet is selected AND we are NOT inside a full-screen panel.
    let Some(selected_entity) = fleet_ui_state.selected_fleet else {
        return;
    };

    if matches!(
        active_menu.current,
        GameMenu::Fleets
            | GameMenu::Research
            | GameMenu::Construction
            | GameMenu::Economy
    ) {
        return;
    }

    let Ok((_, fleet, maybe_orbit, maybe_maneuver)) = fleet_query.get(selected_entity) else {
        return; // Fleet was despawned
    };

    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    // `in_transit` was previously used by the old action bar logic but
    // the status string now derives directly from `maybe_maneuver`.
    let in_orbit = maybe_orbit.is_some();
    let ship_count = fleet.ships.len();
    // Only show abort when the fleet is waiting to depart (still has its parking orbit).
    // Canceling mid-flight is unsupported: there is no FleetOrbit to return to.
    let is_waiting_for_departure = maybe_maneuver
        .map(|m| sim_time.elapsed_seconds() < m.departure_time)
        .unwrap_or(false);

    // Determine which ship-class-dependent actions are available.
    // For now all friendly fleets can survey; combat actions are always shown
    // (hostile fleet detection comes in a future PR).
    let has_ships = ship_count > 0;

    egui::TopBottomPanel::bottom("fleet_action_bar")
        .min_height(48.0)
        .max_height(56.0)
        .show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                ui.add_space(8.0);

                // Fleet name + status label
                // Note: we compute the status string directly rather than creating
                // an `in_transit` boolean earlier, which keeps the variable list
                // minimal and avoids unused variable warnings.
                let status_str = if let Some(maneuver) = maybe_maneuver {
                    if sim_time.elapsed_seconds() < maneuver.departure_time {
                        " ⏳ Waiting to depart".to_string()
                    } else {
                        " ✈ In Transit".to_string()
                    }
                } else {
                    " 🛰 In Orbit".to_string()
                };
                ui.label(
                    egui::RichText::new(format!("🚀 {} —{status_str}", fleet.name))
                        .size(13.0)
                        .strong()
                        .color(egui::Color32::from_rgb(130, 220, 130)),
                );

                ui.separator();

                // Transfer Planner — always available
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("🗺 Transfer Planner").size(13.0),
                        )
                        .min_size(egui::Vec2::new(140.0, 36.0)),
                    )
                    .on_hover_text("Open the orbital transfer planner for this fleet")
                    .clicked()
                {
                    fleet_ui_state.show_transfer_popup = true;
                }

                ui.add_space(4.0);

                // Split Fleet — only useful when in orbit and has > 1 ship
                let can_split = in_orbit && ship_count > 1;
                if ui
                    .add_enabled(
                        can_split,
                        egui::Button::new(egui::RichText::new("✂ Split Fleet").size(13.0))
                            .min_size(egui::Vec2::new(110.0, 36.0)),
                    )
                    .on_hover_text("Detach selected ships into a new fleet")
                    .clicked()
                {
                    // Stub: split action will be fully implemented in a future update
                    info!("Split fleet requested for {:?}", selected_entity);
                }

                ui.add_space(4.0);

                // Merge Fleet — only when in orbit (merging in transit is not possible)
                let can_merge = in_orbit;
                if ui
                    .add_enabled(
                        can_merge,
                        egui::Button::new(egui::RichText::new("🔗 Merge Fleet").size(13.0))
                            .min_size(egui::Vec2::new(110.0, 36.0)),
                    )
                    .on_hover_text("Merge with another fleet at the same location")
                    .clicked()
                {
                    // Stub: merge action will be fully implemented in a future update
                    info!("Merge fleet requested for {:?}", selected_entity);
                }

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                // Survey — available when in orbit
                if ui
                    .add_enabled(
                        in_orbit && has_ships,
                        egui::Button::new(egui::RichText::new("🔭 Survey").size(13.0))
                            .min_size(egui::Vec2::new(86.0, 36.0)),
                    )
                    .on_hover_text("Survey the body this fleet is orbiting")
                    .clicked()
                {
                    info!("Survey requested for {:?}", selected_entity);
                }

                ui.add_space(4.0);

                // Attack — available when in transit or orbit
                if ui
                    .add_enabled(
                        has_ships,
                        egui::Button::new(
                            egui::RichText::new("⚔ Attack")
                                .size(13.0)
                                .color(egui::Color32::from_rgb(230, 130, 100)),
                        )
                        .min_size(egui::Vec2::new(80.0, 36.0)),
                    )
                    .on_hover_text("Engage enemy vessels in combat")
                    .clicked()
                {
                    info!("Attack requested for {:?}", selected_entity);
                }

                ui.add_space(4.0);

                // Bombard — requires orbit
                if ui
                    .add_enabled(
                        in_orbit && has_ships,
                        egui::Button::new(
                            egui::RichText::new("💣 Bombard")
                                .size(13.0)
                                .color(egui::Color32::from_rgb(230, 130, 100)),
                        )
                        .min_size(egui::Vec2::new(90.0, 36.0)),
                    )
                    .on_hover_text("Bombard the surface of the body being orbited")
                    .clicked()
                {
                    info!("Bombard requested for {:?}", selected_entity);
                }

                ui.add_space(4.0);

                // Invade — requires orbit
                if ui
                    .add_enabled(
                        in_orbit && has_ships,
                        egui::Button::new(
                            egui::RichText::new("👊 Invade")
                                .size(13.0)
                                .color(egui::Color32::from_rgb(230, 130, 100)),
                        )
                        .min_size(egui::Vec2::new(86.0, 36.0)),
                    )
                    .on_hover_text("Land troops to take control of the colony")
                    .clicked()
                {
                    info!("Invade requested for {:?}", selected_entity);
                }

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                // Refuel — only when in stable orbit
                let needs_fuel = fleet.ships.iter().any(|s| s.fuel_mass_t < s.max_fuel_t);
                if ui
                    .add_enabled(
                        in_orbit && needs_fuel,
                        egui::Button::new(egui::RichText::new("⛽ Refuel").size(13.0))
                            .min_size(egui::Vec2::new(86.0, 36.0)),
                    )
                    .on_hover_text(if in_orbit {
                        if needs_fuel {
                            "Refuel all ships to full capacity"
                        } else {
                            "All ships are already at full fuel"
                        }
                    } else {
                        "Cannot refuel while in transit"
                    })
                    .clicked()
                {
                    pending_fleet_actions.refuel_fleets.push(selected_entity);
                }

                // Abort button — only while waiting to depart (fleet still has its parking orbit)
                if is_waiting_for_departure {
                    ui.add_space(8.0);
                    ui.separator();
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("⛔ Abort Transfer")
                                    .size(13.0)
                                    .color(egui::Color32::from_rgb(220, 80, 80)),
                            )
                            .min_size(egui::Vec2::new(120.0, 36.0)),
                        )
                        .on_hover_text("Abort the planned transfer and return to parking orbit")
                        .clicked()
                    {
                        pending_fleet_actions.cancel_maneuvers.push(selected_entity);
                    }
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
        let entity = Entity::from_raw_u32(1).unwrap();
        selection.select(entity);

        assert!(selection.has_selection());
        assert_eq!(selection.get(), Some(entity));
    }
}
