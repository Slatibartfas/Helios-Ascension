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

use bevy::input::mouse::MouseWheel;
use bevy::picking::events::{Out, Over, Pointer, Press, Release};
use bevy::picking::hover::HoverMap;
use bevy::picking::pointer::{PointerButton, PointerId};
use bevy::picking::Pickable;
use bevy::prelude::*;
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
use super::resource_icons::{get_energy_icon_handle_bevy, get_resource_icon_handle_bevy, ResourceIcons};
// v0.5.2 (2026-08-06): the three canonical font handles, loaded once
// at Startup by `widgets::init_ui_fonts`. Per-frame systems read
// `Res<UiFonts>` instead of calling `asset_server.load("fonts/...")`.
use super::widgets::{spawn_scrollable_container, UiFonts};

// One row of a building's resource demand: the resource name as
// it appears in `buildings.ron`, the per-unit amount (already
// multiplied by the build quantity), and the parsed
// `ResourceType` (used to look up the icon `Handle<Image>` and
// the category tint). `resource` is `None` when the RON string
// doesn't match a known variant (defensive — the canary falls
// back to a tinted placeholder square + `TEXT_BODY` so a future
// RON addition never panics).
///
// v0.5.2 PR-A.4 follow-up: the canary emits these for every
// cost entry and renders them as `[PNG icon | tinted amount]`.
// The icon is the asset-server PNG from
// `assets/textures/ui/resources/<name>.png`, post-processed
// (white → transparent, dark → un-premultiplied alpha) and
// tinted to the resource's category colour at render time
// via `ImageNode::color`.
#[derive(Debug, Clone)]
pub struct ResourceCostRow {
    pub name: String,
    pub amount: f64,
    pub resource: Option<ResourceType>,
}

// One chip's worth of power data for a building card. Renders as
// a `[bolt-in-hex PNG | tinted amount]` chip in the card body,
// matching the `ResourceCostRow` chip pattern but with the
// dedicated `assets/textures/ui/resources/energy.png` icon
// (post-processed to white-on-transparent in
// `src/ui/resource_icons.rs`) tinted to the power tone:
///
// * `Produces` (net generator) — green.
// * `Demand` (net consumer) — throughput green if the batch
//   fits the active colony's spare grid, negative orange if
//   `power_insufficient` is set, neutral text-body otherwise.
// * `None` (no power interaction) — neutral text-body, shown
//   as `0 W` so the chip still reads as "this building has no
//   power interaction".
///
// `multiplier` is the active build-qty so the displayed number
// reflects the batch total (e.g. a 5 GW fusion plant queued at
// ×10 reads as `50 GW`, not `5 GW × 10`). The `tooltip_lines`
// vec feeds the per-chip hover overlay (see `PowerChipTooltip`
// below) and carries the per-unit + batch + spare-grid
// breakdown.
#[derive(Debug, Clone)]
pub struct PowerChipData {
    // "Demand" or "Produces" — the verb shown next to the icon.
    pub verb: &'static str,
    // Pre-formatted batch total (e.g. `"5.0 GW"`, `"900 MW"`,
    // `"0 W"`). Single source of truth for the chip label.
    pub amount: String,
    // Per-unit power value in MW (positive = generation,
    // negative = demand, 0.0 = no power interaction). Drives
    // the chip tone + tooltip breakdown.
    pub per_unit_mw: f64,
    // Active build-qty multiplier (≥ 1). Echoed in the tooltip
    // so the player can read the per-unit vs batch split.
    pub multiplier: u32,
    // Active colony's grid surplus in MW. `None` if no colony
    // is selected (menu-screen previews) — the tooltip falls
    // back to "no active colony" wording.
    pub spare_mw: Option<f64>,
    // `true` if the batch demand exceeds the spare grid. Drives
    // the negative-orange tone and the "not enough energy"
    // tooltip line. Always `false` for generators and no-ops.
    pub insufficient: bool,
    // Pre-built tooltip body lines (one per visual line, no
    // trailing newlines). The first line is the headline.
    pub tooltip_lines: Vec<String>,
}

// Total `PowerGeneration` modifier value (GW per unit) for a
// building. `0.0` = not a generator. Shared by the three card-data
// builders via [`build_power_chip_data`] (v0.5.2, 2026-08-06) —
// previously each builder re-implemented the same filter+sum inline.
fn power_output_gw_per_unit(def: &BuildingDefinition) -> f64 {
    def.modifiers
        .iter()
        .filter(|m| m.modifier_type == "PowerGeneration")
        .map(|m| m.value)
        .sum()
}

// Build the power-chip data for a building card (v0.5.2, 2026-08-06).
//
// Extracted from the near-identical power-chip blocks that used to
// live in `card_data_with_multiplier` (Build tab), `build_mine_card_data`
// (Mining tab), and the constant chip in `build_constructed_card_data`.
// One helper, three call sites — the 3-way generator / no-op / demand
// branch is the same everywhere.
//
// `mult` is the active build-qty multiplier (≥ 1); the displayed
// number reflects the batch total. `spare` is the active colony's grid
// surplus in MW: `None` = no colony selected, `Some(s)` = the real
// surplus (s may be ≤ 0 for a deficit). The three builders map their
// own sentinels onto this:
// - Build tab: `spare_power_mw > 0.0 → Some`, else `None`
//   (its `compute_colony_spare_power_mw` returns 0.0 for no colony).
// - Mining tab: `spare_power_mw.is_nan() → None`, else `Some`
//   (the mining refresh passes `f64::NAN` for no colony).
fn build_power_chip_data(
    def: &BuildingDefinition,
    mult: f64,
    spare: Option<f64>,
) -> PowerChipData {
    let gw_per_unit = power_output_gw_per_unit(def);
    // Generator: green "Produces X" with a net-surplus tooltip.
    if gw_per_unit > 0.0 {
        let per_unit_mw = gw_per_unit * 1_000.0;
        let total_mw = per_unit_mw * mult;
        let line = format_power(total_mw);
        let per_unit = format_power(per_unit_mw);
        let tooltip_lines = if mult > 1.0 {
            vec![
                format!("Power generation: {line}"),
                format!("({per_unit} per unit × {mult})"),
                "Net surplus to the grid".to_string(),
            ]
        } else {
            vec![
                format!("Power generation: {line}"),
                "Net surplus to the grid".to_string(),
            ]
        };
        return PowerChipData {
            verb: "Produces",
            amount: line,
            per_unit_mw,
            multiplier: mult as u32,
            spare_mw: None, // generators don't gate on spare
            insufficient: false,
            tooltip_lines,
        };
    }
    // No power interaction: neutral "0 W".
    if def.power_demand_mw.abs() < 0.01 {
        return PowerChipData {
            verb: "Power",
            amount: "0 W".to_string(),
            per_unit_mw: 0.0,
            multiplier: mult as u32,
            spare_mw: None,
            insufficient: false,
            tooltip_lines: vec!["No grid interaction".to_string()],
        };
    }
    // Net consumer: "Demand" with the spare-grid breakdown.
    let total = def.power_demand_mw * mult;
    let per_unit = format_power(def.power_demand_mw);
    let line = format_power(total);
    let insufficient = spare.is_some_and(|s| total > s);
    let mut tooltip_lines = vec![format!("Power demand: {line}")];
    if mult > 1.0 {
        tooltip_lines.push(format!("({per_unit} per unit × {mult})"));
    }
    match spare {
        Some(s) if s > 0.0 => {
            tooltip_lines.push(format!("Spare grid: {}", format_power(s)));
        }
        Some(_) => tooltip_lines.push("No spare grid (deficit)".to_string()),
        None => tooltip_lines.push("No active colony".to_string()),
    }
    if insufficient {
        tooltip_lines.push("Not enough energy".to_string());
    }
    PowerChipData {
        verb: "Demand",
        amount: line,
        // `per_unit_mw` is the SIGNED power per unit — positive for
        // generation, NEGATIVE for consumption. The RON stores
        // `power_demand_mw` as a positive magnitude, so we negate it
        // here so the chip's `+`/`-` sign prefix + red/green text
        // correctly identifies this building as a consumer.
        per_unit_mw: -def.power_demand_mw,
        multiplier: mult as u32,
        spare_mw: spare.filter(|s| *s > 0.0),
        insufficient,
        tooltip_lines,
    }
}

// Compute the active colony's spare power in MW. Returns 0.0 if no
// colony is selected or no `BuildingsData` is loaded. Used by the
// Build card to color-code the Power effect line: green when the
// batch demand fits inside the grid, red when it would push the
// grid into deficit.
///
// v0.5.2 PR-A.2: per user feedback 2026-08-02, the canary now shows
// "per-building demand × multiplier = total vs spare" as the single
// source of truth for power on the build card. The old design had
// three independent power readouts (PWR top stat, "Power:" body
// effect, and the workforce mislabeled as MW) which read as a
// confusing stack of similar numbers.
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

// Top-level sub-tab in the Construction menu.
///
// v0.5.2: the legacy `Stockpiles` minimum-stockpile editor was
// renamed to `Mining` — the dedicated mining grid replaces it
// (production mgmt with [-] [+] buttons + reserve / accessibility
// readouts per resource card).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConstructionTab {
    Overview,
    Buildings,
    #[default]
    Build,
    // v0.5.2: dedicated mining grid (replaces the v0.5.x
    // `Stockpiles` minimum-stockpile editor).  One compact card per
    // per-resource base mine + AutoMine, grouped by resource group,
    // with [−] [+] buttons for direct inventory edits and a live
    // readout of count / production / reserve / accessibility.
    Mining,
}

impl ConstructionTab {
    // All four variants in display order. Used by the canary's
    // `tick_construction_body_visibility` system to know which body
    // to show.
    pub const ALL: [ConstructionTab; 4] = [
        ConstructionTab::Overview,
        ConstructionTab::Buildings,
        ConstructionTab::Build,
        ConstructionTab::Mining,
    ];
}

// Persistent UI state for the Construction menu. Lives across frames
// so the player's last-selected tab, colony, and build multiplier
// survive re-mounts.
///
// v0.5.2: this is now owned by the bevy_ui canary. The legacy
// egui `construction_panel.rs` referenced it via
// `use crate::ui::construction_panel::ConstructionUiState`; the
// import is gone (the canary defines the type directly).
#[derive(Resource, Debug, Clone)]
pub struct ConstructionUiState {
    // Build multiplier: how many copies to queue at once
    pub build_multiplier: u32,
    // Currently selected colony entity (None = auto-select first)
    pub selected_colony: Option<bevy::ecs::entity::Entity>,
    // Selected top-level tab within the construction menu.
    pub selected_tab: ConstructionTab,
    // Selected build-category tab within the Build view.
    // v0.5.2: 8 categories with "All" at index 8. The Build
    // tab opens with the "All" chip active (the player most often
    // wants to scroll the full catalog first), so the default is
    // 8 — keep this in lockstep with
    // `ActiveChips::default().category` (also 8) so the visual
    // chip and the filter logic agree on the first frame.
    pub selected_build_tab: usize,
    // Functional-role filter (Food / Power / Industry / Research /
    // Synergy Active). Overlays the 9-category tabs.
    pub selected_filter: BuildFilter,
    // v0.5.2 Mining tab: which surface groups are currently
    // collapsed. Persists across tab switches. Empty = all
    // surface groups visible.
    pub mining_groups_collapsed: std::collections::HashSet<MiningGroupId>,
    // v0.5.2 Mining tab: whether the entire orbital section is
    // collapsed (it carries 25 cards; collapsed by default so
    // the initial paint is dominated by surface mines).
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
            mining_groups_collapsed: std::collections::HashSet::new(),
            mining_orbital_collapsed: true, // collapsed by default
        }
    }
}

// Identifier for one of the 8 surface mine groups in the Mining
// tab. Used as a key into `ConstructionUiState::mining_groups_collapsed`
// so collapse state persists per group.
///
// The orbital section is a single collapsible
// (`mining_orbital_collapsed` bool) — it has 25 cards spread
// across 5 sub-groups, and per spec the sub-groups themselves
// are non-collapsible (matches the legacy egui tab's behaviour).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MiningGroupId {
    Construction,
    Precious,
    Strategic,
    Fissile,
    Hydrocarbons,
    HeavyWater,
    // `WaterProcessor` is a water extraction facility (atmospheric
    // condenser / ice miner) — body-restricted to `[None]`
    // atmospheres. Sits in its own group so it pairs with the
    // orbital `AutoWaterProcessor` (the only AutoMine without a
    // surface mine counterpart before this group was added).
    Water,
    Helium3,
}

// Surface mine group definitions for the Mining tab. Each entry
// is `(group_id, display_label, buildings)` in display order.
///
// Source-of-truth: 25 entries — 24 base mines (one per
// mineable resource, per v0.5.2's per-resource dedicated
// design) + `WaterProcessor` (the only AutoMine counterpart
// that wasn't a "mine" in the strict sense — it's an
// atmospheric condenser / ice miner that pairs with the
// orbital `AutoWaterProcessor`). The 24+1 layout mirrors
// the 25-card orbital section so the player can see a
// matched surface/orbital pair for every orbital AutoMine.
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

// Orbital AutoMine sub-group definitions for the Mining tab.
// Single collapsible, 5 non-collapsible sub-groups, 25 cards
// total. Source-of-truth: 25 AutoMines from `parse_building_type`
// (24 per the spec + `AutoTitaniumMine` which the spec omitted).
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

// Functional-role filter chip overlaid on top of the 9-category tabs
// in the Build view. Lets the player pivot by what the building
// *does* (feeds the colony, generates power, drives industry,
// advances research) rather than by its internal category label.
///
// v0.5.2: moved out of `construction_panel.rs`. The canary's
// `ChipKind::Filter(BuildFilter)` is the only consumer in the
// bevy_ui surface; the canary's `visible_cards(_filter: BuildFilter, ...)`
// still receives it for future use.
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
    // All variants in display order. Used by the chip-row primitive
    // in the canary (Phase C5 when the chip row lands).
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

// Loaded + post-processed building icons keyed by `BuildingType`. The
// icons are 4-byte RGBA PNGs sourced from `assets/textures/ui/buildings/`
// — dark line art on a white background. The `process_building_icons`
// system runs once per icon and converts the white pixels to
// transparent + dark lines to white (premultiplied), matching the
// runtime tint pattern used by [`crate::ui::icons::process_menu_icons`].
///
// `None` value means the building has no icon asset (defensive; in
// practice all 52 buildings have PNGs). The card-spawn code falls back
// to a cyan-tinted placeholder square when the handle is `None`.
#[derive(Resource, Default)]
pub struct BuildingIcons {
    pub handles: std::collections::HashMap<BuildingType, Handle<Image>>,
    pub processed: std::collections::HashSet<BuildingType>,
}

// Startup system: load all building icons from disk. Runs once at
// startup; the assets are small (a few KB each) so the load is fast.
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

// Post-process building icons: white background → transparent, dark
// lines → white (premultiplied). Same recipe as
// `crate::ui::icons::process_menu_icons`. Runs every frame until
// every icon has been processed once; the per-icon flag in `processed`
// makes it a no-op after the first pass.
//
// ## Per-frame budget (2026-08-05)
//
// The icons are 256×256 (65K px) so a single one is cheap, but the
// batch is ~95 and they all load asynchronously — the frame where
// the whole batch lands would run 95 × 65K-pixel luminance-key loops
// inline. That's the same hazard class as the resource-icon stall
// (GRA regression, fixed 2026-08-05): not seconds, but it shares its
// frame with `process_menu_icons` + `process_research_icons` and the
// first boot-chain steps. Capping at 4/frame spreads the batch over
// ~24 frames with no single-frame spike; unprocessed icons are
// retried next frame (they early-continue when not decoded yet).
pub fn process_building_icons(mut icons: ResMut<BuildingIcons>, mut images: ResMut<Assets<Image>>) {
    const MAX_ICONS_PER_FRAME: usize = 4;
    let mut processed_this_frame = 0usize;

    let to_process: Vec<(BuildingType, Handle<Image>)> = icons
        .handles
        .iter()
        .filter(|(bt, _)| !icons.processed.contains(*bt))
        .map(|(bt, h)| (*bt, h.clone()))
        .collect();

    for (building_type, handle) in to_process {
        if processed_this_frame >= MAX_ICONS_PER_FRAME {
            break;
        }
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
            processed_this_frame += 1;
        }
    }
}

// ── Construction Queue (canary placeholder) ──────────────────────

// One item in the construction queue.
#[derive(Debug, Clone)]
pub struct QueuedBuild {
    pub name: String,
    pub qty: u32,
    pub bp_per_unit: f64,
}

