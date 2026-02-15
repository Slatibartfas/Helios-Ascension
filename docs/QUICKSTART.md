# Helios Ascension - Quick Start Guide

## Installation

### Step 1: Install Rust
If you don't have Rust installed:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Step 2: Install System Dependencies

#### Ubuntu/Debian
```bash
sudo apt-get update
sudo apt-get install -y \
    libwayland-dev \
    libxkbcommon-dev \
    libvulkan-dev \
    libasound2-dev \
    libudev-dev
```

#### Fedora
```bash
sudo dnf install -y \
    wayland-devel \
    libxkbcommon-devel \
    vulkan-devel \
    alsa-lib-devel \
    systemd-devel
```

#### Arch Linux
```bash
sudo pacman -S wayland libxkbcommon vulkan-icd-loader alsa-lib systemd
```

### Step 3: Clone and Build

```bash
# Clone the repository
git clone https://github.com/Slatibartfas/Helios-Ascension.git
cd Helios-Ascension

# Build in debug mode (faster compilation)
cargo build

# Or build in release mode (optimized)
cargo build --release
```

## Running the Game

### Debug Mode (with inspector UI)
```bash
cargo run
```

### Release Mode (optimal performance)
```bash
cargo run --release
```

### Fast Build Mode (quick iteration)
```bash
cargo run --profile fast
```

## First Steps

When you launch the game, you'll see:

1. **The Solar System**: A 3D view with 377+ celestial bodies including all planets, moons, asteroids, and comets
2. **Dashboard UI**: Top menu bar with navigation tabs (Survey, Construction, Research, Economy, Fleet, Shipbuilding)
3. **Time Controls**: Date display and speed controls (pause/play, speed selection)
4. **3D Scene**: Celestial bodies orbiting with realistic orbital mechanics and time acceleration

### Initial Game State
- You start with a colony on Earth
- Starting resources and stockpiles are available
- Basic technologies are unlocked
- Time is paused initially - press play to begin simulation

## Controls

### Camera Movement
- **W**: Move forward
- **A**: Move left
- **S**: Move backward
- **D**: Move right
- **Q**: Move down
- **E**: Move up

### Camera Rotation
- **Right Mouse Button + Drag**: Look around

### Camera Zoom
- **Mouse Wheel**: Zoom in/out
  - Zoom out far enough (~100 AU) to switch to starmap view
  - Zoom back in to return to system view

### Game Interactions
- **Left Click**: Select celestial bodies or UI elements
- **Double Click**: Select star systems in starmap view
- **Hover**: Show tooltips for bodies and stars
- **Space**: Pause/resume simulation
- **F12**: Toggle debug settings (in Construction/Research panels)

## Exploring the UI

### Survey Panel
- Browse celestial bodies in the current system
- View detailed information about selected bodies
- See mineral deposits, resources, and colony data
- Switch to starmap view to explore nearby star systems

### Construction Panel
- Select a colony to manage
- View 29 building types across 8 categories
- Queue construction projects
- Monitor build progress

### Research Panel
- Browse technology tree with 15 categories
- Select technologies to research
- View prerequisites and unlocks
- Track research progress with Research Points (RP)

### Economy Panel
- Monitor 20 resource types
- Track production and consumption rates
- View treasury and budget
- Check energy grid status

## Understanding the Solar System

The game includes a complete solar system simulation:

- **Sol (The Sun)**: Central star with realistic properties
- **8 Planets**: Mercury through Neptune with accurate masses, radii, and orbital parameters
- **148 Moons**: Including all major and many minor moons
- **145 Asteroids**: Main belt, Trojans, and Near-Earth Objects
- **55 Kuiper Belt Objects**: Including Pluto, Eris, and scattered disc objects
- **20 Comets**: Including famous comets like Halley

All bodies have:
- Realistic orbital mechanics (Keplerian orbits)
- Accurate physical properties (mass, radius, density)
- Time-accelerated motion (up to 1 year per second)
- Procedural mineral deposits
- Full colonization support

## Starting Your Civilization

### Early Goals
1. **Explore**: Survey celestial bodies to discover mineral deposits
2. **Build**: Construct essential buildings (Life Support, Habitat Domes, Power Plants)
3. **Research**: Progress through the technology tree
4. **Expand**: Establish colonies on other bodies
5. **Grow**: Increase population and production capacity

### Resource Management
- Monitor your stockpiles in the Economy panel
- Build mines to extract resources
- Ensure adequate power generation
- Maintain logistics efficiency with cargo terminals and mass drivers

### Time Control
- Start with 1 day/second for learning
- Increase to 1 week/second for construction
- Use 1 month/second or 1 year/second for research progression
- Pause when needed to review and plan

## Performance Tips

### For Development
- Use debug mode: `cargo run`
- Inspector UI helps with debugging
- Faster compilation times

### For Testing Performance
- Use release mode: `cargo run --release`
- Significantly better FPS
- Optimized rendering

### For Quick Iteration
- Use fast profile: `cargo run --profile fast`
- Balance between compile time and runtime performance

## Troubleshooting

### "Cannot find -lwayland-client" error
Install Wayland development libraries:
```bash
sudo apt-get install libwayland-dev
```

### Black screen or no window
Make sure Vulkan drivers are installed:
```bash
sudo apt-get install mesa-vulkan-drivers
```

### Low FPS
Try running in release mode:
```bash
cargo run --release
```

### Inspector not showing
The inspector is only enabled in debug builds. Run without `--release` flag.

## Next Steps

Once you're comfortable with the basics:

1. **Learn the Systems**: 
   - Read `docs/UI.md` for comprehensive UI guide
   - Check `docs/RESOURCES.md` for resource details
   - Review `docs/MODDING.md` for customization options

2. **Expand Your Empire**:
   - Establish colonies on the Moon, Mars, or other bodies
   - Build advanced research facilities for faster tech progression
   - Develop mining operations on asteroids
   - Explore nearby star systems in starmap view

3. **Master the Game**:
   - Balance resource production and consumption
   - Optimize construction queues
   - Plan long-term research strategy
   - Manage population growth and housing

4. **Contribute**:
   - Explore the codebase in `src/`
   - Read `ARCHITECTURE.md` for design details
   - Check `CONTRIBUTING.md` to add features
   - Report bugs or suggest features on GitHub

## Getting Help

- Open an issue on GitHub
- Check existing documentation
- Read Bevy documentation: https://bevyengine.org/learn/

## Useful Commands

```bash
# Format code
cargo fmt

# Check for issues
cargo clippy

# Run tests
cargo test

# Build documentation
cargo doc --open

# Clean build artifacts
cargo clean
```

Enjoy building your galactic empire!
