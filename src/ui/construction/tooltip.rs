//! Tooltip systems for the construction UI.
//!
//! Phase 4 (2026-08-10): ported off the four per-overlay hover-state
//! resources (`ResourceCostHoverState`, `PowerChipHoverState`,
//! `QueueButtonTooltipState`, `ConstructionTooltipState`) and the four
//! per-frame driver systems onto the generic
//! [`crate::ui::widgets::TooltipRequest`] /
//! [`crate::ui::widgets::tick_tooltip`] primitive. All four tooltip
//! surfaces (ResourceCostChip, PowerChip, Queue CTA, disabled CTA)
//! now route through a single overlay that
//! [`crate::ui::widgets::tick_tooltip`] positions at the cursor.

use bevy::picking::events::{Out, Over, Pointer};
use bevy::prelude::*;

use crate::ui::bevy_theme::*;
use super::data::{format_mining_reserve, format_power};
use super::markers::*;
use super::state::*;
use crate::ui::widgets::{
    ChipGroup, TooltipBody, TooltipContent, TooltipEntry, TooltipOverlay,
    TooltipRequest, TooltipTitle, TooltipTone,
};

// ── Hover observers ───────────────────────────────────────────────

// Observer: on `Pointer<Over>`, snapshot the hovered chip's
// `ResourceCostChip` data into [`TooltipRequest`]. The
// generic `tick_tooltip` system reads that resource each
// frame to populate the singleton overlay.
pub fn on_chip_hover_over(
    on: On<Pointer<Over>>,
    chip_query: Query<&ResourceCostChip>,
    time: Res<Time>,
    mut request: ResMut<TooltipRequest>,
) {
    let Ok(chip) = chip_query.get(on.entity) else {
        return;
    };
    request.content = Some(TooltipContent {
        title: chip.name.clone(),
        entries: vec![TooltipEntry::Stat {
            label: chip.name.clone(),
            value: chip.amount.clone(),
            tone: tone_for_chip_color(chip.category),
        }],
    });
    request.hover_started_at = Some(time.elapsed_secs());
}

// Observer: on `Pointer<Out>`, clear the request iff the request
// currently reflects the chip the cursor just left. We check the
// request's title against the chip's display name (cheap `String`
// compare) so a stale Out from a sibling chip doesn't drop a
// freshly-set tooltip.
pub fn on_chip_hover_out(
    on: On<Pointer<Out>>,
    chip_query: Query<&ResourceCostChip>,
    mut request: ResMut<TooltipRequest>,
) {
    let Ok(chip) = chip_query.get(on.entity) else {
        return;
    };
    let should_clear = request
        .content
        .as_ref()
        .map(|c| c.title == chip.name)
        .unwrap_or(false);
    if should_clear {
        request.content = None;
        request.hover_started_at = None;
    }
}

// ── Power chip tooltip ────────────────────────────────────────────

// Observer: on `Pointer<Over>`, snapshot the hovered power
// chip's `PowerChip` data into [`TooltipRequest`].
pub fn on_power_chip_hover_over(
    on: On<Pointer<Over>>,
    chip_query: Query<&PowerChip>,
    time: Res<Time>,
    mut request: ResMut<TooltipRequest>,
) {
    let Ok(chip) = chip_query.get(on.entity) else {
        return;
    };
    // Build a single Stat entry carrying the tone for the chip; the
    // body renderer in `populate_tooltip_body` respects `Stat::tone`
    // when picking the value's text colour.
    let mut entries: Vec<TooltipEntry> = Vec::new();
    if let Some((first, rest)) = chip.tooltip_lines.split_first() {
        // First line becomes the Stat row's label, the rest of the
        // lines are Paragraph rows beneath it (rendered muted). The
        // tone from the chip is applied to the Stat value.
        let value = rest.first().cloned().unwrap_or_default();
        entries.push(TooltipEntry::Stat {
            label: first.clone(),
            value,
            tone: tone_for_chip_color(chip.tone),
        });
        for line in rest.iter().skip(1) {
            entries.push(TooltipEntry::Paragraph(line.clone()));
        }
    }
    let title = chip
        .tooltip_lines
        .first()
        .cloned()
        .unwrap_or_else(|| "Power".to_string());
    request.content = Some(TooltipContent { title, entries });
    request.hover_started_at = Some(time.elapsed_secs());
}

// Observer: on `Pointer<Out>`, clear the request iff the request
// currently reflects the chip the cursor just left.
pub fn on_power_chip_hover_out(
    on: On<Pointer<Out>>,
    chip_query: Query<&PowerChip>,
    mut request: ResMut<TooltipRequest>,
) {
    let Ok(chip) = chip_query.get(on.entity) else {
        return;
    };
    let should_clear = request
        .content
        .as_ref()
        .map(|c| c.title == chip.tooltip_lines.first().cloned().unwrap_or_default())
        .unwrap_or(false);
    if should_clear {
        request.content = None;
        request.hover_started_at = None;
    }
}

// ── Helpers ────────────────────────────────────────────────────────

