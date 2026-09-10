# Resources

Reference for the 39-resource economy in Helios Ascension. All variants live in
[`src/economy/types.rs`](../src/economy/types.rs); this doc mirrors that source
of truth.

Cross-references: [COLONIES.md](COLONIES.md) (construction draws from per-body
stockpile), [ARCHITECTURE.md §EconomyPlugin](../ARCHITECTURE.md) (system layout),
[RESEARCH_MODDING.md](RESEARCH_MODDING.md) (tech-gated unlocks).

---

## Headline

| Metric | Value |
|---|---|
| Total variants | **39** |
| Categories | **10** |
| Source | `src/economy/types.rs::ResourceType` |
| Storage | per-body `LocalStockpile` (Mt) — no system-pool fallback |
| Display | view-scoped `ContextualStockpile` (read-only aggregate) |
| Unit | **Megatonnes (Mt)** = 10⁶ t = 10⁹ kg |
| Calibration | USGS 2024 / worldsteel 2024 / OECD 2024 / WNA 2024 / NMA 2024 / FAO 2024 |

### Category roster (10 groups)

| Group | Count | Subgroup / role |
|---|---|---|
| Volatiles | 4 | beyond the frost line (>2.5 AU) |
| Phosphorus | 1 | hard limit on hydroponics / population |
| Biological | 1 | colony-produced (Food aggregate) |
| Atmospheric Gases | 4 | terraforming feedstock |
| Construction Materials | 9 | inner solar system (<2.5 AU) |
| Fusion Fuel | 3 | He3 / D / T |
| Fissiles | 3 | U / Th / Pu (Pu bred) |
| Precious Metals | 3 | Au / Ag / Pt |
| Strategic Materials | 7 | advanced-tech enablers |
| Exotic Materials | 4 | K2-tier, roadmap-gated |
| **Total** | **39** | — |

---

## 1. Resource catalogue

Columns: **Name** · **Symbol** · **Typical source** · **Mt-unit scale** · **Primary consumer**.

### Volatiles (4)

| Name | Symbol | Typical source | Mt scale | Primary consumer |
|---|---|---|---|---|
| Water | H₂O | ice moons, C-type asteroids, polar caps, comets | 10⁰–10⁹ | LifeSupport, colony maintenance, construction |
| Hydrogen | H₂ | gas giants (atmospheric harvest), industrial SMR | 10¹–10³ | Industry (`ChemicalPlant`), refinery feedstock |
| Ammonia | NH₃ | ice giants, Titan, atmospheric harvest | 10⁰–10² | Fertilizer maintenance (Farm / Greenhouse / Aquaculture) |
| Methane | CH₄ | Titan, gas giants, natural-gas analogue | 10⁰–10⁴ | Polymer industry (`ChemicalPlant`), power |

### Phosphorus (1)

| Name | Symbol | Typical source | Mt scale | Primary consumer |
|---|---|---|---|---|
| Phosphorus | P | C-/D-/P-type asteroids (0.05–0.3% by mass) | 10⁻¹–10⁰ | **Hard limit on hydroponics** — every Farm/Greenhouse/Aquaculture/AgriDome drains P₂O₅ fertilizer; per-capita 18.8 kg/p/yr |

### Biological (1)

| Name | Symbol | Typical source | Mt scale | Primary consumer |
|---|---|---|---|---|
| Food | — | colony-produced (crops + algae + cultured protein aggregate) | 10³–10⁴/yr per colony | Population (`food_consumption_per_capita_mt_per_year = 0.0000011`, FAO 2024) |

### Atmospheric Gases (4)

| Name | Symbol | Typical source | Mt scale | Primary consumer |
|---|---|---|---|---|
| Nitrogen | N₂ | Earth / Titan / Venus atmospheres | 10⁰–10³ | Haber-Bosch input (`ChemicalPlant`), terraforming |
| Oxygen | O₂ | Earth atmosphere, electrolysis from water | 10⁰–10³ | LifeSupport, combustion, steelmaking |
| Carbon Dioxide | CO₂ | Venus / Mars atmospheres, industrial output | 10⁰–10² | Terraforming greenhouse gas, urea/chemicals feedstock |
| Argon | Ar | Earth atmosphere (1.3% by mass) | 10⁻³–10⁰ | Inert shielding, welding, semiconductor |

