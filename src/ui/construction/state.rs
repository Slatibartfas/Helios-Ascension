//! State types and resources for the construction UI.
//!
//! These types hold persistent UI state (tab selection, queue state,
//! tooltip state, etc.) and the enums that drive them. They're
//! re-exported through the parent module.

use bevy::prelude::*;
use std::collections::HashSet;

use crate::colony::types::BuildingType;

// ── Construction UI shared types (v0.5.2: moved out of
//    construction_panel.rs, which is being deleted as the canary
//    becomes the new main construction menu) ────────────────────────

// Top-level sub-tab in the Construction menu.
//
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
//
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
    pub mining_groups_collapsed: HashSet<MiningGroupId>,
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
            mining_groups_collapsed: HashSet::new(),
            mining_orbital_collapsed: true, // collapsed by default
        }
    }
}

// Identifier for one of the 8 surface mine groups in the Mining
// tab. Used as a key into `ConstructionUiState::mining_groups_collapsed`
// so collapse state persists per group.
//
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
//
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
//
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
//
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
    buildings_data: Option<Res<crate::colony::data::BuildingsData>>,
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
//
// This is the single source of truth for visual active state. The
// `tick_chip_button_active_overlay` system reads this resource each
// Resource: the construction queue + current output rate.
//
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
    pub building_type: Option<BuildingType>,
    pub count: u32,
}

// Resource: hover-driven tooltip state for the construction canary.
//
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

// Marker component on the **root** of each sub-tab body. The canary
// spawns one of these per sub-tab (Overview / Buildings / Build /
// Stockpiles) at setup time, all as children of the `ConstructionRoot`.
// The `tick_construction_body_visibility` system makes exactly one
// visible (the one matching `ui_state.selected_tab`) every frame,
// keeping the others hidden via `Visibility::Hidden`.
//
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
