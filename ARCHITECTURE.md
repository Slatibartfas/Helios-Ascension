# Helios Ascension - Architecture Documentation

## Overview
Helios Ascension is a Grand Strategy 4X game built with the Bevy Engine, featuring realistic orbital mechanics. The project emphasizes high performance, modularity, and extensibility.

## Core Technologies
- **Game Engine**: Bevy 0.18 (ECS-based game engine)
- **Language**: Rust 2021 edition
- **Graphics**: 3D rendering with PBR materials
- **Debug Tools**: bevy_inspector_egui for runtime inspection

## Plugin Architecture

The game follows a modular plugin architecture where each major system is isolated into its own plugin:

### Current Plugins

#### 1. CameraPlugin (`src/plugins/camera.rs`)
Manages the game's 3D camera system with intuitive controls and view mode transitions.

**Components:**
- `GameCamera`: Stores camera movement and zoom speeds

**Resources:**
- `ViewMode`: Tracks the current view (`System` or `Starmap`), driven by zoom level

**Systems:**
- `spawn_camera`: Initializes the 3D camera at startup
- `orbit_camera_controls`: Handles right-click rotation and mouse wheel zoom
- `update_camera_transform`: Positions camera relative to anchor target
- `update_view_mode`: Switches between System and Starmap views based on zoom radius

**Features:**
- Right-click mouse look
- Mouse wheel zoom (up to ~333 AU from anchor)
- Automatic System ↔ Starmap transition at ~100 AU with hysteresis
- Configurable speeds

#### 2. SolarSystemPlugin (`src/plugins/solar_system.rs`)
Simulates celestial bodies and their orbital mechanics.

**Components:**
- `CelestialBody`: Basic properties (name, radius, mass, body_type, visual_radius)
- `Star`, `Planet`, `Moon`, `DwarfPlanet`, `Asteroid`, `Comet`: Type markers
- `RotationSpeed`: Angular speed in radians/second (rotation computed analytically)
- `Billboard`: Marker for entities that always face the camera
- `LogicalParent`: Tracks hierarchical parent (e.g., moons -> planet)

**Systems:**
- `setup_solar_system`: Creates 377+ celestial bodies from RON data at startup
- `rotate_bodies`: Analytical body rotation from `SimulationTime` (angle = speed × t)
- `update_billboards`: Keeps glow/flare quads facing the camera

#### 3. AstronomyPlugin (`src/astronomy/`)
High-precision Keplerian orbital mechanics with f64 coordinates.

**Components:**
- `SpaceCoordinates`: Double-precision (DVec3) position in AU
- `KeplerOrbit`: Full Keplerian elements (e, a, i, Ω, ω, M₀, n)
- `OrbitPath`: Orbit trail rendering configuration
- `Selected`, `Hovered`: Interaction markers

**Systems:**
- `propagate_orbits`: Analytical position from `SimulationTime` (M = M₀ + n·t)
- `update_render_transform`: Floating-origin conversion (DVec3 → Vec3 with scaling)
- `draw_orbit_paths`: Trail rendering with true-anomaly sampling
- `handle_body_selection`, `handle_body_hover`: Click/hover detection

#### 4. ColonyPlugin (`src/colony/`)
Manages colonies, buildings, and construction.

**Components:**
- `Colony`: Colony data (name, population, housing capacity, food balance, growth-rate modifier)
- `ColonyEnvironmentCosts`: Per-tick O₂ and Water drain for outpost colonies (non-breathable/hostile worlds)
- `BuildingInventory`: List of constructed buildings per colony
- `ConstructionQueue`: Queue of buildings under construction
- `ConstructionProject`: Per-entity in-progress build with remaining BP

**Resources:**
- `BuildingsData`: Building definitions loaded from `assets/data/buildings.ron`
- `PendingConstructionActions`: Thread-safe action queue (start-construction, establish-outpost)
- `ConstructionDebugSettings`: Debug toggles (free construction, instant build, bypass tech)

**Key systems:**
- `load_buildings`: Loads building definitions at startup
- `process_construction_actions`: Handles construction progress and completion
- `deduct_environment_costs`: Drains O₂/Water from outpost stockpile each tick
- `update_colony_resources`: Updates resource production/consumption
- `population_growth`: Simulates population changes using food-factor × housing-utilisation × logistics

**Buildings (52 types across 8 categories):**
- **Infrastructure**: Housing, HabitatDome, UndergroundHabitat, LifeSupport, WaterTreatmentPlant, DesalinationPlant, RecyclingCenter
- **Industry**: Mine, Refinery, Factory, AtmosphericProcessor, ChemicalPlant, HydrocarbonExtractor, DeepDrill, LaserDrill, StripMine, SemiconductorFab, PharmaceuticalPlant
- **Logistics**: MassDriver, OrbitalLift, CargoTerminal, Warehouse
- **Power**: SolarPower, WindFarm, HydroelectricDam, GeothermalPlant, CoalPowerPlant, NaturalGasPlant, FissionReactor, FusionReactor, DTFusionReactor, DHe3FusionReactor, ThoriumReactor, BreederReactor
- **Population**: AgriDome, Farm, Greenhouse, AquacultureFacility, MedicalCenter, PharmaceuticalPlant
- **Research**: ResearchLab, EngineeringBay, AiCluster, DataCenter, OrbitalSurveyStation
- **Financial**: CommercialHub, FinancialCenter, TradePort
- **Military**: Shipyard, MissileSilo, LaunchSite, SpacePort, GroundDefenseBattery

