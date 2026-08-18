//! Construction panel rendered with `bevy_ui` (Phase C, `rework-ui-design`).
//!
//! This renders the Construction panel using
//! `bevy_ui` (replacing the legacy egui version).
//! Activated on `F4` (Construction menu).
//!
//! # Module layout (Phase 1.5B refactor)
//!
//! The original `construction.rs` was 11 865 LOC with 250+ items. It is
//! split into the following submodules (no behavior change):
//!
//! - `state`    — persistent UI state (ConstructionUiState, queue state, etc.)
//! - `data`     — pure-data types and pure-data helpers (BuildCardData, formatters)
//! - `markers`  — `#[derive(Component)]` markers (cards, CTAs, scrollbars, etc.)
//! - `cards`    — the shared card chrome (`spawn_card`, `spawn_constructed_card`)
//! - `mining`   — Mining tab body + per-resource/AutoMine spawn helpers
//! - `overview` — Overview tab body + per-frame updates
//! - `buildings`— Buildings tab body + per-frame updates
//! - `demolish` — Demolish button + confirmation dialog
//! - `queue`    — Queue panel (AppBar chip + slide-out panel + row management)
//! - `dropdown` — Active Colony dropdown menu
//! - `tooltip`  — cursor-following chip + power + queue-CTA tooltips
//! - `scrollbar`— shared scrollbar chrome (track + thumb + drag)
//! - `disabled` — affordability + hover + marquee + refresh
//! - `setup`    — `setup_construction` (the GOLIATH — 1085 LOC)
//!
//! Every `pub` item from the original file is re-exported below so
//! `crate::ui::construction::Foo` paths continue to resolve unchanged.

// Module-level `#[allow(dead_code, unused_imports)]` covers two patterns that
// emerge naturally from this layout:
//
// 1. Wildcard re-exports (`pub use super::markers::*;`) preserve the public
//    API of the original 11,865-LOC file but pull in items that the rest of
//    the construction/ tree may not reference directly.
// 2. Cross-submodule imports via `use super::data::*;` (etc.) are broader
//    than necessary because the split is mechanical — some items imported
//    are actually only used inside their defining submodule.
//
// TODO (Phase 1.5D follow-up): tighten by replacing `use super::X::*;` with
// explicit `use super::X::Foo, Bar;` lists and dropping items the rest of
// the construction/ tree does not use. Then remove this file-level allow.
#![allow(dead_code, unused_imports)]

use bevy::prelude::*;

// ── Module declarations ───────────────────────────────────────────────

mod buildings;
mod cards;
mod data;
mod demolish;
mod disabled;
mod dropdown;
mod markers;
mod mining;
mod overview;
mod queue;
mod scrollbar;
mod setup;
mod state;
mod tooltip;

// Internal use — siblings reference each other.
use bevy::input::mouse::MouseWheel;
use bevy::picking::events::{Out, Over, Pointer, Press, Release};
use bevy::picking::hover::HoverMap;
use bevy::picking::pointer::{PointerButton, PointerId};
use bevy::picking::Pickable;
use bevy::ui::RelativeCursorPosition;
use bevy::window::{CursorMoved, PrimaryWindow};

use crate::colony::components::PendingConstructionActions;
use crate::game_state::{ActiveMenu, GameMenu};
use crate::ui::bevy_theme::*;
use crate::ui::widgets::{
    tick_active_chip_glow, tick_chip_button_active_overlay, tick_chip_button_hover,
};
use state::ColonyDropdownState;

// ── Re-exports — pure data types ──────────────────────────────────────
//
// Phase 1.5C: `friendly_label`, `format_mining_rate`, `format_residents`,
// and `EffectTone` moved to `crate::colony::building_data` because they
// are pure-data transformations with no UI dependencies. The remaining
// items below still live in `data.rs`.

pub use crate::colony::building_data::{
    format_mining_rate, format_residents, friendly_label, EffectTone,
};
pub use data::{
    apply_effect_cap, build_power_chip_data, card_data_from_definition, card_data_with_multiplier,
    category_from_index, compute_colony_spare_power_mw, format_mining_reserve, format_power,
    parse_category, power_output_gw_per_unit, visible_cards, BuildCardData, MiningCardData,
    PowerChipData, ResourceCostRow,
};

// ── Re-exports — state types / resources ──────────────────────────────

pub use state::{
    card_eta_seconds, load_building_icons, process_building_icons, queue_remaining_seconds,
    BuildFilter, BuildingIcons, ConstructionQueue, ConstructionState, ConstructionTab,
    ConstructionTabBody, ConstructionUiState, ConstructionUiState as ConstructionUiStateExport,
    DemolishConfirmState, MiningGroupId, QueuePanelState, QueuedBuild, ShowOnBuildOrBuildings,
    MINING_GROUPS_SURFACE,
};

