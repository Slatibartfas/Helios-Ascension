//! UI module for the Helios Ascension interface
//!
//! Provides an egui-based dashboard showing:
//! - Resource stockpiles and critical resources
//! - Power grid status
//! - Selected celestial body information
//! - Time controls for simulation speed

use bevy::asset::AssetServer;
use bevy::asset::Handle;
use bevy::image::Image;
use bevy::prelude::*;
use bevy::time::Real;
use bevy::window::PrimaryWindow;
use bevy_egui::{egui, EguiContexts, EguiPrimaryContextPass};
use std::collections::HashMap;

pub mod interaction;

pub use interaction::Selection;

pub mod screenshot;
mod screenshot_state;

mod construction_panel;
pub mod cursors;
mod dashboard;
mod dossier_panel;
mod economy_panel;
mod fleets_panel;
pub mod icons;
pub mod notifications;
mod porkchop_panel;
mod research_panel;
mod resources_bar;
mod settings;
mod shipbuilding_state;
mod shipbuilding_tooltip;
mod shipbuilding_workspace;
mod tab;
mod tech_tree;
pub(super) mod theme;
pub mod time;
pub mod transfer_planner;

pub use settings::Settings;

pub use icons::{MenuIcons, ResearchIcons};
pub use time::{SimulationTime, TimeScale};

use construction_panel::ui_construction_panels;
use dashboard::{ui_dashboard, ui_time_controls};
use economy_panel::ui_economy_panels;
use fleets_panel::{
    switch_anchor_on_arrival, ui_fleet_action_bar, ui_fleets_panel, ui_transfer_planner_popup,
    ShippingCompanyFilter,
};
use icons::{load_menu_icons, load_research_icons, process_menu_icons, process_research_icons};
use research_panel::ui_research_panels;
use resources_bar::ui_resources_bar;
use shipbuilding_workspace::ShipbuildingWorkspacePlugin;
use time::advance_simulation_time;

use crate::astronomy::components::{CurrentStarSystem, SystemId};
use crate::astronomy::nearby_stars::NearbyStarsData;
use crate::astronomy::{
    AtmosphereComposition, Hovered, KeplerOrbit, LagrangePointMarkers, LastLpClick, Selected,
    SpaceCoordinates,
};
use crate::colony::{
    BuildingCategory, BuildingType, BuildingsData, Colony, ConstructionDebugSettings,
    ConstructionProject, EstablishOutpostRequest, PendingConstructionActions,
};
use crate::economy::components::{MineralDeposit, Population, SurveyLevel};
use crate::economy::{
    format_currency, format_power, GlobalBudget, MiningOperation, PlanetResources, PowerSourceType,
    ResourceRateTracker, ResourceType,
};
use crate::fleets::orbital_mechanics::{
    apply_thrust_limits, calculate_transfer_options, calculate_transfer_options_phased,
    co_orbital_phasing_options, compute_burn_time_s, compute_transfer_window,
    course_correction_transfer_options, direct_lp_transfer_options, find_gravity_assist_options,
    format_delta_v, format_duration, hohmann_transfer, keplerian_velocity_vector,
    kinematic_transfer_options, plane_change_angle, GravityAssistOption,
};
use crate::fleets::{
    AbortToOriginAction, ActiveManeuver, Fleet, FleetOrbit, MergeFleetAction, PendingFleetActions,
    PlannedTransfer, StartTransferAction, TransferOption, TransferReferenceFrame,
    TransferWindowInfo, AU_IN_METERS, GM_SUN, G_CONST,
};
use crate::game_state::{ActiveMenu, GameMenu};
use crate::plugins::camera::{
    capture_egui_panel_bounds, starmap_transition_radius, CameraAnchor, GameCamera, OrbitCamera,
    ViewMode,
};
use crate::plugins::solar_system::{CelestialBody, LogicalParent};
use crate::plugins::solar_system_data::BodyType;
use crate::plugins::starmap::{
    HoveredStarSystem, SelectedStarSystem, StarSystemIcon, SystemMetadata,
};
use crate::research::{
    ContextMenuState, EngineeringProject, ModifierType, PendingResearchActions, ResearchProject,
    ResearchState, ResearchTeam, ResearchTeamCapacity, TechCategory, TechEditData, TechModifierDef,
    TechTreeEditState, TechnologiesData, Technology,
};

/// Minimum supported window dimensions before showing the low-resolution warning.
/// The UI is now intended to remain usable at 1280×720, even though larger
/// windows still provide a better strategic overview.
const MIN_WINDOW_WIDTH: f32 = 1280.0;
const MIN_WINDOW_HEIGHT: f32 = 720.0;

/// Tracks which ledger category groups are currently expanded in the bodies panel.
/// Cleared at the start of each `ui_dashboard` frame, then repopulated as the
/// tree is rendered.  Key: `(parent_entity, group_label)`.
#[derive(Resource, Default)]
pub struct ExpandedLedgerGroups {
    pub groups: std::collections::HashSet<(Entity, String)>,
}

/// Resource to track if we should display the low resolution warning
#[derive(Resource, Default)]
pub struct ResolutionWarning {
    pub should_show: bool,
    pub dismissed: bool,
}

