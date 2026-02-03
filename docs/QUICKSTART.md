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

1. **The Solar System**: A 3D view of Sol and the inner planets
2. **Inspector Panel**: Debug UI on the right side (in debug builds)
3. **3D Scene**: Planets orbiting the sun with proper lighting

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

## Using the Inspector

The inspector (visible in debug builds) allows you to:

1. **View Entities**: See all entities in the scene
2. **Inspect Components**: Click on an entity to see its components
3. **Edit Values**: Modify component values in real-time
4. **Monitor Performance**: Check frame times and system performance

### Example: Changing Planet Color

1. Click on a planet entity in the inspector
2. Find the `StandardMaterial` component
3. Modify the `base_color` values
4. See the changes immediately in the 3D view

## Understanding the Solar System

The initial scene includes:

- **Sol (The Sun)**: Central star with emissive material and point light
- **Mercury**: Smallest, gray planet, closest to the sun
- **Venus**: Yellow-tinted, second planet
- **Earth**: Blue planet, third from the sun
- **Mars**: Red planet, fourth position
- **Jupiter**: Large gas giant, fifth planet

All planets orbit the sun with different speeds and distances.

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

- Explore the codebase in `src/plugins/`
- Read `ARCHITECTURE.md` for design details
- Check `CONTRIBUTING.md` to add features
- Experiment with the inspector to understand ECS

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
