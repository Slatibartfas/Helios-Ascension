# Colonies, Buildings & Resources

Player-facing reference for founding colonies, constructing buildings, and supplying them with physical resources. Companion to `ARCHITECTURE.md` §Colony Management and `docs/design/LOGISTICS_NETWORK.md`.

## Headline Counts (v0.5.2)

| | |
|---|---|
| Building types | **96** across **9** categories (`BuildingCategory` enum) |
| Resources | **39** types (`ResourceType` enum, `src/economy/types.rs`) |
| Celestial bodies | **713** (Sol system) + **60** nearby star systems in `assets/data/nearest_stars_raw.json` |
| Sol-system body breakdown | 1 Star · 4 Planet · 4 GasGiant · 2 Ring · 55 DwarfPlanet · 147 Moon · 450 Asteroid · 50 Comet |
| Earth seed population | 8.2 B |
| Per-capita food | **0.0000011 Mt/person/yr** (1,100 kg, FAO 2024 SOFA) |

## 1. How Colonies Work

A colonised body carries a `Colony` component (population, building counts, development tier) and a `LocalStockpile` component (per-body resource inventory in Mt). **Resources are physical** — there is no system-wide pool. Construction, maintenance, food production, and population consumption all draw from the body's *own* `LocalStockpile`. Other bodies' stockpiles can only be reached by transporting material via Freighter fleet (player or AI shipping company).

`Colony` records development on a 4-tier scale (`ColonyTier`):

| Tier | Yield × | Notes |
|---|---|---|
| Outpost | 0.10 | Default for newly founded colonies |
| Settlement | 0.40 | Player-driven upgrade |
| City | 0.70 | Player-driven upgrade |
| Civilisation | 1.00 | Earth starts here |

Yield × scales production, maintenance, and population growth. It does **not** scale per-capita food consumption — a population of N always eats the same amount of food.

The `LocalStockpile` draw model replaced the v0.3 system-pool fallback in v0.4.x (GRA-31 PR-A). Construction on body X with insufficient local stock publishes `ResourceRequest`s tagged `RequestPriority::Construction` and waits for delivery before advancing; the project carries `awaiting_resources = true` and the queue shows an `awaiting_resources` badge.

## 2. Building Reference

96 buildings across 9 categories. All values per-build unless noted. **Build Points (BP)** measures construction cost; colonies produce BP from `Factory` buildings (10 BP/yr each + 1 BP/yr base). **Workforce** must fit within population × `available_workforce_fraction` (0.40). **Power demand** is in MW.

Maintenance resources per building = **4–6 distinct** items (`MAINTENANCE_AUDIT_MIN = 4`, `MAINTENANCE_AUDIT_MAX = 6` in `src/colony/data.rs`, GRA-22a audit). Disabling any one of them must noticeably weaken or shut down the building. **`money_cost_mc_per_year`** is an additional financial operating cost (MC = mega-credits/yr) distinct from the Mt-denominated resource draw.

### Infrastructure (9)

| Building | Effect | BP | Workers | First buildable on | Notes |
|---|---|---:|---:|---|---|
| `LifeSupport` | 50 Mt O₂/yr, 5 Mt N₂/yr harvest, 30 Mt CO₂/yr scrubbed | 500 | 2,000 | Any | Required on non-breathable bodies |
| `Housing` (Complex) | **+25 M** residents | 200 | 500 | Habitable worlds | 116 Mt BoM, Si 49 %; workhorse metropolitan tier |
| `HabitatDome` | **+5 M** residents (v3.10) | 800 | 1,000 | Any | Pressurised; 350 Mt BoM, Fe 45 % |
| `UndergroundHabitat` | **+8 M** residents (v3.10) | 1,200 | 1,500 | Vacuum/hostile | Buried, regolith-shielded; 500 Mt BoM |
| `HabitatTent` | +1,000 residents (v3.2 starter tier 1) | 50 | 5 | Any | First buildable on a fresh outpost |
| `HabitatModule` | +10,000 residents (v3.2 starter tier 2) | 200 | 50 | Any | Second-tier for growing colonies |
| `WaterProcessor` | 16 Mt water/yr | varies | varies | Non-breathable bodies only | Body-restricted |
| `WaterTreatmentPlant` | +2 % population growth | 400 | 500 | Any | |
| `DesalinationPlant` | +1 % population growth | 600 | 400 | Any | Tech-gated: `desalination` |