**Building output scale** (district-level, not single structure):
- Housing: 25M capacity/building; HabitatDome: 50M; UndergroundHabitat: 30M
- Farm: 1,000 Mt/yr food (~10M people); AgriDome: 4 Mt/yr; Greenhouse: 500 Mt/yr; Aquaculture: 750 Mt/yr
- Each new building is a perceptible ~0.3% improvement so construction remains meaningful

**Outpost founding flow** (`EstablishOutpostRequest`):
1. Player clicks "🏗 Establish Outpost" in the dossier panel (Survey tab)
2. Hard blocks: gas giants and gravity > 3 g are suppressed; amber warning at cost > 7/10
3. `needs_oxygen` derived from `AtmosphereComposition.breathable`
4. `ColonyEnvironmentCosts` attached: Water = 0.00005 Mt/person/yr always; Oxygen = 0.0001 Mt/person/yr when `needs_oxygen`
5. Starter buildings queued: LifeSupport, Housing ×1, FissionReactor ×2, AgriDome ×2
6. (v0.3) Construction draws from same-system `ContextualStockpile` pool; (v0.4+) will require `ResourceRequest` delivery to local stockpile

**Planned: Logistics Network (v0.4+):**
- `ResourceRequest` — published when construction needs materials not locally available; priority: Emergency > Construction > Maintenance > Trade
- `MinimumStockpile` (per-body) — player-configured per-resource thresholds; auto-creates Maintenance requests when below threshold
- `ShippingCompany` (Resource) — AI-controlled private companies that bid on requests, earn credits, buy more ships at shipyards
- `ContextualStockpile` is retained **display-only**; construction will read local `LocalStockpile` only
- See `docs/design/LOGISTICS_NETWORK.md` for the full design specification.

#### 4a. Resource Locality & Logistics (v0.4.x, shipped)

Resources in Helios are physical. They live on a specific body and must be transported by ship to be used elsewhere. The model has three layers:

- **`LocalStockpile`** (`src/economy/components.rs:501`) is a `Component` on every body that produces, stores, or consumes resources. It is a `HashMap<ResourceType, f64>` in megatonnes; production deposits here, consumption deducts here. Construction draws **only** from the destination body's `LocalStockpile`; if local materials are short, `process_construction_actions` (`src/colony/systems.rs:140`) publishes a `ResourceRequest` and sets `ConstructionProject::awaiting_resources = true` until delivery arrives.
- **`ContextualStockpile`** (`src/economy/budget.rs:506`) is a view-scoped `Resource` aggregating `LocalStockpile`s for the player UI. The `update_contextual_stockpile` system reads `ViewMode` and `CurrentStarSystem` and sums: in **System view**, every body in the active star system; in **Starmap view**, every body across all systems. The label (`"Sol System"` vs `"All Systems"`) is set on the resource and surfaced in the top resource bar marquee (`src/ui/resources_bar.rs:1196`). Construction does **not** read this — it is display-only.
- **Request / delivery flow.** When a body needs materials it cannot produce locally, a `ResourceRequest` is created in `PendingResourceRequests` (ECS `Resource`; see `src/economy/logistics.rs`). Requests carry a `RequestPriority` (`Emergency` > `Construction` > `Maintenance` > `Trade`) and a `RequestState` (`Pending` → `Assigned` → `InTransit` → `Delivered`, or `Expired`). Triggers: construction that exceeds local stock, `MinimumStockpile` thresholds falling below their configured level, and life-support shortfalls (Emergency). Delivery is performed either by a private `ShippingCompany` AI (`src/economy/company.rs`) or by a player-assigned Freighter fleet from the Fleet panel. The `complete_deliveries` system credits the destination `LocalStockpile` and unblocks the linked `ConstructionProject` when all linked requests are `Delivered`.

UI surfaces: the construction menu shows "⏳ Awaiting resources" / "⏳ Waiting for freighter" badges per project (`src/ui/construction/queue.rs` + `src/ui/construction/tooltip.rs`); the top resource bar reads the `ContextualStockpile` for the current view and switches its label between `"Sol System"` and `"All Systems"` accordingly; the Economy panel's Logistics tab lists open requests, company registry, and recent deliveries (`src/ui/economy_panel.rs:3551`).

See `docs/design/LOGISTICS_NETWORK.md` for the full design specification.

> See `docs/COLONIES.md` for the player-facing guide.

#### 5. EconomyPlugin (`src/economy/`)
Handles resources, budgets, and energy systems.

**Components:**
- `PlanetResources`: Resource stockpiles and production rates per body
- `MineralDeposit`: Mineral deposits with accessibility and quantity
- `GlobalBudget`: Treasury, income, expenses
- `EnergyGrid`: Power generation and consumption