// Resource: the currently active chip index for each row.
///
// This is the single source of truth for visual active state. The
// `tick_chip_button_active_overlay` system reads this resource each
// frame and applies ACTIVE_CHIP_BG to the matching chip +
// INACTIVE_CHIP_BG to the rest. We store the active index per row
// instead of using a ChipActive marker because marker add/remove
// ordering with Commands is fragile.
#[derive(Resource, Debug, Clone)]
pub struct ActiveChips {
    // Active sub-tab index (0=Overview, 1=Buildings, 2=Build, 3=Mining)
    pub tab: usize,
    // Active build qty multiplier (Build tab)
    pub qty: u32,
    // Active filter/category index (0..8 = category, 9 = All).
    // v0.5.2: 8 categories → 9 — Mining was split out of Industry.
    pub category: usize,
    // Active mining qty multiplier (Mining tab).
    // v0.5.2 PR-A.2: separate from `qty` so the Build tab's
    // qty and the Mining tab's qty don't cross-pollute.
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

// Resource: the construction queue + current output rate.
///
// In Phase C4 this will be replaced with the real queue (driven by
// `process_construction_actions`); for now it's static placeholder data
// so the canary can show ETA + queue length without a real queue system.
#[derive(Resource, Debug, Clone)]
pub struct ConstructionQueue {
    // BP per year the colony currently produces (output rate).
    pub output_bp_per_year: f64,
    // Items currently in the queue (FIFO).
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

// Total seconds remaining in the queue (sum of each item's build time).
// Returns 0.0 when the queue is empty.
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

// ETA for a single card: queue_remaining + own_build_time.
pub fn card_eta_seconds(queue: &ConstructionQueue, card_bp: f64) -> f64 {
    queue_remaining_seconds(queue)
        + (card_bp / (queue.output_bp_per_year / 365.25 / 24.0 / 3600.0).max(1e-9))
}

// Resource that flags the Construction canary as visible.
#[derive(Debug, Default, Resource, PartialEq, Eq, Clone, Copy)]
pub enum ConstructionState {
    #[default]
    Off,
    On,
}

// Resource: whether the queue panel is open. Toggled by the AppBar
// `QUEUE` chip. Lives on a separate resource (not on `ConstructionUiState`)
// so the egui reference panel doesn't see the expansion and so future
// queue-panel-only state (e.g. scroll position) can land here too.
#[derive(Debug, Default, Resource, Clone, Copy)]
pub struct QueuePanelState {
    pub open: bool,
}

// Resource: whether the Active Colony dropdown menu is open. Toggled by
// clicking the picker. When `open`, the floating dropdown menu shows
// every existing colony and selecting one updates
// `ConstructionUiState::selected_colony` plus closes the menu.
#[derive(Debug, Default, Resource, Clone, Copy)]
pub struct ColonyDropdownState {
    pub open: bool,
}

// Resource: state for the centered Demolish confirmation modal
// (v0.5.2 PR-A.7). `tick_demolish_click` sets `open: true` and
// stores the building + count when the player presses a Demolish
// button. `tick_demolish_confirm_yes_click` applies the edit and
// resets the state; `tick_demolish_confirm_no_click` and
// `tick_demolish_dialog_close_on_tab_switch` (and
// `tick_demolish_dialog_close_on_colony_change`) reset without
// action. `update_demolish_dialog_text` re-reads the live colony
// count every frame and clamps `count` down, so the title never
// claims to demolish more buildings than exist (e.g. player picks
// ×25 when only 3 are present → title reads "Demolish 3 …?").
#[derive(Debug, Default, Resource, Clone)]
pub struct DemolishConfirmState {
    pub open: bool,
    pub building_type: Option<crate::colony::types::BuildingType>,
    pub count: u32,
}

// Resource: hover-driven tooltip state for the construction canary.
///
// When the player hovers over a disabled Queue CTA (one they can't
// afford at the current multiplier), `tick_construction_tooltip`
// populates `text` with a short reason and flips `visible` to true.
// The `update_tooltip_text` system mirrors that to the on-screen
// tooltip Text node each frame so the player sees *why* the button
// is greyed out.
#[derive(Debug, Default, Resource)]
pub struct ConstructionTooltipState {
    pub text: String,
    pub visible: bool,
}
#[allow(dead_code)]

// v0.5.2 (build menu fix): cursor-following tooltip for disabled
// Queue CTAs (resource-shortage OR body-blocked). The
// `tick_construction_tooltip` system populates this from the
// per-frame CTA scan; the `update_queue_button_tooltip` system
// positions a singleton overlay at the cursor and writes the
// lines to its text node. Mirrors the resource-cost chip
// tooltip pattern at `ResourceCostHoverState` /
// `update_resource_cost_tooltip`, but with a multi-line
// payload (the `Missing:` list) instead of a single line.
#[derive(Debug, Default, Resource)]
pub struct QueueButtonTooltipState {
    // Entity id of the CTA the cursor is currently hovering, so
    // the per-frame system can guard against stale state (the
    // CTA may be despawned between frames when the player
    // switches sub-tabs, multiplier, or colony).
    pub hovered_cta: Option<Entity>,
    // Pre-formatted lines, one per row of the tooltip. The
    // per-frame system writes them into the overlay's Text node
    // verbatim — no further formatting / wrapping.
    pub lines: Vec<String>,
}

// Marker on the singleton cursor-following overlay for the
// disabled Queue CTA tooltip. Spawned once at panel setup time
// (alongside `ResourceCostTooltipOverlay` / `PowerChipTooltipOverlay`);
// populated each frame by `update_queue_button_tooltip` from
// `QueueButtonTooltipState`. Same ZIndex as the chip tooltips
// (20) so it draws on top of card chrome but below the topbar.
#[derive(Component)]
pub struct QueueButtonTooltipOverlay;

// Marker on the inner text node of the overlay. The update
// system uses `Single<&mut Text, With<…>>` so it doesn't have
// to walk a Children hierarchy.
#[derive(Component)]
pub struct QueueButtonTooltipText;

// Marker component for the canary root container.
#[derive(Component)]
pub struct ConstructionRoot;

// Marker component for the AppBar title text.
#[derive(Component)]
pub struct ConstructionTitle;

// Marker component for the AppBar subtitle text.
#[derive(Component)]
pub struct ConstructionSubtitle;

// Marker component for each build card. Holds the building name.
#[derive(Component)]
pub struct ConstructionCard {
    pub name: String,
}

// v0.5.2 (2026-08-06): marker for **Build-tab-only** cards.
//
// `spawn_card` (the shared chrome builder) attaches `ConstructionCard`
// to every card — Build, Buildings, AND Mining. `refresh_card_grid`
// used to despawn `Query<Entity, With<ConstructionCard>>` — i.e. EVERY
// card in the world — whenever `ConstructionUiState` changed (chip
// click, tab switch, mining group collapse). That silently wiped the
// Mining + Buildings cards, and their per-tab caches (the mining
// fingerprint gate + the buildings `spawned_cards` map) had no way to
// recover: the Buildings cache still believed the cards existed, so it
// never re-spawned them (the "Buildings tab shows the header but no
// cards" bug), and the Mining tab churned a full 49-card teardown +
// rebuild on every click (the "Build button flickers" bug).
//
// `refresh_card_grid` now despawns ONLY `With<BuildCard>` entities —
// the cards IT spawned — and inserts `BuildCard` on each. Mining and
// Buildings cards are untouched.
#[derive(Component)]
pub struct BuildCard;

// Marker component for the Queue CTA. Carries the `BuildingType` this
// card represents, so the click handler knows which building to enqueue.
#[derive(Component)]
pub struct ConstructionCta {
    pub building_type: BuildingType,
}

// Marker component for Queue CTAs that should be **disabled** (visible
// but inactive). The player can't afford N copies of the building at
// the current multiplier. The `tick_construction_cta_disabled` system
// inserts this marker after each refresh, and the click handler skips
// the push when the marker is present.
#[derive(Component)]
pub struct ConstructionCtaDisabled;

// v0.5.2 (build menu fix): marker for Queue CTAs that are
// **permanently** disabled because the building isn't allowed on
// the current body (e.g. an Iron Mine on a gas giant). Distinct
// from `ConstructionCtaDisabled` so the per-frame
// `tick_construction_cta_disabled` system — which only reasons
// about resources / power — doesn't silently re-enable the CTA
// when the player happens to have the resources on hand. The
// hover system treats this marker identically to
// `ConstructionCtaDisabled` (no hover scale, no colour change);
// the affordability system ignores it (the spawn-time decision
// is permanent until the body changes).
#[derive(Component)]
pub struct ConstructionCtaBodyBlocked;

// Marker component for the AppBar "OPEN QUEUE" toggle chip. The
// `tick_open_queue_chip_click` system reads this marker to know
// which chip's `Interaction::Pressed` should toggle `QueuePanelState`.
#[derive(Component)]
pub struct OpenQueueChip;

// Marker component for the "Active Colony" picker. The
// `tick_colony_picker_click` system reads this marker to toggle
// `ColonyDropdownState::open` when the player clicks the picker.
// Distinct from `OpenQueueChip` so the click handlers can dispatch
// independently.
#[derive(Component)]
pub struct ColonyPicker;

// Marker component for the floating Active Colony dropdown menu
// (the list of colonies that appears below the picker when it's
// clicked). The `tick_colony_dropdown_visibility` system shows /
// hides this container based on `ColonyDropdownState::open`.
#[derive(Component)]
pub struct ColonyDropdownMenu;

// Marker component for a single option row inside the colony dropdown
// menu. Carries the `Entity` of the `Colony` it represents so the
// `tick_colony_option_click` system can update
// `ConstructionUiState::selected_colony` when an option is clicked.
#[derive(Component)]
pub struct ColonyDropdownOption {
    pub colony_entity: bevy::ecs::entity::Entity,
}

// Marker component on the colony value text inside the picker. The
// `update_colony_picker_text` system writes the active colony's name
// here every frame so the label always reflects the current selection.
#[derive(Component)]
pub struct ColonyPickerText;

// Marker component on each row of the colony dropdown menu that holds
// the colony name text. The `refresh_colony_dropdown` system uses
// these to know which rows to keep vs. despawn when the list of
// colonies changes.
#[derive(Component)]
pub struct ColonyDropdownOptionText;

// Marker on the canary's hover tooltip text. The
// `update_construction_tooltip` system reads `ConstructionTooltipState`
// every frame and writes the text + toggles visibility.
#[derive(Component)]
pub struct ConstructionTooltipText;

// Marker component for the marquee track that wraps an overflowing
// build-card subtitle. The track holds two copies of the subtitle
// back-to-back (no gap — a gap would create a visible "blank" beat
// when copy A scrolls out and copy B scrolls in) and is animated via
// `UiTransform.translation` by `tick_subtitle_marquee`.
///
// `card`, `text_node`, and `clip_container` are pre-resolved entity
// handles stored at spawn time so the tick system can do direct
// `Query::get` lookups instead of walking `Parent` / `Children`
// chains every frame. All three are required; missing entities
// (the card was despawned mid-tick) just keep the marquee dormant
// so the engine can clean it up.
///
// `text_width` and `container_width` are the most recent
// `ComputedNode`-measured values (pixels) used to decide whether
// the description actually overflows. When `text_width <=
// container_width` the marquee is dormant and the track sits at
// translation `(0, 0)` — the text fits naturally and we leave it
// alone.
///
// `phase` is the current **pixel offset** of the track (always
// non-negative, wrapped modulo `text_width`). At `phase = 0`
// copy A sits at the clip container's left edge; at
// `phase = text_width` copy B sits exactly where copy A
// started — the seamless loop. `tick_subtitle_marquee`
// integrates `phase += dt * PIXELS_PER_SECOND` each tick while
// the description overflows, so the track drifts leftward at
// a constant rate that doesn't depend on card size, hover
// state, or how many cards are visible.
#[derive(Component)]
pub struct SubtitleMarquee {
    // Parent build-card entity, used by `tick_subtitle_marquee`
    // to read the card's `Interaction` each frame. Stored
    // explicitly so the system does one `Query::get` instead of
    // walking `Parent` chains.
    pub card: Entity,
    // Entity of the first text copy, used to measure
    // `ComputedNode::content_size` for the per-frame overflow
    // check. The second copy shares the same string + font so we
    // only need to read one.
    pub text_node: Entity,
    // Entity of the outer clip container, used to measure
    // `ComputedNode::size` (the visible horizontal extent the
    // track scrolls within).
    pub clip_container: Entity,
    // Most recent measured text content width in pixels. Updated
    // each frame by `tick_subtitle_marquee` from `ComputedNode`.
    pub text_width: f32,
    // Most recent measured clip-container width in pixels.
    pub container_width: f32,
    // Current pixel offset of the track (leftward drift).
    // `tick_subtitle_marquee` integrates this each tick and
    // wraps modulo `text_width` so the value stays bounded
    // over a long session. Initialised to 0.0 at spawn — each
    // card therefore starts at a different drift point in its
    // loop depending on when it was spawned, which keeps the
    // grid from marching in lockstep.
    pub phase: f32,
}

// Marker component for the QueuePanel root. The
// `tick_queue_panel_visibility` system hides all but the active
// state — there's only one QueuePanel so this is a singleton query.
#[derive(Component)]
pub struct QueuePanelRoot;

// Marker component for a single row in the queue panel. The
// `update_queue_panel` system spawns one of these per
// `ConstructionProject` for the selected colony, and removes them
// when the project is gone (cancel / complete). The mapping
// `project_entity -> QueueRowEntity` lives in a `Local` storage
// inside the system.
#[derive(Component)]
pub struct QueuePanelRow {
    pub project_entity: Entity,
}

// Marker component for the cancel button on a queue row. Click handler
// pushes the project entity to `PendingConstructionActions::cancel_construction`.
#[derive(Component)]
pub struct QueuePanelRowCancel {
    pub project_entity: Entity,
}

// Marker component for the QueuePanel close button. The click handler
// toggles `QueuePanelState::open` to `false`.
#[derive(Component)]
pub struct QueuePanelClose;

// Marker component for the card grid container. The refresh system
// queries for this to find the grid and re-parent new cards.
#[derive(Component)]
pub struct CardGrid;

// Marker for the always-visible vertical scrollbar track. The
// track pins itself to the right edge of whichever scrollable
// body the construction menu exposes (Build tab card grid, Mining
// tab body, etc.). The `target` field tells
// `tick_construction_scrollbar` which entity's
// `ScrollPosition` + `ComputedNode::content_size` drives the
// thumb's height + Y offset. The `tab` field tells
// `tick_construction_body_visibility` which tab this scrollbar
// belongs to so it can show/hide alongside the body. This is the
// v0.5.2 PR-A.8 generalisation of the original Build-tab-only
// `CardGridScrollbarTrack`: the same chrome now appears on every
// construction tab whose content can overflow vertically.
#[derive(Component)]
pub struct ConstructionScrollbarTrack {
    // The scrollable entity this track drives. Set at spawn time
    // from `setup_construction` (Build tab → `card_grid`) or
    // `spawn_mining_body` (Mining tab → `mining_content`). The
    // visual tick reads `target`'s `ScrollPosition` each frame.
    pub target: Entity,
    // The tab this scrollbar belongs to. The visibility system
    // shows the track only when the matching body is active, so a
    // Mining-track overlay doesn't bleed onto the Build tab and
    // vice versa. Mirrors the `ConstructionTabBody` marker the
    // body root carries — the two move in lockstep.
    pub tab: ConstructionTabBody,
}

// Marker for the thumb (the draggable handle inside the track).
// Driven each frame by `tick_construction_scrollbar`. The thumb
// inherits its `target` from the parent track via the observer
// pattern (`on_thumb_press` / `on_track_press` both look up the
// track's `target` so the drag updates the right scrollable).
#[derive(Component)]
pub struct ConstructionScrollbarThumb;

// Effect-bullet tone (drives the color of the corresponding line).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectTone {
    Positive,
    Negative,
    Neutral,
    Cost,
    Throughput,
}

// Build card data: name, subtitle, stats, effects, queue label.
#[derive(Debug, Clone)]
pub struct BuildCardData {
    pub name: String,
    pub subtitle: String,
    // The actual `BuildingType` this card represents. The Queue button
    // pushes `(selected_colony, building_type)` to
    // `PendingConstructionActions::start_construction` so
    // `process_construction_actions` can spawn the project.
    pub building_type: BuildingType,
    // Path to the building's icon, relative to the `assets/` directory
    // (e.g. `textures/ui/buildings/mine.png`). Sourced from
    // `BuildingDefinition::icon` in `buildings.ron`. The canary loads
    // this via `AssetServer::load` and renders it as the card header icon.
    pub icon: String,
    // The player's chosen build multiplier. The card ETA is derived
    // from `build_points * multiplier` so the player can see the full
    // batch ETA at a glance. The Queue button also pushes this many
    // copies to `PendingConstructionActions`.
    pub multiplier: u32,
    pub stat_a: (&'static str, String),
    pub stat_b: (&'static str, String),
    // Build points for one unit. The ETA row derives from
    // `build_points * multiplier` divided by the static
    // placeholder output. v0.5.2: added so the Mining card can
    // show ETA (the Mining card's `stat_a` carries the live
    // inventory count, not BP — without this separate field the
    // ETA calculation would parse the count as 0 BP and show
    // "0s" regardless of multiplier).
    pub build_points: f64,
    // stat_c is unused (kept for struct stability) — the power
    // readout moved to the body's first effect line.
    pub stat_c: (&'static str, String),
    pub effects: Vec<(EffectTone, String)>,
    // Rich resource-cost rows: each entry is rendered as a
    // `[PNG icon | tinted amount]` row in the card body, so
    // the player can identify the resource at a glance and group
    // related costs (Construction metals vs Volatiles vs
    // Precious metals …) by hue. The icon is the asset-server
    // PNG from `assets/textures/ui/resources/<name>.png`,
    // post-processed (white → transparent, dark →
    // un-premultiplied alpha) and tinted to the resource's
    // category colour at render time via
    // `ImageNode::color` (`bevy_theme::category_color_for_resource`).
    ///
    // v0.5.2 PR-A.4 follow-up: supersedes the emoji-prefixed
    // cost bullets that previously lived in `effects` with
    // `EffectTone::Cost`. Cost entries are no longer pushed to
    // `effects`; the canary renders the rows in this vec instead.
    pub resource_costs: Vec<ResourceCostRow>,
    // Power chip for this card. `Some` for every building —
    // the chip itself is the single source of truth for power
    // on the card (the old `Power: … MW` text line in
    // `effects` has been removed; see PR-A.7 2026-08-04). The
    // `PowerChipData::verb` distinguishes "Demand" from
    // "Produces" and the `insufficient` flag drives the
    // negative-orange tone when the batch would push the
    // active colony's grid into deficit.
    pub power_chip: PowerChipData,
    // The label on the Queue button. v0.5.2: dynamic per
    // multiplier so the Mining card reads "Build +5" instead of
    // just "Queue" — gives the player a quick read of how many
    // copies one click will enqueue. Build cards keep the
    // simpler "Queue" label (the player has 6 fixed chips to
    // pick from, the chip itself shows the value).
    pub queue_label: String,
    // `true` if the batch's total power demand exceeds the active
    // colony's grid surplus. The Queue button reads this and adds
    // `ConstructionCtaDisabled` so the player can't push a build
    // the grid can't power; the tooltip system reads it to show
    // "not enough energy".
    ///
    // `false` when the building doesn't draw grid power, when
    // no colony is selected (menu-screen previews), or when the
    // batch fits inside the grid.
    pub power_insufficient: bool,
    // v0.5.2 (build menu fix): `true` when the building can't be
    // built on the current body (mining tab only — e.g. an Iron
    // Mine on a gas giant). The Queue button is disabled with a
    // `ConstructionCtaBodyBlocked` marker so the per-frame
    // affordability system (`tick_construction_cta_disabled`)
    // doesn't silently re-enable it when the player happens to
    // have the resources on hand. `false` for every Build tab
    // card.
    pub body_blocked: bool,
    // v0.5.2 (2026-08-06): `true` for the Buildings tab's
    // constructed-building cards. `spawn_card` skips the Queue
    // CTA + the ETA row (and the CTA bottom-padding reservation)
    // when set — a constructed card has no "build" action and no
    // ETA (it's already built). The Demolish button is the only
    // action. `false` for Build + Mining cards.
    pub constructed: bool,
}

// Build a `BuildCardData` from a `BuildingDefinition`. This is the
// single conversion function used by the canary — all building
// cards are derived from real `BuildingsData`, no hard-coding.
pub fn card_data_from_definition(
    building_type: BuildingType,
    def: &BuildingDefinition,
) -> BuildCardData {
    card_data_with_multiplier(building_type, def, 1, 0.0)
}

// Build a `BuildCardData` with the player's chosen build multiplier
// factored into the cost/ETA display. The `multiplier` parameter scales
// the resource costs and workforce in place — no extra "Total ×N" line
// is appended. Per user feedback 2026-08-02: when the player picks
// "x25", every existing effect bullet reflects the batched amount
// (e.g. "Iron 250k/t" instead of "Iron 10k/t" plus a separate
// "Total ×25" line).
///
// `spare_power_mw` is the active colony's grid surplus (produced
// minus consumed, in MW). The Power effect line uses it to color-
// code insufficient batches (red) vs fitting ones (green) so the
// player sees at a glance whether the multiplier will push the grid
// into deficit. Pass 0.0 if no colony is active (e.g. menu-screen
// previews) — the line still reads cleanly.
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
    // Power chip (v0.5.2, 2026-08-06): shared builder extracted from
    // the three card-data builders. The Build tab maps its spare-power
    // sentinel (0.0 = no colony) onto `Option`: a positive surplus is
    // `Some`, anything ≤ 0 (no colony OR a deficit) is `None` — the
    // chip then reads "No active colony" and never gates on a deficit
    // it can't distinguish from "no colony".
    let power_chip = build_power_chip_data(
        def,
        mult,
        if spare_power_mw > 0.0 {
            Some(spare_power_mw)
        } else {
            None
        },
    );
    // `power_insufficient` drives the Queue-button disable. It matches
    // the chip's `insufficient` (the shared builder computes the same
    // "batch demand > spare" predicate, `false` for generators and
    // no-colony cases).
    let power_insufficient = power_chip.insufficient;

    // v0.5.2 canary fix: surface the building's `*Production`
    // modifier (per-mine yield) as the most prominent non-Power
    // effect. Without this, the cost bullets (Iron, Copper, ...)
    // fill the visible card area and the actual produced resource
    // is buried in the description.
    //
    // v0.5.2 PR-A.7 (2026-08-04): the Power line is no longer
    // pushed to `effects` — it now lives on a dedicated
    // `power_chip` chip rendered separately on the card. The
    // insertion rule for Production simplifies to "always at
    // index 0" (before any other effect). The previous code did
    // `if power_demand_mw > 0.01 { insert_at = 1 }` on the
    // assumption that a Power effect was sitting at `effects[0]`,
    // which is no longer true — leaving the conditional in
    // caused a `Vec::insert(1)` panic on an empty vec at startup
    // for any building that has both a non-zero `power_demand_mw`
    // and a `*Production` modifier.
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
                // v0.5.2 (2026-08-03): `format_mining_rate` already
                // trails its band in `/yr` (e.g. `120 kt/yr`), so we
                // do NOT append another `/yr` after the resource
                // name — that would produce `120 kt/yr Iron/yr`. The
                // formatter owns the suffix; the resource name is
                // the bare noun (`Iron`).
                let line = if mult > 1.0 {
                    format!(
                        "Produces {} \u{00d7} {} = {} {}",
                        format_mining_rate(per_unit),
                        mult as u32,
                        format_mining_rate(total),
                        res_name
                    )
                } else {
                    format!("Produces {} {}", format_mining_rate(per_unit), res_name)
                };
                // PR-A.7 fix: insert at the head of the effects
                // vec. Power is now a separate chip, so there is
                // no "after Power" slot to target.
                effects.insert(0, (EffectTone::Positive, line));
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
        // readout moved to a dedicated Power chip in PR-A.7
        // (2026-08-04), see `power_chip` below.
        stat_c: ("", String::new()),
        effects,
        // v0.5.2 PR-A.4 follow-up: typed resource-demand rows
        // rendered with PNG icon + category tint (see
        // `resource_costs` doc). Always passed alongside
        // `effects`; the canary renders the two sets in
        // separate visual zones (Power chip → Produces effect
        // → [resource_cost rows]).
        resource_costs,
        // v0.5.2 PR-A.7 (2026-08-04): the power readout now
        // lives in a dedicated chip with the bolt-in-hex icon
        // (`energy.png`) and a hover tooltip, mirroring the
        // resource_cost chip pattern. See `PowerChipData` for
        // the field semantics.
        power_chip,
        queue_label: "Queue".to_string(),
        power_insufficient,
        // v0.5.2 (build menu fix): Build tab cards are never body-blocked.
        // The field exists for spawn_card's benefit so it can
        // distinguish spawn-time-true body-blocked mining cards
        // from resource-driven disabled states.
        body_blocked: false,
        build_points: def.build_points,
        // v0.5.2 (2026-08-06): Build-tab cards are NOT constructed —
        // they show the buildable catalog with a Queue CTA + ETA.
        constructed: false,
    }
}

// Clamp a description string to roughly two lines of caption-size text
// (12 px) inside a card column of ~145 px width —— ~80 chars at the
// prototype's character density. Appends an ellipsis when truncated so
// the player knows the description continues. The 80-char budget keeps
// every card subtitle visually consistent and prevents the effect
// bullets below from being pushed off the card.
fn clamp_subtitle_two_lines(s: &str) -> String {
    const MAX_CHARS: usize = 80;
    if s.chars().count() <= MAX_CHARS {
        return s.to_string();
    }
    let mut out: String = s.chars().take(MAX_CHARS - 1).collect();
    out.push('…');
    out
}

// Format a mining production rate (Mt/yr) with a human-readable
// unit suffix. Mirrors the helper in `src/ui/construction_panel.rs`
// (egui Mining tab) so the canary and the legacy panel agree on
// the same scale labels.
///
// | Range (Mt/yr)     | Suffix       | Example                     |
// |-------------------|--------------|-----------------------------|
// | < 1e-12           | "0"          | (effect suppressed upstream)|
// | 1e-12 .. 1e-9     | g/yr         | "100 g/yr" Gold             |
// | 1e-9 .. 1e-6      | kg/yr        | "500 kg/yr" Platinum        |
// | 1e-6 .. 1e-3      | t/yr         | "500 t/yr" RareEarths       |
// | 1e-3 .. 1         | kt/yr        | "120 kt/yr" Iron            |
// | 1 .. 1e3          | Mt/yr        | "120 Mt/yr" Silicates       |
// | 1e3 .. 1e6        | Gt/yr        | "120 Gt/yr" Water           |
// | 1e6 .. 1e9        | Tt/yr        | "1.20 Tt/yr" Carbon         |
// | 1e9 .. 1e12       | Pt/yr        | "1.20 Pt/yr" (planet bulk)  |
// | 1e12 .. 1e15      | Et/yr        | "1.20 Et/yr" (planet bulk)  |
// | 1e15 .. 1e18      | Zt/yr        | "5.97 Zt/yr" Earth          |
// | ≥ 1e18            | Yt/yr        | (stellar-scale)             |
fn format_mining_rate(mt_per_year: f64) -> String {
    if mt_per_year.abs() < 1e-12 {
        return "0".to_string();
    }
    let v = mt_per_year.abs();
    // SI ladder for **Mt** input. Boundaries land each suffix in
    // the 1..=999 range (one- to three-digit display). SI mass
    // prefixes:
    //   1 g  = 1e-12 Mt     1 Mt = 1    Mt
    //   1 kg = 1e-9  Mt     1 Gt = 1e3  Mt
    //   1 t  = 1e-6  Mt     1 Tt = 1e6  Mt
    //   1 kt = 1e-3  Mt     1 Pt = 1e9  Mt
    //                        1 Et = 1e12 Mt
    //                        1 Zt = 1e15 Mt
    //                        1 Yt = 1e18 Mt
    if v < 1e-9 {
        // grams (1..999 g)
        format!("{:.0} g/yr", mt_per_year * 1e12)
    } else if v < 1e-6 {
        // kilograms (1..999 kg)
        format!("{:.0} kg/yr", mt_per_year * 1e9)
    } else if v < 1e-3 {
        // tonnes (1..999 t)
        format!("{:.0} t/yr", mt_per_year * 1e6)
    } else if v < 1.0 {
        // kilotonnes (1..999 kt)
        format!("{:.0} kt/yr", mt_per_year * 1e3)
    } else if v < 1e3 {
        // megatonnes (1..999 Mt)
        format!("{:.0} Mt/yr", mt_per_year)
    } else if v < 1e6 {
        // gigatonnes (1..999 Gt)
        format!("{:.0} Gt/yr", mt_per_year / 1e3)
    } else if v < 1e9 {
        // teratonnes (1..999 Tt)
        format!("{:.2} Tt/yr", mt_per_year / 1e6)
    } else if v < 1e12 {
        // petatonnes (1..999 Pt)
        format!("{:.2} Pt/yr", mt_per_year / 1e9)
    } else if v < 1e15 {
        // exatonnes (1..999 Et)
        format!("{:.2} Et/yr", mt_per_year / 1e12)
    } else if v < 1e18 {
        // zettatonnes (Earth-class planetary-bulk deposits)
        format!("{:.2} Zt/yr", mt_per_year / 1e15)
    } else {
        // yottatonnes — stellar-mass scale, theoretical
        format!("{:.2} Yt/yr", mt_per_year / 1e18)
    }
}

// ── Mining tab (v0.5.2 PR-A.2) ────────────────────────────────────
//
// One card per mine / AutoMine (24 base + 25 orbital = 49 total).
// Per-card [-] [+] buttons push to `PendingConstructionActions::
// mining_edits` (positive=add, negative=remove). The actual
// inventory edit is handled by `process_construction_actions`
// in `src/colony/systems.rs` — this file owns only the UI.

// Per-card derived data for the Mining tab. Computed once per
// card per frame; cheap (one `HashMap::get` per resource + a
// modifier scan). The `count` lives on `Colony::buildings` and
// is read directly in the spawn loop, not pre-computed here.
#[derive(Debug, Clone, Copy)]
pub struct MiningCardData {
    // Per-build base yield in Mt/yr, from the building's
    // `*Production` modifier. 0.0 if the building has no
    // production modifier (e.g. He3Mine with no per-build yield
    // listed; rare).
    pub base_yield_mt_per_year: f64,
    // Per-resource deposit accessibility on the active body
    // (0.0-1.0). 0.0 if no deposit for this resource on the body.
    pub accessibility: f32,
    // Total reserves on the body in Mt (proven + deep + bulk
    // rolled up). 0.0 if no deposit.
    pub reserve_mt: f64,
}

impl MiningCardData {
    // Total per-year production for `count` builds of this card.
    // Per-mine yield × count × body accessibility.
    pub fn production_mt_per_year(&self, count: u32) -> f64 {
        count as f64 * self.base_yield_mt_per_year * self.accessibility as f64
    }
}

// Compute `MiningCardData` for `(building, deposits)`.
///
// Strips `Production` from the first matching modifier to recover
// the produced `ResourceType`, then reads the deposit's
// `accessibility` and reserves from `PlanetResources::deposits`.
// Returns zeroed data if either side is missing.
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

// Format a total-reserve value with a scale suffix
// (g/kg/t/kt/Mt/Gt/Tt/Pt/Et/Zt). Mirrors `format_mining_rate` for
// consistency — input is in **Mt** and the ladder picks the
// smallest suffix that lands the displayed value in the 1..=999
// range. Used in the "Available Deposits:" row on each Mining card.
pub fn format_mining_reserve(total_mt: f64) -> String {
    if total_mt.abs() < 1e-12 {
        return "0".to_string();
    }
    let v = total_mt.abs();
    // Same SI ladder as `format_mining_rate`, minus the `/yr`
    // suffix. Boundaries land each unit in the 1..=999 range.
    if v < 1e-9 {
        format!("{:.0} g", total_mt * 1e12)
    } else if v < 1e-6 {
        format!("{:.0} kg", total_mt * 1e9)
    } else if v < 1e-3 {
        format!("{:.0} t", total_mt * 1e6)
    } else if v < 1.0 {
        format!("{:.0} kt", total_mt * 1e3)
    } else if v < 1e3 {
        format!("{:.0} Mt", total_mt)
    } else if v < 1e6 {
        format!("{:.0} Gt", total_mt / 1e3)
    } else if v < 1e9 {
        format!("{:.2} Tt", total_mt / 1e6)
    } else if v < 1e12 {
        format!("{:.2} Pt", total_mt / 1e9)
    } else if v < 1e15 {
        format!("{:.2} Et", total_mt / 1e12)
    } else if v < 1e18 {
        format!("{:.2} Zt", total_mt / 1e15)
    } else {
        format!("{:.2} Yt", total_mt / 1e18)
    }
}

// Format a power value (MW) with a human-readable SI suffix
// ladder (W / kW / MW / GW / TW).
///
// v0.5.2 (2026-08-03): per user feedback, the legacy inline
// `format!("{:.0} MW", total_mw)` always rendered in megawatts
// regardless of magnitude — a 5 GW fusion plant read as
// `5000 MW` instead of `5 GW`. This formatter picks the smallest
// suffix that lands the displayed value in the 1..=999 range.
///
// Input is in **megawatts (MW)**. SI power prefixes:
///
// | 1 W  = 1e-6 MW  | 1 MW = 1   MW |
// | 1 kW = 1e-3 MW  | 1 GW = 1e3  MW |
// |                  | 1 TW = 1e6  MW |
// Strip trailing zeros from a formatted number so `"5.0 GW"`
// becomes `"5 GW"`, `"12.50 TW"` becomes `"12.5 TW"`, and
// `"0.10"` becomes `"0.1"`. The dot itself is dropped when
// all fractional digits are zero. The input is the *full*
// formatted string (number + space + unit suffix); we split
// at the first space, strip zeros from the numeric half,
// then rejoin.
//
// v0.5.2 (build menu fix): the previous implementation did
// `s.trim_end_matches('0').trim_end_matches('.')` which only
// worked for inputs without a unit suffix (the trailing
// character of `"5.0 GW"` is `'W'`, not `'0'`, so the trim
// was a no-op). Splitting on the space first fixes the
// format_power integration.
fn strip_trailing_zeros(s: String) -> String {
    if !s.contains('.') {
        return s;
    }
    // Split at the first space — everything before is the
    // numeric portion, everything after is the unit suffix
    // (e.g. " GW", " MW").
    let (num, suffix) = match s.find(' ') {
        Some(i) => (&s[..i], &s[i..]),
        None => (s.as_str(), ""),
    };
    let trimmed = num.trim_end_matches('0').trim_end_matches('.');
    if suffix.is_empty() {
        trimmed.to_string()
    } else {
        format!("{}{}", trimmed, suffix)
    }
}

pub fn format_power(total_mw: f64) -> String {
    if total_mw.abs() < 1e-6 {
        return "0 W".to_string();
    }
    let v = total_mw.abs();
    if v < 1e-3 {
        // watts (1..999 W)
        format!("{:.0} W", total_mw * 1e6)
    } else if v < 1.0 {
        // kilowatts (1..999 kW)
        format!("{:.0} kW", total_mw * 1e3)
    } else if v < 1e3 {
        // megawatts (1..999 MW)
        format!("{:.0} MW", total_mw)
    } else if v < 1e6 {
        // gigawatts (1..999 GW) — one decimal, trailing zeros stripped
        strip_trailing_zeros(format!("{:.1} GW", total_mw / 1e3))
    } else if v < 1e9 {
        // terawatts (1..999 TW) — two decimals, trailing zeros stripped
        strip_trailing_zeros(format!("{:.2} TW", total_mw / 1e6))
    } else {
        // petawatts - colony-scale totals only — two decimals, trailing zeros stripped
        strip_trailing_zeros(format!("{:.2} PW", total_mw / 1e9))
    }
}

// Build the list of `BuildCardData` for the Build sub-tab, filtered by
// the active category + `BuildFilter`, sorted by `build_points` ascending.
///
// `category_index == 0` is the "Infrastructure" tab, `1` is "Industry", etc.
// The last index (8 in the 8-category case) is the "Locked" tab — we
// return the locked buildings instead of the unlocked ones.
///
// `multiplier` is the player's chosen build-multiplier (1, 5, 10, 25,
// 50, 100). It is passed through to `card_data_with_multiplier` so the
// rendered card shows the batch cost / total BP for the whole batch.
///
// `spare_power_mw` is the active colony's grid surplus (produced -
// consumed, in MW). Forwarded to `card_data_with_multiplier` so each
// card's Power effect line can show "demand vs spare" with a
// green/red sufficient/insufficient marker. v0.5.2 PR-A.2.
///
// `research_state` is used to decide which tech-gated buildings are
// visible. The legacy version used `required_tech_opt().is_none()` which
// hid every building with a tech requirement; the canary mirrors the
// egui panel's behavior (only show buildings whose required tech is
// absent or already unlocked).
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
        .map(|(bt, def)| {
            (
                bt,
                card_data_with_multiplier(bt, def, multiplier, spare_power_mw),
            )
        })
        .collect()
}

// Parse the data file's `category: String` into a `BuildingCategory`
// enum. Returns `None` for unknown categories (defensive).
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

// Convert the `selected_build_tab: usize` into a `BuildingCategory`.
// Index 8 maps to `None` (the "All" chip in the filter row).
// Any other out-of-range index also maps to `None`.
///
// v0.5.2 PR-A.2 (round 2): the Mining chip is removed from the
// Build tab's category row. Mines are now managed exclusively
// in the dedicated Mining tab; the Build tab's category
// indices renumber to 0..7 with "All" at index 8. The internal
// `BuildingCategory::Mining` variant is preserved (the
// Mining tab still uses it; `parse_category` still maps the
// RON `"Mining"` string to it) but it has no chip in the
// Build tab UI.
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

// Marker component on the **root** of each sub-tab body. The canary
// spawns one of these per sub-tab (Overview / Buildings / Build /
// Stockpiles) at setup time, all as children of the `ConstructionRoot`.
// The `tick_construction_body_visibility` system makes exactly one
// visible (the one matching `ui_state.selected_tab`) every frame,
// keeping the others hidden via `Visibility::Hidden`.
///
// The bodies are siblings, not nested — each sub-tab body is its own
// flex container at the same level as the card grid. The canary's
// card grid (the Build tab body) carries `BuildBody`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstructionTabBody {
    Overview,
    Buildings,
    Build,
    // v0.5.2: replaces v0.5.x `Stockpiles` (the minimum-stockpile
    // editor) with the dedicated mining grid.
    Mining,
}

impl ConstructionTabBody {
    // Map to the matching `ConstructionTab` enum.
    pub fn from_tab(tab: ConstructionTab) -> Self {
        match tab {
            ConstructionTab::Overview => Self::Overview,
            ConstructionTab::Buildings => Self::Buildings,
            ConstructionTab::Build => Self::Build,
            ConstructionTab::Mining => Self::Mining,
        }
    }

    // Reverse mapping for the visibility system.
    pub fn tab(&self) -> ConstructionTab {
        match self {
            Self::Overview => ConstructionTab::Overview,
            Self::Buildings => ConstructionTab::Buildings,
            Self::Build => ConstructionTab::Build,
            Self::Mining => ConstructionTab::Mining,
        }
    }
}

// Marker for chrome elements that should only be visible on the
// Build and Buildings tabs. Used by `tick_construction_body_visibility`
// to flip `Node::display` so e.g. the category filter row stays
// hidden on Overview and Mining where there are no card grids to
// filter. v0.5.2: the chrome was lifted out of the per-tab bodies
// into a shared `shared_chrome` container — this marker is the
// per-element visibility hook for chrome that only some tabs need.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShowOnBuildOrBuildings;

// System: make the body matching `ui_state.selected_tab` visible and
// hide the others. Touches `Node::display` + `Visibility` so it runs
// cheaply every frame — the bodies themselves are spawned once at
// startup.
///
// **Visibility inheritance**: Bevy 0.18's `Visibility::Visible` renders
// the entity unconditionally (ignoring the parent's visibility). The
// canary root toggles between `Hidden` and `Visible` based on whether
// the Construction menu is open. If the active body used
// `Visibility::Visible`, it would render on the main menu / other
// menus too — the canary cards would show in front of the main menu.
// Setting the active body to `Visibility::Inherited` makes its
// visibility follow the root, so the cards only render when the
// root is visible.
///
// **Layout isolation**: `Visibility::Hidden` is a render-only flag —
// hidden nodes still occupy their taffy-computed box and steal
// `flex_grow: 1.0` siblings' share of the remaining height. With four
// bodies each carrying `flex_grow: 1.0`, the active body was being
// squeezed to ~25% of the remaining height and the card grid clipped
// every card to its first ~15 px (the documented "single-pixel-tall
// row" symptom). Toggling `Node::display` between `Display::Flex`
// (active) and `Display::None` (inactive) removes the inactive
// bodies from the layout entirely so the active body gets the full
// remaining height.
pub fn tick_construction_body_visibility(
    ui_state: Res<ConstructionUiState>,
    mut body_query: Query<(&ConstructionTabBody, &mut Node, &mut Visibility)>,
    mut show_on_build_or_buildings: Query<
        &mut Node,
        (
            With<ShowOnBuildOrBuildings>,
            Without<ConstructionTabBody>,
            Without<ConstructionScrollbarTrack>,
            Without<ConstructionScrollbarThumb>,
        ),
    >,
    mut scrollbar_track: Query<
        (&ConstructionScrollbarTrack, &mut Node),
        (
            Without<ConstructionTabBody>,
            Without<ShowOnBuildOrBuildings>,
            Without<ConstructionScrollbarThumb>,
        ),
    >,
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
    // The shared chrome (output row, picker, build qty chips) is always
    // visible. Elements that only make sense on Build + Buildings
    // (currently the filter row) carry `ShowOnBuildOrBuildings` and
    // are flipped off on Overview / Mining.
    let show_chrome_subset = matches!(
        active,
        ConstructionTabBody::Build | ConstructionTabBody::Buildings
    );
    for mut node in show_on_build_or_buildings.iter_mut() {
        node.display = if show_chrome_subset {
            Display::Flex
        } else {
            Display::None
        };
    }
    // The custom scrollbar tracks + thumbs are parented to the
    // Construction menu root (not to the scrollable body — see
    // the commentary at `setup_construction` line ~4107). Without
    // this flip they stay visible on every tab, where they (a)
    // read as stray vertical pills on Overview / Buildings /
    // Stockpiles and (b) intercept hover events that should land
    // on the cards behind them. v0.5.2 PR-A.8: each scrollbar
    // carries a `tab` field so we can show only the matching
    // scrollbar. Build + Buildings show the card-grid scrollbar;
    // Mining shows its own body scrollbar; Overview / Stockpiles
    // show nothing (their bodies fit on a single screen).
    for (track, mut node) in scrollbar_track.iter_mut() {
        let visible = track.tab == active;
        node.display = if visible { Display::Flex } else { Display::None };
    }
    // The thumb is a direct child of the track entity. When the
    // track flips to `Display::None` its children inherit Hidden
    // via Bevy's normal hierarchy propagation — no separate
    // visibility flip on the thumb is required.
}

// ── Sub-tab body spawn helpers ────────────────────────────────────────

// Build the **Overview** body. Read-only summary of the active colony:
// name, population, BP/yr, queue count. The body is a single column
// of label/value rows inside a `ConstructionTabBody::Overview` root.
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
                // v0.5.2 (2026-08-06, bugfix): `min_height: 0` is the
                // flexbox shrink fix — WITHOUT it this wrapper refuses
                // to shrink below its intrinsic content height, so the
                // inner `Overflow::scroll_y` container grows to fit its
                // content and never overflows (no scroll, no thumb).
                min_height: Val::Px(0.0),
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

    // Queue section scroll container (v0.5.2, 2026-08-06): shared
    // `Overflow::scroll_y` + `min_height: 0` + `flex_grow: 1` helper
    // from `widgets::spawn_scrollable_container`. The entity is found
    // by the `OverviewQueueContent` marker; no id binding needed here.
    spawn_scrollable_container(
        commands,
        body,
        "overview_queue_content",
        SPACE_XS,
        OverviewQueueContent,
    );

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

// Marker component on the value text of a single Overview row.
// Carries the semantic role so the `update_overview_body` system
// can find each row by its role (Colony / Population / etc.) and
// update the text content every frame.
#[derive(Component)]
pub struct OverviewRowValue {
    pub kind: OverviewRowKind,
}

// Identifies which semantic row the value text belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverviewRowKind {
    Colony,
    Population,
    ActiveConstruction,
    UniqueBuildingTypes,
}

// Marker on the Overview body's queue content container. The
// `update_overview_queue` system despawns the rows inside this
// container and re-spawns them based on the selected colony's
// `ConstructionProject`s every frame.
#[derive(Component)]
pub struct OverviewQueueContent;

// Update the Overview body's queue section every frame.
///
// Spawn-once-update-many (v0.5.2): rows persist across frames.
// Each frame:
// 1. Despawn rows whose project is no longer live (project
//    completed, cancelled, or its `colony_entity` no longer matches
//    the selected colony).
// 2. Mutate the text + fill width on existing rows in place.
// 3. Spawn rows for projects we haven't seen before.
///
// The `Local<HashMap<Entity, OverviewQueueRow>>` cache is keyed by
// the project entity and carries the row's child-node IDs so the
// per-frame updates are direct `Query::get_mut` lookups — no
// `Children` / `ChildOf` walks.
///
// The "no active construction projects" placeholder is owned by a
// separate `Local<Option<Entity>>` so it can be toggled without
// coupling it to the project list.
pub fn update_overview_queue(
    mut commands: Commands,
    fonts: Res<UiFonts>,
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
    let Ok(content) = content_query.single() else {
        return;
    };

    // Fonts come from the cached `UiFonts` resource (v0.5.2,
    // 2026-08-06) — no per-frame `asset_server.load` lookup.
    let body_font: Handle<Font> = fonts.body.clone();
    let body_font_medium: Handle<Font> = fonts.medium.clone();
    let mono_font: Handle<Font> = fonts.mono.clone();

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
        spawned_rows.insert(
            *project_entity,
            OverviewQueueRow {
                project_entity: *project_entity,
                row,
                name_text,
                status_text,
                progress_text,
                progress_fill,
            },
        );
    }
}

// Marker on each row in the Overview body's queue section. Carries
// the project entity plus the child-node entity IDs so the
// `update_overview_queue` system can mutate the progress label,
// status text, and fill bar in place via direct `Query::get_mut`
// lookups instead of walking `Children` / `ChildOf` chains.
///
// Spawn-once-update-many refactor (v0.5.2): rows persist across
// frames; the system diffs the live project set against the cached
// map and only despawns rows whose project is gone, only spawns
// rows for new projects.

// Marker component on the `Text` node that holds the building
// display name within a queued row. Used by `update_overview_queue`
// to `Query::get_mut` the text in place each frame.
#[derive(Component)]
pub struct OverviewQueueRowNameChild;

// Marker component on the `Text` node that holds the human-readable
// status ("Building" / "Awaiting delivery") within a queued row.
#[derive(Component)]
pub struct OverviewQueueRowStatusChild;

// Marker component on the `Text` node that holds the formatted
// "{:.0}%" progress label within a queued row.
#[derive(Component)]
pub struct OverviewQueueRowProgressChild;

// Marker component on the `Node` whose `width` encodes the
// progress fill (0 % – 100 % of the track) within a queued row.
#[derive(Component)]
pub struct OverviewQueueRowFillChild;

#[derive(Component)]
pub struct OverviewQueueRow {
    pub project_entity: Entity,
    // The row entity itself. Stored so the despawn step can drop
    // the whole subtree in one `commands.entity(...).despawn()`
    // call (which cascade-despawns the header + track + all their
    // children).
    pub row: Entity,
    // The `Text` node holding the building display name.
    pub name_text: Entity,
    // The `Text` node holding the human-readable status
    // ("Building" / "Awaiting delivery").
    pub status_text: Entity,
    // The `Text` node holding the formatted "{:.0}%" progress label.
    pub progress_text: Entity,
    // The `Node` overlay whose width encodes progress
    // (0 % – 100 % of the track).
    pub progress_fill: Entity,
}