**Housing gradient** (v3.10, GRA-22c Phase 4A): Tent 1k → Module 10k → Dome 5M / Underground 8M → Housing Complex 25M. The 10k → 5M step is 500× (manageable); the 5M → 25M step is 5× once city-scale infrastructure exists.

### Mining (49)

v0.5.2 split out per-resource mines (consolidated from the legacy generic Mine / Refinery / DeepDrill / LaserDrill / StripMine / HydrocarbonExtractor). Each mine produces one resource at `base_yield × deposit.accessibility × yield_multiplier`. The `line = "Mine"` field groups them for tier-based tech upgrades. **Body-restricted mines** are noted. **AutoMines** are orbital rigs calibrated at ~1/10 of surface yields and body-restricted to `[Asteroid, Moon, GasGiant]`; they require `asteroid_mining` tech.

**Base mines** (24 — surface mining; build on any body with the resource):

| Resource | Yield Mt/yr | Tech gate |
|---|---:|---|
| Iron (`IronMine`) | 97.15 | — |
| Aluminum (`AluminumMine`) | 2.62 | — |
| Titanium (`TitaniumMine`) | 0.0196 | — |
| Silicates (`SilicatesMine`) | 137.3 | — |
| Nickel (`NickelMine`) | 0.0717 | — |
| Tungsten (`TungstenMine`) | 0.004 | — |
| Carbon (`CarbonMine`) | 392.2 | — |
| Chromium (`ChromiumMine`) | 0.735 | — |
| Magnesium (`MagnesiumMine`) | 0.0233 | — |
| Gold (`GoldMine`) | 0.000204 | — |
| Silver (`SilverMine`) | 0.00204 | — |
| Platinum (`PlatinumMine`) | 0.0000306 | — |
| Copper (`CopperMine`) | 2.12 | — |
| RareEarths (`RareEarthsMine`) | 0.0168 | — |
| Lithium (`LithiumMine`) | 0.0119 | — |
| Sulfur (`SulfurMine`) | 4.78 | — |
| Phosphorus (`PhosphorusMine`) | 12.63 | — |
| Cobalt (`CobaltMine`) | 0.00758 | — |
| Fluorine (`FluorineMine`) | 0.0778 | — |
| Uranium (`UraniumMine`) | 0.0101 | — |
| Thorium (`ThoriumMine`) | 0.0000914 | — |
| Methane (`MethaneExtractor`) | 438.8 | — |
| Deuterium (`DeuteriumExtractor`) | 0.00088 | — |
| Helium-3 (`He3Mine`) | 0.5 | `lunar_colony`; bodies `[Moon, GasGiant, Asteroid]` |

**AutoMines** (25 — orbital/asteroidic rigs, body-restricted `[Asteroid, Moon, GasGiant]`, require `asteroid_mining`):

