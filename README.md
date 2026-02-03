# Helios-Ascension
A 4X game inspired by Aurora 4X and Terra Invicta with realistic orbital mechanics and a big focus on resource management, logistics and research. Climb the Kardashev scale starting at 0.7 and expand!

## Features

- **High-Performance Foundation**: Built with Bevy 0.14 engine with optimized compilation profiles
- **Modular Plugin Architecture**: Extensible plugin system for game systems
- **3D Solar System Simulation**: Realistic orbital mechanics with Mercury, Venus, Earth, Mars, and Jupiter
- **Debug UI**: Integrated inspector using bevy_inspector_egui for runtime entity inspection
- **Advanced Camera Controls**: 
  - WASD for movement
  - Q/E for vertical movement
  - Right mouse button + drag for camera rotation
  - Mouse wheel for zoom

## System Requirements

### Linux
```bash
sudo apt-get install libwayland-dev libxkbcommon-dev libvulkan-dev libasound2-dev libudev-dev
```

## Building and Running

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

### Fast Build (For rapid iteration)
```bash
cargo build --profile fast
cargo run --profile fast
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