// Phase 2: ActiveChips and ChipKind → widgets.
// - `ActiveChips` moved to `crate::ui::widgets::ActiveChips`.
// - `ChipKind` renamed to `crate::ui::widgets::ChipGroup` (the
//   `Filter(BuildFilter)` variant is dropped — see widget docs).
// Re-export them here so existing `crate::ui::construction::ActiveChips`
// and `crate::ui::construction::ChipKind` paths continue to resolve.
pub use crate::ui::widgets::{ActiveChips, ChipGroup};

// ── Re-exports — component markers ────────────────────────────────────

pub use buildings::{BuildingsContent, BuildingsHeader};
pub use markers::{
    BuildCard, CardGrid, ColonyDropdownMenu, ColonyDropdownOption, ColonyDropdownOptionText,
    ColonyPicker, ColonyPickerText, ConstructionCard, ConstructionCta, ConstructionCtaBodyBlocked,
    ConstructionCtaCapped, ConstructionCtaDisabled, ConstructionCtaLabelMarker, ConstructionRoot,
    ConstructionScrollbarTab, ConstructionScrollbarThumb, ConstructionScrollbarTrack,
    ConstructionSubtitle, ConstructionTitle, DemolishButton, DemolishButtonLabel,
    DemolishConfirmDialog, DemolishConfirmNo, DemolishConfirmSubtitle, DemolishConfirmTitle,
    DemolishConfirmYes, DemolishDisabled, DemolishMultiplierSource, MiningCard, MiningContent,
    MiningGroupBody, MiningGroupHeader, OpenQueueChip, PowerChip, QueuePanelBody, QueuePanelClose,
    QueuePanelRoot, QueuePanelRow, QueuePanelRowCancel, QueuePanelRowEta, QueuePanelRowFill,
    QueuePanelSummaryText, ResourceCostChip, SubtitleMarquee,
};
pub use overview::{
    OverviewQueueContent, OverviewQueueRow, OverviewQueueRowFillChild, OverviewQueueRowNameChild,
    OverviewQueueRowProgressChild, OverviewQueueRowStatusChild, OverviewRowKind, OverviewRowValue,
};

// ── Re-exports — systems ──────────────────────────────────────────────

// Body / visibility
pub use setup::setup_construction;
pub use state::process_building_icons as _reexport_state_process_building_icons;

// Card systems
pub use cards::{build_constructed_card_data, spawn_card, spawn_constructed_card};

// Mining tab
pub use mining::{
    build_mine_card_data, spawn_mining_body, tick_mining_group_visibility, update_mining_body,
};

// Overview tab
pub use overview::{spawn_overview_body, update_overview_body, update_overview_queue};

// Buildings tab
pub use buildings::{spawn_buildings_body, update_buildings_body};

// Demolish
pub use demolish::{
    spawn_demolish_button, spawn_demolish_confirm_dialog, tick_demolish_click,
    tick_demolish_confirm_no_click, tick_demolish_confirm_yes_click,
    tick_demolish_dialog_close_on_colony_change, tick_demolish_dialog_close_on_esc,
    tick_demolish_dialog_close_on_tab_switch, tick_demolish_dialog_visibility,
    tick_demolish_disabled, tick_demolish_hover, update_demolish_button_labels,
    update_demolish_dialog_text,
};

// Queue panel
pub use queue::{
    tick_open_queue_chip_click, tick_queue_panel_close_click, tick_queue_panel_close_on_esc,
    tick_queue_panel_row_cancel_click, tick_queue_panel_visibility, update_queue_panel,
    update_queue_row_eta, update_queue_row_progress, update_queue_summary,
};

// Dropdown
pub use dropdown::{
    auto_select_first_colony, refresh_colony_dropdown, tick_colony_dropdown_visibility,
    tick_colony_option_click, tick_colony_picker_click, update_colony_picker_text,
};

// Tooltip
pub use tooltip::{
    on_chip_hover_out, on_chip_hover_over, on_power_chip_hover_out, on_power_chip_hover_over,
    spawn_construction_tooltip_overlay, tick_construction_chip_click, tick_construction_tooltip,
};

// Scrollbar
pub use scrollbar::{
    spawn_construction_scrollbar, tick_construction_scrollbar, tick_construction_scrollbar_drag,
    tick_ui_scroll_on_wheel,
};