| Resource | Yield Mt/yr |
|---|---:|
| Iron (`AutoIronMine`) | 12.0 |
| Aluminum (`AutoAluminumMine`) | 0.5 |
| Titanium (`AutoTitaniumMine`) | 0.002 |
| Silicates (`AutoSilicatesMine`) | 70.0 |
| Nickel (`AutoNickelMine`) | 0.02 |
| Tungsten (`AutoTungstenMine`) | 0.0005 |
| Carbon (`AutoCarbonMine`) | 35.0 |
| Chromium (`AutoChromiumMine`) | 0.2 |
| Magnesium (`AutoMagnesiumMine`) | 0.007 |
| Gold (`AutoGoldMine`) | 0.00001 |
| Silver (`AutoSilverMine`) | 0.0001 |
| Platinum (`AutoPlatinumMine`) | 0.000001 |
| Copper (`AutoCopperMine`) | 0.15 |
| RareEarths (`AutoRareEarthsMine`) | 0.0025 |
| Lithium (`AutoLithiumMine`) | 0.0012 |
| Sulfur (`AutoSulfurMine`) | 0.5 |
| Phosphorus (`AutoPhosphorusMine`) | 0.0003 |
| Cobalt (`AutoCobaltMine`) | 0.0015 |
| Fluorine (`AutoFluorineMine`) | 0.02 |
| Uranium (`AutoUraniumMine`) | 0.0003 |
| Thorium (`AutoThoriumMine`) | 0.00007 |
| Methane (`AutoMethaneExtractor`) | 27.0 |
| Deuterium (`AutoDeuteriumExtractor`) | 0.05 |
| Helium-3 (`AutoHe3Mine`) | 0.05 |
| Water (`AutoWaterProcessor`) | 1.6 |

### Industry (5)

| Building | Effect | BP | Workers | Tech gate |
|---|---|---:|---:|---|
| `Factory` | +10 BP/yr construction; 100 MC/yr wealth | 1,000 | 12,000 | — |
| `AtmosphericProcessor` | 0.398 N₂ + 0.442 O₂ + 0.0032 Ar + 0.51 CO₂ Mt/yr | 500 | 3,000 | — |
| `ChemicalPlant` | H₂ 0.0937 + NH₃ 0.1929 + Polymers 0.4597 Mt/yr | 800 | 4,000 | — |
| `SemiconductorFab` | +8 % research, +5 % engineering | 5,000 | 5,000 | `semiconductor_manufacturing` |
| `PharmaceuticalPlant` | +3 % population growth | 800 | 4,000 | — |

### Logistics (4)

Capacity per build (`LogisticsCapacity` modifier; sum vs. industrial-buildings demand × 1,000 → efficiency):

| Building | Capacity | BP | Workers |
|---|---:|---:|---:|
| `MassDriver` | 5,000 t/yr | 2,000 | 2,500 |
| `OrbitalLift` | 20,000 t/yr | 5,000 | 6,000 |
| `CargoTerminal` | 2,000 t/yr | 300 | 3,000 |
| `Warehouse` | +5 % global stockpile capacity | 300 | 1,000 |

Earth has effectively infinite logistics capacity (1 × 10⁹).

### Power Generation (12)

Output in GW. Power deficit reduces output of all powered buildings.

| Building | Output (GW) | Fuel | BP | Workers | Tech gate |
|---|---:|---|---:|---:|---|
| `SolarPower` | 5 | — | 200 | 500 | — |
| `WindFarm` | 3 | — | 300 | 200 | — |
| `HydroelectricDam` | 15 | — | 2,500 | 1,000 | — |
| `GeothermalPlant` | 18 | — | 1,800 | 800 | `geothermal_energy` |
| `CoalPowerPlant` | 10 | Coal | 800 | 2,000 | — |
| `NaturalGasPlant` | 12 | Gas | 600 | 1,500 | — |
| `FissionReactor` | 20 | Uranium | 1,500 | 4,000 | — |
| `FusionReactor` | 40 | He-3 + D | 5,000 | 8,000 | `fusion_power` |
| `DTFusionReactor` | 50 | D + T (+ Li blanket) | 6,000 | 9,000 | `fusion_power` |
| `DHe3FusionReactor` | 45 | D + He-3 | 7,000 | 9,500 | `helium3_fusion` |
| `ThoriumReactor` | 24 | Thorium | 1,800 | 4,500 | `molten_salt_fission` |
| `BreederReactor` | 22 | U (+ Pu output) | 2,600 | 5,000 | `breeder_reactors` |

### Population & Growth (5)

Food buildings produce `FoodProduction` (Mt/yr); consumption = population × 0.0000011 Mt/p/yr.