### Construction Materials (9)

| Name | Symbol | Typical source | Mt scale | Primary consumer |
|---|---|---|---|---|
| Iron | Fe | M-/S-type asteroids, Earth core, Mars crust | 10⁰–10⁶ | Steel (worldsteel 2024: 214.7 kg/p/yr finished × 0.97 Fe = 208 kg Fe/p/yr), hulls |
| Aluminum | Al | bauxite, regolith | 10⁰–10⁴ | Alloys, wiring (USGS 2024: 8.5 kg/p/yr) |
| Titanium | Ti | ilmenite/rutile, lunar maria | 10⁻²–10¹ | D-T fusion vessel, aerospace alloys (industrial-only — per-capita = 0) |
| Silicates | SiO₂ | quarries, regolith | 10¹–10⁵ | Aggregate, glass, ceramics (USGS NMA 2024: 410 kg/p/yr) |
| Nickel | Ni | M-type asteroids, iron meteorites | 10⁻¹–10² | Stainless steel, superalloys (0.001 Mt/yr per build) |
| Tungsten | W | rare-earth mines, M-type asteroids | 10⁻²–10⁰ | Railguns, carbide tooling, fusion magnets (3e-5 Mt/yr per build) |
| Carbon | C | coal, graphite, C-/D-type asteroids | 10⁰–10⁴ | Graphene/nanotube hulls (USGS NMA 2024: 700 kg/p/yr coal) |
| Chromium | Cr | chromite, stainless feed | 10⁻¹–10¹ | Stainless steel, corrosion-resistant alloys |
| Magnesium | Mg | magnesite, dolomite, seawater | 10⁻¹–10¹ | Mg-Al alloys, sacrificial anodes |

### Fusion Fuel (3)

| Name | Symbol | Typical source | Mt scale | Primary consumer |
|---|---|---|---|---|
| Helium-3 | He3 | lunar regolith, gas giants (atmospheric harvest) | 10⁻³–10⁰ | D-³He fusion reactors, antimatter-catalyst drives |
| Deuterium | D | heavy-water extraction (seawater, ice) | 10⁻¹–10² | "Oil of the 22nd century" — D-T and D-D fusion |
| Tritium | T | bred from lithium blankets in `DTFusionReactor` | 10⁻³–10⁻¹ | D-T fusion (short half-life, bred in-loop) |

### Fissiles (3)

| Name | Symbol | Typical source | Mt scale | Primary consumer |
|---|---|---|---|---|
| Uranium | U | U ore (rarite ~3 ppm crustal) | 10⁻³–10⁰ | Fission reactors (WNA 2024: 74 kt/yr world — per-capita = 0) |
| Thorium | Th | monazite, rare-earth byproduct | 10⁻²–10¹ | MSR / breed-blanket reactors |
| Plutonium | Pu | bred from fertile U in `BreederReactor` (0.23 Mt/yr per park) | 10⁻⁴–10⁻² | Fast-spectrum reactors, naval propulsion |

### Precious Metals (3)

| Name | Symbol | Typical source | Mt scale | Primary consumer |
|---|---|---|---|---|
| Gold | Au | placer / lode (cyanidation) | 10⁻⁴–10⁻² | Treasury anchor, electronics |
| Silver | Ag | Pb-Zn byproduct, lode | 10⁻³–10⁻¹ | Electronics, currency |
| Platinum | Pt | M-type asteroids (PGM-rich), Bushveld | 10⁻⁵–10⁻³ | Catalysts, fuel cells, labware |

### Strategic Materials (7)

