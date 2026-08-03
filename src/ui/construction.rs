//! Construction panel rendered with `bevy_ui` (Phase C, `rework-ui-design`).
//!
//! This renders the Construction panel using
//! `bevy_ui` (replacing the legacy egui version).
//! Activated on `F4` (Construction menu).


//!
//! The canary renders:
//! - A window-filling root container with `BODY_BG` background.
//! - A top AppBar with the panel title "Construction" and subtitle.
//! - A 4-column grid of build-card placeholders (5 cards from the
//!   current egui panel's data).
//! - A bottom dock with status + speed chips + date.
//!
//! It does NOT (yet) render:
//! - The tab strip (Overview / Buildings / Build / Stockpiles).
//! - The category chips (Infrastructure / Mining & Industry / ...).
//! - The resource strip (Treasury / Balance / Energy / Active Colony).
//! - Hover scale / click feedback.

#![allow(dead_code)]

use bevy::prelude::*;
use bevy::input::mouse::MouseWheel;
use bevy::picking::events::{Out, Over, Pointer, Press, Release};
use bevy::picking::hover::HoverMap;
use bevy::picking::pointer::{PointerButton, PointerId};
use bevy::picking::Pickable;
use bevy::ui::RelativeCursorPosition;
use bevy::window::CursorMoved;
use bevy::window::PrimaryWindow;

use super::bevy_theme::*;
use crate::colony::components::PendingConstructionActions;
use crate::colony::data::{parse_resource_type, BuildingDefinition, BuildingsData};
use crate::colony::types::{BuildingCategory, BuildingType};
use crate::economy::budget::calculate_colony_power_totals;
use crate::economy::ResourceType;
use crate::game_state::{ActiveMenu, GameMenu};
use crate::plugins::solar_system::CelestialBody;
use crate::plugins::solar_system_data::BodyType;
use crate::research::systems::ResearchState;
// v0.5.2 PR-A.4 follow-up: the canary now renders resource-cost
// rows with a real PNG icon (loaded via `AssetServer` and
// post-processed in `src/ui/resource_icons.rs`) tinted to the
// resource's category colour (see `bevy_theme::category_color`).
// The emoji fallback above is kept for the legacy egui code
// path — bevy_ui uses `ResourceCostRow` directly.
use super::resource_icons::{get_resource_icon_handle_bevy, ResourceIcons};

/// One row of a building's resource demand: the resource name as
/// it appears in `buildings.ron`, the per-unit amount (already
/// multiplied by the build quantity), and the parsed
/// `ResourceType` (used to look up the icon `Handle<Image>` and
/// the category tint). `resource` is `None` when the RON string
/// doesn't match a known variant (defensive — the canary falls
/// back to a tinted placeholder square + `TEXT_BODY` so a future
/// RON addition never panics).
///
/// v0.5.2 PR-A.4 follow-up: the canary emits these for every
/// cost entry and renders them as `[PNG icon | tinted amount]`.
/// The icon is the asset-server PNG from
/// `assets/textures/ui/resources/<name>.png`, post-processed
/// (white → transparent, dark → un-premultiplied alpha) and
/// tinted to the resource's category colour at render time
/// via `ImageNode::color`.
#[derive(Debug, Clone)]
pub struct ResourceCostRow {
    pub name: String,
    pub amount: f64,
    pub resource: Option<ResourceType>,
}

/// Compute the active colony's spare power in MW. Returns 0.0 if no
/// colony is selected or no `BuildingsData` is loaded. Used by the
/// Build card to color-code the Power effect line: green when the
/// batch demand fits inside the grid, red when it would push the
/// grid into deficit.
///
/// v0.5.2 PR-A.2: per user feedback 2026-08-02, the canary now shows
/// "per-building demand × multiplier = total vs spare" as the single
/// source of truth for power on the build card. The old design had
/// three independent power readouts (PWR top stat, "Power:" body
/// effect, and the workforce mislabeled as MW) which read as a
/// confusing stack of similar numbers.
fn compute_colony_spare_power_mw(
    ui_state: &ConstructionUiState,
    colonies: &Query<(Entity, &crate::colony::Colony)>,
    buildings_data: Option<&BuildingsData>,
) -> f64 {
    let Some(colony_entity) = ui_state.selected_colony else {
        return 0.0;
    };
    let Some(data) = buildings_data else {
        return 0.0;
    };
    let Ok((_, colony)) = colonies.get(colony_entity) else {
        return 0.0;
    };
    let totals = calculate_colony_power_totals(colony, Some(data));
    // produced_watts / consumed_watts are in W. Convert to MW for the
    // card display (the build definitions use MW for `power_demand_mw`
    // so the two scales line up).
    (totals.produced_watts - totals.consumed_watts) / 1_000_000.0
}

// ── Construction UI shared types (v0.5.2: moved out of
//    construction_panel.rs, which is being deleted as the canary
//    becomes the new main construction menu) ────────────────────────

/// Top-level sub-tab in the Construction menu.
///
/// v0.5.2: the legacy `Stockpiles` minimum-stockpile editor was
/// renamed to `Mining` — the dedicated mining grid replaces it
/// (production mgmt with [-] [+] buttons + reserve / accessibility
/// readouts per resource card).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConstructionTab {
    Overview,
    Buildings,
    #[default]
    Build,
    /// v0.5.2: dedicated mining grid (replaces the v0.5.x
    /// `Stockpiles` minimum-stockpile editor).  One compact card per
    /// per-resource base mine + AutoMine, grouped by resource group,
    /// with [−] [+] buttons for direct inventory edits and a live
    /// readout of count / production / reserve / accessibility.
    Mining,
}

impl ConstructionTab {
    /// All four variants in display order. Used by the canary's
    /// `tick_construction_body_visibility` system to know which body
    /// to show.
    pub const ALL: [ConstructionTab; 4] = [
        ConstructionTab::Overview,
        ConstructionTab::Buildings,
        ConstructionTab::Build,
        ConstructionTab::Mining,
    ];
}

/// Persistent UI state for the Construction menu. Lives across frames
/// so the player's last-selected tab, colony, and build multiplier
/// survive re-mounts.
///
/// v0.5.2: this is now owned by the bevy_ui canary. The legacy
/// egui `construction_panel.rs` referenced it via
/// `use crate::ui::construction_panel::ConstructionUiState`; the
/// import is gone (the canary defines the type directly).
#[derive(Resource, Debug, Clone)]
pub struct ConstructionUiState {
    /// Build multiplier: how many copies to queue at once
    pub build_multiplier: u32,
    /// Currently selected colony entity (None = auto-select first)
    pub selected_colony: Option<bevy::ecs::entity::Entity>,
    /// Selected top-level tab within the construction menu.
    pub selected_tab: ConstructionTab,
    /// Selected build-category tab within the Build view.
    /// v0.5.2: 8 categories with "All" at index 8. The Build
    /// tab opens with the "All" chip active (the player most often
    /// wants to scroll the full catalog first), so the default is
    /// 8 — keep this in lockstep with
    /// `ActiveChips::default().category` (also 8) so the visual
    /// chip and the filter logic agree on the first frame.
    pub selected_build_tab: usize,
    /// Functional-role filter (Food / Power / Industry / Research /
    /// Synergy Active). Overlays the 9-category tabs.
    pub selected_filter: BuildFilter,
    /// v0.5.2 Mining tab: build multiplier for the [−] [+] buttons
    /// on each mine card. Independent of the Build tab's
    /// `build_multiplier` because the chip sets can diverge. Default 1.
    pub mining_build_multiplier: u32,
    /// v0.5.2 Mining tab: which surface groups are currently
    /// collapsed. Persists across tab switches. Empty = all
    /// surface groups visible.
    pub mining_groups_collapsed: std::collections::HashSet<MiningGroupId>,
    /// v0.5.2 Mining tab: whether the entire orbital section is
    /// collapsed (it carries 25 cards; collapsed by default so
    /// the initial paint is dominated by surface mines).
    pub mining_orbital_collapsed: bool,
}

impl Default for ConstructionUiState {
    fn default() -> Self {
        Self {
            build_multiplier: 1,
            selected_colony: None,
            selected_tab: ConstructionTab::Build,
            // 8 = "All" chip; matches `ActiveChips::default().category`
            // so the visual chip and the filter agree on the first
            // frame. Was 0 (Infrastructure) which made the Build tab
            // show only the infrastructure category even though the
            // chip row highlighted "All".
            selected_build_tab: 8,
            selected_filter: BuildFilter::default(),
            mining_build_multiplier: 1,
            mining_groups_collapsed: std::collections::HashSet::new(),
            mining_orbital_collapsed: true, // collapsed by default
        }
    }
}

/// Identifier for one of the 8 surface mine groups in the Mining
/// tab. Used as a key into `ConstructionUiState::mining_groups_collapsed`
/// so collapse state persists per group.
///
/// The orbital section is a single collapsible
/// (`mining_orbital_collapsed` bool) — it has 25 cards spread
/// across 5 sub-groups, and per spec the sub-groups themselves
/// are non-collapsible (matches the legacy egui tab's behaviour).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MiningGroupId {
    Construction,
    Precious,
    Strategic,
    Fissile,
    Hydrocarbons,
    HeavyWater,
    /// `WaterProcessor` is a water extraction facility (atmospheric
    /// condenser / ice miner) — body-restricted to `[None]`
    /// atmospheres. Sits in its own group so it pairs with the
    /// orbital `AutoWaterProcessor` (the only AutoMine without a
    /// surface mine counterpart before this group was added).
    Water,
    Helium3,
}

/// Build-qty chip set for the Mining tab. Matches the canary's
/// existing Build tab (6 chips: 1, 5, 10, 25, 50, 100) per
/// user decision 2026-08-02. The legacy egui spec said 4 chips;
/// the user chose to unify with the Build tab.
pub const MINING_QTY_CHIPS: [u32; 6] = [1, 5, 10, 25, 50, 100];

/// Surface mine group definitions for the Mining tab. Each entry
/// is `(group_id, display_label, buildings)` in display order.
///
/// Source-of-truth: 25 entries — 24 base mines (one per
/// mineable resource, per v0.5.2's per-resource dedicated
/// design) + `WaterProcessor` (the only AutoMine counterpart
/// that wasn't a "mine" in the strict sense — it's an
/// atmospheric condenser / ice miner that pairs with the
/// orbital `AutoWaterProcessor`). The 24+1 layout mirrors
/// the 25-card orbital section so the player can see a
/// matched surface/orbital pair for every orbital AutoMine.
pub const MINING_GROUPS_SURFACE: &[(MiningGroupId, &str, &[BuildingType])] = &[
    (
        MiningGroupId::Construction,
        "Construction Materials",
        &[
            BuildingType::IronMine,
            BuildingType::CopperMine,
            BuildingType::AluminumMine,
            BuildingType::TitaniumMine,
            BuildingType::SilicatesMine,
            BuildingType::NickelMine,
            BuildingType::TungstenMine,
            BuildingType::CarbonMine,
            BuildingType::ChromiumMine,
            BuildingType::MagnesiumMine,
        ],
    ),
    (
        MiningGroupId::Precious,
        "Precious Metals",
        &[
            BuildingType::GoldMine,
            BuildingType::SilverMine,
            BuildingType::PlatinumMine,
        ],
    ),
    (
        MiningGroupId::Strategic,
        "Strategic Materials",
        &[
            BuildingType::RareEarthsMine,
            BuildingType::LithiumMine,
            BuildingType::SulfurMine,
            BuildingType::PhosphorusMine,
            BuildingType::CobaltMine,
            BuildingType::FluorineMine,
        ],
    ),
    (
        MiningGroupId::Fissile,
        "Fissile",
        &[BuildingType::UraniumMine, BuildingType::ThoriumMine],
    ),
    (
        MiningGroupId::Hydrocarbons,
        "Hydrocarbons",
        &[BuildingType::MethaneExtractor],
    ),
    (
        MiningGroupId::HeavyWater,
        "Heavy Water",
        &[BuildingType::DeuteriumExtractor],
    ),
    (
        MiningGroupId::Water,
        "Water (body: any atmosphere)",
        &[BuildingType::WaterProcessor],
    ),
    (
        MiningGroupId::Helium3,
        "Helium-3 (body: Moon, GasGiant, Asteroid)",
        &[BuildingType::He3Mine],
    ),
];

/// Orbital AutoMine sub-group definitions for the Mining tab.
/// Single collapsible, 5 non-collapsible sub-groups, 25 cards
/// total. Source-of-truth: 25 AutoMines from `parse_building_type`
/// (24 per the spec + `AutoTitaniumMine` which the spec omitted).
pub const MINING_GROUPS_ORBITAL: &[(&str, &[BuildingType])] = &[
    (
        "Orbital — Construction",
        &[
            BuildingType::AutoIronMine,
            BuildingType::AutoCopperMine,
            BuildingType::AutoAluminumMine,
            BuildingType::AutoTitaniumMine,
            BuildingType::AutoSilicatesMine,
            BuildingType::AutoNickelMine,
            BuildingType::AutoTungstenMine,
            BuildingType::AutoCarbonMine,
            BuildingType::AutoChromiumMine,
            BuildingType::AutoMagnesiumMine,
        ],
    ),
    (
        "Orbital — Precious",
        &[
            BuildingType::AutoGoldMine,
            BuildingType::AutoSilverMine,
            BuildingType::AutoPlatinumMine,
        ],
    ),
    (
        "Orbital — Strategic",
        &[
            BuildingType::AutoRareEarthsMine,
            BuildingType::AutoLithiumMine,
            BuildingType::AutoSulfurMine,
            BuildingType::AutoPhosphorusMine,
            BuildingType::AutoCobaltMine,
            BuildingType::AutoFluorineMine,
        ],
    ),
    (
        "Orbital — Fissile",
        &[BuildingType::AutoUraniumMine, BuildingType::AutoThoriumMine],
    ),
    (
        "Orbital — Hydrocarbons / Heavy water / He-3 / Water",
        &[
            BuildingType::AutoMethaneExtractor,
            BuildingType::AutoDeuteriumExtractor,
            BuildingType::AutoHe3Mine,
            BuildingType::AutoWaterProcessor,
        ],
    ),
];

/// Functional-role filter chip overlaid on top of the 9-category tabs
/// in the Build view. Lets the player pivot by what the building
/// *does* (feeds the colony, generates power, drives industry,
/// advances research) rather than by its internal category label.
///
/// v0.5.2: moved out of `construction_panel.rs`. The canary's
/// `ChipKind::Filter(BuildFilter)` is the only consumer in the
/// bevy_ui surface; the canary's `visible_cards(_filter: BuildFilter, ...)`
/// still receives it for future use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BuildFilter {
    #[default]
    All,
    Food,
    Power,
    Industry,
    Research,
    SynergyActive,
}

impl BuildFilter {
    /// All variants in display order. Used by the chip-row primitive
    /// in the canary (Phase C5 when the chip row lands).
    pub fn all() -> &'static [BuildFilter] {
        &[
            BuildFilter::All,
            BuildFilter::Food,
            BuildFilter::Power,
            BuildFilter::Industry,
            BuildFilter::Research,
            BuildFilter::SynergyActive,
        ]
    }
}

// ── Building icons (menu-style post-processed) ─────────────────────

/// Loaded + post-processed building icons keyed by `BuildingType`. The
/// icons are 4-byte RGBA PNGs sourced from `assets/textures/ui/buildings/`
/// — dark line art on a white background. The `process_building_icons`
/// system runs once per icon and converts the white pixels to
/// transparent + dark lines to white (premultiplied), matching the
/// runtime tint pattern used by [`crate::ui::icons::process_menu_icons`].
///
/// `None` value means the building has no icon asset (defensive; in
/// practice all 52 buildings have PNGs). The card-spawn code falls back
/// to a cyan-tinted placeholder square when the handle is `None`.
#[derive(Resource, Default)]
pub struct BuildingIcons {
    pub handles: std::collections::HashMap<BuildingType, Handle<Image>>,
    pub processed: std::collections::HashSet<BuildingType>,
}

/// Startup system: load all building icons from disk. Runs once at
/// startup; the assets are small (a few KB each) so the load is fast.
pub fn load_building_icons(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    buildings_data: Option<Res<BuildingsData>>,
) {
    let Some(buildings_data) = buildings_data else {
        // No data yet — defer; the second startup pass will catch it.
        return;
    };
    let mut map = std::collections::HashMap::new();
    for (building_type, def) in buildings_data.definitions.iter() {
        // The RON `icon` field is sometimes a multi-byte emoji glyph
        // (e.g. "🌬") instead of a real asset path. Passing those
        // strings to `asset_server.load` produces a wall of "Path not
        // found" errors on startup. Skip any entry whose `icon` does
        // not look like a path (no `/` separator and no `.png` suffix)
        // — the card spawn code falls back to a cyan placeholder
        // square when the handle is missing, so the cards still
        // render correctly. Once `apply_building_icons.py` is re-run
        // on the data file this filter becomes a no-op for the real
        // entries.
        let looks_like_path = def.icon.contains('/') || def.icon.ends_with(".png");
        if !looks_like_path {
            continue;
        }
        let handle: Handle<Image> = asset_server.load(&def.icon);
        map.insert(*building_type, handle);
    }
    commands.insert_resource(BuildingIcons {
        handles: map,
        processed: Default::default(),
    });
}

/// Post-process building icons: white background → transparent, dark
/// lines → white (premultiplied). Same recipe as
/// `crate::ui::icons::process_menu_icons`. Runs every frame until
/// every icon has been processed once; the per-icon flag in `processed`
/// makes it a no-op after the first pass.
pub fn process_building_icons(
    mut icons: ResMut<BuildingIcons>,
    mut images: ResMut<Assets<Image>>,
) {
    let to_process: Vec<(BuildingType, Handle<Image>)> = icons
        .handles
        .iter()
        .filter(|(bt, _)| !icons.processed.contains(*bt))
        .map(|(bt, h)| (*bt, h.clone()))
        .collect();

    for (building_type, handle) in to_process {
        if let Some(image) = images.get_mut(&handle) {
            let bytes_per_pixel = 4usize;
            let expected = (image.texture_descriptor.size.width as usize)
                .saturating_mul(image.texture_descriptor.size.height as usize)
                .saturating_mul(bytes_per_pixel);
            if image.data.as_ref().map(|d| d.len()).unwrap_or(0) != expected {
                // Image not loaded yet (data None or size mismatch).
                // Skip and try again next frame — DO NOT mark as
                // processed, because the previous code path that
                // did so silently dropped icons that decoded to
                // the wrong pixel format (e.g. RGB instead of RGBA).
                // Re-trying each frame is cheap (one map lookup)
                // and self-heals once bevy finishes loading the
                // PNG.
                continue;
            }

            // Same luminance-key recipe as the menu icons:
            //   alpha = (1.0 - luminance).powf(3.0)
            //   premultiplied RGB = alpha (since base color is white)
            for chunk in image
                .data
                .as_mut()
                .unwrap()
                .chunks_exact_mut(bytes_per_pixel)
            {
                let r = chunk[0] as f32 / 255.0;
                let g = chunk[1] as f32 / 255.0;
                let b = chunk[2] as f32 / 255.0;
                let luminance = 0.299 * r + 0.587 * g + 0.114 * b;
                let alpha = (1.0_f32 - luminance).powf(3.0);
                let a = alpha.clamp(0.0, 1.0);
                let pa = (a * 255.0) as u8;
                chunk[0] = pa;
                chunk[1] = pa;
                chunk[2] = pa;
                chunk[3] = pa;
            }
            icons.processed.insert(building_type);
        }
    }
}


// ── Construction Queue (canary placeholder) ──────────────────────

/// One item in the construction queue.
#[derive(Debug, Clone)]
pub struct QueuedBuild {
    pub name: String,
    pub qty: u32,
    pub bp_per_unit: f64,
}

/// Resource: the currently active chip index for each row.
///
/// This is the single source of truth for visual active state. The
/// `tick_chip_button_active_overlay` system reads this resource each
/// frame and applies ACTIVE_CHIP_BG to the matching chip +
/// INACTIVE_CHIP_BG to the rest. We store the active index per row
/// instead of using a ChipActive marker because marker add/remove
/// ordering with Commands is fragile.
#[derive(Resource, Debug, Clone)]
pub struct ActiveChips {
    /// Active sub-tab index (0=Overview, 1=Buildings, 2=Build, 3=Mining)
    pub tab: usize,
    /// Active build qty multiplier (Build tab)
    pub qty: u32,
    /// Active filter/category index (0..8 = category, 9 = All).
    /// v0.5.2: 8 categories → 9 — Mining was split out of Industry.
    pub category: usize,
    /// Active mining qty multiplier (Mining tab).
    /// v0.5.2 PR-A.2: separate from `qty` so the Build tab's
    /// qty and the Mining tab's qty don't cross-pollute.
    pub mining_qty: u32,
}

impl Default for ActiveChips {
    fn default() -> Self {
        Self {
            tab: 2,        // Build tab is default
            qty: 1,        // x1 is default
            category: 8,   // "All" is default (was 9 before Mining chip was removed)
            mining_qty: 1, // x1 is default for the Mining tab
        }
    }
}

/// Resource: the construction queue + current output rate.
///
/// In Phase C4 this will be replaced with the real queue (driven by
/// `process_construction_actions`); for now it's static placeholder data
/// so the canary can show ETA + queue length without a real queue system.
#[derive(Resource, Debug, Clone)]
pub struct ConstructionQueue {
    /// BP per year the colony currently produces (output rate).
    pub output_bp_per_year: f64,
    /// Items currently in the queue (FIFO).
    pub items: Vec<QueuedBuild>,
}

impl Default for ConstructionQueue {
    fn default() -> Self {
        Self {
            // Same value the static placeholder uses; the real value
            // comes from the active colony's build output.
            output_bp_per_year: 12001.0,
            // 1 item queued: Housing Complex × 1. Gives a non-zero
            // queue length so the ETA + summary show real values.
            items: vec![QueuedBuild {
                name: "Housing Complex".to_string(),
                qty: 1,
                bp_per_unit: 200.0,
            }],
        }
    }
}

/// Total seconds remaining in the queue (sum of each item's build time).
/// Returns 0.0 when the queue is empty.
pub fn queue_remaining_seconds(queue: &ConstructionQueue) -> f64 {
    let mut total = 0.0;
    for item in &queue.items {
        // BP per year → seconds per BP. If output is 0, treat as 1 BP/yr
        // to avoid division by zero (an idle build pipeline is a separate
        // concern handled elsewhere).
        let bp_per_sec = (queue.output_bp_per_year / 365.25 / 24.0 / 3600.0).max(1e-9);
        total += (item.bp_per_unit * item.qty as f64) / bp_per_sec;
    }
    total
}

/// ETA for a single card: queue_remaining + own_build_time.
pub fn card_eta_seconds(queue: &ConstructionQueue, card_bp: f64) -> f64 {
    queue_remaining_seconds(queue)
        + (card_bp / (queue.output_bp_per_year / 365.25 / 24.0 / 3600.0).max(1e-9))
}

/// Resource that flags the Construction canary as visible.
#[derive(Debug, Default, Resource, PartialEq, Eq, Clone, Copy)]
pub enum ConstructionState {
    #[default]
    Off,
    On,
}

/// Resource: whether the queue panel is open. Toggled by the AppBar
/// `QUEUE` chip. Lives on a separate resource (not on `ConstructionUiState`)
/// so the egui reference panel doesn't see the expansion and so future
/// queue-panel-only state (e.g. scroll position) can land here too.
#[derive(Debug, Default, Resource, Clone, Copy)]
pub struct QueuePanelState {
    pub open: bool,
}

/// Resource: whether the Active Colony dropdown menu is open. Toggled by
/// clicking the picker. When `open`, the floating dropdown menu shows
/// every existing colony and selecting one updates
/// `ConstructionUiState::selected_colony` plus closes the menu.
#[derive(Debug, Default, Resource, Clone, Copy)]
pub struct ColonyDropdownState {
    pub open: bool,
}

/// Resource: hover-driven tooltip state for the construction canary.
///
/// When the player hovers over a disabled Queue CTA (one they can't
/// afford at the current multiplier), `tick_construction_tooltip`
/// populates `text` with a short reason and flips `visible` to true.
/// The `update_tooltip_text` system mirrors that to the on-screen
/// tooltip Text node each frame so the player sees *why* the button
/// is greyed out.
#[derive(Debug, Default, Resource)]
pub struct ConstructionTooltipState {
    pub text: String,
    pub visible: bool,
}
#[allow(dead_code)]

/// Marker component for the canary root container.
#[derive(Component)]
pub struct ConstructionRoot;

/// Marker component for the AppBar title text.
#[derive(Component)]
pub struct ConstructionTitle;

/// Marker component for the AppBar subtitle text.
#[derive(Component)]
pub struct ConstructionSubtitle;

/// Marker component for each build card. Holds the building name.
#[derive(Component)]
pub struct ConstructionCard {
    pub name: String,
}

/// Marker component for the Queue CTA. Carries the `BuildingType` this
/// card represents, so the click handler knows which building to enqueue.
#[derive(Component)]
pub struct ConstructionCta {
    pub building_type: BuildingType,
}

/// Marker component for Queue CTAs that should be **disabled** (visible
/// but inactive). The player can't afford N copies of the building at
/// the current multiplier. The `tick_construction_cta_disabled` system
/// inserts this marker after each refresh, and the click handler skips
/// the push when the marker is present.
#[derive(Component)]
pub struct ConstructionCtaDisabled;

/// Marker component for the AppBar "OPEN QUEUE" toggle chip. The
/// `tick_open_queue_chip_click` system reads this marker to know
/// which chip's `Interaction::Pressed` should toggle `QueuePanelState`.
#[derive(Component)]
pub struct OpenQueueChip;

/// Marker component for the "Active Colony" picker. The
/// `tick_colony_picker_click` system reads this marker to toggle
/// `ColonyDropdownState::open` when the player clicks the picker.
/// Distinct from `OpenQueueChip` so the click handlers can dispatch
/// independently.
#[derive(Component)]
pub struct ColonyPicker;

/// Marker component for the floating Active Colony dropdown menu
/// (the list of colonies that appears below the picker when it's
/// clicked). The `tick_colony_dropdown_visibility` system shows /
/// hides this container based on `ColonyDropdownState::open`.
#[derive(Component)]
pub struct ColonyDropdownMenu;

/// Marker component for a single option row inside the colony dropdown
/// menu. Carries the `Entity` of the `Colony` it represents so the
/// `tick_colony_option_click` system can update
/// `ConstructionUiState::selected_colony` when an option is clicked.
#[derive(Component)]
pub struct ColonyDropdownOption {
    pub colony_entity: bevy::ecs::entity::Entity,
}

/// Marker component on the colony value text inside the picker. The
/// `update_colony_picker_text` system writes the active colony's name
/// here every frame so the label always reflects the current selection.
#[derive(Component)]
pub struct ColonyPickerText;

/// Marker component on each row of the colony dropdown menu that holds
/// the colony name text. The `refresh_colony_dropdown` system uses
/// these to know which rows to keep vs. despawn when the list of
/// colonies changes.
#[derive(Component)]
pub struct ColonyDropdownOptionText;

/// Marker on the canary's hover tooltip text. The
/// `update_construction_tooltip` system reads `ConstructionTooltipState`
/// every frame and writes the text + toggles visibility.
#[derive(Component)]
pub struct ConstructionTooltipText;

/// Marker component for the marquee track that wraps an overflowing
/// build-card subtitle. The track holds two copies of the subtitle
/// back-to-back (no gap — a gap would create a visible "blank" beat
/// when copy A scrolls out and copy B scrolls in) and is animated via
/// `UiTransform.translation` by `tick_subtitle_marquee`.
///
/// `card`, `text_node`, and `clip_container` are pre-resolved entity
/// handles stored at spawn time so the tick system can do direct
/// `Query::get` lookups instead of walking `Parent` / `Children`
/// chains every frame. All three are required; missing entities
/// (the card was despawned mid-tick) just keep the marquee dormant
/// so the engine can clean it up.
///
/// `text_width` and `container_width` are the most recent
/// `ComputedNode`-measured values (pixels) used to decide whether
/// the description actually overflows. When `text_width <=
/// container_width` the marquee is dormant and the track sits at
/// translation `(0, 0)` — the text fits naturally and we leave it
/// alone.
///
/// `phase` is a `0.0..=1.0` oscillation parameter driven by hover
/// state. It increments while the parent card is hovered and the
/// text overflows, decrements otherwise. The track's `translation.x`
/// is set to `-phase * text_width` each frame, so `phase = 0`
/// shows copy A in the original position, `phase = 1.0` shows copy
/// B exactly where copy A started — perfect for a seamless loop on
/// release. We deliberately oscillate rather than snap-back: the
/// snap from `phase = 1.0` to `0.0` on hover-end would jerk the
/// viewer's eye away from the description mid-read.
#[derive(Component)]
pub struct SubtitleMarquee {
    /// Parent build-card entity, used by `tick_subtitle_marquee`
    /// to read the card's `Interaction` each frame. Stored
    /// explicitly so the system does one `Query::get` instead of
    /// walking `Parent` chains.
    pub card: Entity,
    /// Entity of the first text copy, used to measure
    /// `ComputedNode::content_size` for the per-frame overflow
    /// check. The second copy shares the same string + font so we
    /// only need to read one.
    pub text_node: Entity,
    /// Entity of the outer clip container, used to measure
    /// `ComputedNode::size` (the visible horizontal extent the
    /// track scrolls within).
    pub clip_container: Entity,
    /// Most recent measured text content width in pixels. Updated
    /// each frame by `tick_subtitle_marquee` from `ComputedNode`.
    pub text_width: f32,
    /// Most recent measured clip-container width in pixels.
    pub container_width: f32,
    /// Oscillation phase `0.0..=1.0`. Increments on hover (while
    /// overflowing); decrements when not hovered or when the text
    /// fits. Capped at 1.0 and floored at 0.0.
    pub phase: f32,
}

/// Marker component for the QueuePanel root. The
/// `tick_queue_panel_visibility` system hides all but the active
/// state — there's only one QueuePanel so this is a singleton query.
#[derive(Component)]
pub struct QueuePanelRoot;

/// Marker component for a single row in the queue panel. The
/// `update_queue_panel` system spawns one of these per
/// `ConstructionProject` for the selected colony, and removes them
/// when the project is gone (cancel / complete). The mapping
/// `project_entity -> QueueRowEntity` lives in a `Local` storage
/// inside the system.
#[derive(Component)]
pub struct QueuePanelRow {
    pub project_entity: Entity,
}

/// Marker component for the cancel button on a queue row. Click handler
/// pushes the project entity to `PendingConstructionActions::cancel_construction`.
#[derive(Component)]
pub struct QueuePanelRowCancel {
    pub project_entity: Entity,
}

/// Marker component for the QueuePanel close button. The click handler
/// toggles `QueuePanelState::open` to `false`.
#[derive(Component)]
pub struct QueuePanelClose;

/// Marker component for the card grid container. The refresh system
/// queries for this to find the grid and re-parent new cards.
#[derive(Component)]
pub struct CardGrid;

/// Marker for the always-visible vertical scrollbar track pinned to
/// the right edge of the card grid. `tick_construction_scrollbar`
/// queries this to find the track and resize/reposition its thumb
/// based on the grid's `ScrollPosition` + content size.
#[derive(Component)]
pub struct CardGridScrollbarTrack;

/// Marker for the thumb (the draggable handle inside the track).
/// Driven each frame by `tick_construction_scrollbar`.
#[derive(Component)]
pub struct CardGridScrollbarThumb;

/// Effect-bullet tone (drives the color of the corresponding line).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectTone {
    Positive,
    Negative,
    Neutral,
    Cost,
    Throughput,
}