// Disabled / hover / marquee / CTA click
pub use disabled::{
    refresh_card_grid, tick_construction_cta_click, tick_construction_cta_disabled,
    tick_construction_cta_hover, tick_construction_cta_label_dim, tick_subtitle_marquee,
};

// ── Sub-tab body visibility + state system ───────────────────────────

// System: make the body matching `ui_state.selected_tab` visible and
// hide the others.
pub fn tick_construction_body_visibility(
    ui_state: Res<ConstructionUiState>,
    body_query: Query<(&ConstructionTabBody, &mut Node, &mut Visibility)>,
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
        (&ConstructionScrollbarTab, &mut Node),
        (
            Without<ConstructionTabBody>,
            Without<ShowOnBuildOrBuildings>,
            Without<ConstructionScrollbarThumb>,
        ),
    >,
) {
    let active = ConstructionTabBody::from_tab(ui_state.selected_tab);
    // Phase 9: body visibility now goes through the generic
    // `widgets::tick_tab_body_visibility` primitive.
    crate::ui::widgets::tick_tab_body_visibility(body_query, active as usize, |kind| {
        *kind == active
    });
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
    // Phase 5B follow-up (2026-08-11): the scrollbar tracks are
    // children of the panel ROOT, NOT of their tab bodies — so the
    // Phase 5B assumption ("each track follows its body's Visibility
    // via inherited Visibility") was wrong: all three tracks stayed
    // visible and overlapped at the right edge (the two-tone grey
    // bar, and Build-tab click/drag hitting the topmost Mining track
    // and scrolling a hidden body). Restore the pre-migration
    // per-track gating via the `ConstructionScrollbarTab` marker —
    // only the active tab's track gets `Display::Flex`; the rest are
    // `Display::None` (no render, no pick). The visual driver stays
    // the ungated `widgets::tick_scrollbar`; hidden tracks simply
    // compute zero-height thumbs.
    for (tab_marker, mut node) in scrollbar_track.iter_mut() {
        let visible = tab_marker.0 == active;
        node.display = if visible {
            Display::Flex
        } else {
            Display::None
        };
    }
}

// Visibility system: shows / hides the canary root based on
// `ActiveMenu.current == GameMenu::Construction`.
pub fn tick_construction_state(
    active_menu: Res<ActiveMenu>,
    mut state: ResMut<ConstructionState>,
    mut root_query: Query<&mut Visibility, With<ConstructionRoot>>,
    mut tooltip_request: ResMut<crate::ui::widgets::TooltipRequest>,
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
        *tooltip_request = crate::ui::widgets::TooltipRequest::default();
        *demolish_state = DemolishConfirmState::default();
        dropdown_state.open = false;
        queue_panel_state.open = false;
    }
}

/// `run_if` predicate: the Construction canary is currently visible.
fn construction_menu_open(state: Res<ConstructionState>) -> bool {
    *state == ConstructionState::On
}

// Phase 4: canary root's `top: 126.0` (see `setup_construction`).
// Passed to `widgets::tick_tooltip` so cursor coords are translated
// from window-space into the overlay's local frame.
const CONSTRUCTION_CANARY_TOP_PX: f32 = 126.0;

// Plugin: registers the Construction canary on `bevy_ui`.
pub struct ConstructionPlugin;

