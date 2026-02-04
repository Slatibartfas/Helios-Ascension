# Multi-Star System Support

## Overview
The economic system has been refactored to support multiple star systems with different stellar properties and frost lines. This enables scaling from a single solar system to hundreds or thousands of star systems, as required for Kardashev Scale Level 2-3 civilizations.

## Architecture Changes

### 1. StarSystem Component
A new `StarSystem` component marks stars and defines their system properties:

```rust
#[derive(Component)]
pub struct StarSystem {
    /// Frost line distance in AU for this star
    pub frost_line_au: f64,
    
    /// Stellar classification (O, B, A, F, G, K, M)
    pub spectral_class: SpectralClass,
}
```

**Key Features:**
- `sun_like()`: Creates a G-type star with standard 2.5 AU frost line
- `from_luminosity()`: Calculates frost line from stellar luminosity using the formula: `frost_line ≈ 2.7 * sqrt(L/L_sun) AU`
- Different stellar types have different frost lines:
  - M-type red dwarfs: ~0.5 AU (cooler, closer frost line)
  - G-type like our Sun: ~2.5 AU
  - A-type blue giants: ~17 AU (hotter, farther frost line)

### 2. OrbitsBody Component
Tracks parent-child relationships in stellar hierarchies:

```rust
#[derive(Component)]
pub struct OrbitsBody {
    pub parent: Entity,
}
```

This enables:
- Planets orbiting specific stars
- Moons orbiting planets
- Binary/trinary star systems
- Complex hierarchical systems

### 3. Updated Resource Generation
The `generate_solar_system_resources` system now:

1. **Queries for parent stars**: Looks up the `StarSystem` component to get the frost line
2. **Calculates distance from parent**: Uses the parent star's coordinates, not just origin
3. **Applies star-specific frost line**: Resource distribution adapts to each star's properties
4. **Falls back gracefully**: Works without `OrbitsBody` for backward compatibility

```rust
pub fn generate_solar_system_resources(
    body_query: Query<(Entity, &CelestialBody, &SpaceCoordinates, Option<&OrbitsBody>)>,
    star_query: Query<(&StarSystem, &SpaceCoordinates)>,
) {
    // For each body, find its parent star and calculate distance
    // Generate resources based on that star's frost line
}
```

## Usage Examples

### Single Star System (Backward Compatible)
```rust
// No changes needed - works as before
// Bodies without OrbitsBody use origin and default 2.5 AU frost line
```

### Multiple Star Systems
```rust
// Spawn a red dwarf star
let red_dwarf = commands.spawn((
    StarSystem::from_luminosity(0.04, SpectralClass::M), // 0.5 AU frost line
    SpaceCoordinates::new(DVec3::new(100.0, 0.0, 0.0)),
    Star,
    CelestialBody { /* ... */ },
)).id();

// Spawn a planet orbiting the red dwarf
commands.spawn((
    OrbitsBody::new(red_dwarf),
    SpaceCoordinates::new(DVec3::new(101.0, 0.0, 0.0)), // 1 AU from star
    Planet,
    CelestialBody { /* ... */ },
));
// At 1 AU from a 0.5 AU frost line star, this planet is in the outer system
// and will have high volatiles!

// Spawn a blue giant star
let blue_giant = commands.spawn((
    StarSystem::from_luminosity(40.0, SpectralClass::A), // 17 AU frost line
    SpaceCoordinates::new(DVec3::new(1000.0, 0.0, 0.0)),
    Star,
    CelestialBody { /* ... */ },
)).id();

// Spawn a planet orbiting the blue giant
commands.spawn((
    OrbitsBody::new(blue_giant),
    SpaceCoordinates::new(DVec3::new(1001.0, 0.0, 0.0)), // 1 AU from star
    Planet,
    CelestialBody { /* ... */ },
));
// At 1 AU from a 17 AU frost line star, this planet is in the inner system
// and will have high construction materials!
```

## Scaling to Thousands of Systems

### Memory Efficiency
- `StarSystem`: 16 bytes per star
- `OrbitsBody`: 8 bytes per body
- Minimal overhead for massive scale

### Performance Considerations
1. **Resource generation runs once at startup** per body
2. **Efficient queries**: Only queries bodies without `PlanetResources`
3. **Star lookup**: O(1) entity lookup via Bevy ECS
4. **Parallel processing**: Bevy can parallelize across systems

### Recommended Approach for Massive Scale
```rust
// Generate star systems procedurally
for i in 0..10000 {
    let luminosity = rng.gen_range(0.01..100.0);
    let spectral_class = classify_by_luminosity(luminosity);
    let position = generate_galaxy_position(i);
    
    let star = commands.spawn((
        StarSystem::from_luminosity(luminosity, spectral_class),
        SpaceCoordinates::new(position),
        Star,
        CelestialBody { name: format!("Star-{}", i), /* ... */ },
    )).id();
    
    // Generate planets for this star
    for j in 0..rng.gen_range(0..10) {
        spawn_planet_orbiting(star, j, &mut commands);
    }
}
```

## Global Budget (Unchanged)
The `GlobalBudget` resource remains civilization-wide, tracking resources across ALL star systems. This is intentional:
- Represents a unified interstellar civilization
- Supports Kardashev Scale progression (Type II+ needs multi-star resources)
- Simplifies gameplay at massive scale

## Testing
New tests verify:
- Different frost lines produce different resource distributions
- M-type stars (0.5 AU frost line) have volatiles closer to the star
- A-type stars (17 AU frost line) have construction materials farther out
- Backward compatibility maintained (no `OrbitsBody` = use origin)

## Migration Path
Existing code works without changes. To enable multi-star support:

1. **Add `StarSystem` to your star entities**
   ```rust
   commands.entity(sun).insert(StarSystem::sun_like());
   ```

2. **Add `OrbitsBody` to planetary bodies**
   ```rust
   commands.entity(earth).insert(OrbitsBody::new(sun));
   ```

3. **Resources will regenerate** with new frost line calculations

## Future Enhancements
- Binary/trinary star systems with multiple frost lines
- Stellar evolution (frost line changes over time)
- Procedural galaxy generation
- Inter-system trade routes
- Resource scarcity based on galactic location