/// Build card data: name, subtitle, stats, effects, queue label.
#[derive(Debug, Clone)]
pub struct BuildCardData {
    pub name: String,
    pub subtitle: String,
    /// The actual `BuildingType` this card represents. The Queue button
    /// pushes `(selected_colony, building_type)` to
    /// `PendingConstructionActions::start_construction` so
    /// `process_construction_actions` can spawn the project.
    pub building_type: BuildingType,
    /// Path to the building's icon, relative to the `assets/` directory
    /// (e.g. `textures/ui/buildings/mine.png`). Sourced from
    /// `BuildingDefinition::icon` in `buildings.ron`. The canary loads
    /// this via `AssetServer::load` and renders it as the card header icon.
    pub icon: String,
    /// The player's chosen build multiplier. The card ETA is derived
    /// from `build_points * multiplier` so the player can see the full
    /// batch ETA at a glance. The Queue button also pushes this many
    /// copies to `PendingConstructionActions`.
    pub multiplier: u32,
    pub stat_a: (&'static str, String),
    pub stat_b: (&'static str, String),
    /// Build points for one unit. The ETA row derives from
    /// `build_points * multiplier` divided by the static
    /// placeholder output. v0.5.2: added so the Mining card can
    /// show ETA (the Mining card's `stat_a` carries the live
    /// inventory count, not BP — without this separate field the
    /// ETA calculation would parse the count as 0 BP and show
    /// "0s" regardless of multiplier).
    pub build_points: f64,
    /// stat_c is unused (kept for struct stability) — the power
    /// readout moved to the body's first effect line.
    pub stat_c: (&'static str, String),
    pub effects: Vec<(EffectTone, String)>,
    /// Rich resource-cost rows: each entry is rendered as a
    /// `[PNG icon | tinted amount]` row in the card body, so
    /// the player can identify the resource at a glance and group
    /// related costs (Construction metals vs Volatiles vs
    /// Precious metals …) by hue. The icon is the asset-server
    /// PNG from `assets/textures/ui/resources/<name>.png`,
    /// post-processed (white → transparent, dark →
    /// un-premultiplied alpha) and tinted to the resource's
    /// category colour at render time via
    /// `ImageNode::color` (`bevy_theme::category_color_for_resource`).
    ///
    /// v0.5.2 PR-A.4 follow-up: supersedes the emoji-prefixed
    /// cost bullets that previously lived in `effects` with
    /// `EffectTone::Cost`. Cost entries are no longer pushed to
    /// `effects`; the canary renders the rows in this vec instead.
    pub resource_costs: Vec<ResourceCostRow>,
    /// The label on the Queue button. v0.5.2: dynamic per
    /// multiplier so the Mining card reads "Build +5" instead of
    /// just "Queue" — gives the player a quick read of how many
    /// copies one click will enqueue. Build cards keep the
    /// simpler "Queue" label (the player has 6 fixed chips to
    /// pick from, the chip itself shows the value).
    pub queue_label: String,
    /// `true` if the batch's total power demand exceeds the active
    /// colony's grid surplus. The Queue button reads this and adds
    /// `ConstructionCtaDisabled` so the player can't push a build
    /// the grid can't power; the tooltip system reads it to show
    /// "not enough energy".
    ///
    /// `false` when the building doesn't draw grid power, when
    /// no colony is selected (menu-screen previews), or when the
    /// batch fits inside the grid.
    pub power_insufficient: bool,
}

/// Build a `BuildCardData` from a `BuildingDefinition`. This is the
/// single conversion function used by the canary — all building
/// cards are derived from real `BuildingsData`, no hard-coding.
pub fn card_data_from_definition(
    building_type: BuildingType,
    def: &BuildingDefinition,
) -> BuildCardData {
    card_data_with_multiplier(building_type, def, 1, 0.0)
}

/// Build a `BuildCardData` with the player's chosen build multiplier
/// factored into the cost/ETA display. The `multiplier` parameter scales
/// the resource costs and workforce in place — no extra "Total ×N" line
/// is appended. Per user feedback 2026-08-02: when the player picks
/// "x25", every existing effect bullet reflects the batched amount
/// (e.g. "Iron 250k/t" instead of "Iron 10k/t" plus a separate
/// "Total ×25" line).
///
/// `spare_power_mw` is the active colony's grid surplus (produced
/// minus consumed, in MW). The Power effect line uses it to color-
/// code insufficient batches (red) vs fitting ones (green) so the
/// player sees at a glance whether the multiplier will push the grid
/// into deficit. Pass 0.0 if no colony is active (e.g. menu-screen
/// previews) — the line still reads cleanly.
pub fn card_data_with_multiplier(
    building_type: BuildingType,
    def: &BuildingDefinition,
    multiplier: u32,
    spare_power_mw: f64,
) -> BuildCardData {
    let mult = multiplier.max(1) as f64;

    // Stats row: BP + workforce. Per user feedback 2026-08-02, the
    // old 3-stat layout (BP / COST / PWR) duplicated the body's
    // "Power:" effect line and created a confusing "three power
    // numbers" stack. v0.5.2 PR-A.2 collapses this to two stats
    // (BP | COST) and lets the body line be the single source of
    // truth for power (with ×multiplier + vs-spare breakdown).
    let unit_bp = def.build_points;
    let batch_bp = unit_bp * mult;
    let bp = if mult > 1.0 {
        format!("{:.0} BP (×{})", batch_bp, mult as u32)
    } else {
        format!("{:.0} BP", unit_bp)
    };
    // Workforce (people, not MW). v0.5.2 PR-A.2: the canary was
    // showing workforce with a `MW` unit, which collided visually
    // with the power-demand stat and read as "6000 MW of power"
    // (per user feedback 2026-08-02). Displaying the actual unit
    // (`workers`) makes it clear this is a staffing cost, not
    // a power draw. The real cost in MC comes from `resource_costs`
    // at queue time — the canary displays workforce as a proxy.
    let unit_workforce = def.workforce;
    let batch_workforce = unit_workforce as f64 * mult;
    let cost = if mult > 1.0 {
        format!("{:.0} workers (×{})", batch_workforce, mult as u32)
    } else {
        format!("{} workers", unit_workforce)
    };

    // Effects: 1-3 lines from the definition's resource_costs +
    // maintenance_resources, plus a Production effect for any
    // `*Production` modifier (v0.5.2 — previously the canary's
    // effect bullets only showed costs, which made Silver Mine /
    // Rare Earths Mine / etc. read as "produces Iron, Copper,
    // Thorium" because those costs dominated the visible text).
    // The multiplier is folded into the per-line amount so the line
    // count stays the same regardless of batch size (per user
    // feedback 2026-08-02: an extra "Total ×N" line was pushing the
    // card body past its 240 px height).
    //
    // v0.5.2 PR-A.4 (cost-icon strip): the cost bullets now lead
    // with the resource emoji (e.g. "🔩 250k/t" instead of
    // "Iron 250k/t"). The resource-name column is dropped because
    // the icon already conveys the resource — saves ~28 px of
    // vertical space on a 4-line cost bullet (avoids the screenshot
    // bug where the longest cards had their content overflow the
    // 244 px card height and clip the ETA row). The cap is raised
    // from 3 → 8 so buildings with more than 3 distinct costs
    // (rare but defined in buildings.ron) display all of them; the
    // card's new 320 px height has room for up to 6 effect lines +
    // header + subtitle + stats + ETA without clipping.
    // v0.5.2 PR-A.4 follow-up: typed resource-demand rows. The
    // canary renders these as `[PNG icon | tinted amount]` —
    // see `spawn_card`'s resource-cost loop. Emoji/icon
    // rendering happens at spawn time; the builder only emits
    // data. Cap at 8 lines (rare but defined in buildings.ron —
    // e.g. Refinery has 4 costs, ChemicalPlant has 5,
    // SemiconductorFab has 6) so even tall cards don't overflow
    // the 320 px height.
    let mut effects: Vec<(EffectTone, String)> = Vec::new();
    let mut resource_costs: Vec<ResourceCostRow> = Vec::new();
    for (name, amt) in def.resource_costs.iter().take(8) {
        let total = amt * mult;
        resource_costs.push(ResourceCostRow {
            name: name.clone(),
            amount: total,
            resource: parse_resource_type(name),
        });
    }
    // v0.5.2 PR-A.2 power display (round 2, 2026-08-02): the
    // player's screenshot showed the body line carrying
    // "(vs 581798080 MW spare ✓)" which read as a giant number
    // without context. Drop the "vs X spare" suffix from the line
    // itself; instead, set `power_insufficient` on the card data so
    // the Queue button can disable itself (with a "not enough
    // energy" tooltip) when the batch would push the grid into
    // deficit. The line is now just the demand:
    //
    //   "Power: 150 MW × 6 = 900 MW"   (mult=6)
    //   "Power: 150 MW"               (mult=1)
    //   "Power: 0 MW"                 (no grid draw)
    //
    // v0.5.2 PR-A.5 (2026-08-02): power plants (Wind Farm, Coal
    // Power Sector, Fission Reactor, Hydroelectric Dam, …) store
    // their output on a `PowerGeneration` modifier (in GW per unit
    // — see `src/economy/budget.rs::calculate_colony_power_totals`
    // for the canonical reader). They have `power_demand_mw == 0.0`
    // because they are net producers, not consumers. The old card
    // only looked at `power_demand_mw`, so every power plant showed
    // the misleading "Power: 0 MW" neutral line and the player had
    // no idea what the plant actually generated. Now we surface the
    // generation as a green "Produces X MW" line that scales with
    // the build-qty multiplier (consistent with the demand line's
    // ×N expansion for batching). When the building has BOTH a
    // generation modifier AND a non-zero demand, the demand line
    // still appears as the second power-related effect (rare in
    // practice — most producers have a tiny parasitic draw which is
    // folded into the modifier or omitted).
    let power_output_gw_per_unit: f64 = def
        .modifiers
        .iter()
        .filter(|m| m.modifier_type == "PowerGeneration")
        .map(|m| m.value)
        .sum();
    if power_output_gw_per_unit > 0.0 {
        // RON is GW per unit; convert to MW (× 1000) for the card
        // line so it lines up with the demand line's MW units.
        let per_unit_mw = power_output_gw_per_unit * 1_000.0;
        let total_mw = per_unit_mw * mult;
        let line = if mult > 1.0 {
            format!(
                "Produces {:.0} MW \u{00d7} {} = {:.0} MW",
                per_unit_mw, mult as u32, total_mw
            )
        } else {
            format!("Produces {:.0} MW", per_unit_mw)
        };
        effects.insert(0, (EffectTone::Positive, line));
    } else if def.power_demand_mw.abs() < 0.01 {
        effects.insert(0, (EffectTone::Neutral, "Power: 0 MW".to_string()));
    } else {
        let per_unit = def.power_demand_mw;
        let total = per_unit * mult;
        let line = if mult > 1.0 {
            format!(
                "Power: {:.0} MW \u{00d7} {} = {:.0} MW",
                per_unit, mult as u32, total
            )
        } else {
            format!("Power: {:.0} MW", per_unit)
        };
        effects.insert(0, (EffectTone::Throughput, line));
    }
    // Compute `power_insufficient` so the Queue button can disable
    // itself. `false` when no colony is selected (spare=0 → no gate)
    // OR when the building doesn't draw power.
    let power_insufficient = if def.power_demand_mw.abs() < 0.01 {
        false
    } else if spare_power_mw <= 0.0 {
        false
    } else {
        def.power_demand_mw * mult > spare_power_mw
    };

    // v0.5.2 canary fix: surface the building's `*Production`
    // modifier (per-mine yield) as the most prominent non-Power
    // effect. Without this, the cost bullets (Iron, Copper, ...)
    // fill the visible card area and the actual produced resource
    // is buried in the description.
    //
    // Insertion rule: right after Power (if any) and before the
    // first cost. If there's no Power line, the Production line
    // is the very first effect.
    if let Some(prod) = def
        .modifiers
        .iter()
        .find(|m| m.modifier_type.ends_with("Production"))
    {
        if prod.value > 0.0 {
            if let Some(res_name) = prod.modifier_type.strip_suffix("Production") {
                // v0.5.2 fix (2026-08-02): per user feedback, the
                // "Produces" line did not scale with the build
                // multiplier — every other value on the card (BP,
                // workforce, resource costs, Power demand ×N) folds
                // the batch size into the displayed number, but
                // Produces showed the raw per-unit RON value, so a
                // ×6 build read as 1× (e.g. "9.00 Gt/yr Food/yr"
                // instead of "54.00 Gt/yr Food/yr"). Mirror the
                // Power-generation pattern immediately above:
                // show the per-unit rate, the multiplier, and the
                // batch total so the player sees the full impact of
                // the batch on the colony's food/feedstock output.
                let per_unit = prod.value;
                let total = per_unit * mult;
                let line = if mult > 1.0 {
                    format!(
                        "Produces {} {}/yr \u{00d7} {} = {} {}/yr",
                        format_mining_rate(per_unit),
                        res_name,
                        mult as u32,
                        format_mining_rate(total),
                        res_name
                    )
                } else {
                    format!("Produces {} {}/yr", format_mining_rate(per_unit), res_name)
                };
                let insert_at = if def.power_demand_mw.abs() >= 0.01 { 1 } else { 0 };
                effects.insert(insert_at, (EffectTone::Positive, line));
            }
        }
    }

    BuildCardData {
        name: def.display_name.clone(),
        subtitle: clamp_subtitle_two_lines(&def.description),
        building_type,
        icon: def.icon.clone(),
        multiplier: multiplier.max(1),
        stat_a: ("BP", bp),
        stat_b: ("COST", cost),
        // stat_c is unused (kept for struct stability) — the power
        // readout moved to the body's first effect line.
        stat_c: ("", String::new()),
        effects,
        // v0.5.2 PR-A.4 follow-up: typed resource-demand rows
        // rendered with PNG icon + category tint (see
        // `resource_costs` doc). Always passed alongside
        // `effects`; the canary renders the two sets in
        // separate visual zones (Power → Produces →
        // [resource_cost rows]).
        resource_costs,
        queue_label: "Queue".to_string(),
        power_insufficient,
        build_points: def.build_points,
    }
}

/// Clamp a description string to roughly two lines of caption-size text
/// (12 px) inside a card column of ~145 px width —— ~80 chars at the
/// prototype's character density. Appends an ellipsis when truncated so
/// the player knows the description continues. The 80-char budget keeps
/// every card subtitle visually consistent and prevents the effect
/// bullets below from being pushed off the card.
fn clamp_subtitle_two_lines(s: &str) -> String {
    const MAX_CHARS: usize = 80;
    if s.chars().count() <= MAX_CHARS {
        return s.to_string();
    }
    let mut out: String = s.chars().take(MAX_CHARS - 1).collect();
    out.push('…');
    out
}

/// Format a mining production rate (Mt/yr) with a human-readable
/// unit suffix. Mirrors the helper in `src/ui/construction_panel.rs`
/// (egui Mining tab) so the canary and the legacy panel agree on
/// the same scale labels.
///
/// | Range (Mt/yr)     | Suffix       | Example                     |
/// |-------------------|--------------|-----------------------------|
/// | < 1e-6            | "0"          | (effect suppressed upstream)|
/// | 1e-6 .. 1e-3      | g/yr         | "100 g/yr" Gold             |
/// | 1e-3 .. 1         | kg/yr        | "1.0 kg/yr" Platinum        |
/// | 1 .. 1e3          | Mt/yr        | "120.0 Mt/yr" Iron          |
/// | 1e3 .. 1e6        | Gt/yr        | "1.20 Gt/yr" Silicates      |
/// | ≥ 1e6             | Tt/yr        | "1.20 Tt/yr" (Carbon at scale) |
fn format_mining_rate(mt_per_year: f64) -> String {
    if mt_per_year.abs() < 1e-6 {
        return "0".to_string();
    }
    let v = mt_per_year.abs();
    if v < 1e-3 {
        // grams
        format!("{:.0} g/yr", mt_per_year * 1e9)
    } else if v < 1.0 {
        // kilograms
        format!("{:.1} kg/yr", mt_per_year * 1e6)
    } else if v < 1_000.0 {
        format!("{:.1} Mt/yr", mt_per_year)
    } else if v < 1_000_000.0 {
        format!("{:.2} Gt/yr", mt_per_year / 1_000.0)
    } else {
        format!("{:.2} Tt/yr", mt_per_year / 1_000_000.0)
    }
}

// ── Mining tab (v0.5.2 PR-A.2) ────────────────────────────────────
//
// One card per mine / AutoMine (24 base + 25 orbital = 49 total).
// Per-card [-] [+] buttons push to `PendingConstructionActions::
// mining_edits` (positive=add, negative=remove). The actual
// inventory edit is handled by `process_construction_actions`
// in `src/colony/systems.rs` — this file owns only the UI.

/// Per-card derived data for the Mining tab. Computed once per
/// card per frame; cheap (one `HashMap::get` per resource + a
/// modifier scan). The `count` lives on `Colony::buildings` and
/// is read directly in the spawn loop, not pre-computed here.
#[derive(Debug, Clone, Copy)]
pub struct MiningCardData {
    /// Per-build base yield in Mt/yr, from the building's
    /// `*Production` modifier. 0.0 if the building has no
    /// production modifier (e.g. He3Mine with no per-build yield
    /// listed; rare).
    pub base_yield_mt_per_year: f64,
    /// Per-resource deposit accessibility on the active body
    /// (0.0-1.0). 0.0 if no deposit for this resource on the body.
    pub accessibility: f32,
    /// Total reserves on the body in Mt (proven + deep + bulk
    /// rolled up). 0.0 if no deposit.
    pub reserve_mt: f64,
}

impl MiningCardData {
    /// Total per-year production for `count` builds of this card.
    /// Per-mine yield × count × body accessibility.
    pub fn production_mt_per_year(&self, count: u32) -> f64 {
        count as f64 * self.base_yield_mt_per_year * self.accessibility as f64
    }
}

/// Compute `MiningCardData` for `(building, deposits)`.
///
/// Strips `Production` from the first matching modifier to recover
/// the produced `ResourceType`, then reads the deposit's
/// `accessibility` and reserves from `PlanetResources::deposits`.
/// Returns zeroed data if either side is missing.
pub fn compute_mining_card_data(
    def: &BuildingDefinition,
    planet_resources: Option<&crate::economy::PlanetResources>,
) -> MiningCardData {
    // Find the building's `*Production` modifier.
    let produced_resource = def
        .modifiers
        .iter()
        .find(|m| m.modifier_type.ends_with("Production"))
        .and_then(|m| m.modifier_type.strip_suffix("Production"))
        .and_then(crate::colony::data::parse_resource_type);

    let Some(res_type) = produced_resource else {
        return MiningCardData {
            base_yield_mt_per_year: 0.0,
            accessibility: 0.0,
            reserve_mt: 0.0,
        };
    };

    let base_yield = def
        .modifiers
        .iter()
        .find(|m| m.modifier_type.ends_with("Production"))
        .map(|m| m.value)
        .unwrap_or(0.0);

    let Some(resources) = planet_resources else {
        return MiningCardData {
            base_yield_mt_per_year: base_yield,
            accessibility: 0.0,
            reserve_mt: 0.0,
        };
    };

    let Some(deposit) = resources.deposits.get(&res_type) else {
        return MiningCardData {
            base_yield_mt_per_year: base_yield,
            accessibility: 0.0,
            reserve_mt: 0.0,
        };
    };

    let total_reserve = deposit.reserve.proven_crustal
        + deposit.reserve.deep_deposits
        + deposit.reserve.planetary_bulk;

    MiningCardData {
        base_yield_mt_per_year: base_yield,
        accessibility: deposit.accessibility,
        reserve_mt: total_reserve,
    }
}

/// Format a total-reserve value with a scale suffix (kg/t/Mt/Gt/Tt).
/// Mirrors `format_mining_rate` for consistency. Used in the
/// "Res:" row on each Mining card.
pub fn format_mining_reserve(total_mt: f64) -> String {
    if total_mt.abs() < 1e-6 {
        return "0 Mt".to_string();
    }
    let v = total_mt.abs();
    if v < 1e-3 {
        format!("{:.1} kg", total_mt * 1e3)
    } else if v < 1.0 {
        format!("{:.1} t", total_mt * 1e3)
    } else if v < 1_000.0 {
        format!("{:.1} Mt", total_mt)
    } else if v < 1_000_000.0 {
        format!("{:.2} Gt", total_mt / 1_000.0)
    } else {
        format!("{:.2} Tt", total_mt / 1_000_000.0)
    }
}

/// Build the list of `BuildCardData` for the Build sub-tab, filtered by
/// the active category + `BuildFilter`, sorted by `build_points` ascending.
///
/// `category_index == 0` is the "Infrastructure" tab, `1` is "Industry", etc.
/// The last index (8 in the 8-category case) is the "Locked" tab — we
/// return the locked buildings instead of the unlocked ones.
///
/// `multiplier` is the player's chosen build-multiplier (1, 5, 10, 25,
/// 50, 100). It is passed through to `card_data_with_multiplier` so the
/// rendered card shows the batch cost / total BP for the whole batch.
///
/// `spare_power_mw` is the active colony's grid surplus (produced -
/// consumed, in MW). Forwarded to `card_data_with_multiplier` so each
/// card's Power effect line can show "demand vs spare" with a
/// green/red sufficient/insufficient marker. v0.5.2 PR-A.2.
///
/// `research_state` is used to decide which tech-gated buildings are
/// visible. The legacy version used `required_tech_opt().is_none()` which
/// hid every building with a tech requirement; the canary mirrors the
/// egui panel's behavior (only show buildings whose required tech is
/// absent or already unlocked).
pub fn visible_cards(
    data: &BuildingsData,
    research_state: &ResearchState,
    category_index: usize,
    _filter: BuildFilter,
    multiplier: u32,
    spare_power_mw: f64,
) -> Vec<(BuildingType, BuildCardData)> {
    let mut entries: Vec<(BuildingType, &BuildingDefinition)> = data
        .definitions
        .iter()
        .map(|(bt, def)| (*bt, def))
        .collect();

    // Sort by category, then by build_points ascending.
    // BuildingCategory doesn't derive Ord, so use a stable u8 rank.
    fn cat_rank(c: Option<BuildingCategory>) -> u8 {
        match c {
            Some(BuildingCategory::Infrastructure) => 0,
            Some(BuildingCategory::Mining) => 1,
            Some(BuildingCategory::Industry) => 2,
            Some(BuildingCategory::Logistics) => 3,
            Some(BuildingCategory::Power) => 4,
            Some(BuildingCategory::Population) => 5,
            Some(BuildingCategory::Research) => 6,
            Some(BuildingCategory::Financial) => 7,
            Some(BuildingCategory::Military) => 8,
            None => 9, // unknown / unparseable category — sort last
        }
    }
    entries.sort_by(|a, b| {
        cat_rank(parse_category(&a.1.category))
            .cmp(&cat_rank(parse_category(&b.1.category)))
            .then(
                a.1.build_points
                    .partial_cmp(&b.1.build_points)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
    });

    // Filter by category.
    //
    // Special case: if the index is 8 (the "All" chip in the filter row),
    // the player wants to see every building regardless of category. For
    // 0..8, the category chip narrows down to one `BuildingCategory`.
    // v0.5.2 PR-A.2 (round 2): the Mining chip is removed from the
    // Build tab, so 9 → 8 categories with "All" at index 8.
    //
    // v0.5.2 PR-A.2 (round 3): mining buildings (per-resource base
    // mines + AutoMines) are managed exclusively in the dedicated
    // `ConstructionTab::Mining` body, which renders the per-resource
    // grid with [-]/[+] buttons. Excluding `BuildingCategory::Mining`
    // here means mines no longer leak into the Build tab's Industry
    // chip or the "All" chip. The category enum still has the
    // `Mining` variant (the Mining tab's own `visible_cards`-style
    // helpers read it from the RON `category:` string) so the data
    // round-trip is preserved.
    let category = category_from_index(category_index);
    let in_category: Vec<_> = if category_index == 8 {
        // "All" chip: show every building EXCEPT mining (managed in
        // the Mining tab).
        entries
            .into_iter()
            .filter(|(_, def)| parse_category(&def.category) != Some(BuildingCategory::Mining))
            .collect()
    } else {
        entries
            .into_iter()
            .filter(|(_, def)| {
                if let Some(cat) = category {
                    // Mines never appear in the Build tab at all —
                    // they live in the Mining tab — so reject them
                    // even if a category chip somehow resolves to
                    // `Mining` (defensive: no chip currently maps to
                    // it, but `category_from_index` is the single
                    // source of truth).
                    if cat == BuildingCategory::Mining {
                        return false;
                    }
                    parse_category(&def.category) == Some(cat)
                } else {
                    // No category selected: show all non-mining.
                    parse_category(&def.category) != Some(BuildingCategory::Mining)
                }
            })
            .collect()
    };

    in_category
        .into_iter()
        .filter(|(_, def)| {
            // Tech filter: only show buildings whose required tech is
            // either absent or already unlocked. Without this fix every
            // tech-gated building would be hidden from the canary
            // (the legacy version used `is_none()` which prevented
            // anything with a required tech from showing).
            match def.required_tech_opt() {
                None => true,
                Some(tech_id) => research_state.is_unlocked(tech_id),
            }
        })
        .map(|(bt, def)| (bt, card_data_with_multiplier(bt, def, multiplier, spare_power_mw)))
        .collect()
}

/// Parse the data file's `category: String` into a `BuildingCategory`
/// enum. Returns `None` for unknown categories (defensive).
fn parse_category(s: &str) -> Option<BuildingCategory> {
    match s {
        "Infrastructure" => Some(BuildingCategory::Infrastructure),
        "Mining" => Some(BuildingCategory::Mining),
        "Industry" => Some(BuildingCategory::Industry),
        "Logistics" => Some(BuildingCategory::Logistics),
        "Power" => Some(BuildingCategory::Power),
        "Population" => Some(BuildingCategory::Population),
        "Research" => Some(BuildingCategory::Research),
        "Financial" => Some(BuildingCategory::Financial),
        "Military" => Some(BuildingCategory::Military),
        _ => None,
    }
}

/// Convert the `selected_build_tab: usize` into a `BuildingCategory`.
/// Index 8 maps to `None` (the "All" chip in the filter row).
/// Any other out-of-range index also maps to `None`.
///
/// v0.5.2 PR-A.2 (round 2): the Mining chip is removed from the
/// Build tab's category row. Mines are now managed exclusively
/// in the dedicated Mining tab; the Build tab's category
/// indices renumber to 0..7 with "All" at index 8. The internal
/// `BuildingCategory::Mining` variant is preserved (the
/// Mining tab still uses it; `parse_category` still maps the
/// RON `"Mining"` string to it) but it has no chip in the
/// Build tab UI.
fn category_from_index(idx: usize) -> Option<BuildingCategory> {
    match idx {
        0 => Some(BuildingCategory::Infrastructure),
        1 => Some(BuildingCategory::Industry),
        2 => Some(BuildingCategory::Logistics),
        3 => Some(BuildingCategory::Power),
        4 => Some(BuildingCategory::Population),
        5 => Some(BuildingCategory::Research),
        6 => Some(BuildingCategory::Financial),
        7 => Some(BuildingCategory::Military),
        8 => None, // "All" — bypass category filter
        _ => None,
    }
}

// ── Sub-tab body markers ──────────────────────────────────────────────

/// Marker component on the **root** of each sub-tab body. The canary
/// spawns one of these per sub-tab (Overview / Buildings / Build /
/// Stockpiles) at setup time, all as children of the `ConstructionRoot`.
/// The `tick_construction_body_visibility` system makes exactly one
/// visible (the one matching `ui_state.selected_tab`) every frame,
/// keeping the others hidden via `Visibility::Hidden`.
///
/// The bodies are siblings, not nested — each sub-tab body is its own
/// flex container at the same level as the card grid. The canary's
/// card grid (the Build tab body) carries `BuildBody`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstructionTabBody {
    Overview,
    Buildings,
    Build,
    /// v0.5.2: replaces v0.5.x `Stockpiles` (the minimum-stockpile
    /// editor) with the dedicated mining grid.
    Mining,
}

impl ConstructionTabBody {
    /// Map to the matching `ConstructionTab` enum.
    pub fn from_tab(tab: ConstructionTab) -> Self {
        match tab {
            ConstructionTab::Overview => Self::Overview,
            ConstructionTab::Buildings => Self::Buildings,
            ConstructionTab::Build => Self::Build,
            ConstructionTab::Mining => Self::Mining,
        }
    }

    /// Reverse mapping for the visibility system.
    pub fn tab(&self) -> ConstructionTab {
        match self {
            Self::Overview => ConstructionTab::Overview,
            Self::Buildings => ConstructionTab::Buildings,
            Self::Build => ConstructionTab::Build,
            Self::Mining => ConstructionTab::Mining,
        }
    }
}

/// System: make the body matching `ui_state.selected_tab` visible and
/// hide the others. Touches `Node::display` + `Visibility` so it runs
/// cheaply every frame — the bodies themselves are spawned once at
/// startup.
///
/// **Visibility inheritance**: Bevy 0.18's `Visibility::Visible` renders
/// the entity unconditionally (ignoring the parent's visibility). The
/// canary root toggles between `Hidden` and `Visible` based on whether
/// the Construction menu is open. If the active body used
/// `Visibility::Visible`, it would render on the main menu / other
/// menus too — the canary cards would show in front of the main menu.
/// Setting the active body to `Visibility::Inherited` makes its
/// visibility follow the root, so the cards only render when the
/// root is visible.
///
/// **Layout isolation**: `Visibility::Hidden` is a render-only flag —
/// hidden nodes still occupy their taffy-computed box and steal
/// `flex_grow: 1.0` siblings' share of the remaining height. With four
/// bodies each carrying `flex_grow: 1.0`, the active body was being
/// squeezed to ~25% of the remaining height and the card grid clipped
/// every card to its first ~15 px (the documented "single-pixel-tall
/// row" symptom). Toggling `Node::display` between `Display::Flex`
/// (active) and `Display::None` (inactive) removes the inactive
/// bodies from the layout entirely so the active body gets the full
/// remaining height.
pub fn tick_construction_body_visibility(
    ui_state: Res<ConstructionUiState>,
    mut body_query: Query<(&ConstructionTabBody, &mut Node, &mut Visibility)>,
) {
    let active = ConstructionTabBody::from_tab(ui_state.selected_tab);
    for (kind, mut node, mut visibility) in body_query.iter_mut() {
        if *kind == active {
            node.display = Display::Flex;
            *visibility = Visibility::Inherited;
        } else {
            node.display = Display::None;
            *visibility = Visibility::Hidden;
        }
    }
}

// ── Sub-tab body spawn helpers ────────────────────────────────────────

/// Build the **Overview** body. Read-only summary of the active colony:
/// name, population, BP/yr, queue count. The body is a single column
/// of label/value rows inside a `ConstructionTabBody::Overview` root.
fn spawn_overview_body(
    commands: &mut Commands,
    parent: Entity,
    body_font: &Handle<Font>,
    body_font_medium: &Handle<Font>,
    mono_font: &Handle<Font>,
) {
    let body = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                width: Val::Percent(100.0),
                flex_grow: 1.0,
                padding: UiRect::all(Val::Px(SPACE_LG)),
                row_gap: Val::Px(SPACE_SM),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
            ConstructionTabBody::Overview,
            Visibility::Hidden,
            Name::new("overview_body"),
        ))
        .id();
    commands.entity(parent).add_child(body);

    // Section header.
    let header = commands
        .spawn((
            Text::new("Colony Overview"),
            TextFont {
                font: body_font_medium.clone(),
                font_size: BODY_SIZE,
                ..default()
            },
            TextColor(CYAN),
            Name::new("overview_header"),
        ))
        .id();
    commands.entity(body).add_child(header);

    // Helper to spawn a label / value row with a marker on the value
    // text so the `update_overview_body` system can find each row by
    // its semantic role (colony name, population, etc.) and update
    // the text content every frame. The row is spawned once at startup;
    // only the value text changes.
    let mut row = |label: &str, marker: OverviewRowKind, initial: &str, color: Color| {
        let row_node = commands
            .spawn((
                Node {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(SPACE_MD),
                    padding: UiRect::vertical(Val::Px(SPACE_XS)),
                    width: Val::Percent(100.0),
                    ..default()
                },
                Name::new("overview_row"),
            ))
            .id();
        commands.entity(body).add_child(row_node);
        let l = commands
            .spawn((
                Text::new(label.to_string()),
                TextFont {
                    font: body_font.clone(),
                    font_size: BODY_SIZE,
                    ..default()
                },
                TextColor(TEXT_DIM),
                Node {
                    width: Val::Px(180.0),
                    ..default()
                },
                Name::new("overview_row_label"),
            ))
            .id();
        commands.entity(row_node).add_child(l);
        let v = commands
            .spawn((
                Text::new(initial.to_string()),
                TextFont {
                    font: mono_font.clone(),
                    font_size: BODY_SIZE,
                    ..default()
                },
                TextColor(color),
                Name::new("overview_row_value"),
                OverviewRowValue { kind: marker },
            ))
            .id();
        commands.entity(row_node).add_child(v);
    };

    // Spawn all four rows with placeholder text. The
    // `update_overview_body` system overwrites the text every frame
    // with the live colony data.
    row("Colony", OverviewRowKind::Colony, "(none)", ORANGE_ORE);
    row("Population", OverviewRowKind::Population, "—", TEXT_BODY);
    row(
        "Active Construction",
        OverviewRowKind::ActiveConstruction,
        "—",
        TEXT_BODY,
    );
    row(
        "Unique Building Types",
        OverviewRowKind::UniqueBuildingTypes,
        "—",
        TEXT_BODY,
    );

    // Queue section: live list of active construction projects for
    // the selected colony (each project's name + progress bar + ETA).
    // The `update_overview_queue` system re-spawns the rows every
    // frame based on the selected colony's `ConstructionProject`s.
    let queue_section_header = commands
        .spawn((
            Text::new("Construction Queue"),
            TextFont {
                font: body_font_medium.clone(),
                font_size: BODY_SIZE,
                ..default()
            },
            TextColor(CYAN),
            Node {
                margin: UiRect::top(Val::Px(SPACE_MD)),
                ..default()
            },
            Name::new("overview_queue_header"),
        ))
        .id();
    commands.entity(body).add_child(queue_section_header);

    let queue_content = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                width: Val::Percent(100.0),
                flex_grow: 1.0,
                // v0.5.2 PR-A.5: same flex-overflow fix as the
                // mining/buildings/queue bodies — without
                // `min_height: 0`, the column refuses to shrink
                // below its intrinsic content height and the
                // scroll wheel silently no-ops.
                min_height: Val::Px(0.0),
                row_gap: Val::Px(SPACE_XS),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
            Name::new("overview_queue_content"),
            OverviewQueueContent,
        ))
        .id();
    commands.entity(body).add_child(queue_content);

    // Short help line at the bottom.
    let help = commands
        .spawn((
            Text::new(
                "Tip: switch to the Build tab to queue new structures, or open the Queue panel from the AppBar to track progress.",
            ),
            TextFont {
                font: body_font.clone(),
                font_size: CAPTION_SIZE,
                ..default()
            },
            TextColor(TEXT_DIM),
            Node {
                margin: UiRect::top(Val::Px(SPACE_LG)),
                width: Val::Percent(100.0),
                ..default()
            },
            Name::new("overview_help"),
        ))
        .id();
    commands.entity(body).add_child(help);
}

/// Marker component on the value text of a single Overview row.
/// Carries the semantic role so the `update_overview_body` system
/// can find each row by its role (Colony / Population / etc.) and
/// update the text content every frame.
#[derive(Component)]
pub struct OverviewRowValue {
    pub kind: OverviewRowKind,
}

/// Identifies which semantic row the value text belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverviewRowKind {
    Colony,
    Population,
    ActiveConstruction,
    UniqueBuildingTypes,
}

/// Marker on the Overview body's queue content container. The
/// `update_overview_queue` system despawns the rows inside this
/// container and re-spawns them based on the selected colony's
/// `ConstructionProject`s every frame.
#[derive(Component)]
pub struct OverviewQueueContent;

/// Update the Overview body's queue section every frame.
///
/// Spawn-once-update-many (v0.5.2): rows persist across frames.
/// Each frame:
/// 1. Despawn rows whose project is no longer live (project
///    completed, cancelled, or its `colony_entity` no longer matches
///    the selected colony).
/// 2. Mutate the text + fill width on existing rows in place.
/// 3. Spawn rows for projects we haven't seen before.
///
/// The `Local<HashMap<Entity, OverviewQueueRow>>` cache is keyed by
/// the project entity and carries the row's child-node IDs so the
/// per-frame updates are direct `Query::get_mut` lookups — no
/// `Children` / `ChildOf` walks.
///
/// The "no active construction projects" placeholder is owned by a
/// separate `Local<Option<Entity>>` so it can be toggled without
/// coupling it to the project list.
pub fn update_overview_queue(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    ui_state: Res<ConstructionUiState>,
    buildings_data: Res<BuildingsData>,
    projects: Query<(Entity, &crate::colony::ConstructionProject)>,
    content_query: Query<Entity, With<OverviewQueueContent>>,
    mut spawned_rows: Local<std::collections::HashMap<Entity, OverviewQueueRow>>,
    // B0001 (Bevy 0.18): three separate `Query<&mut Text, ...>` parameters
    // would conflict because they all yield `&mut Text` access on the
    // same archetype. The `With<...>` filters are disjoint in practice
    // (each marker is set on a different child entity), but the planner
    // can't prove that statically. Wrap in `ParamSet` and use `p0()..p2()`
    // in non-overlapping scopes — see the `update_overview_queue` body
    // for the canonical readout-then-write pattern. Same fix shape is
    // used by `update_card_scrollbar_metrics` below.
    mut text_params: ParamSet<(
        Query<&mut Text, With<OverviewQueueRowNameChild>>,
        Query<&mut Text, With<OverviewQueueRowProgressChild>>,
        Query<(&mut Text, &mut TextColor), With<OverviewQueueRowStatusChild>>,
    )>,
    mut progress_fill_query: Query<&mut Node, With<OverviewQueueRowFillChild>>,
    mut empty_placeholder: Local<Option<Entity>>,
) {
    let Ok(content) = content_query.single() else { return; };

    let body_font: Handle<Font> = asset_server.load("fonts/Inter-Regular.otf");
    let body_font_medium: Handle<Font> = asset_server.load("fonts/Inter-SemiBold.otf");
    let mono_font: Handle<Font> = asset_server.load("fonts/GeistMono-Medium.ttf");

    // Resolve the selected colony's projects.
    let selected_colony = ui_state.selected_colony;
    let colony_projects: Vec<(Entity, crate::colony::ConstructionProject)> = selected_colony
        .map(|colony_entity| {
            projects
                .iter()
                .filter(|(_, p)| p.colony_entity == colony_entity)
                .map(|(e, p)| (e, p.clone()))
                .collect()
        })
        .unwrap_or_default();
    let live_keys: std::collections::HashSet<Entity> =
        colony_projects.iter().map(|(e, _)| *e).collect();

    // 1. Despawn rows whose project is gone.
    let to_remove: Vec<Entity> = spawned_rows
        .keys()
        .filter(|k| !live_keys.contains(k))
        .copied()
        .collect();
    for key in to_remove {
        if let Some(row_info) = spawned_rows.remove(&key) {
            // Cascade-despawn the row (which drops the header,
            // name, status, progress text, track, and fill
            // children). `try_despawn` keeps this silent if the
            // row was already cascade-despawned by an earlier
            // system in the same tick.
            commands.entity(row_info.row).try_despawn();
        }
    }

    // 2. Empty-queue placeholder: spawn once if needed, despawn
    //    once if the queue transitioned from empty to non-empty.
    if colony_projects.is_empty() {
        let need_spawn = match *empty_placeholder {
            Some(p) => commands.get_entity(p).is_err(),
            None => true,
        };
        if need_spawn {
            let placeholder = commands
                .spawn((
                    Text::new(
                        "No active construction projects. Switch to the Build tab to queue a building.",
                    ),
                    TextFont {
                        font: body_font.clone(),
                        font_size: BODY_SIZE,
                        ..default()
                    },
                    TextColor(TEXT_DIM),
                    Name::new("overview_queue_empty"),
                ))
                .id();
            commands.entity(content).add_child(placeholder);
            *empty_placeholder = Some(placeholder);
        }
        return;
    } else if let Some(placeholder) = empty_placeholder.take() {
        commands.entity(placeholder).try_despawn();
    }

    // 3. Mutate existing rows in place: text + fill width.
    for (project_entity, project) in &colony_projects {
        let Some(row) = spawned_rows.get(project_entity) else {
            continue;
        };
        let progress = project.progress_percent();
        let status = if project.awaiting_resources {
            "Awaiting delivery"
        } else {
            "Building"
        };
        let display_name = buildings_data
            .get(&project.building_type)
            .map(|d| d.display_name.as_str())
            .unwrap_or("(unknown)");
        // ParamSet readout-then-write: each accessor lives in its
        // own scoped block so the borrow on `text_params` is
        // released before the next one is taken. The same idiom
        // is used by `update_buildings_body` below.
        let new_text = display_name.to_string();
        {
            if let Ok(mut text) = text_params.p0().get_mut(row.name_text) {
                **text = new_text;
            }
        }
        let status_text = status.to_string();
        let status_color = if project.awaiting_resources {
            ORANGE_ORE
        } else {
            GREEN_FIN
        };
        {
            if let Ok((mut text, mut color)) = text_params.p2().get_mut(row.status_text) {
                **text = status_text;
                *color = TextColor(status_color);
            }
        }
        let progress_text = format!("{:.0}%", (progress as f64) * 100.0);
        {
            if let Ok(mut text) = text_params.p1().get_mut(row.progress_text) {
                **text = progress_text;
            }
        }
        if let Ok(mut node) = progress_fill_query.get_mut(row.progress_fill) {
            node.width = Val::Percent(progress.clamp(0.0, 1.0) * 100.0);
        }
    }

    // 4. Spawn rows for projects we haven't seen before.
    for (project_entity, project) in &colony_projects {
        if spawned_rows.contains_key(project_entity) {
            continue;
        }
        let display_name = buildings_data
            .get(&project.building_type)
            .map(|d| d.display_name.as_str())
            .unwrap_or("(unknown)");
        let progress = project.progress_percent();
        let status = if project.awaiting_resources {
            "Awaiting delivery"
        } else {
            "Building"
        };

        let row = commands
            .spawn((
                Node {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(Val::Px(SPACE_MD)),
                    row_gap: Val::Px(SPACE_XS),
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(4.0)),
                    width: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(CARD_BG),
                BorderColor::all(CYAN_BORDER),
                Name::new("overview_queue_row"),
            ))
            .id();
        commands.entity(content).add_child(row);

        // Header line: name + status + progress label.
        let header = commands
            .spawn((
                Node {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(SPACE_MD),
                    width: Val::Percent(100.0),
                    ..default()
                },
                Name::new("overview_queue_row_header"),
            ))
            .id();
        commands.entity(row).add_child(header);

        let name_text = commands
            .spawn((
                Text::new(display_name.to_string()),
                TextFont {
                    font: body_font_medium.clone(),
                    font_size: BODY_SIZE,
                    ..default()
                },
                TextColor(CYAN),
                Node {
                    flex_grow: 1.0,
                    ..default()
                },
                Name::new("overview_queue_row_name"),
                OverviewQueueRowNameChild,
            ))
            .id();
        commands.entity(header).add_child(name_text);

        let status_text = commands
            .spawn((
                Text::new(status.to_string()),
                TextFont {
                    font: body_font.clone(),
                    font_size: CAPTION_SIZE,
                    ..default()
                },
                TextColor(if project.awaiting_resources {
                    ORANGE_ORE
                } else {
                    GREEN_FIN
                }),
                Name::new("overview_queue_row_status"),
                OverviewQueueRowStatusChild,
            ))
            .id();
        commands.entity(header).add_child(status_text);

        let progress_text = commands
            .spawn((
                Text::new(format!("{:.0}%", (progress as f64) * 100.0)),
                TextFont {
                    font: mono_font.clone(),
                    font_size: CAPTION_SIZE,
                    ..default()
                },
                TextColor(CYAN),
                Name::new("overview_queue_row_progress"),
                OverviewQueueRowProgressChild,
            ))
            .id();
        commands.entity(header).add_child(progress_text);

        // Progress bar.
        let track = commands
            .spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(4.0),
                    border_radius: BorderRadius::all(Val::Px(2.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.196, 0.529, 0.612, 0.30)),
                Name::new("overview_queue_row_track"),
            ))
            .id();
        commands.entity(row).add_child(track);
        let progress_fill = commands
            .spawn((
                Node {
                    width: Val::Percent(progress.clamp(0.0, 1.0) * 100.0),
                    height: Val::Percent(100.0),
                    border_radius: BorderRadius::all(Val::Px(2.0)),
                    ..default()
                },
                BackgroundColor(CYAN),
                Name::new("overview_queue_row_fill"),
                OverviewQueueRowFillChild,
            ))
            .id();
        commands.entity(track).add_child(progress_fill);

        // Attach the marker with all child IDs.
        commands.entity(row).insert(OverviewQueueRow {
            project_entity: *project_entity,
            row,
            name_text,
            status_text,
            progress_text,
            progress_fill,
        });
        spawned_rows.insert(*project_entity, OverviewQueueRow {
            project_entity: *project_entity,
            row,
            name_text,
            status_text,
            progress_text,
            progress_fill,
        });
    }
}

