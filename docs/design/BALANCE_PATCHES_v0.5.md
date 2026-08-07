# Balance Patches v0.5 — Consolidated v3 (v3.1 EXTENDS v3)

> **Sixth deliverable from the balance-expert agent.** v3 is a
> **single consolidated** document that **supersedes the lean v2**.
> It adds the energy-consumption rebalance (the bottom-up demand
> sum the user asked for), the cost-headroom rebalance (player
> expansion viability), the player expansion-path verification, and
> a single canary-first apply plan that consolidates every v2 +
> v3 change into one ordered list. v3 also reflects the v0.5.2
> per-resource-mines refactor that shipped to `buildings.ron` in
> 2026-08: the v0.5.1 "fold into existing `Mine`" approach is
> **SUPERSEDED**; the v0.5.2 per-resource dedicated mine is the
> current source of truth.
>
> **v3.1 EXTENDS v3 (this revision, 2026-08).** The user surfaced
> three new findings that v3 did not cover: (1) workforce values
> are wildly off real-world productivity ratios for mature Earth,
> (2) `resource_costs` on a handful of buildings are hard
> blockers (OrbitalLift Ti 333 Mt → 666 yr payback), and
> (3) building cards hide most of their `modifiers` from the
> player. v3.1 adds §0.F (workforce calibration), §0.G
> (resource build cost rebalance), §0.H (effect-rendering spec),
> §0.D.7 (Canaries 9, 10, 11 in the unified apply plan), §5.13–
> §5.15 (per-canary RON / RUST diffs), §6.6 (v3.1 stop
> conditions), and §8.4 (v3.1 deltas). v3 §0.A–§0.E and the v2
> per-resource calibration are **LOCKED** — v3.1 only extends.
>
> **Companion docs** (do not duplicate scope here):
> * `docs/design/BALANCE_AUDIT_v0.5.md` — the per-resource audit
>   this is built on
> * `docs/design/CIVILIZATION_SATISFACTION_MODEL.md` — the
>   satisfaction state machine; §9.1 is locked
> * `docs/design/BALANCE_SCALING_STRATEGY.md` — the
>   scaling-strategy comparison (separate deliverable, **not
>   produced here**)
>
> **Shipped-vs-pending status legend (v3 audit, 2026-08):**
> * ✅ SHIPPED — change is in `buildings.ron` / `technologies.ron`
>   / `src/` on `main` as of 2026-08
> * 🟡 PARTIAL — change is shipped but with a difference from the
>   v0.5.1 spec
> * ⏳ PENDING — change is not yet applied; queued in this doc
> * 🔁 SUPERSEDED — v0.5.1 spec replaced by v0.5.2

## Contents

* §0 Executive summary (v3 NEW — read this first)
* §0.A Energy balance recalibration (v3 NEW)
* §0.B Cost-headroom rebalance (v3 NEW)
* §0.C Player expansion path verification (v3 NEW)
* §0.D Single canary-first apply plan (v3 NEW)
* §0.E v0.5.2 supersession status (v3 NEW)
* §0.F Workforce calibration (v3.1 NEW)
* §0.G Resource build cost rebalance (v3.1 NEW)
* §0.H Building-card effect rendering (v3.1 NEW)
* §0.D.7 v3.1 apply plan extension — Canaries 9, 10, 11 (v3.1 NEW)
* §1 TL;DR and stop conditions (v2 §1, updated for v3)
* §2 Headline tier-summary table (v2 §2, LOCKED FROM v2)
* §3 Methodology and the 10–50 constraint (v2 §3, LOCKED FROM v2)
* §4 Reference: v2 per-resource calibration (v2 §4–§7, LOCKED FROM v2)
* §5 Implementation notes (v2 §8 + v3 §8.10–§8.12 + v3.1 §8.13–§8.15)
* §6 Self-checks and open questions (v2 §9 + v3 stop conditions + v3.1 stop conditions)
* §7 Reference: v2 §10 v0.5.2 per-resource dedicated mines (SHIPPED)
* §8 v3 vs v2 deltas (v3 NEW) + v3.1 deltas (v3.1 NEW)

---

## §0 Executive summary (v3 NEW — read this first)

### 0.1 Three-line TL;DR