| Building | Food Mt/yr | Feeds (approx) | BP | Workers | Notes |
|---|---:|---|---:|---:|---|
| `Farm` | 360 | 327 M | 100 | 1,000 | Open-air; habitable worlds only |
| `Greenhouse` (Complex) | 200 | 182 M | 400 | 2,000 | Closed environment |
| `AquacultureFacility` (Complex) | 200 | 182 M | 500 | 1,500 | Planetary aquatic protein |
| `AgriDome` | 4 | 3.6 M | 600 | 4,000 | Closed-env off-world; needs `Hydroponics` tech |
| `MedicalCenter` | +0.03 % growth each, cap 0.9 % | — | 800 | 6,000 | — |

### Research & Engineering (5)

| Building | Effect | BP | Workers | Tech gate |
|---|---|---:|---:|---|
| `ResearchLab` | +5 % research speed | 1,000 | 8,000 | — |
| `EngineeringBay` | +5 % engineering speed | 1,200 | 10,000 | — |
| `AiCluster` | +15 % research, +10 % engineering | 4,000 | 2,000 | `neural_networks` |
| `DataCenter` | +10 % research, +8 % engineering | 2,000 | 1,000 | — |
| `SemiconductorFab` | See Industry | — | — | — |

### Financial & Commerce (2)

`TradePort` was **removed** in v3.10 (GRA-22c Phase 4C-2); the enum variant no longer exists. Saves carrying TradePort counts drop them as unknown enum. Trade revenue now flows through `CommercialHub` + `FinancialCenter` + future `LaunchSite` transfer fees.

| Building | Effect | BP | Workers |
|---|---|---:|---:|
| `CommercialHub` | +wealth from trade | 500 | 8,000 |
| `FinancialCenter` | +wealth from banking | 1,500 | 10,000 |

### Military & Shipbuilding (5)

`SpacePort` enum variant is intentionally **orphaned** in v3.10 (GRA-22c Phase 4C-2). The RON entry was renamed `ControlCenter`. Do **not** use `SpacePort` as a building id in saves or new RON.

| Building | Effect | BP | Workers | Tech gate |
|---|---|---:|---:|---|
| `ControlCenter` | **+1 FleetCapacity per build** (max concurrent fleets hosted) | 4,000 | 20,000 | — |
| `Shipyard` | Enables ship construction; +10 % ship efficiency; −10 % build costs | 10,000 | 80,000 | `orbital_construction` |
| `LaunchSite` | Surface-to-orbit access | 2,000 | 12,000 | — |
| `MissileSilo` | Planetary anti-orbital defence | 3,000 | 5,000 | `missile_systems` |
| `GroundDefenseBattery` | Anti-orbital / anti-missile defence | 2,500 | 3,000 | — |

### Survey (1)

| Building | Effect | BP | Workers | Notes |
|---|---|---:|---:|---|
| `OrbitalSurveyStation` | Continuous orbital survey of host body; mining yield bonus to local mines | — | — | v0.5.0 (GRA-83 PR-E); per-body, no transfer |

## 3. Resources (39)

Grouped by `ResourceType::is_*` predicate in `src/economy/types.rs`.

### Volatiles (5)

Beyond the frost line (> 2.5 AU). Phase depends on body temperature and pressure (see `determine_resource_phase`).

| Resource | Use | Typical source |
|---|---|---|
| Water | Habitat life-support, hydroponics, rocket propellant | Icy moons, carbonaceous asteroids, atmospheric condensers |
| Hydrogen | SMR (methane reformer), rocket propellant, ammonia synthesis | Gas-giant atmospheres, icy regolith |
| Ammonia | Fertilizer for Farm / Greenhouse / Aquaculture / AgriDome | Haber-Bosch synthesis (ChemicalPlant) |
| Methane | Polymer feedstock (methane cracker), fuel | Gas giants, Titan lakes, clathrates |
| Phosphorus | Hard limit on hydroponics / population growth | Phosphate rock (PhosphorusMine) |

