# Survey System Rework

Design specification for the progressive, multi-instrument survey system targeted at v0.5.0 (Roadmap §5.1 + §5.2).

Closes GRA-35 (`Prepare Survey Rework`). The original issue body asks for an analysis of the current implementation, then a plan for a reworked system using satellites, rovers, probes, ground teams, drilling, seismic analysis, and radar.

This document is the LGD design pass. No RON is added in this phase. The schema delta at the end names the new components, tech entries, and gameplay flow for the Coder who picks it up after the CEO/CTO approve scope.

---

## Table of Contents

1. [Design Goals](#design-goals)
2. [Current State (Audit)](#current-state-audit)
3. [The Problem with the Current System](#the-problem-with-the-current-system)
4. [Discovery Dimensions](#discovery-dimensions)
5. [Survey Methods and Instruments](#survey-methods-and-instruments)
6. [Progressive Survey Levels](#progressive-survey-levels)
7. [The Gameplay Loop](#the-gameplay-loop)
8. [Personnel: Field Scientists](#personnel-field-scientists)
9. [Tech Tree Integration](#tech-tree-integration)
10. [UI/UX](#uiux)
11. [Resource Reveal Matrix](#resource-reveal-matrix)
12. [Anomalies and Discoveries](#anomalies-and-discoveries)
13. [Data-Driven Surface: New RON Files](#data-driven-surface-new-ron-files)
14. [Schema Delta (Rust Side)](#schema-delta-rust-side)
15. [Migration Plan](#migration-plan)
16. [Modder Surface](#modder-surface)
17. [Test Plan](#test-plan)
18. [Out of Scope (v0.5.0)](#out-of-scope-v050)
19. [Acceptance Criteria](#acceptance-criteria)
20. [Open Questions](#open-questions)

---

## Design Goals

| Goal | Description |
|------|-------------|
| **Progressive discovery** | Survey is a layered, multi-instrument process. Each instrument reveals a new dimension of the body's properties; nothing is fully known after a single click. |
| **Realism** | Surface landers cannot confirm subsurface ore bodies. A seismic survey cannot map surface mineralogy. Each method has a bounded, physically reasonable domain. |
| **Player arc** | The survey research path is a real investment. The early game is orbital flybys and broad mineral classes. Mid-game adds rovers, ground teams, and seismic. Late game unlocks deep core sampling. |
| **Personnel integration** | Scientists are the analysis engine. Without scientists, raw survey data sits in the queue unprocessed. With scientists, data turns into decisions. |
| **Modder-first** | Survey methods, instruments, and dimensions are all RON-driven. A modder can add a new instrument (e.g. muon tomography) without touching Rust. |
| **Backward compat** | Existing saves load without error. The 3-tier `SurveyLevel` enum is replaced with the new `SurveyState` component; a migration shim maps old `SurveyLevel` values to the new state at load. |
| **Rover and probe classes earn their place** | The existing `lander_frame`, `small_probe_frame`, `probe_carrier_frame` hulls in `ship_hulls.ron` are repurposed as survey ships. Survey is their primary use; combat is not. |

---

## Current State (Audit)

The current system lives in **three places** and is more shallow than the player's intuition expects.

### Data model — `src/economy/components.rs:271`

```rust
pub enum SurveyLevel {
    Unsurveyed,
    OrbitalScan,   // Reveals proven_crustal
    SeismicSurvey, // Reveals deep_deposits
    CoreSample,    // Reveals planetary_bulk
}
```

A single `SurveyLevel` enum on the body entity. It is incremented by clicking an **UPGRADE** button in the body's dossier panel (`src/ui/dossier_panel.rs:1418`, inside `draw_resource_section` at `src/ui/dossier_panel.rs:1380-1477`). The increment is instantaneous; there is no time cost, no resource cost, no personnel requirement, and no chance of failure. Earth starts at `CoreSample` (`src/plugins/solar_system.rs:922`); every other body in the system starts at `Unsurveyed`.

The `discovered_amount()` function (`src/economy/components.rs:280`) maps each level to a fixed slice of the `ResourceReserve` triple `(proven_crustal, deep_deposits, planetary_bulk)`. Three levels, three slices, no other information is gated.

### UI surface

- **Body dossier** (`src/ui/dossier_panel.rs:1380-1477`, function `draw_resource_section`): shows `SURVEY: ORBITAL / SEISMIC / CORE SAMPLE` plus an UPGRADE button (currently at `src/ui/dossier_panel.rs:1418`). Click to bump one level. The button is always enabled.
- **Mining panel** (`src/ui/economy_panel.rs:114-142`): a `MiningSurveyFilter` enum filters the mining body list by `Surveyed / Seismic+ / CoreOnly`. The filter is a UI-only view-state — it does not affect what the body actually yields, which is already gated by `discovered_amount()`.
- **Starmap system summary** (`src/ui/dashboard.rs:1732-1851`): a system-wide `SURVEY %` and a `SURVEYED BODIES` count, plus a tile grid showing the union of resources discovered across all bodies in the active system. The percentage is `(discovered_weight / total_weight) × 100` over the `ResourceReserve.total_mass()` sum.
- **History ledger** (`src/economy/history.rs:553-557`, the `match survey_level` arm): per-era count of bodies in each level for analytics.

### What it doesn't do

- No flyby reconnaissance. No flyby probes.
- No distinction between *orbital* survey and *surface* survey.
- No satellite constellations. No time-on-station.
- No scientists. No data-interpretation step. No analysis queue.
- No anomalies, no discoveries, no "we found water ice!" events.
- No chance of failure. A seismic survey on a body with no actual deep deposits still claims them as "discovered" because `discovered_amount()` returns the deposit's full `deep_deposits` value, which is what the body actually has. The model assumes the surveyor is always correct.
- No hidden deposits. Every body has its true deposit set in `solar_system.ron`; the survey system only delays visibility, it does not introduce uncertainty.
- No connection to the existing probe/rover hulls in `ship_hulls.ron` — those exist purely as ResearchVessel build targets with no gameplay function.

---

## The Problem with the Current System

The current 3-tier model is **synchronous, instantaneous, and omniscient-per-tier**. Click UPGRADE → 100% of that tier's data is now visible. There is no:

1. **Time cost.** Survey should be a campaign, not a button.
2. **Resource cost.** A seismic survey ship with a phased-array radar is a real capital asset.
3. **Method specificity.** Seismic does not reveal surface mineralogy; orbital spectroscopy does not reveal deep deposits. The current model treats survey as a single axis.
4. **Failure / surprise.** A real survey often returns ambiguous data. A lander might find *different* concentrations than the orbital survey suggested, forcing a deeper survey.
5. **Personnel involvement.** Real surveys are run by scientists. The game's existing `Personnel` menu (`src/ui/dashboard.rs:1249`) is a stub ("Officers, managers, and personnel assignments will be shown here.").
6. **Connection to existing hulls.** The `lander_frame`, `small_probe_frame`, `micro_probe_frame`, and `probe_carrier_frame` hulls are in `ship_hulls.ron` but have no gameplay loop that drives player demand for them. They are museum pieces.

The result: survey is the least-interesting part of the game. The fix is to make survey the *most* interesting part of the game, the way Aurora 4X makes the "what's on this planet?" question a multi-year campaign.

---

## Discovery Dimensions

A body has multiple independent **discovery dimensions**. Each dimension is filled in (or refined) by a specific survey method. A body is "fully surveyed" only when every dimension is at its highest fidelity.

| Dimension | What it tells the player | Example methods |
|-----------|--------------------------|-----------------|
| **Orbital mechanics** | Mass, gravity, orbital parameters, axial tilt, rotation period | Flyby probe, satellite |
| **Atmosphere** | Pressure, temperature, composition, breathability | Flyby + remote sensing, in-situ atmospheric probe |
| **Surface features** | Topography, craters, volcanism, oceans, ice caps | Orbital imaging, surface lander, rover |
| **Mineral classes** | Broad mineralogical families present (silicates, oxides, ices) | Remote sensing, sample return |
| **Mineral deposits** | Specific resource types + location | Surface lander, rover, drilling |
| **Subsurface structure** | Deep crust, mantle hints, core composition | Seismic, gravimetry, magnetometry |
| **Habitability** | Radiation, soil chemistry, water availability | Surface lander, biological assay |
| **Anomalies** | Water ice, organic compounds, unusual signatures | All methods can surface anomalies |

The first two dimensions (orbital mechanics, atmosphere) are revealed at L1 (flyby). The rest layer on with each subsequent method. See [§Progressive Survey Levels](#progressive-survey-levels) for the gating.

> **Why eight dimensions and not three.** The current three-level model collapses everything into "how much of `ResourceReserve` do I see". Eight dimensions match the real categories of planetary-science data. The dimension count is also RON-defined, so a modder can add a ninth (e.g. "Magnetosphere") without Rust changes.

---

## Survey Methods and Instruments

Each method is a **player action** that consumes time, requires an instrument, and advances one or more dimensions.

| Method | Instrument | Dimensions advanced | Typical cost | Typical duration |
|--------|-----------|---------------------|--------------|------------------|
| **Flyby probe** | A probe on a hyperbolic trajectory past the body | Orbital mechanics, atmosphere (gross) | Cheap (one probe) | 1–6 sim-months flight + analysis |
| **Orbital satellite** | A probe inserted into orbit | Surface features, mineral classes, atmosphere refinement | Moderate (probe + stationkeeping) | 1–3 sim-years to full coverage |
| **Remote sensing pass** | Satellite with multispectral / hyperspectral imager | Mineral classes, surface features, anomalies | Cheap (just analysis time) | 6–12 sim-months |
| **Atmospheric probe** | A small entry probe | Atmosphere (full), habitability (partial) | Moderate (one entry probe) | 1–2 sim-years |
| **Surface lander** | A lander that touches down | Mineral deposits (one site), surface features (one site), anomalies | Expensive (lander bus + payload) | 1–3 sim-years |
| **Rover survey** | A rover that traverses the surface | Mineral deposits (broad), surface features, habitability | Very expensive (rover + com relay) | 3–10 sim-years |
| **Seismic survey** | A surface network of seismometers | Subsurface structure, deep deposits | Moderate (network deployment) | 2–5 sim-years |
| **Drill core sample** | A drill rig | Mineral deposits (deep), anomalies, planetary bulk | Very expensive (drill + power) | 1–2 sim-years per core |
| **Sample return** | Lander + ascent + return capsule | Mineral deposits (confirmed), anomalies, mineral classes | Most expensive (round-trip) | 3–8 sim-years |

### Instrument cards (RON-defined)

Each instrument is a `SurveyInstrument` entry in a new `assets/data/survey_instruments.ron` file. Fields:

```ron
(
    id: "phased_array_radar",
    display_name: "Phased-Array Radar",
    description: "Synthetic-aperture radar for surface and shallow-subsurface imaging through cloud and dust.",
    method: "remote_sensing",
    required_tech: Some("advanced_radar"),
    base_duration_days: 365,
    scientist_requirement: 2,
    accuracy_tier: 2,  // 0-5; gates the resolution of the data returned
    consumes: [
        (Power, 0.5),  // MW-yrs
    ],
    produces_anomalies: true,
    valid_targets: [Planet, Moon, Asteroid, DwarfPlanet],  // not GasGiant
)
```

The instrument is fitted to a ship module, a satellite, or a ground facility. The LGD rule is: **an instrument only does what its method permits**. A `phased_array_radar` cannot advance the `Subsurface structure` dimension past accuracy tier 1 — that requires a `seismic_network`.

### Method taxonomy

The `method` field is a RON enum: `Flyby`, `Orbital`, `RemoteSensing`, `AtmosphericProbe`, `SurfaceLander`, `Rover`, `Seismic`, `Drill`, `SampleReturn`. Adding a new method (e.g. `MuonTomography`) is one enum variant + one RON entry.

---

## Progressive Survey Levels

The single `SurveyLevel` enum is replaced with a `SurveyState` component:

```rust
pub struct SurveyState {
    pub dimensions: HashMap<SurveyDimension, DimensionFidelity>,
    pub active_missions: Vec<ActiveSurveyMission>,
    pub last_updated_sim_time: f64,
    pub total_science_points_invested: f64,
}

pub struct DimensionFidelity {
    pub tier: u8,        // 0–5; 0 = unknown, 5 = fully characterized
    pub last_measured: Option<f64>,
    pub confidence: f32, // 0.0–1.0; rises with more measurements, falls with time
}
```

A body is "fully surveyed" when every dimension is at tier 5 with confidence ≥ 0.8. The body is "survey-active" when at least one dimension is at tier 1+.

### Tier semantics per dimension

| Tier | Orbital mechanics | Atmosphere | Surface features | Mineral classes | Mineral deposits | Subsurface | Habitability | Anomalies |
|------|-------------------|------------|------------------|-----------------|------------------|------------|--------------|-----------|
| 0 | None | None | None | None | None | None | None | None |
| 1 | Mass, radius, orbit | Pressure, gross composition | Crater density | Family hints (silicates?) | — | — | — | "anomaly detected" |
| 2 | + axial tilt, rotation | + temperature, gas % | + major features (oceans, ice caps) | + class (carbonaceous?) | Suspected | — | — | + 1 named anomaly |
| 3 | + moons, libration | + breathability | + minor features | + specific minerals (olivine) | + 1 site | Shallow crust | Soil chemistry | + multiple |
| 4 | + dust/torus | + escape velocity, weather | + surface age | + deposit type | + 3 sites | + deep crust | + water availability | refined |
| 5 | full ephemeris | full climate model | full topographic map | full mineral map | + concentration, depth | mantle hints | full biological assay | full catalog |

The exact tier values for each dimension-method pair are in `assets/data/survey_tiers.ron`. Modders can rebalance.

### Confidence decay

A dimension's `confidence` decays at 0.5% per sim-year if no new measurement is taken. At confidence 0.3, the tier is shown but with a warning icon. At confidence 0.1, the data is treated as "stale" and a re-survey is recommended. This rewards ongoing presence (a satellite constellation) over one-shot missions.

### Total survey percentage

The system summary's "SURVEY %" stat becomes a weighted average over all dimensions and all bodies in the active system, where each `(dimension, body)` pair has a target tier of 5. The result is a real percentage of what is knowable, not just a count of body-tiers.

---

## The Gameplay Loop

The end-to-end player experience for a typical survey campaign:

1. **Identify a target.** Player has a new body in the system. The dossier shows the body is at "Unsurveyed" with **unknown everything**. The mineral tile is "?" in the system resource grid.
2. **Plan the campaign.** Player opens the body's dossier → "Survey" tab. Sees the eight dimensions, all greyed out. The "Recommended" panel suggests a flyby probe first (cheap, reveals orbital mechanics + gross atmosphere).
3. **Build and launch the flyby probe.** Player uses the existing shipbuilding workspace to build a `small_probe_frame` (already in `ship_hulls.ron` line 31), fits it with a `survey_radar_suite` and a `passive_sensor_array`, and launches it via the fleet panel.
4. **Probe arrives, data flows.** Probe executes a flyby maneuver. The sim advances. A few sim-months later, the probe's data starts arriving at the player's comms network (limited by distance; requires a relay or nearby station). The "Analysis Queue" populates.
5. **Scientists process the data.** A scientist is assigned to the analysis queue. With 1 scientist and 1 active analysis, the data clears in ~6 sim-months. With 3 scientists, ~2 months. Without any scientists, the data sits.
6. **Tier 1 data populates.** Orbital mechanics dimension goes to tier 1. Atmosphere to tier 1. The dossier now shows mass, radius, orbit, pressure, gross composition. The system resource grid still shows "?" for minerals.
7. **Plan the next step.** Recommended: orbital satellite for mineral classes. Player builds one, launches it, stationkeeps. 1–3 sim-years of coverage.
8. **Iterate.** Each method costs more, takes longer, and reveals more. The player makes tradeoffs: rover on this small body now (expensive) or orbital + seismic on a different body (cheaper, less detail).
9. **Discover anomalies.** Along the way, the analysis queue surfaces anomalies. Player gets an event: "Anomalous reflectance detected in Mare Tranquillitatis. Spectral signature consistent with hydrated minerals." Click → opens a research project to follow up.
10. **Commit to exploitation.** Once the player has tier 3+ on mineral deposits for a specific resource on a specific body, the mining panel unlocks that body's extraction. The player builds a mine.

The loop is **multi-year, multi-instrument, and rewards investment in personnel and time**.

---

## Personnel: Field Scientists

Scientists are the analysis engine. Without scientists, raw instrument data sits in a queue indefinitely.

### Scientist entity

```rust
pub struct Scientist {
    pub id: ScientistId,
    pub name: String,
    pub specialty: ScientistSpecialty,  // Geology, Atmospherics, Biology, etc.
    pub seniority: SeniorityTier,        // Junior, Senior, Principal
    pub assigned_body: Option<Entity>,
    pub current_analysis: Option<AnalysisJobId>,
    pub lifetime_data_processed: f64,
    pub lifetime_anomalies_flagged: u32,
}
```

Seniority affects throughput and quality:
- **Junior** (1.0× throughput, 0.8× confidence multiplier)
- **Senior** (1.5× throughput, 1.0× confidence multiplier)
- **Principal** (2.0× throughput, 1.2× confidence multiplier, +10% chance of finding anomalies)

### Hiring

Scientists are produced by a new **University** building. Output: 1 junior scientist per University per ~5 sim-years. Seniority is upgraded by assigning the scientist to multiple successful analysis jobs (a slow career ladder).

### Specialty matching

A `Geology` senior scientist gets a 1.5× multiplier on `Mineral deposits` analysis. A `Geophysics` senior gets a 1.5× multiplier on `Subsurface structure`. Mismatched specialty = 0.7× multiplier. The player needs to build a balanced team for full coverage.

### Personnel cap

The total number of scientists the player can field is gated by a `scientific_administration` tech (or similar). Early game: 3 scientists. Mid game: 20. Late game: 200. The cap is **soft** — exceeding it is allowed but the throughput multiplier per scientist drops 5% per scientist over cap.

### Existing Personnel menu

The existing `GameMenu::Personnel` (`src/game_state.rs:32`) is filled out to be the scientist roster and assignments panel. The "Officers, managers, and personnel assignments will be shown here." stub is replaced with a real scientist list, sortable by specialty / seniority / current assignment.

---

## Tech Tree Integration

The current tech tree has **good coverage already**. Many existing techs become natural gates for survey methods. New techs fill the gaps.

### Existing techs reused as method gates

| Tech | Id | Unlocks (as survey method) |
|------|-----|---------------------------|
| Basic Sensors | `basic_sensors` | Flyby probe (basic), remote sensing (basic) |
| Satellite Networks | `satellite_networks` | Orbital satellite, comms relay for data downlink |
| Remote Sensing | `remote_sensing` | Multispectral scan, IR telescope, hyperspectral imager |
| Radio Astronomy | `radio_astronomy` | Radio dish array (deep-space data downlink) |
| Advanced Radar Systems | `advanced_radar` | Phased-array radar (surface imaging through dust) |
| Deep Drilling | `deep_drilling` | Drill core sample (shallow) |
| Laser Drilling | `laser_drilling` | Drill core sample (deep) |
| Asteroid Prospecting | `asteroid_prospecting` | Spectral analyzer, prospecting probe (small-body specialist) |
| Closed-Loop Ecology | `closed_loop_ecology` | Biological assay payload (habitability tier 4+) |
| Cryogenics (if present) | — | Cryogenic sample handling for icy bodies |

> **Note:** the PR #123 commit message listed 7 reused techs; the canonical list above has 8 confirmed (`basic_sensors`, `satellite_networks`, `remote_sensing`, `radio_astronomy`, `advanced_radar`, `deep_drilling`, `laser_drilling`, `asteroid_prospecting`) plus 1 conditional (`closed_loop_ecology`, gates the habitability tier 4+ bio-assay) and 1 tentative (`cryogenics`, present in `technologies.ron` but its prereq role for `cryogenic_sampling` is provisional). The table above is authoritative; the commit-message undercount did not reflect `radio_astronomy`.

The existing tech tree already covers the **instrument** side. The new design adds techs for the **methodology** and **personnel** side.

### New techs to add

| Tech | Id | Tier | Prereqs | Effect |
|------|-----|------|---------|--------|
| Survey Methodology | `survey_methodology` | 1 | `basic_sensors` | Unlocks the analysis queue. Analysis throughput +20%. |
| Planetary Geology | `planetary_geology` | 2 | `survey_methodology`, `basic_physics` | Scientists with Geology specialty get +25% throughput. |
| Geophysics | `geophysics` | 2 | `survey_methodology`, `basic_physics` | Unlocks seismic survey method. Seismic accuracy tier 2. |
| Field Science Operations | `field_science_operations` | 2 | `planetary_geology`, `closed_loop_ecology` | Unlocks surface lander with extended life support (≥1 sim-year surface ops). |
| Cryogenic Sampling | `cryogenic_sampling` | 3 | `cryogenics`, `field_science_operations` | Unlocks sample return from icy bodies (Europa, Titan, Enceladus, comet nuclei). |
| Deep Seismic Array | `deep_seismic_array` | 3 | `geophysics`, `deep_drilling` | Seismic accuracy tier 4. Unlocks deep mantle probes. |
| Roving Autonomy | `roving_autonomy` | 3 | `field_science_operations`, `basic_automation` | Unlocks rover survey. Rover range +50%. |
| Sample Return Architecture | `sample_return_architecture` | 4 | `cryogenic_sampling`, `orbital_mechanics` | Sample return from any body < 5 AU. |
| Interstellar Probe | `interstellar_probe` | 5 | `sample_return_architecture`, `fusion_torch` | Flyby of bodies in other star systems (v0.6+). |

The total adds **9 new techs** in the survey / personnel / geology area, distributed across tiers 1–5.

---

## UI/UX

### Survey tab in the body dossier

The dossier panel (`src/ui/dossier_panel.rs`) gains a "Survey" tab alongside the existing sections. Layout:

```
┌─ SURVEY ───────────────────────────────────────────────────┐
│  Progress: 32% (target: 100%)                              │
│  Scientists assigned: 3                                    │
│  Active missions: 2                                        │
│                                                          │
│  DIMENSIONS                          TIER   CONFIDENCE    │
│  ● Orbital mechanics                 5/5   98%  ▓▓▓▓▓ │
│  ● Atmosphere                        4/5   82%  ▓▓▓▓▓ │
│  ● Surface features                  2/5   71%  ▓▓▓░░ │
│  ● Mineral classes                   2/5   68%  ▓▓▓░░ │
│  ○ Mineral deposits                  0/5    0%  ░░░░░ │
│  ○ Subsurface structure              0/5    0%  ░░░░░ │
│  ● Habitability                      1/5   45%  ▓░░░░ │
│  ○ Anomalies                         0/5    0%  ░░░░░ │
│                                                          │
│  RECOMMENDED NEXT STEP                                    │
│  → Surface lander at equatorial site                     │
│    Cost: ~1,200 BP, 2–3 sim-yrs, 1 senior geologist     │
│    [PLAN MISSION]                                         │
│                                                          │
│  ACTIVE MISSIONS                                          │
│  • Orbital satellite "Mare Imbrium 1" (327 days elapsed) │
│  • Atmospheric probe "Ariel-2"      (89 days elapsed)   │
│                                                          │
│  ANOMALIES DETECTED                                       │
│  • "Anomalous reflectance, suspected hydrated silicates" │
└──────────────────────────────────────────────────────────┘
```

### Analysis queue panel

A new top-level panel reachable from the existing `Personnel` menu, or as a sub-panel of the dossier. Lists pending analysis jobs:

```
ANALYSIS QUEUE
─────────────
1. Mare Imbrium 1 — multispectral data (orbital, 412 GB)
   Assigned: Dr. R. Vasquez (Geology, Senior) — ETA 47 days
2. Ariel-2 — atmospheric spectra
   Unassigned — needs Atmospherics specialist
3. Survey Carrier "Magellan" — gravimetry pass
   Assigned: Dr. K. Park (Geophysics, Junior) — ETA 312 days
```

### System survey summary

The existing `SURVEY %` in the starmap system summary becomes the weighted average over all `(dimension, body)` pairs at tier ≥ 1. The `SURVEYED BODIES` count becomes the count of bodies with at least one dimension at tier 3+.

### Notification events

When an analysis surfaces a new anomaly, a one-line notification fires (in the existing notification surface once §5.4 lands, or as a toast for now). Examples:
- "Anomalous methane plume detected over equatorial region. Follow-up recommended."
- "Spectral signature consistent with tholins on surface. Organic chemistry research unlocked."

---

## Resource Reveal Matrix

This is the player's question: "When can I start mining this resource, and what does each tier cost me?"

| Resource | Minimum tier to begin mining | Mining efficiency at min tier | Required for full efficiency |
|----------|------------------------------|------------------------------|------------------------------|
| **Proven crustal** (shallow surface deposits) | Mineral deposits tier 2 | 40% (high uncertainty) | Mineral deposits tier 4 |
| **Deep deposits** (km-deep) | Subsurface structure tier 3 + Mineral deposits tier 2 | 30% | Subsurface tier 4 + deposits tier 4 |
| **Planetary bulk** (mantle / core) | Subsurface tier 5 + drill confirmation | 20% | Drill core sample complete |
| **Atmospheric gases** | Atmosphere tier 3 | 60% | Atmosphere tier 5 |
| **Trace isotopes** (e.g. He-3, lunar volatiles) | Anomaly tier 2 + dedicated instrument | 10% | Sample return + lab analysis |

The mining efficiency ramps as the player invests in deeper survey. This means **the early-game choice between "cheap orbital scan" and "expensive lander" is real**: cheap gives you the existence of the resource and a 40% mining rate; expensive gives you the deposit map and full efficiency.

The numbers in this table are the **default** curve. The Coder reads them from `assets/data/survey/mining_efficiency.ron` (see §[Data-Driven Surface: New RON Files]); a modder can rebalance the curve (e.g. make tier 2 a 25% rate instead of 40%, or push full efficiency to tier 5) without recompiling Rust. The table is the *balance* sheet the player reads; the RON file is the *config* sheet a modder edits.

### Refining the existing `discovered_amount()`

The current `discovered_amount()` function (`src/economy/components.rs:280`) returns a fixed slice per `SurveyLevel`. The new system replaces it with a function that takes a `(MineralDeposit, DimensionFidelity)` pair and returns a confidence-weighted estimate. The estimate's `low_estimate / mid_estimate / high_estimate` triplet is shown in the UI so the player knows the uncertainty.

---

## Anomalies and Discoveries

Anomalies are the **story engine** of the survey system. They turn "scan the planet" into "what did we find?"

### Anomaly types

A RON-defined list of anomaly types, each with a discovery method affinity, a "coolness" weight, and gameplay effects:

| Anomaly | Discovery method | Effect |
|---------|-----------------|--------|
| `water_ice_deposit` | Remote sensing, surface lander | Unlocks a water extraction building on the body |
| `hydrated_silicates` | Remote sensing | Unlocks a research project to confirm water history |
| `methane_plume` | Atmospheric probe | Triggers follow-up atmospheric survey mission |
| `tholin_signature` | Remote sensing | Unlocks "prebiotic chemistry" research |
| `magnetic_anomaly` | Flyby, orbital | Reveals subsurface conducting layer; helps target drilling |
| `radioactive_hotspot` | Rover, drill | Unlocks "rare earth prospect" event chain |
| `fossil_microbe_signature` | Drill, sample return | Major research unlock; triggers media event |
| `cryovolcanic_feature` | Orbital imaging | Unlocks "interior ocean" research branch |
| `unidentified_reflectance` | (placeholder) | Triggers follow-up "unknown" mission with no guarantee of resolution |

### Discovery events

When a scientist's analysis flags an anomaly, a one-line event fires and the anomaly is logged in the body's dossier. Each anomaly has a 10–60 sim-day follow-up timer. If the player doesn't follow up, the anomaly's confidence decays like other dimensions.

### Coolness weighting

The "coolness" of an anomaly (used for media coverage, fame, and player satisfaction) is independent of its gameplay effect. A `fossil_microbe_signature` is a top-tier coolness event but doesn't unlock a new building — it's a research unlock. A `water_ice_deposit` is mid-coolness but unlocks a real building. This split prevents the design from collapsing into "anomalies are just quest triggers."

---

## Data-Driven Surface: New RON Files

The new design adds six RON files and modifies `technologies.ron`. The CTO/Coder can place these in the existing `assets/data/` directory or create a `survey/` subdir; the LGD recommendation is the subdir for namespace clarity.

| File | Purpose | New entries |
|------|---------|-------------|
| `assets/data/survey/dimensions.ron` | Definition of discovery dimensions | 8 dimensions (OrbitalMech, Atmosphere, …) |
| `assets/data/survey/instruments.ron` | Survey instruments and their method/tech/accuracy | ~25 instruments (phased_array_radar, core_sampler, rover_payload, etc.) |
| `assets/data/survey/missions.ron` | Mission templates (what a single "send probe" click costs in time/resources) | ~10 mission templates (flyby_recon, orbital_imaging, surface_lander_v1, …) |
| `assets/data/survey/anomalies.ron` | Anomaly types, discovery methods, effects | ~12 anomaly types |
| `assets/data/survey/tiers.ron` | Per-dimension tier semantics (what tier 3 of "subsurface" means exactly) | 8 dimensions × 6 tiers = 48 rows |
| `assets/data/survey/mining_efficiency.ron` | Per-(resource class, dimension, tier) mining efficiency ramp; gates when mining is unlocked and at what yield. Powers §[Resource Reveal Matrix]. | One row per (resource_class, dimension, min_tier) tuple; ~24 rows for the default 4 resource classes × 6 tiers × the unlock thresholds. Modder-editable. |
| `assets/data/technologies.ron` (modified) | 9 new techs as per §Tech Tree Integration | 9 entries |

The schema for each file is fixed but the contents are entirely data. A modder can add a new dimension (e.g. "magnetosphere") by:
1. Adding one entry to `dimensions.ron`
2. Adding tier semantics to `tiers.ron` (6 rows)
3. Adding 1–2 instruments that advance the dimension to `instruments.ron`
4. (Optionally) adding an anomaly type to `anomalies.ron`

No Rust change. No recompile. RON modding is the player-influence path.

### Mining efficiency entries

`mining_efficiency.ron` is the data-driven form of §[Resource Reveal Matrix]. One row per `(resource_class, dimension, min_tier)` tuple; the Coder reads it at survey-state evaluation time to compute the player's actual mining yield. Suggested shape:

```ron
(
    id: "proven_crustal_min_deposits_t2",
    resource_class: "ShallowOre",        // v0.5.0 design slot — see note below
    dimension: "MineralDeposits",
    min_tier: 2,                         // mining unlocks at this dimension tier
    efficiency_pct: 40.0,                // 40% of nominal yield until tier 4
    requires_confirmation: false,        // tier 2 is "suspected"; no deposit-pin yet
),
(
    id: "proven_crustal_min_deposits_t4",
    resource_class: "ShallowOre",
    dimension: "MineralDeposits",
    min_tier: 4,
    efficiency_pct: 100.0,               // tier 4 unlocks full yield
    requires_confirmation: false,
),
(
    id: "deep_deposits_subsurface_t5",
    resource_class: "DeepOre",
    dimension: "SubsurfaceStructure",
    min_tier: 5,
    efficiency_pct: 100.0,
    requires_confirmation: true,         // mantle/core access requires drill rig
),
```

> **Note on `resource_class` (v0.5.0 aspirational):** the current `solar_system.ron` has no per-deposit `resource_class` tags — deposits are generated at runtime by `src/economy/generation.rs` and stored as the `ResourceReserve` triple `(proven_crustal, deep_deposits, planetary_bulk)` in `src/economy/components.rs:271`. There is no `ShallowOre` / `DeepOre` enum on deposits today. The `resource_class` field in this schema is a forward-looking tag for v0.5.0: when the Coder lands the new deposit model, deposit entries in `solar_system.ron` (or a new generated-deposit RON) will carry a `resource_class` tag, and this row keys the efficiency curve to that tag. Until that lands, the Coder can either (a) ignore `resource_class` and key efficiency on `(dimension, min_tier)` alone, or (b) define a small `ResourceClass` enum in `components.rs` and stub the deposit-side field. The schema in this doc is the v0.5.0 target shape.

Modders rebalance the curve by editing this file. The Coder exposes a `MiningEfficiencyRegistry` resource; the dossier UI looks up the relevant row to show the player the current `low / mid / high` estimate.

---

## Schema Delta (Rust Side)

A summary of the Rust changes needed. The Coder chooses the implementation details; the LGD rule is **additive, never breaking**.

### New components

| Component | File (suggested) | Purpose |
|-----------|------------------|---------|
| `SurveyState` | `src/economy/components.rs` (replaces `SurveyLevel`) | Per-body dimension-by-dimension state |
| `ActiveSurveyMission` | `src/economy/components.rs` | A running survey mission on a body |
| `Scientist` | `src/colony/components.rs` (or new `src/personnel/components.rs`) | A scientist entity |
| `AnalysisJob` | `src/economy/components.rs` | A pending data analysis job in the queue |
| `Anomaly` | `src/economy/components.rs` | A discovered anomaly on a body |

### New resources

| Resource | Purpose |
|----------|---------|
| `SurveyInstrumentRegistry` | Loaded once at startup from `instruments.ron` |
| `SurveyDimensionRegistry` | Loaded from `dimensions.ron` |
| `SurveyAnomalyRegistry` | Loaded from `anomalies.ron` |
| `SurveyMissionTemplates` | Loaded from `missions.ron` |
| `AnalysisQueueIndex` | Fast lookup of analysis jobs by scientist / by body |

### New systems

| System | Schedule | Purpose |
|--------|----------|---------|
| `advance_survey_missions` | `Update` | Tick active missions, advance state machines |
| `process_analysis_queue` | `Update` | Assign scientists, advance analysis jobs, flag anomalies |
| `decay_survey_confidence` | `SimulationTime` tick | Reduce confidence of stale dimensions |
| `surface_anomaly_events` | `Update` | Fire events for newly-detected anomalies |
| `update_survey_summary` | `Update` | Update system-wide survey % stat |
| `hire_scientists` | `Update` | University buildings produce scientists over time |
| `seniority_promotion` | `Update` | Promote scientists based on analysis success |

### Removed / deprecated

| Old | New |
|-----|-----|
| `SurveyLevel` enum | `SurveyState.dimensions` HashMap |
| `discovered_amount()` (level-based) | `discovered_amount_deposit()` (dimension-based) — old function kept as a 1:1 adapter for the migration shim |
| `mining_survey_filter` (level-based) | New filter based on `SurveyState.average_tier()` |

### Backward compat

A migration shim at startup maps:
- `SurveyLevel::Unsurveyed` → empty `SurveyState` (all dimensions at tier 0)
- `SurveyLevel::OrbitalScan` → OrbitalMech tier 1, Atmosphere tier 1, MineralClasses tier 1
- `SurveyLevel::SeismicSurvey` → all of the above + Subsurface tier 2
- `SurveyLevel::CoreSample` → all dimensions at tier 5

Earth's hardcoded `SurveyLevel::CoreSample` (`src/plugins/solar_system.rs:922`) becomes a `SurveyState::fully_surveyed()` factory call.

---

## Migration Plan

1. **Phase 1 (this design + Rust component scaffold):** Add the new `SurveyState` component, the registries, and the analysis queue. The old `SurveyLevel` enum is kept and its `discovered_amount()` is used in parallel with the new function. UI shows both the old and new state side-by-side for one release. No save breakage.
2. **Phase 2 (tech + instruments + missions):** Add the 9 new techs, the RON files, and the analysis queue UI. The new techs are gated by existing prerequisites; the new system is the only way to advance past `OrbitalScan` for new bodies.
3. **Phase 3 (personnel):** Add `Scientist` entities, the `University` building, and the analysis queue personnel interactions. The `Personnel` menu stub is filled out.
4. **Phase 4 (anomalies + events):** Add the anomaly RON file, the discovery event system, and the notification surface. Player gets their first "anomalous methane plume detected" event.
5. **Phase 5 (deprecate old):** Remove `SurveyLevel` enum from the code. The `discovered_amount()` 1:1 adapter is deleted. The mining panel and dashboard use the new state. Saves from v0.4.x load correctly via the migration shim.

Each phase is its own PR, gated on the previous phase's PR landing. The Coder can do Phase 1 immediately after this design is approved.

---

## Modder Surface

The whole system is RON-driven. The Coder's interface to a modder is the schema of these files. Concretely:

- A modder can add a new survey method (`MuonTomography`) by adding one entry to `methods` (in `missions.ron`) + a few entries to `instruments.ron` + a tier mapping in `tiers.ron`. No Rust change.
- A modder can add a new dimension (`Magnetosphere`) by adding one entry to `dimensions.ron` + tier rows in `tiers.ron` + instrument(s) that advance the dimension. The dimension shows up in the dossier automatically.
- A modder can add a new anomaly type by adding one entry to `anomalies.ron`. The event fires automatically.
- A modder can rebalance the accuracy/confidence curves by editing `tiers.ron` and `instruments.ron` accuracy tiers.
- A modder can add a new scientist specialty by adding one entry to the specialty enum in RON + a matching `specialty_multiplier` row. The hiring UI picks it up.

The LGD rule: **every gameplay-visible survey behavior is in RON**. Rust only loads the registries, runs the systems, and renders the UI.

---

## Test Plan

The Coder writes tests alongside each phase. The LGD list of tests that prove the design works:

| Test | What it proves |
|------|----------------|
| `survey_state_migration` | An old `SurveyLevel::CoreSample` body loads as a fully-surveyed `SurveyState` (all dimensions tier 5) |
| `survey_flyby_advances_dimensions` | Sending a flyby probe to a body advances OrbitalMech and Atmosphere to tier 1, leaves MineralDeposits at tier 0 |
| `survey_lander_does_not_advance_subsurface` | A surface lander does not advance Subsurface dimension past tier 0 |
| `analysis_queue_throughput` | 1 senior scientist processes a 412 GB dataset in the time predicted by the throughput formula |
| `confidence_decay` | After 5 sim-years without re-survey, a tier 4 dimension drops to confidence 0.6 |
| `scientist_specialty_multiplier` | A Geophysics senior gets 1.5× throughput on Subsurface jobs, 0.7× on MineralDeposits |
| `anomaly_discovery_event` | Running multispectral_scan over a body with hidden tholin_signature fires the discovery event and adds the anomaly to the dossier |
| `mining_efficiency_ramps` | A body with MineralDeposits tier 2 yields 40% of the deposit; tier 4 yields 100% |
| `ron_load_validation` | All five new RON files parse without error and the registries match the dimension count |
| `personnel_cap_soft` | Exceeding the soft personnel cap reduces per-scientist throughput by the expected 5% per scientist over cap |
| `modder_dimension_added` | Loading a mod that adds a "Magnetosphere" dimension does not break the loader; the dossier shows the new dimension as tier 0 |

The `cargo test` invocation is the authoritative check; the Coder chooses the test framework. Loader-side tests in `tests/survey_data_tests.rs`; runtime tests in the appropriate `src/economy/systems.rs` module.

---

## Out of Scope (v0.5.0)

The following are explicitly **not** in v0.5.0. They are noted so the design doesn't accidentally box them out.

- **Interstellar survey.** A different star system is opaque until v0.6+ (interstellar travel). The `interstellar_probe` tech is a future hook.
- **Active SETI / listening for signals.** The survey system characterizes bodies; it does not listen for extraterrestrial signals. (Future: §6.x.)
- **Reverse-engineering alien artifacts.** Same as above.
- **Cooperative multi-player survey.** The Personnel system is single-player (the player's scientists). Multi-player would be a §6.x feature.
- **Sample return from gas giants.** The pressure/temperature regime is incompatible with current probe architecture. Future: `gas_giant_atmospheric_probe` is its own tech arc.
- **Underground / submarine survey (Europa's ocean, Enceladus's ocean).** The `cryovolcanic_feature` anomaly is a hook for the interior-ocean research branch (a later v0.5.x sub-feature).
- **Real-time orbital mechanics integration.** The current game already propagates orbits; survey is a logical layer on top, not a physics one. A body can be "in survey range" without its current orbit being modeled. (Future hook: GRA-39 / logistics may want to compute time-to-flyby.)
- **In-game survey strategy tutorials.** The UI is intuitive enough that tutorials are deferred to a UX pass.

---

## Acceptance Criteria

The design is considered "landed" when:

- [ ] A new player opening the game can identify a body, see it is "Unsurveyed", and follow a multi-step plan (flyby → orbital → lander → rover) to characterize it over multiple in-game years.
- [ ] Each survey method requires a real instrument, costs real time, and advances specific dimensions (not "all dimensions").
- [ ] Scientists are hired from a University, assigned to analyses, and visibly speed up the queue. Without scientists, the queue is blocked.
- [ ] The `Survey %` stat in the system summary reflects the actual state, not a placeholder.
- [ ] At least one anomaly type triggers per body that the player fully surveys, with a real gameplay hook (research unlock, building unlock, or event).
- [ ] A modder adding a new survey dimension does not require a Rust recompile.
- [ ] Existing v0.4.x saves load without data loss; the body's old `SurveyLevel` maps to a `SurveyState` via the migration shim.

---

## Open Questions

These are flagged for the CEO/CTO before implementation starts:

1. **Where do the new RON files live?** `assets/data/survey/*.ron` (recommended, this design) or `assets/data/*.ron` flat? Subdir is cleaner but the existing project uses flat.
2. **Does the Personnel system stay under LGD or move to a new "Personnel Designer" role?** No such role exists yet; the LGD is a natural fit, but Personnel also touches training, recruitment, and (later) Generals / Governors. CEO call.
3. **What does the existing `Personnel` menu become in v0.5.0?** Currently a stub. Should it host Scientists only, or all personnel classes (scientists, generals, governors) in a unified roster? v0.5.0 is Scientists only; v0.5.x adds Generals/Governors. Recommendation: host Scientists only for v0.5.0, refactor the menu layout to make room for more.
4. **Should the analysis queue be its own panel, or part of the Personnel menu?** Recommendation: sub-panel of Personnel, but a dockable / pin-able window for power users.
5. **How does survey interact with v0.4.x logistics?** A survey mission needs a probe to be built and launched. The probe is a ship, the launch costs propellant, and the data downlink requires a comms relay. Is this in scope for the survey PR or assumed-pre-existing? (Assumption: the existing shipbuilding + fleet panel handles the "build and launch" side; the survey PR handles the "data flows and is processed" side.)
6. **Sample-return mission duration: 3–8 sim-years. Is that the right scale?** Real sample-return missions are 5–10 years. The numbers in this design are placeholders; the Coder can adjust with a data-driven rebalance pass.
7. **Anomaly coolness scoring: how does it surface in the game?** A `fame` or `reputation` resource? An event log entry? Both? Deferred to v0.5.x event system.

These are noted so the design isn't blocked on answering them; the Coder can stub answers and the LGD/CTO can refine.

---

## Why This Is Worth the Effort

Survey is the **first hour** of the game for any new player. It is also the **last hour** of a long campaign (anomalies drive late-game research arcs). Making it the most interesting part of the game is high-leverage.

The current 3-tier system is a placeholder that ships with v0.4.0 but does not earn the player's time. The proposed design:

- Reuses 7 existing techs as method gates (no tech-tree rewrite).
- Adds 9 new techs distributed across tiers 1–5 (a clean ~3% growth in the tech tree).
- Adds 5 new RON files (entirely data, modder-influenceable).
- Adds 1 new building type (University) and 1 new personnel class (Scientist).
- Replaces 1 enum (`SurveyLevel`) with 1 struct (`SurveyState`) and a HashMap of dimension states.
- Replaces 1 ad-hoc UI click (UPGRADE) with a multi-year, multi-instrument, personnel-driven campaign.

The investment is large but the payoff is that the game's most-frequently-clicked button becomes a real strategic decision. v0.5.0 ships with a survey system that is fun, deep, and faithful to the operator's brief: "make surveying very exciting and also realistic."
