# Modding Guide - Adding Custom Textures and Bodies

This guide explains how to add custom textures and celestial bodies to Helios Ascension, including texture packs, custom bodies, and even entire solar systems.

## Table of Contents
1. [Quick Start - Replace a Texture](#quick-start---replace-a-texture)
2. [Understanding the Texture System](#understanding-the-texture-system)
3. [Adding Custom Textures](#adding-custom-textures)
4. [Adding New Bodies](#adding-new-bodies)
5. [Creating a Texture Pack](#creating-a-texture-pack)
6. [Future: Multiple Solar Systems](#future-multiple-solar-systems)

## Quick Start - Replace a Texture

Want to add a custom Mars texture? Here's how:

1. Create your custom texture (JPEG, 2K-8K resolution, equirectangular projection)
2. Place it in: `assets/textures/celestial/planets/mars_custom_8k.jpg`
3. Edit `assets/data/solar_system.ron` and find the Mars entry:
```ron
(
    name: "Mars",
    body_type: Planet,
    // ... other fields ...
    texture: Some("textures/celestial/planets/mars_8k.jpg"),  // Change this line
)
```
4. Change the texture path to your new file:
```ron
    texture: Some("textures/celestial/planets/mars_custom_8k.jpg"),
```
5. Done! Your custom texture will now be used instead of the default.

## Understanding the Texture System

### Priority System

The game uses a **priority-based texture system**:

```
1. Dedicated Texture (if specified in RON file)
   ↓ (if not available)
2. Generic Texture (based on body type/class)
   ↓ (with)
3. Procedural Variation (unique per body)
```

**Key Code** (in `src/plugins/solar_system.rs`):
```rust
let texture_path = body_data.texture.clone()      // Try dedicated first
    .or_else(|| get_generic_texture_path(body_data));  // Fall back to generic
```

This means:
- ✅ **Dedicated textures ALWAYS override procedural ones**
- ✅ Adding a texture to the RON file immediately uses it
- ✅ Removing a texture path falls back to procedural
- ✅ Perfect for mods and customization!

### Texture Requirements

**Format**: JPEG (recommended for size) or PNG
**Resolution**: 1K to 8K (2048x1024 to 8192x4096)
**Projection**: Equirectangular (latitude-longitude mapping)
**Path**: Relative to `assets/` directory

**Examples**:
- Good: `textures/celestial/planets/custom_mars_4k.jpg`
- Good: `textures/custom/my_mod/earth_alternative.jpg`
- Bad: `/home/user/mars.jpg` (absolute paths won't work)
- Bad: `mars.jpg` (must be in assets directory)

## Adding Custom Textures

### Method 1: Replace Existing Texture

**Easiest approach** - just swap the file:

1. **Keep the same filename**: Replace `mars_8k.jpg` with your texture
2. Restart the game - done!

**Pros**: No configuration needed
**Cons**: Harder to maintain multiple texture sets

### Method 2: New Texture with RON Edit

**Recommended approach** - add new file and update RON:

1. Add your texture: `assets/textures/celestial/planets/mars_realistic_8k.jpg`
2. Edit `assets/data/solar_system.ron`:
```ron
(
    name: "Mars",
    // ... other fields ...
    texture: Some("textures/celestial/planets/mars_realistic_8k.jpg"),
)
```
3. Restart the game

**Pros**: Can keep multiple textures, easy to switch
**Cons**: Need to edit RON file

### Method 3: Add Texture to Body Without One

Many small moons and asteroids use procedural textures. You can give them dedicated textures:

**Before** (using procedural):
```ron
(
    name: "Metis",  // Small Jupiter moon
    body_type: Moon,
    // ... other fields ...
    // No texture field = uses generic rocky texture
)
```

**After** (dedicated texture):
```ron
(
    name: "Metis",
    body_type: Moon,
    // ... other fields ...
    texture: Some("textures/celestial/moons/metis_custom_2k.jpg"),  // Add this!
)
```

Now Metis has a dedicated texture instead of the generic one!

## Adding New Bodies

Want to add a fictional moon or exoplanet? Here's how:

### Step 1: Create the Body Data

Edit `assets/data/solar_system.ron` and add a new body entry:

```ron
(
    name: "MyCustomMoon",
    body_type: Moon,
    mass: 1.0e20,              // Mass in kg
    radius: 500.0,             // Radius in km
    color: (0.8, 0.7, 0.6),   // RGB color (0-1)
    emissive: (0.0, 0.0, 0.0), // For stars only
    parent: Some("Jupiter"),   // Orbits Jupiter
    orbit: Some((
        semi_major_axis: 0.5,      // Distance in AU
        eccentricity: 0.01,        // Orbit shape (0=circle)
        inclination: 2.0,          // Tilt in degrees
        longitude_ascending_node: 0.0,
        argument_of_periapsis: 0.0,
        orbital_period: 30.0,      // Days to orbit
        initial_angle: 0.0,        // Starting position
    )),
    rotation_period: 1.0,      // Rotation in days
    texture: Some("textures/celestial/moons/mycustommoon_2k.jpg"),
    asteroid_class: None,      // Only for asteroids
)
```

### Step 2: Add the Texture

Create and place your texture at the path specified.

### Step 3: Test

Restart the game and look for your custom moon orbiting Jupiter!

## Creating a Texture Pack

Want to create a complete texture replacement pack? Here's the structure:

### Directory Structure

```
assets/
├── textures/
│   └── celestial/
│       ├── planets/
│       │   ├── mars_mypack_8k.jpg
│       │   ├── earth_mypack_8k.jpg
│       │   └── jupiter_mypack_8k.jpg
│       ├── moons/
│       │   ├── moon_mypack_8k.jpg
│       │   └── titan_mypack_2k.jpg
│       └── stars/
│           └── sun_mypack_8k.jpg
└── data/
    └── solar_system.ron  # Modified with your texture paths
```

### Texture Pack RON Modifications

Create a script or guide to update texture paths:

**Original**:
```ron
texture: Some("textures/celestial/planets/mars_8k.jpg"),
```

**Your Pack**:
```ron
texture: Some("textures/celestial/planets/mars_mypack_8k.jpg"),
```

### Distribution

Package your texture pack as:
```
my_texture_pack/
├── README.md              # Installation instructions
├── textures/              # Your texture files
│   └── celestial/
│       └── planets/
│           └── mars_mypack_8k.jpg
└── solar_system_mod.ron   # Modified RON with your paths
```

**Installation instructions**:
1. Copy textures to `assets/textures/`
2. Copy modified bodies to `assets/data/solar_system.ron`
3. Restart game

## Future: Multiple Solar Systems

Planning for future multi-system support:

### Proposed Structure

```
assets/
└── data/
    ├── solar_system.ron      # Sol (our system)
    ├── alpha_centauri.ron    # Another system
    └── trappist1.ron         # Another system
```

### Loading Multiple Systems

**Future code** (not yet implemented):
```rust
// Load multiple star systems
let systems = vec![
    "assets/data/solar_system.ron",
    "assets/data/alpha_centauri.ron",
];

for system_file in systems {
    let data = SolarSystemData::load_from_file(system_file)?;
    // Spawn bodies...
}
```

### Texture Organization

```
assets/textures/
├── sol/              # Our solar system
│   ├── planets/
│   └── moons/
├── alpha_centauri/   # Alpha Centauri system
│   └── planets/
└── generic/          # Generic textures for any system
    ├── asteroids/
    └── comets/
```

## Tips and Best Practices

### Texture Creation

1. **Use equirectangular projection** - Required for proper sphere mapping
2. **Powers of 2** - Use 1024, 2048, 4096, 8192 pixel widths
3. **Aspect ratio 2:1** - Width should be 2x height (e.g., 4096x2048)
4. **JPEG compression** - Balance quality vs file size (80-90% quality)
5. **Test in-game** - Some textures look different when mapped to a sphere

### Performance

- **8K textures**: Use for major planets and Sun (high detail when close)
- **4K textures**: Good balance for most planets
- **2K textures**: Sufficient for moons and distant objects
- **1K textures**: Acceptable for small moons and asteroids

### Body Parameters

- **Mass**: Affects gravity (use realistic values)
- **Radius**: Affects visual size (use realistic km values)
- **Color**: Fallback if texture fails to load
- **Semi-major axis**: Distance from parent (in AU for planets, fraction for moons)
- **Orbital period**: Time to complete orbit (Earth days)

### Troubleshooting

**Texture not showing**:
- Check file path is relative to `assets/`
- Verify file exists at that location
- Check filename spelling and capitalization
- Check file format (JPEG or PNG)
- Look for errors in console/log

**Body not appearing**:
- Check RON syntax (commas, parentheses)
- Verify parent body exists
- Check orbital parameters are reasonable
- Verify body_type is correct

**Procedural texture instead of custom**:
- Verify texture path in RON file
- Check `texture: Some("path")` not `texture: None`
- Verify file exists at path

## Example Mods

### Simple Mars Retexture

**Files**:
- `assets/textures/celestial/planets/mars_hd_8k.jpg`

**RON Edit** in `solar_system.ron`:
```ron
// Find Mars entry and change:
texture: Some("textures/celestial/planets/mars_hd_8k.jpg"),
```

### Add Custom Asteroid

**Files**:
- `assets/textures/celestial/asteroids/psyche_2k.jpg`

**RON Addition** in `solar_system.ron`:
```ron
// Add new entry in bodies array:
(
    name: "Psyche",
    body_type: Asteroid,
    mass: 2.72e19,
    radius: 113.0,
    color: (0.5, 0.5, 0.5),
    emissive: (0.0, 0.0, 0.0),
    parent: Some("Sol"),
    orbit: Some((
        semi_major_axis: 2.92,
        eccentricity: 0.134,
        inclination: 3.1,
        longitude_ascending_node: 150.0,
        argument_of_periapsis: 228.0,
        orbital_period: 1826.0,
        initial_angle: 0.0,
    )),
    rotation_period: 0.175,
    texture: Some("textures/celestial/asteroids/psyche_2k.jpg"),
    asteroid_class: Some(MType),
)
```

### Complete Moon Texture Pack

Replace all Saturnian moon textures with custom set:

**Files** (7 textures):
- `assets/textures/celestial/moons/titan_mypack_2k.jpg`
- `assets/textures/celestial/moons/rhea_mypack_2k.jpg`
- `assets/textures/celestial/moons/iapetus_mypack_2k.jpg`
- ... etc for all Saturn moons

**RON Edits**: Update all Saturn moon entries with new paths.

## Community Resources

### Where to Find Textures

- **NASA**: https://science.nasa.gov/3d-resources/ (Public Domain)
- **Solar System Scope**: https://www.solarsystemscope.com/textures/ (CC BY 4.0)
- **Planet Pixel Emporium**: http://planetpixelemporium.com/ (Various licenses)
- **Community**: Check game forums for texture packs

### Sharing Your Mods

When sharing texture packs:
1. **Include README** with installation instructions
2. **Document licenses** for any textures used
3. **Provide credits** for texture sources
4. **List compatible game version**
5. **Show screenshots** of your textures in-game

## Advanced: Dynamic Texture Loading

**Future Feature** (not yet implemented):

The game could support a mod directory structure:

```
mods/
├── realistic_textures/
│   ├── mod.ron           # Mod metadata
│   ├── textures/
│   └── bodies.ron        # Body modifications
└── fantasy_bodies/
    ├── mod.ron
    ├── textures/
    └── bodies.ron
```

This would allow:
- Hot-swapping texture packs
- Enabling/disabling mods
- Mod priority ordering
- Automatic conflict resolution

## Conclusion

The texture override system is already built into Helios Ascension! You can:

✅ Replace any texture by adding it to the RON file  
✅ Add textures to bodies that use procedural ones  
✅ Create new bodies with custom textures  
✅ Build complete texture packs  
✅ Prepare for future multi-solar-system support  

**The dedicated texture ALWAYS takes priority** - just add it to the RON file and it works!

Happy modding! 🚀