**Resources:**
- `ResourceType`: Enum defining 38 resource types (Volatiles: Water, Hydrogen, Ammonia, Methane, Phosphorus, Food; Atmospheric gases: Nitrogen, Oxygen, CarbonDioxide, Argon; Construction metals: Iron, Aluminum, Titanium, Silicates, Nickel, Tungsten, Carbon, Chromium, Magnesium; Fusion fuels: Helium3, Deuterium, Tritium; Fissiles: Uranium, Thorium, Plutonium; Precious metals: Gold, Silver, Platinum; Specialty: Copper, RareEarths, Lithium, Sulfur, Cobalt, Fluorine, Polymers; Late-game: Antimatter, ExoticMatter, Metamaterials, Computronium)

**Systems:**
- `generate_mineral_deposits`: Creates procedural deposits on bodies at startup
- `mining_production`: Calculates resource extraction from mining buildings
- `update_budget`: Tracks financial flows
- `energy_management`: Balances power generation and consumption

**Resource Types (38, defined in `economy::types::ResourceType`):**
- Volatiles: Water, Hydrogen, Ammonia, Methane, Phosphorus, Food
- Atmospheric Gases: Nitrogen, Oxygen, CarbonDioxide, Argon
- Construction Materials: Iron, Aluminum, Titanium, Silicates, Nickel, Tungsten, Carbon, Chromium, Magnesium
- Fusion Fuels: Helium3, Deuterium, Tritium
- Fissiles: Uranium, Thorium, Plutonium
- Precious Metals: Gold, Silver, Platinum
- Specialty Materials: Copper, RareEarths, Lithium, Sulfur, Cobalt, Fluorine, Polymers
- Late-game: Antimatter, ExoticMatter, Metamaterials, Computronium

#### 6. ResearchPlugin (`src/research/`)
Technology progression and engineering projects.

**Components:**
- `TechnologyProgress`: Tracks research progress per technology
- `EngineeringProject`: Component design projects

**Resources:**
- `TechTree`: Complete technology tree loaded from assets/data/technologies.ron
- `ResearchDebugSettings`: Debug toggles for instant research

**Systems:**
- `load_tech_tree`: Loads technology definitions at startup
- `advance_research`: Progresses active research projects with RP
- `unlock_technologies`: Applies tech unlocks (buildings, modifiers)
- `apply_tech_modifiers`: Applies bonuses from completed techs

**Technology Categories (15):**
Electronics, Military, SpaceTechnology, Biology, Physics, Energy, Sociology, Construction, Propulsion, Materials, Sensors, Weapons, DefensiveSystems, LifeSupport, Industry

#### ShipbuildingPlugin (`src/shipbuilding/`)
Modular ship and station design, construction queues, refit, slipways, and the first slice of launch-aware production flow. The plugin follows the standard `mod.rs` + `data.rs` + `systems.rs` + `components.rs` + `types.rs` layout, with `refit.rs` and `slipway.rs` added for the design-upgrade and construction-capacity sub-domains.

**Components:**
- `ShipConstructionProject`: Active ship or station project with build progress, selected construction mode, and resulting launch/orbit state
- `OrbitalStation`: Marker reserved for immobile station assets once completed projects spawn into the fleet layer
- Design drafts, refit state, and slipway state (see `components.rs` for the full list)

**Resources:**
- `ShipbuildingData`: Hull and module definitions loaded from `assets/data/ship_hulls.ron` and `assets/data/ship_modules.ron`
- `PendingShipbuildingActions`: Thread-safe action queue for project creation and cancellation

**Systems:**
- `load_shipbuilding_data`: Loads hull and module definitions at startup
- `process_pending_shipbuilding_actions`: Validates selected hull/module combinations against research unlocks and local facilities
- `advance_ship_construction`: Advances queued projects using shipyard throughput, then transitions them to `ReadyForLaunch` or `CompletedInOrbit`
- Refit systems (`refit.rs`): design upgrade, technology gating, and module replacement
- Slipway systems (`slipway.rs`): construction capacity, throughput, and queue helpers

**Sub-module map:**
- `mod.rs` — `ShipbuildingPlugin` and the system set chain
- `components.rs` — ship-design / construction-project resources and design drafts
- `data.rs` — RON loading for `assets/data/ship_hulls.ron` and `assets/data/ship_modules.ron`, design summaries
- `systems.rs` — queue validation, shipyard throughput progression, project lifecycle
- `types.rs` — `ShipModuleCategory` (21-variant consolidated + legacy taxonomy), `HullSizeTier`, `ConstructionMode`, `ShipDesignTemplate`
- `refit.rs` — design upgrade / refit logic (technology gating, module replacement)
- `slipway.rs` — construction capacity and slipway helpers

**Current scope:**
- Data-driven hulls and module families for the first shipbuilder slice
- In-game Shipbuilding panel with modular slot selection, design summaries, and queue inspection
- Surface build and orbital assembly project progression from colony shipyards
- True launch execution, logistics-blocked inputs, and station-hosted orbital yards are the next implementation layers