impl Plugin for ConstructionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ConstructionState>()
            .init_resource::<ConstructionQueue>()
            .init_resource::<ActiveChips>()
            .init_resource::<QueuePanelState>()
            .init_resource::<ColonyDropdownState>()
            .init_resource::<DemolishConfirmState>()
            .init_resource::<crate::ui::widgets::TooltipRequest>()
            .init_resource::<scrollbar::ScrollbarDragState>()
            .add_systems(
                Startup,
                (
                    setup_construction
                        .after(crate::colony::data::load_buildings)
                        .after(load_building_icons),
                    load_building_icons.after(crate::colony::data::load_buildings),
                ),
            )
            .add_systems(Update, process_building_icons)
            .add_systems(Update, tick_construction_state)
            .add_systems(
                Update,
                tick_construction_cta_hover.run_if(construction_menu_open),
            )
            // Phase 10: tick_subtitle_marquee is now a thin shim over
            // widgets::tick_marquee. The generic system runs ungated
            // (so any panel can adopt the primitive); the shim stays
            // construction-gated for parity with the original
            // always-on-while-menu-open behaviour.
            .add_systems(Update, tick_subtitle_marquee.run_if(construction_menu_open))
            // Phase 10: per-frame progress bar width write. Reads the
            // `ProgressFill(f32)` percentage on every ProgressFill
            // entity (QueuePanelRow + OverviewQueueRow) and writes
            // Node.width. Runs ungated so any panel with a progress
            // fill benefits automatically.
            .add_systems(Update, crate::ui::widgets::tick_progress_fill)
            // Phase 5B: tick_construction_scrollbar is a no-op
            // re-export of widgets::tick_scrollbar (the per-track
            // visual driver lives in widgets.rs and reads the
            // ScrollbarMetrics Component on each track). The drag
            // system stays construction-side because it needs the
            // construction's ScrollbarDragState Resource.
            .add_systems(
                Update,
                tick_construction_scrollbar_drag.run_if(construction_menu_open),
            )
            .add_systems(
                Update,
                tick_ui_scroll_on_wheel.run_if(construction_menu_open),
            )
            // Per-track visual driver — ungated so all construction
            // scrollbars (and any future panel) keep their thumbs in
            // sync regardless of which menu is active. The drag
            // observer handlers (`on_thumb_press` / `on_track_press`)
            // are entity-scoped and don't need a run_if.
            .add_systems(Update, crate::ui::widgets::tick_scrollbar)
            .add_systems(
                Update,
                tick_construction_cta_click.run_if(construction_menu_open),
            )
            .add_systems(
                Update,
                tick_construction_chip_click.run_if(construction_menu_open),
            )
            .add_systems(Update, auto_select_first_colony.before(refresh_card_grid))
            .add_systems(
                Update,
                refresh_card_grid
                    .run_if(resource_changed::<ConstructionUiState>)
                    .run_if(construction_menu_open),
            )
            .add_systems(
                Update,
                tick_construction_cta_disabled
                    .after(refresh_card_grid)
                    .after(auto_select_first_colony)
                    .run_if(construction_menu_open),
            )
            .add_systems(
                Update,
                tick_construction_body_visibility.run_if(construction_menu_open),
            )
            .add_systems(
                Update,
                (
                    update_overview_body,
                    update_overview_queue,
                    update_buildings_body,
                    update_mining_body,
                    tick_mining_group_visibility,
                    tick_demolish_click,
                    tick_demolish_disabled,
                    update_demolish_button_labels,
                    tick_demolish_hover,
                    tick_demolish_dialog_visibility,
                    update_demolish_dialog_text,
                    tick_demolish_confirm_yes_click,
                    tick_demolish_confirm_no_click,
                    tick_demolish_dialog_close_on_esc,
                    tick_demolish_dialog_close_on_tab_switch,
                    tick_demolish_dialog_close_on_colony_change,
                )
                    .run_if(construction_menu_open),
            )
            .add_systems(
                Update,
                (
                    tick_open_queue_chip_click,
                    tick_queue_panel_close_click,
                    tick_queue_panel_close_on_esc,
                    tick_queue_panel_visibility,
                    update_queue_summary,
                    update_queue_panel,
                    update_queue_row_eta,
                    update_queue_row_progress,
                    tick_queue_panel_row_cancel_click,
                )
                    .run_if(construction_menu_open),
            )
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
            .add_systems(
                Update,
                tick_construction_tooltip
                    .after(tick_construction_cta_disabled)
                    .run_if(construction_menu_open),
            )
            // Phase 4: the generic tooltip driver. Reads
            // `TooltipRequest` (written by hover observers + the
            // CTA scan), applies 250 ms latency, positions the
            // overlay next to the cursor with viewport clamping.
            // `top_offset_px: 126.0` translates window-space cursor
            // coords into the canary root's local frame.
            .add_systems(
                Update,
                tick_construction_tooltip_with_offset
                    .after(tick_construction_tooltip)
                    .run_if(construction_menu_open),
            )
            .add_systems(
                Update,
                tick_construction_cta_label_dim
                    .after(tick_construction_cta_hover)
                    .run_if(construction_menu_open),
            )
            .add_systems(
                Update,
                (
                    super::widgets::tick_chip_button_hover,
                    super::widgets::tick_chip_button_active_overlay,
                    super::widgets::tick_active_chip_glow,
                )
                    .chain()
                    .run_if(construction_menu_open),
            );
    }
}