// Update the Overview body's four value rows every frame so the
// colony summary reflects the current `selected_colony` state. The
// body is spawned once at startup with placeholder text; this
// system overwrites the value text each frame with live data.
pub fn update_overview_body(
    ui_state: Res<ConstructionUiState>,
    colonies: Query<(Entity, &crate::colony::Colony)>,
    projects: Query<&crate::colony::ConstructionProject>,
    mut value_query: Query<(&OverviewRowValue, &mut Text, &mut TextColor)>,
) {
    // Resolve the selected colony + its project count.
    let selected_colony = ui_state.selected_colony;
    let colony_data = selected_colony.and_then(|e| {
        colonies
            .iter()
            .find(|(ce, _)| *ce == e)
            .map(|(_, c)| c.clone())
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
                let c = if project_count == 0 {
                    GREEN_OK
                } else {
                    YELLOW_ETA
                };
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

// Build the **Buildings** body. A persistent container with a header
// + a content scroll area. The `update_buildings_body` system
// re-spawns the content rows every frame based on the selected
// colony's `buildings` HashMap, so the list reflects the live state.
fn spawn_buildings_body(commands: &mut Commands, parent: Entity, body_font_medium: &Handle<Font>) {
    let body = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                width: Val::Percent(100.0),
                flex_grow: 1.0,
                // v0.5.2 (2026-08-06, bugfix): `min_height: 0` — see
                // the overview_body comment. Without it the inner
                // `buildings_content` scroll container never overflows.
                min_height: Val::Px(0.0),
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
    // the cards inside this container every frame so the list reflects
    // the live colony state.
    //
    // v0.5.2 (Buildings tab as a card list): the body now uses the
    // same flex-wrap card grid as the Build tab (`card_grid`)
    // so the existing `spawn_card` helper can render each building.
    // Cards are 320 px wide (same as Build/Mining cards) and wrap
    // to the next row on overflow. `min_height: 0` is the Bevy 0.18
    // flex fix that lets `Overflow::scroll_y` engage when the card
    // count exceeds the viewport height (without it, the flex item
    // refuses to shrink below its intrinsic content height, so the
    // scrollbar thumb is always full-size and the wheel handler no-ops).
    let content = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                align_content: AlignContent::Start,
                align_items: AlignItems::Stretch,
                width: Val::Percent(100.0),
                flex_grow: 1.0,
                min_height: Val::Px(0.0),
                row_gap: Val::Px(SPACE_LG),
                column_gap: Val::Px(SPACE_LG),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
            Visibility::Inherited,
            Name::new("buildings_content"),
            BuildingsContent,
        ))
        .id();
    commands.entity(body).add_child(content);
}

// Marker on the Buildings body's header text so the update system
// can update just the header without re-spawning it.
#[derive(Component)]
pub struct BuildingsHeader;

// Marker on the Buildings body's content container. The
// `update_buildings_body` system walks the children of this
// container via `Children` and re-spawns them every frame.
#[derive(Component)]
pub struct BuildingsContent;

// Build a `BuildCardData` for a building already constructed in the
// active colony. Mirrors `build_mine_card_data` but reads the
// live colony inventory (current count) for the stats row, and
// uses the player's `build_multiplier` (not the mining
// multiplier) for the Demolish button label.
///
// v0.5.2 (Buildings tab as a card list): the Buildings tab now
// renders each constructed building as a card with quantity,
// output production, power consumption, and a Demolish button.
// The card has no Build CTA — the player already built these
// buildings, so they're not buildable from this tab. The Demolish
// button uses `build_multiplier` so the player's x1/x5/x25 chip
// choice carries straight through.
///
// `count` is the live inventory count on the active colony (from
// `Colony::buildings[bt]`).
pub fn build_constructed_card_data(
    bt: BuildingType,
    def: &BuildingDefinition,
    count: u32,
    multiplier: u32,
) -> BuildCardData {
    let _ = multiplier; // the Demolish label scales per-frame from `build_multiplier`; not needed here.

    // Stats row: current count (left). The right-hand stat is left
    // empty — v0.5.2 (2026-08-06): the old "Demolish -N" text in
    // the header is removed (the Demolish button itself carries the
    // batch size, and it updates live from the qty chip).
    let count_label = format!("\u{00d7}{}", count);
    let bp_str = count_label;

    // Effects: production (if any), summed across the built count.
    // v0.5.2 (2026-08-06): the Power line is NOT pushed to effects —
    // the power chip below carries the aggregated demand/production.
    let mut effects: Vec<(EffectTone, String)> = Vec::new();
    // Production: "X Mt/yr Iron" — total = per-unit × count (the
    // actual built inventory, NOT the build-multiplier).
    if let Some(prod) = def
        .modifiers
        .iter()
        .find(|m| m.modifier_type.ends_with("Production"))
    {
        if prod.value > 0.0 {
            if let Some(res_name) = prod.modifier_type.strip_suffix("Production") {
                let per_unit = prod.value;
                let total = per_unit * count as f64;
                let line = if count > 1 {
                    format!(
                        "Produces {} \u{00d7} {} = {} {}",
                        format_mining_rate(per_unit),
                        count,
                        format_mining_rate(total),
                        res_name
                    )
                } else {
                    format!("Produces {} {}", format_mining_rate(per_unit), res_name)
                };
                effects.push((EffectTone::Positive, line));
            }
        }
    }

    // Power chip: the AGGREGATED power across the built count.
    //
    // v0.5.2 (2026-08-06): the old chip was a per-unit no-op
    // ("Already built", 0 W). Now it sums the stock:
    // - Net generator: `+{count × gen_mw}` green ("Produces").
    // - Net consumer:  `-{count × demand_mw}` red ("Demand").
    // The `per_unit_mw` sign drives the chip's red/green + sign
    // prefix (see `spawn_card`), and `amount` is the total.
    let power_output_gw_per_unit: f64 = def
        .modifiers
        .iter()
        .filter(|m| m.modifier_type == "PowerGeneration")
        .map(|m| m.value)
        .sum();
    let power_chip = if power_output_gw_per_unit > 0.0 {
        let per_unit_mw = power_output_gw_per_unit * 1_000.0;
        let total_mw = per_unit_mw * count as f64;
        PowerChipData {
            verb: "Produces",
            amount: format_power(total_mw),
            // Signed per-unit × count so the render treats the whole
            // stock as one producer (green "+N").
            per_unit_mw: total_mw,
            multiplier: count.max(1) as u32,
            spare_mw: None,
            insufficient: false,
            tooltip_lines: vec![
                format!("Total generation: {}", format_power(total_mw)),
                format!("{} built \u{00d7} {} each", count, format_power(per_unit_mw)),
                "Net surplus to the grid".to_string(),
            ],
        }
    } else if def.power_demand_mw.abs() < 0.01 {
        PowerChipData {
            verb: "Power",
            amount: "0 W".to_string(),
            per_unit_mw: 0.0,
            multiplier: count.max(1) as u32,
            spare_mw: None,
            insufficient: false,
            tooltip_lines: vec!["No grid interaction".to_string()],
        }
    } else {
        let per_unit_mw = def.power_demand_mw;
        let total_mw = per_unit_mw * count as f64;
        PowerChipData {
            verb: "Demand",
            amount: format_power(total_mw),
            // Negative × count so the render treats the whole stock
            // as one consumer (red "-N").
            per_unit_mw: -total_mw,
            multiplier: count.max(1) as u32,
            spare_mw: None,
            insufficient: false,
            tooltip_lines: vec![
                format!("Total demand: {}", format_power(total_mw)),
                format!("{} built \u{00d7} {} each", count, format_power(per_unit_mw)),
            ],
        }
    };

    BuildCardData {
        name: def.display_name.clone(),
        subtitle: clamp_subtitle_two_lines(&def.description),
        building_type: bt,
        icon: def.icon.clone(),
        multiplier: multiplier.max(1),
        stat_a: ("\u{00d7}N", bp_str),
        stat_b: ("", String::new()),
        stat_c: ("", String::new()),
        effects,
        // No build cost rows — the building is already built; the player
        // doesn't re-pay for it. The Demolish button is the only action.
        resource_costs: Vec::new(),
        // Aggregated power across the built count (v0.5.2, 2026-08-06).
        power_chip,
        // Placeholder; the Buildings tab skips the Build CTA.
        queue_label: String::new(),
        // The Buildings tab doesn't drive construction so the ETA is
        // irrelevant (and `spawn_card` skips it for constructed cards).
        build_points: 0.0,
        power_insufficient: false,
        // Buildings tab cards are never body-blocked (they show
        // already-built structures, no construction gate).
        body_blocked: false,
        // v0.5.2 (2026-08-06): Buildings-tab cards ARE constructed —
        // `spawn_card` skips the Queue CTA + ETA row.
        constructed: true,
    }
}

// Spawn a single constructed-buildings card on the Buildings tab.
///
// v0.5.2 (Buildings tab as a card list): the Buildings tab uses the
// same `spawn_card` chrome as the Build tab so the cards match
// visually. The Build CTA is removed (we never queue construction
// from these cards) and a Demolish button is added at the
// bottom-right. Returns the card entity id so the caller can cache it.
#[allow(clippy::too_many_arguments)]
fn spawn_constructed_card(
    commands: &mut Commands,
    parent: Entity,
    bt: BuildingType,
    def: &BuildingDefinition,
    count: u32,
    multiplier: u32,
    body_font: &Handle<Font>,
    body_font_medium: &Handle<Font>,
    mono_font: &Handle<Font>,
    icon: Option<&Handle<Image>>,
    resource_icons: &ResourceIcons,
) -> Entity {
    let data = build_constructed_card_data(bt, def, count, multiplier);
    // Constructed cards use the same visual chrome as build cards.
    // `spawn_card` always creates a Queue CTA; it is harmless here because
    // constructed cards do not carry a construction request handler, and the
    // Demolish action is the functional control for this tab.
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
    spawn_demolish_button(
        commands,
        card,
        bt,
        count,
        multiplier,
        body_font_medium,
        DemolishMultiplierSource::Build,
    );
    card
}

// Update the Buildings body every frame.
///
// v0.5.2 (Buildings tab as a card list): spawns one card per
// `BuildingType` in the selected colony's `buildings` HashMap.
// Each card shows the building count, current production, power
// consumption, and a Demolish button. The cards use the same
// `spawn_card` chrome as the Build tab cards, so the visual
// language is consistent across all three content tabs.
///
// Spawn-once-update-many (v0.5.2): cards persist across frames.
// Each frame:
// 1. Despawn cards whose `BuildingType` is no longer present in
//    the selected colony's `buildings` map.
// 2. The Demolish button reads the live count each frame via
//    `tick_demolish_disabled`, so the count display is always
//    correct without re-spawning the card.
// 3. Spawn cards for `BuildingType`s we haven't seen before.
pub fn update_buildings_body(
    mut commands: Commands,
    fonts: Res<UiFonts>,
    ui_state: Res<ConstructionUiState>,
    buildings_data: Res<BuildingsData>,
    building_icons: Option<Res<BuildingIcons>>,
    resource_icons: Option<Res<ResourceIcons>>,
    colonies: Query<(Entity, &crate::colony::Colony)>,
    content_query: Query<Entity, With<BuildingsContent>>,
    mut spawned_cards: Local<std::collections::HashMap<crate::colony::BuildingType, Entity>>,
    mut header_query: Query<&mut Text, With<BuildingsHeader>>,
    mut empty_placeholder: Local<Option<Entity>>,
    mut no_colony_placeholder: Local<Option<Entity>>,
    // v0.5.2 (2026-08-06): the colony the cached cards were spawned
    // for. `spawned_cards` is keyed by `BuildingType`, so switching
    // colonies would otherwise keep cards from the previous colony
    // (same building types → stale ×N / demolish labels). Clear the
    // cache on colony change so the cards re-spawn for the new one.
    mut last_colony: Local<Option<bevy::ecs::entity::Entity>>,
    // v0.5.2 (2026-08-06): fingerprint of the colony's buildings map.
    // Cards are spawn-once; when the player builds or demolishes, the
    // count / power chip would go stale. Rebuild when the map changes
    // (building + count pairs — not population, which doesn't affect
    // the rendered cards).
    mut last_buildings_sig: Local<Option<u64>>,
) {
    let Ok(content) = content_query.single() else {
        return;
    };

    // Fonts come from the cached `UiFonts` resource (v0.5.2,
    // 2026-08-06) — no per-frame `asset_server.load` lookup.
    let body_font: Handle<Font> = fonts.body.clone();
    let body_font_medium: Handle<Font> = fonts.medium.clone();
    let mono_font: Handle<Font> = fonts.mono.clone();

    // Resolve the selected colony.
    let colony = ui_state
        .selected_colony
        .and_then(|e| colonies.iter().find(|(ce, _)| *ce == e));

    // Header text. `ParamSet` is unnecessary here because there's
    // only one Query<&mut Text> parameter (we removed the
    // quantity-text write loop when we replaced the text rows
    // with cards).
    let header_text = match &colony {
        Some((_, c)) => format!("Constructed Buildings ({})", c.buildings.len()),
        None => "Constructed Buildings".to_string(),
    };
    for mut text in header_query.iter_mut() {
        **text = header_text.clone();
    }

    // Resolve `ResourceIcons` once so the spawn loop doesn't bind the
    // Option<Res<...>> per card. `Option<Res<T>>` is a SystemParam
    // so we read it once and stash the inner map via Res::as_ref.
    let empty_resource_icons = ResourceIcons::default();
    let resource_icons: &ResourceIcons = resource_icons
        .as_ref()
        .map(|r: &Res<ResourceIcons>| -> &ResourceIcons { r.as_ref() })
        .unwrap_or(&empty_resource_icons);

    // Helper: spawn the "(no colony selected)" placeholder.
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
                Text::new(
                    "No buildings yet. Switch to the Build tab to queue your first structure.",
                ),
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
        // Despawn any cached cards and placeholders.
        for (_, card_entity) in spawned_cards.drain() {
            commands.entity(card_entity).try_despawn();
        }
        if let Some(p) = no_colony_placeholder.take() {
            commands.entity(p).try_despawn();
        }
        if let Some(p) = empty_placeholder.take() {
            commands.entity(p).try_despawn();
        }
        *last_colony = None;
        *no_colony_placeholder =
            spawn_no_colony_placeholder(&mut commands, content, body_font.clone(), None);
        return;
    };

    // v0.5.2 (2026-08-06): if the selected colony changed, drop the
    // cached cards so they re-spawn with the new colony's counts.
    // Without this, switching colonies with overlapping building
    // types would keep the previous colony's cards (stale ×N).
    if *last_colony != ui_state.selected_colony {
        for (_, card_entity) in spawned_cards.drain() {
            commands.entity(card_entity).try_despawn();
        }
        *last_colony = ui_state.selected_colony;
        *last_buildings_sig = None;
    }

    // v0.5.2 (2026-08-06): rebuild when the buildings map changes
    // (the player queued or demolished). Cards are spawn-once — the
    // ×N stat + aggregated power chip would otherwise go stale.
    let mut sig: u64 = 0xcbf29ce484222325;
    for (bt, count) in colony.buildings.iter() {
        sig ^= *bt as u64;
        sig = sig.wrapping_mul(0x100000001b3);
        sig ^= *count as u64;
        sig = sig.wrapping_mul(0x100000001b3);
    }
    if *last_buildings_sig != Some(sig) {
        for (_, card_entity) in spawned_cards.drain() {
            commands.entity(card_entity).try_despawn();
        }
        *last_buildings_sig = Some(sig);
    }

    if colony.buildings.is_empty() {
        for (_, card_entity) in spawned_cards.drain() {
            commands.entity(card_entity).try_despawn();
        }
        if let Some(p) = no_colony_placeholder.take() {
            commands.entity(p).try_despawn();
        }
        if let Some(p) = empty_placeholder.take() {
            commands.entity(p).try_despawn();
        }
        *empty_placeholder =
            spawn_empty_placeholder(&mut commands, content, body_font.clone(), None);
        return;
    }

    // We have cards to show — clear both placeholders.
    if let Some(p) = no_colony_placeholder.take() {
        commands.entity(p).try_despawn();
    }
    if let Some(p) = empty_placeholder.take() {
        commands.entity(p).try_despawn();
    }

    // v0.5.2 (2026-08-06): apply the active Build-tab category chip to
    // the Buildings index — the filter previously had no effect here
    // (every constructed building showed regardless of the chip). The
    // "All" chip (index 8) shows every constructed building; 0..8
    // narrows to one `BuildingCategory`. Mines are excluded — they
    // are handled in the dedicated Mining tab.
    let active_category = category_from_index(ui_state.selected_build_tab);
    let filtered: Vec<(BuildingType, u32)> = colony
        .buildings
        .iter()
        .filter(|(bt, _count)| {
            // Skip Mining-category buildings (handled in the Mining tab).
            let def = buildings_data.get(*bt);
            let cat = def.and_then(|d| parse_category(&d.category));
            if cat == Some(BuildingCategory::Mining) {
                return false;
            }
            match active_category {
                Some(c) => cat == Some(c),
                None => true, // "All" chip (index 8)
            }
        })
        .map(|(bt, count)| (*bt, *count))
        .collect();

    // Sort buildings by name for stable presentation.
    let mut entries: Vec<BuildingType> = filtered
        .iter()
        .map(|(bt, _)| *bt)
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| {
        let an = buildings_data
            .get(a)
            .map(|d| d.display_name.as_str())
            .unwrap_or("");
        let bn = buildings_data
            .get(b)
            .map(|d| d.display_name.as_str())
            .unwrap_or("");
        an.cmp(bn)
    });

    let live_keys: std::collections::HashSet<crate::colony::BuildingType> =
        entries.iter().copied().collect();

    // 1. Despawn cards whose BuildingType is gone.
    let to_remove: Vec<crate::colony::BuildingType> = spawned_cards
        .keys()
        .filter(|k| !live_keys.contains(k))
        .copied()
        .collect();
    for key in to_remove {
        if let Some(card_entity) = spawned_cards.remove(&key) {
            commands.entity(card_entity).try_despawn();
        }
    }

    // 2. Spawn cards for BuildingTypes we haven't seen before.
    // The card display is updated in place by `tick_demolish_disabled`
    // and the live `count` reading on the Demolish button label. We
    // don't re-spawn cards for count changes — that would be wasteful
    // and would defeat the per-frame debouncing of the cache.
    let multiplier = ui_state.build_multiplier;
    for building_type in &entries {
        if spawned_cards.contains_key(building_type) {
            continue;
        }
        let Some(def) = buildings_data.get(building_type) else {
            continue;
        };
        let icon_handle = building_icons
            .as_ref()
            .and_then(|bi| bi.handles.get(building_type).cloned());
        let count = colony.buildings.get(building_type).copied().unwrap_or(0);
        let card = spawn_constructed_card(
            &mut commands,
            content,
            *building_type,
            def,
            count,
            multiplier,
            &body_font,
            &body_font_medium,
            &mono_font,
            icon_handle.as_ref(),
            resource_icons,
        );
        spawned_cards.insert(*building_type, card);
    }
}

// Build the **Mining** body. Persistent container with a header
// + a content scroll area. The `update_mining_body` system
// re-spawns the rows every frame based on the selected colony's
// mine inventory + body deposits.
///
// v0.5.2 PR-A.2: replaces the v0.5.x legacy egui mining tab
// (`src/ui/construction_panel.rs:872-1392`). 7 surface groups
// (24 base mines) + 1 collapsible orbital section (25 AutoMines
// across 5 non-collapsible sub-groups). One card per mine. Each
// card has [-] [+] buttons that push to
// `PendingConstructionActions::mining_edits` (positive=add,
// negative=remove) — see `process_construction_actions` in
// `src/colony/systems.rs` for the consumer.
fn spawn_mining_body(commands: &mut Commands, parent: Entity) {
    let body = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                width: Val::Percent(100.0),
                flex_grow: 1.0,
                // v0.5.2 (2026-08-06, bugfix): `min_height: 0` — see
                // the overview_body comment. Without it the inner
                // `mining_content` scroll container never overflows and
                // the Mining tab shows a grey track with no thumb.
                min_height: Val::Px(0.0),
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

    // Content container — `update_mining_body` despawns and
    // re-spawns the rows inside this container every frame.
    //
    // v0.5.2 (2026-08-06): uses the shared
    // `widgets::spawn_scrollable_container` helper (the
    // `min_height: 0` + `flex_grow: 1` + `scroll_y` trio that
    // `update_mining_body`'s wheel-scroll depends on).
    let content = spawn_scrollable_container(
        commands,
        body,
        "mining_content",
        SPACE_XS,
        MiningContent,
    );

    // Always-visible vertical scrollbar pinned to the right
    // edge of the Mining body. Mirrors the Build tab's
    // card_grid scrollbar — same chrome, same drag handler,
    // same thumb style — so the two surfaces read as one UI.
    // The track is parented to `parent` (the Construction menu
    // root, NOT `body`) for the same reason as the Build tab:
    // if we parented it to `body` it would scroll out of view
    // when the player scrolls the content, because `body`
    // inherits `Overflow::clip` from the body root.
    //
    // v0.5.2 PR-A.8: track carries `target: content` so
    // `tick_construction_scrollbar` knows which scrollable to
    // drive, and `tab: ConstructionTabBody::Mining` so
    // `tick_construction_body_visibility` flips the track's
    // visibility in lockstep with the body. Without the `tab`
    // marker, the track would show on every tab and read as
    // a stray vertical pill on Overview / Build / Buildings.
    spawn_construction_scrollbar(
        commands,
        parent,
        content,
        "mining_scrollbar_track",
        98.0_f32,
        SPACE_SM,
        ConstructionTabBody::Mining,
    );
}

// Shared helper: spawn the always-visible vertical scrollbar
// (track + thumb + observers) parented to a panel root, aimed
// at a specific scrollable body. Used by both the Build tab's
// card grid and the Mining tab's body so the two surfaces
// share identical chrome.
//
// `tab` lets `tick_construction_body_visibility` show/hide the
// track alongside the body. The track is parented to `root`
// (not the scrollable) so it stays pinned to the panel even
// when the player scrolls content — `Overflow::clip` /
// `Overflow::scroll_y` on the parent would otherwise carry
// the track away with the content.
//
// Takes `&mut Commands` so it composes cleanly with the
// surrounding `setup_construction` (owned `Commands`) and
// `spawn_mining_body` (`&mut Commands`) call sites.
fn spawn_construction_scrollbar(
    commands: &mut Commands,
    root: Entity,
    target: Entity,
    track_name: &'static str,
    track_top_px: f32,
    track_bottom_px: f32,
    tab: ConstructionTabBody,
) {
    let scrollbar_track = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(2.0),
                top: Val::Px(track_top_px),
                bottom: Val::Px(track_bottom_px),
                width: Val::Px(12.0),
                border_radius: BorderRadius::all(Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.06)),
            ZIndex(10),
            Pickable::default(),
            Name::new(track_name),
            ConstructionScrollbarTrack { target, tab },
        ))
        .id();
    commands.entity(root).add_child(scrollbar_track);
    // Press/release observers keep the drag locked to the track
    // entity even when the cursor slips off the slim track
    // surface mid-drag. Without these, `Interaction::Pressed`
    // would drop to `None` the moment the cursor leaves the
    // 12-px-wide track and the drag would die immediately.
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
                border_radius: BorderRadius::all(Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(CYAN.with_alpha(0.6)),
            ZIndex(11),
            Pickable::default(),
            Name::new("construction_scrollbar_thumb"),
            ConstructionScrollbarThumb,
        ))
        .id();
    commands.entity(scrollbar_track).add_child(scrollbar_thumb);
    commands.entity(scrollbar_thumb).observe(on_thumb_press);
    commands.entity(scrollbar_thumb).observe(on_thumb_release);
}

// Marker on the Mining body's content container.
#[derive(Component)]
pub struct MiningContent;

// Marker on a per-group header row (chevron + label + count).
#[derive(Component)]
pub struct MiningGroupHeader {
    pub group_id: MiningGroupId,
}

// Marker on a per-group body container (the row of cards). The
// group's visibility is driven by `tick_mining_group_visibility`
// toggling this entity's `Display`.
#[derive(Component)]
pub struct MiningGroupBody {
    pub group_id: MiningGroupId,
}

// Marker on a single mine / AutoMine card's outer container.
#[derive(Component)]
pub struct MiningCard {
    pub building_type: BuildingType,
}

// Which build multiplier the Demolish button should use when it
// opens the confirmation dialog. The Mining tab uses its own
// `mining_build_multiplier` (independent of the Build tab's
// `build_multiplier` because the chip sets can diverge), while
// the Buildings tab uses the Build tab's `build_multiplier` so
// the player's x1/x5/x25 chip choice carries straight through to
// the demolish button. The dialog reads the multiplier via
// `DemolishConfirmState::count` (clamped to the live count).
///
// v0.5.2 (Buildings tab as a card list): the Demolish button
// was originally a Mining-tab-only feature. Extending it to the
// Buildings tab means the same handler (`tick_demolish_click`)
// needs to know which chip row governs the multiplier; the
// `DemolishButton` marker carries a `multiplier_source` field
// that the click handler reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DemolishMultiplierSource {
    // Use `ui_state.mining_build_multiplier`. Mining tab cards.
    Mining,
    // Use `ui_state.build_multiplier`. Buildings tab cards.
    Build,
}

// Marker on a Demolish button. The button removes `mining_edits`
// (negative delta) inventory from the active colony via
// `PendingConstructionActions::mining_edits`. The handler
// `tick_demolish_click` does the actual work.
///
// Originally a Mining-tab-only feature (`MiningDemolishButton`),
// v0.5.2 (Buildings tab as a card list) extends the same button
// to the Buildings tab so the player can demolish existing
// buildings with the same UI. The `multiplier_source` field tells
// the click handler which chip-row value to apply (Mining vs
// Build) — they can diverge.
#[derive(Component)]
pub struct DemolishButton {
    pub building_type: BuildingType,
    pub multiplier_source: DemolishMultiplierSource,
}

// v0.5.2 (2026-08-06): marker on the Demolish button's label text.
// `update_demolish_button_labels` rewrites it every frame from the
// live `build_multiplier` — the old code spawned the label once at
// card-spawn time, so a Buildings-tab card (spawn-once-update-many)
// kept "Demolish -1" forever even after the player picked ×25.
#[derive(Component)]
pub struct DemolishButtonLabel;

// Marker added when the Demolish button should be disabled (no
// buildings to remove — `count == 0`). Mirrors
// `ConstructionCtaDisabled` for the Queue button: the click
// handler skips pushing when the marker is present, and the
// spawn code drops the marker at spawn time when the count is
// non-zero.
///
// v0.5.2 (Buildings tab as a card list): the Buildings tab
// cards also use this marker so the same hover / click effect
// system (`tick_demolish_hover`, `tick_demolish_disabled`) can
// service both tab families without per-tab branches.
#[derive(Component)]
pub struct DemolishDisabled;

// ── Demolish confirmation modal (v0.5.2 PR-A.7) ──────────────────────

// Marker on the centered Demolish confirmation modal root.
// Visibility is driven by `tick_demolish_dialog_visibility` based on
// `DemolishConfirmState::open`. Parented to the Construction menu
// root, fills the entire menu area with a semi-transparent dark
// backdrop and centers the content card. The Construction menu
// root sits at `top: 126.0`; absolutely-positioned children are
// measured against the *parent's content-area* (so the backdrop
// fills the area below the global chrome, not the window).
#[derive(Component)]
pub struct DemolishConfirmDialog;

// Marker on the dialog title text. Updated by
// `update_demolish_dialog_text` to read e.g. "Demolish 5 Iron
// Mines?" — the count is clamped to the live colony count.
#[derive(Component)]
pub struct DemolishConfirmTitle;

// Marker on the dialog subtitle text. Updated to read e.g.
// "You currently have 5 on Earth." — the live count + colony
// name. Lets the player double-check before confirming.
#[derive(Component)]
pub struct DemolishConfirmSubtitle;

// Marker on the Yes button. `tick_demolish_confirm_yes_click`
// applies the `mining_edits` entry and closes the dialog.
#[derive(Component)]
pub struct DemolishConfirmYes;

// Marker on the No button. `tick_demolish_confirm_no_click`
// closes the dialog without applying the action. The backdrop
// also clicks-through to No via a `Pointer<Click>` observer.
#[derive(Component)]
pub struct DemolishConfirmNo;

// Marker on the orbital section's outer container (the 5 sub-groups).
// Visibility is driven by `tick_mining_group_visibility` based on
// `ui_state.mining_orbital_collapsed`.
#[derive(Component)]
pub struct MiningOrbitalBody;

// Per-chip data for the cost-row hover tooltip. Carried by
// every `ResourceCostChip` so the observer handlers can look up
// the resource name + amount + category tint via the picked
// entity id and write them into the tooltip's text node.
///
// `name` is the display string (`"Iron"`, `"Water"`, `"He-3"`,
// etc.) — not the raw RON name. `amount` is the formatted
// `kg / t / Mt / Gt / Tt` string produced by `format_mining_reserve`.
// `category` is the chip's category colour (Construction /
// Volatiles / Fissile / etc.) so the tooltip can match the
// chip's tint. `card` is the host card's entity id — the
// observer uses it to find the right tooltip among the many
// (one per visible card).
#[derive(Component, Clone)]
pub struct ResourceCostChip {
    pub name: String,
    pub amount: String,
    pub category: Color,
    pub card: Entity,
}

// Marker on the singleton cost-chip hover tooltip overlay.
// Spawned once at panel setup time (parented to the
// construction `root`), populated each frame by
// [`update_resource_cost_tooltip`] from
// [`ResourceCostHoverState`]. Visual style mirrors the 3D
// body-hover tooltip (`src/ui/mod.rs::ui_hover_tooltip`):
// `TOOLTIP_BG` fill, cyan border, lg inner margin — so the
// panel chrome and the world tooltips read as the same
// design language.
#[derive(Component)]
pub struct ResourceCostTooltipOverlay;

// Marker on the inner text node of the overlay. The update
// system finds the text via `Single<&mut Text, With<…>>` so
// it doesn't have to walk a Children hierarchy.
#[derive(Component)]
pub struct ResourceCostTooltipText;

// Resource tracking which cost chip (if any) the cursor is
// currently hovering. The `Pointer<Over>` observer writes
// `Some(…)` on hover-in and the `Pointer<Out>` observer
// writes `None` on hover-out. The `update_resource_cost_tooltip`
// system reads this each frame to drive the overlay's
// text/colour/position.
///
// Cloning the small `String` data on every hover is cheap
// (a hover can only happen on one chip at a time, and the
// hovered chip is one of a few visible chips in the panel).
#[derive(Resource, Default)]
pub struct ResourceCostHoverState {
    pub chip: Option<HoveredChipData>,
}

// Snapshot of a hovered chip's display data. The
// `category` field is the chip's category tint
// (Construction / Volatiles / Fissile / etc.) so the
// overlay's text can match the chip's hue. `entity` is
// the chip entity id and is mainly there for debug logs.
#[derive(Clone)]
pub struct HoveredChipData {
    pub name: String,
    pub amount: String,
    pub category: Color,
    pub entity: Entity,
}

// Observer: on `Pointer<Over>`, snapshot the hovered chip's
// `ResourceCostChip` data into [`ResourceCostHoverState`].
// The `update_resource_cost_tooltip` system reads that
// resource each frame to populate the singleton overlay.
///
// The observer doesn't touch the overlay entity directly
// — pointer observers shouldn't mutate other entities'
// per-frame state. The system handles the visible position
// + text + colour work because the overlay's `left/top`
// must be set from the live cursor position, which the
// observer doesn't have.
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

// Observer: on `Pointer<Out>`, clear the hover state if the
// cursor left the chip we're currently tracking. `Pointer<Out>`
// fires once per entity whose bounds the cursor leaves; we
// compare against `hover_state.chip.entity` so the state
// isn't cleared by a stale event from a sibling element.
///
// If the cursor moves from chip A → chip B without crossing
// any other interactive element, Bevy firing order for the
// same frame is: `Pointer<Out>(A)` then `Pointer<Over>(B)`.
// The Over fires *after* the Out, so the resource ends up
// holding chip B's data — which is the desired state. The
// guard above ensures a stray Out event on a non-tracked
// entity is a no-op.
fn on_chip_hover_out(on: On<Pointer<Out>>, mut hover_state: ResMut<ResourceCostHoverState>) {
    if let Some(current) = &hover_state.chip {
        if current.entity == on.entity {
            hover_state.chip = None;
        }
    }
}

// ── Power chip tooltip (v0.5.2 PR-A.7, 2026-08-04) ──────────────
//
// Mirrors the `ResourceCostChip` hover pattern but for the
// dedicated Power chip. The overlay is a separate singleton so
// the resource-cost and power tooltips don't share state (a
// player can hover a resource chip while the power chip overlay
// is also visible during a cursor transition — the two systems
// would otherwise fight over the same `Single<…>`).

// Carries the tooltip payload for one Power chip. Attached
// to the chip entity at spawn time; the observer reads it on
// `Pointer<Over>` and snapshots the lines + tone into
// [`PowerChipHoverState`]. The `card` field is the host card's
// entity id — kept for future per-card context (e.g. showing
// the batch's combined cost in the tooltip).
#[derive(Component, Clone)]
pub struct PowerChip {
    pub tooltip_lines: Vec<String>,
    pub tone: Color,
    pub card: Entity,
}

// Marker on the singleton power-chip hover tooltip overlay.
// Spawned once at panel setup time (alongside
// `ResourceCostTooltipOverlay`); populated each frame by
// [`update_power_chip_tooltip`].
#[derive(Component)]
pub struct PowerChipTooltipOverlay;

// Marker on the inner text node of the power-chip overlay.
#[derive(Component)]
pub struct PowerChipTooltipText;

// Resource tracking which power chip (if any) the cursor is
// currently hovering. Same write/read pattern as
// [`ResourceCostHoverState`].
#[derive(Resource, Default)]
pub struct PowerChipHoverState {
    pub chip: Option<HoveredPowerChipData>,
}

// Snapshot of a hovered power chip's display data. The
// `tone` field is the chip's colour (GREEN_FIN for
// fitting demand / production, ORANGE_ORE for
// insufficient demand, TEXT_BODY for no-power-interaction)
// so the overlay's text matches the chip's hue. `entity`
// is the chip entity id, used by the hover-out observer
// to guard against stale events from sibling elements.
#[derive(Clone)]
pub struct HoveredPowerChipData {
    pub tooltip_lines: Vec<String>,
    pub tone: Color,
    pub entity: Entity,
}

// Observer: on `Pointer<Over>`, snapshot the hovered power
// chip's `PowerChip` data into [`PowerChipHoverState`].
fn on_power_chip_hover_over(
    on: On<Pointer<Over>>,
    chip_query: Query<&PowerChip>,
    mut hover_state: ResMut<PowerChipHoverState>,
) {
    let Ok(chip) = chip_query.get(on.entity) else {
        return;
    };
    hover_state.chip = Some(HoveredPowerChipData {
        tooltip_lines: chip.tooltip_lines.clone(),
        tone: chip.tone,
        entity: on.entity,
    });
}

