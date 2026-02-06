# Orbital System Fixes and Behavior

## Issues Fixed

### 1. Time Scaling Bug in Body Rotation
**Problem**: The `rotate_bodies` system was using `Res<Time>` instead of `Res<Time<Virtual>>`, causing body rotations to ignore time acceleration controls.

**Fix**: Changed to use `Time<Virtual>` so rotations now properly respond to time scaling.

**Location**: `src/plugins/solar_system.rs`, `rotate_bodies()` function

### 2. Incorrect Rotation Speed Multiplier
**Problem**: Body rotation had an unexplained `* 1000.0` multiplier that caused overly fast rotations.

**Fix**: Removed the multiplier. Rotation speeds are now correctly calculated from the rotation period in the body data.

**Location**: `src/plugins/solar_system.rs`, `rotate_bodies()` function

### 3. Orbital Movement Visibility
**Problem**: Users reported "no movement along the orbits seem to happen" even though the orbital mechanics were working correctly.

**Root Cause**: This was NOT a bug in the orbital system. The orbital propagation system works correctly. The issue is that when the camera is anchored to a body (like Earth), the camera follows that body, making it appear stationary from the camera's perspective.

**Solution**: To see orbital movement:
- Anchor the camera to the Sun (or any non-moving reference point)
- Use the orbit trails (already implemented) to visualize the movement path
- The orbit trails move in real-time and show where bodies are in their orbits

## How the Orbital System Works

### Orbital Propagation
1. **System**: `propagate_orbits()` in `src/astronomy/systems.rs`
2. **How it works**: 
   - Uses Keplerian orbital elements (eccentricity, semi-major axis, inclination, etc.)
   - Calculates position based on elapsed time using the mean motion
   - Updates high-precision `SpaceCoordinates` (f64) every frame
   - Responds to time scaling via `Time<Virtual>`

### Transform Updates
1. **System**: `update_render_transform()` in `src/astronomy/systems.rs`
2. **How it works**:
   - Converts high-precision coordinates (AU, f64) to render coordinates (Bevy units, f32)
   - Only updates when `SpaceCoordinates` changes (change detection)
   - Preserves rotation set by `rotate_bodies`
   - Skips updates when change is below f32 precision threshold

### Orbit Visualization
1. **System**: `draw_orbit_paths()` in `src/astronomy/systems.rs`
2. **Features**:
   - Draws fading trails behind bodies (Terra Invicta style)
   - Trail is brightest at current position, fades backwards
   - Uses same calculations as position updates
   - Includes LOD (Level of Detail) system for performance

## Performance Optimizations

### 1. LOD System for Orbit Trails
**Implementation**: Dynamically adjusts segment count based on camera distance
- **Close orbits** (< 300 units): Full detail (128 segments)
- **Distant orbits** (> 3000 units): Reduced detail (32 segments minimum)
- **Selected orbits**: Always full detail regardless of distance
- **Formula**: `segments = base_segments * clamp(3000 / distance, 0.25, 1.0)`

**Impact**: Reduces rendering cost from O(bodies × 128) to O(bodies × adaptive) segments

### 2. Transform Update Optimization
**Implementation**: Only updates Transform when change exceeds threshold
- Checks if new translation differs from current by > sqrt(1e-6) ≈ 0.001 Bevy units (using squared distance for efficiency)
- Prevents unnecessary updates when orbital changes are below f32 precision
- Safe for slow-moving bodies: at 1000x time acceleration, even distant asteroids move > 0.001 units/frame
- At normal speed, imperceptibly slow movement doesn't need visual updates
- Orbital calculations still run at full f64 precision regardless of this rendering threshold
- Reduces downstream transform propagation in Bevy's hierarchy

### 3. Change Detection Usage
**Implementation**: `Changed<SpaceCoordinates>` filter in `update_render_transform`
- Only processes entities where coordinates actually changed
- Leverages Bevy's built-in change detection system
- Avoids iterating over static bodies (like the Sun)

## Future Scalability

The current implementation is designed to scale to thousands of bodies:

1. **Orbital Calculations**: O(n) per frame, where n is number of orbiting bodies
   - Uses efficient Keplerian propagation (no physics simulation)
   - Newton-Raphson solver converges in ~5 iterations typically
   - Double-precision (f64) math for accuracy

2. **Transform Updates**: O(m) per frame, where m is number of changed bodies
   - Change detection filters out static/slow-moving bodies
   - Additional threshold check for sub-pixel changes

3. **Orbit Rendering**: O(k × s) per frame, where k is visible orbits, s is LOD-adjusted segments
   - Visibility culling hides distant/unimportant orbits
   - LOD reduces segment count for distant orbits
   - Can be further optimized with orbit caching if needed

## Recommended Usage

### To See Orbital Movement:
1. **Anchor to Sun**: Press the anchor button (⚓) next to "Sol" in the UI
2. **Speed up time**: Use time controls to increase simulation speed (10x, 100x, 1000x)
3. **Observe orbits**: Planets will visibly move along their orbital paths
4. **Orbit trails**: The glowing trails show the current position and path

### To Focus on a Planet:
1. **Anchor to planet**: Click the planet or use the anchor button
2. **Observe rotation**: The planet will spin on its axis
3. **See moons**: If the planet has moons, they'll orbit around it
4. **Orbit trail**: The planet's own trail will be visible, showing its path

### Time Scaling:
- **Paused (0x)**: Everything stops
- **Slow (0.1x)**: 10× slower than real time
- **Normal (1x)**: Real-time simulation
- **Fast (10x-100x)**: Good for observing planetary motion
- **Very Fast (1000x)**: Good for seeing complete orbits quickly

## Technical Details

### Coordinate Systems
- **Space Coordinates**: High-precision (f64) positions in Astronomical Units (AU)
  - 1 AU ≈ 150 million km (Earth-Sun distance)
  - Used for accurate orbital calculations
  
- **Render Coordinates**: Single-precision (f32) positions in Bevy units
  - 1 AU = 1500 Bevy units (scaling factor)
  - Used for 3D rendering and visualization
  
- **Parent-Relative**: Moons and rings use their planet as coordinate origin
  - Makes moon orbital calculations simpler
  - Keeps Transform hierarchy clean

### Keplerian Elements
The system uses standard Keplerian orbital elements:
- **Semi-major axis (a)**: Size of orbit in AU
- **Eccentricity (e)**: Shape of orbit (0 = circle, 0-1 = ellipse)
- **Inclination (i)**: Tilt of orbital plane
- **Longitude of ascending node (Ω)**: Where orbit crosses reference plane
- **Argument of periapsis (ω)**: Orientation of ellipse
- **Mean anomaly at epoch (M₀)**: Initial position in orbit
- **Mean motion (n)**: Angular velocity in radians/second

### Time System
- **Real Time**: `Time` resource, not affected by time controls
  - Used for UI animations, input handling
  
- **Virtual Time**: `Time<Virtual>` resource, affected by time scaling
  - Used for orbital propagation, body rotation
  - Controlled by `TimeScale` resource
  - Can be paused, slowed, or accelerated up to 1000x

## Testing

To verify the fixes:
1. **Time scaling**: Set time to 100x, observe bodies rotating and orbits moving
2. **Orbital movement**: Anchor to Sun, set time to 100x, watch Earth orbit
3. **Performance**: Load all asteroids, check frame rate with orbit trails visible
4. **Camera anchoring**: Anchor to Earth, note that Earth appears stationary
5. **Moon orbits**: Anchor to Jupiter, watch its moons orbit

## Known Limitations

1. **N-body effects**: Bodies follow Keplerian orbits around their parent, no gravitational interactions
2. **Relativistic effects**: Uses Newtonian mechanics, no relativity
3. **Perturbations**: Orbital elements are constant, no planetary perturbations
4. **Precision**: Positions accurate to ~1 meter at 1 AU distance (f64 precision)
5. **Camera anchoring**: When following a body, its own orbital motion is not visible (by design)

These limitations are acceptable for the gameplay scope and can be enhanced later if needed.
