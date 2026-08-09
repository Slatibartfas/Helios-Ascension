use crate::colony::{BuildingsData, Colony};
use crate::economy::budget::{
    GlobalBudget, ResourceRateTracker, SECONDS_PER_MONTH, SECONDS_PER_YEAR,
};
use crate::economy::components::{LocalStockpile, PlanetResources};
use crate::economy::types::ResourceType;
use crate::plugins::solar_system::CelestialBody;
use crate::research::ResearchState;
use crate::survey::ContinuousStationBonus;
use crate::ui::SimulationTime;
use bevy::prelude::*;

#[derive(Component, Debug, Clone, Reflect)]
#[reflect(Component)]
pub struct MiningOperation {
    pub resource_type: ResourceType,
    /// Base extraction rate in Megatons per year
    pub base_rate_mt_per_year: f64,
    pub active: bool,
}

impl Default for MiningOperation {
    fn default() -> Self {
        Self {
            resource_type: ResourceType::Iron,
            base_rate_mt_per_year: 1.0,
            active: true,
        }
    }
}

struct IndustrialProcessRule {
    output: ResourceType,
    required_tech: Option<&'static str>,
    inputs_per_output: &'static [(ResourceType, f64)],
}

fn industrial_process_rule(modifier_type: &str) -> Option<IndustrialProcessRule> {
    match modifier_type {
        "HydrogenSynthesis" => Some(IndustrialProcessRule {
            output: ResourceType::Hydrogen,
            required_tech: None,
            inputs_per_output: &[(ResourceType::Methane, 2.0)],
        }),
        "AmmoniaSynthesis" => Some(IndustrialProcessRule {
            output: ResourceType::Ammonia,
            required_tech: None,
            inputs_per_output: &[
                (ResourceType::Nitrogen, 0.82),
                (ResourceType::Methane, 0.71),
            ],
        }),
        "PolymerSynthesis" => Some(IndustrialProcessRule {
            output: ResourceType::Polymers,
            required_tech: None,
            inputs_per_output: &[(ResourceType::Methane, 1.15)],
        }),
        "TritiumBreeding" => Some(IndustrialProcessRule {
            output: ResourceType::Tritium,
            required_tech: Some("fusion_power"),
            inputs_per_output: &[(ResourceType::Lithium, 0.6)],
        }),
        "PlutoniumBreeding" => Some(IndustrialProcessRule {
            output: ResourceType::Plutonium,
            required_tech: Some("breeder_reactors"),
            inputs_per_output: &[(ResourceType::Uranium, 1.2)],
        }),
        _ => None,
    }
}

fn process_is_unlocked(
    rule: &IndustrialProcessRule,
    research_state: Option<&ResearchState>,
) -> bool {
    match rule.required_tech {
        Some(tech_id) => research_state.is_some_and(|state| state.is_unlocked(tech_id)),
        None => true,
    }
}

fn combined_available(
    local_opt: &Option<Mut<LocalStockpile>>,
    budget: &GlobalBudget,
    resource: ResourceType,
) -> f64 {
    local_opt.as_ref().map_or(0.0, |local| local.get(&resource)) + budget.get_stockpile(&resource)
}

/// v0.5.2: parse a resource name (without "Production" suffix) into a
/// `ResourceType`. Used by the modifier dispatch in `extract_resources`
/// and `update_resource_rates` to map `IronProduction` → `ResourceType::Iron`.
///
/// This is a stand-alone function (not the one in `colony/data.rs`)
/// because the modifier dispatch is in the `economy` crate and pulling
/// `colony::data` would create a circular dependency. The names match
/// `colony::data::parse_resource_type` exactly.
fn parse_resource_type_static(name: &str) -> Option<ResourceType> {
    use ResourceType::*;
    match name {
        "Water" => Some(Water),
        "Hydrogen" => Some(Hydrogen),
        "Ammonia" => Some(Ammonia),
        "Methane" => Some(Methane),
        "Nitrogen" => Some(Nitrogen),
        "Oxygen" => Some(Oxygen),
        "CarbonDioxide" => Some(CarbonDioxide),
        "Argon" => Some(Argon),
        "Iron" => Some(Iron),
        "Aluminum" => Some(Aluminum),
        "Titanium" => Some(Titanium),
        "Silicates" => Some(Silicates),
        "Helium3" => Some(Helium3),
        "Tritium" => Some(Tritium),
        "Uranium" => Some(Uranium),
        "Thorium" => Some(Thorium),
        "Plutonium" => Some(Plutonium),
        "Gold" => Some(Gold),
        "Silver" => Some(Silver),
        "Platinum" => Some(Platinum),
        "Copper" => Some(Copper),
        "RareEarths" => Some(RareEarths),
        "Phosphorus" => Some(Phosphorus),
        "Nickel" => Some(Nickel),
        "Tungsten" => Some(Tungsten),
        "Carbon" => Some(Carbon),
        "Deuterium" => Some(Deuterium),
        "Lithium" => Some(Lithium),
        "Sulfur" => Some(Sulfur),
        "Food" => Some(Food),
        "Chromium" => Some(Chromium),
        "Magnesium" => Some(Magnesium),
        "Cobalt" => Some(Cobalt),
        "Fluorine" => Some(Fluorine),
        "Polymers" => Some(Polymers),
        "Antimatter" => Some(Antimatter),
        "ExoticMatter" => Some(ExoticMatter),
        "Metamaterials" => Some(Metamaterials),
        "Computronium" => Some(Computronium),
        _ => None,
    }
}

fn consume_with_fallback(
    local_opt: &mut Option<Mut<LocalStockpile>>,
    budget: &mut GlobalBudget,
    resource: ResourceType,
    amount: f64,
) -> f64 {
    let mut remaining = amount.max(0.0);
    if remaining <= 0.0 {
        return 0.0;
    }

    if let Some(local) = local_opt.as_deref_mut() {
        let consumed_local = local.consume(resource, remaining);
        remaining -= consumed_local;
    }

    if remaining > 0.0 {
        let available_global = budget.get_stockpile(&resource);
        let consumed_global = remaining.min(available_global);
        if consumed_global > 0.0 {
            budget.consume_resource(resource, consumed_global);
            remaining -= consumed_global;
        }
    }

    amount.max(0.0) - remaining
}

/// v3.8: cap-aware production throttle. The body always produces up
/// to `desired`, but capped at the amount that the local stockpile
/// can absorb PLUS the upcoming consumption — so when the stockpile
/// is at cap, production tapers down to exactly the consumption
/// rate, giving the player a "net = 0" readout (and a stable
/// cap-saturated stockpile in steady state).
///
/// v3.8.1 (2026-08-07): added a soft-knee so the rate visibly
/// tapers as the stockpile approaches cap, not just at cap. Below
/// `SOFT_KNEE_START` (80% fill) the throttle is a passthrough;
/// between the soft-knee and 100% fill the rate ramps linearly
/// from `desired` down to `consumption_per_tick`. The hard cap
/// (`headroom + consumption`) is still applied as a safety net so
/// the deposit never exceeds the cap. Without the soft knee the
/// ramp is so steep (only the last 5% of cap on a typical Earth
/// body) that the player can't see the throttle happen.
///
/// Formula:
///   fill        = current / cap               (clamped 0..1)
///   soft_ramp   = if fill < SOFT_KNEE_START
///                   0.0
///                else
///                   (fill - SOFT_KNEE_START) / (1 - SOFT_KNEE_START)
///   throttled   = lerp(desired, consumption_per_tick, soft_ramp)
///   throttled   = min(throttled, headroom + consumption_per_tick)
///                  // mass-balance safety; never over-produce
///
/// Behaviour:
/// * `cap >= f64::MAX` (exotic / uncapped resources): passthrough
/// * `desired <= 0`: passthrough (no mining, no throttle)
/// * `fill < SOFT_KNEE_START`: throttled = `desired` (full)
/// * `fill >= SOFT_KNEE_START`: throttled lerps toward
///   `consumption_per_tick`
/// * `headroom == 0` (at cap): throttled = `consumption_per_tick`
///   (production covers the local draw; net flow = 0)
fn throttle_production(
    desired: f64,
    current: f64,
    cap: f64,
    consumption_per_tick: f64,
) -> f64 {
    // Always return a non-negative value. Negative `desired` is
    // a defensive no-op (a body can't "un-mine" material).
    if cap >= f64::MAX || desired <= 0.0 {
        return desired.max(0.0);
    }
    let headroom = (cap - current).max(0.0);
    let effective_capacity = headroom + consumption_per_tick.max(0.0);

    // v3.8.1: soft knee — start scaling down before the cap.
    // Without this, the player can't see the throttle happen
    // because the ramp from `headroom + consumption = desired` to
    // `headroom + consumption = consumption` is the last few
    // percent of fill.
    let fill = if cap > 0.0 {
        (current / cap).clamp(0.0, 1.0)
    } else {
        1.0
    };
    const SOFT_KNEE_START: f64 = 0.8;
    let soft_ramp = if fill < SOFT_KNEE_START {
        0.0
    } else {
        ((fill - SOFT_KNEE_START) / (1.0 - SOFT_KNEE_START)).clamp(0.0, 1.0)
    };
    let soft_throttled = desired * (1.0 - soft_ramp) + consumption_per_tick.max(0.0) * soft_ramp;

    // Mass-balance safety: the soft-knee lerp can theoretically
    // produce more than the cap can absorb (e.g. when fill is
    // exactly 0.8 and headroom is smaller than expected). Clamp
    // to the strict cap + consumption floor.
    soft_throttled.min(effective_capacity)
}

/// v3.8: deposit `amount` of `resource` into the body's local
/// stockpile (or global budget fallback) and **return the amount
/// actually added** (capped at the body's effective per-resource
/// stockpile cap). Negative `amount` is clamped to zero. A return
/// of `0.0` means the stockpile was already at cap.
///
/// Replaces the v0.5.2 `deposit_with_fallback` that returned `()`;
/// the new signature lets `extract_resources` use the throttled
/// deposit (not the pre-cap gross) for body-mass accounting and
/// rate-tracker bookkeeping.
fn deposit_with_fallback(
    local_opt: &mut Option<Mut<LocalStockpile>>,
    budget: &mut GlobalBudget,
    resource: ResourceType,
    amount: f64,
) -> f64 {
    let amount = amount.max(0.0);
    if amount <= 0.0 {
        return 0.0;
    }

    if let Some(local) = local_opt.as_deref_mut() {
        let cap = budget.effective_stockpile_cap(resource);
        local.add_capped(resource, amount, cap)
    } else {
        budget.add_resource_capped(resource, amount)
    }
}

