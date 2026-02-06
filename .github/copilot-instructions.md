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
- **Game Engine**: Bevy 0.14
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
│   └── plugins/             # Bevy plugin modules
│       ├── mod.rs           # Plugin exports
│       ├── camera.rs        # Camera control system
│       └── solar_system.rs  # Celestial body simulation
├── tests/                   # Integration tests
├── assets/                  # Game assets (textures, models, etc.)
└── docs/                    # Documentation
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
- Use `EventReader<T>` and `EventWriter<T>` in systems

## Domain-Specific Knowledge

### Celestial Bodies
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

### Orbit Rendering
- Orbit trails sample uniformly in **true anomaly** for even point density
- Highly eccentric orbits (e > 0.6) automatically get more segments
- Trail fades from the body's current position backwards around the orbit

### Camera System
- Free-flight camera with WASD + Q/E controls
- Right-click drag for rotation
- Mouse wheel for zoom
- Camera focuses on selected celestial bodies

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

- **Cleanliness**: Maintain a clean project root. Move detailed docs to `docs/`, `docs/design/`, or `docs/archive/`.
- **Maintenance**: Prefer updating existing documents over creating new ones. Consolidate related information.
- **Synchronization**: Ensure every code change is reflected in the relevant documentation immediately.
- **Review**: Regularly scan detailed documentation (`docs/`) to ensure it matches the current codebase state.
- Document all public APIs with `///` doc comments
- Include examples in doc comments
- Keep README.md up to date
- Update ARCHITECTURE.md for significant changes

## Getting Help

- Check the [Bevy documentation](https://bevyengine.org/)
- Review the [Rust Book](https://doc.rust-lang.org/book/)
- See existing code patterns in the plugins/ directory
- Use the specialized chat modes in `.github/agents/`