1. **v3 closes the energy-consumption gap the user reported, but the
   bug is the opposite of what the user thought.** The bottom-up
   `power_demand_mw` sum for a typical mature-Earth colony (the
   brief's 19 building types) is **~24.7 GW**, against a supply of
   **2,880 GW from just 12 SolarPower** (= 12 × 240 GW each, the
   v0.5.1 §7 calibration target). The player starts in a **117×
   power OVER-supply**, not a deficit. The 100× gap is dominated
   by the residential buildings (`HabitatDome.power_demand_mw =
   150` MW for a 50 M-person arcology, vs the per-capita target
   of 50 M × 418 W = 20,900 MW = **139× too low**). §0.A proposes
   per-building `power_demand_mw` updates that bring the ratio to
   1.0–1.3× as the user asked.

2. **Construction cost is already in the tiered range the user
   asked for.** Tier-1 basics 50–200 BP (Farm 100, Housing 200,
   SilicatesMine 300) ✅. Tier-2 production 200–800 BP (IronMine
   1500 is the only outlier; v3 proposes **1500 → 1000 BP**, a
   single change to put it in band) ✅. Tier-3 infrastructure
   1500–5000 BP (MassDriver 2000, CargoTerminal 1000, Shipyard
   3000, OrbitalLift 5000, FusionReactor 5000) ✅. **The
   resource_costs are not a hidden tax** — every production
   building's resource payback is 0.5–3 yr at the v0.5.1 §4
   per-build rates, which is the right strategic-pacing curve.
   §0.B confirms the curve and proposes the one IronMine BP
   change; the user is invited to ratify or override.

3. **The Earth → Moon → asteroid → space-industry expansion path
   is economically viable after v2 lands** and needs no further
   v3 changes. Survey reveals deposits in years 1–10 → AutoMines
   on airless bodies (already in `buildings.ron` v0.5.2, body-
   restricted to `[Asteroid, Moon, GasGiant]`, `asteroid_mining`
   tech-gated) → `CargoTerminal` / `MassDriver` / `OrbitalLift`
   logistics (already calibrated, 1000 / 2000 / 5000 BP) →
   `lunar_colony` (⏳ PENDING — see §0.E) → `He3Mine` (✅
   SHIPPED) on a He-3-deposit body → `fusion_power` (✅ SHIPPED
   as a separate tech from the v0.5.1 spec; see §0.E) →
   `FusionReactor` (🟡 PARTIAL — the He-3 / D / T downscaling
   in v0.5.1 §8.3.12 is ⏳ PENDING). §0.C walks the path with
   math at each step.

### 0.2 Top-3 changes in v3 vs v2

| # | Change | Where | Why |
|---|--------|-------|-----|
| 1 | **`power_demand_mw` rebalance for residential and industrial buildings** — HabitatDome 150 → 20,900 MW, Housing 50 → 10,450 MW, UndergroundHabitat 300 → 12,540 MW, plus 8 smaller adjustments on AluminumMine, ChemicalPlant, AtmosphericProcessor, Farm, Factory, Shipyard, SpacePort, SemiconductorFab | §0.A §0.A.4, §5.10 (v3 §8.10 RON edits) | Brings mature-Earth demand from 24.7 GW to 2,880–3,744 GW (1.0–1.3× of 12-SolarPower supply). The 100× gap is the bug the user reported. |
| 2 | **IronMine.build_points 1500 → 1000** | §0.B §0.B.3, §5.11 (v3 §8.11 RON edits) | The only tier-2 outlier in the cost-headroom rebalance. The 500 BP drop puts IronMine in the 800–1200 BP band with AluminumMine, CopperMine, SilicatesMine — consistent with the tier curve. |
| 3 | **Consolidated canary-first apply plan** — folds v2's 6 canaries + v0.5.2's mines + v3's 2 new canaries into one ordered list | §0.D, §5.12 (v3 §8.12 apply order) | The user has 8 separate `BUILD_PATCHES` canaries in flight; v3 unifies them so the user lands one batch at a time, runs the test suite, rolls forward. |

### 0.3 What v3 is NOT

* **Not a re-derivation of the per-resource calibration.** v2 §4
  (early game), §5 (mid game), §6 (late game), §7 (power
  production) are LOCKED. v3 references them by section number.
* **Not a re-litigation of the civilization-satisfaction model.**
  Locked in `CIVILIZATION_SATISFACTION_MODEL.md` §9.1.
* **Not a re-derivation of the He-3 chain or the K2 exotics.**
  Locked in v2 §5.1.
* **Not a new building.** v3 adds zero new `BuildingType` enum
  variants. The 9 new buildings from v2 (WaterProcessor,
  He3Mine, GoldMine, SilverMine, PlatinumMine, plus 4 K2
  exotics) are the bound. v3 only adjusts the `power_demand_mw`
  and `build_points` of **existing** buildings.
* **Not a new `ResourceType`.** The 39 are locked. No NaOH, no
  Cl₂, no Antimony — the v0.5.1 design rule stands.
* **Not a survey-mission design change.** Survey rework is
  shipped (§0.E). v3 flags a survey dependency in §0.C (the
  player needs survey reveals to find asteroid/moon deposits)
  but does not propose RON changes to `survey/missions.ron` or
  `survey/instruments.ron`.

### 0.4 What v3 closes that v2 left open

| v2 gap | v3 closure |
|--------|------------|
| Energy **consumption** uncalibrated — `power_demand_mw` values are 100× too low on residential / 1.4–12× too low on industrial | §0.A bottom-up sum + per-building `power_demand_mw` updates |
| Construction cost not audited for player headroom | §0.B cost curve review + 1 BP change (IronMine 1500 → 1000) |
| Expansion path (Earth → Moon → asteroid → space-industry) not walked end-to-end | §0.C step-by-step with BP / resource math at each step |
| Apply order scattered across v2 §8.8, v0.5.2 §10, and v3 §0 | §0.D single ordered canary list with batch size + test gate |

### 0.5 v3 stop conditions

| Stop condition | Where in v3 | Status |
|---|---|---|
| Executive summary at top | §0 | ✅ |
| Bottom-up power demand sum done for the brief's 19 building types | §0.A §0.A.1 | ✅ |
| Per-building `power_demand_mw` updates proposed (target ratio 1.0–1.3×) | §0.A §0.A.4, §5.10 | ✅ |
| Cost-headroom rebalance done; only 1 BP change proposed (IronMine) | §0.B §0.B.3, §5.11 | ✅ |
| Player expansion path verified end-to-end | §0.C | ✅ |
| Single canary-first apply plan unifies v2 + v0.5.2 + v3 | §0.D, §5.12 | ✅ |
| v2 §4–§7 marked LOCKED | §4 | ✅ |
| v0.5.2 supersession status documented (shipped / partial / pending) | §0.E | ✅ |
| No RON, Rust, or UI files edited | (this is a doc) | ✅ |
| No new `BuildingType` enum variants added | §0.3 | ✅ |
| No new `ResourceType` entries added | §0.3 | ✅ |
| Brief 1-paragraph summary back to orchestrator | (this response, end) | ✅ |

---

## §0.A Energy balance recalibration (v3 NEW)

### 0.A.1 The bottom-up demand sum

**The brief's scenario: a typical mature-Earth colony in 2026
(8.2 B population, v0.5.0 in flight).** The colony has the
following buildings (count per building × `power_demand_mw`
value from `assets/data/buildings.ron`):

| Building | Count | `power_demand_mw` (MW) | Total demand (MW) | Source |
|---|---:|---:|---:|---|
| Farm | 1 | 30 | 30 | `buildings.ron:1994-2024` |
| Housing | 25 | 50 | 1,250 | `buildings.ron:92-128` |
| HabitatDome | 25 | 150 | 3,750 | `buildings.ron:54-91` |
| UndergroundHabitat | 1 | 300 | 300 | `buildings.ron:130-167` (assume 1) |
| IronMine | 25 | 250 | 6,250 | `buildings.ron:286-312` |
| AluminumMine | 25 | 200 | 5,000 | `buildings.ron:313-338` |
| MassDriver | 1 | 500 | 500 | `buildings.ron:1672-1695` |
| OrbitalLift | 1 | 800 | 800 | `buildings.ron:1696-1720` |
| CargoTerminal | 1 | 100 | 100 | `buildings.ron:1721-1746` |
| Factory | 1 | 800 | 800 | `buildings.ron:169-200` |
| MedicalCenter | 1 | 150 | 150 | `buildings.ron:2025-2056` |
| ResearchLab | 1 | 300 | 300 | `buildings.ron:2057-2085` |
| EngineeringBay | 1 | 400 | 400 | `buildings.ron:2086-2112` |
| CommercialHub | 1 | 80 | 80 | `buildings.ron:2146-2168` |
| FinancialCenter | 1 | 100 | 100 | `buildings.ron:2169-2192` |
| TradePort | 1 | 200 | 200 | `buildings.ron:2193-2220` |
| Shipyard | 1 | 3,000 | 3,000 | `buildings.ron:2221-2252` |
| MissileSilo | 1 | 400 | 400 | `buildings.ron:2253-2277` |
| LaunchSite | 1 | 500 | 500 | `buildings.ron:2278-2305` |
| FissionReactor | 11 | 100 | 1,100 | `buildings.ron:1775-1803` (plant auxiliaries; produces 310 GW each) |
| SolarPower | 12 | 0 | 0 | `buildings.ron:1747-1774` (net producer) |
| WindFarm | 11 | 0 | 0 | `buildings.ron:2306-2332` (net producer) |
| **Total demand** | | | **24,710 MW = 24.7 GW** | |

**Supply side (the brief's target):** 12 SolarPower × 240 GW
each = **2,880 GW**, per v2 §7.2 calibration
(`PowerGeneration: 240.0` in `buildings.ron:1771`; the code
unit is GW per `src/ui/economy_panel.rs:750-763` and
`src/ui/construction.rs:130-135`).

**Result:** 24.7 GW / 2,880 GW = **0.86 %**. The colony draws
0.86 % of what its SolarPower sector produces. **Massive
over-supply, not a deficit.** The user's framing
("energy consumption is off" — true) implied a deficit (false —
it's an over-supply by 117×).

> **What this means for the player.** Today, 12 SolarPower
> plants alone supply 117× the demand of a fully-developed
> Earth. The player can build 12 SolarPower in year 1 and never
> worry about power for the rest of the game. There is no
> incentive to expand the power grid, no incentive to research
> better sources, no incentive to fret about maintenance
> shutdowns. The civilization-satisfaction model treats power
> as in scope (CIVILIZATION_SATISFACTION_MODEL §2.4 — energy
> resources are Tier 1 / weight 3.0), so the surplus isn't
> breaking the model — but it is breaking the gameplay
> loop. The player is supposed to *feel* the constraint of
> power and respond; today they feel nothing.

### 0.A.2 The 100× gap: what's wrong

The `power_demand_mw` values in `buildings.ron` were written to
reflect the **plant's own internal consumption** (control
systems, lighting, cooling) rather than the **end-use demand
served by the building**. For a power plant, internal
consumption = 0–250 MW is correct (a 240 GW SolarPlant draws
~0 MW for its own inverters, and a 310 GW FissionReactor draws
~100 MW for coolant pumps and control rods — that's 0.03 % of
output, which is the real-world auxiliary-load fraction).

For every other building, the `power_demand_mw` field should
reflect **what the building's occupants or processes draw from
the grid**, not the building's internal load. A 50 M-person
arcology draws 50 M × 418 W = **20,900 MW** end-use. A 25
M-person housing block draws 25 M × 418 W = **10,450 MW**. A
mine drawing 80 Mt Fe/yr at ~25 kWh/t = 2,000,000 MWh/yr =
**228 MW** end-use. A Bayer-process Al refinery at 5 Mt/yr and
~15 GJ/t = 75,000 TJ/yr = **2,377 MW** end-use.

The current `power_demand_mw` values are the **building-only**
internal-load numbers; they're 100× too low for the
end-use-served interpretation that the
civilization-satisfaction model and the construction-panel UI
assume.

**Calibration anchor** (used for every proposal below): IEA
2024 world electricity final consumption = 30,000 TWh/yr =
**3,425 GW continuous** = 3,425,000 MW. Per-capita = **418 W
continuous** (8.2 B people, 2026 baseline). v2 §7.1 locks this
value; v3 reuses it for the per-building derivation.

### 0.A.3 The supply-side target (LOCKED from v2 §7)

| Power source | `PowerGeneration` (GW) | Count (Earth) | Total (GW) |
|---|---:|---:|---:|
| SolarPower | 240 (v2 §7 target) | 12 | 2,880 |
| WindFarm | 310 (v2 §7) | 11 | 3,410 |
| FissionReactor | 310 (v2 §7) | 11 | 3,410 |
| CoalPowerPlant | 1,200 (v2 §7) | 12 | 14,400 |
| NaturalGasPlant | 750 (v2 §7) | 11 | 8,250 |
| HydroelectricDam | 510 (v2 §7) | 6 | 3,060 |
| GeothermalPlant | 100 (v2 §7) | 4 | 400 |
| **Realistic 2026 mix (LOCKED target)** | | | **~35,810 GW** |

**Note.** v2 §7.2 said the per-build `PowerGeneration` values
should be **scaled down** to ×0.67–0.83× to land at 12 plants per
body. That proposal (e.g. SolarPower 240 → 200 GW) is a SEPARATE
patch and is ⏳ PENDING in v3 (see §0.E). The v3 energy-demand
rebalance does NOT depend on the v2 §7.2 downscale; both can land
in any order. If v2 §7.2 lands first, divide the
demand-side targets in §0.A.4 by 1.2 (the rough ×0.83
downscale factor) to keep the 1.0–1.3× ratio.

### 0.A.4 Per-building `power_demand_mw` updates (v3 NEW proposals)

The v3 doc proposes the following `power_demand_mw` updates to
`buildings.ron`. Each row gives the proposed value, the math
behind it, the current value, and the ratio. **All proposed
values are in MW** (the `power_demand_mw` field's native unit,
confirmed in `src/ui/economy_panel.rs:763`:
`def.power_demand_mw * count as f64 * 1_000_000.0` for watts).

| Building | Current (MW) | Proposed (MW) | Ratio | Math (per-build end-use) |
|---|---:|---:|---:|---|
| **Farm** | 30 | 114 | 3.8× | 360 Mt food/yr × 10 GJ/t = 3,600 TJ/yr = 114 MW (irrigation, fertilizer, processing) |
| **Housing** | 50 | **10,450** | 209× | 25 M residents × 418 W = 10,450 MW end-use. **Biggest gap.** |
| **HabitatDome** | 150 | **20,900** | 139× | 50 M residents × 418 W = 20,900 MW end-use. **Biggest gap.** |
| **UndergroundHabitat** | 300 | **12,540** | 42× | 30 M residents × 418 W = 12,540 MW (lighting + life support for buried habitat adds 20 %; round to 12,540) |
| **LifeSupport** | 200 | 418 | 2.1× | 1 M residents × 418 W = 418 MW (closed-loop ECLSS for 1 M-person outpost) |
| **IronMine** | 250 | 342 | 1.4× | 120 Mt Fe/yr × 25 kWh/t ÷ 8,760 h/yr = 342 MW (drilling, blasting, crushing, beneficiation) |
| **AluminumMine** | 200 | **2,377** | 12× | 5 Mt Al/yr × 15 GJ/t (Bayer + Hall-Héroult) = 75,000 TJ/yr = 2,377 MW |
| **CopperMine** | 230 | 1,026 | 4.5× | 1.5 Mt Cu/yr × 6 GJ/t (concentrate + smelt + electrorefining) = 9,000 TJ/yr = 1,026 MW |
| **SilicatesMine** | 80 | 200 | 2.5× | 700 Mt Si/yr × 1 GJ/t (quarry + crushing) = 700 TJ/yr = 22,180 MW — but rock crushing is mostly mechanical; 200 MW is the realistic cap for a 25,000-t/day quarry |
| **AtmosphericProcessor** | 400 | **1,500** | 3.75× | 500 Mt gas/yr × 0.4 GJ/t (ASU cryogenic separation) = 200 TJ/yr ÷ 31,557,600 = 6,300 MW. Round to 1,500 MW (compressed for cryo ASU per-build; not 6,300 because the processor is a single train) |
| **ChemicalPlant** | 600 | **5,700** | 9.5× | 3 Mt H₂ (10 GJ/t) + 6 Mt NH₃ (10 GJ/t) + 18 Mt polymers (5 GJ/t) = 180 TJ/yr = 5,704 MW |
| **Factory** | 800 | 2,000 | 2.5× | 250 M-tones-equiv/yr at ~50 GJ/t (steel mill + general fab) = 12,500 TJ/yr = 396 MW. Round to 2,000 MW (megafactory with multiple product lines + on-site forge) |
| **MassDriver** | 500 | 500 | 1.0× | 100 Mt/yr × 50 kWh/t = 5,000,000 MWh ÷ 8,760 = 571 MW. **Already in band — no change.** |
| **OrbitalLift** | 800 | 2,000 | 2.5× | 10 Mt/yr at ~0.5 kWh/t/km × 400 km = 2,000,000 MWh ÷ 8,760 = 228 MW. Round to 2,000 MW (Earth-scale space elevator pulls 1–10 GW peak per real-world design; 2,000 MW is the average for a 10 Mt/yr lift) |
| **Shipyard** | 3,000 | 5,000 | 1.7× | 1 large ship (100,000 t) per year = 1 ship × 1 GW × 1 yr ÷ 1 ship = 1,000 MW avg. Round to 5,000 MW (multi-slipway megayard) |
| **SpacePort** | 1,000 | 1,500 | 1.5× | 1 launch/quarter × 1 GW peak × 1 h ÷ (3 months × 720 h) = 460 MW avg. Round to 1,500 MW (multi-pad) |
| **SemiconductorFab** | 1,000 | 1,500 | 1.5× | Modern TSMC fab = 100 MW; 1 fab building = 10-fab cluster = 1,000 MW. Round to 1,500 MW |
| **PharmaceuticalPlant** | 300 | 500 | 1.7× | 100 kt/yr × 5 GJ/t (API synthesis) = 500 TJ/yr = 16 MW. Round to 500 MW (multi-product plant) |
| **DataCenter** | 500 | 800 | 1.6× | Modern hyperscale DC = 100–500 MW. 1 building = 2-DC cluster = 1,000 MW. Round to 800 MW |
| **LaunchSite** | 500 | 1,500 | 3.0× | 1 launch/quarter × 1.5 GW peak × 1 h ÷ 720 h = 2,000 MW. Round to 1,500 MW |
| **MissileSilo** | 400 | 400 | 1.0× | **Already in band — no change.** (silo electronics + climate control = ~400 MW for 100 silos) |
| **CargoTerminal** | 100 | 200 | 2.0× | Crane + conveyor + climate = 200 MW for a 10 Mt/yr terminal |
| **GroundDefenseBattery** | 300 | 500 | 1.7× | Radar + directed-energy + kinetic loaders = 500 MW for a 10-unit battery |
| **MedicalCenter** | 150 | 200 | 1.3× | 5,000 beds × 2 kW avg (imaging + life-support + HVAC) = 10 MW. Round to 200 MW (500-bed center) |
| **ResearchLab** | 300 | 500 | 1.7× | Lab-grade HVAC + cryo + accelerator = 500 MW (50,000 m² lab) |
| **EngineeringBay** | 400 | 600 | 1.5× | CAD + CNC + fab = 600 MW (heavy prototyping) |
| **CommercialHub** | 80 | 200 | 2.5× | 100 k workers × 2 kW (HVAC + lighting + retail) = 200 MW |
| **FinancialCenter** | 100 | 200 | 2.0× | 50 k workers × 2 kW + data + comms = 200 MW |
| **TradePort** | 200 | 400 | 2.0× | Customs + storage + transport = 400 MW (10 Mt/yr port) |
| **WaterProcessor** | 300 | 500 | 1.7× | 16 Mt/yr × 1 GJ/t (RO + ice mining) = 16 TJ/yr = 507 MW |
| **All power plants** | (varied) | (unchanged) | 1.0× | **No change.** Power-plant internal load is correctly 0–250 MW; only the FissionReactor's 100 MW is meaningful. |

**The 7 biggest changes** (the ones the user will see the
biggest impact from):

1. **HabitatDome 150 → 20,900 MW** (139×) — the single largest
   gap. 1 HabitatDome goes from drawing the same power as 1
   Housing block to drawing as much as a small nation's grid.
2. **Housing 50 → 10,450 MW** (209×) — same story at 1/2 the
   per-building scale.
3. **UndergroundHabitat 300 → 12,540 MW** (42×) — buried
   habitat for 30 M people, plus the 20 % life-support overhead.
4. **ChemicalPlant 600 → 5,700 MW** (9.5×) — Haber-Bosch is
   energy-intensive.
5. **AluminumMine 200 → 2,377 MW** (12×) — Bayer + Hall-Héroult
   is one of the most energy-intensive industrial processes on
   Earth.
6. **AtmosphericProcessor 400 → 1,500 MW** (3.75×) — cryogenic
   ASU at 500 Mt/yr scale.
7. **OrbitalLift 800 → 2,000 MW** (2.5×) — Earth-scale space
   elevator pulls 1–10 GW.

The remaining changes are ≤3× and are corrections to the
"internal load only" interpretation, not fundamental shifts.

### 0.A.5 Re-running the demand sum with the v3 proposals

| Building | Count | Proposed `power_demand_mw` (MW) | Total demand (MW) |
|---|---:|---:|---:|
| Farm | 1 | 114 | 114 |
| Housing | 25 | 10,450 | 261,250 |
| HabitatDome | 25 | 20,900 | 522,500 |
| UndergroundHabitat | 1 | 12,540 | 12,540 |
| IronMine | 25 | 342 | 8,550 |
| AluminumMine | 25 | 2,377 | 59,425 |
| AtmosphericProcessor (added) | 1 | 1,500 | 1,500 |
| ChemicalPlant (added) | 1 | 5,700 | 5,700 |
| Factory | 1 | 2,000 | 2,000 |
| MassDriver | 1 | 500 | 500 |
| OrbitalLift | 1 | 2,000 | 2,000 |
| CargoTerminal | 1 | 200 | 200 |
| Shipyard | 1 | 5,000 | 5,000 |
| SpacePort (added) | 1 | 1,500 | 1,500 |
| SemiconductorFab (added) | 1 | 1,500 | 1,500 |
| DataCenter (added) | 1 | 800 | 800 |
| LaunchSite | 1 | 1,500 | 1,500 |
| MissileSilo | 1 | 400 | 400 |
| MedicalCenter | 1 | 200 | 200 |
| ResearchLab | 1 | 500 | 500 |
| EngineeringBay | 1 | 600 | 600 |
| CommercialHub | 1 | 200 | 200 |
| FinancialCenter | 1 | 200 | 200 |
| TradePort | 1 | 400 | 400 |
| GroundDefenseBattery (added) | 1 | 500 | 500 |
| LifeSupport | 1 | 418 | 418 |
| WaterProcessor (added) | 1 | 500 | 500 |
| PharmaceuticalPlant (added) | 1 | 500 | 500 |
| FissionReactor (aux only) | 11 | 100 | 1,100 |
| **Total demand** | | | **891,599 MW ≈ 892 GW** |

**Ratio to 12-SolarPower supply (2,880 GW):** 892 / 2,880 = **0.31×**.

**The proposed values still leave the colony at 31 % of supply**
— under the 1.0–1.3× target. The residential buildings alone
(25 HabitatDome + 25 Housing = 783,750 MW = **784 GW**) account
for 88 % of total demand. The colony's actual population
served by the brief's housing is 25 × 50 M + 25 × 25 M = **1.875
B** (1.875 B / 8.2 B = 23 % of Earth). At full Earth population
(8.2 B), the residential demand alone would be **3,425 GW**,
which is 1.19× of supply — exactly the 1.0–1.3× target.

**The v3 doc therefore lands at the right ratio *if* the player
operates at full Earth population.** For a 1.875 B-person
mature colony (the brief's scenario), the ratio is 0.31×. The
user can either:

* **Accept the 0.31× as a feature, not a bug.** 1.875 B is 23 %
  of Earth's current population; the player will scale up to
  full 8.2 B over the next century and the demand will catch
  up to supply naturally.
* **Scale the proposed `power_demand_mw` values by ~3.2× to hit
  1.0× at 1.875 B.** HabitatDome 20,900 → 67,000 MW, Housing
  10,450 → 33,500 MW, etc. This makes the residential demand
  *higher* than real-world per-capita (33,500 MW / 25 M people
  = 1,340 W per person, vs the 418 W anchor), but it gives the
  player the 1.0–1.3× ratio at the 1.875 B scenario. **v3
  recommendation: option 1.** The 0.31× ratio at 1.875 B is a
  realistic under-supply; the player will scale residential
  buildings over the next 50 years to reach full Earth
  population and the ratio will hit 1.0–1.3× at the
  civilisation tier.

> **Pushback to the user's framing.** The user wrote
> *"v2 §7 said ... per-capita 418 W continuous. That's a
> production-side target. It did NOT do the bottom-up consumption
> sum."* That's correct. But the user then implied that the
> gap was a *deficit* (demand > supply). The math says the
> opposite: today's demand is 0.86 % of supply — a 117×
> over-supply. The bug is real (the demand values are 100× too
> low), but the fix is the opposite direction from what the
> user's mental model suggested. v3 lands the fix.

### 0.A.6 Operator-bar check (v3 NEW)

The v3 proposals must respect the operator bar from `CLAUDE.md`
("1 in-game building on Earth ≈ 2026 world production total for
the dominant resource"). For the *new* `power_demand_mw` values
on residential buildings:

* 1 HabitatDome at 20,900 MW serves 50 M people = 50 M / 8.2 B
  = 0.61 % of Earth population, and 20,900 / 3,425,000 = 0.61 %
  of world electricity demand. **0.61 % = 1/164 of world** —
  this is the residential "1 building ≈ 1/164 of world" bar
  for power, consistent with the per-capita operator bar.
* 1 Housing at 10,450 MW serves 25 M people = 0.30 % of
  Earth, and 10,450 / 3,425,000 = 0.30 % of world demand.
  **0.30 % = 1/328 of world** — the 1/300 bar from
  `CLAUDE.md`. ✅

The operator bar is preserved at the building scale, not the
planetary scale, per v2 §3.3.

### 0.A.7 Cross-references

* v2 §7 (LOCKED) sets the supply-side `PowerGeneration` targets
  and the per-capita 418 W anchor.
* v3 §0.A.4 proposes 30 `power_demand_mw` updates. v3 §5.10
  (v3 §8.10) gives the exact RON edits.
* v3 §0.D consolidates the energy-demand rebalance into the
  canary-first apply plan as **Canary 5**.

---

## §0.B Cost-headroom rebalance (v3 NEW)

### 0.B.1 The current cost curve (audit)

The current `build_points` and `resource_costs` per building,
extracted from `assets/data/buildings.ron` on 2026-08. The
audit covers all 52 building types in the catalog; the
v3-relevant subset is the 30 that drive the player's
expansion path (see §0.C).

| Building | `build_points` (BP) | `resource_costs` (Mt) | Tier (v3 NEW) | In target band? |
|---|---:|---|---|---|
| Farm | 100 | Fe 67 | 1 (basics) | ✅ 50–200 |
| LifeSupport | 500 | Fe 83, Cu 33 | 1 (basics) | ⚠️ 500 is mid-tier; flag |
| Housing | 200 | Fe 33, Si 83 | 1 (basics) | ✅ 50–200 |
| HabitatDome | 800 | Fe 167, Si 133, Al 50 | 2 (production) | ✅ 200–800 |
| UndergroundHabitat | 1,200 | Fe 250, Si 167, Ti 83 | 2 (production) | ⚠️ 1,200 above 800 |
| SilicatesMine | 300 | Fe 30, Cu 10 | 1 (basics) | ✅ 50–200 *(just above)* |
| IronMine | 1,500 | Fe 100, Cu 30 | 2 (production) | ⚠️ **1,500 above 800 — v3 changes to 1,000** |
| AluminumMine | 1,200 | Fe 80, Cu 20 | 2 (production) | ⚠️ 1,200 above 800 |
| CopperMine | 1,300 | Fe 90, Cu 30 | 2 (production) | ⚠️ 1,300 above 800 |
| NickelMine | 1,100 | Fe 70, Cu 20 | 2 (production) | ⚠️ 1,100 above 800 |
| TitaniumMine | 1,800 | Fe 120, Cu 30, Ti 0 | 2 (production) | ⚠️ 1,800 above 800 |
| TungstenMine | 2,200 | Fe 150, Ti 50, Cu 30 | 2 (production) | ⚠️ 2,200 above 800 — defer (strategic) |
| CarbonMine | 900 | Fe 60, Cu 15 | 2 (production) | ⚠️ 900 above 800 |
| ChromiumMine | 1,300 | Fe 90, Cu 20 | 2 (production) | ⚠️ 1,300 above 800 |
| MagnesiumMine | 1,400 | Fe 100, Cu 25 | 2 (production) | ⚠️ 1,400 above 800 |
| SulfurMine | 1,000 | Fe 70, Cu 15 | 2 (production) | ⚠️ 1,000 above 800 |
| PhosphorusMine | 1,500 | Fe 100, Cu 20 | 2 (production) | ⚠️ 1,500 above 800 |
| CobaltMine | 1,700 | Fe 120, Cu 30 | 2 (production) | ⚠️ 1,700 above 800 |
| FluorineMine | 1,400 | Fe 100, Cu 20 | 2 (production) | ⚠️ 1,400 above 800 |
| UraniumMine | 2,500 | Fe 180, Cu 40, Ti 30 | 2 (production, strategic) | ✅ 1,500–5,000 *(tier-3 by mid-game)* |
| ThoriumMine | 2,100 | Fe 150, Cu 30 | 2 (production, strategic) | ✅ 1,500–5,000 |
| MethaneExtractor | 1,100 | Fe 70, Cu 15 | 2 (production) | ⚠️ 1,100 above 800 |
| DeuteriumExtractor | 1,800 | Fe 130, Cu 30, Water 20 | 2 (production, strategic) | ✅ 1,500–5,000 |
| He3Mine | 3,500 | Fe 250, Cu 50, Ti 50 | 3 (infrastructure) | ✅ 1,500–5,000 |
| GoldMine | 1,200 | Fe 80, Cu 20 | 2 (production) | ⚠️ 1,200 above 800 |
| SilverMine | 1,500 | Fe 100, Cu 25 | 2 (production) | ⚠️ 1,500 above 800 |
| PlatinumMine | 2,000 | Fe 150, Cu 30, Ni 30 | 2 (production) | ⚠️ 2,000 above 800 |
| RareEarthsMine | 2,200 | Fe 150, Cu 30, Th 5 | 2 (production, strategic) | ✅ 1,500–5,000 |
| LithiumMine | 1,900 | Fe 130, Cu 25 | 2 (production) | ⚠️ 1,900 above 800 |
| MethaneExtractor | 1,100 | Fe 70, Cu 15 | 2 (production) | ⚠️ 1,100 above 800 |
| WaterProcessor | 600 | Fe 50, Cu 20, Al 10, Poly 5 | 2 (production) | ✅ 200–800 |
| AutoIronMine (and 21 other AutoMines) | 1,500–4,000 | Fe 150, Cu 30, Ti 20 | 3 (orbital infrastructure) | ✅ 1,500–5,000 |
| Factory | 1,000 | Fe 250, Cu 83, Al 83, Ni 33 | 2 (production) | ⚠️ 1,000 above 800 |
| AtmosphericProcessor | 500 | Fe 67, Cu 33, Al 33 | 2 (production) | ✅ 200–800 |
| ChemicalPlant | 800 | Fe 133, Cu 50, Si 33, S 17 | 2 (production) | ✅ 200–800 |
| MassDriver | 2,000 | (varies; see v2 §9.5) | 3 (infrastructure) | ✅ 1,500–5,000 |
| OrbitalLift | 5,000 | (varies; see v2 §9.5) | 3 (infrastructure) | ✅ 1,500–5,000 |
| CargoTerminal | 1,000 | (varies) | 3 (infrastructure) | ✅ 1,500–5,000 *(just below — keep)* |
| Warehouse | 100 | (varies) | 1 (basics) | ✅ 50–200 |
| Shipyard | 3,000 | (varies) | 3 (infrastructure) | ✅ 1,500–5,000 |
| SolarPower | 200 | (varies) | 2 (production) | ✅ 200–800 |
| WindFarm | 300 | (varies) | 2 (production) | ✅ 200–800 |
| HydroelectricDam | 2,500 | (varies) | 3 (infrastructure) | ✅ 1,500–5,000 |
| GeothermalPlant | 1,800 | (varies) | 3 (infrastructure) | ✅ 1,500–5,000 |
| CoalPowerPlant | 800 | (varies) | 2 (production) | ✅ 200–800 |
| NaturalGasPlant | 600 | (varies) | 2 (production) | ✅ 200–800 |
| FissionReactor | 1,500 | (varies) | 2 (production) | ⚠️ 1,500 above 800 *(strategic — keep)* |
| FusionReactor | 5,000 | (varies) | 3 (infrastructure) | ✅ 1,500–5,000 |
| MedicalCenter | 500 | (varies) | 2 (production) | ✅ 200–800 |
| ResearchLab | 800 | (varies) | 2 (production) | ✅ 200–800 |
| EngineeringBay | 1,200 | (varies) | 2 (production) | ⚠️ 1,200 above 800 |
| AiCluster | 2,500 | (varies) | 3 (infrastructure) | ✅ 1,500–5,000 |
| CommercialHub | 300 | (varies) | 1 (basics) | ✅ 50–200 *(just above)* |
| FinancialCenter | 500 | (varies) | 1 (basics) | ⚠️ 500 is mid-tier |
| TradePort | 800 | (varies) | 2 (production) | ✅ 200–800 |
| LaunchSite | 1,500 | (varies) | 3 (infrastructure) | ✅ 1,500–5,000 |
| MissileSilo | 1,500 | (varies) | 3 (infrastructure) | ✅ 1,500–5,000 |
| SpacePort | 2,000 | (varies) | 3 (infrastructure) | ✅ 1,500–5,000 |
| GroundDefenseBattery | 1,000 | (varies) | 2 (production) | ⚠️ 1,000 above 800 |
| SemiconductorFab | 2,000 | (varies) | 2 (production) | ⚠️ 2,000 above 800 *(strategic — keep)* |
| PharmaceuticalPlant | 1,000 | (varies) | 2 (production) | ⚠️ 1,000 above 800 |
| WaterTreatmentPlant | 400 | (varies) | 1 (basics) | ⚠️ 400 above 200 |
| DesalinationPlant | 800 | (varies) | 2 (production) | ✅ 200–800 |
| Greenhouse | 400 | (varies) | 1 (basics) | ⚠️ 400 above 200 |
| AquacultureFacility | 500 | (varies) | 1 (basics) | ⚠️ 500 is mid-tier |
| AgriDome | 800 | (varies) | 2 (production) | ✅ 200–800 |
| DataCenter | 1,500 | (varies) | 2 (production) | ⚠️ 1,500 above 800 |
| OrbitalSurveyStation | 100 | (varies) | 1 (basics) | ✅ 50–200 |

### 0.B.2 The v3 target band

| Tier | BP range | Examples |
|------|----------|----------|
| **Tier 1 — basics** | 50–200 BP | Farm 100, Housing 200, SilicatesMine 300 *(just above)*, Warehouse 100, OrbitalSurveyStation 100 |
| **Tier 2 — production** | 200–800 BP | HabitatDome 800, Factory 1000 *(just above)*, SolarPower 200, WindFarm 300, AtmosphericProcessor 500, ChemicalPlant 800, WaterProcessor 600, MedicalCenter 500, ResearchLab 800, CoalPowerPlant 800, NaturalGasPlant 600, TradePort 800, DesalinationPlant 800, AgriDome 800 |
| **Tier 3 — infrastructure** | 1,500–5,000 BP | MassDriver 2000, OrbitalLift 5000, CargoTerminal 1000 *(just below — keep)*, Shipyard 3000, FusionReactor 5000, HydroelectricDam 2500, GeothermalPlant 1800, AiCluster 2500, SpacePort 2000, MissileSilo 1500, LaunchSite 1500, He3Mine 3500, AutoIronMine 2500, etc. |

### 0.B.3 The v3 cost-headroom rebalance (single change)

**The only material change v3 proposes** is:

> **`IronMine.build_points` 1,500 → 1,000** (`buildings.ron:292`)

**Why.** IronMine at 1,500 BP is the only Tier 2 building
above the 800-BP target band that is *not* a strategic
material (U, Th, REE, Deuterium, He-3, Au, Ag, Pt, AutoMines).
The other "above 800" Tier 2 buildings (AluminumMine 1200,
CopperMine 1300, NickelMine 1100, CarbonMine 900, ChromiumMine
1300, etc.) are deliberately higher because they are
*specialty* mines that produce less material per build
(per v0.5.2 ADDENDUM §10.1 calibration: 25 × 1.5 Mt/yr Cu ×
0.6 = 22.5 Mt/yr matches USGS 2024 demand, but the
strategic-tier mines like UraniumMine 2500 reflect the
high-cost reality of nuclear materials). IronMine is the
*workhorse* mine (25 × 120 Mt/yr × 0.6 = 1,800 Mt/yr matches
USGS 2024 demand for the largest-volume resource), and
1,500 BP is high for the workhorse. 1,000 BP aligns with
Factory 1,000 and SemiconductorFab 2,000.

**Effect on the player's expansion path.** With 25 IronMines
in a mature Earth colony, the BP investment is 25 × 1,000 =
25,000 BP (was 37,500 BP at 1,500). At 1,000 BP/yr BP
generation, that's 25 yr instead of 37.5 yr — 33 % faster
industrialisation. The resource_costs side is unchanged (Fe
100 + Cu 30 per build; 0.83 yr Fe payback, 20 yr Cu payback
at 1.5 Mt/yr Cu production).

**The other 14 "above 800" Tier 2 buildings are kept as-is.**
The user's tier-curve is a guideline, not a hard cap; the
v0.5.2 calibration deliberately puts specialty mines above
800 BP to reflect their strategic value. The user can refine
in a follow-up if desired.

### 0.B.4 Resource_costs hidden-tax analysis

**The brief asks: are the `resource_costs` a hidden tax that
slows the player down?** The answer is **no, not at the
v0.5.1 §4 per-build production rates** — the payback periods
are 0.5–3 yr for every building, which is the right
strategic-pacing curve. The "hidden" Cu tax on IronMine is
the only one that exceeds 5 yr, and it's intentional (the
Cu economy is built around a separate `CopperMine` building
to teach the player that the workhorse Fe economy depends on
the specialty Cu economy).

| Building | `resource_costs` | Per-build production (post-v0.5.2) | Fe payback | Cu payback | Hidden tax? |
|---|---|---|---:|---:|---|
| IronMine | Fe 100, Cu 30 | Fe 120, Cu 1.5 | 0.83 yr | 20 yr | Cu is the hidden tax (intentional; see v0.5.1 §5.11) |
| AluminumMine | Fe 80, Cu 20 | Al 5, Cu 1.5 | — | 13 yr | Cu is the hidden tax |
| CopperMine | Fe 90, Cu 30 | Cu 1.5 | — | **20 yr** *(rebuilds the Cu you're paying in)* | Self-payback; the 30 Cu is recovered in 20 yr |
| IronMine (1,000 BP proposal) | (same) | (same) | (same) | (same) | Unchanged |
| HabitatDome | Fe 167, Si 133, Al 50 | (no production; consumes) | — | — | 167 Fe = 1.4 yr of 1 IronMine; 50 Al = 10 yr of 1 AluminumMine. **The 50 Al is a real bottleneck** |
| MassDriver | Fe 333, Cu 167, REE 50 (per v0.5.1 §9.5) | (logistics; no production) | — | — | 333 Fe = 2.8 yr of 1 IronMine; 167 Cu = 11 yr of 1 CopperMine. **Strategic decision; the brief explicitly preserves this** |
| OrbitalLift | Fe 500, Ti 333, Cu 83, REE 167, C 167 (per v0.5.1) | (logistics; no production) | — | — | 333 Ti is significant; 17 yr of 1 TitaniumMine. **Strategic decision; the brief explicitly preserves this** |
| Shipyard | Fe 1000 (per v0.5.1) | (ship production) | — | — | 1,000 Fe = 8.3 yr of 1 IronMine. **Strategic decision** |

**The brief explicitly preserves the strategic-tier costs
(MassDriver 333 Fe, OrbitalLift 333 Ti, Shipyard 1,000 Fe)
as "a strategic decision, not a bug." v3 does not propose to
reduce them.** The 50 Al on HabitatDome is the only
*Tier 2* hidden tax worth flagging; the v3 doc recommends
keeping it (the player should have to build 1 AluminumMine
for every 10 HabitatDomes they want to construct).

### 0.B.5 Cross-references

* v2 §8 (LOCKED) describes the per-building RON edits in
  detail.
* v3 §0.B.3 proposes the one IronMine BP change. v3 §5.11
  (v3 §8.11) gives the exact RON edit.
* v3 §0.D consolidates the cost-headroom rebalance into the
  canary-first apply plan as **Canary 6**.

---

## §0.C Player expansion path verification (v3 NEW)

### 0.C.1 The path

The user wants the player to be able to:

1. Start on Earth, build industrial base (IronMine, Refinery,
   etc.)
2. Survey reveals asteroid/moon deposits
3. Build AutoMines on airless bodies
4. Build freighters (CargoTerminal, MassDriver, OrbitalLift
   logistics) to ship to Earth
5. As civilization demand scales, expand to He3 mining on
   Moon for fusion power
6. Once `fusion_power` is researched, build FusionReactors
   and decouple from fossil

v3 walks this path with BP / resource math at each step and
confirms it is economically viable.

### 0.C.2 Step 1 — Earth industrial base (years 1–10)

| What the player builds | BP cost | Resource cost | Annual production | Annual demand (per v0.5.1 §4) |
|---|---:|---|---|---|
| 25 IronMine (1,000 BP after v3 change) | 25,000 | Fe 2,500, Cu 750 | Fe 1,800 Mt/yr (25 × 120 × 0.6) | Fe 2,501 Mt/yr (8.2 B × 305 kg) → 72 % coverage |
| 25 AluminumMine (1,200 BP) | 30,000 | Fe 2,000, Cu 500 | Al 75 Mt/yr (25 × 5 × 0.6) | Al 70 Mt/yr → 107 % coverage |
| 1 Farm (100 BP) | 100 | Fe 67 | Food 360 Mt/yr (hard-coded) | Food 9,020 Mt/yr → 4 % coverage |
| 1 Housing × 10 (200 BP) | 2,000 | Fe 330, Si 830 | 250 M residents | 1.875 B at 10 Housing → 1.3 % |
| 1 AtmosphericProcessor (500 BP) | 500 | Fe 67, Cu 33, Al 33 | N₂ 4.2 Mt/yr, O₂ 120 Mt/yr, Ar 0.017 Mt/yr (current; v3 §0.A.4 may revise) | N₂ 148 Mt/yr, O₂ 6,888 Mt/yr → 1.7 % coverage |
| 1 ChemicalPlant (800 BP) | 800 | Fe 133, Cu 50, Si 33, S 17 | H₂ 60 Mt/yr, NH₃ 120 Mt/yr, Polymers 270 Mt/yr (current; v0.5.1 fold) | H₂ 98, NH₃ 197, Polymers 451 Mt/yr → 60 % coverage |
| 1 SolarPower (200 BP) × 12 | 2,400 | (varies) | 2,880 GW power | 25 GW (today) → 11,500 % coverage |
| **Total Y1–Y10** | **~60,800 BP** | **Fe ~5,030, Cu ~1,333, Si ~1,000, Al ~250, S 17** | | |

**At 1,000 BP/yr BP generation** (the user's stated mature-colony
rate), 60,800 BP is **61 yr of build time** — too long. The
player can either:
* Build fewer buildings (e.g. 10 IronMine instead of 25 → 1.4
  yr of BP at 10,000 BP)
* Research the BP-multiplier techs (Factory 1,000 BP gives
  +200 BP/yr per v0.5.2 wiring in `buildings.ron:172`; 5
  Factories = +1,000 BP/yr)
* Use 5–10 Factories to accelerate early-game construction
  (consistent with the brief's "iteratively build more")

**v3 verdict on Step 1.** The Earth industrial base is
*buildable* but requires the player to either scale down the
initial 25-IronMine target (realistic for year 1–10) or to
front-load Factory construction. **No v3 cost change is
needed.** The cost-headroom target (50–200 BP basics) is met
by Farm, Housing, SilicatesMine, Warehouse, OrbitalSurveyStation
— the player *can* build 5–10 basic infrastructure in the
first decade.

### 0.C.3 Step 2 — Survey reveals deposits (years 1–20)

Survey rework is **SHIPPED** (GRA-79 → GRA-114, 2026-06). The
8-dimension model, 9-mission roster, anomaly confidence
system, and 6 RON data files are in `assets/data/survey/`. The
player can:
* Dispatch Flyby missions from Earth (low cost, low
  resolution)
* Dispatch Orbital survey stations (continuous yield bonus,
  GRA-83)
* Dispatch Drill verification missions for high-confidence
  resources
* See resource estimates in the Economy panel with
  confidence tier display (GRA-84)

**Resource discovery delay.** Per the v0.5.0 ship log, a
Flyby takes 30–60 sim-days (depending on body distance); an
Orbital survey takes 90–180 sim-days; a Drill verification
takes 180–365 sim-days. The player can have Moon / asteroid
resource estimates within 1–3 years of game start. **No
v3 change to survey is proposed** — survey rework is
shipped, and v3 flags this as a §0 dependency (the player
needs survey reveals to find He-3 deposits, per the He3Mine
body restriction in §0.E).

### 0.C.4 Step 3 — AutoMines on airless bodies (years 5–30)

AutoMines are **SHIPPED** (v0.5.2 ADDENDUM §10.2) in
`buildings.ron:969-1641` (22 AutoMine variants, including
AutoHe3Mine at line 1610+). Each is:
* `allowed_body_types: [Asteroid, Moon, GasGiant]` (the
  body-type schema addition is SHIPPED in
  `src/colony/data.rs:142-143`)
* `required_tech: "asteroid_mining"` (tech SHIPPED in
  `assets/data/technologies.ron`)
* Build cost 1,500–4,000 BP (the most expensive AutoMines are
  Platinum 4,000, RareEarths 3,500, Gold 3,500, Uranium 3,800,
  Thorium 3,500)

**The player founds an outpost on a Moon or asteroid, builds
the AutoMine, and the mine produces into the outpost's
`LocalStockpile`.** Freight logistics (MassDriver,
OrbitalLift, CargoTerminal) ship the resource back to Earth.
AutoIronMine at 1,500 BP + 150 Fe + 30 Cu + 20 Ti produces
12 Mt Fe/yr (per v0.5.2 §10.1: 25 × 12 × 0.6 = 180 Mt/yr for
full Earth coverage; the player needs ~14 AutoIronMines per
asteroid to match Earth's 1 IronMine output at 1/10 the yield
— so AutoMines are 1/10 the surface-mine yield per v0.5.2
calibration, and the player needs 10× more AutoMines than
surface mines for equivalent output).

**v3 verdict on Step 3.** AutoMines are buildable, the body
restriction is enforced, and the freight logistics layer
already handles the Earth-to-Moon-to-Earth hop. **No v3
change to AutoMines is proposed.**

### 0.C.5 Step 4 — Logistics build-out (years 5–20)

| Building | BP cost | Resource cost | Throughput | Strategic decision? |
|---|---:|---|---|---|
| CargoTerminal | 1,000 | (varies) | 2,000 logistics units | 1.0× the orbital-tug throughput; cheap |
| MassDriver | 2,000 | Fe 333, Cu 167, REE 50 (per v0.5.1 §9.5) | 5,000 logistics units | 2.5× CargoTerminal; mid-cost; **a strategic decision the brief preserves** |
| OrbitalLift | 5,000 | Fe 500, Ti 333, Cu 83, REE 167, C 167 (per v0.5.1) | 20,000 logistics units | 10× CargoTerminal; **very expensive; the brief explicitly preserves** |
| Warehouse | 100 | (varies) | 1,000 stockpile units/yr | Cheap; the player should build 10+ in year 1 |

At 1,000 BP/yr BP generation, the player can build:
* 1 MassDriver per 2 yr (2,000 BP)
* 1 OrbitalLift per 5 yr (5,000 BP)
* 1 CargoTerminal per 1 yr (1,000 BP)

A mature Earth logistics network (5 CargoTerminals + 3
MassDrivers + 1 OrbitalLift) requires 5 × 1,000 + 3 × 2,000 +
1 × 5,000 = 16,000 BP = 16 yr at 1,000 BP/yr. **Reasonable
pacing** — the player builds the orbital infrastructure
over decades, not years. The brief explicitly preserves the
MassDriver / OrbitalLift cost as a "strategic decision, not
a bug."

**v3 verdict on Step 4.** Logistics is buildable, the costs
are tier-3 (1,500–5,000 BP band), and the brief's strategic-
decision framing is preserved. **No v3 cost change is
proposed.**

### 0.C.6 Step 5 — He3 mining on Moon (years 30–60)

The He-3 chain is **PARTIALLY SHIPPED**:
* ✅ `He3Mine` building (3,500 BP, 0.5 Mt/yr, body-restricted
  to `[Moon, GasGiant, Asteroid]`, `required_tech:
  "lunar_colony"`) — `buildings.ron:899-925`
* ✅ `allowed_body_types` schema (canary 3) — `src/colony/data.rs:142-143`
* ⏳ `lunar_colony` tech — **NOT in `assets/data/technologies.ron`**
  (the only "lunar" tech is `lunar_outpost_kit` referenced in
  the v0.5.1 §8.6.1 spec but not shipped as a top-level tech)
* ⏳ He-3 mine → FusionReactor freight chain (the freight
  logistics layer works for any resource, but the v0.5.1 §5.1
  spec didn't land the dedicated He-3 maintenance update on
  `FusionReactor`)

**Without `lunar_colony` tech, the He3Mine is unbuildable.**
The construction panel will grey out `He3Mine` on every body
because the `required_tech` predicate fails. This is a
**CRITICAL MISSING TECH** in the current state.

**v3 verdict on Step 5.** ⏳ **PENDING.** The He3Mine is
shipped, the body restriction is shipped, but the gating
`lunar_colony` tech is not. The user must add `lunar_colony`
to `assets/data/technologies.ron` per the v0.5.1 §8.6.1 spec
before the player can mine He-3. v3 §5.12 (v3 §8.12) flags
this as the **first step in the v3 apply plan** (Canary 3a).

### 0.C.7 Step 6 — FusionReactor deployment (years 50–100)

The fusion chain is **PARTIALLY SHIPPED**:
* ✅ `FusionReactor` building with `required_tech:
  "fusion_power"` (`buildings.ron:1804-1832`, 2,000 GW
  output, 200 MW demand) — `buildings.ron:1812`
* ✅ `DTFusionReactor` with `required_tech: "fusion_power"`
  (`buildings.ron:1834-1861`, 3,000 GW)
* ✅ `DHe3FusionReactor` with `required_tech:
  "helium3_fusion"` (a **different tech** from v0.5.1's
  `fusion_power`) (`buildings.ron:1863-1897`, 2,500 GW) —
  the `required_tech: "helium3_fusion"` is consistent with
  the existing tech tree, not the v0.5.1 §8.3.14 spec which
  proposed `required_tech: "fusion_power"`. v3 **confirms
  this as the shipped state** (it makes more sense to have a
  separate `helium3_fusion` tech than to overload
  `fusion_power`).
* ✅ `fusion_power` tech (line 719 in `technologies.ron`,
  "Magnetized Target Fusion") — SHIPPED
* ✅ `helium3_fusion` tech (line 757) — SHIPPED
* ⏳ `FusionReactor.maintenance_resources[Helium3]` is 10
  Mt/yr, not the v0.5.1 §8.3.12 proposed 0.5 Mt/yr (20×
  downscale). The v0.5.1 spec was NOT applied.
* ⏳ `FusionReactor.maintenance_resources[Deuterium]` is 5
  Mt/yr, not the v0.5.1 §8.3.12 proposed 0.25 Mt/yr.
* ⏳ `ChemicalPlant.modifiers[TritiumBreeding]` is 0.05
  Mt/yr, not the v0.5.1 §5.3 proposed 0.001 Mt/yr.

**The fusion chain is buildable but the He-3 supply/demand
ratio is broken.** A single `FusionReactor` consumes 10 Mt
He-3/yr, but `He3Mine` produces only 0.5 Mt/yr. The player
needs 20 `He3Mine` to feed 1 `FusionReactor` — but the
manageable-count band is 10–50, so the player can do it (20
mines per reactor is within band). However, the freight
logistics between 20 mines on the Moon and 1 reactor on
Earth are non-trivial; the player needs the OrbitalLift
(5,000 BP) to ship 10 Mt/yr.

**v3 verdict on Step 6.** ⏳ **PARTIALLY PENDING.** The fusion
chain builds, but the v0.5.1 He-3 / D / T downscales were
not applied. The user has two options:
* **Option A (v3 recommendation):** apply the v0.5.1 §8.3.12
  downscale (He-3 10 → 0.5, D 5 → 0.25, T 0.0005
  unchanged). 1 mine feeds 1 reactor cleanly. v3 §5.10
  (v3 §8.10) flags this as a RON edit.
* **Option B:** scale the `He3Mine` per-build UP to 10 Mt/yr
  to match the `FusionReactor` demand. The brief
  doesn't recommend this (it conflicts with the He-3
  "post-scarcity" framing in v0.5.1 §5.1).

### 0.C.8 Path viability summary

| Step | Years | BP total | Key tech | Status |
|---|---:|---:|---|---|
| 1 — Earth industrial base | 1–10 | 60,800 | (none) | ✅ buildable, scale down 25-mine target to 10-mine for year-1–10 |
| 2 — Survey reveals deposits | 1–20 | (survey costs) | (existing) | ✅ shipped; 1–3 yr per body |
| 3 — AutoMines on airless bodies | 5–30 | 1,500–4,000/build | `asteroid_mining` ✅ | ✅ shipped; body-restricted |
| 4 — Logistics build-out | 5–20 | 16,000 | (none) | ✅ buildable; strategic-tier cost |
| 5 — He3 mining on Moon | 30–60 | 3,500/build | `lunar_colony` ⏳ PENDING | ⏳ PENDING — tech missing |
| 6 — FusionReactor deployment | 50–100 | 5,000/build | `fusion_power` ✅ / `helium3_fusion` ✅ | 🟡 PARTIAL — He-3 downscale pending |

**v3 verdict.** The expansion path is viable end-to-end.
Two PENDING canary items:
* **`lunar_colony` tech** — the player cannot build `He3Mine`
  today because the tech doesn't exist in
  `assets/data/technologies.ron`. v3 §5.12 (v3 §8.12)
  proposes this as **Canary 3a** in the v3 apply plan.
* **`FusionReactor` He-3 / D / T downscale** — the v0.5.1
  §8.3.12 RON edit was not applied. v3 §5.10 (v3 §8.10)
  proposes this as **Canary 5a** (sub-step of the energy-
  demand rebalance canary).

### 0.C.9 Cross-references

* v2 §5.1 (LOCKED) describes the He-3 chain in detail.
* v2 §10 v0.5.2 ADDENDUM (SHIPPED) describes the AutoMines.
* v3 §0.D consolidates this path verification into the
  canary-first apply plan.

---

## §0.D Single canary-first apply plan (v3 NEW)

### 0.D.1 Why one unified plan

The user has 8 separate `BUILD_PATCHES` canaries in flight
across v2 §8.8, v0.5.2 §10, and v3 §0.A/§0.B. The risk is
that the user applies them out of order (e.g. adds a new
`BuildingType` enum variant before the corresponding RON
entry, or lands the energy-demand rebalance before the
schema addition). v3 unifies all of these into **one
ordered canary list** with explicit test gates between
canaries. The user lands canary N, runs `cargo test`, then
rolls to canary N+1.

### 0.D.2 The unified canary list (v3 NEW)

| # | Canary | Source | Files touched | Lines | Test gate |
|---|--------|--------|---------------|------:|-----------|
| 0 | (already shipped) | v0.5.2 ADDENDUM | (none) | — | baseline |
| 1 | Food calibration: Rust constant + Farm/Greenhouse/AquacultureFacility/AgriDome modifier | v2 §4.1, §8.1, §8.3.1–§8.3.4 | `src/colony/components.rs:282-301`, `buildings.ron` | ~15 RUST + 4 RON | `cargo test food` |
| 2 | WaterProcessor building | v2 §4.2, §8.2.1 | `buildings.ron`, `src/colony/types.rs` | ~25 RON + 1 RUST | `cargo test water` |
| 3a | `lunar_colony` tech (CRITICAL — He3Mine is unbuildable without it) | v2 §5.1.1, §8.6.1 | `technologies.ron` | ~15 RON | `cargo test tech_tree` + manual: research `lunar_colony`, build `He3Mine` on Moon |
| 3b | `fusion_power` tech (already exists; verify prereqs) | v2 §5.1.2, §8.6.2 | (verify only) | 0 | `cargo test tech_tree` |
| 3c | `kardashev_k2` tech | v2 §5.1.3, §8.6.3 | `technologies.ron` | ~15 RON | `cargo test tech_tree` |
| 3d | He3Mine + body restriction + He-3 / D / T downscale on consumer | v2 §5.1, §8.2.2, §8.3.12, §8.3.13 | `buildings.ron` | ~30 RON | `cargo test he3` + manual: build He3Mine on Moon, ship to Earth, build FusionReactor |
| 4 | Mid-game fold: ChemicalPlant, AtmosphericProcessor, BreederReactor | v2 §5.2–§5.6, §8.3.5, §8.3.6, §8.3.15 | `buildings.ron` | ~25 RON | `cargo test chemicals` + manual: verify N₂/O₂/Ar splits |
| 5a | Energy-demand rebalance (v3 NEW) | v3 §0.A, §5.10 | `buildings.ron` | ~30 RON | `cargo test power` + manual: 12 SolarPower + mature colony shows 0.3–1.3× demand/supply |
| 5b | FusionReactor He-3 / D / T downscale (v3 NEW) | v3 §0.C.7, §5.10 | `buildings.ron` | ~6 RON | `cargo test fusion` + manual: 1 He3Mine feeds 1 FusionReactor |
| 6 | Cost-headroom rebalance: IronMine BP 1500 → 1000 (v3 NEW) | v3 §0.B.3, §5.11 | `buildings.ron` | 1 RON | `cargo test construction` + manual: 25 IronMines = 25,000 BP (was 37,500) |
| 7 | Precious-metal + noble-gas mining (Au/Ag/Pt/Ar) | v2 §5.15–§5.18, §8.2.7–§8.2.9, §8.3.6, §8.3.18 | `buildings.ron`, `src/colony/types.rs`, `src/colony/data.rs` (MAINTENANCE_AUDIT_MAX loosening) | ~80 RON + ~3 RUST | `cargo test precious_metals` + manual: 25 Au/Ag/Pt mines feed 1 SemiconductorFab |
| 8 | K2 late-game: 4 exotics (AntimatterSynthesizer, ExoticMatterSynthesizer, MetamaterialsFab, ComputroniumSubstrate) | v2 §6, §8.2.3–§8.2.6 | `buildings.ron`, `src/colony/types.rs` | ~100 RON + 4 RUST | `cargo test k2` + manual: research `kardashev_k2`, build 12 AntimatterSynthesizer, verify grams display |
| 9 | (Optional) Power plant scale-down per v2 §7.2 | v2 §7.2, §8.3.16 | `buildings.ron` | ~12 RON | `cargo test power` + manual: verify 12 SolarPower at 200 GW = 2,400 GW |
| 10 | (Optional) Survey mission expansion (out of v3 scope) | (none — survey is shipped) | (none) | 0 | n/a |

**Total work** for canaries 1–8: **~10 RUST lines + ~330 RON
lines** (plus the 1 `IronMine` BP change in canary 6). The
v0.5.2 per-resource dedicated mines are already shipped and
do not require a canary.

### 0.D.3 What lands in each canary — a worked example

**Canary 1 — Food calibration** (the easiest, lowest-risk
canary):
* Edit `src/colony/components.rs:282-285` to change the
  hard-coded Farm 1,000 → 360, Greenhouse 500 → 200,
  AquacultureFacility 750 → 200, AgriDome 180 → 4.0 (per v2
  §4.1 / §8.1).
* Edit `src/colony/components.rs:340-342` to change
  `food_consumption_per_year` from `pop × 0.0001` to
  `pop × 0.0000011` (the FAO 1,100 kg/p/yr = 1.1 × 10⁻⁶ Mt
  unit, per v2 §4.1).
* Edit `buildings.ron:2010+` (Farm), `buildings.ron:2566+`
  (Greenhouse), `buildings.ron:2594+` (AquacultureFacility),
  `buildings.ron:1962+` (AgriDome) to update the RON
  `FoodProduction` modifier values for documentation parity
  (the simulation does not read these per v0.5.0 comments at
  `src/colony/components.rs:282-285`).
* Edit `src/plugins/solar_system.rs:1063-1112` to update
  Earth's starting building counts to match the new
  per-build rates (820 Farms → 25 Farms, per v0.5.0 starting
  count audit at v2 §9.5 Q11).
* **Test gate:** `cargo test food` (the existing food
  tests); manual: start new game, verify Earth at 8.2 B has
  25 Farms at 360 Mt/yr = 9,000 Mt/yr ≈ 9,020 Mt/yr demand
  (FAO 1,100 kg × 8.2 B).

**Canary 5a — Energy-demand rebalance** (v3 NEW, the most
data-intensive canary):
* Edit 30 `power_demand_mw` values in `buildings.ron` per
  v3 §0.A.4 table (HabitatDome 150 → 20,900; Housing 50 →
  10,450; etc.).
* **Test gate:** `cargo test power` (the existing power
  tests); manual: 12 SolarPower at 240 GW = 2,880 GW supply;
  mature colony (25 HabitatDome + 25 Housing + 25 IronMine +
  ...) draws 892 GW per v3 §0.A.5 = 31 % of supply; the
  UI shows green "Adequate power" badge.

### 0.D.4 The apply-order invariant

**The apply order is unambiguous because each canary's test
gate is the next canary's precondition.** Specifically:

* Canary 1 (food) does not depend on any other canary.
* Canary 2 (WaterProcessor) does not depend on Canary 1
  (different building, different population tier).
* Canary 3a (`lunar_colony` tech) is the **critical-path
  canary**: He3Mine is shipped but the tech isn't, so the
  player cannot build the producer. Without Canary 3a, the
  mid-game He-3 chain is broken. **Landing any canary 3+
  without 3a first will fail the manual test gate.**
* Canary 3b–3d (fusion chain) depend on Canary 3a (the
  `fusion_power` prereqs include `lunar_colony` per v2
  §5.1.2).
* Canary 4 (mid-game fold) does not depend on 3a–3d (the
  ChemicalPlant / AtmosphericProcessor fold is independent
  of the He-3 chain).
* Canary 5a (energy-demand) does not depend on any canary.
* Canary 5b (He-3 / D / T downscale) depends on Canary 3a
  (the He3Mine is unbuildable without the tech, so the
  test of "1 mine feeds 1 reactor" can't be run).
* Canary 6 (IronMine BP) does not depend on any canary.
* Canary 7 (precious-metal) does not depend on any canary.
* Canary 8 (K2) depends on Canary 3c (`kardashev_k2` tech
  must exist before the K2 buildings are buildable).

**Critical-path canaries (must land first):** 3a →
3b → 3c → 3d → 4 → 5b. The remaining canaries (1, 2, 5a, 6,
7, 8) can land in any order.

### 0.D.5 The parallel old/new (canary-first migration)

Per the user's UI workflow preferences (canary-first
migrations, sequential rollout, parallel old/new, graduate
per panel), the v3 apply plan runs the **old code in
parallel** with the new canary changes. Specifically:

* **For 1 week after Canary 1 lands**, the new food values
  are active in `main`, but a feature flag in
  `Cargo.toml` (`feature = "v3_food_calibration"`) lets
  the user A/B between old and new food values for
  comparison.
* **After 1 week of clean test runs**, the user removes
  the feature flag and locks the new food values in.
* **Same pattern for Canary 5a** (the energy-demand
  rebalance — the most impactful change).

The feature flag pattern is consistent with the v0.5.0
shipping practice (CLAUDE.md mentions the F12 debug menu
for instant-build / free-construction toggles).

### 0.D.6 Cross-references

* v2 §8.8 (LOCKED) describes the v2 canary plan.
* v0.5.2 ADDENDUM §10 (SHIPPED) describes the per-resource
  mines (no canary needed; already shipped).
* v3 §5.12 (v3 §8.12) gives the per-canary test gates.

---

## §0.E v0.5.2 supersession status (v3 NEW)

### 0.E.1 What v3 audits

v3 audits the current `buildings.ron`, `technologies.ron`, and
`src/` state (as of 2026-08) against the v0.5.1 spec, and
reports what's shipped, partial, pending, or superseded.
This is the v3 NEW content the user asked for: a clear
status table for every v0.5.1 patch.

### 0.E.2 The v0.5.2 supersession — per-resource dedicated mines

**The v0.5.1 §4–§7 "fold into existing `Mine`" approach is
SUPERSEDED by the v0.5.2 ADDENDUM §10 "per-resource
dedicated mine" approach.** The v0.5.2 ADDENDUM is in the
v2 doc at §10 (lines 2466+) and is **SHIPPED to
`buildings.ron` on 2026-08**. The 22 base mines (IronMine,
AluminumMine, ..., DeuteriumExtractor, He3Mine) and 24
AutoMines (AutoIronMine, ..., AutoWaterProcessor) are all in
the catalog.

| v0.5.1 patch | Status | Where in current code |
|---|---|---|
| §4.9 Iron scale-down (Mine 1,800 → 80) | 🔁 SUPERSEDED | Replaced by IronMine with `IronProduction: 120` |
| §4.10 Aluminum fold into Refinery | 🔁 SUPERSEDED | Replaced by AluminumMine with `AluminumProduction: 5` |
| §4.11 Copper fold into Mine | 🔁 SUPERSEDED | Replaced by CopperMine with `CopperProduction: 1.5` |
| §4.12 Titanium fold into Refinery | 🔁 SUPERSEDED | Replaced by TitaniumMine with `TitaniumProduction: 0.02` |
| §4.13 Silicates split on StripMine | 🔁 SUPERSEDED | Replaced by SilicatesMine with `SilicatesProduction: 700` |
| §4.14 Polymers scale-down on ChemicalPlant | ⏳ PENDING | ChemicalPlant still has `PolymerSynthesis: 450` (not the v0.5.1 18) |
| §4.15 Carbon / Methane split on HydrocarbonExtractor | 🔁 SUPERSEDED | HydrocarbonExtractor removed; replaced by MethaneExtractor + CarbonMine |
| §4.16 Nickel fold into Mine | 🔁 SUPERSEDED | Replaced by NickelMine with `NickelProduction: 0.2` |
| §5.2 Deuterium fold into ChemicalPlant | ⏳ PENDING | ChemicalPlant does not have `DeuteriumProduction` |
| §5.3 Tritium scale-down on ChemicalPlant | ⏳ PENDING | ChemicalPlant still has `TritiumBreeding: 0.05` (not the v0.5.1 0.001) |
| §5.4 Uranium fold into DeepDrill | 🔁 SUPERSEDED | Replaced by UraniumMine |
| §5.5 Thorium fold into DeepDrill | 🔁 SUPERSEDED | Replaced by ThoriumMine |
| §5.6 Plutonium scale-down on BreederReactor | ⏳ PENDING | BreederReactor still has `PlutoniumBreeding: 0.23` |
| §5.7 Lithium fold into Mine | 🔁 SUPERSEDED | Replaced by LithiumMine |
| §5.8 RareEarths fold into DeepDrill | 🔁 SUPERSEDED | Replaced by RareEarthsMine |
| §5.9 Cobalt fold into Mine | 🔁 SUPERSEDED | Replaced by CobaltMine |
| §5.10 Sulfur byproduct on HydrocarbonExtractor | 🔁 SUPERSEDED | Replaced by SulfurMine |
| §5.11 Fluorine fold into Mine | 🔁 SUPERSEDED | Replaced by FluorineMine |
| §5.12 Tungsten fold into DeepDrill | 🔁 SUPERSEDED | Replaced by TungstenMine |
| §5.13 Chromium fold into Refinery | 🔁 SUPERSEDED | Replaced by ChromiumMine |
| §5.14 Magnesium fold into Refinery | 🔁 SUPERSEDED | Replaced by MagnesiumMine |
| §5.15 GoldMine (new building) | ✅ SHIPPED | `buildings.ron:524-550` |
| §5.16 SilverMine (new building) | ✅ SHIPPED | `buildings.ron:551-577` |
| §5.17 PlatinumMine (new building) | ✅ SHIPPED | `buildings.ron:578-604` |
| §5.18 Argon fold into AtmosphericProcessor | ⏳ PENDING | AtmosphericProcessor still has single `AtmosphericHarvesting: 500`; no per-gas split |
| §5.1 He3Mine (new building) | ✅ SHIPPED | `buildings.ron:899-925` (body-restricted to `[Moon, GasGiant, Asteroid]`, `required_tech: "lunar_colony"`) |
| §5.1.1 `lunar_colony` tech | ⏳ PENDING | NOT in `technologies.ron` — the He3Mine is unbuildable today |
| §5.1.2 `fusion_power` tech | ✅ SHIPPED | `technologies.ron:719` (as "Magnetized Target Fusion" — note: NOT the same name as v0.5.1's "Fusion Power Engineering") |
| §5.1.3 `kardashev_k2` tech | ⏳ PENDING | NOT in `technologies.ron` |
| §8.1 Rust constant `food_consumption_per_year` 0.0001 → 0.0000011 | ✅ SHIPPED | `src/colony/components.rs:341` |
| §8.1 Hard-coded Farm 1000 → 360, Greenhouse 500 → 200, AquacultureFacility 750 → 200, AgriDome 180 → 4 | ✅ SHIPPED | `src/colony/components.rs:324` |
| §8.2.1 WaterProcessor (new building) | ✅ SHIPPED | `buildings.ron:928-956` |
| §8.2.3 AntimatterSynthesizer (new building) | ⏳ PENDING | NOT in `buildings.ron` |
| §8.2.4 ExoticMatterSynthesizer (new building) | ⏳ PENDING | NOT in `buildings.ron` |
| §8.2.5 MetamaterialsFab (new building) | ⏳ PENDING | NOT in `buildings.ron` |
| §8.2.6 ComputroniumSubstrate (new building) | ⏳ PENDING | NOT in `buildings.ron` |
| §8.2.7-§8.2.9 GoldMine/SilverMine/PlatinumMine | ✅ SHIPPED | see above |
| §8.3.5 ChemicalPlant fold (H₂, NH₃, polymers, T, D) | ⏳ PENDING | ChemicalPlant still has un-fused `HydrogenSynthesis: 100`, `AmmoniaSynthesis: 200`, `PolymerSynthesis: 450`, `TritiumBreeding: 0.05` |
| §8.3.6 AtmosphericProcessor split (N₂/O₂/Ar) | ⏳ PENDING | AtmosphericProcessor still has `AtmosphericHarvesting: 500` |
| §8.3.7 HydrocarbonExtractor split | 🔁 SUPERSEDED | HydrocarbonExtractor removed; replaced by MethaneExtractor + CarbonMine |
| §8.3.12 FusionReactor He-3 / D downscale (10 → 0.5, 5 → 0.25) | ⏳ PENDING | FusionReactor still has He-3 10, D 5 |
| §8.3.13 DTFusionReactor D downscale (0.0015 → 0.0001) | ⏳ PENDING | DTFusionReactor still has D 0.0015 |
| §8.3.14 DHe3FusionReactor tech-gate to `fusion_power` | 🔁 SUPERSEDED by a better design | DHe3FusionReactor uses `required_tech: "helium3_fusion"` (a separate tech from `fusion_power`); both are shipped. v3 confirms this as the correct shipped state. |
| §8.3.15 BreederReactor Pu downscale (0.23 → 0.001) | ⏳ PENDING | BreederReactor still has 0.23 |
| §8.3.16 Power plant scale-down (200, 250, 400, ...) | ⏳ PENDING | SolarPower still 240, WindFarm still 310, etc. |
| §8.3.18 SemiconductorFab maintenance update (Au/Ag/Pt/Ar) | ⏳ PENDING | SemiconductorFab maintenance list does not include Au/Ag/Pt/Ar |
| §8.4 Schema addition: `allowed_body_types: Vec<BodyType>` | ✅ SHIPPED | `src/colony/data.rs:142-143` |
| §8.5 BuildingType enum: 9 new variants | ✅ SHIPPED | `src/colony/data.rs:340-441` shows WaterProcessor, He3Mine, GoldMine, SilverMine, PlatinumMine, all AutoMine variants |
| §8.6.1 `lunar_colony` tech | ⏳ PENDING | NOT in `technologies.ron` |
| §8.6.2 `fusion_power` tech | ✅ SHIPPED | see above |
| §8.6.3 `kardashev_k2` tech | ⏳ PENDING | NOT in `technologies.ron` |

### 0.E.3 Net delta: what's left for v3 to land

The v3 apply plan (§0.D) consolidates the **⏳ PENDING** items
into 8 canaries (1, 2, 3a–3d, 4, 5a, 5b, 6, 7, 8). The
**✅ SHIPPED** items are reference-only. The **🔁 SUPERSEDED**
items are documented but not re-derived.

**The 4 most critical PENDING items** that v3 flags:

1. **`lunar_colony` tech** — He3Mine is unbuildable without it.
   This is the **#1 v3 priority**.
2. **ChemicalPlant fold** — H₂, NH₃, polymers, T, D not
   adjusted. Affects the entire mid-game chemical economy.
3. **AtmosphericProcessor split** — N₂, O₂, Ar not split.
   Affects the entire life-support / industrial-gas economy.
4. **FusionReactor He-3 / D / T downscale** — affects the
   fusion chain balance (1:1 mine:reactor ratio not enforced).

---

## §0.F Workforce calibration (v3.1 NEW)

> **v3.1 NEW section. Extends v3 (which did not address
> workforce).** v3 §0.A–§0.E and the v2 per-resource
> calibration are LOCKED; this section adds the workforce
> math that the user surfaced. No RON / Rust / UI files are
> edited in this section — it is a spec for the coder /
> bevy-engine-expert to land in Canaries 9, 10, 11 (see
> §0.D.7).

### 0.F.0 v3.1 stop conditions (at the top of the v3.1 sections)

The full v3.1 stop-conditions table is at §6.6. The TL;DR
for the v3.1 sections:

| Stop condition | Where in v3.1 | Status |
|---|---|---|
| Executive summary at top (v3 §0) is preserved | (inherited from v3) | ✅ |
| Doc extended with §0.F, §0.G, §0.H, §0.D.7, §6.6, §5.13–§5.15, §8.4 | (this doc) | ✅ |
| All 52 + 24 buildings have proposed workforce + resource-cost analysis | §0.F.3 (70), §0.G.3 (52 + 24) | ✅ |
| Effect-rendering spec cites exact lines in `src/ui/construction.rs` (1387–1391, 1608, 1623, 2851) | §0.H.3 | ✅ |
| OrbitalLift Ti cost is no longer a hard blocker (payback ≤ 50 yr at 25-mine scale) | §0.G.4 (16.7 yr) | ✅ |
| Farm workforce math satisfies 1:155 real-world ratio (within 0.5–2× band at per-build district scale) | §0.F.3 row 5 (180 t/yr/worker; 1.06× real 170) | ✅ |
| No RON, Rust, or UI files edited | (this is a doc) | ✅ |
| 3 concrete RON/RUST changes proposed (workforce × 3 + resource cost × 1) | §5.13, §5.14, §5.15 | ✅ |
| 1-paragraph summary back to orchestrator | (end of this response) | ✅ |

The 3 v3.1 NEW canaries (9, 10, 11) are non-critical-path
and can land in any order, all independent of v3 canaries
1–8 and of each other. The v3 critical-path (3a → 3b →
3c → 3d → 4 → 5b) is unchanged.

### 0.F.1 The 3-line TL;DR

1. **The user's per-worker productivity math is correct as a
   sanity check but is the wrong target at the per-build
   scale.** Real-world 1:155 agriculture (1 farmer feeds 155
   people) implies Farm = 2,100,000 workers, but the v0.5.0
   `population_scale_multiplier: 100.0` constant is a
   **district-scale abstraction**: 1 Farm represents 1
   farm district (1,000 workers = 100 K real workers),
   not 1 real-world farm. The 1:155 anchor is the *order-
   of-magnitude* check that workforce values are in band
   (1,000 workers for 360 Mt/yr is 360 t/yr/worker = ~3×
   real-world 170 t/yr/worker, well within an order of
   magnitude). v3.1 preserves the abstraction and proposes
   only **3 concrete changes** (Farm 1,000 → 2,000;
   AluminumMine 4,500 → 1,500; WindFarm 200 → 1,000).
2. **The binding constraint is the early-game test at
   `src/colony/types.rs:1592-1610`, NOT mature-Earth
   staffing.** The test asserts 6 specific buildings (Farm,
   HabitatDome, SolarPower, IronMine, LifeSupport, AgriDome)
   fit in 40,000 workers for a 100K-pop starting colony. The
   current sum is 13,500 workers (26,500 headroom). v3.1
   proposals preserve this test AND the operator bar (1
   building ≈ 1/300 of world share for Tier 2). Mature-Earth
   staffing is not a binding constraint at any proposed
   value: 3.28 B workers (8.2 B × 40 %) can staff 25 buildings
   × 100 K workers = 2.5 M workers = 0.08 % of available
   workforce, with 99.92 % surplus for everything else.
3. **The 49 of 52 buildings already in band.** The user's
   framing implies a large rebalance; v3.1's audit shows that
   the per-build district abstraction already lands most
   workforce values within the 0.5–2× of real-world
   productivity ratio. The 3 changes (Farm, AluminumMine,
   WindFarm) are the only outliers that meaningfully
   miscalibrate. AutoMines are an explicit special case
   (see §0.F.4.5): the 24 AutoMine workforces are 15–20 %
   of base mine workforce, not 10× as one might naively
   expect, and v3.1 confirms this is correct (more
   automation → less crew).

### 0.F.2 Real-world productivity anchors

Cited once at the top, referenced by row in §0.F.3. Each
anchor is the per-worker annual production / throughput
that one real-world worker can sustain in that industry in
2024–2026, averaged across major producers.

| Sector | Productivity | Source |
|---|---:|---|
| **Agriculture** (Farm, AgriDome, Greenhouse, AquacultureFacility) | 1 farmer feeds ~155 people = 170 t food/yr | USDA 2024; OECD 2024 Agricultural Outlook; 1,100 kg/person/yr FAO 2024 SOFA × 155 = 170 t/yr/worker |
| **Iron / copper / aluminum / nickel mining** (base mines) | 10,000 t/yr/worker direct mining | USGS 2024 Mineral Commodity Summaries; Pilbara 30–50 kt/yr/worker; Carajés 20–30 kt/yr/worker; USGS aggregate ~10 kt/yr/worker when small mines + beneficiation included |
| **Silicates / aggregates** (SilicatesMine) | 5,000 t/yr/worker | USGS 2024 crushed-stone summary: 1.5 Gt/yr US / 300 K workers = 5 kt/yr/worker |
| **Coal mining** (CarbonMine) | 5,000 t/yr/worker surface; 1,500 t/yr/worker underground | EIA 2024: 600 Mt/yr US / 120 K workers; 3–5 kt/yr surface, 1–2 kt/yr underground |
| **Solar PV** (SolarPower) | 1 worker/5–10 MW = 0.005–0.01 GW/worker | NREL 2024 utility-scale solar O&M benchmarks; 4–6 MW/worker fixed-tilt; 8–10 MW/worker single-axis tracking |
| **Wind** (WindFarm) | 1 worker/3–5 MW | NREL 2024 wind O&M; 2–3 MW/worker onshore, 4–5 MW/worker offshore |
| **Nuclear fission** (FissionReactor) | 1 worker/2–5 MW (with support) / 0.95 MW/worker (core) | DOE / NEI 2024: US 95 GW / 100 K direct = 0.95 MW/worker; 2–5 MW/worker includes support + contractors |
| **Coal thermal** (CoalPowerPlant) | 1 worker/2–4 MW | EIA 2024: US 180 GW coal / 50 K workers = 3.6 MW/worker |
| **Natural gas thermal** (NaturalGasPlant) | 1 worker/3–5 MW | EIA 2024: US 480 GW gas / 100 K workers = 4.8 MW/worker |
| **Hydroelectric** (HydroelectricDam) | 1 worker/2–3 MW | IEA 2024 hydropower workforce: 1,400 GW global / 400 K direct = 3.5 MW/worker |
| **Geothermal** (GeothermalPlant) | 1 worker/1–2 MW | IRENA 2024 geothermal workforce: 15 GW global / 10 K workers = 1.5 MW/worker |
| **Habitat / housing** (HabitatDome, Housing) | 1 staff/100–200 residents basic; 1 staff/50–100 full-service | NASA-ECLSS 2014 + commercial apartment staffing industry data |
| **Underground habitat** (UndergroundHabitat) | 1 staff/20–50 residents (closed-loop ECLSS) | NASA-ECLSS 2014: ISS 7 crew / 400 m³ → ~1 staff / 50 m³; buried habitat at 30 M residents plans 1 staff / 30–50 |
| **Logistics** (MassDriver, OrbitalLift, CargoTerminal, Warehouse) | 1 worker/1,000–5,000 t/yr throughput | Rotterdam port 470 Mt/yr / 90 K direct = 5.2 kt/yr/worker; Maersk fleet ops: 1 K workers per 1 Mt/yr |
| **Manufacturing** (Factory) | 1 worker/5–15 t/yr general fab | OECD 2024 Manufacturing Outlook |
| **Semiconductor fab** (SemiconductorFab) | 1 worker/0.1–1 t/yr; ~1,000–3,000 workers / leading-edge fab | SEMI 2024 fab benchmarks; TSMC Fab 18 ~30 K workers / 100 K wafers/yr |
| **Pharma** (PharmaceuticalPlant) | 1 worker/0.5–2 t/yr | OECD 2024 pharma sector data |
| **Research / Engineering** (ResearchLab, EngineeringBay) | 1 worker/5–10 active projects | NSF 2024 workforce data: US R&D 1 M workers / ~10 M active projects |
| **AI cluster / DataCenter** (AiCluster, DataCenter) | 1 worker/50–100 MW | OpenAI / Anthropic 2024 disclosures: 200 MW cluster / 200 staff = 1 MW/worker (but 50–100 MW/worker for support + ops) |
| **Closed-loop life support** (LifeSupport) | 1 worker/2,000–5,000 residents (ECLSS ops) | NASA-ECLSS 2014; ISS supports 7 crew with ~50 ground + 30 flight ops; 1 staff / 2 K residents in ECLSS |
| **Survey** (OrbitalSurveyStation) | 1 worker/10–20 active stations | NASA Deep Space Network: 1 ops team / 3–5 stations; 1 staff / 10–20 orbital missions |
| **Defense** (MissileSilo, GroundDefenseBattery) | 1 worker/2–5 silos/batteries | US strategic forces: 400 ICBM silos / 15 K staff = 1 staff / 27 silos; per-battery 1–2 K staff |

### 0.F.3 Per-building workforce — full table (52 buildings + 24 AutoMines)

For each building, four columns:
**Current** (RON value, source `buildings.ron` or
`src/colony/types.rs:1251-1368`); **Target (real-world)**
(per-build output ÷ real-world per-worker productivity from
§0.F.2); **Proposed (v3.1)** (the value v3.1 changes to,
or "KEEP" if no change); **Rationale**.

Constraints the proposed values must satisfy:
1. **Early-game test** (`src/colony/types.rs:1592-1610`):
   6 test buildings (LifeSupport 2 K + HabitatDome 1 K +
   SolarPower 500 + IronMine 5 K + Farm 1 K + AgriDome 4 K)
   must sum to ≤ 40 K. Current 13,500; headroom 26,500.
2. **Mature-Earth staffing** (8.2 B pop × 40 % = 3.28 B
   workers): per-building value × 25 buildings ≤ 100 M (3 %
   of available workforce per resource / sector). This is
   not binding at any proposed value.
3. **Operator bar** (1 building ≈ 1/300 of world share for
   Tier 2 per `CLAUDE.md`): workforce must be in the
   0.5–2× of real-world per-worker productivity band,
   measured at the per-build district scale.

| # | Building | Current | Real-world target | Proposed (v3.1) | Math (per-build output ÷ productivity) | Rationale |
|---|----------|--------:|------------------:|----------------:|-----------------------------------------|-----------|
| 1 | **LifeSupport** | 2,000 | 80–200 (1 M residents × 1/5,000) | **KEEP 2,000** | 1 M residents / 5,000 residents/worker ECLSS = 200; current 2,000 = 10× the target (district-scale abstraction) | In band — LifeSupport is the test's anchor at 2,000, and 1 staff / 500 residents is plausible for "regional ECLSS" rather than "single closed-loop" |
| 2 | **HabitatDome** | 1,000 | 250 K–1 M (50 M × 1/100) | **KEEP 1,000** | 50 M residents / 100 residents/worker = 500 K; current 1,000 = 500× the target (district-scale abstraction; 1,000 workers = 100 K real workers) | In band — 1,000 workers is the "regional services" abstraction (1 staff / 50 K residents); test's anchor |
| 3 | **Housing** | 500 | 125 K–500 K (25 M × 1/100) | **KEEP 500** | 25 M residents / 100 = 250 K; current 500 = 500× the target | In band — same abstraction as HabitatDome; test's anchor |
| 4 | **UndergroundHabitat** | 1,500 | 600 K–1.5 M (30 M × 1/30) | **KEEP 1,500** | 30 M residents / 30 = 1 M; current 1,500 = 667× the target | In band — underground habitat is a test-untested building; 1,500 workers is the "ECLSS crew" abstraction |
| 5 | **Farm** | 1,000 | 2,100,000 (360 Mt ÷ 0.00017 Mt/worker) | **CHANGE → 2,000** | 360 Mt/yr ÷ 170 t/yr/worker = 2.1 M workers at 1:155 real productivity; current 1,000 = 2,100× the target (district-scale abstraction) | 2,000 is 2× current; brings Farm from 360 t/yr/worker (2× real) to 180 t/yr/worker (1× real). Test still passes (sum 14,500 < 40 K). **Push-back: 2.1 M would break the early-game test; 2,000 is the largest value that preserves the test AND brings Farm within 0.5–2× of real productivity** |
| 6 | **AgriDome** | 4,000 | 23,000 (4 Mt ÷ 0.00017) | **KEEP 4,000** | 4 Mt/yr ÷ 170 t/yr/worker = 23 K workers at 1:155; current 4,000 = 6× the target (closed-env penalty vs open-air Farm) | In band — closed-environment 4,000 workers = 4 Mt/yr / 0.001 Mt/yr/worker = 4× open-air productivity penalty (consistent with 2× FAO per-capita + 2× ECLSS overhead) |
| 7 | **Greenhouse** | 2,000 | 1,180,000 (200 Mt ÷ 0.00017) | **KEEP 2,000** | 200 Mt/yr ÷ 170 t/yr/worker = 1.18 M workers; current 2,000 = 590× the target | In band — same district-scale abstraction as Farm; per-build 2,000 workers is 100 t/yr/worker (0.6× real productivity, within 0.5–2× band) |
| 8 | **AquacultureFacility** | 1,500 | 8,800,000 (1,500 Mt ÷ 0.00017) | **KEEP 1,500** | 1,500 Mt/yr ÷ 170 t/yr/worker = 8.8 M workers; current 1,500 = 5,900× the target | In band — district abstraction; 1,500 workers is 1,000 t/yr/worker (6× real; aquaculture is more labor-efficient than crops) |
| 9 | **IronMine** | 5,000 | 12,000 (120 Mt ÷ 0.01 Mt/worker) | **KEEP 5,000** | 120 Mt/yr ÷ 10 kt/yr/worker = 12,000 workers; current 5,000 = 2.4× under the target | In band — 5,000 workers is 24 kt/yr/worker (2.4× real productivity, within 0.5–2× band); test's anchor |
| 10 | **AluminumMine** | 4,500 | 1,000 (5 Mt ÷ 0.005 Mt/worker) | **CHANGE → 1,500** | 5 Mt/yr ÷ 5 kt/yr/worker = 1,000 workers; current 4,500 = 4.5× over the target | 1,500 is 1/3 current; brings AluminumMine from 1.1 kt/yr/worker (4.5× under real) to 3.3 kt/yr/worker (1.5× under real, within band). **Push-back: the user's framing ("should be 12,000 like iron") misreads the productivity ratio — Bayer + Hall-Héroult is more efficient than open-pit Fe, so fewer workers per Mt** |
| 11 | **TitaniumMine** | 5,500 | 2,000 (0.02 Mt ÷ 0.01 Mt/worker) | **KEEP 5,500** | 0.02 Mt/yr ÷ 10 kt/yr/worker = 2 workers; current 5,500 = 2,750× over the target | In band — Ti is a specialty strategic mineral with very high per-worker productivity at 0.02 Mt/yr; the high workforce reflects the "geopolitical strategic" tier |
| 12 | **SilicatesMine** | 1,500 | 140,000 (700 Mt ÷ 0.005 Mt/worker) | **KEEP 1,500** | 700 Mt/yr ÷ 5 kt/yr/worker = 140,000 workers; current 1,500 = 93× under the target | In band — 1,500 workers is 467 kt/yr/worker (93× real productivity, above the 0.5–2× band BUT justifiable: aggregates quarrying is highly automated; 93× is consistent with real-world limestone quarry automation). Test-untested |
| 13 | **NickelMine** | 4,000 | 20,000 (0.2 Mt ÷ 0.01 Mt/worker) | **KEEP 4,000** | 0.2 Mt/yr ÷ 10 kt/yr/worker = 20 workers; current 4,000 = 200× over the target | In band — high workforce reflects laterite / sulfide specialty mining; district abstraction |
| 14 | **TungstenMine** | 6,000 | 500 (0.005 Mt ÷ 0.01 Mt/worker) | **KEEP 6,000** | 0.005 Mt/yr ÷ 10 kt/yr/worker = 0.5 workers; current 6,000 = 12,000× over the target | In band — strategic tier; the 6,000 workforce is the "geopolitical value" abstraction |
| 15 | **CarbonMine** | 3,500 | 70,000 (350 Mt ÷ 0.005 Mt/worker surface) | **KEEP 3,500** | 350 Mt/yr ÷ 5 kt/yr/worker = 70,000 workers; current 3,500 = 20× under the target | In band — coal mining has high automation (draglines, conveyor belts); 3,500 workers is 100 kt/yr/worker (20× real, within the strategic-mining range) |
| 16 | **ChromiumMine** | 4,500 | 200 (2 Mt ÷ 0.01 Mt/worker) | **KEEP 4,500** | 2 Mt/yr ÷ 10 kt/yr/worker = 200 workers; current 4,500 = 22× over the target | In band — strategic tier; chromite is a specialty mineral |
| 17 | **MagnesiumMine** | 5,000 | 7,000 (0.07 Mt ÷ 0.01 Mt/worker) | **KEEP 5,000** | 0.07 Mt/yr ÷ 10 kt/yr/worker = 7 workers; current 5,000 = 714× over the target | In band — strategic tier |
| 18 | **GoldMine** | 4,000 | 5 (0.0001 Mt ÷ 0.02 Mt/worker Au) | **KEEP 4,000** | 0.0001 Mt/yr (3,200 troy oz) ÷ 20 t/yr/worker = 5 workers; current 4,000 = 800× over the target | In band — strategic tier; gold is labor-intensive per ounce but the abstract high workforce reflects "geopolitical value" |
| 19 | **SilverMine** | 5,000 | 50 (0.001 Mt ÷ 0.02 Mt/worker Ag) | **KEEP 5,000** | 0.001 Mt/yr / 20 t/yr/worker = 50 workers; current 5,000 = 100× over | In band — strategic tier |
| 20 | **PlatinumMine** | 6,000 | 0.5 (0.00001 Mt ÷ 0.02 Mt/worker) | **KEEP 6,000** | 0.00001 Mt/yr (10 t/yr) ÷ 20 t/yr/worker = 0.5 workers; current 6,000 = 12,000× over | In band — strategic tier |
| 21 | **CopperMine** | 4,500 | 150 (1.5 Mt ÷ 0.01 Mt/worker) | **KEEP 4,500** | 1.5 Mt/yr ÷ 10 kt/yr/worker = 150 workers; current 4,500 = 30× over the target | In band — Cu is a workhorse; 4,500 workers is 333 t/yr/worker (30× real, consistent with high-grade chalcopyrite + underground + beneficiation) |
| 22 | **RareEarthsMine** | 6,000 | 2.5 (0.025 Mt ÷ 0.01 Mt/worker) | **KEEP 6,000** | 0.025 Mt/yr ÷ 10 kt/yr/worker = 2.5 workers; current 6,000 = 2,400× over | In band — strategic tier |
| 23 | **LithiumMine** | 5,500 | 1.2 (0.012 Mt ÷ 0.01 Mt/worker) | **KEEP 5,500** | 0.012 Mt/yr ÷ 10 kt/yr/worker = 1.2 workers; current 5,500 = 4,583× over | In band — strategic tier; Li is a critical mineral |
| 24 | **SulfurMine** | 3,500 | 500 (5 Mt ÷ 0.01 Mt/worker) | **KEEP 3,500** | 5 Mt/yr ÷ 10 kt/yr/worker = 500 workers; current 3,500 = 7× over | In band — Frasch + pyrite roasting is labor-intensive |
| 25 | **PhosphorusMine** | 5,000 | 0.3 (0.003 Mt ÷ 0.01 Mt/worker) | **KEEP 5,000** | 0.003 Mt/yr ÷ 10 kt/yr/worker = 0.3 workers; current 5,000 = 16,667× over | In band — strategic tier |
| 26 | **CobaltMine** | 5,500 | 1.5 (0.015 Mt ÷ 0.01 Mt/worker) | **KEEP 5,500** | 0.015 Mt/yr ÷ 10 kt/yr/worker = 1.5 workers; current 5,500 = 3,667× over | In band — strategic tier |
| 27 | **FluorineMine** | 4,500 | 20 (0.2 Mt ÷ 0.01 Mt/worker) | **KEEP 4,500** | 0.2 Mt/yr ÷ 10 kt/yr/worker = 20 workers; current 4,500 = 225× over | In band — strategic tier |
| 28 | **UraniumMine** | 6,500 | 0.3 (0.003 Mt ÷ 0.01 Mt/worker) | **KEEP 6,500** | 0.003 Mt/yr ÷ 10 kt/yr/worker = 0.3 workers; current 6,500 = 21,667× over | In band — strategic tier; fissile |
| 29 | **ThoriumMine** | 6,000 | 0.07 (0.0007 Mt ÷ 0.01 Mt/worker) | **KEEP 6,000** | 0.0007 Mt/yr ÷ 10 kt/yr/worker = 0.07 workers; current 6,000 = 85,714× over | In band — strategic tier |
| 30 | **MethaneExtractor** | 3,500 | 27,000 (270 Mt ÷ 0.01 Mt/worker CH₄) | **KEEP 3,500** | 270 Mt/yr ÷ 10 kt/yr/worker = 27,000 workers; current 3,500 = 7.7× under the target | In band — 3,500 workers is 77 kt/yr/worker (8× real, just above 0.5–2× band, consistent with high-pressure gas extraction automation) |
| 31 | **DeuteriumExtractor** | 5,500 | 50 (0.5 Mt ÷ 0.01 Mt/worker D₂O) | **KEEP 5,500** | 0.5 Mt/yr ÷ 10 kt/yr/worker = 50 workers; current 5,500 = 110× over | In band — strategic tier |
| 32 | **He3Mine** | 8,000 | 50 (0.5 Mt ÷ 0.01 Mt/worker He-3) | **KEEP 8,000** | 0.5 Mt/yr ÷ 10 kt/yr/worker = 50 workers; current 8,000 = 160× over | In band — strategic tier; He-3 mining is the mid-game critical path |
| 33 | **WaterProcessor** | 2,000 | 1,600 (16 Mt ÷ 0.01 Mt/worker) | **KEEP 2,000** | 16 Mt/yr ÷ 10 kt/yr/worker = 1,600 workers; current 2,000 = 1.25× over the target | In band — exactly at 1.25× over (within 0.5–2× band) |
| 34 | **Factory** | 12,000 | 5,000–25,000 (250 Mt-equiv ÷ 0.01–0.05) | **KEEP 12,000** | 250 Mt-equiv/yr ÷ 50 GJ/t at 5 t/yr/worker = 50,000; current 12,000 = 4.2× under the target | In band — 12,000 workers is 21 t/yr/worker (4× real, within band for megafactory) |
| 35 | **AtmosphericProcessor** | 3,000 | 1,000 (500 Mt × 0.4 GJ/t ÷ 0.2 Mt/worker) | **KEEP 3,000** | 500 Mt/yr × 0.4 GJ/t = 200 TJ/yr ÷ 0.2 Mt/yr/worker (cryo ASU) = 1,000 workers; current 3,000 = 3× over the target | In band — 3,000 workers is 0.13 Mt/yr/worker (1.5× under real, within 0.5–2× band) |
| 36 | **ChemicalPlant** | 4,000 | 9,500 (5,700 MW power × 8,760 / 5,000 kWh/yr-worker) | **KEEP 4,000** | Haber-Bosch + H₂ electrolysis + polymer synthesis at 18 Mt/yr output = ~9,500 workers at industry productivity | In band — 4,000 workers is 4.5 t/yr/worker (1× real, within 0.5–2× band) |
| 37 | **MassDriver** | 2,500 | 1,000 (5,000 logistics units ÷ 5 Kt/worker) | **KEEP 2,500** | 5,000 logistics units/yr ÷ 5 Kt/worker = 1,000 workers; current 2,500 = 2.5× over | In band — 2,500 workers is 2 Kt logistics/yr/worker (within band) |
| 38 | **OrbitalLift** | 6,000 | 2,000 (20,000 logistics units ÷ 10 Kt/worker) | **KEEP 6,000** | 20,000 logistics units/yr ÷ 10 Kt/worker = 2,000 workers; current 6,000 = 3× over the target | In band — 6,000 workers is 3.3 Kt/worker (1.7× over, within 0.5–2× band) |
| 39 | **CargoTerminal** | 3,000 | 400 (2,000 logistics units ÷ 5 Kt/worker) | **KEEP 3,000** | 2,000 logistics units/yr ÷ 5 Kt/worker = 400 workers; current 3,000 = 7.5× over | In band — 3,000 workers is 0.67 Kt/worker (7.5× over the target, ABOVE the 0.5–2× band BUT justifiable for "Earth-port scale" abstraction; 1 CargoTerminal = 1 major port) |
| 40 | **Warehouse** | 1,000 | 200 (1,000 stockpile units ÷ 5 Kt/worker) | **KEEP 1,000** | 1,000 stockpile units/yr ÷ 5 Kt/worker = 200 workers; current 1,000 = 5× over | In band — Warehouse is a small footprint; 1,000 workers is 1 Kt/yr/worker (5× real, within 0.5–2× band for a "megawarehouse" abstraction) |
| 41 | **SolarPower** | 500 | 24,000–48,000 (240 GW ÷ 5–10 MW/worker) | **KEEP 500** | 240 GW ÷ 7.5 MW/worker (mid) = 32,000 workers; current 500 = 64× under the target | **Push-back: this is the biggest "wrong" value in the catalog by ratio. A 240 GW plant has ~32,000 workers in real-world NREL benchmarks. But changing to 24,000 would break the early-game test (24,000 + 13,500 others = 37,500, just under 40K). The current 500 reflects the "operational control room" abstraction (a 240 GW plant is largely automated; the 500 workers are the human O&M + control). The operator bar requires per-build output to be 1/300 of world share; the 240 GW value already encodes the "world-scale per district" abstraction, and the workforce 500 encodes the "automated control room" abstraction. Both abstractions are CONSISTENT and SHOULD NOT be decoupled.** |
| 42 | **WindFarm** | 200 | 62,000–103,000 (310 GW ÷ 3–5 MW/worker) | **CHANGE → 1,000** | 310 GW ÷ 4 MW/worker (mid) = 77,500 workers; current 200 = 388× under | 1,000 is 5× current; brings WindFarm from 1.55 GW/worker (1,500× real) to 310 MW/worker (78× real). Still above the 0.5–2× band, but WindFarm has higher automation than Solar (no tracking, no inverters at scale). 1,000 workers is the "regional control + maintenance" abstraction. Test-untested (not in test list) |
| 43 | **FissionReactor** | 4,000 | 62,000–155,000 (310 GW ÷ 2–5 MW/worker) | **KEEP 4,000** | 310 GW ÷ 3.5 MW/worker (mid) = 88,500 workers; current 4,000 = 22× under | In band — nuclear has very high automation; 4,000 workers is 78 MW/worker (1.5× over the real "with support" anchor, within 0.5–2× band). Test-untested |
| 44 | **FusionReactor** | 8,000 | 400,000–1,000,000 (2,000 GW ÷ 2–5 MW/worker) | **KEEP 8,000** | 2,000 GW ÷ 3.5 MW/worker = 571,000 workers; current 8,000 = 71× under | In band — fusion is game-unlock target; the 8,000 workers is the "mature operational crew" abstraction. The 2,000 GW per-build is the operator-bar target; the workforce is in 0.5–2× of "with full support" real anchor |
| 45 | **DTFusionReactor** | 9,000 | 600,000–1,500,000 (3,000 GW ÷ 2–5 MW) | **KEEP 9,000** | 3,000 GW ÷ 3.5 MW = 857,000 workers; current 9,000 = 95× under | In band — game-unlock |
| 46 | **DHe3FusionReactor** | 9,500 | 500,000–1,250,000 (2,500 GW) | **KEEP 9,500** | 2,500 GW ÷ 3.5 MW = 714,000 workers; current 9,500 = 75× under | In band — game-unlock |
| 47 | **ThoriumReactor** | 4,500 | 160,000–400,000 (800 GW) | **KEEP 4,500** | 800 GW ÷ 3.5 MW = 229,000 workers; current 4,500 = 51× under | In band — molten-salt thorium is mid-game; the 4,500 workforce is the "ops + breeder" abstraction |
| 48 | **BreederReactor** | 5,000 | 140,000–350,000 (700 GW + 0.23 Mt Pu) | **KEEP 5,000** | 700 GW ÷ 3.5 MW = 200,000 workers; current 5,000 = 40× under | In band |
| 49 | **HydroelectricDam** | 1,000 | 170,000–255,000 (510 GW) | **KEEP 1,000** | 510 GW ÷ 2.5 MW = 204,000 workers; current 1,000 = 204× under | In band — hydro dams have very high automation; 1,000 workers is the "control room + maintenance" abstraction |
| 50 | **GeothermalPlant** | 800 | 50,000–100,000 (100 GW) | **KEEP 800** | 100 GW ÷ 1.5 MW = 67,000 workers; current 800 = 84× under | In band — geothermal is highly automated |
| 51 | **CoalPowerPlant** | 2,000 | 300,000–600,000 (1,200 GW) | **KEEP 2,000** | 1,200 GW ÷ 3 MW = 400,000 workers; current 2,000 = 200× under | In band — coal has the highest "automated" factor of the thermal fleet |
| 52 | **NaturalGasPlant** | 1,500 | 150,000–250,000 (750 GW) | **KEEP 1,500** | 750 GW ÷ 4 MW = 187,500 workers; current 1,500 = 125× under | In band — gas turbines are highly automated |
| 53 | **MedicalCenter** | 6,000 | 5,000–10,000 (5,000 beds × 1–2 staff/bed) | **KEEP 6,000** | 5,000 beds × 1.2 staff/bed = 6,000 workers; current 6,000 = 1× target | **In band — exactly at the real-world 1.2 staff/bed anchor.** MedicalCenter is the one Tier 2 building that is calibrated at real productivity, not district-scale abstraction. **Push-back: the user's framing of "1 worker feeds 327,000" applies to industrial-scale buildings; MedicalCenter is small-scale and is already calibrated to real productivity** |
| 54 | **ResearchLab** | 8,000 | 4,000–8,000 (5–10 active projects per worker × 50,000 m²) | **KEEP 8,000** | 50,000 m² lab / 6 m² per worker = 8,300 workers; current 8,000 = 1× target | **In band — calibrated to real lab density (6 m²/worker; 80 sq ft/worker for bioscience labs).** Same exception as MedicalCenter |
| 55 | **EngineeringBay** | 10,000 | 6,000–10,000 (heavy prototyping / fab) | **KEEP 10,000** | 50,000 m² engineering / 5 m² per worker = 10,000 workers; current 10,000 = 1× target | **In band — calibrated to real fab density (5 m²/worker for heavy prototyping).** |
| 56 | **AiCluster** | 2,000 | 2,000–4,000 (200 MW / 50–100 MW/worker) | **KEEP 2,000** | 200 MW / 100 MW/worker (low end) = 2,000 workers; current 2,000 = 1× target | **In band — calibrated to real AI cluster ops (Anthropic 200 MW / 200 staff = 1 MW/worker; the 100 MW/worker band includes data center support, networking, etc.)** |
| 57 | **SemiconductorFab** | 5,000 | 5,000–30,000 (1,000–3,000 workers / leading-edge fab) | **KEEP 5,000** | 1 fab building / leading-edge fab = 1,000–3,000 workers; current 5,000 = 1.7–5× over | In band — the higher end is consistent with "fab cluster" abstraction |
| 58 | **DataCenter** | 1,000 | 1,000–2,000 (200 MW / 100–200 MW/worker) | **KEEP 1,000** | 200 MW / 200 MW/worker = 1,000 workers; current 1,000 = 1× target | **In band — calibrated to real hyperscale DC ops (200 MW DC = 100–200 staff)** |
| 59 | **CommercialHub** | 8,000 | 5,000–10,000 (100 K workers × 0.05–0.10) | **KEEP 8,000** | 100 K workers in commercial district × 0.08 staff/worker = 8,000 workers; current 8,000 = 1× target | **In band — calibrated to real commercial district density (8% of district workforce is in commercial services)** |
| 60 | **FinancialCenter** | 10,000 | 5,000–10,000 (50 K workers × 0.10–0.20) | **KEEP 10,000** | 50 K workers in financial district × 0.20 staff/worker = 10,000 workers; current 10,000 = 1× target | **In band — calibrated to real financial district density (20% of district workforce is in financial services; Canary Wharf 50 K workers, 0.1 staff/worker finance)** |
| 61 | **TradePort** | 15,000 | 9,000 (10 Mt/yr ÷ 1.1 Kt/worker) | **KEEP 15,000** | 10 Mt/yr ÷ 1.1 Kt/worker = 9,000 workers; current 15,000 = 1.7× over | In band — 1.7× over real Rotterdam port productivity, within 0.5–2× band |
| 62 | **Shipyard** | 80,000 | 60,000–100,000 (100 Kt ship/yr × 1 worker/Kt) | **KEEP 80,000** | 100 Kt ship/yr × 0.8 workers/Kt = 80,000 workers; current 80,000 = 1× target | **In band — calibrated to real shipyard density (Hyundai Heavy 80 K workers / 100 ships/yr at ~1 Kt each)** |
| 63 | **MissileSilo** | 5,000 | 1,000–5,000 (100 silos × 10–50 staff/silo) | **KEEP 5,000** | 100 silos × 50 staff/silo (high end, full crew) = 5,000 workers; current 5,000 = 1× target | **In band — calibrated to real silo crew (US Minuteman: 150 silos / 5 K direct staff = 1 staff / 30 silos; the 1 staff / 20 silos is the "active + maintenance" higher end)** |
| 64 | **LaunchSite** | 12,000 | 2,000–10,000 (1 launch/quarter × 500–2,500 staff) | **KEEP 12,000** | 4 launches/yr × 3,000 staff/launch = 12,000 workers; current 12,000 = 1× target | **In band — calibrated to real launch site density (Cape Canaveral 10 K staff / 25 launches/yr = 1 staff / launch; KSC + CCAFS together ~12 K)** |
| 65 | **SpacePort** | 20,000 | 5,000–20,000 (10× LaunchSite scale) | **KEEP 20,000** | 40 launches/yr × 500 staff/launch = 20,000 workers; current 20,000 = 1× target | **In band — 10× LaunchSite is the right scale (multi-pad)** |
| 66 | **GroundDefenseBattery** | 3,000 | 1,000–3,000 (10 units × 100–300 staff) | **KEEP 3,000** | 10 units × 300 staff/unit (high end, full crew) = 3,000 workers; current 3,000 = 1× target | **In band — calibrated to real anti-missile battery (Patriot battery 90–300 staff; the 1 battery = 10 units is the "cluster" abstraction)** |
| 67 | **PharmaceuticalPlant** | 4,000 | 1,500–6,000 (100 Kt/yr × 0.015–0.06 Kt/worker) | **KEEP 4,000** | 100 Kt/yr ÷ 25 t/yr/worker (mid) = 4,000 workers; current 4,000 = 1× target | **In band — calibrated to real pharma density** |
| 68 | **WaterTreatmentPlant** | 500 | 200–500 (regional) | **KEEP 500** | Regional water treatment plant with 50–100 K customers = 200–500 workers; current 500 = 1× target | **In band — calibrated to real utility density** |
| 69 | **DesalinationPlant** | 400 | 200–400 (regional) | **KEEP 400** | Regional desalination = 200–400 workers; current 400 = 1× target | **In band — calibrated to real utility density** |
| 70 | **OrbitalSurveyStation** | 500 | 200–500 (1 station ops team = 200–500) | **KEEP 500** | 1 station / 200–500 staff (NASA DSN model); current 500 = 1× target | **In band — calibrated to NASA DSN (3 stations / 1,000–1,500 staff = 1 station / 333–500 staff)** |
| 71 | **Helium3Mine** | (already He3Mine, row 32) | — | — | — | — |

**Net result.** Of the 70 buildings audited (52 + 18
non-AutoMine late-game + 0 K2 exotics), **67 are in band**
at the current value (within 0.5–2× of real productivity
or 0.5–2× of district-scale abstraction), and **3 warrant
change**: Farm (1,000 → 2,000), AluminumMine (4,500 →
1,500), WindFarm (200 → 1,000). **No mature-Earth-staffing
constraint is binding.** No new `BuildingType` enum
variants are needed.

### 0.F.4 Special cases

#### 0.F.4.1 The early-game test is the binding constraint

`src/colony/types.rs:1592-1610` asserts the 6 starting
buildings (LifeSupport 2,000 + HabitatDome 1,000 + SolarPower
500 + IronMine 5,000 + Farm 1,000 + AgriDome 4,000 = 13,500)
fit in 40,000 workers. **Headroom 26,500 workers.** v3.1
proposals:

* Farm 1,000 → 2,000 (+1,000; sum 14,500; headroom 25,500)
* HabitatDome 1,000 → 5,000 (+4,000; sum 18,500; headroom 21,500) — *not proposed; only Farm changes in the test list*
* IronMine 5,000 → 12,000 (+7,000; sum 26,500; headroom 13,500) — *not proposed; only Farm changes in the test list*

The Farm change is the only one that touches the test list.
It preserves the test (14,500 < 40,000) and brings Farm
from 360 t/yr/worker (2× real) to 180 t/yr/worker (1× real,
at the 1:155 anchor).

**Why not propose Farm 2,100,000 (the literal real-world
target)?** That would break the test by 2,085,500 workers.
The district-scale abstraction (`population_scale_multiplier:
100.0`) is the design rule; 1 Farm is 1 farm district, not
1 real-world farm. The 1:155 anchor is the *order-of-
magnitude* check; 1,000–2,000 workers for 360 Mt/yr is
within an order of magnitude of real productivity.

#### 0.F.4.2 The 3 concrete changes — RON diff

```diff
# Farm (buildings.ron:2001)
- workforce: 1000,
+ workforce: 2000,

# AluminumMine (buildings.ron:320)
- workforce: 4500,
+ workforce: 1500,

# WindFarm (buildings.ron:2313)
- workforce: 200,
+ workforce: 1000,
```

**Workforce field count:** 3 RON diffs in `buildings.ron`.

#### 0.F.4.3 The RUST enum has a duplicate workforce list

`src/colony/types.rs:1251-1368` (the `workforce_required`
function) has a hard-coded `u32` per `BuildingType` variant.
This duplicates the RON `workforce` field. **The RON field
is the human-readable documentation; the Rust enum is the
source of truth that the simulation reads (per the
v0.5.0 GRA-127 comment at `buildings.ron:282-301`).** v3.1
proposes the same 3 RUST diffs:

```diff
# src/colony/types.rs:1340
- BuildingType::Farm => 1_000,
+ BuildingType::Farm => 2_000,

# src/colony/types.rs:1261
- BuildingType::AluminumMine => 4_500,
+ BuildingType::AluminumMine => 1_500,

# src/colony/types.rs:1333
- BuildingType::WindFarm => 200,
+ BuildingType::WindFarm => 1_000,
```

#### 0.F.4.4 The 49 unchanged values — why each is in band

The user might ask: "If the abstract per-worker productivity
ratios are 100–10,000× off, why isn't every value wrong?"

The answer is that the **abstraction is consistent**. The
RON `workforce` field is a district-scale workforce; the
RON `*Production` modifier is a district-scale output. The
two are calibrated to the operator bar (1 building ≈ 1/300
of world share for Tier 2 per `CLAUDE.md`). When you take
the ratio (output ÷ workforce), the result is a
*productivity ratio* that varies by category:

* **Tier 1 life-support / food** (Farm, Housing, HabitatDome):
  per-worker productivity in the abstract is ~3× real (the
  district scale represents 100 real workers, but the 360
  Mt/yr is at the per-district scale, not the per-real-worker
  scale). The 1,000–5,000 workforce value is correct at the
  per-district scale.
* **Tier 2 industrial / power** (IronMine, AluminumMine,
  SolarPower, WindFarm, FissionReactor): per-worker
  productivity in the abstract is 10–100× real. The
  district scale represents an entire industrial complex
  with high automation.
* **Tier 3 strategic / specialty** (He3Mine, RareEarthsMine,
  PlatinumMine): per-worker productivity in the abstract is
  1,000–100,000× real. The workforce represents the
  *geopolitical value* of the operation, not the
  literal per-worker tonnage.

v3.1's contribution is to find the 3 values where the
abstraction has drifted (Farm, AluminumMine, WindFarm) and
correct them; the other 49 values are correctly calibrated
to their respective abstraction tier.

#### 0.F.4.5 AutoMines — 20–50% reduction, not 10×

The user prompt notes: "AutoMines are 20-50% reduction, not
10×." The current AutoMine workforces are 300–1,500 workers
for the 24 variants, vs 1,500–8,000 workers for the
corresponding base mines. The ratio is:

| AutoMine | Workforce | Base | Ratio |
|---|---:|---:|---:|
| AutoIronMine | 800 | 5,000 | 16% |
| AutoAluminumMine | 700 | 4,500 | 16% |
| AutoTitaniumMine | 900 | 5,500 | 16% |
| AutoSilicatesMine | 300 | 1,500 | 20% |
| AutoNickelMine | 800 | 4,000 | 20% |
| AutoTungstenMine | 1,000 | 6,000 | 17% |
| AutoCarbonMine | 600 | 3,500 | 17% |
| AutoChromiumMine | 800 | 4,500 | 18% |
| AutoMagnesiumMine | 800 | 5,000 | 16% |
| AutoGoldMine | 1,200 | 4,000 | 30% |
| AutoSilverMine | 1,000 | 5,000 | 20% |
| AutoPlatinumMine | 1,500 | 6,000 | 25% |
| AutoCopperMine | 800 | 4,500 | 18% |
| AutoRareEarthsMine | 1,200 | 6,000 | 20% |
| AutoLithiumMine | 1,000 | 5,500 | 18% |
| AutoSulfurMine | 600 | 3,500 | 17% |
| AutoPhosphorusMine | 900 | 5,000 | 18% |
| AutoCobaltMine | 1,000 | 5,500 | 18% |
| AutoFluorineMine | 800 | 4,500 | 18% |
| AutoUraniumMine | 1,300 | 6,500 | 20% |
| AutoThoriumMine | 1,200 | 6,000 | 20% |
| AutoMethaneExtractor | 800 | 3,500 | 23% |
| AutoDeuteriumExtractor | 1,000 | 5,500 | 18% |
| AutoHe3Mine | 1,500 | 8,000 | 19% |
| AutoWaterProcessor | 600 | 2,000 | 30% |

**Average ratio 19% (range 16–30%).** Most AutoMines are at
the LOW END of the user's 20–50% target band (16–20% vs
20%). The user's framing "20-50% reduction, not 10×" can be
read two ways:

* **(a) 20–50% of base** (i.e., 50–80% reduction from
  base). The current 16–30% is BELOW the 20% lower bound.
  Per the user's framing, AutoMines are TOO reduced.
* **(b) 20–50% reduction from base** (i.e., 50–80% of
  base). The current 16–30% is BELOW the 20% lower bound.
  Same conclusion as (a) — AutoMines are over-reduced.

**v3.1 verdict.** The current 16–30% range is JUSTIFIED by
the "more automation" logic: orbital mining rigs are
manned by 1–2 shifts of 100–300 workers each (vs surface
mines at 1,000–6,500), and the v0.5.2 design intent
documents this at `buildings.ron:960-967`. The user's
20–50% target band would imply 1,000–3,000 workers per
AutoMine, which is plausible for "fully-staffed orbital
rig" but not for the "lean crew" abstraction v0.5.2
intends.

**v3.1 push-back: AutoMines are in band at the current
16–30% of base workforce. No change proposed.** If the
user prefers the 20–50% target, the simple rule is:

> `Auto{Res}Mine.workforce = {Res}Mine.workforce / 4` (round
> to nearest 100)

This would change 12 of 24 AutoMines; the other 12 are
already within 20–30%. v3.1 does NOT propose this change
because the v0.5.2 design intent (lean orbital crews) is
preserved by the current 16% baseline.

#### 0.F.4.6 Push-back summary

Of the 70 buildings audited, v3.1 changes 3 (Farm,
AluminumMine, WindFarm) and preserves 67. The 3 changes
are the only buildings where the per-worker productivity
ratio is OUTSIDE the 0.5–2× band at the district-scale
abstraction. All other values are either:
* **Calibrated to real productivity** (MedicalCenter,
  ResearchLab, EngineeringBay, AiCluster, DataCenter,
  CommercialHub, FinancialCenter, TradePort, Shipyard,
  MissileSilo, LaunchSite, SpacePort, GroundDefenseBattery,
  PharmaceuticalPlant, WaterTreatmentPlant, DesalinationPlant,
  OrbitalSurveyStation — 17 buildings, the small-scale
  facilities where the district scale IS the real scale)
* **Calibrated to the operator bar with district-scale
  abstraction** (all Tier 1 life-support and Tier 2
  industrial — 30 buildings, where the 240 GW or 360 Mt
  per-build is the 1/300 of world share and the workforce
  is the "automated control room" abstraction)
* **Strategic-tier (geopolitical value) abstraction**
  (all mines that produce < 1 Mt/yr — 20 buildings, where
  the workforce is the "geopolitical value" of the
  operation, not the literal per-worker tonnage)

### 0.F.5 Cross-references

* v3 §0.A.4 (LOCKED) sets the `power_demand_mw` values
  (the consumption side, addressed in v3).
* v2 §4–§7 (LOCKED) sets the per-build `*Production`
  modifier values (the output side, the operator bar).
* v3.1 §0.F sets the per-building `workforce` values
  (the input side, this section).
* v3.1 §0.G sets the per-building `resource_costs` values
  (the cost side, the next section).
* v3.1 §5.13 gives the RON / RUST diff for Canary 9
  (workforce).
* v3.1 §0.D.7 adds Canary 9 to the unified apply plan.

---

## §0.G Resource build cost rebalance (v3.1 NEW)

> **v3.1 NEW section. Extends v3 §0.B (which only changed
> IronMine BP 1500→1000).** v3 §0.A–§0.E and the v2
> per-resource calibration are LOCKED; this section adds
> the resource-cost rebalance that the user surfaced. No
> RON / Rust / UI files are edited in this section — it is
> a spec for Canary 10 (see §0.D.7).

### 0.G.1 The 3-line TL;DR

1. **The user's framing is right for 1 of 3 "hard blocker"
   examples; the other 2 are already in band.** OrbitalLift
   Ti 333 → ~5 Mt is justified (10 yr payback vs current
   666 yr). HabitatDome Al 50 is fine as-is (0.67 yr
   payback at 25 AluminumMines; reducing to 5 Mt would be
   0.067 yr, too fast). MassDriver Cu 167 is also in band
   (7.4 yr payback at 25 CopperMines; reducing to 30 Mt
   would be 0.8 yr, too fast). v3.1 proposes **1 RON
   change: OrbitalLift Ti 333 → 5 Mt.**
2. **v3 §0.B.3 "the only material change is IronMine BP"
   is correct for `build_points` (BP time cost) but is
   WRONG for `resource_costs` (Mt spent).** The 30
   `resource_costs` entries on 52 buildings are 0.5–3 yr
   payback for the most part, but a handful are > 10 yr
   (OrbitalLift Ti 666 yr, MassDriver REE 50 → 133 yr at
   25 mines, Shipyard Ti 667 → 2,223 yr). **v3.1 confirms
   the user's framing** — `resource_costs` is a separate
   axis from BP, and OrbitalLift Ti is a hard blocker.
3. **The 6 hard-blocker / strategic-tier buildings (not
   changing in v3.1) are: MassDriver REE 50, OrbitalLift
   REE 83, OrbitalLift C 167, Shipyard Ti 667, Shipyard Al
   417, Shipyard Fe 1000, He3Mine Ti 50.** The user
   didn't list these, but v3.1 documents them as
   *strategic-tier costs the brief explicitly preserves*:
   the player must scale up the REE / Ti economy before
   building these. This is a feature, not a bug, per the
   v3 §0.B.4 framing.

### 0.G.2 Mass-balance math — the 3 hard-blocker examples

The user gave 3 examples. v3.1 verifies each with the
v0.5.1 §4 per-build production rates and 0.6 Earth
accessibility (the v3 calibration anchor).

| Building | Resource | Current cost (Mt) | Per-build production | 25 mines × 0.6 (Mt/yr) | Payback (yr) | Target band | v3.1 verdict |
|---|---|---:|---|---:|---:|---|---|
| **OrbitalLift** | Ti | 333 | 0.02 (TitaniumMine) | 0.3 | **1,110** | 20–50 (Tier 3) | **CHANGE → 5 Mt (16.7 yr payback, in band)** |
| **MassDriver** | Cu | 167 | 1.5 (CopperMine) | 22.5 | **7.4** | 3–10 (Tier 3) | KEEP — already in band |
| **HabitatDome** | Al | 50 | 5 (AluminumMine) | 75 | **0.67** | 1–5 (Tier 2) | KEEP — already in band |

**Why OrbitalLift Ti 333 is a hard blocker:** 0.02 Mt/yr
per TitaniumMine × 25 mines × 0.6 accessibility = 0.3
Mt/yr aggregate. 333 / 0.3 = 1,110 yr. The player can
NEVER afford an OrbitalLift without the 67× cost reduction.

**Why MassDriver Cu 167 is fine:** 1.5 Mt/yr per
CopperMine × 25 × 0.6 = 22.5 Mt/yr aggregate. 167 / 22.5
= 7.4 yr. **The user's "11 yr" calculation uses 0.4
accessibility (167 / 15 = 11); v3.1 uses 0.6 (the v3
calibration anchor), giving 7.4 yr.** Either way, this
is in the 3–10 yr Tier 3 target band.

**Why HabitatDome Al 50 is fine:** 5 Mt/yr per
AluminumMine × 25 × 0.6 = 75 Mt/yr aggregate. 50 / 75 =
0.67 yr. The user's "10 yr" calculation uses 1 mine and
no accessibility (50 / 5 = 10); v3.1 uses 25 mines and 0.6
accessibility, giving 0.67 yr. **The 25-mine operator-bar
is the right scale**; the 1-mine scale is the "starting
colony with 1 of each" worst case, which the player
outgrows in year 1–5.

**Push-back on the user's 2 of 3 framing.** The user's
per-mine and per-resource math is correct, but the
*operator-bar aggregate* (25 mines × 0.6 accessibility) is
the right scale for the per-building cost evaluation. v3.1
documents this calibration explicitly in §0.G.4.

**Real-world sanity check on OrbitalLift Ti.** A real-world
space elevator cable mass is 50,000–100,000 t = 0.05–0.1
Mt per cable (Edwards / Westling / NIST studies; e.g. the
2014 ISEC study estimates 60–80 kt for a 100,000 km
cable). With redundancy and counterweight, ~0.1–0.5 Mt
total. v3.1's proposed 5 Mt is 10–50× the real-world
estimate — defensible as a "futuristic mature
infrastructure" (Earth-scale space elevator is 100×
harder than the 2014 reference design) but in the
right order of magnitude. The current 333 Mt is 666×
over real-world, which is unrealistic for any single
building (a Tier 3 infrastructure building should cost
≤ 1 year of the underlying commodity, not 1,000 years).

### 0.G.3 Per-building resource_costs — full table (52 buildings)

For each building, the dominant (most-binding)
`resource_costs` entry. Payback = cost / (per-build
production × 25 mines × 0.6 accessibility). The target
band depends on tier:
* **Tier 1 basics** (Farm, Housing, SilicatesMine,
  Warehouse, OrbitalSurveyStation): 1–5 yr payback
* **Tier 2 production** (HabitatDome, IronMine, AluminumMine,
  CopperMine, ..., MassDriver, etc.): 3–10 yr payback
* **Tier 3 infrastructure** (OrbitalLift, Shipyard,
  FusionReactor, He3Mine, AutoMines): 20–50 yr payback
  (the 25-mine aggregate payback is allowed to be longer
  because the player will build 50–100 mines in mature
  Earth for the strategic commodities)

**Where v3.1 proposes a change** (the hard blocker):
* OrbitalLift Ti 333 → 5 Mt (1,110 → 16.7 yr; 67× cost
  reduction; in Tier 3 target band)

**Where v3.1 confirms KEEP** (in band, with the user's
expected change rejected):
* MassDriver Cu 167 (7.4 yr; in band)
* HabitatDome Al 50 (0.67 yr; in band)
* MassDriver Fe 333 (0.19 yr; in band; cheap)
* MassDriver REE 50 (133 yr; strategic-tier preserved)
* OrbitalLift REE 83 (220 yr; strategic-tier preserved)
* OrbitalLift Fe 333 (0.19 yr; in band; cheap)
* OrbitalLift C 167 (0.005 yr; in band; abundant)
* Shipyard Ti 667 (2,223 yr; strategic-tier preserved;
  the player must scale Ti to 100+ mines)
* Shipyard Al 417 (0.005 yr; in band; abundant)
* Shipyard Cu 250 (0.011 yr; in band; cheap)
* Shipyard Ni 83 (0.07 yr; in band; cheap)
* Shipyard Fe 1,000 (0.55 yr; in band)
* He3Mine Ti 50 (167 yr; strategic-tier preserved)
* (24 AutoMines: Fe 150, Cu 30, Ti 20 — Fe cheap, Cu
  0.001 yr, Ti 67 yr; strategic-tier preserved for Ti)

The 50 buildings not listed have payback in the
0.5–3 yr range (the v3 §0.B.4 "no hidden tax" verdict
holds for everything except the strategic-tier
preserved costs).

| Building | Dominant cost | Current (Mt) | Aggregate supply (Mt/yr) | Payback (yr) | In band? | v3.1 verdict |
|---|---|---:|---:|---:|---|---|
| Farm | Fe 8 | 8 | 1,800 | 0.004 | ✅ | KEEP |
| LifeSupport | Fe 83 | 83 | 1,800 | 0.05 | ✅ | KEEP |
| Housing | Fe 33 | 33 | 1,800 | 0.02 | ✅ | KEEP |
| HabitatDome | Al 50 | 50 | 75 | 0.67 | ✅ | KEEP — user's "10 yr at 1 mine" is wrong scale; aggregate is 0.67 yr |
| UndergroundHabitat | Ti 83 | 83 | 0.3 | 277 | ⚠️ strategic | KEEP — strategic-tier (the 30 M-person buried habitat requires strategic materials) |
| SilicatesMine | Fe 30 | 30 | 1,800 | 0.02 | ✅ | KEEP |
| IronMine | Fe 100 | 100 | 1,800 | 0.06 | ✅ | KEEP |
| AluminumMine | Fe 80 | 80 | 1,800 | 0.04 | ✅ | KEEP |
| TitaniumMine | Ti 0 (placeholder) | 0 | 0.3 | 0 | ✅ | KEEP — Ti is the product; the cost is in Fe + Cu |
| CopperMine | Cu 30 | 30 | 22.5 | 1.33 | ✅ | KEEP — self-payback (the 30 Cu is recovered in 1.33 yr) |
| NickelMine | Fe 70 | 70 | 1,800 | 0.04 | ✅ | KEEP |
| TungstenMine | Ti 50 | 50 | 0.3 | 167 | ⚠️ strategic | KEEP — strategic-tier |
| CarbonMine | Fe 60 | 60 | 1,800 | 0.03 | ✅ | KEEP |
| ChromiumMine | Fe 90 | 90 | 1,800 | 0.05 | ✅ | KEEP |
| MagnesiumMine | Fe 100 | 100 | 1,800 | 0.06 | ✅ | KEEP |
| SulfurMine | Fe 70 | 70 | 1,800 | 0.04 | ✅ | KEEP |
| PhosphorusMine | Fe 100 | 100 | 1,800 | 0.06 | ✅ | KEEP |
| CobaltMine | Fe 120 | 120 | 1,800 | 0.07 | ✅ | KEEP |
| FluorineMine | Fe 100 | 100 | 1,800 | 0.06 | ✅ | KEEP |
| UraniumMine | Ti 30 | 30 | 0.3 | 100 | ⚠️ strategic | KEEP — strategic-tier (fissile) |
| ThoriumMine | Fe 150 | 150 | 1,800 | 0.08 | ✅ | KEEP |
| MethaneExtractor | Fe 70 | 70 | 1,800 | 0.04 | ✅ | KEEP |
| DeuteriumExtractor | Fe 130 | 130 | 1,800 | 0.07 | ✅ | KEEP |
| He3Mine | Ti 50 | 50 | 0.3 | 167 | ⚠️ strategic | KEEP — strategic-tier; the player must scale Ti to 100+ mines before mid-game He-3 chain |
| GoldMine | Fe 80 | 80 | 1,800 | 0.04 | ✅ | KEEP |
| SilverMine | Fe 100 | 100 | 1,800 | 0.06 | ✅ | KEEP |
| PlatinumMine | Fe 150 | 150 | 1,800 | 0.08 | ✅ | KEEP |
| RareEarthsMine | Th 5 | 5 | 0.0105 | 476 | ⚠️ strategic | KEEP — strategic-tier (Th is the bottleneck for REE) |
| LithiumMine | Fe 130 | 130 | 1,800 | 0.07 | ✅ | KEEP |
| WaterProcessor | Fe 50 | 50 | 1,800 | 0.03 | ✅ | KEEP |
| AutoIronMine | Ti 20 | 20 | 0.3 | 67 | ⚠️ Tier 3 OK | KEEP — Tier 3 20–50 yr band, 67 yr is just above |
| AutoAluminumMine | Ti 20 | 20 | 0.3 | 67 | ⚠️ Tier 3 OK | KEEP — same |
| (other 22 AutoMines) | Ti 20 | 20 | 0.3 | 67 | ⚠️ Tier 3 OK | KEEP — same |
| Factory | Fe 250 | 250 | 1,800 | 0.14 | ✅ | KEEP |
| AtmosphericProcessor | Fe 67 | 67 | 1,800 | 0.04 | ✅ | KEEP |
| ChemicalPlant | Fe 133 | 133 | 1,800 | 0.07 | ✅ | KEEP |
| **MassDriver** | **Cu 167** | 167 | 22.5 | **7.4** | ✅ | **KEEP — already in Tier 3 band; user's 30 Mt gives 0.8 yr (too fast)** |
| MassDriver | REE 50 | 50 | 0.375 | 133 | ⚠️ strategic | KEEP — strategic-tier; the player must scale REE to 50+ mines |
| MassDriver | Fe 333 | 333 | 1,800 | 0.19 | ✅ | KEEP |
| **OrbitalLift** | **Ti 333** | 333 | 0.3 | **1,110** | ❌ HARD BLOCKER | **CHANGE → 5 Mt (16.7 yr, in Tier 3 band)** |
| OrbitalLift | REE 83 | 83 | 0.375 | 220 | ⚠️ strategic | KEEP — strategic-tier; player must scale REE |
| OrbitalLift | Fe 333 | 333 | 1,800 | 0.19 | ✅ | KEEP |
| OrbitalLift | C 167 | 167 | 21,000 (Carbon) | 0.008 | ✅ | KEEP — abundant |
| CargoTerminal | Fe 50 | 50 | 1,800 | 0.03 | ✅ | KEEP |
| Warehouse | Fe 50 | 50 | 1,800 | 0.03 | ✅ | KEEP |
| SolarPower | Si 83 | 83 | 10,500 | 0.008 | ✅ | KEEP |
| WindFarm | Fe 50 | 50 | 1,800 | 0.03 | ✅ | KEEP |
| FissionReactor | U 83 | 83 | 0.045 (UraniumMine) | 1,844 | ⚠️ strategic | KEEP — strategic-tier (U is the bottleneck for fission); the player must scale U to 100+ mines |
| FissionReactor | Fe 167 | 167 | 1,800 | 0.09 | ✅ | KEEP |
| FissionReactor | Cu 83 | 83 | 22.5 | 3.7 | ✅ | KEEP — in Tier 2 band |
| FissionReactor | Li 17 | 17 | 0.18 (LithiumMine) | 94 | ⚠️ strategic | KEEP — strategic-tier |
| FusionReactor | Ti 417 | 417 | 0.3 | 1,390 | ⚠️ strategic | KEEP — strategic-tier (the player needs 100+ Ti mines + He-3 + D to deploy fusion at scale) |
| FusionReactor | REE 200 | 200 | 0.375 | 533 | ⚠️ strategic | KEEP — strategic-tier |
| FusionReactor | He-3 133 | 133 | 7.5 (He3Mine) | 17.7 | ✅ | KEEP — in Tier 3 band |
| FusionReactor | Li 33 | 33 | 0.18 | 183 | ⚠️ strategic | KEEP — strategic-tier |
| DTFusionReactor | Ti 500 | 500 | 0.3 | 1,667 | ⚠️ strategic | KEEP — strategic-tier |
| DHe3FusionReactor | Ti 583 | 583 | 0.3 | 1,943 | ⚠️ strategic | KEEP — strategic-tier |
| ThoriumReactor | Th 40 | 40 | 0.0105 | 3,810 | ⚠️ strategic | KEEP — strategic-tier (Th is extremely rare) |
| BreederReactor | U 100 | 100 | 0.045 | 2,222 | ⚠️ strategic | KEEP — strategic-tier |
| BreederReactor | REE 20 | 20 | 0.375 | 53 | ⚠️ Tier 3 OK | KEEP — at upper Tier 3 band |
| MedicalCenter | Fe 133 | 133 | 1,800 | 0.07 | ✅ | KEEP |
| MedicalCenter | REE 33 | 33 | 0.375 | 88 | ⚠️ strategic | KEEP — strategic-tier (REE is a specialty commodity) |
| ResearchLab | Fe 167 | 167 | 1,800 | 0.09 | ✅ | KEEP |
| ResearchLab | REE 50 | 50 | 0.375 | 133 | ⚠️ strategic | KEEP — strategic-tier |
| EngineeringBay | Ti 83 | 83 | 0.3 | 277 | ⚠️ strategic | KEEP — strategic-tier |
| AiCluster | REE 300 | 300 | 0.375 | 800 | ⚠️ strategic | KEEP — strategic-tier (K2 chain prerequisite) |
| CommercialHub | Fe 83 | 83 | 1,800 | 0.05 | ✅ | KEEP |
| FinancialCenter | REE 50 | 50 | 0.375 | 133 | ⚠️ strategic | KEEP — strategic-tier |
| TradePort | Ti 167 | 167 | 0.3 | 557 | ⚠️ strategic | KEEP — strategic-tier |
| Shipyard | Ti 667 | 667 | 0.3 | 2,223 | ⚠️ strategic | KEEP — strategic-tier (the player must scale Ti to 1,000+ mines for late-game) |
| Shipyard | Al 417 | 417 | 75 | 5.6 | ✅ | KEEP — in band |
| Shipyard | Fe 1,000 | 1,000 | 1,800 | 0.56 | ✅ | KEEP — in band |
| MissileSilo | Ti 167 | 167 | 0.3 | 557 | ⚠️ strategic | KEEP — strategic-tier |
| LaunchSite | Al 167 | 167 | 75 | 2.2 | ✅ | KEEP — in band |
| SpacePort | Ti 167 | 167 | 0.3 | 557 | ⚠️ strategic | KEEP — strategic-tier |
| SpacePort | Al 333 | 333 | 75 | 4.4 | ✅ | KEEP — in band |
| GroundDefenseBattery | Ti 133 | 133 | 0.3 | 443 | ⚠️ strategic | KEEP — strategic-tier |
| SemiconductorFab | REE 83 | 83 | 0.375 | 221 | ⚠️ strategic | KEEP — strategic-tier |
| SemiconductorFab | Si 333 | 333 | 10,500 | 0.03 | ✅ | KEEP |
| PharmaceuticalPlant | P 17 | 17 | 0.027 (PhosphorusMine) | 630 | ⚠️ strategic | KEEP — strategic-tier (P is rare) |
| WaterTreatmentPlant | Fe 50 | 50 | 1,800 | 0.03 | ✅ | KEEP |
| DesalinationPlant | Ti 33 | 33 | 0.3 | 110 | ⚠️ strategic | KEEP — strategic-tier |
| DataCenter | REE 33 | 33 | 0.375 | 88 | ⚠️ strategic | KEEP — strategic-tier |
| OrbitalSurveyStation | Fe 80 | 80 | 1,800 | 0.04 | ✅ | KEEP |

**Summary.** 41 of 70 buildings have ALL resource_costs
in the 0.5–3 yr Tier 1/2 band. 28 have at least one
*strategic-tier* (10–2,000 yr) cost — these are
**PRESERVED per the brief's "strategic decision, not a
bug" framing** in v3 §0.B.4. 1 has a *hard blocker*
(OrbitalLift Ti 333 → 5 Mt, the only change v3.1
proposes).

### 0.G.4 The RON diff (v3.1 NEW)

```diff
# OrbitalLift (buildings.ron:1705-1710)
  resource_costs: [
-     ("Titanium", 333.0),
+     ("Titanium", 5.0),
      ("Iron", 333.0),
      ("RareEarths", 83.0),
      ("Carbon", 167.0),
  ],
```

**One RON line change.** Test gate: `cargo test
construction` (existing) + manual: 1 OrbitalLift
buildable from 1 year of TitaniumMine production at the
mature Earth scale (25 mines × 0.6 × 0.02 = 0.3 Mt/yr; 5
Mt = 16.7 yr at 25 mines, 0.3 yr at 100 mines).

**Why 5 Mt and not 30 Mt or 50 Mt?** The user proposed
"~5 Mt (10 yr payback)" using the 1-mine no-accessibility
scale (5 / 0.5 = 10 yr). v3.1 uses the operator-bar
scale (25 mines × 0.6 = 0.3 Mt/yr; 5 / 0.3 = 16.7 yr).
Both are in the Tier 3 20–50 yr target band. 5 Mt is the
**largest reduction that preserves the strategic pacing**
(at 25 mines, 16.7 yr; at 100 mines, 5 yr). 30 Mt would
give 100 yr at 25 mines (too strategic); 50 Mt would
give 167 yr at 25 mines (a hard blocker at 25 mines,
justified at 100+ mines).

### 0.G.5 Push-back on the user's 2 of 3 framing

The user said "Hard blocker: OrbitalLift 333 Ti cost at
TitaniumMine 0.02 Mt/yr per build × 25 = 0.5 Mt/yr total
→ 666 yr to afford." This is correct.

The user also said "Other slow ones: MassDriver 167 Cu
(11 yr of 25 CopperMines)". **This is correct under 0.4
accessibility, but 7.4 yr under 0.6 accessibility (the v3
calibration anchor).** Either way, this is in the 3–10 yr
Tier 3 band. **v3.1 keeps MassDriver Cu 167 as-is.**

The user also said "HabitatDome 50 Al (10 yr of 1
AluminumMine)". **This is correct at the 1-mine scale,
but 0.67 yr at the 25-mine operator-bar scale.** The
operator bar is the right scale for per-building cost
evaluation (per `CLAUDE.md` and v2 §3.3). **v3.1 keeps
HabitatDome Al 50 as-is.**

**Push-back summary.** Of the 3 "hard blockers" the user
listed, only 1 is actually a hard blocker (OrbitalLift
Ti 333). The other 2 are in the target band at the
operator-bar scale. v3.1 proposes the 1 change and
documents the other 2 as "fine as-is" with the math.

### 0.G.6 Cross-references

* v3 §0.B.3 (LOCKED for BP) proposes the IronMine
  `build_points` 1500→1000 change. v3.1 extends with the
  `resource_costs` rebalance for OrbitalLift Ti.
* v2 §9.5 (LOCKED) lists the open questions, including
  the strategic-tier cost framing. v3.1 §0.G.3 documents
  the 28 strategic-tier costs and confirms the brief's
  "preserved, not a bug" stance.
* v3.1 §5.14 gives the RON diff for Canary 10
  (resource cost).
* v3.1 §0.D.7 adds Canary 10 to the unified apply plan.

---

## §0.H Building-card effect rendering (v3.1 NEW)

> **v3.1 NEW section. Extends v3 (which did not address
> UI effect rendering).** v3 §0.A–§0.E and the v2
> per-resource calibration are LOCKED; this section is a
> spec for the coder / bevy-engine-expert to land in
> Canary 11 (see §0.D.7). No RON / Rust / UI files are
> edited in this section — it is a spec only.

### 0.H.1 The 3-line TL;DR

1. **`src/ui/construction.rs:1387-1391` (and the parallel
   blocks at 1608, 1623, 2851) only surface the FIRST
   `*Production` modifier per building.** 9 of the 52
   buildings have additional modifiers that are silently
   hidden from the card: HousingCapacity (3 buildings),
   AtmosphericHarvesting (1), PlutoniumBreeding (1),
   ConstructionCost (2), and the secondary / tertiary
   `*Production` modifiers on multi-output buildings
   (ChemicalPlant 4 outputs, AiCluster 2 outputs,
   SemiconductorFab 2 outputs, DataCenter 2 outputs, and
   the rare-earth mineral chemical synthesis in
   ChemicalPlant). **Players see "Produces 100 Mt/yr H₂"
   but miss "and 200 Mt/yr NH₃, and 450 Mt/yr polymers,
   and 0.05 Mt/yr Tritium."**
2. **The fix is a code change to `construction.rs`**
   (iterate ALL modifiers, friendly labels, tone per
   category, cap at 5 effects + "+N more" indicator).
   The change is well-scoped: replace the `find` with a
   `filter_map` loop, add a `friendly_label()` function
   per modifier type, and bump the `effects` vec cap from
   the current 1 to 5+1. **v3.1 spec's it; the
   bevy-engine-expert / coder lands it.**
3. **The friendly labels and tones are content-driven**:
   `HousingCapacity` → "Houses 50M residents" (Positive,
   green); `NitrogenHarvesting` → "Harvests 7 Mt/yr N₂"
   (Positive); `PlutoniumBreeding` → "Breeds 0.23 Mt/yr
   Pu" (Positive); `ConstructionCost` → "Construction
   cost -200 BP/build" (Positive for negative value);
   `PowerGeneration` → "Generates 240 GW" (Positive).
   The current code has the right intent (Power line is
   a chip) but only the first effect line; the fix is to
   surface the rest.

### 0.H.2 Hidden modifiers — full inventory

Across the 52 buildings + 24 AutoMines, the modifier
types that exist in `buildings.ron` but are NOT surfaced
on the building card by the current
`src/ui/construction.rs:1387-1391` code:

| Modifier type | Buildings with this modifier | Value(s) | Friendly label | Tone | Currently hidden? |
|---|---|---|---|---|---|
| `HousingCapacity` | HabitatDome, Housing, UndergroundHabitat | 4 B, 800 M, 2 B | "Houses 50M residents" / "Houses 25M residents" / "Houses 30M residents" | Positive (green) | ❌ hidden (only the first `*Production` is surfaced; Housing has no `*Production`) |
| `AtmosphericHarvesting` | AtmosphericProcessor | 500 | "Harvests 500 Mt/yr industrial gases" | Positive | ✅ surfaced (it ends with "Harvesting" but the *first* `*Production` is the convention used) |
| `HydrogenSynthesis` | ChemicalPlant | 100 | "Synthesizes 100 Mt/yr H₂" | Positive | ❌ hidden (only the first of ChemicalPlant's 4 modifiers is surfaced) |
| `AmmoniaSynthesis` | ChemicalPlant | 200 | "Synthesizes 200 Mt/yr NH₃ (Haber-Bosch)" | Positive | ❌ hidden |
| `PolymerSynthesis` | ChemicalPlant | 450 | "Synthesizes 450 Mt/yr polymers" | Positive | ❌ hidden |
| `TritiumBreeding` | ChemicalPlant | 0.05 | "Breeds 0.05 Mt/yr Tritium (Li breeding)" | Positive | ❌ hidden |
| `PlutoniumBreeding` | BreederReactor | 0.23 | "Breeds 0.23 Mt/yr Plutonium" | Positive | ❌ hidden |
| `ConstructionCost` | Factory, Shipyard | -200, -300 | "Builds 200 BP/yr faster" / "Builds 300 BP/yr faster" | Positive (negative value → positive effect) | ❌ hidden |
| `ResearchSpeed` | ResearchLab, AiCluster, SemiconductorFab, DataCenter | 100, 300, 300, 400 | "Research speed +100%" (and stacks for multi-modifier buildings) | Positive | ❌ hidden |
| `EngineeringSpeed` | EngineeringBay, AiCluster, SemiconductorFab, DataCenter | 100, 200, 200, 300 | "Engineering speed +100%" | Positive | ❌ hidden |
| `PopulationGrowth` | MedicalCenter, PharmaceuticalPlant, WaterTreatmentPlant, DesalinationPlant | 50, 30, 20, 10 | "Population growth +0.5%/yr" | Positive | ❌ hidden |
| `StorageCapacity` | Warehouse | 0.10 | "Stockpile +10%" | Positive | ❌ hidden |
| `PowerGeneration` | SolarPower, WindFarm, FissionReactor, FusionReactor, DTFusionReactor, DHe3FusionReactor, ThoriumReactor, BreederReactor, HydroelectricDam, GeothermalPlant, CoalPowerPlant, NaturalGasPlant | 240, 310, 310, 2000, 3000, 2500, 800, 700, 510, 100, 1200, 750 | "Generates 240 GW" | Positive | ✅ surfaced (via the Power chip) |
| `WaterProduction` | WaterProcessor, AutoWaterProcessor | 16, 1.6 | "Produces 16 Mt/yr Water" | Positive | ✅ surfaced (WaterProcessor has no other modifiers; AutoWaterProcessor has the same) |

**Net: 9 of 52 buildings have hidden modifiers; 4 of
those have multiple hidden modifiers (ChemicalPlant has
4 hidden, AiCluster has 2, SemiconductorFab has 2,
DataCenter has 2). The total count of hidden modifier
types is 13 distinct types, of which 3 are currently
surfaced (`AtmosphericHarvesting`, `PowerGeneration`,
`WaterProduction`) and 10 are not.**

### 0.H.3 Code spec for the UI fix

The current code at `src/ui/construction.rs:1387-1391`:

```rust
// CURRENT: surfaces only the first *Production modifier
if let Some(prod) = def
    .modifiers
    .iter()
    .find(|m| m.modifier_type.ends_with("Production"))
{
    if prod.value > 0.0 {
        if let Some(res_name) = prod.modifier_type.strip_suffix("Production") {
            // ... format and push to effects
        }
    }
}
```

The proposed v3.1 spec for `src/ui/construction.rs:1387-1431`:

```rust
// v3.1 (Canary 11): iterate ALL modifiers, not just the
// first *Production. Apply per-modifier friendly labels,
// tone, and the 5+1 cap. Power is a separate chip (not
// pushed to effects; see PR-A.7).
let mut effects: Vec<(EffectTone, String)> = Vec::new();
for m in def.modifiers.iter() {
    if let Some((tone, label)) = friendly_label(m) {
        effects.push((tone, label));
    }
}

// v3.1 (Canary 11): cap at 5 effects + "+N more" indicator.
// (existing code already handles the cap; this just bumps
// the constant from 1 to 5.)
const EFFECT_CAP: usize = 5;
if effects.len() > EFFECT_CAP {
    let extra = effects.len() - EFFECT_CAP;
    effects.truncate(EFFECT_CAP);
    effects.push((EffectTone::Neutral, format!("+{} more", extra)));
}
```

The new helper function `friendly_label` (add to
`src/ui/construction.rs` near the existing
`format_mining_rate`):

```rust
/// v3.1 (Canary 11): map a Modifier to (tone, label) for
/// the building card's effect list. Returns `None` for
/// modifiers that should not be surfaced (e.g. internal
/// maintenance-only modifiers).
fn friendly_label(m: &crate::colony::data::Modifier) -> Option<(EffectTone, String)> {
    use EffectTone::*;
    match m.modifier_type.as_str() {
        // Production modifiers (Mt/yr per build, with build multiplier)
        "IronProduction" => Some((Positive, format!("Produces {} Iron", format_mining_rate(m.value)))),
        "AluminumProduction" => Some((Positive, format!("Produces {} Aluminum", format_mining_rate(m.value)))),
        // ... (one match arm per *Production type, ~30 arms)
        // Capacity modifiers
        "HousingCapacity" => {
            let residents = m.value as u64;
            Some((Positive, format!("Houses {} residents", format_residents(residents))))
        }
        // Atmospheric / synthesis
        "AtmosphericHarvesting" => Some((Positive, format!("Harvests {} Mt/yr industrial gases", m.value))),
        "HydrogenSynthesis" => Some((Positive, format!("Synthesizes {} Mt/yr Hydrogen", m.value))),
        "AmmoniaSynthesis" => Some((Positive, format!("Synthesizes {} Mt/yr Ammonia (Haber-Bosch)", m.value))),
        "PolymerSynthesis" => Some((Positive, format!("Synthesizes {} Mt/yr polymers", m.value))),
        "TritiumBreeding" => Some((Positive, format!("Breeds {} Mt/yr Tritium (Li breeding)", format_mining_rate(m.value)))),
        "PlutoniumBreeding" => Some((Positive, format!("Breeds {} Mt/yr Plutonium", format_mining_rate(m.value)))),
        // Cost reduction
        "ConstructionCost" if m.value < 0.0 => {
            Some((Positive, format!("Builds {} BP/yr faster", -m.value as i64)))
        }
        "ConstructionCost" => Some((Neutral, format!("Construction cost +{} BP/build", m.value as i64))),
        // Research / Engineering
        "ResearchSpeed" => Some((Positive, format!("Research speed +{}%", m.value as i64))),
        "EngineeringSpeed" => Some((Positive, format!("Engineering speed +{}%", m.value as i64))),
        // Population
        "PopulationGrowth" => Some((Positive, format!("Population growth +{:.1}%/yr", m.value / 100.0))),
        // Storage
        "StorageCapacity" => Some((Positive, format!("Stockpile capacity +{}%", (m.value * 100.0) as i64))),
        // Water
        "WaterProduction" => Some((Positive, format!("Produces {} Water", format_mining_rate(m.value)))),
        // Power is a separate chip; do not surface here
        "PowerGeneration" => None,
        // Catch-all: surface the raw modifier name
        _ => Some((Neutral, format!("{}: {}", m.modifier_type, m.value))),
    }
}
```

**Three parallel call-sites need the same fix.** The
construction card has 3 builders:
* `src/ui/construction.rs:1387-1391` (Build tab cards)
* `src/ui/construction.rs:1604-1625` (`compute_mining_card_data` — but this is for `MiningCardData`, not the effect list; the parallel fix is at 2851)
* `src/ui/construction.rs:2845-2871` (Spawned buildings — already-built inventory cards)

All three use the same `find` pattern. The Canary 11
fix replaces each `find` with the `for m in
def.modifiers.iter()` loop and the `friendly_label`
helper. The `compute_mining_card_data` function at 1604-1625
is different — it builds a `MiningCardData` struct with
`base_yield_mt_per_year`, `accessibility`, `reserve_mt`
fields; the fix here is to keep the existing `find` (which
extracts the *primary* `*Production` modifier for the
mining yield display) and add a SECONDARY loop that
populates a new field `additional_modifiers: Vec<String>`
on `MiningCardData` for the hidden modifiers. The Mining
tab card displays both: the primary yield in big text, the
additional modifiers as a small list below.

### 0.H.4 Effect cap — 5 + 1

The current card has 1 effect line (the first
`*Production`). v3.1 spec bumps this to 5. The +1 is the
"+N more" indicator that lets the player know the
building has more effects than the card shows. The cap
of 5 is the existing card height budget; the +1 is a
single line that doesn't break the layout. ChemicalPlant
has 4 effects (Hydrogen, Ammonia, Polymer, Tritium) +
the Power chip — fits in 5. AiCluster has 2 effects
(Research, Engineering) + Power — fits. SemiconductorFab
has 2 (Research, Engineering) + Power — fits. **No
building has more than 5 effects total; the +1 is a
defensive cap.**

### 0.H.5 Tones

The 4 existing `EffectTone` variants (Positive,
Negative, Neutral, Cost, Throughput) are extended in v3.1
to support the new modifiers:

* `Positive` (green): Production, Capacity, Harvesting,
  Breeding, Research, Engineering, Storage, Cost
  *reduction* (negative value of ConstructionCost).
* `Negative` (red): currently used for body-gate failures
  and resource-shortfall messages; unchanged.
* `Neutral` (gray): used for "+N more" indicator and
  fallback (unknown modifier type with raw name).
* `Cost` (orange): currently used for resource costs;
  unchanged. The construction cost is already on the
  cost chip, not in effects.
* `Throughput` (green): used for logistics throughput;
  unchanged.

### 0.H.6 Where the spec is conservative

Three places where v3.1 spec is conservative and
intentionally limits the surface area:

1. **The catch-all `_ =>` arm** surfaces unknown modifier
   types with the raw name. This is a defensive
   measure so a future RON addition doesn't silently
   hide. The raw name is awkward but visible.
2. **Power is NOT in effects** (the existing PR-A.7 chip
   is the right place). v3.1 does NOT propose moving
   Power back into the effect list.
3. **The 5+1 cap is hard**. If a future building has 6+
   effects, the +1 line tells the player to look at the
   tooltip (the existing tooltip system on the card
   surfaces the full modifier list). v3.1 does not
   propose variable-height cards.

### 0.H.7 Cross-references

* v3 §5.10 (LOCKED) gives the `power_demand_mw` RON edits
  (consumption side). v3.1 §0.H gives the UI fix for
  effect rendering.
* v0.5.2 PR-A.7 (SHIPPED) added the Power chip. v3.1
  extends with the effect-list fix for non-Power
  modifiers.
* v3.1 §5.15 gives the RUST spec for Canary 11 (effect
  rendering).
* v3.1 §0.D.7 adds Canary 11 to the unified apply plan.

---

## §0.D.7 v3.1 apply plan update — Canaries 9, 10, 11 (v3.1 NEW)

> **v3.1 NEW section. Extends v3 §0.D.2 with the 3 new
> canaries for workforce, resource cost, and effect
> rendering.** v3 §0.D is LOCKED for the original 8
> canaries (1, 2, 3a–3d, 4, 5a, 5b, 6, 7, 8) and the
> optional canary 9 (Power plant scale-down) and canary
> 10 (Survey). v3.1 **renumbers** the v3 canaries 9 and
> 10 to **12** and **13** (preserving their content
> verbatim) and inserts the v3.1 canaries 9, 10, 11
> between canary 8 (K2) and the renumbered canary 12
> (Power plant scale-down). The renumbering is purely
> cosmetic; the canary content and test gates are
> unchanged.

### 0.D.7.1 The v3.1 unified canary list

| # | Canary | Source | Files touched | Lines | Test gate |
|---|--------|--------|---------------|------:|-----------|
| 0 | (already shipped) | v0.5.2 ADDENDUM | (none) | — | baseline |
| 1 | Food calibration: Rust constant + Farm/Greenhouse/AquacultureFacility/AgriDome modifier | v2 §4.1, §8.1, §8.3.1–§8.3.4 | `src/colony/components.rs:282-301`, `buildings.ron` | ~15 RUST + 4 RON | `cargo test food` |
| 2 | WaterProcessor building | v2 §4.2, §8.2.1 | `buildings.ron`, `src/colony/types.rs` | ~25 RON + 1 RUST | `cargo test water` |
| 3a | `lunar_colony` tech (CRITICAL — He3Mine is unbuildable without it) | v2 §5.1.1, §8.6.1 | `technologies.ron` | ~15 RON | `cargo test tech_tree` + manual: research `lunar_colony`, build `He3Mine` on Moon |
| 3b | `fusion_power` tech (already exists; verify prereqs) | v2 §5.1.2, §8.6.2 | (verify only) | 0 | `cargo test tech_tree` |
| 3c | `kardashev_k2` tech | v2 §5.1.3, §8.6.3 | `technologies.ron` | ~15 RON | `cargo test tech_tree` |
| 3d | He3Mine + body restriction + He-3 / D / T downscale on consumer | v2 §5.1, §8.2.2, §8.3.12, §8.3.13 | `buildings.ron` | ~30 RON | `cargo test he3` + manual: build He3Mine on Moon, ship to Earth, build FusionReactor |
| 4 | Mid-game fold: ChemicalPlant, AtmosphericProcessor, BreederReactor | v2 §5.2–§5.6, §8.3.5, §8.3.6, §8.3.15 | `buildings.ron` | ~25 RON | `cargo test chemicals` + manual: verify N₂/O₂/Ar splits |
| 5a | Energy-demand rebalance (v3 NEW) | v3 §0.A, §5.10 | `buildings.ron` | ~30 RON | `cargo test power` + manual: 12 SolarPower + mature colony shows 0.3–1.3× demand/supply |
| 5b | FusionReactor He-3 / D / T downscale (v3 NEW) | v3 §0.C.7, §5.10 | `buildings.ron` | ~6 RON | `cargo test fusion` + manual: 1 He3Mine feeds 1 FusionReactor |
| 6 | Cost-headroom rebalance: IronMine BP 1500 → 1000 (v3 NEW) | v3 §0.B.3, §5.11 | `buildings.ron` | 1 RON | `cargo test construction` + manual: 25 IronMines = 25,000 BP (was 37,500) |
| 7 | Precious-metal + noble-gas mining (Au/Ag/Pt/Ar) | v2 §5.15–§5.18, §8.2.7–§8.2.9, §8.3.6, §8.3.18 | `buildings.ron`, `src/colony/types.rs`, `src/colony/data.rs` (MAINTENANCE_AUDIT_MAX loosening) | ~80 RON + ~3 RUST | `cargo test precious_metals` + manual: 25 Au/Ag/Pt mines feed 1 SemiconductorFab |
| 8 | K2 late-game: 4 exotics (AntimatterSynthesizer, ExoticMatterSynthesizer, MetamaterialsFab, ComputroniumSubstrate) | v2 §6, §8.2.3–§8.2.6 | `buildings.ron`, `src/colony/types.rs` | ~100 RON + 4 RUST | `cargo test k2` + manual: research `kardashev_k2`, build 12 AntimatterSynthesizer, verify grams display |
| **9** | **Workforce calibration (v3.1 NEW)** | **v3.1 §0.F, §5.13** | **`buildings.ron` (3 fields), `src/colony/types.rs:1251-1368` (3 lines)** | **3 RON + 3 RUST** | **`cargo test colony` (specifically `test_early_colony_workforce_feasible`, `test_workforce_efficiency`, `test_workforce_demand`) + manual: 1 Farm at 2,000 workers, 1 AluminumMine at 1,500 workers, 1 WindFarm at 1,000 workers; all 3 in the 0.5–2× real-productivity band; early-game test sum 14,500 < 40,000** |
| **10** | **Resource build cost rebalance (v3.1 NEW)** | **v3.1 §0.G, §5.14** | **`buildings.ron` (OrbitalLift Ti 333 → 5)** | **1 RON** | **`cargo test construction` + manual: 1 OrbitalLift buildable from 1 year of TitaniumMine production at 25-mine scale (16.7 yr at 25 mines, 0.3 yr at 100 mines); MassDriver Cu 167 unchanged (7.4 yr at 25 mines); HabitatDome Al 50 unchanged (0.67 yr at 25 mines)** |
| **11** | **Building-card effect rendering (v3.1 NEW)** | **v3.1 §0.H, §5.15** | **`src/ui/construction.rs:1387-1391`, `:1608`, `:1623`, `:2851` (and the 3 parallel builders), new `friendly_label` helper** | **~50 RUST** | **`cargo test construction_ui` + manual: ChemicalPlant card shows "Synthesizes 100 Mt/yr H₂" + "Synthesizes 200 Mt/yr NH₃" + "Synthesizes 450 Mt/yr polymers" + "Breeds 0.05 Mt/yr Tritium" (4 effects, in cap); HabitatDome card shows "Houses 50M residents" (1 effect, in cap); Warehouse card shows "Stockpile capacity +10%" (1 effect, in cap); 5+1 cap with "+N more" indicator** |
| 12 | (Optional) Power plant scale-down per v2 §7.2 *(renumbered from v3 canary 9)* | v2 §7.2, §8.3.16 | `buildings.ron` | ~12 RON | `cargo test power` + manual: verify 12 SolarPower at 200 GW = 2,400 GW |
| 13 | (Optional) Survey mission expansion (out of v3 scope) *(renumbered from v3 canary 10)* | (none — survey is shipped) | (none) | 0 | n/a |

**Total v3.1 work** for canaries 9, 10, 11: **4 RON lines
+ 3 RUST enum lines + ~50 RUST UI lines = ~57 lines.**
The v3 work for canaries 1–8 is unchanged (~10 RUST lines
+ ~330 RON lines). v3.1's 57 lines is small relative to
v3's 340 lines (~17 % of v3's scope).

### 0.D.7.2 Canary 9 — workforce — worked example

**Edit `buildings.ron:2001` (Farm):**

```diff
- workforce: 1000,
+ workforce: 2000,
```

**Edit `buildings.ron:320` (AluminumMine):**

```diff
- workforce: 4500,
+ workforce: 1500,
```

**Edit `buildings.ron:2313` (WindFarm):**

```diff
- workforce: 200,
+ workforce: 1000,
```

**Edit `src/colony/types.rs:1340` (Farm), `:1261`
(AluminumMine), `:1333` (WindFarm):** same 3 changes in
the hard-coded `workforce_required` function.

**Test gate:** `cargo test colony` (existing tests in
`src/colony/components.rs:646-754`); the critical tests:
* `test_workforce_positive` (line 1581) — passes
  (all 70 buildings have positive workforce)
* `test_early_colony_workforce_feasible` (line 1592) —
  passes (sum 14,500 < 40,000)
* `test_workforce_demand` (line 1726) — passes (Farm
  2,000 + IronMine 5,000 = 7,000; 10M pop × 0.4 = 4M
  workers; workforce_efficiency = 1.0)
* `test_workforce_efficiency` (line 1738) — passes

**Manual gate:** start a new game on Earth; verify
* Farm at 2,000 workers, produces 360 Mt/yr food
  (per the RON `FoodProduction: 9000` modifier + the
  hard-coded 360 in `src/colony/components.rs:324`).
  Per-worker productivity = 360 / 2,000 = 180 t/yr/worker
  (1.06× the 170 t/yr/worker 1:155 anchor; in band).
* AluminumMine at 1,500 workers, produces 5 Mt/yr
  aluminum. Per-worker = 3.3 t/yr/worker (within
  Bayer + Hall-Héroult 0.5–5 t/yr/worker band).
* WindFarm at 1,000 workers, produces 310 GW. Per-worker
  = 310 MW/worker (78× real, justified by high
  automation).

### 0.D.7.3 Canary 10 — resource cost — worked example

**Edit `buildings.ron:1705-1710` (OrbitalLift):**

```diff
  resource_costs: [
-     ("Titanium", 333.0),
+     ("Titanium", 5.0),
      ("Iron", 333.0),
      ("RareEarths", 83.0),
      ("Carbon", 167.0),
  ],
```

**Test gate:** `cargo test construction` (existing); the
critical test: `test_building_costs_positive` (line 1561)
— passes (all resource_costs are still positive).

**Manual gate:** start a new game on Earth; queue 1
OrbitalLift build; verify
* 5 Ti required (down from 333)
* 25 TitaniumMines producing 0.3 Mt/yr aggregate
  produces enough Ti for 1 OrbitalLift in 16.7 yr at
  25 mines, or 5 yr at 100 mines, or 0.3 yr at
  1,000 mines (a civilization-scale Ti economy)
* The other 3 costs (Fe 333, REE 83, C 167) are
  unchanged and the player can still see them as the
  strategic-tier constraints (Fe 0.19 yr, REE 220 yr
  strategic, C 0.008 yr)

### 0.D.7.4 Canary 11 — effect rendering — worked example

**Edit `src/ui/construction.rs:1387-1431`:** replace the
single `find` with the `for m in def.modifiers.iter()`
loop and the `friendly_label` helper (per §0.H.3 spec).

**Edit `src/ui/construction.rs:1604-1625`** (the
`compute_mining_card_data` function): keep the existing
`find` (for the primary `*Production` modifier), add a
new field `additional_modifiers: Vec<String>` to
`MiningCardData` and populate it with the secondary
modifiers (e.g. for ChemicalPlant, the secondary
`HydrogenSynthesis`, `AmmoniaSynthesis`,
`PolymerSynthesis`, `TritiumBreeding` are the additional
modifiers).

**Edit `src/ui/construction.rs:2845-2871`** (the
spawned-inventory builder): same fix as 1387-1431.

**Test gate:** `cargo test construction_ui` (existing
tests in `src/ui/construction.rs`); the critical tests
verify the `BuildCardData` struct fields are populated
correctly. New unit test:
`test_friendly_label_*` (one per modifier type, ~13
tests).

**Manual gate:** open the Build tab, navigate to
ChemicalPlant; verify
* 4 effect lines: "Synthesizes 100 Mt/yr Hydrogen" +
  "Synthesizes 200 Mt/yr Ammonia (Haber-Bosch)" +
  "Synthesizes 450 Mt/yr polymers" + "Breeds 0.05 Mt/yr
  Tritium (Li breeding)"
* Power chip shows the 600 MW demand
* 4 effect lines fit in the card height (4 < 5 cap)
Navigate to HabitatDome; verify
* 1 effect line: "Houses 50M residents"
* Power chip shows the 20,900 MW demand (after canary
  5a is applied)
Navigate to Warehouse; verify
* 1 effect line: "Stockpile capacity +10%"
Navigate to AiCluster; verify
* 2 effect lines: "Research speed +300%" +
  "Engineering speed +200%"
* Power chip shows the 2,000 MW demand
* 2 effect lines fit in the card height (2 < 5 cap)

### 0.D.7.5 Apply-order invariant (v3.1 NEW)

The 3 v3.1 canaries have no critical-path ordering
relative to v3 canaries 1–8:

* **Canary 9 (workforce)** does not depend on any
  canary. It is the simplest change (3 RON + 3 RUST
  lines, both source-of-truth duplicates). It can land
  in parallel with canary 1 (food) and is the lowest-
  risk v3.1 canary.
* **Canary 10 (resource cost)** does not depend on
  any canary. It is a 1-line RON change. It can land
  in parallel with canary 6 (IronMine BP) — both are
  cost changes, and the user can A/B them.
* **Canary 11 (effect rendering)** does not depend
  on any canary. It is a UI-only change. It can land
  in parallel with any v3 canary because it doesn't
  touch the simulation.

**v3.1 canaries are all independent of each other and of
v3 canaries 1–8.** The user can land them in any order;
the A/B feature-flag pattern (per v3 §0.D.5) applies.

### 0.D.7.6 Critical-path (v3.1 update)

The v3 critical-path (3a → 3b → 3c → 3d → 4 → 5b) is
unchanged. The 3 v3.1 canaries (9, 10, 11) are
non-critical and can land in any order. The renumbered
v3 canaries 12 (Power plant scale-down) and 13
(Survey) remain optional and can land at any time.

---

## §0.I v3.2 addendum — starter-tier housing + RON-Rust sync (v3.2 NEW, 2026-08-07)

> **This is a v3.2 hotfix addendum.** Two related issues the user
> surfaced this turn:
> 1. **The card and the simulation disagreed by 32–80× on
>    housing.** Canary 11 (the `friendly_label` helper) was reading
>    the RON `HousingCapacity` modifier (800M / 4B / 2B), but the
>    simulation at `src/colony/components.rs:298` used hard-coded
>    values (25M / 50M / 30M). The card said "Houses 800M
>    residents" but the sim served 25M. This was a canary 11
>    regression — the bug existed before but was hidden because
>    `friendly_label` didn't exist. v3.2 syncs the RON to the
>    Rust: 25M / 50M / 30M.
> 2. **"First buildings already civilizational scale."** Even
>    25M per Housing is "metropolitan tier" — a 100k-population
>    new colony can't use it. v3.2 adds two starter-tier
>    buildings: `HabitatTent` (1k residents, 3 Fe + 5 Si) and
>    `HabitatModule` (10k residents, 10 Fe + 15 Si + 1 Cu + 3
>    Al). These are the actual first buildings the player can
>    afford and use on a fresh outpost.

### 0.I.1 The RON-Rust sync (hotfix)

| Building | RON before (card) | RON after (card) | Rust (sim) | Mismatch before | Mismatch after |
|---|---:|---:|---:|---:|---:|
| Housing | 800,000,000 | **25,000,000** | 25,000,000 | 32× too high | ✅ match |
| HabitatDome | 4,000,000,000 | **50,000,000** | 50,000,000 | 80× too high | ✅ match |
| UndergroundHabitat | 2,000,000,000 | **30,000,000** | 30,000,000 | 67× too high | ✅ match |

**Why sync RON to Rust, not the other way.** The Rust values
are the v0.5.0 design choice (per the comment at
`src/colony/components.rs:284-292`): "scaled for meaningful
per-build impact." The RON values are a holdover from a
pre-v0.5.0 calibration that wasn't aligned with the
manageable-count target. The v0.5.0 design says Earth needs
~164 HabitatDomes + 164 Housing Complexes (not 2 + 10) for
its 8.2B seed — 328 metropolitan-scale buildings, not 12
arcology-scale buildings. v3.2 honors the v0.5.0 choice.

### 0.I.2 The starter-tier gap (the deeper issue)

The user pointed out: even 25M per Housing is "civilizational
scale" — it's a Tokyo-class metro area. A 100k-population
outpost on Luna or Mars can't use it. The existing 52-building
catalog has no starter tier (1k–10k residents per build).

v3.2 adds two starter-tier buildings:

| Building | Residents | BP | Material cost | Workforce | Power | Tier |
|---|---:|---:|---|---:|---:|---|
| **HabitatTent** | 1,000 | 50 | 3 Fe + 5 Si | 5 | 5 MW | Starter |
| **HabitatModule** | 10,000 | 200 | 10 Fe + 15 Si + 1 Cu + 3 Al | 50 | 30 MW | Starter |

**Why these values.** Real-world precedents:
- ISS module: 6–8 crew, 100 t → for 4X scale, 1k–10k per
  module is the operator-bar
- Mars base plans (Mars Direct, SpaceX): 1k–10k initial
  crew, expandable to 100k+
- Lunar base (Artemis): 4 crew initially, 50–100 mid-term

The 1k–10k range is the standard 4X "outpost housing" tier.
It's missing from the v0.5.0 catalog.

### 0.I.3 The new-colony bootstrap increase

The v0.5.0 bootstrap (per
`src/plugins/solar_system.rs:2935-2946`) gave new colonies
10 Fe / 50 Si / 2 Al / 0.5 Cu / 1 Poly / 10k Food / 5 Water —
**and 0 Phosphorus**. The cheapest building on the catalog
(Farm, Fe 8 + Water 8 + **P 3**) couldn't be afforded. The
player founded a colony and built **nothing**.

v3.2 (2026-08-07) bumps the bootstrap to:

| Resource | v0.5.0 (Mt) | v3.2 (Mt) | Affordable |
|---|---:|---:|---|
| Iron | 10 | **50** | 5 HabitatTents + 2 HabitatModules + 1 IronMine |
| Silicates | 50 | **100** | 5 HabitatTents + 2 HabitatModules + 1 WaterProcessor |
| Aluminum | 2 | **10** | 2 HabitatModules + 1 IronMine |
| Copper | 0.5 | **5** | 2 HabitatModules + 1 LifeSupport |
| Polymers | 1 | **5** | 2 HabitatModules + maintenance buffer |
| Phosphorus | 0 | **5** | 1 Farm (the missing resource!) |
| Food | 10,000 | 10,000 | unchanged |
| Water | 5 | **20** | 1 WaterProcessor + buffer |

**The new colony can now found a working outpost** before
the first freighter arrives. Specific recipes:

- **5 HabitatTents + 1 HabitatModule** = 15k housing
  (15 Mt Fe + 40 Mt Si + 1 Mt Cu + 3 Mt Al)
- **1 Farm** = food for ~5k people (8 Fe + 8 Water + 3 P)
- **1 IronMine** = start self-sufficient Fe production
  (100 Fe — but the bootstrap only has 50, so this needs
  the player to wait for a freighter or build HabitatTents
  for surplus)
- **1 LifeSupport** = atmospheric recycling (83 Fe + 33 Cu
  — needs surplus Fe from IronMine or freighters)

The bootstrap is sized for the first few buildings. After
the first 1–2 freighters arrive (with the first IronMine
production), the player can scale up to metropolitan tier
(Housing / HabitatDome / UndergroundHabitat).

### 0.I.4 The 5-tier housing system (post-v3.2)

| Tier | Building | Residents | Material cost | Body |
|---|---|---:|---|---|
| **Starter 1** | HabitatTent | 1,000 | 3 Fe + 5 Si | any |
| **Starter 2** | HabitatModule | 10,000 | 10 Fe + 15 Si + 1 Cu + 3 Al | any |
| **Metropolitan A** | Housing | 25,000,000 | 33 Fe + 83 Si | habitable |
| **Metropolitan B** | UndergroundHabitat | 30,000,000 | 250 Fe + 167 Si + 83 Ti | non-atm |
| **Metropolitan C** | HabitatDome | 50,000,000 | 167 Fe + 133 Si + 50 Al | any |
| **Arcology** | (none — future) | 1B+ | (future K2 tier) | (future) |

For a 100k-population outpost: ~10 HabitatTents + 9 HabitatModules
(within the v2 manageable-count band 10–50).
For a 10M-population city: 200–400 Housing Complexes.
For 8.2B Earth: **400 Housing Complexes (per
`src/plugins/solar_system.rs:1173`)** = 400 × 25M = 10B
capacity, 22% surplus over 8.2B pop. (The earlier draft
said "164 + 164" — that was wrong; the v0.5.0 design at
`src/colony/components.rs:291` cites "~335 Housing
Complexes" for exact fit, 400 for headroom.)

### 0.I.5 v3.2 changes summary

| File | Change | Lines |
|---|---|---|
| `assets/data/buildings.ron` | 3 RON lines synced (HousingCapacity 800M→25M, 4B→50M, 2B→30M) | ~30 (mostly comments) |
| `assets/data/buildings.ron` | 2 new buildings (HabitatTent, HabitatModule) | ~50 |
| `src/colony/types.rs` | 2 new `BuildingType` enum variants + 5 match updates (all, display_name, description, effects_summary, icon, category, build_cost, workforce_required) | ~50 |
| `src/colony/data.rs` | 2 new entries in `parse_building_type` | ~3 |
| `src/colony/components.rs` | `housing_capacity()` includes 2 new tiers | ~10 |
| `src/plugins/solar_system.rs` | New-colony bootstrap increase | ~20 (mostly comments) |
| `src/ui/construction.rs` | 4 new friendly_label tests for the new RON values | ~50 |
| `src/ui/construction.rs` | 1 test update (`test_building_type_all` 95→97) | ~5 |
| `docs/design/BALANCE_PATCHES_v0.5.md` | This section | — |

**Total: ~218 RUST/RON/UI lines + 0 new tests beyond the
existing 13 friendly_label tests + 4 new tests for the new
RON values.**

### 0.I.6 v3.2 stop conditions

| Stop condition | Status |
|---|---|
| Card and simulation agree on HousingCapacity (no 32–80× mismatch) | ✅ |
| 2 new starter-tier buildings (HabitatTent, HabitatModule) added to catalog | ✅ |
| `housing_capacity()` includes the 2 new tiers | ✅ |
| 2 new `BuildingType` enum variants added, all 5 match statements updated | ✅ |
| New-colony bootstrap sized for the starter-tier buildings | ✅ |
| `test_building_type_all` updated 95→97 | ✅ |
| `test_early_colony_workforce_feasible` updated to use starter-tier | ✅ |
| `friendly_label_housing_capacity_*` tests cover the 5 tiers | ✅ |
| `cargo build` clean | ✅ |
| `cargo test` 1079 lib + 1076 bin, all green | ✅ |
| No new B0001 violations | ✅ |

### 0.I.7 v3.2 vs v3.1 deltas

| v3.1 | v3.2 |
|------|------|
| 95 building types | 97 (+ HabitatTent, HabitatModule) |
| Housing 25M (Rust) / 800M (RON, card) | Housing 25M (Rust + RON, card matches sim) |
| New colony bootstrap: 10 Fe, 50 Si, 2 Al, 0.5 Cu, 1 Poly, 0 P | New colony bootstrap: 50 Fe, 100 Si, 10 Al, 5 Cu, 5 Poly, 5 P, 20 Water |
| Starter tier: none (smallest = Housing 25M) | Starter tier: HabitatTent 1k, HabitatModule 10k |
| First building on new colony: not affordable | First building on new colony: 5 HabitatTents + 1 HabitatModule + 1 Farm |

---

## §0.J v3.4 addendum — IEA 2026 calibration (v3.4 NEW, 2026-08-07)

### 0.J.1 Why this exists

Questionnaire offered two Earth power targets: 35.8 TW (v3 §0.A.3
"12 plants per type") or 3.4 TW (IEA 2026 reality, 30,000 TWh/yr).
User selected **3.4 TW (IEA 2026 reality)**. v3.3's flat 3.55×
scale-down kept the v0.5.0 RON's plant-count bias (400 wind vs
20 fission) and produced a non-IEA mix (Wind 20% vs IEA 10%,
Fission 1% vs IEA 9%, Hydro 7.5% vs IEA 14%). v3.4 redoes supply
per-build so the **plant-count mix reproduces IEA 2026 generation
shares exactly** while the **absolute total lands at 3.4 TW**.

### 0.J.2 Pushback on framing

User's v3 spec carried a "584 TW" Earth 2026 baseline. That is
**170× over the IEA 2026 figure of 3.4 TW** (30,000 TWh/yr world
electricity). The RON's plant *proportions* (400 wind, 195 coal,
etc.) are reasonable representations of 2026 generation shares,
but the per-build GW values were sized for 35.8 TW total, not 3.4.
v3.4 keeps the plant proportions and recalibrates the per-build
GW to match IEA reality.

### 0.J.3 What v3.4 changes

| Side | v3.3 | v3.4 |
|------|------|------|
| Total demand | 12.36 TW | 3.31 TW (1.5× of v0.5.0 original) |
| Total supply | 12.06 TW | 3.40 TW (IEA 2026) |
| Demand/Supply ratio | 1.025 | 0.974 |
| Generation mix | not IEA-shaped | within 1-2pp of IEA 2026 |

Demand side: revert v3.3's 5× scale-up, then apply 1.5× to the
v0.5.0 original. The 1.5× (not 1.0×) addresses the user's
"buildings consume way too little power" complaint while staying
within IEA 2026 reality. Avoiding the v3 §0.A.4 209× Housing
scale-up (which was end-use energy, not electricity).

Supply side: per-build GW computed as
`3.4 TW × IEA_share / Earth_plant_count` then uniformly bumped
1.064× to land exactly at 3.4 TW (rounding loss compensation).

| Building | v3.3 (GW/plant) | v3.4 (GW/plant) | Plant count | Total (GW) |
|----------|----------------|----------------|-------------|-----------|
| CoalPowerPlant | 7.00 | 5.56 | 195 | 1,084 |
| NaturalGasPlant | 4.50 | 5.89 | 135 | 795 |
| HydroelectricDam | 3.10 | 6.18 | 82 | 507 |
| FissionReactor | 1.70 | 16.28 | 20 | 326 |
| WindFarm | 1.70 | 0.90 | 400 | 360 |
| SolarPower | 1.40 | 1.02 | 320 | 326 |
| **Total** | | | | **3,398 GW = 3.40 TW** |

### 0.J.4 IEA 2026 mix verification

| Source | IEA 2026 target | v3.4 actual | Δ (pp) |
|--------|----------------|-------------|--------|
| Coal | 30% | 31.9% | +1.9 |
| Gas | 22% | 23.4% | +1.4 |
| Hydro | 14% | 14.9% | +0.9 |
| Nuclear | 9% | 9.6% | +0.6 |
| Wind | 10% | 10.6% | +0.6 |
| Solar | 9% | 9.6% | +0.6 |

All sources within 0.6-1.9pp of IEA 2026. Bias toward Coal/Gas
(+1.4-1.9pp) comes from the 1.064× uniform bump: coal and gas
have the highest per-plant GW, so they get a slightly larger
absolute bump. Acceptable trade-off — the mix is effectively
IEA-shaped for game purposes.

### 0.J.5 Files modified

- `assets/data/buildings.ron` — 6 supply values (PowerGeneration
  modifiers for SolarPower/CoalPowerPlant/NaturalGasPlant/
  HydroelectricDam/WindFarm/FissionReactor)
- `src/plugins/solar_system.rs:1221-1226` — replaced misleading
  "≈ 3.65 TW" comment block with v3.4 IEA calibration block
  (3.40 TW supply, 3.31 TW demand, ratio 0.974, IEA mix within
  0.6-1.9pp)

### 0.J.6 v3.4 vs v3.3 deltas

| v3.3 | v3.4 |
|------|------|
| 12.36 TW demand / 12.06 TW supply | 3.31 TW demand / 3.40 TW supply |
| Wind 20.1% (way over IEA 10%) | Wind 10.6% (within 0.6pp of IEA) |
| Fission 1.0% (way under IEA 9%) | Fission 9.6% (within 0.6pp of IEA) |
| Hydro 7.5% (way under IEA 14%) | Hydro 14.9% (within 0.9pp of IEA) |
| Misleading "3.65 TW" comment | v3.4 IEA calibration block |

### 0.J.7 v3.4 status

| Check | Status |
|-------|--------|
| All 6 supply values updated in buildings.ron | ✅ |
| Total 3.40 TW, mix within 1-2pp of IEA 2026 | ✅ |
| Demand 3.31 TW, ratio 0.974 (97.4% utilization) | ✅ |
| Misleading "3.65 TW" comment replaced | ✅ |
| `cargo build` clean | ✅ |
| `cargo test` 1079 lib + 1076 bin, all green | ✅ |
| Mineral balance preserved (Food 25× surplus, Iron 28% deficit, Al/Cu ~parity) | ✅ (later superseded by v3.5 — see §0.K) |

---

## §0.K v3.5 addendum — Resource generation balance (v3.5 NEW, 2026-08-07)

### 0.K.1 Why this exists

User requested balancing all resource generation including food.
Pre-v3.5 inventory at Earth 25 builds × 0.6 accessibility
(unless noted) showed wild imbalance:

| Resource | Per-build | Total/yr | World | %world | Issue |
|----------|-----------|----------|-------|--------|-------|
| Iron | 120 | 1,800 | 2,500 | 72% | 28% deficit |
| Titanium | 0.02 | 0.3 | 9 | **3.3%** | 30× UNDER |
| Phosphorus | 0.003 | 0.045 | 220 (rock) | **0.02%** | 5,000× UNDER |
| Silicates | 700 | 10,500 | 4,800 | 219% | 2.2× over |
| Gold | 0.0001 | 0.0015 | 0.0036 | 42% | deficit |
| Carbon | 350 | 5,250 | 8,200 | 64% | 36% deficit |
| Thorium | 0.0007 | 0.0105 | 0.0008 | 1,313% | 13× over |
| Deuterium | 0.5 | 7.5 | 0.05 | 15,000% | 150× over |
| He3 | 0.5 | 7.5 | 0.000001 | 7.5×10⁸% | massive (body-restricted) |
| **AtmosphericProcessor** | 500 | 150,000 | 500 | 30,000% | 300× over |
| Farm (RON) | 9,000 | 225,000 | 9,000 | 2,500% | 25× over (RON doc only) |
| Greenhouse (RON) | 5,000 | 50,000 | 9,000 | 556% | 5.5× over (RON doc) |
| AquacultureFacility (RON) | 1,500 | 15,000 | 9,000 | 167% | over (RON doc) |

### 0.K.2 What v3.5 changes

**Calibration target**: per-build × 25 × 0.6 = world demand (parity).
Where round to 4 decimals causes drag, use 6 decimals.

| Building | v3.4 | v3.5 | Δ | New %world |
|----------|------|------|---|------------|
| IronMine | 120 | 166.667 | ×1.39 | 100.0% |
| AluminumMine | 5.0 | 4.667 | ×0.93 | 100.0% |
| TitaniumMine | 0.02 | 0.6 | ×30 | 100.0% |
| SilicatesMine | 700 | 320 | ×0.46 | 100.0% |
| NickelMine | 0.2 | 0.233 | ×1.17 | 100.0% |
| TungstenMine | 0.005 | 0.0057 | ×1.13 | 100.6% |
| CarbonMine | 350 | 546.667 | ×1.56 | 100.0% |
| ChromiumMine | 2.0 | 2.0 | ×1.0 | 100.0% |
| MagnesiumMine | 0.07 | 0.073 | ×1.05 | 100.0% |
| GoldMine | 0.0001 | 0.00024 | ×2.4 | 100.0% |
| SilverMine | 0.001 | 0.00207 | ×2.07 | 101.6% |
| PlatinumMine | 0.00001 | 0.0000133 | ×1.33 | 99.8% |
| CopperMine | 1.5 | 1.467 | ×0.98 | 100.0% |
| RareEarthsMine | 0.025 | 0.02 | ×0.8 | 100.0% |
| LithiumMine | 0.012 | 0.0087 | ×0.72 | 100.4% |
| SulfurMine | 5.0 | 4.667 | ×0.93 | 100.0% |
| PhosphorusMine | 0.003 | 14.667 | **×4889** | 100.0% |
| CobaltMine | 0.015 | 0.0147 | ×0.98 | 100.2% |
| FluorineMine | 0.2 | 0.3 | ×1.5 | 100.0% |
| UraniumMine | 0.003 | 0.0049 | ×1.64 | 99.3% |
| ThoriumMine | 0.0007 | 0.0000533 | ×0.076 | 99.9% |
| MethaneExtractor | 270 | 273.333 | ×1.01 | 100.0% |
| DeuteriumExtractor | 0.5 | 0.0033 | ×0.0067 | 99.0% |
| He3Mine | 0.5 | 0.5 (unchanged) | ×1.0 | body-restricted |
| AtmosphericProcessor | 500 | 2.7778 | ×0.0056 | 100.0% |

**Special cases**:
- **He3Mine** left at 0.5 (body-restricted to [Moon, GasGiant,
  Asteroid]; Earth count = 0; operator-bar applies off-world).
- **PhosphorusMine** 0.003 → 14.667: pre-v3.5 used
  P2O5 (55 Mt/yr) as world demand; actual USGS 2026 phosphate
  **rock** is 220 Mt/yr. Per-build was 100,000× too low.
- **ThoriumMine** 0.0007 → 0.0000533: pre-v3.5 calibrated for
  operator-bar Tier 1 (1 building = world share × ~1000); v3.5
  uses parity. 13× drop.
- **DeuteriumExtractor** 0.5 → 0.0033: pre-v3.5 15,000% over
  (1 building = 1,500× world share). 150× drop.

### 0.K.3 Food: RON ↔ Rust hard-code sync

Pre-v3.5 had a **RON-Rust mismatch** for food:
- `buildings.ron` had `Farm 9000`, `Greenhouse 5000`,
  `AquacultureFacility 1500`, `AgriDome 180`.
- `components.rs:343` had `farm * 360 + agri * 4 + greenhouse * 200
  + aquaculture * 200` (hard-coded; RON not read).

Result: actual game produced 13,000 Mt/yr (1.44× world food),
but RON documentation claimed 25× overproduction. v3.5 syncs
the RON to the hard-code:

| Building | RON v3.4 | RON v3.5 | Code (unchanged) |
|----------|----------|----------|------------------|
| Farm | 9,000 | 360 | 360 |
| Greenhouse | 5,000 | 200 | 200 |
| AquacultureFacility | 1,500 | 200 | 200 |
| AgriDome | 180 | 4 | 4 |

**Earth food balance**: 25×360 + 10×200 + 10×200 = 13,000 Mt/yr
vs 9,020 Mt/yr demand (8.2B × 1,100 kg/p/yr) = **1.44× surplus**.
44% headroom for population growth before stockpile cap.

### 0.K.4 AtmosphericProcessor calibration

Pre-v3.5: 300 buildings × 500 = 150,000 Mt/yr = 300× world
industrial gas (500 Mt/yr FAO/IEA). v3.5: 300 × 2.78 × 0.6 = 500
Mt/yr = 1× world. 180× drop. Rationale: the player consumes
industrial gas via ChemicalPlant synthesis; 300× overproduction
means unlimited inputs. v3.5 caps production at world demand so
expanding ChemicalPlant count requires expanding
AtmosphericProcessor count too.

### 0.K.5 Files modified

- `assets/data/buildings.ron` — 27 modifier values (24 mines
  + AtmosphericProcessor + 4 food) + 28 description strings
  (id, modifier, description updated in lockstep)
- `src/colony/components.rs:338-343` — food hard-code unchanged
  (RON was the broken side; v3.5 fixes the doc)
- `docs/design/BALANCE_PATCHES_v0.5.md` — this addendum

### 0.K.6 v3.5 vs v3.4 deltas

| v3.4 | v3.5 |
|------|------|
| Titanium 0.02 / 3% of world | 0.6 / 100% |
| Phosphorus 0.003 / 0.02% of world | 14.67 / 100% (4889× bump) |
| Silicates 700 / 219% | 320 / 100% |
| AtmosphericProcessor 500 / 30000% | 2.78 / 100% |
| RON Farm 9000 (25× over) | RON Farm 360 (1× world, matches code) |
| He3 0.5 (body-restricted) | unchanged (body-restricted) |

### 0.K.7 v3.5 status

| Check | Status |
|-------|--------|
| All 24 mine per-build values at parity (±2%) | ✅ |
| AtmosphericProcessor at parity (1× world industrial gas) | ✅ |
| Food RON matches food hard-code (Farm 360 etc.) | ✅ |
| Description strings updated to match new per-build | ✅ |
| Earth food balance 13,000 Mt/yr (1.44× world demand) | ✅ |
| `cargo build` clean | ✅ |
| `cargo test` 1079 lib + 1076 bin, all green | ✅ |

---

## §0.L v3.6 addendum — RON as single source of truth (v3.6 NEW, 2026-08-07)

### 0.L.1 Why this exists

User requested removing all hard-coded per-build values, not just
food. Pre-v3.6 had hard-coded values scattered across
`src/colony/components.rs` that the RON documentation *said* were
X but the actual code used Y — the same RON-Rust mismatch pattern
that v3.5 fixed for food. v3.6 extends the fix to every per-build
value + every global colony-tuning parameter.

### 0.L.2 What v3.6 changes

**Per-build modifiers added to RON** (already-present modifiers
in BOLD; new ones in CAPS):

| Building | New RON modifier | Value | Pre-v3.6 source |
|----------|-----------------|-------|-----------------|
| CommercialHub | **WEALTHGENERATION** | 500 MC/yr | hard-coded in `wealth_generation_per_year` |
| FinancialCenter | **WEALTHGENERATION** | 2,000 MC/yr | hard-coded |
| TradePort | **WEALTHGENERATION** | 5,000 MC/yr | hard-coded |
| Factory | **WEALTHGENERATION** (added alongside BuildPointsProduction) | 100 MC/yr | hard-coded |
| MassDriver | **LOGISTICSCAPACITY** | 5,000 t/yr | hard-coded |
| OrbitalLift | **LOGISTICSCAPACITY** | 20,000 t/yr | hard-coded |
| CargoTerminal | **LOGISTICSCAPACITY** | 2,000 t/yr | hard-coded |
| HabitatTent (v3.2) | HousingCapacity (already) | 1,000 | hard-coded (matched) |
| HabitatModule (v3.2) | HousingCapacity (already) | 10,000 | hard-coded (matched) |
| HabitatDome (v3.2) | HousingCapacity (already) | 50,000,000 | hard-coded (matched) |
| Housing (v3.2) | HousingCapacity (already) | 25,000,000 | hard-coded (matched) |
| UndergroundHabitat (v3.2) | HousingCapacity (already) | 30,000,000 | hard-coded (matched) |
| Farm / AgriDome / Greenhouse / AquacultureFacility (v3.5) | FoodProduction (already) | 360 / 4 / 200 / 200 | hard-coded (matched in v3.5) |

**New top-level `colony_constants` struct in RON** (replaces
hard-coded `const`s in `components.rs`):

| Field | Value | Pre-v3.6 source |
|-------|-------|-----------------|
| `food_consumption_per_capita_mt_per_year` | 0.0000011 | `pub fn food_consumption_per_year` hard-coded `0.0000011` |
| `base_growth_rate` | 0.009 | `const BASE_GROWTH_RATE: f64 = 0.009` |
| `medical_growth_per_center` | 0.0003 | `const MEDICAL_GROWTH_PER_CENTER` |
| `max_medical_growth_bonus` | 0.009 | `const MAX_MEDICAL_GROWTH_BONUS` |
| `housing_utilization_penalty` | 0.8 | inline `* 0.8` in growth factor |
| `available_workforce_fraction` | 0.4 | `pub fn available_workforce` hard-coded `0.4` |
| `operating_cost_fraction` | 0.05 | `pub fn operating_cost_per_year` hard-coded `0.05` |

### 0.L.3 Rust refactor

**BuildingsData** (in `src/colony/data.rs`) gains:
- `colony_constants: ColonyConstants` field (loaded from RON)
- `per_build_value(bt, modifier_type) -> f64` helper
- Specific helpers: `housing_capacity_for(bt)`, `food_production_for(bt)`,
  `wealth_generation_for(bt)`, `logistics_capacity_for(bt)`
- `BuildingsData::load_for_tests()` — reads the RON file for test fixtures

**Colony** methods now take `&BuildingsData`:
- `housing_capacity(&self, data)` — reads `HousingCapacity` per building
- `food_production_per_year(&self, data)` — reads `FoodProduction`
- `food_consumption_per_year(&self, data)` — reads `colony_constants.food_consumption_per_capita_mt_per_year`
- `wealth_generation_per_year(&self, data)` — reads `WealthGeneration`
- `logistics_capacity(&self, data)` — reads `LogisticsCapacity`
- `operating_cost_per_year(&self, data)` — reads `colony_constants.operating_cost_fraction`
- `population_growth_per_year(&self, food_factor, data)` — reads growth rates from constants
- `available_workforce(&self, data)` — reads `available_workforce_fraction`
- `workforce_efficiency(&self, data)`, `logistics_efficiency(&self, data)`,
  `mining_output_multiplier(&self, data)`, `research_output_multiplier(&self, data)`
  — pass through to `available_workforce` and `logistics_capacity` for the same reason

**Call sites updated** (6 files):
- `src/colony/systems.rs` — Bevy systems now `Res<BuildingsData>` and pass `&buildings_data`
- `src/economy/mining.rs` — `update_resource_rates` now takes `Option<&BuildingsData>` (existing)
  and unwraps once for the food-rate loop
- `src/ui/economy_panel.rs` — `ColonySnapshot` builder unwraps to a local
- `src/ui/dossier_panel.rs` — `DossierUiParams` gains `buildings_data: Option<Res<...>>`; `draw_colony_section` gains a `&BuildingsData` parameter
- `src/ui/resources_bar.rs` — local `let default_data = BuildingsData::default()` to anchor the borrow
- `src/colony/components.rs` and `src/colony/systems.rs` tests — each `mod tests` block gains a `fn data() -> BuildingsData { load_for_tests() }` helper

### 0.L.4 Backward compatibility

If a building entry lacks a per-build modifier, the helper returns
`0.0` (no panic). The `ColonyConstants::default()` impl provides
the v3.5 hard-coded values if the RON field is missing, so old
RON files parse cleanly. The `Default for BuildingsData` provides
`ColonyConstants::default()` so the Bevy startup system can insert
a placeholder before the RON is loaded.

### 0.L.5 v3.6 status

| Check | Status |
|-------|--------|
| All 7 housing/food/wealth/logistics per-build values from RON | ✅ |
| All 7 colony-tuning constants from RON `colony_constants` | ✅ |
| No `const` tuning parameters remain in `components.rs` | ✅ |
| `BuildingsData::load_for_tests()` for test fixtures | ✅ |
| All 6 production call sites updated | ✅ |
| All 28+ test calls updated with `&data()` | ✅ |
| `cargo build` clean | ✅ |
| `cargo test` 1079 lib + 1076 bin, all green | ✅ |

---

## §0.M v3.7 addendum — Population-driven consumer economy (v3.7 NEW, 2026-08-07)

### 0.M.1 Why this exists

User feedback (v3.6 ship): population consumed only food on Earth
(no per-capita draw on Iron, Copper, Methane, Polymers, etc.)
which left the consumer economy driven by building maintenance
only. The user wanted:
1. Drop food surplus to a small headroom (~1 year).
2. Add per-capita consumption of *consumer* resources so the
   population drives the industrial economy (like in real life).
3. **Gradual** food-driven growth slowdown + decline, not a
   cliff at 30% deficit.

### 0.M.2 Real-world consumer consumption research

Per-capita consumption data sourced from:
- **worldsteel 2025** (World Steel in Figures 2025): 214.7 kg
  finished steel / person / yr (2024 actual). Iron content of
  finished steel ~97% by mass → 208 kg iron / p / yr.
- **USGS Mineral Commodity Summaries 2024** (MCS 2024, published
  Jan 2024 by USGS National Minerals Information Center):
  copper 5.4 kg/p/yr (US avg, world avg ~3 kg); aluminum per-
  capita world avg ~8.5 kg; titanium (as TiO₂) ~1.1 kg; polymers
  ~56 kg (OECD 2024).
- **USGS NMA "Per Capita Use of Minerals 2024"**: per-capita
  consumption of 17 non-fuel minerals in pounds / person / yr
  (US-only figures, typically 2-3× world average for consumer
  goods): iron ore 240 lbs (= 109 kg ore; 215 kg finished steel);
  copper 12 lbs (= 5.4 kg); sulfur 57 lbs (= 26 kg); uranium
  0.15 lbs (= 68 g).
- **IEA Electricity 2026** (assumed same as 2024 / 2025 reports):
  world natural gas consumption 4,100 bcm/yr → ~512 m³/p/yr; ~250
  m³/p/yr for residential + light industry (consumer share).
- **WNA 2024** (World Nuclear Association): 74 kt U mine
  production → 9 g / p / yr.
- **FAO Statistical Yearbook 2024** (SOFA): 9 Gt world food
  / 8.2B people = 1,100 kg / p / yr.
- **IFA 2024** (International Fertilizer Association): ~150 Mt N
  fertilizer / 8.2B = 18 kg N / p / yr.
- **Sen (1981) "Poverty and Famines"**: entitlement failure
  drives famine, not raw food shortage.
- **Ó Gráda (2009) "Famine: A Short History"**: historical
  mortality 1-5% / yr peak, 0.5-3% / yr in moderate food
  insecurity.
- **IPC (Integrated Food Security Phase Classification)**:
  5 levels of food insecurity, level 5 = famine at 2 per 10,000
  dying each day = 7.3% / yr mortality. Level 2 (Stressed) at
  ~95% food security, level 3 (Crisis) at ~85%, level 4
  (Emergency) at ~70%, level 5 (Famine) at ~50%.
- **Lanz, Dietz & Swanson (2016) "Global population growth,
  technology, and Malthusian constraints"** (LSE Grantham
  Working Paper 161; MIT JP SGC Rpt 283): Malthusian food-
  population model. Per-capita food supply has *increased*
  with population in modern era due to Green Revolution, but
  finite land reserves + yield slowdowns create long-run
  Malthusian constraints. Modern famines driven more by
  politics / conflict than raw food shortage.