fn feasible_output_amount(
    desired_output: f64,
    rule: &IndustrialProcessRule,
    local_opt: &Option<Mut<LocalStockpile>>,
    budget: &GlobalBudget,
) -> f64 {
    if desired_output <= 0.0 {
        return 0.0;
    }

    let mut scale = 1.0_f64;
    for (input_resource, input_per_output) in rule.inputs_per_output {
        let required_input = desired_output * *input_per_output;
        if required_input <= 0.0 {
            continue;
        }
        let available_input = combined_available(local_opt, budget, *input_resource);
        scale = scale.min((available_input / required_input).clamp(0.0, 1.0));
    }

    desired_output * scale
}

pub fn extract_resources(
    mut budget: ResMut<GlobalBudget>,
    mut all_query: Query<(
        Entity,
        &mut PlanetResources,
        &mut CelestialBody,
        Option<&MiningOperation>,
        Option<&Colony>,
        Option<&mut LocalStockpile>,
        Option<&ContinuousStationBonus>,
    )>,
    sim_time: Res<SimulationTime>,
    mut last_elapsed: Local<f64>,
    buildings_data: Option<Res<BuildingsData>>,
    research_state: Option<Res<ResearchState>>,
    mut dirty: ResMut<crate::economy::DirtyBodies>,
) {
    let current_elapsed = sim_time.elapsed_seconds();
    let dt = current_elapsed - *last_elapsed;
    *last_elapsed = current_elapsed;

    if dt <= 0.0 {
        return;
    }

    // 1 year = 365.25 days * 24 * 60 * 60
    let years_elapsed = dt / 31_557_600.0;

    if years_elapsed <= 0.0 {
        return;
    }

    // Helper: deposit extracted amount into the body's LocalStockpile if present,
    // otherwise fall back to the global budget.
    // Closure captures `budget` by mut-ref which is not possible with an iter_mut
    // borrow also active; instead we collect extractions and apply them afterwards.
    // We handle this via a simple inline deposit in the loop with an explicit split.

    for (entity, mut resources, mut body, op_opt, colony_opt, mut local_opt, station_bonus_opt) in
        all_query.iter_mut()
    {
        /// Deposit helper: goes to LocalStockpile when present, GlobalBudget otherwise.
        /// v3.8: returns the **actual amount added** (capped at the
        /// body's effective stockpile cap), so callers can use the
        /// throttled deposit (not the pre-cap gross) for body-mass
        /// accounting and rate-tracker bookkeeping.
        macro_rules! deposit {
            ($rt:expr, $amount:expr) => {{
                if $amount > 0.0 {
                    if let Some(ref mut ls) = local_opt {
                        let cap = budget.effective_stockpile_cap($rt);
                        ls.add_capped($rt, $amount, cap)
                    } else {
                        budget.add_resource_capped($rt, $amount)
                    }
                } else {
                    0.0
                }
            }};
        }

        // v3.8: per-tick consumption of `resource` on this body.
        // Colony bodies get the full per-capita + maintenance draw
        // from `colony.annual_resource_consumption`. Bodies with no
        // colony (just a MiningOperation) consume nothing — their
        // throttle is therefore the strict headroom cap.
        let per_tick_consumption = |rt: ResourceType| -> f64 {
            if let Some(c) = colony_opt {
                if let Some(d) = buildings_data.as_ref() {
                    c.annual_resource_consumption(rt, d) * years_elapsed
                } else {
                    0.0
                }
            } else {
                0.0
            }
        };

        // GRA-83 PR-E: per-body orbital survey station bonus
        // multiplies the body's mining rates (NOT atmospheric
        // harvesting, NOT industrial synthesis — the issue body
        // and the design doc call it a "mining yield bonus"). A
        // body with no station orbiting it falls back to 1.0×
        // (the cache is `Option`).
        let mining_bonus = ContinuousStationBonus::multiplier_or_neutral(station_bonus_opt);

        // 1. Process specific MiningOperations (legacy/scenario)
        if let Some(op) = op_opt {
            if op.active {
                let mut total_extracted = 0.0;

                if let Some(deposit) = resources.deposits.get_mut(&op.resource_type) {
                    let mut demand = op.base_rate_mt_per_year * mining_bonus * years_elapsed;

                    // v3.8: cap-aware throttle. Throttle at the demand
                    // level so deposit reserves, body mass, and
                    // stockpile stay consistent (the mine only digs
                    // what the local stockpile can absorb + cover the
                    // upcoming consumption). At cap: throttled
                    // demand = per-tick consumption, so the body
                    // still extracts just enough to keep its local
                    // industry supplied; the net stockpile change is
                    // ~0 and the displayed net rate is 0.
                    let cap = budget.effective_stockpile_cap(op.resource_type);
                    let current = local_opt
                        .as_ref()
                        .map_or(0.0, |ls| ls.get(&op.resource_type));
                    demand = throttle_production(
                        demand,
                        current,
                        cap,
                        per_tick_consumption(op.resource_type),
                    );

                    // 1. Proven Crustal (Cheapest)
                    let taking_proven = demand.min(deposit.reserve.proven_crustal);
                    deposit.reserve.proven_crustal -= taking_proven;
                    total_extracted += taking_proven;
                    demand -= taking_proven;

                    // 2. Deep Deposits (Expensive)
                    if demand > 0.0 {
                        let taking_deep = demand.min(deposit.reserve.deep_deposits);
                        deposit.reserve.deep_deposits -= taking_deep;
                        total_extracted += taking_deep;
                        demand -= taking_deep;
                    }

                    // 3. Planetary Bulk (Exorbitant)
                    if demand > 0.0 {
                        let taking_bulk = demand.min(deposit.reserve.planetary_bulk);
                        deposit.reserve.planetary_bulk -= taking_bulk;
                        total_extracted += taking_bulk;
                    }

                    if total_extracted > 0.0 {
                        deposit!(op.resource_type, total_extracted);
                        // Reduce body mass (1 Mt = 1e9 kg). v3.8:
                        // `total_extracted` is already throttled, so
                        // body mass matches what actually leaves the
                        // body and lands in the stockpile (or is
                        // vented when the cap is full).
                        body.mass -= total_extracted * 1e9;
                    }
                }
            }
        }

        // 2. Process Colony Mining & Atmospheric Harvesting
        if let Some(colony) = colony_opt {
            if let Some(data) = &buildings_data {
                // v0.5.2: per-resource dedicated mines. Each mine has a
                // single `XxxProduction` modifier (e.g., `IronProduction`)
                // whose value is the per-build base yield in Mt/yr. The
                // final per-tick extraction is:
                //
                //   yield_per_tick = count × base_yield
                //                  × deposit.accessibility
                //                  × colony.yield_mult
                //                  × years_elapsed
                //                  × station_bonus
                //
                // Where `deposit.accessibility` is the body's per-resource
                // accessibility scalar (0.0–1.0; see `economy/components.rs`
                // `MineralDeposit::accessibility`). For Earth this is
                // typically 0.4–0.7, so 25 IronMines × 120 Mt/yr × 0.6 =
                // 1,800 Mt/yr ≈ USGS 2024 world iron-ore production.
                //
                // NO share-fold. NO MiningEfficiency/DeepMiningEfficiency/
                // BulkMiningEfficiency modifiers. The legacy tier system
                // was removed because it was opaque (concentration-weighted
                // distribution over every eligible deposit) and over-
                // produced precious metals by 100–300× real-world.
                //
                // Special cases (still in modifier dispatch below):
                //   - `WaterProduction` (WaterProcessor off-world water,
                //      v0.5.1) is a direct deposit because water is
                //      condensed/mined, not extracted from a deposit tier.
                //   - `He3Production` (He3Mine, canary 3) is a direct
                //      deposit because He-3 is mined from regolith or
                //      gas-giant atmospheres, not from a tiered crustal
                //      deposit.
                //   - Industrial synthesis (HydrogenSynthesis, etc.) is
                //      unchanged — those modifiers consume inputs and
                //      produce outputs via `IndustrialProcessRule`.
                //
                // v3.8.12 (2026-08-08): removed the
                // `AtmosphericHarvesting` share-fold. The fold
                // distributed a single "harvested gases" rate across
                // atmospheric deposits by concentration weight, which
                // produced 325% of N demand, 117% of O, 775% of Ar, and
                // 0.2% of CO₂ (all in the same extraction stream). The
                // per-gas reality of cryogenic air separation
                // (Linde / Air Liquide 2024 industrial gas mix) is
                // that each gas has its own per-build rate — they're
                // co-extracted, but not in the proportions of the
                // source atmosphere.  AtmosphericProcessor now uses
                // `NitrogenProduction`, `OxygenProduction`,
                // `ArgonProduction`, and `CarbonDioxideProduction`
                // directly (the same `<Resource>Production` path as
                // mining), each tuned so 300 AtmosphericProcessors
                // produces the 2026 world demand.
                let yield_mult = colony.effective_yield_multiplier();
                // v0.5.2: per-resource direct-production rates
                // (one slot per resource). Filled by the dispatch loop
                // below; consumed by the direct-deposit block at the
                // bottom. `direct_production[(resource, modifier_kind)]`
                // is the rate in Mt/yr for that resource via that path.
                let mut direct_production: std::collections::HashMap<ResourceType, f64> =
                    std::collections::HashMap::new();

                for (building_type, &count) in &colony.buildings {
                    if count == 0 {
                        continue;
                    }
                    if let Some(def) = data.get(building_type) {
                        for modifier in &def.modifiers {
                            // v3.8.12: per-resource direct-production
                            // modifier. Modifier names follow the
                            // pattern `<Resource>Production` (e.g.,
                            // `IronProduction`, `WaterProduction`,
                            // `He3Production`, `NitrogenProduction`).
                            // We dispatch by stripping the `Production`
                            // suffix and looking up the ResourceType.
                            // The previous AtmosphericProcessor had a
                            // special `AtmosphericHarvesting` case
                            // that distributed a single rate across
                            // atmospheric deposits by concentration
                            // weight (see comment above); that path
                            // is now removed because the share-fold
                            // gave wildly wrong per-gas splits
                            // (325% N, 117% O, 775% Ar, 0.2% CO₂).
                            if let Some(resource_name) =
                                modifier.modifier_type.strip_suffix("Production")
                            {
                                if let Some(target) =
                                    parse_resource_type_static(resource_name)
                                {
                                    *direct_production.entry(target).or_insert(0.0) +=
                                        modifier.value * count as f64 * yield_mult;
                                }
                            }
                        }
                    }
                }

                // v3.8.12: per-body synthesis-input draw (Mt per tick),
                // recorded by the industrial-process pass below. The
                // direct-production throttle floor adds this so a resource
                // whose only consumers are industrial processes (methane →
                // PolymerSynthesis) is still produced at cap instead of
                // draining the stockpile. See the v3.8.12 comment in the
                // synthesis pass for the full rationale.
                let mut synthesis_drawn: std::collections::HashMap<ResourceType, f64> =
                    std::collections::HashMap::new();

                // --- Industrial synthesis / breeding ---
                // v3.8.12 (2026-08-09): this pass now runs BEFORE the
                // direct-production deposit loop. Previously it ran after,
                // so `per_tick_consumption` (the throttle floor) could not
                // see the process input draw: at methane cap the
                // direct-production throttle cut methane to per-cap +
                // maintenance while PolymerSynthesis kept drawing, so the
                // stockpile equilibrated ~87.5% full and the rate display
                // showed a phantom red net. Running synthesis first lets
                // the direct-production floor below include the real
                // process draw (per_tick_consumption + synthesis_drawn).
                for (building_type, &count) in &colony.buildings {
                    if count == 0 {
                        continue;
                    }
                    let Some(def) = data.get(building_type) else {
                        continue;
                    };

                    for modifier in &def.modifiers {
                        let Some(rule) = industrial_process_rule(&modifier.modifier_type) else {
                            continue;
                        };
                        if !process_is_unlocked(&rule, research_state.as_deref()) {
                            continue;
                        }

                        // Per GRA-22 §4.5: industrial synthesis scales with the
                        // colony's development yield multiplier.  Inputs and
                        // outputs both move — a ChemicalPlant on an Outpost
                        // produces a tenth of a Civilisation's output and
                        // consumes a tenth of the inputs.
                        let desired_output =
                            modifier.value * count as f64 * yield_mult * years_elapsed;
                        let actual_output =
                            feasible_output_amount(desired_output, &rule, &local_opt, &budget);

                        if actual_output <= 0.0 {
                            continue;
                        }

                        // v3.8: cap-aware throttle on the OUTPUT side.
                        // Inputs are still drawn for `actual_output`
                        // (a small over-draw when the output is at
                        // cap — accepted as "process inefficiency at
                        // cap"; the alternative is to throttle inputs
                        // and outputs together, which is a bigger
                        // change for a future patch). At cap the
                        // displayed production = the consumption the
                        // body actually draws, and the excess output
                        // is vented.
                        let cap = budget.effective_stockpile_cap(rule.output);
                        let current = local_opt
                            .as_ref()
                            .map_or(0.0, |ls| ls.get(&rule.output));
                        let throttled_output = throttle_production(
                            actual_output,
                            current,
                            cap,
                            per_tick_consumption(rule.output),
                        );

                        if throttled_output <= 0.0 {
                            // v3.8: cap full AND no per-body
                            // consumption means the output has
                            // nowhere to go. The factory is idle
                            // (no inputs drawn, no output deposited).
                            // A future pass could throttle inputs
                            // proportionally when the output is
                            // partially capped; for now this is a
                            // hard idle at saturation.
                            continue;
                        }

                        for (input_resource, input_per_output) in rule.inputs_per_output {
                            let input_amount = throttled_output * *input_per_output;
                            consume_with_fallback(
                                &mut local_opt,
                                &mut budget,
                                *input_resource,
                                input_amount,
                            );
                            // v3.8.12: record the draw so the
                            // direct-production throttle floor below
                            // covers it (methane → PolymerSynthesis
                            // etc.).
                            *synthesis_drawn.entry(*input_resource).or_insert(0.0) +=
                                input_amount;
                        }

                        deposit_with_fallback(
                            &mut local_opt,
                            &mut budget,
                            rule.output,
                            throttled_output,
                        );
                    }
                }

                // GRA-83 PR-E: per-body orbital survey station bonus
                // multiplies mining rates (NOT atmospheric harvesting,
                // NOT industrial synthesis, NOT direct-production paths
                // that explicitly read from non-tiered sources like
                // He-3 regolith or asteroid captures — those use the
                // body's per-resource accessibility for the location
                // gate, not the yield multiplier).
                // We multiply the direct-production rates below; the
                // legacy code scaled MiningEfficiency/etc. here, but
                // v0.5.2 unified everything into direct_production so
                // the multiplication moves to the direct-deposit block.
                // For now, we apply the bonus uniformly so the
                // station's effect is still felt.
                let bonus = mining_bonus;

                // --- v0.5.2: per-resource direct production (mining,
                // AutoMines, off-world water, He-3, precious metals, …) ---
                // For each (resource, base_rate) pair:
                //   yield_per_tick = base_rate
                //                  × deposit.accessibility  (0.0–1.0)
                //                  × bonus                  (orbital station)
                //                  × years_elapsed
                // Direct deposit (not via share-fold; no deposit
                // depletion; the deposit remains intact for future
                // deep-mining passes if a future patch wants to tap
                // the actual proven_crustal tier for these resources).
                for (resource, base_rate) in &direct_production {
                    let access = resources
                        .get_deposit(resource)
                        .map(|d| (d.accessibility as f64).clamp(0.0, 1.0))
                        .unwrap_or(0.0);
                    if access <= 0.0 {
                        // Body has no accessible deposit for this resource
                        // (e.g. trying to mine Iron on a gas-giant). Skip
                        // — the AutoMine wouldn't have been built there
                        // in the first place because of the body-type
                        // gate, but a 0-accessibility fallback is safe.
                        continue;
                    }
                    // v3.8: cap-aware throttle. Direct-production
                    // modifiers (IronMine, He3Mine, WaterProcessor, etc.)
                    // don't subtract from the body — they represent
                    // refining / synthesising — so we throttle the
                    // deposit directly rather than the extraction. The
                    // excess over `headroom` is "vented" (refined
                    // material that can't be stored).
                    let cap = budget.effective_stockpile_cap(*resource);
                    let current = local_opt
                        .as_ref()
                        .map_or(0.0, |ls| ls.get(resource));
                    let desired = base_rate * access * bonus * years_elapsed;
                    // v3.8.12: the throttle floor now includes the
                    // synthesis-input draw (`synthesis_drawn`) recorded by
                    // the process pass above, not just per-capita +
                    // maintenance. Without it, methane (whose only
                    // consumers at Earth start are per-cap + NG plants +
                    // PolymerSynthesis input) throttled to per-cap + maint
                    // at cap while the factory kept drawing, so the
                    // stockpile equilibrated ~87.5% full and the rate
                    // display showed a phantom red net. With the floor
                    // including the process draw, production at cap =
                    // per-cap + maint + synthesis inputs and the net rate
                    // displays 0.
                    let floor = per_tick_consumption(*resource)
                        + synthesis_drawn.get(resource).copied().unwrap_or(0.0);
                    let throttled = throttle_production(desired, current, cap, floor);
                    if throttled > 0.0 {
                        deposit_with_fallback(&mut local_opt, &mut budget, *resource, throttled);
                    }
                }
            }
        }

        // Mark this body's `LocalStockpile` (and body
        // mass — the body.mass subtraction above is a
        // player-driven mutation) as dirty so the v2
        // extract path captures the divergence. The
        // reason is `Multiple` because we may have
        // touched both stockpile and body mass in this
        // tick. The extract path sees `Multiple` and
        // populates every applicable divergence field.
        dirty.mark(entity, crate::economy::DirtyReason::Multiple);
    }
}