#[derive(Resource, Debug, Clone)]
pub struct ResearchUiPreferences {
    pub show_inactive_warning: bool,
    pub selected_engineering_target: Option<String>,
    /// Currently selected category in the Archive tab's
    /// `theme::tab_strip<TechCategory>` (PR-D / GRA-69). `None` means
    /// the first category in `TechCategory::all()` — kept as `Option`
    /// so a future "All categories" pseudo-tab can be added without
    /// a schema change.
    pub selected_archive_category: Option<crate::research::types::TechCategory>,
}

impl Default for ResearchUiPreferences {
    fn default() -> Self {
        Self {
            show_inactive_warning: true,
            selected_engineering_target: None,
            selected_archive_category: None,
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
    /// User-controlled parking radius (AU) for a star-approach target.
    /// `(star_entity, radius_au)`.  Set by the interactive
    /// `DestEntry::StarApproach` picker (GRA-161); consumed by
    /// `build_planned_transfer` to override `star_approach_radius_au(b)`
    /// for the destination star.  `None` means "use the per-body default
    /// from `CelestialBody.star_approach_au`".
    pub target_star_approach: Option<(Entity, f64)>,
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
    /// Cached porkchop grid for the current (fleet, target) pair.  When
    /// `Some`, the transfer planner renders the `PorkchopPanel` in place
    /// of the Efficient / Moderate / Fast `selectable_label` block.  When
    /// `None`, the legacy 3-option row is rendered.  GRA-152 (H-1).
    pub porkchop_grid: Option<crate::fleets::porkchop::PorkchopGrid>,
    /// `(col, row)` index of the player-selected cell in `porkchop_grid`.
    pub selected_porkchop_cell: Option<(usize, usize)>,
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
    /// Ship pending scrap confirmation popup: (fleet_entity, ship_index).
    pub scrap_confirm_ship: Option<(Entity, usize)>,
    /// Anchor for shift-range selection (the last plain-click entity).
    pub last_single_selected: Option<Entity>,
    /// Selected spawn location body for the "Create Fleet" picker.
    pub spawn_location_body: Option<Entity>,
    /// Number of full orbital laps the fleet will complete while waiting for planned
    /// departure (0 = depart immediately or no target selected).  Updated each frame
    /// by `draw_fleet_transfer_preview` and consumed by the Transfer Planner UI.
    pub waiting_orbit_count: u32,
}

impl FleetUiState {
    /// Clear all per-target state (transfer planning, rename, etc.).
    pub fn clear_target(&mut self) {
        self.target_body = None;
        self.target_lagrange = None;
        self.target_fleet = None;
        self.target_star_system = None;
        self.target_star_approach = None;
        self.selected_dest_category = None;
        self.departure_offset_days = 0.0;
        self.computed_options.clear();
        self.porkchop_grid = None;
        self.selected_porkchop_cell = None;
        self.planned_transfer = None;
        self.selected_option = 0;
        self.gravity_assist_candidates.clear();
        self.selected_gravity_assist = None;
        self.editing_fleet_name = None;
        self.waiting_orbit_count = 0;
    }

    /// Clear multi-selection state.
    pub fn clear_multi_selection(&mut self) {
        self.selected_fleets.clear();
        self.last_single_selected = None;
    }

    /// Select a Lagrange-point target and clear every other target slot.
    ///
    /// The four target fields (`target_body`, `target_lagrange`, `target_fleet`,
    /// `target_star_system`) are mutually exclusive — picking one invalidates
    /// the others and resets the per-target transfer-planning state
    /// (`computed_options`, `planned_transfer`, `selected_option`, etc.).
    /// GRA-160; mirrors the `Body`/`Ring`/`FleetTarget`/`StarSystem` branches
    /// in `render_transfer_planner`'s destination picker and the 3D-scene
    /// `ui_lp_click_handler` so both paths mutate state through one contract.
    ///
    /// Also clears the GRA-159 porkchop-grid cache and the GRA-161
    /// `target_star_approach` so the planner does not render a stale panel
    /// for a previous target.
    ///
    /// The porkchop-grid build for the new origin/dest pair is intentionally
    /// **not** triggered here — it is the caller's responsibility (the
    /// planner re-runs `build_grid_for_body_target` on its next tick once
    /// the new `target_lagrange` is observed).
    pub fn select_lagrange_target(&mut self, lp: LagrangeTarget) {
        self.target_lagrange = Some(lp);
        self.target_body = None;
        self.target_fleet = None;
        self.target_star_system = None;
        // GRA-161: Lagrange targets are not stars — drop any
        // star-approach parking radius the user had dialed in.
        self.target_star_approach = None;
        self.computed_options.clear();
        self.planned_transfer = None;
        self.selected_option = 0;
        self.selected_gravity_assist = None;
        // GRA-159: drop any cached body-path grid so a stale
        // (e.g. Earth→Mars) panel does not render with the Lagrange
        // picker selected.  The planner re-builds on its next tick.
        self.porkchop_grid = None;
        self.selected_porkchop_cell = None;
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
        egui::FontData::from_static(include_bytes!("../../assets/fonts/GeistMono-Medium.ttf"))
            .into(),
    );
    fonts.font_data.insert(
        "HackNerdFont".to_owned(),
        egui::FontData::from_static(include_bytes!(
            "../../assets/fonts/HackNerdFont-Regular.ttf"
        ))
        .into(),
    );
    // Hubot Sans for Headers
    fonts.font_data.insert(
        "Hubot-Sans-ExtraBoldExpanded".to_owned(),
        egui::FontData::from_static(include_bytes!(
            "../../assets/fonts/Hubot-Sans-ExtraBoldExpanded.ttf"
        ))
        .into(),
    );
    fonts.font_data.insert(
        "Hubot-Sans-SemiBoldCondensed".to_owned(),
        egui::FontData::from_static(include_bytes!(
            "../../assets/fonts/Hubot-Sans-SemiBoldCondensed.ttf"
        ))
        .into(),
    );
    // Noto Emoji for broad monochrome emoji coverage (Unicode 15+)
    fonts.font_data.insert(
        "NotoEmoji".to_owned(),
        egui::FontData::from_static(include_bytes!("../../assets/fonts/NotoEmoji-Regular.ttf"))
            .into(),
    );
    // Noto Sans Symbols 2 for astronomical (☉), geometric, and misc symbols
    fonts.font_data.insert(
        "NotoSansSymbols2".to_owned(),
        egui::FontData::from_static(include_bytes!(
            "../../assets/fonts/NotoSansSymbols2-Regular.ttf"
        ))
        .into(),
    );

    // Setup font families with Inter as primary, HackNerdFont as fallback for icons
    // Added "emoji-icon-font" (default egui emoji font) to fix broken emojis
    fonts.families.insert(
        egui::FontFamily::Proportional,
        vec![
            "Inter-Regular".to_owned(),
            "HackNerdFont".to_owned(),     // Fallback for developer icons
            "NotoEmoji".to_owned(),        // Broad emoji coverage
            "NotoSansSymbols2".to_owned(), // Astronomical & geometric symbols
            "emoji-icon-font".to_owned(),  // egui built-in (last resort)
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
        theme::apply_global_visuals(ctx);
    }
}

impl Plugin for UIPlugin {
    fn build(&self, app: &mut App) {
        app
            // Egui plugin is added in `main.rs` (explicit bevy_egui integration)
            .add_plugins(cursors::CursorPlugin)
            .add_plugins(ShipbuildingWorkspacePlugin)
            // Resources
            .init_resource::<Selection>()
            .init_resource::<TimeScale>()
            .init_resource::<SimulationTime>()
            .init_resource::<ResearchUiPreferences>()
            .init_resource::<Settings>()
            .init_resource::<ShippingCompanyFilter>()
            .init_resource::<FleetUiState>()
            .init_resource::<ResolutionWarning>()
            .init_resource::<ExpandedLedgerGroups>()
            .init_resource::<construction_panel::ConstructionUiState>()
            .init_resource::<shipbuilding_state::ShipbuildingUiState>()
            // ActiveMenu is now initialized in GameStatePlugin
            // to allow access in camera/starmap plugins
            // Load menu icons at startup
            .add_systems(
                Startup,
                (
                    load_menu_icons,
                    load_research_icons,
                    setup_egui_fonts,
                    check_window_resolution,
                ),
            )
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
            // Notifications render is a foreign SystemSet (declared
            // in `src/ui/notifications/systems/mod.rs`) so it can't
            // join the same `chain()` tuple as `UiSystemSet`. Order
            // it explicitly with `.after()` here so toasts paint
            // after the Overlays set.
            .configure_sets(
                EguiPrimaryContextPass,
                notifications::NotificationsSystemSet::Render.after(UiSystemSet::Overlays),
            )
            .add_systems(
                EguiPrimaryContextPass,
                (ui_resources_bar, ui_top_menu_bar, ui_time_controls)
                    .chain()
                    .in_set(UiSystemSet::TopBar),
            )
            .add_systems(
                EguiPrimaryContextPass,
                ui_dashboard.in_set(UiSystemSet::MainPanels),
            )
            .add_systems(
                EguiPrimaryContextPass,
                dossier_panel::ui_planet_dossier.in_set(UiSystemSet::MainPanels),
            )
            .add_systems(
                EguiPrimaryContextPass,
                ui_research_panels.in_set(UiSystemSet::MainPanels),
            )
            .add_systems(
                EguiPrimaryContextPass,
                ui_construction_panels.in_set(UiSystemSet::MainPanels),
            )
            .add_systems(
                EguiPrimaryContextPass,
                ui_economy_panels.in_set(UiSystemSet::MainPanels),
            )
            .add_systems(
                EguiPrimaryContextPass,
                ui_fleets_panel.in_set(UiSystemSet::MainPanels),
            )
            .add_systems(
                EguiPrimaryContextPass,
                ui_fleet_action_bar.in_set(UiSystemSet::MainPanels),
            )
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
                    switch_anchor_on_arrival
                        .after(crate::fleets::systems::complete_fleet_maneuvers),
                ),
            )
            // Capture egui's available_rect AFTER all panels have registered themselves
            // this frame. The camera system reads this next frame to detect panel bounds.
            // Must run in Update (inside egui's frame), not PostUpdate (context is closed).
            .add_systems(
                EguiPrimaryContextPass,
                capture_egui_panel_bounds.after(UiSystemSet::Overlays),
            )
            // Notifications toast panel (GRA-136 PR-B). Paints in
            // `EguiPrimaryContextPass`; the chain above
            // (`TopBar → MainPanels → Overlays → Render`) already
            // orders it after `Overlays` so toasts sit on top of
            // every other surface. The `NotificationsSystemSet::Tick`
            // set is added in `Update` from `NotificationsPlugin::build`
            // — the tick systems need to be free of the egui context.
            .add_systems(
                EguiPrimaryContextPass,
                notifications::systems::render_notification_toasts
                    .in_set(notifications::NotificationsSystemSet::Render),
            )
            // Screenshot plugin (Shift+F12 manual capture, 5 named slots).
            // Pure data + keybind + capture pump; the heavy
            // `bevy::render::view::screenshot` import is gated under
            // `#[cfg(not(test))]` so the test target's incremental compile
            // stays within the GHA 5:00 cliff.
            .add_plugins(screenshot::ScreenshotPlugin)
            // Notifications foundation (GRA-135 PR-A) + tick /
            // render wiring (GRA-136 PR-B). The Tick set is
            // added in `Update` from inside the plugin so the
            // tick systems stay decoupled from the egui pass.
            .add_plugins(notifications::NotificationsPlugin);
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
/// - `ViewMode::Starmap` → `GameMenu::Starmap` when the neutral survey view is active
/// - `ViewMode::System` → `GameMenu::Survey` when the neutral starmap ledger is active
fn sync_active_menu_with_view_mode(view_mode: Res<ViewMode>, mut active_menu: ResMut<ActiveMenu>) {
    if !view_mode.is_changed() {
        return;
    }

    match *view_mode {
        ViewMode::Starmap => {
            if active_menu.current == GameMenu::Survey {
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

fn switch_to_starmap_menu(
    view_mode: &mut ResMut<ViewMode>,
    camera_query: &mut Query<(&mut OrbitCamera, &mut CameraAnchor), With<GameCamera>>,
    starmap_radius: f32,
) {
    **view_mode = ViewMode::Starmap;
    if let Ok((mut orbit, mut anchor)) = camera_query.single_mut() {
        orbit.radius = starmap_radius;
        orbit.target_center = Vec3::ZERO;
        anchor.0 = None;
    }
}

fn switch_to_survey_menu(
    view_mode: &mut ResMut<ViewMode>,
    camera_query: &mut Query<(&mut OrbitCamera, &mut CameraAnchor), With<GameCamera>>,
    star_icon_query: &Query<(Entity, Option<&SelectedStarSystem>), With<StarSystemIcon>>,
    survey_radius: f32,
) {
    **view_mode = ViewMode::System;
    if let Ok((mut orbit, mut anchor)) = camera_query.single_mut() {
        if anchor.0.is_none() {
            if let Some((sel_entity, _)) = star_icon_query.iter().find(|(_, sel)| sel.is_some()) {
                anchor.0 = Some(sel_entity);
            }
        }

        orbit.radius = survey_radius.clamp(orbit.min_radius, orbit.max_radius);
    }
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
    mut notifications_open: ResMut<notifications::NotificationsSettingsOpen>,
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
                icon_textures.entry(*mkey).or_insert_with(|| {
                    contexts.add_image(bevy_egui::EguiTextureHandle::Strong(handle.clone()))
                });
            }
            // Clone the cached map so the rest of the UI code can use an owned
            // HashMap just like before.
            Some(icon_textures.clone())
        } else {
            None
        };

    // Pre-compute camera radii for explicit navigation between the neutral
    // survey and starmap views.
    let starmap_threshold = {
        let bounding_radius_au = system_metadata.get_bounding_radius(current_system.0);
        starmap_transition_radius(bounding_radius_au)
    };
    let starmap_radius = starmap_threshold * 1.5;
    let survey_radius = (starmap_threshold * 0.75).max(20_000.0);

    let ctx = match contexts.ctx_mut() {
        Ok(ctx) => ctx,
        Err(_) => return,
    };

    if pending_research.navigate_to_available_tab
        || pending_research.navigate_to_available_engineering_tab
    {
        active_menu.current = GameMenu::Research;
    }

    egui::TopBottomPanel::top("top_menu_bar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.add_space(10.0);

            // Add each menu button
            for (idx, &menu) in GameMenu::all().iter().enumerate() {
                let is_active = active_menu.current == menu;

                // compute tooltip with corresponding F-key. The key is
                // rendered through `theme::kbd_shortcut_label` so it picks
                // up the project's keycap style (bold mono, accent colour)
                // and stays consistent with the dashboard speed controls.
                // No number-key alias: digits 1-5 are bound to game-speed
                // presets in dashboard.rs and digits 6-9 are reserved for
                // future speed tiers, so they cannot double as menu openers.
                let fkey_label = format!("F{}", idx + 1);
                let menu_name = menu.name();
                let render_tooltip = |ui: &mut egui::Ui| {
                    theme::tooltip_frame().show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(menu_name).color(theme::TEXT));
                            ui.label(egui::RichText::new("(hotkey ").color(theme::TEXT_DIM));
                            ui.label(theme::kbd_shortcut_label(&fkey_label));
                            ui.label(egui::RichText::new(")").color(theme::TEXT_DIM));
                        });
                    });
                };

                if let Some(map) = texture_map.as_ref() {
                    if let Some(texture_id) = map.get(&menu) {
                        let size = egui::vec2(80.0, 80.0);

                        // Tint the icon:
                        // Cyan for active, clearly visible light-grey for inactive
                        let tint = if is_active {
                            theme::ACCENT
                        } else {
                            theme::ICON_INACTIVE
                        };

                        let mut img = egui::Image::new((*texture_id, size));
                        img = img.tint(tint);

                        let resp = ui.add(egui::Button::image(img));

                        // Highlight active menu by drawing a subtle stroke around the widget
                        if is_active {
                            let rect = resp.rect;
                            ui.painter().rect_stroke(
                                rect,
                                4.0,
                                egui::Stroke::new(2.0, theme::ACCENT),
                                egui::StrokeKind::Outside,
                            );
                        }

                        // Keyboard-focus ring (visible even when the menu isn't active)
                        theme::paint_focus_ring(ui.painter(), resp.rect, resp.has_focus());

                        let resp = resp.on_hover_ui(render_tooltip);
                        if resp.clicked() {
                            active_menu.current = menu;
                            match menu {
                                GameMenu::Starmap => switch_to_starmap_menu(
                                    &mut view_mode,
                                    &mut camera_query,
                                    starmap_radius,
                                ),
                                GameMenu::Survey => switch_to_survey_menu(
                                    &mut view_mode,
                                    &mut camera_query,
                                    &star_icon_query,
                                    survey_radius,
                                ),
                                _ => {}
                            }
                        }
                    } else {
                        // Fallback to text button when the texture is not available
                        let button_text = format!("{} {}", menu.icon(), menu.name());
                        let button = if is_active {
                            egui::Button::new(
                                egui::RichText::new(button_text)
                                    .size(14.0)
                                    .color(theme::ACCENT),
                            )
                            .fill(theme::SURFACE_RAISED)
                        } else {
                            egui::Button::new(egui::RichText::new(button_text).size(14.0))
                                .fill(theme::SURFACE)
                        };

                        let resp = ui.add(button).on_hover_ui(render_tooltip);
                        theme::paint_focus_ring(ui.painter(), resp.rect, resp.has_focus());
                        if resp.clicked() {
                            active_menu.current = menu;
                            match menu {
                                GameMenu::Starmap => switch_to_starmap_menu(
                                    &mut view_mode,
                                    &mut camera_query,
                                    starmap_radius,
                                ),
                                GameMenu::Survey => switch_to_survey_menu(
                                    &mut view_mode,
                                    &mut camera_query,
                                    &star_icon_query,
                                    survey_radius,
                                ),
                                _ => {}
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
                                .color(theme::ACCENT),
                        )
                        .fill(theme::SURFACE_RAISED)
                    } else {
                        egui::Button::new(egui::RichText::new(button_text).size(14.0))
                            .fill(theme::SURFACE)
                    };

                    let resp = ui.add(button).on_hover_ui(render_tooltip);
                    theme::paint_focus_ring(ui.painter(), resp.rect, resp.has_focus());
                    if resp.clicked() {
                        active_menu.current = menu;
                        match menu {
                            GameMenu::Starmap => switch_to_starmap_menu(
                                &mut view_mode,
                                &mut camera_query,
                                starmap_radius,
                            ),
                            GameMenu::Survey => switch_to_survey_menu(
                                &mut view_mode,
                                &mut camera_query,
                                &star_icon_query,
                                survey_radius,
                            ),
                            _ => {}
                        }
                    }
                }

                ui.add_space(5.0);
            }

            // ── Notifications settings toggle (PR-E / GRA-139) ───────
            // Pushed to the right of the menu buttons so the player
            // can reach it without crowding the existing icons. The
            // panel itself is rendered by
            // `notifications::ui_notifications_settings_panel` in
            // `UiSystemSet::Overlays` so it paints on top of every
            // other panel.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(theme::Spacing::md);
                let mut settings_open = notifications_open.0;
                let button_text = if settings_open {
                    "🔔 Notifications ✓"
                } else {
                    "🔔 Notifications"
                };
                let button = if settings_open {
                    egui::Button::new(
                        egui::RichText::new(button_text)
                            .size(14.0)
                            .color(theme::ACCENT),
                    )
                    .fill(theme::SURFACE_RAISED)
                } else {
                    egui::Button::new(egui::RichText::new(button_text).size(14.0))
                        .fill(theme::SURFACE)
                };
                let resp = ui.add(button);
                theme::paint_focus_ring(ui.painter(), resp.rect, resp.has_focus());
                if resp.clicked() {
                    settings_open = !settings_open;
                }
                notifications_open.0 = settings_open;
            });
        });
    });

    // ── Keyboard hotkeys ──────────────────────────────────────────────────────
    // Skip hotkeys while a text widget has focus (e.g. fleet-name editor).
    let has_keyboard_focus = ctx.memory(|m| m.focused().is_some());
    if !has_keyboard_focus {
        enum HotkeyIntent {
            SetMenu(usize),
            Escape,
        }
        // F11 is reserved for the screenshot pipeline (GRA-53, PR-0 of the
        // UI harmonization roadmap); see `screenshot_state` for the slot
        // and keybind contract. F12 stays as the construction/research
        // debug toggle.
        //
        // Number-key aliases (added in GRA-57, removed per operator
        // sign-off on GRA-59): digits 1-5 are bound to game-speed presets
        // in `dashboard.rs` (see `speed_keys` + `SPEED_PRESETS`), so
        // doubling them as menu openers caused the game to speed up
        // whenever the player tried to open a menu. Numbers stay
        // reserved for the dashboard speed tier ladder; menus open
        // exclusively via F1-F11.
        let fkeys = [
            egui::Key::F1,
            egui::Key::F2,
            egui::Key::F3,
            egui::Key::F4,
            egui::Key::F5,
            egui::Key::F6,
            egui::Key::F7,
            egui::Key::F8,
            egui::Key::F9,
            egui::Key::F10,
            egui::Key::F11,
        ];
        let intent: Option<HotkeyIntent> = ctx.input_mut(|i| {
            for (idx, &fkey) in fkeys.iter().enumerate() {
                if i.consume_key(egui::Modifiers::NONE, fkey) {
                    return Some(HotkeyIntent::SetMenu(idx));
                }
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
                return Some(HotkeyIntent::Escape);
            }
            None
        });
        if let Some(intent) = intent {
            match intent {
                HotkeyIntent::SetMenu(idx) => {
                    if let Some(&target_menu) = GameMenu::all().get(idx) {
                        active_menu.current = target_menu;
                        match target_menu {
                            GameMenu::Starmap => switch_to_starmap_menu(
                                &mut view_mode,
                                &mut camera_query,
                                starmap_radius,
                            ),
                            GameMenu::Survey => switch_to_survey_menu(
                                &mut view_mode,
                                &mut camera_query,
                                &star_icon_query,
                                survey_radius,
                            ),
                            _ => {}
                        }
                    }
                }
                HotkeyIntent::Escape => {
                    // If we're on the neutral Survey / Starmap view, ESC opens the main menu.
                    // If a menu panel is open, ESC dismisses it and returns to the base view.
                    let base_view = match *view_mode {
                        ViewMode::Starmap => GameMenu::Starmap,
                        ViewMode::System => GameMenu::Survey,
                    };
                    if matches!(active_menu.current, GameMenu::Survey | GameMenu::Starmap) {
                        active_menu.current = GameMenu::Main;
                    } else {
                        active_menu.current = base_view;
                    }
                }
            }
        }
    }
}