### Biological (1)

| Resource | Use | Source |
|---|---|---|
| Food | Per-capita consumption; growth throttled if ratio < 0.95 | Farm / Greenhouse / Aquaculture / AgriDome |

### Atmospheric Gases (4)

| Resource | Use | Source |
|---|---|---|
| Nitrogen | Haber-Bosch input (via AmmoniaSynthesis); pressurisation | AtmosphericProcessor; LifeSupport harvest |
| Oxygen | Life-support draw (0.0001 Mt/p/yr on non-breathable worlds) | AtmosphericProcessor; LifeSupport production |
| CarbonDioxide | Urea / greenhouse enrichment / industrial chemistry | AtmosphericProcessor; LifeSupport scrubber |
| Argon | Welding shield gas, semiconductor fab | AtmosphericProcessor |

### Construction Materials (9)

Inner solar system (< 2.5 AU); mined by base `*Mine` buildings or surface ore.

| Resource | Use | BoM share examples |
|---|---|---|
| Iron | Hull steel, structures | 17–500 Mt per building |
| Aluminum | Lightweight structures, propellant tanks | 1.5–333 Mt per building |
| Titanium | Pressure vessels, aerospace alloys, medical implants | 2–167 Mt per building |
| Silicates | Aggregate, dome glass, insulation | 1.5–130 Mt per building |
| Nickel | Stainless steel, superalloys (incl. megafactory maintenance) | 0.5–70 Mt per building |
| Tungsten | Kinetic weapons, drill bits, cutting tools | 0.5–4 Mt per building |
| Carbon | Graphene/nanotubes, composite reinforcement | Coal / graphite |
| Chromium | Stainless steel, corrosion-resistant alloys | Chromite ore |
| Magnesium | Lightweight Mg-Al alloys, sacrificial anodes | Magnesite / dolomite / seawater |

### Fusion Fuel (3)

| Resource | Use | Source |
|---|---|---|
| Helium-3 | D-He3 fusion fuel | Solar-wind-implanted regolith; primordial gas-giant atmospheres |
| Deuterium | Easier fusion than He-3; "oil of the 22nd century" | Seawater / ice (DeuteriumExtractor) |
| Tritium | Bred from Li blankets inside `DTFusionReactor` | D-T reactor breeding (not natural) |

### Fissiles (3)

| Resource | Use | Source |
|---|---|---|
| Uranium | Fission reactor fuel | UraniumMine (U₃O₈ ore) |
| Thorium | Molten-salt reactor fuel | ThoriumMine (monazite) |
| Plutonium | Manufactured from fertile U in BreederReactor | Reactor output |

### Precious Metals (3)

| Resource | Use | Source |
|---|---|---|
| Gold | Electronics, currency reserve | Placer / lode extraction (cyanidation) |
| Silver | Solar panels, antibacterial, photography | Lead-zinc byproduct |
| Platinum | Fuel cells, catalysts, labware | Layered intrusions (Bushveld / Norilsk analog) |

### Strategic Materials (7)

| Resource | Use | Source |
|---|---|---|
| Copper | Wiring, motors, electromagnets | Chalcopyrite / porphyry |
| RareEarths | Motors, magnets, electronics | Bastnäsite / monazite |
| Lithium | Battery tech, fusion reactor maintenance | Spodumene / brine |
| Sulfur | Sulfuric acid, battery electrolytes | Frasch / pyrite roasting |
| Cobalt | Li-Co-oxide cathodes, superalloys | Cobalt ore |
| Fluorine | FLOX oxidiser, UF₆ enrichment, semiconductor etching | Fluorite (CaF₂) |
| Polymers | Manufactured plastics, lubricants | Methane cracker (ChemicalPlant output) |

### Exotic Materials (4)

Late-game; engineered, not mined.

