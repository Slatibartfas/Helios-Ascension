# Multi-System Architecture Design

## Overview

This document describes the architecture for supporting multiple star systems with efficient rendering and simulation at scale (hundreds to thousands of systems).

## Goals

1. **Scalability**: Support hundreds to thousands of star systems
2. **Performance**: Maintain 60 FPS by selectively rendering only visible systems
3. **Simulation**: Continue lightweight simulation for all systems
4. **User Experience**: Seamless transitions between system view and galaxy view
5. **Extensibility**: Easy to add new systems procedurally or from data

## Architecture Layers

### Layer 1: Galaxy Coordinates (Light Years)

- **Scale**: Light years (ly)
- **Precision**: f64 for star system positions
- **Purpose**: Position star systems in galactic space
- **Visualization**: Galaxy view showing star systems as points/icons

### Layer 2: System Coordinates (Astronomical Units)

- **Scale**: AU (Astronomical Units)
- **Precision**: f64 for orbital calculations
- **Purpose**: Position bodies within a star system
- **Visualization**: System view showing planets, orbits, bodies

### Layer 3: Render Coordinates (Bevy Units)

- **Scale**: Arbitrary Bevy units
- **Precision**: f32 for rendering
- **Purpose**: GPU rendering coordinates
- **Conversion**: AU → Bevy units (current: 1 AU = 1500 units)

## Component Architecture

### New Components

```rust
/// Marks an entity as a star system container
#[derive(Component)]
pub struct StarSystem {
    /// Unique identifier for this system
    pub id: u64,
    /// System name (e.g., "Sol", "Alpha Centauri")
    pub name: String,
    /// Position in galactic coordinates (light years)
    pub galactic_position: DVec3,
    /// Whether this system is currently active (fully simulated + rendered)
    pub active: bool,
    /// Bounding radius in AU for culling (largest orbit + margin)
    pub bounding_radius_au: f64,
}

/// Marks the currently selected/active star system
#[derive(Component)]
pub struct ActiveSystem;

/// Galactic-scale coordinates for star systems
#[derive(Component)]
pub struct GalacticCoordinates {
    /// Position in light years from galactic center
    pub position: DVec3,
}

/// Simulation state for a star system
#[derive(Component)]
pub enum SystemSimulationState {
    /// Full simulation + full rendering (active system)
    Active,
    /// Lightweight simulation, no rendering (background systems)
    Background,
    /// Paused, no simulation or rendering (distant systems)
    Dormant,
}

/// Marks bodies that belong to a specific star system
#[derive(Component)]
pub struct SystemMember {
    /// Entity ID of the parent star system
    pub system_entity: Entity,
}
```

### Enhanced Existing Components

```rust
/// Extended CelestialBody to support multi-system
#[derive(Component)]
pub struct CelestialBody {
    pub name: String,
    pub radius: f32,
    pub mass: f64,
    pub body_type: BodyType,
    pub visual_radius: f32,
    pub asteroid_class: Option<AsteroidClass>,
    /// NEW: Reference to parent system
    pub system_entity: Option<Entity>,
}
```

## View Modes

### System View (Current Implementation)

- **Camera Distance**: 0 - 100,000 Bevy units (~70 AU)
- **Rendering**: Full 3D rendering of bodies, orbits, textures
- **Simulation**: Full Keplerian propagation every frame
- **Visible**: One system (active system)
- **UI**: System browser, body selection, resource display

### Transition Zone

- **Camera Distance**: 100,000 - 500,000 Bevy units
- **Rendering**: Gradual LOD reduction, orbit trails fade
- **Simulation**: Still full for active system
- **Purpose**: Smooth visual transition between views

### Galaxy View (New Implementation)

- **Camera Distance**: > 500,000 Bevy units (conceptually at light year scale)
- **Rendering**: Star systems as points/icons with names
- **Simulation**: Lightweight background simulation
- **Visible**: Multiple systems in view frustum
- **UI**: System browser, strategic map, expansion planning

## Rendering Strategy

### Active System (1 system)

```rust
// Full rendering for active system
- All bodies with meshes and textures
- All orbit trails with LOD
- Selection markers, hover effects
- Billboards, lighting, shadows
```

### Background Systems (Inactive, but close)

```rust
// Minimal rendering for nearby inactive systems
- Star only (point light or simple sphere)
- No planets or moons rendered
- No orbit trails
- System name label only
```