#### 7. StarmapPlugin (`src/plugins/starmap.rs`)
Interstellar navigation and star system visualization.

**Components:**
- `StarIcon`: Visual representation of star systems in starmap view
- `SelectedStarSystem`: Marks selected star for detailed view
- `HoveredStarSystem`: Marks hovered star for tooltips

**Resources:**
- `NearbyStarsData`: 60 nearest star systems with real astronomical data

**Systems:**
- `spawn_star_icons`: Creates icons for nearby star systems
- `update_star_icon_visibility`: Toggles visibility based on view mode
- `handle_starmap_selection`: Handles star system selection
- `handle_starmap_hover`: Detects mouse hover over stars

#### 8. UIPlugin (`src/ui/`)
Egui-based dashboard with time controls, body info, and resource display. The module is split into focused sub-files to keep each file manageable.

**Sub-modules:**
- `time.rs`: `SimulationTime` custom clock, `TimeScale` multiplier, time-formatting helpers
- `icons.rs`: `MenuIcons` and `ResearchIcons` texture handles, icon load/process systems
- `resource_icons.rs`: Build / mining / research icon atlas + per-frame-bounded async bake (the canonical `MAX_ICONS_PER_FRAME = 2` pattern)
- `icon_cache.rs`: Hash-validated icon cache, async baking, splash/boot progress reporting
- `widgets.rs`: Menu-agnostic native-Bevy-UI primitive library — `UiFonts`, `HoverElevation`, `KeyedList`, `TooltipRequest`, `Scrollbar`, `Marquee`, `ProgressFill`, `ActiveTabs<T>`, six `CardShell*` composers; consumable by any future bevy_ui menu
- `bevy_theme.rs`: Native-Bevy-UI palette mirror (`CARD_BG`, `CYAN`, `CARD_BORDER`, shadow styles) — coexists with `theme.rs` (the egui palette)
- `resources_bar.rs`: Top resource bar rendering (~1 000 lines)
- `dashboard.rs`: Main survey panel, time controls bar, star system detail panel
- `research_panel.rs`: Full research/engineering UI including interactive tech tree
- `construction/`: **Native Bevy UI Construction menu** (v0.5.2 canary — split from the legacy 11 865-LOC `src/ui/construction.rs`, refactored out into a 15-file directory). Sub-modules: `state.rs`, `data.rs`, `markers.rs`, `cards.rs`, `mining.rs`, `overview.rs`, `buildings.rs`, `demolish.rs`, `queue.rs`, `dropdown.rs`, `tooltip.rs`, `scrollbar.rs`, `disabled.rs`, `setup.rs`, `mod.rs`
- `shipbuilding_workspace.rs`: Native Bevy UI shipbuilding workspace (Logistics Hub, Design Blueprint, Engineering Analytics; hull design, module selection, ship/station build queues)
- `shipbuilding_state.rs`: Shared shipbuilding UI state (selected hull, focused slot, queued builds) consumed by the workspace and the resource-side construction systems. The legacy `shipbuilding_tooltip.rs` was inlined into `shipbuilding_workspace.rs` in commit `17472dd` — slot hover tooltips and module-compatibility hints now live there.
- `economy_panel.rs`: Economy overview, per-resource rates, colony/mining/power tabs
- `fleets_panel.rs`: Fleet list, detail view, `FleetUiState`, transfer planner, LP transfers
- `porkchop_panel.rs`, `transfer_planner.rs`, `transfer_planner_card.rs`: Transfer planner UI (porkchop plot, Lagrange routing)
- `launch/`: Splash, main-menu, sub-views (New Game, Load Game, Save Game, Settings), save index, userdata persistence, menu backdrop
- `notifications/`: Toast panel, per-category settings, event bridges, click-to-focus
- `personnel_panel.rs`: Personnel Roster UI (v0.5.0 data layer; scientists, hiring, seniority/specialty display)
- `interaction.rs`: `Selection` resource, body selection helpers
- `mod.rs`: `UIPlugin`, shared constants, overlay systems (tooltips, starmap labels), re-exports

**Resources:**
- `SimulationTime`: Custom game clock (elapsed f64 seconds, no delta cap)
- `TimeScale`: Speed multiplier (1 day/s, 1 wk/s, 1 mo/s, 1 yr/s)
- `Selection`: Currently selected entity
- `FleetUiState`: Per-frame fleet panel state — selected fleet, planned transfer, target body, gravity-assist candidates

**Panels:**
- Survey Panel: Body details, resources, population
- Construction Panel: Building management with queue
- Research Panel: Technology tree browser
- Economy Panel: Budget and resource tracking
- Starmap Panel: System selection and navigation
- Fleet Panel: Full fleet management — spawn, transfer planning, transfer-window countdown, gravity-assist routing, Lagrange-point targeting, intercept planning, refuel, abort
- Shipbuilding Panel: Initial modular design and queue management for ships and stations

