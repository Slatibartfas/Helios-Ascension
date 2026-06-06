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

**Buildings (47 types across 8 categories):**
- **Infrastructure**: Housing, HabitatDome, UndergroundHabitat, LifeSupport, WaterTreatmentPlant, DesalinationPlant, RecyclingCenter
- **Industry**: Mine, Refinery, Factory, AtmosphericProcessor, ChemicalPlant, HydrocarbonExtractor, DeepDrill, LaserDrill, StripMine, SemiconductorFab, PharmaceuticalPlant
- **Logistics**: MassDriver, OrbitalLift, CargoTerminal, Warehouse
- **Power**: SolarPower, WindFarm, HydroelectricDam, GeothermalPlant, CoalPowerPlant, NaturalGasPlant, FissionReactor, FusionReactor
- **Population**: AgriDome, Farm, Greenhouse, AquacultureFacility, MedicalCenter
- **Research**: ResearchLab, EngineeringBay, AiCluster, DataCenter
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

> See `docs/COLONIES.md` for the player-facing guide.

#### 5. EconomyPlugin (`src/economy/`)
Handles resources, budgets, and energy systems.

**Components:**
- `PlanetResources`: Resource stockpiles and production rates per body
- `MineralDeposit`: Mineral deposits with accessibility and quantity
- `GlobalBudget`: Treasury, income, expenses
- `EnergyGrid`: Power generation and consumption

**Resources:**
- `ResourceType`: Enum defining 20 resource types (Water, Hydrogen, Ammonia, Methane, Nitrogen, Oxygen, CarbonDioxide, Argon, Iron, Aluminum, Titanium, Silicates, Helium3, Uranium, Thorium, Gold, Silver, Platinum, Copper, RareEarths)

**Systems:**
- `generate_mineral_deposits`: Creates procedural deposits on bodies at startup
- `mining_production`: Calculates resource extraction from mining buildings
- `update_budget`: Tracks financial flows
- `energy_management`: Balances power generation and consumption

**Resource Types (20, defined in `economy::types::ResourceType`):**
- Volatiles: Water, Hydrogen, Ammonia, Methane
- Atmospheric Gases: Nitrogen, Oxygen, CarbonDioxide, Argon
- Construction Materials: Iron, Aluminum, Titanium, Silicates
- Fusion Fuel: Helium3
- Fissiles: Uranium, Thorium
- Precious Metals: Gold, Silver, Platinum
- Specialty Materials: Copper, RareEarths

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
- `resources_bar.rs`: Top resource bar rendering (~1 000 lines)
- `dashboard.rs`: Main survey panel, time controls bar, star system detail panel
- `research_panel.rs`: Full research/engineering UI including interactive tech tree
- `construction_panel.rs`: Construction queue panel
- `shipbuilding_workspace.rs`: Native Bevy UI shipbuilding workspace (Logistics Hub, Design Blueprint, Engineering Analytics; hull design, module selection, ship/station build queues)
- `shipbuilding_state.rs`: Shared shipbuilding UI state (selected hull, focused slot, queued builds) consumed by the workspace and the resource-side construction systems
- `shipbuilding_tooltip.rs`: Slot hover tooltips and module compatibility hints for the workspace
- `economy_panel.rs`: Economy overview, per-resource rates, colony/mining/power tabs
- `fleets_panel.rs`: Fleet list, detail view, `FleetUiState`, transfer planner, LP transfers
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

**Propulsion Types (5):**
- Chemical (450 s Isp) — high thrust, low efficiency
- NuclearThermal (900 s Isp)
- IonDrive (5 000 s Isp) — low thrust, very high efficiency
- NuclearPulse (10 000 s Isp)
- FusionTorch (50 000 s Isp) — high thrust and high efficiency

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
1. **Interstellar Travel**: Ship movement between star systems
2. **Ship Construction Pipeline**: Shipyard-to-fleet construction queue
3. **Combat System**: Space battles and defense
4. **Diplomacy**: AI factions and relations
5. **Terraforming**: Long-term planetary modification
6. **Advanced Ship Design**: Modular spacecraft construction

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
├── astronomy/           # Orbital mechanics & coordinate systems
│   ├── components.rs    # SpaceCoordinates, KeplerOrbit, OrbitPath
│   ├── systems.rs       # Orbit propagation, rendering, selection
│   └── mod.rs           # AstronomyPlugin
├── economy/             # Resource & budget systems
│   ├── components.rs    # PlanetResources, MineralDeposit
│   ├── budget.rs        # GlobalBudget, EnergyGrid
│   ├── generation.rs    # Procedural resource generation
│   └── types.rs         # ResourceType definitions
├── fleets/              # Fleet management & orbital transfer
│   ├── components.rs    # Fleet, FleetOrbit, ActiveManeuver, PlannedTransfer
│   ├── orbital_mechanics.rs # Hohmann transfers, transfer windows, gravity assists
│   ├── systems.rs       # Fleet position, maneuver execution, visualisation
│   ├── types.rs         # ShipClass, PropulsionType
│   └── mod.rs           # FleetPlugin
├── shipbuilding/        # Modular hull/module data, ship construction queues, refit, slipways
│   ├── components.rs    # ShipDesignDraft, ShipConstructionProject, pending actions
│   ├── data.rs          # Hull and module definitions + design summaries
│   ├── refit.rs         # Design upgrade and refit logic (technology gating, module replacement)
│   ├── slipway.rs       # Construction capacity and slipway helpers
│   ├── systems.rs       # Queue validation and shipyard throughput progression
│   ├── types.rs         # ShipModuleCategory (21 variants), HullSizeTier, ConstructionMode, ShipDesignTemplate
│   └── mod.rs           # ShipbuildingPlugin
├── plugins/             # Game systems
│   ├── camera.rs        # Camera movement, anchoring & ViewMode
│   ├── solar_system.rs  # Body spawning, rotation, billboards
│   ├── solar_system_data.rs # RON data loader
│   ├── starmap.rs       # Starmap view (system icons, visibility toggle)
│   └── visual_effects.rs    # Bloom, starfield, night materials
├── render/              # Rendering utilities
│   └── backdrop.rs      # Skybox background
└── ui/                  # User interface
    ├── mod.rs                 # UIPlugin, shared constants, overlay systems, re-exports
    ├── time.rs                # SimulationTime, TimeScale, time helpers
    ├── icons.rs               # MenuIcons, ResearchIcons, icon loading/processing
    ├── resources_bar.rs       # Top resource bar UI
    ├── dashboard.rs           # Main dashboard, time controls, star system panel
    ├── research_panel.rs      # Research/engineering UI and tech tree
    ├── construction_panel.rs  # Construction queue UI
    ├── economy_panel.rs       # Economy/budget UI
    ├── fleets_panel.rs        # Fleet management, transfer planner, FleetUiState
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