### Distant Systems

```rust
// No rendering
- Completely culled from rendering
- Only simulation state tracked
```

## Simulation Strategy

### Active System (Full Simulation)

```rust
// Run every frame for active system
propagate_orbits(active_system_bodies)
update_render_transform(active_system_bodies)
rotate_bodies(active_system_bodies)
draw_orbit_paths(active_system_bodies)
```

### Background Systems (Lightweight Simulation)

```rust
// Run less frequently (e.g., every 10 frames)
// Only critical state updates
- Orbital position updates (low frequency)
- Resource production/consumption
- Construction progress
- Colony population
- No rendering updates
```

### Dormant Systems (Minimal Simulation)

```rust
// Run very infrequently (e.g., every 100 frames)
// Only essential state
- Abstract resource flow
- Long-term processes
- No positional updates
```

## System Loading and Unloading

### Dynamic Loading

```rust
// When system becomes active (player selects it)
fn activate_system(system_entity: Entity) {
    1. Load all celestial body entities if not loaded
    2. Attach meshes and materials
    3. Initialize orbital propagation
    4. Set SystemSimulationState::Active
    5. Update camera focus
}

// When system becomes inactive
fn deactivate_system(system_entity: Entity) {
    1. Set SystemSimulationState::Background
    2. Despawn visual components (meshes, materials, markers)
    3. Keep simulation components (orbits, resources)
    4. Reduce update frequency
}

// When system becomes very distant
fn hibernate_system(system_entity: Entity) {
    1. Set SystemSimulationState::Dormant
    2. Store simplified state
    3. Despawn all non-essential entities
    4. Pause updates
}
```

### Memory Management

- **Budget**: Target ~500 MB per fully loaded system
- **Active Limit**: 1 active system at a time
- **Background Limit**: 5-10 background systems in memory
- **Total Systems**: Unlimited (load on demand)

## Camera Distance-Based Transitions

```rust
pub struct ViewMode {
    pub current: ViewModeType,
    pub transition_progress: f32, // 0.0 to 1.0
}

pub enum ViewModeType {
    SystemView,
    GalaxyView,
}

fn update_view_mode(
    camera: &OrbitCamera,
    active_system: &StarSystem,
    mut view_mode: ResMut<ViewMode>,
) {
    let distance = camera.radius;
    
    // Define thresholds
    const SYSTEM_VIEW_MAX: f32 = 100_000.0;
    const TRANSITION_START: f32 = 100_000.0;
    const TRANSITION_END: f32 = 500_000.0;
    const GALAXY_VIEW_MIN: f32 = 500_000.0;
    
    if distance < SYSTEM_VIEW_MAX {
        view_mode.current = ViewModeType::SystemView;
        view_mode.transition_progress = 0.0;
    } else if distance < GALAXY_VIEW_MIN {
        // In transition zone
        let t = (distance - TRANSITION_START) / (TRANSITION_END - TRANSITION_START);
        view_mode.transition_progress = t.clamp(0.0, 1.0);
    } else {
        view_mode.current = ViewModeType::GalaxyView;
        view_mode.transition_progress = 1.0;
    }
}
```

## Galaxy View Visualization

### Star System Representation

```rust
// Each star system shown as:
1. Icon/sprite (based on star type)
2. System name label
3. Strategic information (optional):
   - Number of colonies
   - Resource output
   - Construction projects
   - Military presence
```

### Interaction

```rust
// Click system to select it
// Double-click to focus and transition to system view
// Hover to show tooltip with system info
```

## Data Format

### Multi-System Data File

```ron
// galaxy.ron
(
    systems: [
        (
            id: 0,
            name: "Sol",
            position: (0.0, 0.0, 0.0), // galactic coordinates in light years
            star_type: "G2V",
            data_file: "assets/data/solar_system.ron",
        ),
        (
            id: 1,
            name: "Alpha Centauri A",
            position: (4.37, 0.0, 0.0),
            star_type: "G2V",
            data_file: "assets/data/alpha_centauri.ron",
        ),
        (
            id: 2,
            name: "Barnard's Star",
            position: (5.96, 0.0, 0.0),
            star_type: "M4V",
            data_file: "assets/data/barnards_star.ron",
        ),
    ],
    starting_system: 0, // Sol
)
```

## Performance Targets

### System View (1 active system)

- **FPS**: 60 FPS
- **Bodies**: 377+ (current solar system)
- **Memory**: ~500 MB