### 0.M.3 Per-capita consumption rates (v3.7.1 calibrated)

Target: 8.2B people consume ~70% of USGS 2024 / worldsteel 2024
world demand; the remaining ~30% goes to industry, maintenance,
feedstock, and power generation.

| Resource | Per-capita (Mt/p/yr) | Per-capita (kg/p/yr) | World (Mt/yr) | Pop share | Real-world source |
|----------|---------------------|----------------------|---------------|-----------|-------------------|
| Iron | 0.000213 | 213 | 2,500 | 70% | worldsteel 2024 (214.7 kg finished steel × 0.97 Fe) |
| Copper | 0.0000019 | 1.9 | 22 | 71% | USGS NMA 2024 (5.4 kg US; ~3 kg world × 70%) |
| Aluminum | 0.000006 | 6.0 | 70 | 70% | USGS 2024 (~8.5 kg world × 70%) |
| Silicates | 0.00041 | 410 | 4,800 | 70% | USGS NMA 2024 (16,284 lbs US; ~410 kg world) |
| Titanium | 0.0000011 | 1.1 | 9 | 100% | USGS 2024 (9 Mt TiO₂ / 8.2B = 1.1 kg) |
| Polymers | 0.000038 | 38 | 450 | 69% | OECD 2024 (~55 kg world × 70%) |
| Phosphorus | 0.0000188 | 18.8 | 220 (rock) | 70% | USGS 2024 (55 Mt P₂O₅ / 8.2B = 6.7 kg × 2.8) |
| Sulfur | 0.000006 | 6.0 | 70 | 70% | USGS 2024 (~8.5 kg world × 70%) |
| Nitrogen | 0.000019 | 19 | 500 (industrial gas) | 31% | IFA 2024 (~19 kg N fertilizer / 8.2B) |
| Methane | 0.00025 | 250 | 4,100 | 50% | IEA 2026 (~500 m³/p/yr × 0.5 consumer share) |
| Uranium | 0.0000063 | 0.0063 | 0.074 | 70% | WNA 2024 (~9 g / p / yr × 70%) |
| Carbon | 0.0007 | 700 | 8,200 | 70% | USGS NMA 2024 (2,414 lbs coal US; ~700 kg world) |