**Key Design Decision — SimulationTime:**
- Bevy's `Time<Virtual>` caps delta at 250ms, limiting effective speed to ~15×.
- `SimulationTime` advances by `real_delta × time_scale` with no cap.
- All game-world systems MUST use `SimulationTime`, not `Time<Virtual>`.
- All calculations must be analytical (state from total time), not incremental.

#### 9. FleetPlugin (`src/fleets/`)
Fleet spawning, orbital transfer planning, trajectory propagation, and visualisation.

**Components:**
- `Fleet`: Named collection of ships with mass, Isp, thrust, and fuel properties
- `ShipInfo`: Per-ship data (class, dry mass, fuel, thrust, Isp, propulsion type)
- `FleetOrbit`: Stable circular parking orbit tracked per frame for a fleet
- `ActiveManeuver`: Keplerian transfer arc being executed — drives position analytically from `SimulationTime`
- `PlannedTransfer`: A fully computed transfer ready for execution

**Resources:**
- `PendingFleetActions`: Thread-safe action queue: spawn, start-transfer, cancel, refuel requests
- `FleetUiState` (in `ui`): Selected fleet, target body/Lagrange, computed transfer options, transfer-window data

**Systems:**
- `spawn_initial_fleet`: Creates Earth-orbit frigate fleet at game start
- `process_fleet_actions`: Consumes `PendingFleetActions` — spawns/refuels/cancels
- `update_fleet_orbit_positions`: Advances parking-orbit angle at visual rate (1 rev / 40 s real time)
- `update_fleet_maneuver_positions`: Propagates transfer-arc position analytically each frame
- `complete_fleet_maneuvers`: Transitions fleet from `ActiveManeuver` → `FleetOrbit` at arrival
- `draw_fleet_trajectories`, `draw_fleet_icons`, `draw_fleet_orbit_rings`, `draw_fleet_selection_reticule`: Gizmo-based visualisation
- `draw_fleet_transfer_preview`, `draw_gravity_assist_preview`: Preview arcs before committing a transfer
- `draw_fleet_starmap_icons`: Fleet markers in starmap view
- `ensure_fleet_meshes`, `update_fleet_transforms`: Entity transform upkeep

**Ship Classes (7):**
Courier, Frigate, Destroyer, Cruiser, ResearchVessel, Freighter, Station

**Propulsion Types (6):**
- Chemical (450 s Isp) — high thrust, low efficiency
- NuclearThermal (900 s Isp)
- IonDrive (5 000 s Isp) — low thrust, very high efficiency
- NuclearPulse (10 000 s Isp)
- FusionTorch (50 000 s Isp) — high thrust and high efficiency
- AntimatterDrive (1 000 000 s Isp) — late-game, requires antimatter fuel

**Porkchop Plot Planner (`src/fleets/porkchop.rs`, GRA-152):**
- Δv / phase-angle grid computed from synodic-period sweep of departure & arrival windows
- Interactive cursor with hover & selection wired to `FleetUiState.porkchop_grid`
- Deferred-build cache so non-click entries can build the grid on demand
- Now + synodic-period anchor for time-progressive updates

**Lagrange-Point Transfers (`src/astronomy/lagrange.rs` + `src/fleets/`):**
- L4 / L5 for any planet; L1 / L2 / L3 for Sun–planet pairs
- Destination state-mutation contract (GRA-160) — single Hohmann fallback when L4 is empty
- Interactive star-approach parking-radius picker (GRA-161) for cross-system transfers

**Orbital Mechanics (`src/fleets/orbital_mechanics.rs`):**
- `hohmann_transfer()`: Minimum-energy co-planar transfer between circular orbits
- `calculate_transfer_options()`: Returns 3 transfer options (efficient / moderate / fast)
- `calculate_transfer_options_phased()`: Same but departing after a player-chosen offset (days)
- `compute_transfer_window()`: Synodic-period countdown and live phase-angle rate
- `estimate_fuel_cost_tonnes()`: Tsiolkovsky rocket-equation fuel estimate
- `GravityAssistOption`: Represents a flyby body that bends the trajectory, reducing Δv

**Transfer Window Planning:**
- `TransferWindowInfo` computed every frame: seconds to next optimal Hohmann window, synodic period, current phase-angle error, phase-rate (rad/s)
- Player can slide the departure-time offset to align with the window
- Gravity-assist candidates automatically surfaced in the UI for heliocentric transfers
- Lagrange-point targets (L1/L2/L3 for Sun-Earth, L4/L5 for any planet)

### Custom Start Dates & Ephemeris (New)
- The project includes an **ephemeris module** (`src/astronomy/ephemeris.rs`) capable of calculating mean anomalies (orbital positions) for planets, moons, and dwarf planets at any Unix timestamp using J2000-based elements.
- To support custom game start dates, create a `SimulationTime` with `SimulationTime::with_start_timestamp(start_timestamp)` where `start_timestamp` is a Unix timestamp for the desired start date.
- Immediately after creating the world (or during world initialization), call `calculate_positions_at_timestamp(start_timestamp)` to compute mean anomalies for all bodies and use the returned values to set the Keplerian `mean_anomaly_epoch` (or `initial_angle` in degree form) for each celestial body before spawning them.
- This ensures that the visual and simulated positions of bodies match the chosen start date and remain analytically correct as the simulation advances.