/// Render floating labels next to star system icons in starmap view
fn ui_starmap_labels(
    mut contexts: EguiContexts,
    view_mode: Res<ViewMode>,
    active_menu: Res<ActiveMenu>,
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

    if active_menu.current.blocks_world_interaction() {
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
                theme::ACCENT
            } else {
                theme::TEXT_DIM
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

fn ui_hover_tooltip(
    mut contexts: EguiContexts,
    hovered_query: Query<
        (
            &CelestialBody,
            Option<&crate::plugins::starmap::PlanetCategory>,
            Option<&crate::astronomy::OceanProperties>,
        ),
        With<Hovered>,
    >,
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
                        .fill(theme::TOOLTIP_BG)
                        .stroke(egui::Stroke::new(2.0, theme::ACCENT_DIM))
                        .inner_margin(theme::Spacing::lg)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(format!("L{}", m.point))
                                        .size(16.0)
                                        .color(theme::ACCENT)
                                        .strong(),
                                );
                                ui.label(
                                    egui::RichText::new(format!(" \u{2013} {}", m.planet_name))
                                        .size(16.0)
                                        .color(theme::TEXT_VALUE)
                                        .strong(),
                                );
                            });
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(lp_qualifier(m.point))
                                        .size(12.0)
                                        .color(theme::TEXT_DIM),
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
                                format!(
                                    "{:.0} km from {}",
                                    dist_from_planet_au * AU_KM,
                                    m.planet_name
                                )
                            } else {
                                format!("{:.3} AU from {}", dist_from_planet_au, m.planet_name)
                            };
                            let stability = match m.point {
                                4 | 5 => ("Stable", theme::GREEN),
                                _ => ("Unstable", theme::AMBER),
                            };
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(dist_str)
                                        .size(11.0)
                                        .color(theme::TEXT_DIM),
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
                                        .color(theme::TEXT_HINT),
                                );
                            });
                        });
                });
            return;
        }
    }

    // Display hover tooltip if a body is hovered
    if let Ok((body, category_opt, ocean_props)) = hovered_query.single() {
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
                    .fill(theme::TOOLTIP_BG)
                    .stroke(egui::Stroke::new(2.0, theme::ACCENT_DIM))
                    .inner_margin(theme::Spacing::lg)
                    .show(ui, |ui| {
                        // Use horizontal layout to prevent narrow wrapping
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(&body.name)
                                    .size(16.0)
                                    .color(theme::ACCENT)
                                    .strong(),
                            );
                        });

                        // Show planet category if available, otherwise fall back to body type
                        let type_label = if let Some(cat) = category_opt {
                            // Capitalise the category for display (e.g. "desert" → "Desert")
                            let mut s = cat.0.clone();
                            if let Some(first) = s.get_mut(..1) {
                                first.make_ascii_uppercase();
                            }
                            s
                        } else {
                            format!("{:?}", body.body_type)
                        };
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!("Type: {}", type_label))
                                    .size(12.0)
                                    .color(theme::TEXT_DIM),
                            );
                        });

                        // Ocean indicator
                        if let Some(ocean) = ocean_props {
                            let (icon, text, color) = if ocean.is_subsurface {
                                ("\u{1F9CA}", "Subsurface Ocean", theme::OCEAN_SUBSURFACE)
                            } else {
                                match ocean.ocean_type {
                                    crate::astronomy::OceanType::Water => {
                                        ("\u{1F30A}", "Water Ocean", theme::OCEAN_WATER)
                                    }
                                    crate::astronomy::OceanType::Methane => {
                                        ("\u{1F7E0}", "Methane Ocean", theme::OCEAN_METHANE)
                                    }
                                    crate::astronomy::OceanType::Hydrocarbon => {
                                        ("\u{26FD}", "Hydrocarbon Lakes", theme::OCEAN_HYDROCARBON)
                                    }
                                    crate::astronomy::OceanType::Ammonia => {
                                        ("\u{1F7E3}", "Ammonia Ocean", theme::OCEAN_AMMONIA)
                                    }
                                    crate::astronomy::OceanType::Subsurface => {
                                        ("\u{1F9CA}", "Subsurface Ocean", theme::OCEAN_SUBSURFACE)
                                    }
                                }
                            };
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(format!("{} {}", icon, text))
                                        .size(11.0)
                                        .color(color),
                                );
                            });
                        }
                    });
            });
    }
}

