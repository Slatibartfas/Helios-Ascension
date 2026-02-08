# Texture Assets

This directory contains texture assets for celestial bodies in Helios: Ascension.

## Structure

- `celestial/planets/` - Planet textures
- `celestial/moons/` - Moon textures  
- `celestial/asteroids/` - Asteroid textures (C-type, M-type, S-type, V-type)
- `celestial/comets/` - Comet nucleus and tail textures

## Texture System

### Multi-Layer Textures

Planets and moons support multi-layer texturing:
- Base texture (albedo/color map)
- Night lights texture (for civilization)
- Cloud texture (atmospheric effects)
- Specular map (reflectivity)

### Procedural System

The game uses procedural generation for:
- Asteroid surfaces (based on spectral type)
- Comet nucleus textures
- Terrain variation on procedurally generated bodies

### Asteroid Specifications

**C-type (Carbonaceous):**
- Color: Very dark gray to black (#404040 to #505050)
- Albedo: 0.03-0.10 (darkest asteroids)
- Surface: Carbon-rich, ancient material
- Texture: `generic_c_type_2k.jpg`

**M-type (Metallic):**
- Color: Metallic gray (#606060 to #808080)
- Albedo: 0.10-0.18
- Surface: Metallic sheen, crater-marked
- Texture: Procedurally generated

**S-type (Silicaceous):**
- Color: Gray to reddish-gray (#787878 to #8B7B75)
- Albedo: 0.10-0.22
- Surface: Rocky, cratered
- Texture: `generic_s_type_2k.jpg`

**V-type (Basaltic):**
- Color: Dark gray to black with red tint (#505050 to #604848)
- Albedo: 0.30-0.40
- Surface: Basaltic, smooth volcanic flows
- Texture: `vesta_4k.png` (Vesta family)

## Sources

Textures are sourced from:
- NASA/JPL planetary imagery
- ESA mission data
- Public domain astronomical photography
- Procedurally generated content

See individual subdirectories for specific attribution.