// Phase 4 adapter: the generic `widgets::tick_tooltip` takes a
// `top_offset_px: f32` parameter, but `add_systems` requires a
// system with no extra params. This thin shim supplies the
// canary-root offset (126.0) at registration time.
fn tick_construction_tooltip_with_offset(
    commands: Commands,
    time: Res<Time>,
    primary_window: Query<&bevy::window::Window, With<bevy::window::PrimaryWindow>>,
    request: Res<crate::ui::widgets::TooltipRequest>,
    overlay_node: bevy::ecs::system::Single<&mut Node, With<crate::ui::widgets::TooltipOverlay>>,
    title_text: bevy::ecs::system::Single<
        &mut Text,
        (
            With<crate::ui::widgets::TooltipTitle>,
            Without<crate::ui::widgets::TooltipBody>,
        ),
    >,
    body_children: Query<&Children>,
    body_entity_q: bevy::ecs::system::Single<
        Entity,
        (
            With<crate::ui::widgets::TooltipBody>,
            Without<crate::ui::widgets::TooltipTitle>,
        ),
    >,
) {
    crate::ui::widgets::tick_tooltip(
        commands,
        time,
        primary_window,
        request,
        overlay_node,
        title_text,
        body_children,
        body_entity_q,
        CONSTRUCTION_CANARY_TOP_PX,
    );
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod formatter_tests {
    use super::*;

    #[test]
    fn format_mining_rate_ladder_lands_in_one_to_three_digits() {
        assert_eq!(format_mining_rate(0.0), "0");
        assert_eq!(format_mining_rate(120.0 * 25.0), "3 Gt/yr");
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
        assert_eq!(format_mining_reserve(1e-3), "1 kt");
    }

    #[test]
    fn format_power_picks_smallest_suffix() {
        assert_eq!(format_power(0.0), "0 W");
        assert_eq!(format_power(0.5), "500 kW");
        assert_eq!(format_power(50.0), "50 MW");
        assert_eq!(format_power(900.0), "900 MW");
        assert_eq!(format_power(250.0), "250 MW");
        assert_eq!(format_power(5000.0), "5 GW");
        assert_eq!(format_power(12_000_000.0), "12 TW");
    }
}

#[cfg(test)]
mod body_blocked_tests {
    use super::*;
    use crate::colony::data::{BuildingDefinition, BuildingModifierDef};
    use crate::colony::types::BuildingCategory;
    use crate::colony::types::BuildingType;
    use crate::colony::AtmosphereKind;
    use crate::plugins::solar_system_data::BodyType;

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
            available_atmospheres: vec![AtmosphereKind::Breathable, AtmosphereKind::None],
            required_anomalies: vec![],
            allowed_body_types: vec![BodyType::Moon, BodyType::GasGiant, BodyType::Asteroid],
        }
    }

    fn iron_mine_def() -> BuildingDefinition {
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
            available_atmospheres: vec![AtmosphereKind::Breathable, AtmosphereKind::None],
            required_anomalies: vec![],
            allowed_body_types: vec![],
        }
    }

    #[test]
    fn he3_mine_on_earth_is_body_blocked() {
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
        assert!(data.body_blocked);
        assert!(data.power_insufficient);
    }

    #[test]
    fn he3_mine_on_moon_is_body_available() {
        let def = he3_mine_def();
        let data = build_mine_card_data(
            BuildingType::He3Mine,
            &def,
            0,
            false,
            Some(BodyType::Moon),
            None,
            1,
            f64::NAN,
        );
        assert!(!data.body_blocked);
    }

    #[test]
    fn iron_mine_on_earth_is_body_available() {
        let def = iron_mine_def();
        let data = build_mine_card_data(
            BuildingType::IronMine,
            &def,
            0,
            true,
            Some(BodyType::Planet),
            None,
            1,
            f64::NAN,
        );
        assert!(!data.body_blocked);
    }

    #[test]
    fn body_blocked_skips_per_frame_affordability_system() {
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
        assert!(data.body_blocked);
        assert!(data.power_insufficient);
    }

    #[test]
    fn mine_power_demand_gates_cta_when_grid_short() {
        let def = iron_mine_def();
        let data = build_mine_card_data(
            BuildingType::IronMine,
            &def,
            0,
            true,
            Some(BodyType::Planet),
            None,
            5,
            100.0,
        );
        assert!(data.power_insufficient);
        assert!(data.power_chip.insufficient);
        assert!(data.power_chip.spare_mw.is_some());

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
        assert!(!ok.power_insufficient);
        assert!(!ok.power_chip.insufficient);

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
        assert!(!no_colony.power_insufficient);
    }

    #[test]
    fn constructed_card_aggregates_power_and_has_no_cta() {
        let def = iron_mine_def();
        let data = build_constructed_card_data(BuildingType::IronMine, &def, 3, 5);
        assert!(data.constructed);
        assert!(data.power_chip.per_unit_mw < 0.0);
        assert_eq!(data.power_chip.amount, "150 MW");
        assert_eq!(data.stat_b.1, "");
        assert_eq!(data.stat_a.1, "\u{00d7}3");

        let mut gen_def = iron_mine_def();
        gen_def.modifiers = vec![BuildingModifierDef {
            modifier_type: "PowerGeneration".to_string(),
            value: 2.0,
        }];
        let gen = build_constructed_card_data(BuildingType::IronMine, &gen_def, 4, 1);
        assert!(gen.power_chip.per_unit_mw > 0.0);
        assert_eq!(gen.power_chip.amount, "8 GW");
    }
}