/// Read [`LastLpClick`] resource and update the fleet transfer planner
/// so that the clicked LP becomes the active transfer target.
///
/// GRA-160: left-clicking an L4/L5 marker in the 3D scene now drives the
/// planner just like picking the LP in the destination dropdown.  The
/// `handle_lp_hover` system in `astronomy::lagrange` writes
/// [`LastLpClick`] on each left-click; this system drains it and routes
/// the click through [`FleetUiState::select_lagrange_target`] so the
/// destination-picker and 3D-scene click paths share one state-mutation
/// contract.
fn ui_lp_click_handler(
    mut last_click: ResMut<LastLpClick>,
    mut fleet_ui_state: ResMut<FleetUiState>,
) {
    let Some(m) = last_click.info.take() else {
        return;
    };
    let lp = LagrangeTarget {
        point: m.point,
        planet_entity: m.planet_entity,
        planet_name: m.planet_name,
        planet_sma_au: m.planet_sma_au,
        radius_au: m.lp_radius_au,
        gm: m.gm,
    };
    fleet_ui_state.select_lagrange_target(lp);
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
                    .fill(theme::TOOLTIP_BG)
                    .stroke(egui::Stroke::new(2.0, theme::AMBER))
                    .inner_margin(theme::Spacing::lg)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(&icon.name)
                                    .size(16.0)
                                    .color(theme::STAR_GOLD)
                                    .strong(),
                            );
                        });

                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!("Distance: {:.2} ly", distance_ly))
                                    .size(12.0)
                                    .color(theme::TEXT_DIM),
                            );
                        });

                        if body_count > 0 {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(format!("Bodies: {}", body_count))
                                        .size(12.0)
                                        .color(theme::TEXT_DIM),
                                );
                            });
                        }
                    });
            });
    }
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
                        .color(theme::STATUS_WARN)
                );
                ui.add_space(10.0);
                ui.label(
                    egui::RichText::new("Low Resolution Detected")
                        .size(18.0)
                        .strong()
                        .color(theme::STAR_GOLD)
                );
            });

            ui.separator();
            ui.add_space(theme::Spacing::lg);

            // Current vs Required
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label("Your resolution:");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(format!("{}×{}", current_width as u32, current_height as u32))
                                .strong()
                                .size(15.0)
                                .color(theme::STATUS_ERROR)
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
                                .color(theme::STATUS_SUCCESS)
                        );
                    });
                });
            });

            ui.add_space(theme::Spacing::lg);

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
            ui.add_space(theme::Spacing::sm);
            ui.label(
                egui::RichText::new("At lower resolutions, these elements will overlap and become difficult or impossible to use.")
                    .size(12.0)
                    .color(theme::STATUS_NEUTRAL)
            );

            ui.add_space(theme::Spacing::lg);
            ui.separator();
            ui.add_space(theme::Spacing::sm);

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
            ui.add_space(theme::Spacing::sm);

            let mut dismiss = false;
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("You may continue, but expect UI issues.")
                        .size(11.0)
                        .italics()
                        .color(theme::STATUS_MUTED)
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

