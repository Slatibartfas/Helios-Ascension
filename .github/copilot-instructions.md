# Helios Ascension - GitHub Copilot Instructions

Welcome to Helios Ascension, a 4X grand strategy game built with Rust and the Bevy game engine. These instructions help GitHub Copilot understand our project's architecture, conventions, and best practices.

## Project Overview

Helios Ascension is a high-performance space strategy game inspired by Aurora 4X and Terra Invicta. The project emphasizes:

- **Performance**: Optimized compilation profiles and runtime performance
- **Realism**: Accurate astronomical data for 377+ celestial bodies
- **Modularity**: Plugin-based architecture using Bevy's ECS
- **Maintainability**: Clear separation of concerns and testable code

## Technology Stack

- **Language**: Rust 2021 Edition
- **Game Engine**: Bevy 0.18
- **Architecture**: Entity Component System (ECS)
- **Serialization**: RON (Rusty Object Notation) and Serde
- **Math**: glam for high-performance vector/matrix operations
- **Development Tools**: bevy-inspector-egui for debugging

## Project Structure

```
helios_ascension/
├── src/
│   ├── main.rs              # Application entry point
│   ├── lib.rs               # Library root
│   ├── game_state.rs        # Top-level game state management
│   ├── astronomy/           # Orbital mechanics & coordinate systems
│   │   ├── components.rs    # SpaceCoordinates, KeplerOrbit, OrbitPath
│   │   ├── systems.rs       # Orbit propagation, rendering, selection
│   │   ├── ephemeris.rs     # Ephemeris calculations for custom start dates
│   │   ├── nearby_stars.rs  # Star catalog (60 nearest star systems)
│   │   ├── exoplanets.rs    # Exoplanet data
│   │   ├── procedural.rs    # Procedural system generation
│   │   └── mod.rs           # AstronomyPlugin
│   ├── colony/              # Colony management system
│   │   ├── components.rs    # Colony, BuildingInventory, ConstructionQueue
│   │   ├── types.rs         # BuildingType enum (29 types)
│   │   ├── data.rs          # Building data loading from RON
│   │   ├── systems.rs       # Construction processing, population growth
│   │   └── mod.rs           # ColonyPlugin
│   ├── economy/             # Resource & budget systems
│   │   ├── components.rs    # PlanetResources, MineralDeposit
│   │   ├── budget.rs        # GlobalBudget, EnergyGrid
│   │   ├── generation.rs    # Procedural resource generation
│   │   ├── mining.rs        # Mining operations and efficiency
│   │   └── types.rs         # ResourceType definitions (20 types)
│   ├── research/            # Technology tree system
│   │   ├── components.rs    # TechnologyProgress, EngineeringProject
│   │   ├── types.rs         # TechCategory enum (15 categories)
│   │   ├── data.rs          # Technology data loading from RON
│   │   ├── systems.rs       # Research progression, tech unlocks
│   │   └── mod.rs           # ResearchPlugin
│   ├── plugins/             # Game systems
│   │   ├── camera.rs        # Camera movement, anchoring & ViewMode
│   │   ├── music.rs         # Background music playlist & CC-BY attribution overlay
│   │   ├── solar_system.rs  # Body spawning, rotation, billboards
│   │   ├── solar_system_data.rs # RON data loader
│   │   ├── starmap.rs       # Starmap view (system icons, visibility toggle)
│   │   ├── system_populator.rs  # Populates visited star systems procedurally
│   │   ├── comet_vfx.rs     # Comet visual effects (tail, glow)
│   │   └── visual_effects.rs    # Bloom, starfield, night materials
│   ├── render/              # Rendering utilities
│   │   └── backdrop.rs      # Skybox background
│   └── ui/                  # User interface
│       ├── mod.rs           # UIPlugin, SimulationTime, TimeScale, all panels
│       └── interaction.rs   # Selection management
├── assets/
│   ├── audio/
│   │   └── music/           # Background music (CC-BY 4.0, Scott Buckley)
│   ├── data/
│   │   ├── buildings.ron    # 29 building definitions
│   │   ├── technologies.ron # Technology tree data
│   │   ├── solar_system.ron # Solar system configuration
│   │   └── nearest_stars_raw.json # Star catalog
│   └── textures/            # Visual assets
├── docs/
│   ├── MODDING.md           # Texture & celestial body modding guide
│   ├── RESEARCH_MODDING.md  # Technology tree modding guide
│   └── ...                  # Other reference docs
└── tests/                   # Integration tests
```

## Architecture Principles

### Plugin-Based Design
- Each major game system is a Bevy plugin
- Plugins should be self-contained and composable
- Use Bevy's `App::add_plugins()` to register plugins
- Keep plugins focused on a single responsibility