// Map a chip's raw `bevy::Color` to the nearest [`TooltipTone`].
// ResourceCostChip uses the chip's category tint; PowerChip uses
// GREEN_FIN/ORANGE_ORE/TEXT_BODY depending on whether demand fits.
pub(crate) fn tone_for_chip_color(color: Color) -> TooltipTone {
    if approx_eq(color, GREEN_FIN) {
        TooltipTone::Positive
    } else if approx_eq(color, ORANGE_ORE) {
        TooltipTone::Warning
    } else if approx_eq(color, TEXT_BODY) {
        TooltipTone::Neutral
    } else {
        // Chip category tints (construction / volatiles / fissile /
        // etc.). The tooltip body uses the Accent tone so it pops
        // against the dark background regardless of which chip it
        // belongs to. The original `update_resource_cost_tooltip`
        // applied the chip's raw colour to the text node; the
        // generic `tick_tooltip` only honours `TooltipTone` buckets,
        // and Accent is the closest match for an arbitrary chip
        // category tint.
        TooltipTone::Accent
    }
}

fn approx_eq(a: Color, b: Color) -> bool {
    const EPS: f32 = 0.05;
    let ar = a.to_srgba().to_f32_array();
    let br = b.to_srgba().to_f32_array();
    (ar[0] - br[0]).abs() < EPS
        && (ar[1] - br[1]).abs() < EPS
        && (ar[2] - br[2]).abs() < EPS
}

// ── Overlay spawn helper ──────────────────────────────────────────

// Spawn the singleton cursor-following tooltip overlay tree as a
// child of the construction canary root. The overlay is a Node
// carrying `TooltipOverlay`; its title (`TooltipTitle`) and body
// (`TooltipBody`) are siblings under the overlay. The body holds
// dynamic children re-spawned by [`crate::ui::widgets::tick_tooltip`]
// on every content change.
//
// Called once at startup from `setup_construction`. Must NOT be
// called per-frame — the overlay is a singleton, and re-spawning it
// would orphan the in-flight hover latency tracking.
pub fn spawn_construction_tooltip_overlay(
    commands: &mut Commands,
    parent: Entity,
    body_font: Handle<Font>,
) {
    let overlay = commands
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
            TooltipOverlay,
            Name::new("construction_tooltip_overlay"),
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new(String::new()),
                TextFont {
                    font: body_font.clone(),
                    font_size: 12.0,
                    ..default()
                },
                TextColor(TOOLTIP_TITLE_FALLBACK),
                TooltipTitle,
                Name::new("construction_tooltip_title"),
            ));
            parent.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(2.0),
                    ..default()
                },
                TooltipBody,
                Name::new("construction_tooltip_body"),
            ));
        })
        .id();
    commands.entity(parent).add_child(overlay);
}

/// Title text colour — light cyan. Matches the shipbuilding native
/// tooltip's title treatment; the body's tone-coloured entries are
/// rendered below this row.
const TOOLTIP_TITLE_FALLBACK: Color = Color::srgba(0.55, 0.95, 1.0, 1.0);

// ── Per-CTA scan (disabled CTA / Queue CTA tooltip) ────────────────

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
// Scans every CTA's `Interaction` and `ConstructionCtaDisabled` /
// `ConstructionCtaBodyBlocked` state. If the player is hovering a
// disabled CTA, populates [`TooltipRequest`] with the most-binding
// constraint (resource shortfalls + power deficit, in that order).
// Queue-CTA body-blocked CTAs take precedence over resource
// shortfalls so the most-blocking constraint wins.
//
// Merges the legacy "ConstructionTooltipState" (bottom-left
// text mirror) and "QueueButtonTooltipState" (cursor-following
// Missing: payload) into a single [`TooltipRequest`]. The
// [`crate::ui::widgets::tick_tooltip`] driver positions the
// shared overlay next to the cursor and renders the merged
// content.
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
    time: Res<Time>,
    mut request: ResMut<TooltipRequest>,
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

    let mut best: Option<Vec<String>> = None;
    for (_entity, interaction, cta, is_disabled, is_body_blocked) in ctas.iter() {
        if !matches!(interaction, Interaction::Hovered | Interaction::Pressed) {
            continue;
        }
        let def = match buildings_data.get(&cta.building_type) {
            Some(d) => d,
            None => continue,
        };
        if is_body_blocked {
            best = Some(vec![
                "Missing:".to_string(),
                "  Body unavailable".to_string(),
            ]);
            break;
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
            best = Some(lines);
            break;
        }
    }

    if let Some(lines) = best {
        let title = "Missing resources".to_string();
        let entries: Vec<TooltipEntry> = lines
            .iter()
            .map(|line| TooltipEntry::Paragraph(line.clone()))
            .collect();
        request.content = Some(TooltipContent { title, entries });
        request.hover_started_at = Some(time.elapsed_secs());
    } else if request
        .content
        .as_ref()
        .map(|c| c.title == "Missing resources")
        .unwrap_or(false)
    {
        // No CTA hovered AND the previous tooltip was ours — clear it
        // so a stale "Missing resources" doesn't linger after the
        // cursor leaves the CTA. We DO NOT touch the request when the
        // previous content was set by a chip hover observer (e.g.
        // `ResourceCostChip`, `PowerChip`) because we don't own that
        // tooltip — the corresponding `Pointer<Out>` observer is
        // responsible for clearing it.
        request.content = None;
        request.hover_started_at = None;
    }
}

// ── Click handler (Phase 3: uses the shared `detect_rising_edges` helper) ─

pub fn tick_construction_chip_click(
    interactions: Query<(Entity, &Interaction, &ChipGroup), With<Button>>,
    mut ui_state: ResMut<ConstructionUiState>,
    mut active: ResMut<crate::ui::widgets::ActiveChips>,
    mut prev: Local<std::collections::HashMap<Entity, Interaction>>,
) {
    crate::ui::widgets::detect_rising_edges(&mut prev, &interactions, |_entity, kind| {
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
    });
}
