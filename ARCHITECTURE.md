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
- `CelestialBody`: Basic properties (name, radius, mass)
- `Star`: Marker for stars
- `Planet`: Marker for planets
- `OrbitalPath`: Defines orbital parameters and current position
- `RotationSpeed`: Controls body rotation speed

**Systems:**
- `setup_solar_system`: Creates the initial solar system at startup
- `update_orbits`: Updates planetary positions based on orbital mechanics
- `rotate_bodies`: Rotates celestial bodies

**Celestial Bodies:**
- Sol (Sun) - Emissive material with point light
- Mercury - Small gray planet
- Venus - Yellow-tinted planet
- Earth - Blue planet
- Mars - Red planet
- Jupiter - Large gas giant

**Features:**
- Circular orbits with configurable parameters
- Self-rotation of bodies
- Physically-based rendering (PBR)
- Emissive sun with dynamic lighting

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
1. **UIPlugin**: Game HUD and menus
2. **ResourcePlugin**: Resource management system
3. **ResearchPlugin**: Technology tree
4. **ShipPlugin**: Spacecraft management
5. **ColonyPlugin**: Planetary colonies
6. **DiplomacyPlugin**: Faction interactions
7. **EconomyPlugin**: Economic simulation

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
├── plugins/             # Game systems
│   ├── mod.rs          # Plugin exports
│   ├── camera.rs       # Camera system
│   └── solar_system.rs # Solar system simulation
├── components/          # Shared components (future)
├── resources/           # Global resources (future)
└── utils/              # Helper functions (future)
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