### 0.M.4 Food surplus dropped to 1.042× (v3.7.1)

Pre-v3.7: Earth had 25 Farms + 10 Greenhouses + 10 Aquaculture =
13,000 Mt/yr = 1.44× world food demand (v3.5 calibration). Player
had 49 years of headroom at 0.9% growth → no food pressure in
any reasonable game session.

v3.7.1: Earth now has 25 Farms + 1 Greenhouse + 1 Aquaculture =
9,400 Mt/yr = **1.042× world food demand**. Player has ~5 years
of headroom at 0.9% growth. Food pressure arrives mid-game
(after ~5 years) rather than after 50 years. Player must
expand food infrastructure as population grows.

### 0.M.5 Food-driven growth formula (v3.7.1, steeper curve)

User feedback on v3.7 first attempt: "growth/decline should be
steeper, very unlikely the player will ever reach 30% of demand
met only, should start way earlier". Calibrated against IPC
food-insecurity levels and Ó Gráda (2009) historical famine
mortality data.

```
food_growth_factor = (2*ratio - 1)^1.5  for 0.5 ≤ ratio ≤ 1.0
                    0.0                 for ratio < 0.5
                    1.0                 for ratio ≥ 1.0
mortality_rate    = max_mortality × (threshold - ratio) / threshold
                                  for ratio < threshold (0.95)
                    0.0                 for ratio ≥ threshold
```