/// Marker on each row in the Overview body's queue section. Carries
/// the project entity plus the child-node entity IDs so the
/// `update_overview_queue` system can mutate the progress label,
/// status text, and fill bar in place via direct `Query::get_mut`
/// lookups instead of walking `Children` / `ChildOf` chains.
///
/// Spawn-once-update-many refactor (v0.5.2): rows persist across
/// frames; the system diffs the live project set against the cached
/// map and only despawns rows whose project is gone, only spawns
/// rows for new projects.

/// Marker component on the `Text` node that holds the building
/// display name within a queued row. Used by `update_overview_queue`
/// to `Query::get_mut` the text in place each frame.
#[derive(Component)]
pub struct OverviewQueueRowNameChild;

/// Marker component on the `Text` node that holds the human-readable
/// status ("Building" / "Awaiting delivery") within a queued row.
#[derive(Component)]
pub struct OverviewQueueRowStatusChild;

/// Marker component on the `Text` node that holds the formatted
/// "{:.0}%" progress label within a queued row.
#[derive(Component)]
pub struct OverviewQueueRowProgressChild;

/// Marker component on the `Node` whose `width` encodes the
/// progress fill (0 % – 100 % of the track) within a queued row.
#[derive(Component)]
pub struct OverviewQueueRowFillChild;

#[derive(Component)]
pub struct OverviewQueueRow {
    pub project_entity: Entity,
    /// The row entity itself. Stored so the despawn step can drop
    /// the whole subtree in one `commands.entity(...).despawn()`
    /// call (which cascade-despawns the header + track + all their
    /// children).
    pub row: Entity,
    /// The `Text` node holding the building display name.
    pub name_text: Entity,
    /// The `Text` node holding the human-readable status
    /// ("Building" / "Awaiting delivery").
    pub status_text: Entity,
    /// The `Text` node holding the formatted "{:.0}%" progress label.
    pub progress_text: Entity,
    /// The `Node` overlay whose width encodes progress
    /// (0 % – 100 % of the track).
    pub progress_fill: Entity,
}

/// Update the Overview body's four value rows every frame so the
/// colony summary reflects the current `selected_colony` state. The
/// body is spawned once at startup with placeholder text; this
/// system overwrites the value text each frame with live data.
pub fn update_overview_body(
    ui_state: Res<ConstructionUiState>,
    colonies: Query<(Entity, &crate::colony::Colony)>,
    projects: Query<&crate::colony::ConstructionProject>,
    mut value_query: Query<(&OverviewRowValue, &mut Text, &mut TextColor)>,
) {
    // Resolve the selected colony + its project count.
    let selected_colony = ui_state.selected_colony;
    let colony_data = selected_colony.and_then(|e| {
        colonies.iter().find(|(ce, _)| *ce == e).map(|(_, c)| c.clone())
    });

    let project_count: u32 = selected_colony
        .map(|colony_entity| {
            projects
                .iter()
                .filter(|p| p.colony_entity == colony_entity)
                .count() as u32
        })
        .unwrap_or(0);

    for (marker, mut text, mut color) in value_query.iter_mut() {
        let (new_text, new_color) = match marker.kind {
            OverviewRowKind::Colony => match &colony_data {
                Some(c) => (c.name.clone(), CYAN),
                None => ("(no colony selected)".to_string(), ORANGE_ORE),
            },
            OverviewRowKind::Population => match &colony_data {
                Some(c) => (format!("{:.0}", c.population), TEXT_BODY),
                None => ("—".to_string(), TEXT_DIM),
            },
            OverviewRowKind::ActiveConstruction => {
                let c = if project_count == 0 { GREEN_OK } else { YELLOW_ETA };
                (format!("{}", project_count), c)
            }
            OverviewRowKind::UniqueBuildingTypes => match &colony_data {
                Some(c) => (format!("{}", c.buildings.len()), TEXT_BODY),
                None => ("—".to_string(), TEXT_DIM),
            },
        };
        **text = new_text;
        *color = TextColor(new_color);
    }
}

/// Build the **Buildings** body. A persistent container with a header
/// + a content scroll area. The `update_buildings_body` system
/// re-spawns the content rows every frame based on the selected
/// colony's `buildings` HashMap, so the list reflects the live state.
fn spawn_buildings_body(
    commands: &mut Commands,
    parent: Entity,
    body_font_medium: &Handle<Font>,
) {
    let body = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                width: Val::Percent(100.0),
                flex_grow: 1.0,
                padding: UiRect::all(Val::Px(SPACE_LG)),
                row_gap: Val::Px(SPACE_SM),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
            ConstructionTabBody::Buildings,
            Visibility::Hidden,
            Name::new("buildings_body"),
        ))
        .id();
    commands.entity(parent).add_child(body);

    // Header (text updated each frame by `update_buildings_body`).
    let header = commands
        .spawn((
            Text::new("Constructed Buildings"),
            TextFont {
                font: body_font_medium.clone(),
                font_size: BODY_SIZE,
                ..default()
            },
            TextColor(CYAN),
            Name::new("buildings_header"),
            BuildingsHeader,
        ))
        .id();
    commands.entity(body).add_child(header);

    // Content container — `update_buildings_body` despawns and re-spawns
    // the rows inside this container every frame.
    let content = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                width: Val::Percent(100.0),
                flex_grow: 1.0,
                // v0.5.2 PR-A.5: same `min_height: 0` fix as the
                // mining content container — without it, the flex
                // item refuses to shrink below its intrinsic content
                // height and `Overflow::scroll_y` never engages.
                // The build tab's `card_grid` has the same pattern
                // with the same line. Mirrors the documentation
                // comment on `mining_content` below.
                min_height: Val::Px(0.0),
                row_gap: Val::Px(SPACE_XS),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
            Name::new("buildings_content"),
            BuildingsContent,
        ))
        .id();
    commands.entity(body).add_child(content);
}

/// Marker on the Buildings body's header text so the update system
/// can update just the header without re-spawning it.
#[derive(Component)]
pub struct BuildingsHeader;

/// Marker on the Buildings body's content container. The
/// `update_buildings_body` system walks the children of this
/// container via `Children` and re-spawns them every frame.
#[derive(Component)]
pub struct BuildingsContent;

/// Update the Buildings body every frame.
///
/// Spawn-once-update-many (v0.5.2): rows persist across frames.
/// Each frame:
/// 1. Despawn rows whose `BuildingType` is no longer present in
///    the selected colony's `buildings` map.
/// 2. Mutate the count text on existing rows in place — the count
///    changes whenever the player queues or cancels construction.
/// 3. Spawn rows for `BuildingType`s we haven't seen before.
///
/// The `Local<HashMap<BuildingType, BuildingsRow>>` cache is keyed
/// by `BuildingType` and stores the row id + the quantity-text
/// child entity. The display name never changes for a given
/// `BuildingType` (it's metadata) so we don't mutate it — only the
/// `×{count}` text and the row existence itself.
pub fn update_buildings_body(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    ui_state: Res<ConstructionUiState>,
    buildings_data: Res<BuildingsData>,
    colonies: Query<(Entity, &crate::colony::Colony)>,
    content_query: Query<Entity, With<BuildingsContent>>,
    mut spawned_rows: Local<std::collections::HashMap<crate::colony::BuildingType, BuildingsRow>>,
    // B0001 (Bevy 0.18): two `Query<&mut Text, ...>` parameters both
    // yield `&mut Text` access on arbitrary archetypes. The
    // `With<…>` filters are disjoint in practice (different child
    // markers), but the planner can't prove that. Wrap in `ParamSet`
    // and use `.p0()` / `.p1()` in non-overlapping scopes — see the
    // sibling `update_overview_queue` system for the same fix shape.
    mut text_params: ParamSet<(
        Query<&mut Text, With<BuildingsHeader>>,
        Query<&mut Text, With<BuildingsRowQtyChild>>,
    )>,
    mut empty_placeholder: Local<Option<Entity>>,
    mut no_colony_placeholder: Local<Option<Entity>>,
) {
    let Ok(content) = content_query.single() else { return; };

    // Load fonts for the text nodes (cached by the asset server).
    let body_font: Handle<Font> = asset_server.load("fonts/Inter-Regular.otf");
    let mono_font: Handle<Font> = asset_server.load("fonts/GeistMono-Medium.ttf");

    // Resolve the selected colony.
    let colony = ui_state
        .selected_colony
        .and_then(|e| colonies.iter().find(|(ce, _)| *ce == e));

    // Update the header text. The ParamSet borrows drop at the
    // end of this block so the qty_text write below doesn't
    // overlap (B0001-safe).
    let header_text = match &colony {
        Some((_, c)) => format!("Constructed Buildings ({})", c.buildings.len()),
        None => "Constructed Buildings".to_string(),
    };
    for mut text in text_params.p0().iter_mut() {
        **text = header_text.clone();
    }

    // Helper: spawn the "(no colony selected)" placeholder if we
    // don't have one. `Local<Option<Entity>>` survives across
    // frames so we only spawn once per "no colony" stretch.
    fn spawn_no_colony_placeholder(
        commands: &mut Commands,
        content: Entity,
        body_font: Handle<Font>,
        existing: Option<Entity>,
    ) -> Option<Entity> {
        if let Some(p) = existing {
            if commands.get_entity(p).is_ok() {
                return Some(p);
            }
        }
        let placeholder = commands
            .spawn((
                Text::new("(no colony selected)"),
                TextFont {
                    font: body_font,
                    font_size: BODY_SIZE,
                    ..default()
                },
                TextColor(TEXT_DIM),
                Name::new("buildings_no_colony"),
            ))
            .id();
        commands.entity(content).add_child(placeholder);
        Some(placeholder)
    }

    fn spawn_empty_placeholder(
        commands: &mut Commands,
        content: Entity,
        body_font: Handle<Font>,
        existing: Option<Entity>,
    ) -> Option<Entity> {
        if let Some(p) = existing {
            if commands.get_entity(p).is_ok() {
                return Some(p);
            }
        }
        let placeholder = commands
            .spawn((
                Text::new("No buildings yet. Switch to the Build tab to queue your first structure."),
                TextFont {
                    font: body_font,
                    font_size: BODY_SIZE,
                    ..default()
                },
                TextColor(TEXT_DIM),
                Name::new("buildings_empty"),
            ))
            .id();
        commands.entity(content).add_child(placeholder);
        Some(placeholder)
    }

    let Some((_, colony)) = colony else {
        // Despawn any cached rows and placeholders.
        for (_, row_info) in spawned_rows.drain() {
            commands.entity(row_info.row).try_despawn();
        }
        if let Some(p) = no_colony_placeholder.take() {
            commands.entity(p).try_despawn();
        }
        if let Some(p) = empty_placeholder.take() {
            commands.entity(p).try_despawn();
        }
        *no_colony_placeholder = spawn_no_colony_placeholder(
            &mut commands,
            content,
            body_font.clone(),
            None,
        );
        return;
    };

    if colony.buildings.is_empty() {
        for (_, row_info) in spawned_rows.drain() {
            commands.entity(row_info.row).try_despawn();
        }
        if let Some(p) = no_colony_placeholder.take() {
            commands.entity(p).try_despawn();
        }
        if let Some(p) = empty_placeholder.take() {
            commands.entity(p).try_despawn();
        }
        *empty_placeholder = spawn_empty_placeholder(
            &mut commands,
            content,
            body_font.clone(),
            None,
        );
        return;
    }

    // We have rows to show — clear both placeholders.
    if let Some(p) = no_colony_placeholder.take() {
        commands.entity(p).try_despawn();
    }
    if let Some(p) = empty_placeholder.take() {
        commands.entity(p).try_despawn();
    }

    // Sort buildings by name for stable presentation.
    let mut entries: Vec<_> = colony.buildings.iter().collect();
    entries.sort_by(|a, b| {
        let an = buildings_data
            .get(a.0)
            .map(|d| d.display_name.as_str())
            .unwrap_or("");
        let bn = buildings_data
            .get(b.0)
            .map(|d| d.display_name.as_str())
            .unwrap_or("");
        an.cmp(bn)
    });

    let live_keys: std::collections::HashSet<crate::colony::BuildingType> =
        entries.iter().map(|(bt, _)| **bt).collect();

    // 1. Despawn rows whose BuildingType is gone.
    let to_remove: Vec<crate::colony::BuildingType> = spawned_rows
        .keys()
        .filter(|k| !live_keys.contains(k))
        .copied()
        .collect();
    for key in to_remove {
        if let Some(row_info) = spawned_rows.remove(&key) {
            commands.entity(row_info.row).try_despawn();
        }
    }

    // 2. Mutate existing rows in place: count text.
    for (building_type, count) in &entries {
        let Some(row_info) = spawned_rows.get(building_type) else {
            continue;
        };
        if let Ok(mut text) = text_params.p1().get_mut(row_info.qty_text) {
            **text = format!("\u{00d7}{}", *count);
        }
    }

    // 3. Spawn rows for BuildingTypes we haven't seen before.
    for (building_type, count) in &entries {
        if spawned_rows.contains_key(building_type) {
            continue;
        }
        let display_name = buildings_data
            .get(building_type)
            .map(|d| d.display_name.as_str())
            .unwrap_or("(unknown)");
        let row = commands
            .spawn((
                Node {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(SPACE_MD),
                    padding: UiRect::vertical(Val::Px(SPACE_XS)),
                    width: Val::Percent(100.0),
                    ..default()
                },
                Name::new("buildings_row"),
            ))
            .id();
        commands.entity(content).add_child(row);
        let name = commands
            .spawn((
                Text::new(display_name.to_string()),
                TextFont {
                    font: body_font.clone(),
                    font_size: BODY_SIZE,
                    ..default()
                },
                TextColor(TEXT_BODY),
                Node {
                    flex_grow: 1.0,
                    ..default()
                },
                Name::new("buildings_row_name"),
            ))
            .id();
        commands.entity(row).add_child(name);
        let qty_text = commands
            .spawn((
                Text::new(format!("\u{00d7}{}", **count)),
                TextFont {
                    font: mono_font.clone(),
                    font_size: BODY_SIZE,
                    ..default()
                },
                TextColor(CYAN),
                Name::new("buildings_row_qty"),
                BuildingsRowQtyChild,
            ))
            .id();
        commands.entity(row).add_child(qty_text);
        spawned_rows.insert(**building_type, BuildingsRow { row, qty_text });
    }
}

/// Cached child-entity ids for a single Buildings row. Carries the
/// row entity id (for cascade despawn) and the quantity-text child
/// entity (the only per-frame-mutable field on a building row).
#[derive(Clone, Copy)]
pub struct BuildingsRow {
    pub row: Entity,
    pub qty_text: Entity,
}

/// Child-node marker for the `Text` node holding the `×{count}`
/// quantity inside a Buildings row.
#[derive(Component)]
pub struct BuildingsRowQtyChild;

/// Build the **Mining** body. Persistent container with a header
/// + a content scroll area. The `update_mining_body` system
/// re-spawns the rows every frame based on the selected colony's
/// mine inventory + body deposits.
///
/// v0.5.2 PR-A.2: replaces the v0.5.x legacy egui mining tab
/// (`src/ui/construction_panel.rs:872-1392`). 7 surface groups
/// (24 base mines) + 1 collapsible orbital section (25 AutoMines
/// across 5 non-collapsible sub-groups). One card per mine. Each
/// card has [-] [+] buttons that push to
/// `PendingConstructionActions::mining_edits` (positive=add,
/// negative=remove) — see `process_construction_actions` in
/// `src/colony/systems.rs` for the consumer.
fn spawn_mining_body(
    commands: &mut Commands,
    parent: Entity,
    body_font: &Handle<Font>,
) {
    let body = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                width: Val::Percent(100.0),
                flex_grow: 1.0,
                padding: UiRect::all(Val::Px(SPACE_LG)),
                row_gap: Val::Px(SPACE_SM),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
            ConstructionTabBody::Mining,
            Visibility::Hidden,
            Name::new("mining_body"),
        ))
        .id();
    commands.entity(parent).add_child(body);

    // Header (text updated by `update_mining_body`).
    let header = commands
        .spawn((
            Text::new("MINING"),
            TextFont {
                font: body_font.clone(),
                font_size: BODY_SIZE,
                ..default()
            },
            TextColor(CYAN),
            Name::new("mining_header"),
            MiningHeader,
        ))
        .id();
    commands.entity(body).add_child(header);

    // Content container — `update_mining_body` despawns and
    // re-spawns the rows inside this container every frame.
    //
    // v0.5.2 PR-A.5: `min_height: Val::Px(0.0)` mirrors the build
    // tab's `card_grid` (line ~3694). Without it, the flex item
    // refuses to shrink below its intrinsic content height — the
    // default `min-height: auto` in flexbox layout. The result is
    // that the scroll container grows to fit its content, the
    // `Overflow::scroll_y()` never has anything to scroll, and the
    // `tick_ui_scroll_on_wheel` system silently no-ops on every
    // wheel event (the computed `max_y` is 0 because the content
    // fits without clipping). This is the exact fix the build tab
    // relies on for the same reason — the `card_grid` comment
    // explicitly calls this out as "critical for Bevy 0.18's flex
    // sizing behavior".
    let content = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                width: Val::Percent(100.0),
                flex_grow: 1.0,
                min_height: Val::Px(0.0),
                row_gap: Val::Px(SPACE_XS),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
            Name::new("mining_content"),
            MiningContent,
        ))
        .id();
    commands.entity(body).add_child(content);
}

/// Marker on the Mining body's header text.
#[derive(Component)]
pub struct MiningHeader;

/// Marker on the Mining body's content container.
#[derive(Component)]
pub struct MiningContent;

/// Marker on a per-group header row (chevron + label + count).
#[derive(Component)]
pub struct MiningGroupHeader {
    pub group_id: MiningGroupId,
}

/// Marker on a per-group body container (the row of cards). The
/// group's visibility is driven by `tick_mining_group_visibility`
/// toggling this entity's `Display`.
#[derive(Component)]
pub struct MiningGroupBody {
    pub group_id: MiningGroupId,
}

/// Marker on a single mine / AutoMine card's outer container.
#[derive(Component)]
pub struct MiningCard {
    pub building_type: BuildingType,
}

/// Marker on the Demolish button of a Mining card. The button removes
/// `mining_build_multiplier` (or the current count, whichever is
/// smaller) mines from the active colony via
/// `PendingConstructionActions::mining_edits` with a negative delta.
/// The handler `tick_mining_demolish_click` does the actual work.
#[derive(Component)]
pub struct MiningDemolishButton {
    pub building_type: BuildingType,
}

/// Marker added when the Demolish button should be disabled (no mines
/// to remove — `count == 0`). Mirrors `ConstructionCtaDisabled` for
/// the Queue button: the click handler skips pushing when this marker
/// is present, and the spawn code drops the marker at spawn time when
/// the count is non-zero.
#[derive(Component)]
pub struct MiningDemolishDisabled;

/// Marker on the orbital section's outer container (the 5 sub-groups).
/// Visibility is driven by `tick_mining_group_visibility` based on
/// `ui_state.mining_orbital_collapsed`.
#[derive(Component)]
pub struct MiningOrbitalBody;

/// Per-chip data for the cost-row hover tooltip. Carried by
/// every `ResourceCostChip` so the observer handlers can look up
/// the resource name + amount + category tint via the picked
/// entity id and write them into the tooltip's text node.
///
/// `name` is the display string (`"Iron"`, `"Water"`, `"He-3"`,
/// etc.) — not the raw RON name. `amount` is the formatted
/// `kg / t / Mt / Gt / Tt` string produced by `format_mining_reserve`.
/// `category` is the chip's category colour (Construction /
/// Volatiles / Fissile / etc.) so the tooltip can match the
/// chip's tint. `card` is the host card's entity id — the
/// observer uses it to find the right tooltip among the many
/// (one per visible card).
#[derive(Component, Clone)]
pub struct ResourceCostChip {
    pub name: String,
    pub amount: String,
    pub category: Color,
    pub card: Entity,
}

/// Marker on the singleton cost-chip hover tooltip overlay.
/// Spawned once at panel setup time (parented to the
/// construction `root`), populated each frame by
/// [`update_resource_cost_tooltip`] from
/// [`ResourceCostHoverState`]. Visual style mirrors the 3D
/// body-hover tooltip (`src/ui/mod.rs::ui_hover_tooltip`):
/// `TOOLTIP_BG` fill, cyan border, lg inner margin — so the
/// panel chrome and the world tooltips read as the same
/// design language.
#[derive(Component)]
pub struct ResourceCostTooltipOverlay;

/// Marker on the inner text node of the overlay. The update
/// system finds the text via `Single<&mut Text, With<…>>` so
/// it doesn't have to walk a Children hierarchy.
#[derive(Component)]
pub struct ResourceCostTooltipText;

/// Resource tracking which cost chip (if any) the cursor is
/// currently hovering. The `Pointer<Over>` observer writes
/// `Some(…)` on hover-in and the `Pointer<Out>` observer
/// writes `None` on hover-out. The `update_resource_cost_tooltip`
/// system reads this each frame to drive the overlay's
/// text/colour/position.
///
/// Cloning the small `String` data on every hover is cheap
/// (a hover can only happen on one chip at a time, and the
/// hovered chip is one of a few visible chips in the panel).
#[derive(Resource, Default)]
pub struct ResourceCostHoverState {
    pub chip: Option<HoveredChipData>,
}

/// Snapshot of a hovered chip's display data. The
/// `category` field is the chip's category tint
/// (Construction / Volatiles / Fissile / etc.) so the
/// overlay's text can match the chip's hue. `entity` is
/// the chip entity id and is mainly there for debug logs.
#[derive(Clone)]
pub struct HoveredChipData {
    pub name: String,
    pub amount: String,
    pub category: Color,
    pub entity: Entity,
}

/// Observer: on `Pointer<Over>`, snapshot the hovered chip's
/// `ResourceCostChip` data into [`ResourceCostHoverState`].
/// The `update_resource_cost_tooltip` system reads that
/// resource each frame to populate the singleton overlay.
///
/// The observer doesn't touch the overlay entity directly
/// — pointer observers shouldn't mutate other entities'
/// per-frame state. The system handles the visible position
/// + text + colour work because the overlay's `left/top`
/// must be set from the live cursor position, which the
/// observer doesn't have.
fn on_chip_hover_over(
    on: On<Pointer<Over>>,
    chip_query: Query<&ResourceCostChip>,
    mut hover_state: ResMut<ResourceCostHoverState>,
) {
    let Ok(chip) = chip_query.get(on.entity) else {
        return;
    };
    hover_state.chip = Some(HoveredChipData {
        name: chip.name.clone(),
        amount: chip.amount.clone(),
        category: chip.category,
        entity: on.entity,
    });
}

/// Observer: on `Pointer<Out>`, clear the hover state if the
/// cursor left the chip we're currently tracking. `Pointer<Out>`
/// fires once per entity whose bounds the cursor leaves; we
/// compare against `hover_state.chip.entity` so the state
/// isn't cleared by a stale event from a sibling element.
///
/// If the cursor moves from chip A → chip B without crossing
/// any other interactive element, Bevy firing order for the
/// same frame is: `Pointer<Out>(A)` then `Pointer<Over>(B)`.
/// The Over fires *after* the Out, so the resource ends up
/// holding chip B's data — which is the desired state. The
/// guard above ensures a stray Out event on a non-tracked
/// entity is a no-op.
fn on_chip_hover_out(
    on: On<Pointer<Out>>,
    mut hover_state: ResMut<ResourceCostHoverState>,
) {
    if let Some(current) = &hover_state.chip {
        if current.entity == on.entity {
            hover_state.chip = None;
        }
    }
}

/// Per-frame driver for the cost-chip hover overlay. Reads
/// `ResourceCostHoverState` (written by the chip observers)
/// and `Window::cursor_position()`, then either:
/// - hides the overlay (`Display::None`) when no chip is
///   hovered, when the chip entity was despawned between
///   frames (Build ↔ Mining sub-tab switch, multiplier chip
///   change, colony switch), or when the construction menu
///   isn't the active menu, OR
/// - positions the overlay next to the cursor (4 px below
///   the cursor vertically, 8 px right horizontally) and
///   populates the text with `"<name>  <amount>"`.
///
/// Coordinate-frame note (this is the bug behind the "tooltip
/// is way below the cursor" report):
///
/// The overlay is parented to the [`ConstructionRoot`]
/// node, which itself is positioned at
/// `top: 126.0; bottom: 72.0; left: 0; right: 0` so it lives
/// below the top resource-bar chrome and above the
/// time-controls dock (see `setup_construction`). Absolute-
/// positioned children of a node with `top: 126` place
/// relative to that node's content-area origin — i.e. an
/// inner node at `top: Val::Px(0)` lands at **window** Y=126,
/// not Y=0. `Window::cursor_position()` returns coordinates
/// in **window** space, so subtracting the canary root's
/// `top` constant is required to translate cursor → overlay
/// local coords. Without that subtraction the overlay
/// renders 126 px **below** the cursor — exactly the bottom-
/// right-of-card offset shown in the bug report.
///
/// Earlier this comment claimed the root spans the full
/// window and child `Val::Px(x)` values map to window
/// coords; that was wrong because the root's `top: 126`
/// offset is inherited by absolutely-positioned descendants.
/// This implementation subtracts that constant explicitly so
/// future readers don't have to re-derive the offset.
///
/// Visual style matches the body-hover tooltip in
/// `src/ui/mod.rs::ui_hover_tooltip`: same `TOOLTIP_BG`
/// fill, same cyan border. The only difference vs the body-
/// hover tooltip is this is Bevy UI, not egui, so positioning
/// is `Node::left/top` rather than `egui::Area`.
///
/// The clamp on `left/top` keeps the tooltip on-screen when
/// the cursor is near the right/bottom edges — mirrors the
/// `clamp(0.0, max_left)` / `clamp(0.0, max_top)` pattern in
/// `update_shipbuilding_hover_tooltip`.
fn update_resource_cost_tooltip(
    active_menu: Res<ActiveMenu>,
    primary_window: Query<&Window, With<PrimaryWindow>>,
    chip_query: Query<&ResourceCostChip>,
    mut hover_state: ResMut<ResourceCostHoverState>,
    mut overlay_node: Single<&mut Node, With<ResourceCostTooltipOverlay>>,
    mut tooltip_text: Single<&mut Text, With<ResourceCostTooltipText>>,
    mut tooltip_color: Single<&mut TextColor, With<ResourceCostTooltipText>>,
) {
    // The canary root's `top: 126.0` offset (set in
    // `setup_construction`) is inherited by absolutely-
    // positioned descendants; subtract it from the cursor Y
    // to translate window-space cursor coords into overlay
    // local coords. If the canary root ever moves (e.g. a
    // future chrome redesign changes the topbar height),
    // update this constant and the matching root-anchor
    // value in `setup_construction` together.
    const CANARY_ROOT_TOP_PX: f32 = 126.0;

    // Hide the overlay whenever the construction canary isn't
    // the active menu (player switched to Shipbuilding /
    // Notifications / starmap / etc.). This catches the
    // "stuck tooltip" class of bugs: the canary root keeps
    // its `ResourceCostTooltipOverlay` child alive across
    // menu transitions, so without this guard the overlay
    // would stay visible — anchored to whatever cursor
    // position was last seen — until the player hovers a
    // new chip or the state is otherwise reset.
    let construction_menu_active = matches!(active_menu.current, GameMenu::Construction);
    if !construction_menu_active {
        overlay_node.display = Display::None;
        if hover_state.chip.is_some() {
            hover_state.chip = None;
        }
        return;
    }

    // No chip hovered: hide the overlay. The
    // `Pointer<Out>` observer already cleared the hover
    // state on the normal cursor-out path, but there are
    // three races where `Some(...)` can linger:
    //   1. The chip entity is cascade-despawned mid-frame
    //      (Build ↔ Mining sub-tab switch, multiplier chip
    //      change, colony switch, queue-row despawn). Bevy
    //      0.18's pointer backend doesn't reliably fire
    //      `Pointer<Out>` for entities that vanish between
    //      frames, so the `Some(...)` survives.
    //   2. The canary root is rebuilt by a future
    //      re-root teardown. We also defensively catch that
    //      above by returning when the menu isn't active.
    //   3. A hot-reload / replay-restore hands back a stale
    //      `ResourceCostHoverState`. The chip-entity check
    //      below catches all three uniformly.
    //
    // B0001 (Bevy 0.18): we mutate `hover_state.chip` here
    // so we hold `ResMut<ResourceCostHoverState>` and never
    // pair it with a second `Query<...>` or `Res<...>` that
    // also reaches for the same data.
    let stale = match &hover_state.chip {
        Some(data) => chip_query.get(data.entity).is_err(),
        None => false,
    };
    if stale {
        hover_state.chip = None;
        overlay_node.display = Display::None;
        return;
    }
    let Some(data) = &hover_state.chip else {
        overlay_node.display = Display::None;
        return;
    };

    // Need a window to position relative to. If there's no
    // primary window yet (shouldn't happen post-Startup),
    // bail out without showing the overlay.
    let Ok(window): Result<&Window, _> = primary_window.single() else {
        overlay_node.display = Display::None;
        return;
    };

    // Off-screen cursor (window not focused, or cursor left
    // the window): hide the overlay. `cursor_position()` is
    // called twice — once for the early-out, once to bind —
    // because the alternative (`if let Some(cursor) = ...`)
    // confused the type inference around `Vec2` and produced
    // E0282 on `primary_window.single()`.
    if window.cursor_position().is_none() {
        overlay_node.display = Display::None;
        return;
    }
    let cursor = window.cursor_position().unwrap();

    // Translate window-space cursor → canary-root-local pixel
    // coords. The X axis is unaffected (root has `left: 0`).
    // The Y axis subtracts the canary-root top offset; the
    // +4 below-cursor vertical nudge keeps the tooltip snug
    // under the chip without overlapping it.
    let local_x = cursor.x;
    let local_y = cursor.y - CANARY_ROOT_TOP_PX + 4.0;

    // Conservative right/bottom clamp: leave 240 px of room
    // on the right for the tooltip's width (the longest text
    // is e.g. "Helium-3  25.0 kt" at ~12 px / char × ~14 chars
    // ≈ 170 px) and 48 px on the bottom. Clamps are against
    // the canary root's content-area dimensions (the overlay
    // is a child of the root, so its right/bottom edge is
    // measured against the root's box).
    const TOOLTIP_W: f32 = 240.0;
    const TOOLTIP_H: f32 = 48.0;
    let root_width = (window.width() - 0.0).max(TOOLTIP_W);
    let root_height = (window.height() - CANARY_ROOT_TOP_PX - 72.0).max(TOOLTIP_H);
    let max_left = (root_width - TOOLTIP_W).max(0.0);
    let max_top = (root_height - TOOLTIP_H).max(0.0);
    overlay_node.left = Val::Px(local_x.clamp(0.0, max_left));
    overlay_node.top = Val::Px(local_y.clamp(0.0, max_top));
    overlay_node.display = Display::Flex;

    // Update text + colour. Two spaces between name and
    // amount so the formatted units (`"250.0 t"`, `"1.20 Gt"`)
    // read as a separate visual unit. Colour matches the chip
    // so the tooltip carries the chip's category hue. The
    // shipbuilding workspace's `update_shipbuilding_hover_tooltip`
    // uses the same `**` deref pattern on its
    // `Single<&mut Text>`; it works because `Single` derefs
    // to the underlying `Mut<Text>` and `Mut<Text>` derefs to
    // `Text` — so `**` lands on the `Text` itself, not its
    // inner `String` (since `Text(pub String)` is a tuple
    // struct without a Deref impl to `String`).
    **tooltip_text = Text::new(format!("{}  {}", data.name, data.amount));
    **tooltip_color = TextColor(data.category);
}

