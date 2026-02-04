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

## Modding Support

Helios Ascension supports **easy texture and body modding**:

- ✅ **Replace any texture**: Add custom textures that automatically override defaults
- ✅ **Add new bodies**: Create custom moons, planets, asteroids, or entire solar systems
- ✅ **Texture packs**: Create complete texture replacement packs
- ✅ **Procedural fallback**: Bodies without textures get appropriate generic textures with variations

**The system prioritizes dedicated textures** - just add your texture path to the RON file and it works!

📖 **See [docs/MODDING_GUIDE.md](docs/MODDING_GUIDE.md)** for complete modding documentation and examples.

### Quick Example: Replace Mars Texture

1. Add your texture: `assets/textures/celestial/planets/mars_custom_8k.jpg`
2. Edit `assets/data/solar_system.ron`:
```ron
(
    name: "Mars",
    // ... other fields ...
    texture: Some("textures/celestial/planets/mars_custom_8k.jpg"),  // Your texture!
)
```
3. Restart the game - done!

## Development

The project uses Bevy's development profile optimizations to provide fast compile times while maintaining good runtime performance. The inspector UI is enabled by default for debugging purposes.

## Planetary Textures Attribution

This game uses high-resolution (8K) planetary textures provided by Solar System Scope:

**Textures provided by Solar System Scope**  
https://www.solarsystemscope.com/  
License: CC BY 4.0 (https://creativecommons.org/licenses/by/4.0/)  
Resolution: Up to 8K (8192x4096 pixels) for major celestial bodies

These textures are based on NASA public domain mission data from:
- Mercury: NASA Messenger mission
- Venus: NASA Magellan mission
- Earth: NASA Blue Marble project
- Mars: NASA Viking/MGS missions
- Jupiter: NASA Cassini/Juno missions
- Saturn: NASA Cassini mission
- Moon: NASA Lunar Reconnaissance Orbiter
- Other bodies: Various NASA missions

**Note**: The original NASA data is public domain and available at lower resolutions (2K-4K) from:
- NASA 3D Resources: https://science.nasa.gov/3d-resources/
- NASA Image Library: https://images.nasa.gov/
- NASA GitHub: https://github.com/nasa/NASA-3D-Resources

We chose to use Solar System Scope's convenient 8K packages for superior visual quality, which requires the CC BY 4.0 attribution above.

## License

MIT