| Resource | Use |
|---|---|
| Antimatter | Antimatter drive fuel (1,000,000 s Isp) — particle accelerators |
| ExoticMatter | Negative-energy-density matter for warp bubbles / wormholes |
| Metamaterials | Engineered optical/EM composites — cloaking, perfect lenses, advanced shielding |
| Computronium | Optimised computational substrate — post-singularity AI automation |

## 4. Population & Growth

Population grows at `base_growth_rate × modifiers × yield_multiplier` per year. `base_growth_rate = 0.009` (0.9 %/yr, Earth 2026 demographic baseline) lives in the `colony_constants` header of `buildings.ron`.

| Modifier | Effect |
|---|---|
| Housing utilisation | At 100 % full → growth × 0.20 (`housing_utilization_penalty = 0.8`); empty → ×1.0 |
| Food adequacy | `food_ratio = production / consumption`. v3.7.1 curve: `factor = (2·ratio − 1)^1.5` for ratio ∈ [0.5, 1.0], 0 below 0.5, 1 above 1.0. Power 1.5 makes the curve pressure-early: 0.95 → 0.85, 0.85 → 0.59, 0.70 → 0.25, 0.50 → 0.00 |
| Medical centers | +0.03 % each, capped at +0.9 % (`medical_growth_per_center`, `max_medical_growth_bonus`) |
| Logistics efficiency | Penalty if `demand > capacity` (research ≥ 50 % floor) |
| Ocean / liquid water | `OceanProperties::habitability_modifier()` bonus on bodies with `OceanType::Water` |

**Population is hard-capped** by housing capacity: `population ≤ housing_capacity`. Colonies with `housing == 0` are uncapped (gives the player time to build starter housing on fresh outposts).

**Housing tiers** (per-build resident capacity):

| Tier | Building | Capacity | v3.2 starter? |
|---|---|---:|---|
| 1 | `HabitatTent` | 1,000 | Yes — first buildable |
| 2 | `HabitatModule` | 10,000 | Yes — second tier |
| 3 (breathable) | `HabitatDome` | 5,000,000 | No — small dome, post-starter |
| 3 (airless) | `UndergroundHabitat` | 8,000,000 | No — buried habitat |
| 4 (workhorse) | `Housing` (Complex) | 25,000,000 | No — metropolitan tier |

Earth seed 8.2 B → ~328 Housing Complexes (manageable-count band per GRA-22a operator bar: one colony building ≈ 1/300 of world population).

## 5. Construction Pipeline

1. Open the **Construction** menu (native Bevy UI, v0.5.2 — `src/ui/construction/` directory: `mod.rs`, `state.rs`, `data.rs`, `cards.rs`, `mining.rs`, `queue.rs`, `overview.rs`, `buildings.rs`, `demolish.rs`, `dropdown.rs`, `tooltip.rs`, `scrollbar.rs`, `disabled.rs`, `setup.rs`, `markers.rs`). The legacy `src/ui/construction_panel.rs` and canary-era `src/ui/construction.rs` no longer exist.
2. Pick the target colony from the colony dropdown.
3. Browse by category. Each card shows `need / available` for cost resources.
4. Click **Queue** (or **Queue ×N** for batch build multipliers).
5. `process_construction_actions` (`src/colony/systems.rs`) drains `LocalStockpile` for affordable portions. If the body cannot pay the full cost in full, the project is spawned with `awaiting_resources = true`, `ResourceRequest`s are filed at `RequestPriority::Construction`, and the project accumulates zero BP until every linked request is delivered.
6. `advance_construction` ticks each project in queue order: `1 + factories × 10` BP/yr, oldest first.
7. On completion, `colony.add_building(building_type)` fires a `ConstructionEvent::Completed` (consumed by the notifications bridge, GRA-137).

**Queue badges**: `awaiting_resources` (red, blocks progress), build-time progress bar, per-tick BP delivery.

**Tier replacement** (GRA-22c plan §4.6): when a player queues a building whose RON `replaces` field names a tier-(N-1) predecessor and the colony has at least one of it, the predecessor count drops by one *before* the project is spawned. The new building still pays its own `resource_costs` (no refund).