/// Update the Mining tab body. Re-spawns the cards inside the
/// `MiningContent` container every time it runs. Triggered by
/// `ConstructionUiState` changes (tab switch, qty chip, group
/// collapse) and `Colony` changes (mine count edit, deposit
/// update, colony switch). Skips on other frames — see the
/// `update_mining_body` system registration in the plugin.
///
/// Per spec: 7 surface groups (24 base mines) + 1 orbital
/// section (25 AutoMines across 5 sub-groups). One card per
/// mine. Each card shows count / production / reserve /
/// accessibility and [-] [+] buttons that route to
/// `PendingConstructionActions::mining_edits`.
#[allow(clippy::too_many_arguments)]
pub fn update_mining_body(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    ui_state: Res<ConstructionUiState>,
    buildings_data: Res<BuildingsData>,
    resource_icons: Option<Res<ResourceIcons>>,
    // v0.5.2 PR-A.5: thread the BuildingIcons resource through so each
    // mining card can render the same cyan-tinted building icon as the
    // Build tab. Without this, `spawn_mining_card` always passes `None`
    // to `spawn_card` and the cards render the placeholder square
    // instead of the real icon. `None` is acceptable — the resource
    // may not be populated yet on the first frame after startup.
    building_icons: Option<Res<BuildingIcons>>,
    colonies: Query<(Entity, &crate::colony::Colony)>,
    body_query: Query<(
        &CelestialBody,
        Option<&crate::astronomy::components::AtmosphereComposition>,
        Option<&crate::economy::PlanetResources>,
    )>,
    content_query: Query<Entity, With<MiningContent>>,
    mut header_query: Query<&mut Text, With<MiningHeader>>,
    mut spawned_rows: Local<Vec<Entity>>,
) {
    let Ok(content) = content_query.single() else { return; };

    // Despawn the previous frame's spawn.
    //
    // `try_despawn` is the Bevy 0.18 idiom for "despawn if alive,
    // silently drop if gone." The `Local<Vec<Entity>>` cache can hold
    // IDs from a frame where the `MiningContent` parent (and all its
    // children) was cascade-despawned — for example when the player
    // toggled the Construction menu visibility off, or when a UI
    // re-root teardown cleared the body. Without `try_despawn` we get
    // a flood of `WARN ... Entity despawned` log lines every frame.
    for entity in spawned_rows.drain(..) {
        commands.entity(entity).try_despawn();
    }

    let body_font: Handle<Font> = asset_server.load("fonts/Inter-Regular.otf");
    let body_font_medium: Handle<Font> =
        asset_server.load("fonts/Inter-SemiBold.otf");
    let mono_font: Handle<Font> = asset_server.load("fonts/GeistMono-Medium.ttf");
    let multiplier = ui_state.mining_build_multiplier;
    // v0.5.2 PR-A.4 follow-up: hand each mining card a
    // concrete reference to the resource-icon atlas (or an
    // empty fallback when the Startup loader hasn't
    // populated `ResourceIcons` yet — `post_process_resource_icons`
    // will catch up on the next tick).
    let empty_resource_icons = ResourceIcons::default();
    let resource_icons: &ResourceIcons = resource_icons
        .as_ref()
        .map(|r: &Res<ResourceIcons>| -> &ResourceIcons { r.as_ref() })
        .unwrap_or(&empty_resource_icons);

    // Resolve the active colony + body data in one pass.
    let active_colony_entity = ui_state.selected_colony;
    let colony_data: Option<(
        String,
        bool,
        Option<BodyType>,
        Option<&crate::economy::PlanetResources>,
        std::collections::HashMap<BuildingType, u32>,
    )> = active_colony_entity.and_then(|e| {
        colonies.get(e).ok().and_then(|(_, c)| {
            body_query.get(e).ok().map(|(body, atmo_opt, res_opt)| {
                let name = format!("{} (colony)", body.name);
                let breathable = atmo_opt.map(|a| a.breathable);
                (
                    name,
                    breathable.unwrap_or(false),
                    Some(body.body_type),
                    res_opt,
                    c.buildings.clone(),
                )
            })
        })
    });

    // Update the header text.
    let header_text = match &colony_data {
        Some((name, _, _, _, _)) => format!("MINING — {}", name),
        None => "MINING — (no colony selected)".to_string(),
    };
    for mut text in header_query.iter_mut() {
        **text = header_text.clone();
    }

    // No colony → render a single placeholder and bail.
    let Some((_colony_name, body_breathable, body_type, planet_resources, building_counts)) =
        colony_data
    else {
        let placeholder = commands
            .spawn((
                Text::new("(no colony selected)"),
                TextFont {
                    font: body_font.clone(),
                    font_size: BODY_SIZE,
                    ..default()
                },
                TextColor(TEXT_DIM),
                Name::new("mining_no_colony"),
            ))
            .id();
        commands.entity(content).add_child(placeholder);
        spawned_rows.push(placeholder);
        return;
    };

    // Build-qty chip row (mirrors the Build tab's qty row but
    // routes clicks to `ChipKind::MiningQty`).
    spawn_mining_qty_row(
        &mut commands,
        content,
        &ui_state,
        &body_font,
        &mono_font,
        &mut |entity| spawned_rows.push(entity),
    );

    // v0.5.2 PR-A.5: resolve the BuildingIcons borrow once so the
    // closure below can look up each building's icon without
    // re-borrowing the `Option<Res<BuildingIcons>>` inside the loop.
    // `update_card_grid` (build tab) does the same `building_icons
    // .handles.get(&building_type)` lookup inline per card; here we
    // centralize it so the surface + orbital sections share the
    // `&BuildingIcons` reference.
    let empty_building_icons = BuildingIcons::default();
    let building_icons_ref: &BuildingIcons = building_icons
        .as_ref()
        .map(|r: &Res<BuildingIcons>| -> &BuildingIcons { r.as_ref() })
        .unwrap_or(&empty_building_icons);

    // 7 surface groups.
    for (group_id, group_label, group_buildings) in MINING_GROUPS_SURFACE {
        let group_collapsed = ui_state.mining_groups_collapsed.contains(group_id);
        let group_node = spawn_mining_group_section(
            &mut commands,
            content,
            *group_id,
            group_label,
            group_buildings,
            group_collapsed,
            body_breathable,
            body_type,
            planet_resources,
            &buildings_data,
            &building_counts,
            &body_font,
            &body_font_medium,
            &mono_font,
            multiplier,
            &resource_icons,
            building_icons_ref,
        );
        spawned_rows.push(group_node);
    }

    // 1 orbital section (collapsible).
    let orbital_node = spawn_mining_orbital_section(
        &mut commands,
        content,
        ui_state.mining_orbital_collapsed,
        body_breathable,
        body_type,
        planet_resources,
        &buildings_data,
        &building_counts,
        &body_font,
        &body_font_medium,
        &mono_font,
        multiplier,
        &resource_icons,
        building_icons_ref,
    );
    spawned_rows.push(orbital_node);
}

/// Setup system: spawns the Construction panel entities once at startup.
///
/// The canary is gated by `ConstructionState`. When `Off`, the
/// root is hidden via `Visibility::Hidden`.
pub fn setup_construction(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    buildings_data_opt: Option<Res<BuildingsData>>,
    research_state: Res<ResearchState>,
    ui_state: Res<ConstructionUiState>,
    building_icons: Option<Res<BuildingIcons>>,
    resource_icons: Option<Res<ResourceIcons>>,
    colonies: Query<(Entity, &crate::colony::Colony)>,
) {
    let body_font = asset_server.load("fonts/Inter-Regular.otf");
    let body_font_medium = asset_server.load("fonts/Inter-SemiBold.otf");
    let mono_font = asset_server.load("fonts/GeistMono-Medium.ttf");

    // Window-filling root container. The `top: 126.0` offset pushes the
    // canary below the global in-game chrome (top resource bar + icon
    // strip) so the canary's own AppBar doesn't get visually overwritten
    // by the egui chrome. The 126 px offset matches the convention used
    // by `shipbuilding_workspace.rs` for its floating workspace.
    //
    // The `bottom: 72.0` offset lifts the panel above the bottom
    // `ui_time_controls` dock (register in `UiSystemSet::TopBar`,
    // `min_height: 54.0` plus ~18 px of egui frame margins). Without
    // this offset, the last row of cards scrolled into the panel
    // would slip behind the dock and become unreachable with the
    // scrollbar — the player's only visual cue would be the
    // scrollbar thumb reaching the bottom while the bottom row
    // remained hidden.
    let root = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(126.0),
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                bottom: Val::Px(72.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::FlexStart,
                // `align_items: Start` makes children that don't have a
                // fixed width (the chip-row containers) hug the left edge.
                // The card grid still stretches to fill the remaining
                // space because it has `flex_grow: 1.0`.
                align_items: AlignItems::Start,
                // Vertical spacing between rows. The 8-px gap between
                // chip rows matches the prototype's visual rhythm.
                row_gap: Val::Px(SPACE_SM),
                padding: UiRect::all(Val::Px(SPACE_LG)),
                ..default()
            },
            // Heavy dark navy backdrop — substitutes for a backdrop blur
            // (Bevy 0.18 has no `backdrop-filter` for UI). The 97% alpha
            // heavily dims the 3D map so the panel chrome reads cleanly.
            // The previous 92% left the menu icons visible through the
            // panel, which made the cards "bleed into the main menu" —
            // cranking the alpha to 0.97 cuts the bleed while leaving a
            // faint hint of the orbital view still visible at the edges.
            BackgroundColor(Color::srgba(0.012, 0.024, 0.047, 0.97)),
            ZIndex(1),
            Visibility::Hidden,
            Name::new("construction_canary_root"),
            ConstructionRoot,
        ))
        .id();

    // ── Tab strip ────────────────────────────────────────────────────────
    // Overview | Buildings | Build | Mining — Build is the active tab.
    // v0.5.2: "Stockpiles" renamed to "Mining" (the dedicated mining
    // grid replaces the v0.5.x minimum-stockpile editor).
    // Static placeholder; will be wired to ConstructionUiState in Phase C4.
    // The tab strip is wrapped in a single bordered container like the
    // other chip rows.
    let tab_strip = commands
        .spawn(ChipRowContainerBundle::new("tabs", TAB_STRIP_H))
        .id();
    commands.entity(root).add_child(tab_strip);

    for (i, (label, is_active)) in [
        ("Overview", false),
        ("Buildings", false),
        ("Build", true),
        ("Mining", false),
    ]
    .iter()
    .enumerate()
    {
        // The active tab gets a thicker bottom border (2 px) to match the
        // prototype's tab indicator, plus a `BoxShadow` glow on the bottom
        // edge for the "lit underline" look.
        let border_override = if *is_active {
            Some(UiRect {
                left: Val::Px(1.0),
                right: Val::Px(1.0),
                top: Val::Px(1.0),
                bottom: Val::Px(2.0),
            })
        } else {
            None
        };
        let chip = ChipButtonBundle::new_with_border(label, *is_active, border_override);
        let mut entity_commands = commands.spawn(chip);
        entity_commands.insert(ChipKind::Tab(i));
        if *is_active {
            // Subtle cyan glow under the active tab — the "lit underline"
            // effect seen in the prototype. Tight spread (4 px) and low
            // alpha (35%) so it reads as a glow, not a flare.
            entity_commands.insert(BoxShadow::new(
                Color::srgba(0.373, 0.784, 0.847, 0.35),
                Val::Px(0.0),
                Val::Px(2.0),
                Val::Px(0.0),
                Val::Px(4.0),
            ));
        }
        let tab = entity_commands.id();
        commands.entity(tab_strip).add_child(tab);
        // Spawn the text as a child node so it gets centered within the
        // button (see `spawn_chip_text` doc for the bevy_ui 0.18 pattern).
        spawn_chip_text(&mut commands, tab, label, body_font.clone(), *is_active, TAB_FONT_SIZE);
    }

        // ── Build output (BP/year) + Active Colony dropdown ─────────────
    // Per user feedback (OOB 2026-07-31):
    //   - Treasury + Balance removed: those are already in the global top
    //     resource bar and don't need to be repeated here.
    //   - Build output (BP/year) is its own row, more prominent.
    //   - Active Colony becomes a dropdown selector.
    //   - Both have icons (placeholder squares until per-icon art lands).
    // Static placeholder; will be wired to ConstructionUiState in Phase C4.
    // Build-only header stack. Wraps the output row, active-colony
    // picker, build-qty chips, and filter chips in a single flex
    // column with `ConstructionTabBody::Build` so the
    // `tick_construction_body_visibility` system hides it on every
    // other tab (Overview / Buildings / Stockpiles). These chrome
    // rows are build-menu specific and don't belong on the read-only
    // tabs.
    let build_header_stack = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                width: Val::Percent(100.0),
                // `align_items: Start` keeps the build-only chrome rows
                // (output / picker / build-qty / filter) at their
                // natural content width on the left edge of the panel.
                // Without this, every child gets stretched to the
                // column's full width by the default
                // `AlignItems::Stretch` and the chip-row containers
                // (e.g. `ChipRowContainerBundle::new("build_qty", …)`)
                // render as wide horizontal bars instead of hugging
                // their chips.
                align_items: AlignItems::Start,
                row_gap: Val::Px(SPACE_SM),
                ..default()
            },
            // `ZIndex(1)` on the entire header stack lifts the build
            // chrome (filter / qty / picker rows) above the
            // `card_grid` (default `ZIndex(0)`) so the chip rows stay
            // readable when the cards scroll underneath. The colony
            // dropdown uses `GlobalZIndex(100)` on its own entity so
            // it can draw on top of *every* UI node — see the
            // dropdown's own ZIndex comment for the rationale.
            ZIndex(1),
            ConstructionTabBody::Build,
            Visibility::Hidden,
            Name::new("build_header_stack"),
        ))
        .id();
    commands.entity(root).add_child(build_header_stack);

    let output_row = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                height: Val::Px(32.0),
                padding: UiRect::horizontal(Val::Px(SPACE_LG)),
                column_gap: Val::Px(SPACE_LG),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
            Name::new("output_row"),
        ))
        .id();
    commands.entity(build_header_stack).add_child(output_row);

    // Build output (BP/year) — icon + label + value.
    let output_icon = commands
        .spawn((
            Node {
                width: Val::Px(20.0),
                height: Val::Px(20.0),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(3.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.196, 0.529, 0.612, 0.30)),
            BorderColor::all(CYAN_BORDER),
            Name::new("output_icon"),
        ))
        .id();
    commands.entity(output_row).add_child(output_icon);
    let output_label = commands
        .spawn((
            Text::new("Output: "),
            TextFont {
                font: body_font.clone(),
                font_size: BODY_SIZE,
                ..default()
            },
            TextColor(TEXT_DIM),
            Name::new("output_label"),
        ))
        .id();
    commands.entity(output_row).add_child(output_label);
    let output_value = commands
        .spawn((
            Text::new("12001.0 BP/year"),
            TextFont {
                font: mono_font.clone(),
                font_size: BODY_SIZE,
                ..default()
            },
            TextColor(CYAN),
            Name::new("output_value"),
        ))
        .id();
    commands.entity(output_row).add_child(output_value);
    // Queue summary: "Queue: 5d 2h" (yellow) or "Empty Queue" (green).
    // Reads from the ConstructionQueue resource at spawn time and renders
    // the appropriate label. For now the static placeholder has 1 item
    // queued so we always show the time format — the green pill is the
    // empty-queue variant.
    let queue_label = commands
        .spawn((
            Text::new("    │   Queue: "),
            TextFont {
                font: body_font.clone(),
                font_size: BODY_SIZE,
                ..default()
            },
            TextColor(TEXT_DIM),
            Name::new("queue_label"),
        ))
        .id();
    commands.entity(output_row).add_child(queue_label);
    let queue_value = commands
        .spawn((
            Text::new("6d 2h"),  // static placeholder; reads from resource in C4
            TextFont {
                font: mono_font.clone(),
                font_size: BODY_SIZE,
                ..default()
            },
            TextColor(YELLOW_ETA),
            Name::new("queue_value"),
            QueuePanelSummaryText,
        ))
        .id();
    commands.entity(output_row).add_child(queue_value);

    // "Open Queue" chip — clicking it toggles `QueuePanelState::open`.
    // The chip is the same size as the build-qty / category chips so the
    // visual rhythm stays consistent. The label includes a small dot
    // when the panel is open (placeholder; the active-overlay system
    // handles the actual visual).
    let queue_chip = commands
        .spawn((
            Button,
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                height: Val::Px(20.0),
                padding: UiRect::horizontal(Val::Px(SPACE_MD)),
                column_gap: Val::Px(SPACE_XS),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(3.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
            BorderColor::all(CYAN_BORDER),
            Pickable::default(),
            Name::new("open_queue_chip"),
            OpenQueueChip,
        ))
        .id();
    commands.entity(output_row).add_child(queue_chip);
    let queue_chip_label = commands
        .spawn((
            Text::new("OPEN QUEUE"),
            TextFont {
                font: body_font_medium.clone(),
                font_size: CAPTION_SIZE,
                ..default()
            },
            TextColor(CYAN),
            Name::new("open_queue_chip_label"),
        ))
        .id();
    commands.entity(queue_chip).add_child(queue_chip_label);

    // Active Colony — styled as a dropdown selector with a chevron.
    let picker = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                height: Val::Px(24.0),
                padding: UiRect::horizontal(Val::Px(SPACE_MD)),
                column_gap: Val::Px(SPACE_SM),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.012, 0.039, 0.078, 0.40)),
            BorderColor::all(CYAN_BORDER),
            Pickable::default(),
            Button,
            Name::new("active_colony_picker"),
            ColonyPicker,
        ))
        .id();
    commands.entity(build_header_stack).add_child(picker);
    let colony_icon = commands
        .spawn((
            Node {
                width: Val::Px(14.0),
                height: Val::Px(14.0),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.373, 0.784, 0.847, 0.40)),
            BorderColor::all(CYAN_BORDER),
            Name::new("colony_icon"),
        ))
        .id();
    commands.entity(picker).add_child(colony_icon);
    let colony_label = commands
        .spawn((
            Text::new("Active Colony: "),
            TextFont {
                font: body_font.clone(),
                font_size: CAPTION_SIZE,
                ..default()
            },
            TextColor(TEXT_DIM),
            Name::new("colony_label"),
        ))
        .id();
    commands.entity(picker).add_child(colony_label);
    let colony_value = commands
        .spawn((
            Text::new("(no colony)"),
            TextFont {
                font: body_font_medium.clone(),
                font_size: CAPTION_SIZE,
                ..default()
            },
            TextColor(CYAN),
            Name::new("colony_value"),
            ColonyPickerText,
        ))
        .id();
    commands.entity(picker).add_child(colony_value);
    let chevron = commands
        .spawn((
            Text::new("▾"),
            TextFont {
                font: body_font.clone(),
                font_size: CAPTION_SIZE,
                ..default()
            },
            TextColor(CYAN),
            Name::new("colony_chevron"),
        ))
        .id();
    commands.entity(picker).add_child(chevron);

    // Colony dropdown menu — floats below the picker when open. Each
    // colony gets its own `ColonyDropdownOption` row that, when clicked,
    // updates `ConstructionUiState::selected_colony` and closes the menu.
    // The `tick_colony_dropdown_visibility` system toggles the
    // `Visibility` based on `ColonyDropdownState::open`; the
    // `refresh_colony_dropdown` system keeps the row set in sync with
    // the live `Query<Entity, With<Colony>>` list every frame.
    //
    // `PositionType::Absolute` lets the menu overlay the card grid when
    // open (the grid would otherwise push the menu down and break the
    // dropdown affordance). The top offset lands the menu directly below
    // the picker (24 px picker height + 4 px gap).
    let dropdown = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(28.0),
                left: Val::Px(0.0),
                // Match the picker width. The picker itself uses
                // content-bounded flex layout, so we set a generous
                // fixed width here that comfortably fits a long colony
                // name plus population suffix.
                min_width: Val::Px(220.0),
                max_width: Val::Px(320.0),
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(SPACE_XS)),
                row_gap: Val::Px(SPACE_XS),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.008, 0.039, 0.094, 0.96)),
            BorderColor::all(CYAN_BORDER_STRONG),
            // Drop shadow — same recipe as the build card. BoxShadow
            // is a separate bundle component, not a `Node` field in
            // Bevy 0.18, so it's listed alongside the other bundle
            // members here.
            BoxShadow::new(
                Color::srgba(0.0, 0.0, 0.0, 0.65),
                Val::Px(0.0),
                Val::Px(4.0),
                Val::Px(2.0),
                Val::Px(12.0),
            ),
            // `GlobalZIndex` (not `ZIndex`) lifts the dropdown out of
            // the `build_header_stack` subtree so it can draw on top
            // of the `card_grid` and any other siblings of the header
            // stack. Per Bevy 0.18, `ZIndex` only orders siblings of
            // the same parent — the dropdown is nested inside the
            // picker which is inside `build_header_stack`, so its
            // `ZIndex` would only order it among the picker's
            // children (which is just the dropdown itself). Using
            // `GlobalZIndex(100)` puts it above every other UI node
            // in the canary, which is the popup-layer behavior the
            // player expects.
            GlobalZIndex(100),
            Visibility::Hidden,
            Name::new("active_colony_dropdown"),
            ColonyDropdownMenu,
        ))
        .id();
    commands.entity(picker).add_child(dropdown);

    // ── Build qty row ──────────────────────────────────────────────────
    // Build output (BP/yr) and quick-select quantity buttons (x1, x5, x10,
    // x25, x50, x100). Static placeholder; will be wired to ConstructionUiState
    // in Phase C4. The whole row is wrapped in a single bordered container.
    let build_qty_row = commands
        .spawn(ChipRowContainerBundle::new("build_qty", 28.0))
        .id();
    commands.entity(build_header_stack).add_child(build_qty_row);

    // "Build" label.
    let build_qty_label = commands
        .spawn((
            Text::new("Build qty: "),
            TextFont {
                font: body_font.clone(),
                font_size: CAPTION_SIZE,
                ..default()
            },
            TextColor(TEXT_DIM),
            Name::new("build_qty_label"),
        ))
        .id();
    commands.entity(build_qty_row).add_child(build_qty_label);

    // Quantity buttons (x1, x5, x10, x25, x50, x100). x1 is active by default.
    for (i, (label, qty)) in [
        ("x1", 1u32),
        ("x5", 5),
        ("x10", 10),
        ("x25", 25),
        ("x50", 50),
        ("x100", 100),
    ]
    .iter()
    .enumerate()
    {
        let is_active = i == 0;
        let btn = commands.spawn(ChipButtonBundle::new(label, is_active)).id();
        commands.entity(btn).insert(ChipKind::Qty(*qty));
        commands.entity(build_qty_row).add_child(btn);
        spawn_chip_text(&mut commands, btn, label, mono_font.clone(), is_active, 16.0);
    }

    // ── Filter row ─────────────────────────────────────────────────────
    // All | Food | Power | Industry | Research | Synergy Active. Static
    // placeholder; will be wired to ConstructionUiState in Phase C4.
    // The whole row is wrapped in a single bordered container.
    let filter_row = commands
        .spawn(ChipRowContainerBundle::new("filter", 28.0))
        .id();
    commands.entity(build_header_stack).add_child(filter_row);

    let filter_label = commands
        .spawn((
            Text::new("Filter: "),
            TextFont {
                font: body_font.clone(),
                font_size: CAPTION_SIZE,
                ..default()
            },
            TextColor(TEXT_DIM),
            Name::new("filter_label"),
        ))
        .id();
    commands.entity(filter_row).add_child(filter_label);

    // Filter row: 8 category chips + "All" (index 8 = show every building).
    // The previous "Food/Power/Industry/Research/Synergy Active" chips
    // are removed — the category axis already covers them, and the dual
    // filter was confusing (per user feedback 2026-08-01).
    //
    // Index mapping (v0.5.2 PR-A.2 round 2: Mining chip removed):
    //   0 = Infrastructure
    //   1 = Industry        (was 2)
    //   2 = Logistics       (was 3)
    //   3 = Power           (was 4)
    //   4 = Population      (was 5)
    //   5 = Research        (was 6)
    //   6 = Financial       (was 7)
    //   7 = Military        (was 8)
    //   8 = All (special: bypasses category filter, was 9)
    //
    // Mines are managed exclusively in the dedicated Mining tab; the
    // Build tab's category axis no longer exposes a "Mining" chip
    // even though the buildings.ron still tags mines with
    // `category: "Mining"`. Selecting "All" on the Build tab still
    // includes mines (the chip is the user's filter, not a hard
    // exclusion).
    for (i, label) in [
        "Infrastructure",
        "Industry",
        "Logistics",
        "Power",
        "Population",
        "Research",
        "Financial",
        "Military",
        "All",
    ]
    .iter()
    .copied()
    .enumerate()
    {
        // The "All" chip is active by default (index 8).
        let is_active = i == 8;
        let chip = commands.spawn(ChipButtonBundle::new(label, is_active)).id();
        commands.entity(chip).insert(ChipKind::Category(i));
        commands.entity(filter_row).add_child(chip);
        spawn_chip_text(&mut commands, chip, label, body_font.clone(), is_active, 16.0);
    }

    // ── Category chips row ─────────────────────────────────────────────
    // 9 category chips with counts: Infrastructure (5) | Mining & Industry
    // (3) | Logistics (4) | Power Generation (6) | ... | Locked (16).
    // Static placeholder; will be wired to BuildingsData in Phase C4.
    // The whole row is wrapped in a single bordered container.
    // 4-per-row card layout. Uses flex-wrap (NOT CSS Grid) because
    // Bevy 0.18's grid sizing was producing single-pixel-tall rows
    // when combined with `flex_grow: 1.0` + `Overflow::scroll_y()` —
    // the cards rendered as visible top edges only. Flex-wrap is
    // battle-tested: 4 cards/row via `width: Val::Percent(25%)`,
    // wraps to the next row on overflow, each card keeps its 240 px
    // height.
    let card_grid = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                align_content: AlignContent::Start,
                // `align_items: Stretch` makes every card in the same
                // row share the tallest card's height. Combined with
                // the card's `min_height: 244`, this gives uniform card
                // heights across the row regardless of content count
                // (different buildings have different numbers of effect
                // bullets — 2, 3, or 4).
                align_items: AlignItems::Stretch,
                width: Val::Percent(100.0),
                flex_grow: 1.0,
                min_height: Val::Px(0.0),
                padding: UiRect::all(Val::Px(SPACE_LG)),
                row_gap: Val::Px(SPACE_LG),
                column_gap: Val::Px(SPACE_LG),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
            ConstructionTabBody::Build,
            Visibility::Hidden,
            Name::new("card_grid"),
        ))
        .id();
    commands.entity(root).add_child(card_grid);
    // Mark the grid so the refresh system can find it.
    commands.entity(card_grid).insert(CardGrid);

    // ── Scrollbar overlay ─────────────────────────────────────────────
    // Always-visible vertical scrollbar pinned to the right edge of
    // the card grid. The track is a full-height rounded rectangle;
    // the thumb is an absolutely-positioned child whose `height` +
    // `top` are driven each frame by `tick_construction_scrollbar`
    // from the grid's `ScrollPosition` + `ComputedNode::content_size`.
    // Clicking the track jumps the scroll position; dragging the
    // thumb pans the scroll. Bevy 0.18's `bevy_ui` core auto-renders
    // scrollbar visuals only when overflow exists, with no
    // always-visible track option; the custom overlay below gives
    // the player a permanent visual cue that the grid is scrollable.
    //
    // Parenting: the track is a child of `root` (the panel chrome),
    // NOT of `card_grid`. If we parented it to `card_grid` it would
    // scroll out of view as the player scrolls the content — the
    // `Overflow::scroll_y` on `card_grid` would carry the track
    // away with the cards. Mounting on `root` keeps the track pinned
    // to the panel.
    //
    // Positioning the track relative to `root` requires us to know
    // where the card grid sits within root. The chrome layout puts
    // a tab strip + filter chips above the grid; we offset the
    // track's `top` past those rows. 138 px empirically matches the
    // current row-stack (tab strip ~36 + chip rows ~80 + the 12-px
    // panel padding) — kept as a magic number to avoid wiring a new
    // layout constant.
    let track_top_px = 138.0_f32;
    let track_bottom_px = SPACE_SM;
    // Width was 6 px before v0.5.2 PR-A.4 — felt too thin to hit
    // reliably with a mouse. Doubled to 12 px so the track and
    // thumb read as a proper "rail" without dominating the panel.
    let scrollbar_track = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(2.0),
                top: Val::Px(track_top_px),
                bottom: Val::Px(track_bottom_px),
                width: Val::Px(12.0),
                // No flex children — the thumb is `position_type:
                // Absolute` so it positions itself via `top` /
                // `height`. The track itself has no children layout
                // responsibilities. Border radius is half the
                // width so the track reads as a pill.
                border_radius: BorderRadius::all(Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.06)),
            ZIndex(10),
            Pickable::default(),
            Name::new("card_grid_scrollbar_track"),
            CardGridScrollbarTrack,
        ))
        .id();
    commands.entity(root).add_child(scrollbar_track);
    // Attach press / release observers so the drag stays
    // "locked" to the track entity even when the cursor moves
    // out of the slim track area during the drag. Without
    // these observers we'd be polling `Interaction::Pressed`,
    // which drops to `None` the moment the cursor leaves the
    // thin (6px-wide) track — making drag impossible.
    commands.entity(scrollbar_track).observe(on_track_press);
    commands.entity(scrollbar_track).observe(on_track_release);

    let scrollbar_thumb = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),    // updated each frame
                height: Val::Px(0.0), // updated each frame
                // Border radius matches the track's pill (half its
                // 12 px width) so the thumb visually nests inside
                // the track.
                border_radius: BorderRadius::all(Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(CYAN.with_alpha(0.6)),
            ZIndex(11),
            Pickable::default(),
            Name::new("card_grid_scrollbar_thumb"),
            CardGridScrollbarThumb,
        ))
        .id();
    commands.entity(scrollbar_track).add_child(scrollbar_thumb);
    // Same rationale as the track observers: the thumb is only
    // ~6 px wide, so a one-pixel slip on `Interaction::Pressed`
    // would drop the drag. The press/release observers keep
    // `drag.active` true until the user actually releases the
    // pointer button, regardless of where the cursor goes.
    commands.entity(scrollbar_thumb).observe(on_thumb_press);
    commands.entity(scrollbar_thumb).observe(on_thumb_release);

    // Spawn card placeholders from real `BuildingsData` filtered by the
    // active category tab + functional-role filter (all read from
    // `ConstructionUiState`).
    let buildings_data = match buildings_data_opt {
        Some(data) => data,
        None => {
            // No buildings data loaded yet - skip card rendering. The
            // Build sub-tab will be empty until the data file loads.
            return;
        }
    };
    let category_idx = ui_state.selected_build_tab;
    let filter = ui_state.selected_filter;
    let multiplier = ui_state.build_multiplier;
    // v0.5.2 PR-A.2: thread the active colony's grid spare into each
    // card so the Power effect line can show "demand vs spare" with
    // a red ⚠ marker when the batch would push the grid into deficit.
    let spare_power_mw =
        compute_colony_spare_power_mw(&ui_state, &colonies, Some(&buildings_data));
    for (building_type, card_data) in visible_cards(
        &buildings_data,
        &research_state,
        category_idx,
        filter,
        multiplier,
        spare_power_mw,
    ) {
        // Look up the loaded + post-processed icon from `BuildingIcons`.
        // The handles got the white→transparent / dark→white treatment in
        // `process_building_icons` so the cards render the same theme-tinted
        // line-art as the menu icons. If the resource hasn't been
        // populated yet (e.g. startup race) we fall back to a default
        // `Handle<Image>` and the spawn_card helper renders a placeholder.
        let icon_handle: Option<&Handle<Image>> = building_icons
            .as_ref()
            .and_then(|icons| icons.handles.get(&building_type));
        // v0.5.2 PR-A.4 follow-up: thread the resource-icon
        // atlas through so the card body can render
        // `[PNG icon | tinted amount]` rows for each
        // `ResourceCostRow`. Empty atlas is fine for the
        // first frame after startup — the per-frame
        // post-processor catches up on the next tick.
        let empty_resource_icons = ResourceIcons::default();
        let resource_icons_ref: &ResourceIcons = resource_icons
            .as_ref()
            .map(|r: &Res<ResourceIcons>| -> &ResourceIcons { r.as_ref() })
            .unwrap_or(&empty_resource_icons);
        spawn_card(
            &mut commands,
            card_grid,
            &card_data,
            building_type,
            &body_font,
            &body_font_medium,
            &mono_font,
            icon_handle,
            resource_icons_ref,
        );
    }

    // No bottom dock — the global in-game footer (status + speed control + date)
    // is rendered by the existing dashboard system and stays untouched. The
    // canary only renders the panel itself, not a duplicated footer.

    // Spawn the three non-Build sub-tab bodies. The Build body is the
    // card grid we just spawned (carries `ConstructionTabBody::Build`).
    // The visibility system (`tick_construction_body_visibility`) makes
    // exactly one of these visible at a time based on the active tab.
    // The body content is updated by `update_overview_body` /
    // `update_buildings_body` / `update_stockpiles_body` every frame
    // so the summary reflects the current selected colony.
    spawn_overview_body(
        &mut commands,
        root,
        &body_font,
        &body_font_medium,
        &mono_font,
    );
    spawn_buildings_body(
        &mut commands,
        root,
        &body_font_medium,
    );
    spawn_mining_body(
        &mut commands,
        root,
        &body_font,
    );

    // Spawn the QueuePanel root. Anchored to the right edge of the
    // canary, 360 px wide, full height. Hidden by default — the
    // `OPEN QUEUE` chip in the output_row toggles `QueuePanelState::open`,
    // and `tick_queue_panel_visibility` mirrors that into the entity's
    // `Visibility`. The body contains a header row + a scrollable
    // `Overflow::scroll_y()` column of `QueuePanelRow` entities managed
    // by `update_queue_panel`.

    // Construction hover tooltip — a single line of text pinned to
    // the bottom-left of the canary. Hidden by default; the
    // `tick_construction_tooltip` system flips it visible when the
    // player hovers a disabled Queue CTA and writes a "Need X more Y"
    // reason. ZIndex(3) lifts it above the queue panel (which uses
    // ZIndex(2)) so it stays readable even when the queue panel is
    // open. The container uses `align_items: Start` (inherited from
    // the canary root) so it hugs the left edge of the panel and
    // doesn't push the card grid down.
    let tooltip = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(SPACE_LG),
                left: Val::Px(SPACE_LG),
                // Wide enough for "Need 250.0k more Iron at x100" +
                // padding. The text node has its own width so the
                // container hugs it.
                padding: UiRect::all(Val::Px(SPACE_SM)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.008, 0.039, 0.094, 0.96)),
            BorderColor::all(CYAN_BORDER_STRONG),
            ZIndex(3),
            Visibility::Hidden,
            Name::new("construction_tooltip"),
        ))
        .id();
    commands.entity(root).add_child(tooltip);
    let tooltip_text = commands
        .spawn((
            Text::new(String::new()),
            TextFont {
                font: body_font.clone(),
                font_size: CAPTION_SIZE,
                ..default()
            },
            TextColor(ORANGE_ORE),
            Name::new("construction_tooltip_text"),
            ConstructionTooltipText,
        ))
        .id();
    commands.entity(tooltip).add_child(tooltip_text);

    // ── Resource-cost chip hover tooltip ─────────────────────────────
    // Singleton overlay (one per panel) parented to the
    // construction root. The per-chip `Pointer<Over>` /
    // `Pointer<Out>` observers update `ResourceCostHoverState`,
    // and the `update_resource_cost_tooltip` system reads that
    // resource each frame to position + populate this overlay
    // near the cursor — matching the body-hover tooltip pattern
    // from `src/ui/mod.rs::ui_hover_tooltip` and the
    // shipbuilding module tooltip from
    // `src/ui/shipbuilding_workspace.rs::update_shipbuilding_hover_tooltip`.
    //
    // Coordinate-frame note: the overlay is a child of the
    // canary root, which itself is offset by `top: 126.0` from
    // the window's top-left. Bevy's `Window::cursor_position()`
    // returns coords in **window** space, but `Node::left` /
    // `top` on absolute-positioned descendants are measured
    // against the parent node's content-area origin. The
    // position system (`update_resource_cost_tooltip`) must
    // subtract the canary-root `top` constant before setting
    // `overlay_node.top` — see the matching `CANARY_ROOT_TOP_PX`
    // constant in that system. Don't be tempted to "simplify"
    // that subtraction by changing the root's `top` to `0`:
    // the 126 px offset intentionally pushes the canary below
    // the top resource-bar chrome (`src/ui/resources_bar.rs`)
    // so the canary's own AppBar doesn't collide with the
    // global chrome.
    //
    // `display: Display::None` initially so the layout pass
    // ignores it. The system toggles `Flex`/`None` based on
    // whether any chip is currently hovered.
    //
    // Style mirrors the body hover tooltip: `TOOLTIP_BG`
    // fill, cyan `STATUS_INFO_BORDER` 1-px stroke, lg inner
    // margin (10 px), 4-px radius. ZIndex(20) lifts it above
    // the queue panel (ZIndex(2)) and the disabled-CTA
    // tooltip (ZIndex(3)).
    let chip_tooltip_overlay = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                display: Display::None,
                padding: UiRect::all(Val::Px(10.0)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(crate::ui::theme::Color::TOOLTIP_BG),
            BorderColor::all(crate::ui::theme::Color::STATUS_INFO_BORDER),
            ZIndex(20),
            ResourceCostTooltipOverlay,
            Name::new("resource_cost_tooltip_overlay"),
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new(String::new()),
                TextFont {
                    font: body_font.clone(),
                    font_size: CAPTION_SIZE,
                    ..default()
                },
                TextColor(TEXT_BODY),
                ResourceCostTooltipText,
                Name::new("resource_cost_tooltip_overlay_text"),
            ));
        })
        .id();
    commands.entity(root).add_child(chip_tooltip_overlay);

    let queue_panel = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(0.0),
                right: Val::Px(0.0),
                bottom: Val::Px(0.0),
                width: Val::Px(360.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(SPACE_LG)),
                row_gap: Val::Px(SPACE_SM),
                border: UiRect::left(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.012, 0.024, 0.047, 0.96)),
            BorderColor::all(CYAN_BORDER),
            ZIndex(2),
            Visibility::Hidden,
            Pickable::default(),
            Name::new("queue_panel"),
            QueuePanelRoot,
        ))
        .id();
    commands.entity(root).add_child(queue_panel);

    // Header row: title + close X chip.
    let queue_header = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                width: Val::Percent(100.0),
                column_gap: Val::Px(SPACE_MD),
                ..default()
            },
            Name::new("queue_panel_header"),
        ))
        .id();
    commands.entity(queue_panel).add_child(queue_header);

    let title = commands
        .spawn((
            Text::new("CONSTRUCTION QUEUE"),
            TextFont {
                font: body_font_medium.clone(),
                font_size: BODY_SIZE,
                ..default()
            },
            TextColor(CYAN),
            Node {
                flex_grow: 1.0,
                ..default()
            },
            Name::new("queue_panel_title"),
        ))
        .id();
    commands.entity(queue_header).add_child(title);

    // Close button (X).
    let close_btn = commands
        .spawn((
            Button,
            Node {
                display: Display::Flex,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                width: Val::Px(24.0),
                height: Val::Px(24.0),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(3.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
            BorderColor::all(CYAN_BORDER),
            Pickable::default(),
            Name::new("queue_panel_close"),
            QueuePanelClose,
        ))
        .id();
    commands.entity(queue_header).add_child(close_btn);
    let close_label = commands
        .spawn((
            Text::new("×"),
            TextFont {
                font: body_font_medium.clone(),
                font_size: BODY_SIZE,
                ..default()
            },
            TextColor(CYAN),
            Name::new("queue_panel_close_label"),
        ))
        .id();
    commands.entity(close_btn).add_child(close_label);

    // Body container — scrollable column. The `update_queue_panel`
    // system spawns one `QueuePanelRow` per `ConstructionProject`
    // filtered by the selected colony, and removes stale rows when
    // projects are cancelled or completed.
    //
    // v0.5.2 PR-A.5: `min_height: 0` is the same flex-overflow
    // fix as the mining/buildings content containers — without it,
    // the scroll container grows to fit its content and the wheel
    // handler's `max_y` is always 0. The `card_grid` (build tab)
    // has the same line with the same rationale.
    let queue_body = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                width: Val::Percent(100.0),
                flex_grow: 1.0,
                min_height: Val::Px(0.0),
                row_gap: Val::Px(SPACE_SM),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
            Name::new("queue_panel_body"),
            QueuePanelBody,
        ))
        .id();
    commands.entity(queue_panel).add_child(queue_body);
}