| Name | Symbol | Typical source | Mt scale | Primary consumer |
|---|---|---|---|---|
| Copper | Cu | porphyry, sedimentary | 10⁻¹–10² | Conductors (USGS NMA 2024: 1.9 kg/p/yr), D-T reactor coils |
| Rare Earths | REO | monazite, bastnäsite, ion-adsorption clays | 10⁻²–10¹ | D-T superconducting magnets (Nb/REBCO), motors |
| Lithium | Li | pegmatite, brine, seawater | 10⁻³–10⁰ | Batteries, fusion-breeding blanket (D-T reactor input) |
| Sulfur | S | elemental / sulfide ores, Frasch | 10⁻²–10¹ | H₂SO₄, fertilizer (USGS 2024: 6 kg/p/yr) |
| Cobalt | Li-Co oxide | sediment-hosted Cu-Co, laterite | 10⁻³–10⁻¹ | Li-ion cathodes, superalloys, turbopumps |
| Fluorine | F | fluorspar | 10⁻²–10⁰ | UF₆ enrichment, semiconductor etch, FLOX oxidiser |
| Polymers | — | manufactured (`ChemicalPlant`) | 10¹–10³ | Plastics, lubricants (OECD 2024: 38 kg/p/yr) |

### Exotic Materials (4) — K2-tier, roadmap-gated

| Name | Symbol | Typical source | Mt scale | Primary consumer |
|---|---|---|---|---|
| Antimatter | p̄ | particle accelerators (locked behind K2 tech) | 10⁻⁶–10⁻³ | Antimatter drives (1 000 000 s Isp) |
| Exotic Matter | Xm | theoretical / Alcubierre-style harvest | — | Warp bubbles, wormholes |
| Metamaterials | Mm | engineered composite (locked) | — | Cloaking, perfect lenses, EM shielding |
| Computronium | Qb | optimised computational substrate (locked) | — | Post-singularity AI / Culture-level minds |

---

## 2. Real-world calibration

All building rates in `assets/data/buildings.ron` are calibrated so that one
in-game building on Earth ≈ 2026 world production for the dominant resource
(operator-binding criterion). Two calibration layers are documented inline in
the RON header.

### `colony_constants` block (`buildings.ron:34-44`)

| Field | Value | Source |
|---|---|---|
| `food_consumption_per_capita_mt_per_year` | 0.0000011 | FAO 2024 SOFA — 1,100 kg/p/yr |
| `base_growth_rate` | 0.009 | Earth 2026 demographic baseline (0.9%/yr) |
| `food_decline_threshold` | 0.95 | Stressed level — feedback from any deficit |
| `food_decline_max_mortality` | 0.03 | Real-world 0.5–3% mortality in moderate food insecurity |

### `per_capita_consumption` block (`buildings.ron:79-130`)

Each value is **Mt/p/yr**. Calibrated so 8.2 B people consume ~70% of world
demand; the remaining ~30% covers industry, maintenance, feedstock, and power.

| Resource | Value (Mt/p/yr) | kg/p/yr | World source |
|---|---|---|---|
| Iron (steel) | 2.13 × 10⁻⁷ | 208 | worldsteel 2024 (214.7 kg × 0.97 Fe) |
| Copper | 1.9 × 10⁻⁹ | 1.9 | USGS NMA 2024 (12 lb US ≈ 5.4 kg, world ~3 kg) |
| Aluminum | 6 × 10⁻⁹ | 6.0 | USGS 2024 (70 Mt / 8.2B = 8.5 kg) |
| Silicates | 4.1 × 10⁻⁷ | 410 | USGS NMA 2024 (16,284 lb US; world ~410 kg) |
| Titanium | 0 | 0 | Industrial-only (TiO₂ pigment / aerospace); was 1.1 kg/p/yr in v3.8.9 |
| Polymers | 3.8 × 10⁻⁸ | 38 | OECD 2024 (450 Mt / 8.2B = 55 kg) |
| Phosphorus | 1.88 × 10⁻⁸ | 18.8 | USGS 2024 (55 Mt P₂O₅ / 8.2B = 6.7 kg × 70% scale) |
| Sulfur | 6 × 10⁻⁹ | 6 | USGS 2024 (70 Mt / 8.2B = 8.5 kg) |
| Nitrogen | 0 | 0 | Fertilizer-N flows through Haber-Bosch chain, not direct per-capita |
| Methane | 2.5 × 10⁻⁷ | 250 | IEA 2026 (4,100 bcm / 8.2B = 500 m³ × 50% consumer share) |
| Uranium | 0 | 0 | Industrial-only nuclear power; was 6.3 g/p/yr in v3.8.9 |
| Carbon (coal) | 7 × 10⁻⁷ | 700 | USGS NMA 2024 (2,414 lb US ≈ 1,095 kg; world ~700 kg) |