// Observer: on `Pointer<Out>`, clear the hover state if the
// cursor left the chip we're currently tracking. Same guard
// pattern as `on_chip_hover_out` — only clear when the Out
// event matches the tracked entity.
fn on_power_chip_hover_out(
    on: On<Pointer<Out>>,
    mut hover_state: ResMut<PowerChipHoverState>,
) {
    if let Some(current) = &hover_state.chip {
        if current.entity == on.entity {
            hover_state.chip = None;
        }
    }
}

// Per-frame driver for the cost-chip hover overlay. Reads
// `ResourceCostHoverState` (written by the chip observers)
// and `Window::cursor_position()`, then either:
// - hides the overlay (`Display::None`) when no chip is
//   hovered, when the chip entity was despawned between
//   frames (Build ↔ Mining sub-tab switch, multiplier chip
//   change, colony switch), or when the construction menu
//   isn't the active menu, OR
// - positions the overlay next to the cursor (4 px below
//   the cursor vertically, 8 px right horizontally) and
//   populates the text with `"<name>  <amount>"`.
///
// Coordinate-frame note (this is the bug behind the "tooltip
// is way below the cursor" report):
///
// The overlay is parented to the [`ConstructionRoot`]
// node, which itself is positioned at
// `top: 126.0; bottom: 72.0; left: 0; right: 0` so it lives
// below the top resource-bar chrome and above the
// time-controls dock (see `setup_construction`). Absolute-
// positioned children of a node with `top: 126` place
// relative to that node's content-area origin — i.e. an
// inner node at `top: Val::Px(0)` lands at **window** Y=126,
// not Y=0. `Window::cursor_position()` returns coordinates
// in **window** space, so subtracting the canary root's
// `top` constant is required to translate cursor → overlay
// local coords. Without that subtraction the overlay
// renders 126 px **below** the cursor — exactly the bottom-
// right-of-card offset shown in the bug report.
///
// Earlier this comment claimed the root spans the full
// window and child `Val::Px(x)` values map to window
// coords; that was wrong because the root's `top: 126`
// offset is inherited by absolutely-positioned descendants.
// This implementation subtracts that constant explicitly so
// future readers don't have to re-derive the offset.
///
// Visual style matches the body-hover tooltip in
// `src/ui/mod.rs::ui_hover_tooltip`: same `TOOLTIP_BG`
// fill, same cyan border. The only difference vs the body-
// hover tooltip is this is Bevy UI, not egui, so positioning
// is `Node::left/top` rather than `egui::Area`.
///
// The clamp on `left/top` keeps the tooltip on-screen when
// the cursor is near the right/bottom edges — mirrors the
// `clamp(0.0, max_left)` / `clamp(0.0, max_top)` pattern in
// `update_shipbuilding_hover_tooltip`.
fn update_resource_cost_tooltip(
    active_menu: Res<ActiveMenu>,
    primary_window: Query<&Window, With<PrimaryWindow>>,
    chip_query: Query<&ResourceCostChip>,
    mut hover_state: ResMut<ResourceCostHoverState>,
    mut overlay_node: Single<&mut Node, With<ResourceCostTooltipOverlay>>,
    tooltip: Single<(&mut Text, &mut TextColor), With<ResourceCostTooltipText>>,
) {
    let (mut tooltip_text, mut tooltip_color) = tooltip.into_inner();

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
    // so the tooltip carries the chip's category hue.
    *tooltip_text = Text::new(format!("{}  {}", data.name, data.amount));
    *tooltip_color = TextColor(data.category);
}

// Per-frame driver for the **power**-chip hover overlay.
// Mirrors [`update_resource_cost_tooltip`] but renders the
// multi-line payload from [`PowerChip::tooltip_lines`] instead
// of a single `<name>  <amount>` string. Same coordinate-frame
// translation (subtract `CANARY_ROOT_TOP_PX` from cursor Y),
// same stale-entity guard, same `Display::None` fallback when
// the construction menu isn't the active menu.
///
// The overlay has its own marker ([`PowerChipTooltipOverlay`])
// and resource ([`PowerChipHoverState`]) so a player hovering a
// resource chip and a power chip at the same time (e.g. a
// cursor crossing the card body's two zones) gets clean
// per-zone tooltips instead of one stomping the other.
fn update_power_chip_tooltip(
    active_menu: Res<ActiveMenu>,
    primary_window: Query<&Window, With<PrimaryWindow>>,
    chip_query: Query<&PowerChip>,
    mut hover_state: ResMut<PowerChipHoverState>,
    mut overlay_node: Single<&mut Node, With<PowerChipTooltipOverlay>>,
    tooltip: Single<(&mut Text, &mut TextColor), With<PowerChipTooltipText>>,
) {
    let (mut tooltip_text, mut tooltip_color) = tooltip.into_inner();

    // Same canary-root top offset as the resource cost tooltip.
    // Both overlays are parented to the same construction root,
    // so they share the translation constant.
    const CANARY_ROOT_TOP_PX: f32 = 126.0;
    const TOOLTIP_W: f32 = 280.0;
    // Power tooltips can run 4 lines (verb + per-unit + spare
    // grid + "Not enough energy") at CAPTION_SIZE — reserve
    // enough vertical room.
    const TOOLTIP_H: f32 = 80.0;

    let construction_menu_active = matches!(active_menu.current, GameMenu::Construction);
    if !construction_menu_active {
        overlay_node.display = Display::None;
        if hover_state.chip.is_some() {
            hover_state.chip = None;
        }
        return;
    }

    // Stale-entity guard: the chip may have been despawned
    // between frames (multiplier chip change, colony switch,
    // Build ↔ Mining tab switch). Same races the resource
    // cost tooltip guards against.
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

    let Ok(window): Result<&Window, _> = primary_window.single() else {
        overlay_node.display = Display::None;
        return;
    };
    if window.cursor_position().is_none() {
        overlay_node.display = Display::None;
        return;
    }
    let cursor = window.cursor_position().unwrap();

    let local_x = cursor.x;
    let local_y = cursor.y - CANARY_ROOT_TOP_PX + 4.0;

    // Power tooltips sit a bit lower-right than resource
    // tooltips (4 px vertical nudge + 8 px horizontal nudge)
    // so the two overlays don't overlap if the player sweeps
    // the cursor from one chip to the other. Without the
    // horizontal nudge the two tooltips land at the same X
    // and the power one would draw on top of the resource
    // one (or vice versa) when both are visible in the same
    // frame.
    let local_x = local_x + 8.0;

    let root_width = (window.width() - 0.0).max(TOOLTIP_W);
    let root_height = (window.height() - CANARY_ROOT_TOP_PX - 72.0).max(TOOLTIP_H);
    let max_left = (root_width - TOOLTIP_W).max(0.0);
    let max_top = (root_height - TOOLTIP_H).max(0.0);
    overlay_node.left = Val::Px(local_x.clamp(0.0, max_left));
    overlay_node.top = Val::Px(local_y.clamp(0.0, max_top));
    overlay_node.display = Display::Flex;

    // Multi-line payload: join the `tooltip_lines` vec with
    // `\n` so the Text node renders each line on its own row.
    // Newlines in `Text` are honoured by the line-breaker
    // (mirrors the body-hover tooltip's multi-line rendering).
    *tooltip_text = Text::new(data.tooltip_lines.join("\n"));
    *tooltip_color = TextColor(data.tone);
}

// Update the Mining tab body. Re-spawns the cards inside the
// `MiningContent` container every time it runs. Triggered by
// `ConstructionUiState` changes (tab switch, qty chip, group
// collapse) and `Colony` changes (mine count edit, deposit
// update, colony switch). Skips on other frames — see the
// `update_mining_body` system registration in the plugin.
///
// Per spec: 7 surface groups (24 base mines) + 1 orbital
// section (25 AutoMines across 5 sub-groups). One card per
// mine. Each card shows count / production / reserve /
// accessibility and [-] [+] buttons that route to
// `PendingConstructionActions::mining_edits`.

// Fingerprint of the inputs that determine the Mining body's card
// content (v0.5.2, 2026-08-06). The body used to despawn + re-spawn
// all ~49 cards every frame; this captures the values that can
// change the rendered cards, so a rebuild only happens when one of
// them actually changes. `spare_power_mw` is deliberately NOT in
// the fingerprint — `calculate_colony_power_totals` is a pure
// function of the building counts (see `budget.rs`), so spare is
// fully determined by `counts` and recomputed on rebuild.
//
// v0.5.2 (2026-08-06, bugfix round 2): `deposit_sig` now folds ONLY
// the accessibility (generation-static — it changes only on survey
// completion, a discrete event). The old signature also folded the
// rounded total reserve, which `extract_resources` decrements every
// frame while mines run — so the fingerprint changed on extraction
// boundaries and the whole grid rebuilt periodically (one of the
// "Build button flickers" sources). The reserve display being stale
// between rebuilds is acceptable; accessibility reveals still
// trigger a rebuild.
#[derive(PartialEq)]
struct MiningBodyFingerprint {
    /// The colony the cards were spawned for (counts are
    /// colony-scoped; `counts` alone can't distinguish two colonies
    /// with identical building tallies).
    colony_entity: Option<bevy::ecs::entity::Entity>,
    multiplier: u32,
    collapsed: std::collections::HashSet<MiningGroupId>,
    orbital_collapsed: bool,
    counts: std::collections::HashMap<BuildingType, u32>,
    breathable: bool,
    body_type: Option<BodyType>,
    /// Hash of the body's deposit accessibility (see the doc above —
    /// reserves deliberately excluded).
    deposit_sig: u64,
}

impl MiningBodyFingerprint {
    fn no_colony(ui_state: &ConstructionUiState) -> Self {
        Self {
            colony_entity: None,
            multiplier: ui_state.build_multiplier,
            collapsed: ui_state.mining_groups_collapsed.clone(),
            orbital_collapsed: ui_state.mining_orbital_collapsed,
            counts: std::collections::HashMap::new(),
            breathable: false,
            body_type: None,
            deposit_sig: 0,
        }
    }
}

/// Cheap hash of a body's deposit accessibility. Folds each deposit's
/// `(resource, accessibility)` into an FNV-1a hash — enough to detect
/// survey reveals without cloning `PlanetResources`.
///
/// NOTE: reserves are deliberately NOT folded in — `extract_resources`
/// decrements them every frame while mines run, so including them made
/// the mining-body fingerprint change on extraction boundaries and
/// trigger a full grid rebuild periodically.
fn mining_deposit_signature(resources: Option<&crate::economy::PlanetResources>) -> u64 {
    let Some(res) = resources else {
        return 0;
    };
    let mut h: u64 = 0xcbf29ce484222325;
    for (rt, dep) in res.deposits.iter() {
        h ^= *rt as u64;
        h = h.wrapping_mul(0x100000001b3);
        let bits = dep.accessibility.to_bits() as u64;
        h ^= bits;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

#[allow(clippy::too_many_arguments)]
fn update_mining_body(
    mut commands: Commands,
    fonts: Res<UiFonts>,
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
    mut spawned_rows: Local<Vec<Entity>>,
    // v0.5.2 (2026-08-06): the fingerprint of the last rebuild.
    // When the current inputs match it, nothing changed — skip the
    // despawn+re-spawn entirely (the "huge compute for a simple
    // menu" fix: was ~150 entity spawns per frame, now a hash
    // compare).
    mut last_fingerprint: Local<Option<MiningBodyFingerprint>>,
) {
    let Ok(content) = content_query.single() else {
        return;
    };

    // Per-frame respawn gate (v0.5.2, 2026-08-05): the Mining body
    // re-spawns ~30 cards every frame from scratch (despawn-all +
    // spawn-all). When the player is on another tab (Overview /
    // Buildings / Build) the Mining body is `Visibility::Hidden`
    // and re-spawning its content is pure waste — the cards are
    // invisible AND the spawn happens before the visibility toggle
    // in `tick_construction_body_visibility`, so it's ~30 card
    // spawns per frame even when the player never looks at Mining.
    // Skipping the respawn when the Mining tab isn't active keeps
    // the tab-switch paint fresh (the first frame on Mining
    // re-spawns) while eliminating the hidden-tab churn.
    if ui_state.selected_tab != ConstructionTab::Mining {
        return;
    }

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

    // v0.5.2 (2026-08-06): diff gate. The inputs above (counts,
    // multiplier, collapse state, body context, deposits) fully
    // determine the rendered cards — spare power is a pure function
    // of the counts. When nothing changed since the last rebuild,
    // skip the despawn+re-spawn entirely (was ~150 entity spawns /
    // frame; now a hash compare + clone).
    let fingerprint = match &colony_data {
        Some((_, breathable, body_type, planet_resources, counts)) => MiningBodyFingerprint {
            colony_entity: active_colony_entity,
            multiplier: ui_state.build_multiplier,
            collapsed: ui_state.mining_groups_collapsed.clone(),
            orbital_collapsed: ui_state.mining_orbital_collapsed,
            counts: counts.clone(),
            breathable: *breathable,
            body_type: *body_type,
            deposit_sig: mining_deposit_signature(*planet_resources),
        },
        None => MiningBodyFingerprint::no_colony(&ui_state),
    };
    if last_fingerprint.as_ref() == Some(&fingerprint) {
        // Nothing changed — the existing cards are current.
        return;
    }
    *last_fingerprint = Some(fingerprint);

    // Despawn the previous frame's spawn (only reached when the
    // fingerprint changed).
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

    let body_font: Handle<Font> = fonts.body.clone();
    let body_font_medium: Handle<Font> = fonts.medium.clone();
    let mono_font: Handle<Font> = fonts.mono.clone();
    let multiplier = ui_state.build_multiplier;
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

    // v0.5.2 PR-A.7 (2026-08-04): compute the active colony's
    // grid surplus so the mining cards' power chip tooltips can
    // show the real "Spare grid" value instead of the old
    // "No active grid" placeholder. `f64::NAN` when no colony is
    // selected. Only computed on rebuild — it's a pure function of
    // the building counts, which the fingerprint already tracks.
    let spare_power_mw: f64 = if let Some(e) = active_colony_entity {
        colonies
            .get(e)
            .ok()
            .map(|(_, c)| calculate_colony_power_totals(c, Some(&buildings_data)))
            .map(|totals| (totals.produced_watts - totals.consumed_watts) / 1_000_000.0)
            .unwrap_or(f64::NAN)
    } else {
        f64::NAN
    };

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
            spare_power_mw,
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
        spare_power_mw,
    );
    spawned_rows.push(orbital_node);
}

// Setup system: spawns the Construction panel entities once at startup.
///
// The canary is gated by `ConstructionState`. When `Off`, the
// root is hidden via `Visibility::Hidden`.
pub fn setup_construction(
    mut commands: Commands,
    fonts: Res<UiFonts>,
    buildings_data_opt: Option<Res<BuildingsData>>,
    research_state: Res<ResearchState>,
    ui_state: Res<ConstructionUiState>,
    building_icons: Option<Res<BuildingIcons>>,
    resource_icons: Option<Res<ResourceIcons>>,
    colonies: Query<(Entity, &crate::colony::Colony)>,
) {
    let body_font = fonts.body.clone();
    let body_font_medium = fonts.medium.clone();
    let mono_font = fonts.mono.clone();

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
    // v0.5.2 (Commit D): the tab strip is no longer wrapped in a
    // bordered `ChipRowContainerBundle` — that bordered box was the
    // "trailer" the player saw on Overview / Buildings, where the
    // shared chrome sits below it on every other tab. The active
    // tab still gets a `BoxShadow` glow on its bottom edge, so the
    // selected affordance is preserved without the surrounding frame.
    let tab_strip = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(SPACE_XS),
                padding: UiRect::all(Val::Px(SPACE_XS)),
                width: Val::Auto,
                height: Val::Px(TAB_STRIP_H),
                ..default()
            },
            Name::new("tab_strip"),
        ))
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
        spawn_chip_text(
            &mut commands,
            tab,
            label,
            body_font.clone(),
            *is_active,
            TAB_FONT_SIZE,
        );
    }

    // ── Build output (BP/year) + Active Colony dropdown ─────────────
    // Per user feedback (OOB 2026-07-31):
    //   - Treasury + Balance removed: those are already in the global top
    //     resource bar and don't need to be repeated here.
    //   - Build output (BP/year) is its own row, more prominent.
    //   - Active Colony becomes a dropdown selector.
    //   - Both have icons (placeholder squares until per-icon art lands).
    // Static placeholder; will be wired to ConstructionUiState in Phase C4.
    // v0.5.2: the chrome (output row, active-colony picker, build-qty
    // chips, filter chips) is lifted out of the per-tab bodies and
    // lives at the ConstructionRoot level so it's visible on every
    // tab. The filter row carries a `ShowOnBuildOrBuildings` marker
    // and stays hidden on Overview / Mining (those tabs have no card
    // grid to filter). The other three rows are always visible.
    //
    // `ZIndex(1)` on the shared chrome lifts it above the
    // `card_grid` (default `ZIndex(0)`) so the chip rows stay
    // readable when the cards scroll underneath. The colony
    // dropdown uses `GlobalZIndex(100)` on its own entity so it can
    // draw on top of *every* UI node.
    let shared_chrome = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                width: Val::Percent(100.0),
                // `align_items: Start` keeps the chrome rows (output /
                // picker / build-qty / filter) at their natural content
                // width on the left edge of the panel. Without this,
                // every child gets stretched to the column's full width
                // by the default `AlignItems::Stretch` and the chip-row
                // containers render as wide horizontal bars instead of
                // hugging their chips.
                align_items: AlignItems::Start,
                row_gap: Val::Px(SPACE_SM),
                ..default()
            },
            ZIndex(1),
            Name::new("shared_chrome"),
        ))
        .id();
    commands.entity(root).add_child(shared_chrome);

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
    commands.entity(shared_chrome).add_child(output_row);

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
            Text::new("6d 2h"), // static placeholder; reads from resource in C4
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
    commands.entity(shared_chrome).add_child(picker);
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
            // the `shared_chrome` subtree so it can draw on top
            // of the `card_grid` and any other siblings of the
            // shared chrome. Per Bevy 0.18, `ZIndex` only orders
            // siblings of the same parent — the dropdown is nested
            // inside the picker which is inside `shared_chrome`, so
            // its `ZIndex` would only order it among the picker's
            // children (which is just the dropdown itself). Using
            // `GlobalZIndex(100)` puts it above every other UI node
            // in the Construction menu, which is the popup-layer
            // behavior the player expects.
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
    commands.entity(shared_chrome).add_child(build_qty_row);

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
        spawn_chip_text(
            &mut commands,
            btn,
            label,
            mono_font.clone(),
            is_active,
            16.0,
        );
    }

    // ── Filter row ─────────────────────────────────────────────────────
    // 8 category chips + "All" (index 8 = show every building). The
    // whole row is wrapped in a single bordered container. Carries
    // `ShowOnBuildOrBuildings` so the visibility system hides it on
    // Overview / Mining (those tabs have no card grid to filter).
    let filter_row = commands
        .spawn((
            ChipRowContainerBundle::new("filter", 28.0),
            ShowOnBuildOrBuildings,
        ))
        .id();
    commands.entity(shared_chrome).add_child(filter_row);

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
        spawn_chip_text(
            &mut commands,
            chip,
            label,
            body_font.clone(),
            is_active,
            16.0,
        );
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
    // v0.5.2 PR-A.8: scrollbar spawn goes through the shared
    // helper so Build + Mining tabs use identical chrome. The
    // inline spawn is gone (the helper replaces it). Same
    // `track_top_px = 138` magic number (tab strip ~36 + chip
    // rows ~80 + 12-px padding) preserved from the original
    // Build tab setup.
    spawn_construction_scrollbar(
        &mut commands,
        root,
        card_grid,
        "card_grid_scrollbar_track",
        track_top_px,
        track_bottom_px,
        ConstructionTabBody::Build,
    );

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
    let spare_power_mw = compute_colony_spare_power_mw(&ui_state, &colonies, Some(&buildings_data));
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
        let card = spawn_card(
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
        // v0.5.2 (2026-08-06): tag this initial card as Build-tab-owned.
        // WITHOUT this, the first chip click's `refresh_card_grid`
        // (which despawns only `With<BuildCard>`) finds nothing to
        // remove and spawns a fresh filtered set ON TOP of these —
        // the old "All" cards persist, so the filter never visibly
        // narrows the grid (and cards stack up). `refresh_card_grid`
        // inserts the same marker on its own spawns, so after the
        // first refresh the sets are consistent.
        commands.entity(card).insert(BuildCard);
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
    spawn_buildings_body(&mut commands, root, &body_font_medium);
    spawn_mining_body(&mut commands, root);

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
                // Wide enough for "Missing:\n  17 Mt Copper\n  33 Mt Silicates" +
                // padding. The text node has its own width so the
                // container hugs it. Multi-line content (one line per
                // shortfall) is rendered via `\n` in the Text — Bevy's
                // `TextLayout` wraps both characters and `\n`.
                padding: UiRect::all(Val::Px(SPACE_SM)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.008, 0.039, 0.094, 0.96)),
            // RED border — matches the disabled Queue CTA's red
            // frame so the tooltip reads as a continuation of the
            // "this is broken" surface, not a separate overlay.
            BorderColor::all(RED),
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
            // RED — the tooltip lists what's missing from a
            // disabled Queue CTA, which already paints itself red
            // (see `tick_construction_cta_hover` /
            // `tick_construction_cta_label_dim`). Matching the
            // tooltip colour to the button reinforces the
            // "this is broken, here's why" reading.
            TextColor(RED),
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

    // v0.5.2 PR-A.7 (2026-08-04): power-chip tooltip overlay.
    // Mirrors the `ResourceCostTooltipOverlay` above but with its
    // own marker so the two tooltip systems don't fight over
    // the same `Single<…>`. Sits at the same ZIndex (20) so it
    // draws on top of card chrome but below the topbar.
    let power_chip_tooltip_overlay = commands
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
            PowerChipTooltipOverlay,
            Name::new("power_chip_tooltip_overlay"),
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
                PowerChipTooltipText,
                Name::new("power_chip_tooltip_overlay_text"),
            ));
        })
        .id();
    commands.entity(root).add_child(power_chip_tooltip_overlay);

    // v0.5.2 (build menu fix): cursor-following Queue-CTA
    // tooltip overlay. Sibling of the chip tooltips above
    // (resource-cost and power-chip); rendered by
    // `update_queue_button_tooltip` from the per-frame
    // `QueueButtonTooltipState` resource populated by
    // `tick_construction_tooltip`. Same ZIndex (20) so it
    // draws on top of card chrome but below the topbar.
    let queue_tooltip_overlay = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                display: Display::None,
                padding: UiRect::all(Val::Px(10.0)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                // Cap the overlay width so long resource
                // names + amounts (e.g. "Construction Metals
                // 1.2 Gt") wrap cleanly without overflowing
                // the canary root. Height is set per-frame
                // by the system so the multi-line Missing:
                // list fits without manual layout.
                max_width: Val::Px(260.0),
                ..default()
            },
            BackgroundColor(crate::ui::theme::Color::TOOLTIP_BG),
            BorderColor::all(crate::ui::theme::Color::STATUS_INFO_BORDER),
            ZIndex(20),
            QueueButtonTooltipOverlay,
            Name::new("queue_button_tooltip_overlay"),
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
                QueueButtonTooltipText,
                Name::new("queue_button_tooltip_overlay_text"),
            ));
        })
        .id();
    commands.entity(root).add_child(queue_tooltip_overlay);

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
    // v0.5.2 (2026-08-06): uses the shared
    // `widgets::spawn_scrollable_container` helper (the
    // `min_height: 0` + `flex_grow: 1` + `scroll_y` trio).
    spawn_scrollable_container(
        &mut commands,
        queue_panel,
        "queue_panel_body",
        SPACE_SM,
        QueuePanelBody,
    );

    // ── Demolish confirmation modal (v0.5.2 PR-A.7) ──────────────────
    // Centered modal that opens when the player clicks a Demolish
    // button. Parented to the Construction menu root, fills the
    // entire menu area with a semi-transparent dark backdrop, and
    // centers a content card. The Construction menu root sits at
    // `top: 126.0`; the backdrop's `top: 0, bottom: 0, left: 0,
    // right: 0` is in the parent's content-area space, so the
    // backdrop correctly fills the area below the global chrome
    // (not the window) — same coordinate-frame gotcha the chip
    // tooltip fix (memory `construction-chip-tooltip-2026-08-03.md`)
    // documents for the cost-chip overlay.
    spawn_demolish_confirm_dialog(&mut commands, root, &body_font, &body_font_medium);
}

