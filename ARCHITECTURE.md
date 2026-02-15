# Helios Ascension - Architecture Documentation

## Overview
Helios Ascension is a Grand Strategy 4X game built with the Bevy Engine, featuring realistic orbital mechanics inspired by Aurora 4X and Terra Invicta. The project emphasizes high performance, modularity, and extensibility.

## Core Technologies
- **Game Engine**: Bevy 0.14 (ECS-based game engine)
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
- `Colony`: Colony data (name, population, stockpiles)
- `BuildingInventory`: List of constructed buildings per colony
- `ConstructionQueue`: Queue of buildings under construction

**Resources:**
- `BuildingsData`: Building definitions loaded from assets/data/buildings.ron
- `ConstructionDebugSettings`: Debug toggles (free construction, instant build, bypass tech)

**Systems:**
- `load_buildings`: Loads building definitions at startup
- `process_construction_actions`: Handles construction progress and completion
- `update_colony_resources`: Updates resource production/consumption
- `population_growth`: Simulates population changes

**Buildings (29 types in 8 categories):**
- Infrastructure: LifeSupport, HabitatDome, Housing, UndergroundHabitat
- Industry: Mine, Refinery, Factory, AtmosphericProcessor
- Logistics: MassDriver, OrbitalLift, CargoTerminal
- Power: SolarPower, FissionReactor, FusionReactor
- Population: AgriDome, Farm, MedicalCenter
- Research: ResearchLab, EngineeringBay, AiCluster
- Financial: CommercialHub, FinancialCenter, TradePort
- Military: Shipyard, MissileSilo, LaunchSite
- Advanced: DeepDrill, LaserDrill, StripMine, NeuralNetwork, OrbitalConstruction

#### 5. EconomyPlugin (`src/economy/`)
Handles resources, budgets, and energy systems.

**Components:**
- `PlanetResources`: Resource stockpiles and production rates per body
- `MineralDeposit`: Mineral deposits with accessibility and quantity
- `GlobalBudget`: Treasury, income, expenses
- `EnergyGrid`: Power generation and consumption

**Resources:**
- `ResourceTypes`: Defines 20 resource types (Iron, Copper, Water, Volatiles, Oxygen, Hydrogen, etc.)

**Systems:**
- `generate_mineral_deposits`: Creates procedural deposits on bodies at startup
- `mining_production`: Calculates resource extraction from mining buildings
- `update_budget`: Tracks financial flows
- `energy_management`: Balances power generation and consumption

**Resource Types (20):**
- Construction: Iron, Copper, Aluminum, Titanium, Silicates
- Volatiles: Water, Volatiles (general)
- Gases: Oxygen, Hydrogen, Nitrogen, CarbonDioxide, Helium, Methane
- Precious: Gold, Platinum
- Fissiles: Uranium, Thorium
- Specialty: RareEarths, Deuterium, Antimatter

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

#### 7. StarmapPlugin (`src/plugins/starmap.rs`)
Interstellar navigation and star system visualization.

**Components:**
- `StarIcon`: Visual representation of star systems in starmap view
- `SelectedStarSystem`: Marks selected star for detailed view
- `HoveredStarSystem`: Marks hovered star for tooltips

**Resources:**
- `NearbyStarsData`: ~1000 nearest stars with real astronomical data

**Systems:**
- `spawn_star_icons`: Creates icons for nearby star systems
- `update_star_icon_visibility`: Toggles visibility based on view mode
- `handle_starmap_selection`: Handles star system selection
- `handle_starmap_hover`: Detects mouse hover over stars

#### 8. UIPlugin (`src/ui/`)
Egui-based dashboard with time controls, body info, and resource display.

**Resources:**
- `SimulationTime`: Custom game clock (elapsed f64 seconds, no delta cap)
- `TimeScale`: Speed multiplier (1 day/s, 1 wk/s, 1 mo/s, 1 yr/s)
- `Selection`: Currently selected entity

**Panels:**
- Survey Panel: Body details, resources, population
- Construction Panel: Building management with queue
- Research Panel: Technology tree browser
- Economy Panel: Budget and resource tracking
- Starmap Panel: System selection and navigation
- Fleet Panel: Ship management (placeholder)
- Shipbuilding Panel: Vessel construction (placeholder)

**Key Design Decision — SimulationTime:**
- Bevy's `Time<Virtual>` caps delta at 250ms, limiting effective speed to ~15×.
- `SimulationTime` advances by `real_delta × time_scale` with no cap.
- All game-world systems MUST use `SimulationTime`, not `Time<Virtual>`.
- All calculations must be analytical (state from total time), not incremental.

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
2. **Combat System**: Space battles and defense
3. **Diplomacy**: AI factions and relations
4. **Terraforming**: Long-term planetary modification
5. **Advanced Ship Design**: Modular spacecraft construction

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
├── plugins/             # Game systems
│   ├── camera.rs        # Camera movement, anchoring & ViewMode
│   ├── solar_system.rs  # Body spawning, rotation, billboards
│   ├── solar_system_data.rs # RON data loader
│   ├── starmap.rs       # Starmap view (system icons, visibility toggle)
│   └── visual_effects.rs    # Bloom, starfield, night materials
├── render/              # Rendering utilities
│   └── backdrop.rs      # Skybox background
└── ui/                  # User interface
    ├── mod.rs           # UIPlugin, SimulationTime, TimeScale
    └── interaction.rs   # Selection management
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
