# Balance Patches v0.5 — Per-Resource Calibration (Lean v2)

> **Fifth deliverable from the balance-expert agent (revised pass).** v1
> was 1,955 lines with 23 new buildings, NaOH/Cl₂ new resources, and
> `He3Mine` buildable on Earth at game start. v2 is a **lean
> canary-first pass**: 9 new buildings (down from 23), 3 new
> technologies, **no new `ResourceType` entries**, and the mid-game
> He-3 chain is locked behind `lunar_colony` tech + a body-type
> restriction on `He3Mine` (Moon / GasGiant / Asteroid — the three
> body classes with He-3 deposits). Calibration math and the 10–50
> manageable-count constraint are **preserved from v1**; the lean set
> just folds most fixes into existing `Mine` / `Refinery` /
> `ChemicalPlant` / `AtmosphericProcessor` / `DeepDrill` /
> `HydrocarbonExtractor` / `StripMine` `effects` fields.
>
> **Companion docs** (do not duplicate scope here):
> * `docs/design/BALANCE_AUDIT_v0.5.md` — the per-resource audit this
>   is built on
> * `docs/design/CIVILIZATION_SATISFACTION_MODEL.md` — the satisfaction
>   state machine; §9.1 is locked
> * `docs/design/BALANCE_SCALING_STRATEGY.md` — the scaling-strategy
>   comparison (separate deliverable, **not produced here**)

## Contents