// Spawn the Demolish confirmation dialog as a single `Display::None`
// child of the Construction menu root. The dialog is hidden at
// startup; `tick_demolish_dialog_visibility` flips it visible when
// `DemolishConfirmState::open` is true.
///
// Structure:
// ```
// dialog (DemolishConfirmDialog, Display::None, GlobalZIndex(150))
// ├── backdrop (pickable, click → no)
// └── card (centered, fixed width 480)
//     ├── title   (DemolishConfirmTitle)   "Demolish 5 Iron Mines?"
//     ├── subtitle (DemolishConfirmSubtitle) "You currently have 5 on Earth."
//     ├── yes_button (DemolishConfirmYes)
//     └── no_button  (DemolishConfirmNo)
// ```
fn spawn_demolish_confirm_dialog(
    commands: &mut Commands,
    parent: Entity,
    body_font: &Handle<Font>,
    body_font_medium: &Handle<Font>,
) {
    // v0.5.2: `ZIndex(150)` so the modal floats above every other
    // UI element in the Construction menu (queue panel uses
    // `ZIndex(2)`, resource-cost tooltip overlay uses `ZIndex(20)`,
    // the construction hover tooltip uses `ZIndex(3)`). The
    // backdrop is `Pickable::default()` so a click on the dim
    // area is treated as No (handled by
    // `tick_demolish_confirm_no_click`'s path that fires on the
    // backdrop's `Interaction::Pressed`).
    let dialog = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(0.0),
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                bottom: Val::Px(0.0),
                // Center the content card with flex.
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            // Dark semi-transparent backdrop so the cards behind
            // the modal are dimmed and visually de-emphasized.
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.70)),
            // `Pickable::default()` so backdrop clicks fire — see
            // `tick_demolish_confirm_no_click` which reads the
            // dialog root's `Interaction` (the backdrop is the
            // root in this layout).
            Pickable::default(),
            Button,
            GlobalZIndex(150),
            Visibility::Hidden,
            DemolishConfirmDialog,
            Name::new("demolish_confirm_dialog"),
        ))
        .id();
    commands.entity(parent).add_child(dialog);

    // Centered content card. Fixed width 480 px to match the
    // prototype's modal proportion. The card is a flex column
    // with title / subtitle / button row, all stacked with a
    // 16-px gap. The buttons share a row with a 16-px gap; both
    // are 50% of the row width so they read as a matched pair.
    let card = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(SPACE_LG),
                padding: UiRect::all(Val::Px(SPACE_XL)),
                width: Val::Px(480.0),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(8.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.008, 0.039, 0.094, 0.96)),
            BorderColor::all(CYAN_BORDER_STRONG),
            BoxShadow::new(
                Color::srgba(0.0, 0.0, 0.0, 0.65),
                Val::Px(0.0),
                Val::Px(4.0),
                Val::Px(2.0),
                Val::Px(12.0),
            ),
            Name::new("demolish_confirm_card"),
        ))
        .id();
    commands.entity(dialog).add_child(card);

    // Title — `update_demolish_dialog_text` writes the live
    // "Demolish N <BuildingType>?" string each frame.
    let title = commands
        .spawn((
            Text::new("Demolish"),
            TextFont {
                font: body_font_medium.clone(),
                font_size: BODY_SIZE,
                ..default()
            },
            TextColor(RED),
            DemolishConfirmTitle,
            Name::new("demolish_confirm_title"),
        ))
        .id();
    commands.entity(card).add_child(title);

    // Subtitle — live "You currently have N on <colony>." text.
    let subtitle = commands
        .spawn((
            Text::new(""),
            TextFont {
                font: body_font.clone(),
                font_size: CAPTION_SIZE,
                ..default()
            },
            TextColor(TEXT_BODY),
            DemolishConfirmSubtitle,
            Name::new("demolish_confirm_subtitle"),
        ))
        .id();
    commands.entity(card).add_child(subtitle);

    // Button row: Yes (red, mirrors Demolish button style) +
    // No (neutral, mirrors chip row). Both are 50% width.
    let button_row = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(SPACE_LG),
                ..default()
            },
            Name::new("demolish_confirm_buttons"),
        ))
        .id();
    commands.entity(card).add_child(button_row);

    // Yes button — same red palette as the Demolish card button.
    let yes_button = commands
        .spawn((
            Button,
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                flex_grow: 1.0,
                height: Val::Px(36.0),
                padding: UiRect::horizontal(Val::Px(SPACE_XL)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.353, 0.157, 0.169, 0.85)),
            BorderColor::all(Color::srgba(0.847, 0.373, 0.392, 0.50)),
            DemolishConfirmYes,
            Pickable::default(),
            Name::new("demolish_confirm_yes"),
        ))
        .id();
    commands.entity(button_row).add_child(yes_button);
    let yes_label = commands
        .spawn((
            Text::new("Demolish"),
            TextFont {
                font: body_font_medium.clone(),
                font_size: BODY_SIZE,
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
            Name::new("demolish_confirm_yes_label"),
        ))
        .id();
    commands.entity(yes_button).add_child(yes_label);

    // No button — neutral palette (matches the chip row resting
    // state). Cancels without applying the action.
    let no_button = commands
        .spawn((
            Button,
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                flex_grow: 1.0,
                height: Val::Px(36.0),
                padding: UiRect::horizontal(Val::Px(SPACE_XL)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.094, 0.298, 0.353, 0.85)),
            BorderColor::all(CYAN_BORDER_STRONG),
            DemolishConfirmNo,
            Pickable::default(),
            Name::new("demolish_confirm_no"),
        ))
        .id();
    commands.entity(button_row).add_child(no_button);
    let no_label = commands
        .spawn((
            Text::new("Cancel"),
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
            Name::new("demolish_confirm_no_label"),
        ))
        .id();
    commands.entity(no_button).add_child(no_label);
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
                    //
                    // v0.5.2 (2026-08-06): constructed cards skip the
                    // CTA entirely (the Demolish button is the only
                    // action), so they don't need the bottom
                    // reservation — use SPACE_LG everywhere.
                    top: Val::Px(SPACE_LG),
                    right: Val::Px(SPACE_LG),
                    bottom: Val::Px(if data.constructed { SPACE_LG } else { CTA_FOOTPRINT }),
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
            // Stronger border for a 3D "raised glass" reading — the
            // previous CYAN_BORDER @ 50% alpha blended into the card
            // background and the cards looked flat. Full alpha cyan
            // at the top fades to a dim outline at the bottom via the
            // two-tone gradient overlay spawned below, so the border
            // reads as a beveled edge (light catches the top).
            BorderColor::all(Color::srgba(0.498, 0.804, 0.847, 0.90)),
            // v0.5.2 PR-A.9 (2026-08-05): single subtle dark drop
            // shadow for 3D lift. The previous two-layer version
            // (cyan glow halo + dark drop shadow) read as a heavy
            // "neon outline" against the dark navy chrome and
            // distracted from the card content. Replacing the halo
            // with a darker, smaller, lower-offset drop shadow gives
            // the same 3D lift (card visibly sits above the panel)
            // without the glow artefact. The top + bottom inner
            // rim overlays (cyan highlight above, dark line below)
            // still provide the bevel hint; this shadow just adds
            // the depth offset.
            BoxShadow(vec![bevy::ui::ShadowStyle {
                color: Color::srgba(0.0, 0.0, 0.0, 0.45),
                // 4 px y-offset (was 8) so the shadow sits closer
                // to the card and reads as a tight drop rather
                // than a far-away cast. 10 px blur (was 18) keeps
                // it soft but compact. 0 px spread (was 4) so the
                // shadow doesn't bleed beyond the card's
                // silhouette and create a wider halo.
                x_offset: Val::Px(0.0),
                y_offset: Val::Px(4.0),
                spread_radius: Val::Px(0.0),
                blur_radius: Val::Px(10.0),
            }]),
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

    // Inner bottom-edge shadow — thin dark line at the bottom of
    // the card body, complementary to the cyan rim at the top.
    // Together they read as a beveled glass edge: light hits the
    // top, shadow falls to the bottom, and the card visibly
    // protrudes above the panel backdrop. The previous single-rim
    // approach left the bottom edge visually flat, so the cards
    // read as 2D rectangles rather than raised panels.
    let bottom_shadow = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(0.0),
                // Inset 0.5 px to match the top rim and avoid
                // overlapping the corner radius.
                left: Val::Px(0.5),
                right: Val::Px(0.5),
                height: Val::Px(1.0),
                ..default()
            },
            // Dark shadow colour — same as the drop shadow's
            // tint, slightly less alpha so it reads as the inner
            // shadow cast by the rim rather than a duplicate
            // outline.
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45)),
            Name::new("card_bottom_shadow"),
        ))
        .id();
    commands.entity(card).add_child(bottom_shadow);

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
                    // v0.5.2 (2026-08-06): never let the header row
                    // squeeze the icon — when a long no-wrap subtitle
                    // overflowed, flexbox shrank the icon (default
                    // `flex_shrink: 1`) down to ~1 px, rendering it
                    // as a "vertical cyan line". The title column's
                    // `min_width: 0` (see above) fixes the overflow;
                    // this guarantees the icon size is untouchable.
                    flex_shrink: 0.0,
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
                    flex_shrink: 0.0,
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
                // v0.5.2 (2026-08-06): `min_width: 0` is load-bearing.
                // The subtitle's no-wrap marquee track (two wide text
                // copies, `flex_shrink: 0`) forces this column's
                // intrinsic width to the full text width. WITHOUT
                // `min_width: 0` the column refuses to shrink below
                // that content, so it overflows the header row and
                // the flex algorithm shrinks the 36-px icon (default
                // `flex_shrink: 1`) down to ~1 px — the "vertical
                // cyan line" instead of the building icon. With it,
                // the column bounds to the row, the icon keeps its
                // size, and the clip container's measured width is
                // the real column width (so the marquee's overflow
                // detection fires).
                min_width: Val::Px(0.0),
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
                // Single-line reservation (v0.5.2). The text uses
                // `TextLayout::new_with_no_wrap()` so it's one
                // horizontal line; a 20-px-tall slot comfortably
                // fits a 14 px line-height CAPTION_SIZE text
                // (line-height ≈ 14 × 1.2 ≈ 16.8 px). The previous
                // 28-px height was sized for two lines but the
                // text never wraps, so the extra 8 px of headroom
                // was dead space that the player read as
                // "truncated" — the slot looked too tall for a
                // single line. `height` (not `min_height`) is
                // required so the Queue button doesn't get pushed
                // down. `Overflow::clip` is the safety net for
                // stray longer strings and masks the off-screen
                // half during the marquee scroll animation.
                height: Val::Px(20.0),
                // v0.5.2 (2026-08-06): bound the clip to the title
                // column so its measured `size().x` is the real
                // visible width — the marquee's overflow test
                // (`text_width > container_width`) needs this to
                // be smaller than the full no-wrap text width.
                // Without an explicit width the clip grows to fit
                // its wide track, container_width == text_width,
                // and the marquee never fires (subtitles just get
                // cut off). `min_width: 0` lets the flex column
                // shrink it below the track's intrinsic width.
                width: Val::Percent(100.0),
                min_width: Val::Px(0.0),
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

    // v0.5.2 PR-A.7 (2026-08-04): dedicated Power chip — the FIRST
    // item in the card body, sitting between the stats row and
    // the effect bullets. Rendering it first (rather than after
    // the effects loop) means every card has the same top-of-
    // body anchor: BP / workers → [Power chip] → Produces
    // effect (if any) → resource-cost strip. Cards without a
    // Produces effect (the common case) now have a consistent
    // top zone instead of jumping straight from the stats row
    // to the resource-cost strip.
    //
    // Visual recipe (per user feedback 2026-08-04):
    //   * Chip chrome (background, border, icon) = YELLOW_ENERGY.
    //     Yellow was chosen so the chip reads as a distinct
    //     third category (consumption vs production, both
    //     yellow-chromed) while the text colour inside the
    //     chip still differentiates the two.
    //   * Text colour = RED for consumption (`per_unit_mw < 0`),
    //     GREEN_FIN for production (`per_unit_mw > 0`),
    //     TEXT_BODY for no power interaction (`== 0`).
    //   * Label = `<sign><amount>` with the +/- prefix taken
    //     from the sign of `per_unit_mw`. Replaces the old
    //     `"Demand 50 MW"` / `"Produces 5.0 GW"` verb strings.
    //     Examples: `"-50 MW"` (red), `"+5.0 GW"` (green),
    //     `"0 W"` (neutral).
    //   * `power_insufficient` (still on `BuildCardData`) is
    //     used by the Queue button to disable itself with a
    //     "not enough energy" tooltip — it no longer drives
    //     the chip colour because the sign of the demand
    //     already conveys the consumption/production split.
    let power_chrome = YELLOW_ENERGY;
    let power_text_color = if data.power_chip.per_unit_mw < -0.01 {
        RED
    } else if data.power_chip.per_unit_mw > 0.01 {
        GREEN_FIN
    } else {
        TEXT_BODY
    };
    // Sign prefix: `-` for consumption, `+` for production,
    // nothing for no interaction. The amount is already
    // absolute (formatter takes the magnitude); we just
    // prepend the sign for the +/- convention.
    let sign_prefix = if data.power_chip.per_unit_mw < -0.01 {
        "-"
    } else if data.power_chip.per_unit_mw > 0.01 {
        "+"
    } else {
        ""
    };
    let power_chip_label = format!(
        "{}{}",
        sign_prefix, data.power_chip.amount
    );
    let power_chip = commands
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(4.0),
                padding: UiRect::horizontal(Val::Px(6.0)),
                height: Val::Px(32.0),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(power_chrome.with_alpha(0.12)),
            BorderColor::all(power_chrome.with_alpha(0.35)),
            Pickable::default(),
            PowerChip {
                tooltip_lines: data.power_chip.tooltip_lines.clone(),
                tone: power_text_color,
                card,
            },
            Name::new("power_chip"),
        ))
        .id();
    commands.entity(card).add_child(power_chip);
    // Hover observers (mirror the resource_cost_chip pattern).
    commands.entity(power_chip).observe(on_power_chip_hover_over);
    commands.entity(power_chip).observe(on_power_chip_hover_out);

    // Icon: 24×24, bolt-in-hex PNG, tinted yellow. v0.5.2
    // PR-A.7 (2026-08-04): bumped from 20×20 to 24×24 (and
    // the chip height from 28 to 32) so the bolt-in-hex is
    // the most prominent visual element on the chip — the
    // user explicitly wanted the icon "brought in front and
    // made larger" so it reads as the chip's identity, with
    // the +/- amount as supporting text. Falls back to a
    // yellow-tinted square if the asset hasn't decoded yet.
    let power_icon_node = match get_energy_icon_handle_bevy(resource_icons) {
        Some(handle) => commands
            .spawn((
                Node {
                    width: Val::Px(24.0),
                    height: Val::Px(24.0),
                    ..default()
                },
                ImageNode::new(handle.clone()).with_color(power_chrome),
                Name::new("power_chip_icon"),
            ))
            .id(),
        None => commands
            .spawn((
                Node {
                    width: Val::Px(24.0),
                    height: Val::Px(24.0),
                    border_radius: BorderRadius::all(Val::Px(2.0)),
                    ..default()
                },
                BackgroundColor(power_chrome.with_alpha(0.85)),
                Name::new("power_chip_icon_placeholder"),
            ))
            .id(),
    };
    commands.entity(power_chip).add_child(power_icon_node);

    // Label: `<sign><amount>` so the chip reads as
    // "-50 MW" / "+5.0 GW" / "0 W". Text colour is the
    // sign-driven red/green/neutral, NOT the yellow chrome —
    // the colour contrast inside the chip carries the
    // consumption/production signal.
    let label = commands
        .spawn((
            Text::new(power_chip_label),
            TextFont {
                font: mono_font.clone(),
                font_size: CAPTION_SIZE,
                ..default()
            },
            TextColor(power_text_color),
            Name::new("power_chip_label"),
        ))
        .id();
    commands.entity(power_chip).add_child(label);

    // Effect bullets. v0.5.2 PR-A.6 (2026-08-03): bullet text
    // is rendered in `mono_font` (GeistMono) instead of
    // `body_font` (Inter Regular) so the lines read as a
    // continuation of the chip-strip font and the BP/COST
    // stat row above. Previously the Inter Regular body font
    // broke the visual rhythm — the bullets sat next to the
    // chip strip in two different fonts and the card body read
    // as two unrelated panels (per screenshot feedback
    // 2026-08-03). The colour map is unchanged.
    //
    // v0.5.2 PR-A.7 (2026-08-04): the Power chip is now above
    // this loop (not below), so the "Produces X Mt/yr"
    // effect is the second body element, right under the
    // power chip. Read order: [Power chip] → [Produces effect,
    // if any] → [resource-cost strip].
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
        // Costs in `BuildingDefinition::resource_costs` are
        // stored in **megatonnes** (Mt) — the same unit the
        // `LocalStockpile` and `process_construction_actions`
        // use, so the chip number matches the actual deduction.
        // `format_mining_reserve` expects Mt and auto-picks the
        // smallest SI suffix that lands the value in 1..=999
        // (so 0.05 Mt reads as "50 kt", 50 Mt reads as
        // "50 Mt", 50,000 Mt reads as "50 Gt").
        //
        // v0.5.2 (build menu fix): a previous version of this
        // line did `format_mining_reserve(cost.amount / 1_000.0)`
        // on the (incorrect) assumption that RON stored kt. That
        // displayed a number 1 000× smaller than the real cost —
        // a 50 Mt Iron cost showed as "50 kt", a 17 Mt Copper
        // cost showed as "17 kt", and the player never saw the
        // true amount. The chip now matches the per-building
        // deduction.
        let amount_str = format_mining_reserve(cost.amount);

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
    //
    // v0.5.2 (2026-08-06): constructed cards have no ETA (already
    // built) — skip the hairline + ETA row entirely.
    if !data.constructed {
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
    }

    // Filled cyan CTA. Uses the `Button` widget so picking interaction
    // is wired automatically (Button has `#[require(Interaction)]`,
    // so the picking plugin auto-sets the hover/pressed state).
    //
    // v0.5.2 (2026-08-06): constructed cards skip the CTA entirely
    // (the Demolish button is the only action) — the old code spawned
    // an empty grey button stub on every Buildings-tab card.
    if !data.constructed {
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
                ConstructionCta { building_type },
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
        // v0.5.2 (build menu fix): body-blocked mining cards (e.g. an
        // Iron Mine on a gas giant) get a separate, **permanent**
        // disabled marker so the per-frame affordability system
        // doesn't silently re-enable the CTA when the player has
        // the resources on hand. The hover system treats this
        // marker identically to `ConstructionCtaDisabled` (no hover
        // scale, no colour change); the affordability system
        // ignores it.
        if data.body_blocked {
            commands.entity(cta).insert(ConstructionCtaBodyBlocked);
        }
        commands.entity(card).add_child(cta);

        // CTA label (text child needs flex_grow: 1.0 to participate in the parent's
        // justify-content: center — without it, text defaults to its own intrinsic
        // width and sits at the left edge).
        //
        // v0.5.2 (build menu fix): tagged with `ConstructionCtaLabelMarker`
        // so `tick_construction_cta_label_dim` can find it without
        // walking the name. The marker dim pass tints the label
        // `TEXT_DIM` when the parent CTA carries
        // `ConstructionCtaDisabled` so the disabled state reads at a
        // glance — the cyan text on a dim background was misleading
        // ("looks clickable, does nothing").
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
                ConstructionCtaLabelMarker,
                Name::new("card_cta_label"),
            ))
            .id();
        commands.entity(cta).add_child(cta_label);
    }

    // Return the card entity so callers (e.g. the Mining tab's
    // `spawn_mining_card` wrapper) can attach additional
    // siblings (body-gate caption, etc.) if they need to.
    card
}

// Hover / click effect system for the Queue CTAs.
///
// On hover: background brightens from CTA_FILL to CTA_FILL_HOVER, border goes
// to fully-opaque cyan, and the entity scales up by 1.02.
// On press: scale drops to 0.98.
// On release: returns to default.
///
// Disabled CTAs (marked `ConstructionCtaDisabled` by the can_afford
// check) get a **red** treatment — red border, dim-red fill, no scale
// change — so the player reads the issue at a glance ("this button is
// broken, here's why"). The hover state does not light the button up:
// the click is a no-op, so the cyan hover affordance would be
// misleading. The label color is handled by
// `tick_construction_cta_label_dim` (kept separate to sidestep the
// Bevy 0.18 dual-mut rule). We use a `ParamSet` to read the same
// `ConstructionCta` set twice (once for the visual loop, once to
// check the disabled marker) without conflicting with the mutable
// accesses below.
///
// **Flicker mitigation**: we track each CTA's previous
// `(interaction, is_disabled)` pair in a `Local<HashMap>` and only
// re-apply the visual when the pair changes. Without this, the
// Queue button could visibly flicker when the disabled marker
// toggles frame-to-frame (e.g. `ContextualStockpile` updates while
// the cursor is hovering the button) — every frame would re-write
// the same BackgroundColor / BorderColor / UiTransform, but the
// cursor-mapped interaction state + the stock-driven disabled
// state don't always settle, so the visual oscillates between
// the dim (disabled) and bright (hovered) palettes. Edge-only
// state changes get filtered out — a CTA that just had its
// disabled marker flipped from `true` → `false` because the
// player earned enough Iron now renders its hover affordance on
// this frame instead of next.
pub fn tick_construction_cta_hover(
    mut params: ParamSet<(
        Query<
            (
                Entity,
                &Interaction,
                &mut BackgroundColor,
                &mut BorderColor,
                &mut UiTransform,
            ),
            With<ConstructionCta>,
        >,
        // v0.5.2 (build menu fix): the disabled set now matches
        // both resource-driven (`ConstructionCtaDisabled`) and
        // body-blocked (`ConstructionCtaBodyBlocked`) markers so
        // a body-blocked mining card never gets a hover scale or
        // colour change — the per-frame affordability system
        // doesn't toggle the body-blocked marker, so this is the
        // only place the hover affordance gets suppressed.
        Query<Entity, Or<(With<ConstructionCtaDisabled>, With<ConstructionCtaBodyBlocked>)>>,
    )>,
    mut prev_state: Local<std::collections::HashMap<Entity, (Interaction, bool)>>,
) {
    // Pre-compute the disabled set so the visual loop stays single-Q.
    let mut disabled_set: std::collections::HashSet<Entity> = std::collections::HashSet::new();
    for entity in params.p1().iter() {
        disabled_set.insert(entity);
    }
    for (entity, interaction, mut bg, mut border, mut ui_transform) in params.p0().iter_mut() {
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
            Interaction::None if !is_disabled => {
                *bg = BackgroundColor(CTA_FILL);
                *border = BorderColor::all(CYAN_BORDER_STRONG);
                ui_transform.scale = Vec2::splat(1.00);
            }
            // Disabled — every interaction state (None, Hovered,
            // Pressed) gets the same dim/grey treatment. We
            // deliberately avoid the red palette here: red
            // reads as a *highlight* / *alert* in the rest of
            // the UI (notifications, warning chips, error
            // states), so a red button looks "active and
            // important" rather than "broken and inert". The
            // dim/grey fill + grey border + no hover scale
            // matches standard disabled-CTA conventions and
            // tells the player the click is a no-op without
            // implying anything will happen on hover.
            _ => {
                // Recessed fill — ~4% white on the card's dark
                // navy, so the button reads as a sunken panel
                // rather than a glowing target. Border is a
                // desaturated dim grey (TEXT_DIM @ 40% alpha)
                // so the outline stays visible but doesn't pop
                // against the surrounding card chrome. The
                // label is tinted TEXT_DIM by
                // `tick_construction_cta_label_dim`.
                *bg = BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.04));
                *border = BorderColor::all(Color::srgba(
                    0.498, 0.580, 0.659, 0.40,
                ));
                ui_transform.scale = Vec2::splat(1.00);
            }
        }
        prev_state.insert(entity, (*interaction, is_disabled));
    }
}

// Animate the `UiTransform.translation` of every `SubtitleMarquee`
// track so an overflowing subtitle scrolls left/right on hover.
///
// Two text copies live in the track; at `phase = 1.0` the visible
// content is byte-identical to `phase = 0.0`, so the cycle is
// seamless. Phase increments while the parent card is hovered
// (rate: ~one full cycle every 2.4 s) and decrements back to zero
// when hover ends, releasing the player back to copy A's start
// position so they can re-read from the beginning.
///
// Why this lives in `Update` and not `EguiPrimaryContextPass`:
// `bevy_ui` native widgets are layout-driven (their picking and
// hover state are computed by the engine in `Update`), so the
// system has to read them on the same schedule the engine writes
// them. The egui-pass restriction only applies to systems that
// call egui context APIs.
///
// B0001 audit: the system takes two `Query` parameters but they
// reach for disjoint component sets
// (`Interaction` on cards vs `UiTransform`+`SubtitleMarquee` on
// tracks), so the planner treats them as parallel-friendly.
///
// The first-frame issue: `ComputedNode` is populated by the
// engine's layout pass *after* our spawn but before the next
// `Update`. We read it on tick 2+. Until then `text_width` and
// `container_width` stay at 0 and the marquee is dormant — which
// is the correct visual (text at translation `(0, 0)`, no scroll).
pub fn tick_subtitle_marquee(
    time: Res<Time>,
    text_computed: Query<&ComputedNode>,
    mut tracks: Query<(Entity, &mut SubtitleMarquee, &mut UiTransform)>,
) {
    // v0.5.2 (2026-08-03): continuous slow rotation. The
    // previous oscillation (phase 0 → 1 → 0 → 1, reversing at
    // each end) read as "the marquee fires once and then just
    // sits there" because each direction reversal came with a
    // visible reset to copy A's start position. The new
    // marquee drives the phase linearly in one direction
    // (modulo 1.0) so the track loops seamlessly and never
    // resets.
    //
    // Speed tuning: ~30 px/s — slow enough to feel ambient
    // (the player can read at their own pace), fast enough
    // that the seam between copy A and copy B is never far
    // away. At 320 px wide subtitles the full text-width
    // traversal takes ~10 s; at 150 px wide ones ~5 s. Each
    // card carries its own `phase` accumulator so they
    // appear desynchronized (no marching-band effect), but
    // the **rate** is uniform — every card scrolls at
    // `PIXELS_PER_SECOND`, which kills the "different speed
    // on each card" complaint from the v0.5.2 visual review.
    const PIXELS_PER_SECOND: f32 = 30.0;
    // Clamp delta — first frame after a long stall (eg
    // debugger pause) shouldn't snap phase across the seam
    // in one tick. The 0.1 s cap is gentle enough that the
    // marquee catches up within ~150 ms of unpausing.
    let dt = time.delta_secs().min(0.1);

    for (_track_entity, mut marquee, mut ui_transform) in tracks.iter_mut() {
        // 1) Refresh measured widths from `ComputedNode`. The
        // text node carries its measured content size (the
        // rendered pixel width of the wrapped description);
        // the clip container carries its allocated outer
        // width. Both are read each frame because layout can
        // change between ticks (window resize, font fallback).
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

        // 2) Detect overflow vs. fit. Below the +0.5 px
        // epsilon the description fits the container and
        // the marquee is dormant (translation `(0, 0)`,
        // phase stops advancing). This is the same threshold
        // the v0.5.0 oscillation used, but the phase is
        // **held** rather than reversed — a non-overflowing
        // description just stops drifting instead of
        // snapping back to its start.
        //
        // v0.5.2: the marquee is **always on** for overflowing
        // descriptions — no hover gate, no per-card delay.
        // The previous hover-gated design made the player
        // perceive the subtitle as "cut off" (because the
        // text was static at `(0, 0)` until they hovered, by
        // which point the clip height of 28 px was already
        // reading as too tall for a single line). The clip
        // is now 20 px and the marquee runs from the first
        // frame, so the visible contract is "text that
        // doesn't fit scrolls leftward, smoothly, on its
        // own."
        let overflows = text_width > container_width + 0.5;

        if overflows {
            // 3) Advance the phase linearly. Phase is in
            // **pixels** (not 0..=1) so we can integrate at
            // a constant pixel rate without per-card
            // normalisation. `mod text_width` wraps the
            // value back to copy A's start position; the
            // resulting translation is
            // `-phase_in_pixels (mod text_width)`, which is
            // exactly the seamless loop the v0.5.0 oscillation
            // approximated but couldn't deliver.
            let delta_pixels = dt * PIXELS_PER_SECOND;
            marquee.phase += delta_pixels;
            // Wrap so the floating-point value doesn't grow
            // unboundedly over a long-running session.
            if marquee.phase >= text_width {
                marquee.phase -= text_width;
            }
        }
        // Non-overflowing cards leave phase untouched and
        // translation sits at `(0, 0)` below.

        // 4) Apply the translation. Negative x drifts the
        // track leftward; at `phase = 0` the visible content
        // is byte-identical to `phase = text_width`, so the
        // seam is invisible. The translation magnitude is
        // bounded by `text_width`, so the off-screen copy
        // never bleeds past the clip container (Bevy's
        // `Overflow::clip` on the parent hides it anyway,
        // but staying within the bound keeps `UiTransform`
        // numerically clean).
        let dx = -marquee.phase;
        ui_transform.translation = Val2::px(dx, 0.0);
    }
}

