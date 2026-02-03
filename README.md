# Helios-Ascension
A 4X game inspired by Aurora 4X and Terra Invicta with realistic orbital mechanics and a big focus on resource management, logistics and research. Climb the Kardashev scale starting at 0.7 and expand!

## Features

- **High-Performance Foundation**: Built with Bevy 0.14 engine with optimized compilation profiles
- **Modular Plugin Architecture**: Extensible plugin system for game systems
- **Comprehensive Solar System Simulation**: 
  - **377 celestial bodies** with realistic astronomical data from NASA/IAU sources
  - Complete planetary systems:
    - All 8 planets with accurate properties
    - **148 moons** including all major and many minor moons
    - Jupiter's complete 79-moon system
    - Saturn's complete 83-moon system
    - All Uranus (27) and Neptune (14) moons
  - **145 asteroids**:
    - Main belt comprehensive catalog
    - 30 Jupiter Trojans (L4 and L5 groups)
    - 17 Near-Earth Objects (mission targets)
  - **55 Kuiper Belt Objects** including Pluto, Eris, and scattered disc
  - **20 comets** including Halley, Hale-Bopp, and other famous visitors
  - Accurate masses, radii, and orbital parameters for all bodies
  - Real orbital mechanics with time-accelerated simulation
  - Complete coverage from Mercury to the outer solar system
- **Debug UI**: Integrated inspector using bevy_inspector_egui for runtime entity inspection
- **Advanced Camera Controls**: 
  - WASD for movement
  - Q/E for vertical movement
  - Right mouse button + drag for camera rotation
  - Mouse wheel for zoom

## System Requirements

### Linux
```bash
# Required for running the game with graphics
sudo apt-get install libwayland-dev libxkbcommon-dev libvulkan-dev libasound2-dev libudev-dev

# Required for optimized build performance (LLD linker)
# Without this, builds will fail on Linux due to .cargo/config.toml configuration
sudo apt-get install lld clang
```

### macOS / Windows
No additional system requirements - uses default system linkers.

## Building and Running

The project is configured with optimizations for fast compilation:
- **LLD linker** (Linux only): 2-5x faster linking than GNU ld
- **Parallel compilation**: Uses all available CPU cores automatically
- **Optimized test profile**: Faster test compilation

### Debug Build
```bash
cargo build
cargo run
```

### Release Build (Optimized)
```bash
cargo build --release
cargo run --release
```

### Fast Build Profile (Pre-configured, for rapid iteration)
```bash
cargo build --profile fast
cargo run --profile fast
```

### Testing

Run tests with standard cargo:
```bash
cargo test
```

Or use cargo-nextest for faster parallel test execution:
```bash
# Install cargo-nextest (one-time setup)
cargo install cargo-nextest

# Run tests in parallel
cargo nextest run
```

## Project Structure

```
helios_ascension/
├── src/
│   ├── main.rs                    # Application entry point
│   └── plugins/
│       ├── mod.rs                 # Plugin module exports
│       ├── camera.rs              # Camera control system
│       └── solar_system.rs        # Solar system simulation
├── Cargo.toml                     # Project configuration
└── README.md                      # This file
```

## Architecture

The game uses a modular plugin architecture built on Bevy's ECS (Entity Component System):

- **CameraPlugin**: Handles 3D camera movement and controls
- **SolarSystemPlugin**: Manages celestial bodies and orbital mechanics

## Controls

- **W/A/S/D**: Move camera forward/left/backward/right
- **Q/E**: Move camera down/up
- **Right Mouse Button + Drag**: Rotate camera
- **Mouse Wheel**: Zoom in/out

## Development

The project uses Bevy's development profile optimizations to provide fast compile times while maintaining good runtime performance. The inspector UI is enabled by default for debugging purposes.

## License

MIT

