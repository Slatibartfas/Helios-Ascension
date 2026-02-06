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
Manages the game's 3D camera system with intuitive controls.

**Components:**
- `GameCamera`: Stores camera movement and zoom speeds

**Systems:**
- `spawn_camera`: Initializes the 3D camera at startup
- `camera_movement`: Handles WASD/QE keyboard movement and mouse look
- `camera_zoom`: Handles mouse wheel zoom

**Features:**
- Smooth WASD movement
- Right-click mouse look
- Mouse wheel zoom
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

#### 4. UIPlugin (`src/ui/`)
Egui-based dashboard with time controls, body info, and resource display.

**Resources:**
- `SimulationTime`: Custom game clock (elapsed f64 seconds, no delta cap)
- `TimeScale`: Speed multiplier (1 day/s, 1 wk/s, 1 mo/s, 1 yr/s)
- `Selection`: Currently selected entity

**Key Design Decision — SimulationTime:**
- Bevy's `Time<Virtual>` caps delta at 250ms, limiting effective speed to ~15×.
- `SimulationTime` advances by `real_delta × time_scale` with no cap.
- All game-world systems MUST use `SimulationTime`, not `Time<Virtual>`.
- All calculations must be analytical (state from total time), not incremental.

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

### Upcoming Plugins
1. **ResearchPlugin**: Technology tree
2. **ShipPlugin**: Spacecraft management
3. **ColonyPlugin**: Planetary colonies
4. **DiplomacyPlugin**: Faction interactions

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
│   ├── camera.rs        # Camera movement & anchoring
│   ├── solar_system.rs  # Body spawning, rotation, billboards
│   ├── solar_system_data.rs # RON data loader
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