#[cfg(test)]
mod tests {
    use super::{ui_lp_click_handler, FleetUiState, LagrangeTarget};
    use crate::astronomy::components::LpMarkerInfo;
    use crate::fleets::orbital_mechanics::TransferOption;
    use crate::ui::LastLpClick;
    use bevy::ecs::system::RunSystemOnce;
    use bevy::prelude::*;

    fn earth_lp(point: u8) -> LagrangeTarget {
        LagrangeTarget {
            point,
            planet_entity: Entity::PLACEHOLDER,
            planet_name: "Earth".to_string(),
            planet_sma_au: 1.0,
            radius_au: if point == 1 { 0.99 } else { 1.01 },
            gm: 1.327_124_400_18e11_f64,
        }
    }

    fn earth_lp_marker(point: u8) -> LpMarkerInfo {
        LpMarkerInfo {
            render_pos: bevy::math::Vec3::ZERO,
            hit_radius: 4.0,
            point,
            planet_entity: Entity::PLACEHOLDER,
            planet_name: "Earth".to_string(),
            planet_sma_au: 1.0,
            lp_radius_au: if point == 1 { 0.99 } else { 1.01 },
            gm: 1.327_124_400_18e11_f64,
        }
    }

    fn legacy_option() -> TransferOption {
        TransferOption {
            label: "legacy",
            total_delta_v_ms: 0.0,
            delta_v1_ms: 0.0,
            delta_v2_ms: 0.0,
            transfer_time_s: 0.0,
            sma_au: 0.0,
            eccentricity: 0.0,
            energy_multiplier: 0.0,
            burn_time_s: 0.0,
            plane_change_dv_ms: 0.0,
            is_thrust_limited: false,
            transfer_orbit_override: None,
        }
    }