fn spawn_card(
    commands: &mut Commands,
    parent: Entity,
    data: &BuildCardData,
    building_type: BuildingType,
    body_font: &Handle<Font>,
    body_font_medium: &Handle<Font>,
    mono_font: &Handle<Font>,
    icon: Option<&Handle<Image>>,
    resource_icons: &ResourceIcons,
) -> Entity {
    let card = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                padding: UiRect {
                    // All four sides carry SPACE_LG, but the bottom
                    // needs extra padding to reserve vertical space
                    // for the absolute-positioned CTA. Without this,
                    // the flex content (subtitle, stats, hairline,
                    // effects, ETA) extends down to the card's bottom
                    // edge and overlaps the CTA. The reservation is
                    // `CTA_FOOTPRINT` (CTA_HEIGHT + SPACE_LG gap +
                    // SPACE_LG baseline padding) so the flex flow
                    // never crosses into the CTA's absolute-positioned
                    // area.
                    top: Val::Px(SPACE_LG),
                    right: Val::Px(SPACE_LG),
                    bottom: Val::Px(CTA_FOOTPRINT),
                    left: Val::Px(SPACE_LG),
                },
                row_gap: Val::Px(SPACE_SM),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(8.0)),
                // Fixed pixel width — enlarging the window adds
                // more cards per row rather than stretching the
                // individual cards. The card_grid parent uses
                // flex_wrap + column_gap, so cards lay out in
                // 320-px columns and overflow to the next row at
                // the viewport edge. `flex_shrink: 0` prevents the
                // flex container from compressing the cards when
                // the viewport is narrower than `320 + gap + 320`
                // (a single card would otherwise shrink to fit).
                width: Val::Px(320.0),
                flex_shrink: 0.0,
                // `min_height` (not `height`) so every card has the same
                // baseline Y for the Queue button (the CTA is pinned via
                // `position: absolute` at the bottom — see below). Using
                // `height` would clip the CTA on cards with longer
                // content (3+ effect bullets, long subtitles). The CTA
                // v0.5.2: fixed height (was `min_height: 244`).
                // `min_height` allowed the card to grow with content,
                // so the Mining tab's bottom row of cards (with
                // fewer effect lines) ended up shorter than the top
                // row — the row heights within a flex_wrap group
                // take the tallest card, so a 244-vs-280 px mix
                // left visible gaps. A fixed 244 + Overflow::clip
                // keeps every card the same shape; the 4-line
                // effect cap (in `build_mine_card_data`) ensures
                // the content fits without clipping the ETA.
                //
                // v0.5.2 PR-A.4 (card height bump): 244 px was too
                // short for buildings with more than 3 resource
                // demands. Each cost line is ~18 px including line
                // spacing, so a 6-cost building (rare but defined
                // in buildings.ron — e.g. Refinery has 4 costs,
                // ChemicalPlant has 5, SemiconductorFab has 6)
                // overflowed by ~30 px and clipped the ETA row.
                // Bump fixed height to 320 px (≈ +76 px ≈ 2 extra
                // rows) so every cost line fits with breathing room.
                // The icon-strip cost bullets (PR-A.4) also dropped
                // the resource-name column, so the vertical budget
                // per cost line is unchanged — only the *count* of
                // fitting lines grows. 320 px matches the card
                // width for a clean 1:1 aspect ratio.
                height: Val::Px(320.0),
                flex_grow: 0.0,
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(CARD_BG),
            BorderColor::all(CYAN_BORDER),
            // Outer drop shadow — gives the card a 3D lift off the backdrop.
            // Heavy (16 px blur) so it's clearly visible against the deep
            // space backdrop. The blur radius is the key parameter: a
            // 2-4 px blur disappears into the dark navy; 16 px stays
            // distinct.
            BoxShadow::new(
                Color::srgba(0.0, 0.0, 0.0, 0.6),
                Val::Px(0.0),
                Val::Px(4.0),
                Val::Px(2.0),
                Val::Px(16.0),
            ),
            // Pickable makes the whole card area pickable so
            // `tick_subtitle_marquee` can read the card's `Interaction`
            // and scroll the description on hover. The CTA keeps its
            // own `Pickable` for clicks — picking bubbles to the
            // deepest pickable entity, so hovering the CTA itself sets
            // `Interaction::Hovered` on the CTA (and not the card).
            // That's fine: marquee-on-hover is for the description area;
            // hovering the CTA signals "click intent" not "read intent".
            Pickable::default(),
            Name::new("build_card"),
            ConstructionCard {
                name: data.name.clone(),
            },
        ))
        .id();
    commands.entity(parent).add_child(card);

    // Inner top-edge highlight (1.5 px tall, full width, light cyan at
    // 80% alpha) — the "glass lift" effect. The lighter color + thicker
    // height makes the card read as 3D, with the top edge catching light.
    // The previous CYAN_RIM at 65% alpha was too subtle (read as a
    // straight line); this version is more pronounced but still feels
    // like a light reflection, not a hard border.
    let rim = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(0.0),
                // Inset 0.5 px so the rim doesn't overlap the 8-px corner
                // radius (avoids the "right angle" artifact at the corners).
                left: Val::Px(0.5),
                right: Val::Px(0.5),
                height: Val::Px(1.5),
                ..default()
            },
            BackgroundColor(CARD_TOP_HIGHLIGHT),
            Name::new("card_rim"),
        ))
        .id();
    commands.entity(card).add_child(rim);

    // Header row: icon + title + subtitle column. The row is a flex
    // row with the icon (16x16) on the left and the title_col on the
    // right. title_col has flex_grow: 1.0 so it fills the remaining
    // width.
    let header_row = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(SPACE_SM),
                ..default()
            },
            Name::new("card_header_row"),
        ))
        .id();
    commands.entity(card).add_child(header_row);

    // Card icon — 36×36, processed by `process_building_icons` (white
    // background → transparent, dark lines → white tintable). The
    // processing pipeline is the same one used by `MenuIcons` and
    // `ResearchIcons`, so the canary's building cards match the visual
    // language of the rest of the UI. The `ImageNode::color` field
    // tints the white pixels to CYAN at runtime, matching the active
    // accent color. If the icon handle is missing we fall back to a
    // cyan-tinted placeholder square (defensive).
    let card_icon = match icon {
        Some(handle) => commands
            .spawn((
                Node {
                    width: Val::Px(36.0),
                    height: Val::Px(36.0),
                    border_radius: BorderRadius::all(Val::Px(4.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BorderColor::all(CYAN_BORDER),
                ImageNode::new(handle.clone()).with_color(CYAN),
                Name::new("card_icon"),
            ))
            .id(),
        None => commands
            .spawn((
                Node {
                    width: Val::Px(36.0),
                    height: Val::Px(36.0),
                    border_radius: BorderRadius::all(Val::Px(4.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BorderColor::all(CYAN_BORDER),
                // v0.5.2: brighter placeholder so the icon slot
                // is clearly visible while the PNG loads. The old
                // 30% alpha was nearly invisible against the dark
                // navy card background — players thought the icon
                // was missing entirely.
                BackgroundColor(Color::srgba(0.373, 0.784, 0.847, 0.60)),
                Name::new("card_icon_placeholder"),
            ))
            .id(),
    };
    commands.entity(header_row).add_child(card_icon);

    // Title + subtitle column.
    let title_col = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                row_gap: Val::Px(SPACE_XS),
                ..default()
            },
            Name::new("card_title_col"),
        ))
        .id();
    commands.entity(header_row).add_child(title_col);

    let title = commands
        .spawn((
            Text::new(data.name.clone()),
            TextFont {
                font: body_font_medium.clone(),
                font_size: BODY_SIZE,
                ..default()
            },
            TextColor(CYAN),
            Name::new("card_title"),
        ))
        .id();
    commands.entity(title_col).add_child(title);

    // Subtitle container — wraps the text in an EXACT-height flex
    // column so the slot below it (the Queue CTA, hairline, etc.)
    // sits at a consistent Y position regardless of whether the
    // description wraps to one or two lines. Without an exact
    // height, the container grows with its content: a 2-line
    // subtitle pushes the Queue button ~14 px lower than a 1-line
    // subtitle and the card grid reads as uneven. `height: 28`
    // (not `min_height`) pins the slot exactly, and `Overflow::clip`
    // hides any third-line overflow rather than letting it squeeze
    // the ETA row.
    //
    // For descriptions that overflow horizontally (single long line
    // that won't wrap inside the container's pixel width), we mount
    // a horizontal marquee track inside the clip container: two
    // copies of the description back-to-back (no gap). The track
    // has `flex_shrink: 0` so the inner text nodes don't get
    // squished to fit — that would make them visually shorter than
    // the actual description and break the overflow detection. The
    // `SubtitleMarquee` marker carries the latest measured
    // text_width / container_width so `tick_subtitle_marquee` can
    // decide whether to drift the track left on card hover.
    let subtitle_clip = commands
        .spawn((
            Node {
                // Two-line reservation; `height` (not `min_height`)
                // is required here — `min_height` only enforces a
                // floor, so a 2-line description (28.8 px natural
                // height) would still grow the container and push
                // the Queue button. The clamp in
                // `clamp_subtitle_two_lines` keeps descriptions
                // inside the 2-line budget; `Overflow::clip` is the
                // safety net for stray longer strings. The marquee
                // track inside relies on this clip to mask the
                // off-screen half during the scroll animation.
                height: Val::Px(28.0),
                overflow: Overflow::clip(),
                ..default()
            },
            Name::new("card_subtitle_clip"),
        ))
        .id();
    commands.entity(title_col).add_child(subtitle_clip);

    // First copy. The text node is its own child of the track; the
    // track width is auto-driven by this child plus the second copy.
    // `ComputedNode` is read each frame in `tick_subtitle_marquee`
    // to detect overflow vs the clip container. Spawn this first
    // so we have its entity id for the marquee marker below.
    let subtitle_text_a = commands
        .spawn((
            Text::new(data.subtitle.clone()),
            TextFont {
                font: body_font.clone(),
                font_size: CAPTION_SIZE,
                ..default()
            },
            TextColor(TEXT_DIM),
            // Disable soft-wrapping so the description renders as a
            // single horizontal line. `tick_subtitle_marquee` compares
            // the rendered text width against the clip container width
            // and scrolls the track left/right when the line is wider
            // than the container. With wrapping enabled, long lines
            // wrap to 2 lines and never horizontally overflow — the
            // marquee never fires because there's nothing to scroll.
            // Hard-wrap on `\n` still works (we don't put any in
            // descriptions), but the player can read the full
            // description by hovering the card.
            TextLayout::new_with_no_wrap(),
            Name::new("card_subtitle_text"),
        ))
        .id();

    // Second copy — same text, same styling, zero gap. Together
    // these form a seamless loop: at `phase = 1.0` the track has
    // drifted left by exactly `text_width`, which puts copy B in
    // copy A's original position. Resetting phase to 0 is a no-op
    // visually.
    let subtitle_text_b = commands
        .spawn((
            Text::new(data.subtitle.clone()),
            TextFont {
                font: body_font.clone(),
                font_size: CAPTION_SIZE,
                ..default()
            },
            TextColor(TEXT_DIM),
            // Same no-wrap rationale as copy A above. Both copies
            // share the same string + layout, so their
            // `ComputedNode::content_size().x` is identical and the
            // marquee's overflow detection (which reads copy A)
            // matches the visible track width exactly.
            TextLayout::new_with_no_wrap(),
            Name::new("card_subtitle_text"),
        ))
        .id();

    // Horizontal marquee track. Sits at translation `(0, 0)` when the
    // text fits the container; on hover, `tick_subtitle_marquee`
    // animates `translation.x` leftward (oscillating `phase` 0→1→0)
    // so copy B slides into copy A's original position, then copy A
    // slides back. Two back-to-back copies (no gap) keep the loop
    // seamless — at `phase = 1.0` the visible content is identical
    // to `phase = 0.0`, so the release snap is invisible.
    let subtitle_track = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                // The two text copies are wider than the container
                // when the description overflows; `flex_shrink: 0`
                // stops flexbox from compressing them to fit, which
                // would defeat the whole marquee. Children are sized
                // by their content (auto-width) so the track's
                // overall width equals `text_width * 2`.
                flex_shrink: 0.0,
                align_items: AlignItems::Center,
                ..default()
            },
            UiTransform::default(),
            SubtitleMarquee {
                // Pre-resolve the entities `tick_subtitle_marquee`
                // needs to query each frame. Doing it at spawn is
                // cheap (just entity handles) and removes the need
                // for `Children` walks in the hot loop.
                card,
                text_node: subtitle_text_a,
                clip_container: subtitle_clip,
                text_width: 0.0,
                container_width: 0.0,
                phase: 0.0,
            },
            Name::new("card_subtitle_track"),
        ))
        .id();
    commands.entity(subtitle_clip).add_child(subtitle_track);
    commands.entity(subtitle_track).add_child(subtitle_text_a);
    commands.entity(subtitle_track).add_child(subtitle_text_b);

    // Stats row: 2 stats (BP | COST) evenly spaced. Per user feedback
    // 2026-08-02, the legacy 3-stat layout duplicated the body's Power
    // effect line and confused the player. v0.5.2 PR-A.2 collapses
    // to a clean 2-col row; power lives in the body effects.
    let stats_row = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                width: Val::Percent(100.0),
                justify_content: JustifyContent::SpaceBetween,
                ..default()
            },
            Name::new("card_stats"),
        ))
        .id();
    commands.entity(card).add_child(stats_row);

    for (_label, value) in [&data.stat_a, &data.stat_b] {
        let stat = commands
            .spawn((
                Node {
                    align_items: AlignItems::Center,
                    ..default()
                },
                Text::new(value.clone()),
                TextFont {
                    font: mono_font.clone(),
                    font_size: CAPTION_SIZE,
                    ..default()
                },
                TextColor(CYAN),
                Name::new("card_stat"),
            ))
            .id();
        commands.entity(stats_row).add_child(stat);
    }

    // Hairline divider (below the stats row). The chip strip and
    // ETA row each add their own hairlines in their respective
    // blocks below; this one is the top-of-body separator.
    let hairline = commands.spawn(HairlineBundle::default()).id();
    commands.entity(card).add_child(hairline);

    // Effect bullets. v0.5.2 PR-A.6 (2026-08-03): bullet text
    // is rendered in `mono_font` (GeistMono) instead of
    // `body_font` (Inter Regular) so the lines read as a
    // continuation of the chip-strip font and the BP/COST
    // stat row above. Previously the Inter Regular body font
    // broke the visual rhythm — the bullets sat next to the
    // chip strip in two different fonts and the card body read
    // as two unrelated panels (per screenshot feedback
    // 2026-08-03). The colour map is unchanged.
    for (tone, line) in &data.effects {
        let color = match tone {
            EffectTone::Positive => GREEN_FIN,
            EffectTone::Negative => ORANGE_ORE,
            EffectTone::Neutral => TEXT_BODY,
            EffectTone::Cost => ORANGE_ORE,
            EffectTone::Throughput => GREEN_FIN,
        };
        let bullet = commands
            .spawn((
                Text::new(line.clone()),
                TextFont {
                    font: mono_font.clone(),
                    font_size: CAPTION_SIZE,
                    ..default()
                },
                TextColor(color),
                Name::new("effect_bullet"),
            ))
            .id();
        commands.entity(card).add_child(bullet);
    }

    // v0.5.2 PR-A.4 follow-up: rich resource-cost **chips**. Each
    // chip is a fixed-size `[PNG icon | tinted amount]` row
    // with a thin category-tinted border + low-alpha category
    // background so the player can identify the resource at a
    // glance and group related costs (Construction metals vs
    // Volatiles vs Precious metals …) by hue. Chips lay out in
    // a horizontal flex strip with wrapping, so a 6-cost
    // building occupies two rows of three chips instead of six
    // vertical rows — saves ~40 px of card height and makes
    // the chips read as a unified "this is what you pay" group.
    // The icon is the asset-server PNG from
    // `assets/textures/ui/resources/<name>.png`, post-processed
    // (white → transparent, dark → un-premultiplied alpha) and
    // tinted to the resource's category colour
    // (`bevy_theme::category_color_for_resource`). Falls back to
    // a tinted square for unknown resources or missing assets
    // (defensive — a future RON addition never panics).
    //
    // Amount format: `format_mining_reserve` already produces
    // `kg / t / Mt / Gt / Tt` with one- or two-decimal precision
    // depending on scale. Cost values in
    // `BuildingDefinition::resource_costs` are stored in **kt**
    // (kilotonne, 10⁶ t), so we pass the value in **Mt** (=
    // kt × 1000) to the formatter. Example:
    //   33 kt      → `format_mining_reserve(33 / 1000)` → `33.0 t`
    //   250 kt     → `format_mining_reserve(0.25)` → `250.0 t`
    //   16_700 kt  → `format_mining_reserve(16.7)` → `16.7 Mt`
    //   1_200_000 kt → `format_mining_reserve(1200)` → `1.20 Gt`
    //
    // The `/yr` suffix that was on the old `k/t` units is gone
    // — the cost row is a total build quantity, not a rate.
    let strip = if data.resource_costs.is_empty() {
        None
    } else {
        let s = commands
            .spawn((
                Node {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(6.0),
                    row_gap: Val::Px(4.0),
                    width: Val::Percent(100.0),
                    ..default()
                },
                Name::new("resource_cost_strip"),
            ))
            .id();
        commands.entity(card).add_child(s);
        Some(s)
    };

    // Hover tooltip for the cost chips. Spawned once at panel
    // setup time (see `setup_construction` — the
    // `ResourceCostTooltipOverlay` singleton), not per-card.
    // The chip's `Pointer<Over>` observer snapshots the chip
    // data into [`ResourceCostHoverState`], and the
    // `update_resource_cost_tooltip` system reads that
    // resource each frame to position + populate the overlay
    // near the cursor. Mirrors the body-hover tooltip
    // pattern in `src/ui/mod.rs::ui_hover_tooltip` and the
    // shipbuilding module-hover tooltip in
    // `src/ui/shipbuilding_workspace.rs::update_shipbuilding_hover_tooltip`.

    for cost in &data.resource_costs {
        let category = cost
            .resource
            .map(|r| category_color_for_resource(&r))
            .unwrap_or(TEXT_BODY);
        // kt → Mt for the formatter (1 Mt = 1000 kt). Costs in
        // `BuildingDefinition::resource_costs` are stored in
        // kt; `format_mining_reserve` expects Mt and produces
        // `kg / t / Mt / Gt / Tt` with auto-precision.
        let amount_str = format_mining_reserve(cost.amount / 1_000.0);

        // Chip: fixed 28-px height, category-tinted border +
        // 12% alpha category background. Parent is the wrap-
        // capable strip spawned once above the loop, so chips
        // row-break automatically on narrow cards. Carries
        // `Pickable` so the hover observers fire, and
        // `ResourceCostChip` so the observer knows which
        // resource this chip is. The two observers
        // (`on_chip_hover_over` / `on_chip_hover_out`) handle
        // the tooltip popup. Display name: prefer the parsed
        // `ResourceType::display_name` over the raw RON name
        // so `He-3` shows as `"Helium-3"`, `RareEarths` shows
        // as `"Rare Earths"`, etc.
        let display_name: String = cost
            .resource
            .map(|r| r.display_name().to_string())
            .unwrap_or_else(|| cost.name.clone());
        let chip = commands
            .spawn((
                Node {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(4.0),
                    padding: UiRect::horizontal(Val::Px(6.0)),
                    height: Val::Px(28.0),
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(4.0)),
                    ..default()
                },
                BackgroundColor(category.with_alpha(0.12)),
                BorderColor::all(category.with_alpha(0.35)),
                Pickable::default(),
                ResourceCostChip {
                    name: display_name,
                    amount: amount_str.clone(),
                    category,
                    card,
                },
                Name::new("resource_cost_chip"),
            ))
            .id();
        commands.entity(strip.unwrap()).add_child(chip);
        // Attach the hover observers. The chip's picked-id
        // is `chip`; the observer receives it via
        // `on.entity` in the event payload.
        commands.entity(chip).observe(on_chip_hover_over);
        commands.entity(chip).observe(on_chip_hover_out);

        // Icon. 20×20 — sits comfortably inside the 28-px chip
        // with 4 px horizontal padding. The chunkier 22-px-
        // stroke icon set (regenerated 2026-08-03) reads
        // cleanly at this size against the tinted background;
        // the post-processor's
        // `white → straight (un-premultiplied) alpha` step
        // means `ImageNode::color = category` lands at full
        // colour brightness instead of `α × category`.
        let icon_node = match cost
            .resource
            .and_then(|r| get_resource_icon_handle_bevy(resource_icons, r))
        {
            Some(handle) => commands
                .spawn((
                    Node {
                        width: Val::Px(20.0),
                        height: Val::Px(20.0),
                        ..default()
                    },
                    ImageNode::new(handle.clone()).with_color(category),
                    Name::new("resource_cost_chip_icon"),
                ))
                .id(),
            None => {
                // Unknown resource string OR the asset hasn't
                // loaded yet OR the PNG is malformed — fall back
                // to a small tinted square so the chip still
                // reads as a cost.
                let placeholder = commands
                    .spawn((
                        Node {
                            width: Val::Px(20.0),
                            height: Val::Px(20.0),
                            border_radius: BorderRadius::all(Val::Px(2.0)),
                            ..default()
                        },
                        BackgroundColor(category.with_alpha(0.85)),
                        Name::new("resource_cost_chip_icon_placeholder"),
                    ))
                    .id();
                placeholder
            }
        };
        commands.entity(chip).add_child(icon_node);

        let label = commands
            .spawn((
                Text::new(amount_str),
                TextFont {
                    font: mono_font.clone(),
                    font_size: CAPTION_SIZE,
                    ..default()
                },
                TextColor(category),
                Name::new("resource_cost_chip_label"),
            ))
            .id();
        commands.entity(chip).add_child(label);
    }

    // ETA hairline (v0.5.2 PR-A.6, 2026-08-03). Sits between the
    // chip strip / cost section and the ETA row so the player can
    // visually separate "what does this building cost?" from
    // "how long until it's built?". Mirrors the hairline below
    // the stats row so the card body reads as three labelled
    // zones: [stats | build requirements | ETA].
    let eta_hairline = commands.spawn(HairlineBundle::default()).id();
    commands.entity(card).add_child(eta_hairline);

    // ETA row — derived from `BuildDefinition::build_points × multiplier`
    // divided by the static placeholder output (12 001 BP/yr; the full
    // live recompute is gated on the queue panel + active colony wiring
    // in Phase C4). The batch-aware ETA makes the per-card progress
    // visible to the player when they pick x25 / x50 / x100.
    //
    // v0.5.2: uses the dedicated `build_points` field on the card
    // data instead of parsing it from `stat_a` — the Mining card's
    // `stat_a` carries the live inventory count (e.g. "×25"), not
    // BP, so the old parser would read 0 and the ETA would always
    // be "0s" on the Mining tab.
    //
    // v0.5.2 PR-A.6 (2026-08-03): the "ETA:" label now uses
    // `mono_font` (was `body_font`) so the dim "ETA:" prefix and
    // the yellow duration both render in GeistMono, matching the
    // bullet text and chip-strip amounts above. The mixed Inter
    // Regular + GeistMono pair read as a "two different fonts in
    // one row" mismatch.
    let unit_bp = data.build_points;
    let batch_bp = unit_bp * data.multiplier.max(1) as f64;
    let eta_seconds = batch_bp / 12_001.0 * 365.25 * 24.0 * 3600.0;
    let eta_str = format_duration_compact(eta_seconds);
    let eta_row = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                width: Val::Percent(100.0),
                padding: UiRect::horizontal(Val::Px(SPACE_XS)),
                column_gap: Val::Px(SPACE_SM),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
            Name::new("card_eta_row"),
        ))
        .id();
    commands.entity(card).add_child(eta_row);
    let eta_label = commands
        .spawn((
            Text::new("ETA: "),
            TextFont {
                font: mono_font.clone(),
                font_size: CAPTION_SIZE,
                ..default()
            },
            TextColor(TEXT_DIM),
            Name::new("card_eta_label"),
        ))
        .id();
    commands.entity(eta_row).add_child(eta_label);
    let eta_value = commands
        .spawn((
            Text::new(eta_str),
            TextFont {
                font: mono_font.clone(),
                font_size: CAPTION_SIZE,
                ..default()
            },
            TextColor(YELLOW_ETA),
            Name::new("card_eta_value"),
        ))
        .id();
    commands.entity(eta_row).add_child(eta_value);

    // Filled cyan CTA. Uses the `Button` widget so picking interaction
    // is wired automatically (Button has `#[require(Interaction)]`,
    // so the picking plugin auto-sets the hover/pressed state).
    //
    // The CTA is `position: absolute` and pinned to the bottom-left
    // corner of the card. This is the cleanest way to keep the Queue
    // button at the same Y position across cards with different content
    // heights (cards with 2 effect bullets vs 3+ effect bullets had
    // visibly different CTA positions when using `margin: top(Auto)`).
    // Absolute positioning removes the CTA from the flex column flow,
    // so it doesn't push the card content above it down. The CTA is
    // inset from the card edges by SPACE_LG (matching the card's own
    // padding) so it sits aligned with the title and stats row above.
    let cta = commands
        .spawn((
            Button,
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                align_self: AlignSelf::FlexStart,
                height: Val::Px(32.0),
                padding: UiRect::horizontal(Val::Px(SPACE_XL)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                // Pin to the bottom-left of the card. The card's
                // `Overflow::clip` ensures the CTA never bleeds outside
                // the card border, even if the absolute Y is bigger than
                // the card's natural content height.
                position_type: PositionType::Absolute,
                bottom: Val::Px(SPACE_LG),
                left: Val::Px(SPACE_LG),
                ..default()
            },
            BackgroundColor(CTA_FILL),
            BorderColor::all(CYAN_BORDER_STRONG),
            Name::new("card_cta"),
            ConstructionCta {
                building_type,
            },
            // v0.5.2 PR-A.2 (round 2): when the batch's power demand
            // exceeds the active colony's grid spare, disable the
            // Queue button at spawn time. The affordability system
            // (`tick_construction_cta_disabled`) re-checks the
            // resource gate every frame; the power gate is a
            // derived property of the card data and changes only
            // when the multiplier or spare changes (both trigger a
            // refresh), so a spawn-time check is enough.
            ConstructionCtaDisabled,
            // Make the CTA pickable so `Interaction::Pressed` fires.
            Pickable::default(),
        ))
        .id();
    if !data.power_insufficient {
        // Spawn-time default is "disabled" for the affordability
        // gate; if the batch fits the grid AND the player can
        // afford it, the per-frame affordability system removes
        // the marker. But for the power gate we make the decision
        // here at spawn time and never flip it.
        commands.entity(cta).remove::<ConstructionCtaDisabled>();
    }
    commands.entity(card).add_child(cta);

    // CTA label (text child needs flex_grow: 1.0 to participate in the parent's
    // justify-content: center — without it, text defaults to its own intrinsic
    // width and sits at the left edge).
    let cta_label = commands
        .spawn((
            Text::new(data.queue_label.clone()),
            TextFont {
                font: body_font_medium.clone(),
                font_size: BODY_SIZE,
                ..default()
            },
            TextColor(CYAN),
            Node {
                flex_grow: 1.0,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                display: Display::Flex,
                ..default()
            },
            Name::new("card_cta_label"),
        ))
        .id();
    commands.entity(cta).add_child(cta_label);

    // Return the card entity so callers (e.g. the Mining tab's
    // `spawn_mining_card` wrapper) can attach additional
    // siblings (body-gate caption, etc.) if they need to.
    card
}

/// Hover / click effect system for the Queue CTAs.
///
/// On hover: background brightens from CTA_FILL to CTA_FILL_HOVER, border goes
/// to fully-opaque cyan, and the entity scales up by 1.02.
/// On press: scale drops to 0.98.
/// On release: returns to default.
///
/// Disabled CTAs (marked `ConstructionCtaDisabled` by the can_afford
/// check) keep the dim CTA_FILL background and do not scale on hover —
/// the affine feedback would be misleading for a button that does
/// nothing on click. We use a `ParamSet` to read the same `ConstructionCta`
/// set twice (once for the visual loop, once to check the disabled
/// marker) without conflicting with the mutable accesses below.
///
/// **Flicker mitigation**: we track each CTA's previous
/// `(interaction, is_disabled)` pair in a `Local<HashMap>` and only
/// re-apply the visual when the pair changes. Without this, the
/// Queue button could visibly flicker when the disabled marker
/// toggles frame-to-frame (e.g. `ContextualStockpile` updates while
/// the cursor is hovering the button) — every frame would re-write
/// the same BackgroundColor / BorderColor / UiTransform, but the
/// cursor-mapped interaction state + the stock-driven disabled
/// state don't always settle, so the visual oscillates between
/// the dim (disabled) and bright (hovered) palettes. Edge-only
/// state changes get filtered out — a CTA that just had its
/// disabled marker flipped from `true` → `false` because the
/// player earned enough Iron now renders its hover affordance on
/// this frame instead of next.
pub fn tick_construction_cta_hover(
    mut params: ParamSet<(
        Query<
            (Entity, &Interaction, &mut BackgroundColor, &mut BorderColor, &mut UiTransform),
            With<ConstructionCta>,
        >,
        Query<Entity, With<ConstructionCtaDisabled>>,
    )>,
    mut prev_state: Local<std::collections::HashMap<Entity, (Interaction, bool)>>,
) {
    // Pre-compute the disabled set so the visual loop stays single-Q.
    let mut disabled_set: std::collections::HashSet<Entity> = std::collections::HashSet::new();
    for entity in params.p1().iter() {
        disabled_set.insert(entity);
    }
    for (entity, interaction, mut bg, mut border, mut ui_transform) in
        params.p0().iter_mut()
    {
        let is_disabled = disabled_set.contains(&entity);
        let prev = prev_state.get(&entity).copied();
        // Skip the write when the (interaction, is_disabled) pair is
        // identical to last frame — the visual is already correct.
        // First frame (`prev == None`) always writes so newly-spawned
        // CTAs get their initial paint.
        if let Some((prev_int, prev_disabled)) = prev {
            if prev_int == *interaction && prev_disabled == is_disabled {
                continue;
            }
        }
        match interaction {
            Interaction::Pressed if !is_disabled => {
                *bg = BackgroundColor(CTA_FILL_HOVER);
                *border = BorderColor::all(CYAN);
                ui_transform.scale = Vec2::splat(0.98);
            }
            Interaction::Hovered if !is_disabled => {
                *bg = BackgroundColor(CTA_FILL_HOVER);
                *border = BorderColor::all(CYAN);
                ui_transform.scale = Vec2::splat(1.02);
            }
            Interaction::None => {
                *bg = BackgroundColor(CTA_FILL);
                *border = BorderColor::all(CYAN_BORDER_STRONG);
                ui_transform.scale = Vec2::splat(1.00);
            }
            // Disabled + hover/pressed: keep the dim CTA_FILL background
            // and unfocus scale (1.0) — the click is a no-op so the
            // visual feedback would be misleading.
            _ => {
                *bg = BackgroundColor(CTA_FILL);
                *border = BorderColor::all(CYAN_BORDER_STRONG);
                ui_transform.scale = Vec2::splat(1.00);
            }
        }
        prev_state.insert(entity, (*interaction, is_disabled));
    }
}