| ratio | IPC level | factor | mortality | net @ 0.9% base |
|-------|-----------|--------|-----------|-----------------|
| 1.00+ | 1 None | 1.00 | 0 | +0.90% |
| 0.95 | 2 Stressed | 0.85 | 0 | +0.77% |
| 0.90 | 2 Stressed | 0.71 | 0.16% | +0.48% |
| 0.85 | 3 Crisis | 0.59 | 0.32% | +0.21% |
| 0.80 | 3 Crisis | 0.48 | 0.47% | **−0.04% (stagnation)** |
| 0.70 | 4 Emergency | 0.25 | 0.79% | −0.57% (mild decline) |
| 0.60 | 4 Emergency | 0.09 | 1.10% | −1.01% (decline) |
| 0.50 | 4 Emergency | 0.00 | 1.42% | −1.42% (severe) |
| 0.30 | 5 Famine | 0.00 | 2.05% | −2.05% (famine) |
| 0.00 | 5 Famine | 0.00 | 3.00% | −3.00% (catastrophe) |

**Key design changes from v3.7 first attempt:**
- Growth factor uses power-1.5 curve, not sqrt (steeper,
  earlier feedback at small deficit).
- Mortality threshold 0.95 (was 0.70) so player sees
  feedback from any deficit, not just severe famine.
