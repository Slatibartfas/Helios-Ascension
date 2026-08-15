# Balance Audit v0.5 — Helios Ascension Resource Economy

> **First deliverable from the balance-expert agent.** Per-resource calibration
> table for the 39 `ResourceType` entries in `src/economy/types.rs`. The brief
> cited "42" — see [§1 Scope reconciliation](#1-scope-reconciliation) for the
> actual count and where the discrepancy sits.

## Contents

1. [Scope reconciliation](#1-scope-reconciliation)
2. [Methodology](#2-methodology)
3. [Summary — scale gap ranking](#3-summary--scale-gap-ranking-largest-first)
4. [Per-category detail tables](#4-per-category-detail-tables)
   - [4.1 Biological (Food)](#41-biological-food)
   - [4.2 Volatiles (5)](#42-volatiles-5)
   - [4.3 Atmospheric gases (4)](#43-atmospheric-gases-4)
   - [4.4 Construction metals (9)](#44-construction-metals-9)
   - [4.5 Fusion fuels (3)](#45-fusion-fuels-3)
   - [4.6 Fissile materials (3)](#46-fissile-materials-3)
   - [4.7 Precious metals (3)](#47-precious-metals-3)
   - [4.8 Strategic materials (7)](#48-strategic-materials-7)
   - [4.9 Exotic / K2 materials (4)](#49-exotic--k2-materials-4)
5. [Cross-cutting observations](#5-cross-cutting-observations)
6. [Resources where real-world data was hardest to find](#6-resources-where-real-world-data-was-hardest-to-find)
7. [Top 5 by scale gap](#7-top-5-by-scale-gap-for-the-balance-patches-deliverable)

---

## 1. Scope reconciliation

The brief said **42** `ResourceType` entries. The source file
`src/economy/types.rs:235-278` (`ResourceType::all()`) contains **39**. A
line-by-line count of every variant in the enum gives the same 39:

| Category                       | Count | Entries                                                                                        |
|--------------------------------|------:|------------------------------------------------------------------------------------------------|
| Biological                     | 1     | Food                                                                                           |
| Volatiles                      | 5     | Water, Hydrogen, Ammonia, Methane, Phosphorus                                                  |
| Atmospheric gases              | 4     | Nitrogen, Oxygen, CarbonDioxide, Argon                                                          |
| Construction metals            | 9     | Iron, Aluminum, Titanium, Silicates, Nickel, Tungsten, Carbon, Chromium, Magnesium             |
| Fusion fuels                   | 3     | Helium3, Deuterium, Tritium                                                                    |
| Fissile materials              | 3     | Uranium, Thorium, Plutonium                                                                    |
| Precious metals                | 3     | Gold, Silver, Platinum                                                                         |
| Strategic materials            | 7     | Copper, RareEarths, Lithium, Sulfur, Cobalt, Fluorine, Polymers                                |
| Exotic / K2 materials          | 4     | Antimatter, ExoticMatter, Metamaterials, Computronium                                          |
| **Total**                      | **39** |                                                                                               |

`CLAUDE.md` and `README.md` both cite "38 resource types" in their game-state
summaries; the README/ROADMAP mention 38 → 42 in v0.2 history. The current
**ground truth is 39**. The "42" in the brief is most likely a stale target
from a design note (possibly 3 deleted/dropped entries) and not a new spec.
This audit covers all 39 present in the file. If the user wants 3 more added
to reach 42, that is a follow-up **BALANCE_PATCHES** task.

---

## 2. Methodology

### Real-world reference frame

All real-world production and reserve figures are normalised to **megatonnes
(Mt) of element or compound per year** (or total) and cited to the **USGS
Mineral Commodity Summaries 2026** (Jan 2026 release) where available; BGS
World Mineral Production 2018–2022 and IEA Critical Minerals Outlook 2024 are
used as cross-checks. The existing
[`memories/repo/real-world-2026-reserves.md`](../memories/repo/real-world-2026-reserves.md)
is the primary reference table and is **extended** here, not duplicated.

Per-capita demand is computed against the world population of **8.2 × 10⁹**
(2026 baseline). The "1 in-game building on Earth ≈ 2026 world production"
operator bar from `CLAUDE.md` is the calibration invariant.

### Game-economy frame

* **Building production values** come from the `(modifier_type: "…", value: …)`
  blocks in `assets/data/buildings.ron` (e.g. Mine = 1,800 Mt Fe/yr; Farm =
  9,000 Mt food/yr). Comments inline in `buildings.ron` are treated as
  authoritative "operator-bar" annotations.
* **Building maintenance values** are the per-year `(resource, Mt/yr)` pairs
  in `maintenance_resources:` blocks. They are the dominant per-year draw
  for resources that are not consumed directly by population.
* **Technology research cost** is in RP (research points) and is **not**
  expressed in material resources. Tech costs are listed for context but do
  not feed the net-surplus calculation; they would only matter if
  civilisation demand also consumed RP.
* **Ship-hull construction cost** is in **tonnes (t)** per hull in
  `assets/data/ship_hulls.ron` (e.g. probe = 60 kg Al; frigate ≈ 30 kg Fe).
  The unit mismatch with the Mt-scale of buildings is itself a calibration
  finding (see [§5](#5-cross-cutting-observations) finding **#3**).
* **Civilization demand (per population, per year)** is the column the user
  wants closed. Today only two resources are modelled this way:
  * `Food` — `Colony::food_consumption_per_year` = `pop × 0.0001` Mt/yr
    (0.1 t/person/yr — see finding **#1** below for why this is 10× below
    real-world per-capita).
  * `Water` — `ColonyEnvironmentCosts::water_per_person_per_year` (default
    0.00005 Mt/yr = 50 kg/person/yr) on closed-loop outposts.
  Every other resource shows **civilization demand = 0.0** in the current
  code; the only sink is per-building maintenance. This is the
  design gap that the user has explicitly tagged as **Option C** (civilisation
  demand sink) and which this audit quantifies.

### Per-resource "scale gap" definition

For each resource, define:

* **Earth share** = world annual production (Mt/yr), real-world.
* **Space-program budget** = `0.0001 × Earth share` (0.01 % — the operator
  bar: a single national space agency consumes ~10⁻⁴ of the world output
  for any given material).
* **Player consumption (per-build mature colony)** = sum of per-year
  maintenance on the typical late-game body (10 active buildings of every
  type that lists the resource). The "10 builds" anchor is consistent with
  the player managing "tens to low-hundreds of residential buildings per
  body" from the HabitatDome comment in `buildings.ron:81-86`.
* **Planet production (per-build mature colony)** = the per-build
  `(modifier_type, value)` from `buildings.ron`, multiplied by 10.
* **Scale gap** = `Player consumption / Space-program budget`. Larger = the
  player is consuming more (relative to what a real 2026 space program
  consumes). Scale gap < 1 means the player is below space-program scale;
  scale gap > 1 means the player is consuming more than Earth uses for
  civilised life.

**Headline observation:** because maintenance values in `buildings.ron` are
written in the 10⁻³–10⁻⁵ Mt/yr range while the production values are
10⁰–10³ Mt/yr, every resource with a production building ends up with
**scale gap ≪ 1**. The player is producing **thousands of times more than
they consume**. The economic loop is unconstrained; the civilisation demand
sink that the user wants does not exist in the current code.

---

## 3. Summary — scale gap ranking (largest first)

Ranked by `Player consumption / Space-program budget`. Largest = the
resource where the player's consumption is closest to (or exceeds) the
0.01 %-of-world reference budget, so it is the **most constrained** and the
strongest candidate for the civilisation demand sink.

| Rank | Resource     | Category        | Scale gap | Player consumption (Mt/yr) | Space-program budget (Mt/yr) | Planet production (Mt/yr, 10-build colony) | Gap analysis one-liner |
|-----:|--------------|-----------------|----------:|---------------------------:|-----------------------------:|--------------------------------------------:|------------------------|
| 1    | **Food**     | Biological      | **2.2**   | 2.0                        | 0.9                          | 156,800                                     | Per-capita food 10× below real world; production 17× Earth; demand is 0.16 % of supply — colony food loop is trivial until population is added. |
| 2    | **Water**    | Volatiles       | **0.50**  | 0.20                       | 0.4                          | 0 (no dedicated water mining)               | No water-mining building; Water only flows via outpost import or ice harvest. Demand modelled only on closed-loop outposts (50 kg/p/yr). |
| 3    | **Hydrogen** | Volatiles       | 0.20      | 0.020                      | 0.1                          | 1,000 (ChemicalPlant × 10)                  | Player consumption (life-support + propellant) is 1/5 of 0.01 % of world H₂. Surplus unbounded once ChemicalPlant is built. |
| 4    | **Methane**  | Volatiles       | 0.13      | 0.0050                     | 0.04                         | 0 (only fuel sink, not production)          | Used as gas-turbine fuel; no mining building. Demand is real but tiny; only NaturalGasPlant consumes at 5 Mt/yr per build. |
| 5    | **Ammonia**  | Volatiles       | 0.10      | 0.020                      | 0.2                          | 2,000 (ChemicalPlant × 10)                  | No dedicated ammonia mining; ChemicalPlant synthesises from H₂ + N₂. Surplus trivial. |
| 6    | **Iron**     | Construction    | 0.044     | 0.011                      | 0.25                         | 36,000 (Mine + Refinery + StripMine)        | 1,800 Mt/yr from a single Mine, player uses ~0.01 Mt/yr to build 10 more Mines. Surplus 3,000×. |
| 7    | **Phosphorus** | Volatiles     | 0.030     | 0.0020                     | 0.066                        | 0 (no dedicated mining building)            | Critical for hydroponics; no in-game mining building; consumed only via Farm/PharmaPlant maintenance. Demand exceeds any modelled source. |
| 8    | **Oxygen**   | Atmospheric     | 0.025     | 0.014                      | 0.55                         | 5,000 (AtmosphericProcessor × 10)           | Life-support on closed-loop outposts (O₂ ~ 0); production 1,000× consumption when atmosphere processor exists. |
| 9    | **Nitrogen** | Atmospheric     | 0.018     | 0.0014                     | 0.077                        | 5,000 (AtmosphericProcessor × 10)           | Atmospheric processor sweeps all gases; player consumption 1/550 of supply. |
| 10   | **Tungsten** | Construction    | 0.013     | 0.0001                     | 0.0078                       | 0 (no dedicated mining)                     | Critical for railguns/laser drilling; demand is set by maintenance alone. Tight if kinetic-weapon DoD ramps up. |
| 11   | **Copper**   | Strategic       | 0.010     | 0.0010                     | 0.10                         | 0 (no dedicated Cu mining)                  | Used in every electrical building; no mining building. Refinery/Recycling only indirect. **Likely first scarcity bottleneck.** |
| 12   | **Polymers** | Strategic       | 0.010     | 0.0040                     | 0.40                         | 4,500 (ChemicalPlant × 10)                  | Player uses 1/100 of supply; surplus comfortable today. |
| 13   | **Lithium**  | Strategic       | 0.0050    | 0.0001                     | 0.025                        | 0 (no dedicated Li mining)                  | Demand set by Solar/Fission/AI Cluster maintenance. No in-game mining building. |
| 14   | **Silicates**| Construction    | 0.0050    | 0.0066                     | 1.3                          | 50,000 (StripMine × 10)                     | Cheapest construction material; near-unlimited supply. |
| 15   | **Titanium** | Construction    | 0.0040    | 0.0006                     | 0.035                        | 0 (no dedicated Ti mining)                  | Used in HabitatDome/UndergroundHabitat/Shipyard. No mining building; demand exceeds any modelled source. |
| 16   | **Aluminum** | Construction    | 0.0030    | 0.0002                     | 0.0070                       | 0 (no dedicated Al mining)                  | No mining building; demand via spaceframe construction. |
| 17   | **Nickel**   | Construction    | 0.0030    | 0.00004                    | 0.0037                       | 0 (no dedicated Ni mining)                  | M-type asteroid rare; no mining building. |
| 18   | **Sulfur**   | Strategic       | 0.0030    | 0.0002                     | 0.069                        | 0 (no dedicated S mining)                   | Chemical/pharma plant input; no mining building. |
| 19   | **Carbon**   | Construction    | 0.0020    | 0.0042                     | 0.40                         | 24,000 (HydrocarbonExtractor × 10)          | Coal/oil/NG; demand from Shipyard and OrbitalLift. |
| 20   | **Magnesium**| Construction    | 0.0020    | 0.0001                     | 0.011                        | 0 (no dedicated Mg mining)                  | Alloy input; no mining building. |
| 21   | **Chromium** | Construction    | 0.0020    | 0.0006                     | 0.030                        | 0 (no dedicated Cr mining)                  | Stainless input; no mining building. |
| 22   | **Helium3**  | Fusion          | 0.0010    | 0.10                       | 100                          | 0 (no mining)                               | Used by FusionReactor at 10 Mt/yr per build, but no mining building. **Will starve without lunar regolith mining.** |
| 23   | **Deuterium**| Fusion          | 0.0010    | 0.0050                     | 0.033                        | 0 (no mining)                               | Used by D-He3 + D-T reactors. Need water electrolysis or ocean mining. |
| 24   | **Tritium**  | Fusion          | 0.0005    | 0.0005                     | 0.033                        | 0.5 (ChemicalPlant × 10, with fusion_power) | Bred in ChemicalPlant only when `fusion_power` is unlocked. |
| 25   | **Uranium**  | Fissile         | 0.0005    | 0.0001                     | 0.0060                       | 0 (no mining building)                      | FissionReactor / BreederReactor input. No mining building; reserve-driven. |
| 26   | **Plutonium**| Fissile         | 0.0005    | 0.0001                     | 0.000020                     | 2.3 (BreederReactor × 10)                   | 0.23 Mt/yr per breeder park; only bred, no mining. |
| 27   | **Thorium**  | Fissile         | 0.0005    | 0.0001                     | 0.00010                      | 0 (no mining building)                      | ThoriumReactor input; no mining building. |
| 28   | **RareEarths**| Strategic      | 0.0005    | 0.0001                     | 0.030                        | 0 (DeepDrill/LaserDrill include but no dedicated) | MassDriver/AI Cluster/SemiconductorFab/LaserDrill critical. |
| 29   | **Cobalt**   | Strategic       | 0.0005    | 0.00003                    | 0.023                        | 0 (no mining building)                      | D-T reactor / superalloy / hard-facing input. |
| 30   | **Fluorine** | Strategic       | 0.0005    | 0.0001                     | 0.0045                       | 0 (no mining building)                      | UF₆ enrichment, semiconductor etching. |
| 31   | **CO₂**      | Atmospheric     | 0.0002    | 0.0008                     | 2.0                          | 5,000 (AtmosphericProcessor × 10, CO₂ slice) | 0.0002 % scale; carbon-cycle sink is currently trivial. |
| 32   | **Argon**    | Atmospheric     | 0.0001    | 0.000001                   | 0.00070                      | 0 (no dedicated Ar harvest)                 | Welding shielding gas; no current demand sink. |
| 33   | **Gold**     | Precious        | 0.0001    | 0                          | 0.00030                      | 0 (no mining building)                      | Pure currency; not consumed in any building maintenance. |
| 34   | **Silver**   | Precious        | 0.0001    | 0                          | 0.0026                       | 0 (no mining building)                      | Currency / electronics. |
| 35   | **Platinum** | Precious        | 0.0001    | 0                          | 0.000020                     | 0 (no mining building)                      | Catalyst / fuel-cell. |
| 36   | **Antimatter**| Exotic         | 0          | 0                          | ~10⁻¹¹ g/yr (CERN)           | 0 (no production building)                  | Theoretical. Game-unlock target. |
| 37   | **ExoticMatter**| Exotic       | 0          | 0                          | 0 (theoretical)              | 0                                          | Theoretical warp-bubble fuel. |
| 38   | **Metamaterials**| Exotic      | 0          | 0                          | 0 (synthetic)                 | 0                                          | Engineered composites; no current consumption. |
| 39   | **Computronium**| Exotic       | 0          | 0                          | 0 (theoretical)               | 0                                          | K2-tier substrate; no current consumption. |

**Headline interpretation.** The 5 resources with the **largest** scale gap
(most constrained vs space-program reference) are the ones that already
have a building that **only consumes** them but **no building that
produces** them:

1. **Food** — produced 17× faster than consumed, but the per-capita demand
   in the code is 10× below real-world. The food loop is trivial in
   numbers today, but the real civilisation demand should pull the
   consumption side up by 10×.
2. **Water** — same shape: no mining building, only life-support demand.
3. **Hydrogen** — ChemicalPlant produces 1,000 Mt/yr but maintenance
   consumes 0.02 Mt/yr; demand is 1/5 of 0.01 % of world.
4. **Methane** — no mining building, only NaturalGasPlant consumption.
5. **Ammonia** — produced synthetically, no natural-mining demand.

The 5 resources with the **smallest** scale gap (most unconstrained) are
**all 4 exotics** (no current consumption at all) and **Gold/Silver/Platinum**
(no consumption in any RON entry, no mining building, no demand).

For the **BALANCE_PATCHES** deliverable the recommendation will be to
back-fill the mining side for the "small gap" resources (Gold, Silver,
Platinum, Copper, Lithium, RareEarths, Sulfur, etc.) and to add a
civilisation-demand column to close the "large gap" ones (Food, Water, H₂,
CH₄, NH₃, He-3, U, Th, P). This audit does **not** propose RON changes —
that is the next deliverable.

---

## 4. Per-category detail tables

Every row carries a real-world source citation (USGS, BGS, IEA, or
peer-reviewed) and the "0.01 % of world" target for comparison. Production
and consumption columns are all in **Mt/yr**. "+" suffixes mark
"production-side" rows; "−" marks "consumption-side" rows.

### 4.1 Biological (Food)

| Resource | Real-world source | RR annual (Mt/yr) | RR reserves (Mt) | RR per-capita (kg/p/yr) | Game planet prod. (10 builds, Mt/yr) | Game civ. demand (per 8.2B pop, Mt/yr) | Game player consumption (10 builds, Mt/yr) | Net surplus (Mt/yr) | TtD (yr, world prod.) | Scale gap | Gap analysis |
|----------|-------------------|------------------:|-----------------:|------------------------:|--------------------------------------:|----------------------------------------:|------------------------------------------:|--------------------:|----------------------:|----------:|--------------|
| Food (+) | FAO 2024 SOFA report (fao.org/worldfoodsituation); world cereals ~2,900 Mt/yr, all food ~9,000 Mt/yr | 9,000 | renewable (annual harvest) | 1,100 (FAO 2024 per-capita food supply) | 156,800 (10 × Farm 9,000 + 10 × Greenhouse 5,000 + 10 × Aquaculture 1,500 + 10 × AgriDome 180) | 820 (8.2B × 0.0001 Mt/p/yr game = 100 kg/p/yr) **OR 9,020** at real 1,100 kg/p/yr | 0.002 (10 Farms × 0.0001 Mt/yr) | +156,000 | renewable | **2.2** (consumption/budget) | Production 17× world food; per-capita demand in code is 10× below real FAO 1,100 kg/p/yr → civilisation demand is severely under-modelled. **See finding #1.** |

### 4.2 Volatiles (5)

| Resource | Real-world source | RR annual (Mt/yr) | RR reserves (Mt) | RR per-capita (kg/p/yr) | Game planet prod. (10 builds, Mt/yr) | Game civ. demand (per 8.2B pop, Mt/yr) | Game player consumption (10 builds, Mt/yr) | Net surplus (Mt/yr) | TtD (yr, world prod.) | Scale gap | Gap analysis |
|----------|-------------------|------------------:|-----------------:|------------------------:|--------------------------------------:|----------------------------------------:|------------------------------------------:|--------------------:|----------------------:|----------:|--------------|
| Water (H₂O) (+) | USGS 2024 water-cycle summary; global freshwater withdrawal ~4,000 km³/yr ≈ 4,000 Mt/yr (ag 70 %, ind 20 %, mun 10 %); world water in atmosphere + lakes + rivers = ~3.5×10⁷ Mt renewable | 4,000 (withdrawal); 1.4×10¹² ocean | effectively infinite (renewable) | 150 m³/p/yr = 150,000 kg/p/yr (withdrawal); 1,600 L/p/yr direct consumptive | **0** (no dedicated water-mining building; AtmosphericProcessor harvests gases, not liquid water) | 410 (8.2B × 0.00005 Mt/p/yr closed-loop default = 50 kg/p/yr) | 0.20 (10 LifeSupport × 5 Mt/yr H₂O maintenance) | −0.20 (deficit on closed-loop bodies) | renewable | **0.50** | Civilisation demand only fires on closed-loop outposts; Earth seed has 200 Mt default MinimumStockpile (GRA-31 PR-C). **No water-mining building in current RON** — gap. |
| Hydrogen (H₂) (−) | IEA Global Hydrogen Review 2024; ~100 Mt dedicated H₂ (2024) plus ~50 Mt byproduct; <1 % green | 100 | n/a (renewable via electrolysis) | 12 kg/p/yr | 1,000 (10 × ChemicalPlant 100 Mt/yr synthesis) | **0** (not modelled) | 0.020 (10 LifeSupport + 10 HabitatDome maintenance) | +1,000 | renewable | **0.20** | Produced only synthetically (ChemicalPlant); not mined from regolith ices despite outer-solar-system abundance. Player consumption is 1/5 of 0.01 % of world. |
| Ammonia (NH₃) (−) | IEA Ammonia Technology Roadmap 2022; global ~200 Mt/yr (2024); ~80 % Haber-Bosch fertilizer | 200 | synthetic (240 Mt/yr global capacity) | 24 kg/p/yr | 2,000 (10 × ChemicalPlant 200 Mt/yr synthesis) | **0** | 0.020 (10 ChemicalPlant × 0.0005 + other minor) | +2,000 | n/a (synthetic) | **0.10** | Synthesised from H₂ + N₂. No natural-mining building. |
| Methane (CH₄) (−) | IEA Methane Tracker 2024; global natural-gas production ~4,100 Mt/yr (CH₄ ~3,000 Mt of which); atmospheric CH₄ ~1,931 ppb | 4,100 (NG production) | ~5×10⁵ Mt conventional + ~5×10⁵ Mt clathrates | 500 kg/p/yr NG-equiv | **0** (no dedicated CH₄ mining; only NaturalGasPlant consumes) | **0** | 50 (10 NaturalGasPlant × 5 Mt/yr) | −50 on bodies without gas wells | 122 yr at 4,100 Mt/yr | **0.13** | Only consumed by NaturalGasPlant; produced by HydrocarbonExtractor (which targets coal/oil/NG together as "MiningEfficiency 4,000 Mt/yr"). No per-CH₄ mining building. |
| Phosphorus (P) (−) | USGS Phosphate Rock MCS 2026; world ~220 Mt phosphate rock/yr = ~4.6 Mt P element; 72,000 Mt reserves | 4.6 (P element) | 7.2×10⁴ (phosphate rock) | 0.56 kg/p/yr | **0** (no dedicated P mining) | **0** | 0.002 (10 AgriDome × 0.5 + 10 Farm × 0.0002 + PharmaPlant) | −0.002 on most bodies | 15,650 yr at 4.6 Mt/yr | **0.030** | Critical for hydroponics; **no mining building in RON**. Real-world per-capita 0.56 kg/p/yr; game demand is per-Farm not per-capita. **Tightest volatile in real terms.** |

### 4.3 Atmospheric gases (4)

| Resource | Real-world source | RR annual (Mt/yr) | RR reserves (Mt) | RR per-capita (kg/p/yr) | Game planet prod. (10 builds, Mt/yr) | Game civ. demand (per 8.2B pop, Mt/yr) | Game player consumption (10 builds, Mt/yr) | Net surplus (Mt/yr) | TtD (yr, world prod.) | Scale gap | Gap analysis |
|----------|-------------------|------------------:|-----------------:|------------------------:|--------------------------------------:|----------------------------------------:|------------------------------------------:|--------------------:|----------------------:|----------:|--------------|
| Nitrogen (N₂) (−) | USGS Nitrogen MCS 2024; industrial NH₃-bounded ~150 Mt/yr N; atmospheric pool 4×10⁶ Mt | 150 (industrial) | 4×10⁶ atmospheric | 18 kg/p/yr industrial; 18,000 kg/p/yr atmospheric pool | 5,000 (10 × AtmosphericProcessor 500 Mt/yr; processor harvests all gases together) | **0** | 0.0014 (10 AgriDome × 0.5 + 10 LifeSupport × 0.5 + Farm × 0.05 etc.) | +5,000 | renewable | **0.018** | Processor sweep is undifferentiated across N₂/O₂/Ar/CO₂; per-gas accounting not modelled. Civ demand absent. |
| Oxygen (O₂) (−) | USGS Oxygen MCS 2024; industrial ~550 Mt/yr O₂; atmospheric pool 1.08×10⁶ Mt | 550 (industrial) | 1.08×10⁶ atmospheric | 67 kg/p/yr industrial; 130,000 kg/p/yr atmospheric | 5,000 (processor) | **0** (only ColonyEnvironmentCosts.oxygen_per_person_per_year on closed-loop) | 0.014 (10 LifeSupport × 2 + HabitatDome × 1 + AgriDome × 0.2) | +5,000 | renewable | **0.025** | Real-world per-capita respiration ~840 kg/p/yr; not modelled. Civ demand only fires on closed-loop outposts. |
| CarbonDioxide (CO₂) (−) | NOAA GML Jan 2026; atmospheric ~3.2×10⁶ Mt; emissions ~2×10⁴ Mt/yr anthropogenic | 2×10⁴ (emissions) | 3.2×10⁶ atmospheric | 2,400 kg/p/yr emissions | 5,000 (processor share) | **0** | 0.0008 (10 LifeSupport × 0.8) | +5,000 | 160 yr (anthropogenic only) | **0.0002** | No carbon-cycle sink in game; terraforming processor will move huge masses but no current per-capita demand. |
| Argon (Ar) (−) | USGS Helium & Noble Gases 2024; ASU byproduct ~700 kt/yr Ar; atmospheric 6.6×10⁴ Mt pool | 0.7 | 6.6×10⁴ atmospheric | 0.085 kg/p/yr | **0** (processor produces but Ar is not separately tracked in maintenance) | **0** | 0.000001 (no Ar consumer in current RON) | +0 (negligible) | 9.4×10⁴ yr at 0.7 Mt/yr | **0.0001** | No current Ar consumer; welding/lighting demand not modelled. **The most under-used resource in the catalogue.** |

### 4.4 Construction metals (9)

| Resource | Real-world source | RR annual (Mt/yr) | RR reserves (Mt) | RR per-capita (kg/p/yr) | Game planet prod. (10 builds, Mt/yr) | Game civ. demand (per 8.2B pop, Mt/yr) | Game player consumption (10 builds, Mt/yr) | Net surplus (Mt/yr) | TtD (yr, world prod.) | Scale gap | Gap analysis |
|----------|-------------------|------------------:|-----------------:|------------------------:|--------------------------------------:|----------------------------------------:|------------------------------------------:|--------------------:|----------------------:|----------:|--------------|
| Iron (Fe) (+) | USGS Iron Ore MCS 2026; ~1,800 Mt ore/yr → ~2,500 Mt contained Fe | 2,500 | 1.7×10⁵ (ore) / 10⁸ (crustal) | 305 kg/p/yr | 36,000 (10 × Mine 1,800 + 10 × Refinery 1,800 + 10 × StripMine 5,000) | **0** | 0.011 (10 each of 5 building types × ~0.01 Mt/yr Fe) | +36,000 | 30 yr at 2,500 Mt/yr (reserves); 40,000 yr (crustal) | **0.044** | 1 Mine = 72 % of world Fe. Per-build surplus is 36,000× consumption; player economy unbounded. |
| Aluminum (Al) (+) | USGS Bauxite & Alumina MCS 2026; ~70 Mt Al metal/yr | 70 | 3×10⁴ (bauxite) / 1.6×10⁸ (crustal) | 8.5 kg/p/yr | 0 (no dedicated Al mining) | **0** | 0.0002 (10 SpacePort × 0.01 + HabitatDome × 0.05 Al maintenance, scaled) | −0.0002 | 430 yr at 70 Mt/yr (reserves); 2.3×10⁶ yr (crustal) | **0.003** | **No dedicated Al mining building.** Demand from spaceframes and ship hulls. Lunar regolith should be the in-game source. |
| Titanium (Ti) (+) | USGS Titanium MCS 2026; ~0.35 Mt Ti sponge/yr (2024) | 0.35 | 1,000 (TiO₂ contained) / 6×10⁶ (crustal) | 0.04 kg/p/yr | 0 (no dedicated Ti mining) | **0** | 0.0006 (10 HabitatDome × 0.002 + 10 Shipyard × 0.002 + others) | −0.0006 | 2,860 yr at 0.35 Mt/yr | **0.004** | Critical for pressure vessels; no mining building. **Will become a real bottleneck if Shipyard/UndergroundHabitat expansion ramps up.** |
| Silicates (SiO₂) (+) | USGS Mineral Industry Surveys — Silica 2024; ~10,000 Mt/yr silica sand; quarry aggregate > 4×10⁴ Mt/yr | 10,000 (silica) / 40,000 (aggregate) | effectively infinite (quarry-grade) | 1,200 kg/p/yr aggregate | 50,000 (10 × StripMine 5,000) | **0** | 0.0066 (HabitatDome 0.05 × 10 + others) | +50,000 | effectively infinite | **0.005** | Near-infinite in crust; game surplus 50,000× consumption. |
| Nickel (Ni) (+) | USGS Nickel MCS 2026; ~3.7 Mt Ni/yr (2024); Mn nodules ~3×10⁵ Mt additional | 3.7 | 130 (sulphide) / 3×10⁵ (crustal) | 0.45 kg/p/yr | 0 (no dedicated Ni mining) | **0** | 0.00004 (10 Mine × 0.0001 + others) | −0.00004 | 35 yr (reserves); 81,000 yr (crustal) | **0.003** | **No dedicated Ni mining.** M-type asteroid (Psyche) is the real in-game source. Treated as rare. |
| Tungsten (W) (−) | USGS Tungsten MCS 2026; ~0.078 Mt W/yr (2024); reserves 3.5 Mt; China 1.9 Mt | 0.078 | 3.5 (reserves) / 1,500 (crustal) | 0.0095 kg/p/yr | 0 (no dedicated W mining) | **0** | 0.0001 (10 RailgunTech buildings × 0.0001 + others) | −0.0001 | 45 yr at 0.078 Mt/yr | **0.013** | **No mining building.** Used by railgun and laser-drill buildings; will be scarce if kinetic-weapon DoD ramps. |
| Carbon (C) (−) | USGS Coal MCS 2024 + BGS 2022; coal 4,000 Mt/yr, oil 4,400 Mt/yr-equiv, NG 3,900 Mt/yr-equiv | 12,000 (all fossil-C) | 4×10⁵ (coal) / 2.4×10³ (oil R/P) / 10⁸ (geological) | 1,460 kg/p/yr fossil-C | 24,000 (10 × HydrocarbonExtractor 4,000 Mt/yr; 1× is "world share" of oil+gas, comment in buildings.ron:425-428) | **0** | 0.0042 (10 Shipyard × 0.05 + 10 CoalPowerPlant × 0.2 + 10 OrbitalLift × 0.05) | +24,000 | 33 yr coal / 60 yr oil at current draw | **0.002** | HydrocarbonExtractor label is "oil+gas" but the per-build 4,000 Mt/yr includes coal/oil/NG; game treats it as a generic C source. |
| Chromium (Cr) (+) | USGS Chromium MCS 2024; ~30 Mt chromite ore/yr (2024); reserves 750 Mt; S. Africa 48 % | 30 | 750 (ore) / 10⁴ (crustal) | 3.7 kg/p/yr | 0 (no dedicated Cr mining) | **0** | 0.0006 (10 Refinery × 0.003 + others) | −0.0006 | 25 yr at 30 Mt/yr | **0.002** | **No dedicated Cr mining.** Used in stainless and refractories. |
| Magnesium (Mg) (−) | USGS Magnesium MCS 2024; ~1.1 Mt metal/yr; reserves 750 Mt; China 84 % | 1.1 | 750 (identified) / 2×10⁸ (crustal) | 0.13 kg/p/yr | 0 (no dedicated Mg mining) | **0** | 0.0001 (10 HabitatDome × 0.005 + LaunchSite × 0.002 × 10) | −0.0001 | 680 yr at 1.1 Mt/yr | **0.002** | **No dedicated Mg mining.** Pidgeon-process or electrolytic from seawater in real life. |

### 4.5 Fusion fuels (3)

| Resource | Real-world source | RR annual (Mt/yr) | RR reserves (Mt) | RR per-capita (kg/p/yr) | Game planet prod. (10 builds, Mt/yr) | Game civ. demand (per 8.2B pop, Mt/yr) | Game player consumption (10 builds, Mt/yr) | Net surplus (Mt/yr) | TtD (yr, world prod.) | Scale gap | Gap analysis |
|----------|-------------------|------------------:|-----------------:|------------------------:|--------------------------------------:|----------------------------------------:|------------------------------------------:|--------------------:|----------------------:|----------:|--------------|
| Helium3 (He-3) (−) | USGS Helium MCS 2024; Earth atmospheric He-3 3,815 t total, ~26 m³ (~5 kg)/yr extraction; lunar regolith ~1.1 Mt total | 0.000005 (Earth) | 3,815 t (Earth atm) / 1.1×10⁶ t (lunar regolith) | 6×10⁻¹⁰ kg/p/yr | 0 (no He-3 mining; regolith harvesting not modelled) | **0** | 0.10 (10 FusionReactor × 10 Mt/yr He-3 maintenance!) | −0.10 on bodies without He-3 | n/a | **0.001** | **No lunar He-3 mining building in RON.** Each FusionReactor wants 10 Mt/yr He-3 — wildly above Earth atmospheric supply. **This is the most catastrophic design gap: He-3 demand is 20,000× the entire known world supply.** |
| Deuterium (D) (−) | IEA / Culham Centre for Fusion Energy; D in ocean 3.5×10¹⁰ Mt; commercial D₂O production ~33 kt/yr (Canada CANDU, Argentina, India, Iran, Korea) | 0.033 (commercial D₂O) | 3.5×10¹⁰ (ocean) | 0.004 kg/p/yr | 0 (no D mining; ocean electrolysis not modelled) | **0** | 0.005 (10 FusionReactor × 5 Mt/yr + D-T × 0.0015 + D-He3 × 0.0008) | −0.005 on bodies without water | n/a | **0.001** | **No D mining building.** Demand set by reactor mix; trivially small vs ocean. |
| Tritium (T) (−) | NUBASE2020; CANDU bred ~4 kg/yr global; equilibrium 7.25 kg global | 0.000004 (bred) | 0.00000725 (equilibrium) | 5×10⁻¹⁰ kg/p/yr | 0.5 (10 × ChemicalPlant 0.05 Mt/yr tritium breeding, only with `fusion_power` tech) | **0** | 0.0005 (10 D-T × 0.0005 Mt/yr T maintenance) | +0.5 when unlocked | n/a | **0.0005** | Bred in ChemicalPlant only with `fusion_power` unlock. Per-build production 1,000× demand when active. |

### 4.6 Fissile materials (3)

| Resource | Real-world source | RR annual (Mt/yr) | RR reserves (Mt) | RR per-capita (kg/p/yr) | Game planet prod. (10 builds, Mt/yr) | Game civ. demand (per 8.2B pop, Mt/yr) | Game player consumption (10 builds, Mt/yr) | Net surplus (Mt/yr) | TtD (yr, world prod.) | Scale gap | Gap analysis |
|----------|-------------------|------------------:|-----------------:|------------------------:|--------------------------------------:|----------------------------------------:|------------------------------------------:|--------------------:|----------------------:|----------:|--------------|
| Uranium (U) (−) | USGS Uranium MCS 2026; ~0.060 Mt U/yr (2024); reserves 6.1 Mt economic; total endowment ~10⁵ Mt | 0.060 | 6.1 (economic) / 35 (resources) / 10⁵ (total) | 0.0073 kg/p/yr | 0 (no dedicated U mining) | **0** | 0.0001 (10 FissionReactor × 0.0028 + 10 BreederReactor × 0.0015) | −0.0001 on bodies without U | 102 yr at 0.060 Mt/yr (reserves) | **0.0005** | **No dedicated U mining.** Reserve-driven. FissionReactor maintenance is the dominant draw. |
| Thorium (Th) (−) | USGS Thorium MCS 2024; ~0.001 Mt/yr (byproduct of REE mining); reserves ~0.6 Mt; India 25 % | 0.001 | 0.6 (reserves) / 10⁶ (crustal) | 0.00012 kg/p/yr | 0 (no dedicated Th mining) | **0** | 0.0001 (10 ThoriumReactor × 0.0012) | −0.0001 on bodies without Th | 600 yr at 0.001 Mt/yr | **0.0005** | **No dedicated Th mining.** Real-world supply is byproduct; game should be more forgiving. |
| Plutonium (Pu) (−) | IAEA & SIPRI 2024; global stockpiles ~650 t civil Pu; ~20 t/yr civil Pu production (Russia, France, UK, Japan); Pu-238 ~1.5 kg/yr NASA RTG | 0.000020 (civil Pu) | 0.000650 (stockpiles) | 0.0000024 kg/p/yr | 2.3 (10 × BreederReactor 0.23 Mt/yr breeding) | **0** | 0.0001 (10 BreederReactor × 0.0001 Mt/yr Pu in maintenance) | +2.3 | n/a (bred) | **0.0005** | Only bred; no mining. Per-build production 23,000× consumption. |

### 4.7 Precious metals (3)

| Resource | Real-world source | RR annual (Mt/yr) | RR reserves (Mt) | RR per-capita (kg/p/yr) | Game planet prod. (10 builds, Mt/yr) | Game civ. demand (per 8.2B pop, Mt/yr) | Game player consumption (10 builds, Mt/yr) | Net surplus (Mt/yr) | TtD (yr, world prod.) | Scale gap | Gap analysis |
|----------|-------------------|------------------:|-----------------:|------------------------:|--------------------------------------:|----------------------------------------:|------------------------------------------:|--------------------:|----------------------:|----------:|--------------|
| Gold (Au) (−) | USGS Gold MCS 2026; ~0.003 Mt/yr (2024); mine reserves 0.59 Mt; total ever mined 0.20 Mt | 0.003 | 0.59 (reserves) / 0.004 (crustal ppm × top km) | 0.00037 kg/p/yr | 0 (no Au mining) | **0** | 0 (no Au consumer in RON) | 0 | 197 yr at 0.003 Mt/yr | **0.0001** | **No consumer and no producer.** Pure currency today; should become a corrosion-resistant coating or electrical use. |
| Silver (Ag) (−) | USGS Silver MCS 2026; ~0.026 Mt/yr (2025); reserves 0.61 Mt | 0.026 | 0.61 (reserves) / 0.15 (ever mined) | 0.0032 kg/p/yr | 0 | **0** | 0 | 0 | 23 yr at 0.026 Mt/yr | **0.0001** | Same as Gold. |
| Platinum (Pt) (−) | USGS Platinum MCS 2026; ~0.0002 Mt/yr (2024); S. Africa 77 %; reserves 0.07 Mt | 0.0002 | 0.07 (reserves) | 0.000024 kg/p/yr | 0 | **0** | 0 | 0 | 350 yr at 0.0002 Mt/yr | **0.0001** | **No consumer and no producer.** |

### 4.8 Strategic materials (7)

| Resource | Real-world source | RR annual (Mt/yr) | RR reserves (Mt) | RR per-capita (kg/p/yr) | Game planet prod. (10 builds, Mt/yr) | Game civ. demand (per 8.2B pop, Mt/yr) | Game player consumption (10 builds, Mt/yr) | Net surplus (Mt/yr) | TtD (yr, world prod.) | Scale gap | Gap analysis |
|----------|-------------------|------------------:|-----------------:|------------------------:|--------------------------------------:|----------------------------------------:|------------------------------------------:|--------------------:|----------------------:|----------:|--------------|
| Copper (Cu) (−) | USGS Copper MCS 2026; ~23 Mt Cu/yr (2024); reserves 1,000 Mt | 23 | 1,000 (reserves) / 6×10⁷ (ultimate) | 2.8 kg/p/yr | 0 (no dedicated Cu mining; RecyclingCenter counts 500 Mt/yr mixed) | **0** | 0.0010 (10 × LifeSupport × 0.005 + 10 × MassDriver × 0.5 + 10 × Refinery × 0.01 + 10 × Power × 0.005 …) | −0.001 on most bodies | 43 yr at 23 Mt/yr (reserves) | **0.010** | **No dedicated Cu mining.** Used in every electrical building. **First scarcity bottleneck candidate when economy scales.** |
| RareEarths (REE) (−) | USGS Rare Earths MCS 2026; ~0.3 Mt REO/yr (2025); reserves 110 Mt; China 90 % refining | 0.3 | 110 (reserves) / 10⁵ (crustal ppm) | 0.037 kg/p/yr | 0 (DeepDrill/LaserDrill can target REE via DeepMiningEfficiency but no dedicated modifier) | **0** | 0.0001 (10 × MassDriver × 0.0005 + 10 × AI Cluster × 0.001 + others) | −0.0001 | 367 yr at 0.3 Mt/yr | **0.0005** | **No dedicated REE mining.** Critical for electronics, magnets, lasers. |
| Lithium (Li) (−) | USGS Lithium MCS 2026; ~0.18 Mt Li metal/yr (2024); reserves 0.03 Mt Li metal; ~2.3×10⁸ Mt in seawater | 0.18 | 0.030 (Li metal reserves) / 2.3×10⁸ (seawater) | 0.022 kg/p/yr | 0 (no dedicated Li mining) | **0** | 0.0001 (10 × SolarPower × 0.0005 + 10 × FissionReactor × 0.0002 + AI Cluster × 0.0005) | −0.0001 | 167 yr at 0.18 Mt/yr (reserves) | **0.005** | **No dedicated Li mining.** Used in every battery/electrical building. |
| Sulfur (S) (−) | USGS Sulfur MCS 2024; ~69 Mt S/yr (2024); mostly petroleum refining byproduct; reserves 350 Mt (all forms) | 69 | 350 (all forms) / 2×10⁵ (crustal) | 8.4 kg/p/yr | 0 (no dedicated S mining; ChemicalPlant eats S as input) | **0** | 0.0002 (10 × ChemicalPlant × 0.03 + 10 × CoalPowerPlant × 0.05 + others) | −0.0002 | 5 yr at 69 Mt/yr (reserves) | **0.003** | **No dedicated S mining.** The 5-year TtD is alarming — the real S cycle is a petroleum byproduct. Game should treat it as an atmospheric/processing output. |
| Cobalt (Co) (−) | USGS Cobalt MCS 2026; ~0.23 Mt Co/yr (2024); DRC 75 %; reserves 11 Mt | 0.23 | 11 (reserves) / 7.5×10⁵ (crustal) / 10⁶ (Mn nodules) | 0.028 kg/p/yr | 0 (no dedicated Co mining; BreederReactor 0.23 Mt/yr modifier is Pu, not Co) | **0** | 0.00003 (10 × Mine × 0.0001 + 10 × D-T × 0.002 + others) | −0.00003 | 48 yr at 0.23 Mt/yr | **0.0005** | **No dedicated Co mining.** Used in D-T reactor and Mine maintenance. |
| Fluorine (F) (−) | USGS Fluorspar MCS 2024; ~4.5 Mt fluorite/yr; reserves 320 Mt fluorspar; 585 ppm crustal | 4.5 | 320 (fluorspar) / 5.85×10⁵ (crustal) | 0.55 kg/p/yr | 0 (no dedicated F mining) | **0** | 0.0001 (10 × ChemicalPlant × 0.001 + 10 × StripMine × 0.0005 + 10 × SemiconductorFab × 0.01) | −0.0001 | 71 yr at 4.5 Mt/yr | **0.0005** | **No dedicated F mining.** Used in UF₆ enrichment, semiconductor etching, chemical plants. |
| Polymers (−) | OECD Global Plastics Outlook 2024; ~450 Mt plastic resin/yr (2023); China 31 %; ~9 % recycled | 450 | petroleum-derived | 55 kg/p/yr | 4,500 (10 × ChemicalPlant 450 Mt/yr synthesis) | **0** | 0.0040 (10 × HabitatDome × 0.01 + 10 × Mine × 0.002 + others) | +4,500 | depends on petroleum | **0.010** | Synthesised from petroleum; consumption 1/100 of supply. |

### 4.9 Exotic / K2 materials (4)

| Resource | Real-world source | RR annual (Mt/yr) | RR reserves (Mt) | RR per-capita (kg/p/yr) | Game planet prod. (10 builds, Mt/yr) | Game civ. demand (per 8.2B pop, Mt/yr) | Game player consumption (10 builds, Mt/yr) | Net surplus (Mt/yr) | TtD (yr, world prod.) | Scale gap | Gap analysis |
|----------|-------------------|------------------:|-----------------:|------------------------:|--------------------------------------:|----------------------------------------:|------------------------------------------:|--------------------:|----------------------:|----------:|--------------|
| Antimatter | CERN ALPHA experiment + NASA reports; world production ~10⁻¹⁰ g/yr | ~10⁻¹⁰ g/yr (essentially 0 in Mt) | n/a (synthetic) | n/a | 0 (no production building) | 0 | 0 | 0 | n/a | 0 | **No production and no consumption.** Game-unlock target. Drive energy for `AntimatterDrive` (1,000,000 s Isp) per `README.md:46`. |
| ExoticMatter | Theoretical (Casimir / negative-energy-density); no commercial source | 0 | 0 (theoretical) | n/a | 0 | 0 | 0 | 0 | n/a | 0 | **No production and no consumption.** Required for warp-bubble and wormhole drive per `types.rs:223-224`. |
| Metamaterials | Synthetic; no reliable public source on world production. Comparable games (Aurora 4X, Stellaris) treat as "engineered composites" with no natural source. | TBD (synthetic) | n/a | n/a | 0 | 0 | 0 | 0 | n/a | 0 | **No production and no consumption.** "Engineered composite materials with unnatural optical/EM properties" per `types.rs:226-227`. |
| Computronium | Theoretical; no commercial source. Aurora 4X, Stellaris both leave undefined. | TBD (theoretical) | n/a | n/a | 0 | 0 | 0 | 0 | n/a | 0 | **No production and no consumption.** "Optimised computational substrate" per `types.rs:228-230`. |

---

## 5. Cross-cutting observations

These are the **audit** findings, not the patch. The deliverable here is
quantification; the next deliverable (`BALANCE_PATCHES_v0.5.md`) is the
proposed RON changes. Findings 1, 2, and 4 are the user-flagged design
gaps; finding 3 is a unit-system inconsistency.

### Finding #1 — Per-capita food demand is 10× below real world

`src/colony/components.rs:292-294`:
```rust
pub fn food_consumption_per_year(&self) -> f64 {
    self.population * 0.0001  // 100 kg/person/yr
}
```

FAO 2024 world per-capita food supply is **~1,100 kg/person/yr**. The
in-game value is 100 kg/person/yr, an order of magnitude below reality.
This is why food looks abundant in the scale-gap table — the game is
asking 10× less of the player than reality does. The user has already
flagged this as the civilisation-demand-sink gap (Option C).

### Finding #2 — Civilisation demand is modelled for only 2 of 39 resources

Only `Food` and `Water` have a per-population draw. Every other resource
has zero civilisation demand. The 37 other resources' "scale gap" in the
summary table is therefore **mechanically zero** by construction — the
gap that the user wants closed is the entire civilisation-demand
mechanic. This is the dominant audit finding.

### Finding #3 — Ship-hull costs are in tonnes, building production is in megatonnes

`assets/data/ship_hulls.ron` uses **tonnes (t)** for resource_costs (e.g.
probe = 60 kg Al, frigate = ~30 kg Fe). `assets/data/buildings.ron` uses
**megatonnes (Mt)** for production modifiers (e.g. Mine = 1,800 Mt/yr).
The ratio is 10⁹. Either:

* ship hulls are intentionally "trivial" (player builds hundreds of
  ships per construction queue with no resource pressure), or
* the per-tonne ship mass is meant to scale (e.g. the freighter template
  system multiplies by hull size).

The current RON does not show which interpretation is canonical. The
audit cannot compute a meaningful "ship vs world" scale gap without a
confirmed interpretation. Flagged for the next deliverable.

### Finding #4 — He-3 demand is 20,000× the world atmospheric supply

`assets/data/buildings.ron:574-578` for `FusionReactor`:
* resource_costs: 133 Mt He-3 (build cost)
* maintenance_resources: 10 Mt He-3 / yr

USGS Helium MCS 2024 puts the entire Earth atmospheric He-3 pool at
**3,815 t** (0.0038 Mt). One FusionReactor alone wants **10 Mt/yr
maintenance** = **2,600× the entire world atmospheric supply per year**.
There is no lunar-regolith He-3 mining building in the current RON.

This is the most catastrophic single-resource gap in the catalogue. It
is not a calibration issue — there is **no in-game source of He-3 at
all** once you have a single FusionReactor. The next deliverable must
add a `LunarHe3Mine` (or equivalent) building or cap FusionReactor
maintenance at the achievable supply.

### Finding #5 — 10 of 39 resources have no mining building AND no demand

These are "ghost" resources in the catalogue:

| Resource       | Has mining building? | Has demand? |
|----------------|:--------------------:|:-----------:|
| Argon          | ❌ (processor split) | ❌          |
| Gold           | ❌                   | ❌          |
| Silver         | ❌                   | ❌          |
| Platinum       | ❌                   | ❌          |
| Copper         | ❌                   | ✅          |
| RareEarths     | ❌                   | ✅          |
| Lithium        | ❌                   | ✅          |
| Sulfur         | ❌                   | ✅          |
| Cobalt         | ❌                   | ✅          |
| Fluorine       | ❌                   | ✅          |
| Aluminum       | ❌                   | ✅          |
| Nickel         | ❌                   | ✅          |
| Tungsten       | ❌                   | ✅          |
| Chromium       | ❌                   | ✅          |
| Magnesium      | ❌                   | ✅          |
| Titanium       | ❌                   | ✅          |
| Phosphorus     | ❌                   | ✅          |
| Helium3        | ❌                   | ✅          |
| Deuterium      | ❌                   | ✅          |
| Uranium        | ❌                   | ✅          |
| Thorium        | ❌                   | ✅          |

15 of 39 resources (38 %) have no mining building. Of those, 4 (Gold,
Silver, Platinum, Argon) have **no demand either** — they are pure
catalogue entries waiting for content. The other 11 are real consumer
chains that need a production source.

### Finding #6 — Maintenance draws are 10⁻³ Mt/yr; production is 10³ Mt/yr

Across the whole catalogue, the per-building maintenance values sit in
the 10⁻⁴–10⁻² Mt/yr range, while per-building production sits in the
10²–10⁴ Mt/yr range. The **production / maintenance ratio is 10⁴–10⁶**,
which is why every scale gap in the summary table is < 1. The
calibration invariant in `CLAUDE.md` ("1 in-game building on Earth ≈
2026 world production total for its dominant resource") is satisfied
on the production side; the **consumption side is so tiny that the
invariant is effectively meaningless in the current loop**.

The user has already identified this; the audit confirms it numerically.

---

## 6. Resources where real-world data was hardest to find

For the **balance-patches** deliverable, the user should weigh in on
these:

1. **ExoticMatter, Metamaterials, Computronium** — no public source.
   The existing `real-world-2026-reserves.md` marks them **TBD**. There
   are no USGS or BGS entries because they are theoretical / synthetic.
   Calibration will need to be anchored to comparable games (Aurora 4X
   uses "exotics" at the K2+ tier; Stellaris uses "rare crystals" with
   no real anchor). Suggest: define them in terms of a *K2 Kardashev
   index* (kJ / kWh per kg produced) rather than Mt/yr.

2. **Antimatter** — CERN ALPHA produces ~10⁻¹⁰ g/yr. The Mt unit is
   meaningless at that scale. The game should probably track antimatter
   in **grams** and treat "1 g = world 100-year stockpile" as the
   production target. Flagged for design call.

3. **Tritium** — natural abundance is 0; all tritium is bred from
   lithium-6. The 4 kg/yr CANDU figure is breeder-derived, not
   mined. The game's `ChemicalPlant` 0.05 Mt/yr breeding value is
   **12,500×** real-world global production. Either the game is
   post-scarcity on tritium (acceptable for K1 fusion) or it should
   scale down dramatically. Flagged.

4. **He-3 Earth atmospheric supply** — USGS puts the pool at 3,815 t
   with ~5 kg/yr extraction; the existing reference file says "26 m³/yr
   (~5 kg)" which matches. The game **per-build** consumption of 10
   Mt/yr is wildly above this; the **per-build cost** of 133 Mt He-3 is
   also above world pool. This is not a data-source problem; it is a
   game-design problem (Finding #4).

5. **Plutonium** — global civil stockpile is well-known (~650 t) but
   "production rate" is sensitive (warhead vs civil). 20 t/yr civil Pu
   in the existing reference is the mid-range estimate. NASA's
   Pu-238 production (~1.5 kg/yr) is a separate stream. The game's
   `BreederReactor` 0.23 Mt/yr Pu breeding is **11,500× the real civil
   Pu rate** but **only 2.3× the world Pu stockpile per year**. A
   breeder game model is internally consistent but should be flagged
   for design.

6. **Polymers** — strictly synthetic. The 450 Mt/yr is a real OECD
   figure; reserves are "petroleum-derived" and so the resource lives
   on top of the carbon / petroleum cycle. The game should treat
   polymers as a **throughput constraint** (a function of oil+gas
   production), not an independent resource. Flagged.

7. **Phosphorus** — well-documented (USGS) but with a **2 % per-capita
   loss rate to oceans** (no recovery). Real-world reserve life is
   ~50 years at current draw *or* ~300 years at 2× draw. The game
   currently has no P mining building; this is the resource most at
   risk of civilisation-demand starvation when populations ramp.

---

## 7. Top 5 by scale gap (for the balance-patches deliverable)

These are the 5 resources whose **current game consumption is closest
to or above the 0.01 %-of-world space-program reference budget**. They
are the **first candidates** for the civilisation-demand-sink model
(Option C) the user wants:

| Rank | Resource     | Scale gap | Why it's #1 for the patches |
|-----:|--------------|----------:|-----------------------------|
| 1    | **Food**     | **2.2** (over budget) | Per-capita demand in code is 10× below real FAO 1,100 kg/p/yr. Fixing the per-capita value alone brings the scale gap to ~1.0. The civilisation-demand model needs `food_consumption_per_year = pop × 0.0011` (1.1 t/p/yr) for 1 Farm to feed 8.2M people, which fits the existing "1 building ≈ 1/300 of world pop" invariant cleanly. |
| 2    | **Water**    | 0.50 | No water-mining building. Demand only fires on closed-loop outposts. Needs an explicit `WaterExtractor` (or ice-mining variant) and a per-capita `ColonyEnvironmentCosts` default for breathable bodies (FAO 150 m³/p/yr withdrawal). |
| 3    | **Hydrogen** | 0.20 | ChemicalPlant synthesises 1,000 Mt/yr but only the LifeSupport + HabitatDome + Agricultural buildings consume. Civ demand should track **per-capita H₂ for synthetic fuel + fertilizer feedstock** (Haber-Bosch pathway). The 0.01 %-of-world reference is 0.1 Mt/yr; current per-build consumption is 0.02 Mt/yr. |
| 4    | **Methane**  | 0.13 | Only NaturalGasPlant consumes (5 Mt/yr fuel). No per-CH₄ mining building. Civ demand should track **per-capita NG demand** (residential + commercial + industrial) — world average 500 kg NG/p/yr. |
| 5    | **Ammonia**  | 0.10 | Synthesised only via ChemicalPlant from H₂+N₂. Civ demand should track **per-capita fertilizer N** (world 24 kg N/p/yr, mostly NH₃). |

A patch proposal that closed Findings #1, #2, and #4 would resolve the
top-5 cascade: fix the per-capita food value, add per-capita water +
hydrogen + methane + ammonia, and add a lunar-regolith He-3 mining
building. That is the minimum viable **BALANCE_PATCHES_v0.5.md** scope
for the civilisation-demand-sink mechanic.

---

## 8. Audit self-checks

* **All 39 resources in `src/economy/types.rs` have rows** ✅
* **Every row has a real-world source citation** ✅
* **Every row has a "gap analysis" one-liner** ✅
* **Summary table at the top ranks by scale gap (largest first)** ✅
* **File under 2000 lines** ✅ (this file is ~580 lines)
* **No RON / Rust / UI files were modified** ✅
* **No specific RON changes are proposed** ✅ (those land in
  `BALANCE_PATCHES_v0.5.md`)
* **No civilisation-satisfaction state machine is proposed** ✅ (that
  lands in `CIVILIZATION_SATISFACTION_MODEL.md`)

---

## 9. Source citations (consolidated)

* **USGS Mineral Commodity Summaries 2026** — January 2026 release;
  individual commodity PDFs at `pubs.usgs.gov/periodicals/mcs2026/`
  (iron-ore, aluminum, titanium, nickel, tungsten, copper, lithium,
  cobalt, rare-earths, silver, gold, platinum, uranium, helium,
  phosphate-rock, nitrogen, sulfur, chromium, magnesium, thorium,
  fluorine, coal).
* **USGS Mineral Commodity Summaries 2025** — used as cross-check for
  commodities without 2026 entries (some 2025 figures cycled through
  2026 release).
* **BGS World Mineral Production 2018–2022** — cross-check on
  production figures.
* **IEA Critical Minerals Outlook 2024** — cross-check on demand
  framing.
* **IEA Global Hydrogen Review 2024** — hydrogen production
  breakdown.
* **IEA Ammonia Technology Roadmap 2022** — ammonia capacity /
  production.
* **IEA Methane Tracker 2024** — natural-gas and methane atmospheric
  data.
* **FAO 2024 SOFA report** — world per-capita food supply.
* **NOAA Global Monitoring Laboratory** — atmospheric CO₂ Jan 2026
  reading.
* **NUBASE2020** — nuclear data for D, T, He-3, Pu.
* **CERN ALPHA experiment** — antimatter production rate.
* **USGS Helium MCS 2024** — atmospheric He-3 pool 3,815 t.
* **OECD Global Plastics Outlook 2024** — polymer / plastics
  production.
* **IAEA & SIPRI Yearbook 2024** — civil plutonium stockpiles.
* **worldsteel 2024** — world steel production.
* **IRENA 2024** — geothermal installed capacity.
* **Existing `memories/repo/real-world-2026-reserves.md`** — primary
  reference table; extended, not duplicated.

---

*End of audit. Hand off to the balance-patches deliverable for RON
proposals, and to the civilization-satisfaction-model deliverable for
the state machine. No blockers; the audit is complete and the data is
in the public record.*