/// Animate the `UiTransform.translation` of every `SubtitleMarquee`
/// track so an overflowing subtitle scrolls left/right on hover.
///
/// Two text copies live in the track; at `phase = 1.0` the visible
/// content is byte-identical to `phase = 0.0`, so the cycle is
/// seamless. Phase increments while the parent card is hovered
/// (rate: ~one full cycle every 2.4 s) and decrements back to zero
/// when hover ends, releasing the player back to copy A's start
/// position so they can re-read from the beginning.
///
/// Why this lives in `Update` and not `EguiPrimaryContextPass`:
/// `bevy_ui` native widgets are layout-driven (their picking and
/// hover state are computed by the engine in `Update`), so the
/// system has to read them on the same schedule the engine writes
/// them. The egui-pass restriction only applies to systems that
/// call egui context APIs.
///
/// B0001 audit: the system takes two `Query` parameters but they
/// reach for disjoint component sets
/// (`Interaction` on cards vs `UiTransform`+`SubtitleMarquee` on
/// tracks), so the planner treats them as parallel-friendly.
///
/// The first-frame issue: `ComputedNode` is populated by the
/// engine's layout pass *after* our spawn but before the next
/// `Update`. We read it on tick 2+. Until then `text_width` and
/// `container_width` stay at 0 and the marquee is dormant — which
/// is the correct visual (text at translation `(0, 0)`, no scroll).
pub fn tick_subtitle_marquee(
    time: Res<Time>,
    text_computed: Query<&ComputedNode>,
    mut tracks: Query<(Entity, &mut SubtitleMarquee, &mut UiTransform)>,
) {
    // Speed tuning: one full `0 → 1 → 0` cycle (the player's
    // perceived "scroll once across and back") takes ~4.0 s. The
    // marquee always rolls (no hover gate) so the player can read
    // the description without first hovering, but the slow cycle
    // keeps the grid visually calm — a single line drifting past
    // every few seconds reads as a hint, not a notification.
    const PHASE_PER_SECOND: f32 = 1.0 / 4.0;
    // Clamp delta — first frame after a long stall (eg debugger
    // pause) shouldn't snap phase to 1.0 in one tick.
    let dt = time.delta_secs().min(0.1);

    for (_track_entity, mut marquee, mut ui_transform) in tracks.iter_mut() {
        // 1) Refresh measured widths from `ComputedNode`. The text
        // node carries its measured content size (the rendered
        // pixel width of the wrapped description); the clip
        // container carries its allocated outer width.
        //
        // `ComputedNode` is populated by the engine's layout pass
        // before our `Update` tick, so reads on tick 1+ are valid.
        // On the very first frame after a card spawn both widths
        // could still be 0; the marquee correctly stays dormant
        // until next frame.
        let text_width = text_computed
            .get(marquee.text_node)
            .map(|c| c.content_size().x)
            .unwrap_or(0.0);
        let container_width = text_computed
            .get(marquee.clip_container)
            .map(|c| c.size().x)
            .unwrap_or(0.0);
        marquee.text_width = text_width;
        marquee.container_width = container_width;

        // 2) Detect overflow vs. fit. `text_width > container_width`
        // means one copy of the description is wider than the clip
        // container can show at translation `(0, 0)`. Below that
        // threshold the marquee is dormant: phase stays at 0,
        // translation stays at 0, no scroll. The +0.5 epsilon
        // absorbs 1-px measurement noise so short descriptions don't
        // oscillate on rounding.
        let overflows = text_width > container_width + 0.5;

        // 3) Drive the phase. Always rolls regardless of hover
        // state — the player can read the description without
        // moving the cursor. Phase ramps toward 1.0 at
        // `PHASE_PER_SECOND` (one cycle per 4.0 s). When the text
        // fits, or the card was despawned, phase ramps back to 0
        // at the same rate — a smooth reverse-scroll that resets
        // the visible copy to copy A's start position so the
        // player can re-read from the beginning.
        // The phase is clamped at both ends so a long pause that
        // would otherwise overshoot 1.0 just stops at the seam.
        if overflows {
            marquee.phase = (marquee.phase + dt * PHASE_PER_SECOND).min(1.0);
        } else {
            marquee.phase = (marquee.phase - dt * PHASE_PER_SECOND).max(0.0);
        }

        // 4) Apply the translation. At phase = 0 we sit at
        // translation `(0, 0)` (copy A in original position); at
        // phase = 1 we sit at `(-text_width, 0)` which puts copy B
        // exactly where copy A started — making the loop seam
        // invisible.
        let dx = -marquee.phase * text_width;
        ui_transform.translation = Val2::px(dx, 0.0);
    }
}

/// Wheel-scroll handler for `Overflow::scroll_y` containers.
///
/// Bevy 0.18 ships with no built-in scroll wheel handler for
/// `bevy_ui` — the engine renders scrollbars and clamps
/// `ScrollPosition` correctly, but it never reads `MouseWheel`
/// events to *update* `ScrollPosition`. Without this system, the
/// `card_grid` (and the queue panel body, etc.) silently ignore
/// the scroll wheel even when content overflows.
///
/// Strategy:
/// 1. For each `MouseWheel` event, look up the topmost hovered
///    entity via `HoverMap` (populated by the picking plugin).
/// 2. Walk up the parent chain until we find an entity with
///    `Overflow::scroll_y` set on its `Node`. That's our
///    scrollable.
/// 3. Adjust the scrollable's `ScrollPosition` by the wheel's
///    `y` delta (with a small multiplier for "snappy" feel) and
///    clamp to `[0, max_y]` where `max_y` is the difference
///    between `content_size().y` and `size().y`.
///
/// Why parent-walk and not iterate every scrollable: the user
/// usually has the cursor over a card, not the scrollable itself.
/// Cards live several parents deep inside `card_grid`. We can't
/// put `Pickable` on `card_grid` directly (it's not in the
/// hover-to-scroll contract) and we don't want to put picking
/// state on every card just to find the scrollable. The walk
/// is bounded by the panel depth (typically 4-5 hops) and only
/// runs on wheel events, not every frame.
///
/// Edge case: when the player hovers a card and scrolls, the
/// hover entity is the card. We walk up: card → `header_row` →
/// `title_col` → `subtitle_clip` (overflow: clip, not scroll) →
/// etc. The first ancestor with `Overflow::scroll_y` is the
/// `card_grid` itself. Good.
///
/// Drive the always-visible scrollbar overlay on the card grid.
///
/// For each `CardGridScrollbarTrack`, this system measures the
/// parent's (the `card_grid`'s) content height + visible height +
/// current `ScrollPosition` and resizes/repositions the thumb
/// proportionally. When the content fits the viewport the thumb
/// is hidden (height 0).
///
/// Why the system exists: Bevy 0.18's `bevy_ui` core auto-renders
/// scrollbar visuals only when content overflows and never exposes
/// an always-visible track — there's no API to say "show this
/// scrollbar even when not hovering, in this exact color, at this
/// exact width". The overlay node we spawned in
/// `setup_construction` is just a styled box; this system wires it
/// to the actual scroll position so the thumb moves with the
/// content.
///
/// B0001: we hold `&ComputedNode` on the grid + track and `&mut Node`
/// + `&mut UiTransform` on the thumb. Reading `&mut ScrollPosition`
/// would force a third `Query` for no benefit (we only read it),
/// but Bevy 0.18 still flags overlapping component reads on the
/// same entity as B0001. Solution: the grid is read here as a
/// plain data source (`ComputedNode` + `ScrollPosition` via
/// `&ScrollPosition` not `&mut ScrollPosition`) and the thumb
/// writes happen on a different archetype (different components).
/// To be belt-and-braces we wrap in a `ParamSet` so the planner
/// sees the disjoint access scopes explicitly.
pub fn tick_construction_scrollbar(
    // ParamSet gives the system disjoint mutable + immutable borrows
    // on the same grid without tripping B0001. `tracks` reads the
    // track node's measured height + size to scale the thumb; `grids`
    // reads each grid's content height + scroll position so we can
    // position the thumb. `thumbs` then writes the thumb's height
    // + Y top-offset.
    //
    // The two `With<CardGrid>` queries are read-only on disjoint
    // components (ComputedNode vs ScrollPosition) — but Bevy 0.18's
    // planner still flags overlapping access to the same entity's
    // archetype. We split them into the ParamSet so the access
    // scopes are explicit.
    mut params: ParamSet<(
        Query<&ComputedNode, With<CardGridScrollbarTrack>>,
        Query<&ComputedNode, With<CardGrid>>,
        Query<&ScrollPosition, With<CardGrid>>,
        Query<&mut Node, With<CardGridScrollbarThumb>>,
    )>,
    mut metrics: ResMut<ConstructionScrollbarMetrics>,
) {
    // We expect exactly one track and one grid, but loop to be safe
    // (the canary is a single-canary test panel so the count is
    // always 1). Read all inputs first so the ParamSet borrows drop
    // before we write — Bevy 0.18 still flags holding multiple
    // mutable borrows from one ParamSet as overlapping.
    let track_height: f32 = {
        let p0 = params.p0();
        p0.iter().next().map(|c| c.size().y).unwrap_or(0.0)
    };
    let (grid_size, grid_content_height, scroll_y) = {
        let p1 = params.p1();
        let Some(grid_computed) = p1.iter().next() else {
            return;
        };
        let grid_size = grid_computed.size();
        let grid_content_height = grid_computed.content_size().y;
        let scroll_y = {
            let p2 = params.p2();
            let Some(scroll_pos) = p2.iter().next() else {
                return;
            };
            scroll_pos.y
        };
        (grid_size, grid_content_height, scroll_y)
    };
    // Track pixel padding matches the spawn (`top: SPACE_SM,
    // bottom: SPACE_SM`), giving 16 px of unused vertical space
    // total. Subtract twice the SM constant from the track height
    // so the thumb math uses the actual available space. SPACE_SM is
    // project-private to `bevy_theme`; we use a literal here to avoid
    // a cross-module constant dep.
    let usable_track = (track_height - 16.0).max(0.0);
    // If the content fits inside the viewport, hide the thumb
    // entirely. `grid_size.y == 0` means the layout hasn't ticked
    // yet — also hide the thumb so the player doesn't see a
    // 0-height ghost thumb.
    if grid_content_height <= grid_size.y || grid_size.y <= 0.0 {
        for mut node in params.p3().iter_mut() {
            node.height = Val::Px(0.0);
            node.top = Val::Px(0.0);
        }
        metrics.usable_track_height = usable_track;
        metrics.thumb_height = 0.0;
        metrics.max_scroll = 0.0;
        return;
    }
    let ratio = (grid_size.y / grid_content_height).clamp(0.0, 1.0);
    let thumb_height = (usable_track * ratio).max(12.0);
    let max_scroll_y = (grid_content_height - grid_size.y).max(0.0);
    let scroll_progress = if max_scroll_y > 0.0 {
        (scroll_y / max_scroll_y).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let thumb_y = scroll_progress * (usable_track - thumb_height);
    // Publish the layout numbers for the drag system. The drag
    // system runs the same math in reverse (thumb Y → scroll
    // position) and needs these numbers to translate pointer
    // movement to a grid scroll position. We write them every
    // frame even when nothing is pressed — they only cost a few
    // float assignments.
    metrics.usable_track_height = usable_track;
    metrics.thumb_height = thumb_height;
    metrics.max_scroll = max_scroll_y;
    // The thumb is `position_type: Absolute` with `top` /
    // `height` driven each frame. `top` is in parent-local pixels
    // (the track's own coordinate space, top-left origin).
    for mut node in params.p3().iter_mut() {
        node.height = Val::Px(thumb_height);
        node.top = Val::Px(thumb_y);
    }
}

/// Drag-to-scroll for the construction card grid scrollbar. When
/// the player presses the thumb (or the track) and drags the mouse,
/// translate the Y delta into a `ScrollPosition` change. The wheel
/// handler (`tick_ui_scroll_on_wheel`) already covers the scroll
/// wheel; this system covers click-and-drag.
///
/// Why this is its own system (not folded into
/// `tick_construction_scrollbar`): drag is event-driven (only
/// active while `Interaction::Pressed` on the thumb OR the
/// track) and needs a `MessageReader<CursorMoved>` to detect
/// pointer motion. Mixing event-driven work into the per-frame
/// visual tick system is fine in Bevy but a separate system keeps
/// each function's responsibility clear.
///
/// Two press surfaces share the same param:
/// 1. **Thumb**: dragging the thumb scrolls the grid (existing
///    behavior; the thumb's `Interaction` is `Pressed`).
/// 2. **Track empty area**: clicking on the track above/below
///    the thumb "page-jumps" the scroll so the thumb's center
///    lands at the click point — the standard Windows / macOS
///    scrollbar behavior. The track's `Interaction` is `Pressed`
///    and the cursor position is read from `RelativeCursorPosition`
///    on the track entity. After the page jump, the player can
///    keep dragging and the gesture continues as a thumb-drag
///    from the new position.
///
/// Why `ParamSet`: this system holds `Query<&mut Node>` on the
/// thumb (for the press-cursor feedback) and `Query<&mut
/// ScrollPosition>` on the grid (for the actual scroll update).
/// Bevy 0.18's planner treats these as overlapping component
/// access on different archetypes; we wrap in `ParamSet` to make
/// the disjoint access scopes explicit.
///
/// Why a `Resource` (not `Local<DragState>`): we need pe-press /
/// on-release observers to mutate the drag state on entities other
/// than the system that runs the per-frame drag. Bevy 0.18's
/// observer API exposes `Trigger<Pointer<Press>>` /
/// `Trigger<Pointer<Release>>` as entity-scoped events that fire
/// regardless of where the pointer goes after the press — that's
/// the only way to keep a drag "locked" to the originally-clicked
/// surface (the thumb is only 6 px wide, so a one-pixel slip would
/// otherwise drop `Interaction::Pressed` to `None` and the drag
/// would die immediately). Putting the state on a `Resource`
/// lets observers and the per-frame system share the same
/// authoritative state.
#[derive(Resource, Default)]
pub(crate) struct ScrollbarDragState {
    /// Whether a drag is currently in progress.
    pub(crate) active: bool,
    /// Whether the drag started on the empty track (page-jump)
    /// vs the thumb (handle-drag). The former snaps the page
    /// immediately on press; the latter scrolls continuously.
    pub(crate) started_on_track: bool,
    /// Y in the track's local space where the press happened.
    /// Used by the page-jump snap to position the thumb's
    /// `top` so the thumb's centre sits at the click point.
    pub(crate) press_track_y: f32,
}

/// On-press observer for the `CardGridScrollbarThumb`. Fires when
/// the user presses the thumb; sets `ScrollbarDragState.active`
/// and records that the drag started on the thumb (not the
/// track). The observer stays attached for the lifetime of the
/// thumb entity, so it survives every per-frame scrollbar
/// rebuild — the entity itself is the long-lived parent of the
/// visual thumb.
fn on_thumb_press(
    on: On<Pointer<Press>>,
    mut drag: ResMut<ScrollbarDragState>,
) {
    // Only the primary (left) mouse button initiates a drag.
    if on.event.button != PointerButton::Primary {
        return;
    }
    drag.active = true;
    drag.started_on_track = false;
    drag.press_track_y = 0.0;
}

/// On-release observer for the `CardGridScrollbarThumb`. Fires
/// when the user releases anywhere on the thumb (or, by observer
/// propagation, on any descendant). Clears the drag state.
fn on_thumb_release(
    on: On<Pointer<Release>>,
    mut drag: ResMut<ScrollbarDragState>,
) {
    if on.event.button != PointerButton::Primary {
        return;
    }
    drag.active = false;
    drag.started_on_track = false;
}

/// On-press observer for the `CardGridScrollbarTrack`. Fires
/// when the user presses the empty track area (not the thumb,
/// which is a child and would consume the press first). The
/// press position is captured in the track's local Y so the
/// per-frame drag system can apply the page-jump snap.
fn on_track_press(
    on: On<Pointer<Press>>,
    mut drag: ResMut<ScrollbarDragState>,
    track_query: Query<&RelativeCursorPosition, With<CardGridScrollbarTrack>>,
) {
    if on.event.button != PointerButton::Primary {
        return;
    }
    drag.active = true;
    drag.started_on_track = true;
    // `RelativeCursorPosition.normalized` is `None` on the very
    // first frame after the track is spawned. Fall back to 0.0
    // (top of track) so the page-jump math doesn't divide by
    // zero or land on an undefined value.
    let y = track_query
        .get(on.entity)
        .ok()
        .and_then(|rcp| rcp.normalized)
        .map(|n| n.y)
        .unwrap_or(0.0);
    drag.press_track_y = y;
}

/// On-release observer for the `CardGridScrollbarTrack`. Mirrors
/// `on_thumb_release` — clears the drag state when the pointer
/// button is released.
fn on_track_release(
    on: On<Pointer<Release>>,
    mut drag: ResMut<ScrollbarDragState>,
) {
    if on.event.button != PointerButton::Primary {
        return;
    }
    drag.active = false;
    drag.started_on_track = false;
}

pub fn tick_construction_scrollbar_drag(
    mut cursor_events: MessageReader<CursorMoved>,
    mut grid_query: Query<&mut ScrollPosition, With<CardGrid>>,
    metrics: Res<ConstructionScrollbarMetrics>,
    mut drag: ResMut<ScrollbarDragState>,
) {
    // 1) Fast-path: no drag in progress. Drop any stale
    //    `CursorMoved` events so they don't pile up while the
    //    user is just hovering.
    if !drag.active {
        cursor_events.clear();
        return;
    }

    let travel = (metrics.usable_track_height - metrics.thumb_height).max(1.0);
    let factor = metrics.max_scroll / travel;

    // 2) Page-jump snap: if the drag *started* on the empty
    //    track (not the thumb), the observer already captured
    //    the press Y in `drag.press_track_y`. Apply the snap
    //    exactly once and then suppress further snaps until
    //    the next fresh press.
    if drag.started_on_track {
        let click_y_px: f32 = drag.press_track_y * metrics.usable_track_height;
        let target_thumb_y: f32 = (click_y_px - metrics.thumb_height * 0.5).clamp(
            0.0,
            (metrics.usable_track_height - metrics.thumb_height).max(0.0),
        );
        let target_scroll: f32 = target_thumb_y * factor;
        if let Ok(mut pos) = grid_query.single_mut() {
            pos.y = target_scroll.clamp(0.0_f32, metrics.max_scroll);
        }
        // The page-jump snap already moved the scroll. Drop any
        // `CursorMoved` events that arrived in the same frame —
        // the player is likely still moving the cursor toward the
        // thumb and we don't want to double-apply that as a drag
        // delta on top of the snap.
        cursor_events.clear();
        // Suppress further snaps until the next fresh press.
        drag.started_on_track = false;
        return;
    }

    // 3) Continuous drag: each `CursorMoved.delta.y` translates
    //    into a `ScrollPosition` change. The conversion factor
    //    is `max_scroll / (usable_track - thumb_height)`: 1 pixel
    //    of pointer Y → `factor` pixels of scroll.
    //
    //    In Bevy 0.18, positive Y is *down*, so a positive
    //    `delta.y` (cursor moved down) means the scroll should
    //    increase (content scrolls up).
    for event in cursor_events.read() {
        // `CursorMoved::delta` is `Option<Vec2>` in Bevy 0.18
        // (None when the pointer is outside any window). Skip
        // events with no delta — the drag is anchored to the
        // press-start, so a missing delta just means the cursor
        // didn't actually move this frame.
        let Some(delta) = event.delta else { continue };
        let dy = delta.y;
        if let Ok(mut pos) = grid_query.single_mut() {
            pos.y = (pos.y + dy * factor).clamp(0.0, metrics.max_scroll);
        }
    }
}

/// Shared layout numbers from `tick_construction_scrollbar`. The
/// drag system needs to know the thumb's travel range and the
/// grid's max scroll to translate pixel deltas into scroll units.
/// We publish these every frame in a `Resource` rather than via a
/// shared system param because (a) Resources survive system
/// re-scheduling and (b) the drag system shouldn't have to
/// duplicate the visual tick's layout math.
#[derive(Resource, Default, Debug)]
pub struct ConstructionScrollbarMetrics {
    /// Track height minus the top + bottom padding (`SPACE_SM *
    /// 2`). This is the range of Y values the thumb can occupy.
    pub usable_track_height: f32,
    /// Current rendered height of the thumb. Constant between
    /// layout ticks; used to compute the thumb's travel range.
    pub thumb_height: f32,
    /// `content_size.y - size.y` for the card grid — the max
    /// scrollable distance in pixels. Zero when content fits.
    pub max_scroll: f32,
}

/// Wheel-scroll handler for `Overflow::scroll_y` containers.
///
/// Bevy 0.18 ships with no built-in scroll wheel handler for
/// `bevy_ui` — the engine renders scrollbars and clamps
/// `ScrollPosition` correctly, but it never reads `MouseWheel`
/// events to *update* `ScrollPosition`. Without this system, the
/// `card_grid` (and the queue panel body, etc.) silently ignore
/// the scroll wheel even when content overflows.
///
/// Strategy:
/// 1. For each `MouseWheel` event, look up the topmost hovered
///    entity via `HoverMap` (populated by the picking plugin).
/// 2. Walk up the parent chain until we find an entity with
///    `Overflow::scroll_y` set on its `Node`. That's our
///    scrollable.
/// 3. Adjust the scrollable's `ScrollPosition` by the wheel's
///    `y` delta (with a small multiplier for "snappy" feel) and
///    clamp to `[0, max_y]` where `max_y` is the difference
///    between `content_size().y` and `size().y`.
///
/// Why parent-walk and not iterate every scrollable: the user
/// usually has the cursor over a card, not the scrollable itself.
/// Cards live several parents deep inside `card_grid`. We can't
/// put `Pickable` on `card_grid` directly (it's not in the
/// hover-to-scroll contract) and we don't want to put picking
/// state on every card just to find the scrollable. The walk
/// is bounded by the panel depth (typically 4-5 hops) and only
/// runs on wheel events, not every frame.
///
/// Edge case: when the player hovers a card and scrolls, the
/// hover entity is the card. We walk up: card → `header_row` →
/// `title_col` → `subtitle_clip` (overflow: clip, not scroll) →
/// etc. The first ancestor with `Overflow::scroll_y` is the
/// `card_grid` itself. Good.
///
/// B0001 audit: the system has one `Query` parameter; no
/// dual-Query risk.
pub fn tick_ui_scroll_on_wheel(
    mut wheel_events: MessageReader<MouseWheel>,
    hover_map: Res<HoverMap>,
    // ParamSet lets us walk up the parent chain (immutable Node reads)
    // and then mutate the ScrollPosition on the found entity in the
    // same system — Bevy 0.18 forbids holding both `&Node` and
    // `&mut ScrollPosition` borrows on the same query, so we split
    // them across two `Query` parameters accessed in disjoint scopes.
    mut nodes: ParamSet<(
        Query<(Entity, &Node, &ComputedNode)>,
        Query<(Entity, &mut ScrollPosition, &ComputedNode)>,
    )>,
    parents: Query<&ChildOf>,
) {
    for event in wheel_events.read() {
        // Skip zero-delta events (some mice emit X-only scrolls).
        if event.y == 0.0 {
            continue;
        }
        // Find the topmost hovered entity for the default mouse
        // pointer. Other pointers (touch / pen / gamepad) map to
        // their own PointerId and would need their own iteration;
        // the construction canary is mouse-only so this is fine.
        let pointer_id = PointerId::Mouse;
        let Some(hovered_entities) = hover_map.0.get(&pointer_id) else {
            continue;
        };
        if hovered_entities.is_empty() {
            continue;
        };
        let start_entity = *hovered_entities.keys().next().expect("non-empty checked above");
        // Walk up the parent chain (immutable scope) looking for
        // the first ancestor whose `Overflow` is `OverflowAxis::Scroll`
        // on the y axis.
        let mut cursor = start_entity;
        let mut scrollable: Option<Entity> = None;
        loop {
            // Read-only pass via `p0()`. Holding this borrow ends
            // when we exit the `if let` block.
            if let Ok((_entity, node, _computed)) = nodes.p0().get(cursor) {
                if matches!(node.overflow.y, OverflowAxis::Scroll) {
                    scrollable = Some(cursor);
                    break;
                }
            }
            let Ok(parent) = parents.get(cursor) else { break };
            cursor = parent.0;
        }
        let Some(scrollable_entity) = scrollable else {
            continue;
        };
        // Mutating pass via `p1()`. The previous read-only borrow is
        // already released (loop ended without holding a reference
        // past the iteration), so `p1()` is free to take `&mut`.
        if let Ok((_, mut pos, computed)) = nodes.p1().get_mut(scrollable_entity) {
            let max_y = (computed.content_size().y - computed.size().y).max(0.0);
            let new_y = (pos.0.y - event.y * 24.0).clamp(0.0, max_y);
            pos.0.y = new_y;
        }
    }
}

/// currently-selected multiplier, with the current `ContextualStockpile`.
///
/// This is the canary's version of the egui panel's
/// `can_afford_resources_multiplied` gate. The CTA itself stays visible
/// (the player should see the building exists) but the click handler
/// (`tick_construction_cta_click`) skips the push when the marker is
/// present, and the hover system keeps the dim CTA_FILL background.
///
/// Runs every frame in `Update` — the math is O(buildings × costs) and
/// there are < 50 buildings so the cost is negligible. We don't gate
/// this on `resource_changed` because the stockpile can change while
/// the panel is open (mining, deliveries, etc.).
///
/// Uses `queue_silenced` (Bevy 0.18+) so the `insert` / `remove`
/// commands are dropped silently instead of panicking if the
/// target entity was despawned by an earlier system in the same
/// tick (e.g. `refresh_card_grid` despawns the old cards before
/// the new ones are spawned). The system ordering in the plugin
/// (`.after(refresh_card_grid)`) is the primary defence; the
/// silenced queue is the safety net.
pub fn tick_construction_cta_disabled(
    mut commands: Commands,
    buildings_data: Res<BuildingsData>,
    contextual: Res<crate::economy::ContextualStockpile>,
    ui_state: Res<ConstructionUiState>,
    ctas: Query<(Entity, &ConstructionCta, Has<ConstructionCtaDisabled>)>,
) {
    let multiplier = ui_state.build_multiplier.max(1);
    for (entity, cta, already_disabled) in ctas.iter() {
        let costs = buildings_data.resource_costs(&cta.building_type);
        let can_afford = can_afford_costs(contextual.as_ref(), costs, multiplier);
        if !can_afford && !already_disabled {
            commands.entity(entity).queue_silenced(InsertCtaDisabled);
        } else if can_afford && already_disabled {
            commands.entity(entity).queue_silenced(RemoveCtaDisabled);
        }
    }
}

/// `EntityCommand` that inserts `ConstructionCtaDisabled`. Used by
/// `tick_construction_cta_disabled` via `queue_silenced` so the
/// insert is dropped instead of panicking if the entity is already
/// despawned by the time the command applies.
struct InsertCtaDisabled;

impl bevy::ecs::system::EntityCommand for InsertCtaDisabled {
    fn apply(self, mut entity: bevy::ecs::world::EntityWorldMut) {
        entity.insert(ConstructionCtaDisabled);
    }
}

/// `EntityCommand` that removes `ConstructionCtaDisabled`. See
/// `InsertCtaDisabled` for the rationale.
struct RemoveCtaDisabled;

impl bevy::ecs::system::EntityCommand for RemoveCtaDisabled {
    fn apply(self, mut entity: bevy::ecs::world::EntityWorldMut) {
        entity.remove::<ConstructionCtaDisabled>();
    }
}

/// Inner helper: check whether the contextual stockpile can cover
/// `costs × multiplier`. Mirrors the egui panel's
/// `can_afford_resources_multiplied`. Costs with names that don't map
/// to a `ResourceType` (shouldn't happen, but defensive) are skipped.
fn can_afford_costs(
    contextual: &crate::economy::ContextualStockpile,
    costs: &[(String, f64)],
    multiplier: u32,
) -> bool {
    for (name, amount) in costs {
        let total_needed = amount * multiplier as f64;
        if let Some(rt) = crate::colony::data::parse_resource_type(name) {
            if contextual.get(&rt) < total_needed {
                return false;
            }
        }
    }
    true
}

/// Refresh the card grid: despawn all existing `ConstructionCard`
/// entities and re-spawn based on the current `ConstructionUiState`.
/// Runs whenever `ConstructionUiState` changes (via chip clicks).
pub fn refresh_card_grid(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    buildings_data: Res<BuildingsData>,
    research_state: Res<ResearchState>,
    ui_state: Res<ConstructionUiState>,
    building_icons: Option<Res<BuildingIcons>>,
    resource_icons: Option<Res<ResourceIcons>>,
    card_query: Query<Entity, With<ConstructionCard>>,
    grid_query: Query<Entity, With<CardGrid>>,
    colonies: Query<(Entity, &crate::colony::Colony)>,
) {
    // Despawn all existing cards.
    //
    // `try_despawn` so the per-frame loop is silent if any card was
    // already cascade-despawned by an earlier system in the same tick
    // (e.g. tab visibility switching). The query is freshly evaluated
    // every invocation so this is defensive rather than strictly
    // required here, but it keeps the construction canary's body
    // sweep consistent with the four body-update systems below.
    for entity in card_query.iter() {
        commands.entity(entity).try_despawn();
    }
    // Find the card grid (there should be exactly one).
    let Ok(card_grid) = grid_query.single() else { return; };
    // Re-spawn based on the current state.
    let body_font = asset_server.load("fonts/Inter-Regular.otf");
    let body_font_medium = asset_server.load("fonts/Inter-SemiBold.otf");
    let mono_font = asset_server.load("fonts/GeistMono-Medium.ttf");
    let category_idx = ui_state.selected_build_tab;
    let filter = ui_state.selected_filter;
    let multiplier = ui_state.build_multiplier;
    // v0.5.2 PR-A.2: thread the active colony's grid spare into each
    // card so the Power effect line can show "demand vs spare" with
    // a red ⚠ marker when the batch would push the grid into deficit.
    let spare_power_mw =
        compute_colony_spare_power_mw(&ui_state, &colonies, Some(&buildings_data));
    for (building_type, card_data) in visible_cards(
        &buildings_data,
        &research_state,
        category_idx,
        filter,
        multiplier,
        spare_power_mw,
    ) {
        // Look up the loaded + post-processed icon from `BuildingIcons`.
        // The same icon-resource lookup the setup-time spawn uses —
        // keeps the rendered cards identical between startup and refresh.
        let icon_handle: Option<&Handle<Image>> = building_icons
            .as_ref()
            .and_then(|icons| icons.handles.get(&building_type));
        // v0.5.2 PR-A.4 follow-up: thread the resource-icon
        // atlas through so the card body can render
        // `[PNG icon | tinted amount]` rows for each
        // `ResourceCostRow`. Empty atlas is fine for the
        // first frame after startup — the per-frame
        // post-processor catches up on the next tick.
        let empty_resource_icons = ResourceIcons::default();
        let resource_icons_ref: &ResourceIcons = resource_icons
            .as_ref()
            .map(|r: &Res<ResourceIcons>| -> &ResourceIcons { r.as_ref() })
            .unwrap_or(&empty_resource_icons);
        spawn_card(
            &mut commands,
            card_grid,
            &card_data,
            building_type,
            &body_font,
            &body_font_medium,
            &mono_font,
            icon_handle,
            resource_icons_ref,
        );
    }
}

/// Auto-select the first available colony if `selected_colony` is None
/// or points at a despawned entity. This makes the Queue button work
/// out of the box without requiring the player to manually pick a
/// colony from the dropdown, and gracefully recovers when the selected
/// colony has been removed (e.g. dissolved mid-game).
pub fn auto_select_first_colony(
    mut ui_state: ResMut<ConstructionUiState>,
    colonies: Query<Entity, With<crate::colony::Colony>>,
) {
    let needs_pick = match ui_state.selected_colony {
        None => true,
        Some(e) => colonies.get(e).is_err(),
    };
    if needs_pick {
        if let Some(first) = colonies.iter().next() {
            ui_state.selected_colony = Some(first);
        }
    }
}

/// Click handler for the "Active Colony" picker. Toggles
/// `ColonyDropdownState::open` so the floating dropdown appears /
/// disappears. Same rising-edge detection as the chip / CTA click
/// handlers. `Option<ResMut<…>>` defends against a missing-resource
/// startup race (see `tick_open_queue_chip_click`).
pub fn tick_colony_picker_click(
    interactions: Query<(Entity, &Interaction), (With<ColonyPicker>, With<Button>)>,
    mut state: Option<ResMut<ColonyDropdownState>>,
    mut prev: Local<std::collections::HashMap<Entity, Interaction>>,
) {
    let mut current: std::collections::HashMap<Entity, Interaction> =
        std::collections::HashMap::new();
    for (entity, interaction) in interactions.iter() {
        let prev_interaction = prev.get(&entity).copied().unwrap_or(Interaction::None);
        if *interaction == Interaction::Pressed
            && prev_interaction != Interaction::Pressed
        {
            if let Some(ref mut s) = state {
                s.open = !s.open;
            }
        }
        current.insert(entity, *interaction);
    }
    *prev = current;
}

/// Click handler for a single colony option inside the dropdown. When
/// the player presses an option, update `ConstructionUiState::selected_colony`
/// and close the menu. Same rising-edge detection as the other click
/// handlers. `Option<ResMut<…>>` defends against a missing-resource
/// startup race.
pub fn tick_colony_option_click(
    interactions: Query<(Entity, &Interaction, &ColonyDropdownOption), With<Button>>,
    mut ui_state: ResMut<ConstructionUiState>,
    mut state: Option<ResMut<ColonyDropdownState>>,
    mut prev: Local<std::collections::HashMap<Entity, Interaction>>,
) {
    let mut current: std::collections::HashMap<Entity, Interaction> =
        std::collections::HashMap::new();
    for (entity, interaction, option) in interactions.iter() {
        let prev_interaction = prev.get(&entity).copied().unwrap_or(Interaction::None);
        if *interaction == Interaction::Pressed
            && prev_interaction != Interaction::Pressed
        {
            ui_state.selected_colony = Some(option.colony_entity);
            if let Some(ref mut s) = state {
                s.open = false;
            }
        }
        current.insert(entity, *interaction);
    }
    *prev = current;
}

/// Toggle the colony dropdown menu visibility based on
/// `ColonyDropdownState::open`. Same inheritance pattern as
/// `tick_construction_body_visibility`: when open, use `Inherited`
/// so the menu inherits visibility from the canary root; when
/// closed, use `Hidden` to unconditionally hide it.
///
/// `Visibility::Inherited` is required (instead of `Visibility::Visible`)
/// because the parent `picker` has `Visibility::Inherited` from the
/// `build_header_stack`, which is itself `Visibility::Hidden` on
/// every non-Build tab. Using `Visible` here would defeat that gate
/// and leak the menu onto the Overview / Buildings / Stockpiles tabs.
pub fn tick_colony_dropdown_visibility(
    state: Option<Res<ColonyDropdownState>>,
    mut menu_query: Query<&mut Visibility, With<ColonyDropdownMenu>>,
) {
    let is_open = state.as_ref().map(|s| s.open).unwrap_or(false);
    let target = if is_open {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    for mut v in menu_query.iter_mut() {
        *v = target;
    }
}

/// Update the picker's value text every frame based on the active
/// selection. Mirrors the egui ComboBox's `selected_text` behavior:
/// show the colony name (plus population suffix), or a placeholder
/// when no colony is selected.
pub fn update_colony_picker_text(
    ui_state: Res<ConstructionUiState>,
    colonies: Query<(Entity, &crate::colony::Colony)>,
    mut text_query: Query<&mut Text, With<ColonyPickerText>>,
) {
    let label = match ui_state.selected_colony.and_then(|e| colonies.get(e).ok()) {
        Some((_, colony)) => format!(
            "{} ({})",
            colony.name,
            crate::colony::Colony::format_population(colony.population)
        ),
        None => "(no colony)".to_string(),
    };
    for mut text in text_query.iter_mut() {
        **text = label.clone();
    }
}

/// Refresh the colony dropdown menu every frame: keep one
/// `ColonyDropdownOption` row per live `Colony` entity, mutate text
/// and selection-state in place, and only spawn / despawn when the
/// set of colonies changes.
///
/// v0.5.2 pilot for the construction canary's
/// **spawn-once-update-many** refactor (see
/// `update_overview_queue` / `update_buildings_body` /
/// `update_mining_body` for the broader pattern). Rows persist
/// across `ColonyDropdownState::open` toggles and across tab
/// visibility changes — the `Local<HashMap<Entity, Entity>>` cache
/// is keyed by `colony_entity` so the system can identify which
/// rows to keep, which to mutate, and which to despawn.
///
/// Why not the previous `Local<Vec<Entity>>` + per-frame despawn /
/// respawn pattern? Two reasons:
/// 1. The previous pattern triggered Bevy 0.18's
///    `WARN bevy_ecs::error::handler: Encountered an error in
///    command ... Entity despawned` flood whenever a parent
///    content container was cascade-despawned mid-tick, because
///    the `Local` cache still held the now-stale child IDs. We
///    suppressed the warning with `try_despawn` but that was
///    treating the symptom, not the cause.
/// 2. Spawning ~5–10 row entities per frame is a measurable cost
///    on a canary panel that the player opens frequently.
///
/// Mutation strategy per row:
/// - `BackgroundColor` (row) — driven by `is_selected`
/// - `Text` (option-text child) — the colony name + population
/// - `TextColor` (option-text child) — driven by `is_selected`
pub fn refresh_colony_dropdown(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    colonies: Query<(Entity, &crate::colony::Colony)>,
    menu_query: Query<Entity, With<ColonyDropdownMenu>>,
    ui_state: Res<ConstructionUiState>,
    mut spawned_rows: Local<std::collections::HashMap<bevy::ecs::entity::Entity, bevy::ecs::entity::Entity>>,
    mut row_bg_query: Query<
        (&ColonyDropdownOption, &mut BackgroundColor),
        Without<ColonyDropdownOptionText>,
    >,
    mut text_query: Query<
        (&ChildOf, &mut Text, &mut TextColor),
        With<ColonyDropdownOptionText>,
    >,
) {
    let Ok(menu) = menu_query.single() else {
        return;
    };

    let body_font_medium: Handle<Font> = asset_server.load("fonts/Inter-SemiBold.otf");

    // Build the desired set: (colony_entity -> label) for the live
    // colonies, sorted by label so the menu order is stable across
    // re-renders.
    let mut live_colonies: Vec<(bevy::ecs::entity::Entity, String)> = colonies
        .iter()
        .map(|(e, c)| {
            (
                e,
                format!(
                    "{} ({})",
                    c.name,
                    crate::colony::Colony::format_population(c.population)
                ),
            )
        })
        .collect();
    live_colonies.sort_by(|a, b| a.1.cmp(&b.1));
    let live_keys: std::collections::HashSet<bevy::ecs::entity::Entity> =
        live_colonies.iter().map(|(e, _)| *e).collect();

    // 1. Despawn rows whose colony is gone. We use `try_despawn`
    //    defensively in case the row was cascade-despawned by an
    //    earlier system in the same tick (e.g. menu re-rooted
    //    mid-frame).
    let to_remove: Vec<bevy::ecs::entity::Entity> = spawned_rows
        .keys()
        .filter(|k| !live_keys.contains(k))
        .copied()
        .collect();
    for key in to_remove {
        if let Some(row_entity) = spawned_rows.remove(&key) {
            commands.entity(row_entity).try_despawn();
        }
    }

    // 2. Mutate existing rows in place: selection-state visuals
    //    and text content. Iterate the cached map; skip keys
    //    that no longer exist (their rows were just despawned).
    for (colony_entity, row_entity) in spawned_rows.iter() {
        let is_selected = ui_state.selected_colony == Some(*colony_entity);
        // Update the row's background (selection highlight).
        if let Ok((_, mut bg)) = row_bg_query.get_mut(*row_entity) {
            *bg = BackgroundColor(if is_selected {
                Color::srgba(0.196, 0.529, 0.612, 0.78)
            } else {
                Color::srgba(0.0, 0.0, 0.0, 0.0)
            });
        }
        // Update the option-text child: label + colour.
        let label = colonies
            .get(*colony_entity)
            .map(|(_, c)| {
                format!(
                    "{} ({})",
                    c.name,
                    crate::colony::Colony::format_population(c.population)
                )
            })
            .unwrap_or_else(|_| "(unknown)".to_string());
        let text_color = if is_selected { ACTIVE_CHIP_TEXT } else { TEXT_BODY };
        for (parent, mut text, mut color) in text_query.iter_mut() {
            if parent.0 == *row_entity {
                **text = label.clone();
                *color = TextColor(text_color);
                break;
            }
        }
    }

    // 3. Spawn rows for colonies we haven't seen before. We use
    //    `commands.entity(menu).add_child(row)` so the new row
    //    inherits the menu's `Visibility::Inherited` and `ZIndex`.
    for (colony_entity, label) in live_colonies {
        if spawned_rows.contains_key(&colony_entity) {
            continue;
        }
        let is_selected = ui_state.selected_colony == Some(colony_entity);
        let row = commands
            .spawn((
                Button,
                Node {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    width: Val::Percent(100.0),
                    height: Val::Px(22.0),
                    padding: UiRect::horizontal(Val::Px(SPACE_SM)),
                    border: UiRect::all(Val::Px(1.0)),
                    border_radius: BorderRadius::all(Val::Px(3.0)),
                    ..default()
                },
                BackgroundColor(if is_selected {
                    Color::srgba(0.196, 0.529, 0.612, 0.78)
                } else {
                    Color::srgba(0.0, 0.0, 0.0, 0.0)
                }),
                BorderColor::all(Color::NONE),
                Pickable::default(),
                Name::new("colony_dropdown_option"),
                ColonyDropdownOption { colony_entity },
            ))
            .id();
        commands.entity(menu).add_child(row);
        let text_color = if is_selected { ACTIVE_CHIP_TEXT } else { TEXT_BODY };
        let label_text = commands
            .spawn((
                Text::new(label),
                TextFont {
                    font: body_font_medium.clone(),
                    font_size: CAPTION_SIZE,
                    ..default()
                },
                TextColor(text_color),
                Name::new("colony_dropdown_option_text"),
                ColonyDropdownOptionText,
            ))
            .id();
        commands.entity(row).add_child(label_text);
        spawned_rows.insert(colony_entity, row);
    }
}

/// Update the canary's hover tooltip text every frame.
///
/// Scans every CTA's `Interaction` and `ConstructionCtaDisabled`
/// state. If the player is hovering a disabled CTA, populates
/// `ConstructionTooltipState` with the most-binding constraint:
/// "not enough energy" when the batch's power demand exceeds
/// the active colony's grid surplus (v0.5.2 PR-A.2 round 2), or
/// the most expensive resource shortfall otherwise.
///
/// Pure read of `tick_construction_cta_disabled`'s output — no
/// mutation of disabled state. Runs every frame so the tooltip
/// disappears the instant the cursor leaves a disabled button.
pub fn tick_construction_tooltip(
    ctas: Query<(&Interaction, &ConstructionCta, Has<ConstructionCtaDisabled>)>,
    buildings_data: Res<BuildingsData>,
    contextual: Res<crate::economy::ContextualStockpile>,
    colonies: Query<(Entity, &crate::colony::Colony)>,
    ui_state: Res<ConstructionUiState>,
    mut tooltip: ResMut<ConstructionTooltipState>,
) {
    let multiplier = ui_state.build_multiplier.max(1);

    // Pre-compute the active colony's grid surplus in MW so we can
    // detect power-disabled cards. Mirrors `compute_colony_spare_power_mw`.
    let active_colony_entity = ui_state.selected_colony;
    let spare_power_mw: f64 = active_colony_entity
        .and_then(|e| colonies.get(e).ok())
        .map(|(_, colony)| {
            let totals = calculate_colony_power_totals(colony, Some(&buildings_data));
            (totals.produced_watts - totals.consumed_watts) / 1_000_000.0
        })
        .unwrap_or(0.0);

    let mut best: Option<String> = None;
    for (interaction, cta, is_disabled) in ctas.iter() {
        if !is_disabled {
            continue;
        }
        if !matches!(interaction, Interaction::Hovered | Interaction::Pressed) {
            continue;
        }
        let def = match buildings_data.get(&cta.building_type) {
            Some(d) => d,
            None => continue,
        };

        // v0.5.2 PR-A.2 (round 2): power gate is the more
        // fundamental constraint — surface it first. If the
        // batch's total demand exceeds the grid surplus, the
        // tooltip says "not enough energy" regardless of
        // resource shortfalls.
        if def.power_demand_mw > 0.0 && spare_power_mw > 0.0 {
            let total_demand = def.power_demand_mw * multiplier as f64;
            if total_demand > spare_power_mw {
                best = Some("Not enough energy".to_string());
                break;
            }
        }

        // Resource shortfall (the existing behaviour).
        let costs = def.resource_costs.as_slice();
        let mut shortfall: Option<(String, f64)> = None;
        for (name, amount) in costs {
            let total_needed = amount * multiplier as f64;
            if let Some(rt) = crate::colony::data::parse_resource_type(name) {
                let have = contextual.get(&rt);
                if have < total_needed {
                    let missing = total_needed - have;
                    let dominated = match shortfall.as_ref() {
                        Some((_, current)) => missing > *current,
                        None => true,
                    };
                    if dominated {
                        shortfall = Some((name.clone(), missing));
                    }
                }
            }
        }
        if let Some((name, missing)) = shortfall {
            best = Some(format!(
                "Need {} more {} at \u{00d7}{}",
                format_compact_u64_fallback(missing),
                name,
                multiplier
            ));
            break;
        }
    }
    if let Some(text) = best {
        tooltip.text = text;
        tooltip.visible = true;
    } else {
        tooltip.text.clear();
        tooltip.visible = false;
    }
}

/// Format a f64 as a compact number string ("1.2k", "3.4M", etc.).
/// Falls back to a basic integer format if no compact helper is in
/// scope. The function exists so the tooltip system doesn't have to
/// pull in the legacy egui formatting utilities from
/// `src/ui/construction_panel.rs` (which are gated behind
/// `#[allow(dead_code)]` while the canary is in flight).
fn format_compact_u64_fallback(v: f64) -> String {
    let abs = v.abs();
    if abs >= 1.0e9 {
        format!("{:.1}B", v / 1.0e9)
    } else if abs >= 1.0e6 {
        format!("{:.1}M", v / 1.0e6)
    } else if abs >= 1.0e3 {
        format!("{:.1}k", v / 1.0e3)
    } else if (v - v.round()).abs() < 1e-6 {
        format!("{}", v as i64)
    } else {
        format!("{:.1}", v)
    }
}

/// Mirror `ConstructionTooltipState` to the on-screen tooltip Text
/// node + its visibility every frame. The text node is parented to
/// the canary root with absolute positioning at the bottom-left, so
/// it's always visible regardless of which sub-tab the player is on
/// (the canary root is itself gated by the Construction menu state).
pub fn update_construction_tooltip(
    state: Res<ConstructionTooltipState>,
    mut tooltip_query: Query<(&mut Text, &mut Visibility), With<ConstructionTooltipText>>,
) {
    let text = if state.visible {
        state.text.clone()
    } else {
        String::new()
    };
    let visibility = if state.visible {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    for (mut t, mut v) in tooltip_query.iter_mut() {
        **t = text.clone();
        *v = visibility.clone();
    }
}

/// Click handler: when a chip in the Build sub-tab is pressed, mutate
/// `ConstructionUiState` accordingly. The chip's `ChipKind` component
/// tells us what to do (set qty, set filter, set category, etc.).
///
/// This system is the "wiring" that connects the visual chips to the
/// real game state. Without it, the chips light up on hover but don't
/// actually do anything.
///
/// Note: bevy's `Interaction::Pressed` only stays `true` for one or two
/// frames while the user holds the button. We track each chip's
/// previous interaction in a `Local<HashMap>` to detect the rising
/// edge (None/Hovered → Pressed) so a fast click isn't missed.
pub fn tick_construction_chip_click(
    interactions: Query<(Entity, &Interaction, &ChipKind), With<Button>>,
    mut ui_state: ResMut<ConstructionUiState>,
    mut active: ResMut<ActiveChips>,
    mut prev: Local<std::collections::HashMap<Entity, Interaction>>,
) {
    let mut current: std::collections::HashMap<Entity, Interaction> =
        std::collections::HashMap::new();
    for (entity, interaction, kind) in interactions.iter() {
        let prev_interaction = prev.get(&entity).copied().unwrap_or(Interaction::None);
        if *interaction == Interaction::Pressed
            && prev_interaction != Interaction::Pressed
        {
            // Update BOTH the underlying UI state AND the ActiveChips
            // resource (the single source of truth for visual active state).
            match kind {
                ChipKind::Tab(idx) => {
                    ui_state.selected_tab = match idx {
                        0 => ConstructionTab::Overview,
                        1 => ConstructionTab::Buildings,
                        2 => ConstructionTab::Build,
                        _ => ConstructionTab::Mining,
                    };
                    active.tab = *idx;
                }
                ChipKind::Qty(n) => {
                    ui_state.build_multiplier = *n;
                    active.qty = *n;
                }
                ChipKind::Filter(f) => {
                    ui_state.selected_filter = *f;
                    // Filter f is unused now (we use category only) but
                    // keep the click handler wired for future use.
                }
                ChipKind::Category(idx) => {
                    ui_state.selected_build_tab = *idx;
                    active.category = *idx;
                }
                ChipKind::MiningQty(n) => {
                    // Mining tab qty chip. Routed to a separate
                    // `mining_build_multiplier` so the Build tab's
                    // qty and the Mining tab's qty don't cross-pollute
                    // (the player might want 1× on one and 50× on
                    // the other without their qty clicks fighting).
                    ui_state.mining_build_multiplier = *n;
                }
            }
        }
        current.insert(entity, *interaction);
    }
    *prev = current;
}

/// What a chip in the Build sub-tab does when pressed. Attached to
/// each `ChipButtonBundle` so the click handler can dispatch.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChipKind {
    /// Sub-tab at the given index (0=Overview, 1=Buildings, 2=Build,
    /// 3=Mining).
    Tab(usize),
    /// Build quantity multiplier (1, 5, 10, 25, 50, 100) on the
    /// Build tab.
    Qty(u32),
    /// Functional-role filter (All / Food / Power / etc.).
    Filter(BuildFilter),
    /// Build-category tab (0=Infrastructure, 1=Industry, 2=Logistics,
    /// ..., 7=Military, 8=All). v0.5.2 PR-A.2 (round 2): the
    /// Mining chip is removed from the Build tab (mines are
    /// managed in the dedicated Mining tab), so 9 → 8 chips.
    Category(usize),
    /// v0.5.2: Mining tab build quantity multiplier (1, 5, 10, 25,
    /// 50, 100). Separate from `Qty` because the Mining tab lives
    /// in a different visual state and shares the chip row with
    /// the Build tab's `Qty` chips. Click handler routes this to
    /// `ui_state.mining_build_multiplier` rather than
    /// `ui_state.build_multiplier`.
    MiningQty(u32),
}

/// Click handler: when the player presses the Queue button on a build
/// card, push `(selected_colony, building_type)` to
/// `PendingConstructionActions::start_construction`. The
/// `process_construction_actions` system in the colony module then
/// spawns a `ConstructionProject` for that building.
///
/// Same rising-edge detection as `tick_construction_chip_click`.
pub fn tick_construction_cta_click(
    interactions: Query<(Entity, &Interaction, &ConstructionCta), With<ConstructionCta>>,
    disabled: Query<&ConstructionCtaDisabled>,
    ui_state: Res<ConstructionUiState>,
    mut pending: ResMut<PendingConstructionActions>,
    mut prev: Local<std::collections::HashMap<Entity, Interaction>>,
) {
    let mut current: std::collections::HashMap<Entity, Interaction> =
        std::collections::HashMap::new();
    for (entity, interaction, cta) in interactions.iter() {
        let prev_interaction = prev.get(&entity).copied().unwrap_or(Interaction::None);
        if *interaction == Interaction::Pressed
            && prev_interaction != Interaction::Pressed
        {
            let Some(colony_entity) = ui_state.selected_colony else {
                current.insert(entity, *interaction);
                continue;
            };
            // Skip the push when the CTA is in the disabled state
            // (insufficient resources for the chosen multiplier — see
            // `tick_construction_cta_disabled`). The hover system also
            // bails on the disabled marker so the button doesn't brighten.
            if disabled.get(entity).is_ok() {
                current.insert(entity, *interaction);
                continue;
            }
            // Honor the player's chosen multiplier (x1 / x5 / x10 / x25 / x50 / x100).
            // Without this loop, clicking the chip group once would only enqueue
            // a single copy regardless of the active multiplier — the legacy
            // egui panel uses the same `for _ in 0..multiplier` pattern.
            let multiplier = ui_state.build_multiplier.max(1);
            for _ in 0..multiplier {
                pending.start_construction.push((colony_entity, cta.building_type));
            }
        }
        current.insert(entity, *interaction);
    }
    *prev = current;
}

// ── Queue Panel systems ──────────────────────────────────────────────

/// Click handler for the AppBar "OPEN QUEUE" chip. Toggles
/// `QueuePanelState::open`. Same rising-edge detection as the chip /
/// CTA click handlers. `ResMut<QueuePanelState>` is wrapped in
/// `Option<…>` so a missing-resource startup race (e.g. if the plugin
/// is added before `init_resource` runs) doesn't panic the canary.
pub fn tick_open_queue_chip_click(
    interactions: Query<(Entity, &Interaction), (With<OpenQueueChip>, With<Button>)>,
    mut state: Option<ResMut<QueuePanelState>>,
    mut prev: Local<std::collections::HashMap<Entity, Interaction>>,
) {
    let mut current: std::collections::HashMap<Entity, Interaction> =
        std::collections::HashMap::new();
    for (entity, interaction) in interactions.iter() {
        let prev_interaction = prev.get(&entity).copied().unwrap_or(Interaction::None);
        if *interaction == Interaction::Pressed
            && prev_interaction != Interaction::Pressed
        {
            if let Some(ref mut s) = state {
                s.open = !s.open;
            }
        }
        current.insert(entity, *interaction);
    }
    *prev = current;
}

/// Click handler for the QueuePanel close button. Sets
/// `QueuePanelState::open = false`. Defensive `Option<ResMut<…>>` (see
/// `tick_open_queue_chip_click` for rationale).
pub fn tick_queue_panel_close_click(
    interactions: Query<(Entity, &Interaction), (With<QueuePanelClose>, With<Button>)>,
    mut state: Option<ResMut<QueuePanelState>>,
    mut prev: Local<std::collections::HashMap<Entity, Interaction>>,
) {
    let mut current: std::collections::HashMap<Entity, Interaction> =
        std::collections::HashMap::new();
    for (entity, interaction) in interactions.iter() {
        let prev_interaction = prev.get(&entity).copied().unwrap_or(Interaction::None);
        if *interaction == Interaction::Pressed
            && prev_interaction != Interaction::Pressed
        {
            if let Some(ref mut s) = state {
                s.open = false;
            }
        }
        current.insert(entity, *interaction);
    }
    *prev = current;
}

/// Toggle the QueuePanel visibility based on `QueuePanelState::open`.
/// Defensive `Option<Res<…>>` (see `tick_open_queue_chip_click`).
pub fn tick_queue_panel_visibility(
    state: Option<Res<QueuePanelState>>,
    mut panel_query: Query<&mut Visibility, With<QueuePanelRoot>>,
) {
    let is_open = state.as_ref().map(|s| s.open).unwrap_or(false);
    // Same inheritance pattern as `tick_construction_body_visibility`:
    // when the panel is open, use `Inherited` so the panel inherits
    // its visibility from the canary root (which is itself gated by
    // the Construction menu state). When closed, use `Hidden` to
    // unconditionally hide the panel.
    let target = if is_open {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    for mut v in panel_query.iter_mut() {
        *v = target;
    }
}

/// Update the live AppBar queue summary text (`queue_value`) every frame
/// based on the selected colony's queued `ConstructionProject`s. The
/// legacy placeholder was a static "6d 2h" string; this version reads
/// real progress and ETA.
///
/// We compute the remaining time for each project as
/// `remaining_bp / (ConstructionProject::required / batch_seconds)`,
/// roughly equivalent to "how long until this row finishes". Summed
/// across the colony that's the queue ETA.
pub fn update_queue_summary(
    mut text_query: Query<&mut Text, With<QueuePanelSummaryText>>,
    ui_state: Res<ConstructionUiState>,
    projects: Query<&crate::colony::ConstructionProject>,
    output_bp_per_year: Res<ConstructionQueue>,
) {
    // Output rate (BP/yr) — the legacy placeholder used 12 001. Read
    // from the `ConstructionQueue` resource so the canary stays
    // self-contained.
    let bp_per_sec = (output_bp_per_year.output_bp_per_year
        / 365.25 / 24.0 / 3600.0)
        .max(1e-9);
    let total_remaining_bp: f64 = ui_state
        .selected_colony
        .map(|colony_entity| {
            projects
                .iter()
                .filter(|p| p.colony_entity == colony_entity)
                .map(|p| (p.required - p.progress).max(0.0))
                .sum()
        })
        .unwrap_or(0.0);
    let eta_seconds = total_remaining_bp / bp_per_sec;
    // Fixed-width zero-padded format so the "Queue: Xd HHh MMm SSs"
    // text doesn't dance left/right as digits change. The player
    // gets the same chip width every frame.
    let text = if total_remaining_bp <= 0.0 {
        "Empty Queue".to_string()
    } else {
        format_duration_padded(eta_seconds)
    };
    for mut t in text_query.iter_mut() {
        **t = text.clone();
    }
}

/// Diff-based queue row management. Each frame:
/// 1. Query `ConstructionProject` filtered by the selected colony.
/// 2. Compute the desired set of `project_entity` IDs.
/// 3. For each project entity not in the existing row map, spawn a new
///    `QueuePanelRow` with progress bar + ETA + cancel button.
/// 4. Despawn any row whose project entity is no longer in the desired set.
/// 5. Update progress / ETA on existing rows.
///
/// Run condition: every frame (the project set can change whenever the
/// player queues or cancels). The diff cost is O(projects + rows) which
/// is < 50 in the canary's scope.
pub fn update_queue_panel(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    buildings_data: Res<BuildingsData>,
    ui_state: Res<ConstructionUiState>,
    output_bp_per_year: Res<ConstructionQueue>,
    projects: Query<(Entity, &crate::colony::ConstructionProject)>,
    existing_rows: Query<(Entity, &QueuePanelRow)>,
    root_query: Query<Entity, With<QueuePanelRoot>>,
    body_query: Query<Entity, With<QueuePanelBody>>,
) {
    let Ok(panel_root) = root_query.single() else { return; };
    let _ = panel_root; // not needed directly; rows are added to body_root
    let Ok(body_root) = body_query.single() else { return; };

    // Desired set: (project_entity, project_data) for the selected colony.
    let desired: std::collections::HashMap<Entity, crate::colony::ConstructionProject> = ui_state
        .selected_colony
        .map(|colony_entity| {
            projects
                .iter()
                .filter(|(_, p)| p.colony_entity == colony_entity)
                .map(|(entity, p)| (entity, p.clone()))
                .collect()
        })
        .unwrap_or_default();

    // Existing rows: (project_entity -> row_entity).
    let mut existing: std::collections::HashMap<Entity, Entity> =
        std::collections::HashMap::new();
    for (row_entity, row) in existing_rows.iter() {
        existing.insert(row.project_entity, row_entity);
    }

    // Despawn rows whose project is gone.
    //
    // `try_despawn` keeps this loop warning-free if the row's parent
    // (`QueuePanelBody`) was cascade-despawned mid-tick (e.g. the
    // queue panel was just closed). The `existing` map is rebuilt
    // each frame so this is defensive rather than strictly required,
    // but it matches the silenced-despawn idiom used by the four
    // body-update systems in this file.
    for (project_entity, row_entity) in existing.iter() {
        if !desired.contains_key(project_entity) {
            commands.entity(*row_entity).try_despawn();
        }
    }

    // Spawn rows for new projects.
    for (project_entity, project) in desired.iter() {
        if existing.contains_key(project_entity) {
            continue;
        }
        let display_name = buildings_data
            .get(&project.building_type)
            .map(|d| d.display_name.as_str())
            .unwrap_or("(unknown)");
        let row = spawn_queue_row(
            &mut commands,
            body_root,
            *project_entity,
            display_name,
            project,
            &asset_server,
            &output_bp_per_year,
        );
        let _ = row;
    }
}

/// Update the ETA text on every existing queue row every frame. The
/// text is derived from the project's `progress` and `required` plus
/// the configured output rate. Without this per-frame update the
/// spawned text would be frozen at the spawn frame — the queue would
/// appear to never count down. This is the system that makes the
/// queue duration "live".
pub fn update_queue_row_eta(
    projects: Query<(Entity, &crate::colony::ConstructionProject)>,
    output_bp_per_year: Res<ConstructionQueue>,
    mut eta_text_query: Query<(&QueuePanelRowEta, &mut Text, &mut TextColor)>,
) {
    let bp_per_sec = (output_bp_per_year.output_bp_per_year
        / 365.25 / 24.0 / 3600.0)
        .max(1e-9);
    for (eta_marker, mut text, mut color) in eta_text_query.iter_mut() {
        let Some((_, project)) = projects
            .iter()
            .find(|(e, _)| *e == eta_marker.project_entity)
        else {
            continue;
        };
        let remaining_bp = (project.required - project.progress).max(0.0);
        let eta_seconds = remaining_bp / bp_per_sec;
        if project.awaiting_resources {
            **text = "⏳ Awaiting".to_string();
            *color = TextColor(ORANGE_ORE);
        } else if remaining_bp <= 0.0 {
            **text = "Done".to_string();
            *color = TextColor(GREEN_OK);
        } else {
            // Fixed-width padded format keeps the row width constant
            // as digits tick down. Matches the AppBar summary format.
            **text = format_duration_padded(eta_seconds);
            *color = TextColor(YELLOW_ETA);
        }
    }
}

/// Update the progress-bar fill width on every existing queue row
/// every frame so the bar tracks the project's `progress` toward
/// completion. Without this the bar would be frozen at the spawn
/// frame.
pub fn update_queue_row_progress(
    projects: Query<(Entity, &crate::colony::ConstructionProject)>,
    mut fill_query: Query<(&QueuePanelRowFill, &mut Node)>,
) {
    for (fill_marker, mut node) in fill_query.iter_mut() {
        let Some((_, project)) = projects
            .iter()
            .find(|(e, _)| *e == fill_marker.project_entity)
        else {
            continue;
        };
        node.width = Val::Percent(
            (project.progress_percent() as f32).clamp(0.0, 1.0) * 100.0,
        );
    }
}

/// Spawn a single row in the queue panel for a given
/// `ConstructionProject`. The row has:
/// - Header line: building name + ETA
/// - Progress bar: a 320 px wide track with a colored fill node whose
///   width is set to `progress_percent * 320` every frame
/// - Status: "⏳ Waiting for delivery" if `awaiting_resources`, else
///   the time-remaining label
/// - Cancel X button (right-aligned)
fn spawn_queue_row(
    commands: &mut Commands,
    parent: Entity,
    project_entity: Entity,
    display_name: &str,
    project: &crate::colony::ConstructionProject,
    asset_server: &Res<AssetServer>,
    output_bp_per_year: &Res<ConstructionQueue>,
) -> Entity {
    let body_font: Handle<Font> = asset_server.load("fonts/Inter-Regular.otf");
    let body_font_medium: Handle<Font> = asset_server.load("fonts/Inter-SemiBold.otf");
    let mono_font: Handle<Font> = asset_server.load("fonts/GeistMono-Medium.ttf");
    let _ = (body_font.clone(), body_font_medium.clone()); // suppress unused

    let row = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(SPACE_MD)),
                row_gap: Val::Px(SPACE_XS),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                width: Val::Percent(100.0),
                ..default()
            },
            BackgroundColor(CARD_BG),
            BorderColor::all(CYAN_BORDER),
            Name::new("queue_row"),
            QueuePanelRow { project_entity },
        ))
        .id();
    commands.entity(parent).add_child(row);

    // Header row: name + ETA.
    let header = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                width: Val::Percent(100.0),
                column_gap: Val::Px(SPACE_SM),
                ..default()
            },
            Name::new("queue_row_header"),
        ))
        .id();
    commands.entity(row).add_child(header);

    let name = commands
        .spawn((
            Text::new(display_name.to_string()),
            TextFont {
                font: body_font_medium.clone(),
                font_size: BODY_SIZE,
                ..default()
            },
            TextColor(CYAN),
            Node {
                flex_grow: 1.0,
                ..default()
            },
            Name::new("queue_row_name"),
        ))
        .id();
    commands.entity(header).add_child(name);

    // ETA: derived from the active output rate (same as the AppBar
    // summary). Format: "Xh Ym" for short queues, "Xd Yh" for long.
    let bp_per_sec = (output_bp_per_year.output_bp_per_year
        / 365.25 / 24.0 / 3600.0)
        .max(1e-9);
    let remaining_bp = (project.required - project.progress).max(0.0);
    let eta_seconds = remaining_bp / bp_per_sec;
    let eta_text = if project.awaiting_resources {
        "⏳ Awaiting".to_string()
    } else {
        format_duration_compact(eta_seconds)
    };
    let eta_color = if project.awaiting_resources {
        ORANGE_ORE
    } else {
        YELLOW_ETA
    };
    let eta = commands
        .spawn((
            Text::new(eta_text),
            TextFont {
                font: mono_font.clone(),
                font_size: CAPTION_SIZE,
                ..default()
            },
            TextColor(eta_color),
            Name::new("queue_row_eta"),
            QueuePanelRowEta { project_entity },
        ))
        .id();
    commands.entity(header).add_child(eta);

    // Cancel X button.
    let cancel = commands
        .spawn((
            Button,
            Node {
                display: Display::Flex,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                width: Val::Px(20.0),
                height: Val::Px(20.0),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
            BorderColor::all(CYAN_BORDER),
            Pickable::default(),
            Name::new("queue_row_cancel"),
            QueuePanelRowCancel { project_entity },
        ))
        .id();
    commands.entity(header).add_child(cancel);
    let cancel_label = commands
        .spawn((
            Text::new("×"),
            TextFont {
                font: mono_font.clone(),
                font_size: CAPTION_SIZE,
                ..default()
            },
            TextColor(CYAN),
            Name::new("queue_row_cancel_label"),
        ))
        .id();
    commands.entity(cancel).add_child(cancel_label);

    // Progress bar: track (dim) + fill (cyan, width = pct * track width).
    let track = commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Px(6.0),
                border_radius: BorderRadius::all(Val::Px(3.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.196, 0.529, 0.612, 0.30)),
            Name::new("queue_row_progress_track"),
        ))
        .id();
    commands.entity(row).add_child(track);
    let fill = commands
        .spawn((
            Node {
                width: Val::Percent(project.progress_percent().clamp(0.0, 1.0) * 100.0),
                height: Val::Percent(100.0),
                border_radius: BorderRadius::all(Val::Px(3.0)),
                ..default()
            },
            BackgroundColor(CYAN),
            Name::new("queue_row_progress_fill"),
            QueuePanelRowFill { project_entity },
        ))
        .id();
    commands.entity(track).add_child(fill);

    row
}