#[cfg(test)]
mod friendly_label_tests {
    use super::*;
    use crate::colony::data::BuildingModifierDef;

    fn modf(ty: &str, v: f64) -> BuildingModifierDef {
        BuildingModifierDef {
            modifier_type: ty.to_string(),
            value: v,
        }
    }

    #[test]
    fn friendly_label_iron_production() {
        let m = modf("IronProduction", 120.0);
        let (tone, label) = friendly_label(&m).expect("IronProduction should produce a label");
        assert_eq!(tone, EffectTone::Positive);
        assert!(label.contains("Iron"));
        assert!(label.contains("120"));
    }

    #[test]
    fn friendly_label_water_production() {
        let m = modf("WaterProduction", 16.0);
        let (tone, label) = friendly_label(&m).expect("WaterProduction should produce a label");
        assert_eq!(tone, EffectTone::Positive);
        assert!(label.contains("Water"));
    }

    #[test]
    fn friendly_label_gold_production() {
        let m = modf("GoldProduction", 0.0001);
        let (tone, label) = friendly_label(&m).expect("GoldProduction should produce a label");
        assert_eq!(tone, EffectTone::Positive);
        assert!(label.contains("Gold"));
    }

    #[test]
    fn friendly_label_housing_capacity_arcology_4b() {
        let m = modf("HousingCapacity", 4_000_000_000.0);
        let (tone, label) = friendly_label(&m).expect("HousingCapacity should produce a label");
        assert_eq!(tone, EffectTone::Positive);
        assert!(label.contains("residents"));
        assert!(label.contains("4.00B"));
    }

    #[test]
    fn friendly_label_housing_capacity_metropolitan_50m() {
        let m = modf("HousingCapacity", 50_000_000.0);
        let (tone, label) = friendly_label(&m).expect("HousingCapacity should produce a label");
        assert_eq!(tone, EffectTone::Positive);
        assert!(label.contains("50.00M"));
    }

    #[test]
    fn friendly_label_housing_capacity_metropolitan_25m() {
        let m = modf("HousingCapacity", 25_000_000.0);
        let (tone, label) = friendly_label(&m).expect("HousingCapacity should produce a label");
        assert_eq!(tone, EffectTone::Positive);
        assert!(label.contains("25.00M"));
    }

    #[test]
    fn friendly_label_housing_capacity_metropolitan_30m() {
        let m = modf("HousingCapacity", 30_000_000.0);
        let (tone, label) = friendly_label(&m).expect("HousingCapacity should produce a label");
        assert_eq!(tone, EffectTone::Positive);
        assert!(label.contains("30.00M"));
    }

    #[test]
    fn friendly_label_housing_capacity_starter_10k() {
        let m = modf("HousingCapacity", 10_000.0);
        let (tone, label) = friendly_label(&m).expect("HousingCapacity should produce a label");
        assert_eq!(tone, EffectTone::Positive);
        assert!(label.contains("10.0k"));
    }

    #[test]
    fn friendly_label_housing_capacity_starter_1k() {
        let m = modf("HousingCapacity", 1_000.0);
        let (tone, label) = friendly_label(&m).expect("HousingCapacity should produce a label");
        assert_eq!(tone, EffectTone::Positive);
        assert!(label.contains("1.0k"));
    }

    #[test]
    fn friendly_label_nitrogen_harvesting() {
        let m = modf("NitrogenHarvesting", 7.0);
        let (tone, label) = friendly_label(&m).expect("NitrogenHarvesting should produce a label");
        assert_eq!(tone, EffectTone::Positive);
        assert!(label.contains("N\u{2082}"));
    }

    #[test]
    fn friendly_label_plutonium_breeding() {
        let m = modf("PlutoniumBreeding", 0.23);
        let (tone, label) = friendly_label(&m).expect("PlutoniumBreeding should produce a label");
        assert_eq!(tone, EffectTone::Positive);
        assert!(label.contains("Plutonium"));
    }

    #[test]
    fn friendly_label_build_points_production() {
        let m = modf("BuildPointsProduction", 10.0);
        let (tone, label) =
            friendly_label(&m).expect("BuildPointsProduction should produce a label");
        assert_eq!(tone, EffectTone::Positive);
        assert_eq!(label, "Builds +10 BP/yr");
    }

