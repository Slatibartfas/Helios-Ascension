//! Data types and pure data helpers for the construction UI.
//!
//! These types are pure data structures (no systems, no markers).
//! They cross submodule boundaries, so they're re-exported through
//! the parent module.

use bevy::prelude::*;

use crate::colony::data::{parse_resource_type, BuildingDefinition, BuildingModifierDef};
use crate::colony::types::BuildingCategory;
use crate::colony::types::BuildingType;
use crate::economy::ResourceType;

// One row of a building's resource demand: the resource name as
// it appears in `buildings.ron`, the per-unit amount (already
// multiplied by the build quantity), and the parsed
// `ResourceType` (used to look up the icon `Handle<Image>` and
// the category tint). `resource` is `None` when the RON string
// doesn't match a known variant (defensive — the canary falls
// back to a tinted placeholder square + `TEXT_BODY` so a future
// RON addition never panics).
//
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
//
// * `Produces` (net generator) — green.
// * `Demand` (net consumer) — throughput green if the batch
//   fits the active colony's spare grid, negative orange if
//   `power_insufficient` is set, neutral text-body otherwise.
// * `None` (no power interaction) — neutral text-body, shown
//   as `0 W` so the chip still reads as "this building has no
//   power interaction".
//
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
pub fn power_output_gw_per_unit(def: &BuildingDefinition) -> f64 {
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
pub fn build_power_chip_data(
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
//
// v0.5.2 PR-A.2: per user feedback 2026-08-02, the canary now shows
// "per-building demand × multiplier = total vs spare" as the single
// source of truth for power on the build card. The old design had
// three independent power readouts (PWR top stat, "Power:" body
// effect, and the workforce mislabeled as MW) which read as a
// confusing stack of similar numbers.
pub fn compute_colony_spare_power_mw(
    ui_state: &super::state::ConstructionUiState,
    colonies: &Query<(Entity, &crate::colony::Colony)>,
    buildings_data: Option<&crate::colony::data::BuildingsData>,
) -> f64 {
    use crate::economy::budget::calculate_colony_power_totals;

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
    //
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
    //
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
//
// Strips `Production` from the first matching modifier to recover
// the produced `ResourceType`, then reads the deposit's
// `accessibility` and reserves from `PlanetResources::deposits`.
// Returns zeroed data if either side is missing.
pub fn compute_mining_card_data(
    def: &BuildingDefinition,
    planet_resources: Option<&crate::economy::PlanetResources>,
) -> MiningCardData {
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
//
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

    // v3.1 canary 11 (BALANCE_PATCHES_v0.5.md §0.H): replaced the
    // single-modifier `find` with an iterate-all loop. The 13 hidden
    // modifier types (HousingCapacity, NitrogenHarvesting,
    // PlutoniumBreeding, BuildPointsProduction, ConstructionCost,
    // ...) are now surfaced via `friendly_label()`. The
    // `*Production` line still scales with `mult` (per the
    // 2026-08-02 fix); other effects show per-unit values. The
    // 5+1 effect-line cap (per the v0.5.2 PR-A.4 comment at line
    // 1300) is enforced by `apply_effect_cap` after the loop.
    //
    // `BuildPointsProduction` is checked BEFORE the generic
    // `*Production` strip_suffix because it also ends in
    // "Production" — without the special case it would render as
    // "Produces 10 Mt/yr BuildPoints" (wrong). The friendly_label
    // helper knows about it and returns "Builds +10 BP/yr"
    // (per-unit, no batch scaling — the +10 BP/yr is already the
    // single-build rate, and ×N scaling would double-count since
    // the construction system sums across Factories independently;
    // see `src/colony/systems.rs:96`).
    for m in def.modifiers.iter() {
        if m.modifier_type == "BuildPointsProduction" {
            if let Some((tone, label)) = friendly_label(m) {
                effects.push((tone, label));
            }
            continue;
        }
        if let Some(res_name) = m.modifier_type.strip_suffix("Production") {
            if m.value > 0.0 {
                // Production line scales with `mult` (preserves the
                // 2026-08-02 fix: a ×6 build reads as per-unit × N =
                // total, not just per-unit).
                let per_unit = m.value;
                let total = per_unit * mult;
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
                effects.push((EffectTone::Positive, line));
            }
        } else if let Some((tone, label)) = friendly_label(m) {
            effects.push((tone, label));
        }
    }
    apply_effect_cap(&mut effects);

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
pub(super) fn clamp_subtitle_two_lines(s: &str) -> String {
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
//
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
pub fn format_mining_rate(mt_per_year: f64) -> String {
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

// v3.1 canary 11 (BALANCE_PATCHES_v0.5.md §0.H.3): convert a
// building modifier to a (tone, display) pair for the build card.
// Replaces the v0.5.2 single-modifier filter that surfaced only
// the first `*Production` modifier. Now every recognized
// modifier type produces a friendly effect line, and the 5+1
// effect-line cap (per the v0.5.2 PR-A.4 comment at line 1300)
// limits how many appear on the card.
//
// Returns None for unrecognized modifier types — those are
// silently hidden (they exist in the RON but are not surfaced
// on the card; add a case here to surface a new type).
//
// Inventory of modifier types covered (v3.1 §0.H.2 + v3.8.12):
//   * IronProduction, AluminumProduction, CopperMine, ..., WaterProduction, FoodProduction
//   * HousingCapacity
//   * HydrogenSynthesis, AmmoniaSynthesis, PolymerSynthesis (ChemicalPlant)
//   * ResearchSpeed, EngineeringSpeed (ResearchLab, AiCluster, SemiconductorFab, DataCenter)
//   * PopulationGrowth (MedicalCenter, PharmaceuticalPlant, WaterTreatmentPlant, DesalinationPlant)
//   * WealthGeneration (Factory, CommercialHub, FinancialCenter, TradePort)
//   * LogisticsCapacity (MassDriver, OrbitalLift, CargoTerminal)
//   * StorageCapacity (Warehouse, Resource Depot)
//   * NitrogenHarvesting
//   * PlutoniumBreeding, TritiumBreeding
//   * ConstructionCost (negative = "builds faster", positive = "more expensive")
//   * BuildPointsProduction (the Factory's actual effect: +10 BP/yr per build)
//
// See `friendly_label_tests` in the test module below for one
// test per recognized type plus the "+N more" cap test.
pub fn friendly_label(m: &BuildingModifierDef) -> Option<(EffectTone, String)> {
    let ty = m.modifier_type.as_str();
    let v = m.value;

    // Build points. The Factory's actual primary effect: +10 BP/yr per
    // build (the v0.5.0 RON had `ConstructionCost: -200.0` as a legacy
    // fallback from the GRA-22b transition; the Rust at
    // `src/colony/systems.rs:96` reads `factories * 10.0` directly. The
    // RON was renamed to `BuildPointsProduction: 10.0` in canary 11 so
    // the card matches what the code actually does). Checked BEFORE the
    // generic `*Production` strip_suffix because BuildPointsProduction
    // also ends in "Production".
    if ty == "BuildPointsProduction" && v > 0.0 {
        return Some((
            EffectTone::Positive,
            format!("Builds +{} BP/yr", v as i64),
        ));
    }

    // Production modifiers: "IronProduction" → "Iron" + the value as a rate.
    // Multi-output buildings (e.g. ChemicalPlant with H₂ + NH₃ + Polymers
    // + Tritium) produce one line per modifier.
    if let Some(res_name) = ty.strip_suffix("Production") {
        if v > 0.0 {
            return Some((
                EffectTone::Positive,
                format!("Produces {} {}", format_mining_rate(v), res_name),
            ));
        }
        return None;
    }

    // Breeding: "PlutoniumBreeding" → "Plutonium" + the value as a rate.
    if let Some(elem) = ty.strip_suffix("Breeding") {
        if v > 0.0 {
            return Some((
                EffectTone::Positive,
                // v3.8.12: `format_mining_rate` already appends
                // "/yr", so this used to render "Breeds 0.05 Mt/yr
                // Mt/yr Tritium" (double unit). Keep the unit on
                // the rate only.
                format!("Breeds {} {}", format_mining_rate(v), elem),
            ));
        }
        return None;
    }

    // Industrial synthesis: ChemicalPlant's H₂ / NH₃ / polymers
    // ("HydrogenSynthesis" etc.). End in "Synthesis", not
    // "Production", so the generic branch above never saw them —
    // the ChemicalPlant card showed only "Breeds 0.05 Mt/yr
    // Tritium" and hid its three main outputs. The `input_per_output`
    // ratios live in `economy/mining.rs::industrial_process_rule`;
    // here we surface the OUTPUT rate (the card's "effect").
    if let Some(elem) = ty.strip_suffix("Synthesis") {
        if v > 0.0 {
            return Some((
                EffectTone::Positive,
                format!("Synthesizes {} {}", format_mining_rate(v), elem),
            ));
        }
        return None;
    }

    // Research / engineering throughput (ResearchLab, AiCluster,
    // SemiconductorFab, DataCenter). `ResearchSpeed` / `EngineeringSpeed`
    // are additive percent bonuses (see
    // `src/research/systems.rs:research_speed_multiplier`).
    if ty == "ResearchSpeed" && v > 0.0 {
        return Some((
            EffectTone::Positive,
            format!("Research speed +{}%", v as i64),
        ));
    }
    if ty == "EngineeringSpeed" && v > 0.0 {
        return Some((
            EffectTone::Positive,
            format!("Engineering speed +{}%", v as i64),
        ));
    }

    // Population growth: MedicalCenter / PharmaceuticalPlant /
    // WaterTreatmentPlant / DesalinationPlant add a flat bonus
    // (value 50 = +0.5%/yr — see
    // `Colony::population_growth_per_year`, which caps the bonus at
    // `max_medical_growth_bonus`).
    if ty == "PopulationGrowth" && v > 0.0 {
        return Some((
            EffectTone::Positive,
            format!("Population growth +{:.1}%/yr", v / 100.0),
        ));
    }

    // Wealth generation (Factory, CommercialHub, FinancialCenter,
    // TradePort) — Mega-Credits per year.
    if ty == "WealthGeneration" && v > 0.0 {
        return Some((
            EffectTone::Positive,
            format!("Generates {:.0} MC/yr", v),
        ));
    }

    // Logistics throughput (MassDriver / OrbitalLift / CargoTerminal)
    // — tonnes per year surface-to-orbit.
    if ty == "LogisticsCapacity" && v > 0.0 {
        return Some((
            EffectTone::Positive,
            format!("Logistics capacity {:.0} t/yr", v),
        ));
    }

    // Storage capacity (Warehouse / Resource Depot) — +25% per
    // building to ALL global stockpile caps (v3.8.16: raised from
    // +10%; see `economy/budget.rs::update_storage_capacity` and
    // `GlobalBudget::stockpile_cap` for the per-resource base caps).
    if ty == "StorageCapacity" && v > 0.0 {
        return Some((
            EffectTone::Positive,
            format!("Stockpile capacity +{:.0}% (all resources)", v * 100.0),
        ));
    }

    // Atmospheric / harvest modifiers. "NitrogenHarvesting" doesn't end in
    // "Production" so it was hidden by the v0.5.2 filter.
    if ty == "NitrogenHarvesting" && v > 0.0 {
        return Some((
            EffectTone::Positive,
            format!("Harvests {} Mt/yr N\u{2082}", format_mining_rate(v)),
        ));
    }

    // Capacity modifiers.
    if ty == "HousingCapacity" && v > 0.0 {
        return Some((
            EffectTone::Positive,
            format!("Houses {} residents", format_residents(v)),
        ));
    }

    // Legacy ConstructionCost (research-panel modifier, also used as a
    // building modifier pre-v3.1). Negative value = builds faster
    // (positive effect); positive value = more expensive (neutral).
    if ty == "ConstructionCost" {
        if v < 0.0 {
            return Some((
                EffectTone::Positive,
                format!("Builds {} BP/yr faster", (-v) as i64),
            ));
        } else if v > 0.0 {
            return Some((
                EffectTone::Neutral,
                format!("Construction cost +{} BP/build", v as i64),
            ));
        }
    }

    None
}

// v3.1 canary 11 helper: human-readable headcount formatting for
// `HousingCapacity` (e.g. 4,000,000,000 → "4.00B"). Mirrors the
// SI ladder of `format_mining_rate` but for people.
pub fn format_residents(people: f64) -> String {
    let v = people.abs();
    if v < 1.0 {
        return format!("{:.0}", people);
    }
    if v < 1e3 {
        return format!("{:.0}", people);
    }
    if v < 1e6 {
        return format!("{:.1}k", people / 1e3);
    }
    if v < 1e9 {
        return format!("{:.2}M", people / 1e6);
    }
    if v < 1e12 {
        return format!("{:.2}B", people / 1e9);
    }
    format!("{:.2}T", people / 1e12)
}

// v3.1 canary 11 helper: apply the 5+1 effect-line cap. If the
// modifier list produces more than 5 effects, truncate to 5 and
// append a "+N more" indicator. The cap is per the v0.5.2 PR-A.4
// comment at the top of `build_build_card_data` (line 1300-1309).
pub fn apply_effect_cap(
    effects: &mut Vec<(EffectTone, String)>,
) {
    const EFFECT_CAP: usize = 5;
    if effects.len() > EFFECT_CAP {
        let hidden = effects.len() - EFFECT_CAP;
        effects.truncate(EFFECT_CAP);
        effects.push((
            EffectTone::Neutral,
            format!("+{} more", hidden),
        ));
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
        format!("{:.2} Et", total_mt * 1e-12)
    } else if v < 1e18 {
        format!("{:.2} Zt", total_mt * 1e-15)
    } else {
        format!("{:.2} Yt", total_mt * 1e-18)
    }
}

// Format a power value (MW) with a human-readable SI suffix
// ladder (W / kW / MW / GW / TW).
//
// v0.5.2 (2026-08-03): per user feedback, the legacy inline
// `format!("{:.0} MW", total_mw)` always rendered in megawatts
// regardless of magnitude — a 5 GW fusion plant read as
// `5000 MW` instead of `5 GW`. This formatter picks the smallest
// suffix that lands the displayed value in the 1..=999 range.
//
// Input is in **megawatts (MW)**. SI power prefixes:
//
// | 1 W  = 1e-6 MW  | 1 MW = 1   MW |
// | 1 kW = 1e-3 MW  | 1 GW = 1e3  MW |
// |                  | 1 TW = 1e6  MW |
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
//
// `category_index == 0` is the "Infrastructure" tab, `1` is "Industry", etc.
// The last index (8 in the 8-category case) is the "Locked" tab — we
// return the locked buildings instead of the unlocked ones.
//
// `multiplier` is the player's chosen build-multiplier (1, 5, 10, 25,
// 50, 100). It is passed through to `card_data_with_multiplier` so the
// rendered card shows the batch cost / total BP for the whole batch.
//
// `spare_power_mw` is the active colony's grid surplus (produced -
// consumed, in MW). Forwarded to `card_data_with_multiplier` so each
// card's Power effect line can show "demand vs spare" with a
// green/red sufficient/insufficient marker. v0.5.2 PR-A.2.
//
// `research_state` is used to decide which tech-gated buildings are
// visible. The legacy version used `required_tech_opt().is_none()` which
// hid every building with a tech requirement; the canary mirrors the
// egui panel's behavior (only show buildings whose required tech is
// absent or already unlocked).
pub fn visible_cards(
    data: &crate::colony::data::BuildingsData,
    research_state: &crate::research::systems::ResearchState,
    category_index: usize,
    _filter: super::state::BuildFilter,
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
            // either absent or already unlocked.
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
pub fn parse_category(s: &str) -> Option<BuildingCategory> {
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
pub fn category_from_index(idx: usize) -> Option<BuildingCategory> {
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
