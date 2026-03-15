# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Helios Ascension is a 4X grand strategy game built with Rust and Bevy 0.18, featuring realistic orbital mechanics, colony management, fleet operations, and a technology tree. The game simulates 377+ celestial bodies with accurate astronomical data.

## Commands

```bash
# Build
cargo build              # Debug (faster compilation)
cargo build --release   # Optimized
cargo build --profile fast  # Quick iteration

# Run
cargo run
cargo run --release
cargo run --profile fast

# Test
cargo test                    # Run all tests
cargo test test_name          # Run specific test
cargo test -- --nocapture     # Show output

# Parallel testing (faster)
cargo nextest run

# Code quality
cargo fmt
cargo clippy
cargo doc --open
```

## Architecture

The project uses a **plugin-based architecture** with Bevy's ECS:

```
src/
├── astronomy/       # Orbital mechanics, Kepler orbits, selection, ephemeris
├── colony/          # Buildings, construction, population growth
├── economy/         # Resources, mining, budget, energy grid
├── fleets/         # Ships, maneuvers, Hohmann transfers
├── research/       # Technology tree, unlocks
├── plugins/        # Camera, music, solar_system, atmosphere, visual effects
├── ui/             # Egui panels: dashboard, research, construction, economy, fleets
└── render/         # Skybox, backdrop
```

### Key Systems

- **Astronomy**: KeplerOrbit propagation, comet tails, Lagrange points, starmap
- **Colony**: 47 building types across 8 categories, construction queue, population
- **Economy**: 37 resource types, mining operations, energy grid, global budget
- **Fleets**: 7 ship classes, 6 propulsion types, orbital mechanics with gravity assists
- **Research**: 15 technology categories, prerequisite chains, modifiers
- **UI**: Egui-based panels with SimulationTime-driven updates

### Simulation Time (CRITICAL)

Never use `Time<Virtual>` for game-world calculations. Use `SimulationTime` (in `src/ui/time.rs`) instead—it has no speed cap (enables up to 1 year/second), while Bevy's virtual time caps at ~15×.

```rust
// All game systems MUST use SimulationTime
fn system(time: Res<SimulationTime>) {
    let elapsed = time.elapsed_seconds(); // f64, no cap
}
```

Positional/rotational calculations must be **analytical** (compute from total elapsed time), not incremental.

### Bevy 0.18 Specifics

- Entity API: `Entity::index()` (not `row()`)
- Ambient lighting: `GlobalAmbientLight` (resource) or `AmbientLight` (component on Camera)
- State transitions: `NextState::set()` always triggers transitions; use `set_if_neq()` to skip
- Materials: bind groups use `@group(3)` in WGSL shaders
- Bloom: `bevy::post_process::bloom::Bloom`

### Egui Scheduling

All egui systems must run in `EguiPrimaryContextPass`, not `Update`:
```rust
.add_systems(EguiPrimaryContextPass, my_egui_system)
```

### Data-Driven Design

- Buildings: `assets/data/buildings.ron`
- Technologies: `assets/data/technologies.ron`
- Solar system: `assets/data/solar_system.ron`
- Stars: `assets/data/nearest_stars_raw.json`

## Fleet Mechanics

The fleet system uses:
- `FleetOrbit`: Circular parking orbit, angle advances at 1 rev/40s real time
- `ActiveManeuver`: Keplerian transfer arc, propagated analytically each frame
- `PendingFleetActions`: Thread-safe action queue for spawn/transfer/cancel
- Transfer planning: Hohmann transfers, gravity assists, phased departures

Delta-v calculation uses Tsiolkovsky rocket equation: Δv = Isp × g₀ × ln(m_wet / m_dry)

## Development Notes

- F12 toggles debug settings (free construction, instant build, tech bypass)
- Use `bevy-inspector-egui` in debug builds for runtime inspection
- Camera: WASD panning, right-click drag rotation, mouse wheel zoom, Home to recenter
- Starmap view activates at ~100 AU zoom distance
- Background music: CC-BY 4.0 (Scott Buckley), attribution overlay required