    #[test]
    fn friendly_label_construction_cost_negative() {
        let m = modf("ConstructionCost", -200.0);
        let (tone, label) = friendly_label(&m).expect("ConstructionCost<0 should produce a label");
        assert_eq!(tone, EffectTone::Positive);
        assert_eq!(label, "Builds 200 BP/yr faster");
    }

    #[test]
    fn friendly_label_construction_cost_positive() {
        let m = modf("ConstructionCost", 300.0);
        let (tone, label) = friendly_label(&m).expect("ConstructionCost>0 should produce a label");
        assert_eq!(tone, EffectTone::Neutral);
        assert_eq!(label, "Construction cost +300 BP/build");
    }

    #[test]
    fn friendly_label_unknown_returns_none() {
        let m = modf("QuantumFlux", 42.0);
        assert!(friendly_label(&m).is_none());
    }

    #[test]
    fn friendly_label_zero_production_returns_none() {
        let m = modf("IronProduction", 0.0);
        assert!(friendly_label(&m).is_none());
    }

    #[test]
    fn apply_effect_cap_truncates_to_5_plus_more() {
        let mut effects: Vec<(EffectTone, String)> = (0..7)
            .map(|i| (EffectTone::Positive, format!("e{}", i)))
            .collect();
        apply_effect_cap(&mut effects);
        assert_eq!(effects.len(), 6);
        assert_eq!(effects[5].1, "+2 more");
    }

    #[test]
    fn apply_effect_cap_at_5_no_indicator() {
        let mut effects: Vec<(EffectTone, String)> = (0..5)
            .map(|i| (EffectTone::Positive, format!("e{}", i)))
            .collect();
        apply_effect_cap(&mut effects);
        assert_eq!(effects.len(), 5);
    }

    #[test]
    fn friendly_label_deuterium_production() {
        let m = modf("DeuteriumProduction", 0.5);
        let (tone, label) = friendly_label(&m).expect("DeuteriumProduction should produce a label");
        assert_eq!(tone, EffectTone::Positive);
        assert!(label.contains("Deuterium"));
        assert!(label.contains("Produces"));
    }

    #[test]
    fn friendly_label_tritium_breeding() {
        let m = modf("TritiumBreeding", 0.05);
        let (tone, label) = friendly_label(&m).expect("TritiumBreeding should produce a label");
        assert_eq!(tone, EffectTone::Positive);
        assert!(label.contains("Tritium"));
        assert!(label.contains("Breeds"));
    }

    #[test]
    fn friendly_label_helium3_production() {
        let m = modf("Helium3Production", 0.5);
        let (tone, label) = friendly_label(&m).expect("Helium3Production should produce a label");
        assert_eq!(tone, EffectTone::Positive);
        assert!(label.contains("Helium3"));
    }

    #[test]
    fn friendly_label_argon_production() {
        let m = modf("ArgonProduction", 0.028);
        let (tone, label) = friendly_label(&m).expect("ArgonProduction should produce a label");
        assert_eq!(tone, EffectTone::Positive);
        assert!(label.contains("Argon"));
    }

    #[test]
    fn friendly_label_multi_output_iterates_all() {
        let def_mods = [
            modf("IronProduction", 120.0),
            modf("HousingCapacity", 50_000_000.0),
            modf("PlutoniumBreeding", 0.23),
            modf("NitrogenHarvesting", 7.0),
            modf("DeuteriumProduction", 0.5),
            modf("BuildPointsProduction", 10.0),
            modf("ConstructionCost", -200.0),
        ];
        let mut effects: Vec<(EffectTone, String)> = Vec::new();
        for m in def_mods.iter() {
            if let Some((tone, label)) = friendly_label(m) {
                effects.push((tone, label));
            }
        }
        assert_eq!(effects.len(), 7);
        apply_effect_cap(&mut effects);
        assert_eq!(effects.len(), 6);
        assert_eq!(effects[5].0, EffectTone::Neutral);
        assert_eq!(effects[5].1, "+2 more");
    }

    #[test]
    fn friendly_label_hydrogen_synthesis() {
        let m = modf("HydrogenSynthesis", 0.143);
        let (tone, label) = friendly_label(&m).expect("HydrogenSynthesis should produce a label");
        assert_eq!(tone, EffectTone::Positive);
        assert!(label.contains("Synthesizes"));
        assert!(label.contains("Hydrogen"));
        assert!(!label.contains("Mt/yr Mt/yr"));
    }

    #[test]
    fn friendly_label_ammonia_synthesis() {
        let m = modf("AmmoniaSynthesis", 0.286);
        let (tone, label) = friendly_label(&m).expect("AmmoniaSynthesis should produce a label");
        assert_eq!(tone, EffectTone::Positive);
        assert!(label.contains("Synthesizes"));
        assert!(label.contains("Ammonia"));
    }