### ECS Best Practices
- **Components**: Pure data structures, no behavior
- **Systems**: Pure functions that operate on components
- **Resources**: Shared global state, use sparingly
- Use Bevy's query system for efficient entity filtering

### Performance Considerations
- The project is configured with optimized build profiles
- Prefer iterator chains over imperative loops
- Use Bevy's parallel system execution where possible
- Minimize entity spawning/despawning in hot loops
- Profile before optimizing - use `cargo flamegraph` or similar tools

## Coding Standards

Apply the [Rust coding standards](./.github/instructions/rust.instructions.md) to all Rust code.

Key principles:
- Write idiomatic Rust following the Rust API Guidelines
- Use strong types and leverage the ownership system
- Handle errors with `Result<T, E>`, avoid `unwrap()` in library code
- Document public APIs with `///` doc comments
- Keep functions focused and under ~50 lines when possible
- Use `cargo fmt` and `cargo clippy` for code quality

## Testing Strategy

Apply the [testing standards](./.github/instructions/testing.instructions.md) for all tests.

- Write unit tests for individual components and systems
- Use integration tests for plugin interactions
- Test data loading and serialization
- Use `cargo test` for standard testing
- Consider `cargo nextest` for parallel test execution

## Development Workflow

### Building
```bash
cargo build              # Debug build
cargo build --release    # Optimized release
cargo build --profile fast  # Fast iteration profile
```

### Running
```bash
cargo run                # Run debug build
cargo run --release      # Run optimized
```

### Testing
```bash
cargo test               # Run all tests
cargo nextest run        # Parallel testing
```

### Code Quality
```bash
cargo fmt                # Format code
cargo clippy             # Linting
```

## Bevy-Specific Guidelines

### Component Design
- Keep components small and focused
- Use marker components for entity categorization
- Derive common traits: `Component`, `Debug`, `Clone`

### System Design
- Systems should have clear inputs (queries) and outputs (mutations)
- Use system ordering to manage dependencies
- Prefer change detection queries (`Changed<T>`, `Added<T>`) for efficiency
- Use run conditions to control system execution

### Resource Usage
- Resources for global configuration and state
- Use `Res<T>` for immutable access, `ResMut<T>` for mutable
- Consider using events instead of resources for cross-system communication

### Events
- Use Bevy events for loose coupling between systems
- Define custom event types as needed
- Use `MessageReader<T>` and `MessageWriter<T>` in systems (renamed from `EventReader`/`EventWriter` in Bevy 0.17+)

## UI & Asset Guidelines

### Icon Processing
When adding new UI icons (menus, research categories, etc.), applying the following post-processing ensures consistent styling and themeability:

1.  **Format**: Load icons as standard images (e.g. PNG).
2.  **Processing Logic**:
    - Treat input as **dark lines on a white background**.
    - **Alpha Channel**: Calculate alpha from inverted luminance (`alpha = (1.0 - luminance).powf(3.0)`). This makes white backgrounds transparent and dark lines opaque.
    - **Color Channels**: Set all RGB pixels to **pure white** (`255, 255, 255`).
3.  **Runtime Tinting**: Since icons are pure white, they can be tinted to any color using `egui` (e.g., `ui.add(egui::Image::new(...).tint(color))`).

### Egui Integration
- Use `egui::load::SizedTexture` when adding images to `ui.add()` to ensure explicit control over size.
- Example: `ui.add(egui::Image::new(egui::load::SizedTexture::new(texture_id, [width, height])))`.

## Domain-Specific Knowledge

### Game Systems Overview

#### Colony Management
- **29 building types** across 8 categories (Infrastructure, Industry, Logistics, Power, Population, Research, Financial, Military)
- Construction queue system with resource costs and workforce requirements
- Population growth mechanics with housing capacity and food requirements
- Buildings require maintenance resources and generate various effects
- Tech-gated buildings unlock through research progression
- Debug menu (F12) for free construction, instant build, and tech bypass

#### Economy & Resources
- **20 resource types** (defined in `src/economy/types.rs` as `ResourceType` enum): Volatiles (Water, Hydrogen, Ammonia, Methane), Atmospheric Gases (Nitrogen, Oxygen, CarbonDioxide, Argon), Construction Materials (Iron, Aluminum, Titanium, Silicates), Fusion Fuel (Helium3), Fissiles (Uranium, Thorium), Precious Metals (Gold, Silver, Platinum), Specialty Materials (Copper, RareEarths)
- Resource stockpiles with capacity limits
- Mining operations extract resources from mineral deposits
- Refining and processing buildings convert raw materials
- Energy grid with power generation (solar, fission, fusion) and consumption
- Logistics system affects mining and research efficiency (demand vs capacity)
- Global budget tracks treasury, income, and expenses