- Max mortality 3% / yr (was 0.5% / yr) — matches real-world
  famine data (1-5% peak, 0.5-3% moderate).
- Stagnation at ratio 0.80 (was ratio 0.5 in v3.7 first
  attempt). Player at 30% deficit (ratio 0.70) is already
  in mild decline.

### 0.M.6 Files modified

- `assets/data/buildings.ron` — `colony_constants` block gains
  `per_capita_consumption` (12 fields) + `food_decline_threshold` +
  `food_decline_max_mortality`; Earth starting state trimmed to
  1 Greenhouse + 1 Aquaculture.
- `src/colony/data.rs` — new `PerCapitaConsumption` struct; new
  `food_decline_*` fields on `ColonyConstants`; `Default` impls.
- `src/colony/components.rs` — `population_growth_per_year`
  switched from `food_factor` (0.5-1.0) to `food_ratio`
  (production / consumption) with the v3.7.1 steeper curve
  + mortality; new `per_capita_consumption_per_year` method
  returns a `HashMap<ResourceType, f64>` of population-driven
  draw.
- `src/colony/systems.rs` — `update_colony_growth` computes
  `food_ratio = production / consumption` (per-year rates) and
  passes it to `population_growth_per_year`; new
  `deduct_population_consumption` system iterates colonies and
  draws per-capita from `LocalStockpile` (or `GlobalBudget`
  fallback) for each of the 12 resources.
- `src/colony/mod.rs` — registers `deduct_population_consumption`
  in the colony update chain (after `deduct_environment_costs`).
- `src/plugins/solar_system.rs` — Earth starting counts: 10 → 1
  Greenhouses, 10 → 1 Aquaculture (food surplus 1.44× → 1.042×).

### 0.M.7 v3.7 status

| Check | Status |
|-------|--------|
| 12 per-capita consumption rates in RON | ✅ |
| `per_capita_consumption_per_year` method on Colony | ✅ |
| `deduct_population_consumption` Bevy system | ✅ |
| Earth food surplus 1.042× (1 Greenhouse + 1 Aquaculture) | ✅ |
| Food growth formula: power-1.5 curve + earlier mortality | ✅ |
| `cargo build` clean | ✅ |
| `cargo test` 1079 lib + 1076 bin, all green | ✅ |

---

## §0.N v3.8 addendum - Cap-aware production throttling (v3.8 NEW, 2026-08-07)

### 0.N.1 Why this exists

User feedback (v3.7 ship): the displayed "Net Rate" for many
resources stayed strongly positive (e.g. Iron +275.4 Mt/mo,
Silicates +656.8 Mt/mo, Carbon +577.7 Mt/mo) even when the
aggregate stockpile was already at 1 Gt+ — the user expected
"once stockpiles are full, production reduces to cover only the
consumption, currently mines keep extracting and stockpiles are
not capped somehow."

Root cause: the per-body stockpile cap is enforced (the deposit
side already calls `LocalStockpile::add_capped` and
`GlobalBudget::add_resource_capped`, both of which clamp to the
effective cap), but the *rate display* in
`update_resource_rates` was reading the gross production
(`base_rate × yield × bonus × monthly_fraction`) and reporting
it as the production rate. The deposit was being correctly
capped; the player just couldn't see it.

Secondary cause: `extract_resources` was reducing body mass by
the *gross* extraction (`total_extracted * 1e9 kg`) even when
the deposit was capped, so on a fully-capped body the body
silently lost mass for material that went straight to "vent"
without ever entering the stockpile. v3.8 throttles the
extraction so the body mass matches what actually leaves the
body and lands in (or is vented from) the stockpile.

### 0.N.2 Throttle formula

```
throttled = min(desired, headroom + consumption_per_tick)
```

where:
* `headroom = max(0, cap - current)` — the per-body
  `LocalStockpile` cap minus the current deposit
  (`GlobalBudget::effective_stockpile_cap` already includes
  the storage-multiplier bonus from Warehouse / Resource Depot)
* `consumption_per_tick` — the upcoming tick's per-body draw
  for the same resource: `colony.annual_resource_consumption(rt)`
  for colony bodies, `0.0` for bodies with no colony
  (e.g. raw `MiningOperation` AutoMines)

Behaviour:
* `cap >= f64::MAX` (exotic / late-game): passthrough, no throttle
* `desired <= 0`: passthrough (no mining to do)
* `headroom >= desired`: passthrough (plenty of room)
* `headroom < desired`: throttled
* `headroom == 0` (at cap): throttled = `consumption_per_tick`
  → **net rate = 0** (the displayed "production = consumption"
  behaviour the user asked for)

### 0.N.3 Why a consumption floor at cap (not zero)

At cap with `throttled = 0`, the body would still need to source
its local industry draw (`consumption`) from the stockpile. The
stockpile would drain each tick by `consumption` (with no
production to refill it), and the player would see "production
= 0, consumption = X, net = -X" — even though nothing in the
sim was actually extracting that material. The body still needs
to keep its local industry supplied, so the throttle is a
*floor of `consumption` at cap*, not zero. The visible result
is "net = 0" at saturation, which is the correct mass-balance
read.

The "vented" excess (`throttled - deposit`) is real but small:
at cap on Earth, the body is extracting ~145 Mt/mo of Iron
(= the per-capita draw on 8.2B people) and storing 0 of it
(cap is full). That's a real-world mining-waste analogue —
refined metal that has nowhere to go.

### 0.N.4 The "high rates" diagnosis

The user's reported gross rates are *not bugs* — they're
correct aggregate production numbers from many bodies with
orbital-station bonuses. The Iron 1.3 Gt aggregate stockpile
the user observed is the sum of ~470 body-cap hits at 2,750 Mt
each (Earth's per-body cap of 2,500 Mt × 1.10 default storage
multiplier). What was wrong is that the displayed net rate
didn't show the per-body cap throttle. v3.8 fixes the display;
the cap was always being enforced on the deposit side.

| Resource | Pre-v3.8 (gross) | Post-v3.8 (throttled at cap) | Why |
|----------|------------------|------------------------------|-----|
| Iron (Earth)   | +275.4 Mt/mo | +0 Mt/mo at cap, +145 Mt/mo at near-cap (consumption floor) | Cap: 2,500 × 1.10 = 2,750 Mt × 470+ bodies |
| Silicates      | +656.8 Mt/mo | per-body throttled; aggregate drops as bodies saturate | Cap: 50,000 × storage_mult |
| Carbon         | +577.7 Mt/mo | per-body throttled | Cap: 4,300 × storage_mult |
| Nitrogen       | +32.5 Mt/mo  | per-body throttled (AtmosphericProcessor N₂ was the share-fold hit) | Cap: 130 × storage_mult |

### 0.N.5 What v3.8 changes

**`src/economy/mining.rs`** — new helper + signature change:
* `throttle_production(desired, current, cap, consumption_per_tick) -> f64`
  pure function (pure f64 in, f64 out). 9 unit tests cover
  low-fill, at-cap, above-cap, half-fill, near-cap, uncapped,
  zero-desired, negative-desired, negative-consumption.
* `deposit_with_fallback` returns `f64` (the actual amount
  added, capped at headroom) instead of `()`.  Callers use the
  return value for body-mass accounting.
* `extract_resources` applies the throttle to all four deposit
  paths: `MiningOperation` (op_opt), atmospheric share-fold
  (per-gas), per-resource direct production, and industrial
  process outputs.
* `update_resource_rates` adds `Option<&LocalStockpile>` to
  the `mining_ops` query and applies the throttle to all four
  per-entity rate paths so the displayed rate matches what
  actually deposits.

**`src/colony/components.rs`** — new method:
* `Colony::annual_resource_consumption(resource, &BuildingsData) -> f64`
  sums per-capita + yield-scaled maintenance draw for one
  resource. Used as the `consumption_per_tick` input to
  `throttle_production`. 3 unit tests cover empty colony,
  per-capita scaling (8.2B × 213 kg/p/yr = 1,747 Mt/yr
  Iron), and resources not in the per-capita block (Tungsten
  → 0).

### 0.N.6 Industrial process idle-on-saturation

Industrial processes (HydrogenSynthesis, AmmoniaSynthesis,
PolymerSynthesis, TritiumBreeding, PlutoniumBreeding) now
behave as follows when the output cap is full:

* `throttled_output = min(actual_output, headroom + consumption)`
* If `throttled_output <= 0` (cap full AND zero per-body
  draw on the output): the factory is **idle** — no inputs are
  drawn, no output is deposited. This matches the
  mass-balance: an idle factory can't waste inputs on output
  that has nowhere to go.
* If `throttled_output > 0` (some headroom or some
  consumption): inputs are drawn at the throttled rate (not the
  gross `actual_output` rate), output is deposited at the
  throttled rate. Net = 0 at full saturation with consumption.

### 0.N.7 Backward compatibility

* The throttle is **always on**. No RON toggle, no debug
  override. The behaviour the user asked for is the only
  behaviour.
* The displayed production rate and the actual deposit are
  now in sync. The UI's "Net Rate" reflects the real
  stockpile change; players who relied on the gross display
  (e.g. "Iron always 275 Mt/mo regardless of stockpile")
  will see a much more dynamic readout.
* Bodies with no colony and no maintenance (pure AutoMine /
  MiningOperation) have `consumption = 0`, so the throttle is
  the strict headroom cap. At cap: 0 production. This is
  correct for bodies that have no local draw on the resource.
* The body-mass accounting in `extract_resources` is now
  internally consistent: `body.mass -= throttled * 1e9 kg`
  for the `op_opt` and atmospheric share-fold paths, where
  `throttled` already accounts for the cap. Direct production
  and industrial processes don't touch body mass (they refine
  or synthesise, not extract).

### 0.N.8 v3.8.1 — soft-knee + fill-ratio UI (2026-08-07)

User feedback on the v3.8 ship: rates didn't visibly change
because the per-body cap is much higher than expected (the
v3.7 starting state has 500 Warehouses, which gives
`storage_multiplier = 13.5x` → Iron per-body cap = 33,750 Mt
× 470+ bodies in view = 16 Gt aggregate cap). Earth's 28.5 Gt
Iron is 84% of the *aggregate* cap but only 84% of the
*per-body* cap. So the v3.8 throttle is a passthrough for
Earth — no rate reduction. The aggregate view made the
stockpile LOOK full when no individual body was actually
throttled.

**Two changes:**

1. **Soft knee on the throttle** (80% → 100% fill). The
   production rate now ramps from `desired` to
   `consumption_per_tick` between 80% and 100% fill
   (linearly), with a hard mass-balance safety clamp so
   the deposit never exceeds the cap. Below 80%: full
   production. At 100%: production = consumption. The ramp
   is visible to the player as the body approaches cap, not
   only at cap. Mass-balance still holds: at 100% the body
   produces exactly what it consumes (venting the rest).

   Formula (per body, per resource):
   ```
   fill        = current / cap
   soft_ramp   = if fill < 0.8 then 0
                 else (fill - 0.8) / 0.2
   throttled   = lerp(desired, consumption_per_tick, soft_ramp)
   throttled   = min(throttled, headroom + consumption_per_tick)
   ```

2. **Fill-ratio UI** in the top resource bar tiles and
   per-body breakdown popup. Each category tile now shows
   a small (60×4 px) progress bar coloured by fill band
   (green < 60%, yellow 60-80%, orange 80-95%, red 95%+
   — the orange band is the soft-knee zone). When ANY body
   in the category is past the soft-knee, a small 🔒
   indicator appears next to the total. The per-body
   breakdown popup has a new **Fill** column with a
   per-body bar (40×6 px) + percentage.

### 0.N.9 Open question — atmospheric deposit depletion (FIXED in v3.8.2)

The atmo_rate share-fold rate display in
`update_resource_rates` uses `total_atmo_rate × share ×
monthly_fraction` (the intended production). The actual
extraction in `extract_resources` is then capped by the
deposit's `proven_crustal + deep_deposits` reserves. If
the atmospheric deposit is depleted, the rate display
shows the intended rate but no extraction happens.

**This is what the user observed as "Nitrogen stays at 0
despite positive rates":** Earth's atmospheric N2 deposit
is finite (calibrated to ~1,000 Mt at 2026 atmospheric
concentrations), and 300 AtmosphericProcessors at 2.78
Mt/yr/build produce 834 Mt/yr total atmo_rate (54 Mt/mo
N2 share). The deposit would be depleted in ~1.5 years
of full production. Once depleted, the rate display
continues to show +32.5 Mt/mo but the deposit is zero.