Example (conceptual):
```rust
use helios_ascension::astronomy::{calculate_positions_at_timestamp};
use helios_ascension::ui::SimulationTime;

let start_ts = 1_767_225_600; // Jan 1, 2026 00:00:00 UTC
let sim_time = SimulationTime::with_start_timestamp(start_ts);
let positions = calculate_positions_at_timestamp(start_ts);

// For each CelestialBodyData loaded from RON, override its orbit.initial_angle
// with `positions.get(&body_name)` (degrees → radians) and then spawn.
```

*Note:* The current implementation uses simplified moon/dwarf-planet models; for higher precision use JPL Horizons data and expand the ephemeris module accordingly.

#### Exoplanets & Nearby Stars (v0.4.x → v0.6)

`src/astronomy/exoplanets.rs` defines the `ConfirmedPlanet` data model and `RealPlanet` marker component but **the NASA Exoplanet Archive CSV loader is not yet wired up**. The `assets/data/Exoplanets_NASA.csv` dump is untracked (see `assets/data/README.md`) — the structs and tests are in place, ready for the v0.6 interstellar travel milestone. `src/astronomy/nearby_stars.rs` provides the 60+ nearest star systems from `assets/data/nearest_stars_raw.json`. Systems without confirmed planets fall back to `src/astronomy/procedural.rs` today; the CSV-driven path ships with v0.6. The `interstellar_probe` tech (tier 5, added in GRA-106) unlocks flyby of bodies in other star systems.

#### SurveyPlugin (`src/survey/`, v0.5.0)

Replaces the legacy 3-tier `SurveyLevel` enum with an eight-dimension discovery model. Sub-modules: `components.rs` (per-body `SurveyState`), `data.rs` (RON loader for the 6 `assets/data/survey/*.ron` files), `systems.rs` (mission dispatch, anomaly detection, recovery missions, continuous-orbital-station yield bonus), `events.rs` (mission lifecycle events), `visibility.rs` (UI surfacing), `types.rs` (dimension / instrument / mission / anomaly types).

**State model** — `SurveyState` on every body, with eight dimensions (Orbital mechanics, Atmosphere, Surface features, Mineral classes, Mineral deposits, Subsurface structure, Habitability, Anomalies). Each dimension has a tier (0–5) and a confidence score (0.0–1.0). Confidence decays 0.5% per sim-year without new measurements.

**Mission lifecycle** — Flyby, Orbital satellite, Remote sensing pass, Atmospheric probe, Surface lander, Rover, Seismic survey, Drill core sample, Sample return. Each mission is dispatched from the dossier SURVEY tab, runs against a target body, and progresses over sim-years with mission-gate hazards (GRA-120).

**Anomalies** — 9 hardcoded anomaly types + a `ModderAnomalyDef` RON path. Each has a `coolness` weight (independent of gameplay effect), a 10–60 sim-day follow-up timer, and a confidence model with verification. Anomalies trigger follow-up missions, research projects, or building unlocks.

**Failure modes & recovery** (PR-G / GRA-85) — Probe loss (5%), Rover stuck (8%), Drill bit stuck (10%), Solar storm (2%), Crew injury (2%). Each failure mode spawns a recovery mission template in `recovery_missions.ron`.

**Continuous orbital survey station** (PR-E / GRA-83) — `OrbitalSurveyStation` building provides per-body 5/10/15% mining-yield bonus at survey tier 1/2/3.

**Landing sites** (PR-D / GRA-82) — Tier 2+ on Surface features AND Mineral deposits unlocks per-site evaluation: latitude / longitude, terrain rating (slope, regolith, radiation), resource estimate triplet, risk profile.

#### PersonnelPlugin (`src/personnel/`, v0.5.0)

**Data layer (shipped):** `Scientist` component with **8 specialties** (Geology, Atmospherics, Biology, Geophysics, …) and **3 seniority tiers** (Junior, Senior, Principal). `hire_scientists` system runs from `University` buildings. `seniority_promotion` system upgrades scientists as they complete analysis jobs. Specialty → analysis multiplier (matched ×1.5, mismatched ×0.7); seniority → throughput and quality (Junior 1.0× / 0.8×, Senior 1.5× / 1.0×, Principal 2.0× / 1.2× + 10% anomaly find).

**UI layer (pending):** `GameMenu::Personnel` is a stub at `src/game_state.rs`; the `Personnel` menu is filled out as the scientist roster & assignments panel — the design contract is in `docs/UI.md` §8.3 (Preview).

**Cap model:** soft cap gated by tech (`scientific_administration`) — early game 3 scientists, mid game 20, late game 200. Exceeding the cap applies a 5%-per-scientist penalty.

#### NotificationsPlugin (`src/ui/notifications/`, v0.5.0)

