# Helios-Ascension
A 4X grand strategy game with realistic orbital mechanics and a big focus on resource management, logistics and research. Climb the Kardashev scale starting at 0.7 and expand your civilization across the stars!

## Current Status: v0.3.0 - Fleet & Orbital Transfer System Implemented ✨

The game now has fully functional colony management, economy, research, interstellar navigation, and a complete fleet movement system with realistic orbital mechanics!

## Features

### Core Game Systems

- **Colony Management**: Establish and manage colonies across the solar system
  - **31 distinct building types** across 8 categories (Infrastructure, Industry, Logistics, Power, Population, Research, Financial, Military)
  - Construction queue system with resource costs and build times
  - Workforce allocation and efficiency management
  - Population growth and housing systems
  - Building maintenance and operating costs

- **Economy & Resources**: Deep resource management with real scarcity
  - **37 resource types**: Volatiles, gases, construction materials, precious metals, fissiles, and specialty materials
  - Mining operations to extract resources from celestial bodies
  - Resource stockpiles, production rates, and consumption tracking
  - Global budget management with income and expenses
  - Energy grid with power generation (solar, fission, fusion) and distribution

- **Fleet Management & Orbital Mechanics**: Command fleets across the solar system
  - **7 ship classes**: Courier, Frigate, Destroyer, Cruiser, Research Vessel, Freighter, Station
  - **6 propulsion types**: Chemical (450 s Isp), Nuclear Thermal (900 s), Ion Drive (5 000 s), Nuclear Pulse (10 000 s), Fusion Torch (50 000 s), Antimatter Drive (1 000 000 s)
  - Realistic Tsiolkovsky rocket-equation Δv and fuel calculations per ship
  - **3 transfer options** per route: efficient Hohmann, moderate, and fast burns
  - Transfer window planner: live synodic-period countdown and phase-angle display
  - Phased departure planning — adjust departure time to hit the optimal window
  - **Gravity-assist flyby candidates** automatically computed for each heliocentric transfer
  - **Lagrange-point targeting** (L4/L5 for any planet, Earth-Sun L1/L2/L3)
  - Fleet intercept planning with configurable passing distance and encounter speed
  - Mid-transit course-correction with abort-burn fuel deduction
  - Refuelling from planetary stockpiles
  - Visual trajectory arcs, orbit rings, selection reticules, and starmap icons
  - Initial Earth-orbit frigate fleet spawned at game start

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
  - **60 nearest star systems** from real astronomical catalogs
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
  - Fleet Panel: Full fleet management — spawn fleets, select transfer options, gravity assists, Lagrange-point routing, refuel, and abort maneuvers
  - Shipbuilding Panel: Vessel construction (planned)
  
- **Time Control**: Variable simulation speed (1 day/s to 1 year/s)
- **Debug Inspector**: Integrated inspector using bevy_inspector_egui for runtime entity inspection

### Technical Features

- **Atmospheric Scattering**: Real-time single-scattering Rayleigh + Mie shader on all bodies with atmospheres
  - Physically-derived parameters (scale height, Rayleigh tint, Mie haze) from gas composition data
  - RON overrides for Earth (blue limb glow), Mars (reddish dust), Venus (thick yellow-white), Titan (amber haze)
  - Correct layer ordering: surface → night lights → cloud deck → scattering shell
- **High-Performance Foundation**: Built with Bevy 0.18 engine with optimized compilation profiles
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
│   ├── fleets/              # Fleet management, orbital mechanics & transfer planning
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
- **ColonyPlugin**: Colony management with 31 building types
- **EconomyPlugin**: Resource production, consumption, and budget tracking
- **ResearchPlugin**: Technology tree and research progression
- **FleetPlugin**: Fleet management, orbital transfer planning, gravity assists, and trajectory rendering
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

### Debug Menu (F12)
Press **F12** in-game to open the debug settings panel:
- **Colony**: Toggle free construction (no resource costs) and instant build completion
- **Research**: Instantly complete the selected research project; bypass tech prerequisites
- **Inspector**: Runtime entity/component inspector via `bevy_inspector_egui`

### Time Control
- **Pause/Resume**: Control simulation time
- **Speed Selection**: 1 day/s, 1 week/s, 1 month/s, or 1 year/s

## Modding Support

Helios Ascension is designed to be data-driven and moddable without touching Rust code:

### Textures & Celestial Bodies
- ✅ **Replace any texture**: Add custom textures that automatically override defaults
- ✅ **Add new bodies**: Create custom moons, planets, asteroids, or entire solar systems
- ✅ **Texture packs**: Create complete texture replacement packs
- ✅ **Procedural fallback**: Bodies without textures get appropriate generic textures with variations

**The system prioritizes dedicated textures** - just add your texture path to the RON file and it works!

📖 **See [docs/MODDING.md](docs/MODDING.md)** for complete modding documentation and examples.

#### Quick Example: Replace a Texture

1. Add your texture: `assets/textures/celestial/planets/mars_custom_8k.jpg`
2. Edit `assets/data/solar_system.ron`:
```ron
(
    name: "Mars",
    // ... other fields ...
    texture: Some("textures/celestial/planets/mars_custom_8k.jpg"),
)
```
3. Restart the game - done!

### Research & Technology Tree
- ✅ **Data-driven tech tree**: All technologies defined in `assets/data/technologies.ron`
- ✅ **Add new technologies**: Define custom techs with costs, prerequisites, and modifiers
- ✅ **15 tech categories**: Electronics, Physics, Propulsion, Materials, and more
- ✅ **Modifier system**: Technologies grant percentage bonuses to research, mining, construction, and more
- ✅ **Component & engineering projects**: Technologies can unlock designs requiring engineering points

#### Quick Example: Add a Technology

1. Edit `assets/data/technologies.ron` and add an entry:
```ron
(
    id: "quantum_comms",
    name: "Quantum Communications",
    category: Electronics,
    description: "Instantaneous FTL communication using quantum entanglement.",
    research_cost: 15000.0,
    prerequisites: ["neural_networks"],
    unlocks_components: [],
    unlocks_engineering: [],
    modifiers: [],
    tier: 4,
)
```
2. Run the game and open the Research panel (🔬 icon) - your tech appears immediately.

📖 **See [docs/RESEARCH_MODDING.md](docs/RESEARCH_MODDING.md)** for the full technology modding guide, including all modifier types, component definitions, and balancing guidelines.

### Buildings
- ✅ **Data-driven buildings**: All 31 building types defined in `assets/data/buildings.ron`
- ✅ **Custom buildings**: Add new construction options with resource costs and effects

## Development

The project uses Bevy's development profile optimizations to provide fast compile times while maintaining good runtime performance. The inspector UI is enabled by default for debugging purposes.

## Music Attribution

In-game background music is provided by **Scott Buckley** and licensed under [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/).

| Track | Attribution |
|-------|-------------|
| Starfire | 'Starfire' by Scott Buckley — released under CC-BY 4.0. www.scottbuckley.com.au |
| Adrift Among Infinite Stars | 'Adrift Among Infinite Stars' by Scott Buckley — released under CC-BY 4.0. www.scottbuckley.com.au |
| Passage Of Time | 'Passage Of Time' by Scott Buckley — released under CC-BY 4.0. www.scottbuckley.com.au |

Music files are stored in `assets/audio/music/`. Attribution is also displayed in-game via a small overlay in the bottom-right corner during gameplay.

To add more tracks, push a new `TrackInfo` entry into `MusicPlaylist::default()` in `src/plugins/music.rs`.

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