#### Research & Technology
- **15 technology categories**: Electronics, Military, SpaceTechnology, Biology, Physics, Energy, Sociology, Construction, Propulsion, Materials, Sensors, Weapons, DefensiveSystems, LifeSupport, Industry
- Technology tree with prerequisite chains
- Research points (RP) and engineering points (EP) progression
- Technologies unlock buildings, components, and modifiers
- Tech modifiers affect construction costs, productivity, and capabilities
- **Data-driven**: All technologies defined in `assets/data/technologies.ron` — add techs without touching Rust code
- Debug menu (F12) for instant research
- See [docs/RESEARCH_MODDING.md](docs/RESEARCH_MODDING.md) for the full modding guide (modifier types, component definitions, balancing)

#### Background Music (`src/plugins/music.rs`)
- `MusicPlugin` plays a sequential looping playlist of ambient tracks during gameplay
- Tracks use `PlaybackMode::Despawn` — Bevy auto-despawns the entity when a track ends; the `advance_playlist` system detects this and starts the next track
- A non-interactive egui overlay in the bottom-right corner shows the current track title and CC-BY attribution (required by the Scott Buckley license)
- **Current playlist** (all CC-BY 4.0, Scott Buckley — www.scottbuckley.com.au):
  - `audio/music/starfire.mp3` — 'Starfire'
  - `audio/music/adrift-among-infinite-stars.mp3` — 'Adrift Among Infinite Stars'
  - `audio/music/passage-of-time.mp3` — 'Passage Of Time'
- **Adding a track**: push a new `TrackInfo { path, title }` into the `Vec` in `MusicPlaylist::default()`. No other code changes needed.
- **License requirement**: every track MUST include a `title` string matching the official Scott Buckley attribution so the overlay stays correct.

#### Celestial Body & Texture Modding
- All solar system data defined in `assets/data/solar_system.ron`
- Add custom textures by setting the `texture` field in the RON file — no code changes needed
- See [docs/MODDING.md](docs/MODDING.md) for the full texture and body modding guide

#### Celestial Bodies & Astronomy
- All astronomical data is based on real NASA/IAU sources
- Orbital mechanics use simplified Keplerian elements
- Time acceleration is supported for simulation speed (up to 1 year/second)
- Bodies are organized hierarchically (Sun -> Planets -> Moons)

### Simulation Time (IMPORTANT)
- **Never use `Time<Virtual>`** for game-world calculations. Bevy's virtual time has a hard `max_delta` cap (~250ms) that silently limits effective speed to ~15×.
- Use `SimulationTime` (defined in `src/ui/mod.rs`) for all game-world elapsed time. It reads `Time<Real>` delta and scales it by `TimeScale`, with **no cap**, enabling speeds up to 1 year/second.
- Access via `Res<SimulationTime>` — call `.elapsed_seconds()` to get total simulation time in f64.
- All time-dependent game systems (orbits, rotation, economy ticks, research, production) **must** use `SimulationTime`, not `Time`, `Time<Virtual>`, or `time.delta_seconds()`.
- `Time` / `Time<Real>` should only be used for UI animations, camera movement, and other real-time visual effects that should not scale with game speed.
- All positional/rotational calculations must be **analytical** (compute state from total elapsed time), not **incremental** (accumulate deltas). This ensures correctness at any speed.

#### Custom game start dates & ephemeris (how to implement)
- Use the ephemeris utility in `src/astronomy/ephemeris.rs` to compute mean anomalies for a chosen Unix timestamp: `calculate_positions_at_timestamp(start_timestamp)`.
- Create `SimulationTime` with `SimulationTime::with_start_timestamp(start_timestamp)` so that the UI and systems display the correct date and time.
- When spawning bodies at game start (e.g., in `src/plugins/solar_system.rs::setup_solar_system`), override loaded `initial_angle` / set `KeplerOrbit.mean_anomaly_epoch` from the values returned by `calculate_positions_at_timestamp` (convert degrees to radians as necessary).
- Ensure this initialization runs before any systems that propagate or render orbits, so all bodies begin at the correct positions for the start date.
- Add tests to validate a few canonical dates (e.g., Jan 1, 2026) to ensure the ephemeris integration remains correct.

### Orbit Rendering
- Orbit trails sample uniformly in **true anomaly** for even point density
- Highly eccentric orbits (e > 0.6) automatically get more segments
- Trail fades from the body's current position backwards around the orbit

### Camera System
- Free-flight camera with WASD + Q/E controls
- Right-click drag for rotation
- Mouse wheel for zoom
- Camera focuses on selected celestial bodies

