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

use super::data::{format_mining_reserve, format_power};
use super::markers::*;
use super::state::*;
use crate::ui::bevy_theme::*;
use crate::ui::widgets::{
    ChipGroup, TooltipBody, TooltipContent, TooltipEntry, TooltipOverlay, TooltipRequest,
    TooltipTitle, TooltipTone,
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
    // Title is empty so the resource name isn't rendered twice.
    // The body's `Stat` row shows `<name>: <amount>` in the chip's
    // category tone — that single line is the full tooltip payload.
    request.content = Some(TooltipContent {
        title: String::new(),
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
// body's first-entry value against the chip's `amount` because the
// title is intentionally empty for ResourceCostChip (the title
// would otherwise duplicate the body's label). A stale `Out` from
// a sibling chip won't drop a freshly-set tooltip — the new chip's
// `Pointer<Over>`>` runs synchronously before this `Out` fires
// (Bevy's picking guarantees order).
pub fn on_chip_hover_out(
    _on: On<Pointer<Out>>,
    _chip_query: Query<&ResourceCostChip>,
    mut request: ResMut<TooltipRequest>,
) {
    // Clear any active tooltip when the cursor leaves a chip. The
    // chip's own body content (label = name, value = amount) is
    // unique enough to identify it. If a sibling chip's hover has
    // already overwritten the request, that chip's Pointer<Out> will
    // fire on its own cursor exit, not from this observer.
    if request.content.is_some() {
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
    // Render each tooltip line as a Paragraph in the body. The first
    // line ("Power demand: 45 MW") is the title-equivalent — no
    // separate title to avoid the name being shown twice. The body
    // renderer applies the chip's tone to the first line via the
    // paragraph above the rest; we keep it as Paragraph for
    // simplicity and rely on the chip's category colour in the
    // overlay background.
    let entries: Vec<TooltipEntry> = chip
        .tooltip_lines
        .iter()
        .map(|line| TooltipEntry::Paragraph(line.clone()))
        .collect();
    request.content = Some(TooltipContent {
        title: String::new(),
        entries,
    });
    request.hover_started_at = Some(time.elapsed_secs());
}

// Observer: on `Pointer<Out>`, clear the request iff the request
// currently reflects the chip the cursor just left.
pub fn on_power_chip_hover_out(_on: On<Pointer<Out>>, mut request: ResMut<TooltipRequest>) {
    if request.content.is_some() {
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
    (ar[0] - br[0]).abs() < EPS && (ar[1] - br[1]).abs() < EPS && (ar[2] - br[2]).abs() < EPS
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
// `ConstructionCtaBodyBlocked` / `ConstructionCtaCapped` state.
//
// v3.9 (GRA-22c Phase 1.5): cap-gated CTAs (pop-growth at the
// `max_medical_growth_bonus` ceiling) take precedence over the
// "Missing resources" disabled branch because the player's
// question is "why is this dim?" and "you're at the cap" is
// the most actionable answer. Body-blocked (permanent body gate)
// keeps its own precedence. Affordability-blocked (transient,
// usually fixable by waiting for mining) sorts below.
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
        Has<ConstructionCtaCapped>,
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

    // Compute once: the colony's raw pop-growth sum and the cap. Used
    // for the cap-gated CTA tooltip's marginal-benefit detail line.
    let pop_growth_cap = buildings_data.colony_constants.max_medical_growth_bonus;
    let pop_growth_raw: f64 = active_colony_entity
        .and_then(|e| colonies.get(e).ok())
        .map(|colony| {
            colony
                .buildings
                .iter()
                .map(|(bt, count)| buildings_data.population_growth_for(*bt) * *count as f64)
                .sum()
        })
        .unwrap_or(0.0);

    // Three bracketed "best" payloads, ordered by precedence: cap >
    // body > affordability. We keep three separate `Option`s to
    // avoid tuple gymnastics in the inner loop.
    let mut cap_best: Option<Vec<String>> = None;
    let mut body_best: Option<Vec<String>> = None;
    let mut afford_best: Option<Vec<String>> = None;
    for (_entity, interaction, cta, is_disabled, is_body_blocked, is_capped) in ctas.iter() {
        if !matches!(interaction, Interaction::Hovered | Interaction::Pressed) {
            continue;
        }
        let def = match buildings_data.get(&cta.building_type) {
            Some(d) => d,
            None => continue,
        };
        if is_capped {
            // The cap message is identical regardless of which
            // PopGrowth building the player is hovering (it's the
            // colony-wide ceiling, not the per-building delta), so
            // we render a concise two-line body. Compute the
            // marginal benefit ("another copy would add 0%") so
            // the player sees the math.
            let per_build = buildings_data.population_growth_for(cta.building_type);
            // Project the would-be raw sum if the player pressed
            // the button anyway (clamped at the cap). The diff
            // vs. current `pop_growth_raw` is the marginal growth.
            let clamped_now = pop_growth_raw.min(pop_growth_cap);
            let projected = (pop_growth_raw + per_build).min(pop_growth_cap);
            let marginal = projected - clamped_now;
            let marginal_pct = (marginal * 100.0 * 100.0).max(0.0);
            // Display as "+0.000 %/yr" — the precise current
            // semantics. Most cases land on 0.000 because the colony
            // is already saturated.
            cap_best = Some(vec![
                format!(
                    "  Population-growth cap reached ({cap_pct:.3}%/yr)",
                    cap_pct = pop_growth_cap * 100.0
                ),
                format!("  This building adds 0%/yr above the cap."),
                format!(
                    "  Marginal benefit: +{marg_pct:.3}%/yr",
                    marg_pct = marginal_pct
                ),
                "  Demolish an existing facility to free headroom.".to_string(),
            ]);
            // We deliberately don't `break` here: a single CTA can
            // carry both `ConstructionCtaCapped` AND
            // `ConstructionCtaDisabled` (e.g. it's the cap-reached
            // building AND the player can't afford it), but the cap
            // is the more strategic message so we keep iterating
            // and `cap_best` wins via precedence in the final
            // `or()` chain below.
            continue;
        }
        if is_body_blocked {
            body_best = Some(vec![
                "Missing:".to_string(),
                "  Body unavailable".to_string(),
            ]);
            continue;
        }
        if !is_disabled {
            continue;
        }

        let mut lines: Vec<String> = Vec::new();
        // Title is "Missing resources" so the body starts with the
        // first concrete shortfall (no "Missing:" prefix — it would
        // render immediately after the title and look like a typo:
        // "Missing resourcesMissing:").

        if let Some(ls) = local {
            let shortfalls =
                collect_local_shortfalls(ls, def.resource_costs.as_slice(), multiplier);
            const MAX_LINES: usize = 4;
            let show = shortfalls.len().min(MAX_LINES);
            for (name, missing) in shortfalls.iter().take(show) {
                lines.push(format!("  {} {}", name, format_mining_reserve(*missing)));
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
            afford_best = Some(lines);
        }
    }

    // Precedence: cap > body > affordability.
    let is_cap_tooltip = cap_best.is_some();
    let best: Option<Vec<String>> = cap_best.or(body_best).or(afford_best);

    if let Some(lines) = best {
        // Cap-gated CTAs use a dedicated title so the rendered
        // overlay reads "Population-growth cap reached" rather than
        // "Missing resources" — the player needs to see which
        // constraint blocked them, and the cap is a strategic
        // decision (demolish to free headroom) rather than a
        // transient resource wait.
        let title = if is_cap_tooltip {
            "Population-growth cap reached".to_string()
        } else {
            "Missing resources".to_string()
        };
        let entries: Vec<TooltipEntry> = lines
            .iter()
            .map(|line| TooltipEntry::Paragraph(line.clone()))
            .collect();
        let was_empty = request.content.is_none();
        request.content = Some(TooltipContent { title, entries });
        // Only set `hover_started_at` on the first frame the CTA
        // becomes hovered (transition from None → Some). Re-setting
        // it every frame would reset the 250 ms latency gate
        // indefinitely, preventing the overlay from ever appearing.
        if was_empty {
            request.hover_started_at = Some(time.elapsed_secs());
        }
    } else if request
        .content
        .as_ref()
        .map(|c| c.title == "Missing resources" || c.title == "Population-growth cap reached")
        .unwrap_or(false)
    {
        // No CTA hovered AND the previous tooltip was ours — clear it
        // so a stale tooltip doesn't linger after the cursor leaves
        // the CTA. We DO NOT touch the request when the previous
        // content was set by a chip hover observer (e.g.
        // ResourceCostChip, PowerChip) because we don't own that
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
    crate::ui::widgets::detect_rising_edges(&mut prev, &interactions, |_entity, kind| match kind {
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
    });
}