/// Marker component for the ETA text on a queue row. The
/// `update_queue_row_eta` system uses this to find the text node by
/// project entity without iterating every `Text` node in the world.
#[derive(Component)]
pub struct QueuePanelRowEta {
    pub project_entity: Entity,
}

/// Marker component for the progress-fill bar on a queue row. The
/// `update_queue_row_progress` system uses this to update the bar
/// width every frame as the project advances.
#[derive(Component)]
pub struct QueuePanelRowFill {
    pub project_entity: Entity,
}

/// Marker component for the `queue_value` text in the AppBar so the
/// `update_queue_summary` system can find it without iterating every
/// Text node.
#[derive(Component)]
pub struct QueuePanelSummaryText;

/// Click handler for the cancel button on each queue row. Pushes the
/// project entity to `PendingConstructionActions::cancel_construction`.
pub fn tick_queue_panel_row_cancel_click(
    interactions: Query<(Entity, &Interaction, &QueuePanelRowCancel), With<QueuePanelRowCancel>>,
    mut pending: ResMut<PendingConstructionActions>,
    mut prev: Local<std::collections::HashMap<Entity, Interaction>>,
) {
    let mut current: std::collections::HashMap<Entity, Interaction> =
        std::collections::HashMap::new();
    for (entity, interaction, cancel) in interactions.iter() {
        let prev_interaction = prev.get(&entity).copied().unwrap_or(Interaction::None);
        if *interaction == Interaction::Pressed
            && prev_interaction != Interaction::Pressed
        {
            pending.cancel_construction.push(cancel.project_entity);
        }
        current.insert(entity, *interaction);
    }
    *prev = current;
}

/// Marker component for the QueuePanel body container (the scrollable
/// column where rows are spawned). The `update_queue_panel` system
/// queries for this to find the parent for new rows.
#[derive(Component)]
pub struct QueuePanelBody;

/// Visibility system: shows / hides the canary root based on
/// `ActiveMenu.current == GameMenu::Construction`. The canary spawns at
/// startup (so the entities exist) and this system keeps visibility in
/// sync each frame.
pub fn tick_construction_state(
    active_menu: Res<ActiveMenu>,
    mut state: ResMut<ConstructionState>,
    mut root_query: Query<&mut Visibility, With<ConstructionRoot>>,
) {
    let should_be_on = matches!(active_menu.current, GameMenu::Construction);
    let is_on = *state == ConstructionState::On;

    if should_be_on && !is_on {
        *state = ConstructionState::On;
        for mut v in root_query.iter_mut() {
            *v = Visibility::Visible;
        }
    } else if !should_be_on && is_on {
        *state = ConstructionState::Off;
        for mut v in root_query.iter_mut() {
            *v = Visibility::Hidden;
        }
    }
}

/// Plugin: registers the Construction canary on `bevy_ui`.
pub struct ConstructionPlugin;

