# Procedural Texture Variation System

This document explains the procedural texture variation system that provides visual variety to celestial bodies.

## Overview

The system automatically assigns appropriate textures to all 377 celestial bodies and applies procedural variations to ensure visual diversity.

## How It Works

### 1. Texture Assignment

Bodies are assigned textures in this priority:

1. **Dedicated Texture** (if specified in `solar_system.ron`)
   - 30 bodies have unique, high-quality textures
   - These are loaded directly without modification

2. **Generic Texture** (automatically assigned by type)
   - Bodies without dedicated textures get appropriate generic textures
   - Assignment based on body type and classification

### 2. Generic Texture Selection

The `get_generic_texture_path()` function assigns textures based on:

**Asteroids** (145 bodies):
- **C-type** (Carbonaceous): Dark, carbon-rich - ~75% of asteroids
  - Texture: `generic_c_type_2k.jpg`
- **S-type** (Silicaceous): Stony, lighter - ~17% of asteroids
  - Texture: `generic_s_type_2k.jpg`
- **M-type** (Metallic): Metal-rich - ~8% of asteroids
  - Texture: `generic_s_type_2k.jpg` (with high metallic property)
- **Unknown**: Defaults to C-type

**Comets** (20 bodies):
- Generic icy/rocky nucleus texture
- Texture: `generic_nucleus_2k.jpg`

**Moons without dedicated textures** (~180 bodies):
- Generic rocky surface texture
- Texture: `generic_c_type_2k.jpg` (reused for rocky appearance)

### 3. Procedural Variation

The `apply_procedural_variation()` function adds diversity by varying:

#### Color Variation

**Asteroids**:
- Brightness varies from 0.8x to 1.2x base color
- Creates lighter and darker variants of the same texture

**Comets**:
- Ice factor varies composition between icy white and dusty brown
- RGB: (0.6-0.9, 0.6-0.8, 0.5-0.9)

**Moons**:
- Gray variation from 0.9x to 1.1x base color
- Subtle differences in surface brightness

#### Roughness Variation

- **Textured bodies**: 0.7 to 0.9
- **Non-textured bodies**: 0.6 to 0.9
- Affects how matte or shiny the surface appears

#### Metallic Variation

- **M-type asteroids**: 0.5 to 0.8 (highly metallic)
- **Other asteroids**: 0.05 to 0.15 (low metallic)
- **Other bodies**: 0.1 to 0.2 (slight metallic)
- Affects how metal-like the surface appears

### 4. Deterministic Randomness

All variations use the body's name as a seed:

```rust
let mut seed = 0u32;
for byte in body_data.name.bytes() {
    seed = seed.wrapping_mul(31).wrapping_add(byte as u32);
}
```

This ensures:
- ✅ Same body always looks the same (reproducible)
- ✅ Different bodies look different (varied)
- ✅ No need for random number generator
- ✅ Consistent across game restarts

## Examples of Variation

### Asteroids

**Ceres** (C-type, large):
- Texture: generic_c_type_2k.jpg
- Brightness: ~1.05x (slightly brighter)
- Roughness: ~0.78
- Metallic: ~0.08

**Vesta** (S-type, dedicated texture):
- Texture: vesta_2k.jpg (dedicated)
- Color: WHITE (no tinting)
- No procedural variation applied
- Shows texture exactly as authored

**Random C-type asteroid**:
- Texture: generic_c_type_2k.jpg
- Brightness: ~0.92x (darker variant)
- Roughness: ~0.83
- Metallic: ~0.11

### Comets

Each comet gets unique ice/dust composition:

**Halley's Comet**:
- Texture: generic_nucleus_2k.jpg
- Ice factor: 0.67 (more icy)
- Color: (0.80, 0.73, 0.77) - lighter, icier

**Some other comet**:
- Texture: generic_nucleus_2k.jpg
- Ice factor: 0.23 (more dusty)
- Color: (0.67, 0.65, 0.59) - darker, dustier

### Moons

**Small Jupiter moon (Metis)**:
- Texture: generic_c_type_2k.jpg
- Gray variation: 0.97x
- Roughness: 0.74
- Metallic: 0.13

**Small Saturn moon (Pan)**:
- Texture: generic_c_type_2k.jpg
- Gray variation: 1.08x (brighter)
- Roughness: 0.82
- Metallic: 0.17

## Visual Impact

The procedural system provides:

1. **Consistency**: Same body always looks the same
2. **Variety**: 347 bodies with generic textures all look different
3. **Realism**: Variations match real astronomical diversity
4. **Performance**: No runtime cost, variations computed once at startup
5. **Memory efficient**: Only 4 generic textures for 347+ bodies

## Future Enhancements

Possible improvements:

1. **Normal maps**: Add bumps and surface detail
2. **Emissive variation**: Comets could have subtle glow
3. **More generic types**: Icy moon texture, rocky moon texture
4. **Albedo variation**: Vary reflectivity more dramatically
5. **UV offset**: Rotate/shift texture for more variety
6. **Noise overlay**: Add procedural noise patterns

## Technical Details

### Files Modified

- `src/plugins/solar_system_data.rs`: Added `AsteroidClass` enum
- `src/plugins/solar_system.rs`: Added procedural variation functions
- `assets/data/solar_system.ron`: Added classifications for some asteroids

### Generic Textures

Located in `assets/textures/celestial/`:
- `asteroids/generic_c_type_2k.jpg` (2K, 1.1MB)
- `asteroids/generic_s_type_2k.jpg` (2K, 1.2MB)
- `comets/generic_nucleus_2k.jpg` (2K, 880KB)

### Performance

- Variation computed once per body at startup
- No runtime overhead
- Total memory: 3.2MB for 347+ bodies worth of variation
- Very efficient compared to unique textures (would be ~50-100MB)

## Classification System

### Asteroid Classes

Asteroids can be classified in `solar_system.ron`:

```ron
(
    name: "Vesta",
    body_type: Asteroid,
    asteroid_class: Some(SType),
    // ... other fields
)
```

Available classes:
- `CType`: Carbonaceous (dark)
- `SType`: Silicaceous (stony)
- `MType`: Metallic
- `Unknown`: Will default to CType

If not specified, defaults to `CType` (most common).

### Adding New Classifications

To classify more asteroids:

1. Research the asteroid's spectral type
2. Add `asteroid_class: Some(XType)` to its entry in `solar_system.ron`
3. System will automatically use appropriate texture and properties

## Summary

The procedural variation system ensures all 377 celestial bodies have appropriate, varied textures using only a few generic base textures and smart variation algorithms. This provides excellent visual diversity while maintaining low memory usage and high performance.
