# Survey System — Player Manual

The v0.5.0 survey rework replaces the old three-level "Surveyed / Scanned / Surveyed Completely" state with a per-body **eight-dimension discovery model** backed by real instrument campaigns, an analysis queue staffed by scientists, and an anomaly system that drives research and building unlocks.

This document is the player reference for the new system. For the design rationale, see `docs/design/SURVEY_REWORK.md`. For modding (adding your own dimensions, instruments, anomalies, missions), see `docs/MODDING.md`.

> **Status note (2026-06-12)** — this is a **pre-draft** of the v0.5.0 player manual. The eight dimensions, the method taxonomy, and the analysis queue described here are the design contract; minor wording may shift when the engineering chain (PR-A through PR-G) lands. The doc is reviewed against the actual implementation at PR-open time.

---

## Table of Contents

1. [What Surveying Is](#1-what-surveying-is)
2. [The Eight Discovery Dimensions](#2-the-eight-discovery-dimensions)
3. [Survey Methods and Instruments](#3-survey-methods-and-instruments)
4. [Tiers and Confidence](#4-tiers-and-confidence)
5. [A Survey Campaign From Start to Finish](#5-a-survey-campaign-from-start-to-finish)
6. [Personnel: Field Scientists](#6-personnel-field-scientists)
7. [Tech Tree](#7-tech-tree)
8. [Anomalies and Discoveries](#8-anomalies-and-discoveries)
9. [Mining Unlocks — How Survey Gates Extraction](#9-mining-unlocks--how-survey-gates-extraction)
10. [Failure Modes and Recovery (preview)](#10-failure-modes-and-recovery-preview)
11. [Landing Sites (preview)](#11-landing-sites-preview)
12. [See Also](#12-see-also)

---

## 1. What Surveying Is

Surveying is the multi-year process of learning what a body is made of, what it can yield, and what is worth going back for. The old system reduced this to a single `SurveyLevel` (0/1/2) that gated how much of a deposit you could see. The new system splits the problem into **eight independent discovery dimensions** that each fill in over time, at different costs, with different instruments.

Two consequences fall out of this:

- A body is "fully surveyed" only when every dimension is at its top tier with high confidence. Most bodies in the active game will be partially surveyed for years; that is normal and intended.
- Mining for a resource on a body requires a specific dimension to be at a specific tier, regardless of the body's overall survey percentage. A body at 80% total coverage may still show "?" for a particular resource if the dimension that gates that resource has not been advanced.

The dossier's **Survey** tab is the canonical view of a body's survey state. The system summary's `SURVEY %` stat is a weighted average across all bodies in the active system.

---

## 2. The Eight Discovery Dimensions

A body's `SurveyState` tracks each dimension independently. Each dimension has a **tier** (0–5; 0 = unknown, 5 = fully characterised) and a **confidence** (0.0–1.0; rises with more measurements, falls with time).

| # | Dimension | What it tells you | Tier 1 example | Tier 5 example |
|---|-----------|-------------------|----------------|----------------|
| 1 | **Orbital mechanics** | Mass, gravity, orbit, axial tilt, rotation | Mass, radius, orbit | Full ephemeris |
| 2 | **Atmosphere** | Pressure, temperature, composition, breathability | Pressure, gross composition | Full climate model |
| 3 | **Surface features** | Topography, craters, volcanism, oceans, ice caps | Crater density | Full topographic map |
| 4 | **Mineral classes** | Broad mineralogical families (silicates, oxides, ices) | Family hints | Full mineral map |
| 5 | **Mineral deposits** | Specific resource types and locations | — | Concentration, depth, accessibility |
| 6 | **Subsurface structure** | Deep crust, mantle hints, core composition | — | Mantle hints |
| 7 | **Habitability** | Radiation, soil chemistry, water availability | — | Full biological assay |
| 8 | **Anomalies** | Water ice, organic compounds, unusual signatures | "anomaly detected" | Full catalog |

The first two dimensions are revealed at tier 1 by the cheapest method (a flyby probe). The rest layer on with each subsequent campaign.

> **Eight dimensions, not three** — the old `SurveyLevel` enum collapsed everything into "how much of a deposit do I see". The new model matches the real categories of planetary-science data, which is why mineralogical survey and habitability survey can be in completely different states on the same body.

---

## 3. Survey Methods and Instruments

A **method** is the kind of campaign you run; an **instrument** is the physical sensor that does the work. Each method advances a specific set of dimensions and has a typical cost and duration:

| Method | What it does | Typical cost | Typical duration |
|--------|--------------|--------------|------------------|
| **Flyby probe** | A probe on a hyperbolic trajectory past the body | Cheap (one probe) | 1–6 sim-months flight + analysis |
| **Orbital satellite** | A probe inserted into orbit, stationkept | Moderate | 1–3 sim-years to full coverage |
| **Remote sensing pass** | Satellite with multispectral / hyperspectral imager | Cheap (just analysis time) | 6–12 sim-months |
| **Atmospheric probe** | A small entry probe | Moderate | 1–2 sim-years |
| **Surface lander** | A lander that touches down | Expensive | 1–3 sim-years |
| **Rover survey** | A rover that traverses the surface | Very expensive | 3–10 sim-years |
| **Seismic survey** | A surface network of seismometers | Moderate | 2–5 sim-years |
| **Drill core sample** | A drill rig | Very expensive | 1–2 sim-years per core |
| **Sample return** | Lander + ascent + return capsule | Most expensive | 3–8 sim-years |

Instruments are the RON-defined entry points for these methods. The default v0.5.0 set includes `passive_sensor_array`, `phased_array_radar`, `multispectral_imager`, `hyperspectral_imager_v2`, `atmospheric_sampler`, `deep_atmospheric_sonde`, `core_sampler`, `biology_assay_payload`, `rover_payload`, `cryogenic_sampling_arm`, `seismic_network`, `deep_seismic_array_v2`, `deep_drill`, `laser_drill`, `sample_return_capsule`, `asteroid_prospector_kit`, and `interstellar_probe_payload` (see `assets/data/survey/instruments.ron`). Modders can add their own — see `docs/MODDING.md`.

The rule is: **an instrument only does what its method permits.** A `phased_array_radar` (remote sensing) cannot advance `Subsurface structure` past accuracy tier 1; that requires a `seismic_network` (seismic). When a higher-accuracy follow-up is required, the dossier's "Recommended next step" panel will tell you which.

---

## 4. Tiers and Confidence

A body is "fully surveyed" when every dimension is at tier 5 with confidence ≥ 0.8. The body is "survey-active" when at least one dimension is at tier 1+.

**Tier semantics** are the player's mental model of "what do I know at this tier?". The default per-dimension tier descriptions are in `assets/data/survey/tiers.ron` and are a literal transcription of the design table. A few highlights:

| Tier | Mineral deposits | Atmosphere | Subsurface | Anomalies |
|------|------------------|------------|------------|-----------|
| 2 | Suspected (mining unlocks at 40% of nominal) | + temperature, gas % | — | + 1 named anomaly |
| 3 | + 1 site pinned (mining 60%) | + breathability | Shallow crust | + multiple |
| 4 | + 3 sites pinned (mining 100%) | + escape velocity, weather | + deep crust | refined |
| 5 | concentration, depth | full climate model | mantle hints | full catalog |

**Confidence decay** is the second time-based mechanic. A dimension's `confidence` decays at **0.5% per sim-year** if no new measurement is taken. At confidence 0.3, the tier is shown with a warning icon. At confidence 0.1, the data is treated as "stale" and a re-survey is recommended. The design intent is to reward ongoing presence (a satellite constellation, a permanent lander) over one-shot missions.

**Total survey percentage.** The system summary's `SURVEY %` is a weighted average over all `(dimension, body)` pairs in the active system, where each pair has a target tier of 5. The result is a real percentage of what is knowable, not just a count of body-tiers.

---

## 5. A Survey Campaign From Start to Finish

The end-to-end player experience for a typical campaign:

1. **Identify a target.** Open the dossier for a body that is "Unsurveyed" with **unknown everything**. The mineral tile is "?" in the system resource grid.
2. **Plan the campaign.** Open the **Survey** tab in the dossier. See the eight dimensions, all greyed out. The "Recommended" panel suggests a flyby probe first (cheap, reveals orbital mechanics + gross atmosphere).
3. **Build and launch the flyby probe.** In the shipbuilding workspace, build a `small_probe_frame` and fit it with a `survey_radar_suite` and a `passive_sensor_array`. Launch via the fleet panel.
4. **Probe arrives, data flows.** The probe executes a flyby. A few sim-months later, the probe's data starts arriving at the player's comms network (limited by distance; may require a relay or nearby station). The **Analysis Queue** populates.
5. **Scientists process the data.** Assign a scientist to the analysis queue. With 1 scientist and 1 active analysis, the data clears in ~6 sim-months. With 3 scientists, ~2 months. Without any scientists, the data sits in the queue indefinitely.
6. **Tier 1 data populates.** Orbital mechanics and atmosphere go to tier 1. The dossier now shows mass, radius, orbit, pressure, gross composition. The system resource grid still shows "?" for minerals.
7. **Plan the next step.** Recommended: an orbital satellite for mineral classes. Build one, launch it, stationkeep. 1–3 sim-years of coverage.
8. **Iterate.** Each method costs more, takes longer, and reveals more. Tradeoffs: rover on this small body now (expensive) or orbital + seismic on a different body (cheaper, less detail).
9. **Discover anomalies.** Along the way, the analysis queue surfaces anomalies. A one-line event fires: "Anomalous reflectance detected in Mare Tranquillitatis. Spectral signature consistent with hydrated minerals." Click the event to open a research project.
10. **Commit to exploitation.** Once the player has tier 3+ on mineral deposits for a specific resource on a specific body, the mining panel unlocks that body's extraction. Build a mine.

The loop is **multi-year, multi-instrument, and rewards investment in personnel and time**.

---

## 6. Personnel: Field Scientists

Scientists are the analysis engine. Without scientists, raw instrument data sits in the queue indefinitely.

### What scientists do

Each scientist has a **specialty** (Geology, Atmospherics, Biology, Geophysics) and a **seniority** (Junior, Senior, Principal). A scientist can be assigned to one analysis job at a time. Seniority affects throughput and quality:

- **Junior** — 1.0× throughput, 0.8× confidence multiplier
- **Senior** — 1.5× throughput, 1.0× confidence multiplier
- **Principal** — 2.0× throughput, 1.2× confidence multiplier, +10% chance of finding anomalies

### Specialty matching

A `Geology` senior scientist gets a 1.5× multiplier on `Mineral deposits` analysis. A `Geophysics` senior gets 1.5× on `Subsurface structure`. Mismatched specialty = 0.7× multiplier. Build a balanced team for full coverage.

### How you hire

Scientists are produced by a new **University** building. Output: 1 junior scientist per University per ~5 sim-years. Seniority is upgraded by assigning the scientist to multiple successful analysis jobs (a slow career ladder).

### Personnel cap

The total number of scientists you can field is gated by a research tech (e.g. `scientific_administration`). Early game: 3 scientists. Mid game: 20. Late game: 200. The cap is **soft** — exceeding it is allowed but the throughput multiplier per scientist drops 5% per scientist over cap.

### The Personnel panel

The existing **Personnel** menu is filled out to be the scientist roster and assignments panel. Sort by specialty, seniority, or current assignment. Assign / unassign scientists to analysis jobs from the **Analysis Queue** sub-panel.

---

## 7. Tech Tree

The v0.5.0 survey system adds **9 new techs** to the existing tree, in the survey / personnel / geology area. The existing **Sensors** techs are reused as method gates — they were already in the tree and now drive which instruments you can build.

### New techs

| Tech | Id | Tier | Prereqs | What it unlocks |
|------|-----|------|---------|-----------------|
| Survey Methodology | `survey_methodology` | 1 | `basic_sensors` | The analysis queue. +20% analysis throughput. |
| Planetary Geology | `planetary_geology` | 2 | `survey_methodology`, `basic_physics` | Geology scientists +25% throughput. |
| Geophysics | `geophysics` | 2 | `survey_methodology`, `basic_physics` | Seismic survey method. Seismic accuracy tier 2. |
| Field Science Operations | `field_science_operations` | 2 | `planetary_geology`, `closed_loop_ecology` | Surface lander with extended life support (≥ 1 sim-year surface ops). |
| Cryogenic Sampling | `cryogenic_sampling` | 3 | `cryogenics`, `field_science_operations` | Sample return from icy bodies (Europa, Titan, Enceladus, comet nuclei). |
| Deep Seismic Array | `deep_seismic_array` | 3 | `geophysics`, `deep_drilling` | Seismic accuracy tier 4. Deep mantle probes. |
| Roving Autonomy | `roving_autonomy` | 3 | `field_science_operations`, `basic_automation` | Rover survey. Rover range +50%. |
| Sample Return Architecture | `sample_return_architecture` | 4 | `cryogenic_sampling`, `orbital_mechanics` | Sample return from any body < 5 AU. |
| Interstellar Probe | `interstellar_probe` | 5 | `sample_return_architecture`, `fusion_torch` | Flyby of bodies in other star systems (v0.6+). |

### Existing techs reused as method gates

These existed before v0.5.0 and now unlock specific survey instruments:

| Tech | Id | What it unlocks (as a method gate) |
|------|-----|------------------------------------|
| Basic Sensors | `basic_sensors` | Flyby probe (basic), remote sensing (basic) |
| Satellite Networks | `satellite_networks` | Orbital satellite, comms relay for data downlink |
| Remote Sensing | `remote_sensing` | Multispectral scan, IR telescope, hyperspectral imager |
| Radio Astronomy | `radio_astronomy` | Radio dish array (deep-space data downlink) |
| Advanced Radar Systems | `advanced_radar` | Phased-array radar (surface imaging through dust) |
| Deep Drilling | `deep_drilling` | Drill core sample (shallow) |
| Laser Drilling | `laser_drilling` | Drill core sample (deep) |
| Asteroid Prospecting | `asteroid_prospecting` | Spectral analyzer, prospecting probe (small-body specialist) |
| Closed-Loop Ecology | `closed_loop_ecology` | Biological assay payload (habitability tier 4+) |

> **Note for modders** — the tech tree is data-driven. Adding a 10th survey tech, or moving `interstellar_probe` to a different tier, is a RON edit to `assets/data/technologies.ron` plus a corresponding `required_tech` in `assets/data/survey/instruments.ron`. No Rust recompile.

---

## 8. Anomalies and Discoveries

Anomalies are the story engine. They turn "scan the planet" into "what did we find?". The v0.5.0 system ships with **9 hardcoded anomaly types** plus a `ModderAnomalyDef` RON path for additions:

| Anomaly | Discovery method | What it does |
|---------|------------------|--------------|
| `water_ice_deposit` | Remote sensing, surface lander | Unlocks a water extraction building on the body |
| `hydrated_silicates` | Remote sensing | Unlocks a research project to confirm water history |
| `methane_plume` | Atmospheric probe | Triggers a follow-up atmospheric survey mission |
| `tholin_signature` | Remote sensing | Unlocks the "prebiotic chemistry" research branch |
| `magnetic_anomaly` | Flyby, orbital | Reveals a subsurface conducting layer; helps target drilling |
| `radioactive_hotspot` | Rover, drill | Unlocks the "rare earth prospect" event chain |
| `fossil_microbe_signature` | Drill, sample return | Major research unlock; triggers a media event |
| `cryovolcanic_feature` | Orbital imaging | Unlocks the "interior ocean" research branch |
| `unidentified_reflectance` | (placeholder) | Triggers a follow-up "unknown" mission with no guarantee of resolution |

When a scientist's analysis flags an anomaly, a one-line event fires and the anomaly is logged in the body's dossier. Each anomaly has a 10–60 sim-day follow-up timer. If you don't follow up, the anomaly's confidence decays like other dimensions.

### Coolness vs. effect

The "coolness" of an anomaly (used for media coverage, fame, and player satisfaction) is **independent of its gameplay effect**. A `fossil_microbe_signature` is a top-tier coolness event but doesn't unlock a new building — it's a research unlock. A `water_ice_deposit` is mid-coolness but unlocks a real building. This split prevents the design from collapsing into "anomalies are just quest triggers." See `assets/data/survey/anomalies.ron` for the per-anomaly `coolness` weight.

---

## 9. Mining Unlocks — How Survey Gates Extraction

Survey directly gates when mining is unlocked and at what yield, per `(resource_class, dimension, min_tier)` row in `assets/data/survey/mining_efficiency.ron`. The default v0.5.0 curve:

| Resource class | Gating dimension | Tier 2 (suspected) | Tier 4 (proven) | Tier 5 (deep) | Notes |
|----------------|------------------|--------------------|-----------------|----------------|-------|
| **ShallowOre** (proven crustal) | `MineralDeposits` | 40% yield | 100% yield | 100% | No confirmation needed |
| **DeepOre** (deep deposits) | `Subsurface` | — | — | 100% | Requires confirmation (drill rig) |
| **PlanetaryBulk** (mantle / core) | `Subsurface` | — | — | 20% | Requires confirmation; tier 5 only |
| **AtmosphericGas** | `Atmosphere` | — | 60% (breathability) | 100% | Gas-giant filter applies |
| **TraceIsotope** (He-3, lunar volatiles) | `MineralDeposits` | 20% | 60% | 100% | Modder-rebalanceable |

The dossier's resource grid shows one of four tile shapes per resource: `Unknown`, `ClassOnly`, `Range`, `Precise`. The tile is a function of the gating dimension's tier and confidence:

- `Unknown` — the gating dimension is tier 0.
- `ClassOnly` — tier 1+ shows the resource class ("silicates", "ices") but no estimate.
- `Range` — tier 2+ shows a `low_estimate / mid_estimate / high_estimate` triplet (the wider the band, the lower the confidence).
- `Precise` — tier 4+ shows a tight band; the body's mining yield is at or near nominal.

Modders can rebalance the curve by editing `assets/data/survey/mining_efficiency.ron` directly (e.g. push `ShallowOre` to 100% at tier 3 instead of tier 4) without recompiling.

---

## 10. Failure Modes and Recovery (preview)

> **Preview** — failure modes and recovery missions land in the PR-G area of the v0.5.0 engineering chain. The summary below is the design contract; minor wording may shift at PR-G open.

Survey missions are not free of risk. A mission can fail in ways that cost the player assets, time, and occasionally people. The design default:

| Failure | Typical rate | What happens | Recovery |
|---------|--------------|--------------|----------|
| **Probe loss** | 5% (flyby) | Mission slot freed, no data, probe entity destroyed. | Re-launch another probe. |
| **Rover stuck** | 8% (rover) | Spawns a `rescue_mission` sub-mission (1 chemical survey ship, 60–180 sim-days, 10% chance unrecoverable). | Run the rescue mission or write off the rover. |
| **Drill bit stuck** | 10% (drill) | Rig stranded. Must retrieve (1 NTR ship, ~1 sim-year) or abandon (lose rig + drilling progress). | Run the retrieval mission or abandon. |
| **Solar storm** | 2% (orbital) | Corrupts `ResourceEstimate` and `SubsurfaceImaging` data by 0.1–0.2. | Next orbital pass recovers. |
| **Crew injury** | 2% (ground team) | Scientist transitions to `Injured { 60–180 sim-days }`. | Wait for recovery; no permanent loss. |

Recovery mission types: `equipment_recovery`, `crew_extraction`, `rig_retrieval`, and `data_recovery` (re-fly a corrupted pass). Each has its own RON template in `assets/data/survey/missions.ron` and its own cost / duration / risk profile.

---

## 11. Landing Sites (preview)

> **Preview** — landing sites and extraction site evaluation land in the PR-D area of the v0.5.0 engineering chain. The summary below is the design contract; minor wording may shift at PR-D open.

When a body has tier 2+ on `Surface features` and tier 2+ on `Mineral deposits`, the player can begin evaluating **landing sites** — a specific spot on the body where a mine or settlement could be placed. A landing site has:

- A **location** (latitude / longitude, or "geostationary" for orbital).
- A **terrain rating** (slope, regolith depth, radiation — affects construction cost).
- A **resource estimate** (the `(low, mid, high)` triplet for the resource classes within reach).
- A **risk profile** (what failure modes are most likely at this site; affects insurance / cost).

The dossier's **Landing Sites** sub-panel lists candidate sites for the selected body, sortable by resource estimate, terrain, and risk. Click a site to see the per-site dossier (extraction panel, buildable structures, expected yield at current survey tier).

For a full design walk-through of the landing site model and the per-site resource extraction evaluation, see `docs/design/SURVEY_REWORK.md` §[Landing Sites and Extraction Sites] (added in PR-D).

---

## 12. See Also

- `docs/design/SURVEY_REWORK.md` — the design rationale, full per-dimension tier tables, the schema delta, and the migration plan.
- `docs/MODDING.md` — how to add a new dimension, instrument, anomaly, or mission via RON edits.
- `docs/RESEARCH_MODDING.md` — the 9 new survey / personnel / geology techs and how to edit `assets/data/technologies.ron`.
- `docs/UI.md` — the dossier Survey tab and Personnel panel layout conventions.
- `docs/COLONIES.md` — founding an outpost on a body you have surveyed.
- `docs/RESOURCES.md` — the resource catalogue and per-class economic rules.