### Demand-sized outputs (Mt/yr per build, Earth × 1.0)

| Building | Mt/yr per build | Earth start × count | World target |
|---|---|---|---|
| Farm | 360 | 25 × 360 = 9,000 | FAO 2024 world food |
| GreenhouseComplex | 200 | 10 × 200 = 2,000 | 22% of world food |
| AquacultureComplex | 200 | 10 × 200 = 2,000 | FAO 2024 aquaculture target |
| AgriDome | 4 | 5 × 4 = 20 | closed-env off-world |
| IronMine | 97.15 | 25 × 97.15 × 0.9 = 2,185 | USGS 2024 2,500 Mt |
| AluminumMine | 2.62 | 25 × 2.62 × 0.8 = 52 | USGS 2024 |
| SilicatesMine | 137.3 | — | USGS NMA 2024 |
| CarbonMine | 392.2 | — | USGS NMA 2024 coal |
| AtmosphericProcessor | N 0.667, O 0.5, Ar 0.00333, CO₂ 0.667 | 300 × rates = world demand | USGS / OECD 2024 |
| ChemicalPlant | H₂ 65.6, NH₃ 135, Pol 322 | Earth-start consumption | USGS / OECD 2024 |

---

## 3. Food production chain

Food is the only colony-produced aggregate (crops + algae + cultured protein)
in the economy. Per-capita demand sets the housing-colony bottleneck.

| Building | Output (Mt/yr) | People fed @ 1,100 kg/p/yr | Use site |
|---|---|---|---|
| Farm | 360 | 327 M | open-air, Earth/Mars only |
| GreenhouseComplex | 200 | 182 M | climate-controlled, specialty crops |
| AquacultureComplex | 200 | 182 M | aquatic protein (FAO 2024 target: 1.5 Gt/yr) |
| AgriDome | 4 | 3.6 M | closed-environment off-world (Moon, Mars, asteroids) |

### Demand formula

```
demand_mt_per_year = population × food_consumption_per_capita_mt_per_year
                   = population × 0.0000011           (FAO 2024)
```

### Growth factor (v3.7.1, steeper curve)

`ratio = supply / demand`:

| ratio | factor | outcome |
|---|---|---|
| ≥ 1.0 | 1.0 | nominal growth (`base_growth_rate = 0.009`) |
| 0.95 | 0.85 | Stressed (warning) — 5% deficit → 15% slowdown |
| 0.85 | 0.59 | Crisis — 41% slowdown |
| 0.70 | 0.25 | Emergency — 75% slowdown |
| 0.50 | 0.00 | Famine — 0% growth, mortality ramp to `food_decline_max_mortality` |

Reference: IPC-level feedback (Lanz 2016, Ó Gráda 2009, Sen 1981). Threshold
0.95 (was 0.70) so the player sees feedback from any deficit.

---

## 4. Mining & extraction

### Per-resource dedicated mines (in `buildings.ron`)

Every mineable resource has a paired `*Mine` + `Auto*Mine` entry. Auto variants
are crewless and scaled by `assets/data/survey/mining_efficiency.ron` (v0.5.0
survey rework).