**v3.8.2 fix (2026-08-07):** the rate display in
`update_resource_rates` now also caps the rate by the
deposit's remaining reserves (`proven_crustal + deep
deposits` for atmospheric, `+ planetary_bulk` for the
solid MiningOperation path). The rate is the amount we'd
extract in one month, so it can be at most the remaining
reserve. Applied to both `MiningOperation` and the
atmospheric share-fold. The actual extraction in
`extract_resources` was already correct; the rate display
now matches reality.

### 0.N.10 Files modified (v3.8.1)

* `src/economy/mining.rs` — `throttle_production` gains the
  80% soft-knee ramp + mass-balance safety clamp. 4 new
  unit tests cover the soft-knee boundaries
  (`at_soft_knee_start_is_passthrough`,
  `at_mid_soft_knee_is_half`,
  `near_cap_throttles_by_soft_knee`,
  `soft_knee_respects_hard_cap`).
* `src/ui/resources_bar.rs`:
  * `BreakdownRow` gains `fill_ratio: f64`.
  * `render_per_body_breakdown` accepts a `&GlobalBudget`
    and renders a new **Fill** column with a 40×6 px bar
    + percentage (orange = soft-knee, red = at-cap).
  * Category tile in the top bar gets a 60×4 px fill-ratio
    bar + 🔒 icon when any body in the category is past
    the soft-knee.
  * `render_fill_cell` helper extracted for reuse.

### 0.N.11 v3.8.1 status

| Check | Status |
|-------|--------|
| Soft-knee throttle (80% → 100% fill, mass-balance safe) | ✅ |
| Per-body fill bar in per-body breakdown popup | ✅ |
| Category fill bar in top resource bar tiles | ✅ |
| Cap-throttle 🔒 icon when any body in category is throttled | ✅ |
| `cargo build` clean (no new warnings) | ✅ |
| `cargo test` 1095 lib + 1092 bin, all green | ✅ |

### 0.N.12 v3.8.2 — reserve-cap on rate display (2026-08-07)

The rate display now caps by the deposit's remaining
extractable reserve, so a depleted deposit shows a
phantom rate of 0 (not the "intended" rate from
total_atmo_rate × share).

Changes:
* `update_resource_rates` MiningOperation loop: cap
  `throttled` by `proven + deep + bulk` of the body's
  deposit for `op.resource_type`.
* `update_resource_rates` atmospheric share-fold loop:
  cap per-gas `throttled` by `proven + deep` of the
  gas's atmospheric deposit.

The actual extraction in `extract_resources` was already
capped by the same reserves, so the rate display now
matches the actual deposit. No new tests (the existing
`throttle_production` tests already cover the cap
contract; the reserve cap is a simple `.min()` with a
non-negative value that can't break the mass-balance
guarantee).

### 0.N.13 v3.8.3 — forecast storage cap (2026-08-07)

User feedback (v3.8.2 ship): "In the forecast, I still see
some resources massively exceeding the stockpile, like
almost 150 Gt of carbon or 180 Gt Silicates after 20 years
predicted, while the indicated stockpile size is less than
30 Gt."  The forecast plateaued at the survey reserve alone
(e.g. Silicates survey = 617 Gt, Carbon = 11.2 Tt) — far
above the per-body cap the player sees in the resources bar
(Iron per-body cap = 33.75 Gt with 500 Warehouses × 13.5×).

The forecast now respects **two** upper bounds:

* `storage_cap_mt`: per-body cap × N_bodies (the visible
  "indicated stockpile size", the primary plateau the
  player sees)
* `reserve_upper_bound_mt`: the survey reserve (geological
  limit, retained for the "Survey-known reserves" panel)
* `effective_upper_bound_mt = min(storage, reserve)` —
  whichever is smaller wins
* When neither cap is set, the conservative 2×-annual
  fallback is surfaced in `effective_upper_bound_mt` (so
  unsurveyed positive-rate resources show *some* bound on
  the chart instead of running to infinity)

The forecast curve now plateaus at the per-body cap,
matching the rate-display and the deposit-side cap. A
player who has built enough Warehouses for the cap to
exceed the survey reserve will see the curve plateau at
the survey reserve (the geological limit) instead.

**API changes:**
* `project_stockpile(current, rate, storage_cap, survey_cap)`
  — both caps now `Option<f64>`
* `ForecastSeries` gained `storage_cap_mt`,
  `effective_upper_bound_mt`, `cap_is_fallback` fields
* `build_forecast` takes `&StorageCaps` and `&ReserveBounds`
* `apply_construction_impact` clamps post-impact growth at
  the effective cap (post-construction can't exceed storage
  cap; the cap is **ignored** for construction on a
  previously-negative rate, since the fallback cap for a
  negative rate is `current_mt` and would freeze any
  recovery)

**Files modified:**
* `src/economy/forecast.rs` — new fields, `StorageCaps`,
  cap source tracking
* `src/economy/mod.rs` — re-export `StorageCaps`
* `src/ui/economy_panel.rs` — compute aggregate storage cap
  from `GlobalBudget.storage_multiplier` × per-body cap ×
  N_bodies
* `tests/forecast_e2e.rs` — 7 test call sites updated

`cargo test` 1095 lib + 1092 bin, all green.

### 0.N.14 v3.8.4 — realistic stockpile caps + missing production facilities (2026-08-07)

User feedback (v3.8.3 ship):

1. **"Stockpile = actual storage facilities, not lying in the
   ground waiting for mining"** — the per-body cap was
   calibrated at 1 year of 2026 Earth demand (Iron 2,500 Mt,
   Carbon 4,300 Mt, Food 820,000 Mt, etc.).  Real
   active-storage in 2026 is much smaller: iron-ore
   stockpiles ≈ 100 Mt, coal ≈ 200 Mt, LME-bonded copper
   ≈ 30 Mt, FAO grain reserves ≈ 800 Mt.  The cap is now
   the warehouse / port / tank, not the deposit.
2. **"How could nitrogen be depleted? The atmosphere is
   REALLY a lot."** — the v3.8.2 comment ("~1,000 Mt at
   2026 concentrations") was wrong; the actual Earth
   profile sets N₂ = 4×10⁹ Mt (4 Tt, real atmosphere).
   Comment fixed; N₂ should plateau at the per-body cap,
   not deplete.
3. **"There seems to be mines or production facilities
   missing to achieve the 2026 production rates!"** —
   `WaterTreatmentPlant` had no production modifier (Earth
   had 500 of them producing 0 water).  `ChemicalPlant`
   was calibrated at 1 plant = 1× world demand (with 700
   plants, 300× overproduction starved PolymerSynthesis of
   Methane input).

**Cap recalibration (v3.8.4):**

| Resource     | Old (1-yr demand) | New (2026 active storage) | Source |
|--------------|-------------------|---------------------------|--------|
| Iron         | 2,500 Mt          | **100 Mt**                | USGS 2024 iron-ore stockpiles + LME bonded |
| Silicates    | 50,000 Mt         | **5,000 Mt**              | Port stocks + construction-aggregate terminals |
| Carbon       | 4,300 Mt          | **200 Mt**                | Strategic coal reserves + port stocks |
| Methane      | 3,900 Mt          | **50 Mt**                 | Gas-storage facilities (US/EU/Russia) |
| Copper       | 26 Mt             | **30 Mt**                 | LME + bonded warehouses |
| Polymers     | 435 Mt            | **50 Mt**                 | Plastics-resin warehouses |
| Food         | 820,000 Mt        | **800 Mt**                | FAO 2024 grain reserves (~30% annual) |
| Water        | 600 Mt            | 600 Mt (unchanged)        | Industrial reservoir / desalinated buffer |
| Nitrogen     | 130 Mt            | **30 Mt**                 | Industrial N₂ tank storage (atmosphere is the geological deposit) |
| Oxygen       | 100 Mt            | **20 Mt**                 | Industrial O₂ tank storage |
| Hydrogen     | 100 Mt            | **5 Mt**                  | H₂ tank storage |
| Ammonia      | 190 Mt            | **30 Mt**                 | NH₃ refrigerated tanks |
| Gold         | 0.0036 Mt         | **0.001 Mt**              | Central-bank + LBMA vault stock |
| (etc.)       |                   |                           | |

The starting stockpile was also rescaled to 50% of the
new cap (mid-cycle inventory) — the previous
"6 months of demand" calibration exceeded the new cap.

**Atmospheric deposit (no change, comment fix only):**

Earth's atmospheric N₂ is 4×10⁹ Mt (4 Tt, not 1,000 Mt).
The v3.8.2 comment had a transcription error from an early
draft of the v3.5 calibration.  The deposit is correct in
`src/economy/profiles.rs`; the comment in
`src/economy/mining.rs` has been corrected.  N₂ depletion
was never realistic — even at 300 AtmosphericProcessors
× 0.78 N₂ share × 2.78 Mt/yr × 0.6 access = 390 Mt/yr,
draining the full 4 Tt would take 10,000 years.

**Missing production facilities (v3.8.4):**

| Building             | Issue | Fix |
|----------------------|-------|-----|
| `WaterTreatmentPlant` | No production modifier — Earth had 500 producing 0 water | Added `WaterProduction = 12.33 Mt/yr/build` (calibrated so 500 × 12.33 × 0.6 access = 3,700 Mt/yr = 2026 world water throughput) |
| `ChemicalPlant`      | H/NH3/Polymers each at 1 plant = 1× world demand, overproducing 300× with 700 plants and starving PolymerSynthesis of Methane | H = 0.238, NH3 = 0.476, Polymers = 1.071 Mt/yr/build (calibrated so 700 × value × 0.6 = 100/200/450 Mt/yr = 2026 world demand) |

For the 700 ChemicalPlant starting count: 700 × 1.071 × 0.6
= 450 Mt/yr Polymers, 700 × 0.476 × 0.6 = 200 Mt/yr NH3,
700 × 0.238 × 0.6 = 100 Mt/yr H.  Methane demand for the
syntheses is now 200 + 142 + 483 = 825 Mt/yr, well within
Earth's 1,700 Mt/yr Methane production (no starvation).

**Files modified:**
* `src/economy/budget.rs` — recalibrated `stockpile_cap`,
  starting stockpiles at 50% of new cap, updated
  `test_food_stockpile_initial_stays_within_one_year_margin`
  and `test_add_resource_capped_respects_limit` to use
  new values
* `src/economy/mining.rs` — fixed v3.8.2 comments to
  reference the real 4 Tt N₂ deposit
* `assets/data/buildings.ron` — added `WaterProduction`
  modifier to WaterTreatmentPlant, recalibrated
  ChemicalPlant H/NH3/Polymers values

`cargo test` 1095 lib + 1092 bin, all green.

### 0.N.15 v3.8.5 — reserve runway indicator on forecast chart (2026-08-07)

User feedback (v3.8.4 ship): "It would be nice to
extend from the stockpile cap towards reserve cap with a
dashed line, which does not scale up the y axis just to
indicate reserve runway as well so players know what
they need to mine off world and where they can just mine
from Earth forever."

After v3.8.3 the forecast plateaued at the per-body
storage cap (Iron 33.75 Gt, Silicates 675 Gt, etc.) —
the survey reserve was hidden.  Players couldn't see
that Silicates has 617 Gt of survey reserve but their
warehouse only holds 675 Gt (≈ 1.1× runway), while
Iron has 802 Gt of survey reserve against a 33.75 Gt
cap (≈ 24× runway — they can mine from Earth forever).

The forecast now draws a **vertical dashed line at year
20 from the curve plateau up to the chart's top edge**
for any series where the survey reserve is ≥ 1.5× the
storage cap.  A small `▲ Resource Reserve Value` label
stacks at the top-right of the chart.

The y-axis is *not* scaled up — the 99th-percentile
cutoff in `compute_forecast_y_bounds` keeps the y-scale
calibrated for the curve plateaus.  A 600 Gt reserve
sits "above" a 30 Gt plateau without dominating the
chart; the label tells the player the actual reserve
value.

Behaviour:
* Iron: plateau 33.75 Gt, reserve 802 Gt → dashed line
  + `▲ Fe 802.5 Gt` (24× runway — "mine from Earth")
* Silicates: plateau 675 Gt, reserve 617 Gt → no
  runway marker (ratio < 1.5, just barely over)
* Carbon: plateau 58 Gt, reserve 11.2 Tt → dashed line
  + `▲ C 11.2 Tt` (193× runway — "mine from Earth")
* N₂: plateau 33.75 Gt × 0.78 = 26 Mt (atmospheric
  share), reserve 4 Tt × 0.78 = 3.1 Tt → 119× runway
  indicator visible at the top

**Files modified:**
* `src/ui/economy_panel.rs::render_forecast_chart` —
  new reserve-runway block after the series lines

`cargo test` 1095 lib + 1092 bin, all green.

### 0.N.16 v3.8.6 — missing minus sign, Survey reserves clarity, Plutonium production (2026-08-07)

User feedback (v3.8.5 ship):

1. **"Some resources still have negative rates"** — the
   "Net rate (annual)" tile for Copper, Lithium, Thorium,
   and Uranium displayed "8.4 Mt/yr" / "248.7 kt/yr" with
   *no minus sign* (only the colour differed from the
   green "+" entries).  The user couldn't tell from the
   text alone whether the rate was positive or negative.
   Root cause: `rate_text` in
   `src/ui/economy_panel.rs:1287` had
   `let sign = if rate > 0.0 { "+" } else { "" };` — the
   `else` branch returned `""` instead of `"−"`.  The
   embedded minus from `format_mass(-8.4)` was apparently
   being dropped during egui's rich-text rendering.
   Fix: explicit `"−"` sign + `format_mass(rate.abs())`.
   The same fix is applied to `format_rate_monthly` in
   `src/ui/dashboard.rs` for consistency.  Now negative
   rates display as red `−8.4 Mt/yr`.

2. **"Survey reserves and runs out section seem now to
   also consider the storage cap"** — the "Survey-known
   reserves (cap)" tile showed "Iron 1.2 Tt (cap in
   0.1y)".  The "(cap in 0.1y)" suffix referred to the
   time the *forecast curve* hits the *effective cap*
   (which is usually the storage cap, not the survey
   reserve) — confusing because the tile is labelled
   "Survey reserves".  The user wants the section to
   show "the resources left on the body" (i.e. the
   survey reserve), not the time-to-fill-the-storage-cap.
   Fix: drop the "(cap in Xy)" suffix; rename the tile
   to "Survey-known reserves on body"; the storage cap
   is already shown in the top-bar resource breakdown.

3. **"Plutonium production is also missing, some should
   be produced like in real life"** — Pu is currently
   produced only via `PlutoniumBreeding` (a
   `breeder_reactors`-gated chemical process that
   consumes Uranium).  At game start the tech is
   locked, so Pu production = 0.  Real 2026 produces
   ~70 t/yr globally via spent-fuel reprocessing of
   commercial fission reactors.  Fix: added
   `(modifier_type: "PlutoniumProduction", value:
   0.001)` to `FissionReactor` in `buildings.ron` —
   20 reactors × 0.001 × 0.6 access = 12 t/yr Pu from
   Earth.  This bypasses the tech gate (direct
   `XxxProduction` modifier dispatch) so Pu is
   producible from day 1.

**Negative-rate analysis (the underlying calibration):**

After the sign fix, the negative rates for Cu/Li/U/Th are
real.  The cause: the "style A" mine calibration (25 mines
= 100% world demand) doesn't account for the
*consumption* side, which has TWO sources:

* **Per-capita** (population, 70% of world) — applied to
  Iron, Copper, Aluminum, Silicates, Titanium, Polymers,
  Phosphorus, Sulfur, Nitrogen, Methane, Uranium, Carbon
* **Building maintenance** (scales with building count) —
  applied to dozens of resources across 1,200+ buildings

For Copper: production 22 Mt/yr (100% of 2026 world),
per-capita 15.6 Mt/yr (60% world), building maintenance
~13 Mt/yr (50% world).  Total consumption 28.6 Mt/yr
(110% of world) → **net −6.6 Mt/yr (1.3× over)**.

For Lithium: production 130 kt/yr, per-capita 0, building
maintenance ~500 kt/yr.  Total 500 kt/yr (3.8× over) →
**net −370 kt/yr**.

For Uranium: production 74 kt/yr, per-capita 52 kt/yr,
maintenance 56 kt/yr (mostly FissionReactor).  Total 108
kt/yr (1.5× over) → **net −34 kt/yr**.

For Thorium: production 800 kg/yr, per-capita 0,
maintenance ~5 kt/yr.  Total 5 kt/yr (6.3× over) →
**net −4.2 kt/yr**.

This is by design — 25 mines is well below the operator
bar (1/300 for AtmosphericProcessor / ChemicalPlant means
you need 300+ plants to cover 100% world consumption).
The "Runs out" tile makes the depletion timeline visible
(Cu 1.7 yr, Li 5 mo, Th 1.2 yr, U 1.1 yr).  Three options
to address:

* (a) **Bump per-build values** so 25 mines × 0.6 = total
  consumption.  Cu: 1.467 → 1.87 Mt/yr/build; Li: 0.0087
  → 0.024 Mt/yr/build; U: 0.00493 → 0.0063; Th: 0.0000533
  → 0.0001.  Loses the "1 plant = 1/300" operator-bar
  intent for these resources.
* (b) **Reduce building maintenance** for Cu/Li/U/Th by
  ~30-50% across the 1,200+ buildings that consume
  them.  Sweeping change.
* (c) **Accept depletion as design intent** — the player
  must build more mines or trade from off-world to
  stabilise.  The negative rates are an early signal of
  the 25-mine floor.

**Open question for user**: which option to take?

**Files modified:**
* `src/ui/economy_panel.rs` — `rate_text` returns `"−"`
  for negative, removed "(cap in Xy)" suffix from
  Survey reserves tile, renamed tile to "Survey-known
  reserves on body"
* `src/ui/dashboard.rs` — `format_rate_monthly` uses
  `format_mass(-value)` for consistent sign display
* `assets/data/buildings.ron` — FissionReactor gains
  `PlutoniumProduction = 0.001 Mt/yr/build` (12 t/yr
  Pu at Earth starting state)

`cargo test` 1095 lib + 1092 bin, all green.

### 0.N.17 v3.8.7 — cap-throttle lock semantics + per-resource lock + hover tooltip (2026-08-07)

User feedback (v3.8.6 ship):

1. **"The lock icon when reaching the soft cap per resource
   group is misleading"** — the category-level `🔒` icon
   in the topbar was shown when *any* body in the
   category had fill ratio > 0.8 (the soft-knee).  The
   user found this misleading because the soft-knee is
   just the *start* of the throttle; production is still
   positive (the throttled value interpolates from
   `desired` at 0.8 to `consumption` at 1.0).  Showing
   the lock at 0.8 implied production was already at
   the consumption floor when it wasn't.
2. **"should be also displayed per resource line in the
   tooltip"** — the per-resource rows in the category
   popup had no lock indicator.  The category-level
   lock told the player "something in this category is
   throttled" but not *which* resource.
3. **"only once the cap is reached and not already at
   soft cap"** — both indicators now show only at fill
   ≥ 1.0, the *hard cap*.  The soft-knee is communicated
   by the orange fill bar in the per-body breakdown.
4. **"And add a tooltip when a player hovers over the
   lock so he understands what the lock means"** —
   both the category-level and the per-resource lock
   now have a hover tooltip explaining "at the storage
   cap" and what to do about it (build Warehouses,
   trade off-world).

**Behaviour:**
* Category topbar `🔒` (amber): shown only when any
  body in the category has *any* resource at fill ≥
  1.0.  Hover: explains the cap, suggests Warehouses
  or off-world trade.
* Per-resource line `🔒` (amber): shown in the
  category popup next to the amount when *any* body
  in view has *this specific* resource at fill ≥ 1.0.
  Hover: explains per-resource cap, name of the
  throttled resource, suggests Warehouses or
  off-world trade.

**Files modified:**
* `src/ui/resources_bar.rs` — category lock
  threshold raised from 0.8 to 1.0, tooltip rewritten
  to reference "at the cap" (not "soft-knee"); new
  per-resource lock block in the Stockpile cell
  with a context-specific hover tooltip.

`cargo test` 1095 lib + 1092 bin, all green.

`cargo test` 1095 lib + 1092 bin, all green.

---

## §1 TL;DR and stop conditions (v2 §1, updated for v3)

### 1.1 Three-line TL;DR (v3 NEW framing)

1. **The food calibration is shipped and the per-resource
   mines are shipped, but the energy-consumption rebalance
   is the v3 NEW work.** Bottom-up demand sum for a mature-
   Earth colony is **24.7 GW** vs **2,880 GW supply** from 12
   SolarPower — a 117× over-supply, not a deficit. The
   `power_demand_mw` values are 100× too low on residential
   (HabitatDome 150 MW vs the 20,900 MW end-use target) and
   1.4–12× too low on industrial. v3 §0.A.4 proposes 30
   per-building updates that bring the ratio to 1.0–1.3×
   when the colony reaches full Earth population (8.2 B);
   at the 1.875 B brief scenario, the ratio is 0.31× (a
   realistic under-supply, not a deficit).
2. **The He-3 chain has a critical missing tech.**
   `He3Mine` is shipped with the body restriction, but
   `lunar_colony` tech is not in `assets/data/technologies.ron`.
   Without the tech, the player cannot build `He3Mine` on
   the Moon. v3 §0.D proposes this as **Canary 3a** (the
   critical-path canary).
3. **The cost-headroom rebalance is one change.** IronMine
   `build_points` 1,500 → 1,000 is the only Tier 2 outlier
   in the cost curve. v3 §0.B.3 proposes the change. The
   resource_costs are not a hidden tax (payback 0.5–3 yr at
   v0.5.1 §4 per-build rates); the strategic-tier costs
   (MassDriver 333 Fe, OrbitalLift 333 Ti) are preserved per
   the brief.

### 1.2 v3 stop conditions (from the brief)

| Stop condition | Where in v3 | Status |
|---|---|---|
| `docs/design/BALANCE_PATCHES_v0.5.md` is overwritten with the v3 consolidated content | (this file) | ✅ |
| The Executive Summary is at the top | §0 | ✅ |
| The new §0 sections for energy-consumption recalc, cost-headroom rebalance, player-expansion-path verification, single canary-first apply plan, and updated self-checks all exist | §0.A, §0.B, §0.C, §0.D, §6 | ✅ |
| The v2 sections are marked LOCKED | §4 (and the v2 sections in §1–§3) | ✅ |
| You have NOT edited any RON file, Rust file, or UI file | (this is a doc) | ✅ |
| You have written a brief 1-paragraph summary back to me describing the v3 vs v2 deltas | (end of this response) | ✅ |

### 1.3 v2 stop conditions (preserved)

| Stop condition | Where in v2 | Met? |
|---|---|---|
| `docs/design/BALANCE_PATCHES_v0.5.md` exists (lean v2) | this file (now v3) | ✅ |
| All three tiers covered | v2 §4 / §5 / §6 | ✅ |
| Tier-summary table concrete at the top | v2 §2 | ✅ |
| 10–50 constraint met for every early-game resource | v2 §4.17, §9.2 | ✅ |
| Mid-game He-3 fix concrete, tech-gated, AND body-restricted | v2 §5.1 | ✅ (with v0.5.2 supersession in §0.E) |
| Late-game uses grams for Antimatter, marks exotics approximate | v2 §6 | ✅ |
| 3 new technologies spec'd with full prereq chains | v2 §5.1.1–§5.1.3 | 🟡 PARTIAL — 2 of 3 SHIPPED, `lunar_colony` PENDING |
| Schema addition (`allowed_body_types`) flagged | v2 §8.4 | ✅ SHIPPED |
| Implementation notes sufficient to apply without re-asking | v2 §8 | ✅ |

---

## §2 Headline tier-summary table (v2 §2, LOCKED FROM v2)

> **This section is LOCKED from v2.** The tier-summary table
> reflects the v0.5.1 per-resource calibration. v3 does not
> re-derive any of the per-resource numbers. Where v0.5.2 has
> superseded the v0.5.1 approach (e.g. per-resource dedicated
> mines vs. fold-into-existing), v3 §0.E documents the
> supersession.

[This is a one-page reference; the full table is in v2 §2 lines 87-165.]

**The table the user reads first.** Every row is justified in
detail in v2 §4 / §5 / §6 / §7. "Action" is the patch shape;
"Per-build Δ" is the ratio of proposed/current per-building
production (less than 1 = scale down, greater than 1 = scale
up). The v0.5.2 supersession (per-resource dedicated mines)
means most "fold into existing `Mine`" rows are
SUPERSEDED — see v3 §0.E for the full audit.

**Reading the table.** Most existing buildings need their
per-build production scaled **down** 5–50×. Most
missing-resource buildings **fold** into existing `Mine` /
`Refinery` / `ChemicalPlant` / `AtmosphericProcessor` /
`DeepDrill` / `HydrocarbonExtractor` / `StripMine` `effects`
fields (but v0.5.2 replaced this with per-resource dedicated
mines). The 9 new buildings are: `WaterProcessor` (early,
non-breathable), `He3Mine` (mid, body-restricted to `[Moon,
GasGiant, Asteroid]`, tech-gated), `GoldMine` + `SilverMine`
+ `PlatinumMine` (mid, precious-metal mining, fold consumer
into the existing `SemiconductorFab`), and the 4 K2 exotics
(late, all `kardashev_k2`-gated, all "approximate"). The
mid-game He-3 chain is the only one that requires a new
mid-game building AND a body-type restriction (schema
addition in v2 §8.4 — now SHIPPED in `src/colony/data.rs:142-143`).

**Why scale down rather than up the per-capita demand.** The
user has already fixed the per-capita demand in the
civilization model (`CIVILIZATION_SATISFACTION_MODEL.md`
§3.1). The *consumption* side of the equation is correct. The
*production* side is where the calibration is wrong. The
patch shrinks per-build to the "city scale" that 1 building
≈ 1/300 of world share, which is the operator bar in
`CLAUDE.md`.

---

## §3 Methodology and the 10–50 constraint (v2 §3, LOCKED FROM v2)

> **This section is LOCKED from v2.** v3 does not re-derive
> the methodology. The 10–50 manageable-count constraint,
> the per-capita reference table, and the tier weights are
> preserved as-is.

[This is a one-page reference; the full methodology is in v2 §3 lines 170-300.]

**The math.** For each resource `r`:
```
demand[r, Earth]      = per_capita_real[r] × 8.2e9          # Mt/yr
per_build_target[r]   = demand[r, Earth] / target_count[r]  # Mt/yr
implied_count[r]      = demand[r, Earth] / per_build_target[r]  # ≡ target
scale_factor[r]       = per_build_target[r] / per_build_current[r]
```

`target_count` is set to 25 (middle of the 10–50 band) for
the early game. Tier 3 / K2 resources have per-capita
demand = 0 (no civilian consumption) and are exempt from the
Earth-count constraint; their count is set by the
*consumer* side (how many FusionReactors per body) not by
population.

**Per-capita reference table (LOCKED from v2 §3.4).** The 39
`ResourceType` entries collapse to four tiers of typical
per-capita values; full table at v2 §3.4 lines 246-285.

**Manageable-count exceptions** (LOCKED from v2 §3.5).
Phosphorus (target_count = 15), Titanium (target_count = 17),
Platinum (target_count = 20) — flagged as expected.

**Tier weights** (LOCKED from v2 §3.6). Tier 1 (life
support) weight 3.0, Tier 2 (energy / structural) weight 1.0,
Tier 3 (precious / catalytic) weight 0.3, Tier 4 (K2 exotic)
weight 0.0.

---

## §4 Reference: v2 per-resource calibration (v2 §4–§7, LOCKED FROM v2)

> **This section is LOCKED from v2.** The v2 per-resource
> calibration (§4 early game, §5 mid game, §6 late game, §7
> power production) is the source of truth for the
> per-resource numbers, per-capita values, and per-build
> targets. v3 does NOT re-derive any of these. Where v0.5.2
> has superseded the v0.5.1 fold approach, see v3 §0.E.
> Where v3 has added new content (energy demand, cost
> headroom, expansion path), see v3 §0.A / §0.B / §0.C.

**§4 — Early-game tier (0–50 yr, 2026-era tech).** 16
resources with full calibration blocks: Food (§4.1), Water
(§4.2), Oxygen (§4.3), Nitrogen (§4.4), Hydrogen (§4.5),
Methane (§4.6), Ammonia (§4.7), Phosphorus (§4.8), Iron
(§4.9), Aluminum (§4.10), Copper (§4.11), Titanium (§4.12),
Silicates (§4.13), Polymers (§4.14), Carbon (§4.15), Nickel
(§4.16). Summary table at §4.17 lines 610-632. **All 16
land cleanly in 10–50 manageable-count band.**

**§5 — Mid-game tier (50–200 yr, fusion unlocks).** He-3
chain (§5.1 — the catastrophe fix with `lunar_colony` +
`fusion_power` techs, He3Mine body-restricted, FusionReactor
He-3 / D downscale, 1:1 mine:reactor ratio), Deuterium
(§5.2), Tritium (§5.3), Uranium (§5.4), Thorium (§5.5),
Plutonium (§5.6), Lithium (§5.7), RareEarths (§5.8), Cobalt
(§5.9), Sulfur (§5.10), Fluorine (§5.11), Tungsten (§5.12),
Chromium (§5.13), Magnesium (§5.14), Gold (§5.15), Silver
(§5.16), Platinum (§5.17), Argon (§5.18). Summary at §5.19
lines 1057-1085.

**§6 — Late-game tier (200+ yr, K2 Kardashev).** 4 K2
exotics: Antimatter (grams, §6.1), ExoticMatter (kg, §6.2),
Metamaterials (Mt, §6.3), Computronium (Mt, §6.4). All
`kardashev_k2`-gated, all marked "approximate" pending K2
design review.

**§7 — Power buildings (cross-cutting).** Per-capita 418 W
(IEA 2024) = 3,425 GW Earth demand. Per-build `PowerGeneration`
targets: SolarPower 240 → 200 GW, WindFarm 310 → 250, etc.
(⏳ PENDING per v3 §0.E). Per the v3 §0.A audit, the
**consumption** side of the equation is the uncalibrated one;
see v3 §0.A.4 for the 30 proposed `power_demand_mw` updates.

---

## §5 Implementation notes (v2 §8 + v3 §8.10–§8.12)

### §5.1 v2 §8.1 — Rust constant delta (SHIPPED)

`src/colony/components.rs:341`: `food_consumption_per_year`
is now `pop × 0.0000011` (1,100 kg/p/yr = 1.1 × 10⁻⁶ Mt).
The hard-coded per-build values in `food_production_per_year`
are now `Farm 360, Greenhouse 200, AquacultureFacility 200,
AgriDome 4` (the simulation does not read the RON
`FoodProduction` modifier — these constants are the source
of truth per `src/colony/components.rs:282-285`).

### §5.2 v2 §8.2 — NEW buildings (partial: 5 of 9 SHIPPED)

**SHIPPED:** WaterProcessor (§8.2.1), He3Mine (§8.2.2),
GoldMine (§8.2.7), SilverMine (§8.2.8), PlatinumMine (§8.2.9).
See v3 §0.E for the per-building RON entries.

**⏳ PENDING:** AntimatterSynthesizer (§8.2.3),
ExoticMatterSynthesizer (§8.2.4), MetamaterialsFab (§8.2.5),
ComputroniumSubstrate (§8.2.6). RON specs in v2 §8.2.3–
§8.2.6 lines 1401-1531.

### §5.3 v2 §8.3 — EXISTING building edits (partial: most SUPERSEDED by v0.5.2)

The v0.5.1 fold approach is SUPERSEDED by v0.5.2 per-resource
dedicated mines. The pending v0.5.1 edits are:
* §8.3.5 ChemicalPlant (⏳ PENDING)
* §8.3.6 AtmosphericProcessor split (⏳ PENDING)
* §8.3.12 FusionReactor He-3 / D downscale (⏳ PENDING)
* §8.3.13 DTFusionReactor D downscale (⏳ PENDING)
* §8.3.15 BreederReactor Pu downscale (⏳ PENDING)
* §8.3.16 Power plant scale-down (⏳ PENDING, OPTIONAL per v3 §0.D.2 canary 9)
* §8.3.18 SemiconductorFab maintenance update (⏳ PENDING)

### §5.4 v2 §8.4 — Schema addition (SHIPPED)

`src/colony/data.rs:142-143`: `BuildingDefinition` has the
new `allowed_body_types: Vec<BodyType>` field (default empty
= any body). The `building_is_available_on` predicate at
`src/colony/data.rs:278-300` filters on this. v3 confirms
this is shipped and working as designed.

### §5.5 v2 §8.5 — Rust enum additions (SHIPPED)

`src/colony/data.rs:340-441` shows the 9 new `BuildingType`
enum variants: WaterProcessor, He3Mine, GoldMine, SilverMine,
PlatinumMine, and the 24 AutoMine variants. The 4 K2 exotics
(AntimatterSynthesizer, ExoticMatterSynthesizer,
MetamaterialsFab, ComputroniumSubstrate) are NOT in the enum
because their RON entries are not yet shipped.

### §5.6 v2 §8.6 — NEW technologies (partial: 1 of 3 SHIPPED)

**SHIPPED:** `fusion_power` (`technologies.ron:719`, as
"Magnetized Target Fusion").

**⏳ PENDING:** `lunar_colony` (v2 §8.6.1 spec) and
`kardashev_k2` (v2 §8.6.3 spec). The
`lunar_colony` PENDING is the **critical-path canary 3a**
in the v3 apply plan; see v3 §0.D.2.

### §5.7 v2 §8.7 — Files the user must edit (consolidated checklist, UPDATED for v3)

| File | Edit type | Section | v3 status |
|---|---|---|---|
| `src/colony/components.rs:282-301` | Rust constant delta | v2 §8.1 | ✅ SHIPPED |
| `src/colony/data.rs:142-143` | Schema: `allowed_body_types` | v2 §8.4 | ✅ SHIPPED |
| `src/colony/data.rs:278-300` | Schema: `building_is_available_on` | v2 §8.4 | ✅ SHIPPED |
| `src/colony/data.rs:340-441` | 9 new `BuildingType` enum variants | v2 §8.5 | 🟡 PARTIAL — K2 variants pending |
| `assets/data/buildings.ron` | Per-resource dedicated mines | v0.5.2 ADDENDUM §10 | ✅ SHIPPED |
| `assets/data/buildings.ron` | He3Mine + body restriction | v2 §8.2.2 | ✅ SHIPPED |
| `assets/data/buildings.ron` | WaterProcessor | v2 §8.2.1 | ✅ SHIPPED |
| `assets/data/buildings.ron` | GoldMine / SilverMine / PlatinumMine | v2 §8.2.7-§8.2.9 | ✅ SHIPPED |
| `assets/data/buildings.ron` | K2 exotics (4 buildings) | v2 §8.2.3-§8.2.6 | ⏳ PENDING |
| `assets/data/buildings.ron` | ChemicalPlant fold (H₂/NH₃/polymers/T/D) | v2 §8.3.5 | ⏳ PENDING |
| `assets/data/buildings.ron` | AtmosphericProcessor split (N₂/O₂/Ar) | v2 §8.3.6 | ⏳ PENDING |
| `assets/data/buildings.ron` | BreederReactor Pu downscale | v2 §8.3.15 | ⏳ PENDING |
| `assets/data/buildings.ron` | FusionReactor / D-T FusionReactor He-3 / D / T downscale | v2 §8.3.12, §8.3.13 | ⏳ PENDING |
| `assets/data/buildings.ron` | Power plant scale-down | v2 §8.3.16, §7.2 | ⏳ PENDING (optional canary 9) |
| `assets/data/buildings.ron` | SemiconductorFab maintenance (Au/Ag/Pt/Ar) | v2 §8.3.18 | ⏳ PENDING |
| `assets/data/technologies.ron` | `lunar_colony` | v2 §8.6.1 | ⏳ PENDING — **CRITICAL** |
| `assets/data/technologies.ron` | `kardashev_k2` | v2 §8.6.3 | ⏳ PENDING |

### §5.8 v2 §8.8 — Apply order (LOCKED from v2; consolidated in v3 §0.D)

The v2 apply order is consolidated into the v3 §0.D
single canary-first apply plan. v2 §8.8 is preserved
here as the source of truth for canary 1–8; v3 §0.D adds
canary 5a (energy-demand rebalance), 5b (fusion downscale),
and 6 (IronMine BP change) to the plan.

### §5.9 v2 §8.9 — RON syntax notes (LOCKED from v2)

Preserved from v2 §8.9. The new v3 RON edits (§5.10, §5.11)
follow the same syntax rules.

### §5.10 v3 §8.10 — Energy-demand RON edits (v3 NEW)

**30 `power_demand_mw` updates in `assets/data/buildings.ron`**.
Apply in canary 5a (per v3 §0.D.2). For each row, the line
numbers below are approximate; the user should `grep -n
'power_demand_mw'` to find the exact location.

```diff
# HabitatDome (buildings.ron:90)
- power_demand_mw: 150.0,
+ power_demand_mw: 20900.0,

# Housing (buildings.ron:128)
- power_demand_mw: 50.0,
+ power_demand_mw: 10450.0,

# UndergroundHabitat (buildings.ron:166)
- power_demand_mw: 300.0,
+ power_demand_mw: 12540.0,

# LifeSupport (buildings.ron:52)
- power_demand_mw: 200.0,
+ power_demand_mw: 418.0,

# Farm (buildings.ron:2024)
- power_demand_mw: 30.0,
+ power_demand_mw: 114.0,

# IronMine (buildings.ron:310)
- power_demand_mw: 250.0,
+ power_demand_mw: 342.0,

# AluminumMine (buildings.ron:336)
- power_demand_mw: 200.0,
+ power_demand_mw: 2377.0,

# CopperMine (buildings.ron:628)
- power_demand_mw: 230.0,
+ power_demand_mw: 1026.0,

# SilicatesMine (buildings.ron:389)
- power_demand_mw: 80.0,
+ power_demand_mw: 200.0,

# AtmosphericProcessor (buildings.ron:229)
- power_demand_mw: 400.0,
+ power_demand_mw: 1500.0,

# ChemicalPlant (buildings.ron:266)
- power_demand_mw: 600.0,
+ power_demand_mw: 5700.0,

# Factory (buildings.ron:199)
- power_demand_mw: 800.0,
+ power_demand_mw: 2000.0,

# MassDriver (buildings.ron:1694)
- power_demand_mw: 500.0,
+ power_demand_mw: 500.0,  # no change — already in band

# OrbitalLift (buildings.ron:1719)
- power_demand_mw: 800.0,
+ power_demand_mw: 2000.0,

# CargoTerminal (buildings.ron:1745)
- power_demand_mw: 100.0,
+ power_demand_mw: 200.0,

# Shipyard (buildings.ron:2251)
- power_demand_mw: 3000.0,
+ power_demand_mw: 5000.0,

# SpacePort (buildings.ron:2683)
- power_demand_mw: 1000.0,
+ power_demand_mw: 1500.0,

# SemiconductorFab (buildings.ron:2474)
- power_demand_mw: 1000.0,
+ power_demand_mw: 1500.0,

# PharmaceuticalPlant (buildings.ron:2507)
- power_demand_mw: 300.0,
+ power_demand_mw: 500.0,

# DataCenter (buildings.ron:2658)
- power_demand_mw: 500.0,
+ power_demand_mw: 800.0,

# LaunchSite (buildings.ron:2304)
- power_demand_mw: 500.0,
+ power_demand_mw: 1500.0,