    /// GRA-160: selecting an LP via the destination picker (or any other
    /// path that goes through `select_lagrange_target`) must set
    /// `target_lagrange` and clear the other mutually-exclusive target
    /// slots plus the per-target transfer-planning state.
    #[test]
    fn select_lagrange_target_sets_lp_and_clears_other_targets() {
        let mut state = FleetUiState::default();
        // Pre-populate every other target slot and per-target state so we
        // can verify the helper clears them all atomically.
        state.target_body = Some(Entity::PLACEHOLDER);
        state.target_fleet = Some(Entity::PLACEHOLDER);
        state.target_star_system = Some((0, "Sol".to_string(), 0.0_f32));
        state.selected_option = 3;
        state.selected_gravity_assist = Some(2);
        state.computed_options.push(legacy_option());

        state.select_lagrange_target(earth_lp(1));

        assert_eq!(state.target_lagrange.as_ref().map(|lp| lp.point), Some(1));
        assert_eq!(
            state.target_lagrange.as_ref().map(|lp| lp.planet_entity),
            Some(Entity::PLACEHOLDER)
        );
        assert!(state.target_body.is_none(), "target_body must be cleared");
        assert!(state.target_fleet.is_none(), "target_fleet must be cleared");
        assert!(
            state.target_star_system.is_none(),
            "target_star_system must be cleared"
        );
        assert!(
            state.computed_options.is_empty(),
            "computed_options must be cleared"
        );
        assert_eq!(state.selected_option, 0, "selected_option must reset");
        assert!(
            state.selected_gravity_assist.is_none(),
            "selected_gravity_assist must clear"
        );
    }