### Galaxy View (100+ systems visible)

- **FPS**: 60 FPS
- **Systems**: 100-1000 in view
- **Memory**: ~200 MB (mostly UI and system icons)

### Total Simulation (1000+ systems)

- **Active**: 1 system (full simulation)
- **Background**: 10 systems (light simulation)
- **Dormant**: 1000+ systems (minimal state)
- **Memory**: ~1-2 GB total

## Implementation Phases

### Phase 1: Foundation (Current PR)

- [x] Document architecture
- [ ] Create new components (StarSystem, SystemMember, etc.)
- [ ] Add SystemSimulationState enum
- [ ] Create data structures for multi-system support

### Phase 2: Single System Enhancement

- [ ] Wrap current system in StarSystem component
- [ ] Add system_entity references to all bodies
- [ ] Test existing functionality still works
- [ ] Add system activation/deactivation logic

### Phase 3: View Mode System

- [ ] Implement ViewMode resource
- [ ] Add camera distance-based view switching
- [ ] Create transition animations
- [ ] Test smooth transitions

### Phase 4: Multi-System Data

- [ ] Design galaxy.ron format
- [ ] Implement multi-system data loader
- [ ] Add system selection UI
- [ ] Test loading multiple system definitions

### Phase 5: Galaxy View

- [ ] Implement galaxy view rendering
- [ ] Add star system icons/sprites
- [ ] Implement system labels
- [ ] Add system selection/focus
- [ ] Test interaction

### Phase 6: Background Simulation

- [ ] Implement multi-frequency update system
- [ ] Add background simulation logic
- [ ] Test performance with multiple systems
- [ ] Optimize update frequencies

### Phase 7: Dynamic Loading

- [ ] Implement system activation/deactivation
- [ ] Add memory management
- [ ] Test system switching
- [ ] Profile memory usage

## Migration Path

### Existing Code Compatibility

All existing code will continue to work:

1. **Current single system**: Becomes "active system 0"
2. **Existing queries**: Add `With<ActiveSystem>` filter where needed
3. **Orbital propagation**: Only runs on active system bodies
4. **Rendering**: Only renders active system

### Backward Compatibility

```rust
// Old code (still works)
Query<&CelestialBody>

// New code (multi-system aware)
Query<(&CelestialBody, &SystemMember), With<ActiveSystem>>
```

## Future Enhancements

### Procedural Generation

- Generate systems on demand
- Use seed-based generation for consistency
- Cache generated systems to disk

### Multi-Threading

- Simulate background systems on separate threads
- Parallel orbital propagation for multiple systems
- Lock-free state updates

### Networking (Multiplayer)

- Each system can be independently synchronized
- Active system gets priority bandwidth
- Background systems use delta compression

## Performance Monitoring

### Metrics to Track

```rust
pub struct SystemPerformanceMetrics {
    // Per-frame
    pub active_system_bodies: usize,
    pub background_systems: usize,
    pub dormant_systems: usize,
    
    // Timing
    pub active_simulation_time_ms: f32,
    pub background_simulation_time_ms: f32,
    pub render_time_ms: f32,
    
    // Memory
    pub active_system_memory_mb: f32,
    pub total_memory_mb: f32,
}
```

### Debug UI

Add to inspector:
- Current view mode
- Active system name
- Number of systems in each state
- Performance metrics
- Memory usage

## Testing Strategy

### Unit Tests

- System activation/deactivation
- View mode transitions
- Coordinate conversions (galaxy ↔ system ↔ render)

### Integration Tests

- Load multiple systems
- Switch between systems
- Verify simulation continues in background
- Check memory doesn't leak

### Performance Tests

- 1 active + 10 background + 1000 dormant systems
- Verify 60 FPS maintained
- Check memory stays under budget
- Profile simulation time distribution

## Conclusion

This architecture provides a solid foundation for scaling to thousands of star systems while maintaining performance. The key innovations are:

1. **Hierarchical coordinates**: Galaxy → System → Body
2. **Selective rendering**: Only active system fully rendered
3. **Multi-frequency simulation**: Active/Background/Dormant states
4. **View mode transitions**: Smooth zoom from system to galaxy view
5. **Dynamic loading**: Systems loaded/unloaded as needed

The design is extensible, allowing future enhancements like procedural generation, multi-threading, and multiplayer without major refactoring.
