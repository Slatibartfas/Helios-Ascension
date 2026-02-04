# Quick Reference: Texture Priority System

## The Answer

**Yes!** Dedicated textures automatically replace procedural ones. This is already implemented and working.

## How It Works

### Priority Order
```
1. Dedicated Texture (from RON file)  ← ALWAYS USED IF PRESENT
   ↓ (if not found)
2. Generic Texture (based on body type)
   ↓ (with)
3. Procedural Variation (unique per body)
```

### The Code
```rust
// In src/plugins/solar_system.rs (line 190-191)
let texture_path = body_data.texture.clone()           // Try dedicated first
    .or_else(|| get_generic_texture_path(body_data));  // Fallback to generic
```

## Quick Examples

### Replace Existing Texture

**Add your texture file:**
```
assets/textures/celestial/planets/mars_custom_8k.jpg
```

**Edit `assets/data/solar_system.ron`:**
```ron
(
    name: "Mars",
    // ... other fields ...
    texture: Some("textures/celestial/planets/mars_custom_8k.jpg"),  // Changed!
)
```

**Restart game** - Done! Your texture is used.

### Add Texture to Procedural Body

Many small moons use procedural textures. Give them dedicated ones:

**Before (procedural):**
```ron
(
    name: "Metis",
    body_type: Moon,
    // ... fields ...
    // No texture field = uses generic
)
```

**After (dedicated):**
```ron
(
    name: "Metis",
    body_type: Moon,
    // ... fields ...
    texture: Some("textures/celestial/moons/metis_2k.jpg"),  // Added!
)
```

### Add New Body with Texture

**Add texture file:**
```
assets/textures/celestial/moons/custom_moon_2k.jpg
```

**Add to `solar_system.ron`:**
```ron
(
    name: "CustomMoon",
    body_type: Moon,
    mass: 1.0e20,
    radius: 500.0,
    color: (0.8, 0.7, 0.6),
    emissive: (0.0, 0.0, 0.0),
    parent: Some("Jupiter"),
    orbit: Some((
        semi_major_axis: 0.005,
        eccentricity: 0.01,
        inclination: 1.0,
        longitude_ascending_node: 0.0,
        argument_of_periapsis: 0.0,
        orbital_period: 5.0,
        initial_angle: 0.0,
    )),
    rotation_period: 5.0,
    texture: Some("textures/celestial/moons/custom_moon_2k.jpg"),
)
```

## For User Mods

### Method 1: Replace File (Easiest)
1. Replace the texture file directly (keep same name)
2. Restart game

### Method 2: New File + RON Edit (Recommended)
1. Add your texture file
2. Edit RON to point to new file
3. Restart game

### Method 3: Texture Pack
1. Create mod directory with all your textures
2. Provide modified RON file or script to update paths
3. Users copy files and update RON

## For Future Solar Systems

The same system works for multiple solar systems:

**Proposed structure:**
```
assets/
├── data/
│   ├── solar_system.ron      # Sol
│   ├── alpha_centauri.ron    # Alpha Centauri
│   └── trappist1.ron         # TRAPPIST-1
└── textures/
    ├── sol/
    ├── alpha_centauri/
    └── generic/
```

Each system's RON file can reference dedicated textures.

## Requirements

- **Format**: JPEG or PNG
- **Resolution**: 1K-8K (powers of 2)
- **Projection**: Equirectangular
- **Path**: Relative to `assets/`

## Troubleshooting

**Texture not showing?**
- Check file path in RON is correct
- Verify file exists at that location
- Check filename (case-sensitive!)
- Restart game after changes

**Still using procedural?**
- Ensure texture field is `Some("path")` not `None`
- Check path is relative to assets/ directory
- Look for errors in console/log

## Documentation

See **MODDING_GUIDE.md** for:
- Complete instructions
- Body parameter explanations
- Texture creation tips
- Performance guidelines
- Multiple examples

See **docs/examples/mods/** for:
- EXAMPLE_MOD_MARS.md - Replace texture example
- EXAMPLE_MOD_NEW_BODY.md - Add body example

## Summary

✅ **Already implemented** - no code changes needed  
✅ **Priority system** - dedicated always overrides procedural  
✅ **Easy to use** - just edit RON file  
✅ **Perfect for mods** - simple distribution  
✅ **Future-proof** - ready for multiple solar systems  

**Happy modding!** 🚀