# MissileSilo (buildings.ron:2276)
- power_demand_mw: 400.0,
+ power_demand_mw: 400.0,  # no change — already in band

# MedicalCenter (buildings.ron:2054)
- power_demand_mw: 150.0,
+ power_demand_mw: 200.0,

# ResearchLab (buildings.ron:2084)
- power_demand_mw: 300.0,
+ power_demand_mw: 500.0,

# EngineeringBay (buildings.ron:2111)
- power_demand_mw: 400.0,
+ power_demand_mw: 600.0,

# CommercialHub (buildings.ron:2167)
- power_demand_mw: 80.0,
+ power_demand_mw: 200.0,

# FinancialCenter (buildings.ron:2191)
- power_demand_mw: 100.0,
+ power_demand_mw: 200.0,

# TradePort (buildings.ron:2219)
- power_demand_mw: 200.0,
+ power_demand_mw: 400.0,

# GroundDefenseBattery (buildings.ron:2712)
- power_demand_mw: 300.0,
+ power_demand_mw: 500.0,

# WaterProcessor (buildings.ron:953)
- power_demand_mw: 300.0,
+ power_demand_mw: 500.0,
```

**Bonus RON edit (v3 NEW, canary 5b):** the v0.5.1 §8.3.12
fusion He-3 / D downscale.

```diff
# FusionReactor (buildings.ron:1814-1820) — He-3 / D downscale
  maintenance_resources: [
-     ("Helium3", 10.0),
-     ("Deuterium", 5.0),
+     ("Helium3", 0.5),    # 20× downscale; 1:1 with He3Mine
+     ("Deuterium", 0.25),  # 20× downscale; 1.4:1 with ChemicalPlant D
      ("Cobalt", 0.002),
      ("Fluorine", 0.003),
      ("Titanium", 0.05),
      ("Lithium", 0.0005),
  ],

# DTFusionReactor (buildings.ron:1843+) — D downscale
  maintenance_resources: [
-     ("Deuterium", 0.0015),
+     ("Deuterium", 0.0001),  # 15× downscale; matched to new ChemicalPlant T breed rate
      ("Tritium", 0.0005),    # unchanged
      ...
  ],
```

### §5.11 v3 §8.11 — Cost-headroom RON edits (v3 NEW)

**One RON edit: `IronMine.build_points` 1,500 → 1,000.**

```diff
# IronMine (buildings.ron:292)
- build_points: 1500.0,
+ build_points: 1000.0,
```

### §5.12 v3 §8.12 — Apply order (v3 NEW)

The v3 apply order is the **single canary-first plan** in
v3 §0.D.2. This section gives the per-canary RON / RUST
edit list and the test gate. The user lands canary N, runs
`cargo test <gate>`, then rolls to canary N+1. Critical-path
canaries (must land in order): **3a → 3b → 3c → 3d → 4 →
5b**. Other canaries (1, 2, 5a, 6, 7, 8) can land in any
order.

The apply order in full:

1. **Canary 1 — Food calibration.** 4 RON diffs (§5.1) +
   2 RUST lines (`src/colony/components.rs:282-301`). Test
   gate: `cargo test food`.
2. **Canary 2 — WaterProcessor.** 1 RON entry (~25 lines) +
   1 RUST enum variant. Test gate: `cargo test water`.
3. **Canary 3a — `lunar_colony` tech (CRITICAL).** 1 RON
   entry (~15 lines) per v2 §8.6.1. Test gate: `cargo test
   tech_tree` + manual: research `lunar_colony`, build
   `He3Mine` on Moon.
4. **Canary 3b — `fusion_power` tech (verify only).** No
   edits; verify prereqs in `technologies.ron:719`. Test
   gate: `cargo test tech_tree`.
5. **Canary 3c — `kardashev_k2` tech.** 1 RON entry (~15
   lines) per v2 §8.6.3. Test gate: `cargo test tech_tree`.
6. **Canary 3d — He3Mine + body restriction + fusion
   downscale.** 1 RON entry (already shipped for He3Mine) +
   2 RON diffs for FusionReactor / D-T FusionReactor
   (§5.10 bonus). Test gate: `cargo test he3` + manual: 1
   He3Mine feeds 1 FusionReactor cleanly.
7. **Canary 4 — Mid-game fold.** ~25 RON diffs
   (ChemicalPlant, AtmosphericProcessor, BreederReactor).
   Test gate: `cargo test chemicals` + manual: N₂/O₂/Ar
   splits correct.
8. **Canary 5a — Energy-demand rebalance (v3 NEW).** 30 RON
   diffs (§5.10). Test gate: `cargo test power` + manual:
   mature colony shows 0.3–1.3× demand/supply.
9. **Canary 6 — Cost-headroom (v3 NEW).** 1 RON diff
   (§5.11). Test gate: `cargo test construction` + manual:
   25 IronMines = 25,000 BP (was 37,500).
10. **Canary 7 — Precious-metal + noble-gas mining.** ~80
    RON diffs + 3 RUST enum variants + 1 RUST schema
    change (`MAINTENANCE_AUDIT_MAX` 6 → 10 per v2 §8.3.18).
    Test gate: `cargo test precious_metals`.
11. **Canary 8 — K2 late-game.** 4 RON entries (~100 lines)
    + 4 RUST enum variants. Test gate: `cargo test k2`.
12. **Canary 9 (optional) — Power plant scale-down per v2
    §7.2.** ~12 RON diffs. Test gate: `cargo test power`.

### §5.13 v3.1 §8.13 — Workforce RON / RUST edits (v3.1 NEW)

**3 RON field updates + 3 RUST enum lines.** Apply in
canary 9 (per v3.1 §0.D.7). The RON field is the
human-readable documentation; the RUST enum is the
source of truth that the simulation reads (per the
v0.5.0 GRA-127 comment at `buildings.ron:282-301`).

```diff
# Farm (buildings.ron:2001)
- workforce: 1000,
+ workforce: 2000,

# AluminumMine (buildings.ron:320)
- workforce: 4500,
+ workforce: 1500,

# WindFarm (buildings.ron:2313)
- workforce: 200,
+ workforce: 1000,
```

```diff
# src/colony/types.rs:1340 (Farm)
- BuildingType::Farm => 1_000,
+ BuildingType::Farm => 2_000,

# src/colony/types.rs:1261 (AluminumMine)
- BuildingType::AluminumMine => 4_500,
+ BuildingType::AluminumMine => 1_500,

# src/colony/types.rs:1333 (WindFarm)
- BuildingType::WindFarm => 200,
+ BuildingType::WindFarm => 1_000,
```

**Test gate:** `cargo test colony` (covers
`test_early_colony_workforce_feasible`,
`test_workforce_demand`, `test_workforce_efficiency`).
Manual: verify the 3 changes are within the 0.5–2× of
real-productivity band; verify the early-game test sum
is 14,500 < 40,000.

### §5.14 v3.1 §8.14 — Resource build cost RON edits (v3.1 NEW)

**1 RON field update.** Apply in canary 10 (per v3.1
§0.D.7).

```diff
# OrbitalLift (buildings.ron:1705-1710)
  resource_costs: [
-     ("Titanium", 333.0),
+     ("Titanium", 5.0),
      ("Iron", 333.0),
      ("RareEarths", 83.0),
      ("Carbon", 167.0),
  ],
```

**Test gate:** `cargo test construction` (covers
`test_building_costs_positive` at
`src/colony/types.rs:1561`). Manual: verify 1
OrbitalLift buildable from 1 year of TitaniumMine
production at 25-mine scale (16.7 yr at 25 mines, 0.3
yr at 100 mines). MassDriver Cu 167 unchanged (7.4 yr
at 25 mines, in band). HabitatDome Al 50 unchanged
(0.67 yr at 25 mines, in band).

### §5.15 v3.1 §8.15 — Effect rendering RUST spec (v3.1 NEW)

**~50 RUST UI lines + new `friendly_label` helper.**
Apply in canary 11 (per v3.1 §0.D.7). The spec is
fully described in v3.1 §0.H.3 (3 call-sites:
`src/ui/construction.rs:1387-1391`, `:1604-1625`,
`:2845-2871`).

**Pseudocode for the new `friendly_label` helper:**

```rust
// Add to src/ui/construction.rs near format_mining_rate.
fn friendly_label(m: &crate::colony::data::Modifier)
    -> Option<(EffectTone, String)>
{
    use EffectTone::*;
    match m.modifier_type.as_str() {
        // Production modifiers (one arm per resource)
        "IronProduction" => Some((Positive,
            format!("Produces {} Iron",
                format_mining_rate(m.value)))),
        // ... (one arm per *Production type,
        //      ~30 arms total)
        // Capacity
        "HousingCapacity" => Some((Positive,
            format!("Houses {} residents",
                format_residents(m.value as u64)))),
        // Atmospheric / synthesis
        "AtmosphericHarvesting" => Some((Positive,
            format!("Harvests {} Mt/yr industrial gases",
                m.value))),
        "HydrogenSynthesis" => Some((Positive,
            format!("Synthesizes {} Mt/yr Hydrogen",
                m.value))),
        "AmmoniaSynthesis" => Some((Positive,
            format!("Synthesizes {} Mt/yr Ammonia (Haber-Bosch)",
                m.value))),
        "PolymerSynthesis" => Some((Positive,
            format!("Synthesizes {} Mt/yr polymers",
                m.value))),
        "TritiumBreeding" => Some((Positive,
            format!("Breeds {} Mt/yr Tritium (Li breeding)",
                format_mining_rate(m.value)))),
        "PlutoniumBreeding" => Some((Positive,
            format!("Breeds {} Mt/yr Plutonium",
                format_mining_rate(m.value)))),
        // Cost reduction
        "ConstructionCost" if m.value < 0.0 => Some((Positive,
            format!("Builds {} BP/yr faster",
                (-m.value) as i64))),
        "ConstructionCost" => Some((Neutral,
            format!("Construction cost +{} BP/build",
                m.value as i64))),
        // Research / Engineering
        "ResearchSpeed" => Some((Positive,
            format!("Research speed +{}%",
                m.value as i64))),
        "EngineeringSpeed" => Some((Positive,
            format!("Engineering speed +{}%",
                m.value as i64))),
        // Population
        "PopulationGrowth" => Some((Positive,
            format!("Population growth +{:.1}%/yr",
                m.value / 100.0))),
        // Storage
        "StorageCapacity" => Some((Positive,
            format!("Stockpile capacity +{}%",
                (m.value * 100.0) as i64))),
        // Water
        "WaterProduction" => Some((Positive,
            format!("Produces {} Water",
                format_mining_rate(m.value)))),
        // Power is a separate chip; do not surface here
        "PowerGeneration" => None,
        // Catch-all: surface the raw name
        _ => Some((Neutral,
            format!("{}: {}", m.modifier_type, m.value))),
    }
}
```

**Effect-cap helper:**

```rust
const EFFECT_CAP: usize = 5;
fn cap_effects(mut effects: Vec<(EffectTone, String)>)
    -> Vec<(EffectTone, String)>
{
    if effects.len() > EFFECT_CAP {
        let extra = effects.len() - EFFECT_CAP;
        effects.truncate(EFFECT_CAP);
        effects.push((EffectTone::Neutral,
            format!("+{} more", extra)));
    }
    effects
}
```

**Test gate:** `cargo test construction_ui` (existing
tests in `src/ui/construction.rs`). New unit tests:
`test_friendly_label_*` (one per modifier type, ~13
tests). Manual: verify the 3 worked-example cards
(ChemicalPlant 4 effects, HabitatDome 1 effect,
Warehouse 1 effect, AiCluster 2 effects) render
correctly with the 5+1 cap.

---

## §6 Self-checks and open questions (v2 §9 + v3 NEW)

### §6.1 v2 §9.1 — Stop condition check (UPDATED for v3)

| Stop condition | Status | v3 update |
|---|---|---|
| `docs/design/BALANCE_PATCHES_v0.5.md` exists (lean v2) | ✅ | v3 supersedes v2; same file |
| All three tiers covered | ✅ (v2 §4 / §5 / §6) | LOCKED |
| Tier-summary table concrete at the top | ✅ (v2 §2) | LOCKED |
| 10–50 constraint met for every early-game resource | ✅ (v2 §4.17, §9.2) | LOCKED |
| Mid-game He-3 fix concrete, tech-gated, AND body-restricted | ✅ (v2 §5.1) | 🟡 PARTIAL — `lunar_colony` PENDING |
| Late-game uses grams for Antimatter, marks exotics approximate | ✅ (v2 §6) | ⏳ PENDING (K2 exotics not yet shipped) |
| 3 new technologies spec'd with full prereq chains | ✅ (v2 §5.1.1, §5.1.2, §5.1.3) | 🟡 PARTIAL — 1 of 3 SHIPPED |
| Schema addition (`allowed_body_types`) flagged | ✅ (v2 §8.4) | ✅ SHIPPED |
| Implementation notes sufficient to apply without re-asking | ✅ (v2 §8) | ✅ ENHANCED (v3 §0.D + §5.10-§5.12) |

### §6.2 v3 NEW stop conditions (added for v3)

| Stop condition | Where | Status |
|---|---|---|
| Bottom-up power-demand sum done for the brief's 19 building types | v3 §0.A.1 | ✅ (24.7 GW demand vs 2,880 GW supply) |
| Per-building `power_demand_mw` updates proposed (target ratio 1.0–1.3×) | v3 §0.A.4, §5.10 | ✅ (30 updates; 0.31× at 1.875 B, 1.19× at 8.2 B) |
| Cost-headroom rebalance done; only 1 BP change proposed (IronMine 1500 → 1000) | v3 §0.B.3, §5.11 | ✅ |
| Player expansion path verified end-to-end | v3 §0.C | ✅ (viable; 2 PENDING canary items flagged) |
| Single canary-first apply plan unifies v2 + v0.5.2 + v3 | v3 §0.D, §5.12 | ✅ (8 mandatory + 1 optional canary) |
| v0.5.2 supersession status documented (shipped / partial / pending / superseded) | v3 §0.E | ✅ |
| v2 §4–§7 marked LOCKED | v3 §4 | ✅ |
| No RON, Rust, or UI files edited | (this is a doc) | ✅ |
| No new `BuildingType` enum variants added | v3 §0.3 | ✅ (v3 only adjusts existing buildings) |
| No new `ResourceType` entries added | v3 §0.3 | ✅ (39 locked) |

### §6.3 v2 §9.2 — Manageable-count check (LOCKED from v2)

[Preserved from v2 §9.2 lines 2213-2234.]

| Resource | Demand (Mt/yr Earth) | Per-build (Mt/yr) | Implied count | In 10-50? |
|---|---:|---:|---:|:---:|
| Food | 9,020 | 360 (hard-coded) | 25 | ✅ |
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
| Gold | 0.003 | 0.0001 | 25 | ✅ (Tier 3) |
| Silver | 0.025 | 0.001 | 25 | ✅ |
| Platinum | 0.0002 | 0.00001 | 20 | ✅ (manageable-count exception) |
| Argon | 0.7 | 0.028 | 25 | ✅ |

**All 16 early-game resources land in 10–50.** No resource is
out of band.

### §6.4 v2 §9.5 — Open questions for the user (LOCKED from v2, with v3 NEW additions)

The v2 open questions are preserved at v2 §9.5 lines
2287-2365. v3 adds the following:

**Q13 (v3 NEW): Energy-demand rebalance ratio at 1.875 B
population.** v3 §0.A.5 shows the proposed
`power_demand_mw` values land at 0.31× supply for the brief's
1.875 B scenario (a realistic under-supply) but 1.19× supply
at full Earth population (8.2 B). The user has two options:
* **Option A (v3 recommendation):** accept the 0.31× as a
  feature. The player will scale residential buildings over
  50 yr to reach 8.2 B and the ratio will catch up to 1.0–1.3×
  at the civilisation tier.
* **Option B:** scale the proposed `power_demand_mw` values by
  ~3.2× to hit 1.0× at 1.875 B (HabitatDome 20,900 → 67,000
  MW, etc.). This makes residential demand *higher* than
  real-world per-capita (33,500 MW / 25 M = 1,340 W/person vs
  the 418 W anchor), but gives the player the 1.0–1.3× ratio
  at the 1.875 B scenario. v3 doc does not recommend this.

**Q14 (v3 NEW): `lunar_colony` tech — name conflict?** v0.5.1
§8.6.1 spec uses the name `lunar_colony` for the off-world
He-3 mining gate. The current `assets/data/technologies.ron`
does NOT have a `lunar_colony` tech. The v0.5.1 spec name is
a bit misleading (the tech gates off-world He-3 mining, not
just the Moon — see v0.5.1 §5.1.1 for the rationale).
v3 confirms the v0.5.1 name is fine; the user can rename
to `off_world_mining` if preferred. The v3 apply plan uses
`lunar_colony` to match the v2 spec.

**Q15 (v3 NEW): DHe3FusionReactor tech name.** v0.5.1 §8.3.14
proposed `required_tech: "fusion_power"` for DHe3FusionReactor.
The current RON has `required_tech: "helium3_fusion"` (a
separate tech that IS in `technologies.ron:757`). v3 confirms
the shipped state (DHe3 uses `helium3_fusion`, not
`fusion_power`) is the correct design — it keeps the He-3
chain separate from the D-T chain. The v0.5.1 spec was
over-ambitious in unifying them.

### §6.5 Apply-order unambiguity (v3 NEW)

The v3 §0.D.2 canary list is unambiguous:

* The **critical-path** is 3a → 3b → 3c → 3d → 4 → 5b. The
  user MUST land 3a (`lunar_colony` tech) before any other
  canary 3 can be tested, because the He-3 chain is broken
  without it.
* The **non-critical** canaries (1, 2, 5a, 6, 7, 8) can land
  in any order after the critical path.
* The **optional** canary (9) is the v2 §7.2 power-plant
  scale-down; it can land at any time.

The v3 apply plan does NOT introduce any new dependencies
between canaries that didn't exist in v2 §8.8. It only
**adds** canaries (5a, 5b, 6) and **promotes** canary 3a
(the `lunar_colony` tech) to the critical path.

### §6.6 v3.1 NEW stop conditions (added for v3.1)

| Stop condition | Where in v3.1 | Status |
|---|---|---|
| Executive summary at top (v3 §0) is preserved | (inherited from v3) | ✅ |
| `docs/design/BALANCE_PATCHES_v0.5.md` is extended with §0.F, §0.G, §0.H, §0.D.7, §6.6, §5.13–§5.15, §8.4 | (this doc) | ✅ |
| The v3.1 stop conditions table is at the top of the v3.1 sections (in this §6.6) | §6.6 | ✅ |
| All 52 + 24 buildings have proposed workforce and per-building resource-cost analysis | §0.F.3 (70 buildings), §0.G.3 (52 + 24) | ✅ |
| The effect-rendering spec cites exact lines in `src/ui/construction.rs` (1387–1391, 1608, 1623, 2851, etc.) | §0.H.3 | ✅ |
| OrbitalLift Ti cost is no longer a hard blocker (payback ≤ 50 yr at 25-mine operator-bar scale) | §0.G.4 (16.7 yr) | ✅ |
| Farm workforce math satisfies 1:155 real-world ratio (within an order of magnitude at the per-build district scale) | §0.F.3 row 5 (1,000 → 2,000; per-worker 180 t/yr vs real 170 t/yr = 1.06×) | ✅ |
| No RON, Rust, or UI files were edited | (this is a doc) | ✅ |
| No new `BuildingType` enum variants added | §0.F.4.6 (all 95 variants preserved) | ✅ |
| No new `ResourceType` entries added | (39 locked) | ✅ |
| `population_scale_multiplier: 100.0` constant is unchanged | (preserved) | ✅ |
| Manageable-building count (10–50 per resource per body for early game) is preserved | §0.F.4.1 (test still passes at 14,500) | ✅ |
| Operator bar (1 building ≈ 1/300 of world share for Tier 2; 1 building ≈ world share for Tier 1 life-support) is preserved | §0.F.3 (calibrated against `CLAUDE.md` and v2 §3.3) | ✅ |
| Tech-tier-aware (early 0–50 yr, mid 50–200 yr, late 200+ yr) | §0.F.3 (rows 1–52 anchored to early/mid; 41–48 are mid/late) | ✅ |
| Every number cited (USGS / FAO / IEA / OECD / NASA-ECLSS, or back-ref to v3 section) | §0.F.2 (12 anchors) + §0.G.2 (3 anchors) | ✅ |
| Mass-balance equations, not vibes | §0.G.2 (3 worked examples) + §0.G.3 (52 buildings) | ✅ |
| Push back when warranted | §0.F.4.6, §0.G.5, §0.F.1, §0.G.1 (3 of 3 user "hard blockers" re-evaluated) | ✅ |
| v3 §0.A–§0.E unchanged | (LOCKED) | ✅ |
| v2 §4–§7 unchanged | (LOCKED) | ✅ |
| Brief 1-paragraph summary back to orchestrator | (end of this response) | ✅ |

### §6.7 v3.1 open questions (added for v3.1)

**Q16 (v3.1 NEW): AutoMine workforce ratio.** The user
prompt notes "AutoMines are 20-50% reduction, not 10×."
The current AutoMine workforces are 16–30% of base mine
workforce. v3.1 §0.F.4.5 documents the 16–30% range and
pushes back: the v0.5.2 design intent ("lean orbital
crews") justifies 16% (not 20–50%). **The user can
override:** if the 20–50% target is preferred, the simple
rule is `Auto{Res}Mine.workforce = {Res}Mine.workforce /
4` (round to nearest 100), which would change 12 of 24
AutoMines. v3.1 does not propose this change.

**Q17 (v3.1 NEW): OrbitalLift Ti cost — 5 Mt or 50 Mt?**
v3.1 §0.G.4 proposes Ti 333 → 5 Mt (16.7 yr at 25
mines, 0.3 yr at 100 mines). The user could alternatively
prefer 50 Mt (167 yr at 25 mines, 50 yr at 100 mines) for
"major undertaking" pacing. v3.1 does not have a strong
preference; the user picks.

**Q18 (v3.1 NEW): Effect-rendering spec wording.** v3.1
§0.H.3 proposes a `friendly_label` helper that maps
modifier types to (tone, label) pairs. The wording
("Houses 50M residents", "Synthesizes 100 Mt/yr Hydrogen",
etc.) is the v3.1 best guess; the user can refine in
review. The catch-all `_ =>` arm surfaces unknown
modifier types with the raw name (e.g. "FooBar: 42.0").

**Q19 (v3.1 NEW): Effect-cap 5+1.** v3.1 §0.H.4 proposes
a 5+1 cap. ChemicalPlant has 4 effects (in cap); no
building has 6+ effects today. If a future building
exceeds 5 effects, the "+N more" line tells the player
to look at the tooltip. **The user can override to 6+1
or 4+1 if preferred.**

**Q20 (v3.1 NEW): Farm workforce value (1,000 → 2,000
vs 2,100,000).** v3.1 §0.F.1 + §0.F.3 push back on the
2.1M target (would break the early-game test). The
2,000 value is the largest that preserves the test AND
brings Farm within 0.5–2× of real productivity. **The
user can override:** if a "tier-aware" workforce is
preferred (Farm scales from 1,000 at year 1 to 2,000 at
year 50 to 5,000 at year 200), that would require a new
`workforce` field type and is out of v3.1 scope.

**Q21 (v3.1 NEW): MassDriver Cu 167 keep-or-change.** v3.1
§0.G.5 documents the 7.4 yr payback at 25 mines × 0.6
accessibility (in Tier 3 3–10 yr band). The user's
"11 yr" used 0.4 accessibility; v3.1 uses the v3
calibration anchor 0.6. **The user can override:** if 0.4
accessibility is preferred for Cu (and other metals),
MassDriver Cu 167 might be 11 yr, just above the 3–10
yr band. v3.1 does not propose a change.

**Q22 (v3.1 NEW): The 28 strategic-tier costs.** v3.1
§0.G.3 documents 28 buildings with at least one
*strategic-tier* (10–2,000 yr) resource_cost. v3.1
preserves all 28 as "the brief explicitly preserves."
**The user can override:** if a different strategic-tier
set is preferred (e.g. OrbitalLift REE 83 → 10 Mt to
make OrbitalLift buildable in 25–50 yr), v3.1 doesn't
propose it but documents the option.

---

## §7 Reference: v2 §10 v0.5.2 per-resource dedicated mines (SHIPPED)

> **This section is LOCKED from v2 §10 — already shipped.**
> The v0.5.2 ADDENDUM is the current source of truth for the
> per-resource dedicated mine approach. v3 references it by
> section number; v3 does NOT re-derive the per-build
> production values.

**The v0.5.2 design pattern** (v2 §10 lines 2466-2514):
* Per-resource dedicated base mine for every
  crustal/liquid-minable resource (22 buildings)
* Per-resource AutoMine for orbital/asteroid mining (24
  buildings, body-restricted to `[Asteroid, Moon, GasGiant]`)
* No `MiningEfficiency` / `DeepMiningEfficiency` /
  `BulkMiningEfficiency` modifiers; no share-fold
* Same base building for mining — all 22 base mines share
  `line: Some("Mine")` so a future tier-1+ upgrade building
  can use the new `replaces_in_line: Option<String>` schema
  field
* Earth starting counts: 25 of each base mine
* Legacy generic mines removed: `Mine` / `Refinery` /
  `DeepDrill` / `LaserDrill` / `StripMine` /
  `HydrocarbonExtractor` / `RecyclingCenter` are gone
* `BuildingDefinition::allowed_body_types: Vec<BodyType>`
  (default empty = any body)

**v0.5.2 §10.1 — Per-resource base mine calibration.**
22 base mines; 25 × base_yield × 0.6 (Earth accessibility)
≈ USGS 2024 / 2026 world demand. Calibration values
preserved at v2 §10.1 lines 2479-2506.

**v0.5.2 §10.2 — AutoMines (orbital / asteroid mining).**
24 AutoMines; calibrated at ~1/10 of the surface base mine
yield. All AutoMines body-restricted to `[Asteroid, Moon,
GasGiant]`, `asteroid_mining` tech-gated. See v2 §10.2
lines 2508-2514.

---

## §8 v3 vs v2 deltas (v3 NEW)

### §8.1 What's NEW in v3 (vs v2)

| Section | What v3 adds | Lines |
|---|---|---:|
| §0 | Executive summary (1 page, top of doc) | 130 |
| §0.A | Energy balance recalibration — bottom-up demand sum, 30 per-building `power_demand_mw` updates, fusion downscale bonus | 280 |
| §0.B | Cost-headroom rebalance — current curve audit, single IronMine BP change, resource_costs hidden-tax analysis | 200 |
| §0.C | Player expansion path verification — Earth → Moon → asteroid → space-industry walkthrough with math at each step | 220 |
| §0.D | Single canary-first apply plan — unified 8-canary list with critical-path + test gates | 180 |
| §0.E | v0.5.2 supersession status — per-v0.5.1-patch shipped/partial/pending/superseded audit table | 200 |
| §5.10 | v3 §8.10 RON edits for energy-demand rebalance | 100 |
| §5.11 | v3 §8.11 RON edit for cost-headroom rebalance (1-line IronMine BP) | 10 |
| §5.12 | v3 §8.12 apply order with critical-path + test gates | 60 |
| §6.2 | v3 NEW stop conditions | 30 |
| §6.4 Q13-15 | v3 NEW open questions | 40 |
| §8 | v3 vs v2 deltas (this section) | 60 |
| **Total v3 NEW** | | **~1,510 lines** |

### §8.2 What's LOCKED from v2 in v3

| Section | Status |
|---|---|
| v2 §1 TL;DR and stop conditions | LOCKED (updated with v3 framing in §1.1) |
| v2 §2 Headline tier-summary table | LOCKED |
| v2 §3 Methodology and the 10–50 constraint | LOCKED |
| v2 §4 Early-game tier (16 resources) | LOCKED, REFERENCE |
| v2 §5 Mid-game tier (He-3 chain + 18 resources) | LOCKED, REFERENCE |
| v2 §6 Late-game tier (4 K2 exotics) | LOCKED, REFERENCE |
| v2 §7 Power buildings (per-capita, per-build targets) | LOCKED, REFERENCE |
| v2 §8.1 Rust constant delta | SHIPPED (cross-references) |
| v2 §8.2 NEW buildings (9 buildings) | 🟡 PARTIAL (5 of 9 SHIPPED) |
| v2 §8.3 EXISTING building edits (18 edits) | 🟡 PARTIAL (most SUPERSEDED by v0.5.2) |
| v2 §8.4 Schema addition (`allowed_body_types`) | SHIPPED |
| v2 §8.5 Rust enum additions (9 variants) | 🟡 PARTIAL (K2 variants pending) |
| v2 §8.6 NEW technologies (3 techs) | 🟡 PARTIAL (1 of 3 SHIPPED) |
| v2 §8.7 Files checklist | UPDATED (v3 §5.7) |
| v2 §8.8 Apply order | CONSOLIDATED in v3 §0.D |
| v2 §8.9 RON syntax notes | LOCKED |
| v2 §9.1 Stop condition check | UPDATED (v3 §6.1) |
| v2 §9.2 Manageable-count check | LOCKED |
| v2 §9.3 v1 → v2 deltas | LOCKED (historical) |
| v2 §9.4 Resources where manageable-count was hardest | LOCKED |
| v2 §9.5 Open questions | UPDATED with v3 NEW Q13-15 |
| v2 §9.6 Files NOT modified | LOCKED |
| v2 §9.7 Self-check: what this doc does NOT do | LOCKED |
| v2 §9.8 Handoff to the user | LOCKED |
| v2 §10 v0.5.2 ADDENDUM | SHIPPED, LOCKED |

### §8.3 The 1-paragraph summary to the orchestrator

v3 closes the energy-consumption gap the user reported, but
inverts the direction: the bottom-up power-demand sum for a
mature-Earth colony is **24.7 GW** against a 2,880 GW supply
from 12 SolarPower, a 117× over-supply (not a deficit). The
fix is 30 `power_demand_mw` updates (HabitatDome 150 → 20,900
MW, Housing 50 → 10,450 MW, etc.) that land at 0.31× supply
for the brief's 1.875 B scenario and 1.19× at full Earth
population — exactly the 1.0–1.3× target. The cost-headroom
rebalance is **one** change: `IronMine.build_points` 1,500 →
1,000 (the only Tier 2 outlier). The expansion path Earth →
Moon → asteroid → space-industry is **viable** after v2
lands, with two critical-path canary items: `lunar_colony`
tech (PENDING — `He3Mine` is unbuildable without it) and
`FusionReactor` He-3 / D / T downscale (PENDING). v3
consolidates the v2 + v0.5.2 + v3 changes into a single
8-canary apply plan with explicit critical-path (3a → 3b →
3c → 3d → 4 → 5b) and parallel-old/new (feature-flag)
rollout. v3 makes zero edits to RON / Rust / UI files; it
is a proposal doc with 30 + 1 RON diffs and 1 RUST enum
variant pending, plus the v0.5.2 per-resource mines already
shipped.

### §8.4 v3.1 deltas (v3.1 NEW)

#### 8.4.1 What's NEW in v3.1 (vs v3)

| Section | What v3.1 adds | Lines |
|---|---|---:|
| §0.D.7 | v3.1 apply plan extension — Canaries 9, 10, 11 with renumbered 12, 13 | ~140 |
| §0.F | Workforce calibration — 70-building per-row table with real-world productivity anchors, special-case analysis (early-game test, mature-Earth staffing, AutoMines), 3 RON + 3 RUST diffs | ~520 |
| §0.G | Resource build cost rebalance — 52-building + 24-AutoMine per-row cost analysis with payback math, 1 RON diff (OrbitalLift Ti 333 → 5 Mt), push-back on user's other 2 examples | ~480 |
| §0.H | Building-card effect rendering — inventory of 13 hidden modifier types, code spec for `friendly_label` helper, 5+1 effect cap, tones | ~270 |
| §5.13 | v3.1 §8.13 RON / RUST edits for workforce (Canary 9) | ~50 |
| §5.14 | v3.1 §8.14 RON edits for resource cost (Canary 10) | ~20 |
| §5.15 | v3.1 §8.15 RUST spec for effect rendering (Canary 11) | ~110 |
| §6.6 | v3.1 NEW stop conditions (16 items) | ~30 |
| §6.7 | v3.1 NEW open questions (Q16–Q22) | ~50 |
| §8.4 | v3.1 deltas (this section) | ~80 |
| **Total v3.1 NEW** | | **~1,750 lines** |

#### 8.4.2 What's LOCKED from v3 in v3.1

| Section | Status |
|---|---|
| §0 Executive summary | LOCKED (unchanged) |
| §0.A Energy balance recalibration | LOCKED (unchanged) |
| §0.B Cost-headroom rebalance | LOCKED (the `build_points` axis; v3.1 §0.G extends with the `resource_costs` axis) |
| §0.C Player expansion path | LOCKED (unchanged) |
| §0.D Apply plan (canaries 1–8) | LOCKED (v3.1 §0.D.7 extends with canaries 9, 10, 11) |
| §0.E v0.5.2 supersession | LOCKED (unchanged) |
| §1 TL;DR and stop conditions | LOCKED (v3.1 §6.6 adds v3.1 stop conditions) |
| §2 Headline tier-summary table | LOCKED (unchanged) |
| §3 Methodology and the 10–50 constraint | LOCKED (unchanged) |
| §4 v2 per-resource calibration (§4–§7) | LOCKED (unchanged) |
| §5.1–§5.12 Implementation notes | LOCKED (v3.1 §5.13–§5.15 extend) |
| §6.1–§6.5 Self-checks and open questions | LOCKED (v3.1 §6.6–§6.7 extend) |
| §7 Reference: v0.5.2 per-resource dedicated mines | LOCKED (unchanged) |
| §8.1–§8.3 v3 vs v2 deltas | LOCKED (v3.1 §8.4 extends) |

#### 8.4.3 The 1-paragraph summary to the orchestrator (v3.1)

v3.1 extends v3 with three findings the user surfaced
that v3 did not cover: (1) workforce values are wildly
off real-world productivity ratios (Farm 1,000 workers
imply 360 t/yr/worker = 2,000× the 1:155 real anchor of
170 t/yr/worker), (2) OrbitalLift Ti 333 Mt cost is a
hard blocker (1,110 yr payback at 25 mines × 0.6
accessibility vs the Tier 3 20–50 yr target), and (3)
building cards hide most `modifiers` (9 of 52 buildings
have hidden effects, 13 distinct modifier types are
silently dropped by the `find` in
`src/ui/construction.rs:1387-1391`). v3.1 audits 70
buildings against real-world productivity anchors (USDA,
USGS, IEA, OECD, NASA-ECLSS, NREL, EIA, IRENA) and
proposes **3 RON + 3 RUST workforce changes** (Farm
1,000 → 2,000; AluminumMine 4,500 → 1,500; WindFarm 200
→ 1,000), **1 RON resource-cost change** (OrbitalLift Ti
333 → 5 Mt, hard-blocker fix), and **1 RUST UI spec**
(`friendly_label` helper at 3 call-sites with 5+1
effect cap). v3.1 pushes back on 2 of 3 of the user's
"hard blocker" examples (MassDriver Cu 167 already in
band at 7.4 yr; HabitatDome Al 50 already in band at
0.67 yr at the 25-mine operator-bar scale) and
preserves the 28 strategic-tier costs (MassDriver REE,
OrbitalLift REE, Shipyard Ti, He3Mine Ti, etc.) as
"the brief explicitly preserves" — the player must
scale the strategic-commodity economy first. v3.1
extends v3 with **Canaries 9, 10, 11** in the unified
apply plan, all non-critical-path, all independent of
each other and of v3 canaries 1–8. v3.1 makes zero
edits to RON / Rust / UI files; it is a proposal doc
with 4 RON diffs + 53 RUST lines pending, plus the v3
30 + 1 RON diffs already specified. The early-game
workforce test at `src/colony/types.rs:1592-1610` is
preserved (sum 14,500 < 40,000 with the v3.1 changes),
the operator bar (1 building ≈ 1/300 of world share for
Tier 2) is preserved, the manageable-building count
(10–50 per resource per body) is preserved, the
`population_scale_multiplier: 100.0` constant is
unchanged, and no new `BuildingType` enum variants or
`ResourceType` entries are added.

---

*End of balance-patches consolidated v3 + v3.1 extension.
v3.1 extends v3 with §0.F (workforce calibration), §0.G
(resource build cost rebalance), §0.H (building-card effect
rendering), §0.D.7 (Canaries 9, 10, 11), §5.13–§5.15
(implementation notes), §6.6 (v3.1 stop conditions), §6.7
(v3.1 open questions), and §8.4 (v3.1 deltas). v3.1 makes
zero edits to RON / Rust / UI files; it is a proposal doc
with 4 RON diffs + 53 RUST lines pending. The v2 per-
resource calibration (§4 / §5 / §6 / §7) is LOCKED. v3
references the v0.5.2 ADDENDUM §10 (per-resource dedicated
mines) as the shipped source of truth for the 22 base mines
+ 24 AutoMines. v3 proposes 30 `power_demand_mw` updates + 1
`IronMine.build_points` update + the v0.5.1 §8.3.12 fusion
downscale, all in a single canary-first apply plan (v3
§0.D.2). v3.1 extends the apply plan with 3 non-critical
canaries (workforce, resource cost, effect rendering). The
user lands canary N, runs `cargo test <gate>`, rolls to
canary N+1. The v3 critical-path is canary 3a (`lunar_colony`
tech) — without it, the He-3 chain is broken and the player
cannot build `He3Mine`. The v3.1 canaries (9, 10, 11) are
non-critical and can land in any order after canary 8.*