| Resource | Manual | Auto |
|---|---|---|
| Iron | IronMine | AutoIronMine |
| Aluminum | AluminumMine | AutoAluminumMine |
| Titanium | TitaniumMine | AutoTitaniumMine |
| Silicates | SilicatesMine | AutoSilicatesMine |
| Nickel | NickelMine | AutoNickelMine |
| Tungsten | TungstenMine | AutoTungstenMine |
| Carbon | CarbonMine | AutoCarbonMine |
| Chromium | ChromiumMine | AutoChromiumMine |
| Magnesium | MagnesiumMine | AutoMagnesiumMine |
| Gold | GoldMine | AutoGoldMine |
| Silver | SilverMine | AutoSilverMine |
| Platinum | PlatinumMine | AutoPlatinumMine |
| Copper | CopperMine | AutoCopperMine |
| Rare Earths | RareEarthsMine | AutoRareEarthsMine |
| Lithium | LithiumMine | AutoLithiumMine |
| Sulfur | SulfurMine | AutoSulfurMine |
| Cobalt | CobaltMine | AutoCobaltMine |
| Fluorine | FluorineMine | AutoFluorineMine |
| Uranium | UraniumMine | AutoUraniumMine |
| Thorium | ThoriumMine | AutoThoriumMine |
| Helium-3 | He3Mine (Moon / GasGiant / Asteroid) | AutoHe3Mine |
| Deuterium | DeuteriumExtractor | AutoDeuteriumExtractor |

### Output formula

```
output_mt_per_year = base_yield × deposit.accessibility × yield_mult
```

`accessibility` is set by the survey rework's deposit tiers
(`assets/data/survey/tiers.ron`); `yield_mult` aggregates module bonuses,
workforce, and maintenance headroom. Survey progress in
`assets/data/survey/dimensions.ron` unlocks higher yield multipliers.

### Deposit tier gates

Mining availability follows the planetary-body tier model
(`src/economy/generation.rs`):

| Tier | Depth | Tech gate | Share of total |
|---|---|---|---|
| `proven_crustal` | 0–5 km | basic | 0.1–5% |
| `deep_deposits` | 5–100 km | advanced drilling | 5–20% |
| `planetary_bulk` | 100 km–core | deep mantle mining | 75–95% |

Asteroid spectral classes (C / S / M / V / D / P) set which resources are
richly present. Not every body of a given class has every resource — the class
sets probabilities and concentration ranges; see
[`docs/MODDING.md`](MODDING.md) for the full table.

---

## 5. Fusion & Fissile fuels

### Helium-3

- Source: **lunar regolith** (solar-wind implantation) and **gas-giant atmospheric harvest** (`AutoHe3Mine`).
- Body whitelist: Moon, GasGiant, Asteroid.
- Real-world estimate: ~400 kg identified lunar reserves; theoretical lunar regolith endowment ~1 Mt.

### Deuterium

- Source: **heavy-water extraction** from seawater or ice (`DeuteriumExtractor`).
- Real-world: ~156 D per 10⁶ H (VSMOW); effectively unbounded in oceans.

### Tritium

- Source: **bred from lithium blankets** in `DTFusionReactor` (Li + n → T + He).
- Not naturally stockpiled — 12.32 yr half-life.
- World production: ~415 g/yr natural cosmic-ray spallation; reactor-bred is the only scalable path.

### Uranium / Thorium

- Source: U ore (~3 ppm crustal, USGS 2024 reserves ~8 Gt); Th monazite (rare-earth byproduct).
- Buildings: `UraniumMine` / `AutoUraniumMine`, `ThoriumMine` / `AutoThoriumMine`.
- Per-capita consumption = 0 — pure industrial nuclear-power input.

### Plutonium

- Source: **bred from fertile U** in `BreederReactor` (700 GW power + 0.23 Mt/yr Pu per park).
- `BreederReactor` is the only Pu producer; maintains small Pu bleed as part of its operating cost.

---

## 6. Precious & Strategic

### Economic role

Precious metals (Au / Ag / Pt) anchor the in-game **treasury** and high-value
trade goods; they don't run heavy industry but provide the currency
backing and catalyst feedstocks. Real-world 2024 reference:

| Resource | Identified reserves | 2024 world production |
|---|---|---|
| Gold | 100 kt | 3,600 t/yr |
| Silver | 700 kt | 25 kt/yr |
| Platinum | 21 kt | 160 t/yr |

### Strategic supply scarcity

