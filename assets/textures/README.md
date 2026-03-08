# Texture Assets

This directory contains texture assets for celestial bodies in Helios: Ascension.

## Structure

```
celestial/
├── planets/
│   ├── *.jpg / *.png          — Solar-system planet textures
│   ├── barren/                — Dry neutral-toned rocky worlds
│   ├── rock/                  — Bare stony and mineral-rich rocky worlds
│   ├── martian/               — Rust-red oxidised rocky worlds
│   ├── desert/                — Hot sandy arid worlds (60°C – 200°C)
│   ├── temperate/             — Earth-like mixed biome worlds (−20°C – 60°C)
│   ├── jungle/                — Lush, heavily-vegetated worlds (−20°C – 60°C)
│   ├── alpine/                — Mountainous, high-altitude worlds with snowcaps (−20°C – 10°C)
│   ├── savannah/              — Warm grassland worlds with sparse trees (0°C – 40°C)
│   ├── swamp/                 — Wetland worlds with extensive marshes and bogs (0°C – 30°C)
│   ├── ocean/                 — Global-ocean worlds (−20°C – 60°C)
│   ├── tundra/                — Cold permafrost worlds (−100°C – −20°C)
│   ├── ice/                   — Deeply-frozen worlds below −100°C (Pluto-like)
│   ├── scorched/              — Extreme-heat greenhouse worlds (200°C – 500°C)
│   ├── lava/                  — Volcanically-active scorched worlds (> 500°C)
│   ├── gas_giant/             — Warm gas giants (Jupiter / Saturn-like)
│   ├── ice_giant/             — Cold ice giants (Neptune / Uranus-like)
│   └── dwarf/                 — Dwarf planets and Kuiper Belt Objects
├── moons/                     — Moon textures
├── asteroids/                 — Asteroid textures (C-type, M-type, S-type, V-type)
├── comets/                    — Comet nucleus textures
├── rings/                     — Ring textures
└── stars/                     — Star surface textures
```

## Moddable Texture System

Exoplanet and procedurally-generated body textures are driven by a manifest:

```
assets/data/planet_textures.ron
```

Each category in the manifest holds an ordered list of texture paths.  The
game picks one deterministically by body name, so every planet always gets the
same texture across sessions.

**Adding a texture pack:**
1. Drop your texture files into the appropriate subfolder (e.g. `planets/jungle/`).
2. Register the paths in `assets/data/planet_textures.ron`.
3. Restart — the new textures are used immediately.

See `docs/MODDING.md` for the full guide.

## Texture System

### Multi-Layer Textures

Planets and moons in the Sol system support multi-layer texturing:
- Base texture (albedo/color map)
- Night lights texture (for civilization)
- Cloud texture (atmospheric effects)
- Specular map (reflectivity)

### Procedural Variation

The game uses procedural generation for:
- Tint colour — each body gets a temperature-appropriate colour cast
- Material roughness / metallic — per-body variation within each category
- Asteroid surfaces (based on spectral type)
- Comet nucleus textures

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
