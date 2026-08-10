//! Tooltip systems for the construction UI.
//!
//! Two flavours of tooltip:
//! - Cursor-following hover overlays for resource-cost chips,
//!   power chips, and the Queue CTA (multi-line "Missing:" payload)
//! - The legacy bottom-left text tooltip (single-line, mirrors the
//!   cursor-following version)

use bevy::picking::events::{Out, Over, Pointer};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::ui::bevy_theme::*;
use super::data::{format_mining_reserve, format_power};
use super::markers::*;
use super::state::*;
use crate::ui::widgets::{ActiveChips, ChipGroup};
use crate::game_state::{ActiveMenu, GameMenu};

// Observer: on `Pointer<Over>`, snapshot the hovered chip's
// `ResourceCostChip` data into [`ResourceCostHoverState`].
// The `update_resource_cost_tooltip` system reads that
// resource each frame to populate the singleton overlay.
//
// The observer doesn't touch the overlay entity directly
// — pointer observers shouldn't mutate other entities'
// per-frame state. The system handles the visible position
// + text + colour work because the overlay's `left/top`
// must be set from the live cursor position, which the
// observer doesn't have.
pub fn on_chip_hover_over(
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
pub fn on_chip_hover_out(on: On<Pointer<Out>>, mut hover_state: ResMut<ResourceCostHoverState>) {
    if let Some(current) = &hover_state.chip {
        if current.entity == on.entity {
            hover_state.chip = None;
        }
    }
}

// ── Power chip tooltip (v0.5.2 PR-A.7, 2026-08-04) ──────────────

// Observer: on `Pointer<Over>`, snapshot the hovered power
// chip's `PowerChip` data into [`PowerChipHoverState`].
pub fn on_power_chip_hover_over(
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
// cursor left the chip we're currently tracking.
pub fn on_power_chip_hover_out(
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
//   frames, or when the construction menu isn't the active
//   menu, OR
// - positions the overlay next to the cursor (4 px below
//   the cursor vertically, 8 px right horizontally) and
//   populates the text with `"<name>  <amount>"`.
pub fn update_resource_cost_tooltip(
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
    // local coords.
    const CANARY_ROOT_TOP_PX: f32 = 126.0;

    // Hide the overlay whenever the construction canary isn't
    // the active menu.
    let construction_menu_active = matches!(active_menu.current, GameMenu::Construction);
    if !construction_menu_active {
        overlay_node.display = Display::None;
        if hover_state.chip.is_some() {
            hover_state.chip = None;
        }
        return;
    }

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

    const TOOLTIP_W: f32 = 240.0;
    const TOOLTIP_H: f32 = 48.0;
    let root_width = (window.width() - 0.0).max(TOOLTIP_W);
    let root_height = (window.height() - CANARY_ROOT_TOP_PX - 72.0).max(TOOLTIP_H);
    let max_left = (root_width - TOOLTIP_W).max(0.0);
    let max_top = (root_height - TOOLTIP_H).max(0.0);
    overlay_node.left = Val::Px(local_x.clamp(0.0, max_left));
    overlay_node.top = Val::Px(local_y.clamp(0.0, max_top));
    overlay_node.display = Display::Flex;

    *tooltip_text = Text::new(format!("{}  {}", data.name, data.amount));
    *tooltip_color = TextColor(data.category);
}

// Per-frame driver for the **power**-chip hover overlay.
pub fn update_power_chip_tooltip(
    active_menu: Res<ActiveMenu>,
    primary_window: Query<&Window, With<PrimaryWindow>>,
    chip_query: Query<&PowerChip>,
    mut hover_state: ResMut<PowerChipHoverState>,
    mut overlay_node: Single<&mut Node, With<PowerChipTooltipOverlay>>,
    tooltip: Single<(&mut Text, &mut TextColor), With<PowerChipTooltipText>>,
) {
    let (mut tooltip_text, mut tooltip_color) = tooltip.into_inner();

    const CANARY_ROOT_TOP_PX: f32 = 126.0;
    const TOOLTIP_W: f32 = 280.0;
    const TOOLTIP_H: f32 = 80.0;

    let construction_menu_active = matches!(active_menu.current, GameMenu::Construction);
    if !construction_menu_active {
        overlay_node.display = Display::None;
        if hover_state.chip.is_some() {
            hover_state.chip = None;
        }
        return;
    }

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
    let local_x = local_x + 8.0;

    let root_width = (window.width() - 0.0).max(TOOLTIP_W);
    let root_height = (window.height() - CANARY_ROOT_TOP_PX - 72.0).max(TOOLTIP_H);
    let max_left = (root_width - TOOLTIP_W).max(0.0);
    let max_top = (root_height - TOOLTIP_H).max(0.0);
    overlay_node.left = Val::Px(local_x.clamp(0.0, max_left));
    overlay_node.top = Val::Px(local_y.clamp(0.0, max_top));
    overlay_node.display = Display::Flex;

    *tooltip_text = Text::new(data.tooltip_lines.join("\n"));
    *tooltip_color = TextColor(data.tone);
}

// Helper: enumerate every local-stockpile shortfall for the
// given `costs × multiplier`, sorted by *missing* amount
// descending (so the most-binding resource is first).
pub(super) fn collect_local_shortfalls(
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
    all.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    all
}

// Update the canary's hover tooltip text every frame.
//
// Scans every CTA's `Interaction` and `ConstructionCtaDisabled`
// state. If the player is hovering a disabled CTA, populates
// `ConstructionTooltipState` with the most-binding constraint.
pub fn tick_construction_tooltip(
    ctas: Query<(
        Entity,
        &Interaction,
        &ConstructionCta,
        Has<ConstructionCtaDisabled>,
        Has<ConstructionCtaBodyBlocked>,
    )>,
    buildings_data: Res<crate::colony::data::BuildingsData>,
    local_stockpiles: Query<&crate::economy::LocalStockpile>,
    colonies: Query<&crate::colony::Colony>,
    ui_state: Res<ConstructionUiState>,
    mut tooltip: ResMut<ConstructionTooltipState>,
    mut queue_tooltip: ResMut<QueueButtonTooltipState>,
) {
    use crate::economy::budget::calculate_colony_power_totals;

    let multiplier = ui_state.build_multiplier.max(1);

    let active_colony_entity = ui_state.selected_colony;
    let spare_power_mw: f64 = active_colony_entity
        .and_then(|e| colonies.get(e).ok())
        .map(|colony| {
            let totals = calculate_colony_power_totals(colony, Some(&buildings_data));
            (totals.produced_watts - totals.consumed_watts) / 1_000_000.0
        })
        .unwrap_or(0.0);
    let local = active_colony_entity.and_then(|e| local_stockpiles.get(e).ok());

    let mut best: Option<String> = None;
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
            continue;
        }
        if !is_disabled {
            continue;
        }

        let mut lines: Vec<String> = Vec::new();
        lines.push("Missing:".to_string());

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

        if def.power_demand_mw > 0.0 && spare_power_mw > 0.0 {
            let total_demand = def.power_demand_mw * multiplier as f64;
            if total_demand > spare_power_mw {
                let deficit_mw = total_demand - spare_power_mw;
                lines.push(format!("  Power {}", format_power(deficit_mw)));
            }
        }

        if lines.len() > 1 {
            best = Some(lines.join("\n"));
            break;
        }
    }
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

    if let Some(entity) = queue_tooltip_entity {
        queue_tooltip.hovered_cta = Some(entity);
        queue_tooltip.lines = queue_tooltip_lines;
    } else if let Some(text) = best_text {
        queue_tooltip.hovered_cta = None;
        queue_tooltip.lines = text.split('\n').map(|s| s.to_string()).collect();
    } else {
        queue_tooltip.hovered_cta = None;
        queue_tooltip.lines.clear();
    }
}

// Mirror `ConstructionTooltipState` to the on-screen tooltip Text
// node + its visibility every frame.
pub fn update_construction_tooltip(
    state: Res<ConstructionTooltipState>,
    mut tooltip_query: Query<(&mut Text, &mut Visibility), With<ConstructionTooltipText>>,
) {
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
// **cursor-following** Queue-CTA tooltip.
pub fn update_queue_button_tooltip(
    active_menu: Res<ActiveMenu>,
    primary_window: Query<&Window, With<PrimaryWindow>>,
    cta_query: Query<Entity, With<ConstructionCta>>,
    queue_state: Res<QueueButtonTooltipState>,
    mut overlay_node: Single<&mut Node, With<QueueButtonTooltipOverlay>>,
    tooltip: Single<(&mut Text, &mut Visibility), With<QueueButtonTooltipText>>,
) {
    let (mut tooltip_text, mut tooltip_visibility) = tooltip.into_inner();

    const CANARY_ROOT_TOP_PX: f32 = 126.0;
    const TOOLTIP_W: f32 = 260.0;
    const TOOLTIP_H: f32 = 110.0;

    let construction_menu_active = matches!(active_menu.current, GameMenu::Construction);
    if !construction_menu_active {
        overlay_node.display = Display::None;
        return;
    }

    let cta_alive = queue_state
        .hovered_cta
        .map(|e| cta_query.get(e).is_ok())
        .unwrap_or(true);
    if !queue_state.lines.is_empty() && !cta_alive {
        overlay_node.display = Display::None;
        return;
    }
    if queue_state.lines.is_empty() {
        overlay_node.display = Display::None;
        *tooltip_visibility = Visibility::Hidden;
        return;
    }

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

    let root_width = window.width().max(TOOLTIP_W);
    let root_height = (window.height() - CANARY_ROOT_TOP_PX - 72.0).max(TOOLTIP_H);
    let max_left = (root_width - TOOLTIP_W).max(0.0);
    let max_top = (root_height - TOOLTIP_H).max(0.0);
    overlay_node.left = Val::Px(local_x.clamp(0.0, max_left));
    overlay_node.top = Val::Px(local_y.clamp(0.0, max_top));
    overlay_node.display = Display::Flex;
    *tooltip_visibility = Visibility::Inherited;

    *tooltip_text = Text::new(queue_state.lines.join("\n"));
}

// Click handler: when a chip in the Build sub-tab is pressed, mutate
// `ConstructionUiState` accordingly. The chip's `ChipKind` component
// tells us what to do (set qty, set filter, set category, etc.).
pub fn tick_construction_chip_click(
    interactions: Query<(Entity, &Interaction, &ChipGroup), With<Button>>,
    mut ui_state: ResMut<ConstructionUiState>,
    mut active: ResMut<ActiveChips>,
    mut prev: Local<std::collections::HashMap<Entity, Interaction>>,
) {
    let mut current: std::collections::HashMap<Entity, Interaction> =
        std::collections::HashMap::new();
    for (entity, interaction, kind) in interactions.iter() {
        let prev_interaction = prev.get(&entity).copied().unwrap_or(Interaction::None);
        if *interaction == Interaction::Pressed && prev_interaction != Interaction::Pressed {
            match kind {
                ChipGroup::Tab(idx) => {
                    ui_state.selected_tab = match idx {
                        0 => ConstructionTab::Overview,
                        1 => ConstructionTab::Buildings,
                        2 => ConstructionTab::Build,
                        _ => ConstructionTab::Mining,
                    };
                    active.tab = *idx;
                }
                ChipGroup::Qty(n) => {
                    ui_state.build_multiplier = *n;
                    active.qty = *n;
                }
                ChipGroup::Category(idx) => {
                    ui_state.selected_build_tab = *idx;
                    active.category = *idx;
                }
            }
        }
        current.insert(entity, *interaction);
    }
    *prev = current;
}