Player-facing event bus. Sub-modules: `components.rs` (per-toast `ActiveNotification`, action queues), `data.rs` (RON loader for `assets/data/notifications.ron`), `events.rs` (`NotificationEvent` Bevy `Message` + `NotificationContextLink`), `settings.rs` (`NotificationSettings` per-category overrides), `ui_settings.rs` (settings modal), `systems/` (tick / coalesce / click_handler / render / bridges).

**Flow:** Survey / Construction / Research bridges write to `Messages<NotificationEvent>` → spawn system attaches an `ActiveNotification` → tick system auto-dismisses / pauses → coalesce system deduplicates within a 2 s window (PR-D / GRA-138) → click_handler dispatches to the context (PR-G / GRA-141) → settings panel (PR-E / GRA-139) per-category overrides.

## ECS Architecture

The game uses Bevy's Entity Component System (ECS) architecture:

### Entities
Game objects (cameras, planets, stars, etc.)

### Components
Data attached to entities:
- Transform, mesh, material (Bevy built-ins)
- CelestialBody, OrbitalPath, GameCamera (custom)

### Systems
Functions that operate on entities with specific components:
- Run in parallel when possible
- Organized by plugin
- Execute in defined schedules (Startup, Update, etc.)

## Performance Optimizations

### Compile-Time
- **Development Profile**: Fast compilation with opt-level 1 for dependencies
- **Release Profile**: LTO + single codegen unit for maximum performance
- **Fast Profile**: Quick iteration with minimal optimizations

### Runtime
- ECS parallelization
- Efficient state management
- Minimal allocations

## Future Architecture Plans

### Upcoming Features
1. **Interstellar Travel (v0.6)**: Ship movement between star systems; the `interstellar_probe` tech (GRA-106) is already on `main`. The exoplanet ingestion path in `src/astronomy/exoplanets.rs` is staged (structs + tests in place, CSV loader pending) — see `assets/data/README.md` for the deferred data source.
2. **Combat System (v0.6 → v0.7)**: Space battles and defense; the `GroundDefenseBattery` and `MissileSilo` buildings are scaffolded for ground-side combat
3. **Diplomacy & AI Factions (v0.6)**: Multi-faction competition, alliances, treaties, and victory conditions
4. **Terraforming (v0.7 → v0.8)**: Long-term planetary modification; hooks in `BuildingType::category()` and `OrbitalSurveyStation` continuous-yield bonus
5. **Save / Load (v1.0)**: Persistence layer; currently the game is single-session; B0001 postmortem (GRA-51) and the LocalStockpile data layer are pre-requisites
6. **Inter-system logistics (0.4.x → 0.5.x follow-up)**: Convoys between stars, capacity market, Mega/Gigaton freighter hulls — design spec at `docs/design/LOGISTICS_LATE_GAME.md` and `docs/design/MEGA_GIGATON_FREIGHTER_TIERS.md`

### Data-Driven Design
Future systems will use data files (RON/JSON) for configuration:
- Celestial body definitions
- Technology definitions
- Ship blueprints
- Resource types

## Code Organization

