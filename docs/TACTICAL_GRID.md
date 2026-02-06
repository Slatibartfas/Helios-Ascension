# Tactical Grid System

## Overview

The Tactical Grid system provides visual aids for spatial awareness in the 3D solar system view, following common 4X space game conventions (Homeworld, Terra Invicta).

## Components

### 1. Ecliptic Grid Plane

A subtle, semi-transparent circular grid rendered on the ecliptic plane (XZ plane at Y=0).

**Features:**
- Circular pattern with radial and angular lines
- Fades out with distance from the sun (origin)
- Almost transparent (alpha 0.1) to avoid cluttering the view
- Configurable grid scale, fade distance, and max distance

**Parameters** (in `GridMaterial`):
- `grid_scale: 1000.0` - Size of grid cells in game units
- `fade_distance: 20000.0` - Distance where fade begins
- `max_distance: 50000.0` - Distance where grid is fully transparent
- `alpha: 0.1` - Base transparency level

### 2. Vertical Droplines ("Lollipops")

Thin vertical lines connecting entities to the ecliptic plane, helping visualize 3D position.

**Features:**
- Automatically created for all entities with a `Transform` component
- Excludes: Cameras, the grid itself, background stars, UI elements
- Updates in real-time to follow parent entity movement
- Subtle cyan color with slight emissive glow

**Exclusions:**
The system automatically excludes the following from dropline creation:
- `Camera` entities
- `TacticalGrid` (the grid plane itself)
- `StarParticle` (background starfield)
- `Dropline` entities (to prevent recursion)

## Usage

The `GridPlugin` is automatically added to the app in `main.rs`:

```rust
.add_plugins(GridPlugin)
```

No additional configuration is required. The grid and droplines will automatically appear in the game world.

## Technical Details

### Shader

The grid is rendered using a custom WGSL shader (`grid_material.wgsl`) that:
1. Uses world position XZ coordinates for grid pattern
2. Calculates distance from center for fade effect
3. Generates circular grid with radial and angular lines
4. Applies smooth fade-out based on distance

### Systems

**`setup_grid` (Startup)**
- Creates the grid plane mesh
- Instantiates the grid material
- Spawns the grid entity at origin

**`spawn_droplines` (Update)**
- Queries for entities needing droplines
- Creates cylinder mesh scaled to reach from entity to ecliptic
- Marks entities to prevent duplicate creation

**`update_droplines` (Update)**
- Updates dropline positions to follow parent entities
- Scales cylinders based on entity height above/below ecliptic
- Positions at midpoint between entity and plane

## Customization

To adjust grid appearance, modify the parameters in `setup_grid()`:

```rust
let grid_material = materials.add(GridMaterial {
    grid_params: GridParams {
        grid_scale: 1000.0,      // Grid cell size
        fade_distance: 20000.0,  // Fade start
        max_distance: 50000.0,   // Fade end
        alpha: 0.1,              // Transparency
    },
});
```

To adjust dropline appearance, modify the material in `spawn_droplines()`:

```rust
let dropline_material = materials.add(StandardMaterial {
    base_color: Color::srgba(0.2, 0.6, 0.8, 0.3),  // RGBA color
    emissive: LinearRgba::rgb(0.1, 0.3, 0.4),      // Glow
    // ...
});
```

## Performance Considerations

- Grid is a single large plane (no per-frame updates)
- Droplines use shared meshes and materials (clone handles, not data)
- `HasDropline` marker prevents duplicate creation
- System early-exits when no new entities need droplines
- Uses efficient ECS queries with filters

## Future Enhancements

Potential improvements:
- Toggle grid visibility with keyboard shortcut
- Configurable grid patterns (switch between circular/hexagonal/square)
- Color-coded droplines based on entity type
- Adjustable dropline thickness based on zoom level
- Grid LOD system for performance at extreme scales