1. [TL;DR and stop conditions](#1-tldr-and-stop-conditions)
2. [Headline tier-summary table](#2-headline-tier-summary-table-read-this-first)
3. [Methodology and the 10–50 constraint](#3-methodology-and-the-1050-constraint)
4. [Early-game tier (0–50 yr, 2026-era tech)](#4-early-game-tier-050-yr-2026-era-tech)
5. [Mid-game tier (50–200 yr, fusion unlocks)](#5-mid-game-tier-50200-yr-fusion-unlocks)
6. [Late-game tier (200+ yr, K2 Kardashev)](#6-late-game-tier-200-yr-k2-kardashev)
7. [Power buildings (cross-cutting)](#7-power-buildings-cross-cutting)
8. [Implementation notes](#8-implementation-notes)
9. [Self-checks and open questions](#9-self-checks-and-open-questions)

---

## 1. TL;DR and stop conditions

### 1.1 Three-line TL;DR

1. **Almost every existing per-building production modifier is 5–50× too high** for a player who manages 10–50 buildings per body. 1 Farm = 9,000 Mt/yr today (RON) but the simulation uses a *hard-coded* 1,000 Mt/yr (`src/colony/components.rs:282`); for Earth 8.2 B to need ~25 Farms the per-build must drop to **360 Mt/yr** (per the new 1/25-of-world operator bar). The single Rust constant delta (`food_consumption_per_year` 0.0001 → 0.0000011) closes the demand loop. **Unit conversion note:** 1,100 kg/p/yr = 0.0000011 Mt/p/yr (since 1 Mt = 10⁹ kg), NOT 0.0011. v0.5.1 corrected this 1,000× error.
2. **v1 added 23 new buildings; v2 adds 9.** Most "missing-resource" fixes in v1 fold into existing `Mine` / `Refinery` / `ChemicalPlant` / `AtmosphericProcessor` / `DeepDrill` / `HydrocarbonExtractor` / `StripMine` `effects` fields as multi-resource outputs. The 9 that remain as new buildings are: closed-loop water, He-3 mining (body-restricted), 3 precious-metal mines (Au/Ag/Pt) matched to the existing `SemiconductorFab` consumer, and the four K2 exotics.
3. **The He-3 catastrophe is closed by (a) downscaling `FusionReactor` He-3 maintenance 20× to 0.5 Mt/yr, (b) adding `He3Mine` at 0.5 Mt/yr (buildable on any body with He-3 deposits: Moon, GasGiant, Asteroid), AND (c) locking the entire chain with 2 new techs:** `lunar_colony` → `He3Mine` (body-restricted to `[Moon, GasGiant, Asteroid]`) → `fusion_power` (requires `lunar_colony`) → `FusionReactor` (He-3 freight-shipped from the producer body's `LocalStockpile`). No fusion, no off-world He-3 mining, no antimatter at game start in 2026.

### 1.2 What this doc does and doesn't

* **Does.** Propose RON value changes for 18 existing buildings (17 + `SemiconductorFab` maintenance update), 9 new buildings (full spec), 3 new technologies (full prereq chain), 1 schema addition (`allowed_body_types` on `BuildingDefinition`), and 1 hard-coded Rust constant delta (`food_consumption_per_year` 0.0001 → 0.0000011, the FAO 2024 SOFA 1,100 kg/p/yr = 1.1 × 10⁻⁶ Mt/p/yr). **The hard-coded per-build values in `Colony::food_production_per_year` (`src/colony/components.rs:282-285`) are the source of truth for food production** — the RON `FoodProduction` modifier is documentation and is NOT read by the simulation. Every number is sourced.
* **Doesn't.** Implement the changes (this is a proposal doc). Produce
  the scaling strategy doc. Re-litigate the civilization model. Add
  new `ResourceType` entries. Preserve deprecated aliases. Propose
  buildings the player can't build at the given tier.

### 1.3 The 10–50 manageable-count invariant (formal)

For each resource `r` at body `b` with population `pop[b]`:

```
implied_count[r,b] = ceiling( per_capita[r] × pop[b] / per_build_production[r] )
```

For `b = Earth`, `pop = 8.2e9`. The patch **must** satisfy
`10 ≤ implied_count[r, Earth] ≤ 50` for every early-game Tier 1 + Tier 2
resource (16 resources, see §4). Mid-game and late-game resources are
exempt — they're Tier 3 / K2 and the satisfaction model filters them
accordingly (CIVILIZATION_SATISFACTION_MODEL §7).

### 1.4 Stop conditions (per the brief)

| Stop condition | Met? | Where |
|---|---|---|
| `docs/design/BALANCE_PATCHES_v0.5.md` exists (lean v2) | ✅ | this file |
| All three tiers covered | ✅ | §4 / §5 / §6 |
| Tier-summary table concrete at the top | ✅ | §2 |
| 10–50 constraint met for every early-game resource | ✅ | §4.17, §9.1 |
| Mid-game He-3 fix concrete, tech-gated, AND body-restricted | ✅ | §5.1 |
| Late-game uses grams for Antimatter, marks exotics approximate | ✅ | §6 |
| 3 new technologies spec'd with full prereq chains | ✅ | §5.1.1-§5.1.3 |
| Schema addition (`allowed_body_types`) flagged | ✅ | §8.4 |
| Implementation notes sufficient to apply without re-asking | ✅ | §8 |
| User has been pinged with the lean v2 vs v1 deltas | ⏳ | end of this task |

---

## 2. Headline tier-summary table (read this first)

This is the **table the user reads first**. Every row is justified in
detail in the per-tier sections below. "Action" is the patch shape;
"Per-build Δ" is the ratio of proposed/current per-building production
(less than 1 = scale down, greater than 1 = scale up).

| Resource | Tier | Current count (Earth) | Proposed count (Earth) | Per-build Δ | Action |
|---|---|---:|---:|---:|---|
| Food | Early | 1 Farm | 25 | ×0.040 (DOWN 25×) | Scale Farm down; fix per-capita |
| Water | Early | 0 | 25 (non-breathable) | n/a (NEW) | Add `WaterProcessor` building |
| Oxygen | Early | 0 | 34 (non-breathable) | n/a (fold) | Add `OxygenProduction` to `AtmosphericProcessor` (split sweep) |
| Nitrogen | Early | 0 | 21 | n/a (rename) | Rename `AtmosphericHarvesting` → `NitrogenHarvesting`, value 7 |
| Hydrogen | Early | 1 | 33 | ×0.030 (DOWN 33×) | Scale `ChemicalPlant.HydrogenSynthesis` down |
| Methane | Early | 1 | 25 | ×0.041 (split) | `HydrocarbonExtractor`: split `MiningEfficiency` into `MethaneProduction` (164) |
| Ammonia | Early | 1 | 33 | ×0.030 (DOWN 33×) | Scale `ChemicalPlant.AmmoniaSynthesis` down |
| Phosphorus | Early | 0 | 15 | n/a (fold) | Add `PhosphorusProduction` to `Mine` (multi-output) |
| Iron | Early | 1 | 25 | ×0.044 (DOWN 23×) | Scale `Mine.MiningEfficiency` (Fe) down 1800 → 80 |
| Aluminum | Early | 0 | 30 | n/a (fold) | Add `AluminumProduction` to `Refinery` |
| Copper | Early | 0 | 23 | n/a (fold) | Add `CopperProduction` to `Mine` (multi-output) |
| Titanium | Early | 0 | 17 | n/a (fold) | Add `TitaniumProduction` to `Refinery` |
| Silicates | Early | 2 | 25 | ×0.040 (split) | `StripMine`: split `BulkMiningEfficiency` into per-resource (Si 400) |
| Polymers | Early | 1 | 25 | ×0.040 (DOWN 25×) | Scale `ChemicalPlant.PolymerSynthesis` down |
| Nickel | Early | 0 | 21 | n/a (fold) | Add `NickelProduction` to `Mine` (multi-output) |
| Carbon | Early | 1 | 25 | ×0.040 (split) | `HydrocarbonExtractor`: add `CarbonProduction` (480) |
| Helium-3 | Mid | 0 | 10 (any He-3-deposit body, post-`lunar_colony` + `fusion_power`) | n/a (NEW + DOWN) | **NEW** `He3Mine` (body-restricted to `[Moon, GasGiant, Asteroid]`, tech-gated); `FusionReactor` He-3 10 → 0.5; tech-gated to `fusion_power` |
| Deuterium | Mid | 0 | 18 | n/a (fold) | `ChemicalPlant`: add `DeuteriumProduction` (0.18); `FusionReactor` D 5 → 0.25 |
| Tritium | Mid | 0 | 15 | ×0.05 (DOWN) | `ChemicalPlant.TritiumBreeding` 0.05 → 0.001 |
| Uranium | Mid | 0 | 18 | n/a (fold) | Add `UraniumProduction` to `DeepDrill` |
| Thorium | Mid | 0 | 16 | n/a (fold) | Add `ThoriumProduction` to `DeepDrill` |
| Plutonium | Mid | 0 | 12 | ×0.0043 (DOWN 230×) | `BreederReactor.PlutoniumBreeding` 0.23 → 0.001 |
| Lithium | Mid | 0 | 22 | n/a (fold) | Add `LithiumProduction` to `Mine` (brine-extraction slice) |
| RareEarths | Mid | 0 | 20 | n/a (fold) | Add `RareEarthsProduction` to `DeepDrill` |
| Cobalt | Mid | 0 | 18 | n/a (fold) | Add `CobaltProduction` to `Mine` (Cu-mine byproduct) |
| Sulfur | Mid | 0 | 19 | n/a (fold) | `HydrocarbonExtractor.SulfurByproduct` (5) — already in the C/CH4 split |
| Fluorine | Mid | 0 | 17 | n/a (fold) | Add `FluorineProduction` to `Mine` (fluorite) |
| Tungsten | Mid | 0 | 15 | n/a (fold) | Add `TungstenProduction` to `DeepDrill` |
| Chromium | Mid | 0 | 18 | n/a (fold) | Add `ChromiumProduction` to `Refinery` |
| Magnesium | Mid | 0 | 19 | n/a (fold) | Add `MagnesiumProduction` to `Refinery` |
| Gold / Silver / Platinum | Mid | 0 (no consumer) | 25 / 25 / 20 | n/a (NEW) | **NEW** `GoldMine` (0.0001 Mt/yr) / `SilverMine` (0.001) / `PlatinumMine` (0.00001); each added to `SemiconductorFab.maintenance_resources` |
| Argon | Mid | 0 (no consumer) | 25 | n/a (fold) | Add `ArgonProduction` (0.028 Mt/yr) to `AtmosphericProcessor`; add to `SemiconductorFab.maintenance_resources` |
| Antimatter | Late (K2) | 0 | 12 | n/a (NEW) | **NEW** `AntimatterSynthesizer`; **grams/yr**; required_tech: `kardashev_k2` |
| ExoticMatter | Late (K2) | 0 | 0 (placeholder) | n/a (NEW) | **NEW** `ExoticMatterSynthesizer`; **kg/yr**; K2-gated; "approximate" |
| Metamaterials | Late (K2) | 0 | 12 | n/a (NEW) | **NEW** `MetamaterialsFab`; K2-gated; "approximate" |
| Computronium | Late (K2) | 0 | 10 | n/a (NEW) | **NEW** `ComputroniumSubstrate`; K2-gated; "approximate" |
| **Energy (avg power)** | Early | 1 plant per source | 10–30 plants/body | varies | Per-plant GW targets in §7 |
| **RUST constant: `food_consumption_per_year`** | — | `pop × 0.0001` | `pop × 0.0000011` | ÷91 (real-world FAO 2024 SOFA 1,100 kg/p/yr = 1.1 × 10⁻⁶ Mt) | `src/colony/components.rs:300-301` |
| **HARD-CODED per-build (food_production_per_year)** | — | `Farm 1,000, GHG 500, Aqua 750, AgriDome 4` | `Farm 360, GHG 200, Aqua 200, AgriDome 4` | Farm ÷2.78, GHG ÷2.5, Aqua ÷3.75 | `src/colony/components.rs:282-285` (the RON `FoodProduction` modifier is **NOT read** — the simulation uses these hard-coded values) |

**Reading the table.** Most existing buildings need their per-build
production scaled **down** 5–50×. Most missing-resource buildings
**fold** into existing `Mine` / `Refinery` / `ChemicalPlant` /
`AtmosphericProcessor` / `DeepDrill` / `HydrocarbonExtractor` /
`StripMine` `effects` fields — 0 new buildings for the 16 mid-game
Tier 2/3 missing-resource fixes. The 9 new buildings are:
`WaterProcessor` (early, non-breathable), `He3Mine` (mid, body-restricted
to `[Moon, GasGiant, Asteroid]`, tech-gated), `GoldMine` + `SilverMine`
+ `PlatinumMine` (mid, precious-metal mining, fold consumer into the
existing `SemiconductorFab`), and the 4 K2 exotics
(late, all `kardashev_k2`-gated, all "approximate"). The mid-game He-3
chain is the only one that requires a new mid-game building AND a
body-type restriction (schema addition in §8.4). Argon is an
atmospheric byproduct of `AtmosphericProcessor` and folds into the
existing modifier stack — no dedicated `ArgonExtractor` building.

**Why scale down rather than up the per-capita demand.** The user has
already fixed the per-capita demand in the civilization model
(CIVILIZATION_SATISFACTION_MODEL §3.1; per the audit the correct
values are Food 1,100 kg/p/yr, Water 150 m³/p/yr on breathable, etc.).
The *consumption* side of the equation is correct. The *production*
side is where the calibration is wrong. The patch shrinks per-build to
the "city scale" that 1 building ≈ 1/300 of world share, which is the
operator bar in `CLAUDE.md` and the basis of the housing/commercial
building calibration already in the code
(`src/colony/components.rs:255-265` comment). 1 Farm feeds ~327 M
people (1/25 of Earth at 1,100 kg/p/yr — the new 1/25 manageable-count operator bar; the v0.5 canary-1 doc previously quoted 25 M from the 1/300 housing bar, which is now superseded), 1 Mine produces ~80 Mt/yr (1/22 of world Fe),
1 ChemicalPlant produces 3 Mt/yr of NH₃ (1/67 of world NH₃). This
matches the existing housing-capacity calibration: "1 Housing
Complex = 25 M people, so Earth needs ~335 Housing Complexes" (335
= 8.2 B / 25 M).

---

## 3. Methodology and the 10–50 constraint

### 3.1 The math

For each resource `r`:

```
demand[r, Earth]      = per_capita_real[r] × 8.2e9          # Mt/yr
per_build_target[r]   = demand[r, Earth] / target_count[r]  # Mt/yr
implied_count[r]      = demand[r, Earth] / per_build_target[r]  # ≡ target
scale_factor[r]       = per_build_target[r] / per_build_current[r]
```

`target_count` is set to 25 (middle of the 10–50 band) for the early
game, with two adjustments preserved from v1:

* **Resources with very low per-capita demand** (Phosphorus 0.56
  kg/p/yr → 4.6 Mt/yr at Earth) get `target_count = 15` because 25
  buildings with sub-Mt/yr per-build makes per-build too small to be
  a meaningful game unit (e.g. 0.18 Mt/yr per PhosphateMiner is below
  the rounding precision of the UI). Resources with high per-capita
  (Silicates 1,200 kg/p/yr → 9,840 Mt/yr) use `target_count = 25`
  since per-build is still in the 100s of Mt/yr.
* **Tier 3 / K2 resources (He-3, D, T, Au, Ag, Pt, Antimatter, …)**
  have per-capita demand = 0 (no civilian consumption) and are exempt
  from the Earth-count constraint. Their count is set by the
  *consumer* side (how many FusionReactors per body) not by
  population.

### 3.2 Real-world data sources

The full source list is consolidated in §9 of the audit. The 2026
real-world data used in this doc:

* **USGS Mineral Commodity Summaries 2026** (Jan 2026 release) for Fe,
  Al, Ti, Ni, W, Cr, Mg, Cu, REE, Li, Co, S, F, P, U, Th, Au, Ag, Pt,
  helium, coal, phosphate-rock, nitrogen, sulfur, chromium, magnesium,
  thorium, fluorine, silica.
* **FAO 2024 SOFA report** for per-capita food supply (1,100 kg/p/yr).
* **IEA Global Hydrogen Review 2024** for H₂ production (~100 Mt/yr).
* **IEA Ammonia Technology Roadmap 2022** for NH₃ (~200 Mt/yr).
* **IEA Methane Tracker 2024** for natural gas / methane (~4,100 Mt NG/yr).
* **NOAA Global Monitoring Laboratory Jan 2026** for atmospheric CO₂.
* **NUBASE2020** for nuclear data (D, T, He-3, Pu).
* **USGS Helium MCS 2024** for atmospheric He-3 (3,815 t pool).
* **OECD Global Plastics Outlook 2024** for polymer production (~450 Mt/yr).
* **IAEA & SIPRI Yearbook 2024** for civil Pu stockpiles.
* **worldsteel 2024** for steel production.
* **IRENA 2024** for geothermal capacity.
* **NASA ECLSS factsheet** for closed-loop life support (water, O₂).
* **Existing `memories/repo/real-world-2026-reserves.md`** for the
  consolidated reserves table; the audit extended, not duplicated.

Where the audit flagged a data source as TBD (Antimatter, ExoticMatter,
Metamaterials, Computronium), this doc marks the patch as "approximate"
and the user is expected to refine at K2 design review.

### 3.3 The audit's operator bar vs the patch

The audit's operator bar from `CLAUDE.md` is "1 in-game building on
Earth ≈ 2026 world production total for the dominant resource." That
bar is satisfied at the *production modifier* level: 1 Mine = 1,800 Mt
Fe/yr ≈ 72% world iron share. The problem is the player manages
**dozens** of Mines, not **one**. The patch keeps the 1-building-per-
body-share philosophy at a *smaller* scale: 1 Farm feeds ~327 M people
(1/25 of Earth at 1,100 kg/p/yr), 1 Mine produces ~80 Mt/yr (1/22 of world Fe), 1
ChemicalPlant produces 3 Mt/yr of NH₃ (1/67 of world NH₃). The
operator bar is preserved but at the *building* scale, not the
*planetary* scale — i.e. "1 building ≈ 1/300 of world share" rather
than "1 building ≈ 1 world share."

### 3.4 Per-capita reference table (proposed Rust / RON; preserved from v1)

This is the canonical per-capita demand that the satisfaction model
consumes. Already locked in the civilization model; restated here so
the patch doc is self-contained. Per-capita for the 16 early-game
resources, plus the mid-game and late-game resources:

| Resource | Per-capita (kg/p/yr) | Source |
|---|---:|---|
| Food | 1,100 | FAO 2024 SOFA |
| Water (closed-loop only) | 50 | ISS ECLSS water recovery ~93% closure loss |
| Water (breathable, withdrawal) | 150,000 (150 m³) | FAO Aquastat |
| Oxygen (respiration) | 840 | NASA ECLSS: 0.84 kg/p/day × 365 |
| Nitrogen (industrial) | 18 | USGS N (industrial Haber-Bosch input) |
| Hydrogen (NH₃ feedstock, refinery) | 12 | IEA H₂ Review 2024, 100 Mt/yr ÷ 8.2 B |
| Methane (NG residential+industrial) | 500 | IEA Methane Tracker, 4,100 Mt NG/yr ÷ 8.2 B |
| Ammonia (fertilizer N) | 24 | IEA Ammonia, 200 Mt NH₃/yr ÷ 8.2 B |
| Phosphorus (P element) | 0.56 | USGS Phosphate Rock, 4.6 Mt P/yr ÷ 8.2 B |
| Iron (contained) | 305 | USGS Iron Ore 2026, 2,500 Mt/yr ÷ 8.2 B |
| Aluminum | 8.5 | USGS Bauxite & Alumina 2026, 70 Mt/yr ÷ 8.2 B |
| Copper | 2.8 | USGS Copper MCS 2026, 23 Mt/yr ÷ 8.2 B |
| Titanium | 0.04 | USGS Titanium MCS 2026, 0.35 Mt/yr ÷ 8.2 B |
| Silicates (aggregate) | 1,200 | USGS Silica 2024, ~10,000 Mt sand/yr |
| Polymers (plastic resin) | 55 | OECD Plastics Outlook 2024, 450 Mt/yr ÷ 8.2 B |
| Nickel | 0.45 | USGS Nickel MCS 2026, 3.7 Mt/yr ÷ 8.2 B |
| Carbon (fossil) | 1,460 | USGS Coal MCS 2024 + oil + gas, 12,000 Mt/yr ÷ 8.2 B |
| RareEarths | 0.037 | USGS REE 2026, 0.3 Mt/yr ÷ 8.2 B |
| Lithium | 0.022 | USGS Lithium 2026, 0.18 Mt/yr ÷ 8.2 B |
| Cobalt | 0.028 | USGS Cobalt 2026, 0.23 Mt/yr ÷ 8.2 B |
| Sulfur | 8.4 | USGS Sulfur 2024, 69 Mt/yr ÷ 8.2 B |
| Fluorine | 0.55 | USGS Fluorspar 2024, 4.5 Mt/yr ÷ 8.2 B |
| Tungsten | 0.0095 | USGS Tungsten 2026, 0.078 Mt/yr ÷ 8.2 B |
| Chromium | 3.7 | USGS Chromium 2024, 30 Mt/yr ÷ 8.2 B |
| Magnesium | 0.13 | USGS Magnesium 2024, 1.1 Mt/yr ÷ 8.2 B |
| Gold | 0.00037 | USGS Gold MCS 2026, 0.003 Mt/yr ÷ 8.2 B (no consumer) |
| Silver | 0.0032 | USGS Silver MCS 2026, 0.026 Mt/yr ÷ 8.2 B (no consumer) |
| Platinum | 0.000024 | USGS Platinum MCS 2026, 0.0002 Mt/yr ÷ 8.2 B (no consumer) |
| Helium-3 | 6×10⁻¹⁰ | USGS Helium MCS 2024 (essentially 0) |
| Deuterium | 0.004 | Culham / IEA fusion review |
| Tritium | 5×10⁻¹⁰ | NUBASE2020 (essentially 0) |
| Uranium | 0.0073 | USGS Uranium MCS 2026, 0.060 Mt/yr ÷ 8.2 B |
| Thorium | 0.00012 | USGS Thorium MCS 2024 |
| Plutonium | 0.0000024 | IAEA civil stockpile |
| Argon | 0.085 | USGS Helium & Noble Gases 2024 |
| CO₂ | n/a (output, not input) | NOAA 2026 |
| Antimatter / ExoticMatter / Metamaterials / Computronium | (no real anchor) | deferred to K2 design review |

### 3.5 Manageable-count exceptions

The 10–50 invariant is hard for early-game resources and soft for
mid-game. Two early-game exceptions are flagged in §4:

* **Phosphorus** — target_count = 15 not 25; per-build = 0.31 Mt/yr.
  Below the audit's "1 building ≈ 1/300 of world" bar but phosphorus
  is genuinely scarce (USGS reserves 50 yr at current draw). The game
  should reflect the scarcity rather than smooth it.
* **Titanium** — target_count = 17; per-build = 0.019 Mt/yr (19 kt/yr).
  This is *below* the granularity of the game unit (Mt) and is
  intentional. The patch rounds 0.019 to 0.02 Mt/yr per build, and
  flags the rounding precision as a known limitation (no UI panel
  displays tonnes-vs-Mt at this scale). User can refine.

All other early-game resources land cleanly in 10–50.

### 3.6 Tier weights (from `CIVILIZATION_SATISFACTION_MODEL.md` §2.4)

The satisfaction model gives each resource a *tier weight*:

| Tier | Definition | Weight | Examples |
|------|------------|-------:|----------|
| 1 | Life support — failure kills | 3.0 | Food, Water, O₂, N₂ |
| 2 | Energy / structural metals | 1.0 | Iron, Al, Ti, Cu, Ni, Cr, Mg, Si, C, H₂, CH₄, NH₃, U, Th, Li, S, P, Polymers, Tungsten, REE, Co, F, N₂, CO₂, Ar |
| 3 | Precious / catalytic | 0.3 | Au, Ag, Pt, He-3, D, T, Pu |
| 4 | K2 exotic (always excluded until K2.0) | 0.0 | Antimatter, ExoticMatter, Metamaterials, Computronium |

> **Why weight life-support 3×.** The 2007–2008 world food price
> crisis triggered unrest in 30+ countries (World Bank, FAO 2008).
> The 2011 Arab Spring was triggered by a 30% jump in bread prices —
> *food*, not *iron*. A satisfied population needs calories first,
> infrastructure second, luxuries last.

---

## 4. Early-game tier (0–50 yr, 2026-era tech)

**Primary focus.** 16 resources, all required-tech `""` (no tech gate)
or `basic_*` (tier 1, always-available baseline). For each resource: a
calibration block with the same shape — real-world anchor, current game
value, proposed game value, scale factor, why. The big change vs
v1: most missing-resource fixes **fold** into existing building
`effects` fields, not new buildings. New buildings in this tier:
**only `WaterProcessor`** (the audit's finding #5: no water-mining
building). Oxygen folds into `AtmosphericProcessor` by splitting its
single sweep modifier into per-gas effects. The 16 mid-game missing-
resource fixes in v1 (PhosphateMiner, AluminaRefinery, CopperMine,
TitaniumSmelter, NickelMine, UraniumMine, ThoriumMine, LithiumBrinePond,
REEProcessingPlant, CobaltRefinery, FluoriteMiner, TungstenMiner,
ChromiteMiner, MagnesiaRefinery, DeuteriumHarvester) **all fold** into
`Mine` / `Refinery` / `ChemicalPlant` / `DeepDrill` multi-output
effects (see §5 and §8.3).

### 4.1 Food (Tier 1, life support)

| Field | Value |
|---|---|
| Real-world per-capita | **1,100 kg/p/yr** (FAO 2024 SOFA, world food supply) |
| Earth demand (8.2 B) | 9,020 Mt/yr |
| Current per-build | **9,000 Mt/yr** (`Farm`, `modifier_type: "FoodProduction"`) |
| Current implied count | 1 Farm (1 building feeds 8.2 B) |
| Target count | 25 |
| Proposed per-build | **360 Mt/yr** |
| Scale factor | ×0.040 (DOWN 25×) |
| Source | FAO 2024 SOFA report, fao.org/worldfoodsituation |
| Why this factor | 1 Farm feeds ~327 M people (1/25 of Earth at 1,100 kg/p/yr — the new 1/25 manageable-count bar; the v0.5 canary-1 doc previously quoted 25 M from the 1/300 housing bar, which is now superseded) |
| Companion RON edit | `Farm.modifiers[FoodProduction]` 9000 → 360; `Greenhouse.FoodProduction` 5000 → 200; `AgriDome.FoodProduction` 180 → 4.0; `AquacultureFacility.FoodProduction` 1500 → 200 (**but the simulation does NOT read these RON values — see `src/colony/components.rs:282-285` for the hard-coded values that the sim actually uses; the RON values are documentation only**) |
| Rust delta | **`food_consumption_per_year` 0.0001 → 0.0000011** (`src/colony/components.rs:300-301`); **`Farm` hard-coded 1,000 → 360** (`src/colony/components.rs:282`); **`Greenhouse` hard-coded 500 → 200** (line 284); **`AquacultureFacility` hard-coded 750 → 200** (line 285) |

**The per-capita 0.0001 → 0.0000011 change is the single most important Rust edit.**
It is the source of the audit's finding #1 ("per-capita food demand
is 91× below real world"). With this single change, 1 Farm at 360
Mt/yr feeds ~330 M people — i.e. Earth needs ~25 Farms, not 1, and
the 10–50 constraint is satisfied.

### 4.2 Water (Tier 1, life support)

| Field | Value |
|---|---|
| Real-world per-capita | **50 kg/p/yr** (closed-loop life support loss, ISS ECLSS 93% water recovery, NASA factsheet) |
| Earth demand (8.2 B, closed-loop only) | 410 Mt/yr (this is the *non-breathable* figure; on Earth, water is hydrological cycle) |
| Current per-build | **n/a** (no dedicated water-mining building; `WaterTreatmentPlant` is a `PopulationGrowth` modifier only) |
| Current implied count | n/a (audit finding #5) |
| Target count | 25 |
| Proposed per-build | **16 Mt/yr** (new `WaterProcessor` modifier `WaterProduction`) |
| Scale factor | n/a (NEW building) |
| Source | NASA ECLSS water-recovery factsheet (nasa.gov/mission_pages/station/spacewalks/facts); FAO Aquastat (fao.org/aquastat) for per-capita withdrawal on breathable |
| Why new building | The audit's finding #5 flags 15 resources with no mining building. Water is the #1 volatile. A `WaterProcessor` is the closed-loop equivalent of the Farm: takes regolith ice / atmospheric condensate / seawater and outputs water. |
| Companion RON edit | **NEW building** `WaterProcessor` (full spec in §8.2.1). For breathable bodies, `WaterProcessor` is auto-disabled (analogous to `Farm`'s `available_atmospheres: [None]`). For non-breathable, the player needs ~25 per 1 B pop. |
| Rust delta | None. The current `ColonyEnvironmentCosts.water_per_person_per_year` 0.00005 (50 kg/p/yr) is already correct; the model currently only fires on outposts, which is the right behavior. |

**Why 16 Mt/yr not the audit's 50 kg/p/yr × 25 buildings = 1.25 Mt/yr.**
The 10–50 constraint says 25 buildings at 8.2 B / closed-loop. But
the closed-loop figure (50 kg/p/yr) is the *loss rate* in a 93%
recycler; in a 99% recycler the loss is ~5 kg/p/yr. A 16 Mt/yr
processor serves a 320 M-person colony at 50 kg/p/yr. 25 such
processors per body × 320 M = 8 B, which is the 10–50 constraint
satisfied for an Earth-class body. Smaller colonies need fewer.

### 4.3 Oxygen (Tier 1, life support) — folded into `AtmosphericProcessor`

| Field | Value |
|---|---|
| Real-world per-capita | **840 kg/p/yr** (NASA ECLSS: 0.84 kg/p/day respiration; nasa.gov/iss-science) |
| Earth demand (8.2 B) | 6,888 Mt/yr (this is the *non-breathable* figure; on breathable Earth, atmospheric O₂ is "free") |
| Current per-build | **0** dedicated; `AtmosphericProcessor` harvests all gases together at 500 Mt/yr (undifferentiated across N₂/O₂/Ar/CO₂) |
| Current implied count | n/a (no per-gas accounting) |
| Target count | 34 |
| Proposed per-build | **200 Mt/yr** (new `OxygenProduction` modifier on existing `AtmosphericProcessor`) |
| Scale factor | n/a (fold) |
| Source | NASA ECLSS factsheet, 0.84 kg/p/day |
| Why fold (not new `OxygenExtractor` building like v1) | O₂ and N₂ are *both* products of cryogenic atmospheric distillation on a body with atmosphere. They share the same compression train, just different cold traps. Splitting into a separate `OxygenExtractor` building would duplicate ~80% of the maintenance chain (Fe, Cu, F, power) for no real-world reason. v1's `OxygenExtractor` is dropped in favor of a per-gas split on `AtmosphericProcessor`. |
| Companion RON edit | `AtmosphericProcessor.modifiers`: replace `AtmosphericHarvesting: 500` with `NitrogenHarvesting: 7` + `OxygenProduction: 200` (CO₂ and Ar are left as byproducts of the same process — no separate modifier; audit §4.3 finding). |
| Rust delta | None. The current `ColonyEnvironmentCosts.oxygen_per_person_per_year` 0.0001 is 100 kg/p/yr which is 8× too low (real is 840 kg/p/yr). The patch proposes a *new* per-capita field: `oxygen_per_capita_respiration` 0.00084 (= 840 kg/p/yr) for non-breathable bodies. The existing 0.0001 stays as the *recreation/safety-mask* draw and the new 0.00084 is the dominant draw. Deferred to v0.5.x Rust follow-up. |

### 4.4 Nitrogen (Tier 1, life support) — renamed modifier

| Field | Value |
|---|---|
| Real-world per-capita | **18 kg/p/yr** (USGS Nitrogen MCS 2024, industrial NH₃ feedstock) |
| Earth demand (8.2 B) | 148 Mt/yr |
| Current per-build | **0** dedicated (AtmosphericProcessor sweep, see §4.3) |
| Current implied count | n/a |
| Target count | 21 |
| Proposed per-build | **7 Mt/yr** (rename AtmosphericProcessor modifier to `NitrogenHarvesting`, value 7) |
| Scale factor | n/a (renamed modifier; effective 500 → 7, but this is a *new* resource-specific accounting) |
| Source | USGS Nitrogen MCS 2024 (industrial N₂ for Haber-Bosch) |
| Why rename | Same as Oxygen. The sweep must be split to give per-gas production modifiers. The `AtmosphericHarvesting` name is meaningless; v2 makes it resource-specific. **The rename is breaking — no deprecated alias preserved per the v0.5 design rule.** |
| Companion RON edit | `AtmosphericProcessor.modifiers[AtmosphericHarvesting]` 500 → 7, **renamed** `NitrogenHarvesting`. Add NEW `OxygenProduction` (see §4.3). CO₂ and Ar are left as byproducts of the same process. |
| Rust delta | None. Nitrogen per-capita is a new field; for now the patch documents the value but does not implement the Rust hook. Deferred to v0.5.x follow-up. |

### 4.5 Hydrogen (Tier 2, structural feedstock)

| Field | Value |
|---|---|
| Real-world per-capita | **12 kg/p/yr** (IEA H₂ Review 2024, 100 Mt/yr ÷ 8.2 B) |
| Earth demand (8.2 B) | 98 Mt/yr |
| Current per-build | **100 Mt/yr** (`ChemicalPlant.modifiers[HydrogenSynthesis]`) |
| Current implied count | 1 (ChemicalPlant) |
| Target count | 33 |
| Proposed per-build | **3 Mt/yr** |
| Scale factor | ×0.030 (DOWN 33×) |
| Source | IEA Global Hydrogen Review 2024 (iea.org/reports/hydrogen) |
| Why | The audit's tier 1 conclusion: H₂ is bulk feedstock for NH₃ (Haber-Bosch) and refinery. A per-capita of 12 kg/p/yr is correct. 1 ChemicalPlant at 3 Mt/yr H₂ is a city-scale Haber-Bosch train. |
| Companion RON edit | `ChemicalPlant.modifiers[HydrogenSynthesis]` 100 → 3 |

### 4.6 Methane (Tier 2, fuel / feedstock)

| Field | Value |
|---|---|
| Real-world per-capita | **500 kg/p/yr** (IEA Methane Tracker 2024, 4,100 Mt NG/yr ÷ 8.2 B) |
| Earth demand (8.2 B) | 4,100 Mt/yr |
| Current per-build | **0** dedicated (HydrocarbonExtractor outputs 4,000 Mt/yr as generic "MiningEfficiency" covering oil+gas+coal) |
| Current implied count | 1 HydrocarbonExtractor |
| Target count | 25 |
| Proposed per-build | **164 Mt/yr** (split HydrocarbonExtractor into per-resource modifiers) |
| Scale factor | ×0.041 (DOWN 25×) on the gas slice |
| Source | IEA Methane Tracker 2024 |
| Why split | HydrocarbonExtractor today treats oil+gas+coal as one undifferentiated `MiningEfficiency` value 4,000. The audit §4.7 calls this out. The patch splits into per-resource modifiers: `MethaneProduction` 164 Mt/yr, `CarbonProduction` 200 Mt/yr, and removes the legacy undifferentiated modifier. |
| Companion RON edit | `HydrocarbonExtractor.modifiers[MiningEfficiency]` 4000 → remove. Add `(modifier_type: "MethaneProduction", value: 164.0)`, `(modifier_type: "CarbonProduction", value: 480.0)`, `(modifier_type: "SulfurByproduct", value: 5.0)` (see §4.15 for S rationale). |
| Rust delta | None. The existing per-capita methane model doesn't exist yet; the patch defers the Rust hookup to a v0.5.x follow-up. |

### 4.7 Ammonia (Tier 2, fertilizer)

| Field | Value |
|---|---|
| Real-world per-capita | **24 kg/p/yr** (IEA Ammonia Tech Roadmap 2022, 200 Mt NH₃/yr ÷ 8.2 B) |
| Earth demand (8.2 B) | 197 Mt/yr |
| Current per-build | **200 Mt/yr** (`ChemicalPlant.modifiers[AmmoniaSynthesis]`) |
| Current implied count | 1 |
| Target count | 33 |
| Proposed per-build | **6 Mt/yr** |
| Scale factor | ×0.030 (DOWN 33×) |
| Source | IEA Ammonia Technology Roadmap 2022 (iea.org/reports/ammonia) |
| Why | Haber-Bosch NH₃ from H₂ + N₂. Real-world 60% of NH₃ goes to fertilizer, supporting food production for ~50% of world population. 1 ChemicalPlant at 6 Mt/yr NH₃ is a city-scale fertilizer train. |
| Companion RON edit | `ChemicalPlant.modifiers[AmmoniaSynthesis]` 200 → 6 |

### 4.8 Phosphorus (Tier 1, life-support — closed-loop agriculture)

| Field | Value |
|---|---|
| Real-world per-capita | **0.56 kg/p/yr** (USGS Phosphate Rock MCS 2026, 4.6 Mt P/yr ÷ 8.2 B) |
| Earth demand (8.2 B) | 4.6 Mt/yr |
| Current per-build | **0** (no mining; only consumer is `AgriDome.maintenance_resources[Phosphorus]`) |
| Current implied count | n/a (audit finding #5) |
| Target count | **15** (exception — see §3.5) |
| Proposed per-build | **0.31 Mt/yr** (NEW `PhosphorusProduction` modifier on existing `Mine`) |
| Scale factor | n/a (fold) |
| Source | USGS Phosphate Rock MCS 2026 (pubs.usgs.gov/periodicals/mcs2026) |
| Why exception | Per-capita is 0.56 kg/p/yr. At 25 buildings, per-build = 0.18 Mt/yr (180 kt/yr) which is below the audit's 1 building ≈ 1/300 world bar (0.015 Mt/yr of P per 1/300 of 4.6 Mt/yr = 0.015 Mt/yr). 15 buildings at 0.31 Mt/yr = 0.31 Mt/yr which is 1/15 of world, a meaningful "district mine." |
| Companion RON edit | Add `(modifier_type: "PhosphorusProduction", value: 0.31)` to `Mine` (multi-output). |
| Rust delta | New per-capita field `phosphorus_per_capita_agriculture` 0.00000056 (0.56 kg/p/yr). Hooks into the satisfaction model. Deferred Rust work. |

### 4.9 Iron (Tier 2, structural metal)

| Field | Value |
|---|---|
| Real-world per-capita | **305 kg/p/yr** (USGS Iron Ore MCS 2026, 2,500 Mt contained Fe/yr ÷ 8.2 B) |
| Earth demand (8.2 B) | 2,501 Mt/yr |
| Current per-build | **1,800 Mt/yr** (`Mine`), 1,800 (`Refinery`), 5,000 (`StripMine` bulk) — total 8,000 Mt/yr across 3 building types for "iron-equivalent" |
| Current implied count | 1.4 (at Mine scale), 0.3 (at Refinery), 0.5 (at StripMine bulk) |
| Target count | 25 (each type — player builds 25 Mines *or* 25 Refineries *or* mix) |
| Proposed per-build | **80 Mt/yr** per Mine, 80 per Refinery, 80 per StripMine (for the Fe slice) |
| Scale factor | ×0.044 (DOWN 23×) on Mine; same on Refinery; StripMine split into Fe/Si/Al/Cu slices |
| Source | USGS Iron Ore MCS 2026 (pubs.usgs.gov/periodicals/mcs2026/mcs-2026-iron.pdf) |
| Why | The audit's headline finding: 1 Mine = 72% of world Fe; player economy is unbounded. Scaling to 80 Mt/yr per Mine makes 1 Mine = 1/22 of world = 1 city-block mining district. 25 Mines per body × 80 Mt/yr = 2,000 Mt/yr, which is 80% of Earth demand — within the 10–50 constraint. |
| Companion RON edit | `Mine.modifiers[MiningEfficiency]` 1800 → **80** (effective per-Mine Fe). `Refinery.modifiers[MiningEfficiency]` 1800 → 80 (refining capacity). `StripMine.modifiers[BulkMiningEfficiency]` 5000 → split into per-resource (Fe 80, Si 400, Al 2.3, Cu 1.0, Ti 0.02, Ni 0.18, Cr 1.7) — see §4.13. The legacy `MiningEfficiency` is preserved as a multi-resource fallback for the "bulk mining" generic case. |
| Rust delta | None — the existing maintenance-code path already iterates the `maintenance_resources` list. |

### 4.10 Aluminum (Tier 2, structural metal) — folded into `Refinery`

| Field | Value |
|---|---|
| Real-world per-capita | **8.5 kg/p/yr** (USGS Bauxite & Alumina MCS 2026, 70 Mt Al/yr ÷ 8.2 B) |
| Earth demand (8.2 B) | 70 Mt/yr |
| Current per-build | **0** (no mining; consumed via Shipyard/SpacePort/LaunchSite maintenance) |
| Current implied count | n/a (audit finding #5) |
| Target count | 30 |
| Proposed per-build | **2.3 Mt/yr** (NEW `AluminumProduction` modifier on `Refinery`) |
| Scale factor | n/a (fold) |
| Source | USGS Bauxite & Alumina MCS 2026; for in-game, treat as regolith-fed Bayer-process refinery (lunar regolith contains ~10% Al by mass in anorthosite) |
| Why fold (not new `AluminaRefinery` building like v1) | Aluminum is the 2nd-most-used metal in the modern world (transport + construction + packaging). The audit's #11 first-bottleneck candidate when the space economy scales. v1's separate `AluminaRefinery` building is dropped; `Refinery` already has the chemical-processing chain. |
| Companion RON edit | Add `(modifier_type: "AluminumProduction", value: 2.3)` to `Refinery`. |
| Rust delta | New per-capita field `aluminum_per_capita_construction` 0.0000085 (8.5 kg/p/yr). Deferred Rust hookup. |

### 4.11 Copper (Tier 2, electrical metal) — folded into `Mine`

| Field | Value |
|---|---|
| Real-world per-capita | **2.8 kg/p/yr** (USGS Copper MCS 2026, 23 Mt Cu/yr ÷ 8.2 B) |
| Earth demand (8.2 B) | 23 Mt/yr |
| Current per-build | **0** dedicated; `RecyclingCenter.modifiers[MiningEfficiency]` 500 covers "mixed recycled metal" undifferentiated |
| Current implied count | n/a |
| Target count | 23 |
| Proposed per-build | **1.0 Mt/yr** (NEW `CopperProduction` modifier on `Mine`) |
| Scale factor | n/a (fold) |
| Source | USGS Copper MCS 2026 (pubs.usgs.gov/periodicals/mcs2026/mcs-2026-copper.pdf) |
| Why fold (not new `CopperMine` building like v1) | The audit's #11 first scarcity bottleneck. Every electrical building maintains on Cu. 1 Cu mine feeds a city. v1's separate `CopperMine` is dropped; Cu is a porphyry co-product of the Fe ore body, and the multi-output `Mine` reflects the real-world geology. |
| Companion RON edit | Add `(modifier_type: "CopperProduction", value: 1.0)` to `Mine`. |
| Rust delta | None. The per-capita Cu draw is implicit in the building-maintenance chain; no new Rust field. |

### 4.12 Titanium (Tier 2, structural / aerospace) — folded into `Refinery`

| Field | Value |
|---|---|
| Real-world per-capita | **0.04 kg/p/yr** (USGS Titanium MCS 2026, 0.35 Mt Ti sponge/yr ÷ 8.2 B) |
| Earth demand (8.2 B) | 0.33 Mt/yr |
| Current per-build | **0** (consumed via HabitatDome, UndergroundHabitat, Shipyard, LaunchSite maintenance) |
| Current implied count | n/a |
| Target count | **17** (exception — see §3.5) |
| Proposed per-build | **0.02 Mt/yr** (NEW `TitaniumProduction` modifier on `Refinery`) |
| Scale factor | n/a (fold) |
| Source | USGS Titanium MCS 2026 (pubs.usgs.gov/periodicals/mcs2026/mcs-2026-titanium.pdf); for in-game, treat as ilmenite-smelt (Kroll process) |
| Why exception | Per-capita is 0.04 kg/p/yr = 40 g/p/yr. At 25 buildings per-build = 0.013 Mt/yr (13 kt/yr) which rounds poorly in the UI. 17 buildings at 0.02 Mt/yr = 20 kt/yr, a defensible "titanium sponge smelter" size. The game-unit rounding is documented as a known limitation. |
| Companion RON edit | Add `(modifier_type: "TitaniumProduction", value: 0.02)` to `Refinery`. |
| Rust delta | None. |

### 4.13 Silicates (Tier 2, structural / construction) — `StripMine` split

| Field | Value |
|---|---|
| Real-world per-capita | **1,200 kg/p/yr** (USGS Silica 2024, 10,000 Mt sand/yr + ~40 Gt aggregate ÷ 8.2 B) |
| Earth demand (8.2 B) | 9,840 Mt/yr |
| Current per-build | **5,000 Mt/yr** (`StripMine.modifiers[BulkMiningEfficiency]`, shared with Fe/Al/Cu) |
| Current implied count | ~2 |
| Target count | 25 |
| Proposed per-build | **400 Mt/yr** (Si slice of `StripMine`) |
| Scale factor | ×0.040 (DOWN 25×) on the Si slice |
| Source | USGS Mineral Industry Surveys — Silica 2024 |
| Why | Silicates are the dominant construction material; near-infinite crustal supply. The patch keeps it "easy to get" but at city-scale per build. |
| Companion RON edit | `StripMine.modifiers[BulkMiningEfficiency]` 5000 → **remove and replace** with per-resource modifiers (Si 400, Fe 80, Al 2.3, Cu 1.0, Ti 0.02, Ni 0.18, Cr 1.7). `StripMine` = "multi-resource open pit." |
| Rust delta | None. |

### 4.14 Polymers (Tier 2, structural / manufacturing)

| Field | Value |
|---|---|
| Real-world per-capita | **55 kg/p/yr** (OECD Global Plastics Outlook 2024, 450 Mt plastic resin/yr ÷ 8.2 B) |
| Earth demand (8.2 B) | 451 Mt/yr |
| Current per-build | **450 Mt/yr** (`ChemicalPlant.modifiers[PolymerSynthesis]`) |
| Current implied count | 1 |
| Target count | 25 |
| Proposed per-build | **18 Mt/yr** |
| Scale factor | ×0.040 (DOWN 25×) |
| Source | OECD Global Plastics Outlook 2024 (oecd.org/publications/global-plastics-outlook) |
| Why | Polymers are petroleum-derived. The patch treats them as a *throughput constraint* of the C cycle (see §4.15) — they can't exceed C production. 18 Mt/yr per ChemicalPlant is 1/25 of world polymer production, a petrochemical-complex scale. |
| Companion RON edit | `ChemicalPlant.modifiers[PolymerSynthesis]` 450 → 18 |
| Rust delta | None. |

### 4.15 Carbon (Tier 2, fuel / feedstock) and fossil-derived resources

| Field | Value |
|---|---|
| Real-world per-capita | **1,460 kg/p/yr fossil-C** (USGS Coal MCS 2024 + oil + gas, 12,000 Mt fossil-C/yr ÷ 8.2 B) |
| Earth demand (8.2 B) | 11,972 Mt/yr |
| Current per-build | **4,000 Mt/yr** (`HydrocarbonExtractor.modifiers[MiningEfficiency]`, undifferentiated) |
| Current implied count | ~3 |
| Target count | 25 |
| Proposed per-build | **480 Mt/yr** (C slice of split HydrocarbonExtractor) |
| Scale factor | ×0.040 (DOWN 25×) on the C slice |
| Source | USGS Coal MCS 2024; BP Statistical Review 2024; IEA Methane Tracker 2024 |
| Why | HydrocarbonExtractor is a "fossil-C miner." Split into per-resource modifiers: C 480 Mt/yr, CH₄ 164 Mt/yr (see §4.6), S 5 Mt/yr (Claus-process byproduct). Total: 649 Mt/yr per extractor at full output. 25 extractors × 649 = 16,225 Mt/yr, which is 1.35× Earth demand — appropriate (one body has surplus to export to other bodies or stock for chemical industry). |
| Companion RON edit | `HydrocarbonExtractor.modifiers[MiningEfficiency]` 4000 → **remove**. Add `(modifier_type: "CarbonProduction", value: 480.0)`, `(modifier_type: "MethaneProduction", value: 164.0)`, `(modifier_type: "SulfurByproduct", value: 5.0)`. |
| Rust delta | None. The per-capita C demand is a new field; deferred to v0.5.x follow-up (C is the most complex resource — it's both an input to power and a feedstock for polymers). |

### 4.16 Nickel (Tier 2, structural / superalloy) — folded into `Mine`

| Field | Value |
|---|---|
| Real-world per-capita | **0.45 kg/p/yr** (USGS Nickel MCS 2026, 3.7 Mt/yr ÷ 8.2 B) |
| Earth demand (8.2 B) | 3.7 Mt/yr |
| Current per-build | **0** (consumed via Mine, Refinery, Shipyard maintenance in tiny amounts) |
| Current implied count | n/a |
| Target count | 21 |
| Proposed per-build | **0.18 Mt/yr** (NEW `NickelProduction` modifier on `Mine`) |
| Scale factor | n/a (fold) |
| Source | USGS Nickel MCS 2026 (pubs.usgs.gov/periodicals/mcs2026/mcs-2026-nickel.pdf) |
| Why fold | Ni is a superalloy critical for reactor pressure vessels, turbine blades, and the 16-SST 200-series austenitic stainless that is the modern baseline. 21 Ni mines per Earth-class body at 0.18 Mt/yr = 3.78 Mt/yr, matching demand. v1's separate `NickelMine` is dropped; Ni is a co-product of laterite / Psyche mining that the multi-output `Mine` handles. |
| Companion RON edit | Add `(modifier_type: "NickelProduction", value: 0.18)` to `Mine`. |
| Rust delta | None. |

### 4.17 Early-game summary

| Resource | New per-build (Mt/yr) | New target count | Δ per-build | Action |
|---|---:|---:|---:|---|
| Food | 360 | 25 | ×0.040 DOWN | Modify `Farm.FoodProduction` + Rust constant |
| Water | 16 | 25 | NEW | Add `WaterProcessor` |
| Oxygen | 200 | 34 | NEW (fold) | Add `OxygenProduction` to `AtmosphericProcessor` |
| Nitrogen | 7 | 21 | RENAMED | Rename `AtmosphericHarvesting` → `NitrogenHarvesting`, value 7 |
| Hydrogen | 3 | 33 | ×0.030 DOWN | Modify `ChemicalPlant.HydrogenSynthesis` |
| Methane | 164 | 25 | ×0.041 (split) | Modify `HydrocarbonExtractor` (add `MethaneProduction` modifier) |
| Ammonia | 6 | 33 | ×0.030 DOWN | Modify `ChemicalPlant.AmmoniaSynthesis` |
| Phosphorus | 0.31 | 15 | NEW (fold) | Add `PhosphorusProduction` to `Mine` |
| Iron | 80 | 25 | ×0.044 DOWN | Modify `Mine.MiningEfficiency` (Fe) |
| Aluminum | 2.3 | 30 | NEW (fold) | Add `AluminumProduction` to `Refinery` |
| Copper | 1.0 | 23 | NEW (fold) | Add `CopperProduction` to `Mine` |
| Titanium | 0.02 | 17 | NEW (fold) | Add `TitaniumProduction` to `Refinery` |
| Silicates | 400 | 25 | ×0.040 (split) | Modify `StripMine.BulkMiningEfficiency` (Si slice) |
| Polymers | 18 | 25 | ×0.040 DOWN | Modify `ChemicalPlant.PolymerSynthesis` |
| Nickel | 0.18 | 21 | NEW (fold) | Add `NickelProduction` to `Mine` |
| Carbon | 480 | 25 | ×0.040 (split) | Modify `HydrocarbonExtractor` (add `CarbonProduction` modifier) |

**All 16 early-game resources land in the 10–50 manageable-count
band.** The two exceptions (Phosphorus at 15, Titanium at 17) are
flagged as expected — they reflect real-world scarcity and rounding
precision.

---

## 5. Mid-game tier (50–200 yr, fusion unlocks)

Mid-game resources are those unlocked by Tier 2–3 tech. Most are
**only produced for the space program**, not for civilian consumption
(per-capita ≈ 0 for He-3, D, T, Pu). The 10–50 constraint therefore
applies to the *space program* (per-body consumer count) not to
Earth's 8.2 B.

**The mid-game tier is gated by 2 new technologies** (`lunar_colony`
and `fusion_power`) added to `assets/data/technologies.ron` — see
§5.1.1 and §5.1.2 for full prereq chains. The 3rd new technology
(`kardashev_k2`) is the late-game gate (see §6).

### 5.1 Helium-3 — the catastrophe fix (mid-game, tech-gated + body-restricted)

This is the single most important patch in the mid-game tier. The
audit's finding #4: `FusionReactor` maintenance 10 Mt/yr He-3 vs
3,815 t world atmospheric pool = **2,600× world supply per year per
reactor**.

**Two-sided fix.** We do *both* (a) downscale the consumer and (b)
add the producer. Either alone is unstable.

| Field | Value |
|---|---|
| Real-world per-capita | 6×10⁻¹⁰ kg/p/yr (essentially 0) |
| Tier weight | 0.3 (Tier 3 — per CIVILIZATION_SATISFACTION_MODEL §2.4) |
| Real-world source | USGS Helium MCS 2024 (3,815 t atm pool); Wittenberg 1986 / Kulcinski 2000 for lunar regolith estimates (~10 t/yr at industrial scale, 2050+); gas-giant He/He-3 primordial ratios (Conrath et al. 1987 for Jupiter) |
| Current per-build consumer | **10 Mt/yr He-3** (`FusionReactor.maintenance_resources[Helium3]`) — **2,600× world pool per reactor per year** |
| Current per-build producer | **0** (no mining building — audit finding #4) |
| Proposed per-build consumer (downscale) | **0.5 Mt/yr He-3** per `FusionReactor` (×0.05 — DOWN 20×) |
| Proposed per-build producer (new) | **0.5 Mt/yr He-3** per `He3Mine` (1:1 ratio with consumer at 1 mine per reactor) |
| Manageable count | A mature He-3-deposit outpost with 10 `FusionReactor` needs 10 `He3Mine` (1:1) — within the 10–50 band. Earth at 8.2 B has He-3 per-capita = 0, so no Earth demand — count is set by the space program. |
| Tech gate (producer) | `He3Mine.required_tech = "lunar_colony"` |
| Tech gate (consumer) | `FusionReactor.required_tech = "fusion_power"` |
| Body restriction (producer) | `He3Mine.allowed_body_types = [BodyType::Moon, BodyType::GasGiant, BodyType::Asteroid]` (schema addition; see §8.4) — the three body classes with He-3 deposits: regolith-implanted by solar wind (Moon, asteroids) or primordial in atmosphere (gas giants) |
| Companion RON edit | `FusionReactor.maintenance_resources[Helium3]` 10 → 0.5. **NEW** `He3Mine` (full spec in §8.2.2). **NEW** tech `lunar_colony` (§5.1.1). **NEW** tech `fusion_power` (§5.1.2). |
| Rust delta | None — He-3 is tier-gated (CIVILIZATION_SATISFACTION_MODEL §7). Per-capita = 0, so it stays out of scope until the player builds a consumer (which is gated on `fusion_power` tech). |
| Why this fix | The He-3 is *post-scarcity* in the game (lunar regolith is effectively infinite at 1.1 Mt total — that's 1.1 million tonnes vs 10 tonnes/yr demand at 0.5 Mt/yr × 20 reactors × 1 yr = 10 t/yr). The 0.5 Mt/yr per reactor is a design choice, not a real-world anchor: it makes 1 mine = 1 reactor (clean ratio) and gives the player 50 reactors per Mt He-3. Real-world He-3 economy is not yet meaningful; the game gets to define it. |

**The 5-step chain to power a FusionReactor on a He-3-deposit body:**

1. Research `lunar_colony` tech (mid-game; prereqs below). The tech
   name is a holdover from v1 (the *first* off-world He-3 extraction
   in real-world plans is lunar; see Kulcinski 2000). The tech itself
   is the general off-world He-3 mining gate — it does NOT require a
   moon colony, only the prereq chain.
2. Found a colony on a He-3-deposit body — Moon, gas giant (with
   atmospheric scoop station), or large asteroid. Existing
   `EstablishOutpostRequest` flow; the construction panel gates the
   body-type list against the body's `BodyType` (canary-3 schema
   addition).
3. Build `He3Mine` on that body (1:1 with `FusionReactor`, requires
   `lunar_colony` AND body's `BodyType` ∈ `[Moon, GasGiant, Asteroid]`;
   the schema addition in §8.4 enforces the body restriction).
4. The mine produces He-3 into the body's `LocalStockpile` (the
   existing resource accounting system). For gas giants, the building
   is an orbital scoop station that "builds" at the body; for moons
   and asteroids, it's a regolith heating plant.
5. Research `fusion_power` tech (mid-game, requires `lunar_colony` as
   a prereq; §5.1.2 prereqs). The construction panel unlocks
   `FusionReactor` on the He-3-deposit body (and on any other body
   the player can freight He-3 to). The reactor's He-3 is supplied
   via the existing freight logistics (the same `ResourceRequest` /
   Freighter fleet system that handles Earth-to-Mars resource
   transport) from the He-3-deposit body.

**Freight hop.** The freight logistics layer already ships resources
between bodies. The Luna He-3 → consumer-body hop is the same
mechanism. No new freight system is needed. The "freight hop" is
implicit in the existing `ResourceRequest` /
`complete_deliveries` system (see `src/colony/components.rs:439-447`
on `ConstructionProject.awaiting_resources`).

**`DHe3FusionReactor` note.** The existing `DHe3FusionReactor` already
has He-3 maintenance of 0.0008 Mt/yr, which is 1,250× *less* than
`FusionReactor`. The patch leaves `DHe3FusionReactor` unchanged
(0.0008 Mt/yr is fine; the existing `required_anomalies: ["magnetic_anomaly"]`
field is preserved).

#### 5.1.1 `lunar_colony` tech (mid-game, NEW)

| Field | Value |
|---|---|
| `id` | `"lunar_colony"` |
| `name` | `"Lunar Colony Engineering"` |
| `category` | `SpaceTechnology` |
| `description` | "Engineering, life-support, and ISRU techniques for sustained off-world resource extraction. The off-world He-3 mining gate (Moon regolith, gas-giant atmospheric scoops, asteroid regolith), named for the first off-world He-3 extraction source in real-world plans." |
| `research_cost` | `7,500.0` (between `orbital_construction` at 8,000 and `space_habitation` at 7,000; calibration gives a mid-game tier cost) |
| `prerequisites` | `["space_habitation", "closed_loop_ecology"]` — both are realistic foundations for a permanent moon base (Apollo 1969-1972 first proved short-stay, but the first *sustained* presence needs closed-loop ECLSS + long-duration space hab engineering; see Apollo Lunar Surface Experiments Package + Artemis Base Camp plans) |
| `unlocks_engineering` | `["lunar_outpost_kit", "isru_oxygen_plant", "lunar_hab_module"]` |
| `unlocks_components` | `[]` |
| `tier` | `2` |
| Real-world analog | Apollo program (1961-1972, NASA), Artemis program (2024+, NASA + international), China Lunar Exploration Program (Chang'e 1-8, 2007-2030s), Russia Luna-Glob (Roscosmos) |
| Why position here | After `space_habitation` (long-duration crewed facilities, 7,000 RP) and `closed_loop_ecology` (closed-loop ECLSS, 220,100 RP) — both must be in hand before a permanent moon base is feasible. Before `fusion_power` (which requires `lunar_colony` as a prereq, since He-3 supply gates the reactor unlock). |

**Effect on the existing tech tree:** `lunar_colony` slots in at
Tier 2 between `space_habitation` (Tier 2, 7,000 RP) and `fusion_power`
(Tier 3, see §5.1.2). It does not break any existing tech chain. The
two existing moon-touched buildings (`UndergroundHabitat.available_atmospheres: [None]`,
`OrbitalSurveyStation` — neither has a tech gate today) become
*available* in the construction panel on a He-3-deposit body once the player
founds a colony on that body; the building availability is gated by the
*colony* existing, not by a per-building tech.

#### 5.1.2 `fusion_power` tech (mid-game, NEW)

| Field | Value |
|---|---|
| `id` | `"fusion_power"` |
| `name` | `"Fusion Power Engineering"` |
| `category` | `Energy` |
| `description` | "Magnetic-confinement fusion (tokamak, stellarator) and inertial-confinement fusion (laser-driven) reactor engineering at power-plant scale. Requires lunar He-3 supply for D-³He variants; the helium-3 feedstock chain (mining on the Moon, freighter transport to the consumer body) is the gate for the civilian power economy." |
| `research_cost` | `12,000.0` (above `lunar_colony` at 7,500; reflects the engineering jump from "we can build a moon base" to "we can run a fusion reactor" — the ITER project, JET, DEMO, Commonwealth Fusion SPARC all reflect this 20-30 year engineering effort) |
| `prerequisites` | `["lunar_colony", "fission_power"]` — `lunar_colony` is non-negotiable: without it, no He-3 supply, no FusionReactor. `fission_power` is the nearer-term fission prerequisite (the existing tech; without mastering fission, fusion engineering is academic) |
| `unlocks_engineering` | `["fusion_reactor", "dt_fusion_reactor", "dhe3_fusion_reactor"]` |
| `unlocks_components` | `["fusion_plasma_chamber", "tritium_breeding_blanket", "magnetic_confinement_coil"]` |
| `tier` | `3` |
| Real-world analog | ITER (international, 1988-2025+), JET (UK/EU, 1983-2023), DEMO (planned, 2050s), Commonwealth Fusion SPARC (private, 2025+), Helion (private, 2028+), Tokamak Energy (UK private, 2030s) |
| Why position here | After `lunar_colony` (He-3 supply gate) and `fission_power` (fission prerequisite). The fusion power plant is a Tier 3 unlock — the engineering maturity of fission is a precondition for fusion. |

**Why `lunar_colony` is non-negotiable as a fusion prereq.** The
`FusionReactor` consumes He-3 at 0.5 Mt/yr per build. Without a
`He3Mine` (which is gated on `lunar_colony` AND on the body
belonging to `[Moon, GasGiant, Asteroid]`), the player cannot supply
the consumer. Locking `fusion_power` behind `lunar_colony` enforces
the supply chain at the *research* layer: the player cannot unlock
the consumer until they have unlocked the producer's tech tree path.

#### 5.1.3 The K2 `kardashev_k2` tech (late-game, NEW)

The 4 K2 exotics (Antimatter, ExoticMatter, Metamaterials, Computronium)
are all gated on a single late-game tech `kardashev_k2`. Spec:

| Field | Value |
|---|---|
| `id` | `"kardashev_k2"` |
| `name` | `"Kardashev K2 Engineering"` |
| `category` | `Physics` |
| `description` | "Planetary-scale engineering, antimatter pair-production, exotic-matter stabilization, metamaterial lattice synthesis, and computronium substrate fabrication. The K2 Kardashev transition (per ROADMAP §9.1) unlocks the 4 exotic production domains." |
| `research_cost` | `500,000.0` (extreme; reflects the speculative K2-tier scope. This is a *place-holder* pending K2 design review.) |
| `prerequisites` | `["fusion_power"]` (the cheapest path to K2 is through mature fusion; other paths — quantum-AI, post-biological — are also defensible but deferred) |
| `unlocks_engineering` | `["antimatter_synthesizer", "exotic_matter_synthesizer", "metamaterials_fab", "computronium_substrate"]` |
| `unlocks_components` | `["antimatter_magnetic_bottle", "casimir_array", "metamaterial_lattice", "computronium_wafer"]` |
| `tier` | `4` |
| Real-world analog | None (K2 is hypothetical; Kardashev 1964 scale, NASA / Carl Sagan SETI usage) |
| Why this tech | Single-gate all four exotics. The K2.0 transition is a *civilization-scale* event (per ROADMAP §9.1), so a single tech gate is appropriate. |

**Effect on the existing tech tree:** `kardashev_k2` slots in at
Tier 4 with research_cost 500,000 (extreme; deferred to K2 design
review). No existing tech depends on it; it's a leaf.

### 5.2 Deuterium — folded into `ChemicalPlant`

| Field | Value |
|---|---|
| Real-world per-capita | 0.004 kg/p/yr (Culham / IEA fusion review) |
| Real-world supply | Ocean 3.5×10¹⁰ Mt; commercial D₂O production ~33 kt/yr (Canada CANDU et al.) |
| Current per-build consumer | `FusionReactor` 5 Mt/yr, `DTFusionReactor` 0.0015 Mt/yr, `DHe3FusionReactor` 0.0008 Mt/yr |
| Current per-build producer | **0** (no ocean electrolysis) |
| Proposed per-build consumer | Downscale `FusionReactor` D 5 → 0.25 Mt/yr (20× DOWN, matches He-3). `DTFusionReactor` 0.0015 → 0.0001. `DHe3FusionReactor` 0.0008 (unchanged). |
| Proposed per-build producer | NEW `DeuteriumProduction` on `ChemicalPlant` at 0.18 Mt/yr (1:1 with downscale `FusionReactor` at 0.25 Mt/yr ≈ 1.4 harvester per reactor) |
| Manageable count | 18 `ChemicalPlant` per body at 0.18 Mt/yr = 3.24 Mt/yr, enough for ~13 `FusionReactor` at 0.25 Mt/yr. Within 10–50. |
| Source | Culham Centre for Fusion Energy; IEA fusion review (iea.org/reports/fusion-power) |
| Why fold (not new `DeuteriumHarvester` like v1) | D is the *primary* fusion fuel in the D-T reactor and a major component of D-He3. The ocean has 10¹⁰ Mt — effectively infinite. The harvester is a coastal electrolysis plant. v1's separate `DeuteriumHarvester` is dropped; D₂ is produced by electrolysis of water — the same `ChemicalPlant` that already does H₂ synthesis. The marginal cost of adding a `DeuteriumProduction` modifier to `ChemicalPlant` is one line. |
| Companion RON edit | `ChemicalPlant.modifiers`: add `(modifier_type: "DeuteriumProduction", value: 0.18)`. `FusionReactor.maintenance_resources[Deuterium]` 5.0 → 0.25. |
| Rust delta | None. |

### 5.3 Tritium — scaledown `ChemicalPlant.TritiumBreeding`

| Field | Value |
|---|---|
| Real-world per-capita | 5×10⁻¹⁰ kg/p/yr (essentially 0) |
| Real-world supply | Bred from Li-6 in CANDU reactors; ~4 kg/yr globally (NUBASE2020) |
| Current per-build producer | `ChemicalPlant.modifiers[TritiumBreeding]` 0.05 Mt/yr (active only with `fusion_power` tech) |
| Current per-build consumer | `DTFusionReactor.maintenance_resources[Tritium]` 0.0005 Mt/yr |
| Proposed per-build producer | **0.001 Mt/yr** (×0.02 — DOWN 50×, but the *target* is to match consumer at 1:1) |
| Proposed per-build consumer | Keep `DTFusionReactor` at 0.0005 Mt/yr (already 50× lower than production) |
| Manageable count | A mature D-T outpost with 15 `DTFusionReactor` needs 15 ChemicalPlants breeding T at 0.001 Mt/yr each. Within 10–50. |
| Source | NUBASE2020; IAEA T breeding review |
| Why | The audit's finding §6.3: T at 0.05 Mt/yr is 12,500× real-world. The 50× downscale brings it to 0.001 Mt/yr which is 250× real-world — still post-scarcity (D-T is a closed breeding cycle) but not absurdly so. |
| Companion RON edit | `ChemicalPlant.modifiers[TritiumBreeding]` 0.05 → 0.001 |

### 5.4 Uranium — folded into `DeepDrill`

| Field | Value |
|---|---|
| Real-world per-capita | 0.0073 kg/p/yr (USGS Uranium MCS 2026) |
| Real-world supply | 0.060 Mt/yr production; 6.1 Mt reserves; 35 Mt total resources |
| Current per-build consumer | `FissionReactor` 0.0028 Mt/yr, `BreederReactor` 0.0015 Mt/yr |
| Current per-build producer | **0** (no mining) |
| Proposed per-build producer | NEW `UraniumProduction` on `DeepDrill` at 0.0028 Mt/yr (1:1 with `FissionReactor` consumer) |
| Manageable count | 18 `DeepDrill` per body at 0.0028 Mt/yr = 0.050 Mt/yr, matching world production. Within 10–50. |
| Companion RON edit | Add `(modifier_type: "UraniumProduction", value: 0.0028)` to `DeepDrill`. |
| Rust delta | None. U is mid-game Tier 3 (weight 0.3 per CIVILIZATION_SATISFACTION_MODEL §2.4). |

### 5.5 Thorium — folded into `DeepDrill`

| Field | Value |
|---|---|
| Real-world per-capita | 0.00012 kg/p/yr (USGS Thorium MCS 2024) |
| Real-world supply | ~0.001 Mt/yr byproduct of REE mining; 0.6 Mt reserves |
| Current per-build consumer | `ThoriumReactor` 0.0012 Mt/yr |
| Current per-build producer | **0** |
| Proposed per-build producer | NEW `ThoriumProduction` on `DeepDrill` at 0.0025 Mt/yr (couples to REE mining) |
| Manageable count | 16 `DeepDrill` per body at 0.0025 Mt/yr = 0.040 Mt/yr, plenty for 33 `ThoriumReactor` at 0.0012 Mt/yr. Within 10–50. |
| Source | USGS Thorium MCS 2024 |
| Why fold | Th is a Tier 3 strategic for the molten-salt reactor economy (post-2050). The patch bundles Th with REE mining since the real-world supply is a REE byproduct. |
| Companion RON edit | Add `(modifier_type: "ThoriumProduction", value: 0.0025)` to `DeepDrill`. |
| Rust delta | None. |

### 5.6 Plutonium — scaledown `BreederReactor`

| Field | Value |
|---|---|
| Real-world per-capita | 0.0000024 kg/p/yr (IAEA & SIPRI 2024) |
| Real-world supply | Civil stockpile 650 t; production 20 t/yr; bred in reactors |
| Current per-build producer | `BreederReactor.modifiers[PlutoniumBreeding]` 0.23 Mt/yr (the audit's §6.5 finding) |
| Current per-build consumer | `BreederReactor.maintenance_resources[Plutonium]` 0.0001 Mt/yr |
| Proposed per-build producer | **0.001 Mt/yr** (×0.0043 — DOWN 230×, to match real-world 20 t/yr at 0.00002 Mt/yr per breeder) |
| Manageable count | 12 `BreederReactor` at 0.001 Mt/yr Pu breeding = 0.012 Mt/yr = 12 t/yr, matching real-world civil production. Within 10–50. |
| Source | IAEA & SIPRI Yearbook 2024 |
| Why | The audit's finding #6.5: Pu at 0.23 Mt/yr is 11,500× real-world civil Pu rate. The 230× downscale brings it to 0.001 Mt/yr = 1 t/yr, 50× real-world — still post-scarcity (Pu is bred, not mined) but defensible. The breeder is fundamentally a *Pu multiplier*, not a power source, in real-world economics. The game keeps it as a power source (700 GW per breeder) but the Pu breeding rate should match the real ratio. |
| Companion RON edit | `BreederReactor.modifiers[PlutoniumBreeding]` 0.23 → 0.001 |
| Rust delta | None. |

### 5.7 Lithium — folded into `Mine`

| Field | Value |
|---|---|
| Real-world per-capita | 0.022 kg/p/yr (USGS Lithium 2026) |
| Real-world supply | 0.18 Mt/yr Li metal/yr; 0.030 Mt reserves; 2.3×10⁸ Mt in seawater |
| Current per-build consumer | `SolarPower`, `FissionReactor`, `AI Cluster`, `SemiconductorFab` (each 0.0002–0.0005 Mt/yr per building) |
| Current per-build producer | **0** |
| Proposed per-build producer | NEW `LithiumProduction` on `Mine` at 0.006 Mt/yr (matches demand at 22 buildings per body for Earth-class 8.2 B) |
| Manageable count | 22 `Mine` per body at 0.006 Mt/yr = 0.132 Mt/yr, matching world production. Within 10–50. |
| Source | USGS Lithium 2026; IEA Critical Minerals Outlook 2024 |
| Why fold | Li is critical for every battery / electrical building. Real-world supply is brine (Salar de Atacama, Chile) and hard-rock (Greenbushes, Australia). The multi-output `Mine` handles the brine-extraction slice. |
| Companion RON edit | Add `(modifier_type: "LithiumProduction", value: 0.006)` to `Mine`. |
| Rust delta | None. |

### 5.8 RareEarths — folded into `DeepDrill`

| Field | Value |
|---|---|
| Real-world per-capita | 0.037 kg/p/yr (USGS REE 2026) |
| Real-world supply | 0.3 Mt/yr REO; 110 Mt reserves; China 90% refining |
| Current per-build consumer | `MassDriver`, `AI Cluster`, `SemiconductorFab`, `LaserDrill`, `FusionReactor` |
| Current per-build producer | **0** dedicated; `DeepDrill`/`LaserDrill` have a `DeepMiningEfficiency` modifier that *could* target REE but no per-REE modifier exists |
| Proposed per-build producer | NEW `RareEarthsProduction` on `DeepDrill` at 0.012 Mt/yr |
| Manageable count | 20 `DeepDrill` per body at 0.012 Mt/yr = 0.24 Mt/yr, matching world production. Within 10–50. |
| Source | USGS REE MCS 2026 (pubs.usgs.gov/periodicals/mcs2026/mcs-2026-rare-earths.pdf) |
| Why fold | REE is the bottleneck for *every* advanced tech. Mass drivers, AI clusters, fusion reactors all consume it. The deep-crust ore body (monazite, xenotime, bastnäsite) is mined by the multi-output `DeepDrill`. |
| Companion RON edit | Add `(modifier_type: "RareEarthsProduction", value: 0.012)` to `DeepDrill`. |
| Rust delta | None. |

### 5.9 Cobalt — folded into `Mine`

| Field | Value |
|---|---|
| Real-world per-capita | 0.028 kg/p/yr (USGS Cobalt 2026) |
| Real-world supply | 0.23 Mt/yr; 11 Mt reserves; DRC 75% |
| Current per-build consumer | `Mine`, `D-T Reactor`, others (each 0.0001–0.002 Mt/yr) |
| Current per-build producer | **0** |
| Proposed per-build producer | NEW `CobaltProduction` on `Mine` at 0.012 Mt/yr (couples to Cu mining — real-world Co is a Cu-mine byproduct) |
| Manageable count | 18 `Mine` per body at 0.012 Mt/yr = 0.216 Mt/yr, matching world production. Within 10–50. |
| Source | USGS Cobalt MCS 2026 (pubs.usgs.gov/periodicals/mcs2026/mcs-2026-cobalt.pdf) |
| Companion RON edit | Add `(modifier_type: "CobaltProduction", value: 0.012)` to `Mine`. |
| Rust delta | None. |

### 5.10 Sulfur — covered by `HydrocarbonExtractor.SulfurByproduct`

| Field | Value |
|---|---|
| Real-world per-capita | 8.4 kg/p/yr (USGS Sulfur 2024) |
| Real-world supply | 69 Mt/yr (mostly petroleum refining byproduct); 350 Mt reserves |
| Current per-build consumer | `ChemicalPlant` 0.03 Mt/yr; `CoalPowerPlant` 0.05 Mt/yr; `PharmaceuticalPlant` 0.1 Mt/yr; others |
| Current per-build producer | **0** dedicated |
| Proposed per-build producer | Already covered by `HydrocarbonExtractor.SulfurByproduct: 5` (in the §4.15 C/CH4 split). 19 extractors × 5 Mt/yr = 95 Mt/yr > 69 Mt/yr world. **No new building needed.** |
| Source | USGS Sulfur MCS 2024 |
| Why no new building | S is critical for sulfuric acid (the most-produced industrial chemical, 270 Mt/yr global). Real-world S is a *petroleum-refining byproduct*, not a primary mine product. The patch reflects this with the `HydrocarbonExtractor.SulfurByproduct` modifier (Claus-process output). v1's separate `SulfurRecoverer` is dropped. |

### 5.11 Fluorine — folded into `Mine`

| Field | Value |
|---|---|
| Real-world per-capita | 0.55 kg/p/yr (USGS Fluorspar 2024) |
| Real-world supply | 4.5 Mt fluorite/yr; 320 Mt fluorspar reserves; 585 ppm crustal |
| Current per-build consumer | `ChemicalPlant` 0.001 Mt/yr; `StripMine` 0.0005; `SemiconductorFab` 0.01; `FissionReactor` 0.005; `ThoriumReactor` 0.004; `BreederReactor` 0.004; `FusionReactor` 0.003 |
| Current per-build producer | **0** |
| Proposed per-build producer | NEW `FluorineProduction` on `Mine` at 0.25 Mt/yr |
| Manageable count | 17 `Mine` per body at 0.25 Mt/yr = 4.25 Mt/yr, matching world production. Within 10–50. |
| Source | USGS Fluorspar MCS 2024 |
| Companion RON edit | Add `(modifier_type: "FluorineProduction", value: 0.25)` to `Mine`. |
| Rust delta | None. |

**Fluorine is the v2 stand-in for NaOH and Cl₂.** v1's
`AluminaRefinery` used `SodiumHydroxide` in `maintenance_resources`;
v1's `TitaniumSmelter` used `Chlorine`. v0.5 design rule: **no new
`ResourceType` entries** — Fluorine is a real resource with real
demand (UF₆ enrichment, semiconductor etch, fluoropolymer
manufacture), and the Cl/F / NaOH/F substitutions are chemically
defensible: both Cl₂ and F₂ are halogens used in similar
high-temperature halide processes; both NaOH and HF (the F analog
of NaOH) are strong bases / etchants.

### 5.12 Tungsten — folded into `DeepDrill`

| Field | Value |
|---|---|
| Real-world per-capita | 0.0095 kg/p/yr (USGS Tungsten MCS 2026) |
| Real-world supply | 0.078 Mt/yr; 3.5 Mt reserves; 45 yr R/P |
| Current per-build consumer | `MissileSilo` 0.001 Mt/yr; `GroundDefenseBattery` 0.001; `RailgunTech` (implied); `DeepDrill` 0.0001 |
| Current per-build producer | **0** |
| Proposed per-build producer | NEW `TungstenProduction` on `DeepDrill` at 0.005 Mt/yr |
| Manageable count | 15 `DeepDrill` per body at 0.005 Mt/yr = 0.075 Mt/yr, matching world production. Within 10–50. |
| Source | USGS Tungsten MCS 2026 (pubs.usgs.gov/periodicals/mcs2026/mcs-2026-tungsten.pdf) |
| Companion RON edit | Add `(modifier_type: "TungstenProduction", value: 0.005)` to `DeepDrill`. |
| Rust delta | None. |

### 5.13 Chromium — folded into `Refinery`

| Field | Value |
|---|---|
| Real-world per-capita | 3.7 kg/p/yr (USGS Chromium 2024) |
| Real-world supply | 30 Mt chromite ore/yr; 750 Mt reserves; S. Africa 48% |
| Current per-build consumer | `Refinery` 0.003 Mt/yr; `DeepDrill` 0.005; `Shipyard` 0.5 |
| Current per-build producer | **0** |
| Proposed per-build producer | NEW `ChromiumProduction` on `Refinery` at 1.7 Mt/yr |
| Manageable count | 18 `Refinery` per body at 1.7 Mt/yr = 30.6 Mt/yr, matching world production. Within 10–50. |
| Source | USGS Chromium 2024 |
| Companion RON edit | Add `(modifier_type: "ChromiumProduction", value: 1.7)` to `Refinery`. |
| Rust delta | None. |

### 5.14 Magnesium — folded into `Refinery`

| Field | Value |
|---|---|
| Real-world per-capita | 0.13 kg/p/yr (USGS Magnesium 2024) |
| Real-world supply | 1.1 Mt/yr; 750 Mt reserves; China 84% |
| Current per-build consumer | `HabitatDome` 0.005 Mt/yr; `LaunchSite` 0.002; `Shipyard` 0.002; `SpacePort` 0.004 |
| Current per-build producer | **0** |
| Proposed per-build producer | NEW `MagnesiumProduction` on `Refinery` at 0.06 Mt/yr (Pidgeon process from dolomite, or electrolytic from seawater) |
| Manageable count | 19 `Refinery` per body at 0.06 Mt/yr = 1.14 Mt/yr, matching world production. Within 10–50. |
| Source | USGS Magnesium 2024 |
| Companion RON edit | Add `(modifier_type: "MagnesiumProduction", value: 0.06)` to `Refinery`. |
| Rust delta | None. |

### 5.15 Gold (precious metal, electronics + jewelry + investment)

| Field | Value |
|---|---|
| Real-world per-capita | **0.00037 kg/p/yr** (USGS Gold MCS 2026, 3,000 t/yr ÷ 8.2 B) |
| Tier weight | 0.3 (Tier 3, precious / catalytic per CIVILIZATION_SATISFACTION_MODEL §2.4) |
| Earth demand (8.2 B) | 0.003 Mt/yr (3,000 t/yr) |
| Real-world source | USGS Gold MCS 2026; ~3,000 t/yr global mine production; ~59,000 t reserves (USGS 2024) |
| Current per-build consumer | **0** (no consumer in RON) |
| Current per-build producer | **0** (no mining building) |
| Proposed per-build producer (NEW) | **0.0001 Mt/yr Au** per `GoldMine` (≈ 3,200 troy oz/yr — small-mine scale) |
| Proposed per-build consumer | **0.0001 Mt/yr Au** per `SemiconductorFab` (1:1 with producer; electronics-industry share of Au consumption is ~15% of global demand, with the rest going to jewelry / investment / industrial / dental) |
| Manageable count | 25 `GoldMine` × 0.0001 = 0.0025 Mt/yr ≈ 80% of USGS-reported demand (rest comes from recycling). Within 10–50. **Note:** below the audit's "1 building ≈ 1/300 of world" bar, but gold is a Tier 3 / weight 0.3 resource and the per-build value rounds to 0.0001 (3,200 oz/yr — a realistic small-mine output). User can refine. |
| Source | USGS Gold MCS 2026; World Gold Council demand 2024 (electronics ~250 t/yr, jewelry ~1,500 t/yr, investment ~1,100 t/yr, industrial ~300 t/yr); the game uses the global aggregate (3,000 t/yr) for civilian demand |
| Why `SemiconductorFab` as consumer | The existing `SemiconductorFab` (display name "Electronics Industry") is the natural electronics-industry consumer in the current RON. Adding Au/Ag/Pt/Ar to its `maintenance_resources` is additive and non-breaking. v0.5.x does not need a new `ElectronicsIndustry` building. |
| Companion RON edit | **NEW** `GoldMine` (full spec §8.2.7). Add `("Gold", 0.0001)` to `SemiconductorFab.maintenance_resources`. |
| Rust delta | None. Gold is a Tier 3 / weight 0.3 resource (CIVILIZATION_SATISFACTION_MODEL §2.4). No new `ResourceType` (Gold already exists at `ResourceType::Gold`). |
| Why this fix | The audit and §5.15 (v1) deferred Au/Ag/Pt to a K1.5 patch that required a new `ElectronicsIndustry` consumer. The user has revised the v0.5 design rule: fold the consumer into the existing `SemiconductorFab` (no new buildings for consumers) and add 3 dedicated mine buildings (Au/Ag/Pt) plus 1 atmospheric fold (Ar) for the producers. Real 2026 per-capita is sourced. |

### 5.16 Silver (precious metal, electronics + photography + jewelry)

| Field | Value |
|---|---|
| Real-world per-capita | **0.0032 kg/p/yr** (USGS Silver MCS 2026, 26,000 t/yr ÷ 8.2 B) |
| Tier weight | 0.3 (Tier 3, precious / catalytic per CIVILIZATION_SATISFACTION_MODEL §2.4) |
| Earth demand (8.2 B) | 0.025 Mt/yr (25,000 t/yr) |
| Real-world source | USGS Silver MCS 2026; ~25,000 t/yr global mine production; ~530,000 t reserves |
| Current per-build consumer | **0** (no consumer) |
| Current per-build producer | **0** (no mining building) |
| Proposed per-build producer (NEW) | **0.001 Mt/yr Ag** per `SilverMine` (1,000 t/yr per mine — large-mine scale; 1 mine ≈ 4% world share) |
| Proposed per-build consumer | **0.001 Mt/yr Ag** per `SemiconductorFab` (1:1 with producer; the electronics share of Ag demand is ~50% globally — solar PV paste, solders, contacts, photographic film is now ~5%) |
| Manageable count | 25 `SilverMine` × 0.001 = 0.025 Mt/yr = USGS 2026 demand. Within 10–50. |
| Source | USGS Silver MCS 2026; The Silver Institute demand 2024 (industrial ~50%, jewelry/silverware ~20%, investment ~15%, photography ~5%, other ~10%) |
| Companion RON edit | **NEW** `SilverMine` (full spec §8.2.8). Add `("Silver", 0.001)` to `SemiconductorFab.maintenance_resources`. |
| Rust delta | None. Silver is Tier 3 / weight 0.3. No new `ResourceType`. |
| Why dedicated mine (not fold into Mine) | Silver mining is dominantly a **lead-zinc byproduct** (Cannington, Australia; Peñasquito, Mexico; ~70% of global Ag is Pb/Zn byproduct). The `Refinery` already does Pb/Zn processing, but adding Ag as a fold would require Pb/Zn to be in the existing `Refinery` modifiers, which they aren't. A dedicated `SilverMine` is the cleaner game-side abstraction; the user can later fold Ag into `Refinery` if they add Pb/Zn mining. |

### 5.17 Platinum (precious metal, autocatalyst + fuel cell + jewelry)

| Field | Value |
|---|---|
| Real-world per-capita | **0.000024 kg/p/yr** (USGS Platinum MCS 2026, 200 t/yr ÷ 8.2 B) |
| Tier weight | 0.3 (Tier 3, precious / catalytic per CIVILIZATION_SATISFACTION_MODEL §2.4) |
| Earth demand (8.2 B) | 0.0002 Mt/yr (200 t/yr) |
| Real-world source | USGS Platinum MCS 2026; ~200 t/yr global mine production; ~69,000 t reserves (Bushveld Complex, South Africa dominates) |
| Current per-build consumer | **0** (no consumer) |
| Current per-build producer | **0** (no mining building) |
| Proposed per-build producer (NEW) | **0.00001 Mt/yr Pt** per `PlatinumMine` (10 t/yr per mine — realistic PGM output; 1 mine ≈ 5% world share) |
| Proposed per-build consumer | **0.00001 Mt/yr Pt** per `SemiconductorFab` (1:1 with producer; autocatalyst ~40% + jewelry ~30% + industrial ~20% + investment ~10% — the electronics / industrial slice goes through `SemiconductorFab`) |
| Manageable count | 20 `PlatinumMine` × 0.00001 = 0.0002 Mt/yr = USGS 2026 demand. **Manageable-count exception** (flagged in §3.5): per-build is below the 0.0001 Mt/yr rounding precision, so target_count = 20 (not 25). The UI panel does not display tonnes-vs-Mt at this scale (known limitation, same as Titanium / Phosphorus). |
| Source | USGS Platinum MCS 2026; Johnson Matthey PGM market report 2024 |
| Companion RON edit | **NEW** `PlatinumMine` (full spec §8.2.9). Add `("Platinum", 0.00001)` to `SemiconductorFab.maintenance_resources`. |
| Rust delta | None. Platinum is Tier 3 / weight 0.3. No new `ResourceType`. |
| Why dedicated mine (not fold) | PGM (platinum group metals) mining is concentrated in the Bushveld Complex (South Africa) and Norilsk (Russia), with very different processing from base-metal mining. A dedicated `PlatinumMine` is the cleaner game-side abstraction. The user can later fold Pt into `Refinery` if they add Ni/Cu processing. |

### 5.18 Argon (atmospheric noble gas, semiconductor fab + welding + lighting)

| Field | Value |
|---|---|
| Real-world per-capita | **0.085 kg/p/yr** (USGS Helium & Noble Gases 2024, 700,000 t/yr ÷ 8.2 B) |
| Tier weight | 1.0 (Tier 2, industrial gas per CIVILIZATION_SATISFACTION_MODEL §2.4) |
| Earth demand (8.2 B) | 0.7 Mt/yr (700,000 t/yr) |
| Real-world source | USGS Helium & Noble Gases 2024; ~700,000 t/yr global production (mostly from cryogenic air separation as an O₂/N₂ byproduct) |
| Current per-build consumer | **0** (no consumer) |
| Current per-build producer | **0** (no atmospheric argon modifier) |
| Proposed per-build producer (FOLD) | **0.028 Mt/yr Ar** per `AtmosphericProcessor` (28,000 t/yr per processor — major air-separation plant; the existing `AtmosphericProcessor` modifier stack already splits N₂/O₂, so adding Ar as a third atmospheric gas is the natural fold) |
| Proposed per-build consumer | **0.028 Mt/yr Ar** per `SemiconductorFab` (1:1 with producer; semiconductor fab uses Ar as an inert sputtering / wafer-handling atmosphere, plus welding inert shield in `Refinery` is a smaller second consumer) |
| Manageable count | 25 `AtmosphericProcessor` × 0.028 = 0.7 Mt/yr = USGS 2024 demand. Within 10–50. |
| Source | USGS Helium & Noble Gases 2024; Linde / Air Liquide annual reports 2024 |
| Companion RON edit | Add `("ArgonProduction", 0.028)` to `AtmosphericProcessor.modifiers`. Add `("Argon", 0.028)` to `SemiconductorFab.maintenance_resources`. |
| Rust delta | None. Argon is Tier 2 / weight 1.0. No new `ResourceType` (Argon already exists at `ResourceType::Argon`). |
| Why fold (not new `ArgonExtractor`) | Argon is a **byproduct of cryogenic air separation** — it cannot be economically extracted without first producing O₂/N₂. The existing `AtmosphericProcessor` already splits N₂ (`NitrogenHarvesting` 7 Mt/yr) and O₂ (`OxygenProduction` 200 Mt/yr). Adding Ar as a third atmospheric gas in the same processor is the natural fold and avoids a redundant building. The per-build 0.028 Mt/yr is the byproduct rate at the audit's calibration scale. |

### 5.19 Mid-game summary

| Resource | New per-build (Mt/yr) | Action |
|---|---:|---|
| Helium-3 | 0.5 (consumer) + 0.5 (producer) | Modify `FusionReactor`; add `He3Mine` (body-restricted `[Moon, GasGiant, Asteroid]`) |
| Deuterium | 0.25 (consumer) + 0.18 (producer) | Modify `FusionReactor`; add `DeuteriumProduction` to `ChemicalPlant` |
| Tritium | 0.001 (producer) | Modify `ChemicalPlant` |
| Uranium | 0.0028 (producer) | Add `UraniumProduction` to `DeepDrill` |
| Thorium | 0.0025 (producer) | Add `ThoriumProduction` to `DeepDrill` |
| Plutonium | 0.001 (producer) | Modify `BreederReactor` |
| Lithium | 0.006 (producer) | Add `LithiumProduction` to `Mine` |
| RareEarths | 0.012 (producer) | Add `RareEarthsProduction` to `DeepDrill` |
| Cobalt | 0.012 (producer) | Add `CobaltProduction` to `Mine` |
| Sulfur | 3.0 (producer) | Already in `HydrocarbonExtractor.SulfurByproduct` |
| Fluorine | 0.25 (producer) | Add `FluorineProduction` to `Mine` |
| Tungsten | 0.005 (producer) | Add `TungstenProduction` to `DeepDrill` |
| Chromium | 1.7 (producer) | Add `ChromiumProduction` to `Refinery` |
| Magnesium | 0.06 (producer) | Add `MagnesiumProduction` to `Refinery` |
| Gold | 0.0001 (consumer) + 0.0001 (producer) | **NEW** `GoldMine`; add to `SemiconductorFab.maintenance_resources` |
| Silver | 0.001 (consumer) + 0.001 (producer) | **NEW** `SilverMine`; add to `SemiconductorFab.maintenance_resources` |
| Platinum | 0.00001 (consumer) + 0.00001 (producer) | **NEW** `PlatinumMine`; add to `SemiconductorFab.maintenance_resources` |
| Argon | 0.028 (consumer) + 0.028 (producer) | Add `ArgonProduction` to `AtmosphericProcessor`; add to `SemiconductorFab.maintenance_resources` |

**4 NEW buildings + 2 NEW technologies + ~13 existing building edits
= the mid-game changes.** v1 had 11 NEW buildings; v2 has 4
(He3Mine + GoldMine + SilverMine + PlatinumMine; Argon folds into
`AtmosphericProcessor` to honour the "no new `ResourceType` / no
redundant atmospheric buildings" design rule). The fold cut 7 new
buildings from v1.

---

## 6. Late-game tier (200+ yr, K2 Kardashev)

All late-game resources are **K2-exotic** per the audit and the
satisfaction model (CIVILIZATION_SATISFACTION_MODEL §7.6). They are
filtered out of the satisfaction calculation until the player reaches
K2.0 Kardashev (gated on the `kardashev_k2` tech, §5.1.3). The patch
proposes producer buildings at the *spec* level; the user is expected
to refine at K2 design review (the "approximate" flag).

### 6.1 Antimatter

| Field | Value |
|---|---|
| Real-world anchor | CERN ALPHA experiment ~10⁻¹⁰ g/yr (essentially 0 in game units) |
| Game-unit proposal | **Track in grams**, not Mt (per audit §6.2; v0.5 design rule) |
| Per-build producer | NEW `AntimatterSynthesizer` at **100 g/yr** (1 g = world 100-year stockpile at the design rate) |
| Manageable count | 12 `AntimatterSynthesizer` per body at 100 g/yr = 1,200 g/yr, enough for ~12 `AntimatterDrive` ships per year at 100 g/ship. Within 10–50. |
| Tech gate | `required_tech: "kardashev_k2"` |
| Source | CERN ALPHA experiment (home.cern/science/experiments/alpha) |
| Why approximate | No real-world anchor for industrial antimatter. The 100 g/yr is a design placeholder chosen so 12 synthesizers feed a small fleet. K2 design review should re-derive. |
| Companion RON edit | **NEW** building `AntimatterSynthesizer` (full spec in §8.2.3). |
| Rust delta | Antimatter unit convention change: game tracks in **grams** for K2+ tier. Major Rust change; deferred to K2 design pass. |

### 6.2 ExoticMatter

| Field | Value |
|---|---|
| Real-world anchor | None (theoretical; Casimir / negative-energy-density) |
| Game-unit proposal | Track in **kg** of "negative-energy-equivalent" |
| Per-build producer | NEW `ExoticMatterSynthesizer` at **0.1 kg/yr** (placeholder; K2 review) |
| Manageable count | Spec-level only; no real-world anchor to validate. |
| Source | None — comparable-game fallback (Aurora 4X treats "exotics" as a late-game currency; Stellaris uses "rare crystals" with no real anchor) |
| Why approximate | No physics consensus on production. The 0.1 kg/yr is a placeholder that lets the building exist without crashing the economy. |
| Companion RON edit | **NEW** building `ExoticMatterSynthesizer` (full spec in §8.2.4). Required_tech: K2.0 gate. |
| Rust delta | None. |

### 6.3 Metamaterials

| Field | Value |
|---|---|
| Real-world anchor | None (synthetic; no reliable public source) |
| Game-unit proposal | Track in **Mt** (synthetic composite) |
| Per-build producer | NEW `MetamaterialsFab` at **0.05 Mt/yr** |
| Manageable count | 12 labs per body at 0.05 Mt/yr = 0.6 Mt/yr, enough for the cloaking / shielding / perfect-lens applications. Within 10–50. |
| Source | None — comparable-game fallback |
| Why approximate | "Engineered composites with unnatural optical/EM properties" per `src/economy/types.rs:226-227`. The 0.05 Mt/yr is a design placeholder. |
| Companion RON edit | **NEW** building `MetamaterialsFab` (full spec in §8.2.5). Required_tech: K2.0 gate. |
| Rust delta | None. |

### 6.4 Computronium

| Field | Value |
|---|---|
| Real-world anchor | None (theoretical substrate) |
| Game-unit proposal | Track in **Mt** (substrate mass) |
| Per-build producer | NEW `ComputroniumSubstrate` at **0.01 Mt/yr** |
| Manageable count | 10 foundries per body at 0.01 Mt/yr = 0.1 Mt/yr, enough for a Culture-level AI mind cluster. Within 10–50. |
| Source | None — comparable-game fallback (Aurora 4X, Stellaris both leave undefined) |
| Why approximate | "Optimised computational substrate" per `src/economy/types.rs:228-230`. The 0.01 Mt/yr is a design placeholder; K2 review. |
| Companion RON edit | **NEW** building `ComputroniumSubstrate` (full spec in §8.2.6). Required_tech: K2.0 gate. |
| Rust delta | None. |

### 6.5 Late-game summary

| Resource | Unit | New per-build | Action |
|---|---|---:|---|
| Antimatter | **grams** | 100 g/yr | Add `AntimatterSynthesizer`; `kardashev_k2`-gated |
| ExoticMatter | kg | 0.1 kg/yr | Add `ExoticMatterSynthesizer`; `kardashev_k2`-gated |
| Metamaterials | Mt | 0.05 Mt/yr | Add `MetamaterialsFab`; `kardashev_k2`-gated |
| Computronium | Mt | 0.01 Mt/yr | Add `ComputroniumSubstrate`; `kardashev_k2`-gated |

**4 NEW late-game buildings, all `kardashev_k2`-gated, all marked
"approximate" pending K2 design review.**

---

## 7. Power buildings (cross-cutting)

Power is handled separately because it is a *flow* (W = J/s) not a
*stockpile* (Mt). The audit §7 cross-cuts the catalog at 30,000
TWh/yr world electricity (~3,450 GW avg continuous, ~420 W per
capita).

### 7.1 Per-capita and per-build

* **Per-capita demand:** 3,660 kWh/p/yr = **418 W continuous** per
  person (IEA 2024 world electricity).
* **Earth demand (8.2 B):** 30,000 TWh/yr = 3,425 GW continuous.
* **Manageable count target:** 10–30 power plants per body (a small
  asteroid outpost has 1 Solar + 1 Fission; a mature planet has 20+
  of each). The 10–50 constraint is more relaxed for power than for
  resources.

### 7.2 Per-build targets

| Power plant | Current per-build (GW avg) | Current count (Earth) | Proposed per-build (GW avg) | Proposed count (Earth) | Δ |
|---|---:|---:|---:|---:|---|
| SolarPower | 240 | 1 (= world solar share 7%) | 200 | 12 | ×0.83 DOWN |
| WindFarm | 310 | 1 (= world wind share 9%) | 250 | 11 | ×0.81 DOWN |
| HydroelectricDam | 510 | 1 (= world hydro share 15%) | 400 | 6 | ×0.78 DOWN |
| GeothermalPlant | 100 | 1 (= world geothermal) | 80 | 4 | ×0.80 DOWN |
| CoalPowerPlant | 1,200 | 1 (= world coal share 35%) | 800 | 12 | ×0.67 DOWN |
| NaturalGasPlant | 750 | 1 (= world gas share 22%) | 600 | 11 | ×0.80 DOWN |
| FissionReactor | 310 | 1 (= world nuclear share 9%) | 250 | 11 | ×0.81 DOWN |
| FusionReactor | 2,000 | 0 (K-gated) | 1,500 | n/a (post-`fusion_power`) | ×0.75 DOWN |
| DTFusionReactor | 3,000 | 0 (K-gated) | 2,500 | n/a (post-`fusion_power`) | ×0.83 DOWN |
| DHe3FusionReactor | 2,500 | 0 (K-gated) | 2,000 | n/a (post-`fusion_power`) | ×0.80 DOWN |
| ThoriumReactor | 800 | 0 (post-molten-salt) | 600 | 4 | ×0.75 DOWN |
| BreederReactor | 700 | 0 (post-breeder-reactors) | 500 | 5 | ×0.71 DOWN |

**Why scale down (not up).** Each power plant today is "1 plant = 1
world power share," so 1 SolarPower = 7% of world electricity. The
patch scales to "1 plant = 1/12 of one power source share," giving
the player 12 plants of each type per body — within the 10–50
manageable count. 12 SolarPlants at 200 GW = 2,400 GW = 21,000 TWh/yr
(~70% of world demand).

**Why not 30 SolarPlants.** Solar is variable (24h cycle, weather);
the 200 GW figure is *average continuous*. The patch keeps it
realistic — 12 large PV farms at 200 GW avg each is a real-world
scale (largest PV farms are 2-5 GW peak; 200 GW = 100 of those,
which is the output of a continent-spanning solar array, comparable
to the world solar share).

**Fusion power plants are post-`fusion_power` tech** (per §5.1.2).
No fusion plant is buildable at game start.

### 7.3 Companion RON edits

For each power plant, the `PowerGeneration` modifier value scales
down per the table above. No new buildings. The existing tier-1
always-available power plants (Solar, Wind, Hydro, Coal, NG, Fission)
cover the early-game. The fusion plants scale to mid-game (post-
`fusion_power` tech).

### 7.4 Companion Rust / model edits

The satisfaction model treats power as a *flow constraint* not a
*resource stockpile*. No new Rust field. The `PowerSource`
component already aggregates the per-build `PowerGeneration`
modifiers. The patch doesn't change that aggregation.

---

## 8. Implementation notes

This section is the **how-to-apply** reference. The user edits
`assets/data/buildings.ron`, `assets/data/technologies.ron`, and
`src/colony/data.rs` per the specifications below. No other code
changes are required for v0.5.x (the civilization-satisfaction-model
is a separate deliverable).

### 8.1 Rust constant delta (the only Rust edit in v0.5.x — components)

**File:** `src/colony/components.rs`
**Lines:** 300-301 (consumption), 282-285 (production hard-codes)

**Before:**
```rust
/// Per-capita consumption: 0.0001 Mt/person/year (100 tonnes/person/year).
/// At this scale 1 Farm (1,000 Mt/yr) feeds ~10M people.
pub fn food_consumption_per_year(&self) -> f64 {
    self.population * 0.0001
}
```

**After (v0.5.1 — corrected unit conversion):**
```rust
/// Per-capita consumption: 0.0000011 Mt/person/year (1,100 kg/person/year,
/// FAO 2024 SOFA). 1,100 kg = 1.1 × 10⁻⁶ Mt (since 1 Mt = 10⁹ kg).
/// At this scale 1 Farm (360 Mt/yr, post-patch) feeds ~327M people, so
/// Earth (8.2 B) needs ~25 Farms — within the 10–50 manageable-count band.
pub fn food_consumption_per_year(&self) -> f64 {
    self.population * 0.0000011
}
```

**v0.5 unit error:** the canary-1 value 0.0011 was 1,000× too high
(0.0011 Mt = 1,100 tonnes, not 1,100 kg). v0.5.1 corrects this and the
hard-coded per-build values in `food_production_per_year`.

Also update the production-side hard-coded values (the simulation does
**NOT** read the RON `FoodProduction` modifier — these constants are
the source of truth, `src/colony/components.rs:282-285`):

```rust
/// Calculate food production rate (Mt/year) from agricultural buildings.
///
/// Each building is scaled for district-level throughput (post-patch):
/// - Farm:                  360 Mt/yr  → feeds ~327M people (1/25 of Earth)
/// - AgriDome:                4 Mt/yr  → feeds ~3.6M people (enclosed, off-world)
/// - Greenhouse:            200 Mt/yr  → feeds ~182M people (controlled-env, supplemental)
/// - AquacultureFacility:   200 Mt/yr  → feeds ~182M people (seafood, supplemental)
///
/// Per-capita food consumption: 0.0000011 Mt/person/yr (1,100 kg/p/yr,
/// FAO 2024 SOFA — 1,100 kg = 1.1 × 10⁻⁶ Mt since 1 Mt = 10⁹ kg).
pub fn food_production_per_year(&self) -> f64 {
    let farm_count = self.building_count(BuildingType::Farm) as f64;
    let agri_count = self.building_count(BuildingType::AgriDome) as f64;
    let greenhouse_count = self.building_count(BuildingType::Greenhouse) as f64;
    let aquaculture_count = self.building_count(BuildingType::AquacultureFacility) as f64;
    farm_count * 360.0
        + agri_count * 4.0
        + greenhouse_count * 200.0
        + aquaculture_count * 200.0
}
```

The new `farm_count * 360.0` reflects the per-build scale-down of
the `FoodProduction` modifier in the RON. The other production
values also scale down: `agri_count * 4.0` (was 4, unchanged for
AgriDome because off-world scale), `greenhouse_count * 200.0` (was
500), `aquaculture_count * 200.0` (was 750). These mirror the
RON changes in §8.3.1-§8.3.4.

### 8.2 NEW buildings to add to `buildings.ron` (9 total)

The user appends each block to the `buildings: [ ... ]` array. The
schema matches the existing entries; the IDs must match a
`BuildingType` enum variant (which the user adds in Rust if it
doesn't already exist — see §8.5 for the enum additions).

#### 8.2.1 WaterProcessor (early-game, non-breathable)

```ron
(
    id: "WaterProcessor",
    display_name: "Water Processor",
    description: "Atmospheric condenser / ice miner. Extracts 16 Mt/yr water from regolith or atmosphere. Required for non-breathable colony life support.",
    icon: "textures/ui/buildings/water-processor.png",
    category: "Infrastructure",
    build_points: 600.0,
    workforce: 2000,
    required_tech: "",
    resource_costs: [
        ("Iron", 67.0),
        ("Copper", 33.0),
        ("Aluminum", 17.0),
        ("Polymers", 8.0),
    ],
    maintenance_resources: [
        ("Iron", 0.05),
        ("Copper", 0.005),
        ("Polymers", 0.005),
        ("Water", 0.05),  // bootstrap draw
        ("Fluorine", 0.001),  // process reagent (no NaOH resource per v0.5 design rule)
    ],
    modifiers: [
        (modifier_type: "WaterProduction", value: 16.0),
    ],
    power_demand_mw: 300.0,
    available_atmospheres: [None],  // non-breathable only
),
```

**Note on maintenance_resources.** The v0.5 design rule is **no new
`ResourceType` entries** — Fluorine (already a resource) is used as
the process reagent. If the user later decides Fluorine is wrong,
swap for Sulfur or drop the row; do not add NaOH.

#### 8.2.2 He3Mine (mid-game, He-3-deposit body, tech-gated)

```ron
(
    id: "He3Mine",
    display_name: "Helium-3 Mine",
    description: "Regolith heating / atmospheric scoop plant that extracts 0.5 Mt/yr He-3. Powers 1 FusionReactor. Body-restricted to moons, gas giants, and asteroids — the body classes with He-3 deposits (regolith-implanted from solar wind on moons/asteroids, primordial in gas-giant atmospheres).",
    icon: "textures/ui/buildings/he3-mine.png",
    category: "Industry",
    build_points: 8000.0,
    workforce: 5000,
    required_tech: "lunar_colony",  // mid-game gate; see §5.1.1
    resource_costs: [
        ("Iron", 500.0),
        ("Titanium", 250.0),
        ("RareEarths", 100.0),
        ("Hydrogen", 50.0),  // reduction agent (regolith path) / scoop propellant (gas-giant path)
    ],
    maintenance_resources: [
        ("Iron", 1.0),
        ("Titanium", 0.05),
        ("Hydrogen", 0.5),
        ("Helium3", 0.05),  // bootstrap from a starter stockpile
        ("Water", 0.1),
        ("Polymers", 0.005),
    ],
    modifiers: [
        (modifier_type: "Helium3Production", value: 0.5),
    ],
    power_demand_mw: 1500.0,
    // SCHEMA ADDITION: see §8.4. Without this field, the construction panel
    // will show He3Mine as available on every body. The v0.5 design
    // rule explicitly REJECTS "He3Mine buildable on Earth / Mars."
    allowed_body_types: [Moon, GasGiant, Asteroid],
),
```

**Body restriction.** `allowed_body_types: [Moon, GasGiant, Asteroid]`
is the schema addition in §8.4. Without it, the construction panel
will show `He3Mine` as available on every body, including Earth and
Mars. **This is non-negotiable** per the v0.5 design rule: He-3
deposits only exist on these three body classes (regolith-He-3 from
solar-wind implantation on moons and asteroids; primordial He-3 in
gas-giant atmospheres). The construction panel will grey out `He3Mine`
on any body whose `BodyType` is not in the list. **The player does
NOT need a moon colony to build on a gas giant or asteroid** — only
the `lunar_colony` tech (the gate for off-world He-3 mining) and a
colony on the target body. The schema addition is a canary-3
dependency.

#### 8.2.3 AntimatterSynthesizer (late-game, K2-gated, grams)

```ron
(
    id: "AntimatterSynthesizer",
    display_name: "Antimatter Production Facility",
    description: "Particle-antiparticle pair-production reactor. 100 g/yr antimatter (game units; K2.0 unlock).",
    icon: "textures/ui/buildings/antimatter-synthesizer.png",
    category: "Industry",
    build_points: 50000.0,
    workforce: 50000,
    required_tech: "kardashev_k2",  // K2.0; see §5.1.3
    resource_costs: [
        ("Iron", 5000.0),
        ("Titanium", 2500.0),
        ("RareEarths", 1000.0),
        ("Copper", 500.0),
        ("Lithium", 250.0),
        ("Metamaterials", 50.0),
        ("Computronium", 5.0),
    ],
    maintenance_resources: [
        ("Iron", 5.0),
        ("Copper", 0.5),
        ("Lithium", 0.1),
        ("Water", 5.0),
        ("Metamaterials", 0.001),
        ("Computronium", 0.0001),
    ],
    modifiers: [
        // Tracked in GRAMS, not Mt. v0.5 design rule: grams throughout.
        (modifier_type: "AntimatterProduction", value: 100.0),  // 100 g/yr
    ],
    power_demand_mw: 50000.0,
),
```

**Note:** this is a **spec-level placeholder**; the antimatter unit
convention change (grams) is a major Rust change deferred to K2
design review. Until the Rust change lands, the value 100.0 is
interpreted in the *current* game unit (Mt), not grams. The
descriptive text in the UI should read "100 g/yr" regardless of the
underlying unit.

#### 8.2.4 ExoticMatterSynthesizer (late-game, K2-gated, kg)

```ron
(
    id: "ExoticMatterSynthesizer",
    display_name: "Exotic Matter Synthesizer",
    description: "Casimir-array / negative-energy-density synthesizer. 0.1 kg/yr exotic matter (placeholder; K2 design review).",
    icon: "textures/ui/buildings/exotic-synthesizer.png",
    category: "Industry",
    build_points: 100000.0,
    workforce: 100000,
    required_tech: "kardashev_k2",
    resource_costs: [
        ("Iron", 10000.0),
        ("Metamaterials", 100.0),
        ("Antimatter", 0.001),  // grams
    ],
    maintenance_resources: [
        ("Metamaterials", 0.01),
        ("Antimatter", 0.0001),
        ("Computronium", 0.001),
    ],
    modifiers: [
        (modifier_type: "ExoticMatterProduction", value: 0.1),  // 0.1 kg/yr
    ],
    power_demand_mw: 100000.0,
),
```

#### 8.2.5 MetamaterialsFab (late-game, K2-gated)

```ron
(
    id: "MetamaterialsFab",
    display_name: "Metamaterials Fabrication Facility",
    description: "Engineered-composite lab for cloaking, perfect lenses, advanced shielding. 0.05 Mt/yr (placeholder; K2 review).",
    icon: "textures/ui/buildings/metamaterials-fab.png",
    category: "Research",
    build_points: 25000.0,
    workforce: 20000,
    required_tech: "kardashev_k2",
    resource_costs: [
        ("Iron", 1500.0),
        ("Titanium", 750.0),
        ("RareEarths", 500.0),
        ("Copper", 250.0),
    ],
    maintenance_resources: [
        ("Iron", 1.0),
        ("Copper", 0.05),
        ("RareEarths", 0.01),
        ("Computronium", 0.001),
    ],
    modifiers: [
        (modifier_type: "MetamaterialsProduction", value: 0.05),
    ],
    power_demand_mw: 10000.0,
),
```

#### 8.2.6 ComputroniumSubstrate (late-game, K2-gated)

```ron
(
    id: "ComputroniumSubstrate",
    display_name: "Computronium Substrate Foundry",
    description: "Optimised-substrate foundry for Culture-level AI. 0.01 Mt/yr computronium (placeholder; K2 review).",
    icon: "textures/ui/buildings/computronium-substrate.png",
    category: "Research",
    build_points: 30000.0,
    workforce: 25000,
    required_tech: "kardashev_k2",
    resource_costs: [
        ("Iron", 2000.0),
        ("Silicates", 1000.0),
        ("RareEarths", 500.0),
        ("Copper", 250.0),
    ],
    maintenance_resources: [
        ("Iron", 1.0),
        ("Silicates", 0.5),
        ("RareEarths", 0.01),
        ("Copper", 0.05),
    ],
    modifiers: [
        (modifier_type: "ComputroniumProduction", value: 0.01),
    ],
    power_demand_mw: 15000.0,
),
```

#### 8.2.7 GoldMine (mid-game, Tier 3, always-available)

```ron
(
    id: "GoldMine",
    display_name: "Gold Mine",
    description: "Placer / lode gold extraction. 0.0001 Mt/yr Au per mine (~3,200 troy oz/yr — small-mine scale). Consumed by electronics industry (SemiconductorFab maintenance).",
    icon: "textures/ui/buildings/gold-mine.png",
    category: "Industry",
    build_points: 1200.0,
    workforce: 4000,
    required_tech: "",
    resource_costs: [
        ("Iron", 200.0),
        ("Copper", 50.0),
        ("Water", 100.0),
    ],
    maintenance_resources: [
        ("Iron", 0.5),
        ("Copper", 0.05),
        ("Water", 0.1),
        ("Cyanide", 0.001),  // cyanidation process reagent
        ("Polymers", 0.003),
        ("Mercury", 0.0001),  // amalgamation process (small loss rate)
    ],
    modifiers: [
        (modifier_type: "GoldProduction", value: 0.0001),
    ],
    power_demand_mw: 400.0,
),
```

**Note on cyanidation.** The cyanidation process uses sodium cyanide
(NaCN) as a leaching agent. The patch's v0.5 design rule is **no new
`ResourceType` entries**, so cyanidation is represented abstractly
with a small `Iron` / `Cyanide`-placeholder maintenance cost rather
than adding NaCN as a new resource. The display string is `Cyanide`
to indicate process reagent; the underlying simulation reads the
string. Mercury (Hg) is shown for the historical Hg-amalgamation path
— note that industrial gold mining has been moving AWAY from Hg since
the Minamata Convention (2013); the patch keeps Hg as a small loss
item for the historical / artisanal-mining path.

#### 8.2.8 SilverMine (mid-game, Tier 3, always-available)

```ron
(
    id: "SilverMine",
    display_name: "Silver Mine",
    description: "Silver extraction (often a lead-zinc byproduct; in-game modelled as a dedicated Ag operation). 0.001 Mt/yr Ag per mine (~1,000 t/yr — large-mine scale). Consumed by electronics industry (SemiconductorFab maintenance).",
    icon: "textures/ui/buildings/silver-mine.png",
    category: "Industry",
    build_points: 1500.0,
    workforce: 5000,
    required_tech: "",
    resource_costs: [
        ("Iron", 250.0),
        ("Copper", 75.0),
        ("Lead", 50.0),
        ("Zinc", 50.0),
    ],
    maintenance_resources: [
        ("Iron", 0.5),
        ("Copper", 0.05),
        ("Lead", 0.01),
        ("Zinc", 0.01),
        ("Water", 0.1),
        ("Polymers", 0.003),
    ],
    modifiers: [
        (modifier_type: "SilverProduction", value: 0.001),
    ],
    power_demand_mw: 500.0,
),
```

**Note on Lead / Zinc.** Lead and Zinc are *not* in the current
`ResourceType` enum. The patch's v0.5 design rule is **no new
`ResourceType` entries**, so the resource_costs row references them
as placeholder strings. The simulation system reads the string at
evaluation time; if Lead / Zinc are not present in `BuildingsData`
or `ResourceType`, the system treats them as a no-op draw (zero
balance impact). This preserves the design rule while still giving
the player a meaningful cost story (silver mines need lead-zinc
processing gear).

#### 8.2.9 PlatinumMine (mid-game, Tier 3, always-available)

```ron
(
    id: "PlatinumMine",
    display_name: "Platinum Mine",
    description: "Platinum-group-metal extraction from layered intrusions (Bushveld / Norilsk analog). 0.00001 Mt/yr Pt per mine (~10 t/yr — realistic PGM output). Consumed by electronics industry (SemiconductorFab maintenance).",
    icon: "textures/ui/buildings/platinum-mine.png",
    category: "Industry",
    build_points: 2000.0,
    workforce: 6000,
    required_tech: "",
    resource_costs: [
        ("Iron", 300.0),
        ("Nickel", 200.0),  // PGM is often a Ni/Cu byproduct
        ("Copper", 100.0),
        ("Chromium", 50.0),  // layered intrusion mining gear
    ],
    maintenance_resources: [
        ("Iron", 0.5),
        ("Nickel", 0.05),
        ("Copper", 0.01),
        ("Chromium", 0.005),
        ("Water", 0.1),
        ("Polymers", 0.005),
    ],
    modifiers: [
        (modifier_type: "PlatinumProduction", value: 0.00001),
    ],
    power_demand_mw: 600.0,
),
```

**Note on per-build value (0.00001 Mt/yr).** Platinum is the
**rarcst mined resource in the game** (USGS 2026: 200 t/yr global,
Bushveld Complex dominates). The per-build value 0.00001 Mt/yr
(10 t/yr per mine) is below the game's Mt-unit rounding precision
but matches real-world large-mine output. The UI panel does not
display tonnes-vs-Mt at this scale; the player's Economy panel
shows total Pt produced (0.0002 Mt/yr at 20 mines = 200 t/yr). This
is a known calibration limitation, same as Titanium / Phosphorus
(§3.5 manageable-count exceptions).

### 8.3 EXISTING buildings that need their `effects` field adjusted (the folding)

The user edits the listed modifier values. The `resource_costs`,
`maintenance_resources`, and `power_demand_mw` fields are unchanged
unless explicitly noted.

#### 8.3.1 Farm (Population)

```diff
- (modifier_type: "FoodProduction", value: 9000.0),
+ (modifier_type: "FoodProduction", value: 360.0),
```

#### 8.3.2 Greenhouse (Population)

```diff
- (modifier_type: "FoodProduction", value: 5000.0),
+ (modifier_type: "FoodProduction", value: 200.0),
```

#### 8.3.3 AquacultureFacility (Population)

```diff
- (modifier_type: "FoodProduction", value: 1500.0),
+ (modifier_type: "FoodProduction", value: 200.0),
```

#### 8.3.4 AgriDome (Population)

```diff
- (modifier_type: "FoodProduction", value: 180.0),
+ (modifier_type: "FoodProduction", value: 4.0),
```

#### 8.3.5 ChemicalPlant (Industry) — multiple modifier edits

```diff
- (modifier_type: "HydrogenSynthesis", value: 100.0),
+ (modifier_type: "HydrogenSynthesis", value: 3.0),
- (modifier_type: "AmmoniaSynthesis", value: 200.0),
+ (modifier_type: "AmmoniaSynthesis", value: 6.0),
- (modifier_type: "PolymerSynthesis", value: 450.0),
+ (modifier_type: "PolymerSynthesis", value: 18.0),
- (modifier_type: "TritiumBreeding", value: 0.05),
+ (modifier_type: "TritiumBreeding", value: 0.001),
+ (modifier_type: "DeuteriumProduction", value: 0.18),
```

#### 8.3.6 AtmosphericProcessor (Industry) — split sweep into per-gas

```diff
- (modifier_type: "AtmosphericHarvesting", value: 500.0),
+ (modifier_type: "NitrogenHarvesting", value: 7.0),
+ (modifier_type: "OxygenProduction", value: 200.0),
+ (modifier_type: "ArgonProduction", value: 0.028),  // noble-gas byproduct; see §5.18
```

The rename `AtmosphericHarvesting` → `NitrogenHarvesting` is
**breaking** — no deprecated alias. CO₂ remains an implicit
byproduct of the same process (no separate modifier; audit §4.3).
Ar was previously a "no consumer / no producer" pair and is now
explicit (see §5.18) — `ArgonProduction` 0.028 Mt/yr per processor
is the byproduct rate at the audit's calibration scale (USGS 2024:
~700,000 t/yr global Ar, fold = 25 processors × 0.028 = 0.7 Mt/yr).

#### 8.3.7 HydrocarbonExtractor (Industry) — split into per-resource

```diff
- (modifier_type: "MiningEfficiency", value: 4000.0),
+ (modifier_type: "CarbonProduction", value: 480.0),
+ (modifier_type: "MethaneProduction", value: 164.0),
+ (modifier_type: "SulfurByproduct", value: 5.0),
```

#### 8.3.8 Mine (Industry) — scale Fe down + multi-output (fold)

```diff
- (modifier_type: "MiningEfficiency", value: 1800.0),
+ (modifier_type: "MiningEfficiency", value: 80.0),  // Fe slice
+ (modifier_type: "CopperProduction", value: 1.0),
+ (modifier_type: "NickelProduction", value: 0.18),
+ (modifier_type: "PhosphorusProduction", value: 0.31),
+ (modifier_type: "CobaltProduction", value: 0.012),
+ (modifier_type: "LithiumProduction", value: 0.006),
+ (modifier_type: "FluorineProduction", value: 0.25),
```

`MiningEfficiency` is preserved as a generic fallback for backward
compatibility (matches the v1 §9.3.10 reviewer question; the existing
`MiningEfficiency` modifier type is **not** changed to a
per-resource-only scheme).

#### 8.3.9 Refinery (Industry) — scale Fe down + multi-output (fold)

```diff
- (modifier_type: "MiningEfficiency", value: 1800.0),
+ (modifier_type: "MiningEfficiency", value: 80.0),  // Fe slice (refining capacity)
+ (modifier_type: "AluminumProduction", value: 2.3),
+ (modifier_type: "TitaniumProduction", value: 0.02),
+ (modifier_type: "ChromiumProduction", value: 1.7),
+ (modifier_type: "MagnesiumProduction", value: 0.06),
```

#### 8.3.10 DeepDrill (Industry) — multi-output (fold)

```diff
- (modifier_type: "DeepMiningEfficiency", value: 100.0),
+ (modifier_type: "DeepMiningEfficiency", value: 100.0),  // generic fallback preserved
+ (modifier_type: "UraniumProduction", value: 0.0028),
+ (modifier_type: "ThoriumProduction", value: 0.0025),
+ (modifier_type: "TungstenProduction", value: 0.005),
+ (modifier_type: "RareEarthsProduction", value: 0.012),
```

`DeepMiningEfficiency` is preserved as a generic fallback (same
rationale as `MiningEfficiency`).

#### 8.3.11 StripMine (Industry) — split into per-resource

```diff
- (modifier_type: "BulkMiningEfficiency", value: 5000.0),
+ (modifier_type: "SilicatesProduction", value: 400.0),
+ (modifier_type: "IronProduction", value: 80.0),
+ (modifier_type: "AluminumProduction", value: 2.3),
+ (modifier_type: "CopperProduction", value: 1.0),
+ (modifier_type: "TitaniumProduction", value: 0.02),
+ (modifier_type: "NickelProduction", value: 0.18),
+ (modifier_type: "ChromiumProduction", value: 1.7),
```

#### 8.3.12 FusionReactor (Power) — downscale He-3 and D, tech-gate

```diff
- required_tech: "",  // was always-available
+ required_tech: "fusion_power",  // mid-game gate; see §5.1.2
  maintenance_resources: [
-     ("Helium3", 10.0),
-     ("Deuterium", 5.0),
+     ("Helium3", 0.5),  // 20× downscale
+     ("Deuterium", 0.25),  // 20× downscale
      ("Cobalt", 0.002),
      ("Fluorine", 0.003),
      ("Titanium", 0.05),
      ("Lithium", 0.0005),
  ],
```

The `PowerGeneration` modifier stays at 2000 or scales per §7.2 to 1500.

#### 8.3.13 DTFusionReactor (Power) — downscale D, tech-gate

```diff
- required_tech: "",  // was always-available
+ required_tech: "fusion_power",  // mid-game gate
  maintenance_resources: [
-     ("Deuterium", 0.0015),
-     ("Tritium", 0.0005),
+     ("Deuterium", 0.0001),
+     ("Tritium", 0.0005),  // unchanged; matched to new ChemicalPlant T breed rate
      ...
  ],
```

#### 8.3.14 DHe3FusionReactor (Power) — tech-gate only

```diff
- required_tech: "",  // was always-available
+ required_tech: "fusion_power",  // mid-game gate
  // maintenance unchanged: 0.0008 Mt/yr He-3 and 0.0008 Mt/yr D are already
  // appropriately low. The required_anomalies: ["magnetic_anomaly"] field
  // is preserved.
```

#### 8.3.15 BreederReactor (Power) — downscale Pu breeding

```diff
- (modifier_type: "PlutoniumBreeding", value: 0.23),
+ (modifier_type: "PlutoniumBreeding", value: 0.001),
```

#### 8.3.16 Power plants (Power category) — per §7.2

For each power plant, scale the `PowerGeneration` modifier per the
table in §7.2. (The values are "200, 250, 400, 80, 800, 600, 250,
1500, 2500, 2000, 600, 500" for Solar, Wind, Hydro, Geothermal,
Coal, NG, Fission, Fusion, D-T, D-He3, Thorium, Breeder
respectively.)

#### 8.3.17 RecyclingCenter (Infrastructure) — leave as-is

The 500 Mt/yr undifferentiated recycled-metal modifier is kept. It
represents a "mixed recycled metal" output that the player can use
without specifying which metal. Patch leaves the value unchanged
for now; a future patch can split it per resource.

#### 8.3.18 SemiconductorFab (Industry) — add Au/Ag/Pt/Ar maintenance

The v0.5 patch makes `SemiconductorFab` the consumer of the four
new precious-metal + noble-gas resources (§5.15–§5.18). Its
`maintenance_resources` field gains 4 new rows, in priority order
(lowest-value first to keep the GRA-22c audit happy — the
"4–6 distinct resources" rule is exceeded by 1 if the existing
maintenance is already 5 rows):

```diff
  maintenance_resources: [
      ("Iron", 0.01),
      ("Copper", 0.005),
      ("Nickel", 0.0005),
      ("RareEarths", 0.0001),
+     ("Gold", 0.0001),          // electronics-industry Au share; see §5.15
+     ("Silver", 0.001),         // solar PV paste, solders, contacts; see §5.16
+     ("Platinum", 0.00001),     // autocatalyst / sensor / fuel cell; see §5.17
+     ("Argon", 0.028),          // inert fab atmosphere; see §5.18
  ],
```

**GRA-22c audit consideration.** The maintenance list now has
8 resources (4 existing + 4 new). The audit's rule is **4–6
distinct resources** (`src/colony/data.rs:266-290`,
`MAINTENANCE_AUDIT_MIN..MAX = 4..6`). The 8-resource list exceeds
`MAX = 6` by 2. Two paths forward:

* **Loosen the audit** — raise `MAINTENANCE_AUDIT_MAX` from 6 to
  10 (a one-line change in `src/colony/data.rs:267`). Rationale:
  the electronics industry *is* resource-intensive in real life
  (Si, Cu, Au, Ag, Pt, Ar, REE, photoresists, etc.). The 4–6
  budget is a guideline, not a hard physical law.
* **Split the consumer** — fold Au into `Refinery` (general
  industrial), Ag/Pt/Ar into `SemiconductorFab`. This is uglier
  (gold is not a refinery concern) and would still exceed
  `MAINTENANCE_AUDIT_MAX` on the remaining building.

The patch recommends **option 1 (loosen to 10)** and flags this
as a §9.5 open question for the user. v0.5.x does not require
the change to land — the existing maintenance list passes the
audit; the new 4 rows are *additive* and do not break the
existing entries.

### 8.4 Schema additions to `src/colony/data.rs`

**BuildingDefinition needs a body-type field.** The current schema
(`src/colony/data.rs:69-134`) has `available_atmospheres: Vec<AtmosphereKind>`
but no `allowed_body_types: Vec<BodyType>` field. Without it, the
`He3Mine` body restriction cannot be encoded cleanly in RON.

**`BodyType` is already defined** in `src/plugins/solar_system_data.rs:7-17`:
`Star, Planet, GasGiant, DwarfPlanet, Moon, Asteroid, Comet, Ring`.

**Proposed spec** (additive, backward-compatible):

```rust
// In src/colony/data.rs, add to BuildingDefinition:
/// Body kinds on which this building can be constructed.
/// Empty (= default) = buildable on every body kind. The construction
/// panel filters the available and locked lists against the currently
/// selected body's `BodyType` (companion to `available_atmospheres`).
///
/// He3Mine uses this to enforce the "He-3-deposit body" rule
/// (Moon, GasGiant, Asteroid); see `docs/design/BALANCE_PATCHES_v0.5.md` §5.1.
#[serde(default)]
pub allowed_body_types: Vec<crate::plugins::solar_system_data::BodyType>,
```

**The free function `building_is_available_on` (`src/colony/data.rs:253-261`)
needs a `body_type` parameter** (in addition to the existing
`body_breathable: Option<bool>`):

```rust
pub fn building_is_available_on(
    def: &BuildingDefinition,
    body_breathable: Option<bool>,
    body_type: Option<&BodyType>,  // NEW; None = unknown
) -> bool {
    let Some(breathable) = body_breathable else {
        return true;  // atmosphere unknown → pass-through
    };
    if !def.available_atmospheres.iter().any(|a| match a {
        AtmosphereKind::Breathable => breathable,
        AtmosphereKind::None => !breathable,
    }) {
        return false;
    }
    // Body-type check: empty list = buildable everywhere.
    if def.allowed_body_types.is_empty() {
        return true;
    }
    match body_type {
        Some(bt) => def.allowed_body_types.contains(bt),
        None => true,  // body type unknown → pass-through
    }
}
```

**`BuildingsData::is_available_on` (`src/colony/data.rs:210-219`) needs
the same `body_type` parameter** threaded through.

**Impact.** Any existing RON entry without `allowed_body_types` gets
the empty-list default and is buildable on every body kind (the
current behavior). The schema change is **additive and non-breaking**
at the RON level; it requires a one-line update to the predicate
function and a small change to the construction panel to pass the
selected body's `BodyType`.

**Canary 3 dependency.** This schema addition is a canary-3
dependency. Without it, the body restriction on `He3Mine` cannot
be enforced at the construction panel — and the user has explicitly
rejected "He3Mine buildable on Earth" — so the schema addition
is **non-negotiable** for the mid-game tier.

### 8.5 Rust delta in `src/economy/types.rs` (and `src/research/types.rs`)

For the 9 new building IDs (§8.2.1-§8.2.9), the user adds matching
`BuildingType` enum variants in `src/colony/types.rs:7` (where the
existing enum is defined). The user adds the following 9 variants:

```rust
pub enum BuildingType {
    // ... existing variants ...
    WaterProcessor,
    He3Mine,
    GoldMine,
    SilverMine,
    PlatinumMine,
    AntimatterSynthesizer,
    ExoticMatterSynthesizer,
    MetamaterialsFab,
    ComputroniumSubstrate,
}
```

**`ModifierType` is a string-based dispatch** in the current code
(`src/research/types.rs:130-149` shows the enum, but the `BuildingModifierDef`
struct in `src/colony/data.rs:38-44` uses `pub modifier_type: String`).
The RON `modifier_type` is a *string*, not an enum variant. So **no
`ModifierType` enum changes are required** for the new production
modifiers (WaterProduction, OxygenProduction, NitrogenHarvesting, etc.).
The simulation system reads the string at evaluation time.

The 3 new technologies (§5.1.1, §5.1.2, §5.1.3) require new tech
entries in `assets/data/technologies.ron` (no Rust enum changes; the
`id` field is a string).

### 8.6 NEW technologies to add to `assets/data/technologies.ron` (3 total)

#### 8.6.1 `lunar_colony` (Tier 2, mid-game)

```ron
(
    id: "lunar_colony",
    name: "Lunar Colony Engineering",
    category: SpaceTechnology,
    description: "Engineering, life-support, and ISRU techniques for sustained off-world resource extraction. The off-world He-3 mining gate (Moon regolith, gas-giant atmospheric scoops, asteroid regolith), named for the first off-world He-3 extraction source in real-world plans. See docs/design/BALANCE_PATCHES_v0.5.md §5.1.1.",
    research_cost: 7500.0,
    prerequisites: ["space_habitation", "closed_loop_ecology"],
    unlocks_components: [],
    unlocks_engineering: ["lunar_outpost_kit", "isru_oxygen_plant", "lunar_hab_module"],
    modifiers: [],
    tier: 2,
),
```

#### 8.6.2 `fusion_power` (Tier 3, mid-game, requires `lunar_colony`)

```ron
(
    id: "fusion_power",
    name: "Fusion Power Engineering",
    category: Energy,
    description: "Magnetic-confinement fusion (tokamak, stellarator) and inertial-confinement fusion (laser-driven) reactor engineering at power-plant scale. Requires lunar He-3 supply for D-³He variants. See docs/design/BALANCE_PATCHES_v0.5.md §5.1.2.",
    research_cost: 12000.0,
    prerequisites: ["lunar_colony", "fission_power"],
    unlocks_components: ["fusion_plasma_chamber", "tritium_breeding_blanket", "magnetic_confinement_coil"],
    unlocks_engineering: ["fusion_reactor", "dt_fusion_reactor", "dhe3_fusion_reactor"],
    modifiers: [
        (modifier_type: PowerGeneration, value: 100.0),
    ],
    tier: 3,
),
```

#### 8.6.3 `kardashev_k2` (Tier 4, late-game)

```ron
(
    id: "kardashev_k2",
    name: "Kardashev K2 Engineering",
    category: Physics,
    description: "Planetary-scale engineering, antimatter pair-production, exotic-matter stabilization, metamaterial lattice synthesis, and computronium substrate fabrication. K2 Kardashev transition (ROADMAP §9.1). See docs/design/BALANCE_PATCHES_v0.5.md §5.1.3.",
    research_cost: 500000.0,  // placeholder; K2 design review
    prerequisites: ["fusion_power"],
    unlocks_components: ["antimatter_magnetic_bottle", "casimir_array", "metamaterial_lattice", "computronium_wafer"],
    unlocks_engineering: ["antimatter_synthesizer", "exotic_matter_synthesizer", "metamaterials_fab", "computronium_substrate"],
    modifiers: [],
    tier: 4,
),
```

### 8.7 Files the user must edit (consolidated checklist)

| File | Edit type | Section |
|---|---|---|
| `src/colony/components.rs:292-294` | Rust constant delta | §8.1 |
| `src/colony/components.rs:271-286` | Rust helper comment + formula | §8.1 |
| `src/colony/data.rs:69-134` | Schema: add `allowed_body_types: Vec<BodyType>` to `BuildingDefinition` | §8.4 |
| `src/colony/data.rs:253-261` | Schema: extend `building_is_available_on` predicate | §8.4 |
| `src/colony/data.rs:210-219` | Schema: extend `BuildingsData::is_available_on` | §8.4 |
| `src/colony/types.rs:7` | Add 9 new `BuildingType` enum variants | §8.5 |
| `assets/data/buildings.ron` | 9 new building entries | §8.2.1-§8.2.9 |
| `assets/data/buildings.ron` | 18 existing building modifier edits (17 fold + 1 `SemiconductorFab` maintenance) | §8.3.1-§8.3.18 |
| `assets/data/technologies.ron` | 3 new tech entries | §8.6.1-§8.6.3 |
| `assets/icons/...` | 9 new building icons (artwork) | out of scope |

**Total RON entries added: 9 buildings + 3 technologies = 12.**
**Total existing RON entries modified: 18.** **Total Rust lines
changed: ~10 in `components.rs` + ~30 in `data.rs` + ~9 in
`types.rs` = ~49.** **Total `BuildingType` enum variants added: 9.**
**No `ModifierType` enum variants added (string-based dispatch).**

### 8.8 Apply order (canary-first)

The patch is **canary-first** per the user's UI workflow preferences
(canary-first migrations, sequential rollout, parallel old/new,
graduate per panel). The user lands one building edit at a time,
runs the test suite, then rolls forward:

1. **Canary 1** — Rust constant + Farm modifier.
   `src/colony/components.rs:292-294` (1 function, 2 comments; ~10
   lines). Plus `buildings.ron` Farm/Greenhouse/AquacultureFacility/
   AgriDome FoodProduction scaledown (§8.3.1-§8.3.4; ~4 line
   diffs). Total: ~4 lines of code + 4 RON line diffs. Test that
   Earth still feeds its population.

2. **Canary 2** — WaterProcessor.
   `buildings.ron` `WaterProcessor` entry (§8.2.1; ~25 lines). Plus
   `BuildingType::WaterProcessor` enum variant (§8.5; 1 line). Test
   that non-breathable outposts now have a water source.

3. **Canary 3** — Mid-game He-3 chain.
   This is the biggest canary. Three things in sequence:
   1. **Schema addition** (§8.4): `allowed_body_types: Vec<BodyType>`
      on `BuildingDefinition`, plus the `building_is_available_on`
      predicate extension, plus the `BuildingsData::is_available_on`
      threading. Test that existing buildings still parse and remain
      available on every body kind.
   2. **3 new technologies** (§8.6): `lunar_colony`, `fusion_power`,
      `kardashev_k2`. Test that the tech tree renders and the
      prereq chain is enforced.
   3. **He3Mine** (§8.2.2) + **FusionReactor downscale** (§8.3.12)
      + **DTFusionReactor downscale** (§8.3.13) + **DHe3FusionReactor
      tech-gate** (§8.3.14). Test that:
      * The construction panel hides `He3Mine` on Earth, Mars, and
        any non-He-3-deposit body (body restriction enforced by the
        schema).
      * The construction panel hides `FusionReactor` until `fusion_power`
        is researched (tech gate enforced).
      * The construction panel hides `FusionReactor` even after
        `fusion_power` is researched, until `lunar_colony` is also
        researched (prereq chain enforced).
      * A FusionReactor on Luna draws He-3 from Luna's LocalStockpile
        via the existing freight logistics (no new freight code).

4. **Roll forward** — Folding.
   `Mine` multi-output (§8.3.8), `Refinery` multi-output (§8.3.9),
   `DeepDrill` multi-output (§8.3.10), `StripMine` split (§8.3.11),
   `HydrocarbonExtractor` split (§8.3.7), `AtmosphericProcessor`
   split + Ar fold (§8.3.6), `ChemicalPlant` H₂/NH₃/polymers/T/D (§8.3.5),
   `BreederReactor` Pu downscale (§8.3.15), power plants per §7.2
   (§8.3.16). Land in batches of 3-4 building edits each, run tests
   between batches.

5. **Canary 4** — Precious-metal + noble-gas mining.
   `GoldMine` (§8.2.7) + `SilverMine` (§8.2.8) + `PlatinumMine`
   (§8.2.9) + `SemiconductorFab` maintenance update (§8.3.18) +
   `AtmosphericProcessor` `ArgonProduction` fold (§8.3.6 — already
   in the roll-forward). Plus 3 `BuildingType` enum variants
   (§8.5). Test that:
   * The construction panel shows `GoldMine` / `SilverMine` /
     `PlatinumMine` on Earth (no body restriction; deposits are
     crustal on Earth-sized bodies).
   * The construction panel shows `ArgonProduction` modifier
     updating the per-build Ar output on every `AtmosphericProcessor`
     without breaking the existing N₂/O₂ modifier pair.
   * The `SemiconductorFab` maintenance list parses (8 distinct
     resources — see §8.3.18 GRA-22c audit consideration).
   * Earth's 25 `GoldMine` × 0.0001 = 0.0025 Mt/yr (≈ 80% of
     USGS 2026 demand; the rest is recycled Au and is out of scope).
   * Earth's 25 `SilverMine` × 0.001 = 0.025 Mt/yr = USGS 2026 demand.
   * Earth's 20 `PlatinumMine` × 0.00001 = 0.0002 Mt/yr = USGS 2026 demand.
   * Earth's 25 `AtmosphericProcessor` × 0.028 = 0.7 Mt/yr = USGS 2024 demand.
   * The player's economy panel shows the 4 new resources
     (Gold/Silver/Platinum/Argon) and their per-build values.

6. **K2 late-game** — `AntimatterSynthesizer` (§8.2.3) +
   `ExoticMatterSynthesizer` (§8.2.4) + `MetamaterialsFab` (§8.2.5)
   + `ComputroniumSubstrate` (§8.2.6). All gated on `kardashev_k2`.
   Land as a single batch (4 buildings, all share the same K2 gate).
   Mark all values "approximate" pending K2 design review.

### 8.9 RON syntax notes

* `modifier_type` is a **string**, not an enum. New production
  modifiers (e.g. `WaterProduction`, `OxygenProduction`,
  `Helium3Production`, `GoldProduction`, `ArgonProduction`) are
  added as plain strings; the simulation system reads the string
  at evaluation time.
* `required_tech: ""` means *always available*. v2 sets it to the
  specific tech id (e.g. `"lunar_colony"`, `"fusion_power"`,
  `"kardashev_k2"`) for the gated buildings.
* `available_atmospheres` is a list of `AtmosphereKind` variants
  (`Breathable` / `None`). Default `[Breathable, None]` = buildable
  on every body kind.
* `allowed_body_types` is a list of `BodyType` variants (the
  schema addition in §8.4). Empty list = buildable on every body
  kind. `WaterProcessor` uses `[None]` for atmosphere and the
  default empty list for body types. `He3Mine` uses the
  default empty list for atmosphere and `[Moon, GasGiant, Asteroid]`
  for body types (the new field) — the three body classes with
  He-3 deposits. The 3 precious-metal mines and the
  `AtmosphericProcessor` have no `allowed_body_types` (buildable
  everywhere by default).
* `maintenance_resources` is `Vec<ResourceCostEntry>` with 4–6
  distinct resources per the existing GRA-22c audit
  (`src/colony/data.rs:266-290`). The proposed entries in §8.2
  each have 5–6 resources; the existing buildings retain their
  4–6 maintenance entries. **Exception:** `SemiconductorFab` is
  raised to 8 maintenance rows to accommodate the Au/Ag/Pt/Ar
  consumer (see §8.3.18).

---

## 9. Self-checks and open questions

### 9.1 Stop-condition check (per the brief)

| Stop condition | Status |
|---|---|
| `docs/design/BALANCE_PATCHES_v0.5.md` exists (lean v2) | ✅ |
| All three tiers covered | ✅ (§4 early, §5 mid, §6 late) |
| Tier-summary table concrete at the top | ✅ (§2) |
| 10–50 constraint met for every early-game resource | ✅ (§4.17) |
| Mid-game He-3 fix concrete, tech-gated, AND body-restricted | ✅ (§5.1, body-restricted to `[Moon, GasGiant, Asteroid]`) |
| Mid-game precious-metal + noble-gas mining (Au/Ag/Pt/Ar) | ✅ (§5.15–§5.18; USGS 2026 numbers, fold into `SemiconductorFab` consumer) |
| Late-game uses grams for Antimatter, marks exotics approximate | ✅ (§6) |
| 3 new technologies spec'd with full prereq chains | ✅ (§5.1.1, §5.1.2, §5.1.3) |
| Schema addition (`allowed_body_types`) flagged | ✅ (§8.4) |
| Implementation notes sufficient to apply without re-asking | ✅ (§8) |

### 9.2 Manageable-count check (per-resource verification, early game)

| Resource | Demand (Mt/yr Earth) | Per-build (Mt/yr) | Implied count | In 10-50? |
|---|---:|---:|---:|:---:|
| Food | 9,020 | 360 (hard-coded) | 25 | ✅ (1 Farm = 1/25 of Earth = 327M people fed; v0.5.1 corrected the unit error) |
| Water (closed-loop) | 410 | 16 | 26 | ✅ |
| Oxygen (respiration) | 6,888 | 200 | 34 | ✅ |
| Nitrogen (industrial) | 148 | 7 | 21 | ✅ |
| Hydrogen | 98 | 3 | 33 | ✅ |
| Methane | 4,100 | 164 | 25 | ✅ |
| Ammonia | 197 | 6 | 33 | ✅ |
| Phosphorus | 4.6 | 0.31 | 15 | ✅ (exception flagged) |
| Iron | 2,501 | 80 | 31 | ✅ |
| Aluminum | 70 | 2.3 | 30 | ✅ |
| Copper | 23 | 1.0 | 23 | ✅ |
| Titanium | 0.33 | 0.02 | 17 | ✅ (exception flagged) |
| Silicates | 9,840 | 400 | 25 | ✅ |
| Polymers | 451 | 18 | 25 | ✅ |
| Nickel | 3.7 | 0.18 | 21 | ✅ |
| Carbon | 11,972 | 480 | 25 | ✅ |
| Gold | 0.003 | 0.0001 | 25 | ✅ (Au is Tier 3, weighted 0.3; below "1/300 of world" bar — flagged) |
| Silver | 0.025 | 0.001 | 25 | ✅ |
| Platinum | 0.0002 | 0.00001 | 20 | ✅ (manageable-count exception: per-build < 0.0001 Mt/yr) |
| Argon | 0.7 | 0.028 | 25 | ✅ |

**All 16 early-game resources land in 10–50.** No resource is
out of band.

### 9.3 v1 → v2 deltas (summary of the lean pass)

| v1 (1,955 lines) | v2 (this doc) | Why the change |
|---|---|---|
| 23 new buildings | **9 new buildings** | Fold most fixes into existing `Mine` / `Refinery` / `ChemicalPlant` / `AtmosphericProcessor` / `DeepDrill` / `HydrocarbonExtractor` / `StripMine` `effects` fields. The 9 that remain: `WaterProcessor`, `He3Mine`, `GoldMine`, `SilverMine`, `PlatinumMine`, plus 4 K2 exotics. Argon folds into `AtmosphericProcessor` (it's a cryogenic byproduct of O₂/N₂ separation). |
| 27 new `ModifierType` enum variants | **0 new `ModifierType` variants** | `modifier_type` is a string; no enum changes |
| 4 NEW K2 technologies (optional) | **3 new technologies** (`lunar_colony`, `fusion_power`, `kardashev_k2`) | Mid-game He-3 chain is now tech-gated (was always-available in v1) |
| He3Mine buildable on Earth | **He3Mine body-restricted to `[Moon, GasGiant, Asteroid]`** | The v0.5 design rule: He-3 only exists on these three body classes — regolith-implanted by solar wind (moons, asteroids) or primordial in atmosphere (gas giants). He3Mine is never buildable on Earth, Mars, or other He-3-barren bodies. |
| He3Mine Moon-only | **He3Mine body-restricted to `[Moon, GasGiant, Asteroid]`** | The user's revised v0.5 design rule: He-3 deposits are real on all three body classes (Moon regolith, asteroid regolith, gas-giant atmosphere), not just the Moon. |
| No Au/Ag/Pt/Ar consumer (deferred) | **`SemiconductorFab` maintenance update — 4 new rows for Au/Ag/Pt/Ar** | The v0.5 design rule: fold consumers into existing industry buildings rather than adding new `ElectronicsIndustry` / `SemiconductorAdvancedFab` etc. The 3 precious-metal mines + 1 atmospheric fold cover the producers. |
| FusionReactor always-available | **FusionReactor tech-gated to `fusion_power`** (which requires `lunar_colony`) | The v0.5 design rule: no fusion power plants at game start in 2026 |
| No body-type schema | **`allowed_body_types: Vec<BodyType>` schema addition** | Needed to enforce the He-3-deposit body restriction on `He3Mine` |
| NaOH / Cl₂ new `ResourceType` entries | **Fluorine in `maintenance_resources`** (no new resources) | The v0.5 design rule: do not add new `ResourceType` entries |
| NaOH / Cl₂ in maintenance | **No new `ResourceType` entries** | The Bayer and Kroll processes use Fluorine as the process reagent |
| Antimatter in Mt | **Antimatter in grams** | The v0.5 design rule: grams throughout |
| Save compatibility considered | **No deprecated aliases; rename is breaking** | The v0.5 design rule: save compatibility is not a concern |
| 1 Rust constant delta + ~60 Rust lines | **1 Rust constant delta + ~49 Rust lines** | The 23 → 9 building reduction cuts the enum-variant additions; the new 3 precious-metal mines add back 3 enum variants |

### 9.4 Resources where the manageable-count constraint was hardest to satisfy

1. **Phosphorus** (early-game Tier 1, life support closed-loop). Per-
   capita 0.56 kg/p/yr is very low; 25 buildings at 0.18 Mt/yr per
   build is below the audit's "1 building ≈ 1/300 of world" bar.
   v2 keeps v1's exception: target_count = 15, per_build = 0.31 Mt/yr.
2. **Titanium** (early-game Tier 2, structural / aerospace). Per-
   capita 0.04 kg/p/yr = 40 g/p/yr; 17 buildings at 0.02 Mt/yr per
   build = 20 kt/yr. The game unit (Mt) is too coarse for the
   per-build value. v2 keeps v1's exception: target_count = 17,
   per_build = 0.02 Mt/yr. The rounding precision is a known
   limitation (no UI panel displays tonnes-vs-Mt at this scale).
3. **Helium-3** (mid-game). The 1:1 He3Mine : FusionReactor
   ratio makes the count *exactly* 10 for a mature He-3-deposit
   outpost (10 FusionReactors, 10 He3Mines). This is the *lower
   bound* of the 10–50 constraint — any fewer reactors and the
   player hasn't scaled to mid-game yet; any more and the count
   exits the band. The 1:1 ratio is the design choice that makes
   the band work; deviating from it (e.g. 0.5 mine per reactor)
   would let the player run more reactors per mine, but the freight
   logistics would then need a buffer for the He-3 hop.
4. **Platinum** (mid-game Tier 3, precious / catalytic). Per-
   capita 24 mg/p/yr; 200 t/yr global demand; target_count = 20
   mines at 0.00001 Mt/yr per build = 0.0002 Mt/yr = USGS 2026
   demand. The per-build is **below the 0.0001 Mt/yr rounding
   precision** of the game unit (Mt). The patch rounds to 5
   significant figures (0.00001) and the UI panel does not display
   tonnes-vs-Mt at this scale. Same exception pattern as Titanium
   and Phosphorus. User can refine.

### 9.5 Open questions for the user

1. **`AtmosphericHarvesting` → `NitrogenHarvesting` rename** (§8.3.6):
   the rename is breaking. The v0.5 design rule says "no deprecated
   aliases." The user is expected to make this rename and accept the
   break. If the user disagrees, the rename can be deferred.
2. **`MiningEfficiency` vs per-resource modifiers** (§8.3.8-§8.3.10):
   v2 preserves `MiningEfficiency` / `DeepMiningEfficiency` /
   `BulkMiningEfficiency` as generic fallbacks alongside the new
   per-resource effects. This is consistent with the v0.5 design
   rule (no breaking changes to existing modifier types), but it
   means `Mine` now has 7 effects. The user may want to drop the
   generic `MiningEfficiency` once the per-resource effects are
   stable.
3. **Antimatter unit convention** (§6.1, §8.2.3): the game tracks
   antimatter in grams, not Mt. This is a major Rust change that
   affects every resource-balance display. The RON entry stores
   `100.0` (interpreted in the *current* game unit until the Rust
   change lands). Defer to K2 design review.
4. **ExoticMatter physics** (§6.2): no real-world anchor. The
   `ExoticMatterSynthesizer` is a placeholder. Defer to K2 review.
5. **`SemiconductorFab` GRA-22c audit** (§8.3.18): adding 4 new
   `maintenance_resources` rows (Au/Ag/Pt/Ar) raises the building's
   maintenance list from 4 to 8 distinct resources, exceeding the
   existing `MAINTENANCE_AUDIT_MAX = 6` (`src/colony/data.rs:267`).
   Two paths: (a) loosen the audit to `MAX = 10` (recommended;
   electronics *is* resource-intensive in real life), or (b) split
   the consumer across `Refinery` + `SemiconductorFab` (uglier,
   still exceeds the budget). The patch recommends (a).
6. **Body restriction representation** (§8.4): v2 uses the explicit
   list `[Moon, GasGiant, Asteroid]` for `He3Mine`. The user may
   prefer a more general `allowed_deposits: Vec<ResourceType>` field
   on `BuildingDefinition` (e.g. `He3Mine.allowed_deposits: [Helium3]`
   — the building is then available on any body whose
   `PlanetResources` has a He-3 deposit). The current schema is the
   simpler "list body kinds" approach; the deposit-based approach is
   more general and would also cover future bodies (e.g. Mercury, if
   it gets added as a body kind). Defer to a future patch unless
   the user has a preference now.
7. **Power plant per-build scale-down aggressiveness** (§7.2): v2
   preserves v1's conservative ×0.67-0.83× downscale. Some plants
   could be scaled more aggressively (e.g. 10× DOWN to land at 30
   plants per body). The user can refine.
8. **`DTFusionReactor` T maintenance 0.0005** (§8.3.13): the patch
   keeps this value; the new ChemicalPlant T-breeding is 0.001 Mt/yr
   (2× the consumer). One ChemicalPlant feeds 2 D-T reactors.
   Reasonable but flag for review.
9. **`Farm` available_atmospheres** (§4.1, current `[None]` on
   `UndergroundHabitat`-like, but `Farm` is currently unfiltered):
   the patch doesn't change this. If the user wants the `AgriDome`
   to be the only off-world food source, keep as-is. If the user
   wants `Farm` to be re-enabled on off-world with habitat
   support, that's a separate change.
10. **Deuterium `ChemicalPlant` fold** (§5.2): the patch adds
    `DeuteriumProduction` to `ChemicalPlant`. This means a body
    without a ChemicalPlant cannot produce D. The user may want a
    separate `DeuteriumHarvester` if D production on bodies with no
    chemical industry is desired. v2 keeps the fold; user can
    refine.
11. **Earth starting building count** (§9.6 — see also canary 1
    apply): the existing 820 Farms / 60 Greenhouses / 20
    Aquaculture / 2,000 Mines / etc. are calibrated for the OLD
    per-build values (9,000 Mt/yr per Farm, 1,800 Mt/yr per Mine,
    etc.). The patch drops per-build by 25× across the board.
    Applying the patch without also updating the starting count
    would massively over-produce. The recommended starting count
    is in `src/plugins/solar_system.rs:1063-1112` — see canary-1
    apply for the proposed values.
12. **Precious-metal building placement on bodies** (§5.15-§5.17):
    the 3 precious-metal mines (`GoldMine`, `SilverMine`,
    `PlatinumMine`) are unconstrained (`allowed_body_types: []`)
    — they can be built on any body with crustal deposits. In
    reality, gold / silver / platinum deposits are concentrated in
    specific terranes (craton belts, layered intrusions) and not
    uniformly distributed across all bodies. The patch's
    unconstrained approach is the simplest calibration; the user
    may want to add a `allowed_deposits` field (see Q6) so the
    mines only build on bodies whose `PlanetResources` has the
    relevant mineral deposit. Defer to a future patch.

### 9.6 Files NOT modified by this proposal

Per the brief, this is a **proposal doc only**. The following files
are unchanged:

* `src/colony/systems.rs` — no change. The existing
  `process_construction_actions` (line 414-417) attaches
  `MinimumStockpile` defaults that are correct (Food 500, Water
  100, O₂ 200, Water 100). The new resources (H₂, CH₄, NH₃,
  Polymers, Iron, etc.) are NOT added to the default
  `MinimumStockpile` because they have a per-build production that
  exceeds per-capita demand — the player doesn't need a stockpile
  for them.
* `src/economy/components.rs` — no change. `LocalStockpile` and
  `MinimumStockpile` are already designed to handle the new
  resources.
* `assets/data/ship_hulls.ron` — the audit's finding #3 noted the
  Mt-vs-tonnes unit mismatch. This is a separate fix and is out of
  scope for this proposal. The patch leaves ship-hull costs in
  tonnes; the per-building production values in Mt do not interact
  with ship-hull costs in the current code.
* `src/economy/logistics.rs` — no change to
  `DEFAULT_LIFE_SUPPORT_OXYGEN_MT` (200) and
  `DEFAULT_LIFE_SUPPORT_WATER_MT` (100) — these are stockpile
  floors, not per-capita rates.
* Civilization satisfaction state machine — out of scope; lives
  in `CIVILIZATION_SATISFACTION_MODEL.md` and is the next
  deliverable to implement (per the satisfaction model §9.3).

### 9.7 Self-check: what this doc does NOT do

* Does **not** re-litigate the civilization model (locked in §9.1 of
  CIVILIZATION_SATISFACTION_MODEL.md).
* Does **not** produce `BALANCE_SCALING_STRATEGY.md` (separate
  deliverable, not produced here).
* Does **not** implement any RON / Rust changes (proposal only).
* Does **not** propose buildings the player can't build at the
  given tier:
  * No fusion, antimatter, exotics, or off-world He-3 mining at game
    start in 2026. All four are tech-gated.
  * The mid-game He-3 chain is locked behind `lunar_colony` tech
    + body-type restriction to `[Moon, GasGiant, Asteroid]`.
  * The late-game K2 exotics are locked behind `kardashev_k2` tech.
* Does **not** add new `ResourceType` entries (Fluorine is used
  in place of NaOH/Cl₂).
* Does **not** preserve deprecated aliases (the
  `AtmosphericHarvesting` → `NitrogenHarvesting` rename is breaking).
* Does **not** invent real-world numbers without a source. The
  late-game §6 marks all values as "approximate" pending K2 review.

### 9.8 Handoff to the user

To apply the patch, the user:

1. **Edits `src/colony/components.rs`** per §8.1 (1 function, 2
   comments; ~10 lines).
2. **Edits `src/colony/data.rs`** per §8.4 (adds `allowed_body_types`
   field + extends `building_is_available_on` predicate; ~30 lines).
3. **Edits `src/colony/types.rs`** per §8.5 (adds 9 `BuildingType`
   enum variants; ~9 lines).
4. **Appends 9 NEW building entries** to `assets/data/buildings.ron`
   per §8.2 (each ~25 lines; total ~225 lines).
5. **Modifies 18 EXISTING building entries** in
   `assets/data/buildings.ron` per §8.3 (each is 1-7 line diff;
   total ~55 lines of changes — the 18th is the `SemiconductorFab`
   maintenance update with 4 new rows).
6. **Appends 3 NEW technology entries** to
   `assets/data/technologies.ron` per §8.6 (each ~15 lines; total
   ~45 lines).
7. **(Optional) Adds 9 building icons** to `assets/icons/...`
   (artwork, out of scope).

Total estimated effort: **~325 lines of RON + ~49 lines of Rust**.

The patch is **canary-first per the user's UI workflow preferences**
(canary-first migrations, sequential rollout, parallel old/new,
graduate per panel). The recommended canary sequence is in §8.8.

---

*End of balance-patches lean v2. The doc is a revision of the v1
deliverable from the balance-expert agent. v1 had 23 new buildings
and was over-engineered; v2 has 9 new buildings and folds most fixes
into existing `effects` fields (notably `AtmosphericProcessor`
absorbs the `ArgonProduction` modifier; `SemiconductorFab` absorbs
the Au/Ag/Pt/Ar consumer). The mid-game He-3 chain is tech-gated
AND body-restricted to `[Moon, GasGiant, Asteroid]` (the three body
classes with He-3 deposits — regolith-implanted by solar wind on
moons and asteroids, primordial in gas-giant atmospheres). The 3
new technologies (lunar_colony, fusion_power, kardashev_k2) are
spec'd with full prereq chains. The schema addition
(`allowed_body_types: Vec<BodyType>` on `BuildingDefinition`) is
flagged in §8.4. The civilization-satisfaction model
(`CIVILIZATION_SATISFACTION_MODEL.md`) consumes the per-resource
numbers proposed here; the satisfaction model is implemented in a
follow-up v0.5.x pass.*

---

## 10. v0.5.2 — Mining refactor: per-resource dedicated mines + AutoMines + no share-fold

> **Patch v0.5.2 (big-bang rollout):** the user's response to v0.5.1 was that the share-fold / `MiningEfficiency` / `DeepMiningEfficiency` / `BulkMiningEfficiency` modifier approach was opaque (concentration-weighted distribution across every eligible deposit) and that precious-metal production was over-tuning the player bar. The replacement design pattern is:
>
> 1. **Per-resource dedicated base mine** for every crustal/liquid-minable resource (22 buildings: 9 construction + 3 precious + 6 strategic + 2 fissile + hydrocarbons + heavy water + He-3).
> 2. **Per-resource AutoMine** for orbital/asteroid mining (24 buildings, one per resource plus He-3 and WaterProcessor, body-restricted to `[Asteroid, Moon, GasGiant]`).
> 3. **No `MiningEfficiency` / `DeepMiningEfficiency` / `BulkMiningEfficiency` modifiers; no share-fold.** Each mine reads `count × base_yield × deposit.accessibility × yield_mult`.
> 4. **Same base building for mining** — all 22 base mines share `line: Some("Mine")` so a future tier-1+ upgrade building (e.g. `IndustrialMine`) can use the new `replaces_in_line: Option<String>` schema field to upgrade the whole line at once, without needing `IndustrialIronMine` / `IndustrialCopperMine` / … per-resource variants.
> 5. **Earth starting counts: 25 of each base mine** (manageable-count band; 25 × base_yield × 0.6 Earth accessibility ≈ USGS 2024 / 2026 world demand).
> 6. **Legacy generic mines removed:** `Mine` / `Refinery` / `DeepDrill` / `LaserDrill` / `StripMine` / `HydrocarbonExtractor` / `RecyclingCenter` are gone. `HydrocarbonExtractor`'s functionality is captured by `MethaneExtractor`.
> 7. **Body-type schema addition:** `BuildingDefinition::allowed_body_types: Vec<BodyType>` (default empty = any body). The `building_is_available_on` predicate is extended to filter on this. He3Mine uses `[Moon, GasGiant, Asteroid]` (canary 3); all AutoMines use `[Asteroid, Moon, GasGiant]`.
> 8. **Tier upgrade support:** `BuildingDefinition::replaces_in_line: Option<String>` (default None). A tier-1+ building can replace ANY building in the named line — the construction system decrements the colony's count of the lowest-tier building in that line when the new building is added. Most buildings leave this as None.

### 10.1 Per-resource base mine — calibration

| Resource | Per-build (Mt/yr) | Earth demand (Mt/yr) | Earth count | Yield × accessibility | RON modifier |
|---|---:|---:|---:|---|---|
| Iron | 120 | 1,800 (USGS 2024) | 25 | 25 × 120 × 0.6 = 1,800 (100%) | `IronProduction: 120.0` |
| Copper | 1.5 | 22 (USGS 2024) | 25 | 25 × 1.5 × 0.6 = 22.5 (100%) | `CopperProduction: 1.5` |
| Aluminum | 5 | 70 (USGS 2024) | 25 | 25 × 5 × 0.6 = 75 (100%) | `AluminumProduction: 5.0` |
| Silicates | 700 | 10,000 (rough est.) | 25 | 25 × 700 × 0.6 = 10,500 (100%) | `SilicatesProduction: 700.0` |
| Nickel | 0.2 | 3 (USGS 2024) | 25 | 25 × 0.2 × 0.6 = 3.0 (100%) | `NickelProduction: 0.2` |
| Titanium | 0.02 | 0.3 (USGS 2024) | 25 | 25 × 0.02 × 0.6 = 0.3 (100%) | `TitaniumProduction: 0.02` |
| Tungsten | 0.005 | 0.08 (USGS 2024) | 25 | 25 × 0.005 × 0.6 = 0.075 (94%) | `TungstenProduction: 0.005` |
| Carbon | 350 | 5,000 (coal est.) | 25 | 25 × 350 × 0.6 = 5,250 (100%) | `CarbonProduction: 350.0` |
| Chromium | 2 | 30 (USGS 2024) | 25 | 25 × 2 × 0.6 = 30 (100%) | `ChromiumProduction: 2.0` |
| Magnesium | 0.07 | 1 (USGS 2024) | 25 | 25 × 0.07 × 0.6 = 1.05 (100%) | `MagnesiumProduction: 0.07` |
| Gold | 0.0001 | 0.0036 (USGS 2026) | 25 | 25 × 0.0001 × 0.6 = 0.0015 (~80%) | `GoldProduction: 0.0001` (v0.5.1) |
| Silver | 0.001 | 0.025 (USGS 2026) | 25 | 25 × 0.001 × 0.6 = 0.015 (60%) | `SilverProduction: 0.001` (v0.5.1) |
| Platinum | 0.00001 | 0.0002 (USGS 2026) | 20 | 20 × 0.00001 × 0.6 = 0.00012 (manageable-count exception — see §3.5) | `PlatinumProduction: 0.00001` (v0.5.1) |
| RareEarths | 0.025 | 0.35 (USGS 2024) | 25 | 25 × 0.025 × 0.6 = 0.375 (100%) | `RareEarthsProduction: 0.025` |
| Lithium | 0.012 | 0.18 (USGS 2024) | 25 | 25 × 0.012 × 0.6 = 0.18 (100%) | `LithiumProduction: 0.012` |
| Sulfur | 5 | 70 (USGS 2024) | 25 | 25 × 5 × 0.6 = 75 (100%) | `SulfurProduction: 5.0` |
| Phosphorus | 0.003 | 0.05 (USGS 2024) | 25 | 25 × 0.003 × 0.6 = 0.045 (90%) | `PhosphorusProduction: 0.003` |
| Cobalt | 0.015 | 0.23 (USGS 2024) | 25 | 25 × 0.015 × 0.6 = 0.225 (98%) | `CobaltProduction: 0.015` |
| Fluorine | 0.2 | 3 (USGS 2024) | 25 | 25 × 0.2 × 0.6 = 3.0 (100%) | `FluorineProduction: 0.2` |
| Uranium | 0.003 | 0.05 (USGS 2024) | 25 | 25 × 0.003 × 0.6 = 0.045 (90%) | `UraniumProduction: 0.003` |
| Thorium | 0.0007 | 0.01 (USGS 2024) | 25 | 25 × 0.0007 × 0.6 = 0.0105 (100%) | `ThoriumProduction: 0.0007` |
| Methane (Extractor) | 270 | 4,000 (world nat-gas 2024) | 25 | 25 × 270 × 0.6 = 4,050 (100%) | `MethaneProduction: 270.0` |
| Deuterium (Extractor) | 0.5 | ~0 (2024); ~10 fusion startup | 25 | 25 × 0.5 × 0.6 = 7.5 (1 fusion D-D reactor) | `DeuteriumProduction: 0.5` |
| He-3 (Mine) | 0.5 | 0.5 per D-He3 fusion reactor | varies | 25 × 0.5 × 0.6 = 7.5 | `Helium3Production: 0.5` (canary 3) |

### 10.2 AutoMines — orbital / asteroid mining

Per-resource orbital mining rigs. Calibrated at ~1/10 of the surface base mine yield (orbital extraction is harder than surface, asteroid capture is logistically expensive). All AutoMines:
- **Body-restricted to `[Asteroid, Moon, GasGiant]`** via the new `allowed_body_types` schema field.
- **Require `asteroid_mining` tech** (one new tech for the whole class).
- Have a smaller workforce (~600-1,500 workers per build vs 3,500-8,000 for surface mines) because they are space-grade automated rigs.
- Have a higher build cost (space-grade hardware).
- Per-build yields: AutoIronMine 12 Mt/yr, AutoCopperMine 0.15 Mt/yr, AutoAluminumMine 0.5 Mt/yr, AutoSilicatesMine 70 Mt/yr, AutoNickelMine 0.02 Mt/yr, AutoTitaniumMine 0.002 Mt/yr, AutoTungstenMine 0.0005 Mt/yr, AutoCarbonMine 35 Mt/yr, AutoChromiumMine 0.2 Mt/yr, AutoMagnesiumMine 0.007 Mt/yr, AutoGoldMine 0.00001 Mt/yr, AutoSilverMine 0.0001 Mt/yr, AutoPlatinumMine 0.000001 Mt/yr, AutoRareEarthsMine 0.0025 Mt/yr, AutoLithiumMine 0.0012 Mt/yr, AutoSulfurMine 0.5 Mt/yr, AutoPhosphorusMine 0.0003 Mt/yr, AutoCobaltMine 0.0015 Mt/yr, AutoFluorineMine 0.02 Mt/yr, AutoUraniumMine 0.0003 Mt/yr, AutoThoriumMine 0.00007 Mt/yr, AutoMethaneExtractor 27 Mt/yr (Titan analog), AutoDeuteriumExtractor 0.05 Mt/yr, AutoHe3Mine 0.05 Mt/yr (lunar regolith / asteroid), AutoWaterProcessor 1.6 Mt/yr (carbonaceous chondrite ice).

### 10.3 Why the share-fold is gone

The legacy tier system multiplied a per-build `MiningEfficiency` modifier by every eligible deposit's concentration share:
```
yield_per_resource = base_rate × concentration_share × yield_mult
```
This was **opaque** (the player couldn't predict the per-resource output without knowing all deposit concentrations) and **over-produced precious metals by 100-300× real-world** (concentration share gave Gold ~0.0001×0.0001 = effectively a measurable yield from a single `Mine` building, not the ~3,200 troy oz/yr the USGS 2026 number actually implies).

The v0.5.2 replacement is **direct**: each `XxxMine` modifier maps 1:1 to one `ResourceType`. The `mining.rs` dispatch loop:
```rust
for (bt, &count) in &colony.buildings {
    for modifier in &def.modifiers {
        // Strip `Production` suffix → ResourceType
        if let Some(target) = modifier.modifier_type.strip_suffix("Production")
                                            .and_then(parse_resource_type_static) {
            *direct_production.entry(target).or_insert(0.0) +=
                modifier.value * count as f64 * yield_mult;
        }
    }
}
// Direct deposit, scaled by body accessibility:
for (resource, base_rate) in &direct_production {
    let access = resources.get_deposit(resource).map(|d| d.accessibility).unwrap_or(0.0);
    if access <= 0.0 { continue; }  // body has no accessible deposit
    let amount = base_rate * access * bonus * years_elapsed;
    deposit_with_fallback(..., resource, amount);
}
```

The `XxxProduction` modifier pattern lets future tier-1+ upgrade buildings (e.g. `IndustrialMine` with `tier: 1, line: Some("Mine"), replaces_in_line: Some("Mine")`) upgrade the entire line — see §10.4.

### 10.4 `replaces_in_line` schema field (v0.5.2)

The `BuildingDefinition` schema has a new optional field:
```rust
pub replaces_in_line: Option<String>,  // default None
```

A building with `replaces_in_line: Some("Mine")` and `tier: 1` will replace the colony's count of the lowest-tier building in the `line: "Mine"` line (i.e. any `IronMine` / `CopperMine` / `NickelMine` / etc.) by one when the new building is added. This lets the operator add a single tier-1+ upgrade building per line without needing `IndustrialIronMine` / `IndustrialCopperMine` / etc. variants.

**Wiring is deferred to a follow-up patch.** v0.5.2 lands the schema field + the per-resource base mines, but the construction system does not yet decrement the predecessor count on `replaces_in_line` builds. The data is in place; the runtime is a small follow-up.

### 10.5 Body-type schema addition (canary 3)

The `BuildingDefinition` schema has a new optional field:
```rust
pub allowed_body_types: Vec<BodyType>,  // default empty = any body
```

The `building_is_available_on` predicate is extended:
```rust
pub fn building_is_available_on(
    def: &BuildingDefinition,
    body_breathable: Option<bool>,
    body_type: Option<BodyType>,
) -> bool {
    // Atmosphere gate (v0.5.1 GRA-27)
    if let Some(breathable) = body_breathable { ... }
    // Body type gate (v0.5.2 canary 3)
    if let Some(bt) = body_type {
        if !def.allowed_body_types.is_empty() && !def.allowed_body_types.contains(&bt) {
            return false;
        }
    }
    true
}
```

**He3Mine** uses `allowed_body_types: [Moon, GasGiant, Asteroid]`. All 24 AutoMines use `allowed_body_types: [Asteroid, Moon, GasGiant]`. The construction panel pulls the body's `BodyType` from `CelestialBody` and passes it to the predicate (alongside the existing `breathable` flag).

### 10.6 What was REMOVED in v0.5.2

The following legacy generic mines are removed from `BuildingType`, the RON file, and the modifier dispatch:
- `Mine` (generic) — replaced by 22 per-resource base mines
- `Refinery` — folded into per-resource base mines (steel = Iron + Carbon; aluminium = AluminumMine)
- `DeepDrill` / `LaserDrill` / `StripMine` — replaced by tier-1+ `IndustrialMine` upgrade building (deferred)
- `HydrocarbonExtractor` — replaced by `MethaneExtractor`
- `RecyclingCenter` — folded into per-resource base mines (Au/Ag/Pt recovered from share-fold extraction; v0.5.2 drops share-fold entirely)
- The 3 modifier handlers: `MiningEfficiency`, `DeepMiningEfficiency`, `BulkMiningEfficiency`
- The 3 share-fold blocks in `mining.rs` (proven_crustal, deep_deposits, planetary_bulk tiers)

### 10.7 What's NEW in v0.5.2 (code + RON)

- **22 base mines** in `BuildingType`: IronMine, AluminumMine, TitaniumMine, SilicatesMine, NickelMine, TungstenMine, CarbonMine, ChromiumMine, MagnesiumMine, CopperMine, RareEarthsMine, LithiumMine, SulfurMine, PhosphorusMine, CobaltMine, FluorineMine, UraniumMine, ThoriumMine, MethaneExtractor, DeuteriumExtractor, He3Mine. Plus GoldMine/SilverMine/PlatinumMine/WaterProcessor from v0.5.1.
- **24 AutoMines**: one per base mine resource + He-3 + Water (24 total).
- **2 schema fields**: `allowed_body_types: Vec<BodyType>` and `replaces_in_line: Option<String>`.
- **1 modifier pattern**: `XxxProduction` (one per resource; v0.5.1 had `GoldProduction` / `SilverProduction` / `PlatinumProduction` / `WaterProduction` / `ArgonProduction`; v0.5.2 unifies all 36+ resources under the `XxxProduction` suffix convention).
- **Earth starting counts**: 25 of each base mine. No AutoMines (Earth = no orbital mining — the player must colonise an asteroid / gas-giant moon to start using them).

### 10.8 Implementation diffs (high-level)

- `src/colony/types.rs` — `BuildingType` enum: +46 new variants (22 base + 24 Auto), −7 legacy variants. All `match` statements updated. Tests updated to assert 95 total building types (was 56).
- `src/colony/data.rs` — `BuildingDefinition` schema: +`allowed_body_types`, +`replaces_in_line`. `parse_building_type` updated for all 46 new variants. `building_is_available_on` predicate extended.
- `src/economy/mining.rs` — `extract_resources` + `update_resource_rates`: replaced 3 share-fold blocks + 3 modifier handlers with a single `XxxProduction` dispatch + direct-deposit loop scaled by `deposit.accessibility`. Added `parse_resource_type_static` helper.
- `src/colony/components.rs` — `Colony::logistics_demand` now counts all buildings in the `Industry` category (was a hand-curated list of 6 legacy mines).
- `src/plugins/solar_system.rs` — Earth starting counts: 25 of each base mine (replaces 2,000 `Mine` + 500 `Refinery` + 300 `HydrocarbonExtractor` + 300 `RecyclingCenter`).
- `src/ui/construction_panel.rs` — passes `body_type` through to `building_is_available_on` predicate (alongside existing `body_breathable`).
- `assets/data/buildings.ron` — 7 legacy entries removed, 48 new entries added (5 v0.5.1/3 + 18 base + 25 Auto), with full `allowed_body_types` for the body-restricted ones. RON file grew from 1,582 lines to 2,990 lines.

### 10.9 Open questions (deferred)

- **Tier-1+ upgrade buildings** (`IndustrialMine`, `DeepMine`, `RefinedMine`): the `replaces_in_line` schema is in place; the runtime decrement-on-build wiring is a follow-up. v0.5.2 ships base mines only.
- **AutoMine iconography**: 40+ new icons pending generation (icon-artist batch 3 in flight). The construction panel falls back to the Unicode emoji in `BuildingType::icon()` for any missing PNG.
- **SemiconductorFab `Gold` / `Silver` / `Platinum` / `Argon` maintenance** (was a v0.5.1 plan): not applied in v0.5.2; the maintenance audit (`MAINTENANCE_AUDIT_MAX: 6`) would have flagged it. Loose the audit max to 10 to apply. Deferred to a separate patch.
- **DeuteriumExtractor body restriction**: the new extractor should arguably be restricted to bodies with surface liquid water (Earth, Europa, Enceladus, Titan). Not applied in v0.5.2; the construction panel will show it on every body.
- **AtmosphericProcessor share-fold**: still uses the concentration-weighted share-fold across atmospheric deposits (N₂ / O₂ / Ar) because atmospheric gases are co-extracted from a single cryogenic-air-separation stream. This is a deliberate exception to the v0.5.2 "no share-fold" rule; see §10.3 for the rationale.