```
src/
├── main.rs              # Entry point, app setup
├── lib.rs               # Library root
├── game_state.rs        # Top-level state machine (GameMenu, AppState, Personnel menu stub)
├── astronomy/           # Orbital mechanics, ephemeris, exoplanets, nearby stars, Lagrange helpers
│   ├── components.rs    # SpaceCoordinates, KeplerOrbit, OrbitPath
│   ├── systems.rs       # Orbit propagation, rendering, selection
│   ├── ephemeris.rs     # J2000-based mean-anomaly computation for custom start dates
│   ├── exoplanets.rs    # ConfirmedPlanet struct + RealPlanet marker; CSV loader staged for v0.6
│   ├── nearby_stars.rs  # 60+ nearest star systems from `nearest_stars_raw.json`
│   ├── procedural.rs    # Procedural fallback for systems without confirmed planets
│   ├── lagrange.rs      # L1/L2/L3/L4/L5 computation helpers
│   ├── selection.rs     # Body click / hover / range-pick dispatch
│   └── mod.rs           # AstronomyPlugin
├── colony/              # Colony management, buildings, construction, founding flow
├── economy/             # Resources, budget, energy grid, logistics, AI shipping companies, mining
├── fleets/              # Fleet management, orbital mechanics, porkchop, Lagrange transfers
├── personnel/           # Scientists (v0.5.0 data layer; UI panel pending)
├── research/            # Technology tree, engineering, and unlock catalogs
├── shipbuilding/        # Data-driven hulls, modules, projects, refit, and slipways
├── ships/               # Hull templates, migration shims (legacy `standard_freighter`)
├── survey/              # v0.5.0 survey rework: 8-dimension state, missions, anomalies, instruments
├── plugins/             # Bevy plugin modules
│   ├── camera.rs        # Camera movement, anchoring & ViewMode
│   ├── solar_system.rs  # Body spawning, rotation, billboards
│   ├── solar_system_data.rs # RON data loader
│   ├── starmap.rs       # Starmap view (system icons, visibility toggle)
│   ├── atmosphere.rs    # Atmospheric scattering shader (Rayleigh + Mie)
│   ├── music.rs         # Background music playlist (AI-generated, MiniMax Music 3.0)
│   ├── comet_vfx.rs     # Comet tails
│   ├── ocean.rs         # Ocean material
│   ├── star_materials.rs # Stellar materials
│   ├── system_populator.rs # RON-driven body + economy + colony bootstrap
│   └── visual_effects.rs    # Bloom, starfield, night materials
├── render/              # Rendering utilities
└── ui/                  # All UI panels
    ├── mod.rs                 # UIPlugin, theme tokens, overlay systems, re-exports
    ├── time.rs                # SimulationTime, TimeScale, time helpers
    ├── theme.rs               # Color32 / spacing / focus-ring tokens (CI-linted)
    ├── icons.rs               # MenuIcons, ResearchIcons, icon loading/processing
    ├── resources_bar.rs       # Top resource bar UI with in-transit indicator
    ├── dashboard.rs           # Main dashboard, time controls, star system panel
    ├── dossier_panel.rs       # Per-body dossier (Survey + Construction + Resource ledger)
    ├── research_panel.rs      # Research/engineering UI and tech tree (egui)
    ├── construction/          # NATIVE BEVY UI Construction menu (v0.5.2 canary — split
    │                         # from the 11 865-LOC `src/ui/construction.rs` in commits
    │                         # `6e9e8f4` + `4794803`). Sub-modules:
    │                         #   mod.rs (re-exports), state.rs, data.rs, markers.rs,
    │                         #   cards.rs, mining.rs, overview.rs, buildings.rs,
    │                         #   demolish.rs, queue.rs, dropdown.rs, tooltip.rs,
    │                         #   scrollbar.rs, disabled.rs, setup.rs.
    │                         # Everything in src/ui/construction/ builds on the
    │                         # menu-agnostic primitive library in src/ui/widgets.rs.
    ├── widgets.rs             # Menu-agnostic native-Bevy-UI primitive library
    │                         # (v0.5.2; `UiFonts`, `HoverElevation`, `KeyedList`,
    │                         # `TooltipRequest`, `Scrollbar`, `Marquee`,
    │                         # `ProgressFill`, `ActiveTabs<T>`, six `CardShell*`
    │                         # composers). Consumed by Construction today and any
    │                         # future bevy_ui menu.
    ├── bevy_theme.rs          # Native-Bevy-UI palette mirror (`CARD_BG`, `CYAN`, etc.)
    │                         # — coexists with the egui palette in `theme.rs`.
    ├── economy_panel.rs       # Economy/budget UI + Logistics subpanel (egui)
    ├── fleets_panel.rs        # Fleet management, transfer planner, FleetUiState (egui)
    ├── transfer_planner.rs    # Transfer-window planner (Hohmann / moderate / fast)
    ├── transfer_planner_card.rs # Per-fleet transfer planner card UI
    ├── porkchop_panel.rs      # Porkchop plot (GRA-152) with interactive cursor
    ├── porkchop_color_ramp.rs # Porkchop Δv colour ramp helper
    ├── shipbuilding_workspace.rs  # Native Bevy UI shipbuilding workspace (slot hover
    │                         # tooltips + module-compatibility hints inlined here in
    │                         # commit `17472dd`; the legacy `shipbuilding_tooltip.rs`
    │                         # no longer exists).
    ├── shipbuilding_state.rs  # Shared shipbuilding UI state
    ├── notifications/         # v0.5.0 notifications (toast panel, settings, bridges)
    ├── launch/                # Splash, main-menu, sub-views (New Game / Load Game /
    │                         # Save Game / Settings), save index, userdata persistence,
    │                         # menu backdrop
    ├── settings.rs            # Top-menu settings + screenshot slots
    ├── cursors.rs             # Cursor sprite management
    ├── tab.rs                 # Tab-strip primitives (egui side, shared with widgets.rs)
    ├── tech_tree.rs           # Tech tree visualisation
    ├── resource_icons.rs      # Build / mining / research icon atlas + per-frame-bounded
    │                         # async bake (canonical `MAX_ICONS_PER_FRAME = 2` pattern)
    ├── icon_cache.rs          # Hash-validated icon cache, async baking, splash/boot progress
    ├── screenshot.rs / screenshot_state.rs # Shift+F12 screenshot capture
    ├── personnel_panel.rs     # Personnel Roster UI (v0.5.0; scientists, hiring, seniority)
    └── interaction.rs         # Selection management
```

## Adding New Plugins

To add a new plugin:

1. Create a new file in `src/plugins/`
2. Define your plugin struct implementing `Plugin` trait
3. Add components and systems
4. Export from `src/plugins/mod.rs`
5. Register in `main.rs`

Example:
```rust
pub struct MyPlugin;

impl Plugin for MyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_system)
           .add_systems(Update, update_system);
    }
}
```

## Debugging

The project includes bevy_inspector_egui which provides:
- Real-time entity inspection
- Component value editing
- Performance metrics
- Resource viewing

Access the inspector by running the game - it's visible by default in development builds.