### Ambient Lighting
- Use `GlobalAmbientLight` (resource) for the default ambient light for the entire world
- Use `AmbientLight` (component) on a `Camera` entity to override the global ambient for that camera
- In Bevy 0.18, the old `AmbientLight` resource was split into `GlobalAmbientLight` (resource) and `AmbientLight` (component)

### State Transitions (Bevy 0.18)
- `NextState::set()` now **always triggers** `OnEnter`/`OnExit` transitions, even if setting the same state
- Use `NextState::set_if_neq()` if you want the previous behavior of skipping same-state transitions

### Entity API (Bevy 0.18)
- Entity terminology changed from "row" to "index": `Entity::row()` → `Entity::index()`, `EntityRow` → `EntityIndex`
- Many Entity interaction errors changed from `EntityDoesNotExistError` to `EntityNotSpawnedError`

### Custom Materials & Shaders
- Material bind groups use `@group(3)` in WGSL shaders (shifted from `@group(2)` in 0.17)
- Custom materials use `#[derive(AsBindGroup)]` — the derive auto-generates the `label()` method
- `MaterialPlugin` no longer has `prepass_enabled` / `shadows_enabled` fields; override `Material::enable_prepass()` / `Material::enable_shadows()` methods instead
- Camera HDR is controlled via the `Hdr` marker component (not a field on `Camera`)
- Bloom is at `bevy::post_process::bloom::Bloom`
- `ShaderRef` is at `bevy::shader::ShaderRef`

### Automatic Aabb Updates (Bevy 0.18)
- Bevy now auto-updates `Aabb` when meshes/sprites change — no need to manually remove and re-add `Aabb`
- Use `NoAutoAabb` component to opt out of automatic Aabb creation/update

## Security Considerations

Apply the [security standards](./.github/instructions/security.instructions.md).

- Validate all user inputs
- Use safe Rust practices, avoid `unsafe` unless necessary
- Be careful with deserialization from untrusted sources
- Follow Rust's memory safety guarantees

## Performance Guidelines

Apply the [performance standards](./.github/instructions/performance.instructions.md).

- Profile before optimizing
- Use Bevy's built-in diagnostics for frame timing
- Batch operations where possible
- Use Bevy's parallel system execution
- Consider using `bevy_rapier` for physics if needed

## Documentation

Apply the [documentation standards](./.github/instructions/documentation.instructions.md).

### Documentation Principles (CRITICAL)
- **NO PR Summaries**: NEVER create "SUMMARY.md", "IMPLEMENTATION_SUMMARY.md", "FIXES.md" or similar PR-specific documents. These become stale documentation clutter.
- **Update, Don't Create**: Before creating new documentation, search for existing docs that can be updated instead.
- **One Source of Truth**: Each topic should have ONE authoritative document. Avoid multiple documents covering the same subject.
- **Archive Old Content**: Move historical/completed work summaries to `docs/archive/` immediately after PR merge.
- **Lean Documentation**: Every document must serve an ongoing reference purpose. If it's just a progress report, it doesn't belong in main docs.

### Documentation Guidelines
- **Cleanliness**: Maintain a clean project root. Move detailed docs to `docs/`, `docs/design/`, or `docs/archive/`.
- **Maintenance**: ALWAYS prefer updating existing documents over creating new ones. Consolidate related information.
- **Synchronization**: Ensure every code change is reflected in the relevant documentation immediately.
- **Review**: Regularly scan detailed documentation (`docs/`) to ensure it matches the current codebase state.
- **Reference Material Only**: Documentation in `docs/` should be reference material (guides, architecture, APIs), not progress reports or PR summaries.
- Document all public APIs with `///` doc comments
- Include examples in doc comments
- Keep README.md up to date
- Update ARCHITECTURE.md for significant changes

### Egui System Scheduling
- All egui-using systems must be placed in the `EguiPrimaryContextPass` schedule (from `bevy_egui`), **not** `Update`
- This is required because `bevy_egui` 0.36+ runs its context setup in a separate pass; calling `available_rect()` before the context is ready causes a panic
- Example: `.add_systems(EguiPrimaryContextPass, my_egui_system)`

### Bevy 0.18 Feature Collections
- Bevy 0.18 introduced high-level cargo feature collections: `2d`, `3d`, `ui`
- Consider using these instead of listing individual sub-crate features for simpler `Cargo.toml` maintenance
- Our project currently uses individual features for fine-grained control

## Getting Help

- Check the [Bevy documentation](https://bevyengine.org/)
- Review the [Rust Book](https://doc.rust-lang.org/book/)
- See existing code patterns in the plugins/ directory
- Use the specialized chat modes in `.github/agents/`