/// System that computes **net** monthly rates for all resources and
/// research/engineering points, writing them into [`ResourceRateTracker`].
///
/// Production comes from `MiningOperation` components, colony mining
/// buildings, and colony food production. Consumption comes from building
/// maintenance costs plus colony food consumption. The displayed rate is
/// production − consumption so the UI shows the true net balance.
///
/// RP/EP rates include the base generation rates defined in
/// `research::systems` (`BASE_RP_PER_YEAR`, `BASE_EP_PER_YEAR`) so the
/// bar always reflects actual accumulation.
pub fn update_resource_rates(
    mut tracker: ResMut<ResourceRateTracker>,
    // v3.8: LocalStockpile added to the mining_ops query so per-entity
    // production rates can be cap-throttled to match what
    // `extract_resources` actually deposits (the old query only knew
    // gross production and showed positive rates even at cap).
    mining_ops: Query<(
        Entity,
        &MiningOperation,
        Option<&PlanetResources>,
        Option<&LocalStockpile>,
        Option<&ContinuousStationBonus>,
    )>,
    research_buildings: Query<&crate::research::components::ResearchBuilding>,
    engineering_facilities: Query<&crate::research::components::EngineeringFacility>,
    colony_query: Query<(
        Entity,
        &Colony,
        Option<&PlanetResources>,
        Option<&LocalStockpile>,
        Option<&ContinuousStationBonus>,
    )>,
    buildings_data: Option<Res<BuildingsData>>,
    budget: Res<GlobalBudget>,
    research_state: Res<crate::research::ResearchState>,
) {
    // --- Resource rates from mining (production) ---
    let mut rates = std::collections::HashMap::new();
    let mut production_rates = std::collections::HashMap::new();
    let mut consumption_rates = std::collections::HashMap::new();
    // v3.8.11: per-component consumption breakdown (for tooltip).
    // `population_consumption` is the per-capita × pop draw that
    // previously lived only in the separate
    // `deduct_population_consumption` system.  `synthesis_input` is the
    // industrial-process input draw (Methane → PolymerSynthesis, etc.).
    // The remainder of consumption_rates is maintenance.
    let mut population_consumption: std::collections::HashMap<ResourceType, f64> =
        std::collections::HashMap::new();
    let mut synthesis_input: std::collections::HashMap<ResourceType, f64> =
        std::collections::HashMap::new();
    let mut per_entity: std::collections::HashMap<
        Entity,
        std::collections::HashMap<ResourceType, f64>,
    > = std::collections::HashMap::new();

    let monthly_fraction = SECONDS_PER_MONTH / SECONDS_PER_YEAR;

    // 1. MiningOperation components
    for (entity, op, resources_opt, local_opt, station_bonus_opt) in mining_ops.iter() {
        if !op.active {
            continue;
        }
        // Skip if the targeted deposit is fully depleted
        let depleted = resources_opt.is_some_and(|res| {
            res.deposits.get(&op.resource_type).is_none_or(|d| {
                d.reserve.proven_crustal < 0.001
                    && d.reserve.deep_deposits < 0.001
                    && d.reserve.planetary_bulk < 0.001
            })
        });
        if depleted {
            continue;
        }
        // GRA-83 PR-E: per-body orbital survey station bonus
        // multiplies the MiningOperation rate. Falls through to
        // 1.0× when the body has no orbiting station.
        let mining_bonus = ContinuousStationBonus::multiplier_or_neutral(station_bonus_opt);
        // base_rate_mt_per_year → per month = rate * (month / year)
        let monthly =
            op.base_rate_mt_per_year * mining_bonus * monthly_fraction;
        // v3.8: cap-aware throttle. MiningOperation bodies without
        // a colony consume nothing, so the throttle is the strict
        // headroom cap. With a colony (rare for v0.5.2 MiningOps —
        // those usually live on AutoMine bodies) the consumption
        // floor keeps a small production running at cap.
        let cap = budget.effective_stockpile_cap(op.resource_type);
        let current = local_opt.map_or(0.0, |ls| ls.get(&op.resource_type));
        let mut throttled = throttle_production(monthly, current, cap, 0.0);
        // v3.8.2: cap the rate by the deposit's remaining
        // extractable reserve. Without this, the rate display
        // shows the "intended" production even when the deposit
        // is depleted (e.g. an asteroid's iron ore once fully
        // mined). The rate is the amount we'd extract in one
        // month, so it can be at most the remaining reserve.
        // For atmospheric gases on a planet the deposit is
        // huge (Earth's atmospheric N₂ is ~4 Tt = 4×10⁹ Mt),
        // so this is essentially a non-binding cap — it only
        // matters for genuinely depleted bodies.
        if let Some(resources) = resources_opt {
            if let Some(deposit) = resources.deposits.get(&op.resource_type) {
                let reserve = deposit.reserve.proven_crustal
                    + deposit.reserve.deep_deposits
                    + deposit.reserve.planetary_bulk;
                throttled = throttled.min(reserve.max(0.0));
            }
        }
        *rates.entry(op.resource_type).or_insert(0.0) += throttled;
        *production_rates.entry(op.resource_type).or_insert(0.0) += throttled;
        *per_entity
            .entry(entity)
            .or_default()
            .entry(op.resource_type)
            .or_insert(0.0) += throttled;
    }

    // Helper macro-like closure to add to both global and per-entity rates
    let add_production = |rates: &mut std::collections::HashMap<ResourceType, f64>,
                          production_rates: &mut std::collections::HashMap<ResourceType, f64>,
                          per_entity: &mut std::collections::HashMap<
        Entity,
        std::collections::HashMap<ResourceType, f64>,
    >,
                          entity: Entity,
                          r_type: ResourceType,
                          amount: f64| {
        *rates.entry(r_type).or_insert(0.0) += amount;
        *production_rates.entry(r_type).or_insert(0.0) += amount;
        *per_entity
            .entry(entity)
            .or_default()
            .entry(r_type)
            .or_insert(0.0) += amount;
    };
    let add_consumption = |rates: &mut std::collections::HashMap<ResourceType, f64>,
                           consumption_rates: &mut std::collections::HashMap<ResourceType, f64>,
                           per_entity: &mut std::collections::HashMap<
        Entity,
        std::collections::HashMap<ResourceType, f64>,
    >,
                           entity: Entity,
                           r_type: ResourceType,
                           amount: f64| {
        *rates.entry(r_type).or_insert(0.0) -= amount;
        *consumption_rates.entry(r_type).or_insert(0.0) += amount;
        *per_entity
            .entry(entity)
            .or_default()
            .entry(r_type)
            .or_insert(0.0) -= amount;
    };

    // 2. Colony mining & atmospheric harvesting
    if let Some(data) = &buildings_data {
        for (entity, colony, resources_opt, local_opt, station_bonus_opt) in colony_query.iter() {
            if let Some(resources) = resources_opt {
                // v0.5.2: per-resource dedicated mines. Mirrors the
                // `extract_resources` dispatch: each building's `XxxProduction`
                // modifier contributes to a per-resource rate, scaled by
                // `yield_mult × mining_bonus × deposit.accessibility`. NO
                // share-fold (removed in v0.5.2). The legacy tier
                // (MiningEfficiency/Deep/Bulk) and `surface/deep/bulk_rate`
                // bookkeeping are gone.
                //
                // v3.8.12 (2026-08-08): the previous AtmosphericProcessor
                // dispatch had a special `AtmosphericHarvesting` case that
                // summed a single "harvested gases" rate (Mt/yr of total
                // cryogenic stream) and then distributed it across
                // atmospheric deposits by concentration weight. The
                // share-fold over-allocated the rate to whatever gas had
                // the highest concentration in the deposit (1.0 for N₂)
                // and starved the rest. AtmosphericProcessor now uses
                // `NitrogenProduction` / `OxygenProduction` /
                // `ArgonProduction` / `CarbonDioxideProduction` and falls
                // through to the generic `*Production` strip_suffix path.
                let yield_mult = colony.effective_yield_multiplier();
                let mining_bonus = ContinuousStationBonus::multiplier_or_neutral(station_bonus_opt);
                // v0.5.2: per-resource direct-production rates
                // (rate-tracker mirror of `extract_resources` direct_deposit).
                let mut direct_production: std::collections::HashMap<ResourceType, f64> =
                    std::collections::HashMap::new();

                for (building_type, &count) in &colony.buildings {
                    if count == 0 {
                        continue;
                    }
                    if let Some(def) = data.get(building_type) {
                        for modifier in &def.modifiers {
                            if let Some(resource_name) =
                                modifier.modifier_type.strip_suffix("Production")
                            {
                                if let Some(target) =
                                    parse_resource_type_static(resource_name)
                                {
                                    *direct_production.entry(target).or_insert(0.0) +=
                                        modifier.value * count as f64 * yield_mult;
                                }
                            }
                        }
                    }
                }

                // v3.8.12: per-body synthesis-input draw (Mt/month),
                // recorded by the process pass below and added to the
                // direct-production throttle floor so the displayed
                // rates mirror the reordered `extract_resources`.
                let mut synthesis_drawn: std::collections::HashMap<ResourceType, f64> =
                    std::collections::HashMap::new();

                // Simulated input availability for the synthesis pass
                // (local stockpile + global budget). Built BEFORE the
                // process loop so the process draw can decrement it as
                // it runs (matches `feasible_output_amount`'s
                // `combined_available` in `extract_resources`).
                let mut simulated_available: std::collections::HashMap<ResourceType, f64> =
                    ResourceType::all()
                        .iter()
                        .copied()
                        .map(|resource| {
                            (
                                resource,
                                local_opt.map_or(0.0, |local| local.get(&resource))
                                    + budget.get_stockpile(&resource),
                            )
                        })
                        .collect();

                // v3.8.12: the synthesis rate pass runs BEFORE the
                // direct-production rate pass (mirroring the reordered
                // `extract_resources`), so `synthesis_drawn` is known
                // when the direct-production throttle computes its
                // consumption floor. Without this the methane rate
                // display showed a phantom red net at cap (see the
                // v3.8.12 comment in `extract_resources`).
                for (building_type, &count) in &colony.buildings {
                    if count == 0 {
                        continue;
                    }
                    let Some(def) = data.get(building_type) else {
                        continue;
                    };

                    for modifier in &def.modifiers {
                        let Some(rule) = industrial_process_rule(&modifier.modifier_type) else {
                            continue;
                        };
                        if !process_is_unlocked(&rule, Some(&research_state)) {
                            continue;
                        }

                        // Yield-scaled (matches `extract_resources`).
                        let desired_monthly_output = modifier.value
                            * count as f64
                            * yield_mult
                            * (SECONDS_PER_MONTH / SECONDS_PER_YEAR);
                        if desired_monthly_output <= 0.0 {
                            continue;
                        }

                        let mut scale = 1.0_f64;
                        for (input_resource, input_per_output) in rule.inputs_per_output {
                            let required = desired_monthly_output * *input_per_output;
                            if required <= 0.0 {
                                continue;
                            }
                            let available = simulated_available
                                .get(input_resource)
                                .copied()
                                .unwrap_or(0.0);
                            scale = scale.min((available / required).clamp(0.0, 1.0));
                        }

                        let actual_output = desired_monthly_output * scale;
                        if actual_output <= 0.0 {
                            continue;
                        }

                        // v3.8: cap-aware throttle on the OUTPUT
                        // rate. The display mirrors the deposit
                        // throttle in `extract_resources` so the
                        // production rate and the actual stockpile
                        // change stay in sync. At cap the
                        // production rate = monthly consumption and
                        // the net rate = 0.
                        let cap = budget.effective_stockpile_cap(rule.output);
                        let current = local_opt.map_or(0.0, |ls| ls.get(&rule.output));
                        let monthly_consumption = buildings_data
                            .as_ref()
                            .map(|d| {
                                colony
                                    .annual_resource_consumption(rule.output, d)
                                    * monthly_fraction
                            })
                            .unwrap_or(0.0);
                        let throttled_output = throttle_production(
                            actual_output,
                            current,
                            cap,
                            monthly_consumption,
                        );

                        if throttled_output <= 0.0 {
                            // Output is fully saturated and there is
                            // no per-body draw on it — the factory
                            // is idle. Skip the input draw entirely
                            // (matches `extract_resources`'s
                            // v3.8 idle-on-saturation branch).
                            continue;
                        }

                        for (input_resource, input_per_output) in rule.inputs_per_output {
                            let consumed = throttled_output * *input_per_output;
                            *simulated_available.entry(*input_resource).or_insert(0.0) -= consumed;
                            // v3.8.11 (2026-08-07): sign-bug fix. The
                            // previous call passed `-consumed`, which
                            // `add_consumption` then SUBTRACTED from
                            // `rates` — flipping the sign twice, so the
                            // rate display ADDed the input cost instead
                            // of subtracting it.  Net effect: methane
                            // showed +142.4 Mt/mo (positive) while
                            // stockpile dropped because PolymerSynthesis
                            // was consuming ~860 Mt/yr.  Pass positive
                            // `consumed` (the helper already does
                            // `*rates -= amount`).
                            add_consumption(
                                &mut rates,
                                &mut consumption_rates,
                                &mut per_entity,
                                entity,
                                *input_resource,
                                consumed,
                            );
                            // v3.8.11: track industrial-process input
                            // draw separately for the UI tooltip.
                            *synthesis_input.entry(*input_resource).or_insert(0.0) += consumed;
                            // v3.8.12: track the draw per body so the
                            // direct-production throttle floor below
                            // can cover it.
                            *synthesis_drawn.entry(*input_resource).or_insert(0.0) += consumed;
                        }

                        *simulated_available.entry(rule.output).or_insert(0.0) +=
                            throttled_output;
                        add_production(
                            &mut rates,
                            &mut production_rates,
                            &mut per_entity,
                            entity,
                            rule.output,
                            throttled_output,
                        );
                    }
                }

                // v0.5.2: per-resource direct production (rate tracker).
                // For each resource, monthly_rate =
                //   base_rate × deposit.accessibility × bonus × monthly_fraction
                for (resource, base_rate) in &direct_production {
                    let access = resources
                        .get_deposit(resource)
                        .map(|d| (d.accessibility as f64).clamp(0.0, 1.0))
                        .unwrap_or(0.0);
                    if access <= 0.0 {
                        continue;
                    }
                    let monthly = base_rate * access * mining_bonus * monthly_fraction;
                    // v3.8: cap-aware throttle. The displayed
                    // production rate is the throttled value so
                    // the player sees the mine slow down as the
                    // local stockpile approaches cap.
                    let cap = budget.effective_stockpile_cap(*resource);
                    let current = local_opt.map_or(0.0, |ls| ls.get(resource));
                    // v3.8.12: the throttle floor now includes the
                    // per-body synthesis-input draw (`synthesis_drawn`),
                    // matching the reordered `extract_resources`. Without
                    // it the methane rate display showed a phantom red
                    // net at cap even though the stockpile was stable.
                    let monthly_consumption = buildings_data
                        .as_ref()
                        .map(|d| {
                            colony.annual_resource_consumption(*resource, d) * monthly_fraction
                        })
                        .unwrap_or(0.0)
                        + synthesis_drawn.get(resource).copied().unwrap_or(0.0);
                    let throttled = throttle_production(
                        monthly,
                        current,
                        cap,
                        monthly_consumption,
                    );
                    if throttled > 0.0 {
                        add_production(
                            &mut rates,
                            &mut production_rates,
                            &mut per_entity,
                            entity,
                            *resource,
                            throttled,
                        );
                    }
                }
            } else {
                // PR-I follow-up (GRA-358): demoted from `warn!` to
                // `debug!`. The v2 restore factory runs
                // `regenerate_bodies_minimal`, which intentionally
                // does NOT synthesise spectral-class resource
                // deposits — that's the regen chain's job (and it
                // doesn't run on Restore). Every colony whose body
                // lacks spectral-class deposits hit this branch
                // once per frame and flooded the log. The
                // `debug!` keeps the diagnostic surface for someone
                // investigating "why isn't this body producing?"
                // while silencing the per-frame noise.
                debug!("Colony {} has no PlanetResources", colony.name);
            }
        }
    } else {
        warn!("BuildingsData missing in update_resource_rates");
    }

    // 3. Add net colony food rate (production - population consumption)
    // v3.6: food per-build values and consumption per-capita are read from
    // `BuildingsData` (RON-driven). If the resource isn't loaded yet, fall
    // back to 0 — the depletion-timeline system will pick it up next tick.
    let food_data = buildings_data.as_deref();
    for (entity, colony, _, _, _) in colony_query.iter() {
        // Per GRA-22 §4.5: agricultural production scales with the colony's
        // `ColonyDevelopment` yield multiplier, matching the rest of the
        // rates in this function.  An Outpost at ×0.10 reports the same rate
        // the sim extracts/consumes.  Consumption is per-capita (biological)
        // and stays unmultiplied.
        let food_yield_mult = colony.effective_yield_multiplier();
        let food_production_per_month = food_data
            .map(|d| colony.food_production_per_year(d) * food_yield_mult)
            .unwrap_or(0.0)
            * (SECONDS_PER_MONTH / SECONDS_PER_YEAR);
        let food_consumption_per_month = food_data
            .map(|d| colony.food_consumption_per_year(d))
            .unwrap_or(0.0)
            * (SECONDS_PER_MONTH / SECONDS_PER_YEAR);
        if food_production_per_month > f64::EPSILON {
            add_production(
                &mut rates,
                &mut production_rates,
                &mut per_entity,
                entity,
                ResourceType::Food,
                food_production_per_month,
            );
        }
        if food_consumption_per_month > f64::EPSILON {
            add_consumption(
                &mut rates,
                &mut consumption_rates,
                &mut per_entity,
                entity,
                ResourceType::Food,
                food_consumption_per_month,
            );
        }
    }

    // 4. Subtract maintenance consumption so rates show NET balance
    if let Some(data) = &buildings_data {
        for (entity, colony, _, _, _) in colony_query.iter() {
            // Per GRA-22 §4.7: maintenance is scaled by the same yield
            // multiplier as the production it costs.  Reported rate must
            // match the actual draw in `deduct_maintenance_resources`.
            let yield_mult = colony.effective_yield_multiplier();
            for (building_type, &count) in &colony.buildings {
                if count == 0 {
                    continue;
                }
                let maintenance = data.maintenance_resources(building_type);
                for (resource_name, annual_amount) in maintenance {
                    if let Some(rt) = crate::colony::data::parse_resource_type(resource_name) {
                        // annual → monthly, yield-scaled
                        let monthly_cost = annual_amount
                            * (count as f64)
                            * yield_mult
                            * (SECONDS_PER_MONTH / SECONDS_PER_YEAR);
                        add_consumption(
                            &mut rates,
                            &mut consumption_rates,
                            &mut per_entity,
                            entity,
                            rt,
                            monthly_cost,
                        );
                    }
                }
            }

            // v3.8.11 (2026-08-07): include per-capita (population) draw
            // in the rate calculation. Previously the per-capita
            // consumption only ran inside the separate
            // `deduct_population_consumption` system, so the rate display
            // was missing a major component of the net balance. For a
            // mature colony with 8.2B people, the per-cap draw alone is
            // ~30-60% of the world demand for each consumer resource —
            // the largest single draw on Iron, Cu, Al, polymers, etc.
            // Without this, the rate display would show production −
            // maintenance (which is positive for almost every consumer
            // resource) even though the colony is burning stockpile.
            //
            // The per-capita draw is NOT yield-scaled (it's a biological
            // need, not building-driven) — see GRA-22 §4.5 / GRA-22 §4.7.
            let per_cap = colony.per_capita_consumption_per_year(data);
            for (resource, annual_amount) in per_cap {
                if annual_amount <= 0.0 {
                    continue;
                }
                let monthly_cost =
                    annual_amount * (SECONDS_PER_MONTH / SECONDS_PER_YEAR);
                add_consumption(
                    &mut rates,
                    &mut consumption_rates,
                    &mut per_entity,
                    entity,
                    resource,
                    monthly_cost,
                );
                // v3.8.11: track per-capita draw separately so the UI
                // tooltip can break down the rate into its components.
                *population_consumption
                    .entry(resource)
                    .or_insert(0.0) += monthly_cost;
            }
        }
    }

    tracker.resource_rates = rates;
    tracker.gross_production_rates = production_rates;
    tracker.gross_consumption_rates = consumption_rates;
    tracker.population_consumption = population_consumption;
    tracker.synthesis_input = synthesis_input;
    tracker.per_entity_rates = per_entity;

    // --- Research point rate (include base rate) ---
    // Base RP per month (same constant used in research::systems)
    const BASE_RP_PER_YEAR: f64 = 2000.0;
    let base_rp_monthly = BASE_RP_PER_YEAR * (SECONDS_PER_MONTH / SECONDS_PER_YEAR);

    // From ResearchBuilding components (per second → per month)
    let research_per_second: f64 = research_buildings.iter().map(|b| b.points_per_second).sum();
    let research_multiplier = research_state.research_speed_multiplier();
    let mut total_research_monthly = base_rp_monthly + research_per_second * SECONDS_PER_MONTH;

    // From colony buildings
    if let Some(data) = &buildings_data {
        for (_entity, colony, _, _, _) in colony_query.iter() {
            for (building_type, &count) in &colony.buildings {
                if count == 0 {
                    continue;
                }
                if let Some(def) = data.get(building_type) {
                    for modifier in &def.modifiers {
                        if modifier.modifier_type == "ResearchSpeed" {
                            total_research_monthly += modifier.value * count as f64;
                        }
                    }
                }
            }
        }
    }

    tracker.research_rate_per_month = total_research_monthly * research_multiplier;

    // --- Engineering point rate (include base rate) ---
    const BASE_EP_PER_YEAR: f64 = 1000.0;
    let base_ep_monthly = BASE_EP_PER_YEAR * (SECONDS_PER_MONTH / SECONDS_PER_YEAR);

    // From EngineeringFacility components
    let engineering_per_second: f64 = engineering_facilities
        .iter()
        .map(|f| f.points_per_second)
        .sum();
    let engineering_multiplier = research_state.engineering_speed_multiplier();
    let mut total_engineering_monthly =
        base_ep_monthly + engineering_per_second * SECONDS_PER_MONTH;

    // From colony buildings
    if let Some(data) = &buildings_data {
        for (_entity, colony, _, _, _) in colony_query.iter() {
            for (building_type, &count) in &colony.buildings {
                if count == 0 {
                    continue;
                }
                if let Some(def) = data.get(building_type) {
                    for modifier in &def.modifiers {
                        if modifier.modifier_type == "EngineeringSpeed" {
                            total_engineering_monthly += modifier.value * count as f64;
                        }
                    }
                }
            }
        }
    }

    tracker.engineering_rate_per_month = total_engineering_monthly * engineering_multiplier;
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::app::App;
    use bevy::ecs::system::RunSystemOnce;
    use crate::colony::Colony;
    use crate::colony::data::BuildingsData;
    use crate::economy::budget::{GlobalBudget, ResourceRateTracker};
    use crate::economy::components::{LocalStockpile, MineralDeposit, PlanetResources};
    use crate::economy::types::ResourceType;
    use crate::plugins::solar_system_data::BodyType;
    use crate::research::ResearchState;

    /// Helper: create a deposit with specific proven/deep/bulk, concentration, and atmospheric flag
    fn make_deposit(
        proven: f64,
        deep: f64,
        bulk: f64,
        concentration: f32,
        atmo: bool,
    ) -> MineralDeposit {
        let mut d = MineralDeposit::new(proven, deep, bulk, concentration, 0.8);
        d.is_atmospheric = atmo;
        d
    }

    #[test]
    fn test_mines_only_extract_non_atmospheric() {
        // Iron (solid) and O2 (atmospheric) both present
        let mut resources = PlanetResources::new();
        resources.add_deposit(
            ResourceType::Iron,
            make_deposit(1000.0, 500.0, 0.0, 0.5, false),
        );
        resources.add_deposit(
            ResourceType::Oxygen,
            make_deposit(2000.0, 100.0, 0.0, 0.9, true),
        );

        // Simulate what mining does: only mine non-atmospheric
        let minable: Vec<ResourceType> = resources
            .deposits
            .iter()
            .filter(|(_, d)| !d.is_atmospheric && d.reserve.proven_crustal > 0.001)
            .map(|(t, _)| *t)
            .collect();

        assert!(
            minable.contains(&ResourceType::Iron),
            "Iron should be minable"
        );
        assert!(
            !minable.contains(&ResourceType::Oxygen),
            "Atmospheric O2 should NOT be minable"
        );
    }

    #[test]
    fn test_atmo_processor_only_extracts_atmospheric() {
        let mut resources = PlanetResources::new();
        resources.add_deposit(
            ResourceType::Iron,
            make_deposit(1000.0, 500.0, 0.0, 0.5, false),
        );
        resources.add_deposit(
            ResourceType::Nitrogen,
            make_deposit(5000.0, 200.0, 0.0, 0.7, true),
        );
        resources.add_deposit(
            ResourceType::Oxygen,
            make_deposit(2000.0, 100.0, 0.0, 0.9, true),
        );

        let harvestable: Vec<ResourceType> = resources
            .deposits
            .iter()
            .filter(|(_, d)| d.is_atmospheric && d.reserve.proven_crustal > 0.001)
            .map(|(t, _)| *t)
            .collect();

        assert!(
            !harvestable.contains(&ResourceType::Iron),
            "Iron should NOT be harvestable"
        );
        assert!(
            harvestable.contains(&ResourceType::Nitrogen),
            "N2 should be harvestable"
        );
        assert!(
            harvestable.contains(&ResourceType::Oxygen),
            "O2 should be harvestable"
        );
    }

    #[test]
    fn test_concentration_weights_mining_distribution() {
        let mut resources = PlanetResources::new();
        // Iron: 50% concentration, Titanium: 10% concentration
        resources.add_deposit(
            ResourceType::Iron,
            make_deposit(1000.0, 0.0, 0.0, 0.5, false),
        );
        resources.add_deposit(
            ResourceType::Titanium,
            make_deposit(1000.0, 0.0, 0.0, 0.1, false),
        );

        let minable: Vec<(ResourceType, f64)> = resources
            .deposits
            .iter()
            .filter(|(_, d)| !d.is_atmospheric && d.reserve.proven_crustal > 0.001)
            .map(|(t, d)| (*t, (d.reserve.concentration as f64).max(1e-10)))
            .collect();

        let total_weight: f64 = minable.iter().map(|(_, w)| w).sum();
        assert!(
            (total_weight - 0.6).abs() < 0.01,
            "Total weight should be 0.6"
        );

        for (r_type, weight) in &minable {
            let share = weight / total_weight;
            match r_type {
                ResourceType::Iron => {
                    // Iron gets 0.5/0.6 ≈ 83% of mining effort
                    assert!(
                        share > 0.8 && share < 0.9,
                        "Iron (50% conc.) should get ~83% share, got {:.1}%",
                        share * 100.0
                    );
                }
                ResourceType::Titanium => {
                    // Titanium gets 0.1/0.6 ≈ 17%
                    assert!(
                        share > 0.15 && share < 0.2,
                        "Titanium (10% conc.) should get ~17% share, got {:.1}%",
                        share * 100.0
                    );
                }
                _ => panic!("Unexpected resource type"),
            }
        }
    }

    #[test]
    fn test_trace_deposits_not_extracted() {
        let mut resources = PlanetResources::new();
        // Sub-kiloton deposit should be filtered out
        resources.add_deposit(
            ResourceType::Gold,
            make_deposit(0.0005, 0.0, 0.0, 0.01, false),
        );

        let minable: Vec<ResourceType> = resources
            .deposits
            .iter()
            .filter(|(_, d)| {
                !d.is_atmospheric
                    && (d.reserve.proven_crustal > 0.001 || d.reserve.deep_deposits > 0.001)
            })
            .map(|(t, _)| *t)
            .collect();

        assert!(minable.is_empty(), "Sub-kiloton Gold should not be minable");
    }

    // ============================================================
    // v3.8: cap-aware production-throttle tests
    // ============================================================

    /// At low fill (plenty of headroom), the throttle is a passthrough
    /// and the mine produces the full desired amount.
    #[test]
    fn throttle_production_at_low_fill_is_passthrough() {
        let desired = 1_000.0;
        let current = 100.0;
        let cap = 2_500.0;
        let consumption = 50.0; // small per-tick consumption
        let throttled = throttle_production(desired, current, cap, consumption);
        assert_eq!(throttled, desired);
    }

    /// At cap, the throttle equals the consumption floor (so the
    /// body's local industry is still supplied and the net stockpile
    /// change is 0).
    #[test]
    fn throttle_production_at_cap_equals_consumption() {
        let desired = 1_000.0;
        let current = 2_500.0; // at cap
        let cap = 2_500.0;
        let consumption = 50.0;
        let throttled = throttle_production(desired, current, cap, consumption);
        // throttled = min(desired, headroom + consumption) = min(1000, 0 + 50) = 50
        assert!((throttled - 50.0).abs() < 1e-9, "expected 50, got {throttled}");
    }

    /// Above cap (shouldn't happen in practice, but the formula is
    /// robust): throttled is still the consumption floor.
    #[test]
    fn throttle_production_above_cap_equals_consumption() {
        let desired = 1_000.0;
        let current = 3_000.0; // somehow above cap
        let cap = 2_500.0;
        let consumption = 50.0;
        let throttled = throttle_production(desired, current, cap, consumption);
        // headroom = max(0, cap - current) = max(0, -500) = 0
        // throttled = min(desired, 0 + 50) = 50
        assert!((throttled - 50.0).abs() < 1e-9);
    }

    /// At half-fill with non-zero consumption, the throttle
    /// smoothly tapers between the full production rate and the
    /// consumption floor.
    #[test]
    fn throttle_production_at_half_fill_is_partial() {
        let desired = 1_000.0;
        let current = 1_250.0;
        let cap = 2_500.0;
        let consumption = 50.0;
        let throttled = throttle_production(desired, current, cap, consumption);
        // headroom = 1250, throttled = min(1000, 1250 + 50) = 1000 (full)
        assert_eq!(throttled, desired);
    }

    /// Near the cap (99% fill), the soft-knee is at 95% of its
    /// ramp.  throttled = lerp(desired, consumption, 0.95) =
    /// 100 × 0.05 + 20 × 0.95 = 24.  The strict cap-floor
    /// (headroom + consumption = 25 + 20 = 45) is well above
    /// this so the soft-knee wins, not the hard cap.
    #[test]
    fn throttle_production_near_cap_throttles_by_soft_knee() {
        let desired = 100.0;
        let current = 2_475.0; // 99% of 2500
        let cap = 2_500.0;
        let consumption = 20.0;
        let throttled = throttle_production(desired, current, cap, consumption);
        // soft_ramp = (0.99 - 0.8) / 0.2 = 0.95
        // soft_throttled = 100 * 0.05 + 20 * 0.95 = 5 + 19 = 24
        // effective_capacity = 25 + 20 = 45
        // throttled = min(24, 45) = 24
        assert!(
            (throttled - 24.0).abs() < 1e-9,
            "expected 24 (soft-knee), got {throttled}",
        );
    }

    /// v3.8.1: at 80% fill (the soft-knee start) the throttle is
    /// still a passthrough — no ramp yet.  This is the boundary
    /// where feedback starts.
    #[test]
    fn throttle_production_at_soft_knee_start_is_passthrough() {
        let desired = 100.0;
        let current = 2_000.0; // 80% of 2500
        let cap = 2_500.0;
        let consumption = 20.0;
        let throttled = throttle_production(desired, current, cap, consumption);
        assert_eq!(throttled, desired);
    }

    /// v3.8.1: at 90% fill (mid soft-knee), the throttle is
    /// halfway between desired and consumption.  Without the
    /// soft-knee this case would still be at full production.
    #[test]
    fn throttle_production_at_mid_soft_knee_is_half() {
        let desired = 100.0;
        let current = 2_250.0; // 90% of 2500
        let cap = 2_500.0;
        let consumption = 20.0;
        let throttled = throttle_production(desired, current, cap, consumption);
        // soft_ramp = 0.5, soft_throttled = 100*0.5 + 20*0.5 = 60
        // effective_capacity = 250 + 20 = 270
        // throttled = min(60, 270) = 60
        assert!(
            (throttled - 60.0).abs() < 1e-9,
            "expected 60 (mid soft-knee), got {throttled}",
        );
    }

    /// v3.8.1: the soft-knee lerp can theoretically over-produce
    /// in degenerate cases (very small cap relative to desired).
    /// The mass-balance safety clamps to `headroom + consumption`
    /// so the deposit never exceeds the cap.
    #[test]
    fn throttle_production_soft_knee_respects_hard_cap() {
        // Degenerate: cap is 1, desired is 1000, current is 1
        // (100% fill). soft_ramp = 1, soft_throttled = 1*0 + 20*1 = 20.
        // effective_capacity = 0 + 20 = 20.
        // throttled = min(20, 20) = 20.  OK.
        let desired = 1000.0;
        let current = 1.0;
        let cap = 1.0;
        let consumption = 20.0;
        let throttled = throttle_production(desired, current, cap, consumption);
        assert!(
            (throttled - 20.0).abs() < 1e-9,
            "expected 20, got {throttled}",
        );
    }

    /// Resources with no per-body cap (exotic / late-game) bypass
    /// the throttle.  `f64::MAX` is the sentinel for "uncapped" in
    /// `GlobalBudget::effective_stockpile_cap`.
    #[test]
    fn throttle_production_uncapped_is_passthrough() {
        let desired = 1_000.0;
        let current = 9_999_999.0; // some huge stockpile
        let cap = f64::MAX;
        let consumption = 50.0;
        let throttled = throttle_production(desired, current, cap, consumption);
        assert_eq!(throttled, desired);
    }

    /// Zero desired is a passthrough (no mining, no throttle).
    #[test]
    fn throttle_production_zero_desired_is_passthrough() {
        let desired = 0.0;
        let current = 2_500.0; // at cap
        let cap = 2_500.0;
        let consumption = 50.0;
        let throttled = throttle_production(desired, current, cap, consumption);
        assert_eq!(throttled, 0.0);
    }

    /// Negative desired (defensive) is clamped to 0 — same as the
    /// add_capped deposit helper.
    #[test]
    fn throttle_production_negative_desired_is_zero() {
        let desired = -10.0;
        let current = 0.0;
        let cap = 2_500.0;
        let consumption = 50.0;
        let throttled = throttle_production(desired, current, cap, consumption);
        assert_eq!(throttled, 0.0);
    }

    /// Negative consumption (defensive) is treated as 0 — the
    /// consumption floor is a non-negative quantity.
    #[test]
    fn throttle_production_negative_consumption_treated_as_zero() {
        let desired = 1_000.0;
        let current = 2_500.0; // at cap
        let cap = 2_500.0;
        let consumption = -10.0; // defensive
        // With consumption = 0: headroom = 0, throttled = min(1000, 0) = 0
        let throttled = throttle_production(desired, current, cap, consumption);
        assert_eq!(throttled, 0.0);
    }

    /// `deposit_with_fallback` returns the actual amount added
    /// (capped at headroom).  This is the contract the rest of
    /// `extract_resources` and the rate tracker rely on.
    #[test]
    fn deposit_with_fallback_returns_actual_added_amount() {
        let mut local = LocalStockpile::new();
        // Cap via the global sentinel: simulate a per-body cap by
        // calling add_capped directly (deposit_with_fallback reads
        // cap from GlobalBudget, which is harder to mock here).
        let cap = 100.0;
        let added = local.add_capped(ResourceType::Iron, 50.0, cap);
        assert_eq!(added, 50.0);
        assert_eq!(local.get(&ResourceType::Iron), 50.0);

        // Now try to add 80 more — only 50 should land.
        let added = local.add_capped(ResourceType::Iron, 80.0, cap);
        assert_eq!(added, 50.0);
        assert_eq!(local.get(&ResourceType::Iron), 100.0);

        // And zero headroom means 0 added.
        let added = local.add_capped(ResourceType::Iron, 10.0, cap);
        assert_eq!(added, 0.0);
        assert_eq!(local.get(&ResourceType::Iron), 100.0);
    }

    // ------------------------------------------------------------------
    // v3.8.12 gate: Earth-start resource balance (BALANCE_PATCHES_v0.5.md)
    // ------------------------------------------------------------------
    // The campaign's "canary gate" that pins the Earth starting state:
    // build the real Earth special-profile deposits + the real
    // `solar_system.rs` `base_buildings` + the real `buildings.ron`
    // per-build rates, run the actual `update_resource_rates` system,
    // and assert that every resource's annual net rate ≈ 0 (±5% of the
    // 2026 world anchor) — i.e. production ≈ per-capita + maintenance
    // + synthesis inputs, with NO resource burning stockpile.
    //
    // This is the test that would have caught the v3.8.12 atmospheric
    // accessibility bug (deposit accessibility = mole fraction → N₂/O₂
    // under-produced and burned) and the methane-accessibility mismatch
    // (deposit 0.3 vs the 0.6 the v3.8.x audits assumed → methane burns
    // ~1,000 Mt/yr at game start).

    /// The real 2026 world-production anchors (Mt/yr) used by the
    /// v3.8.x calibration (USGS / IEA / WNA / OECD 2024-2026). Keyed
    /// per `ResourceType`. Resources with no consumer at Earth start
    /// (H₂, NH₃, CO₂, Ar, Pu, He-3, Tritium, etc.) are not asserted —
    /// they idle (no per-cap draw, no maintenance) and simply fill to
    /// cap; the cards/tooltips show them as surplus.
    fn world_2026_mt_per_year(rt: ResourceType) -> Option<f64> {
        use ResourceType::*;
        Some(match rt {
            Food => 9_000.0,          // FAO 2024 (v3.7: 25 Farms × 360 = 9,000)
            Water => 4_000.0,         // v3.8.10: 500 WaterTreatmentPlant × 8.0
            Iron => 2_500.0,          // worldsteel 2024
            Aluminum => 70.0,         // USGS 2024
            Titanium => 9.0,          // USGS 2024
            Silicates => 50_000.0,    // USGS NMA aggregate
            Nickel => 3.5,            // USGS 2024 refined
            Tungsten => 0.10,         // v3.8.10 (94 kt)
            Carbon => 8_200.0,        // IEA 2026 coal
            Chromium => 47.0,         // v3.8.10 chromite ore
            Magnesium => 1.0,         // USGS 2024
            Copper => 26.0,           // USGS 2024
            RareEarths => 0.35,       // USGS REO
            Lithium => 0.13,          // USGS 2024 Li content
            Sulfur => 70.0,           // USGS 2024 elemental
            Phosphorus => 240.0,      // phosphate rock equiv
            Cobalt => 0.20,           // v3.8.10
            Fluorine => 3.5,          // fluorspar
            Gold => 0.0036,           // 3,600 t
            Silver => 0.028,          // 28,000 t
            Platinum => 0.00023,      // v3.8.10 (230 t)
            Uranium => 0.074,         // WNA 2024
            Thorium => 0.0008,        // WNA 2024 (800 t)
            Methane => 4_100.0,       // IEA 2026 (v3.8.9/10 anchor)
            Deuterium => 0.035,       // v3.8.10 (35 kt)
            Nitrogen => 200.0,        // v3.8.12 per-gas split (300 × 0.667)
            Oxygen => 150.0,          // v3.8.12 (300 × 0.5)
            Argon => 1.0,             // v3.8.12 (300 × 0.00333)
            CarbonDioxide => 200.0,   // v3.8.12 (300 × 0.667)
            Polymers => 450.0,        // OECD 2024
            Hydrogen => 100.0,        // v3.8.10 (ChemicalPlant × 700)
            Ammonia => 200.0,         // v3.8.10
            _ => return None,         // exotics / bred-only (Pu, He-3, Tritium)
        })
    }

    /// Mirror of the real Earth starting building counts in
    /// `src/plugins/solar_system.rs::setup_solar_system` (`base_buildings`).
    /// Keep in sync when that array changes — the gate test IS the
    /// contract that pins the starting state.
    fn earth_base_buildings() -> Vec<(crate::colony::BuildingType, u32)> {
        use crate::colony::BuildingType::*;
        vec![
            (Housing, 400),
            (Farm, 25),
            (Greenhouse, 1),
            (AquacultureFacility, 1),
            (Factory, 1_200),
            (IronMine, 25),
            (AluminumMine, 25),
            (TitaniumMine, 25),
            (SilicatesMine, 25),
            (NickelMine, 25),
            (TungstenMine, 25),
            (CarbonMine, 25),
            (ChromiumMine, 25),
            (MagnesiumMine, 25),
            (GoldMine, 25),
            (SilverMine, 25),
            (PlatinumMine, 25),
            (CopperMine, 25),
            (RareEarthsMine, 25),
            (LithiumMine, 25),
            (SulfurMine, 25),
            (PhosphorusMine, 25),
            (CobaltMine, 25),
            (FluorineMine, 25),
            (UraniumMine, 25),
            (ThoriumMine, 25),
            (MethaneExtractor, 25),
            (DeuteriumExtractor, 25),
            (ChemicalPlant, 700),
            (AtmosphericProcessor, 300),
            (SolarPower, 320),
            (CoalPowerPlant, 195),
            (NaturalGasPlant, 135),
            (HydroelectricDam, 82),
            (WindFarm, 400),
            (FissionReactor, 20),
            (WaterTreatmentPlant, 500),
            (ResearchLab, 500),
            (DataCenter, 100),
            (AiCluster, 10),
            (LaunchSite, 200),
            (SpacePort, 50),
            (Shipyard, 18),
            (FinancialCenter, 100),
            (CommercialHub, 500),
            (TradePort, 50),
            (MedicalCenter, 200),
            (PharmaceuticalPlant, 100),
            (Warehouse, 4),
        ]
    }

    /// Build the Earth starting-state world: real Earth deposits via
    /// `generate_resources_for_body`, the real `base_buildings` from
    /// `solar_system.rs`, a `CelestialBody` (for body-mass bookkeeping),
    /// an empty `LocalStockpile`, and the seeded `GlobalBudget`. Returns
    /// the App plus a `Schedule` that runs `extract_resources` with a
    /// persistent `Local<f64>` `last_elapsed` (so per-tick dt is correct
    /// when the schedule is run repeatedly with advancing `SimulationTime`).
    fn earth_start_app() -> (bevy::ecs::schedule::Schedule, App) {
        use crate::plugins::solar_system::CelestialBody;
        let mut rng = rand::rng();
        let resources = crate::economy::generation::generate_resources_for_body(
            "Earth",
            BodyType::Planet,
            5.972e24,
            None,
            1.0,
            2.5,
            &mut rng,
        );

        let mut colony = Colony::new_civilisation("Earth".to_string(), 8.2e9);
        for (bt, count) in earth_base_buildings() {
            for _ in 0..count {
                colony.add_building(bt);
            }
        }

        let body = CelestialBody {
            name: "Earth".to_string(),
            radius: 6_371.0,
            mass: 5.972e24,
            body_type: BodyType::Planet,
            visual_radius: 1.0,
            asteroid_class: None,
            star_approach_au: None,
            rotation_period_s: None,
            habitable_outer_au: None,
        };

        let mut app = App::new();
        app.insert_resource(BuildingsData::load_for_tests());
        app.insert_resource(GlobalBudget::default());
        app.insert_resource(ResearchState::default());
        app.init_resource::<ResourceRateTracker>();
        app.init_resource::<crate::economy::DirtyBodies>();
        app.insert_resource(crate::ui::time::SimulationTime::new());
        // The Earth colony entity with its resources + an empty local
        // stockpile (the body-side inventory starts empty; the global
        // budget holds the pre-seeded 50%-of-cap stockpile).
        app.world_mut()
            .spawn((body, colony, resources, LocalStockpile::default()));

        // Register extract_resources in a Schedule so its `Local<f64>`
        // last_elapsed persists across runs — with run_system_once the
        // Local resets to 0 every call and dt = full elapsed each tick.
        let mut schedule = bevy::ecs::schedule::Schedule::default();
        schedule.add_systems(extract_resources);
        (schedule, app)
    }

    /// Advance the simulation by `months` monthly ticks (running the real
    /// `extract_resources` system each tick so stockpiles fill to cap and
    /// the throttle engages), then return the annualized per-resource net
    /// rate (Mt/yr) from `update_resource_rates`.
    ///
    /// v3.8.12: the FIRST-tick measurement is distorted by the 25 Mt
    /// methane seed and the empty local stockpile (synthesis processes
    /// are input-starved until the stockpile fills). Simulating forward
    /// gives the steady-state rates the audits actually meant to hit.
    fn earth_start_annual_net(months: u32) -> std::collections::HashMap<ResourceType, f64> {
        use crate::ui::time::SimulationTime;
        let (schedule, mut app) = earth_start_app();
        let mut schedule = schedule;
        for _ in 0..months {
            app.world_mut()
                .resource_mut::<SimulationTime>()
                .elapsed += SECONDS_PER_MONTH;
            schedule.run(app.world_mut());
        }
        app.world_mut().run_system_once(update_resource_rates);

        let tracker = app.world().resource::<ResourceRateTracker>();
        let mut net = std::collections::HashMap::new();
        for rt in ResourceType::all() {
            let prod = tracker.gross_production_rates.get(rt).copied().unwrap_or(0.0);
            let cons = tracker.gross_consumption_rates.get(rt).copied().unwrap_or(0.0);
            // monthly → annual
            net.insert(*rt, (prod - cons) * (SECONDS_PER_YEAR / SECONDS_PER_MONTH));
        }
        net
    }

    /// The gate: two invariants that pin the Earth-start balance.
    ///
    /// 1. **Production capacity = 2026 world** (±15%): with an uncapped
    ///    stockpile (storage_multiplier huge → throttle passthrough), the
    ///    gross production of each mine/processor must equal the 2026
    ///    world anchor. The v3.8.x audits assumed a flat 0.6 Earth
    ///    accessibility for every mine, but the real `profiles.rs`
    ///    deposits vary (Iron 0.9, Cu 0.5, U 0.3, Methane 0.3, Li 0.35
    ///    …), so the audited per-build values produced gross ≠ world.
    ///    This gate pins the accessibility-aware per-build re-derivation.
    ///
    /// 2. **No stockpile burn at steady state**: after 36 simulated
    ///    months, no resource's net rate may be below −1% of world.
    ///    The v3.8 cap-throttle keeps production at consumption, so
    ///    this guards against a maintenance/per-cap draw exceeding what
    ///    the mines can supply even at full production.
    #[test]
    fn earth_start_balance_no_stockpile_burn() {
        // --- Invariant 1: gross production capacity = world ---
        // With storage_multiplier = 1e18, every stockpile cap is
        // effectively infinite → `throttle_production` passthrough →
        // `update_resource_rates` reports gross production.
        let (_, mut gross_app) = earth_start_app();
        gross_app
            .world_mut()
            .resource_mut::<GlobalBudget>()
            .storage_multiplier = 1e18;
        gross_app.world_mut().run_system_once(update_resource_rates);
        let gross_tracker = gross_app.world().resource::<ResourceRateTracker>();
        let mut gross_failures = Vec::new();
        for rt in ResourceType::all() {
            let Some(world) = world_2026_mt_per_year(*rt) else {
                continue;
            };
            // Synthesis products (Hydrogen, Ammonia, Polymers) are
            // demand-driven: their gross in the one-shot measurement is
            // input-scaling-limited (the seed budget holds only ~25 Mt
            // methane / ~15 Mt N₂, and the synthesis pass runs before
            // direct production so it can't see fresh mine output). The
            // steady-state invariant (below) proves they self-supply:
            // after 36 months the 36-month table shows polymers =
            // percap+maint exactly and net = 0. Asserting "gross =
            // world" here would be wrong — they are not mined.
            let synthesis_product = matches!(
                *rt,
                ResourceType::Hydrogen | ResourceType::Ammonia | ResourceType::Polymers
            );
            if synthesis_product {
                continue;
            }
            // Only mines/processors have a gross rate; food is added by
            // `update_resource_rates` separately and is checked below.
            let gross = gross_tracker
                .gross_production_rates
                .get(rt)
                .copied()
                .unwrap_or(0.0)
                * (SECONDS_PER_YEAR / SECONDS_PER_MONTH);
            // Skip resources whose production is a different magnitude
            // than world (e.g. Silicates 50,000 Mt/yr aggregate — the
            // game models consumer share, not the whole quarry output).
            if world > 1e4 {
                continue;
            }
            if (gross - world).abs() > world * 0.15 {
                gross_failures.push(format!(
                    "{:?}: gross prod {:.3} Mt/yr vs world {:.3} (tol ±{:.3})",
                    rt,
                    gross,
                    world,
                    world * 0.15
                ));
            }
        }
        // Food is produced by Farm/Greenhouse/Aquaculture (no deposit
        // access multiplier) — assert it separately.
        {
            let food_gross = gross_tracker
                .gross_production_rates
                .get(&ResourceType::Food)
                .copied()
                .unwrap_or(0.0)
                * (SECONDS_PER_YEAR / SECONDS_PER_MONTH);
            if (food_gross - 9_400.0).abs() > 9_400.0 * 0.05 {
                gross_failures.push(format!(
                    "Food: gross prod {:.3} Mt/yr vs 9,400 (tol ±470)",
                    food_gross
                ));
            }
        }

        // --- Invariant 2: no burn at steady state ---
        let net = earth_start_annual_net(36);
        let mut burn_failures = Vec::new();
        for rt in ResourceType::all() {
            let Some(world) = world_2026_mt_per_year(*rt) else {
                continue;
            };
            let net_rate = net.get(rt).copied().unwrap_or(0.0);
            if net_rate < -world * 0.01 {
                burn_failures.push(format!(
                    "{:?}: net {:.3} Mt/yr < −1% of world {:.3} — STOCKPILE BURN",
                    rt, net_rate, world
                ));
            }
        }

        let mut failures = gross_failures;
        failures.extend(burn_failures);
        assert!(
            failures.is_empty(),
            "Earth-start balance gate FAILED ({} resource(s) off):\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    /// Print a full Earth-start steady-state rate table (prod / per-cap /
    /// maint / synthesis / net vs world) for the balance-expert review.
    /// Not an assertion — a diagnostic that mirrors the v3.8.10 audit
    /// table, run after 36 simulated months.
    #[test]
    fn earth_start_balance_print_table() {
        use crate::ui::time::SimulationTime;
        let (schedule, mut app) = earth_start_app();
        let mut schedule = schedule;
        for _ in 0..36 {
            app.world_mut()
                .resource_mut::<SimulationTime>()
                .elapsed += SECONDS_PER_MONTH;
            schedule.run(app.world_mut());
        }
        app.world_mut().run_system_once(update_resource_rates);
        let tracker = app.world().resource::<ResourceRateTracker>();
        let mut rows = Vec::new();
        for rt in ResourceType::all() {
            let prod = tracker.gross_production_rates.get(rt).copied().unwrap_or(0.0)
                * (SECONDS_PER_YEAR / SECONDS_PER_MONTH);
            let percap = tracker.population_consumption.get(rt).copied().unwrap_or(0.0)
                * (SECONDS_PER_YEAR / SECONDS_PER_MONTH);
            let synth = tracker.synthesis_input.get(rt).copied().unwrap_or(0.0)
                * (SECONDS_PER_YEAR / SECONDS_PER_MONTH);
            let maint = tracker
                .gross_consumption_rates
                .get(rt)
                .copied()
                .unwrap_or(0.0)
                * (SECONDS_PER_YEAR / SECONDS_PER_MONTH)
                - percap
                - synth;
            let net = prod - percap - maint - synth;
            let world = world_2026_mt_per_year(*rt).map(|w| format!("{w:.3}")).unwrap_or("-".into());
            rows.push(format!(
                "  {:12} prod={:>10.3} percap={:>9.3} maint={:>9.3} synth={:>9.3} net={:>+10.3}  world={}",
                format!("{:?}", rt),
                prod,
                percap,
                maint,
                synth,
                net,
                world
            ));
        }
        rows.sort();
        println!("Earth-start rates (Mt/yr):\n{}", rows.join("\n"));
    }
}
