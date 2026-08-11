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
    //
    // (No orbital section. The legacy orbital-collapsed bool
    // and the `Auto*Mine` buildings were removed when the
    // orbital UI was stripped — orbital construction will be
    // reintroduced later via space stations, not via a
    // duplicate mining grid.)
    pub mining_groups_collapsed: HashSet<MiningGroupId>,
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
        }
    }
}

// Identifier for one of the 6 surface mine groups in the Mining
// tab. Used as a key into `ConstructionUiState::mining_groups_collapsed`
// so collapse state persists per group.
//
// v0.5.2 PR-K: grouped using the **same canonical top-level
// resource categories that the top resources bar uses**
// (`ResourceType::category()`). The previous ad-hoc groups
// ("Hydrocarbons", "HeavyWater", "Water", "Helium-3") were
// confusing because they overlapped canonical categories
// (Methane/Phosphorus are "Volatiles"; Deuterium is
// "Fusion Fuel"; Helium-3 is "Fusion Fuel"; Water is
// "Volatiles"). With the rewrite, the Mining tab groups
// match the top-bar categories one-for-one, so the player
// can mentally map "the resource icon I see in the top bar
// belongs to the Construction group here, and so does its
// mine".
//
// Construction handles building on the currently selected
// body — moon, planet, asteroid, etc. Orbital construction
// (the legacy `Auto*Mine` buildings) is not exposed here;
// it will be reintroduced later via space stations rather
// than via a duplicate mining grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MiningGroupId {
    // Top-bar "Construction" category (Iron, Aluminum, Titanium,
    // Silicates, Nickel, Tungsten, Carbon, Chromium, Magnesium).
    // Copper was moved out to `Strategic` so it matches the
    // top-bar category — the top bar puts Copper under Strategic.
    Construction,
    // Top-bar "Precious Metals" (Gold, Silver, Platinum).
    PreciousMetals,
    // Top-bar "Strategic" (Copper, RareEarths, Lithium, Sulfur,
    // Cobalt, Fluorine). Polymers has no mine.
    Strategic,
    // Top-bar "Fissiles" (Uranium, Thorium). Plutonium has no
    // dedicated mine — it is bred from Uranium in reactors.
    Fissiles,
    // Top-bar "Volatiles" (Water, Hydrogen, Ammonia, Methane,
    // Phosphorus). Mines/extractors that map here:
    //   - WaterProcessor (atmospheric condenser / ice miner)
    //   - MethaneExtractor (CH4 from lakes like Titan)
    //   - PhosphorusMine (phosphate rock / apatite)
    // Phosphorus is "Volatiles" per `ResourceType::category()`;
    // this matches the top resources-bar grouping even though
    // real-world phosphorus is more often classified as a
    // strategic/industrial mineral.
    Volatiles,
    // Top-bar "Fusion Fuel" (Helium-3, Deuterium, Tritium).
    // Mines that map here:
    //   - He3Mine (regolith / gas giants)
    //   - DeuteriumExtractor (heavy water from seawater / ice)
    // Tritium has no dedicated mine — it is bred from Lithium
    // or Deuterium in reactors.
    FusionFuel,
}

// Surface mine group definitions for the Mining tab. Each entry
// is `(group_id, display_label, buildings)` in display order.
//
// v0.5.2 PR-K: 6 groups that mirror the canonical top-bar
// categories (`ResourceType::category()`):
//   1. Construction      (9 mines — Iron .. Magnesium, no Copper)
//   2. Precious Metals   (3 mines — Gold, Silver, Platinum)
//   3. Strategic         (6 mines — Copper, RareEarths .. Fluorine)
//   4. Fissiles          (2 mines — Uranium, Thorium)
//   5. Volatiles         (3 mines/extractor — WaterProcessor, MethaneExtractor, Phosphorus)
//   6. Fusion Fuel       (2 mines/extractor — He3, Deuterium)
//
// 25 cards total: 24 base mines + `WaterProcessor` (an
// atmospheric condenser / ice miner — not a "mine" in the
// strict sense, but the top-level extractor that produces
// water and slots into the same Volatiles group).
pub const MINING_GROUPS_SURFACE: &[(MiningGroupId, &str, &[BuildingType])] = &[
    (
        MiningGroupId::Construction,
        "Construction",
        &[
            BuildingType::IronMine,
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
        MiningGroupId::PreciousMetals,
        "Precious Metals",
        &[
            BuildingType::GoldMine,
            BuildingType::SilverMine,
            BuildingType::PlatinumMine,
        ],
    ),
    (
        MiningGroupId::Strategic,
        "Strategic",
        &[
            BuildingType::CopperMine,
            BuildingType::RareEarthsMine,
            BuildingType::LithiumMine,
            BuildingType::SulfurMine,
            BuildingType::CobaltMine,
            BuildingType::FluorineMine,
        ],
    ),
    (
        MiningGroupId::Fissiles,
        "Fissiles",
        &[BuildingType::UraniumMine, BuildingType::ThoriumMine],
    ),
    (
        MiningGroupId::Volatiles,
        "Volatiles",
        &[
            BuildingType::WaterProcessor,
            BuildingType::MethaneExtractor,
            BuildingType::PhosphorusMine,
        ],
    ),
    (
        MiningGroupId::FusionFuel,
        "Fusion Fuel",
        &[BuildingType::He3Mine, BuildingType::DeuteriumExtractor],
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