Strategic materials gate **advanced tech unlocks** (D-T reactors, fusion magnets,
batteries, semiconductor etch, superalloys). The cost of running short of any
of these is high — D-T tokamak construction fails without Nb/REBCO (RareEarths),
Li-ion grid storage stalls without Lithium / Cobalt, and breeder reactor
maintenance demands Fluorine + Plutonium feedback.

| Material | Why it matters |
|---|---|
| Copper | Conductors, D-T reactor coils |
| Rare Earths | D-T superconducting magnets (Nb/REBCO), motors, wind turbines |
| Lithium | Batteries, fusion-breeding blanket |
| Sulfur | H₂SO₄, fertilizer |
| Cobalt | Li-ion cathodes, superalloys, turbopumps |
| Fluorine | UF₆ enrichment, semiconductor etch, FLOX oxidiser |
| Polymers | Plastics, lubricants, chemical feedstocks |

---

## 7. Exotic materials

All four exotic resources are **K2-tier** (roadmap-gated, post-fusion). They do
not appear in `is_mineable()` and have no natural deposit generation — they
require dedicated producer buildings or narrative events.

| Material | Production path | Use |
|---|---|---|
| Antimatter | particle accelerators (K2 factory) | antimatter drives (1 000 000 s Isp) |
| ExoticMatter | theoretical / harvest | warp bubbles, wormholes |
| Metamaterials | engineered composite | cloaking, perfect lenses, EM shielding |
| Computronium | post-singularity automation | Culture-level AI minds |

Until each is unlocked, the corresponding column in the resource bar shows `—`.

---

## 8. Display vs physical storage

The economy has **two** storage layers; only one is physical.

### `LocalStockpile` (physical, per-body)

- A `Component` on every body that produces, stores, or consumes resources.
- `HashMap<ResourceType, f64>` in **Mt**.
- Production (mining, atmospheric harvesting, food) **deposits** here.
- Consumption (maintenance, food, construction materials) **deducts** here.
- **Construction draws only from the destination body's `LocalStockpile`** —
  no system-pool fallback. When local materials are short, the construction
  system publishes a `ResourceRequest` and the building waits for delivery.
  See [`docs/COLONIES.md`](COLONIES.md) and `src/colony/systems.rs`.

### `ContextualStockpile` (view-scoped, display-only)

- A `Resource` aggregated from `LocalStockpile`s for the **player UI**.
- `update_contextual_stockpile` reads `ViewMode` and `CurrentStarSystem`:
  - **System view** → sum every body in the active star system
  - **Starmap view** → sum every body across all systems
- The label switches between `"Sol System"` and `"All Systems"` accordingly.
- **Construction does not read this** — display-only. Defined in
  `src/economy/budget.rs`.

### Top resource bar

Reads `ContextualStockpile` for the active view; UI surfaces the running total
per resource (`src/ui/resources_bar.rs`). Construction queues per-body in the
dossier panel (`src/ui/construction/`) and the queue badge surfaces the
"⏳ Awaiting resources" / "⏳ Waiting for freighter" state when local materials
are insufficient.

---

## See also

- [`src/economy/types.rs`](../src/economy/types.rs) — `ResourceType` enum, all
  category predicates (`is_volatile`, `is_biological`, `is_atmospheric_gas`,
  `is_construction`, `is_fusion_fuel`, `is_fissile`, `is_precious_metal`,
  `is_strategic`, `is_exotic`, `is_mineable`).
- `assets/data/buildings.ron` — building IDs, costs, outputs, `colony_constants`,
  `per_capita_consumption`.
- `assets/data/survey/` — `dimensions`, `instruments`, `anomalies`,
  `mining_efficiency`, `missions`, `recovery_missions`, `tiers` (v0.5.0 survey rework).
- [`docs/MODDING.md`](MODDING.md) — texture, body, and asteroid-spectral-class authoring.
- [`docs/COLONIES.md`](COLONIES.md) — colony founding, construction queue, life-support.
- [`docs/ARCHITECTURE.md` §EconomyPlugin](../ARCHITECTURE.md) — system layout and data flow.
- [`docs/RESEARCH_MODDING.md`](RESEARCH_MODDING.md) — tech-gated unlocks for mines,
  fusion reactors, breeder reactors, and exotic factories.