// Wheel-scroll handler for `Overflow::scroll_y` containers.
///
// Bevy 0.18 ships with no built-in scroll wheel handler for
// `bevy_ui` — the engine renders scrollbars and clamps
// `ScrollPosition` correctly, but it never reads `MouseWheel`
// events to *update* `ScrollPosition`. Without this system, the
// `card_grid` (and the queue panel body, etc.) silently ignore
// the scroll wheel even when content overflows.
///
// Strategy:
// 1. For each `MouseWheel` event, look up the topmost hovered
//    entity via `HoverMap` (populated by the picking plugin).
// 2. Walk up the parent chain until we find an entity with
//    `Overflow::scroll_y` set on its `Node`. That's our
//    scrollable.
// 3. Adjust the scrollable's `ScrollPosition` by the wheel's
//    `y` delta (with a small multiplier for "snappy" feel) and
//    clamp to `[0, max_y]` where `max_y` is the difference
//    between `content_size().y` and `size().y`.
///
// Why parent-walk and not iterate every scrollable: the user
// usually has the cursor over a card, not the scrollable itself.
// Cards live several parents deep inside `card_grid`. We can't
// put `Pickable` on `card_grid` directly (it's not in the
// hover-to-scroll contract) and we don't want to put picking
// state on every card just to find the scrollable. The walk
// is bounded by the panel depth (typically 4-5 hops) and only
// runs on wheel events, not every frame.
///
// Edge case: when the player hovers a card and scrolls, the
// hover entity is the card. We walk up: card → `header_row` →
// `title_col` → `subtitle_clip` (overflow: clip, not scroll) →
// etc. The first ancestor with `Overflow::scroll_y` is the
// `card_grid` itself. Good.
///
// Drive the always-visible scrollbar overlay on the card grid.
///
// For each `CardGridScrollbarTrack`, this system measures the
// parent's (the `card_grid`'s) content height + visible height +
// current `ScrollPosition` and resizes/repositions the thumb
// proportionally. When the content fits the viewport the thumb
// is hidden (height 0).
///
// Why the system exists: Bevy 0.18's `bevy_ui` core auto-renders
// scrollbar visuals only when content overflows and never exposes
// an always-visible track — there's no API to say "show this
// scrollbar even when not hovering, in this exact color, at this
// exact width". The overlay node we spawned in
// `setup_construction` is just a styled box; this system wires it
// to the actual scroll position so the thumb moves with the
// content.
///
// B0001: we hold `&ComputedNode` on the grid + track and `&mut Node`
// + `&mut UiTransform` on the thumb. Reading `&mut ScrollPosition`
// would force a third `Query` for no benefit (we only read it),
// but Bevy 0.18 still flags overlapping component reads on the
// same entity as B0001. Solution: the grid is read here as a
// plain data source (`ComputedNode` + `ScrollPosition` via
// `&ScrollPosition` not `&mut ScrollPosition`) and the thumb
// writes happen on a different archetype (different components).
// To be belt-and-braces we wrap in a `ParamSet` so the planner
// sees the disjoint access scopes explicitly.
pub fn tick_construction_scrollbar(
    // ParamSet gives the system disjoint mutable + immutable borrows
    // on the same grid without tripping B0001. `tracks` reads each
    // track's measured height + the scrollable target it points
    // at; `bodies` reads each scrollable's content height + scroll
    // position so we can position the thumb. `thumbs` then writes
    // the thumb's height + Y top-offset.
    //
    // v0.5.2 PR-A.8: previously the queries were filtered by
    // `With<CardGrid>` so only the Build tab's card grid had a
    // scrollbar. Now we read the track's `target: Entity` field
    // and look up the scrollable by id, so any tab with a
    // `ConstructionScrollbarTrack { target, tab }` gets the same
    // overlay (currently: Build + Mining).
    mut params: ParamSet<(
        Query<(&ComputedNode, &ConstructionScrollbarTrack)>,
        Query<&ComputedNode>,
        Query<&ScrollPosition>,
        Query<&mut Node, With<ConstructionScrollbarThumb>>,
    )>,
    ui_state: Res<ConstructionUiState>,
    mut metrics: ResMut<ConstructionScrollbarMetrics>,
) {
    // We expect exactly one active track (the visibility system
    // hides the others), but loop to be safe. Read all inputs
    // first so the ParamSet borrows drop before we write — Bevy
    // 0.18 still flags holding multiple mutable borrows from one
    // ParamSet as overlapping.
    let track_data: Option<(f32, Entity)> = {
        let p0 = params.p0();
        // Find the track whose `tab` matches the currently-active
        // body. The visibility system already flipped
        // `Display::None` on the inactive tracks' nodes; their
        // `ComputedNode::size()` may be stale but reading it is
        // safe (returns last-known size). We rely on the `tab`
        // field as the source of truth so we don't accidentally
        // drive a hidden scrollbar's thumb.
        let active = ConstructionTabBody::from_tab(ui_state.selected_tab);
        p0.iter()
            .find(|(_, track)| track.tab == active)
            .map(|(c, track)| (c.size().y, track.target))
    };
    let Some((track_height, scrollable_entity)) = track_data else {
        return;
    };
    let (grid_size, grid_content_height, scroll_y) = {
        let p1 = params.p1();
        let Some(grid_computed) = p1.get(scrollable_entity).ok() else {
            return;
        };
        let grid_size = grid_computed.size();
        let grid_content_height = grid_computed.content_size().y;
        let scroll_y = {
            let p2 = params.p2();
            let Some(scroll_pos) = p2.get(scrollable_entity).ok() else {
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
    // (the track's own coordinate space, top-left origin). All
    // thumbs get the same height + Y (only the visible one is
    // hit by the picking backend; the hidden ones inherit
    // `Display::None` from their parent track and never see a
    // pointer event).
    for mut node in params.p3().iter_mut() {
        node.height = Val::Px(thumb_height);
        node.top = Val::Px(thumb_y);
    }
}

// Drag-to-scroll for the construction card grid scrollbar. When
// the player presses the thumb (or the track) and drags the mouse,
// translate the Y delta into a `ScrollPosition` change. The wheel
// handler (`tick_ui_scroll_on_wheel`) already covers the scroll
// wheel; this system covers click-and-drag.
///
// Why this is its own system (not folded into
// `tick_construction_scrollbar`): drag is event-driven (only
// active while `Interaction::Pressed` on the thumb OR the
// track) and needs a `MessageReader<CursorMoved>` to detect
// pointer motion. Mixing event-driven work into the per-frame
// visual tick system is fine in Bevy but a separate system keeps
// each function's responsibility clear.
///
// Two press surfaces share the same param:
// 1. **Thumb**: dragging the thumb scrolls the grid (existing
//    behavior; the thumb's `Interaction` is `Pressed`).
// 2. **Track empty area**: clicking on the track above/below
//    the thumb "page-jumps" the scroll so the thumb's center
//    lands at the click point — the standard Windows / macOS
//    scrollbar behavior. The track's `Interaction` is `Pressed`
//    and the cursor position is read from `RelativeCursorPosition`
//    on the track entity. After the page jump, the player can
//    keep dragging and the gesture continues as a thumb-drag
//    from the new position.
///
// Why `ParamSet`: this system holds `Query<&mut Node>` on the
// thumb (for the press-cursor feedback) and `Query<&mut
// ScrollPosition>` on the grid (for the actual scroll update).
// Bevy 0.18's planner treats these as overlapping component
// access on different archetypes; we wrap in `ParamSet` to make
// the disjoint access scopes explicit.
///
// Why a `Resource` (not `Local<DragState>`): we need pe-press /
// on-release observers to mutate the drag state on entities other
// than the system that runs the per-frame drag. Bevy 0.18's
// observer API exposes `Trigger<Pointer<Press>>` /
// `Trigger<Pointer<Release>>` as entity-scoped events that fire
// regardless of where the pointer goes after the press — that's
// the only way to keep a drag "locked" to the originally-clicked
// surface (the thumb is only 6 px wide, so a one-pixel slip would
// otherwise drop `Interaction::Pressed` to `None` and the drag
// would die immediately). Putting the state on a `Resource`
// lets observers and the per-frame system share the same
// authoritative state.
#[derive(Resource, Default)]
pub(crate) struct ScrollbarDragState {
    // Whether a drag is currently in progress.
    pub(crate) active: bool,
    // Whether the drag started on the empty track (page-jump)
    // vs the thumb (handle-drag). The former snaps the page
    // immediately on press; the latter scrolls continuously.
    pub(crate) started_on_track: bool,
    // Y in the track's local space where the press happened.
    // Used by the page-jump snap to position the thumb's
    // `top` so the thumb's centre sits at the click point.
    pub(crate) press_track_y: f32,
}

// On-press observer for the `ConstructionScrollbarThumb`. Fires when
// the user presses the thumb; sets `ScrollbarDragState.active`
// and records that the drag started on the thumb (not the
// track). The observer stays attached for the lifetime of the
// thumb entity, so it survives every per-frame scrollbar
// rebuild — the entity itself is the long-lived parent of the
// visual thumb.
fn on_thumb_press(on: On<Pointer<Press>>, mut drag: ResMut<ScrollbarDragState>) {
    // Only the primary (left) mouse button initiates a drag.
    if on.event.button != PointerButton::Primary {
        return;
    }
    drag.active = true;
    drag.started_on_track = false;
    drag.press_track_y = 0.0;
}

// On-release observer for the `ConstructionScrollbarThumb`. Fires
// when the user releases anywhere on the thumb (or, by observer
// propagation, on any descendant). Clears the drag state.
fn on_thumb_release(on: On<Pointer<Release>>, mut drag: ResMut<ScrollbarDragState>) {
    if on.event.button != PointerButton::Primary {
        return;
    }
    drag.active = false;
    drag.started_on_track = false;
}

// On-press observer for the `ConstructionScrollbarTrack`. Fires
// when the user presses the empty track area (not the thumb,
// which is a child and would consume the press first). The
// press position is captured in the track's local Y so the
// per-frame drag system can apply the page-jump snap.
fn on_track_press(
    on: On<Pointer<Press>>,
    mut drag: ResMut<ScrollbarDragState>,
    track_query: Query<&RelativeCursorPosition, With<ConstructionScrollbarTrack>>,
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

// On-release observer for the `ConstructionScrollbarTrack`. Mirrors
// `on_thumb_release` — clears the drag state when the pointer
// button is released.
fn on_track_release(on: On<Pointer<Release>>, mut drag: ResMut<ScrollbarDragState>) {
    if on.event.button != PointerButton::Primary {
        return;
    }
    drag.active = false;
    drag.started_on_track = false;
}

pub fn tick_construction_scrollbar_drag(
    mut cursor_events: MessageReader<CursorMoved>,
    ui_state: Res<ConstructionUiState>,
    tracks: Query<&ConstructionScrollbarTrack>,
    // Untyped `ScrollPosition` query — we look up the scrollable
    // entity by id from the active track's `target` field rather
    // than filtering by a marker, since the scrollable can be the
    // Build tab's card grid OR the Mining tab's body.
    mut scrollable_query: Query<&mut ScrollPosition>,
    metrics: Res<ConstructionScrollbarMetrics>,
    mut drag: ResMut<ScrollbarDragState>,
    // v0.5.2 (2026-08-06): the `Pointer<Release>` observers only
    // fire when the pointer is over the thumb/track at release
    // time. If the player drags the thumb and releases OUTSIDE it
    // (a completely normal gesture), `drag.active` stayed true and
    // the scroll kept following the cursor — "it keeps scrolling
    // when I release outside the thumb". Reading the button state
    // directly catches every release, wherever it happens.
    mouse_buttons: Res<ButtonInput<MouseButton>>,
) {
    // 1) Fast-path: no drag in progress. Drop any stale
    //    `CursorMoved` events so they don't pile up while the
    //    user is just hovering.
    if !drag.active {
        cursor_events.clear();
        return;
    }
    // 1b) Release-anywhere: if the primary button is no longer
    //     pressed (the player let go — possibly outside the
    //     thumb), cancel the drag. This is the fix for "keeps
    //     scrolling after release outside the thumb": the
    //     observer-based release never fired there.
    if !mouse_buttons.pressed(MouseButton::Left) {
        drag.active = false;
        drag.started_on_track = false;
        cursor_events.clear();
        return;
    }

    // 2) Find the active track's scrollable entity. The drag
    //    may have started on a track that's been hidden by a
    //    tab switch (e.g. user pressed the Mining scrollbar
    //    then switched to Build while still holding the
    //    button). Cancel the drag in that case rather than
    //    drive a hidden scrollable.
    let active = ConstructionTabBody::from_tab(ui_state.selected_tab);
    let scrollable = tracks
        .iter()
        .find(|t| t.tab == active)
        .map(|t| t.target);
    let Some(scrollable_entity) = scrollable else {
        drag.active = false;
        drag.started_on_track = false;
        cursor_events.clear();
        return;
    };

    let travel = (metrics.usable_track_height - metrics.thumb_height).max(1.0);
    let factor = metrics.max_scroll / travel;

    // 3) Page-jump snap: if the drag *started* on the empty
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
        if let Ok(mut pos) = scrollable_query.get_mut(scrollable_entity) {
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

    // 4) Continuous drag: each `CursorMoved.delta.y` translates
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
        if let Ok(mut pos) = scrollable_query.get_mut(scrollable_entity) {
            pos.y = (pos.y + dy * factor).clamp(0.0, metrics.max_scroll);
        }
    }
}

// Shared layout numbers from `tick_construction_scrollbar`. The
// drag system needs to know the thumb's travel range and the
// grid's max scroll to translate pixel deltas into scroll units.
// We publish these every frame in a `Resource` rather than via a
// shared system param because (a) Resources survive system
// re-scheduling and (b) the drag system shouldn't have to
// duplicate the visual tick's layout math.
#[derive(Resource, Default, Debug)]
pub struct ConstructionScrollbarMetrics {
    // Track height minus the top + bottom padding (`SPACE_SM *
    // 2`). This is the range of Y values the thumb can occupy.
    pub usable_track_height: f32,
    // Current rendered height of the thumb. Constant between
    // layout ticks; used to compute the thumb's travel range.
    pub thumb_height: f32,
    // `content_size.y - size.y` for the card grid — the max
    // scrollable distance in pixels. Zero when content fits.
    pub max_scroll: f32,
}

// Wheel-scroll handler for `Overflow::scroll_y` containers.
///
// Bevy 0.18 ships with no built-in scroll wheel handler for
// `bevy_ui` — the engine renders scrollbars and clamps
// `ScrollPosition` correctly, but it never reads `MouseWheel`
// events to *update* `ScrollPosition`. Without this system, the
// `card_grid` (and the queue panel body, etc.) silently ignore
// the scroll wheel even when content overflows.
///
// Strategy:
// 1. For each `MouseWheel` event, look up the topmost hovered
//    entity via `HoverMap` (populated by the picking plugin).
// 2. Walk up the parent chain until we find an entity with
//    `Overflow::scroll_y` set on its `Node`. That's our
//    scrollable.
// 3. Adjust the scrollable's `ScrollPosition` by the wheel's
//    `y` delta (with a small multiplier for "snappy" feel) and
//    clamp to `[0, max_y]` where `max_y` is the difference
//    between `content_size().y` and `size().y`.
///
// Why parent-walk and not iterate every scrollable: the user
// usually has the cursor over a card, not the scrollable itself.
// Cards live several parents deep inside `card_grid`. We can't
// put `Pickable` on `card_grid` directly (it's not in the
// hover-to-scroll contract) and we don't want to put picking
// state on every card just to find the scrollable. The walk
// is bounded by the panel depth (typically 4-5 hops) and only
// runs on wheel events, not every frame.
///
// Edge case: when the player hovers a card and scrolls, the
// hover entity is the card. We walk up: card → `header_row` →
// `title_col` → `subtitle_clip` (overflow: clip, not scroll) →
// etc. The first ancestor with `Overflow::scroll_y` is the
// `card_grid` itself. Good.
///
// B0001 audit: the system has one `Query` parameter; no
// dual-Query risk.
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
    // v0.5.2 PR-A.9 (2026-08-05): the scrollbar track is parented
    // to the panel root (not to its target scrollable) so it
    // stays pinned when the user scrolls. That means a wheel
    // event that lands on the track — picking the slim 12-px
    // track instead of the body behind it — would walk up the
    // chain and never find an `Overflow::scroll_y` ancestor
    // (the panel root has none). The user would see "wheel
    // doesn't work" exactly when their cursor happened to be
    // over the new track. We resolve by recognising the track
    // here: if the hover starts on a `ConstructionScrollbarTrack`
    // (or any descendant like the thumb), use its `target` field
    // as the scrollable directly. The walk-up still works for
    // cards / body / chips; only the track needs the lookup.
    tracks: Query<&ConstructionScrollbarTrack>,
    ui_state: Res<ConstructionUiState>,
    // v0.5.2 (2026-08-05 audit): fallback scroll containers for the
    // stale-hover case (Mining tab). `update_mining_body` despawns +
    // re-spawns every mining card each frame, so the `HoverMap`
    // (computed in `PreUpdate`) holds entity ids that are gone by the
    // time this system runs in `Update` — the walk-up above finds no
    // scroll ancestor and the wheel event is silently dropped. Build
    // cards persist (spawned by `refresh_card_grid` only on change),
    // so Build never hit this. Resolve by the active tab instead:
    // the tab's scroll container is stable, so even a stale hover
    // lands on the right scrollable.
    card_grids: Query<Entity, (With<CardGrid>, With<Node>)>,
    mining_contents: Query<Entity, (With<MiningContent>, With<Node>)>,
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
        let start_entity = *hovered_entities
            .keys()
            .next()
            .expect("non-empty checked above");
        // v0.5.2 PR-A.9 (2026-08-05): fast path — if the
        // hovered entity is a scrollbar track (or any
        // descendant of one), use the track's `target` field
        // as the scrollable. Walking up from the track would
        // reach the panel root (no `Overflow::scroll_y`) and
        // the wheel would no-op, which is the user-visible
        // "Mining tab doesn't scroll" bug.
        let mut scrollable: Option<Entity> = None;
        // 1) Is the start entity itself a track?
        if let Ok(track) = tracks.get(start_entity) {
            scrollable = Some(track.target);
        } else {
            // 2) Walk up to find a track ancestor (the thumb is
            //    a child of the track). The walk is bounded by
            //    panel depth (3-4 hops) so the cost is trivial.
            let mut cursor = start_entity;
            for _ in 0..10 {
                if let Ok(track) = tracks.get(cursor) {
                    scrollable = Some(track.target);
                    break;
                }
                let Ok(parent) = parents.get(cursor) else {
                    break;
                };
                cursor = parent.0;
            }
        }
        // 3) If no track ancestor, fall back to the original
        //    walk-up that finds the first `Overflow::scroll_y`
        //    ancestor. This is the path for cards / chips /
        //    group headers / body — i.e. anything inside the
        //    scrollable itself, which has the right overflow.
        if scrollable.is_none() {
            let mut cursor = start_entity;
            loop {
                if let Ok((_entity, node, _computed)) = nodes.p0().get(cursor) {
                    if matches!(node.overflow.y, OverflowAxis::Scroll) {
                        scrollable = Some(cursor);
                        break;
                    }
                }
                let Ok(parent) = parents.get(cursor) else {
                    break;
                };
                cursor = parent.0;
            }
        }
        // 4) v0.5.2 (2026-08-05 audit): tab fallback for stale
        //    hover entities (the Mining respawn churn). When the
        //    walk-up found nothing — the hovered card was despawned
        //    by `update_mining_body` this frame — resolve the
        //    scrollable from the active tab's stable container.
        //    This keeps wheel scrolling working on Mining even
        //    though its cards don't persist between frames.
        if scrollable.is_none() {
            scrollable = match ui_state.selected_tab {
                ConstructionTab::Mining => mining_contents.iter().next(),
                ConstructionTab::Build => card_grids.iter().next(),
                _ => None,
            };
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

// currently-selected multiplier, with the current `ContextualStockpile`.
///
// This is the canary's version of the egui panel's
// `can_afford_resources_multiplied` gate. The CTA itself stays visible
// (the player should see the building exists) but the click handler
// (`tick_construction_cta_click`) skips the push when the marker is
// present, and the hover system keeps the dim CTA_FILL background.
///
// Runs every frame in `Update` — the math is O(buildings × costs) and
// there are < 50 buildings so the cost is negligible. We don't gate
// this on `resource_changed` because the stockpile can change while
// the panel is open (mining, deliveries, etc.).
///
// Uses `queue_silenced` (Bevy 0.18+) so the `insert` / `remove`
// commands are dropped silently instead of panicking if the
// target entity was despawned by an earlier system in the same
// tick (e.g. `refresh_card_grid` despawns the old cards before
// the new ones are spawned). The system ordering in the plugin
// (`.after(refresh_card_grid)`) is the primary defence; the
// silenced queue is the safety net.
pub fn tick_construction_cta_disabled(
    mut commands: Commands,
    buildings_data: Res<BuildingsData>,
    contextual: Res<crate::economy::ContextualStockpile>,
    ui_state: Res<ConstructionUiState>,
    // v0.5.2 (build menu fix): the `Without<ConstructionCtaBodyBlocked>`
    // filter on the per-frame query keeps the system from touching
    // body-blocked mining cards. Their `ConstructionCtaDisabled`
    // marker was set at spawn time (because the Mining tab's
    // `power_insufficient` flag mirrors `body_blocked`) and the
    // resource-gate logic below has no way to know that the
    // marker is a body-block, not a resource shortage. Without
    // this filter, the system would silently re-enable an Iron
    // Mine on a gas giant the moment the player has the
    // resources on hand.
    ctas: Query<
        (Entity, &ConstructionCta, Has<ConstructionCtaDisabled>),
        Without<ConstructionCtaBodyBlocked>,
    >,
    local_stockpiles: Query<&crate::economy::LocalStockpile>,
) {
    let multiplier = ui_state.build_multiplier.max(1);
    // The destination colony's LocalStockpile is the authoritative
    // "can I queue this right now?" gate: `process_construction_actions`
    // (`src/colony/systems.rs`) deducts the cost from the colony's own
    // stockpile, not the system-wide aggregate. The contextual check
    // below is a fast-fail for "no body in the system has this", but
    // the local check is the one that matches the game logic — and is
    // the one the player will actually see fail when they try to
    // build a Water Management Complex on a colony that hasn't been
    // shipped the necessary Construction Metals.
    let active_colony = ui_state.selected_colony;
    let local = active_colony.and_then(|e| local_stockpiles.get(e).ok());
    for (entity, cta, already_disabled) in ctas.iter() {
        let costs = buildings_data.resource_costs(&cta.building_type);
        // v0.5.2 (2026-08-06): parse each cost name ONCE and reuse for
        // both the contextual + local checks. The old code called
        // `parse_resource_type` twice per cost entry (once in each of
        // `can_afford_costs` / `can_afford_costs_local`) — the string
        // parse is the only non-trivial per-CTA work in this sweep,
        // so halving it matters at 60+ CTAs.
        let typed_costs: Vec<(ResourceType, f64)> = costs
            .iter()
            .filter_map(|(name, amt)| {
                parse_resource_type(name).map(|rt| (rt, amt * multiplier as f64))
            })
            .collect();
        let can_afford = typed_costs
            .iter()
            .all(|(rt, need)| contextual.get(rt) >= *need);
        let can_pay_locally = match local {
            Some(ls) => typed_costs.iter().all(|(rt, need)| ls.get(rt) >= *need),
            None => true, // no colony selected (menu-screen preview) — leave the CTA active
        };
        let should_disable = !can_afford || !can_pay_locally;
        if should_disable && !already_disabled {
            commands.entity(entity).queue_silenced(InsertCtaDisabled);
        } else if !should_disable && already_disabled {
            commands.entity(entity).queue_silenced(RemoveCtaDisabled);
        }
    }
}

// `EntityCommand` that inserts `ConstructionCtaDisabled`. Used by
// `tick_construction_cta_disabled` via `queue_silenced` so the
// insert is dropped instead of panicking if the entity is already
// despawned by the time the command applies.
struct InsertCtaDisabled;

impl bevy::ecs::system::EntityCommand for InsertCtaDisabled {
    fn apply(self, mut entity: bevy::ecs::world::EntityWorldMut) {
        entity.insert(ConstructionCtaDisabled);
    }
}

// `EntityCommand` that removes `ConstructionCtaDisabled`. See
// `InsertCtaDisabled` for the rationale.
struct RemoveCtaDisabled;

impl bevy::ecs::system::EntityCommand for RemoveCtaDisabled {
    fn apply(self, mut entity: bevy::ecs::world::EntityWorldMut) {
        entity.remove::<ConstructionCtaDisabled>();
    }
}

// Marker on the `card_cta_label` text entity so the per-frame
// dim pass can identify it without walking the name. Spawned in
// `spawn_card` alongside the `Text` and `TextFont` components.
#[derive(Component)]
pub struct ConstructionCtaLabelMarker;

// Per-frame pass that dims the Queue CTA label when the CTA is
// disabled. The CTA label is a child of the CTA entity (spawned
// in `spawn_card`) — we find it via `ChildOf::parent()` and tint
// it `TEXT_DIM` when the parent is disabled (so the label reads
// as inert and matches the dim/grey chrome of the disabled button
// painted by `tick_construction_cta_hover`), `CYAN` otherwise.
///
// Kept in its own system (not folded into
// `tick_construction_cta_hover`) because Bevy 0.18 forbids two
// `&mut TextColor` borrows in the same system. Hover owns
// `BackgroundColor` / `BorderColor` / `UiTransform`; this system
// owns `TextColor`.
pub fn tick_construction_cta_label_dim(
    // v0.5.2 (build menu fix): the disabled set now includes
    // body-blocked CTAs so their label dims to TEXT_DIM
    // alongside resource-shortage CTAs. The two markers are
    // mutually-exclusive in practice (a body-blocked CTA
    // also has `ConstructionCtaDisabled` from spawn, but the
    // per-frame system skips it — see `tick_construction_cta_disabled`).
    cta_q: Query<
        Entity,
        Or<(With<ConstructionCtaDisabled>, With<ConstructionCtaBodyBlocked>)>,
    >,
    mut label_q: Query<
        (&ChildOf, &mut TextColor),
        With<ConstructionCtaLabelMarker>,
    >,
) {
    // Build a per-CTA disabled set so the label loop is a single
    // borrow of `label_q`. The `Or<…>` filter above already
    // excludes non-disabled CTAs, so every entity in the
    // iteration is dimmed.
    let mut disabled: std::collections::HashSet<Entity> =
        std::collections::HashSet::new();
    for entity in cta_q.iter() {
        disabled.insert(entity);
    }
    for (child_of, mut color) in label_q.iter_mut() {
        let target = if disabled.contains(&child_of.parent()) {
            // Dim grey so the label reads as inert and matches
            // the disabled button's dim/grey chrome. RED would
            // read as a highlight/alert (it's the error colour
            // elsewhere in the UI) and mislead the player into
            // thinking something important is happening on hover.
            TEXT_DIM
        } else {
            CYAN
        };
        *color = TextColor(target);
    }
}

// Helper: enumerate every local-stockpile shortfall for the
// given `costs × multiplier`, sorted by *missing* amount
// descending (so the most-binding resource is first). Returns
// `(display_name, missing)` pairs where `missing` is the
// per-batch gap in the same unit as the RON cost (Mt). The
// display name prefers `ResourceType::display_name()` over the
// raw RON key so `He-3` shows as `"Helium-3"`, `RareEarths` as
// `"Rare Earths"`, etc.
///
// Used by `tick_construction_tooltip` to build the
// `Missing:` multi-line hover tooltip on a disabled Queue CTA.
fn collect_local_shortfalls(
    local: &crate::economy::LocalStockpile,
    costs: &[(String, f64)],
    multiplier: u32,
) -> Vec<(String, f64)> {
    let mut all: Vec<(String, f64)> = Vec::new();
    for (name, amount) in costs {
        let total_needed = amount * multiplier as f64;
        if let Some(rt) = crate::colony::data::parse_resource_type(name) {
            let have = local.get(&rt);
            if have < total_needed {
                let missing = total_needed - have;
                let display = rt.display_name().to_string();
                all.push((display, missing));
            }
        }
    }
    // Largest missing first — the player reads top-down, and the
    // most-binding single resource is the bottleneck that
    // matters most for "why can't I queue this right now?".
    all.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    all
}

// v0.5.2 (2026-08-06): `can_afford_costs` / `can_afford_costs_local`
// were removed — the affordability sweep (`tick_construction_cta_disabled`)
// now parses each cost name once into a typed `Vec<(ResourceType, f64)>`
// and checks both the contextual + local stockpiles against that single
// parse. See the sweep body for the canonical shape.

// Refresh the card grid: despawn all existing `ConstructionCard`
// entities and re-spawn based on the current `ConstructionUiState`.
// Runs whenever `ConstructionUiState` changes (via chip clicks).
pub fn refresh_card_grid(
    mut commands: Commands,
    fonts: Res<UiFonts>,
    buildings_data: Res<BuildingsData>,
    research_state: Res<ResearchState>,
    ui_state: Res<ConstructionUiState>,
    building_icons: Option<Res<BuildingIcons>>,
    resource_icons: Option<Res<ResourceIcons>>,
    // v0.5.2 (2026-08-06): despawn ONLY the Build-tab cards (the ones
    // this system spawned, tagged `BuildCard`). The old
    // `With<ConstructionCard>` matched EVERY card in the world —
    // Build, Mining, AND Buildings — because `spawn_card` attaches
    // `ConstructionCard` to all of them. The Mining + Buildings
    // cards were wiped on every `ConstructionUiState` change (chip
    // click / tab switch / mining group collapse) and their caches
    // couldn't recover (Buildings stayed empty; Mining churned a
    // full rebuild per click → the flicker + missing scrollbar).
    card_query: Query<Entity, With<BuildCard>>,
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
    let Ok(card_grid) = grid_query.single() else {
        return;
    };
    // Re-spawn based on the current state.
    // v0.5.2 (2026-08-06): fonts from the cached `UiFonts` resource
    // (was re-loading all three on every state change).
    let body_font = fonts.body.clone();
    let body_font_medium = fonts.medium.clone();
    let mono_font = fonts.mono.clone();
    let category_idx = ui_state.selected_build_tab;
    let filter = ui_state.selected_filter;
    let multiplier = ui_state.build_multiplier;
    // v0.5.2 PR-A.2: thread the active colony's grid spare into each
    // card so the Power effect line can show "demand vs spare" with
    // a red ⚠ marker when the batch would push the grid into deficit.
    let spare_power_mw = compute_colony_spare_power_mw(&ui_state, &colonies, Some(&buildings_data));
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
        let card = spawn_card(
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
        // v0.5.2 (2026-08-06): tag this card as Build-tab-owned so the
        // next `refresh_card_grid` despawn (filtered on `BuildCard`)
        // leaves Mining + Buildings cards alone.
        commands.entity(card).insert(BuildCard);
    }
}

// Auto-select the first available colony if `selected_colony` is None
// or points at a despawned entity. This makes the Queue button work
// out of the box without requiring the player to manually pick a
// colony from the dropdown, and gracefully recovers when the selected
// colony has been removed (e.g. dissolved mid-game).
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

// Click handler for the "Active Colony" picker. Toggles
// `ColonyDropdownState::open` so the floating dropdown appears /
// disappears. Same rising-edge detection as the chip / CTA click
// handlers. `Option<ResMut<…>>` defends against a missing-resource
// startup race (see `tick_open_queue_chip_click`).
pub fn tick_colony_picker_click(
    interactions: Query<(Entity, &Interaction), (With<ColonyPicker>, With<Button>)>,
    mut state: Option<ResMut<ColonyDropdownState>>,
    mut prev: Local<std::collections::HashMap<Entity, Interaction>>,
) {
    let mut current: std::collections::HashMap<Entity, Interaction> =
        std::collections::HashMap::new();
    for (entity, interaction) in interactions.iter() {
        let prev_interaction = prev.get(&entity).copied().unwrap_or(Interaction::None);
        if *interaction == Interaction::Pressed && prev_interaction != Interaction::Pressed {
            if let Some(ref mut s) = state {
                s.open = !s.open;
            }
        }
        current.insert(entity, *interaction);
    }
    *prev = current;
}

// Click handler for a single colony option inside the dropdown. When
// the player presses an option, update `ConstructionUiState::selected_colony`
// and close the menu. Same rising-edge detection as the other click
// handlers. `Option<ResMut<…>>` defends against a missing-resource
// startup race.
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
        if *interaction == Interaction::Pressed && prev_interaction != Interaction::Pressed {
            ui_state.selected_colony = Some(option.colony_entity);
            if let Some(ref mut s) = state {
                s.open = false;
            }
        }
        current.insert(entity, *interaction);
    }
    *prev = current;
}

// Toggle the colony dropdown menu visibility based on
// `ColonyDropdownState::open`. Same inheritance pattern as
// `tick_construction_body_visibility`: when open, use `Inherited`
// so the menu inherits visibility from the canary root; when
// closed, use `Hidden` to unconditionally hide it.
///
// `Visibility::Inherited` is required (instead of `Visibility::Visible`)
// because the parent `picker` sits inside `shared_chrome`, which
// is itself driven by the construction menu root's `Visibility`.
// v0.5.2: the chrome is now visible on every tab (not just Build),
// so this gate is purely about the parent menu's visibility — the
// dropdown inherits its parent menu's `Visibility::Hidden` when
// the player toggles the Construction menu off, which is the
// intended leak prevention.
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

// Update the picker's value text every frame based on the active
// selection. Mirrors the egui ComboBox's `selected_text` behavior:
// show the colony name (plus population suffix), or a placeholder
// when no colony is selected.
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

// Refresh the colony dropdown menu every frame: keep one
// `ColonyDropdownOption` row per live `Colony` entity, mutate text
// and selection-state in place, and only spawn / despawn when the
// set of colonies changes.
///
// v0.5.2 pilot for the construction canary's
// **spawn-once-update-many** refactor (see
// `update_overview_queue` / `update_buildings_body` /
// `update_mining_body` for the broader pattern). Rows persist
// across `ColonyDropdownState::open` toggles and across tab
// visibility changes — the `Local<HashMap<Entity, Entity>>` cache
// is keyed by `colony_entity` so the system can identify which
// rows to keep, which to mutate, and which to despawn.
///
// Why not the previous `Local<Vec<Entity>>` + per-frame despawn /
// respawn pattern? Two reasons:
// 1. The previous pattern triggered Bevy 0.18's
//    `WARN bevy_ecs::error::handler: Encountered an error in
//    command ... Entity despawned` flood whenever a parent
//    content container was cascade-despawned mid-tick, because
//    the `Local` cache still held the now-stale child IDs. We
//    suppressed the warning with `try_despawn` but that was
//    treating the symptom, not the cause.
// 2. Spawning ~5–10 row entities per frame is a measurable cost
//    on a canary panel that the player opens frequently.
///
// Mutation strategy per row:
// - `BackgroundColor` (row) — driven by `is_selected`
// - `Text` (option-text child) — the colony name + population
// - `TextColor` (option-text child) — driven by `is_selected`
pub fn refresh_colony_dropdown(
    mut commands: Commands,
    fonts: Res<UiFonts>,
    colonies: Query<(Entity, &crate::colony::Colony)>,
    menu_query: Query<Entity, With<ColonyDropdownMenu>>,
    ui_state: Res<ConstructionUiState>,
    mut spawned_rows: Local<
        std::collections::HashMap<bevy::ecs::entity::Entity, bevy::ecs::entity::Entity>,
    >,
    mut row_bg_query: Query<
        (&ColonyDropdownOption, &mut BackgroundColor),
        Without<ColonyDropdownOptionText>,
    >,
    mut text_query: Query<(&ChildOf, &mut Text, &mut TextColor), With<ColonyDropdownOptionText>>,
) {
    let Ok(menu) = menu_query.single() else {
        return;
    };

    let body_font_medium: Handle<Font> = fonts.medium.clone();

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
        let text_color = if is_selected {
            ACTIVE_CHIP_TEXT
        } else {
            TEXT_BODY
        };
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
        let text_color = if is_selected {
            ACTIVE_CHIP_TEXT
        } else {
            TEXT_BODY
        };
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

// Update the canary's hover tooltip text every frame.
///
// Scans every CTA's `Interaction` and `ConstructionCtaDisabled`
// state. If the player is hovering a disabled CTA, populates
// `ConstructionTooltipState` with the most-binding constraint:
// "Not enough energy" when the batch's power demand exceeds
// the active colony's grid surplus (v0.5.2 PR-A.2 round 2), or
// the largest **local-stockpile** shortfall otherwise.
///
// v0.5.2 (build menu fix): the resource-shortfall check now
// reads the selected colony's `LocalStockpile` (the same gate
// `process_construction_actions` enforces) instead of the
// system-wide `ContextualStockpile`. The old check could pass
// while the actual game logic was about to file a
// `ResourceRequest` because the resources lived on other
// bodies — making the tooltip say nothing on hover even though
// the click "did nothing" from the player's perspective. Now
// the tooltip and the red inline reason text both surface the
// real bottleneck.
///
// Pure read of `tick_construction_cta_disabled`'s output — no
// mutation of disabled state. Runs every frame so the tooltip
// disappears the instant the cursor leaves a disabled button.
pub fn tick_construction_tooltip(
    ctas: Query<(
        Entity,
        &Interaction,
        &ConstructionCta,
        Has<ConstructionCtaDisabled>,
        Has<ConstructionCtaBodyBlocked>,
    )>,
    buildings_data: Res<BuildingsData>,
    local_stockpiles: Query<&crate::economy::LocalStockpile>,
    colonies: Query<&crate::colony::Colony>,
    ui_state: Res<ConstructionUiState>,
    mut tooltip: ResMut<ConstructionTooltipState>,
    mut queue_tooltip: ResMut<QueueButtonTooltipState>,
) {
    let multiplier = ui_state.build_multiplier.max(1);

    // Pre-compute the active colony's grid surplus in MW so we can
    // detect power-disabled cards. Mirrors `compute_colony_spare_power_mw`.
    let active_colony_entity = ui_state.selected_colony;
    let spare_power_mw: f64 = active_colony_entity
        .and_then(|e| colonies.get(e).ok())
        .map(|colony| {
            let totals = calculate_colony_power_totals(colony, Some(&buildings_data));
            (totals.produced_watts - totals.consumed_watts) / 1_000_000.0
        })
        .unwrap_or(0.0);
    // Pre-compute the active colony's local stockpile (the real
    // construction gate). Falls back to `None` when no colony is
    // selected — in that case the resource shortfall path is
    // skipped and the tooltip stays empty.
    let local = active_colony_entity.and_then(|e| local_stockpiles.get(e).ok());

    let mut best: Option<String> = None;
    // v0.5.2 (build menu fix): body-blocked CTAs (mining
    // buildings on incompatible bodies) get their own
    // single-line tooltip so the player sees the actual reason
    // — "Body unavailable" — rather than a misleading
    // resource / power shortlist (which wouldn't be the real
    // gate). Body-blocked wins over the resource/power gate
    // because the body check is the more fundamental
    // constraint — the player can't build regardless of how
    // many resources they have on hand.
    let mut queue_tooltip_entity: Option<Entity> = None;
    let mut queue_tooltip_lines: Vec<String> = Vec::new();
    for (entity, interaction, cta, is_disabled, is_body_blocked) in ctas.iter() {
        if !matches!(interaction, Interaction::Hovered | Interaction::Pressed) {
            continue;
        }
        let def = match buildings_data.get(&cta.building_type) {
            Some(d) => d,
            None => continue,
        };
        if is_body_blocked {
            queue_tooltip_lines = vec![
                "Missing:".to_string(),
                "  Body unavailable".to_string(),
            ];
            queue_tooltip_entity = Some(entity);
            // Skip the resource/power checks — the body
            // gate is the dominant reason.
            continue;
        }
        if !is_disabled {
            continue;
        }

        // v0.5.2 polish (2026-08-04): surface every missing
        // prerequisite under a single `Missing:` header so the
        // player sees the full picture in one glance. Resource
        // shortfalls are listed first (most-binding first per
        // `collect_local_shortfalls`), then power if the batch
        // exceeds the grid surplus. The format per user spec:
        //
        //     Missing:
        //       Copper 17 Mt
        //       Power 200 MW
        //       Iron 5 Mt
        //
        // ("Name Amount" order with a 2-space indent for visual
        // grouping under the `Missing:` header.)
        let mut lines: Vec<String> = Vec::new();
        lines.push("Missing:".to_string());

        // v0.5.2 (build menu fix): local-stockpile shortfall
        // instead of the system-wide contextual aggregate. The
        // construction system deducts from the colony's own
        // stockpile, so this is the gate that matters. When no
        // colony is selected, fall through and leave the tooltip
        // empty (the player has bigger problems than the
        // tooltip).
        if let Some(ls) = local {
            let shortfalls = collect_local_shortfalls(
                ls,
                def.resource_costs.as_slice(),
                multiplier,
            );
            const MAX_LINES: usize = 4;
            let show = shortfalls.len().min(MAX_LINES);
            for (name, missing) in shortfalls.iter().take(show) {
                lines.push(format!(
                    "  {} {}",
                    name,
                    format_mining_reserve(*missing)
                ));
            }
            if shortfalls.len() > MAX_LINES {
                lines.push(format!("  +{} more", shortfalls.len() - MAX_LINES));
            }
        }

        // Power gate — list it only when the batch's total demand
        // exceeds the grid surplus. We deliberately use a positive
        // number here (the deficit) so the player reads the
        // magnitude they need to recover.
        if def.power_demand_mw > 0.0 && spare_power_mw > 0.0 {
            let total_demand = def.power_demand_mw * multiplier as f64;
            if total_demand > spare_power_mw {
                let deficit_mw = total_demand - spare_power_mw;
                lines.push(format!("  Power {}", format_power(deficit_mw)));
            }
        }

        // Only show the tooltip if we have *something* to say
        // beyond the header — a lone `Missing:` with no items
        // below would just be noise.
        if lines.len() > 1 {
            best = Some(lines.join("\n"));
            break;
        }
    }
    // Mirror the tooltip text into the legacy bottom-left
    // `ConstructionTooltipState` AND the cursor-following
    // `QueueButtonTooltipState`. Read `best` by reference so
    // we can use it twice (consumed for the legacy
    // `tooltip.text` write, split-on-newline for the
    // cursor-following one).
    let best_text: Option<String> = if queue_tooltip_entity.is_none() {
        best.clone()
    } else {
        None
    };
    if let Some(text) = best {
        tooltip.text = text;
        tooltip.visible = true;
    } else {
        tooltip.text.clear();
        tooltip.visible = false;
    }

    // v0.5.2 (build menu fix): mirror the per-frame payload into
    // the cursor-following `QueueButtonTooltipState` so
    // `update_queue_button_tooltip` can render a tooltip overlay
    // next to the cursor. Body-blocked wins over
    // resource-shortage when both are present (rare — a
    // body-blocked CTA normally doesn't have resource info
    // shown), because the body gate is the dominant reason.
    if let Some(entity) = queue_tooltip_entity {
        queue_tooltip.hovered_cta = Some(entity);
        queue_tooltip.lines = queue_tooltip_lines;
    } else if let Some(text) = best_text {
        // No body-blocked CTA — mirror the resource/power
        // shortlist. Split on `\n` so each line ends up in its
        // own Text layout row.
        queue_tooltip.hovered_cta = None; // update system tracks its own cursor-following CTA
        queue_tooltip.lines = text.split('\n').map(|s| s.to_string()).collect();
    } else {
        queue_tooltip.hovered_cta = None;
        queue_tooltip.lines.clear();
    }
}

// Mirror `ConstructionTooltipState` to the on-screen tooltip Text
// node + its visibility every frame. The text node is parented to
// the canary root with absolute positioning at the bottom-left, so
// it's always visible regardless of which sub-tab the player is on
// (the canary root is itself gated by the Construction menu state).
pub fn update_construction_tooltip(
    state: Res<ConstructionTooltipState>,
    mut tooltip_query: Query<(&mut Text, &mut Visibility), With<ConstructionTooltipText>>,
) {
    // v0.5.2 (2026-08-05 audit): early-out when the tooltip is not
    // visible AND the current text is empty — the old code wrote
    // `Text` + `Visibility` on the node every frame regardless,
    // mutating components with identical values (change-detection /
    // re-layout churn). When `!state.visible` we only need to write
    // once to hide; the `state` stays empty-until-next-hover (the
    // menu-open gate + `tick_construction_state` clearing on close
    // means a stale visible flag never lingers).
    if !state.visible {
        let mut already_hidden = true;
        for (t, v) in tooltip_query.iter() {
            if !t.0.is_empty() || *v != Visibility::Hidden {
                already_hidden = false;
                break;
            }
        }
        if already_hidden {
            return;
        }
        for (mut t, mut v) in tooltip_query.iter_mut() {
            **t = String::new();
            *v = Visibility::Hidden;
        }
        return;
    }
    let text = state.text.clone();
    for (mut t, mut v) in tooltip_query.iter_mut() {
        **t = text.clone();
        *v = Visibility::Inherited;
    }
}

// v0.5.2 (build menu fix): per-frame driver for the
// **cursor-following** Queue-CTA tooltip. Mirrors
// `update_resource_cost_tooltip`'s pattern but renders the
// multi-line `Missing:` payload from `QueueButtonTooltipState`
// instead of a single `<name>  <amount>` string. Same
// coordinate-frame translation (subtract `CANARY_ROOT_TOP_PX`
// from cursor Y), same stale-entity guard, same `Display::None`
// fallback when the construction menu isn't the active menu
// or no CTA is currently hovered.
//
// The overlay has its own marker
// (`QueueButtonTooltipOverlay`) and resource
// (`QueueButtonTooltipState`) so a player hovering the Queue
// button and a resource chip at the same time (e.g. a
// cursor crossing the card body's two zones) gets clean
// per-zone tooltips instead of one stomping the other.
pub fn update_queue_button_tooltip(
    active_menu: Res<ActiveMenu>,
    primary_window: Query<&Window, With<PrimaryWindow>>,
    cta_query: Query<Entity, With<ConstructionCta>>,
    queue_state: Res<QueueButtonTooltipState>,
    mut overlay_node: Single<&mut Node, With<QueueButtonTooltipOverlay>>,
    tooltip: Single<(&mut Text, &mut Visibility), With<QueueButtonTooltipText>>,
) {
    let (mut tooltip_text, mut tooltip_visibility) = tooltip.into_inner();

    // Same canary-root top offset as the resource-cost /
    // power-chip tooltips. All three overlays are parented to
    // the same construction root, so they share the
    // translation constant.
    const CANARY_ROOT_TOP_PX: f32 = 126.0;
    // The Missing: list runs up to ~5 lines (header + 4
    // resource lines, or header + 1 power line + 1
    // resource line, etc.). Reserve enough vertical room
    // for the worst case at CAPTION_SIZE.
    const TOOLTIP_W: f32 = 260.0;
    const TOOLTIP_H: f32 = 110.0;

    // Hide the overlay whenever the construction canary isn't
    // the active menu — same guard as the chip tooltips, so
    // a stuck overlay doesn't follow the cursor across menu
    // transitions.
    let construction_menu_active = matches!(active_menu.current, GameMenu::Construction);
    if !construction_menu_active {
        overlay_node.display = Display::None;
        return;
    }

    // Stale-entity guard: the CTA may have been despawned
    // between frames (multiplier chip change, colony switch,
    // sub-tab switch, queue-row despawn). Bevy 0.18's
    // pointer backend doesn't reliably fire `Pointer<Out>`
    // for entities that vanish between frames, so the
    // hovered_cta id could survive. Re-check it here.
    let cta_alive = queue_state
        .hovered_cta
        .map(|e| cta_query.get(e).is_ok())
        .unwrap_or(true); // hovered_cta is None → not stale
    if !queue_state.lines.is_empty() && !cta_alive {
        // The CTA we were tracking was despawned. The
        // tick system will refresh on the next frame;
        // for now hide the overlay.
        overlay_node.display = Display::None;
        return;
    }
    if queue_state.lines.is_empty() {
        overlay_node.display = Display::None;
        *tooltip_visibility = Visibility::Hidden;
        return;
    }

    // Need a window to position relative to.
    let Ok(window): Result<&Window, _> = primary_window.single() else {
        overlay_node.display = Display::None;
        return;
    };

    // Off-screen cursor: hide the overlay.
    if window.cursor_position().is_none() {
        overlay_node.display = Display::None;
        return;
    }
    let cursor = window.cursor_position().unwrap();

    // Translate window-space cursor → canary-root-local
    // pixel coords. Same X / Y treatment as the chip
    // tooltips.
    let local_x = cursor.x;
    let local_y = cursor.y - CANARY_ROOT_TOP_PX + 4.0;

    // Conservative right/bottom clamp: leave `TOOLTIP_W`
    // on the right and `TOOLTIP_H` on the bottom so the
    // tooltip never clips out of the canary root.
    let root_width = window.width().max(TOOLTIP_W);
    let root_height = (window.height() - CANARY_ROOT_TOP_PX - 72.0).max(TOOLTIP_H);
    let max_left = (root_width - TOOLTIP_W).max(0.0);
    let max_top = (root_height - TOOLTIP_H).max(0.0);
    overlay_node.left = Val::Px(local_x.clamp(0.0, max_left));
    overlay_node.top = Val::Px(local_y.clamp(0.0, max_top));
    overlay_node.display = Display::Flex;
    *tooltip_visibility = Visibility::Inherited;

    // Multi-line text. Bevy's `Text` component supports
    // newline-separated content — each line is laid out
    // automatically.
    *tooltip_text = Text::new(queue_state.lines.join("\n"));
}

// Click handler: when a chip in the Build sub-tab is pressed, mutate
// `ConstructionUiState` accordingly. The chip's `ChipKind` component
// tells us what to do (set qty, set filter, set category, etc.).
///
// This system is the "wiring" that connects the visual chips to the
// real game state. Without it, the chips light up on hover but don't
// actually do anything.
///
// Note: bevy's `Interaction::Pressed` only stays `true` for one or two
// frames while the user holds the button. We track each chip's
// previous interaction in a `Local<HashMap>` to detect the rising
// edge (None/Hovered → Pressed) so a fast click isn't missed.
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
        if *interaction == Interaction::Pressed && prev_interaction != Interaction::Pressed {
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
            }
        }
        current.insert(entity, *interaction);
    }
    *prev = current;
}

// What a chip in the Build sub-tab does when pressed. Attached to
// each `ChipButtonBundle` so the click handler can dispatch.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChipKind {
    // Sub-tab at the given index (0=Overview, 1=Buildings, 2=Build,
    // 3=Mining).
    Tab(usize),
    // Build quantity multiplier (1, 5, 10, 25, 50, 100) on the
    // Build tab.
    Qty(u32),
    // Functional-role filter (All / Food / Power / etc.).
    Filter(BuildFilter),
    // Build-category tab (0=Infrastructure, 1=Industry, 2=Logistics,
    // ..., 7=Military, 8=All). v0.5.2 PR-A.2 (round 2): the
    // Mining chip is removed from the Build tab (mines are
    // managed in the dedicated Mining tab), so 9 → 8 chips.
    Category(usize),
}