    /// GRA-160: `target_lagrange` is mutually exclusive with `target_body`.
    /// Selecting a body via the destination picker (mirrored here by
    /// direct field write) must clear `target_lagrange` — this is the
    /// symmetric half of the contract, locked by the Body/Ring click
    /// branches in `render_transfer_planner`.
    #[test]
    fn selecting_body_clears_target_lagrange() {
        let mut state = FleetUiState::default();
        state.target_lagrange = Some(earth_lp(1));
        // Body/Ring click arms set `target_body = Some(...)` and clear
        // `target_lagrange` together.  Mirror that exact mutation here.
        state.target_body = Some(Entity::PLACEHOLDER);
        state.target_lagrange = None;
        state.target_fleet = None;
        state.target_star_system = None;
        state.computed_options.clear();
        state.planned_transfer = None;
        state.selected_option = 0;
        state.selected_gravity_assist = None;

        assert!(state.target_lagrange.is_none());
        assert_eq!(state.target_body, Some(Entity::PLACEHOLDER));
    }

    /// GRA-160: left-clicking an LP marker in the 3D scene dispatches
    /// through `ui_lp_click_handler` to the same state-mutation contract
    /// as a destination-picker click.  Verifies the 3D-scene click path
    /// populates `target_lagrange` from a `LpMarkerInfo` and clears
    /// `LastLpClick` (so the click doesn't accumulate).
    #[test]
    fn ui_lp_click_handler_dispatches_marker_to_lagrange_target() {
        let mut world = World::new();
        world.init_resource::<LastLpClick>();
        world.insert_resource(FleetUiState::default());
        // Pre-set a body target so we can verify the handler clears it.
        world
            .resource_mut::<FleetUiState>()
            .target_body
            .replace(Entity::PLACEHOLDER);

        // Simulate `handle_lp_hover` writing a click into the resource.
        world.resource_mut::<LastLpClick>().info = Some(earth_lp_marker(4));

        // Run the system under test.  RunSystemOnce only needs the two
        // resources the handler actually reads; the input is exactly what
        // `handle_lp_hover` would produce on a real L4 marker click.
        let _ = world.run_system_once(ui_lp_click_handler);

        let state = world.resource::<FleetUiState>();
        let lp = state
            .target_lagrange
            .as_ref()
            .expect("target_lagrange must be set after LP click");
        assert_eq!(lp.point, 4);
        assert_eq!(lp.planet_entity, Entity::PLACEHOLDER);
        assert_eq!(lp.planet_name, "Earth");
        assert!(state.target_body.is_none(), "click must clear target_body");
        assert!(state.target_fleet.is_none());
        assert!(state.target_star_system.is_none());
        assert!(state.computed_options.is_empty());
        assert_eq!(state.selected_option, 0);
        assert!(state.selected_gravity_assist.is_none());

        // The handler must drain LastLpClick so the click doesn't replay.
        assert!(
            world.resource::<LastLpClick>().info.is_none(),
            "LastLpClick must be consumed by the handler"
        );
    }
}