impl Plugin for ConstructionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ConstructionState>()
            .init_resource::<ConstructionQueue>()
            .init_resource::<ActiveChips>()
            // Queue panel state (open/closed) — required by
            // `tick_open_queue_chip_click`, `tick_queue_panel_close_click`,
            // and `tick_queue_panel_visibility`. Without this init the
            // first frame panics with "Resource does not exist" —
            // `init_resource` must run **before** any system that
            // references the resource is registered.
            .init_resource::<QueuePanelState>()
            // Colony dropdown state (open/closed) — required by
            // `tick_colony_picker_click`, `tick_colony_option_click`,
            // and `tick_colony_dropdown_visibility`. Without this init
            // the first frame panics with "Resource does not exist".
            .init_resource::<ColonyDropdownState>()
            // Tooltip state (text + visibility) — required by
            // `tick_construction_tooltip` and
            // `update_construction_tooltip`. The tooltip pops up
            // when the player hovers a disabled Queue CTA.
            .init_resource::<ConstructionTooltipState>()
            // Cost-chip hover state: written by the
            // `on_chip_hover_over` / `on_chip_hover_out` observers
            // attached to each `ResourceCostChip` entity, read by
            // `update_resource_cost_tooltip` to position and
            // populate the singleton overlay.
            .init_resource::<ResourceCostHoverState>()
            // Scrollbar layout cache: written by
            // `tick_construction_scrollbar` every frame, read by
            // `tick_construction_scrollbar_drag` to translate pointer
            // Y deltas to `ScrollPosition` changes without
            // recomputing the layout math.
            .init_resource::<ConstructionScrollbarMetrics>()
            // Drag state for the scrollbar: mutated by the
            // `On<Pointer<Press>>` / `On<Pointer<Release>>`
            // observers attached to the thumb and track entities
            // at spawn time, read by `tick_construction_scrollbar_drag`.
            // Using a `Resource` (not a `Local`) lets observers on
            // entity-scoped events share state with the per-frame
            // drag system. Initialised to `Default` (no drag in
            // progress); the observers set `active = true` on
            // pointer-button press and clear it on release.
            .init_resource::<ScrollbarDragState>()
            .add_systems(
                Startup,
                (
                    setup_construction.after(crate::colony::data::load_buildings),
                    // Load building icons after the buildings data is
                    // available. The icons themselves are async-loaded
                    // by the asset server; this just registers the
                    // `Handle<Image>` for every known building.
                    load_building_icons.after(crate::colony::data::load_buildings),
                ),
            )
            // Post-process the building icons every frame until each one
            // has been luminance-keyed once. The per-icon `processed` set
            // makes this a no-op after the first pass.
            .add_systems(Update, process_building_icons)
            .add_systems(Update, tick_construction_state)
            // Cost-chip hover tooltip: per-frame cursor-driven
            // placement + text/colour updates on the singleton
            // overlay. Reads `ResourceCostHoverState` (written by
            // the chip observers) and `Window::cursor_position()`;
            // runs every frame even when no chip is hovered so it
            // can set `Display::None` and clear stale state.
            .add_systems(Update, update_resource_cost_tooltip)
            .add_systems(Update, tick_construction_cta_hover)
            // Marquee: oscillate subtitle `UiTransform.translation.x`
            // when the description overflows horizontally. Reads
            // `ComputedNode` (populated by the engine's layout pass)
            // and the card's `Interaction`, so must run on `Update`
            // like every other UI tick system (the egui-pass
            // restriction only applies to systems that call egui
            // context APIs, which this doesn't).
            .add_systems(Update, tick_subtitle_marquee)
            // Always-visible scrollbar overlay: resizes / repositions
            // the thumb of the card-grid scrollbar track based on
            // the grid's `ScrollPosition` + content size. Bevy 0.18
            // has no always-on scrollbar option in `bevy_ui` core,
            // so this drives our custom overlay.
            .add_systems(Update, tick_construction_scrollbar)
            // Drag-to-scroll: while the thumb has Interaction::Pressed,
            // translate pointer Y deltas into ScrollPosition changes.
            // Runs in the same Update schedule as the visual tick; the
            // visual tick publishes layout numbers to
            // `ConstructionScrollbarMetrics` which this system reads.
            .add_systems(Update, tick_construction_scrollbar_drag)
            // Scroll wheel → `ScrollPosition` for any `Overflow::scroll_y`
            // container under the cursor. Bevy 0.18 has no built-in
            // wheel handler (only renders scrollbars + clamps the
            // position); without this the card_grid and queue panel
            // body silently ignore the wheel even when content
            // overflows.
            .add_systems(Update, tick_ui_scroll_on_wheel)
            // Click handler: pushes (colony, building) to
            // PendingConstructionActions when a Queue button is pressed.
            .add_systems(Update, tick_construction_cta_click)
            // Chip-button click handler: when a qty / filter / category /
            // tab chip is pressed, mutate `ConstructionUiState`
            // accordingly. Without this, the chips are visual-only.
            .add_systems(Update, tick_construction_chip_click)
            // Auto-select the first colony if none is picked yet.
            .add_systems(Update, auto_select_first_colony)
            // Refresh the card grid when the user clicks a chip
            // (which mutates `ConstructionUiState`). Must run **before**
            // `tick_construction_cta_disabled` so the disabled-system
            // doesn't grab an entity-id for a card that was just
            // despawned — Bevy would then panic with "Entity despawned"
            // when the queued `insert(ConstructionCtaDisabled)` applies.
            .add_systems(
                Update,
                refresh_card_grid.run_if(resource_changed::<ConstructionUiState>),
            )
            // Affordability gate: toggles the `ConstructionCtaDisabled`
            // marker on every CTA based on the player's
            // `ContextualStockpile` × multiplier. Runs every frame so
            // mining, deliveries, and stock changes flip the gate.
            // `chain()` ordering with `refresh_card_grid` above keeps
            // the disabled system from racing the despawn.
            .add_systems(
                Update,
                tick_construction_cta_disabled
                    .after(refresh_card_grid)
                    .after(auto_select_first_colony),
            )
            // Toggle sub-tab body visibility based on the active tab.
            // Runs every frame; the cost is one Visibility mutation per
            // body (4 bodies) and an early-return when the tab hasn't
            // changed, which is a no-op.
            .add_systems(Update, tick_construction_body_visibility)
            // Per-frame content updates for the non-Build sub-tab bodies.
            // These systems re-spawn (or re-write) the body content
            // each frame so the summary reflects the current selected
            // colony / buildings / stockpile state.
            .add_systems(
                Update,
                (
                    update_overview_body,
                    update_overview_queue,
                    update_buildings_body,
                    update_mining_body,
                    tick_mining_group_visibility,
                    // Demolish button: rising-edge click pushes a
                    // negative `mining_edits` entry; the per-frame
                    // disabled sync keeps the marker in lockstep
                    // with the current mine count (which can change
                    // via the Queue button, this very Demolish
                    // button, or any other system).
                    tick_mining_demolish_click,
                    tick_mining_demolish_disabled,
                ),
            )
            // Queue panel: open/close, summary, diff-based rows, and
            // per-frame ETA + progress-bar updates so the queue
            // counts down as time progresses.
            .add_systems(
                Update,
                (
                    tick_open_queue_chip_click,
                    tick_queue_panel_close_click,
                    tick_queue_panel_visibility,
                    update_queue_summary,
                    update_queue_panel,
                    update_queue_row_eta,
                    update_queue_row_progress,
                    tick_queue_panel_row_cancel_click,
                ),
            )
            // Active Colony dropdown: open/close the picker menu,
            // dispatch option clicks, refresh the row set every frame,
            // and keep the picker value text in sync with the
            // selection. Runs every frame so a colony founded later in
            // the session appears in the menu without a reload.
            //
            // Note: the dropdown does NOT carry a
            // `ConstructionTabBody::Build` marker, so
            // `tick_construction_body_visibility` never touches its
            // `Visibility` field. The dropdown's visibility is owned
            // exclusively by `tick_colony_dropdown_visibility` and the
            // picker chain so the two systems can't fight over the
            // same field.
            .add_systems(
                Update,
                (
                    tick_colony_picker_click,
                    tick_colony_option_click,
                    tick_colony_dropdown_visibility,
                    update_colony_picker_text,
                    refresh_colony_dropdown,
                ),
            )
            // Hover tooltip — surfaces "Need X more Y at ×N" when
            // the player hovers a disabled Queue CTA. Runs every
            // frame so the tooltip disappears the instant the cursor
            // leaves a disabled button. Ordered after
            // `tick_construction_cta_disabled` so it reads the
            // up-to-date disabled state.
            .add_systems(
                Update,
                (
                    tick_construction_tooltip.after(tick_construction_cta_disabled),
                    update_construction_tooltip,
                ),
            )
            // Chip-button hover and active-state overlay. The hover system
            // wins on hover/press, the overlay re-applies the active state on
            // the next frame for chips marked `ChipActive`.
            .add_systems(
                Update,
                (
                    tick_chip_button_hover,
                    tick_chip_button_active_overlay,
                    tick_active_chip_glow,
                )
                    .chain(),
            );
    }
}


// ── Mining tab helper spawn functions (v0.5.2 PR-A.2) ────────
//
// Called from `update_mining_body` to lay out the build-qty
// chip row, the 7 surface group sections (each header +
// collapsible body of cards), and the 1 orbital section
// (collapsible, 5 sub-groups of cards). Each card is spawned
// via `spawn_mining_card` below.

/// Spawn the build-qty chip row at the top of the Mining tab.
/// Mirrors the Build tab's qty row layout but uses
/// `ChipKind::MiningQty` so the click handler routes to
/// `ui_state.mining_build_multiplier`.
#[allow(clippy::too_many_arguments)]
fn spawn_mining_qty_row(
    commands: &mut Commands,
    parent: Entity,
    ui_state: &ConstructionUiState,
    body_font: &Handle<Font>,
    mono_font: &Handle<Font>,
    track: &mut dyn FnMut(Entity),
) {
    let row = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(SPACE_SM),
                padding: UiRect::all(Val::Px(SPACE_SM)),
                width: Val::Percent(100.0),
                ..default()
            },
            BackgroundColor(CARD_BG),
            Name::new("mining_qty_row"),
        ))
        .id();
    commands.entity(parent).add_child(row);
    track(row);

    let label = commands
        .spawn((
            Text::new("Build qty:"),
            TextFont {
                font: body_font.clone(),
                font_size: CAPTION_SIZE,
                ..default()
            },
            TextColor(TEXT_DIM),
            Name::new("mining_qty_label"),
        ))
        .id();
    commands.entity(row).add_child(label);

    for qty in MINING_QTY_CHIPS.iter() {
        let is_active = ui_state.mining_build_multiplier == *qty;
        let label_str: String = format!("×{}", qty);
        let chip = commands
            .spawn(ChipButtonBundle::new(label_str.as_str(), is_active))
            .id();
        commands.entity(chip).insert(ChipKind::MiningQty(*qty));
        commands.entity(row).add_child(chip);
        spawn_chip_text(
            commands,
            chip,
            &label_str,
            mono_font.clone(),
            is_active,
            14.0,
        );
        track(chip);
    }

    if ui_state.mining_build_multiplier > 1 {
        let hint = commands
            .spawn((
                Text::new(format!(
                    "Applies to +{}",
                    ui_state.mining_build_multiplier
                )),
                TextFont {
                    font: body_font.clone(),
                    font_size: CAPTION_SIZE,
                    ..default()
                },
                TextColor(TEXT_DIM),
                Node {
                    flex_grow: 1.0,
                    justify_content: JustifyContent::FlexEnd,
                    ..default()
                },
                Name::new("mining_qty_hint"),
            ))
            .id();
        commands.entity(row).add_child(hint);
        track(hint);
    }
}

/// Spawn one surface group section (header + collapsible body of
/// cards). Returns the outer container entity.
#[allow(clippy::too_many_arguments)]
fn spawn_mining_group_section(
    commands: &mut Commands,
    parent: Entity,
    group_id: MiningGroupId,
    group_label: &str,
    group_buildings: &[BuildingType],
    collapsed: bool,
    body_breathable: bool,
    body_type: Option<BodyType>,
    planet_resources: Option<&crate::economy::PlanetResources>,
    buildings_data: &BuildingsData,
    building_counts: &std::collections::HashMap<BuildingType, u32>,
    body_font: &Handle<Font>,
    body_font_medium: &Handle<Font>,
    mono_font: &Handle<Font>,
    multiplier: u32,
    resource_icons: &ResourceIcons,
    // v0.5.2 PR-A.5: looked up by `update_mining_body` from the
    // `BuildingIcons` resource. Each card fetches its own icon
    // (see the call inside the `for bt in group_buildings` loop)
    // so the per-card icon cost matches the build tab exactly.
    building_icons: &BuildingIcons,
) -> Entity {
    let group_container = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                width: Val::Percent(100.0),
                row_gap: Val::Px(SPACE_XS),
                padding: UiRect::all(Val::Px(SPACE_SM)),
                ..default()
            },
            BackgroundColor(CARD_BG),
            Name::new(format!("mining_group_{:?}", group_id)),
        ))
        .id();
    commands.entity(parent).add_child(group_container);

    let header = commands
        .spawn((
            Button,
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(SPACE_SM),
                padding: UiRect::all(Val::Px(SPACE_XS)),
                width: Val::Percent(100.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
            MiningGroupHeader { group_id },
            Name::new("mining_group_header"),
        ))
        .id();
    commands.entity(group_container).add_child(header);

    let chevron = if collapsed { "▶" } else { "▼" };
    let chevron_text = commands
        .spawn((
            Text::new(chevron),
            TextFont {
                font: body_font.clone(),
                font_size: BODY_SIZE,
                ..default()
            },
            TextColor(CYAN),
            Name::new("mining_group_chevron"),
        ))
        .id();
    commands.entity(header).add_child(chevron_text);

    let label_text = commands
        .spawn((
            Text::new(format!("{} ({})", group_label, group_buildings.len())),
            TextFont {
                font: body_font.clone(),
                font_size: SECTION_SIZE,
                ..default()
            },
            TextColor(CYAN),
            Node {
                flex_grow: 1.0,
                ..default()
            },
            Name::new("mining_group_label"),
        ))
        .id();
    commands.entity(header).add_child(label_text);

    let body_node = commands
        .spawn((
            Node {
                display: if collapsed {
                    Display::None
                } else {
                    Display::Flex
                },
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                align_items: AlignItems::FlexStart,
                column_gap: Val::Px(SPACE_SM),
                row_gap: Val::Px(SPACE_SM),
                width: Val::Percent(100.0),
                padding: UiRect::all(Val::Px(SPACE_XS)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
            MiningGroupBody { group_id },
            Name::new("mining_group_body"),
        ))
        .id();
    commands.entity(group_container).add_child(body_node);

    if !collapsed {
        for bt in group_buildings {
            // v0.5.2 PR-A.5: look up the per-building icon handle
            // so the mining card renders the same cyan-tinted
            // line-art as the Build tab. The `Option<&Handle<Image>>`
            // shape is identical to the build-tab's `icon_handle`
            // in `setup_construction` (line 3790 in the original).
            // `process_building_icons` ran the white→transparent /
            // dark→white pass on every handle in `BuildingIcons`,
            // so the icon is already in the same colour space as
            // `spawn_card` expects.
            let icon_handle: Option<&Handle<Image>> =
                building_icons.handles.get(bt);
            spawn_mining_card(
                commands,
                body_node,
                *bt,
                body_breathable,
                body_type,
                planet_resources,
                buildings_data,
                building_counts,
                body_font,
                body_font_medium,
                mono_font,
                icon_handle,
                multiplier,
                resource_icons,
            );
        }
    }

    group_container
}

/// Spawn the orbital section (collapsible, 5 non-collapsible
/// sub-groups). Returns the outer container entity.
#[allow(clippy::too_many_arguments)]
fn spawn_mining_orbital_section(
    commands: &mut Commands,
    parent: Entity,
    collapsed: bool,
    body_breathable: bool,
    body_type: Option<BodyType>,
    planet_resources: Option<&crate::economy::PlanetResources>,
    buildings_data: &BuildingsData,
    building_counts: &std::collections::HashMap<BuildingType, u32>,
    body_font: &Handle<Font>,
    body_font_medium: &Handle<Font>,
    mono_font: &Handle<Font>,
    multiplier: u32,
    resource_icons: &ResourceIcons,
    // v0.5.2 PR-A.5: see `spawn_mining_group_section` for the
    // rationale. Same per-card icon lookup, applied to the
    // orbital AutoMines.
    building_icons: &BuildingIcons,
) -> Entity {
    let total_orbital: usize = MINING_GROUPS_ORBITAL
        .iter()
        .map(|(_, b)| b.len())
        .sum();

    let container = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                width: Val::Percent(100.0),
                row_gap: Val::Px(SPACE_SM),
                padding: UiRect::all(Val::Px(SPACE_SM)),
                margin: UiRect::top(Val::Px(SPACE_MD)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.020, 0.060, 0.118, 0.85)),
            Name::new("mining_orbital_section"),
        ))
        .id();
    commands.entity(parent).add_child(container);

    let header = commands
        .spawn((
            Button,
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(SPACE_SM),
                padding: UiRect::all(Val::Px(SPACE_XS)),
                width: Val::Percent(100.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
            MiningGroupHeader {
                group_id: MiningGroupId::Helium3,
            },
            Name::new("mining_orbital_header"),
        ))
        .id();
    commands.entity(container).add_child(header);

    let chevron = if collapsed { "▶" } else { "▼" };
    let chevron_text = commands
        .spawn((
            Text::new(chevron),
            TextFont {
                font: body_font.clone(),
                font_size: BODY_SIZE,
                ..default()
            },
            TextColor(ORANGE_ORE),
            Name::new("mining_orbital_chevron"),
        ))
        .id();
    commands.entity(header).add_child(chevron_text);

    let label_text = commands
        .spawn((
            Text::new(format!(
                "ORBITAL MINES (body: Asteroid, Moon, GasGiant) ({})",
                total_orbital
            )),
            TextFont {
                font: body_font.clone(),
                font_size: SECTION_SIZE,
                ..default()
            },
            TextColor(ORANGE_ORE),
            Node {
                flex_grow: 1.0,
                ..default()
            },
            Name::new("mining_orbital_label"),
        ))
        .id();
    commands.entity(header).add_child(label_text);

    let body_node = commands
        .spawn((
            Node {
                display: if collapsed {
                    Display::None
                } else {
                    Display::Flex
                },
                flex_direction: FlexDirection::Column,
                width: Val::Percent(100.0),
                row_gap: Val::Px(SPACE_SM),
                padding: UiRect::all(Val::Px(SPACE_XS)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
            MiningOrbitalBody,
            Name::new("mining_orbital_body"),
        ))
        .id();
    commands.entity(container).add_child(body_node);

    if !collapsed {
        for (sub_label, sub_buildings) in MINING_GROUPS_ORBITAL {
            let sub_header = commands
                .spawn((
                    Text::new(format!("{} ({})", sub_label, sub_buildings.len())),
                    TextFont {
                        font: body_font.clone(),
                        font_size: CAPTION_SIZE,
                        ..default()
                    },
                    TextColor(TEXT_DIM),
                    Node {
                        width: Val::Percent(100.0),
                        padding: UiRect::vertical(Val::Px(SPACE_XS)),
                        ..default()
                    },
                    Name::new("mining_orbital_sub_header"),
                ))
                .id();
            commands.entity(body_node).add_child(sub_header);

            let sub_row = commands
                .spawn((
                    Node {
                        display: Display::Flex,
                        flex_direction: FlexDirection::Row,
                        flex_wrap: FlexWrap::Wrap,
                        column_gap: Val::Px(SPACE_SM),
                        row_gap: Val::Px(SPACE_SM),
                        width: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
                    Name::new("mining_orbital_sub_row"),
                ))
                .id();
            commands.entity(body_node).add_child(sub_row);

            for bt in *sub_buildings {
                // v0.5.2 PR-A.5: per-building icon lookup
                // (mirrors the surface-group call above). The
                // `BuildingIcons::handles` map is keyed by
                // `BuildingType` and is populated in
                // `load_building_icons` from `assets/data/buildings.ron`.
                let icon_handle: Option<&Handle<Image>> =
                    building_icons.handles.get(bt);
                spawn_mining_card(
                    commands,
                    sub_row,
                    *bt,
                    body_breathable,
                    body_type,
                    planet_resources,
                    buildings_data,
                    building_counts,
                    body_font,
                    body_font_medium,
                    mono_font,
                    icon_handle,
                    multiplier,
                    resource_icons,
                );
            }
        }
    }

    container
}

/// Build a `BuildCardData` for a mine / AutoMine. Mirrors
/// `card_data_with_multiplier` but uses the mine's
/// count/accessibility/production/reserve as the primary data,
/// and treats the body's body-gate as a separate disable reason
/// (the resulting Queue button is disabled with the same
/// `ConstructionCtaDisabled` marker the Build tab uses for
/// affordability; the tooltip system distinguishes via the
/// new `body_blocked` field on the card data).
///
/// v0.5.2 PR-A.2 round 2: reuses the Build card's `CardBundle`
/// styling so the Mining tab and Build tab have visually identical
/// card chrome. The body content is mine-specific (count,
/// accessibility, production, reserve) but the wrapper is the same.
///
/// The `multiplier` is the player's build-qty choice. Mines are
/// added to the construction queue (not direct inventory edits),
/// so the multiplier scales the costs the player pays at queue
/// time but the visible `count` on the card is the live inventory
/// count (no multiplier fold — the multiplier is for the next
/// build batch, not the existing inventory).
#[allow(clippy::too_many_arguments)]
pub fn build_mine_card_data(
    bt: BuildingType,
    def: &BuildingDefinition,
    count: u32,
    body_breathable: bool,
    body_type: Option<BodyType>,
    planet_resources: Option<&crate::economy::PlanetResources>,
    multiplier: u32,
) -> BuildCardData {
    let mult = multiplier.max(1) as f64;
    let card_data = compute_mining_card_data(def, planet_resources);
    let body_blocked = !crate::colony::data::building_is_available_on(
        def,
        Some(body_breathable),
        body_type,
    );

    // Stats row: count (left) + accessibility (right).
    let count_label = format!("\u{00d7}{}", count);
    let acc_label = if card_data.accessibility > 0.0 {
        format!("Acc: {:.0}%", card_data.accessibility * 100.0)
    } else {
        "Acc: -".to_string()
    };
    let bp_str = count_label;
    let cost_str = acc_label;

    // Effects: power (if any), production, reserve, costs.
    // v0.5.2 PR-A.5 (2026-08-02): mirrors the Build card's
    // PowerGeneration-aware power line. Mines typically consume
    // power (PowerGeneration modifier is not used on mining
    // buildings), but the same code path also runs for any
    // future mine-with-on-site-generator; in that case the
    // producer line should win over the demand line.
    let mut effects: Vec<(EffectTone, String)> = Vec::new();
    let power_output_gw_per_unit: f64 = def
        .modifiers
        .iter()
        .filter(|m| m.modifier_type == "PowerGeneration")
        .map(|m| m.value)
        .sum();
    if power_output_gw_per_unit > 0.0 {
        let per_unit_mw = power_output_gw_per_unit * 1_000.0;
        let total_mw = per_unit_mw * mult;
        let line = if mult > 1.0 {
            format!(
                "Produces {:.0} MW \u{00d7} {} = {:.0} MW",
                per_unit_mw, mult as u32, total_mw
            )
        } else {
            format!("Produces {:.0} MW", per_unit_mw)
        };
        effects.push((EffectTone::Positive, line));
    } else if def.power_demand_mw.abs() >= 0.01 {
        let per_unit = def.power_demand_mw;
        let line = if mult > 1.0 {
            format!(
                "Power: {:.0} MW \u{00d7} {} = {:.0} MW",
                per_unit,
                mult as u32,
                per_unit * mult
            )
        } else {
            format!("Power: {:.0} MW", per_unit)
        };
        effects.push((EffectTone::Throughput, line));
    }
    // Production: "X.X Mt/yr Iron" using the modifier's value.
    // v0.5.2 fix (2026-08-03): per user feedback, the Mining tab's
    // "Produces" line did NOT scale with the build multiplier while
    // every other value on the card (Power demand ×N, resource
    // costs, BP, …) did. The Build tab's
    // `card_data_with_multiplier` mirrors the Power-generation
    // pattern: show the per-unit rate, the multiplier, and the
    // batch total. Use `mult` (the build multiplier) rather than
    // `count` (already-built mines — that's the inventory tally
    // shown in `stat_a`), and fold accessibility into the
    // per-unit base so the ×N expansion reflects the player's full
    // batch contribution to the colony's output. The base per-mine
    // yield without accessibility is still shown as the "per unit"
    // figure (matches Build tab convention: per-unit, ×N, total).
    if let Some(prod) = def
        .modifiers
        .iter()
        .find(|m| m.modifier_type.ends_with("Production"))
    {
        if prod.value > 0.0 {
            if let Some(res_name) = prod.modifier_type.strip_suffix("Production") {
                let per_unit = prod.value * card_data.accessibility as f64;
                let total = per_unit * mult;
                let line = if mult > 1.0 {
                    format!(
                        "Produces {} {}/yr \u{00d7} {} = {} {}/yr",
                        format_mining_rate(per_unit),
                        res_name,
                        mult as u32,
                        format_mining_rate(total),
                        res_name
                    )
                } else {
                    format!("Produces {} {}/yr", format_mining_rate(per_unit), res_name)
                };
                effects.push((EffectTone::Positive, line));
            }
        }
    }
    // Reserve: "Res: 142.3 Gt" or "no deposit" / "Survey the body...".
    let reserve_label = if card_data.reserve_mt > 0.0 {
        format!("Res: {}", format_mining_reserve(card_data.reserve_mt))
    } else if planet_resources.is_none() {
        "Survey the body to see deposits".to_string()
    } else {
        "no deposit".to_string()
    };
    effects.push((EffectTone::Neutral, reserve_label));
    // Cost lines (top 6). v0.5.2 PR-A.4 follow-up: typed
    // `resource_costs` rows rendered with PNG icon + category
    // tint by the canary, not emoji text in `effects`. The
    // 6-line cap gives the card room for tall cost lists.
    let mut resource_costs: Vec<ResourceCostRow> = Vec::new();
    for (name, amt) in def.resource_costs.iter().take(6) {
        let total = amt * mult;
        resource_costs.push(ResourceCostRow {
            name: name.clone(),
            amount: total,
            resource: parse_resource_type(name),
        });
    }
    // Body-gate caption (only when blocked).
    if body_blocked {
        let gate_label = match def.allowed_body_types.first() {
            Some(bt_value) => format!("\u{26a0} body - requires {:?}", bt_value),
            None => "\u{26a0} body - unavailable".to_string(),
        };
        effects.push((EffectTone::Negative, gate_label));
    }

    // Power gate: if the batch's power demand exceeds the grid
    // spare, disable the Queue. For v0.5.2 PR-A.2 round 2 we
    // don't have spare_power_mw on the Mining tab side (the
    // Build tab threads it through; the Mining tab refresh path
    // is every-frame and doesn't compute spare). The Mining tab
    // gates only on body-blocked, which is the relevant
    // constraint for orbital / He-3 mines.
    let power_insufficient = body_blocked;

    BuildCardData {
        name: def.display_name.clone(),
        subtitle: clamp_subtitle_two_lines(&def.description),
        building_type: bt,
        icon: def.icon.clone(),
        multiplier: multiplier.max(1),
        stat_a: ("\u{00d7}N", bp_str),
        stat_b: ("ACC", cost_str),
        stat_c: ("", String::new()),
        effects,
        // v0.5.2 PR-A.4 follow-up: typed resource-demand rows
        // rendered with PNG icon + category tint. Always
        // passed alongside `effects`; the canary renders the
        // two sets in separate visual zones (Power → Produces
        // → Res → [resource_cost rows] → ⚠ gate).
        resource_costs,
        // v0.5.2: label the Queue button "Build +N" so the player
        // sees the batch size without glancing at the chip row.
        // The Demolish button ("Demolish ×N") already does this,
        // so the two read as a matched pair.
        queue_label: if multiplier > 1 {
            format!("Build \u{002b}{}", multiplier)
        } else {
            "Build +1".to_string()
        },
        // v0.5.2: ETA derivation. The Mining card's `stat_a` carries
        // the live inventory count (e.g. "×25"), not BP, so the ETA
        // row cannot parse the BP from the stat string. Pass it as a
        // dedicated field so the ETA shows real values for any
        // multiplier (was 0s for all Mining cards before this).
        build_points: def.build_points,
        power_insufficient,
    }
}

/// Spawn a single mine / AutoMine card. Reuses the Build card's
/// `CardBundle` style for visual parity with the Build tab. The
/// click handler is the same `tick_construction_cta_click` the
/// Build tab uses, so the Queue button pushes
/// `(colony, building_type)` to
/// `PendingConstructionActions::start_construction` and the player
/// gets a real queue entry with costs / ETA.
#[allow(clippy::too_many_arguments)]
fn spawn_mining_card(
    commands: &mut Commands,
    parent: Entity,
    bt: BuildingType,
    body_breathable: bool,
    body_type: Option<BodyType>,
    planet_resources: Option<&crate::economy::PlanetResources>,
    buildings_data: &BuildingsData,
    building_counts: &std::collections::HashMap<BuildingType, u32>,
    body_font: &Handle<Font>,
    body_font_medium: &Handle<Font>,
    mono_font: &Handle<Font>,
    icon: Option<&Handle<Image>>,
    multiplier: u32,
    resource_icons: &ResourceIcons,
) -> Entity {
    let def = match buildings_data.get(&bt) {
        Some(d) => d,
        None => {
            // Unknown building (shouldn't happen with a clean
            // RON). Render a placeholder card so the grid layout
            // doesn't break.
            let card = commands
                .spawn((
                    Node {
                        display: Display::Flex,
                        flex_direction: FlexDirection::Column,
                        width: Val::Px(320.0),
                        // v0.5.2 PR-A.4: match the main Build card's
                        // bumped height (320 px) so unknown / placeholder
                        // cards align with the rest of the grid.
                        min_height: Val::Px(320.0),
                        padding: UiRect::all(Val::Px(SPACE_LG)),
                        border: UiRect::all(Val::Px(1.0)),
                        border_radius: BorderRadius::all(Val::Px(8.0)),
                        ..default()
                    },
                    BackgroundColor(CARD_BG),
                    BorderColor::all(CARD_BORDER),
                    MiningCard {
                        building_type: bt,
                    },
                    Name::new("mining_card_unknown"),
                ))
                .id();
            commands.entity(parent).add_child(card);
            return card;
        }
    };

    let count = building_counts.get(&bt).copied().unwrap_or(0);
    let data = build_mine_card_data(
        bt,
        def,
        count,
        body_breathable,
        body_type,
        planet_resources,
        multiplier,
    );

    // Delegate to the Build card's spawn helper. This gives us
    // identical chrome (border, shadow, padding, header row,
    // stats row, hairlines, effects, ETA, Queue button) so the
    // Mining tab and Build tab look like one UI surface.
    let card = spawn_card(
        commands,
        parent,
        &data,
        bt,
        body_font,
        body_font_medium,
        mono_font,
        icon,
        resource_icons,
    );

    // Demolish button: opposite side of the Queue button. Removes
    // `multiplier` mines (clamped to current count) from the active
    // colony via `PendingConstructionActions::mining_edits` with a
    // negative delta — instant, no construction queue. Frees
    // workers + power + maintenance immediately. Visually distinct
    // from the Queue button (red border, dim red fill) so the
    // destructive action reads at a glance.
    spawn_demolish_button(
        commands,
        card,
        bt,
        count,
        multiplier,
        body_font_medium,
    );

    card
}
// ── Mining tab systems (v0.5.2 PR-A.2) ──────────────────────────

/// Spawn a Demolish button on a Mining card. Pinned to the
/// bottom-right of the card (opposite of the Queue button) via
/// absolute positioning — the card's `Overflow::clip` keeps the
/// button from bleeding past the border. The button reads
/// "Demolish ×N" (or just "Demolish" when multiplier == 1) so the
/// player sees the batch size at a glance. Red border + dim red
/// fill makes the destructive action visually distinct from the
/// Queue button without screaming.
///
/// `count == 0` → spawn the button with `MiningDemolishDisabled`
/// attached; the click handler skips pushes when the marker is
/// present, and a per-frame system removes the marker once mines
/// are built.
fn spawn_demolish_button(
    commands: &mut Commands,
    card: Entity,
    bt: BuildingType,
    count: u32,
    multiplier: u32,
    body_font_medium: &Handle<Font>,
) {
    let label = if multiplier > 1 {
        // v0.5.2: match the Build button's "Build +N" shape with
        // "Demolish -N". The two buttons read as a matched
        // pair: "Build +5" / "Demolish -5". The ×N form
        // (multiplication sign) is reserved for the count row
        // inside the card body.
        format!("Demolish \u{2212}{}", multiplier)
    } else {
        "Demolish -1".to_string()
    };
    let dim_red = Color::srgba(0.353, 0.157, 0.169, 0.85);
    let dim_red_border = Color::srgba(0.847, 0.373, 0.392, 0.50);
    let demolish = commands
        .spawn((
            Button,
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                align_self: AlignSelf::FlexEnd,
                // v0.5.2: same height as the Queue button (32 px) so
                // the two read as a matched row at the bottom of
                // the card. Was 28 px which made the Demolish look
                // like a secondary control.
                height: Val::Px(32.0),
                padding: UiRect::horizontal(Val::Px(SPACE_XL)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                position_type: PositionType::Absolute,
                bottom: Val::Px(SPACE_LG),
                right: Val::Px(SPACE_LG),
                ..default()
            },
            BackgroundColor(dim_red),
            BorderColor::all(dim_red_border),
            Name::new("card_demolish"),
            MiningDemolishButton {
                building_type: bt,
            },
            Pickable::default(),
        ))
        .id();
    // Spawn-time disabled state — when no mines are built the
    // button is dim and the click handler is a no-op. Removed by
    // `tick_mining_demolish_disabled` once the count rises.
    if count == 0 {
        commands.entity(demolish).insert(MiningDemolishDisabled);
    }
    commands.entity(card).add_child(demolish);

    // Label child.
    let label_entity = commands
        .spawn((
            Text::new(label),
            TextFont {
                font: body_font_medium.clone(),
                font_size: CAPTION_SIZE,
                ..default()
            },
            TextColor(RED),
            Node {
                flex_grow: 1.0,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                display: Display::Flex,
                ..default()
            },
            Name::new("card_demolish_label"),
        ))
        .id();
    commands.entity(demolish).add_child(label_entity);
}

/// Click handler for the Mining Demolish button. Pushes
/// `(colony, bt, -multiplier)` to `PendingConstructionActions::mining_edits`
/// so `process_construction_actions` removes up to `multiplier`
/// mines on the next tick. Skips when `MiningDemolishDisabled` is
/// attached (i.e. count == 0). Rising-edge detection identical to
/// the other chip/CTA click handlers.
pub fn tick_mining_demolish_click(
    mut params: ParamSet<(
        Query<(Entity, &Interaction, &MiningDemolishButton), With<Button>>,
        Query<Entity, With<MiningDemolishDisabled>>,
    )>,
    ui_state: Res<ConstructionUiState>,
    mut pending: ResMut<PendingConstructionActions>,
    mut prev: Local<std::collections::HashMap<Entity, Interaction>>,
) {
    // Pre-compute the disabled set so the click loop stays single-Q.
    let mut disabled_set: std::collections::HashSet<Entity> =
        std::collections::HashSet::new();
    for entity in params.p1().iter() {
        disabled_set.insert(entity);
    }

    let mut current: std::collections::HashMap<Entity, Interaction> =
        std::collections::HashMap::new();
    for (entity, interaction, button) in params.p0().iter() {
        current.insert(entity, *interaction);
        let prev_interaction = prev.get(&entity).copied().unwrap_or(Interaction::None);
        if *interaction == Interaction::Pressed
            && prev_interaction != Interaction::Pressed
            && !disabled_set.contains(&entity)
        {
            let Some(colony_entity) = ui_state.selected_colony else {
                continue;
            };
            let multiplier = ui_state.mining_build_multiplier.max(1) as i32;
            pending.mining_edits.push((
                colony_entity,
                button.building_type,
                -multiplier,
            ));
        }
    }
    *prev = current;
}

/// Per-frame system: re-evaluate which Demolish buttons should be
/// disabled based on the current mine count. Mines can be added by
/// the Queue button (or any other system) and removed by this same
/// Demolish button, so a once-at-spawn check is not enough. Runs
/// every frame the Mining body is open.
///
/// Uses `queue_silenced` (Bevy 0.18+) so the insert / remove commands
/// don't panic if `update_mining_body` despawns the parent card
/// between this system's iter and the command apply at stage end.
pub fn tick_mining_demolish_disabled(
    mut commands: Commands,
    ui_state: Res<ConstructionUiState>,
    colonies: Query<&crate::colony::Colony>,
    demolish_buttons: Query<(Entity, &MiningDemolishButton, Has<MiningDemolishDisabled>)>,
) {
    let Some(colony_entity) = ui_state.selected_colony else {
        return;
    };
    let Ok(colony) = colonies.get(colony_entity) else {
        return;
    };
    for (entity, button, is_disabled) in demolish_buttons.iter() {
        let count = colony.buildings.get(&button.building_type).copied().unwrap_or(0);
        if count == 0 && !is_disabled {
            commands.entity(entity).queue_silenced(InsertDemolishDisabled);
        } else if count > 0 && is_disabled {
            commands.entity(entity).queue_silenced(RemoveDemolishDisabled);
        }
    }
}

/// `EntityCommand` that inserts `MiningDemolishDisabled`. Used by
/// `tick_mining_demolish_disabled` via `queue_silenced` so the insert
/// is dropped instead of panicking if the entity is despawned by the
/// time the command applies.
struct InsertDemolishDisabled;

impl bevy::ecs::system::EntityCommand for InsertDemolishDisabled {
    fn apply(self, mut entity: bevy::ecs::world::EntityWorldMut) {
        entity.insert(MiningDemolishDisabled);
    }
}

/// `EntityCommand` that removes `MiningDemolishDisabled`. See
/// `InsertDemolishDisabled` for the rationale.
struct RemoveDemolishDisabled;

impl bevy::ecs::system::EntityCommand for RemoveDemolishDisabled {
    fn apply(self, mut entity: bevy::ecs::world::EntityWorldMut) {
        entity.remove::<MiningDemolishDisabled>();
    }
}


/// Group-visibility toggle: when the player clicks a group chevron
/// (or the orbital section header), flip the corresponding bit in
/// `ui_state.mining_groups_collapsed` / `ui_state.mining_orbital_collapsed`.
/// The actual `Display::None / Display::Flex` swap happens on the
/// next `update_mining_body` run (which re-spawns the cards).
///
/// The orbital section header uses the same `MiningGroupHeader`
/// marker with a sentinel `Helium3` group id. The handler uses
/// that id to distinguish: `Helium3` → toggle orbital collapsed;
/// anything else → toggle the corresponding surface group.
pub fn tick_mining_group_visibility(
    mut ui_state: ResMut<ConstructionUiState>,
    headers: Query<(Entity, &Interaction, &MiningGroupHeader), With<Button>>,
    mut prev: Local<std::collections::HashMap<Entity, Interaction>>,
) {
    let mut current: std::collections::HashMap<Entity, Interaction> =
        std::collections::HashMap::new();

    for (entity, interaction, header) in headers.iter() {
        current.insert(entity, *interaction);
        let prev_interaction = prev.get(&entity).copied().unwrap_or(Interaction::None);
        if *interaction == Interaction::Pressed
            && prev_interaction != Interaction::Pressed
        {
            if header.group_id == MiningGroupId::Helium3 {
                // The orbital section header reuses the
                // `MiningGroupHeader` marker with the Helium3
                // sentinel. Distinguish by group id.
                ui_state.mining_orbital_collapsed =
                    !ui_state.mining_orbital_collapsed;
            } else {
                let id = header.group_id;
                if ui_state.mining_groups_collapsed.contains(&id) {
                    ui_state.mining_groups_collapsed.remove(&id);
                } else {
                    ui_state.mining_groups_collapsed.insert(id);
                }
            }
        }
    }

    *prev = current;
}