// Click handler: when the player presses the Queue button on a build
// card, push `(selected_colony, building_type)` to
// `PendingConstructionActions::start_construction`. The
// `process_construction_actions` system in the colony module then
// spawns a `ConstructionProject` for that building.
///
// Same rising-edge detection as `tick_construction_chip_click`.
pub fn tick_construction_cta_click(
    interactions: Query<(Entity, &Interaction, &ConstructionCta), With<ConstructionCta>>,
    // v0.5.2 (build menu fix): the click guard now skips both
    // resource-driven (`ConstructionCtaDisabled`) and
    // body-blocked (`ConstructionCtaBodyBlocked`) CTAs. Without
    // the body-blocked entry, a player could click a body-blocked
    // mining CTA (e.g. an Iron Mine on a gas giant) and the
    // click would be silently enqueued — the construction
    // system would then have to surface the gate error
    // downstream. Blocking the click here matches the visible
    // disabled state and the hover affordance.
    disabled: Query<
        Entity,
        Or<(With<ConstructionCtaDisabled>, With<ConstructionCtaBodyBlocked>)>,
    >,
    ui_state: Res<ConstructionUiState>,
    mut pending: ResMut<PendingConstructionActions>,
    mut prev: Local<std::collections::HashMap<Entity, Interaction>>,
) {
    let mut current: std::collections::HashMap<Entity, Interaction> =
        std::collections::HashMap::new();
    for (entity, interaction, cta) in interactions.iter() {
        let prev_interaction = prev.get(&entity).copied().unwrap_or(Interaction::None);
        if *interaction == Interaction::Pressed && prev_interaction != Interaction::Pressed {
            let Some(colony_entity) = ui_state.selected_colony else {
                current.insert(entity, *interaction);
                continue;
            };
            // Skip the push when the CTA is in the disabled state
            // — either resource-driven (`ConstructionCtaDisabled`)
            // or body-blocked (`ConstructionCtaBodyBlocked`).
            // The hover system also bails on both markers so the
            // button doesn't brighten.
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
                pending
                    .start_construction
                    .push((colony_entity, cta.building_type));
            }
        }
        current.insert(entity, *interaction);
    }
    *prev = current;
}

// ── Queue Panel systems ──────────────────────────────────────────────

// Click handler for the AppBar "OPEN QUEUE" chip. Toggles
// `QueuePanelState::open`. Same rising-edge detection as the chip /
// CTA click handlers. `ResMut<QueuePanelState>` is wrapped in
// `Option<…>` so a missing-resource startup race (e.g. if the plugin
// is added before `init_resource` runs) doesn't panic the canary.
pub fn tick_open_queue_chip_click(
    interactions: Query<(Entity, &Interaction), (With<OpenQueueChip>, With<Button>)>,
    mut state: Option<ResMut<QueuePanelState>>,
    mut prev: Local<std::collections::HashMap<Entity, Interaction>>,
) {
    let mut current: std::collections::HashMap<Entity, Interaction> =
        std::collections::HashMap::new();
    for (entity, interaction) in interactions.iter() {
        let prev_interaction = prev.get(&entity).copied().unwrap_or(Interaction::None);
        if *interaction == Interaction::Pressed && prev_interaction != Interaction::Pressed {
            if let Some(ref mut s) = state {
                s.open = !s.open;
            }
        }
        current.insert(entity, *interaction);
    }
    *prev = current;
}

// Click handler for the QueuePanel close button. Sets
// `QueuePanelState::open = false`. Defensive `Option<ResMut<…>>` (see
// `tick_open_queue_chip_click` for rationale).
pub fn tick_queue_panel_close_click(
    interactions: Query<(Entity, &Interaction), (With<QueuePanelClose>, With<Button>)>,
    mut state: Option<ResMut<QueuePanelState>>,
    mut prev: Local<std::collections::HashMap<Entity, Interaction>>,
) {
    let mut current: std::collections::HashMap<Entity, Interaction> =
        std::collections::HashMap::new();
    for (entity, interaction) in interactions.iter() {
        let prev_interaction = prev.get(&entity).copied().unwrap_or(Interaction::None);
        if *interaction == Interaction::Pressed && prev_interaction != Interaction::Pressed {
            if let Some(ref mut s) = state {
                s.open = false;
            }
        }
        current.insert(entity, *interaction);
    }
    *prev = current;
}

// Toggle the QueuePanel visibility based on `QueuePanelState::open`.
// Defensive `Option<Res<…>>` (see `tick_open_queue_chip_click`).
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

// Update the live AppBar queue summary text (`queue_value`) every frame
// based on the selected colony's queued `ConstructionProject`s. The
// legacy placeholder was a static "6d 2h" string; this version reads
// real progress and ETA.
///
// We compute the remaining time for each project as
// `remaining_bp / (ConstructionProject::required / batch_seconds)`,
// roughly equivalent to "how long until this row finishes". Summed
// across the colony that's the queue ETA.
pub fn update_queue_summary(
    mut text_query: Query<&mut Text, With<QueuePanelSummaryText>>,
    ui_state: Res<ConstructionUiState>,
    projects: Query<&crate::colony::ConstructionProject>,
    output_bp_per_year: Res<ConstructionQueue>,
) {
    // Output rate (BP/yr) — the legacy placeholder used 12 001. Read
    // from the `ConstructionQueue` resource so the canary stays
    // self-contained.
    let bp_per_sec = (output_bp_per_year.output_bp_per_year / 365.25 / 24.0 / 3600.0).max(1e-9);
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

// Diff-based queue row management. Each frame:
// 1. Query `ConstructionProject` filtered by the selected colony.
// 2. Compute the desired set of `project_entity` IDs.
// 3. For each project entity not in the existing row map, spawn a new
//    `QueuePanelRow` with progress bar + ETA + cancel button.
// 4. Despawn any row whose project entity is no longer in the desired set.
// 5. Update progress / ETA on existing rows.
///
// Run condition: every frame (the project set can change whenever the
// player queues or cancels). The diff cost is O(projects + rows) which
// is < 50 in the canary's scope.
pub fn update_queue_panel(
    mut commands: Commands,
    fonts: Res<UiFonts>,
    buildings_data: Res<BuildingsData>,
    ui_state: Res<ConstructionUiState>,
    output_bp_per_year: Res<ConstructionQueue>,
    projects: Query<(Entity, &crate::colony::ConstructionProject)>,
    existing_rows: Query<(Entity, &QueuePanelRow)>,
    root_query: Query<Entity, With<QueuePanelRoot>>,
    body_query: Query<Entity, With<QueuePanelBody>>,
) {
    let Ok(panel_root) = root_query.single() else {
        return;
    };
    let _ = panel_root; // not needed directly; rows are added to body_root
    let Ok(body_root) = body_query.single() else {
        return;
    };

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
    let mut existing: std::collections::HashMap<Entity, Entity> = std::collections::HashMap::new();
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
            &fonts,
            &output_bp_per_year,
        );
        let _ = row;
    }
}

// Update the ETA text on every existing queue row every frame. The
// text is derived from the project's `progress` and `required` plus
// the configured output rate. Without this per-frame update the
// spawned text would be frozen at the spawn frame — the queue would
// appear to never count down. This is the system that makes the
// queue duration "live".
//
// v0.5.2 (2026-08-06): the old code did `projects.iter().find(...)`
// per row → O(rows × projects). Build a `HashMap<Entity, _>` once at
// the top so each row is a single `get` — O(rows + projects).
pub fn update_queue_row_eta(
    projects: Query<(Entity, &crate::colony::ConstructionProject)>,
    output_bp_per_year: Res<ConstructionQueue>,
    mut eta_text_query: Query<(&QueuePanelRowEta, &mut Text, &mut TextColor)>,
) {
    let by_entity: std::collections::HashMap<Entity, &crate::colony::ConstructionProject> =
        projects.iter().map(|(e, p)| (e, p)).collect();
    let bp_per_sec = (output_bp_per_year.output_bp_per_year / 365.25 / 24.0 / 3600.0).max(1e-9);
    for (eta_marker, mut text, mut color) in eta_text_query.iter_mut() {
        let Some(project) = by_entity.get(&eta_marker.project_entity) else {
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

// Update the progress-bar fill width on every existing queue row
// every frame so the bar tracks the project's `progress` toward
// completion. Without this the bar would be frozen at the spawn
// frame.
//
// v0.5.2 (2026-08-06): same O(rows + projects) HashMap fix as
// `update_queue_row_eta` (was O(rows × projects) `find` per row).
pub fn update_queue_row_progress(
    projects: Query<(Entity, &crate::colony::ConstructionProject)>,
    mut fill_query: Query<(&QueuePanelRowFill, &mut Node)>,
) {
    let by_entity: std::collections::HashMap<Entity, &crate::colony::ConstructionProject> =
        projects.iter().map(|(e, p)| (e, p)).collect();
    for (fill_marker, mut node) in fill_query.iter_mut() {
        let Some(project) = by_entity.get(&fill_marker.project_entity) else {
            continue;
        };
        node.width = Val::Percent((project.progress_percent() as f32).clamp(0.0, 1.0) * 100.0);
    }
}

// Spawn a single row in the queue panel for a given
// `ConstructionProject`. The row has:
// - Header line: building name + ETA
// - Progress bar: a 320 px wide track with a colored fill node whose
//   width is set to `progress_percent * 320` every frame
// - Status: "⏳ Waiting for delivery" if `awaiting_resources`, else
//   the time-remaining label
// - Cancel X button (right-aligned)
fn spawn_queue_row(
    commands: &mut Commands,
    parent: Entity,
    project_entity: Entity,
    display_name: &str,
    project: &crate::colony::ConstructionProject,
    fonts: &UiFonts,
    output_bp_per_year: &Res<ConstructionQueue>,
) -> Entity {
    let body_font: Handle<Font> = fonts.body.clone();
    let body_font_medium: Handle<Font> = fonts.medium.clone();
    let mono_font: Handle<Font> = fonts.mono.clone();
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
    let bp_per_sec = (output_bp_per_year.output_bp_per_year / 365.25 / 24.0 / 3600.0).max(1e-9);
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

// Marker component for the ETA text on a queue row. The
// `update_queue_row_eta` system uses this to find the text node by
// project entity without iterating every `Text` node in the world.
#[derive(Component)]
pub struct QueuePanelRowEta {
    pub project_entity: Entity,
}

// Marker component for the progress-fill bar on a queue row. The
// `update_queue_row_progress` system uses this to update the bar
// width every frame as the project advances.
#[derive(Component)]
pub struct QueuePanelRowFill {
    pub project_entity: Entity,
}

// Marker component for the `queue_value` text in the AppBar so the
// `update_queue_summary` system can find it without iterating every
// Text node.
#[derive(Component)]
pub struct QueuePanelSummaryText;

// Click handler for the cancel button on each queue row. Pushes the
// project entity to `PendingConstructionActions::cancel_construction`.
pub fn tick_queue_panel_row_cancel_click(
    interactions: Query<(Entity, &Interaction, &QueuePanelRowCancel), With<QueuePanelRowCancel>>,
    mut pending: ResMut<PendingConstructionActions>,
    mut prev: Local<std::collections::HashMap<Entity, Interaction>>,
) {
    let mut current: std::collections::HashMap<Entity, Interaction> =
        std::collections::HashMap::new();
    for (entity, interaction, cancel) in interactions.iter() {
        let prev_interaction = prev.get(&entity).copied().unwrap_or(Interaction::None);
        if *interaction == Interaction::Pressed && prev_interaction != Interaction::Pressed {
            pending.cancel_construction.push(cancel.project_entity);
        }
        current.insert(entity, *interaction);
    }
    *prev = current;
}

// Marker component for the QueuePanel body container (the scrollable
// column where rows are spawned). The `update_queue_panel` system
// queries for this to find the parent for new rows.
#[derive(Component)]
pub struct QueuePanelBody;

// Visibility system: shows / hides the canary root based on
// `ActiveMenu.current == GameMenu::Construction`. The canary spawns at
// startup (so the entities exist) and this system keeps visibility in
// sync each frame.
//
// v0.5.2 (2026-08-05 audit): on the Off transition we also clear the
// transient UI states (tooltip text, demolish dialog, dropdown, queue
// panel). This lets the heavy per-frame systems below be gated on
// `construction_menu_open` — they skip entirely while the menu is
// closed — without leaving a stale tooltip/dialog visible for a
// frame when the menu reopens.
pub fn tick_construction_state(
    active_menu: Res<ActiveMenu>,
    mut state: ResMut<ConstructionState>,
    mut root_query: Query<&mut Visibility, With<ConstructionRoot>>,
    mut tooltip_state: ResMut<ConstructionTooltipState>,
    mut queue_tooltip: ResMut<QueueButtonTooltipState>,
    mut demolish_state: ResMut<DemolishConfirmState>,
    mut dropdown_state: ResMut<ColonyDropdownState>,
    mut queue_panel_state: ResMut<QueuePanelState>,
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
        // Clear transient UI state so a stale tooltip / dialog /
        // dropdown never reappears when the menu reopens.
        *tooltip_state = ConstructionTooltipState::default();
        *queue_tooltip = QueueButtonTooltipState::default();
        *demolish_state = DemolishConfirmState::default();
        dropdown_state.open = false;
        queue_panel_state.open = false;
    }
}

/// `run_if` predicate: the Construction canary is currently visible.
/// v0.5.2 (2026-08-05 audit): ~40 of the 49 per-frame systems only
/// mutate entities inside the hidden canary root, yet ran every frame
/// regardless — the "huge compute for a simple menu" sink. Gating them
/// on this predicate (plus `tick_construction_state` clearing transient
/// state on close) skips them entirely while the menu is closed.
fn construction_menu_open(state: Res<ConstructionState>) -> bool {
    *state == ConstructionState::On
}

// Plugin: registers the Construction canary on `bevy_ui`.
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
            // v0.5.2 PR-A.7: Demolish confirmation modal state.
            // `tick_demolish_click` opens the dialog; the Yes / No
            // / tab-switch / colony-change systems close it. Read by
            // `update_demolish_dialog_text` and
            // `tick_demolish_dialog_visibility` every frame.
            .init_resource::<DemolishConfirmState>()
            // Tooltip state (text + visibility) — required by
            // `tick_construction_tooltip` and
            // `update_construction_tooltip`. The tooltip pops up
            // when the player hovers a disabled Queue CTA.
            .init_resource::<ConstructionTooltipState>()
            // v0.5.2 (build menu fix): cursor-following Queue-CTA
            // tooltip state. Written by `tick_construction_tooltip`
            // (which scans every CTA's `Interaction` /
            // `Has<ConstructionCtaDisabled>` /
            // `Has<ConstructionCtaBodyBlocked>` state), read by
            // `update_queue_button_tooltip` to position the
            // singleton overlay at the cursor and populate the
            // multi-line `Missing:` text.
            .init_resource::<QueueButtonTooltipState>()
            // Cost-chip hover state: written by the
            // `on_chip_hover_over` / `on_chip_hover_out` observers
            // attached to each `ResourceCostChip` entity, read by
            // `update_resource_cost_tooltip` to position and
            // populate the singleton overlay.
            .init_resource::<ResourceCostHoverState>()
            // v0.5.2 PR-A.7 (2026-08-04): power-chip hover state.
            // Written by the `on_power_chip_hover_over` /
            // `on_power_chip_hover_out` observers attached to each
            // `PowerChip` entity, read by `update_power_chip_tooltip`
            // to position and populate the singleton overlay.
            .init_resource::<PowerChipHoverState>()
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
                    // v0.5.2 (2026-08-06): `setup_construction` MUST
                    // run after `load_building_icons` — both were
                    // `.after(load_buildings)` with no mutual order,
                    // so Bevy could run setup FIRST, read
                    // `Option<Res<BuildingIcons>>` as None, and spawn
                    // every initial Build card with the cyan
                    // placeholder square (the "filled cyan square,
                    // no icon" report). Ordering setup after the icon
                    // load guarantees the handles exist at spawn.
                    setup_construction
                        .after(crate::colony::data::load_buildings)
                        .after(load_building_icons),
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
            // v0.5.2 (audit): gated on menu-open — the overlay is
            // parented to the hidden ConstructionRoot when closed.
            .add_systems(Update, update_resource_cost_tooltip.run_if(construction_menu_open))
            // v0.5.2 PR-A.7 (2026-08-04): power-chip tooltip
            // driver. Mirrors `update_resource_cost_tooltip` but
            // reads `PowerChipHoverState` and writes the
            // `PowerChipTooltipOverlay` text.
            .add_systems(Update, update_power_chip_tooltip.run_if(construction_menu_open))
            .add_systems(Update, tick_construction_cta_hover.run_if(construction_menu_open))
            // Marquee: oscillate subtitle `UiTransform.translation.x`
            // when the description overflows horizontally. Reads
            // `ComputedNode` (populated by the engine's layout pass)
            // and the card's `Interaction`, so must run on `Update`
            // like every other UI tick system (the egui-pass
            // restriction only applies to systems that call egui
            // context APIs, which this doesn't).
            .add_systems(Update, tick_subtitle_marquee.run_if(construction_menu_open))
            // Always-visible scrollbar overlay: resizes / repositions
            // the thumb of the card-grid scrollbar track based on
            // the grid's `ScrollPosition` + content size. Bevy 0.18
            // has no always-on scrollbar option in `bevy_ui` core,
            // so this drives our custom overlay.
            .add_systems(Update, tick_construction_scrollbar.run_if(construction_menu_open))
            // Drag-to-scroll: while the thumb has Interaction::Pressed,
            // translate pointer Y deltas into ScrollPosition changes.
            // Runs in the same Update schedule as the visual tick; the
            // visual tick publishes layout numbers to
            // `ConstructionScrollbarMetrics` which this system reads.
            .add_systems(Update, tick_construction_scrollbar_drag.run_if(construction_menu_open))
            // Scroll wheel → `ScrollPosition` for any `Overflow::scroll_y`
            // container under the cursor. Bevy 0.18 has no built-in
            // wheel handler (only renders scrollbars + clamps the
            // position); without this the card_grid and queue panel
            // body silently ignore the wheel even when content
            // overflows.
            .add_systems(Update, tick_ui_scroll_on_wheel.run_if(construction_menu_open))
            // Click handler: pushes (colony, building) to
            // PendingConstructionActions when a Queue button is pressed.
            .add_systems(Update, tick_construction_cta_click.run_if(construction_menu_open))
            // Chip-button click handler: when a qty / filter / category /
            // tab chip is pressed, mutate `ConstructionUiState`
            // accordingly. Without this, the chips are visual-only.
            .add_systems(Update, tick_construction_chip_click.run_if(construction_menu_open))
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
                refresh_card_grid
                    .run_if(resource_changed::<ConstructionUiState>)
                    .run_if(construction_menu_open),
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
                    .after(auto_select_first_colony)
                    .run_if(construction_menu_open),
            )
            // Toggle sub-tab body visibility based on the active tab.
            // Runs every frame; the cost is one Visibility mutation per
            // body (4 bodies) and an early-return when the tab hasn't
            // changed, which is a no-op.
            .add_systems(Update, tick_construction_body_visibility.run_if(construction_menu_open))
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
                    // with the current building count (which can
                    // change via the Queue button, this very
                    // Demolish button, or any other system).
                    //
                    // v0.5.2 (Buildings tab as a card list): the
                    // Demolish button is now used by both the
                    // Mining tab and the Buildings tab, so these
                    // systems are no longer mining-specific.
                    tick_demolish_click,
                    tick_demolish_disabled,
                    // v0.5.2 (2026-08-06): keep the Demolish button
                    // label in sync with the live build_multiplier
                    // (Buildings + Mining cards are spawn-once).
                    update_demolish_button_labels,
                    // v0.5.2: hover affordance for the Demolish
                    // button. Mirrors `tick_construction_cta_hover`
                    // — the Mining tab's [+] and [−] now light up
                    // red on hover, addressing the "no hover" player
                    // feedback on the Mining tab buttons.
                    tick_demolish_hover,
                    // v0.5.2 PR-A.7: Demolish confirmation modal.
                    // `tick_demolish_click` (above) opens the dialog
                    // rather than pushing the edit directly. The
                    // systems below drive the dialog's per-frame
                    // visibility, text, and Yes / No / tab-switch /
                    // colony-change close paths.
                    tick_demolish_dialog_visibility,
                    update_demolish_dialog_text,
                    tick_demolish_confirm_yes_click,
                    tick_demolish_confirm_no_click,
                    tick_demolish_dialog_close_on_tab_switch,
                    tick_demolish_dialog_close_on_colony_change,
                )
                    .run_if(construction_menu_open),
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
                )
                    .run_if(construction_menu_open),
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
                )
                    .run_if(construction_menu_open),
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
                    tick_construction_tooltip
                        .after(tick_construction_cta_disabled)
                        .run_if(construction_menu_open),
                    update_construction_tooltip.run_if(construction_menu_open),
                ),
            )
            // v0.5.2 (build menu fix): cursor-following
            // Queue-CTA tooltip driver. Mirrors the
            // resource-cost chip tooltip system but for
            // the `Missing:` multi-line payload. Runs every
            // frame even when no CTA is hovered so it can
            // hide the overlay and clear stale state. The
            // `tick_construction_tooltip` system above
            // populates the `QueueButtonTooltipState`
            // resource; this system renders it.
            .add_systems(Update, update_queue_button_tooltip.run_if(construction_menu_open))
            // v0.5.2 (build menu fix): per-frame label dim for
            // disabled Queue CTAs. The hover system
            // (`tick_construction_cta_hover`) owns the
            // `BackgroundColor` / `BorderColor` / `UiTransform`;
            // this system owns `TextColor` — Bevy 0.18 forbids
            // two `&mut TextColor` borrows in the same system,
            // so the dim pass is split out and ordered after the
            // hover system.
            .add_systems(
                Update,
                tick_construction_cta_label_dim
                    .after(tick_construction_cta_hover)
                    .run_if(construction_menu_open),
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
                    .chain()
                    .run_if(construction_menu_open),
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

// Spawn one surface group section (header + collapsible body of
// cards). Returns the outer container entity.
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
    // Active colony's grid surplus in MW (f64::NAN when no
    // colony is selected). Threaded through from
    // `update_mining_body` so each surface mine card's power
    // chip tooltip can show the real "Spare grid" value
    // instead of the old "No active grid" placeholder.
    spare_power_mw: f64,
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
            let icon_handle: Option<&Handle<Image>> = building_icons.handles.get(bt);
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
                spare_power_mw,
            );
        }
    }

    group_container
}

// Spawn the orbital section (collapsible, 5 non-collapsible
// sub-groups). Returns the outer container entity.
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
    // Active colony's grid surplus in MW (f64::NAN when no
    // colony is selected). See `spawn_mining_group_section`
    // for the same rationale.
    spare_power_mw: f64,
) -> Entity {
    let total_orbital: usize = MINING_GROUPS_ORBITAL.iter().map(|(_, b)| b.len()).sum();

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
                let icon_handle: Option<&Handle<Image>> = building_icons.handles.get(bt);
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
                    spare_power_mw,
                );
            }
        }
    }

    container
}

// Build a `BuildCardData` for a mine / AutoMine. Mirrors
// `card_data_with_multiplier` but uses the mine's
// count/accessibility/production/reserve as the primary data,
// and treats the body's body-gate as a separate disable reason
// (the resulting Queue button is disabled with the same
// `ConstructionCtaDisabled` marker the Build tab uses for
// affordability; the tooltip system distinguishes via the
// new `body_blocked` field on the card data).
///
// v0.5.2 PR-A.2 round 2: reuses the Build card's `CardBundle`
// styling so the Mining tab and Build tab have visually identical
// card chrome. The body content is mine-specific (count,
// accessibility, production, reserve) but the wrapper is the same.
///
// The `multiplier` is the player's build-qty choice. Mines are
// added to the construction queue (not direct inventory edits),
// so the multiplier scales the costs the player pays at queue
// time but the visible `count` on the card is the live inventory
// count (no multiplier fold — the multiplier is for the next
// build batch, not the existing inventory).
#[allow(clippy::too_many_arguments)]
pub fn build_mine_card_data(
    bt: BuildingType,
    def: &BuildingDefinition,
    count: u32,
    body_breathable: bool,
    body_type: Option<BodyType>,
    planet_resources: Option<&crate::economy::PlanetResources>,
    multiplier: u32,
    // Active colony's grid surplus in MW (produced − consumed).
    // `f64::NAN` when no colony is selected — the mining card
    // then omits the "Spare grid" line from the tooltip and
    // falls back to "no active colony" instead of faking a
    // value. v0.5.2 PR-A.7 (2026-08-04): the mining card used
    // to always show "No active grid" regardless of state; this
    // argument lets it show the real surplus when one exists.
    spare_power_mw: f64,
) -> BuildCardData {
    let mult = multiplier.max(1) as f64;
    let card_data = compute_mining_card_data(def, planet_resources);
    let body_blocked =
        !crate::colony::data::building_is_available_on(def, Some(body_breathable), body_type);

    // Stats row: count + current rate (left) + accessibility (right).
    //
    // v0.5.2 (2026-08-03, restored): per user feedback, the
    // count (`×N`) indicator should sit alongside the
    // **current** mining rate rather than carrying it
    // implicitly. The rate is the live amount this mine cluster
    // is producing RIGHT NOW (count × per-mine yield × body
    // accessibility), distinct from the batch-total rate shown
    // in the "Produces …" effect (which folds the queued
    // multiplier). Format with `format_mining_rate` so the
    // suffix tracks the same g/kg/t/kt/Mt/Gt/Tt/Pt/Et/Zt
    // ladder as the Produced line.
    //
    // The vertical bar (\u{2502}) separates the two numbers so a
    // quick glance answers "how many built, how much output".
    // When the body has no accessibility (yet-surveyed deposit,
    // or no deposit on this body), the rate segment is omitted
    // — there's nothing productive to advertise.
    //
    // `format_mining_rate` already trails its band in `/yr`
    // (e.g. `2700 kt/yr`), so the template does NOT append
    // another `/yr` — that produced `×25 │ 2700 kt/yr/yr`
    // before the dedup pass.
    let current_rate = card_data.production_mt_per_year(count);
    let count_label = if current_rate > 0.0 {
        format!(
            "\u{00d7}{} \u{2502} {}",
            count,
            format_mining_rate(current_rate)
        )
    } else {
        format!("\u{00d7}{}", count)
    };
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
    // Power chip (v0.5.2, 2026-08-06): shared builder extracted from
    // the three card-data builders. The Mining tab maps its spare
    // sentinel (`f64::NAN` = no colony) onto `Option`: `NAN → None`
    // (tooltip reads "No active colony"), a real surplus (including
    // a deficit) → `Some(s)` (tooltip reads "Spare grid" / "No
    // spare grid (deficit)").
    let power_chip = build_power_chip_data(
        def,
        mult,
        if spare_power_mw.is_nan() {
            None
        } else {
            Some(spare_power_mw)
        },
    );
    // Production: the per-mine yield is ALREADY shown in the stat_a
    // row (`×N │ 2700 kt/yr` — count + current rate). v0.5.2
    // (2026-08-06): the old code ALSO pushed a green "Produces
    // X × N = Y Mt/yr" effect, so every mining card showed BOTH the
    // power chip AND a duplicate green production line — the
    // "power demand/production chip AND the text in green" the
    // player reported. The stat row is the single production
    // readout on mining cards; the effect is dropped.
    // Reserve: "Available Deposits: 142 Gt" or "no deposit" /
    // "Survey the body...". v0.5.2 (2026-08-03): the verbose
    // "Available Deposits:" label matches the Build tab so the
    // two read as one UI surface. `format_mining_reserve` now
    // picks the smallest SI suffix (g/kg/t/kt/Mt/Gt/Tt/Pt/Et/Zt)
    // so the value lands in the 1..=999 range.
    let reserve_label = if card_data.reserve_mt > 0.0 {
        format!(
            "Available Deposits: {}",
            format_mining_reserve(card_data.reserve_mt)
        )
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
    // spare, disable the Queue. v0.5.2 (2026-08-06, bugfix round 2):
    // `power_chip` now carries the REAL spare (threaded through from
    // `update_mining_body`) so `power_chip.insufficient` is the true
    // demand-vs-spare gate. Body-blocked is an independent permanent
    // gate. Either one disables the CTA.
    let power_insufficient = body_blocked || power_chip.insufficient;

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
        // two sets in separate visual zones (Power chip →
        // Produces effect → Res → [resource_cost rows] →
        // ⚠ gate).
        resource_costs,
        // v0.5.2 PR-A.7 (2026-08-04): the inline `Power: … MW`
        // text effect is gone; the canary renders this chip
        // with the bolt-in-hex icon and a hover tooltip instead.
        // The Mining tab doesn't thread the active colony's
        // grid surplus (every-frame refresh path) so
        // `power_chip.spare_mw` stays `None`; the chip still
        // reads as Demand/Produces/0 W so the player sees the
        // power interaction of every mine.
        power_chip,
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
        // v0.5.2 (build menu fix): a Mining card is body-blocked
        // when `building_is_available_on(...)` returned false
        // (e.g. an Iron Mine on a gas giant). The Queue button
        // gets a permanent `ConstructionCtaBodyBlocked` marker
        // so the per-frame affordability system doesn't
        // silently re-enable it when the player happens to
        // have the resources on hand.
        body_blocked,
        // v0.5.2 (2026-08-06): Mining cards are NOT constructed —
        // they show the buildable mine catalog with a Queue CTA.
        constructed: false,
    }
}

