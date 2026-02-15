# Helios-Ascension
A 4X grand strategy game inspired by Aurora 4X and Terra Invicta with realistic orbital mechanics and a big focus on resource management, logistics and research. Climb the Kardashev scale starting at 0.7 and expand your civilization across the stars!

## Current Status: v0.2.0 - Core Mechanics Implemented ✨

The game now has fully functional colony management, economy, research, and interstellar navigation systems!

## Features

### Core Game Systems

- **Colony Management**: Establish and manage colonies across the solar system
  - **29 distinct building types** across 8 categories (Infrastructure, Industry, Logistics, Power, Population, Research, Financial, Military)
  - Construction queue system with resource costs and build times
  - Workforce allocation and efficiency management
  - Population growth and housing systems
  - Building maintenance and operating costs

- **Economy & Resources**: Deep resource management with real scarcity
  - **20 resource types**: Volatiles, gases, construction materials, precious metals, fissiles, and specialty materials
  - Mining operations to extract resources from celestial bodies
  - Resource stockpiles, production rates, and consumption tracking
  - Global budget management with income and expenses
  - Energy grid with power generation (solar, fission, fusion) and distribution

- **Research & Technology**: Unlock new capabilities through scientific advancement
  - **15 technology categories**: Electronics, Military, SpaceTechnology, Biology, Physics, Energy, Sociology, Construction, Propulsion, Materials, Sensors, Weapons, DefensiveSystems, LifeSupport, Industry
  - Technology tree with prerequisites and unlocks
  - Research projects progressing with research points (RP)
  - Engineering projects for component design
  - Tech modifiers affecting construction costs and productivity

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
  - Real orbital mechanics with time-accelerated simulation (up to 1 year/second)
  - Complete coverage from Mercury to the outer solar system

- **Interstellar Navigation**: Explore nearby star systems
  - **~1000 nearest stars** from real astronomical catalogs
  - Starmap view for interstellar navigation
  - Real star data including spectral types, masses, luminosities, and metallicities
  - Procedural system generation for visited stars

### User Interface

- **Comprehensive UI Panels**:
  - Survey Panel: Body selection, resources, population, mineral deposits
  - Construction Panel: Building management and construction queues
  - Research Panel: Technology tree browser and project selection
  - Economy Panel: Financial overview and resource tracking
  - Starmap Panel: Interstellar navigation and system selection
  - Fleet Panel: Fleet management (coming soon)
  - Shipbuilding Panel: Vessel construction (coming soon)
  
- **Time Control**: Variable simulation speed (1 day/s to 1 year/s)
- **Debug Inspector**: Integrated inspector using bevy_inspector_egui for runtime entity inspection

### Technical Features

- **High-Performance Foundation**: Built with Bevy 0.14 engine with optimized compilation profiles
- **Modular Plugin Architecture**: Extensible plugin system for game systems
- **Advanced Camera Controls**: 
  - WASD for movement
  - Q/E for vertical movement
  - Right-click drag for camera rotation
  - Mouse wheel for zoom
  - Automatic system ↔ starmap view transitions

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
│   ├── main.rs              # Application entry point
│   ├── lib.rs               # Library root
│   ├── astronomy/           # Orbital mechanics & coordinate systems
│   ├── colony/              # Colony management & buildings
│   ├── economy/             # Resources, budget & energy grid
│   ├── research/            # Technology tree & engineering
│   ├── plugins/             # Bevy plugin modules
│   │   ├── camera.rs        # Camera control system
│   │   ├── solar_system.rs  # Celestial body simulation
│   │   ├── starmap.rs       # Interstellar navigation
│   │   └── ...
│   └── ui/                  # User interface panels
├── assets/
│   ├── data/                # Game data (buildings, tech tree, etc.)
│   └── textures/            # Textures and visual assets
├── tests/                   # Integration tests
├── Cargo.toml               # Project configuration
└── README.md                # This file
```

## Architecture

The game uses a modular plugin architecture built on Bevy's ECS (Entity Component System):

- **CameraPlugin**: 3D camera movement and automatic view transitions
- **SolarSystemPlugin**: Manages celestial bodies and orbital mechanics
- **AstronomyPlugin**: High-precision Keplerian orbital mechanics
- **ColonyPlugin**: Colony management with 29 building types
- **EconomyPlugin**: Resource production, consumption, and budget tracking
- **ResearchPlugin**: Technology tree and research progression
- **UIPlugin**: Dashboard with time controls and interactive panels
- **StarmapPlugin**: Interstellar navigation and system selection

## Controls

### Camera Movement
- **W/A/S/D**: Move camera forward/left/backward/right
- **Q/E**: Move camera down/up
- **Right Mouse Button + Drag**: Rotate camera
- **Mouse Wheel**: Zoom in/out

### Game Controls
- **Left Click**: Select celestial bodies or UI elements
- **Double Click**: Select star systems in starmap view
- **Hover**: Show tooltips for bodies and stars
- **F12**: Toggle debug settings when applicable

### Time Control
- **Pause/Resume**: Control simulation time
- **Speed Selection**: 1 day/s, 1 week/s, 1 month/s, or 1 year/s

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