    #[test]
    fn friendly_label_polymer_synthesis() {
        let m = modf("PolymerSynthesis", 0.643);
        let (tone, label) = friendly_label(&m).expect("PolymerSynthesis should produce a label");
        assert_eq!(tone, EffectTone::Positive);
        assert!(label.contains("Synthesizes"));
        assert!(label.contains("Polymer"));
    }

    #[test]
    fn friendly_label_research_speed() {
        // v3.10 (GRA-22c Phase 4A): RON value is RP/month per build,
        // not a percent. Old test asserted "Research speed +100%".
        let m = modf("ResearchSpeed", 100.0);
        let (tone, label) = friendly_label(&m).expect("ResearchSpeed should produce a label");
        assert_eq!(tone, EffectTone::Positive);
        assert_eq!(label, "+100 RP/month");
    }

    #[test]
    fn friendly_label_engineering_speed() {
        // v3.10 (GRA-22c Phase 4A): RON value is EP/month per build,
        // not a percent. Old test asserted "Engineering speed +200%".
        let m = modf("EngineeringSpeed", 200.0);
        let (tone, label) = friendly_label(&m).expect("EngineeringSpeed should produce a label");
        assert_eq!(tone, EffectTone::Positive);
        assert_eq!(label, "+200 EP/month");
    }

    #[test]
    fn friendly_label_population_growth() {
        // v3.9 (GRA-22c Phase 1.3): the RON value is a raw fraction per
        // build per year. `MedicalCenter`'s current RON value is 0.0003
        // (≈ +0.03%/yr per center), matching the underlying sim rate.
        // The previously-asserted "+0.5%/yr" was a 16.7× over-statement
        // caused by the RON value being interpreted as basis points.
        let m = modf("PopulationGrowth", 0.0003);
        let (tone, label) = friendly_label(&m).expect("PopulationGrowth should produce a label");
        assert_eq!(tone, EffectTone::Positive);
        assert_eq!(label, "Population growth +0.030%/yr");
    }

    #[test]
    fn friendly_label_wealth_generation() {
        let m = modf("WealthGeneration", 500.0);
        let (tone, label) = friendly_label(&m).expect("WealthGeneration should produce a label");
        assert_eq!(tone, EffectTone::Positive);
        assert_eq!(label, "Generates 500 MC/yr");
    }

    #[test]
    fn friendly_label_logistics_capacity() {
        let m = modf("LogisticsCapacity", 20_000.0);
        let (tone, label) = friendly_label(&m).expect("LogisticsCapacity should produce a label");
        assert_eq!(tone, EffectTone::Positive);
        assert_eq!(label, "Logistics capacity 20000 t/yr");
    }

    #[test]
    fn friendly_label_storage_capacity() {
        // v3.10 (GRA-22c Phase 4B): the modifier value is now
        // Mt per depot (additive), not a percent. The RON
        // currently stores 5000.0 (one depot = +5,000 Mt).
        let m = modf("StorageCapacity", 5000.0);
        let (tone, label) = friendly_label(&m).expect("StorageCapacity should produce a label");
        assert_eq!(tone, EffectTone::Positive);
        assert_eq!(label, "+5000 Mt to stockpile caps (all resources)");
    }

    #[test]
    fn friendly_label_breeding_no_double_unit() {
        let m = modf("TritiumBreeding", 0.05);
        let (tone, label) = friendly_label(&m).expect("TritiumBreeding should produce a label");
        assert_eq!(tone, EffectTone::Positive);
        assert!(!label.contains("Mt/yr Mt/yr"));
        assert!(label.contains("Tritium"));
        assert_eq!(label, "Breeds 50 kt/yr Tritium");
    }

    #[test]
    fn friendly_label_chemical_plant_all_outputs() {
        let def_mods = [
            modf("HydrogenSynthesis", 0.143),
            modf("AmmoniaSynthesis", 0.286),
            modf("PolymerSynthesis", 0.643),
            modf("TritiumBreeding", 0.05),
        ];
        let mut labels = Vec::new();
        for m in def_mods.iter() {
            if let Some((_, label)) = friendly_label(m) {
                labels.push(label);
            }
        }
        assert_eq!(labels.len(), 4);
        assert!(labels.iter().any(|l| l.contains("Hydrogen")));
        assert!(labels.iter().any(|l| l.contains("Ammonia")));
        assert!(labels.iter().any(|l| l.contains("Polymer")));
        assert!(labels.iter().any(|l| l.contains("Tritium")));
    }
}