// Spawn a single mine / AutoMine card. Reuses the Build card's
// `CardBundle` style for visual parity with the Build tab. The
// click handler is the same `tick_construction_cta_click` the
// Build tab uses, so the Queue button pushes
// `(colony, building_type)` to
// `PendingConstructionActions::start_construction` and the player
// gets a real queue entry with costs / ETA.
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
    // Active colony's grid surplus in MW (produced − consumed).
    // `f64::NAN` when no colony is selected — the mining card
    // then falls back to "No active colony" in the tooltip.
    // v0.5.2 PR-A.7 (2026-08-04): was previously unavailable
    // because the mining tab's every-frame refresh path
    // didn't thread it through.
    spare_power_mw: f64,
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
                    MiningCard { building_type: bt },
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
        spare_power_mw,
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
        DemolishMultiplierSource::Mining,
    );

    card
}
// ── Mining tab systems (v0.5.2 PR-A.2) ──────────────────────────

// Spawn a Demolish button on a card. Pinned to the
// bottom-right of the card (opposite of the Queue button) via
// absolute positioning — the card's `Overflow::clip` keeps the
// button from bleeding past the border. The button reads
// "Demolish -N" (or just "Demolish -1" when multiplier == 1) so
// the player sees the batch size at a glance. Red border + dim
// red fill makes the destructive action visually distinct from
// the Queue button without screaming.
///
// `count == 0` → spawn the button with `DemolishDisabled`
// attached; the click handler skips pushes when the marker is
// present, and a per-frame system removes the marker once the
// count rises.
///
// `multiplier_source` tells the click handler which chip-row
// value to apply (Mining tab uses `mining_build_multiplier`,
// Buildings tab uses `build_multiplier`).
fn spawn_demolish_button(
    commands: &mut Commands,
    card: Entity,
    bt: BuildingType,
    count: u32,
    multiplier: u32,
    body_font_medium: &Handle<Font>,
    multiplier_source: DemolishMultiplierSource,
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
            DemolishButton {
                building_type: bt,
                multiplier_source,
            },
            Pickable::default(),
        ))
        .id();
    // Spawn-time disabled state — when no buildings are present the
    // button is dim and the click handler is a no-op. Removed by
    // `tick_demolish_disabled` once the count rises.
    if count == 0 {
        commands.entity(demolish).insert(DemolishDisabled);
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
            // v0.5.2 (2026-08-06): per-frame label updates from the
            // live `build_multiplier` (Buildings + Mining tabs).
            DemolishButtonLabel,
        ))
        .id();
    commands.entity(demolish).add_child(label_entity);
}

// Click handler for the Demolish button. Pushes
// `(colony, bt, -multiplier)` to `PendingConstructionActions::mining_edits`
// so `process_construction_actions` removes up to `multiplier`
// buildings on the next tick. Which `multiplier` to use is read
// from `DemolishButton::multiplier_source`: Mining tab cards use
// `mining_build_multiplier` (independent of the Build tab's
// `build_multiplier`); Buildings tab cards use `build_multiplier`
// so the player's x1/x5/x25 chip choice carries straight
// through. Skips when `DemolishDisabled` is attached (i.e.
// count == 0). Rising-edge detection identical to the other
// chip/CTA click handlers.
///
// Originally `tick_mining_demolish_click` (Mining-tab only);
// v0.5.2 (Buildings tab as a card list) generalises the same
// handler to both tabs.
pub fn tick_demolish_click(
    mut params: ParamSet<(
        Query<(Entity, &Interaction, &DemolishButton), With<Button>>,
        Query<Entity, With<DemolishDisabled>>,
    )>,
    ui_state: Res<ConstructionUiState>,
    mut confirm_state: ResMut<DemolishConfirmState>,
    mut prev: Local<std::collections::HashMap<Entity, Interaction>>,
) {
    // v0.5.2 PR-A.7: instead of pushing the `mining_edits` entry
    // directly, this system now opens the centered confirmation
    // dialog (`DemolishConfirmDialog`). The actual edit is applied
    // by `tick_demolish_confirm_yes_click` when the player clicks
    // Yes. Picking No (or switching tabs / colonies while the
    // dialog is open) closes the dialog without applying the edit
    // — see `tick_demolish_confirm_no_click`,
    // `tick_demolish_dialog_close_on_tab_switch`, and
    // `tick_demolish_dialog_close_on_colony_change`.
    //
    // The dialog is also gated on the existence of a colony (the
    // building count clamps against `colony.buildings[bt]` so we
    // need a colony to read). The BuildingType and requested count
    // are stored in `DemolishConfirmState`; the count is the
    // build-qty multiplier the player has selected. `update_demolish_dialog_text`
    // re-reads the live count every frame and clamps the displayed
    // number so the dialog never claims to demolish more than
    // exists.
    let mut disabled_set: std::collections::HashSet<Entity> = std::collections::HashSet::new();
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
            if ui_state.selected_colony.is_none() {
                continue;
            }
            let count = match button.multiplier_source {
                DemolishMultiplierSource::Mining => ui_state.build_multiplier.max(1),
                DemolishMultiplierSource::Build => ui_state.build_multiplier.max(1),
            };
            // Open the dialog. `update_demolish_dialog_text`
            // clamps `count` to the live colony count, so even if
            // the player picks ×25 against 3 buildings the dialog
            // reads "Demolish 3 <BuildingType>?".
            confirm_state.open = true;
            confirm_state.building_type = Some(button.building_type);
            confirm_state.count = count;
        }
    }
    *prev = current;
}

// Per-frame system: re-evaluate which Demolish buttons should be
// disabled based on the current mine count. Mines can be added by
// the Queue button (or any other system) and removed by this same
// Demolish button, so a once-at-spawn check is not enough. Runs
// every frame the Mining body is open.
///
// Uses `queue_silenced` (Bevy 0.18+) so the insert / remove commands
// don't panic if `update_mining_body` despawns the parent card
// between this system's iter and the command apply at stage end.
pub fn tick_demolish_disabled(
    mut commands: Commands,
    ui_state: Res<ConstructionUiState>,
    colonies: Query<&crate::colony::Colony>,
    demolish_buttons: Query<(Entity, &DemolishButton, Has<DemolishDisabled>)>,
) {
    let Some(colony_entity) = ui_state.selected_colony else {
        return;
    };
    let Ok(colony) = colonies.get(colony_entity) else {
        return;
    };
    for (entity, button, is_disabled) in demolish_buttons.iter() {
        let count = colony
            .buildings
            .get(&button.building_type)
            .copied()
            .unwrap_or(0);
        if count == 0 && !is_disabled {
            commands
                .entity(entity)
                .queue_silenced(InsertDemolishDisabled);
        } else if count > 0 && is_disabled {
            commands
                .entity(entity)
                .queue_silenced(RemoveDemolishDisabled);
        }
    }
}

// v0.5.2 (2026-08-06): rewrite every Demolish button's label from the
// live `build_multiplier` each frame. The Buildings-tab cards are
// spawn-once-update-many — their Demolish label used to freeze at the
// spawn-frame multiplier ("Demolish -1" forever). Both multiplier
// sources resolve to `build_multiplier` in `tick_demolish_click`, so
// the label tracks the same value. Rewriting is cheap (a handful of
// Text nodes); the Text value only triggers relayout when it changes.
pub fn update_demolish_button_labels(
    ui_state: Res<ConstructionUiState>,
    mut labels: Query<&mut Text, With<DemolishButtonLabel>>,
) {
    let mult = ui_state.build_multiplier.max(1);
    let label = if mult > 1 {
        format!("Demolish \u{2212}{}", mult)
    } else {
        "Demolish -1".to_string()
    };
    for mut text in labels.iter_mut() {
        if text.0 != label {
            **text = label.clone();
        }
    }
}

// Hover / press effect system for the Demolish buttons. Mirrors
// `tick_construction_cta_hover` so the Build tab's Queue button
// and the Demolish button share the same hover affordance language.
///
// v0.5.2: the previous per-frame visual update was missing
// entirely — the Demolish button's rest color (`dim_red`,
// `0.353, 0.157, 0.169, 0.85`) was a static paint with no
// hover feedback. Players reported "no hover" on Mining tab
// buttons. The hover variant bumps both the alpha to 1.0 and
// the RGB values to a clearly brighter red that reads as
// "hover" at a glance (not a 0.04 RGB delta, but a ~0.15 delta
// with full opacity). Same flicker-mitigation
// `Local<HashMap<Entity, (Interaction, bool)>>` pattern as
// the CTA hover so frame-to-frame toggles of the disabled
// marker don't visually oscillate.
pub fn tick_demolish_hover(
    mut params: ParamSet<(
        Query<
            (
                Entity,
                &Interaction,
                &mut BackgroundColor,
                &mut BorderColor,
                &mut UiTransform,
            ),
            With<DemolishButton>,
        >,
        Query<Entity, With<DemolishDisabled>>,
    )>,
    mut prev_state: Local<std::collections::HashMap<Entity, (Interaction, bool)>>,
) {
    // Resting fill — must match `spawn_demolish_button`'s
    // `dim_red`. Hover fill is brighter and fully opaque; border
    // goes from the 50% alpha to a fully-opaque red on hover.
    let rest_fill = Color::srgba(0.353, 0.157, 0.169, 0.85);
    let hover_fill = Color::srgba(0.55, 0.235, 0.255, 1.0);
    let rest_border = Color::srgba(0.847, 0.373, 0.392, 0.50);
    let hover_border = Color::srgba(0.95, 0.45, 0.45, 1.0);
    let mut disabled_set: std::collections::HashSet<Entity> =
        std::collections::HashSet::new();
    for entity in params.p1().iter() {
        disabled_set.insert(entity);
    }
    for (entity, interaction, mut bg, mut border, mut ui_transform) in params.p0().iter_mut() {
        let is_disabled = disabled_set.contains(&entity);
        let prev = prev_state.get(&entity).copied();
        if let Some((prev_int, prev_disabled)) = prev {
            if prev_int == *interaction && prev_disabled == is_disabled {
                continue;
            }
        }
        match interaction {
            Interaction::Pressed if !is_disabled => {
                *bg = BackgroundColor(hover_fill);
                *border = BorderColor::all(hover_border);
                ui_transform.scale = Vec2::splat(0.98);
            }
            Interaction::Hovered if !is_disabled => {
                *bg = BackgroundColor(hover_fill);
                *border = BorderColor::all(hover_border);
                ui_transform.scale = Vec2::splat(1.02);
            }
            Interaction::None => {
                *bg = BackgroundColor(rest_fill);
                *border = BorderColor::all(rest_border);
                ui_transform.scale = Vec2::splat(1.00);
            }
            // Disabled + hover/pressed: keep the dim rest fill and
            // unfocus scale — the click is a no-op so the visual
            // feedback would be misleading.
            _ => {
                *bg = BackgroundColor(rest_fill);
                *border = BorderColor::all(rest_border);
                ui_transform.scale = Vec2::splat(1.00);
            }
        }
        prev_state.insert(entity, (*interaction, is_disabled));
    }
    // Finalizer pass — drop entries for entities that no longer
    // have a `DemolishButton` marker (Mining cards are despawned
    // every frame by `update_mining_body`). Without this the
    // `Local` grows unbounded across a long session.
    let live: std::collections::HashSet<Entity> = params.p0().iter().map(|(e, ..)| e).collect();
    prev_state.retain(|e, _| live.contains(e));
}

// `EntityCommand` that inserts `DemolishDisabled`. Used by
// `tick_demolish_disabled` via `queue_silenced` so the insert
// is dropped instead of panicking if the entity is despawned by the
// time the command applies.
struct InsertDemolishDisabled;

impl bevy::ecs::system::EntityCommand for InsertDemolishDisabled {
    fn apply(self, mut entity: bevy::ecs::world::EntityWorldMut) {
        entity.insert(DemolishDisabled);
    }
}

// `EntityCommand` that removes `DemolishDisabled`. See
// `InsertDemolishDisabled` for the rationale.
struct RemoveDemolishDisabled;

impl bevy::ecs::system::EntityCommand for RemoveDemolishDisabled {
    fn apply(self, mut entity: bevy::ecs::world::EntityWorldMut) {
        entity.remove::<DemolishDisabled>();
    }
}

// ── Demolish confirmation modal (v0.5.2 PR-A.7) ─────────────────────

// Per-frame: mirror `DemolishConfirmState::open` into the dialog
// root's `Visibility` (Hidden when closed, Inherited when open).
// Same inheritance pattern as `tick_colony_dropdown_visibility`
// so the dialog correctly hides when the Construction menu root
// is hidden (e.g. player toggles the menu off) without leaking
// the modal onto the world map.
pub fn tick_demolish_dialog_visibility(
    state: Option<Res<DemolishConfirmState>>,
    mut dialog_query: Query<&mut Visibility, With<DemolishConfirmDialog>>,
) {
    let is_open = state.as_ref().map(|s| s.open).unwrap_or(false);
    for mut visibility in dialog_query.iter_mut() {
        *visibility = if is_open {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

// Per-frame: re-read the live colony's building count for the
// dialog's target `BuildingType` and clamp the dialog's
// `count` down so the title never claims to demolish more than
// exists (e.g. player picks ×25 but only 3 buildings are
// present → title reads "Demolish 3 …?"). Also writes the
// title and subtitle text each frame so they stay in sync with
// the live state without per-frame string allocations beyond
// the format!.
///
// B0001 audit: title and subtitle both carry `&mut Text` (a
// shared component), so the two queries are folded into a
// `ParamSet` to avoid the dual-`Query<&mut Text>` conflict.
// B0002 audit: the function reads `DemolishConfirmState` (for
// the open flag / building / count) and writes it (to clamp
// `count` down to the live colony count); a single
// `ResMut` access covers both — the read happens first,
// then the conditional write.
pub fn update_demolish_dialog_text(
    ui_state: Res<ConstructionUiState>,
    confirm_state: Option<ResMut<DemolishConfirmState>>,
    colonies: Query<&crate::colony::Colony>,
    mut texts: ParamSet<(
        Query<&mut Text, With<DemolishConfirmTitle>>,
        Query<&mut Text, With<DemolishConfirmSubtitle>>,
    )>,
) {
    let Some(mut confirm) = confirm_state else {
        return;
    };
    if !confirm.open {
        return;
    }
    let Some(bt) = confirm.building_type else {
        return;
    };
    let Some(colony_entity) = ui_state.selected_colony else {
        return;
    };
    let Ok(colony) = colonies.get(colony_entity) else {
        return;
    };
    let live_count = colony.buildings.get(&bt).copied().unwrap_or(0);
    // Clamp the count down (but never below 0). The Yes button
    // handler will see the clamped value.
    let clamped_count = confirm.count.min(live_count);
    if clamped_count != confirm.count {
        confirm.count = clamped_count;
    }
    let title = format!("Demolish {} {}?", clamped_count, bt);
    let subtitle = format!(
        "You currently have {} on {}.",
        live_count, colony.name
    );
    for mut text in texts.p0().iter_mut() {
        **text = title.clone();
    }
    for mut text in texts.p1().iter_mut() {
        **text = subtitle.clone();
    }
}

// Yes button click: rising-edge `Interaction::Pressed` on a
// `DemolishConfirmYes` entity. Applies the `mining_edits` entry
// (negative count) to the selected colony and resets the dialog
// state to default (closed, no building, count 0).
pub fn tick_demolish_confirm_yes_click(
    yes_query: Query<(Entity, &Interaction), (With<DemolishConfirmYes>, With<Button>)>,
    mut confirm_state: ResMut<DemolishConfirmState>,
    ui_state: Res<ConstructionUiState>,
    mut pending: ResMut<PendingConstructionActions>,
    mut prev: Local<std::collections::HashMap<Entity, Interaction>>,
) {
    let mut current: std::collections::HashMap<Entity, Interaction> =
        std::collections::HashMap::new();
    for (entity, interaction) in yes_query.iter() {
        let prev_int = prev.get(&entity).copied().unwrap_or(Interaction::None);
        if *interaction == Interaction::Pressed
            && prev_int != Interaction::Pressed
            && confirm_state.open
        {
            let (Some(bt), Some(colony_entity)) =
                (confirm_state.building_type, ui_state.selected_colony)
            else {
                *confirm_state = DemolishConfirmState::default();
                current.insert(entity, *interaction);
                continue;
            };
            let count = confirm_state.count as i32;
            if count > 0 {
                pending.mining_edits.push((colony_entity, bt, -count));
            }
            *confirm_state = DemolishConfirmState::default();
        }
        current.insert(entity, *interaction);
    }
    *prev = current;
}

// No button click + backdrop click: rising-edge
// `Interaction::Pressed` on a `DemolishConfirmNo` entity OR on
// the `DemolishConfirmDialog` root (which is the backdrop in
// this layout). Resets the dialog state without applying the
// edit.
pub fn tick_demolish_confirm_no_click(
    no_query: Query<(Entity, &Interaction), (With<DemolishConfirmNo>, With<Button>)>,
    backdrop_query: Query<(Entity, &Interaction), (With<DemolishConfirmDialog>, With<Button>)>,
    mut confirm_state: ResMut<DemolishConfirmState>,
    mut prev: Local<std::collections::HashMap<Entity, Interaction>>,
) {
    let mut current: std::collections::HashMap<Entity, Interaction> =
        std::collections::HashMap::new();
    for (entity, interaction) in no_query.iter() {
        let prev_int = prev.get(&entity).copied().unwrap_or(Interaction::None);
        if *interaction == Interaction::Pressed && prev_int != Interaction::Pressed {
            *confirm_state = DemolishConfirmState::default();
        }
        current.insert(entity, *interaction);
    }
    for (entity, interaction) in backdrop_query.iter() {
        let prev_int = prev.get(&entity).copied().unwrap_or(Interaction::None);
        if *interaction == Interaction::Pressed && prev_int != Interaction::Pressed {
            *confirm_state = DemolishConfirmState::default();
        }
        current.insert(entity, *interaction);
    }
    *prev = current;
}

// When the player switches tabs while the dialog is open, reset
// the dialog state (mirrors the "click No" behavior — no action
// is applied). Caches the previous `selected_tab` in a `Local`
// to avoid spurious resets on the first frame.
pub fn tick_demolish_dialog_close_on_tab_switch(
    ui_state: Res<ConstructionUiState>,
    mut confirm_state: ResMut<DemolishConfirmState>,
    mut prev_tab: Local<Option<ConstructionTab>>,
) {
    if let Some(prev) = *prev_tab {
        if prev != ui_state.selected_tab && confirm_state.open {
            *confirm_state = DemolishConfirmState::default();
        }
    }
    *prev_tab = Some(ui_state.selected_tab);
}

// When the player switches colonies while the dialog is open,
// reset the dialog state. Same rationale as the tab-switch
// close — the dialog's `count` is bound to the original
// colony's building map, and switching colonies mid-confirmation
// would silently change the destructive target.
pub fn tick_demolish_dialog_close_on_colony_change(
    ui_state: Res<ConstructionUiState>,
    mut confirm_state: ResMut<DemolishConfirmState>,
    mut prev_colony: Local<Option<Entity>>,
) {
    if *prev_colony != ui_state.selected_colony && confirm_state.open {
        *confirm_state = DemolishConfirmState::default();
    }
    *prev_colony = ui_state.selected_colony;
}

// Group-visibility toggle: when the player clicks a group chevron
// (or the orbital section header), flip the corresponding bit in
// `ui_state.mining_groups_collapsed` / `ui_state.mining_orbital_collapsed`.
// The actual `Display::None / Display::Flex` swap happens on the
// next `update_mining_body` run (which re-spawns the cards).
///
// The orbital section header uses the same `MiningGroupHeader`
// marker with a sentinel `Helium3` group id. The handler uses
// that id to distinguish: `Helium3` → toggle orbital collapsed;
// anything else → toggle the corresponding surface group.
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
        if *interaction == Interaction::Pressed && prev_interaction != Interaction::Pressed {
            if header.group_id == MiningGroupId::Helium3 {
                // The orbital section header reuses the
                // `MiningGroupHeader` marker with the Helium3
                // sentinel. Distinguish by group id.
                ui_state.mining_orbital_collapsed = !ui_state.mining_orbital_collapsed;
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


#[cfg(test)]
mod formatter_tests {
    use super::*;

    #[test]
    fn format_mining_rate_ladder_lands_in_one_to_three_digits() {
        // Sanity-check the SI ladder. Each case picks the smallest
        // suffix that lands the displayed value in 1..=999. Numbers
        // come from real on-screen reads during v0.5.2 calibration
        // so any regression on the ladder is loud.
        assert_eq!(format_mining_rate(0.0), "0");
        // IronMine ×25 = 3000 Mt = 3 Gt → smallest-suffix band is Gt.
        assert_eq!(format_mining_rate(120.0 * 25.0), "3 Gt/yr");
        // 5e6 Mt = 5 Tt = 5000 Gt → smallest-suffix band is Tt.
        assert_eq!(format_mining_rate(5e6), "5.00 Tt/yr");
        assert_eq!(format_mining_rate(5.97e15), "5.97 Zt/yr");
        assert_eq!(format_mining_rate(1.8e5), "180 Gt/yr");
        assert_eq!(format_mining_rate(500.0), "500 Mt/yr");
        assert_eq!(format_mining_rate(0.5), "500 kt/yr");
    }

    #[test]
    fn format_mining_reserve_ladder_lands_in_one_to_three_digits() {
        assert_eq!(format_mining_reserve(0.0), "0");
        assert_eq!(format_mining_reserve(1.8e5), "180 Gt");
        assert_eq!(format_mining_reserve(500.0), "500 Mt");
        assert_eq!(format_mining_reserve(0.5), "500 kt");
        // 1e-3 Mt = 1000 t, lands in the kt band.
        assert_eq!(format_mining_reserve(1e-3), "1 kt");
    }

    #[test]
    fn format_power_picks_smallest_suffix() {
        assert_eq!(format_power(0.0), "0 W");
        assert_eq!(format_power(0.5), "500 kW");
        assert_eq!(format_power(50.0), "50 MW");
        assert_eq!(format_power(900.0), "900 MW");
        assert_eq!(format_power(250.0), "250 MW");
        // 5 GW fusion plant used to read as "5000 MW".
        // v0.5.2 PR-A.7 (2026-08-04): trailing zeros stripped
        // so "5.0 GW" becomes "5 GW" and "12.00 TW" becomes
        // "12 TW". The formatter still rounds to the
        // band-appropriate decimal count (1 for GW, 2 for TW/PW)
        // — it just drops the trailing `.0` / `.00` zeros.
        assert_eq!(format_power(5000.0), "5 GW");
        // 12 TW helioforge-class generator.
        assert_eq!(format_power(12_000_000.0), "12 TW");
    }
}

#[cfg(test)]
mod body_blocked_tests {
    // v0.5.2 (build menu fix): verify the body-blocked gate
    // for the Mining tab. He3Mine is the canonical case —
    // `allowed_body_types: [Moon, GasGiant, Asteroid]` in the
    // RON, so it must come back as body-blocked on Earth (a
    // Planet). The Queue button on the He3Mine card on Earth
    // should be greyed out with a `Body unavailable` tooltip.

    use super::*;
    use crate::plugins::solar_system_data::BodyType;
    use crate::colony::AtmosphereKind;

    fn he3_mine_def() -> BuildingDefinition {
        BuildingDefinition {
            id: "He3Mine".to_string(),
            display_name: "He3 Mine".to_string(),
            description: "solar-wind regolith / gas-giant atmosphere".to_string(),
            icon: "☀".to_string(),
            category: "Industry".to_string(),
            build_points: 3500.0,
            workforce: 8000,
            required_tech: "lunar_colony".to_string(),
            resource_costs: vec![],
            maintenance_resources: vec![],
            modifiers: vec![],
            power_demand_mw: 100.0,
            tier: 0,
            line: Some("Mine".to_string()),
            replaces: None,
            replaces_in_line: None,
            synergy: vec![],
            available_atmospheres: vec![
                AtmosphereKind::Breathable,
                AtmosphereKind::None,
            ],
            required_anomalies: vec![],
            allowed_body_types: vec![
                BodyType::Moon,
                BodyType::GasGiant,
                BodyType::Asteroid,
            ],
        }
    }

    fn iron_mine_def() -> BuildingDefinition {
        // Iron Mine is buildable on every body (empty
        // `allowed_body_types`). Used as the
        // body-available control in the test below.
        BuildingDefinition {
            id: "IronMine".to_string(),
            display_name: "Iron Mine".to_string(),
            description: "iron ore extraction".to_string(),
            icon: "⛏".to_string(),
            category: "Industry".to_string(),
            build_points: 200.0,
            workforce: 500,
            required_tech: "".to_string(),
            resource_costs: vec![],
            maintenance_resources: vec![],
            modifiers: vec![],
            power_demand_mw: 50.0,
            tier: 0,
            line: Some("Mine".to_string()),
            replaces: None,
            replaces_in_line: None,
            synergy: vec![],
            available_atmospheres: vec![
                AtmosphereKind::Breathable,
                AtmosphereKind::None,
            ],
            required_anomalies: vec![],
            allowed_body_types: vec![],
        }
    }

    #[test]
    fn he3_mine_on_earth_is_body_blocked() {
        // Earth: Planet, breathable. He3Mine only allowed on
        // Moon / GasGiant / Asteroid → body-blocked = true.
        let def = he3_mine_def();
        let data = build_mine_card_data(
            BuildingType::He3Mine,
            &def,
            0,
            true,                              // breathable
            Some(BodyType::Planet),           // Earth
            None,                              // no planet_resources
            1,                                 // multiplier
            f64::NAN,                          // spare_power_mw
        );
        assert!(
            data.body_blocked,
            "He3Mine on Earth should be body-blocked; got body_blocked=false"
        );
        assert!(
            data.power_insufficient,
            "body-blocked mining cards should also be power_insufficient so \
             the spawn-time CTA carries the ConstructionCtaDisabled marker"
        );
    }

    #[test]
    fn he3_mine_on_moon_is_body_available() {
        // Moon: in the allowed list → body-blocked = false.
        let def = he3_mine_def();
        let data = build_mine_card_data(
            BuildingType::He3Mine,
            &def,
            0,
            false,                             // Moon has no atmosphere
            Some(BodyType::Moon),
            None,
            1,
            f64::NAN,
        );
        assert!(
            !data.body_blocked,
            "He3Mine on Moon should be body-available; got body_blocked=true"
        );
    }

    #[test]
    fn iron_mine_on_earth_is_body_available() {
        // Iron Mine: empty `allowed_body_types` → any body.
        let def = iron_mine_def();
        let data = build_mine_card_data(
            BuildingType::IronMine,
            &def,
            0,
            true,                              // breathable
            Some(BodyType::Planet),
            None,
            1,
            f64::NAN,
        );
        assert!(
            !data.body_blocked,
            "Iron Mine should be body-available on every body; got body_blocked=true"
        );
    }

    #[test]
    fn body_blocked_skips_per_frame_affordability_system() {
        // The whole point of the body-blocked marker: the
        // per-frame affordability system must NOT silently
        // re-enable a body-blocked CTA when the player has
        // the resources on hand. We exercise that contract
        // by asserting `power_insufficient` is `true` for
        // body-blocked cards (which keeps the spawn-time
        // `ConstructionCtaDisabled` marker present, and
        // the per-frame system filter
        // `Without<ConstructionCtaBodyBlocked>` excludes
        // the CTA from the togglable set).
        let def = he3_mine_def();
        let data = build_mine_card_data(
            BuildingType::He3Mine,
            &def,
            0,
            true,
            Some(BodyType::Planet),
            None,
            1,
            f64::NAN,
        );
        // Sanity: `data` reflects both gates.
        assert!(data.body_blocked);
        assert!(data.power_insufficient);
    }

    /// v0.5.2 (2026-08-06, bugfix round 2): the mining power gate.
    /// Before, `power_insufficient = body_blocked` — a mine whose
    /// batch demand exceeded the colony's grid spare was NOT gated
    /// (the CTA stayed enabled, the player queued it, the grid went
    /// into deficit). Now `power_insufficient = body_blocked ||
    /// power_chip.insufficient`, and the power chip carries the real
    /// spare threaded from `update_mining_body`. A 50 MW mine at ×5
    /// on a 100 MW grid must be power-insufficient (disabled CTA).
    #[test]
    fn mine_power_demand_gates_cta_when_grid_short() {
        let def = iron_mine_def(); // 50 MW demand
        // ×5 batch = 250 MW on a 100 MW spare grid → insufficient.
        let data = build_mine_card_data(
            BuildingType::IronMine,
            &def,
            0,
            true,
            Some(BodyType::Planet),
            None,
            5,
            100.0, // spare_power_mw: 100 MW
        );
        assert!(
            data.power_insufficient,
            "50 MW mine ×5 (250 MW) on a 100 MW spare grid must be \
             power-insufficient (CTA disabled)"
        );
        assert!(data.power_chip.insufficient);
        assert!(data.power_chip.spare_mw.is_some());

        // ×1 batch = 50 MW on the same 100 MW grid → fits.
        let ok = build_mine_card_data(
            BuildingType::IronMine,
            &def,
            0,
            true,
            Some(BodyType::Planet),
            None,
            1,
            100.0,
        );
        assert!(
            !ok.power_insufficient,
            "50 MW mine ×1 on a 100 MW spare grid must be affordable"
        );
        assert!(!ok.power_chip.insufficient);

        // No colony (NAN spare) → no power gate (like the Build tab).
        let no_colony = build_mine_card_data(
            BuildingType::IronMine,
            &def,
            0,
            true,
            Some(BodyType::Planet),
            None,
            5,
            f64::NAN,
        );
        assert!(
            !no_colony.power_insufficient,
            "no colony selected → power gate must not disable the CTA"
        );
    }

    /// v0.5.2 (2026-08-06): the constructed-buildings card.
    /// - `constructed: true` → `spawn_card` skips the CTA + ETA.
    /// - The power chip aggregates across the built count (red for
    ///   a net consumer, green for a generator).
    /// - The header has no "Demolish -N" stat.
    #[test]
    fn constructed_card_aggregates_power_and_has_no_cta() {
        // 3 × 50 MW Iron Mines → -150 MW (red consumer).
        let def = iron_mine_def();
        let data = build_constructed_card_data(BuildingType::IronMine, &def, 3, 5);
        assert!(data.constructed, "constructed card must set the flag");
        assert!(
            data.power_chip.per_unit_mw < 0.0,
            "constructed consumer chip must be signed negative (red)"
        );
        assert_eq!(data.power_chip.amount, "150 MW");
        // The demolish batch label must NOT be in the header anymore.
        assert_eq!(data.stat_b.1, "", "Demolish -N text must not be in the header");
        assert_eq!(data.stat_a.1, "\u{00d7}3", "header shows the built count");

        // A generator (PowerGeneration modifier) → green +N sum.
        let mut gen_def = iron_mine_def();
        gen_def.modifiers = vec![crate::colony::data::BuildingModifierDef {
            modifier_type: "PowerGeneration".to_string(),
            value: 2.0, // 2 GW per unit
        }];
        let gen = build_constructed_card_data(BuildingType::IronMine, &gen_def, 4, 1);
        assert!(
            gen.power_chip.per_unit_mw > 0.0,
            "constructed generator chip must be signed positive (green)"
        );
        // 4 × 2 GW = 8 GW.
        assert_eq!(gen.power_chip.amount, "8 GW");
    }
}