**Direct inventory edits** (v0.5.2 Mining tab): ±N on a mine via the Mining card's [+/–] buttons — bypasses BP / build time and applies immediately. Emits a single `ConstructionEvent::Completed` per batch and marks the body dirty (`DirtyReason::Body`) so the v2 save path picks up the new production rate.

**Per-building money cost** (`money_cost_mc_per_year` in `buildings.ron`, v3.7.1+): the financial operating cost (staff, capital, maintenance contracts) in MC/yr. Defaults to 0 with a 5 %-of-build-cost fallback so old RON entries remain valid. Distinct from the Mt-denominated resource draw.

## 6. Maintenance & Operating Costs

Every building has **4–6 distinct** `maintenance_resources` (GRA-22a audit, `MAINTENANCE_AUDIT_MIN..=MAINTENANCE_AUDIT_MAX = 4..=6` in `src/colony/data.rs`). Duplicate entries count as one. Buildings outside the range fail at load time. `audit_buildings()` in the same module returns the violation list; `tests/buildings_cost_audit.rs` enforces the rule.

`deduct_maintenance_resources` ticks each building's Mt draw proportionally per simulation year, deducted from the body's `LocalStockpile`. If the local stockpile runs out, the global budget is used as fallback; if that is also empty, buildings still operate (no hard shutdown — the audit guarantee is the design pressure: disabling any one of the 4–6 resources must noticeably weaken or shut down the building).

`money_cost_mc_per_year` is summed into `operating_cost_per_year` and aggregated into `GlobalBudget::expenses_per_year` by `update_treasury`. Wealth generation (`Factory`'s 100 MC/yr, etc.) is the income side.

## 7. Outpost Founding

`EstablishOutpostRequest` is pushed from the dossier panel by clicking **🏗 Establish Outpost**. The processing system (`src/colony/systems.rs::process_construction_actions`) inserts the `Colony`, `LocalStockpile`, `MinimumStockpile` (Food 500 / Water 100 defaults), `Population`, and `ColonyEnvironmentCosts` components, then queues the v3.9 starter package:

| Building | Quantity | Cost | Notes |
|---|---|---|---|
| `HabitatTent` | ×1 | 50 BP | 1,000 residents |
| `HabitatModule` | ×1 | 200 BP | 10,000 residents |
| `Farm` | ×1 | 100 BP | 360 Mt/yr food (breathable bodies only) |

Total 350 BP ≈ 11 sim days at default 12,000 BP/yr. (Old package was 5,200 BP / ~5 months — replaced in GRA-22c Phase 3.3 so the queue clears quickly and the player drives power / life-support / expansion.)

**Hard blocks** (button is hidden, red ⛔ shown):

| Condition | Reason |
|---|---|
| Body is a Gas Giant | No solid surface — outpost impossible |
| Surface gravity > 3 g | Exceeds human physiological limits (`heavy_gravity_limit_exceeded`) |

**Amber warning** (button still works): `colony_cost_score > 7.0/10`. The dossier surfaces the score inline ("⚠ Extreme environment — significant life-support required").

**Ongoing environmental costs** (per person per year, attached as `ColonyEnvironmentCosts`):

| Resource | Rate | When |
|---|---|---|
| Water | 0.00005 Mt/p/yr | Always (recycling losses) |
| Oxygen | 0.0001 Mt/p/yr | Non-breathable atmospheres (`needs_oxygen = true`) |

**Resources must be transported.** All starter-building materials (Fe, Si, etc.) must be in the new colony's `LocalStockpile` before construction can advance. With zero starting stock, the system fires `ResourceRequest`s on first tick; private shipping companies or player Freighter fleets fulfil them.

## 8. Orbital Access (Launch Capacity)

Every surface-to-orbit lift (a ship launching from a colony, a probe departing the surface, cargo loaded into an orbiting freighter, or colonists leaving a body) draws from a **body-local launch capacity stockpile**. The stockpile accumulates at a rate set by the body's launch facilities and caps at the sum of their storage modifiers. Capacity is **deducted exactly once at the surface/orbit boundary** — orbital construction, orbital shipyards, orbit-to-orbit transfers, and payloads released from an already-orbiting carrier do not charge capacity.

### Contributors

Any building can contribute by declaring two generic `buildings.ron` modifiers:

| Modifier | Meaning |
|---|---|
| `LaunchCapacityProduction` | Tonnes per simulated year added per building. |
| `LaunchCapacityMax` | Tonnes of stored launch mass per building. |

The shipped facility roster is `LaunchSite` (100 t/yr, 5 kt cap), `MassDriver` (2 kt/yr, 20 kt cap), and `OrbitalLift` (10 kt/yr, 200 kt cap). Per-body figures are the sum of these modifiers across every colony on the body.

### Lifecycle

- **Bootstrap** — a body that gains launch infrastructure receives a fresh `LaunchCapacity` component at `25%` of its derived cap, anchored at the current simulation time. New campaigns and old saves without launch records start at this level.
- **Accrual** — capacity is reconciled lazily when read or consumed: `current += production × (now − anchor)`, clamped at the cap. The anchor always advances, so a backwards clock or a save/load round trip cannot mint capacity.
- **Reservation** — the shipbuilding surface launch path reconciles the body, attempts `try_consume(launch_mass_t)`, and only proceeds if the debit succeeds. If consumables or credits then fail, the reservation is refunded so the project waits on a clean slate.
- **Persisted state** — `LaunchCapacity { current_tonnes, last_updated_sim_seconds }` is stored on the body and persisted through `BodyDivergence::launch_capacity_override`. `DirtyReason::LaunchCapacity` is the dedicated dirty marker; the extract path emits it whenever the body is marked or carries the component.

### UI

- **Top-of-tab readout** in the Shipbuilding workspace's Construction Control tab: `available / cap t launch | +production t/yr`.
- **Per-project blocker**: a `ReadyForLaunch` project that is short on capacity shows `Awaiting Launch Capacity (available / required t)` rather than a generic state label.
- The legacy `LaunchCapacityState` resource and the hard-coded `LAUNCH_SITE / SPACE_PORT / ORBITAL_LIFT` constants have been retired; the workspace and the shipbuilding consumer both read the canonical component.

## 9. Cross-References

- `ARCHITECTURE.md` §Colony Management — engineering-level architecture and component list.
- `docs/design/LOGISTICS_NETWORK.md` — full LocalStockpile / Request / Delivery flow, freighters, shipping-company AI.
- `docs/RESEARCH_MODDING.md` — tech IDs that gate buildings (`orbital_construction`, `fusion_power`, `helium3_fusion`, `semiconductor_manufacturing`, `molten_salt_fission`, `breeder_reactors`, `geothermal_energy`, `desalination`, `missile_systems`, `lunar_colony`, `asteroid_mining`, `neural_networks`, `Hydroponics`).
- `assets/data/buildings.ron` — canonical building definitions; `colony_constants` block holds `food_consumption_per_capita_mt_per_year`, `base_growth_rate`, `housing_utilization_penalty`, `available_workforce_fraction`, `food_decline_*`, and `per_capita_consumption` table.
- `src/colony/types.rs` — `BuildingType`, `BuildingCategory` (9 variants), `ColonyTier` (4 variants).
- `src/economy/types.rs` — `ResourceType` (39 variants) and helpers (`is_volatile`, `is_biological`, `is_atmospheric_gas`, `is_construction`, `is_fusion_fuel`, `is_fissile`, `is_precious_metal`, `is_strategic`, `is_exotic`, `is_mineable`).
- `src/colony/data.rs` — `BuildingDefinition`, `MAINTENANCE_AUDIT_MIN/MAX`, `audit_buildings`.
- `src/colony/systems.rs` — `process_construction_actions`, `advance_construction`, `update_colony_growth`, `deduct_maintenance_resources`, `update_treasury`.
